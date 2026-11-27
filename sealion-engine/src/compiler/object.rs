//! Object literals and property access for the dynamic runtime.
//!
//! An object is a [`crate::value::ObjectOps`] handle to a host-side
//! own-property table; codegen never sees the layout. Every access is a host
//! call, which keeps the object model in one place (`jit::objects`) and lets it
//! grow without changing the ABI.

use cranelift_codegen::ir::{InstBuilder, Value};
use cranelift_frontend::FunctionBuilder;
use oxc_ast::ast::{ObjectExpression, ObjectPropertyKind, PropertyKey};

use super::expr::{compile_expr, compile_string_data};
use super::{CodegenCtx, Flow};
use crate::{Error, value};

fn ops(ctx: &CodegenCtx<'_>) -> Result<value::ObjectOps, Error> {
    ctx.objects
        .ok_or_else(|| Error::Unsupported("object values require the dynamic runtime".to_string()))
}

/// Reads `object[key]`.
pub(super) fn get(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    object: Value,
    key: Value,
) -> Result<Value, Error> {
    let call = builder.ins().call(ops(ctx)?.get, &[object, key]);
    Ok(builder.inst_results(call)[0])
}

/// Writes `object[key] = value` and yields `value`.
pub(super) fn set(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    object: Value,
    key: Value,
    value: Value,
) -> Result<Value, Error> {
    let call = builder.ins().call(ops(ctx)?.set, &[object, key, value]);
    Ok(builder.inst_results(call)[0])
}

/// `key in object`.
pub(super) fn has(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    object: Value,
    key: Value,
) -> Result<Value, Error> {
    let call = builder.ins().call(ops(ctx)?.has, &[object, key]);
    Ok(builder.inst_results(call)[0])
}

/// `delete object[key]`.
pub(super) fn delete(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    object: Value,
    key: Value,
) -> Result<Value, Error> {
    let call = builder.ins().call(ops(ctx)?.delete, &[object, key]);
    Ok(builder.inst_results(call)[0])
}

/// The key of a statically named property, e.g. `o.length` or `{a: 1}`.
pub(super) fn static_key(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    name: &str,
) -> Result<Value, Error> {
    compile_string_data(builder, ctx, name)
}

/// The key of an object-literal property: a static name where the property has
/// one, otherwise the evaluated computed key (the host applies `ToPropertyKey`).
fn literal_key<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    key: &'a PropertyKey<'a>,
) -> Result<Value, Error> {
    if let Some(name) = key.static_name() {
        return static_key(builder, ctx, &name.display().to_string());
    }
    let expression = key
        .as_expression()
        .ok_or_else(|| Error::Unsupported("unsupported property key".to_string()))?;
    compile_expr(builder, ctx, flow, expression)
}

/// Compiles `{ ... }` into a fresh object, defining the properties in source
/// order so that insertion order and evaluation order both match the spec.
pub(super) fn compile_object_literal<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    object: &'a ObjectExpression<'a>,
) -> Result<Value, Error> {
    let new = ops(ctx)?.new;
    let call = builder.ins().call(new, &[]);
    let pointer = builder.inst_results(call)[0];
    let result = value::object(builder, pointer);

    for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return Err(Error::Unsupported(
                "object spread is not supported".to_string(),
            ));
        };
        if property.kind.is_accessor() {
            return Err(Error::Unsupported(
                "getters and setters are not supported".to_string(),
            ));
        }
        if property.key.is_specific_static_name("__proto__") {
            return Err(Error::Unsupported("__proto__ is not supported".to_string()));
        }
        let key = literal_key(builder, ctx, flow, &property.key)?;
        let value = compile_expr(builder, ctx, flow, &property.value)?;
        set(builder, ctx, result, key, value)?;
    }

    Ok(result)
}
