use cranelift_codegen::ir::{InstBuilder, Value, condcodes::FloatCC, condcodes::IntCC, types};
use cranelift_frontend::FunctionBuilder;
use oxc_ast::ast::{
    AssignmentExpression, AssignmentOperator, AssignmentTarget, BinaryExpression, BinaryOperator,
    ConditionalExpression, Expression, LogicalOperator, SimpleAssignmentTarget, UnaryExpression,
    UnaryOperator, UpdateExpression, UpdateOperator,
};

use super::call::{compile_call, compile_closure, compile_property_access, is_free_global};
use super::chain::compile_chain;
use super::numeric::{binding_is_number, expr_is_number, number_value};
use super::stmt::compile_condition;
use super::support::{
    call_fmod, call_pow, check_tdz, load_place, load_var, place, store_place, store_var,
};
use super::{CodegenCtx, Flow};
use crate::{
    Error,
    plan::{is_const, needs_tdz_check, node_key, resolve_binding},
    value,
};

/// Evaluates a parameter initializer only when the argument is `undefined`.
pub(super) fn compile_default<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    arg: Value,
    init: &'a Expression<'a>,
) -> Result<Value, Error> {
    let missing = value::is_undefined(builder, arg);
    let result_var = builder.declare_var(types::F64);
    let init_block = builder.create_block();
    let join_block = builder.create_block();

    builder.def_var(result_var, arg);
    builder
        .ins()
        .brif(missing, init_block, &[], join_block, &[]);

    builder.switch_to_block(init_block);
    let default = compile_expr(builder, ctx, flow, init)?;
    builder.def_var(result_var, default);
    builder.ins().jump(join_block, &[]);

    builder.switch_to_block(join_block);
    Ok(builder.use_var(result_var))
}

fn compile_string_data(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    text: &str,
) -> Result<Value, Error> {
    let ops = ctx.strings.as_ref().ok_or_else(|| {
        Error::Unsupported("string values require the dynamic runtime".to_string())
    })?;
    let (pointer, len) = *ctx
        .string_literals
        .get(text)
        .ok_or_else(|| Error::Codegen("missing string literal data".to_string()))?;
    let len = builder.ins().iconst(types::I64, len);
    let call = builder.ins().call(ops.from_utf8, &[pointer, len]);
    let raw = builder.inst_results(call)[0];
    Ok(value::string(builder, raw))
}

fn compile_template<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    template: &'a oxc_ast::ast::TemplateLiteral<'a>,
) -> Result<Value, Error> {
    if template.expressions.is_empty() {
        let text = template.quasis[0]
            .value
            .cooked
            .as_ref()
            .map(|value| value.display().to_string())
            .unwrap_or_default();
        return compile_string_data(builder, ctx, &text);
    }

    let first = template.quasis[0]
        .value
        .cooked
        .as_ref()
        .map(|value| value.display().to_string())
        .unwrap_or_default();
    let mut result = compile_string_data(builder, ctx, &first)?;
    for (index, expression) in template.expressions.iter().enumerate() {
        let expression = compile_expr(builder, ctx, flow, expression)?;
        let expression =
            value::to_string(builder, expression, ctx.strings.as_ref()).ok_or_else(|| {
                Error::Unsupported("template literals require the dynamic runtime".to_string())
            })?;
        result =
            value::add(builder, result, expression, ctx.strings.as_ref()).ok_or_else(|| {
                Error::Unsupported("template literals require the dynamic runtime".to_string())
            })?;
        let quasi = template
            .quasis
            .get(index + 1)
            .and_then(|quasi| quasi.value.cooked.as_ref())
            .map(|value| value.display().to_string())
            .unwrap_or_default();
        if !quasi.is_empty() {
            let quasi = compile_string_data(builder, ctx, &quasi)?;
            result = value::add(builder, result, quasi, ctx.strings.as_ref()).ok_or_else(|| {
                Error::Unsupported("template literals require the dynamic runtime".to_string())
            })?;
        }
    }
    Ok(result)
}

