use cranelift_codegen::ir::{
    FuncRef, InstBuilder, MemFlags, StackSlotData, StackSlotKind, Value, types,
};
use cranelift_frontend::FunctionBuilder;
use oxc_ast::ast::{
    Argument, CallExpression, ComputedMemberExpression, Expression, StaticMemberExpression,
};

use super::expr::compile_expr;
use super::flow::emit_exception_check;
use super::numeric::expr_is_number;
use super::object;
use super::support::{load_var, new_closure};
use super::{CodegenCtx, Flow};
use crate::{
    Error, builtins,
    plan::{resolve, resolve_binding},
    value,
};

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

/// The namespace object a bare `Math`, `String` or `Date` names, or `None` for
/// a namespace that has no object and outside the dynamic runtime.
pub(super) fn compile_namespace_value(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    namespace: builtins::Namespace,
) -> Result<Option<Value>, Error> {
    if ctx.objects.is_none() {
        return Ok(None);
    }
    let symbol = match namespace {
        builtins::Namespace::Math => "sealion_math_object",
        builtins::Namespace::String => "sealion_string_object",
        builtins::Namespace::Date => "sealion_date_object",
        _ => return Ok(None),
    };
    let Some(&func_ref) = ctx.builtin_refs.get(symbol) else {
        return Ok(None);
    };
    let call = builder.ins().call(func_ref, &[]);
    let pointer = builder.inst_results(call)[0];
    Ok(Some(value::object(builder, pointer)))
}

/// Compiles `Math.foo(...)`, `Number.foo(...)`, `Object.foo(...)`, `Date.now()`
/// and the global conversion/predicate functions. `construct` is true for the
/// callee of a `new` expression.
pub(super) fn compile_builtin_call<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    callee: BuiltinCallee,
    args: &'a [Argument<'a>],
    construct: bool,
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
        construct,
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
    if super::class::is_super(&access.object) {
        let target = super::class::super_target(builder, ctx)?;
        let key = object::static_key(builder, ctx, access.property.name.as_str())?;
        return object::get(builder, ctx, target, key);
    }
    let constant = builtin_member(ctx, access)
        .and_then(|(namespace, name)| ctx.builtins.constant(namespace, name));
    if let Some(value) = constant {
        return Ok(builder.ins().f64const(value));
    }
    if let Expression::Identifier(id) = &access.object
        && is_free_global(ctx, id.name.as_str())
        && let Some(namespace) = ctx.builtins.namespace(id.name.as_str())
    {
        // `Object.prototype`, `Array.prototype` and `Date.prototype` are
        // namespace members that are objects rather than constants or native
        // functions.
        if access.property.name.as_str() == "prototype"
            && matches!(
                namespace,
                builtins::Namespace::Object
                    | builtins::Namespace::Array
                    | builtins::Namespace::Date
            )
        {
            return builtins::call(
                builder,
                ctx.builtin_refs,
                ctx.fmod_ref,
                Some(namespace),
                "prototype",
                &[],
                &[],
                ctx.strings.as_ref(),
                false,
            );
        }
        // `Math`, `String` and `Date` are objects of the dynamic runtime, so a
        // member reads through the ordinary property path there. An unknown
        // member stays a compile error in both modes.
        if let Some(member) = ctx.builtins.property(access.property.name.as_str())
            && ctx.builtins.has(namespace, member)
            && matches!(
                namespace,
                builtins::Namespace::Math | builtins::Namespace::String | builtins::Namespace::Date
            )
        {
            let object = compile_namespace_value(builder, ctx, namespace)?.ok_or_else(|| {
                Error::Unsupported(format!(
                    "{}.{member} requires the dynamic runtime",
                    id.name.as_str()
                ))
            })?;
            let key = object::static_key(builder, ctx, member)?;
            return object::get(builder, ctx, object, key);
        }
        return Err(Error::Unsupported(format!(
            "unsupported builtin member: {}.{}",
            id.name.as_str(),
            access.property.name.as_str()
        )));
    }
    let object = compile_expr(builder, ctx, flow, &access.object)?;
    if ctx.objects.is_none() {
        // Without objects a property read is always `undefined`.
        return Ok(value::undefined(builder));
    }
    let key = object::static_key(builder, ctx, access.property.name.as_str())?;
    object::get(builder, ctx, object, key)
}

/// `object[key]`, where both sides are arbitrary expressions.
pub(super) fn compile_computed_access<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    member: &'a ComputedMemberExpression<'a>,
) -> Result<Value, Error> {
    let object = compile_expr(builder, ctx, flow, &member.object)?;
    let key = compile_expr(builder, ctx, flow, &member.expression)?;
    object::get(builder, ctx, object, key)
}

