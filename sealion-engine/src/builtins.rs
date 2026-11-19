//! The `Math`, `Number`, `Object` and `Date` namespaces.
//!
//! They are not objects: they are compile-time namespaces whose members are
//! either constants (folded to an `f64` literal) or builtin functions (compiled
//! to a libm call or a short instruction sequence). The global `isNaN`,
//! `isFinite`, `Boolean` and `Number` functions are compiled the same way, as
//! are the global `NaN` and `Infinity` constants.
//!
//! Because there are no objects yet, the namespaces cannot be used as values:
//! only `Math.member`, `Number.member`, `Object.member` and `Date.member` are
//! accepted.

use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::{
    FuncRef, Function, InstBuilder, MemFlags, Value, condcodes::FloatCC, condcodes::IntCC, types,
};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;

use crate::{
    Error,
    compiler::{
        make_f64_binary_signature, make_f64_unary_signature, make_i64_unary_signature,
        make_nullary_f64_signature,
    },
    value,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Namespace {
    Math,
    Number,
    Object,
    Date,
}

/// libm entry points taking one `f64`.
const UNARY_LIBM: &[&str] = &[
    "cbrt", "exp", "expm1", "log", "log10", "log1p", "log2", "sin", "cos", "tan", "asin", "acos",
    "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
];

/// libm entry points taking two `f64`s.
const BINARY_LIBM: &[&str] = &["pow", "atan2"];

/// `Math` functions implemented inline rather than by a libm call.
const MATH_INLINE: &[&str] = &[
    "abs", "ceil", "floor", "round", "trunc", "sqrt", "fround", "min", "max", "clz32", "imul",
    "sign", "hypot", "random", "f16round",
];

/// `Number` functions implemented inline.
const NUMBER_INLINE: &[&str] = &["isNaN", "isFinite", "isInteger", "isSafeInteger"];

/// `Object` functions implemented inline.
const OBJECT_INLINE: &[&str] = &["is"];

/// `Date` functions implemented inline.
const DATE_INLINE: &[&str] = &["now"];

const MATH_CONSTANTS: &[(&str, f64)] = &[
    ("PI", std::f64::consts::PI),
    ("E", std::f64::consts::E),
    ("LN2", std::f64::consts::LN_2),
    ("LN10", std::f64::consts::LN_10),
    ("LOG2E", std::f64::consts::LOG2_E),
    ("LOG10E", std::f64::consts::LOG10_E),
    ("SQRT1_2", std::f64::consts::FRAC_1_SQRT_2),
    ("SQRT2", std::f64::consts::SQRT_2),
];

const NUMBER_CONSTANTS: &[(&str, f64)] = &[
    ("MAX_VALUE", f64::MAX),
    ("MIN_VALUE", f64::from_bits(1)),
    ("MAX_SAFE_INTEGER", 9_007_199_254_740_991.0),
    ("MIN_SAFE_INTEGER", -9_007_199_254_740_991.0),
    ("EPSILON", f64::EPSILON),
    ("NaN", f64::NAN),
    ("NEGATIVE_INFINITY", f64::NEG_INFINITY),
    ("POSITIVE_INFINITY", f64::INFINITY),
];

/// `2**53 - 1`, the largest integer a double represents exactly.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

pub struct Builtins {
    /// Property names, so that a member expression can be recognised.
    names: HashSet<&'static str>,
    funcs: HashMap<&'static str, FuncId>,
}

impl Builtins {
    pub fn new(module: &mut ObjectModule) -> Result<Self, Error> {
        let unary_sig = make_f64_unary_signature(module);
        let binary_sig = make_f64_binary_signature(module);
        let mut funcs = HashMap::new();
        for &name in UNARY_LIBM {
            let id = module.declare_function(name, Linkage::Import, &unary_sig)?;
            funcs.insert(name, id);
        }
        for &name in BINARY_LIBM {
            let id = module.declare_function(name, Linkage::Import, &binary_sig)?;
            funcs.insert(name, id);
        }
        let nullary_sig = make_nullary_f64_signature(module);
        let random_id = module.declare_function("drand48", Linkage::Import, &nullary_sig)?;
        funcs.insert("random", random_id);
        let time_sig = make_i64_unary_signature(module);
        let time_id = module.declare_function("time", Linkage::Import, &time_sig)?;
        funcs.insert("now", time_id);

        let names = MATH_CONSTANTS
            .iter()
            .map(|(name, _)| *name)
            .chain(NUMBER_CONSTANTS.iter().map(|(name, _)| *name))
            .chain(UNARY_LIBM.iter().copied())
            .chain(BINARY_LIBM.iter().copied())
            .chain(MATH_INLINE.iter().copied())
            .chain(NUMBER_INLINE.iter().copied())
            .chain(OBJECT_INLINE.iter().copied())
            .chain(DATE_INLINE.iter().copied())
            .collect();

        Ok(Self { names, funcs })
    }

    /// The namespace a bare identifier names.
    pub fn namespace(&self, name: &str) -> Option<Namespace> {
        match name {
            "Math" => Some(Namespace::Math),
            "Number" => Some(Namespace::Number),
            "Object" => Some(Namespace::Object),
            "Date" => Some(Namespace::Date),
            _ => None,
        }
    }

    /// A free global function such as `isNaN` or `Boolean`.
    pub fn global_function(&self, name: &str) -> Option<&'static str> {
        match name {
            "isNaN" => Some("isNaN"),
            "isFinite" => Some("isFinite"),
            "Boolean" => Some("Boolean"),
            "Number" => Some("Number"),
            _ => None,
        }
    }

    /// A free global constant such as `NaN` or `Infinity`.
    pub fn global_constant(&self, name: &str) -> Option<f64> {
        match name {
            "NaN" => Some(f64::NAN),
            "Infinity" => Some(f64::INFINITY),
            _ => None,
        }
    }

    pub fn is_undefined(&self, name: &str) -> bool {
        name == "undefined"
    }

    /// True when `name` is one of the engine's builtin globals.
    pub fn is_global(&self, name: &str) -> bool {
        self.namespace(name).is_some()
            || self.global_function(name).is_some()
            || self.global_constant(name).is_some()
            || self.is_undefined(name)
    }

    pub fn property(&self, name: &str) -> Option<&'static str> {
        self.names.get(name).copied()
    }

    /// True when `name` is a constant or function member of `namespace`.
    pub fn has(&self, namespace: Namespace, name: &str) -> bool {
        if self.constant(namespace, name).is_some() {
            return true;
        }
        match namespace {
            Namespace::Math => {
                UNARY_LIBM.contains(&name)
                    || BINARY_LIBM.contains(&name)
                    || MATH_INLINE.contains(&name)
            }
            Namespace::Number => NUMBER_INLINE.contains(&name),
            Namespace::Object => OBJECT_INLINE.contains(&name),
            Namespace::Date => DATE_INLINE.contains(&name),
        }
    }

    /// The constant folded into the program for `namespace.member`, if any.
    pub fn constant(&self, namespace: Namespace, name: &str) -> Option<f64> {
        let table = match namespace {
            Namespace::Math => MATH_CONSTANTS,
            Namespace::Number => NUMBER_CONSTANTS,
            Namespace::Object | Namespace::Date => return None,
        };
        table
            .iter()
            .find_map(|(candidate, value)| (*candidate == name).then_some(*value))
    }

    pub fn declare_in(
        &self,
        module: &mut ObjectModule,
        func: &mut Function,
    ) -> HashMap<&'static str, FuncRef> {
        self.funcs
            .iter()
            .map(|(name, id)| (*name, module.declare_func_in_func(*id, func)))
            .collect()
    }
}

