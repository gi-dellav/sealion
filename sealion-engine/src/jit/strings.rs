//! Host helpers for JavaScript string values.
//!
//! Compiled code stores a string as a NaN-boxed pointer to a
//! [`sealion_string::JsString`]. These entry points are resolved by the JIT
//! loader through a fixed symbol table instead of `dlsym`, because the engine
//! is normally linked into a test or application binary whose symbols are not
//! exported.

use std::{cmp::Ordering, io::Write, mem::ManuallyDrop, ptr::NonNull, str};

use sealion_string::{JsString, RawJsString};

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

fn tag(value: f64) -> u64 {
    value.to_bits() >> 48
}

fn is_string(value: f64) -> bool {
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
        TAG_OBJECT => super::arrays::array_to_js_string(value)
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

pub(super) fn lookup(name: &str) -> Option<usize> {
    Some(match name {
        "sealion_string_from_utf8" => sealion_string_from_utf8 as *const () as usize,
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