pub(super) fn compile_call<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    call: &'a CallExpression<'a>,
) -> Result<Value, Error> {
    if let Some(builtin) = builtin_callee(ctx, &call.callee) {
        return compile_builtin_call(
            builder,
            ctx,
            flow,
            builtin,
            call.arguments.as_slice(),
            false,
        );
    }

    // A namespace object member the engine does not know stays a compile error,
    // so a typo cannot reach an indirect call through `undefined`.
    if let Expression::StaticMemberExpression(access) = &call.callee
        && let Expression::Identifier(id) = &access.object
        && is_free_global(ctx, id.name.as_str())
        && let Some(namespace) = ctx.builtins.namespace(id.name.as_str())
        && !ctx.builtins.has(namespace, access.property.name.as_str())
    {
        return Err(Error::Unsupported(format!(
            "unsupported builtin member: {}.{}",
            id.name.as_str(),
            access.property.name.as_str()
        )));
    }

    if super::class::is_super(&call.callee) {
        // `super(...)` invokes the parent constructor with the current `this`.
        let callee = super::class::super_constructor(builder, ctx)?;
        let this = load_var(builder, ctx, "this")?;
        return compile_call_value(builder, ctx, flow, callee, this, &call.arguments);
    }

    if let Expression::Identifier(id) = &call.callee
        && let Some(key) = resolve_binding(ctx.program, ctx.scope, id.name.as_str())
        && let Some(&callee) = ctx.program.direct_calls.bindings.get(&key)
        && ctx.program.direct_calls.safe.contains(&callee)
        && let Some(&func_ref) = ctx.child_refs.get(&callee)
    {
        return compile_direct_call(
            builder,
            ctx,
            flow,
            func_ref,
            &call.callee,
            call.arguments.as_slice(),
        );
    }

    // `object.method(...)` passes the object as `this`; every other call form
    // passes `undefined`.
    match &call.callee {
        Expression::StaticMemberExpression(access) if super::class::is_super(&access.object) => {
            let callee = compile_property_access(builder, ctx, flow, access)?;
            let this = load_var(builder, ctx, "this")?;
            return compile_call_value(builder, ctx, flow, callee, this, &call.arguments);
        }
        Expression::ComputedMemberExpression(member) if super::class::is_super(&member.object) => {
            let callee = compile_computed_access(builder, ctx, flow, member)?;
            let this = load_var(builder, ctx, "this")?;
            return compile_call_value(builder, ctx, flow, callee, this, &call.arguments);
        }
        Expression::StaticMemberExpression(access) => {
            let receiver = compile_expr(builder, ctx, flow, &access.object)?;
            let key = object::static_key(builder, ctx, access.property.name.as_str())?;
            let callee = object::get(builder, ctx, receiver, key)?;
            return compile_call_value(builder, ctx, flow, callee, receiver, &call.arguments);
        }
        Expression::ComputedMemberExpression(member) => {
            let receiver = compile_expr(builder, ctx, flow, &member.object)?;
            let key = compile_expr(builder, ctx, flow, &member.expression)?;
            let callee = object::get(builder, ctx, receiver, key)?;
            return compile_call_value(builder, ctx, flow, callee, receiver, &call.arguments);
        }
        _ => {}
    }

    let callee = compile_expr(builder, ctx, flow, &call.callee)?;
    let this = value::undefined(builder);
    compile_call_value(builder, ctx, flow, callee, this, &call.arguments)
}

/// Evaluates call arguments into a stack buffer the callee reads in its
/// prologue. The callee never retains the pointer, so no heap allocation is
/// needed.
fn compile_args<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    args: &'a [Argument<'a>],
) -> Result<Value, Error> {
    if args.len() > MAX_ARGS {
        return Err(Error::Unsupported(format!(
            "calls with more than {MAX_ARGS} arguments are not supported"
        )));
    }

    let args_size = 8 + 8 * MAX_ARGS as u32;
    let args_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        args_size,
        3,
    ));
    let args_ptr = builder.ins().stack_addr(types::I64, args_slot, 0);
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

    Ok(args_ptr)
}

fn compile_direct_call<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    func_ref: FuncRef,
    callee_expr: &'a Expression<'a>,
    args: &'a [Argument<'a>],
) -> Result<Value, Error> {
    let callee = compile_expr(builder, ctx, flow, callee_expr)?;
    let callee_ptr = value::payload(builder, callee);
    let captured_env = builder
        .ins()
        .load(types::I64, MemFlags::new(), callee_ptr, 8i32);
    let args_ptr = compile_args(builder, ctx, flow, args)?;
    let this = value::undefined(builder);
    let call = builder
        .ins()
        .call(func_ref, &[captured_env, args_ptr, callee, this]);
    let result = builder.inst_results(call)[0];
    emit_exception_check(builder, ctx, flow)?;
    Ok(result)
}

pub(super) fn compile_call_value<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    callee: Value,
    this: Value,
    args: &'a [Argument<'a>],
) -> Result<Value, Error> {
    let callee_ptr = value::payload(builder, callee);
    let code = builder
        .ins()
        .load(types::I64, MemFlags::new(), callee_ptr, 0i32);
    let captured_env = builder
        .ins()
        .load(types::I64, MemFlags::new(), callee_ptr, 8i32);
    let args_ptr = compile_args(builder, ctx, flow, args)?;

    let call = builder.ins().call_indirect(
        ctx.js_sig_ref,
        code,
        &[captured_env, args_ptr, callee, this],
    );
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
    compile_closure_of(builder, ctx, child)
}

/// Materialises the closure of a plan of this function: a class constructor or
/// method, or a nested function/arrow expression.
pub(super) fn compile_closure_of(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    child: usize,
) -> Result<Value, Error> {
    let func_ref = *ctx
        .child_refs
        .get(&child)
        .ok_or_else(|| Error::Codegen("missing anonymous function reference".to_string()))?;
    let code = builder.ins().func_addr(types::I64, func_ref);
    // Arrows inherit `this` and cannot be constructed, so they carry no
    // `prototype` property.
    let constructible = !ctx.program.function(child).is_arrow;
    Ok(new_closure(
        builder,
        ctx.calloc_ref,
        code,
        ctx.env,
        constructible,
    ))
}
