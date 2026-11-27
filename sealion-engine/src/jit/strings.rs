//! Host helpers for JavaScript string values.
//!
//! Compiled code stores a string as a NaN-boxed pointer to a
//! [`sealion_string::JsString`]. These entry points are resolved by the JIT
//! loader through a fixed symbol table instead of `dlsym`, because the engine
//! is normally linked into a test or application binary whose symbols are not
//! exported.

use std::{cmp::Ordering, io::Write, mem::ManuallyDrop, ptr::NonNull, str, sync::OnceLock};

use sealion_string::{CodePoint, JsString, RawJsString};

use super::arrays::{arg, arg_count, call};
use super::objects::{define_property, native_function, new_array, new_object, tagged_object};
use crate::value::{
    PAYLOAD_MASK, TAG_BOOLEAN, TAG_FUNCTION, TAG_NULL, TAG_OBJECT, TAG_STRING, TAG_UNDEFINED,
    TAG_UNINITIALIZED,
};

fn string_pointer(value: f64) -> Option<NonNull<RawJsString>> {
    let bits = value.to_bits();
    if bits >> 48 != TAG_STRING {
        return None;
    }
    NonNull::new((bits & PAYLOAD_MASK) as *mut RawJsString)
}

pub(super) fn with_string<R>(value: f64, f: impl FnOnce(&JsString) -> R) -> Option<R> {
    let ptr = string_pointer(value)?;
    // The compiled program never drops string values, so a temporary borrow
    // must not run the vtable destructor.
    let string = ManuallyDrop::new(unsafe { JsString::from_raw(ptr) });
    Some(f(&string))
}

pub(super) fn tagged_string(string: JsString) -> f64 {
    let ptr = string.into_raw().as_ptr() as u64;
    f64::from_bits((TAG_STRING << 48) | ptr)
}

pub(super) fn tagged_bool(value: bool) -> f64 {
    f64::from_bits((TAG_BOOLEAN << 48) | u64::from(value))
}

pub(super) fn tagged_undefined() -> f64 {
    f64::from_bits(TAG_UNDEFINED << 48)
}

pub(super) fn tagged_null() -> f64 {
    f64::from_bits(TAG_NULL << 48)
}

fn tag(value: f64) -> u64 {
    value.to_bits() >> 48
}

pub(super) fn is_string(value: f64) -> bool {
    tag(value) == TAG_STRING
}

pub(super) fn is_object(value: f64) -> bool {
    tag(value) == TAG_OBJECT
}

pub(super) fn is_tagged(value: f64) -> bool {
    (TAG_UNDEFINED..=TAG_OBJECT).contains(&tag(value))
}

pub(super) fn is_nullish(value: f64) -> bool {
    matches!(tag(value), TAG_UNDEFINED | TAG_NULL)
}

/// Host-side truthiness, mirroring `value::truthy`.
pub(super) fn value_truthy(value: f64) -> bool {
    match tag(value) {
        TAG_UNDEFINED | TAG_NULL | TAG_UNINITIALIZED => false,
        TAG_BOOLEAN => value.to_bits() & 1 != 0,
        TAG_FUNCTION | TAG_OBJECT => true,
        TAG_STRING => with_string(value, |string| !string.is_empty()).unwrap_or(false),
        _ => !value.is_nan() && value != 0.0,
    }
}

/// Host-side `===`, mirroring `value::strict_equal`.
pub(super) fn values_strict_equal(lhs: f64, rhs: f64) -> bool {
    if is_string(lhs) || is_string(rhs) {
        is_string(lhs)
            && is_string(rhs)
            && with_string(lhs, |lhs| {
                with_string(rhs, |rhs| lhs == rhs).unwrap_or(false)
            })
            .unwrap_or(false)
    } else if is_tagged(lhs) || is_tagged(rhs) {
        lhs.to_bits() == rhs.to_bits()
    } else {
        lhs == rhs
    }
}

/// Host-side `SameValueZero`, the comparison used by `Array.prototype.includes`.
pub(super) fn same_value_zero(lhs: f64, rhs: f64) -> bool {
    if is_string(lhs) || is_string(rhs) {
        values_strict_equal(lhs, rhs)
    } else if is_tagged(lhs) || is_tagged(rhs) {
        lhs.to_bits() == rhs.to_bits()
    } else {
        lhs == rhs || (lhs.is_nan() && rhs.is_nan())
    }
}

