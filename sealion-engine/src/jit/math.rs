//! Host helpers for the `Math` namespace object.
//!
//! `Math` is a compile-time namespace for the compiler (`builtins::math` emits
//! the same operations inline), and a plain [`JsObject`] at run time, so that
//! `var m = Math`, `var floor = Math.floor` and `Object.keys(Math)` work. Each
//! member is a native closure over a shared entry point whose `env` is the
//! index of the operation in [`OPS`], so `Math.floor` compares equal only to
//! itself, like every other native method in the runtime.

use std::sync::OnceLock;

use super::arrays::{arg, arg_count};
use super::objects::{define_property, native_function, new_object};
use super::strings::to_number_value;

enum Op {
    Unary(fn(f64) -> f64),
    Binary(fn(f64, f64) -> f64),
    Min,
    Max,
    Hypot,
    Clz32,
    Imul,
    Random,
}

const OPS: &[(&str, Op)] = &[
    ("abs", Op::Unary(f64::abs)),
    ("ceil", Op::Unary(f64::ceil)),
    ("floor", Op::Unary(f64::floor)),
    ("round", Op::Unary(js_round)),
    ("trunc", Op::Unary(f64::trunc)),
    ("sqrt", Op::Unary(f64::sqrt)),
    ("cbrt", Op::Unary(f64::cbrt)),
    ("exp", Op::Unary(f64::exp)),
    ("expm1", Op::Unary(f64::exp_m1)),
    ("log", Op::Unary(f64::ln)),
    ("log10", Op::Unary(f64::log10)),
    ("log1p", Op::Unary(f64::ln_1p)),
    ("log2", Op::Unary(f64::log2)),
    ("sin", Op::Unary(f64::sin)),
    ("cos", Op::Unary(f64::cos)),
    ("tan", Op::Unary(f64::tan)),
    ("asin", Op::Unary(f64::asin)),
    ("acos", Op::Unary(f64::acos)),
    ("atan", Op::Unary(f64::atan)),
    ("sinh", Op::Unary(f64::sinh)),
    ("cosh", Op::Unary(f64::cosh)),
    ("tanh", Op::Unary(f64::tanh)),
    ("asinh", Op::Unary(f64::asinh)),
    ("acosh", Op::Unary(f64::acosh)),
    ("atanh", Op::Unary(f64::atanh)),
    ("sign", Op::Unary(sign)),
    ("fround", Op::Unary(|x| f64::from(x as f32))),
    ("f16round", Op::Unary(f16round)),
    ("pow", Op::Binary(f64::powf)),
    ("atan2", Op::Binary(f64::atan2)),
    ("min", Op::Min),
    ("max", Op::Max),
    ("hypot", Op::Hypot),
    ("clz32", Op::Clz32),
    ("imul", Op::Imul),
    ("random", Op::Random),
];

/// `Math.round`: half up, but `-0` for a negative result that rounds to zero.
fn js_round(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let floor = x.floor();
    let rounded = if x - floor >= 0.5 { floor + 1.0 } else { floor };
    if rounded == 0.0 && x < 0.0 {
        -0.0
    } else {
        rounded
    }
}

fn sign(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 {
        x
    } else if x < 0.0 {
        -1.0
    } else {
        1.0
    }
}

/// `Math.f16round`: round to the nearest half precision value, ties to even.
fn f16round(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    // A half has 10 mantissa bits and an exponent bias of 15, so the quantum is
    // `2**(e-10)`, or `2**-24` below the smallest normal.
    let exponent = ((x.abs() as f32).to_bits() >> 23) as i32 - 127;
    let quantum = if exponent < -14 {
        (-24.0f64).exp2()
    } else {
        (exponent - 10) as f64
    };
    let quantum = if exponent < -14 {
        quantum
    } else {
        quantum.exp2()
    };
    let rounded = (x / quantum).round_ties_even() * quantum;
    if rounded.abs() >= 65_520.0 {
        if x < 0.0 {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        }
    } else {
        rounded
    }
}

/// `ToUint32`.
fn to_uint32(value: f64) -> u32 {
    let number = to_number_value(value).trunc();
    if !number.is_finite() {
        return 0;
    }
    number.rem_euclid(4_294_967_296.0) as u32
}

fn min_max(args: *const u8, maximum: bool) -> f64 {
    let count = arg_count(args);
    if count == 0 {
        return if maximum {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let mut result = to_number_value(arg(args, 0));
    for index in 1..count {
        let value = to_number_value(arg(args, index));
        result = if value.is_nan() || result.is_nan() {
            f64::NAN
        } else if value == 0.0 && result == 0.0 {
            let negative = if maximum {
                value.is_sign_negative() && result.is_sign_negative()
            } else {
                value.is_sign_negative() || result.is_sign_negative()
            };
            if negative { -0.0 } else { 0.0 }
        } else if maximum {
            if value > result { value } else { result }
        } else if value < result {
            value
        } else {
            result
        };
    }
    result
}

fn hypot(args: *const u8) -> f64 {
    let count = arg_count(args);
    let mut scale = 0.0f64;
    let mut sum = 0.0f64;
    let mut saw_infinite = false;
    for index in 0..count {
        let value = to_number_value(arg(args, index)).abs();
        if value.is_nan() {
            return f64::NAN;
        }
        if value.is_infinite() {
            saw_infinite = true;
            continue;
        }
        if value > scale {
            sum = sum * (scale / value) * (scale / value) + 1.0;
            scale = value;
        } else if value > 0.0 {
            sum += (value / scale) * (value / scale);
        }
    }
    if saw_infinite {
        return f64::INFINITY;
    }
    scale * sum.sqrt()
}

extern "C" fn math_method(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let Some((_, op)) = OPS.get(env as usize) else {
        return f64::NAN;
    };
    match op {
        Op::Unary(function) => function(to_number_value(arg(args, 0))),
        Op::Binary(function) => {
            function(to_number_value(arg(args, 0)), to_number_value(arg(args, 1)))
        }
        Op::Min => min_max(args, false),
        Op::Max => min_max(args, true),
        Op::Hypot => hypot(args),
        Op::Clz32 => f64::from(to_uint32(arg(args, 0)).leading_zeros()),
        Op::Imul => {
            let lhs = to_uint32(arg(args, 0)) as i32;
            let rhs = to_uint32(arg(args, 1)) as i32;
            f64::from(lhs.wrapping_mul(rhs))
        }
        Op::Random => unsafe { drand48() },
    }
}

unsafe extern "C" {
    fn drand48() -> f64;
}

/// The `Math` namespace object: every constant and every function as a value.
extern "C" fn sealion_math_object() -> u64 {
    static OBJECT: OnceLock<usize> = OnceLock::new();
    *OBJECT.get_or_init(|| {
        let pointer = new_object();
        for (name, value) in crate::builtins::MATH_CONSTANTS {
            define_property(pointer, name, *value);
        }
        for (index, (name, _)) in OPS.iter().enumerate() {
            define_property(pointer, name, native_function(math_method, index as u64));
        }
        pointer as usize
    }) as u64
}

pub(super) fn lookup(name: &str) -> Option<usize> {
    Some(match name {
        "sealion_math_object" => sealion_math_object as *const () as usize,
        _ => return None,
    })
}
