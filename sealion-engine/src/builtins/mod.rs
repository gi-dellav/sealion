//! The `Math`, `Number`, `Object` and `Date` namespaces.
//!
//! They are not objects: they are compile-time namespaces whose members are
//! either constants (folded to an `f64` literal) or builtin functions (compiled
//! to a libm call or a short instruction sequence). The global `isNaN`,
//! `isFinite`, `Boolean` and `Number` functions are compiled the same way, as
//! are the global `NaN` and `Infinity` constants.
//!
//! The namespaces are not first-class values: only `Math.member`,
//! `Number.member`, `Object.member` and `Date.member` are accepted.

use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::{AbiParam, FuncRef, Function, Signature, types};
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;

use crate::{
    Error,
    compiler::{
        make_f64_binary_signature, make_f64_ternary_signature, make_f64_unary_signature,
        make_i64_unary_signature, make_nullary_f64_signature,
    },
};

mod dispatch;
mod math;

pub use dispatch::call;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Namespace {
    Math,
    Number,
    Object,
    Array,
    Date,
    Console,
}

/// libm entry points taking one `f64`.
const UNARY_LIBM: &[&str] = &[
    "cbrt", "exp", "expm1", "log", "log10", "log1p", "log2", "sin", "cos", "tan", "asin", "acos",
    "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
];

/// libm entry points taking two `f64`s.
pub(super) const BINARY_LIBM: &[&str] = &["pow", "atan2"];

/// `Math` functions implemented inline rather than by a libm call.
const MATH_INLINE: &[&str] = &[
    "abs", "ceil", "floor", "round", "trunc", "sqrt", "fround", "min", "max", "clz32", "imul",
    "sign", "hypot", "random", "f16round",
];

/// `Number` functions implemented inline.
const NUMBER_INLINE: &[&str] = &["isNaN", "isFinite", "isInteger", "isSafeInteger"];

/// `Object` functions implemented by the dynamic runtime.
const OBJECT_INLINE: &[&str] = &[
    "is",
    "create",
    "getPrototypeOf",
    "setPrototypeOf",
    "hasOwn",
    "keys",
    "values",
    "entries",
    "prototype",
];

/// `Array` static functions implemented by the dynamic runtime.
const ARRAY_INLINE: &[&str] = &["isArray", "of", "from", "prototype"];

/// `Date` functions implemented inline.
const DATE_INLINE: &[&str] = &["now"];

/// `console` functions implemented by the dynamic runtime.
const CONSOLE_INLINE: &[&str] = &["log"];

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
pub(super) const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

pub struct Builtins {
    /// Property names, so that a member expression can be recognised.
    names: HashSet<&'static str>,
    funcs: HashMap<&'static str, FuncId>,
}