pub(super) fn compile_expr<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    expr: &'a Expression<'a>,
) -> Result<Value, Error> {
    match expr {
        Expression::NumericLiteral(lit) => Ok(builder.ins().f64const(lit.value)),
        Expression::BooleanLiteral(lit) => Ok(value::boolean(builder, lit.value)),
        Expression::NullLiteral(_) => Ok(value::null(builder)),
        Expression::StringLiteral(lit) => {
            let text = lit.value.display().to_string();
            compile_string_data(builder, ctx, &text)
        }
        Expression::TemplateLiteral(template) => compile_template(builder, ctx, flow, template),
        Expression::BigIntLiteral(_) | Expression::RegExpLiteral(_) => Err(Error::Unsupported(
            "this literal is not supported".to_string(),
        )),
        Expression::Identifier(id) => {
            let name = id.name.as_str();
            if is_free_global(ctx, name) {
                if ctx.builtins.is_undefined(name) {
                    return Ok(value::undefined(builder));
                }
                if let Some(constant) = ctx.builtins.global_constant(name) {
                    return Ok(builder.ins().f64const(constant));
                }
            }
            load_var(builder, ctx, name)
        }
        Expression::ParenthesizedExpression(paren) => {
            compile_expr(builder, ctx, flow, &paren.expression)
        }
        Expression::BinaryExpression(binary) => compile_binary(builder, ctx, flow, binary),
        Expression::LogicalExpression(logical) => {
            let lhs = compile_expr(builder, ctx, flow, &logical.left)?;
            compile_logical(builder, ctx, flow, lhs, &logical.right, logical.operator)
        }
        Expression::ConditionalExpression(conditional) => {
            compile_conditional(builder, ctx, flow, conditional)
        }
        Expression::UnaryExpression(unary) => compile_unary(builder, ctx, flow, unary),
        Expression::AssignmentExpression(assign) => compile_assign(builder, ctx, flow, assign),
        Expression::UpdateExpression(update) => compile_update(builder, ctx, update),
        Expression::SequenceExpression(sequence) => {
            let mut result = value::undefined(builder);
            for expr in &sequence.expressions {
                result = compile_expr(builder, ctx, flow, expr)?;
            }
            Ok(result)
        }
        Expression::CallExpression(call) => compile_call(builder, ctx, flow, call),
        Expression::FunctionExpression(func) => compile_closure(builder, ctx, node_key(&**func)),
        Expression::ArrowFunctionExpression(arrow) => {
            compile_closure(builder, ctx, node_key(&**arrow))
        }
        Expression::ChainExpression(chain) => compile_chain(builder, ctx, flow, chain),
        Expression::StaticMemberExpression(access) => {
            compile_property_access(builder, ctx, flow, access)
        }
        other => Err(Error::Unsupported(format!(
            "expression not supported: {other:?}"
        ))),
    }
}

pub(super) fn compile_binary<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    binary: &'a BinaryExpression<'a>,
) -> Result<Value, Error> {
    let lhs = compile_expr(builder, ctx, flow, &binary.left)?;
    let rhs = compile_expr(builder, ctx, flow, &binary.right)?;
    let op = binary.operator;
    if op.is_arithmetic() {
        compile_arithmetic(
            builder,
            ctx,
            op,
            lhs,
            rhs,
            expr_is_number(ctx, &binary.left),
            expr_is_number(ctx, &binary.right),
        )
    } else if op.is_bitwise() {
        Ok(compile_bitwise(builder, ctx, op, lhs, rhs))
    } else if op.is_equality() || op.is_compare() {
        compile_relational(
            builder,
            ctx,
            op,
            lhs,
            rhs,
            expr_is_number(ctx, &binary.left),
            expr_is_number(ctx, &binary.right),
        )
    } else {
        Err(Error::Unsupported(format!(
            "binary operator not supported: {op:?}"
        )))
    }
}

