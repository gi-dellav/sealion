//! NaN-boxed value representation.
//!
//! Every JavaScript value is an `f64`. Numbers use the IEEE-754 encoding
//! directly; the reserved quiet-NaN tag ranges in the top 16 bits carry the
//! non-numeric primitives. `0x7FF8`/`0xFFF8` (the NaNs the FPU actually
//! produces) are left as "number", so arithmetic results stay numbers.
//!
//! | tag      | meaning        | payload (bits 0..48) |
//! |----------|----------------|----------------------|
//! | `0x7FF9` | `undefined`    | —                    |
//! | `0x7FFA` | `null`         | —                    |
//! | `0x7FFB` | boolean        | `0` or `1`           |
//! | `0x7FFC` | uninitialized  | —                    |
//! | `0x7FFD` | function       | closure pointer      |
//! | `0x7FFE` | string         | `JsString` pointer   |
//! | `0x7FFF` | object         | `JsObject` pointer   |
//!
//! The uninitialized tag implements the temporal dead zone for `let`, `const`
//! and parameters: loading such a slot traps until the declaration has run.

use cranelift_codegen::ir::{
    BlockArg, FuncRef, InstBuilder, MemFlags, Value, condcodes::FloatCC, condcodes::IntCC, types,
};
use cranelift_frontend::FunctionBuilder;

pub const TAG_UNDEFINED: u64 = 0x7FF9;
pub const TAG_NULL: u64 = 0x7FFA;
pub const TAG_BOOLEAN: u64 = 0x7FFB;
pub const TAG_UNINITIALIZED: u64 = 0x7FFC;
pub const TAG_FUNCTION: u64 = 0x7FFD;
pub const TAG_STRING: u64 = 0x7FFE;
pub const TAG_OBJECT: u64 = 0x7FFF;

/// `2**32`, the modulus used by `ToInt32`/`ToUint32`.
const TWO_POW_32: f64 = 4_294_967_296.0;

pub(crate) const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

/// Runtime entry points that implement string semantics.
///
/// The AOT backend leaves this unset and rejects string literals; the dynamic
/// (JIT) backend fills it in from the engine's host helpers.
#[derive(Clone, Copy)]
pub struct StringOps {
    pub from_utf8: FuncRef,
    pub add: FuncRef,
    pub strict_equal: FuncRef,
    pub loose_equal: FuncRef,
    pub same_value: FuncRef,
    pub truthy: FuncRef,
    pub to_number: FuncRef,
    pub to_string: FuncRef,
    pub compare: FuncRef,
}

/// Runtime entry points that implement object semantics.
///
/// Like [`StringOps`], the AOT backend leaves this unset and rejects objects;
/// the dynamic (JIT) backend fills it in from the engine's host helpers.
#[derive(Clone, Copy)]
pub struct ObjectOps {
    /// Allocates an empty object and returns its raw pointer bits.
    pub new: FuncRef,
    pub get: FuncRef,
    pub set: FuncRef,
    pub has: FuncRef,
    pub delete: FuncRef,
}

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

pub fn boolean(builder: &mut FunctionBuilder, value: bool) -> Value {
    let bit = builder.ins().iconst(types::I32, value as i64);
    from_bool(builder, bit)
}

pub fn uninitialized(builder: &mut FunctionBuilder) -> Value {
    let bits = builder
        .ins()
        .iconst(types::I64, (TAG_UNINITIALIZED << 48) as i64);
    builder.ins().bitcast(types::F64, MemFlags::new(), bits)
}

pub fn function(builder: &mut FunctionBuilder, ptr: Value) -> Value {
    let tag = builder
        .ins()
        .iconst(types::I64, (TAG_FUNCTION << 48) as i64);
    let bits = builder.ins().bor(tag, ptr);
    builder.ins().bitcast(types::F64, MemFlags::new(), bits)
}

