use std::collections::HashMap;

use cranelift_codegen::ir::{FuncRef, InstBuilder, MemFlags, TrapCode, Value, types};
use cranelift_frontend::{FunctionBuilder, Variable};

use super::{CodegenCtx, Place};
use crate::{
    Error,
    plan::{Program, ScopeId, ScopePlan, needs_tdz_check, resolve, resolve_binding},
    value,
};

pub(super) fn call_calloc(builder: &mut FunctionBuilder, calloc_ref: FuncRef, size: i64) -> Value {
    let one = builder.ins().iconst(types::I64, 1);
    let size = builder.ins().iconst(types::I64, size);
    let call = builder.ins().call(calloc_ref, &[one, size]);
    builder.inst_results(call)[0]
}

pub(super) fn alloc_env(
    builder: &mut FunctionBuilder,
    calloc_ref: FuncRef,
    slots: usize,
    parent: Value,
) -> Value {
    let size = 8 + 8 * slots as i64;
    let env = call_calloc(builder, calloc_ref, size);
    builder.ins().store(MemFlags::new(), parent, env, 0i32);
    env
}

pub(super) fn new_closure(
    builder: &mut FunctionBuilder,
    calloc_ref: FuncRef,
    code: Value,
    env: Value,
) -> Value {
    let ptr = call_calloc(builder, calloc_ref, 16);
    builder.ins().store(MemFlags::new(), code, ptr, 0i32);
    builder.ins().store(MemFlags::new(), env, ptr, 8i32);
    value::function(builder, ptr)
}

pub(super) fn init_slots(builder: &mut FunctionBuilder, env: Value, scope: &ScopePlan<'_>) {
    for slot in 0..scope.slot_count() {
        let initial = if scope.tdz.contains(&slot) {
            value::uninitialized(builder)
        } else {
            value::undefined(builder)
        };
        builder
            .ins()
            .store(MemFlags::new(), initial, env, (8 + 8 * slot) as i32);
    }
}

/// Declares a register-backed local variable for every binding whose scope is
/// not captured by a closure.
pub(super) fn declare_vars(
    builder: &mut FunctionBuilder,
    program: &Program<'_>,
    plan_id: usize,
) -> HashMap<(ScopeId, usize), Variable> {
    let mut vars = HashMap::new();
    for (index, scope) in program.scopes.iter().enumerate() {
        if scope.owner == plan_id && !scope.has_env() {
            for slot in 0..scope.slot_count() {
                vars.insert((ScopeId(index), slot), builder.declare_var(types::F64));
            }
        }
    }
    vars
}

pub(super) fn init_local_slots(
    builder: &mut FunctionBuilder,
    vars: &HashMap<(ScopeId, usize), Variable>,
    scope_id: ScopeId,
    scope: &ScopePlan<'_>,
) {
    for slot in 0..scope.slot_count() {
        let initial = if scope.tdz.contains(&slot) {
            value::uninitialized(builder)
        } else {
            value::undefined(builder)
        };
        let var = vars[&(scope_id, slot)];
        builder.def_var(var, initial);
    }
}

/// The scope planned for a block-like node (`{ ... }`, `switch`, `for`).
pub(super) fn node_scope(ctx: &CodegenCtx<'_>, node_key: usize) -> Result<ScopeId, Error> {
    ctx.program
        .function(ctx.plan_id)
        .blocks
        .get(&node_key)
        .copied()
        .ok_or_else(|| Error::Codegen("missing block scope".to_string()))
}

/// Enters a scope, allocating a fresh environment when it owns bindings.
pub(super) fn enter_scope<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    scope: ScopeId,
) -> CodegenCtx<'a> {
    let plan = ctx.program.scope(scope);
    if plan.has_env() {
        let env = alloc_env(builder, ctx.calloc_ref, plan.slot_count(), ctx.env);
        init_slots(builder, env, plan);
        ctx.enter(scope, env)
    } else {
        init_local_slots(builder, ctx.vars, scope, plan);
        ctx.enter(scope, ctx.env)
    }
}