/// Compiles `cond ? if_true : if_false`, evaluating exactly one branch.
pub(super) fn compile_conditional<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    conditional: &'a ConditionalExpression<'a>,
) -> Result<Value, Error> {
    let cond = compile_condition(builder, ctx, flow, &conditional.test)?;

    let result_var = builder.declare_var(types::F64);
    let true_block = builder.create_block();
    let false_block = builder.create_block();
    let join_block = builder.create_block();
    builder.ins().brif(cond, true_block, &[], false_block, &[]);

    builder.switch_to_block(true_block);
    let if_true = compile_expr(builder, ctx, flow, &conditional.consequent)?;
    builder.def_var(result_var, if_true);
    builder.ins().jump(join_block, &[]);

    builder.switch_to_block(false_block);
    let if_false = compile_expr(builder, ctx, flow, &conditional.alternate)?;
    builder.def_var(result_var, if_false);
    builder.ins().jump(join_block, &[]);

    builder.switch_to_block(join_block);
    Ok(builder.use_var(result_var))
}

/// The short-circuiting operators: `&&`, `||` and `??`.
pub(super) fn compile_logical<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    lhs: Value,
    rhs_expr: &'a Expression<'a>,
    op: LogicalOperator,
) -> Result<Value, Error> {
    let (cond, rhs_when_true) = match op {
        LogicalOperator::And => (value::truthy(builder, lhs, ctx.strings.as_ref()), true),
        LogicalOperator::Or => (value::truthy(builder, lhs, ctx.strings.as_ref()), false),
        LogicalOperator::Coalesce => (value::is_nullish(builder, lhs), true),
    };

    let result_var = builder.declare_var(types::F64);
    let rhs_block = builder.create_block();
    let join_block = builder.create_block();

    builder.def_var(result_var, lhs);
    if rhs_when_true {
        builder.ins().brif(cond, rhs_block, &[], join_block, &[]);
    } else {
        builder.ins().brif(cond, join_block, &[], rhs_block, &[]);
    }

    builder.switch_to_block(rhs_block);
    let rhs = compile_expr(builder, ctx, flow, rhs_expr)?;
    builder.def_var(result_var, rhs);
    builder.ins().jump(join_block, &[]);

    builder.switch_to_block(join_block);
    Ok(builder.use_var(result_var))
}

pub(super) fn compile_arithmetic(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    op: BinaryOperator,
    lhs: Value,
    rhs: Value,
    lhs_numeric: bool,
    rhs_numeric: bool,
) -> Result<Value, Error> {
    if op == BinaryOperator::Addition
        && !(lhs_numeric && rhs_numeric)
        && let Some(value) = value::add(builder, lhs, rhs, ctx.strings.as_ref())
    {
        return Ok(value);
    }
    let lhs = number_value(builder, lhs, lhs_numeric, ctx.strings.as_ref());
    let rhs = number_value(builder, rhs, rhs_numeric, ctx.strings.as_ref());
    let value = match op {
        BinaryOperator::Addition => builder.ins().fadd(lhs, rhs),
        BinaryOperator::Subtraction => builder.ins().fsub(lhs, rhs),
        BinaryOperator::Multiplication => builder.ins().fmul(lhs, rhs),
        BinaryOperator::Division => builder.ins().fdiv(lhs, rhs),
        BinaryOperator::Remainder => call_fmod(builder, ctx.fmod_ref, lhs, rhs),
        BinaryOperator::Exponential => call_pow(builder, ctx.pow_ref, lhs, rhs),
        other => {
            return Err(Error::Unsupported(format!(
                "arithmetic operator not supported: {other:?}"
            )));
        }
    };
    Ok(value)
}

/// Bitwise operators work on `ToInt32`/`ToUint32` views of the operands.
pub(super) fn compile_bitwise(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    op: BinaryOperator,
    lhs: Value,
    rhs: Value,
) -> Value {
    let lhs = value::to_int32(builder, ctx.fmod_ref, lhs, ctx.strings.as_ref());
    let rhs = value::to_int32(builder, ctx.fmod_ref, rhs, ctx.strings.as_ref());

    let result = match op {
        BinaryOperator::BitwiseAnd => builder.ins().band(lhs, rhs),
        BinaryOperator::BitwiseOR => builder.ins().bor(lhs, rhs),
        BinaryOperator::BitwiseXOR => builder.ins().bxor(lhs, rhs),
        BinaryOperator::ShiftLeft => {
            let shift = builder.ins().band_imm(rhs, 31);
            builder.ins().ishl(lhs, shift)
        }
        BinaryOperator::ShiftRight => {
            let shift = builder.ins().band_imm(rhs, 31);
            builder.ins().sshr(lhs, shift)
        }
        BinaryOperator::ShiftRightZeroFill => {
            let shift = builder.ins().band_imm(rhs, 31);
            let shifted = builder.ins().ushr(lhs, shift);
            return value::from_uint32(builder, shifted);
        }
        _ => unreachable!(),
    };
    value::from_int32(builder, result)
}

