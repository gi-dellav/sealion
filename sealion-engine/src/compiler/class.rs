//! `class` declarations and expressions, and the `super` keyword.
//!
//! A class lowers to a function value — the constructor — whose lazily created
//! `prototype` object receives the instance methods and which receives the
//! static ones. `extends B` sets `C.prototype`'s prototype to `B.prototype` and
//! `C`'s own prototype to `B`, so `instanceof` works through the ordinary
//! prototype chain and `C extends B` inherits the static methods for free.
//!
//! The parent constructor and its prototype are stored in two anonymous slots of
//! a scope that encloses every method, which is how `super(...)`, `super.x` and
//! `super.x()` find their target. `super(...)` calls the parent with the
//! receiver the `new` expression already allocated, which is equivalent to the
//! specification for the prototype-chain object model used here.

use cranelift_codegen::ir::{InstBuilder, Value};
use cranelift_frontend::FunctionBuilder;
use oxc_ast::ast::Expression;

use super::expr::compile_expr;
use super::object;
use super::support::{enter_scope, hidden_value, node_scope, store_at, store_var};
use super::{CodegenCtx, Flow};
use crate::{
    Error,
    plan::{ClassPlan, SuperHome},
    value,
};

fn ops(ctx: &CodegenCtx<'_>) -> Result<value::ObjectOps, Error> {
    ctx.objects
        .ok_or_else(|| Error::Unsupported("classes require the dynamic runtime".to_string()))
}

/// Compiles a `class` node into its constructor function value, installing the
/// methods on the prototype object and on the constructor itself.
pub(super) fn compile_class<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    key: usize,
) -> Result<Value, Error> {
    let plan = ctx
        .program
        .classes
        .get(&key)
        .ok_or_else(|| Error::Codegen("missing class plan".to_string()))?
        .clone();
    if ctx.objects.is_none() {
        return Err(Error::Unsupported(
            "classes require the dynamic runtime".to_string(),
        ));
    }

    let inner = enter_scope(builder, ctx, node_scope(ctx, key)?);

    let mut parent_value = value::undefined(builder);
    let mut parent_proto = parent_value;
    if let Some(expression) = plan.parent {
        parent_value = compile_expr(builder, &inner, flow, expression)?;
        let prototype_key = object::static_key(builder, &inner, "prototype")?;
        parent_proto = object::get(builder, &inner, parent_value, prototype_key)?;
    }
    store_hidden(builder, &inner, &plan, plan.ctor_slot, parent_value)?;
    store_hidden(builder, &inner, &plan, plan.proto_slot, parent_proto)?;

    let constructor = super::call::compile_closure_of(builder, &inner, plan.ctor)?;
    if let Some(name) = plan.name_binding {
        store_var(builder, &inner, name, constructor)?;
    }

    let prototype_key = object::static_key(builder, &inner, "prototype")?;
    let prototype = object::get(builder, &inner, constructor, prototype_key)?;

    if plan.parent.is_some() {
        let ops = ops(ctx)?;
        let call = builder
            .ins()
            .call(ops.set_prototype, &[prototype, parent_proto]);
        let _ = builder.inst_results(call);
        let call = builder
            .ins()
            .call(ops.set_prototype, &[constructor, parent_value]);
        let _ = builder.inst_results(call);
    }

    for method in &plan.methods {
        let closure = super::call::compile_closure_of(builder, &inner, method.plan)?;
        let target = if method.is_static {
            constructor
        } else {
            prototype
        };
        let key = object::static_key(builder, &inner, method.name)?;
        object::set(builder, &inner, target, key, closure)?;
    }

    Ok(constructor)
}

/// Writes one of the class's anonymous slots. `ctx` has just entered the class
/// scope, so the slot is at a fixed offset from the current environment.
fn store_hidden(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    plan: &ClassPlan<'_>,
    slot: usize,
    value: Value,
) -> Result<(), Error> {
    debug_assert_eq!(ctx.scope, plan.scope);
    debug_assert!(slot < ctx.program.scope(plan.scope).slot_count());
    store_at(builder, ctx.env, (8 + 8 * slot) as i32, value);
    Ok(())
}

/// The `super` target of the current function: the parent prototype for an
/// instance member access, the parent constructor for a static one.
pub(super) fn super_target(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
) -> Result<Value, Error> {
    let home = super_home(ctx)?;
    let slot = if home.is_static {
        home.ctor_slot
    } else {
        home.proto_slot
    };
    hidden_value(builder, ctx, home.scope, slot)
}

/// The parent constructor value, for `super(...)`.
pub(super) fn super_constructor(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
) -> Result<Value, Error> {
    let home = super_home(ctx)?;
    if ctx.objects.is_none() {
        return Err(Error::Unsupported(
            "`super` requires the dynamic runtime".to_string(),
        ));
    }
    hidden_value(builder, ctx, home.scope, home.ctor_slot)
}

/// The class a `super` reference belongs to: the nearest enclosing function
/// with a class home, so that an arrow (or a nested function) inside a method
/// resolves `super` through the method.
fn super_home(ctx: &CodegenCtx<'_>) -> Result<SuperHome, Error> {
    let mut current = Some(ctx.plan_id);
    while let Some(id) = current {
        let plan = ctx.program.function(id);
        if let Some(home) = plan.super_home {
            return Ok(home);
        }
        current = plan.parent;
    }
    Err(Error::Unsupported(
        "`super` outside of a class is not supported".to_string(),
    ))
}

/// `true` when the expression is the `super` keyword.
pub(super) fn is_super(expression: &Expression<'_>) -> bool {
    matches!(expression, Expression::Super(_))
}
