use cranelift_codegen::ir::{Block, InstBuilder, Value, types};
use cranelift_frontend::FunctionBuilder;
use oxc_ast::ast::{
    BindingPattern, DoWhileStatement, Expression, ForInStatement, ForOfStatement, ForStatement,
    ForStatementInit, ForStatementLeft, LabeledStatement, Statement, SwitchStatement,
    VariableDeclarationKind, WhileStatement,
};

use super::expr::compile_expr;
use super::object;
use super::stmt::{compile_condition, compile_statement, compile_statements, compile_var_list};
use super::support::{copy_env, enter_scope, node_scope, store_var};
use super::{CodegenCtx, Flow, LoopTarget, StmtOutcome};
use crate::{
    Error,
    plan::{is_const, node_key},
    value,
};

pub(super) fn compile_while<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    while_stmt: &'a WhileStatement<'a>,
    labels: &[&'a str],
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let header = builder.create_block();
    let body = builder.create_block();
    let exit = builder.create_block();

    builder.ins().jump(header, &[]);
    builder.switch_to_block(header);

    let cond = compile_condition(builder, ctx, flow, &while_stmt.test)?;
    builder.ins().brif(cond, body, &[], exit, &[]);

    builder.switch_to_block(body);
    flow.loops.push(LoopTarget {
        labels: labels.to_vec(),
        break_block: exit,
        continue_block: Some(header),
        breakable: true,
        cleanup_depth: flow.cleanups.len(),
    });
    let body_out = compile_statement(builder, ctx, &while_stmt.body, flow)?;
    flow.loops.pop();
    if !body_out.terminated {
        builder.ins().jump(header, &[]);
    }

    builder.switch_to_block(exit);
    Ok(StmtOutcome::default())
}

pub(super) fn compile_do_while<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    do_while: &'a DoWhileStatement<'a>,
    labels: &[&'a str],
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let body_block = builder.create_block();
    let condition_block = builder.create_block();
    let exit = builder.create_block();

    builder.ins().jump(body_block, &[]);
    builder.switch_to_block(body_block);
    flow.loops.push(LoopTarget {
        labels: labels.to_vec(),
        break_block: exit,
        continue_block: Some(condition_block),
        breakable: true,
        cleanup_depth: flow.cleanups.len(),
    });
    let body_out = compile_statement(builder, ctx, &do_while.body, flow)?;
    flow.loops.pop();
    if !body_out.terminated {
        builder.ins().jump(condition_block, &[]);
    }

    builder.switch_to_block(condition_block);
    let cond = compile_condition(builder, ctx, flow, &do_while.test)?;
    builder.ins().brif(cond, body_block, &[], exit, &[]);

    builder.switch_to_block(exit);
    Ok(StmtOutcome::default())
}

/// `for (head in source) body`: the enumerable keys of `source`, iterated in
/// specification order. The keys are snapshotted before the loop starts, as the
/// object model has no live key iterator.
pub(super) fn compile_for_in<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    for_in: &'a ForInStatement<'a>,
    labels: &[&'a str],
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let ops = iteration_ops(ctx, "for..in")?;
    let source = compile_expr(builder, ctx, flow, &for_in.right)?;
    let call = builder.ins().call(ops.for_in_keys, &[source]);
    let keys = builder.inst_results(call)[0];
    compile_iteration(
        builder,
        ctx,
        &for_in.left,
        keys,
        &for_in.body,
        labels,
        flow,
        node_key(for_in),
    )
}

/// `for (head of source) body`: iterates arrays, strings (by code point), maps
/// and array-likes.
pub(super) fn compile_for_of<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    for_of: &'a ForOfStatement<'a>,
    labels: &[&'a str],
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let _ = iteration_ops(ctx, "for..of")?;
    let source = compile_expr(builder, ctx, flow, &for_of.right)?;
    compile_iteration(
        builder,
        ctx,
        &for_of.left,
        source,
        &for_of.body,
        labels,
        flow,
        node_key(for_of),
    )
}

fn iteration_ops(ctx: &CodegenCtx<'_>, construct: &str) -> Result<value::ObjectOps, Error> {
    ctx.objects
        .ok_or_else(|| Error::Unsupported(format!("`{construct}` requires the dynamic runtime")))
}

