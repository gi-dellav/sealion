use cranelift_codegen::ir::{InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;
use oxc_ast::ast::{Argument, CallExpression, Expression, StaticMemberExpression};

use super::expr::compile_expr;
use super::flow::emit_exception_check;
use super::numeric::expr_is_number;
use super::support::{call_calloc, new_closure};
use super::{CodegenCtx, Flow};
use crate::{Error, builtins, plan::resolve, value};

const MAX_ARGS: usize = 32;

pub(super) fn is_free_global(ctx: &CodegenCtx<'_>, name: &str) -> bool {
    resolve(ctx.program, ctx.scope, name).is_none()
}

/// Recognises `Math.member`, `Number.member` and `Object.member`.
pub(super) fn builtin_member(
    ctx: &CodegenCtx<'_>,
    access: &StaticMemberExpression<'_>,
) -> Option<(builtins::Namespace, &'static str)> {
    let Expression::Identifier(id) = &access.object else {
        return None;
    };
    let name = id.name.as_str();
    if !is_free_global(ctx, name) {
        return None;
    }
    let namespace = ctx.builtins.namespace(name)?;
    let name = ctx.builtins.property(access.property.name.as_str())?;
    ctx.builtins
        .has(namespace, name)
        .then_some((namespace, name))
}

pub(super) enum BuiltinCallee {
    Global(&'static str),
    Member(builtins::Namespace, &'static str),
}

/// Compiles `Math.foo(...)`, `Number.foo(...)`, `Object.foo(...)`, `Date.now()`
/// and the global conversion/predicate functions.
pub(super) fn compile_builtin_call<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    callee: BuiltinCallee,
    args: &'a [Argument<'a>],
) -> Result<Value, Error> {
    let (namespace, name) = match callee {
        BuiltinCallee::Global(name) => (None, name),
        BuiltinCallee::Member(namespace, name) => {
            if ctx.builtins.constant(namespace, name).is_some() {
                return Err(Error::Unsupported(format!("{name} is not a function")));
            }
            (Some(namespace), name)
        }
    };

    let mut values = Vec::with_capacity(args.len());
    let mut known_numbers = Vec::with_capacity(args.len());
    for arg in args {
        let Some(expr) = arg.as_expression() else {
            return Err(Error::Unsupported(
                "spread arguments are not supported".to_string(),
            ));
        };
        values.push(compile_expr(builder, ctx, flow, expr)?);
        known_numbers.push(expr_is_number(ctx, expr));
    }
    builtins::call(
        builder,
        ctx.builtin_refs,
        ctx.fmod_ref,
        namespace,
        name,
        &values,
        &known_numbers,
        ctx.strings.as_ref(),
    )
}

/// Recognises a call to a builtin member or a free global builtin.
pub(super) fn builtin_callee(
    ctx: &CodegenCtx<'_>,
    callee: &Expression<'_>,
) -> Option<BuiltinCallee> {
    match callee {
        Expression::StaticMemberExpression(access) => builtin_member(ctx, access)
            .map(|(namespace, name)| BuiltinCallee::Member(namespace, name)),
        Expression::Identifier(id) if is_free_global(ctx, id.name.as_str()) => ctx
            .builtins
            .global_function(id.name.as_str())
            .map(BuiltinCallee::Global),
        _ => None,
    }
}

pub(super) fn compile_property_access<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    access: &'a StaticMemberExpression<'a>,
) -> Result<Value, Error> {
    let constant = builtin_member(ctx, access)
        .and_then(|(namespace, name)| ctx.builtins.constant(namespace, name));
    if let Some(value) = constant {
        return Ok(builder.ins().f64const(value));
    }
    if access.property.name.as_str() == "length"
        && let Some(ops) = ctx.strings.as_ref()
    {
        let object = compile_expr(builder, ctx, flow, &access.object)?;
        let call = builder.ins().call(ops.length, &[object]);
        return Ok(builder.inst_results(call)[0]);
    }
    Err(Error::Unsupported(
        "only builtin constants and string length can be read as properties".to_string(),
    ))
}

pub(super) fn compile_call<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    call: &'a CallExpression<'a>,
) -> Result<Value, Error> {
    if let Some(builtin) = builtin_callee(ctx, &call.callee) {
        return compile_builtin_call(builder, ctx, flow, builtin, call.arguments.as_slice());
    }

    let callee = compile_expr(builder, ctx, flow, &call.callee)?;
    compile_call_value(builder, ctx, flow, callee, &call.arguments)
}

pub(super) fn compile_call_value<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    callee: Value,
    args: &'a [Argument<'a>],
) -> Result<Value, Error> {
    if args.len() > MAX_ARGS {
        return Err(Error::Unsupported(format!(
            "calls with more than {MAX_ARGS} arguments are not supported"
        )));
    }

    let callee_ptr = value::payload(builder, callee);
    let code = builder
        .ins()
        .load(types::I64, MemFlags::new(), callee_ptr, 0i32);
    let captured_env = builder
        .ins()
        .load(types::I64, MemFlags::new(), callee_ptr, 8i32);

    let args_ptr = call_calloc(builder, ctx.calloc_ref, 8 + 8 * MAX_ARGS as i64);
    let len = builder.ins().iconst(types::I64, args.len() as i64);
    builder.ins().store(MemFlags::new(), len, args_ptr, 0i32);

    for (i, arg) in args.iter().enumerate() {
        let Some(expr) = arg.as_expression() else {
            return Err(Error::Unsupported(
                "spread arguments are not supported".to_string(),
            ));
        };
        let value = compile_expr(builder, ctx, flow, expr)?;
        builder
            .ins()
            .store(MemFlags::new(), value, args_ptr, (8 + 8 * i) as i32);
    }

    let call = builder
        .ins()
        .call_indirect(ctx.js_sig_ref, code, &[captured_env, args_ptr, callee]);
    let result = builder.inst_results(call)[0];
    emit_exception_check(builder, ctx, flow)?;
    Ok(result)
}

/// Materialises a function or arrow expression as a closure over the current
/// environment.
pub(super) fn compile_closure(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    key: usize,
) -> Result<Value, Error> {
    let child = *ctx
        .program
        .function(ctx.plan_id)
        .nested
        .get(&key)
        .ok_or_else(|| Error::Codegen("missing anonymous function plan".to_string()))?;
    let func_ref = *ctx
        .child_refs
        .get(&child)
        .ok_or_else(|| Error::Codegen("missing anonymous function reference".to_string()))?;
    let code = builder.ins().func_addr(types::I64, func_ref);
    Ok(new_closure(builder, ctx.calloc_ref, code, ctx.env))
}
