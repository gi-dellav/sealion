//! Array literals for the dynamic runtime.
//!
//! An array is an ordinary [`crate::value::ObjectOps`] handle created by the
//! host, so the literal only allocates the dense range and then writes the
//! present elements through the regular property path. Elisions keep their
//! index (the slot stays `undefined` in the host vector) and spread is
//! rejected until the runtime can iterate.

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_frontend::FunctionBuilder;
use oxc_ast::ast::ArrayExpression;

use super::expr::compile_expr;
use super::object;
use super::{CodegenCtx, Flow};
use crate::{Error, value};

pub(super) fn compile_array_literal<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    array: &'a ArrayExpression<'a>,
) -> Result<Value, Error> {
    let ops = ctx.arrays.ok_or_else(|| {
        Error::Unsupported("array values require the dynamic runtime".to_string())
    })?;
    let length = builder
        .ins()
        .iconst(types::I64, array.elements.len() as i64);
    let call = builder.ins().call(ops.new, &[length]);
    let pointer = builder.inst_results(call)[0];
    let result = value::object(builder, pointer);

    for (index, element) in array.elements.iter().enumerate() {
        if element.is_elision() {
            continue;
        }
        let Some(expression) = element.as_expression() else {
            return Err(Error::Unsupported(
                "array spread is not supported".to_string(),
            ));
        };
        let value = compile_expr(builder, ctx, flow, expression)?;
        // A numeric key canonicalises to the same string as `"<index>"`.
        let key = builder.ins().f64const(index as f64);
        object::set(builder, ctx, result, key, value)?;
    }

    Ok(result)
}
