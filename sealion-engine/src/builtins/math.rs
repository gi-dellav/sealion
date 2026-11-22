use cranelift_codegen::ir::{
    InstBuilder, MemFlags, Value, condcodes::FloatCC, condcodes::IntCC, types,
};
use cranelift_frontend::FunctionBuilder;

use crate::value;

/// `Math.round`: halves round towards `+Infinity`, unlike C's `round`. Negative
/// values that round to zero produce `-0`.
pub(super) fn js_round(builder: &mut FunctionBuilder, x: Value) -> Value {
    let floor = builder.ins().floor(x);
    let diff = builder.ins().fsub(x, floor);
    let half = builder.ins().f64const(0.5);
    let below_half = builder.ins().fcmp(FloatCC::LessThan, diff, half);
    let one = builder.ins().f64const(1.0);
    let upper = builder.ins().fadd(floor, one);
    let rounded = builder.ins().select(below_half, floor, upper);

    let zero = builder.ins().f64const(0.0);
    let negative = builder.ins().fcmp(FloatCC::LessThan, x, zero);
    let is_zero = builder.ins().fcmp(FloatCC::Equal, rounded, zero);
    let fix_negative_zero = builder.ins().band(negative, is_zero);
    let negative_zero = builder.ins().f64const(-0.0);
    builder
        .ins()
        .select(fix_negative_zero, negative_zero, rounded)
}

/// `Math.fround`: rounds the value to the nearest `f32` and back.
pub(super) fn fround(builder: &mut FunctionBuilder, x: Value) -> Value {
    let small = builder.ins().fdemote(types::F32, x);
    builder.ins().fpromote(types::F64, small)
}

pub(super) fn is_integer(builder: &mut FunctionBuilder, x: Value) -> Value {
    let finite = value::is_finite(builder, x);
    let trunc = builder.ins().trunc(x);
    let integral = builder.ins().fcmp(FloatCC::Equal, trunc, x);
    builder.ins().band(finite, integral)
}

fn has_negative_sign(builder: &mut FunctionBuilder, x: Value) -> Value {
    let bits = builder.ins().bitcast(types::I64, MemFlags::new(), x);
    builder.ins().icmp_imm(IntCC::SignedLessThan, bits, 0)
}

pub(super) fn sign(builder: &mut FunctionBuilder, x: Value) -> Value {
    let zero = builder.ins().f64const(0.0);
    let one = builder.ins().f64const(1.0);
    let minus_one = builder.ins().f64const(-1.0);
    let positive = builder.ins().fcmp(FloatCC::GreaterThan, x, zero);
    let negative = builder.ins().fcmp(FloatCC::LessThan, x, zero);
    let unsigned = builder.ins().select(positive, one, zero);
    let signed = builder.ins().select(negative, minus_one, unsigned);

    let is_zero = builder.ins().fcmp(FloatCC::Equal, x, zero);
    let negative_sign = has_negative_sign(builder, x);
    let is_negative_zero = builder.ins().band(is_zero, negative_sign);
    let negative_zero = builder.ins().f64const(-0.0);
    let signed = builder
        .ins()
        .select(is_negative_zero, negative_zero, signed);

    let nan = builder.ins().fcmp(FloatCC::Unordered, x, x);
    builder.ins().select(nan, x, signed)
}

fn extremum(builder: &mut FunctionBuilder, lhs: Value, rhs: Value, maximum: bool) -> Value {
    let cc = if maximum {
        FloatCC::GreaterThan
    } else {
        FloatCC::LessThan
    };
    let comparison = builder.ins().fcmp(cc, lhs, rhs);
    let picked = builder.ins().select(comparison, lhs, rhs);

    let zero = builder.ins().f64const(0.0);
    let lhs_zero = builder.ins().fcmp(FloatCC::Equal, lhs, zero);
    let rhs_zero = builder.ins().fcmp(FloatCC::Equal, rhs, zero);
    let both_zero = builder.ins().band(lhs_zero, rhs_zero);
    let lhs_negative = has_negative_sign(builder, lhs);
    let rhs_negative = has_negative_sign(builder, rhs);
    let negative_zero = builder.ins().f64const(-0.0);
    let zero_result = if maximum {
        let lhs_positive = builder.ins().icmp_imm(IntCC::Equal, lhs_negative, 0);
        let rhs_positive = builder.ins().icmp_imm(IntCC::Equal, rhs_negative, 0);
        let either_positive = builder.ins().bor(lhs_positive, rhs_positive);
        builder.ins().select(either_positive, zero, negative_zero)
    } else {
        let either_negative = builder.ins().bor(lhs_negative, rhs_negative);
        builder.ins().select(either_negative, negative_zero, zero)
    };
    let with_zero = builder.ins().select(both_zero, zero_result, picked);

    let lhs_nan = builder.ins().fcmp(FloatCC::Unordered, lhs, lhs);
    builder.ins().select(lhs_nan, lhs, with_zero)
}

