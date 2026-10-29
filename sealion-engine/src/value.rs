//! NaN-boxed value representation.
//!
//! Every JavaScript value is an `f64`. Numbers use the IEEE-754 encoding
//! directly; the five reserved quiet-NaN tag ranges in the top 16 bits carry
//! the non-numeric primitives. `0x7FF8`/`0xFFF8` (the NaNs the FPU actually
//! produces) are left as "number", so arithmetic results stay numbers.
//!
//! | tag      | meaning    | payload (bits 0..48) |
//! |----------|------------|----------------------|
//! | `0x7FF9` | `undefined`| —                    |
//! | `0x7FFA` | `null`     | —                    |
//! | `0x7FFD` | function   | closure pointer      |
//!
//! Booleans are still plain `0.0`/`1.0` numbers, so `true === 1` holds and is a
//! documented deviation until the object string/prototype work lands.

use cranelift_codegen::ir::{
    FuncRef, InstBuilder, MemFlags, Value, condcodes::FloatCC, condcodes::IntCC, types,
};
use cranelift_frontend::FunctionBuilder;

pub const TAG_UNDEFINED: u64 = 0x7FF9;
pub const TAG_NULL: u64 = 0x7FFA;
pub const TAG_FUNCTION: u64 = 0x7FFD;

/// `2**32`, the modulus used by `ToInt32`/`ToUint32`.
const TWO_POW_32: f64 = 4_294_967_296.0;

const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

pub fn undefined(builder: &mut FunctionBuilder) -> Value {
    let bits = builder
        .ins()
        .iconst(types::I64, (TAG_UNDEFINED << 48) as i64);
    builder.ins().bitcast(types::F64, MemFlags::new(), bits)
}

pub fn null(builder: &mut FunctionBuilder) -> Value {
    let bits = builder.ins().iconst(types::I64, (TAG_NULL << 48) as i64);
    builder.ins().bitcast(types::F64, MemFlags::new(), bits)
}

pub fn function(builder: &mut FunctionBuilder, ptr: Value) -> Value {
    let tag = builder
        .ins()
        .iconst(types::I64, (TAG_FUNCTION << 48) as i64);
    let bits = builder.ins().bor(tag, ptr);
    builder.ins().bitcast(types::F64, MemFlags::new(), bits)
}

/// Strips the tag, yielding the raw pointer bits of a closure value.
pub fn payload(builder: &mut FunctionBuilder, value: Value) -> Value {
    let bits = builder.ins().bitcast(types::I64, MemFlags::new(), value);
    builder.ins().band_imm(bits, PAYLOAD_MASK as i64)
}

fn tag_of(builder: &mut FunctionBuilder, value: Value) -> Value {
    let bits = builder.ins().bitcast(types::I64, MemFlags::new(), value);
    builder.ins().ushr_imm(bits, 48)
}

pub fn is_tagged(builder: &mut FunctionBuilder, value: Value) -> Value {
    let tag = tag_of(builder, value);
    let low = builder.ins().iconst(types::I64, TAG_UNDEFINED as i64);
    let high = builder.ins().iconst(types::I64, TAG_FUNCTION as i64);
    let ge = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, tag, low);
    let le = builder
        .ins()
        .icmp(IntCC::UnsignedLessThanOrEqual, tag, high);
    builder.ins().band(ge, le)
}

pub fn not_tagged(builder: &mut FunctionBuilder, value: Value) -> Value {
    let tagged = is_tagged(builder, value);
    builder.ins().icmp_imm(IntCC::Equal, tagged, 0)
}

/// `true` when the value is a number that is `NaN`; tagged values are quiet
/// NaNs too, so they have to be excluded explicitly.
pub fn is_nan(builder: &mut FunctionBuilder, value: Value) -> Value {
    let unordered = builder.ins().fcmp(FloatCC::Unordered, value, value);
    let number = not_tagged(builder, value);
    builder.ins().band(unordered, number)
}

/// `true` for numbers that are neither `NaN` nor `±Infinity`.
pub fn is_finite(builder: &mut FunctionBuilder, value: Value) -> Value {
    let abs = builder.ins().fabs(value);
    let inf = builder.ins().f64const(f64::INFINITY);
    let finite = builder.ins().fcmp(FloatCC::LessThan, abs, inf);
    let number = not_tagged(builder, value);
    builder.ins().band(finite, number)
}