/// The shared loop of `for..in` and `for..of`: iterate `source` with a host
/// iterator, bind each value to `left` and run `body`. A lexical head gets a
/// fresh environment per iteration, exactly like `for (let ...)`.
#[allow(clippy::too_many_arguments)]
fn compile_iteration<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    left: &'a ForStatementLeft<'a>,
    source: Value,
    body: &'a Statement<'a>,
    labels: &[&'a str],
    flow: &mut Flow<'a>,
    node: usize,
) -> Result<StmtOutcome, Error> {
    let ops = iteration_ops(ctx, "iteration")?;
    let parent_env = ctx.env;
    let mut ctx = ctx.clone();
    let mut fresh: Option<(usize, Value)> = None;
    if let ForStatementLeft::VariableDeclaration(decl) = left
        && decl.kind != VariableDeclarationKind::Var
    {
        let scope = node_scope(&ctx, node)?;
        let plan = ctx.program.scope(scope);
        let slots = plan.slot_count();
        let has_env = plan.has_env();
        ctx = enter_scope(builder, &ctx, scope);
        if has_env {
            fresh = Some((slots, ctx.env));
        }
    }

    let header = builder.create_block();
    let body_block = builder.create_block();
    let step = builder.create_block();
    let exit = builder.create_block();

    let handle = builder.declare_var(types::I64);
    let call = builder.ins().call(ops.iter_new, &[source]);
    builder.def_var(handle, builder.inst_results(call)[0]);
    let env_var = builder.declare_var(types::I64);
    if let Some((_, env)) = fresh {
        builder.def_var(env_var, env);
    }
    builder.ins().jump(header, &[]);

    builder.switch_to_block(header);
    let header_ctx = if fresh.is_some() {
        ctx.enter(ctx.scope, builder.use_var(env_var))
    } else {
        ctx.clone()
    };
    let handle_value = builder.use_var(handle);
    let call = builder.ins().call(ops.iter_next, &[handle_value]);
    let more = value::truthy(builder, builder.inst_results(call)[0], ctx.strings.as_ref());
    builder.ins().brif(more, body_block, &[], exit, &[]);

    builder.switch_to_block(body_block);
    let body_ctx = if fresh.is_some() {
        ctx.enter(ctx.scope, builder.use_var(env_var))
    } else {
        header_ctx.clone()
    };
    let handle_value = builder.use_var(handle);
    let call = builder.ins().call(ops.iter_value, &[handle_value]);
    let current = builder.inst_results(call)[0];
    assign_iteration_value(builder, &body_ctx, left, current, flow)?;

    flow.loops.push(LoopTarget {
        labels: labels.to_vec(),
        break_block: exit,
        continue_block: Some(step),
        breakable: true,
        cleanup_depth: flow.cleanups.len(),
    });
    let body_out = compile_statement(builder, &body_ctx, body, flow)?;
    flow.loops.pop();
    if !body_out.terminated {
        builder.ins().jump(step, &[]);
    }

    builder.switch_to_block(step);
    if let Some((slots, _)) = fresh {
        let previous = builder.use_var(env_var);
        let env = copy_env(builder, ctx.calloc_ref, parent_env, previous, slots);
        builder.def_var(env_var, env);
    }
    builder.ins().jump(header, &[]);

    builder.switch_to_block(exit);
    Ok(StmtOutcome::default())
}

/// Binds the iteration value to the loop head: a declaration, an identifier
/// assignment or a member assignment.
fn assign_iteration_value<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    left: &'a ForStatementLeft<'a>,
    value: Value,
    flow: &mut Flow<'a>,
) -> Result<(), Error> {
    match left {
        ForStatementLeft::VariableDeclaration(decl) => {
            let var = decl
                .declarations
                .first()
                .ok_or_else(|| Error::Unsupported("empty loop head".to_string()))?;
            let BindingPattern::BindingIdentifier(id) = &var.id else {
                return Err(Error::Unsupported(
                    "destructuring loop targets are not supported".to_string(),
                ));
            };
            store_var(builder, ctx, id.name.as_str(), value)?;
        }
        ForStatementLeft::AssignmentTargetIdentifier(id) => {
            if is_const(ctx.program, ctx.scope, id.name.as_str()) {
                return Err(Error::Scope(
                    "assignment to a constant variable".to_string(),
                ));
            }
            store_var(builder, ctx, id.name.as_str(), value)?;
        }
        ForStatementLeft::StaticMemberExpression(access) => {
            let object = compile_expr(builder, ctx, flow, &access.object)?;
            let key = object::static_key(builder, ctx, access.property.name.as_str())?;
            object::set(builder, ctx, object, key, value)?;
        }
        ForStatementLeft::ComputedMemberExpression(member) => {
            let object = compile_expr(builder, ctx, flow, &member.object)?;
            let key = compile_expr(builder, ctx, flow, &member.expression)?;
            object::set(builder, ctx, object, key, value)?;
        }
        _ => {
            return Err(Error::Unsupported(
                "destructuring loop targets are not supported".to_string(),
            ));
        }
    }
    Ok(())
}