fn number_string(value: f64) -> JsString {
    if value == 0.0 {
        return JsString::from("0");
    }
    JsString::from(value)
}

pub(super) fn to_js_string(value: f64) -> JsString {
    match tag(value) {
        TAG_STRING => with_string(value, JsString::clone).unwrap_or_default(),
        TAG_UNDEFINED => JsString::from("undefined"),
        TAG_NULL => JsString::from("null"),
        TAG_FUNCTION => JsString::from("function"),
        TAG_OBJECT => super::dates::date_to_js_string(value)
            .or_else(|| super::arrays::array_to_js_string(value))
            .unwrap_or_else(|| JsString::from("[object Object]")),
        TAG_BOOLEAN => JsString::from(if value.to_bits() & 1 != 0 {
            "true"
        } else {
            "false"
        }),
        _ => number_string(value),
    }
}

pub(super) fn to_number_value(value: f64) -> f64 {
    match tag(value) {
        TAG_STRING => with_string(value, JsString::to_number).unwrap_or(f64::NAN),
        TAG_NULL => 0.0,
        TAG_BOOLEAN => {
            if value.to_bits() & 1 != 0 {
                1.0
            } else {
                0.0
            }
        }
        TAG_UNDEFINED | TAG_FUNCTION | TAG_OBJECT => f64::NAN,
        _ => value,
    }
}

extern "C" fn sealion_string_from_utf8(ptr: *const u8, len: u64) -> u64 {
    let bytes = if len == 0 {
        &[][..]
    } else if ptr.is_null() {
        return 0;
    } else {
        unsafe { std::slice::from_raw_parts(ptr, len as usize) }
    };
    let text = str::from_utf8(bytes).unwrap_or("");
    tagged_string(JsString::from(text)).to_bits() & PAYLOAD_MASK
}

extern "C" fn sealion_string_to_number(value: f64) -> f64 {
    to_number_value(value)
}

extern "C" fn sealion_string_to_string(value: f64) -> f64 {
    tagged_string(to_js_string(value))
}

extern "C" fn sealion_string_truthy(value: f64) -> i8 {
    let truthy = with_string(value, |string| !string.is_empty()).unwrap_or(false);
    i8::from(truthy)
}

extern "C" fn sealion_string_add(lhs: f64, rhs: f64) -> f64 {
    if is_string(lhs) || is_string(rhs) || is_object(lhs) || is_object(rhs) {
        let lhs = to_js_string(lhs);
        let rhs = to_js_string(rhs);
        tagged_string(JsString::concat(lhs.as_str(), rhs.as_str()))
    } else {
        to_number_value(lhs) + to_number_value(rhs)
    }
}

extern "C" fn sealion_string_strict_equal(lhs: f64, rhs: f64) -> f64 {
    tagged_bool(values_strict_equal(lhs, rhs))
}

extern "C" fn sealion_string_loose_equal(lhs: f64, rhs: f64) -> f64 {
    let lhs_string = is_string(lhs);
    let rhs_string = is_string(rhs);
    let equal = if lhs_string && rhs_string {
        with_string(lhs, |lhs| {
            with_string(rhs, |rhs| lhs == rhs).unwrap_or(false)
        })
        .unwrap_or(false)
    } else if lhs_string || rhs_string {
        let lhs_tag = tag(lhs);
        let rhs_tag = tag(rhs);
        let lhs_nullish = matches!(lhs_tag, TAG_UNDEFINED | TAG_NULL);
        let rhs_nullish = matches!(rhs_tag, TAG_UNDEFINED | TAG_NULL);
        let lhs_function = lhs_tag == TAG_FUNCTION;
        let rhs_function = rhs_tag == TAG_FUNCTION;
        if lhs_nullish || rhs_nullish || lhs_function || rhs_function {
            false
        } else {
            to_number_value(lhs) == to_number_value(rhs)
        }
    } else {
        let lhs_tag = tag(lhs);
        let rhs_tag = tag(rhs);
        let lhs_nullish = matches!(lhs_tag, TAG_UNDEFINED | TAG_NULL);
        let rhs_nullish = matches!(rhs_tag, TAG_UNDEFINED | TAG_NULL);
        let lhs_boolean = lhs_tag == TAG_BOOLEAN;
        let rhs_boolean = rhs_tag == TAG_BOOLEAN;
        if lhs_nullish && rhs_nullish {
            true
        } else if lhs_boolean || rhs_boolean {
            let number = |value: f64| {
                if tag(value) == TAG_BOOLEAN {
                    f64::from((value.to_bits() & 1) as u32)
                } else {
                    value
                }
            };
            number(lhs) == number(rhs)
        } else if is_tagged(lhs) || is_tagged(rhs) {
            lhs.to_bits() == rhs.to_bits()
        } else {
            lhs == rhs
        }
    };
    tagged_bool(equal)
}