fn argument(builder: &mut FunctionBuilder, args: &[Value], index: usize) -> Value {
    match args.get(index) {
        Some(value) => *value,
        None => value::undefined(builder),
    }
}

fn number_argument(
    builder: &mut FunctionBuilder,
    args: &[Value],
    index: usize,
    known: &[bool],
) -> Value {
    let value = argument(builder, args, index);
    if known.get(index).copied().unwrap_or(false) {
        value
    } else {
        value::to_number(builder, value)
    }
}

fn call_libm(builder: &mut FunctionBuilder, func_ref: FuncRef, args: &[Value]) -> Value {
    let call = builder.ins().call(func_ref, args);
    builder.inst_results(call)[0]
}

/// `Math.round`: halves round towards `+Infinity`, unlike C's `round`. Negative
/// values that round to zero produce `-0`.
fn js_round(builder: &mut FunctionBuilder, x: Value) -> Value {
    let floor = builder.ins().floor(x);
    let diff = builder.ins().fsub(x, floor);
    let half = builder.ins().f64const(0.5);
    let below_half = builder.ins().fcmp(FloatCC::LessThan, diff, half);
    let one = builder.ins().f64const(1.0);
    let upper = builder.ins().fadd(floor, one);
    let rounded = builder.ins().select(below_half, floor, upper);

    let zero = builder.ins().f64const(0.0);
    let negative = builder.ins().fcmp(FloatCC::LessThan, x, zero);
    let is_zero = builder.ins().fcmp(FloatCC::Equal, rounded, zero);
    let fix_negative_zero = builder.ins().band(negative, is_zero);
    let negative_zero = builder.ins().f64const(-0.0);
    builder
        .ins()
        .select(fix_negative_zero, negative_zero, rounded)
}

