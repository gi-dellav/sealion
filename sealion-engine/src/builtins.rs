//! The `Math` and `Number` namespaces.
//!
//! `Math` and `Number` are not objects: they are compile-time namespaces whose
//! members are either constants (folded to an `f64` literal) or builtin
//! functions (compiled to a libm call or a short instruction sequence). The
//! global `isNaN` and `isFinite` functions are aliases for their `Number`
//! counterparts, as the roadmap asks.
//!
//! Because there are no objects yet, `Math` and `Number` cannot be used as
//! values: only `Math.member` and `Number.member` are accepted.

use std::collections::HashMap;

use boa_interner::{Interner, Sym};
use cranelift_codegen::ir::{FuncRef, Function, InstBuilder, Value, condcodes::FloatCC, types};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;

use crate::{
    Error,
    compiler::{make_f64_binary_signature, make_f64_unary_signature},
    value,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Namespace {
    Math,
    Number,
}

/// libm entry points taking one `f64`.
const UNARY_LIBM: &[&str] = &[
    "cbrt", "exp", "expm1", "log", "log10", "log1p", "log2", "sin", "cos", "tan", "asin", "acos",
    "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
];

/// libm entry points taking two `f64`s.
const BINARY_LIBM: &[&str] = &["pow", "atan2"];

/// Builtin functions implemented inline rather than by a libm call.
const INLINE_FUNCTIONS: &[&str] = &[
    "abs",
    "ceil",
    "floor",
    "round",
    "trunc",
    "sqrt",
    "fround",
    "min",
    "max",
    "clz32",
    "imul",
    "sign",
    "isNaN",
    "isFinite",
    "isInteger",
    "isSafeInteger",
];

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
    math: Sym,
    number: Sym,
    is_nan: Sym,
    is_finite: Sym,
    /// Property names, so that a member expression can be recognised.
    names: HashMap<Sym, &'static str>,
    funcs: HashMap<&'static str, FuncId>,
}

impl Builtins {
    pub fn new(module: &mut ObjectModule, interner: &mut Interner) -> Result<Self, Error> {
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

        let mut names = HashMap::new();
        let all = MATH_CONSTANTS
            .iter()
            .map(|(name, _)| *name)
            .chain(NUMBER_CONSTANTS.iter().map(|(name, _)| *name))
            .chain(UNARY_LIBM.iter().copied())
            .chain(BINARY_LIBM.iter().copied())
            .chain(INLINE_FUNCTIONS.iter().copied());
        for name in all {
            names.insert(interner.get_or_intern(name), name);
        }

        Ok(Self {
            math: interner.get_or_intern("Math"),
            number: interner.get_or_intern("Number"),
            is_nan: interner.get_or_intern("isNaN"),
            is_finite: interner.get_or_intern("isFinite"),
            names,
            funcs,
        })
    }

    /// The namespace a bare identifier names, `Math` or `Number`.
    pub fn namespace(&self, sym: Sym) -> Option<Namespace> {
        if sym == self.math {
            Some(Namespace::Math)
        } else if sym == self.number {
            Some(Namespace::Number)
        } else {
            None
        }
    }