pub fn string(builder: &mut FunctionBuilder, ptr: Value) -> Value {
    let tag = builder.ins().iconst(types::I64, (TAG_STRING << 48) as i64);
    let bits = builder.ins().bor(tag, ptr);
    builder.ins().bitcast(types::F64, MemFlags::new(), bits)
}

pub fn object(builder: &mut FunctionBuilder, ptr: Value) -> Value {
    let tag = builder.ins().iconst(types::I64, (TAG_OBJECT << 48) as i64);
    let bits = builder.ins().bor(tag, ptr);
    builder.ins().bitcast(types::F64, MemFlags::new(), bits)
}

/// Strips the tag, yielding the raw pointer bits of a closure or string value.
pub fn payload(builder: &mut FunctionBuilder, value: Value) -> Value {
    let bits = builder.ins().bitcast(types::I64, MemFlags::new(), value);
    builder.ins().band_imm(bits, PAYLOAD_MASK as i64)
}

fn bits_of(builder: &mut FunctionBuilder, value: Value) -> Value {
    builder.ins().bitcast(types::I64, MemFlags::new(), value)
}

fn tag_of(builder: &mut FunctionBuilder, value: Value) -> Value {
    let bits = bits_of(builder, value);
    builder.ins().ushr_imm(bits, 48)
}