/// `Math.fround`: rounds the value to the nearest `f32` and back.
fn fround(builder: &mut FunctionBuilder, x: Value) -> Value {
    let small = builder.ins().fdemote(types::F32, x);
    builder.ins().fpromote(types::F64, small)
}

fn is_integer(builder: &mut FunctionBuilder, x: Value) -> Value {
    let finite = value::is_finite(builder, x);
    let trunc = builder.ins().trunc(x);
    let integral = builder.ins().fcmp(FloatCC::Equal, trunc, x);
    builder.ins().band(finite, integral)
}

fn has_negative_sign(builder: &mut FunctionBuilder, x: Value) -> Value {
    let bits = builder.ins().bitcast(types::I64, MemFlags::new(), x);
    builder.ins().icmp_imm(IntCC::SignedLessThan, bits, 0)
}

fn sign(builder: &mut FunctionBuilder, x: Value) -> Value {
    let zero = builder.ins().f64const(0.0);
    let one = builder.ins().f64const(1.0);
    let minus_one = builder.ins().f64const(-1.0);
    let positive = builder.ins().fcmp(FloatCC::GreaterThan, x, zero);
    let negative = builder.ins().fcmp(FloatCC::LessThan, x, zero);
    let unsigned = builder.ins().select(positive, one, zero);
    let signed = builder.ins().select(negative, minus_one, unsigned);

    let is_zero = builder.ins().fcmp(FloatCC::Equal, x, zero);
    let negative_sign = has_negative_sign(builder, x);
    let is_negative_zero = builder.ins().band(is_zero, negative_sign);
    let negative_zero = builder.ins().f64const(-0.0);
    let signed = builder
        .ins()
        .select(is_negative_zero, negative_zero, signed);

    let nan = builder.ins().fcmp(FloatCC::Unordered, x, x);
    builder.ins().select(nan, x, signed)
}

fn extremum(builder: &mut FunctionBuilder, lhs: Value, rhs: Value, maximum: bool) -> Value {
    let cc = if maximum {
        FloatCC::GreaterThan
    } else {
        FloatCC::LessThan
    };
    let comparison = builder.ins().fcmp(cc, lhs, rhs);
    let picked = builder.ins().select(comparison, lhs, rhs);

    let zero = builder.ins().f64const(0.0);
    let lhs_zero = builder.ins().fcmp(FloatCC::Equal, lhs, zero);
    let rhs_zero = builder.ins().fcmp(FloatCC::Equal, rhs, zero);
    let both_zero = builder.ins().band(lhs_zero, rhs_zero);
    let lhs_negative = has_negative_sign(builder, lhs);
    let rhs_negative = has_negative_sign(builder, rhs);
    let negative_zero = builder.ins().f64const(-0.0);
    let zero_result = if maximum {
        let lhs_positive = builder.ins().icmp_imm(IntCC::Equal, lhs_negative, 0);
        let rhs_positive = builder.ins().icmp_imm(IntCC::Equal, rhs_negative, 0);
        let either_positive = builder.ins().bor(lhs_positive, rhs_positive);
        builder.ins().select(either_positive, zero, negative_zero)
    } else {
        let either_negative = builder.ins().bor(lhs_negative, rhs_negative);
        builder.ins().select(either_negative, negative_zero, zero)
    };
    let with_zero = builder.ins().select(both_zero, zero_result, picked);

    let lhs_nan = builder.ins().fcmp(FloatCC::Unordered, lhs, lhs);
    builder.ins().select(lhs_nan, lhs, with_zero)
}

fn min_or_max(
    builder: &mut FunctionBuilder,
    args: &[Value],
    maximum: bool,
    known: &[bool],
) -> Value {
    let identity = if maximum {
        f64::NEG_INFINITY
    } else {
        f64::INFINITY
    };
    let mut acc = builder.ins().f64const(identity);
    for (index, &arg) in args.iter().enumerate() {
        let arg = if known.get(index).copied().unwrap_or(false) {
            arg
        } else {
            value::to_number(builder, arg)
        };
        acc = extremum(builder, acc, arg, maximum);
    }
    acc
}

