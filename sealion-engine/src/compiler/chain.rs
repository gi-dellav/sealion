use cranelift_codegen::ir::{Block, InstBuilder, Value, types};
use cranelift_frontend::{FunctionBuilder, Variable};
use oxc_ast::ast::{
    CallExpression, ChainElement, ChainExpression, Expression, StaticMemberExpression,
};

use super::call::{
    builtin_callee, builtin_member, compile_builtin_call, compile_call_value,
    compile_property_access,
};
use super::expr::compile_expr;
use super::{CodegenCtx, Flow};
use crate::{Error, value};

pub(super) fn compile_chain<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    chain: &'a ChainExpression<'a>,
) -> Result<Value, Error> {
    let short = builder.create_block();
    let join = builder.create_block();
    let result = builder.declare_var(types::F64);
    let value = compile_chain_element(builder, ctx, flow, &chain.expression, short, result)?;
    builder.def_var(result, value);
    builder.ins().jump(join, &[]);

    builder.switch_to_block(short);
    let undefined = value::undefined(builder);
    builder.def_var(result, undefined);
    builder.ins().jump(join, &[]);

    builder.switch_to_block(join);
    Ok(builder.use_var(result))
}

pub(super) fn compile_chain_element<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    element: &'a ChainElement<'a>,
    short: Block,
    result: Variable,
) -> Result<Value, Error> {
    match element {
        ChainElement::CallExpression(call) => {
            compile_chain_call(builder, ctx, flow, call, short, result)
        }
        ChainElement::StaticMemberExpression(access) => {
            compile_chain_static_member(builder, ctx, flow, access, short, result)
        }
        ChainElement::ComputedMemberExpression(member) => {
            compile_chain_computed_member(builder, ctx, flow, member, short, result)
        }
        other => Err(Error::Unsupported(format!(
            "optional chain element not supported: {other:?}"
        ))),
    }
}

pub(super) fn compile_chain_expr<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    expr: &'a Expression<'a>,
    short: Block,
    result: Variable,
) -> Result<Value, Error> {
    match expr {
        Expression::CallExpression(call) => {
            compile_chain_call(builder, ctx, flow, call, short, result)
        }
        Expression::StaticMemberExpression(access) => {
            compile_chain_static_member(builder, ctx, flow, access, short, result)
        }
        Expression::ChainExpression(chain) => compile_chain(builder, ctx, flow, chain),
        Expression::ComputedMemberExpression(member) => {
            compile_chain_computed_member(builder, ctx, flow, member, short, result)
        }
        Expression::PrivateFieldExpression(_) => Err(Error::Unsupported(
            "private member access requires objects".to_string(),
        )),
        other => compile_expr(builder, ctx, flow, other),
    }
}

pub(super) fn compile_chain_static_member<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    access: &'a StaticMemberExpression<'a>,
    short: Block,
    result: Variable,
) -> Result<Value, Error> {
    if builtin_member(ctx, access).is_some() {
        return compile_property_access(builder, ctx, flow, access);
    }

    let object = compile_chain_expr(builder, ctx, flow, &access.object, short, result)?;
    if access.optional {
        let ok = builder.create_block();
        let nullish = value::is_nullish(builder, object);
        builder.ins().brif(nullish, short, &[], ok, &[]);
        builder.switch_to_block(ok);
    }
    if access.property.name.as_str() == "length"
        && let Some(ops) = ctx.strings.as_ref()
    {
        let call = builder.ins().call(ops.length, &[object]);
        return Ok(builder.inst_results(call)[0]);
    }
    Ok(value::undefined(builder))
}

pub(super) fn compile_chain_computed_member<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    member: &'a oxc_ast::ast::ComputedMemberExpression<'a>,
    short: Block,
    result: Variable,
) -> Result<Value, Error> {
    let object = compile_chain_expr(builder, ctx, flow, &member.object, short, result)?;
    if member.optional {
        let ok = builder.create_block();
        let nullish = value::is_nullish(builder, object);
        builder.ins().brif(nullish, short, &[], ok, &[]);
        builder.switch_to_block(ok);
    }
    let _ = compile_expr(builder, ctx, flow, &member.expression)?;
    Ok(value::undefined(builder))
}

pub(super) fn compile_chain_call<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    call: &'a CallExpression<'a>,
    short: Block,
    result: Variable,
) -> Result<Value, Error> {
    if let Some(builtin) = builtin_callee(ctx, &call.callee) {
        return compile_builtin_call(builder, ctx, flow, builtin, call.arguments.as_slice());
    }

    let callee = compile_chain_expr(builder, ctx, flow, &call.callee, short, result)?;
    if call.optional {
        let ok = builder.create_block();
        let nullish = value::is_nullish(builder, callee);
        builder.ins().brif(nullish, short, &[], ok, &[]);
        builder.switch_to_block(ok);
    }
    compile_call_value(builder, ctx, flow, callee, &call.arguments)
}