extern "C" fn sealion_string_same_value(lhs: f64, rhs: f64) -> f64 {
    let same = if is_string(lhs) && is_string(rhs) {
        with_string(lhs, |lhs| {
            with_string(rhs, |rhs| lhs == rhs).unwrap_or(false)
        })
        .unwrap_or(false)
    } else if is_string(lhs) || is_string(rhs) {
        false
    } else {
        lhs.to_bits() == rhs.to_bits()
            || (lhs.is_nan() && rhs.is_nan() && !is_tagged(lhs) && !is_tagged(rhs))
    };
    tagged_bool(same)
}

extern "C" fn sealion_string_compare(lhs: f64, rhs: f64) -> i32 {
    if is_string(lhs) && is_string(rhs) {
        return with_string(lhs, |lhs| {
            with_string(rhs, |rhs| match lhs.as_str().cmp(&rhs.as_str()) {
                Ordering::Less => -1,
                Ordering::Equal => 0,
                Ordering::Greater => 1,
            })
            .unwrap_or(i32::MIN)
        })
        .unwrap_or(i32::MIN);
    }
    let lhs = to_number_value(lhs);
    let rhs = to_number_value(rhs);
    if lhs.is_nan() || rhs.is_nan() {
        return i32::MIN;
    }
    match lhs.partial_cmp(&rhs) {
        Some(Ordering::Less) => -1,
        Some(Ordering::Equal) => 0,
        Some(Ordering::Greater) => 1,
        None => i32::MIN,
    }
}

extern "C" fn sealion_console_log(values: *const f64, len: u64) {
    let values = unsafe { std::slice::from_raw_parts(values, len as usize) };
    let mut line = String::new();
    for (index, &value) in values.iter().enumerate() {
        if index != 0 {
            line.push(' ');
        }
        line.push_str(&to_js_string(value).to_std_string_lossy());
    }
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}

pub(super) type Method = extern "C" fn(i64, *const u8, f64, f64) -> f64;

/// The tagged string a method record captured as its `env`.
fn receiver_value(env: i64) -> f64 {
    f64::from_bits((TAG_STRING << 48) | (env as u64 & PAYLOAD_MASK))
}

fn receiver(env: i64) -> JsString {
    with_string(receiver_value(env), JsString::clone).unwrap_or_default()
}

fn is_undefined(value: f64) -> bool {
    value.to_bits() >> 48 == TAG_UNDEFINED
}

/// Builds a string from UTF-16 code units.
fn js(units: &[u16]) -> JsString {
    if units.is_empty() {
        JsString::from("")
    } else {
        JsString::from(units)
    }
}

/// `ToIntegerOrInfinity` clamped into `0..=length`, with `default` for
/// `undefined` and negatives clamped to zero.
fn clamp_index(value: f64, length: usize, default: usize) -> usize {
    if is_undefined(value) {
        return default;
    }
    let number = to_number_value(value).trunc();
    if number.is_nan() || number <= 0.0 {
        return 0;
    }
    if number >= length as f64 {
        return length;
    }
    number as usize
}