pub fn is_tagged(builder: &mut FunctionBuilder, value: Value) -> Value {
    let tag = tag_of(builder, value);
    let low = builder.ins().iconst(types::I64, TAG_UNDEFINED as i64);
    let high = builder.ins().iconst(types::I64, TAG_OBJECT as i64);
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

fn tag_is(builder: &mut FunctionBuilder, value: Value, tag: u64) -> Value {
    let actual = tag_of(builder, value);
    let expected = builder.ins().iconst(types::I64, tag as i64);
    builder.ins().icmp(IntCC::Equal, actual, expected)
}

pub fn is_undefined(builder: &mut FunctionBuilder, value: Value) -> Value {
    tag_is(builder, value, TAG_UNDEFINED)
}

pub fn is_uninitialized(builder: &mut FunctionBuilder, value: Value) -> Value {
    tag_is(builder, value, TAG_UNINITIALIZED)
}

fn is_boolean(builder: &mut FunctionBuilder, value: Value) -> Value {
    tag_is(builder, value, TAG_BOOLEAN)
}

fn call1(builder: &mut FunctionBuilder, func_ref: FuncRef, arg: Value) -> Value {
    let call = builder.ins().call(func_ref, &[arg]);
    builder.inst_results(call)[0]
}

fn call2(builder: &mut FunctionBuilder, func_ref: FuncRef, lhs: Value, rhs: Value) -> Value {
    let call = builder.ins().call(func_ref, &[lhs, rhs]);
    builder.inst_results(call)[0]
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

/// Wraps a 0/1 condition in a tagged boolean value.
pub fn from_bool(builder: &mut FunctionBuilder, value: Value) -> Value {
    let wide = builder.ins().uextend(types::I64, value);
    let payload = builder.ins().band_imm(wide, 1);
    let tag = builder.ins().iconst(types::I64, (TAG_BOOLEAN << 48) as i64);
    let bits = builder.ins().bor(tag, payload);
    builder.ins().bitcast(types::F64, MemFlags::new(), bits)
}

/// Truthiness: functions, objects, non-empty strings and tagged `true` are
/// truthy, the other tagged values are not, and numbers keep the plain `!= 0`
/// test (which is already false for NaNs).
pub fn truthy(builder: &mut FunctionBuilder, value: Value, strings: Option<&StringOps>) -> Value {
    let tagged = is_tagged(builder, value);
    let tag = tag_of(builder, value);
    let fn_tag = builder.ins().iconst(types::I64, TAG_FUNCTION as i64);
    let is_fn = builder.ins().icmp(IntCC::Equal, tag, fn_tag);
    let object_tag = builder.ins().iconst(types::I64, TAG_OBJECT as i64);
    let is_object = builder.ins().icmp(IntCC::Equal, tag, object_tag);
    let is_bool = is_boolean(builder, value);
    let bits = bits_of(builder, value);
    let low = builder.ins().band_imm(bits, 1);
    let bool_true = builder.ins().icmp_imm(IntCC::NotEqual, low, 0);
    let bool_truthy = builder.ins().band(is_bool, bool_true);
    let tagged_truthy = builder.ins().bor(is_fn, bool_truthy);
    let tagged_truthy = builder.ins().bor(tagged_truthy, is_object);
    let zero = builder.ins().f64const(0.0);
    let num = builder.ins().fcmp(FloatCC::OrderedNotEqual, value, zero);
    let inline = builder.ins().select(tagged, tagged_truthy, num);

    let Some(ops) = strings else {
        return inline;
    };
    // Only strings need the host runtime; everything else is already decided.
    let string_tag = builder.ins().iconst(types::I64, TAG_STRING as i64);
    let is_string = builder.ins().icmp(IntCC::Equal, tag, string_tag);
    let fast = builder.create_block();
    let slow = builder.create_block();
    let join = builder.create_block();
    builder.append_block_param(join, types::I8);
    builder.ins().brif(is_string, slow, &[], fast, &[]);

    builder.switch_to_block(fast);
    builder.ins().jump(join, &[BlockArg::Value(inline)]);

    builder.switch_to_block(slow);
    let string_truthy = call1(builder, ops.truthy, value);
    builder.ins().jump(join, &[BlockArg::Value(string_truthy)]);

    builder.switch_to_block(join);
    builder.block_params(join)[0]
}

/// `===` over the NaN-boxed encoding: string contents compare equal, bit
/// equality applies when either side is tagged, and numeric equality otherwise
/// (`0 === -0` is true, `NaN === NaN` is false).
pub fn strict_equal(
    builder: &mut FunctionBuilder,
    lhs: Value,
    rhs: Value,
    strings: Option<&StringOps>,
) -> Value {
    if let Some(ops) = strings {
        let lhs_tagged = is_tagged(builder, lhs);
        let rhs_tagged = is_tagged(builder, rhs);
        let any_tagged = builder.ins().bor(lhs_tagged, rhs_tagged);
        let fast = builder.create_block();
        let slow = builder.create_block();
        let join = builder.create_block();
        builder.append_block_param(join, types::F64);
        builder.ins().brif(any_tagged, slow, &[], fast, &[]);

        builder.switch_to_block(fast);
        let num_eq = builder.ins().fcmp(FloatCC::Equal, lhs, rhs);
        let equal = from_bool(builder, num_eq);
        builder.ins().jump(join, &[BlockArg::Value(equal)]);

        builder.switch_to_block(slow);
        let called = call2(builder, ops.strict_equal, lhs, rhs);
        builder.ins().jump(join, &[BlockArg::Value(called)]);

        builder.switch_to_block(join);
        return builder.block_params(join)[0];
    }
    let lhs_bits = bits_of(builder, lhs);
    let rhs_bits = bits_of(builder, rhs);
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

/// Converts a boolean to a number while leaving every other value untouched.
/// This is only used by loose equality, where `null` must not become `0`.
fn boolean_to_number(builder: &mut FunctionBuilder, value: Value) -> Value {
    let bits = bits_of(builder, value);
    let low = builder.ins().band_imm(bits, 1);
    let number = builder.ins().fcvt_from_uint(types::F64, low);
    let is_bool = is_boolean(builder, value);
    builder.ins().select(is_bool, number, value)
}

/// `==` for the current value domain (numbers, booleans, `undefined`, `null`,
/// functions and strings): strict equality, `null == undefined`, booleans
/// compared as numbers, and strings converted to numbers when compared with a
/// primitive of another type.
pub fn loose_equal(
    builder: &mut FunctionBuilder,
    lhs: Value,
    rhs: Value,
    strings: Option<&StringOps>,
) -> Value {
    if let Some(ops) = strings {
        let lhs_tagged = is_tagged(builder, lhs);
        let rhs_tagged = is_tagged(builder, rhs);
        let any_tagged = builder.ins().bor(lhs_tagged, rhs_tagged);
        let fast = builder.create_block();
        let slow = builder.create_block();
        let join = builder.create_block();
        builder.append_block_param(join, types::F64);
        builder.ins().brif(any_tagged, slow, &[], fast, &[]);

        builder.switch_to_block(fast);
        let num_eq = builder.ins().fcmp(FloatCC::Equal, lhs, rhs);
        let equal = from_bool(builder, num_eq);
        builder.ins().jump(join, &[BlockArg::Value(equal)]);

        builder.switch_to_block(slow);
        let called = call2(builder, ops.loose_equal, lhs, rhs);
        builder.ins().jump(join, &[BlockArg::Value(called)]);

        builder.switch_to_block(join);
        return builder.block_params(join)[0];
    }
    let strict = strict_equal(builder, lhs, rhs, None);
    let strict_true = truthy(builder, strict, None);
    let lhs_nullish = is_nullish(builder, lhs);
    let rhs_nullish = is_nullish(builder, rhs);
    let both_nullish = builder.ins().band(lhs_nullish, rhs_nullish);
    let normal = builder.ins().bor(strict_true, both_nullish);

    let lhs_number = boolean_to_number(builder, lhs);
    let rhs_number = boolean_to_number(builder, rhs);
    let lhs_bool = is_boolean(builder, lhs);
    let rhs_bool = is_boolean(builder, rhs);
    let either_bool = builder.ins().bor(lhs_bool, rhs_bool);
    let numbers_equal = builder.ins().fcmp(FloatCC::Equal, lhs_number, rhs_number);

    let result = builder.ins().select(either_bool, numbers_equal, normal);
    from_bool(builder, result)
}

/// ECMAScript `SameValue`, used by `Object.is`.
pub fn same_value(
    builder: &mut FunctionBuilder,
    lhs: Value,
    rhs: Value,
    strings: Option<&StringOps>,
) -> Value {
    if let Some(ops) = strings {
        return call2(builder, ops.same_value, lhs, rhs);
    }
    let lhs_bits = bits_of(builder, lhs);
    let rhs_bits = bits_of(builder, rhs);
    let bits_eq = builder.ins().icmp(IntCC::Equal, lhs_bits, rhs_bits);
    let lhs_nan = is_nan(builder, lhs);
    let rhs_nan = is_nan(builder, rhs);
    let both_nan = builder.ins().band(lhs_nan, rhs_nan);
    let result = builder.ins().bor(bits_eq, both_nan);
    from_bool(builder, result)
}

/// ECMAScript `ToNumber` for the current primitive domain: booleans become
/// `1`/`0`, `null` becomes `0`, strings parse as numbers, and `undefined` and
/// functions become `NaN`.
pub fn to_number(
    builder: &mut FunctionBuilder,
    value: Value,
    strings: Option<&StringOps>,
) -> Value {
    if let Some(ops) = strings {
        let tagged = is_tagged(builder, value);
        let fast = builder.create_block();
        let slow = builder.create_block();
        let join = builder.create_block();
        builder.append_block_param(join, types::F64);
        builder.ins().brif(tagged, slow, &[], fast, &[]);

        builder.switch_to_block(fast);
        builder.ins().jump(join, &[BlockArg::Value(value)]);

        builder.switch_to_block(slow);
        let converted = call1(builder, ops.to_number, value);
        builder.ins().jump(join, &[BlockArg::Value(converted)]);

        builder.switch_to_block(join);
        return builder.block_params(join)[0];
    }
    let bits = bits_of(builder, value);
    let tagged = is_tagged(builder, value);
    let low = builder.ins().band_imm(bits, 1);
    let bool_number = builder.ins().fcvt_from_uint(types::F64, low);
    let is_bool = is_boolean(builder, value);
    let is_null = tag_is(builder, value, TAG_NULL);
    let zero = builder.ins().f64const(0.0);
    let nan = builder.ins().f64const(f64::NAN);
    let non_bool = builder.ins().select(is_null, zero, nan);
    let tagged_number = builder.ins().select(is_bool, bool_number, non_bool);
    builder.ins().select(tagged, tagged_number, value)
}

/// ECMAScript `ToString`, available only with the dynamic runtime.
pub fn to_string(
    builder: &mut FunctionBuilder,
    value: Value,
    strings: Option<&StringOps>,
) -> Option<Value> {
    strings.map(|ops| call1(builder, ops.to_string, value))
}

/// The JavaScript `+` operator, including string concatenation.
pub fn add(
    builder: &mut FunctionBuilder,
    lhs: Value,
    rhs: Value,
    strings: Option<&StringOps>,
) -> Option<Value> {
    let ops = strings?;
    let lhs_tagged = is_tagged(builder, lhs);
    let rhs_tagged = is_tagged(builder, rhs);
    let any_tagged = builder.ins().bor(lhs_tagged, rhs_tagged);
    let fast = builder.create_block();
    let slow = builder.create_block();
    let join = builder.create_block();
    builder.append_block_param(join, types::F64);
    builder.ins().brif(any_tagged, slow, &[], fast, &[]);

    builder.switch_to_block(fast);
    let sum = builder.ins().fadd(lhs, rhs);
    builder.ins().jump(join, &[BlockArg::Value(sum)]);

    builder.switch_to_block(slow);
    let called = call2(builder, ops.add, lhs, rhs);
    builder.ins().jump(join, &[BlockArg::Value(called)]);

    builder.switch_to_block(join);
    Some(builder.block_params(join)[0])
}

/// Abstract relational comparison: returns `-1`, `0`, `1`, or `i32::MIN` when
/// the operands are unordered (`NaN`).
pub fn compare(
    builder: &mut FunctionBuilder,
    lhs: Value,
    rhs: Value,
    strings: Option<&StringOps>,
) -> Option<Value> {
    let ops = strings?;
    let lhs_tagged = is_tagged(builder, lhs);
    let rhs_tagged = is_tagged(builder, rhs);
    let any_tagged = builder.ins().bor(lhs_tagged, rhs_tagged);
    let fast = builder.create_block();
    let slow = builder.create_block();
    let join = builder.create_block();
    builder.append_block_param(join, types::I32);
    builder.ins().brif(any_tagged, slow, &[], fast, &[]);

    builder.switch_to_block(fast);
    let unordered = builder.ins().fcmp(FloatCC::Unordered, lhs, rhs);
    let less = builder.ins().fcmp(FloatCC::LessThan, lhs, rhs);
    let greater = builder.ins().fcmp(FloatCC::GreaterThan, lhs, rhs);
    let minus_one = builder.ins().iconst(types::I32, -1);
    let zero = builder.ins().iconst(types::I32, 0);
    let one = builder.ins().iconst(types::I32, 1);
    let ordered = builder.ins().select(less, minus_one, zero);
    let ordered = builder.ins().select(greater, one, ordered);
    let nan = builder.ins().iconst(types::I32, i64::from(i32::MIN));
    let result = builder.ins().select(unordered, nan, ordered);
    builder.ins().jump(join, &[BlockArg::Value(result)]);

    builder.switch_to_block(slow);
    let called = call2(builder, ops.compare, lhs, rhs);
    builder.ins().jump(join, &[BlockArg::Value(called)]);

    builder.switch_to_block(join);
    Some(builder.block_params(join)[0])
}

/// ECMAScript `ToInt32`: `x` modulo `2**32`, truncated toward zero, reinterpreted
/// as a two's-complement 32-bit integer. `fmod` yields `NaN` for `NaN`/`±Inf` and
/// the saturating conversion maps `NaN` to `0`, exactly as the spec requires.
pub fn to_int32(
    builder: &mut FunctionBuilder,
    fmod_ref: FuncRef,
    value: Value,
    strings: Option<&StringOps>,
) -> Value {
    let value = to_number(builder, value, strings);
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
