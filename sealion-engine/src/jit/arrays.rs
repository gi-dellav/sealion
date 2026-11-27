//! Host helpers for JavaScript array values.
//!
//! An array is a [`JsObject`] whose `elements` vector holds the dense part of
//! the index range; `length`, indices and the methods are answered here, while
//! named properties fall through to the ordinary own-property table in
//! `jit::objects`. Everything leaks for the lifetime of the process, like every
//! other heap value in the runtime.
//!
//! Methods are exposed as native closures: a two-word record whose first word
//! is the address of one of the entry points below and whose second word is the
//! array pointer. They therefore use the ordinary JavaScript calling convention
//! (`env`, argument block, callee, this) and work through the same indirect call path
//! as a user function, including `var push = a.push`.

use sealion_string::JsString;

use super::objects::{JsObject, new_array, tagged_object, with_object};
use super::strings::{
    is_tagged, same_value_zero, tagged_bool, tagged_string, tagged_undefined, to_js_string,
    to_number_value, value_truthy, values_strict_equal,
};
use crate::value::{PAYLOAD_MASK, TAG_FUNCTION, TAG_UNDEFINED};

type Method = extern "C" fn(i64, *const u8, f64, f64) -> f64;
type JsCall = unsafe extern "C" fn(i64, *const u8, f64, f64) -> f64;

/// Argument blocks hold at most this many values (`compiler::call::MAX_ARGS`).
const MAX_ARGS: usize = 32;

pub(super) fn arg_count(args: *const u8) -> usize {
    (unsafe { *args.cast::<u64>() } as usize).min(MAX_ARGS)
}

pub(super) fn arg(args: *const u8, index: usize) -> f64 {
    let words = args.cast::<u64>();
    if index < arg_count(args) {
        f64::from_bits(unsafe { *words.add(index + 1) })
    } else {
        tagged_undefined()
    }
}

pub(super) fn is_array(value: f64) -> bool {
    with_object(value, |object| object.elements.is_some()).unwrap_or(false)
}

fn with_array<R>(env: i64, f: impl FnOnce(&mut Vec<f64>) -> R) -> Option<R> {
    let object = unsafe { &mut *(env as usize as *mut JsObject) };
    object.elements.as_mut().map(f)
}

fn array_value(env: i64) -> f64 {
    tagged_object(env as usize as *mut JsObject)
}

fn length(env: i64) -> usize {
    with_array(env, |elements| elements.len()).unwrap_or(0)
}

fn element(env: i64, index: usize) -> f64 {
    with_array(env, |elements| elements.get(index).copied())
        .flatten()
        .unwrap_or_else(tagged_undefined)
}

/// A canonical array index key: `"0"` yes, `"01"` and `"-1"` no.
fn index_of_key(key: &JsString) -> Option<usize> {
    let text = key.to_std_string_lossy();
    let bytes = text.as_bytes();
    if bytes.is_empty() || bytes.len() > 10 || (bytes.len() > 1 && bytes[0] == b'0') {
        return None;
    }
    if !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let index: usize = text.parse().ok()?;
    (index < u32::MAX as usize).then_some(index)
}

/// `array.length` and `array[index]`, or `None` when `value` is not an array or
/// the key names neither.
pub(super) fn element_property(value: f64, key: &JsString) -> Option<f64> {
    with_object(value, |object| {
        let elements = object.elements.as_ref()?;
        if key == "length" {
            return Some(elements.len() as f64);
        }
        let index = index_of_key(key)?;
        Some(
            elements
                .get(index)
                .copied()
                .unwrap_or_else(tagged_undefined),
        )
    })
    .flatten()
}

/// Writes `array.length` or `array[index]`; returns `false` for other keys.
pub(super) fn set_element(value: f64, key: &JsString, element: f64) -> bool {
    if key == "length" {
        return with_object(value, |object| {
            let Some(elements) = object.elements.as_mut() else {
                return false;
            };
            if !element.is_finite() || element < 0.0 {
                return false;
            }
            elements.resize(element as usize, tagged_undefined());
            true
        })
        .unwrap_or(false);
    }
    let Some(index) = index_of_key(key) else {
        return false;
    };
    with_object(value, |object| {
        let Some(elements) = object.elements.as_mut() else {
            return false;
        };
        if index >= elements.len() {
            elements.resize(index + 1, tagged_undefined());
        }
        elements[index] = element;
        true
    })
    .unwrap_or(false)
}

/// `key in array`, including `length` and the inherited-looking methods.
pub(super) fn has_element(value: f64, key: &JsString) -> bool {
    with_object(value, |object| {
        let Some(elements) = object.elements.as_ref() else {
            return false;
        };
        if key == "length" {
            return true;
        }
        if let Some(index) = index_of_key(key) {
            return index < elements.len();
        }
        method(key).is_some()
    })
    .unwrap_or(false)
}

