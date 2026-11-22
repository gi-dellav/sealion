use std::collections::HashMap;

use cranelift_codegen::ir::{FuncRef, InstBuilder, Value, condcodes::FloatCC, types};
use cranelift_frontend::FunctionBuilder;

use super::{
    BINARY_LIBM, MAX_SAFE_INTEGER, Namespace,
    math::{f16round, fround, hypot, is_integer, js_round, min_or_max, sign},
};
use crate::{Error, value};

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