impl Builtins {
    /// `dynamic` declares the host helpers of the dynamic runtime; the AOT
    /// backend leaves them out so that no unresolved symbol is emitted.
    pub fn new(module: &mut ObjectModule, dynamic: bool) -> Result<Self, Error> {
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

        let string_from_utf8_sig = make_i64_pair_to_i64_signature(module);
        let from_utf8_id = module.declare_function(
            "sealion_string_from_utf8",
            Linkage::Import,
            &string_from_utf8_sig,
        )?;
        funcs.insert("sealion_string_from_utf8", from_utf8_id);
        let f64_predicate_sig = make_f64_predicate_signature(module);
        let truthy_id = module.declare_function(
            "sealion_string_truthy",
            Linkage::Import,
            &f64_predicate_sig,
        )?;
        funcs.insert("sealion_string_truthy", truthy_id);
        let f64_binary_to_i32_sig = make_f64_binary_to_i32_signature(module);
        let compare_id = module.declare_function(
            "sealion_string_compare",
            Linkage::Import,
            &f64_binary_to_i32_sig,
        )?;
        funcs.insert("sealion_string_compare", compare_id);
        let void_sig = make_i64_pair_void_signature(module);
        let console_log_id =
            module.declare_function("sealion_console_log", Linkage::Import, &void_sig)?;
        funcs.insert("sealion_console_log", console_log_id);
        for name in ["sealion_string_to_number", "sealion_string_to_string"] {
            let id = module.declare_function(name, Linkage::Import, &unary_sig)?;
            funcs.insert(name, id);
        }
        for name in [
            "sealion_string_add",
            "sealion_string_strict_equal",
            "sealion_string_loose_equal",
            "sealion_string_same_value",
        ] {
            let id = module.declare_function(name, Linkage::Import, &binary_sig)?;
            funcs.insert(name, id);
        }

        if dynamic {
            let pointer_sig = make_nullary_i64_signature(module);
            let new_id =
                module.declare_function("sealion_object_new", Linkage::Import, &pointer_sig)?;
            funcs.insert("sealion_object_new", new_id);
            for name in [
                "sealion_object_get",
                "sealion_object_has",
                "sealion_object_delete",
            ] {
                let id = module.declare_function(name, Linkage::Import, &binary_sig)?;
                funcs.insert(name, id);
            }
            let ternary_sig = make_f64_ternary_signature(module);
            let set_id =
                module.declare_function("sealion_object_set", Linkage::Import, &ternary_sig)?;
            funcs.insert("sealion_object_set", set_id);

            let length_sig = make_i64_unary_signature(module);
            for name in [
                "sealion_array_new",
                "sealion_array_construct",
                "sealion_array_of",
            ] {
                let id = module.declare_function(name, Linkage::Import, &length_sig)?;
                funcs.insert(name, id);
            }
            let is_array_id =
                module.declare_function("sealion_array_is_array", Linkage::Import, &unary_sig)?;
            funcs.insert("sealion_array_is_array", is_array_id);
            let from_id =
                module.declare_function("sealion_array_from", Linkage::Import, &unary_sig)?;
            funcs.insert("sealion_array_from", from_id);

            let new_map_id =
                module.declare_function("sealion_map_new", Linkage::Import, &pointer_sig)?;
            funcs.insert("sealion_map_new", new_map_id);

            for name in [
                "sealion_object_create",
                "sealion_object_get_prototype",
                "sealion_object_keys",
                "sealion_object_values",
                "sealion_object_entries",
            ] {
                let id = module.declare_function(name, Linkage::Import, &unary_sig)?;
                funcs.insert(name, id);
            }
            for name in ["sealion_object_set_prototype", "sealion_object_has_own"] {
                let id = module.declare_function(name, Linkage::Import, &binary_sig)?;
                funcs.insert(name, id);
            }
            for name in ["sealion_object_prototype", "sealion_array_prototype"] {
                let id = module.declare_function(name, Linkage::Import, &pointer_sig)?;
                funcs.insert(name, id);
            }
        }

        let names = MATH_CONSTANTS
            .iter()
            .map(|(name, _)| *name)
            .chain(NUMBER_CONSTANTS.iter().map(|(name, _)| *name))
            .chain(UNARY_LIBM.iter().copied())
            .chain(BINARY_LIBM.iter().copied())
            .chain(MATH_INLINE.iter().copied())
            .chain(NUMBER_INLINE.iter().copied())
            .chain(OBJECT_INLINE.iter().copied())
            .chain(ARRAY_INLINE.iter().copied())
            .chain(DATE_INLINE.iter().copied())
            .chain(CONSOLE_INLINE.iter().copied())
            .collect();

        Ok(Self { names, funcs })
    }

    /// The namespace a bare identifier names.
    pub fn namespace(&self, name: &str) -> Option<Namespace> {
        match name {
            "Math" => Some(Namespace::Math),
            "Number" => Some(Namespace::Number),
            "Object" => Some(Namespace::Object),
            "Array" => Some(Namespace::Array),
            "Date" => Some(Namespace::Date),
            "console" => Some(Namespace::Console),
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
            "String" => Some("String"),
            "Array" => Some("Array"),
            "Map" => Some("Map"),
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

    /// The `typeof` result for one of the engine's builtin globals. `Math` and
    /// `console` are plain objects, so every other namespace names a function.
    pub fn typeof_name(&self, name: &str) -> Option<&'static str> {
        if self.is_undefined(name) {
            Some("undefined")
        } else if self.global_constant(name).is_some() {
            Some("number")
        } else if self.global_function(name).is_some()
            || matches!(
                self.namespace(name),
                Some(Namespace::Object | Namespace::Date)
            )
        {
            Some("function")
        } else if self.namespace(name).is_some() {
            Some("object")
        } else {
            None
        }
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
            Namespace::Array => ARRAY_INLINE.contains(&name),
            Namespace::Date => DATE_INLINE.contains(&name),
            Namespace::Console => CONSOLE_INLINE.contains(&name),
        }
    }

    /// The constant folded into the program for `namespace.member`, if any.
    pub fn constant(&self, namespace: Namespace, name: &str) -> Option<f64> {
        let table = match namespace {
            Namespace::Math => MATH_CONSTANTS,
            Namespace::Number => NUMBER_CONSTANTS,
            Namespace::Object | Namespace::Array | Namespace::Date | Namespace::Console => {
                return None;
            }
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

fn make_nullary_i64_signature(module: &mut ObjectModule) -> Signature {
    let mut sig = module.make_signature();
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn make_i64_pair_to_i64_signature(module: &mut ObjectModule) -> Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn make_f64_predicate_signature(module: &mut ObjectModule) -> Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::F64));
    sig.returns.push(AbiParam::new(types::I8));
    sig
}

fn make_f64_binary_to_i32_signature(module: &mut ObjectModule) -> Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::F64));
    sig.params.push(AbiParam::new(types::F64));
    sig.returns.push(AbiParam::new(types::I32));
    sig
}

fn make_i64_pair_void_signature(module: &mut ObjectModule) -> Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig
}
