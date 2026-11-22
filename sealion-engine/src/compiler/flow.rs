use cranelift_codegen::ir::{Block, InstBuilder, MemFlags, Value, condcodes::IntCC, types};
use cranelift_frontend::FunctionBuilder;

use super::stmt::compile_block;
use super::{CodegenCtx, Flow, LoopTarget};
use crate::{Error, value};

pub(super) fn break_target(
    loops: &[LoopTarget<'_>],
    label: Option<&str>,
) -> Result<(Block, usize), Error> {
    if let Some(label) = label {
        loops
            .iter()
            .rev()
            .find(|target| target.labels.contains(&label))
            .map(|target| (target.break_block, target.cleanup_depth))
            .ok_or_else(|| Error::Scope(format!("unknown break label: {label}")))
    } else {
        loops
            .iter()
            .rev()
            .find(|target| target.breakable)
            .map(|target| (target.break_block, target.cleanup_depth))
            .ok_or_else(|| Error::Scope("break statement outside of a loop".to_string()))
    }
}

pub(super) fn continue_target(
    loops: &[LoopTarget<'_>],
    label: Option<&str>,
) -> Result<(Block, usize), Error> {
    if let Some(label) = label {
        let target = loops
            .iter()
            .rev()
            .find(|target| target.labels.contains(&label))
            .ok_or_else(|| Error::Scope(format!("unknown continue label: {label}")))?;
        target
            .continue_block
            .map(|block| (block, target.cleanup_depth))
            .ok_or_else(|| Error::Scope("continue label does not target a loop".to_string()))
    } else {
        loops
            .iter()
            .rev()
            .find_map(|target| {
                target
                    .continue_block
                    .map(|block| (block, target.cleanup_depth))
            })
            .ok_or_else(|| Error::Scope("continue statement outside of a loop".to_string()))
    }
}

pub(super) fn has_exception(builder: &mut FunctionBuilder, ctx: &CodegenCtx<'_>) -> Value {
    let pending = builder
        .ins()
        .load(types::I64, MemFlags::new(), ctx.exception, 0i32);
    builder.ins().icmp_imm(IntCC::NotEqual, pending, 0)
}

pub(super) fn take_exception(builder: &mut FunctionBuilder, ctx: &CodegenCtx<'_>) -> Value {
    let value = builder
        .ins()
        .load(types::F64, MemFlags::new(), ctx.exception, 8i32);
    let clear = builder.ins().iconst(types::I64, 0);
    builder
        .ins()
        .store(MemFlags::new(), clear, ctx.exception, 0i32);
    value
}

pub(super) fn set_exception(builder: &mut FunctionBuilder, ctx: &CodegenCtx<'_>, value: Value) {
    let set = builder.ins().iconst(types::I64, 1);
    builder
        .ins()
        .store(MemFlags::new(), set, ctx.exception, 0i32);
    builder
        .ins()
        .store(MemFlags::new(), value, ctx.exception, 8i32);
}

/// Emits every finalizer between `from_depth` and `target_depth`, then jumps to
/// `continuation`. Returns false when a finalizer completes abruptly and
/// therefore overrides the original completion.
pub(super) fn emit_unwind<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &Flow<'a>,
    from_depth: usize,
    target_depth: usize,
    continuation: Block,
    pending: bool,
) -> Result<bool, Error> {
    if from_depth <= target_depth {
        builder.ins().jump(continuation, &[]);
        return Ok(true);
    }

    let index = from_depth - 1;
    let cleanup = flow.cleanups[index].clone();
    let saved = if pending {
        Some(take_exception(builder, ctx))
    } else {
        None
    };
    let mut inner = Flow {
        loops: flow.loops[..cleanup.loop_depth].to_vec(),
        cleanups: flow.cleanups[..index].to_vec(),
        exception: cleanup.exception,
    };
    let finalizer_ctx = ctx.enter(cleanup.scope, cleanup.env);
    let after = builder.create_block();
    let out = compile_block(builder, &finalizer_ctx, cleanup.finalizer, &mut inner)?;
    if out.terminated {
        return Ok(false);
    }
    builder.ins().jump(after, &[]);
    builder.switch_to_block(after);
    if let Some(saved) = saved {
        set_exception(builder, ctx, saved);
    }
    emit_unwind(
        builder,
        ctx,
        flow,
        index,
        target_depth,
        continuation,
        pending,
    )
}

pub(super) fn emit_pending_exception<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &Flow<'a>,
) -> Result<(), Error> {
    let depth = flow.cleanups.len();
    match flow.exception {
        Some(target) => {
            emit_unwind(
                builder,
                ctx,
                flow,
                depth,
                target.cleanup_depth,
                target.block,
                true,
            )?;
        }
        None => {
            let exit = builder.create_block();
            let reached = emit_unwind(builder, ctx, flow, depth, 0, exit, true)?;
            if reached {
                builder.switch_to_block(exit);
                let value = value::undefined(builder);
                builder.ins().return_(&[value]);
            }
        }
    }
    Ok(())
}

pub(super) fn emit_exception_check<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &Flow<'a>,
) -> Result<(), Error> {
    let pending = has_exception(builder, ctx);
    let handler = builder.create_block();
    let ok = builder.create_block();
    builder.ins().brif(pending, handler, &[], ok, &[]);
    builder.switch_to_block(handler);
    emit_pending_exception(builder, ctx, flow)?;
    builder.switch_to_block(ok);
    Ok(())
}