/// Like [`clamp_index`] but negatives count back from the end, as `slice`,
/// `at` and `substr` require.
fn relative_index(value: f64, length: usize, default: usize) -> usize {
    if is_undefined(value) {
        return default;
    }
    let number = to_number_value(value).trunc();
    if number.is_nan() {
        return 0;
    }
    if number < 0.0 {
        let shifted = length as f64 + number;
        if shifted <= 0.0 {
            return 0;
        }
        (shifted as usize).min(length)
    } else if number >= length as f64 {
        length
    } else {
        number as usize
    }
}

/// `ToIntegerOrInfinity` for single-code-unit access: `None` when the index
/// falls outside the string, unlike [`clamp_index`], which saturates.
fn character_index(value: f64, length: usize) -> Option<usize> {
    let number = to_number_value(value).trunc();
    if number.is_nan() {
        return (length > 0).then_some(0);
    }
    if number < 0.0 || number >= length as f64 {
        return None;
    }
    Some(number as usize)
}

/// First occurrence of `needle` in `haystack` at or after `from`.
fn find(haystack: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    let last = haystack.len() - needle.len();
    (from..=last).find(|&index| haystack[index..index + needle.len()] == *needle)
}

/// The native closure for `string.method`, or `None` when the key names no
/// method. Like the array and map closures it is a two-word record holding the
/// entry point and the receiver, so `var f = s.trim` works and `this` is
/// ignored.
pub(super) fn method_property(value: f64, key: &JsString) -> Option<f64> {
    if !is_string(value) {
        return None;
    }
    let method = method(key)?;
    let record = Box::into_raw(Box::new([
        method as usize as u64,
        value.to_bits() & PAYLOAD_MASK,
    ]));
    Some(f64::from_bits(
        (TAG_FUNCTION << 48) | (record as u64 & PAYLOAD_MASK),
    ))
}

fn method(key: &JsString) -> Option<Method> {
    Some(match key.to_std_string_lossy().as_str() {
        "charAt" => char_at,
        "charCodeAt" => char_code_at,
        "codePointAt" => code_point_at,
        "at" => at,
        "indexOf" => index_of,
        "lastIndexOf" => last_index_of,
        "includes" => includes,
        "startsWith" => starts_with,
        "endsWith" => ends_with,
        "slice" => slice,
        "substring" => substring,
        "substr" => substr,
        "split" => split,
        "trim" => trim,
        "trimStart" => trim_start,
        "trimEnd" => trim_end,
        "toUpperCase" => to_upper,
        "toLowerCase" => to_lower,
        "repeat" => repeat,
        "padStart" => pad_start,
        "padEnd" => pad_end,
        "replace" => replace,
        "replaceAll" => replace_all,
        "concat" => concat,
        "toString" | "valueOf" => to_string,
        _ => return None,
    })
}

extern "C" fn char_at(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let text = receiver(env);
    match character_index(arg(args, 0), text.len()).and_then(|index| text.code_unit_at(index)) {
        Some(unit) => tagged_string(js(&[unit])),
        None => tagged_string(JsString::from("")),
    }
}

extern "C" fn char_code_at(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let text = receiver(env);
    match character_index(arg(args, 0), text.len()).and_then(|index| text.code_unit_at(index)) {
        Some(unit) => f64::from(unit),
        None => f64::NAN,
    }
}

extern "C" fn code_point_at(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let text = receiver(env);
    match character_index(arg(args, 0), text.len()) {
        Some(index) => f64::from(text.code_point_at(index).as_u32()),
        None => tagged_undefined(),
    }
}

extern "C" fn at(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let text = receiver(env);
    let index = relative_index(arg(args, 0), text.len(), 0);
    if index >= text.len() {
        return tagged_undefined();
    }
    let mut buffer = [0u16; 2];
    let units = text.code_point_at(index).encode_utf16(&mut buffer);
    tagged_string(js(units))
}

extern "C" fn index_of(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let text = receiver(env);
    let search = to_js_string(arg(args, 0));
    let from = clamp_index(arg(args, 1), text.len(), 0);
    match text.index_of(search.as_str(), from) {
        Some(index) => index as f64,
        None => -1.0,
    }
}

