use cranelift_codegen::ir::{InstBuilder, Value, condcodes::IntCC, types};
use cranelift_frontend::FunctionBuilder;
use oxc_ast::ast::{
    BindingPattern, BlockStatement, Expression, IfStatement, Statement, TryStatement,
    UnaryOperator, VariableDeclaration, VariableDeclarationKind,
};

use super::expr::{compile_condition_compare, compile_expr};
use super::flow::{
    break_target, continue_target, emit_pending_exception, emit_unwind, set_exception,
    take_exception,
};
use super::loops::{
    compile_do_while, compile_for, compile_labelled, compile_switch, compile_while,
};
use super::support::{enter_scope, node_scope, store_var};
use super::{Cleanup, CodegenCtx, ExceptionTarget, Flow, StmtOutcome};
use crate::{Error, plan::node_key, value};

/// Enters a block scope, allocating its environment when it owns bindings.
pub(super) fn compile_block<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    block: &'a BlockStatement<'a>,
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let scope = node_scope(ctx, node_key(block))?;
    let inner = enter_scope(builder, ctx, scope);
    compile_statements(builder, &inner, block.body.as_slice(), flow)
}

pub(super) fn compile_statements<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    list: &'a [Statement<'a>],
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let mut last = None;
    for stmt in list {
        let out = compile_statement(builder, ctx, stmt, flow)?;
        if out.last.is_some() {
            last = out.last;
        }
        if out.terminated {
            return Ok(StmtOutcome {
                last,
                terminated: true,
            });
        }
    }
    Ok(StmtOutcome {
        last,
        terminated: false,
    })
}

pub(super) fn compile_var_list<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    decl: &'a VariableDeclaration<'a>,
    flow: &mut Flow<'a>,
) -> Result<(), Error> {
    let lexical = decl.kind != VariableDeclarationKind::Var;
    for var in &decl.declarations {
        let BindingPattern::BindingIdentifier(id) = &var.id else {
            return Err(Error::Unsupported(
                "destructuring is not supported".to_string(),
            ));
        };
        let name = id.name.as_str();
        if let Some(init) = &var.init {
            let value = compile_expr(builder, ctx, flow, init)?;
            store_var(builder, ctx, name, value)?;
        } else if lexical {
            let value = value::undefined(builder);
            store_var(builder, ctx, name, value)?;
        }
    }
    Ok(())
}

pub(super) fn compile_statement<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    stmt: &'a Statement<'a>,
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    match stmt {
        Statement::BlockStatement(block) => compile_block(builder, ctx, block, flow),
        Statement::VariableDeclaration(decl) => {
            compile_var_list(builder, ctx, decl, flow)?;
            Ok(StmtOutcome::default())
        }
        Statement::FunctionDeclaration(_)
        | Statement::EmptyStatement(_)
        | Statement::DebuggerStatement(_) => Ok(StmtOutcome::default()),
        Statement::ExpressionStatement(expr) => {
            let value = compile_expr(builder, ctx, flow, &expr.expression)?;
            Ok(StmtOutcome {
                last: Some(value),
                terminated: false,
            })
        }
        Statement::IfStatement(if_stmt) => compile_if(builder, ctx, if_stmt, flow),
        Statement::WhileStatement(while_stmt) => compile_while(builder, ctx, while_stmt, &[], flow),
        Statement::DoWhileStatement(do_while) => {
            compile_do_while(builder, ctx, do_while, &[], flow)
        }
        Statement::ForStatement(for_stmt) => compile_for(builder, ctx, for_stmt, &[], flow),
        Statement::SwitchStatement(switch_stmt) => {
            compile_switch(builder, ctx, switch_stmt, &[], flow)
        }
        Statement::LabeledStatement(labelled) => compile_labelled(builder, ctx, labelled, flow),
        Statement::BreakStatement(break_stmt) => {
            let label = break_stmt.label.as_ref().map(|label| label.name.as_str());
            let (target, depth) = break_target(&flow.loops, label)?;
            emit_unwind(
                builder,
                ctx,
                flow,
                flow.cleanups.len(),
                depth,
                target,
                false,
            )?;
            Ok(StmtOutcome {
                last: None,
                terminated: true,
            })
        }
        Statement::ContinueStatement(continue_stmt) => {
            let label = continue_stmt
                .label
                .as_ref()
                .map(|label| label.name.as_str());
            let (target, depth) = continue_target(&flow.loops, label)?;
            emit_unwind(
                builder,
                ctx,
                flow,
                flow.cleanups.len(),
                depth,
                target,
                false,
            )?;
            Ok(StmtOutcome {
                last: None,
                terminated: true,
            })
        }
        Statement::ReturnStatement(ret) => {
            let value = if let Some(target) = &ret.argument {
                compile_expr(builder, ctx, flow, target)?
            } else {
                value::undefined(builder)
            };
            let exit = builder.create_block();
            let reached = emit_unwind(builder, ctx, flow, flow.cleanups.len(), 0, exit, false)?;
            if reached {
                builder.switch_to_block(exit);
                builder.ins().return_(&[value]);
            }
            Ok(StmtOutcome {
                last: None,
                terminated: true,
            })
        }
        Statement::ThrowStatement(throw) => {
            let value = compile_expr(builder, ctx, flow, &throw.argument)?;
            set_exception(builder, ctx, value);
            emit_pending_exception(builder, ctx, flow)?;
            Ok(StmtOutcome {
                last: None,
                terminated: true,
            })
        }
        Statement::TryStatement(try_stmt) => compile_try(builder, ctx, try_stmt, flow),
        other => Err(Error::Unsupported(format!(
            "statement not supported: {other:?}"
        ))),
    }
}