/// `delete array[index]` leaves a hole-like `undefined` and keeps the length.
pub(super) fn delete_element(value: f64, key: &JsString) -> bool {
    let Some(index) = index_of_key(key) else {
        return false;
    };
    with_object(value, |object| {
        let Some(elements) = object.elements.as_mut() else {
            return false;
        };
        if index < elements.len() {
            elements[index] = tagged_undefined();
        }
        true
    })
    .unwrap_or(false)
}

/// The native closure for `array.method`, or `None` when the key names no method.
pub(super) fn method_property(value: f64, key: &JsString) -> Option<f64> {
    if !is_array(value) {
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
        "push" => push,
        "pop" => pop,
        "shift" => shift,
        "unshift" => unshift,
        "join" => join,
        "toString" => to_string,
        "slice" => slice,
        "concat" => concat,
        "indexOf" => index_of,
        "includes" => includes,
        "reverse" => reverse,
        "forEach" => for_each,
        "map" => map,
        "filter" => filter,
        "some" => some,
        "every" => every,
        "find" => find,
        "findIndex" => find_index,
        "reduce" => reduce,
        _ => return None,
    })
}

/// `array.join()` and `String(array)`.
pub(super) fn array_to_js_string(value: f64) -> Option<JsString> {
    with_object(value, |object| {
        let elements = object.elements.as_ref()?;
        Some(join_elements(elements, &JsString::from(",")))
    })
    .flatten()
}

fn join_elements(elements: &[f64], separator: &JsString) -> JsString {
    let mut result = JsString::from("");
    for (index, &element) in elements.iter().enumerate() {
        if index != 0 {
            result = JsString::concat(result.as_str(), separator.as_str());
        }
        if !super::strings::is_nullish(element) {
            let text = to_js_string(element);
            result = JsString::concat(result.as_str(), text.as_str());
        }
    }
    result
}

/// Calls a JavaScript closure with at most four arguments.
pub(super) fn call(callback: f64, arguments: &[f64]) -> f64 {
    if callback.to_bits() >> 48 != TAG_FUNCTION {
        return tagged_undefined();
    }
    let record = (callback.to_bits() & PAYLOAD_MASK) as *const u64;
    let (code, env) = unsafe { (*record, *record.add(1)) };
    let mut block = [0u64; 5];
    block[0] = arguments.len() as u64;
    for (slot, argument) in block[1..].iter_mut().zip(arguments) {
        *slot = argument.to_bits();
    }
    let function: JsCall = unsafe { std::mem::transmute(code as usize) };
    unsafe {
        function(
            env as i64,
            block.as_ptr().cast(),
            callback,
            tagged_undefined(),
        )
    }
}

/// Resolves a relative start/end argument the way `slice` and `indexOf` do.
fn relative(args: *const u8, index: usize, length: usize, default: usize) -> usize {
    let value = arg(args, index);
    if value.to_bits() >> 48 == TAG_UNDEFINED {
        return default;
    }
    let number = to_number_value(value).trunc();
    if number < 0.0 {
        return length.saturating_sub((-number) as usize);
    }
    (number as usize).min(length)
}

extern "C" fn push(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let count = arg_count(args);
    let length = with_array(env, |elements| {
        for index in 0..count {
            elements.push(arg(args, index));
        }
        elements.len()
    })
    .unwrap_or(0);
    length as f64
}

extern "C" fn pop(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    with_array(env, Vec::pop)
        .flatten()
        .unwrap_or_else(tagged_undefined)
}

extern "C" fn shift(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    with_array(env, |elements| {
        if elements.is_empty() {
            None
        } else {
            Some(elements.remove(0))
        }
    })
    .flatten()
    .unwrap_or_else(tagged_undefined)
}

extern "C" fn unshift(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let count = arg_count(args);
    let length = with_array(env, |elements| {
        for index in (0..count).rev() {
            elements.insert(0, arg(args, index));
        }
        elements.len()
    })
    .unwrap_or(0);
    length as f64
}

extern "C" fn join(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let separator = arg(args, 0);
    let separator = if separator.to_bits() >> 48 == TAG_UNDEFINED {
        JsString::from(",")
    } else {
        to_js_string(separator)
    };
    let text = with_array(env, |elements| join_elements(elements, &separator))
        .unwrap_or_else(|| JsString::from(""));
    tagged_string(text)
}

extern "C" fn to_string(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    let text = with_array(env, |elements| {
        join_elements(elements, &JsString::from(","))
    })
    .unwrap_or_else(|| JsString::from(""));
    tagged_string(text)
}

extern "C" fn slice(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let length = length(env);
    let start = relative(args, 0, length, 0);
    let end = relative(args, 1, length, length);
    let sliced = with_array(env, |elements| {
        let start = start.min(elements.len());
        let end = end.min(elements.len()).max(start);
        elements[start..end].to_vec()
    })
    .unwrap_or_default();
    tagged_object(new_array(sliced))
}