pub(super) fn compile_compare(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    op: BinaryOperator,
    lhs: Value,
    rhs: Value,
    lhs_numeric: bool,
    rhs_numeric: bool,
) -> Result<Value, Error> {
    if !(lhs_numeric && rhs_numeric)
        && let Some(ordering) = value::compare(builder, lhs, rhs, ctx.strings.as_ref())
    {
        let unordered = builder
            .ins()
            .icmp_imm(IntCC::Equal, ordering, i64::from(i32::MIN));
        let cc = match op {
            BinaryOperator::GreaterThan => IntCC::SignedGreaterThan,
            BinaryOperator::GreaterEqualThan => IntCC::SignedGreaterThanOrEqual,
            BinaryOperator::LessThan => IntCC::SignedLessThan,
            BinaryOperator::LessEqualThan => IntCC::SignedLessThanOrEqual,
            _ => unreachable!(),
        };
        let cond = builder.ins().icmp_imm(cc, ordering, 0);
        let ordered = builder.ins().icmp_imm(IntCC::Equal, unordered, 0);
        let cond = builder.ins().band(cond, ordered);
        return Ok(value::from_bool(builder, cond));
    }

    let lhs = number_value(builder, lhs, lhs_numeric, ctx.strings.as_ref());
    let rhs = number_value(builder, rhs, rhs_numeric, ctx.strings.as_ref());
    let cc = match op {
        BinaryOperator::GreaterThan => FloatCC::GreaterThan,
        BinaryOperator::GreaterEqualThan => FloatCC::GreaterThanOrEqual,
        BinaryOperator::LessThan => FloatCC::LessThan,
        BinaryOperator::LessEqualThan => FloatCC::LessThanOrEqual,
        _ => unreachable!(),
    };
    let cmp = builder.ins().fcmp(cc, lhs, rhs);
    Ok(value::from_bool(builder, cmp))
}

/// Compiles a comparison or equality as an untagged condition. Numeric
/// operands use a direct `fcmp`, avoiding the tagged-boolean round trip that
/// compiling the expression and then calling `truthy` would perform.
pub(super) fn compile_condition_compare<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    binary: &'a BinaryExpression<'a>,
) -> Result<Value, Error> {
    let lhs = compile_expr(builder, ctx, flow, &binary.left)?;
    let rhs = compile_expr(builder, ctx, flow, &binary.right)?;
    let lhs_numeric = expr_is_number(ctx, &binary.left);
    let rhs_numeric = expr_is_number(ctx, &binary.right);
    if lhs_numeric && rhs_numeric {
        let cond = match binary.operator {
            BinaryOperator::LessThan => builder.ins().fcmp(FloatCC::LessThan, lhs, rhs),
            BinaryOperator::LessEqualThan => builder.ins().fcmp(FloatCC::LessThanOrEqual, lhs, rhs),
            BinaryOperator::GreaterThan => builder.ins().fcmp(FloatCC::GreaterThan, lhs, rhs),
            BinaryOperator::GreaterEqualThan => {
                builder.ins().fcmp(FloatCC::GreaterThanOrEqual, lhs, rhs)
            }
            BinaryOperator::Equality | BinaryOperator::StrictEquality => {
                builder.ins().fcmp(FloatCC::Equal, lhs, rhs)
            }
            BinaryOperator::Inequality | BinaryOperator::StrictInequality => {
                let eq = builder.ins().fcmp(FloatCC::Equal, lhs, rhs);
                builder.ins().icmp_imm(IntCC::Equal, eq, 0)
            }
            _ => unreachable!("not a comparison operator"),
        };
        return Ok(cond);
    }
    let tagged = compile_relational(
        builder,
        ctx,
        binary.operator,
        lhs,
        rhs,
        lhs_numeric,
        rhs_numeric,
    )?;
    Ok(value::truthy(builder, tagged, ctx.strings.as_ref()))
}