pub(super) fn compile_condition<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    expr: &'a Expression<'a>,
) -> Result<Value, Error> {
    match expr {
        Expression::ParenthesizedExpression(paren) => {
            compile_condition(builder, ctx, flow, &paren.expression)
        }
        Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
            let cond = compile_condition(builder, ctx, flow, &unary.argument)?;
            Ok(builder.ins().icmp_imm(IntCC::Equal, cond, 0))
        }
        Expression::BinaryExpression(binary)
            if binary.operator.is_compare() || binary.operator.is_equality() =>
        {
            compile_condition_compare(builder, ctx, flow, binary)
        }
        _ => {
            let value = compile_expr(builder, ctx, flow, expr)?;
            Ok(value::truthy(builder, value, ctx.strings.as_ref()))
        }
    }
}

pub(super) fn compile_if<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    if_stmt: &'a IfStatement<'a>,
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let cond = compile_condition(builder, ctx, flow, &if_stmt.test)?;

    let then_block = builder.create_block();
    let else_block = builder.create_block();
    let join_block = builder.create_block();
    builder.ins().brif(cond, then_block, &[], else_block, &[]);

    builder.switch_to_block(then_block);
    let then_out = compile_statement(builder, ctx, &if_stmt.consequent, flow)?;
    if !then_out.terminated {
        builder.ins().jump(join_block, &[]);
    }

    builder.switch_to_block(else_block);
    let else_out = if let Some(else_node) = &if_stmt.alternate {
        compile_statement(builder, ctx, else_node, flow)?
    } else {
        StmtOutcome::default()
    };
    if !else_out.terminated {
        builder.ins().jump(join_block, &[]);
    }

    if then_out.terminated && else_out.terminated {
        Ok(StmtOutcome {
            last: None,
            terminated: true,
        })
    } else {
        builder.switch_to_block(join_block);
        Ok(StmtOutcome::default())
    }
}

pub(super) fn compile_try<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    try_stmt: &'a TryStatement<'a>,
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let outer_exception = flow.exception;
    let base_cleanup = flow.cleanups.len();
    let base_loop = flow.loops.len();
    let has_finalizer = try_stmt.finalizer.is_some();
    if let Some(finalizer) = &try_stmt.finalizer {
        flow.cleanups.push(Cleanup {
            finalizer,
            loop_depth: base_loop,
            scope: ctx.scope,
            env: ctx.env,
            exception: outer_exception,
        });
    }

    let catch_entry = try_stmt.handler.as_ref().map(|_| builder.create_block());
    flow.exception = match catch_entry {
        Some(block) => Some(ExceptionTarget {
            block,
            cleanup_depth: base_cleanup + usize::from(has_finalizer),
        }),
        None => outer_exception,
    };

    let after = builder.create_block();
    let try_value = builder.declare_var(types::F64);
    let mut reaches_after = false;

    let try_out = compile_block(builder, ctx, &try_stmt.block, flow)?;
    if !try_out.terminated {
        let value = match try_out.last {
            Some(value) => value,
            None => value::undefined(builder),
        };
        builder.def_var(try_value, value);
        reaches_after |= emit_unwind(
            builder,
            ctx,
            flow,
            flow.cleanups.len(),
            base_cleanup,
            after,
            false,
        )?;
    }

    if let Some(handler) = &try_stmt.handler {
        let entry = catch_entry.ok_or_else(|| Error::Codegen("missing catch block".to_string()))?;
        builder.switch_to_block(entry);
        let catch_scope = node_scope(ctx, node_key(&**handler))?;
        let catch_ctx = enter_scope(builder, ctx, catch_scope);
        let exception = take_exception(builder, ctx);
        if let Some(param) = &handler.param {
            let BindingPattern::BindingIdentifier(id) = &param.pattern else {
                return Err(Error::Unsupported(
                    "destructuring catch parameters are not supported".to_string(),
                ));
            };
            store_var(builder, &catch_ctx, id.name.as_str(), exception)?;
        }

        flow.exception = outer_exception;
        let catch_out =
            compile_statements(builder, &catch_ctx, handler.body.body.as_slice(), flow)?;
        if !catch_out.terminated {
            let value = match catch_out.last {
                Some(value) => value,
                None => value::undefined(builder),
            };
            builder.def_var(try_value, value);
            reaches_after |= emit_unwind(
                builder,
                ctx,
                flow,
                flow.cleanups.len(),
                base_cleanup,
                after,
                false,
            )?;
        }
    }

    if has_finalizer {
        flow.cleanups.pop();
    }
    flow.exception = outer_exception;

    if reaches_after {
        builder.switch_to_block(after);
        Ok(StmtOutcome {
            last: Some(builder.use_var(try_value)),
            terminated: false,
        })
    } else {
        Ok(StmtOutcome {
            last: None,
            terminated: true,
        })
    }
}