extern "C" fn concat(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let mut elements = with_array(env, |elements| elements.clone()).unwrap_or_default();
    for index in 0..arg_count(args) {
        let value = arg(args, index);
        match with_object(value, |object| object.elements.clone()).flatten() {
            Some(other) => elements.extend(other),
            None => elements.push(value),
        }
    }
    tagged_object(new_array(elements))
}

extern "C" fn index_of(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let search = arg(args, 0);
    let length = length(env);
    for index in relative(args, 1, length, 0)..length {
        if values_strict_equal(element(env, index), search) {
            return index as f64;
        }
    }
    -1.0
}

extern "C" fn includes(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let search = arg(args, 0);
    let length = length(env);
    for index in relative(args, 1, length, 0)..length {
        if same_value_zero(element(env, index), search) {
            return tagged_bool(true);
        }
    }
    tagged_bool(false)
}

extern "C" fn reverse(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    with_array(env, |elements| elements.reverse());
    array_value(env)
}

extern "C" fn for_each(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let callback = arg(args, 0);
    let this = array_value(env);
    for index in 0..length(env) {
        call(callback, &[element(env, index), index as f64, this]);
    }
    tagged_undefined()
}

extern "C" fn map(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let callback = arg(args, 0);
    let this = array_value(env);
    let mut mapped = Vec::with_capacity(length(env));
    for index in 0..length(env) {
        mapped.push(call(callback, &[element(env, index), index as f64, this]));
    }
    tagged_object(new_array(mapped))
}

extern "C" fn filter(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let callback = arg(args, 0);
    let this = array_value(env);
    let mut kept = Vec::new();
    for index in 0..length(env) {
        let value = element(env, index);
        if value_truthy(call(callback, &[value, index as f64, this])) {
            kept.push(value);
        }
    }
    tagged_object(new_array(kept))
}

extern "C" fn some(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let callback = arg(args, 0);
    let this = array_value(env);
    for index in 0..length(env) {
        if value_truthy(call(callback, &[element(env, index), index as f64, this])) {
            return tagged_bool(true);
        }
    }
    tagged_bool(false)
}

extern "C" fn every(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let callback = arg(args, 0);
    let this = array_value(env);
    for index in 0..length(env) {
        if !value_truthy(call(callback, &[element(env, index), index as f64, this])) {
            return tagged_bool(false);
        }
    }
    tagged_bool(true)
}

extern "C" fn find(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let callback = arg(args, 0);
    let this = array_value(env);
    for index in 0..length(env) {
        let value = element(env, index);
        if value_truthy(call(callback, &[value, index as f64, this])) {
            return value;
        }
    }
    tagged_undefined()
}

extern "C" fn find_index(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let callback = arg(args, 0);
    let this = array_value(env);
    for index in 0..length(env) {
        if value_truthy(call(callback, &[element(env, index), index as f64, this])) {
            return index as f64;
        }
    }
    -1.0
}

extern "C" fn reduce(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let callback = arg(args, 0);
    let this = array_value(env);
    let length = length(env);
    let mut index = 0;
    let mut accumulator = if arg_count(args) > 1 {
        arg(args, 1)
    } else if length > 0 {
        index = 1;
        element(env, 0)
    } else {
        return tagged_undefined();
    };
    while index < length {
        accumulator = call(
            callback,
            &[accumulator, element(env, index), index as f64, this],
        );
        index += 1;
    }
    accumulator
}

extern "C" fn sealion_array_new(length: i64) -> u64 {
    let length = usize::try_from(length).unwrap_or(0);
    new_array(vec![tagged_undefined(); length]) as u64
}

/// `Array(...)` and `new Array(...)`: a single numeric argument is a length,
/// anything else is an element list.
extern "C" fn sealion_array_construct(values: *const u8) -> u64 {
    let words = values.cast::<u64>();
    let count = unsafe { *words } as usize;
    let array = if count == 0 {
        new_array(Vec::new())
    } else if count == 1 {
        let single = f64::from_bits(unsafe { *words.add(1) });
        if is_tagged(single) {
            new_array(vec![single])
        } else {
            let length = if single.is_finite() && single >= 0.0 {
                single as usize
            } else {
                0
            };
            new_array(vec![tagged_undefined(); length])
        }
    } else {
        new_array(
            (0..count)
                .map(|index| f64::from_bits(unsafe { *words.add(index + 1) }))
                .collect(),
        )
    };
    array as u64
}

extern "C" fn sealion_array_is_array(value: f64) -> f64 {
    tagged_bool(is_array(value))
}

pub(super) fn lookup(name: &str) -> Option<usize> {
    Some(match name {
        "sealion_array_new" => sealion_array_new as *const () as usize,
        "sealion_array_construct" => sealion_array_construct as *const () as usize,
        "sealion_array_is_array" => sealion_array_is_array as *const () as usize,
        _ => return None,
    })
}
