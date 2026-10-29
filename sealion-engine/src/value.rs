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
    InstBuilder, MemFlags, Value, condcodes::FloatCC, condcodes::IntCC, types,
};
use cranelift_frontend::FunctionBuilder;

pub const TAG_UNDEFINED: u64 = 0x7FF9;
pub const TAG_NULL: u64 = 0x7FFA;
pub const TAG_FUNCTION: u64 = 0x7FFD;

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

fn is_tagged(builder: &mut FunctionBuilder, value: Value) -> Value {
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