extern "C" fn last_index_of(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let haystack = receiver(env).to_vec();
    let needle = to_js_string(arg(args, 0)).to_vec();
    let position = arg(args, 1);
    let start = if is_undefined(position) {
        haystack.len()
    } else {
        let number = to_number_value(position).trunc();
        if number.is_nan() {
            return -1.0;
        }
        if number <= 0.0 {
            0
        } else {
            (number as usize).min(haystack.len())
        }
    };
    if needle.is_empty() {
        return start as f64;
    }
    if needle.len() > haystack.len() {
        return -1.0;
    }
    let last = start.min(haystack.len() - needle.len());
    match (0..=last)
        .rev()
        .find(|&index| haystack[index..index + needle.len()] == needle[..])
    {
        Some(index) => index as f64,
        None => -1.0,
    }
}

extern "C" fn includes(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let text = receiver(env);
    let search = to_js_string(arg(args, 0));
    let from = clamp_index(arg(args, 1), text.len(), 0);
    tagged_bool(text.index_of(search.as_str(), from).is_some())
}

extern "C" fn starts_with(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let text = receiver(env);
    let search = to_js_string(arg(args, 0));
    let from = clamp_index(arg(args, 1), text.len(), 0);
    if from + search.len() > text.len() {
        return tagged_bool(false);
    }
    tagged_bool(text.slice(from, text.len()).starts_with(search.as_str()))
}

extern "C" fn ends_with(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let text = receiver(env);
    let search = to_js_string(arg(args, 0));
    let end = clamp_index(arg(args, 1), text.len(), text.len());
    if search.len() > end {
        return tagged_bool(false);
    }
    tagged_bool(
        text.slice(end - search.len(), end)
            .ends_with(search.as_str()),
    )
}

extern "C" fn slice(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let text = receiver(env);
    let length = text.len();
    let start = relative_index(arg(args, 0), length, 0);
    let end = relative_index(arg(args, 1), length, length);
    tagged_string(text.slice(start, end))
}

extern "C" fn substring(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let text = receiver(env);
    let length = text.len();
    let first = clamp_index(arg(args, 0), length, 0);
    let second = clamp_index(arg(args, 1), length, length);
    let (start, end) = if first <= second {
        (first, second)
    } else {
        (second, first)
    };
    tagged_string(text.slice(start, end))
}

extern "C" fn substr(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let text = receiver(env);
    let length = text.len();
    let start = relative_index(arg(args, 0), length, 0);
    let count = clamp_index(arg(args, 1), length - start, length - start);
    tagged_string(text.slice(start, start + count))
}

extern "C" fn split(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let haystack = receiver(env).to_vec();
    let separator = arg(args, 0);
    let limit = arg(args, 1);
    let limit = if is_undefined(limit) {
        usize::MAX
    } else {
        let number = to_number_value(limit);
        if number.is_nan() || number <= 0.0 {
            0
        } else {
            number.trunc() as usize
        }
    };
    let mut parts: Vec<f64> = Vec::new();
    if limit == 0 {
        return tagged_object(new_array(parts));
    }
    if is_undefined(separator) {
        parts.push(tagged_string(js(&haystack)));
    } else {
        let needle = to_js_string(separator).to_vec();
        if needle.is_empty() {
            for unit in &haystack {
                if parts.len() >= limit {
                    break;
                }
                parts.push(tagged_string(js(&[*unit])));
            }
        } else {
            let mut cursor = 0;
            while parts.len() < limit {
                let Some(offset) = find(&haystack, &needle, cursor) else {
                    break;
                };
                parts.push(tagged_string(js(&haystack[cursor..offset])));
                cursor = offset + needle.len();
            }
            if parts.len() < limit {
                parts.push(tagged_string(js(&haystack[cursor.min(haystack.len())..])));
            }
        }
    }
    tagged_object(new_array(parts))
}

extern "C" fn trim(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    tagged_string(receiver(env).trim())
}

extern "C" fn trim_start(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    tagged_string(receiver(env).trim_start())
}

extern "C" fn trim_end(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    tagged_string(receiver(env).trim_end())
}

extern "C" fn to_upper(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    tagged_string(JsString::from(
        receiver(env).to_std_string_lossy().to_uppercase(),
    ))
}

