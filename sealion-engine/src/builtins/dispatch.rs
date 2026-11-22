use std::collections::HashMap;

use cranelift_codegen::ir::{
    FuncRef, InstBuilder, MemFlags, StackSlotData, StackSlotKind, Value, condcodes::FloatCC, types,
};
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
    strings: Option<&value::StringOps>,
) -> Value {
    let value = argument(builder, args, index);
    if known.get(index).copied().unwrap_or(false) {
        value
    } else {
        value::to_number(builder, value, strings)
    }
}

fn console_log(
    builder: &mut FunctionBuilder,
    funcs: &HashMap<&'static str, FuncRef>,
    args: &[Value],
) -> Result<Value, Error> {
    let func_ref = *funcs
        .get("sealion_console_log")
        .ok_or_else(|| Error::Unsupported("console.log is not available".to_string()))?;
    let slots = args.len().max(1);
    let size = u32::try_from(slots * 8).unwrap_or(u32::MAX);
    let slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, size, 3));
    let base = builder.ins().stack_addr(types::I64, slot, 0);
    for (index, &arg) in args.iter().enumerate() {
        builder
            .ins()
            .store(MemFlags::new(), arg, base, (index * 8) as i32);
    }
    let len = builder.ins().iconst(types::I64, args.len() as i64);
    builder.ins().call(func_ref, &[base, len]);
    Ok(value::undefined(builder))
}

fn call_libm(builder: &mut FunctionBuilder, func_ref: FuncRef, args: &[Value]) -> Value {
    let call = builder.ins().call(func_ref, args);
    builder.inst_results(call)[0]
}

/// Compiles `Math.foo(...)`, `Number.foo(...)`, `Object.foo(...)` and the free
/// global builtins, with `args` already evaluated.
#[allow(clippy::too_many_arguments)]
pub fn call(
    builder: &mut FunctionBuilder,
    funcs: &HashMap<&'static str, FuncRef>,
    fmod_ref: FuncRef,
    namespace: Option<Namespace>,
    name: &str,
    args: &[Value],
    known: &[bool],
    strings: Option<&value::StringOps>,
) -> Result<Value, Error> {
    if namespace == Some(Namespace::Console) {
        if strings.is_none() {
            return Err(Error::Unsupported(
                "console.log requires the dynamic runtime".to_string(),
            ));
        }
        return console_log(builder, funcs, args);
    }
    let value = match name {
        "abs" => {
            let x = number_argument(builder, args, 0, known, strings);
            builder.ins().fabs(x)
        }
        "ceil" => {
            let x = number_argument(builder, args, 0, known, strings);
            builder.ins().ceil(x)
        }
        "floor" => {
            let x = number_argument(builder, args, 0, known, strings);
            builder.ins().floor(x)
        }
        "round" => {
            let x = number_argument(builder, args, 0, known, strings);
            js_round(builder, x)
        }
        "trunc" => {
            let x = number_argument(builder, args, 0, known, strings);
            builder.ins().trunc(x)
        }
        "sqrt" => {
            let x = number_argument(builder, args, 0, known, strings);
            builder.ins().sqrt(x)
        }
        "fround" => {
            let x = number_argument(builder, args, 0, known, strings);
            fround(builder, x)
        }
        "min" => min_or_max(builder, args, false, known, strings),
        "max" => min_or_max(builder, args, true, known, strings),
        "hypot" => hypot(builder, args, known, strings),
        "f16round" => {
            let x = number_argument(builder, args, 0, known, strings);
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
            let x = number_argument(builder, args, 0, known, strings);
            sign(builder, x)
        }
        "clz32" => {
            let x = argument(builder, args, 0);
            let int = value::to_int32(builder, fmod_ref, x, strings);
            let leading = builder.ins().clz(int);
            let wide = builder.ins().uextend(types::I64, leading);
            builder.ins().fcvt_from_uint(types::F64, wide)
        }
        "imul" => {
            let lhs = argument(builder, args, 0);
            let rhs = argument(builder, args, 1);
            let lhs = value::to_int32(builder, fmod_ref, lhs, strings);
            let rhs = value::to_int32(builder, fmod_ref, rhs, strings);
            let product = builder.ins().imul(lhs, rhs);
            value::from_int32(builder, product)
        }
        "isNaN" => {
            let x = if namespace == Some(Namespace::Number) {
                argument(builder, args, 0)
            } else {
                number_argument(builder, args, 0, known, strings)
            };
            let nan = value::is_nan(builder, x);
            value::from_bool(builder, nan)
        }
        "isFinite" => {
            let x = if namespace == Some(Namespace::Number) {
                argument(builder, args, 0)
            } else {
                number_argument(builder, args, 0, known, strings)
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
            value::same_value(builder, lhs, rhs, strings)
        }
        "String" => {
            let ops = strings.ok_or_else(|| {
                Error::Unsupported("String() requires the dynamic runtime".to_string())
            })?;
            if args.is_empty() {
                let null = builder.ins().iconst(types::I64, 0);
                let call = builder.ins().call(ops.from_utf8, &[null, null]);
                let raw = builder.inst_results(call)[0];
                value::string(builder, raw)
            } else {
                let x = argument(builder, args, 0);
                value::to_string(builder, x, Some(ops)).unwrap()
            }
        }
        "Boolean" => {
            let x = argument(builder, args, 0);
            let truthy = value::truthy(builder, x, strings);
            value::from_bool(builder, truthy)
        }
        "Number" => number_argument(builder, args, 0, known, strings),
        other => {
            let func_ref = *funcs
                .get(other)
                .ok_or_else(|| Error::Unsupported(format!("unknown builtin function: {other}")))?;
            if BINARY_LIBM.contains(&other) {
                let lhs = number_argument(builder, args, 0, known, strings);
                let rhs = number_argument(builder, args, 1, known, strings);
                call_libm(builder, func_ref, &[lhs, rhs])
            } else {
                let x = number_argument(builder, args, 0, known, strings);
                call_libm(builder, func_ref, &[x])
            }
        }
    };
    Ok(value)
}