    /// `isNaN` and `isFinite` are also plain global functions.
    pub fn alias(&self, sym: Sym) -> Option<&'static str> {
        if sym == self.is_nan {
            Some("isNaN")
        } else if sym == self.is_finite {
            Some("isFinite")
        } else {
            None
        }
    }

    pub fn property(&self, sym: Sym) -> Option<&'static str> {
        self.names.get(&sym).copied()
    }

    /// The constant folded into the program for `namespace.member`, if any.
    pub fn constant(&self, namespace: Namespace, name: &str) -> Option<f64> {
        let table = match namespace {
            Namespace::Math => MATH_CONSTANTS,
            Namespace::Number => NUMBER_CONSTANTS,
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

fn call_libm(builder: &mut FunctionBuilder, func_ref: FuncRef, args: &[Value]) -> Value {
    let call = builder.ins().call(func_ref, args);
    builder.inst_results(call)[0]
}

/// `Math.round`: halves round towards `+Infinity`, unlike C's `round`.
fn js_round(builder: &mut FunctionBuilder, x: Value) -> Value {
    let floor = builder.ins().floor(x);
    let diff = builder.ins().fsub(x, floor);
    let half = builder.ins().f64const(0.5);
    let below_half = builder.ins().fcmp(FloatCC::LessThan, diff, half);
    let one = builder.ins().f64const(1.0);
    let upper = builder.ins().fadd(floor, one);
    builder.ins().select(below_half, floor, upper)
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

fn sign(builder: &mut FunctionBuilder, x: Value) -> Value {
    let zero = builder.ins().f64const(0.0);
    let one = builder.ins().f64const(1.0);
    let minus_one = builder.ins().f64const(-1.0);
    let positive = builder.ins().fcmp(FloatCC::GreaterThan, x, zero);
    let negative = builder.ins().fcmp(FloatCC::LessThan, x, zero);
    let unsigned = builder.ins().select(positive, one, zero);
    let signed = builder.ins().select(negative, minus_one, unsigned);
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
    let lhs_nan = builder.ins().fcmp(FloatCC::Unordered, lhs, lhs);
    builder.ins().select(lhs_nan, lhs, picked)
}

fn min_or_max(builder: &mut FunctionBuilder, args: &[Value], maximum: bool) -> Value {
    let identity = if maximum {
        f64::NEG_INFINITY
    } else {
        f64::INFINITY
    };
    let mut acc = builder.ins().f64const(identity);
    for &arg in args {
        acc = extremum(builder, acc, arg, maximum);
    }
    acc
}

/// Compiles `Math.foo(...)` / `Number.foo(...)`, with `args` already evaluated.
pub fn call(
    builder: &mut FunctionBuilder,
    funcs: &HashMap<&'static str, FuncRef>,
    fmod_ref: FuncRef,
    name: &str,
    args: &[Value],
) -> Result<Value, Error> {
    let unary = |builder: &mut FunctionBuilder| argument(builder, args, 0);
    let value = match name {
        "abs" => {
            let x = unary(builder);
            builder.ins().fabs(x)
        }
        "ceil" => {
            let x = unary(builder);
            builder.ins().ceil(x)
        }
        "floor" => {
            let x = unary(builder);
            builder.ins().floor(x)
        }
        "round" => {
            let x = unary(builder);
            js_round(builder, x)
        }
        "trunc" => {
            let x = unary(builder);
            builder.ins().trunc(x)
        }
        "sqrt" => {
            let x = unary(builder);
            builder.ins().sqrt(x)
        }
        "fround" => {
            let x = unary(builder);
            fround(builder, x)
        }
        "min" => min_or_max(builder, args, false),
        "max" => min_or_max(builder, args, true),
        "sign" => {
            let x = unary(builder);
            sign(builder, x)
        }
        "clz32" => {
            let x = unary(builder);
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
            let x = unary(builder);
            let nan = value::is_nan(builder, x);
            value::from_bool(builder, nan)
        }
        "isFinite" => {
            let x = unary(builder);
            let finite = value::is_finite(builder, x);
            value::from_bool(builder, finite)
        }
        "isInteger" => {
            let x = unary(builder);
            let integral = is_integer(builder, x);
            value::from_bool(builder, integral)
        }
        "isSafeInteger" => {
            let x = unary(builder);
            let integral = is_integer(builder, x);
            let abs = builder.ins().fabs(x);
            let limit = builder.ins().f64const(MAX_SAFE_INTEGER);
            let small = builder.ins().fcmp(FloatCC::LessThanOrEqual, abs, limit);
            let safe = builder.ins().band(integral, small);
            value::from_bool(builder, safe)
        }
        other => {
            let func_ref = *funcs
                .get(other)
                .ok_or_else(|| Error::Unsupported(format!("unknown builtin function: {other}")))?;
            if BINARY_LIBM.contains(&other) {
                let lhs = argument(builder, args, 0);
                let rhs = argument(builder, args, 1);
                call_libm(builder, func_ref, &[lhs, rhs])
            } else {
                let x = unary(builder);
                call_libm(builder, func_ref, &[x])
            }
        }
    };
    Ok(value)
}