extern "C" fn to_lower(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    tagged_string(JsString::from(
        receiver(env).to_std_string_lossy().to_lowercase(),
    ))
}

extern "C" fn repeat(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let text = receiver(env);
    let count = to_number_value(arg(args, 0)).trunc();
    if !count.is_finite() || count <= 0.0 {
        return tagged_string(JsString::from(""));
    }
    let units = text.to_vec();
    let count = count as usize;
    let mut repeated = Vec::with_capacity(units.len().saturating_mul(count));
    for _ in 0..count {
        repeated.extend_from_slice(&units);
    }
    tagged_string(js(&repeated))
}

/// `padStart` and `padEnd` share this; `at_start` selects which side grows.
fn pad(env: i64, args: *const u8, at_start: bool) -> f64 {
    let text = receiver(env);
    let length = text.len();
    let target = to_number_value(arg(args, 0)).trunc();
    if !target.is_finite() || target <= length as f64 {
        return tagged_string(text);
    }
    let target = target as usize;
    let filler = arg(args, 1);
    let filler = if is_undefined(filler) {
        JsString::from(" ")
    } else {
        to_js_string(filler)
    };
    if filler.is_empty() {
        return tagged_string(text);
    }
    let filler = filler.to_vec();
    let mut padding = Vec::with_capacity(target - length);
    while padding.len() < target - length {
        let needed = (target - length - padding.len()).min(filler.len());
        padding.extend_from_slice(&filler[..needed]);
    }
    let mut padded = Vec::with_capacity(target);
    if at_start {
        padded.extend_from_slice(&padding);
        padded.extend_from_slice(&text.to_vec());
    } else {
        padded.extend_from_slice(&text.to_vec());
        padded.extend_from_slice(&padding);
    }
    tagged_string(js(&padded))
}

extern "C" fn pad_start(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    pad(env, args, true)
}

extern "C" fn pad_end(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    pad(env, args, false)
}

extern "C" fn concat(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let mut units = receiver(env).to_vec();
    for index in 0..arg_count(args) {
        units.extend_from_slice(&to_js_string(arg(args, index)).to_vec());
    }
    tagged_string(js(&units))
}

extern "C" fn replace(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    replace_impl(env, args, false)
}

extern "C" fn replace_all(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    replace_impl(env, args, true)
}

fn replace_impl(env: i64, args: *const u8, all: bool) -> f64 {
    let text = receiver(env);
    let haystack = text.to_vec();
    let needle = to_js_string(arg(args, 0)).to_vec();
    let replacement = arg(args, 1);
    let mut out: Vec<u16> = Vec::new();
    let mut cursor = 0;
    while cursor <= haystack.len() {
        let Some(offset) = find(&haystack, &needle, cursor) else {
            break;
        };
        out.extend_from_slice(&haystack[cursor..offset]);
        out.extend_from_slice(&replacement_units(
            replacement,
            &needle,
            offset,
            text.clone(),
        ));
        cursor = offset + needle.len();
        // An empty search would otherwise match every position forever.
        if !all || needle.is_empty() {
            break;
        }
    }
    out.extend_from_slice(&haystack[cursor.min(haystack.len())..]);
    tagged_string(js(&out))
}

/// The replacement text for one match: a called function's result, or the
/// literal replacement with `$$` and `$&` expanded.
fn replacement_units(replacement: f64, matched: &[u16], offset: usize, text: JsString) -> Vec<u16> {
    if replacement.to_bits() >> 48 == TAG_FUNCTION {
        let value = call(
            replacement,
            &[
                tagged_string(js(matched)),
                offset as f64,
                tagged_string(text),
            ],
        );
        return to_js_string(value).to_vec();
    }
    let source = to_js_string(replacement).to_vec();
    let mut units = Vec::with_capacity(source.len());
    let mut index = 0;
    while index < source.len() {
        if source[index] == u16::from(b'$') && index + 1 < source.len() {
            match source[index + 1] {
                unit if unit == u16::from(b'$') => {
                    units.push(u16::from(b'$'));
                    index += 2;
                    continue;
                }
                unit if unit == u16::from(b'&') => {
                    units.extend_from_slice(matched);
                    index += 2;
                    continue;
                }
                _ => {}
            }
        }
        units.push(source[index]);
        index += 1;
    }
    units
}