/// Allocates a copy of `src`'s slots, used to give a `for` loop a fresh set of
/// bindings per iteration.
pub(super) fn copy_env(
    builder: &mut FunctionBuilder,
    calloc_ref: FuncRef,
    parent: Value,
    src: Value,
    slots: usize,
) -> Value {
    let env = alloc_env(builder, calloc_ref, slots, parent);
    for slot in 0..slots {
        let offset = (8 + 8 * slot) as i32;
        let value = load_at(builder, src, offset);
        store_at(builder, env, offset, value);
    }
    env
}

pub(super) fn call_f64_binary(
    builder: &mut FunctionBuilder,
    func_ref: FuncRef,
    lhs: Value,
    rhs: Value,
) -> Value {
    let call = builder.ins().call(func_ref, &[lhs, rhs]);
    builder.inst_results(call)[0]
}

pub(super) fn call_pow(
    builder: &mut FunctionBuilder,
    pow_ref: FuncRef,
    lhs: Value,
    rhs: Value,
) -> Value {
    call_f64_binary(builder, pow_ref, lhs, rhs)
}

pub(super) fn call_fmod(
    builder: &mut FunctionBuilder,
    fmod_ref: FuncRef,
    lhs: Value,
    rhs: Value,
) -> Value {
    call_f64_binary(builder, fmod_ref, lhs, rhs)
}

pub(super) fn place(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    name: &str,
) -> Result<Place, Error> {
    let (home, slot) = resolve_binding(ctx.program, ctx.scope, name)
        .ok_or_else(|| Error::Scope(format!("unresolved identifier: {name}")))?;
    if ctx.program.scope(home).has_env() {
        let (depth, _) = resolve(ctx.program, ctx.scope, name)
            .ok_or_else(|| Error::Scope(format!("unresolved identifier: {name}")))?;
        let mut ptr = ctx.env;
        for _ in 0..depth {
            ptr = builder.ins().load(types::I64, MemFlags::new(), ptr, 0i32);
        }
        Ok(Place::Env {
            ptr,
            offset: (8 + 8 * slot) as i32,
        })
    } else {
        let var = *ctx
            .vars
            .get(&(home, slot))
            .ok_or_else(|| Error::Codegen("missing local variable".to_string()))?;
        Ok(Place::Local { var })
    }
}

pub(super) fn load_at(builder: &mut FunctionBuilder, ptr: Value, offset: i32) -> Value {
    builder.ins().load(types::F64, MemFlags::new(), ptr, offset)
}

pub(super) fn store_at(builder: &mut FunctionBuilder, ptr: Value, offset: i32, value: Value) {
    builder.ins().store(MemFlags::new(), value, ptr, offset);
}

pub(super) fn load_place(builder: &mut FunctionBuilder, place: Place) -> Value {
    match place {
        Place::Env { ptr, offset } => load_at(builder, ptr, offset),
        Place::Local { var } => builder.use_var(var),
    }
}

pub(super) fn store_place(builder: &mut FunctionBuilder, place: Place, value: Value) {
    match place {
        Place::Env { ptr, offset } => store_at(builder, ptr, offset, value),
        Place::Local { var } => builder.def_var(var, value),
    }
}

pub(super) fn check_tdz(builder: &mut FunctionBuilder, value: Value) {
    let uninitialized = value::is_uninitialized(builder, value);
    builder
        .ins()
        .trapnz(uninitialized, TrapCode::unwrap_user(1));
}

pub(super) fn load_var(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    name: &str,
) -> Result<Value, Error> {
    let place = place(builder, ctx, name)?;
    let value = load_place(builder, place);
    if needs_tdz_check(ctx.program, ctx.scope, name) {
        check_tdz(builder, value);
    }
    Ok(value)
}

pub(super) fn store_var(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    name: &str,
    value: Value,
) -> Result<(), Error> {
    let place = place(builder, ctx, name)?;
    store_place(builder, place, value);
    Ok(())
}