pub(super) fn min_or_max(
    builder: &mut FunctionBuilder,
    args: &[Value],
    maximum: bool,
    known: &[bool],
) -> Value {
    let identity = if maximum {
        f64::NEG_INFINITY
    } else {
        f64::INFINITY
    };
    let mut acc = builder.ins().f64const(identity);
    for (index, &arg) in args.iter().enumerate() {
        let arg = if known.get(index).copied().unwrap_or(false) {
            arg
        } else {
            value::to_number(builder, arg)
        };
        acc = extremum(builder, acc, arg, maximum);
    }
    acc
}

pub(super) fn hypot(builder: &mut FunctionBuilder, args: &[Value], known: &[bool]) -> Value {
    let mut scale = builder.ins().f64const(0.0);
    let mut sum = builder.ins().f64const(0.0);
    let mut has_nan = builder.ins().iconst(types::I8, 0);
    let mut has_inf = builder.ins().iconst(types::I8, 0);
    let zero = builder.ins().f64const(0.0);
    let one = builder.ins().f64const(1.0);
    let infinity = builder.ins().f64const(f64::INFINITY);
    let nan = builder.ins().f64const(f64::NAN);

    for (index, &arg) in args.iter().enumerate() {
        let x = if known.get(index).copied().unwrap_or(false) {
            arg
        } else {
            value::to_number(builder, arg)
        };
        let abs = builder.ins().fabs(x);
        let x_nan = builder.ins().fcmp(FloatCC::Unordered, x, x);
        let x_inf = builder.ins().fcmp(FloatCC::Equal, abs, infinity);
        has_nan = builder.ins().bor(has_nan, x_nan);
        has_inf = builder.ins().bor(has_inf, x_inf);

        let bigger = builder.ins().fcmp(FloatCC::GreaterThan, abs, scale);
        let new_scale = builder.ins().select(bigger, abs, scale);
        let scale_over_abs = builder.ins().fdiv(scale, abs);
        let abs_over_scale = builder.ins().fdiv(abs, scale);
        let ratio = builder.ins().select(bigger, scale_over_abs, abs_over_scale);
        let square = builder.ins().fmul(ratio, ratio);
        let scaled = builder.ins().fmul(sum, square);
        let increased = builder.ins().fadd(scaled, one);
        let accumulated = builder.ins().fadd(sum, square);
        let candidate = builder.ins().select(bigger, increased, accumulated);
        let scale_is_zero = builder.ins().fcmp(FloatCC::Equal, new_scale, zero);
        sum = builder.ins().select(scale_is_zero, zero, candidate);
        scale = new_scale;
    }

    let root = builder.ins().sqrt(sum);
    let magnitude = builder.ins().fmul(scale, root);
    let with_nan = builder.ins().select(has_nan, nan, magnitude);
    builder.ins().select(has_inf, infinity, with_nan)
}

pub(super) fn f16round(builder: &mut FunctionBuilder, x: Value) -> Value {
    let abs = builder.ins().fabs(x);
    let bits = builder.ins().bitcast(types::I64, MemFlags::new(), abs);
    let exponent = builder.ins().ushr_imm(bits, 52);
    let exponent = builder.ins().band_imm(exponent, 0x7ff);
    let min_normal = builder.ins().f64const(2f64.powi(-14));
    let subnormal = builder.ins().fcmp(FloatCC::LessThan, abs, min_normal);
    let normal_exponent = builder.ins().iadd_imm(exponent, -10);
    let subnormal_exponent = builder.ins().iconst(types::I64, 999);
    let chosen = builder
        .ins()
        .select(subnormal, subnormal_exponent, normal_exponent);
    let quantum_bits = builder.ins().ishl_imm(chosen, 52);
    let quantum = builder
        .ins()
        .bitcast(types::F64, MemFlags::new(), quantum_bits);
    let quotient = builder.ins().fdiv(abs, quantum);
    let rounded = builder.ins().nearest(quotient);
    let magnitude = builder.ins().fmul(rounded, quantum);
    let signed = builder.ins().fcopysign(magnitude, x);

    let overflow = builder.ins().f64const(65520.0);
    let too_big = builder
        .ins()
        .fcmp(FloatCC::GreaterThanOrEqual, abs, overflow);
    let infinity = builder.ins().f64const(f64::INFINITY);
    let infinity = builder.ins().fcopysign(infinity, x);
    builder.ins().select(too_big, infinity, signed)
}