fn hypot(builder: &mut FunctionBuilder, args: &[Value], known: &[bool]) -> Value {
    let mut scale = builder.ins().f64const(0.0);
    let mut sum = builder.ins().f64const(0.0);
    let mut has_nan = builder.ins().iconst(types::I8, 0);
    let mut has_inf = builder.ins().iconst(types::I8, 0);
    let zero = builder.ins().f64const(0.0);
    let one = builder.ins().f64const(1.0);
    let infinity = builder.ins().f64const(f64::INFINITY);
    let nan = builder.ins().f64const(f64::NAN);

    for (index, &arg) in args.iter().enumerate() {
        let x = if known.get(index).copied().unwrap_or(false) {
            arg
        } else {
            value::to_number(builder, arg)
        };
        let abs = builder.ins().fabs(x);
        let x_nan = builder.ins().fcmp(FloatCC::Unordered, x, x);
        let x_inf = builder.ins().fcmp(FloatCC::Equal, abs, infinity);
        has_nan = builder.ins().bor(has_nan, x_nan);
        has_inf = builder.ins().bor(has_inf, x_inf);

        let bigger = builder.ins().fcmp(FloatCC::GreaterThan, abs, scale);
        let new_scale = builder.ins().select(bigger, abs, scale);
        let scale_over_abs = builder.ins().fdiv(scale, abs);
        let abs_over_scale = builder.ins().fdiv(abs, scale);
        let ratio = builder.ins().select(bigger, scale_over_abs, abs_over_scale);
        let square = builder.ins().fmul(ratio, ratio);
        let scaled = builder.ins().fmul(sum, square);
        let increased = builder.ins().fadd(scaled, one);
        let accumulated = builder.ins().fadd(sum, square);
        let candidate = builder.ins().select(bigger, increased, accumulated);
        let scale_is_zero = builder.ins().fcmp(FloatCC::Equal, new_scale, zero);
        sum = builder.ins().select(scale_is_zero, zero, candidate);
        scale = new_scale;
    }

    let root = builder.ins().sqrt(sum);
    let magnitude = builder.ins().fmul(scale, root);
    let with_nan = builder.ins().select(has_nan, nan, magnitude);
    builder.ins().select(has_inf, infinity, with_nan)
}

fn f16round(builder: &mut FunctionBuilder, x: Value) -> Value {
    let abs = builder.ins().fabs(x);
    let bits = builder.ins().bitcast(types::I64, MemFlags::new(), abs);
    let exponent = builder.ins().ushr_imm(bits, 52);
    let exponent = builder.ins().band_imm(exponent, 0x7ff);
    let min_normal = builder.ins().f64const(2f64.powi(-14));
    let subnormal = builder.ins().fcmp(FloatCC::LessThan, abs, min_normal);
    let normal_exponent = builder.ins().iadd_imm(exponent, -10);
    let subnormal_exponent = builder.ins().iconst(types::I64, 999);
    let chosen = builder
        .ins()
        .select(subnormal, subnormal_exponent, normal_exponent);
    let quantum_bits = builder.ins().ishl_imm(chosen, 52);
    let quantum = builder
        .ins()
        .bitcast(types::F64, MemFlags::new(), quantum_bits);
    let quotient = builder.ins().fdiv(abs, quantum);
    let rounded = builder.ins().nearest(quotient);
    let magnitude = builder.ins().fmul(rounded, quantum);
    let signed = builder.ins().fcopysign(magnitude, x);

    let overflow = builder.ins().f64const(65520.0);
    let too_big = builder
        .ins()
        .fcmp(FloatCC::GreaterThanOrEqual, abs, overflow);
    let infinity = builder.ins().f64const(f64::INFINITY);
    let infinity = builder.ins().fcopysign(infinity, x);
    builder.ins().select(too_big, infinity, signed)
}