pub(super) fn compile_relational(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    op: BinaryOperator,
    lhs: Value,
    rhs: Value,
    lhs_numeric: bool,
    rhs_numeric: bool,
) -> Result<Value, Error> {
    match op {
        BinaryOperator::Equality => Ok(value::loose_equal(builder, lhs, rhs, ctx.strings.as_ref())),
        BinaryOperator::StrictEquality => {
            Ok(value::strict_equal(builder, lhs, rhs, ctx.strings.as_ref()))
        }
        BinaryOperator::Inequality | BinaryOperator::StrictInequality => {
            let eq = if op == BinaryOperator::Inequality {
                value::loose_equal(builder, lhs, rhs, ctx.strings.as_ref())
            } else {
                value::strict_equal(builder, lhs, rhs, ctx.strings.as_ref())
            };
            let eq = value::truthy(builder, eq, ctx.strings.as_ref());
            let not = builder.ins().icmp_imm(IntCC::Equal, eq, 0);
            Ok(value::from_bool(builder, not))
        }
        BinaryOperator::GreaterThan
        | BinaryOperator::GreaterEqualThan
        | BinaryOperator::LessThan
        | BinaryOperator::LessEqualThan => {
            compile_compare(builder, ctx, op, lhs, rhs, lhs_numeric, rhs_numeric)
        }
        other => Err(Error::Unsupported(format!(
            "relational operator not supported: {other:?}"
        ))),
    }
}

pub(super) fn compile_unary<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    unary: &'a UnaryExpression<'a>,
) -> Result<Value, Error> {
    if unary.operator == UnaryOperator::Delete {
        return compile_delete(builder, ctx, flow, &unary.argument);
    }
    let target = compile_expr(builder, ctx, flow, &unary.argument)?;
    match unary.operator {
        UnaryOperator::UnaryNegation => {
            let target = number_value(
                builder,
                target,
                expr_is_number(ctx, &unary.argument),
                ctx.strings.as_ref(),
            );
            Ok(builder.ins().fneg(target))
        }
        UnaryOperator::UnaryPlus => Ok(number_value(
            builder,
            target,
            expr_is_number(ctx, &unary.argument),
            ctx.strings.as_ref(),
        )),
        UnaryOperator::LogicalNot => {
            let cond = value::truthy(builder, target, ctx.strings.as_ref());
            let not = builder.ins().icmp_imm(IntCC::Equal, cond, 0);
            Ok(value::from_bool(builder, not))
        }
        UnaryOperator::BitwiseNot => {
            let int = value::to_int32(builder, ctx.fmod_ref, target, ctx.strings.as_ref());
            let inverted = builder.ins().bxor_imm(int, -1);
            Ok(value::from_int32(builder, inverted))
        }
        UnaryOperator::Void => Ok(value::undefined(builder)),
        other => Err(Error::Unsupported(format!(
            "unary operator not supported: {other:?}"
        ))),
    }
}

pub(super) fn compile_delete<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    expr: &'a Expression<'a>,
) -> Result<Value, Error> {
    match expr {
        Expression::ParenthesizedExpression(paren) => {
            compile_delete(builder, ctx, flow, &paren.expression)
        }
        Expression::Identifier(id) => {
            let name = id.name.as_str();
            let deletable = resolve_binding(ctx.program, ctx.scope, name).is_none()
                && !ctx.builtins.is_global(name);
            Ok(value::boolean(builder, deletable))
        }
        Expression::StaticMemberExpression(_)
        | Expression::ComputedMemberExpression(_)
        | Expression::PrivateFieldExpression(_) => Err(Error::Unsupported(
            "delete of a property requires objects".to_string(),
        )),
        other => {
            compile_expr(builder, ctx, flow, other)?;
            Ok(value::boolean(builder, true))
        }
    }
}