extern "C" fn to_string(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    tagged_string(receiver(env))
}

/// `ToUint16`: `ToNumber`, then modulo `2**16` as an unsigned value.
fn to_uint16(value: f64) -> u16 {
    let number = to_number_value(value).trunc();
    if !number.is_finite() {
        return 0;
    }
    number.rem_euclid(65_536.0) as u16
}

/// `ToCodePoint`: an integral value in `0..=0x10FFFF`, lone surrogates
/// included, or `None` when the value is out of range.
///
/// TODO(error-handling): the specification throws a `RangeError` for a
/// non-integral or out-of-range code point; the host ABI has no exception
/// channel yet, so the code point is skipped instead.
fn to_code_point(value: f64) -> Option<CodePoint> {
    let number = to_number_value(value);
    if !number.is_finite() || number.fract() != 0.0 || !(0.0..=0x10_FFFF as f64).contains(&number) {
        return None;
    }
    let number = number as u32;
    Some(match char::from_u32(number) {
        Some(character) => CodePoint::from(character),
        None => CodePoint::UnpairedSurrogate(number as u16),
    })
}

fn from_char_code_units(args: *const u8) -> JsString {
    let units: Vec<u16> = (0..arg_count(args))
        .map(|index| to_uint16(arg(args, index)))
        .collect();
    js(&units)
}

fn from_code_points(args: *const u8) -> JsString {
    let mut units = Vec::new();
    let mut buffer = [0u16; 2];
    for index in 0..arg_count(args) {
        if let Some(code_point) = to_code_point(arg(args, index)) {
            units.extend_from_slice(code_point.encode_utf16(&mut buffer));
        }
    }
    js(&units)
}

extern "C" fn sealion_string_from_char_codes(args: *const u8) -> u64 {
    tagged_string(from_char_code_units(args)).to_bits() & PAYLOAD_MASK
}

extern "C" fn sealion_string_from_code_points(args: *const u8) -> u64 {
    tagged_string(from_code_points(args)).to_bits() & PAYLOAD_MASK
}

extern "C" fn from_char_codes(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let _ = env;
    tagged_string(from_char_code_units(args))
}

extern "C" fn code_points(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let _ = env;
    tagged_string(from_code_points(args))
}

/// The `String` namespace object of the dynamic runtime: `fromCharCode` and
/// `fromCodePoint` as callable values. The engine has no `String.prototype`
/// object, so the object carries no `prototype` property.
extern "C" fn sealion_string_object() -> u64 {
    static OBJECT: OnceLock<usize> = OnceLock::new();
    *OBJECT.get_or_init(|| {
        let pointer = new_object();
        for (name, method) in [
            ("fromCharCode", from_char_codes as Method),
            ("fromCodePoint", code_points as Method),
        ] {
            define_property(pointer, name, native_function(method, 0));
        }
        pointer as usize
    }) as u64
}

pub(super) fn lookup(name: &str) -> Option<usize> {
    Some(match name {
        "sealion_string_from_utf8" => sealion_string_from_utf8 as *const () as usize,
        "sealion_string_from_char_codes" => sealion_string_from_char_codes as *const () as usize,
        "sealion_string_from_code_points" => sealion_string_from_code_points as *const () as usize,
        "sealion_string_object" => sealion_string_object as *const () as usize,
        "sealion_string_to_number" => sealion_string_to_number as *const () as usize,
        "sealion_string_to_string" => sealion_string_to_string as *const () as usize,

        "sealion_string_truthy" => sealion_string_truthy as *const () as usize,
        "sealion_string_add" => sealion_string_add as *const () as usize,
        "sealion_string_strict_equal" => sealion_string_strict_equal as *const () as usize,
        "sealion_string_loose_equal" => sealion_string_loose_equal as *const () as usize,
        "sealion_string_same_value" => sealion_string_same_value as *const () as usize,
        "sealion_string_compare" => sealion_string_compare as *const () as usize,
        "sealion_console_log" => sealion_console_log as *const () as usize,
        _ => return None,
    })
}