/// Compiles `Math.foo(...)`, `Number.foo(...)`, `Object.foo(...)` and the free
/// global builtins, with `args` already evaluated.
pub fn call(
    builder: &mut FunctionBuilder,
    funcs: &HashMap<&'static str, FuncRef>,
    fmod_ref: FuncRef,
    namespace: Option<Namespace>,
    name: &str,
    args: &[Value],
    known: &[bool],
) -> Result<Value, Error> {
    let value = match name {
        "abs" => {
            let x = number_argument(builder, args, 0, known);
            builder.ins().fabs(x)
        }
        "ceil" => {
            let x = number_argument(builder, args, 0, known);
            builder.ins().ceil(x)
        }
        "floor" => {
            let x = number_argument(builder, args, 0, known);
            builder.ins().floor(x)
        }
        "round" => {
            let x = number_argument(builder, args, 0, known);
            js_round(builder, x)
        }
        "trunc" => {
            let x = number_argument(builder, args, 0, known);
            builder.ins().trunc(x)
        }
        "sqrt" => {
            let x = number_argument(builder, args, 0, known);
            builder.ins().sqrt(x)
        }
        "fround" => {
            let x = number_argument(builder, args, 0, known);
            fround(builder, x)
        }
        "min" => min_or_max(builder, args, false, known),
        "max" => min_or_max(builder, args, true, known),
        "hypot" => hypot(builder, args, known),
        "f16round" => {
            let x = number_argument(builder, args, 0, known);
            f16round(builder, x)
        }
        "random" => {
            let func_ref = *funcs
                .get("random")
                .ok_or_else(|| Error::Unsupported("Math.random is not available".to_string()))?;
            call_libm(builder, func_ref, &[])
        }
        "now" => {
            let func_ref = *funcs
                .get("now")
                .ok_or_else(|| Error::Unsupported("Date.now is not available".to_string()))?;
            let null = builder.ins().iconst(types::I64, 0);
            let call = builder.ins().call(func_ref, &[null]);
            let seconds = builder.inst_results(call)[0];
            let millis = builder.ins().imul_imm(seconds, 1000);
            builder.ins().fcvt_from_sint(types::F64, millis)
        }
        "sign" => {
            let x = number_argument(builder, args, 0, known);
            sign(builder, x)
        }
        "clz32" => {
            let x = argument(builder, args, 0);
            let int = value::to_int32(builder, fmod_ref, x);
            let leading = builder.ins().clz(int);
            let wide = builder.ins().uextend(types::I64, leading);
            builder.ins().fcvt_from_uint(types::F64, wide)
        }
        "imul" => {
            let lhs = argument(builder, args, 0);
            let rhs = argument(builder, args, 1);
            let lhs = value::to_int32(builder, fmod_ref, lhs);
            let rhs = value::to_int32(builder, fmod_ref, rhs);
            let product = builder.ins().imul(lhs, rhs);
            value::from_int32(builder, product)
        }
        "isNaN" => {
            let x = if namespace == Some(Namespace::Number) {
                argument(builder, args, 0)
            } else {
                number_argument(builder, args, 0, known)
            };
            let nan = value::is_nan(builder, x);
            value::from_bool(builder, nan)
        }
        "isFinite" => {
            let x = if namespace == Some(Namespace::Number) {
                argument(builder, args, 0)
            } else {
                number_argument(builder, args, 0, known)
            };
            let finite = value::is_finite(builder, x);
            value::from_bool(builder, finite)
        }
        "isInteger" => {
            let x = argument(builder, args, 0);
            let integral = is_integer(builder, x);
            value::from_bool(builder, integral)
        }
        "isSafeInteger" => {
            let x = argument(builder, args, 0);
            let integral = is_integer(builder, x);
            let abs = builder.ins().fabs(x);
            let limit = builder.ins().f64const(MAX_SAFE_INTEGER);
            let small = builder.ins().fcmp(FloatCC::LessThanOrEqual, abs, limit);
            let safe = builder.ins().band(integral, small);
            value::from_bool(builder, safe)
        }
        "is" => {
            let lhs = argument(builder, args, 0);
            let rhs = argument(builder, args, 1);
            value::same_value(builder, lhs, rhs)
        }
        "Boolean" => {
            let x = argument(builder, args, 0);
            let truthy = value::truthy(builder, x);
            value::from_bool(builder, truthy)
        }
        "Number" => number_argument(builder, args, 0, known),
        other => {
            let func_ref = *funcs
                .get(other)
                .ok_or_else(|| Error::Unsupported(format!("unknown builtin function: {other}")))?;
            if BINARY_LIBM.contains(&other) {
                let lhs = number_argument(builder, args, 0, known);
                let rhs = number_argument(builder, args, 1, known);
                call_libm(builder, func_ref, &[lhs, rhs])
            } else {
                let x = number_argument(builder, args, 0, known);
                call_libm(builder, func_ref, &[x])
            }
        }
    };
    Ok(value)
}