/// Compiles a label chain. Labels attached to loops become extra names for the
/// loop target; any other labelled statement gets a break-only target.
pub(super) fn compile_labelled<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    labelled: &'a LabeledStatement<'a>,
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let mut labels = vec![labelled.label.name.as_str()];
    let mut stmt = &labelled.body;
    while let Statement::LabeledStatement(inner) = stmt {
        labels.push(inner.label.name.as_str());
        stmt = &inner.body;
    }

    if matches!(stmt, Statement::FunctionDeclaration(_)) {
        return Err(Error::Unsupported(
            "labelled function declarations are not supported".to_string(),
        ));
    }

    match stmt {
        Statement::WhileStatement(while_stmt) => {
            compile_while(builder, ctx, while_stmt, &labels, flow)
        }
        Statement::DoWhileStatement(do_while) => {
            compile_do_while(builder, ctx, do_while, &labels, flow)
        }
        Statement::ForStatement(for_stmt) => compile_for(builder, ctx, for_stmt, &labels, flow),
        Statement::ForInStatement(for_in) => compile_for_in(builder, ctx, for_in, &labels, flow),
        Statement::ForOfStatement(for_of) => compile_for_of(builder, ctx, for_of, &labels, flow),
        Statement::SwitchStatement(switch_stmt) => {
            compile_switch(builder, ctx, switch_stmt, &labels, flow)
        }
        stmt => {
            let join = builder.create_block();
            flow.loops.push(LoopTarget {
                labels,
                break_block: join,
                continue_block: None,
                breakable: false,
                cleanup_depth: flow.cleanups.len(),
            });
            let out = compile_statement(builder, ctx, stmt, flow)?;
            flow.loops.pop();
            if !out.terminated {
                builder.ins().jump(join, &[]);
            }
            builder.switch_to_block(join);
            Ok(StmtOutcome::default())
        }
    }
}