pub fn from_bool(builder: &mut FunctionBuilder, value: Value) -> Value {
    let ext = builder.ins().uextend(types::I32, value);
    builder.ins().fcvt_from_uint(types::F64, ext)
}

/// Truthiness: only functions are truthy among the tagged values, and numbers
/// keep the plain `!= 0` test (which is already false for NaNs).
pub fn truthy(builder: &mut FunctionBuilder, value: Value) -> Value {
    let tagged = is_tagged(builder, value);
    let tag = tag_of(builder, value);
    let fn_tag = builder.ins().iconst(types::I64, TAG_FUNCTION as i64);
    let is_fn = builder.ins().icmp(IntCC::Equal, tag, fn_tag);
    let zero = builder.ins().f64const(0.0);
    let num = builder.ins().fcmp(FloatCC::OrderedNotEqual, value, zero);
    builder.ins().select(tagged, is_fn, num)
}

/// `===` over the NaN-boxed encoding: bit equality when either side is tagged,
/// numeric equality otherwise (`0 === -0` is true, `NaN === NaN` is false).
pub fn strict_equal(builder: &mut FunctionBuilder, lhs: Value, rhs: Value) -> Value {
    let lhs_bits = builder.ins().bitcast(types::I64, MemFlags::new(), lhs);
    let rhs_bits = builder.ins().bitcast(types::I64, MemFlags::new(), rhs);
    let bits_eq = builder.ins().icmp(IntCC::Equal, lhs_bits, rhs_bits);
    let num_eq = builder.ins().fcmp(FloatCC::Equal, lhs, rhs);
    let lhs_tagged = is_tagged(builder, lhs);
    let rhs_tagged = is_tagged(builder, rhs);
    let either_tagged = builder.ins().bor(lhs_tagged, rhs_tagged);
    let result = builder.ins().select(either_tagged, bits_eq, num_eq);
    from_bool(builder, result)
}

/// `true` when the value is `undefined` or `null`.
pub fn is_nullish(builder: &mut FunctionBuilder, value: Value) -> Value {
    let tag = tag_of(builder, value);
    let undef = builder.ins().iconst(types::I64, TAG_UNDEFINED as i64);
    let null = builder.ins().iconst(types::I64, TAG_NULL as i64);
    let is_undef = builder.ins().icmp(IntCC::Equal, tag, undef);
    let is_null = builder.ins().icmp(IntCC::Equal, tag, null);
    builder.ins().bor(is_undef, is_null)
}

/// `==` for the current value domain (numbers, booleans, `undefined`, `null`,
/// functions): strict equality plus `null == undefined`. Strings and objects
/// would need coercions that do not exist yet.
pub fn loose_equal(builder: &mut FunctionBuilder, lhs: Value, rhs: Value) -> Value {
    let strict = strict_equal(builder, lhs, rhs);
    let lhs_nullish = is_nullish(builder, lhs);
    let rhs_nullish = is_nullish(builder, rhs);
    let both_nullish = builder.ins().band(lhs_nullish, rhs_nullish);
    let zero = builder.ins().f64const(0.0);
    let strict_true = builder.ins().fcmp(FloatCC::OrderedNotEqual, strict, zero);
    let result = builder.ins().bor(strict_true, both_nullish);
    from_bool(builder, result)
}

/// ECMAScript `ToInt32`: `x` modulo `2**32`, truncated toward zero, reinterpreted
/// as a two's-complement 32-bit integer. `fmod` yields `NaN` for `NaN`/`±Inf` and
/// the saturating conversion maps `NaN` to `0`, exactly as the spec requires.
/// Tagged values are quiet NaNs, so `undefined`, `null` and functions become `0`.
pub fn to_int32(builder: &mut FunctionBuilder, fmod_ref: FuncRef, value: Value) -> Value {
    let modulus = builder.ins().f64const(TWO_POW_32);
    let call = builder.ins().call(fmod_ref, &[value, modulus]);
    let remainder = builder.inst_results(call)[0];
    let wide = builder.ins().fcvt_to_sint_sat(types::I64, remainder);
    builder.ins().ireduce(types::I32, wide)
}

pub fn from_int32(builder: &mut FunctionBuilder, value: Value) -> Value {
    builder.ins().fcvt_from_sint(types::F64, value)
}

pub fn from_uint32(builder: &mut FunctionBuilder, value: Value) -> Value {
    let wide = builder.ins().uextend(types::I64, value);
    builder.ins().fcvt_from_uint(types::F64, wide)
}
