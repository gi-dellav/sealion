use cranelift_codegen::ir::{Block, InstBuilder, Value, types};
use cranelift_frontend::FunctionBuilder;
use oxc_ast::ast::{
    DoWhileStatement, Expression, ForStatement, ForStatementInit, LabeledStatement, Statement,
    SwitchStatement, VariableDeclarationKind, WhileStatement,
};

use super::expr::compile_expr;
use super::stmt::{compile_condition, compile_statement, compile_statements, compile_var_list};
use super::support::{copy_env, enter_scope, node_scope};
use super::{CodegenCtx, Flow, LoopTarget, StmtOutcome};
use crate::{Error, plan::node_key, value};

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