pub(super) fn compile_assign<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    assign: &'a AssignmentExpression<'a>,
) -> Result<Value, Error> {
    let name = match &assign.left {
        AssignmentTarget::AssignmentTargetIdentifier(id) => id.name.as_str(),
        _ => {
            return Err(Error::Unsupported(
                "only identifier assignment is supported".to_string(),
            ));
        }
    };
    if is_const(ctx.program, ctx.scope, name) {
        return Err(Error::Scope(
            "assignment to a constant variable".to_string(),
        ));
    }
    let place = place(builder, ctx, name)?;
    let tdz = needs_tdz_check(ctx.program, ctx.scope, name);
    let op = assign.operator;

    // The logical assignment operators only evaluate and store the right-hand
    // side when the current value calls for it.
    let short_circuit = match op {
        AssignmentOperator::LogicalAnd => Some((true, true)),
        AssignmentOperator::LogicalOr => Some((false, true)),
        AssignmentOperator::LogicalNullish => Some((true, false)),
        _ => None,
    };
    if let Some((rhs_when_truthy, use_truthy)) = short_circuit {
        let old = load_place(builder, place);
        if tdz {
            check_tdz(builder, old);
        }
        let cond = if use_truthy {
            value::truthy(builder, old, ctx.strings.as_ref())
        } else {
            value::is_nullish(builder, old)
        };

        let result_var = builder.declare_var(types::F64);
        let rhs_block = builder.create_block();
        let join_block = builder.create_block();
        builder.def_var(result_var, old);
        if rhs_when_truthy {
            builder.ins().brif(cond, rhs_block, &[], join_block, &[]);
        } else {
            builder.ins().brif(cond, join_block, &[], rhs_block, &[]);
        }

        builder.switch_to_block(rhs_block);
        let rhs = compile_expr(builder, ctx, flow, &assign.right)?;
        store_place(builder, place, rhs);
        builder.def_var(result_var, rhs);
        builder.ins().jump(join_block, &[]);

        builder.switch_to_block(join_block);
        return Ok(builder.use_var(result_var));
    }

    if op == AssignmentOperator::Assign {
        let rhs = compile_expr(builder, ctx, flow, &assign.right)?;
        if tdz {
            let current = load_place(builder, place);
            check_tdz(builder, current);
        }
        store_place(builder, place, rhs);
        return Ok(rhs);
    }

    // Compound assignment reads the old value before evaluating the right-hand
    // side, matching the evaluation order of the specification.
    let old = load_place(builder, place);
    if tdz {
        check_tdz(builder, old);
    }
    let rhs = compile_expr(builder, ctx, flow, &assign.right)?;
    let binary = op
        .to_binary_operator()
        .ok_or_else(|| Error::Unsupported(format!("assignment operator not supported: {op:?}")))?;
    let value = if binary.is_arithmetic() {
        compile_arithmetic(
            builder,
            ctx,
            binary,
            old,
            rhs,
            binding_is_number(ctx, name),
            expr_is_number(ctx, &assign.right),
        )?
    } else {
        compile_bitwise(builder, ctx, binary, old, rhs)
    };
    store_place(builder, place, value);
    Ok(value)
}

pub(super) fn compile_update<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    update: &'a UpdateExpression<'a>,
) -> Result<Value, Error> {
    let name = match &update.argument {
        SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => id.name.as_str(),
        _ => {
            return Err(Error::Unsupported(
                "only identifier updates are supported".to_string(),
            ));
        }
    };
    if is_const(ctx.program, ctx.scope, name) {
        return Err(Error::Scope(
            "assignment to a constant variable".to_string(),
        ));
    }
    let old = load_var(builder, ctx, name)?;
    let old = number_value(
        builder,
        old,
        binding_is_number(ctx, name),
        ctx.strings.as_ref(),
    );
    let one = builder.ins().f64const(1.0);
    let new_value = match update.operator {
        UpdateOperator::Increment => builder.ins().fadd(old, one),
        UpdateOperator::Decrement => builder.ins().fsub(old, one),
    };
    let result = if update.prefix { None } else { Some(old) };
    store_var(builder, ctx, name, new_value)?;
    Ok(result.unwrap_or(new_value))
}