/// `for (init; condition; update) body`. A lexical initializer gets one
/// environment per iteration so each iteration's bindings are distinct.
pub(super) fn compile_for<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    for_stmt: &'a ForStatement<'a>,
    labels: &[&'a str],
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let parent_env = ctx.env;
    let mut ctx = ctx.clone();
    let mut fresh: Option<(usize, Value)> = None;
    let has_lexical_init = matches!(
        &for_stmt.init,
        Some(ForStatementInit::VariableDeclaration(decl))
            if decl.kind != VariableDeclarationKind::Var
    );
    if has_lexical_init {
        let scope = node_scope(&ctx, node_key(for_stmt))?;
        let plan = ctx.program.scope(scope);
        let slots = plan.slot_count();
        let has_env = plan.has_env();
        ctx = enter_scope(builder, &ctx, scope);
        if has_env {
            fresh = Some((slots, ctx.env));
        }
    }

    match &for_stmt.init {
        Some(ForStatementInit::VariableDeclaration(decl)) => {
            compile_var_list(builder, &ctx, decl, flow)?;
        }
        Some(init) => {
            let expr = init.as_expression().ok_or_else(|| {
                Error::Unsupported("for-loop initializer is not supported".to_string())
            })?;
            compile_expr(builder, &ctx, flow, expr)?;
        }
        None => {}
    }

    let header = builder.create_block();
    let body_block = builder.create_block();
    let step = builder.create_block();
    let exit = builder.create_block();

    let env_var = builder.declare_var(types::I64);
    if let Some((_, env)) = fresh {
        builder.def_var(env_var, env);
    }
    builder.ins().jump(header, &[]);

    builder.switch_to_block(header);
    let per_iteration = fresh.is_some();
    let header_ctx = if per_iteration {
        ctx.enter(ctx.scope, builder.use_var(env_var))
    } else {
        ctx.clone()
    };
    match &for_stmt.test {
        Some(condition) => {
            let cond = compile_condition(builder, &header_ctx, flow, condition)?;
            builder.ins().brif(cond, body_block, &[], exit, &[]);
        }
        None => {
            builder.ins().jump(body_block, &[]);
        }
    }

    builder.switch_to_block(body_block);
    let body_ctx = if per_iteration {
        ctx.enter(ctx.scope, builder.use_var(env_var))
    } else {
        ctx.clone()
    };
    flow.loops.push(LoopTarget {
        labels: labels.to_vec(),
        break_block: exit,
        continue_block: Some(step),
        breakable: true,
        cleanup_depth: flow.cleanups.len(),
    });
    let body_out = compile_statement(builder, &body_ctx, &for_stmt.body, flow)?;
    flow.loops.pop();
    if !body_out.terminated {
        builder.ins().jump(step, &[]);
    }

    builder.switch_to_block(step);
    let step_ctx = if let Some((slots, _)) = fresh {
        let previous = builder.use_var(env_var);
        let env = copy_env(builder, ctx.calloc_ref, parent_env, previous, slots);
        builder.def_var(env_var, env);
        ctx.enter(ctx.scope, env)
    } else {
        ctx.clone()
    };
    if let Some(update) = &for_stmt.update {
        compile_expr(builder, &step_ctx, flow, update)?;
    }
    builder.ins().jump(header, &[]);

    builder.switch_to_block(exit);
    Ok(StmtOutcome::default())
}

/// `switch (value) { case c: ... default: ... }` with fall-through.
pub(super) fn compile_switch<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    switch_stmt: &'a SwitchStatement<'a>,
    labels: &[&'a str],
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let value = compile_expr(builder, ctx, flow, &switch_stmt.discriminant)?;
    let scope = node_scope(ctx, node_key(switch_stmt))?;
    let inner = enter_scope(builder, ctx, scope);

    let value_var = builder.declare_var(types::F64);
    builder.def_var(value_var, value);

    let cases = &switch_stmt.cases;
    let exit = builder.create_block();
    let case_blocks: Vec<Block> = cases.iter().map(|_| builder.create_block()).collect();
    let no_match = cases
        .iter()
        .position(|case| case.test.is_none())
        .map_or(exit, |i| case_blocks[i]);

    let tests: Vec<(usize, &Expression<'a>)> = cases
        .iter()
        .enumerate()
        .filter_map(|(i, case)| case.test.as_ref().map(|cond| (i, cond)))
        .collect();

    if tests.is_empty() {
        builder.ins().jump(no_match, &[]);
    }
    for (n, (i, condition)) in tests.iter().enumerate() {
        let is_last = n + 1 == tests.len();
        let matched_value = compile_expr(builder, &inner, flow, condition)?;
        let target = builder.use_var(value_var);
        let equal = value::strict_equal(builder, target, matched_value, ctx.strings.as_ref());
        let cond = value::truthy(builder, equal, inner.strings.as_ref());
        let next = if is_last {
            no_match
        } else {
            builder.create_block()
        };
        builder.ins().brif(cond, case_blocks[*i], &[], next, &[]);
        if !is_last {
            builder.switch_to_block(next);
        }
    }

    flow.loops.push(LoopTarget {
        labels: labels.to_vec(),
        break_block: exit,
        continue_block: None,
        breakable: true,
        cleanup_depth: flow.cleanups.len(),
    });
    for (i, case) in cases.iter().enumerate() {
        builder.switch_to_block(case_blocks[i]);
        let out = compile_statements(builder, &inner, case.consequent.as_slice(), flow)?;
        if !out.terminated {
            let next = case_blocks.get(i + 1).copied().unwrap_or(exit);
            builder.ins().jump(next, &[]);
        }
    }
    flow.loops.pop();

    builder.switch_to_block(exit);
    Ok(StmtOutcome::default())
}
