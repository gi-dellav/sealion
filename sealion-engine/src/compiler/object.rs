//! Object literals and property access for the dynamic runtime.
//!
//! An object is a [`crate::value::ObjectOps`] handle to a host-side
//! own-property table, which keeps the object model in one place
//! (`jit::objects`). A *statically named* access compiles to a guarded fast
//! path instead of a call: the site owns a 16-byte inline cache holding the
//! receiver shape it last served and the slot that shape gave the key, and a
//! same-shape receiver is read or written with two loads. Everything else — a
//! computed key, a different shape, a string, a function, an array element, a
//! prototype hit — falls back to the host helper, which refills the cache when
//! the key is an own slot.

use cranelift_codegen::ir::{BlockArg, InstBuilder, MemFlags, Value, types};
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

/// Reads `object.name`, where `name` is known at compile time.
///
/// The fast path is a tag check, a shape comparison and two loads; a miss
/// calls the host helper with the site's inline cache.
pub(super) fn get_named(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    object: Value,
    name: &str,
) -> Result<Value, Error> {
    let ops = ops(ctx)?;
    let Some(cache) = ctx.inline_cache(builder) else {
        let key = static_key(builder, ctx, name)?;
        return get(builder, ctx, object, key);
    };
    let probe = builder.create_block();
    let fast = builder.create_block();
    let slow = builder.create_block();
    let join = builder.create_block();
    builder.append_block_param(join, types::F64);

    let is_object = value::is_object(builder, object);
    builder.ins().brif(is_object, probe, &[], slow, &[]);

    builder.switch_to_block(probe);
    let pointer = value::payload(builder, object);
    let shape = builder.ins().load(
        types::I64,
        MemFlags::new(),
        pointer,
        value::OBJECT_SHAPE_OFFSET,
    );
    let cached = builder.ins().load(types::I64, MemFlags::new(), cache, 0);
    let hit = builder.ins().icmp(
        cranelift_codegen::ir::condcodes::IntCC::Equal,
        shape,
        cached,
    );
    builder.ins().brif(hit, fast, &[], slow, &[]);

    builder.switch_to_block(fast);
    let slots = builder.ins().load(
        types::I64,
        MemFlags::new(),
        pointer,
        value::OBJECT_SLOTS_OFFSET,
    );
    let index = builder.ins().load(types::I64, MemFlags::new(), cache, 8);
    let offset = builder.ins().imul_imm(index, 8);
    let address = builder.ins().iadd(slots, offset);
    let found = builder.ins().load(types::F64, MemFlags::new(), address, 0);
    builder.ins().jump(join, &[BlockArg::Value(found)]);

    builder.switch_to_block(slow);
    let key = static_key(builder, ctx, name)?;
    let call = builder.ins().call(ops.get_cached, &[object, key, cache]);
    let value = builder.inst_results(call)[0];
    builder.ins().jump(join, &[BlockArg::Value(value)]);

    builder.switch_to_block(join);
    Ok(builder.block_params(join)[0])
}

/// Writes `object.name = value`, where `name` is known at compile time.
pub(super) fn set_named(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    object: Value,
    name: &str,
    value: Value,
) -> Result<Value, Error> {
    let ops = ops(ctx)?;
    let Some(cache) = ctx.inline_cache(builder) else {
        let key = static_key(builder, ctx, name)?;
        return set(builder, ctx, object, key, value);
    };
    let probe = builder.create_block();
    let fast = builder.create_block();
    let slow = builder.create_block();
    let join = builder.create_block();
    builder.append_block_param(join, types::F64);

    let is_object = value::is_object(builder, object);
    builder.ins().brif(is_object, probe, &[], slow, &[]);

    builder.switch_to_block(probe);
    let pointer = value::payload(builder, object);
    let shape = builder.ins().load(
        types::I64,
        MemFlags::new(),
        pointer,
        value::OBJECT_SHAPE_OFFSET,
    );
    let cached = builder.ins().load(types::I64, MemFlags::new(), cache, 0);
    let hit = builder.ins().icmp(
        cranelift_codegen::ir::condcodes::IntCC::Equal,
        shape,
        cached,
    );
    builder.ins().brif(hit, fast, &[], slow, &[]);

    builder.switch_to_block(fast);
    let slots = builder.ins().load(
        types::I64,
        MemFlags::new(),
        pointer,
        value::OBJECT_SLOTS_OFFSET,
    );
    let index = builder.ins().load(types::I64, MemFlags::new(), cache, 8);
    let offset = builder.ins().imul_imm(index, 8);
    let address = builder.ins().iadd(slots, offset);
    builder.ins().store(MemFlags::new(), value, address, 0);
    builder.ins().jump(join, &[BlockArg::Value(value)]);

    builder.switch_to_block(slow);
    let key = static_key(builder, ctx, name)?;
    let call = builder
        .ins()
        .call(ops.set_cached, &[object, key, value, cache]);
    let result = builder.inst_results(call)[0];
    builder.ins().jump(join, &[BlockArg::Value(result)]);

    builder.switch_to_block(join);
    Ok(builder.block_params(join)[0])
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
        let key = literal_key(builder, ctx, flow, &property.key)?;
        let value = compile_expr(builder, ctx, flow, &property.value)?;
        set(builder, ctx, result, key, value)?;
    }

    Ok(result)
}
