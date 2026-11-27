//! Host helpers for JavaScript array values.
//!
//! An array is a [`JsObject`] whose `elements` vector holds the dense part of
//! the index range; `length`, indices and the methods are answered here, while
//! named properties fall through to the ordinary own-property table in
//! `jit::objects`. Everything leaks for the lifetime of the process, like every
//! other heap value in the runtime.
//!
//! Methods are exposed as native closures: a function record whose first word
//! is the address of one of the entry points below and whose second word is the
//! array pointer. They therefore use the ordinary JavaScript calling convention
//! (`env`, argument block, callee, this) and work through the same indirect call path
//! as a user function, including `var push = a.push`.

use std::cmp::Ordering;

use sealion_string::JsString;

use super::objects::{JsObject, get_property, new_array, tagged_object, with_object};
use super::strings::{
    is_tagged, same_value_zero, tagged_bool, tagged_string, tagged_undefined, to_js_string,
    to_number_value, value_truthy, values_strict_equal, with_string,
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
///
/// Parsed from the string's own bytes, so an index lookup does not allocate.
pub(super) fn index_of_key(key: &JsString) -> Option<usize> {
    super::strings::with_ascii_key(key, |bytes| {
        if bytes.is_empty() || bytes.len() > 10 || (bytes.len() > 1 && bytes[0] == b'0') {
            return None;
        }
        let mut index = 0usize;
        for &byte in bytes {
            if !byte.is_ascii_digit() {
                return None;
            }
            index = index * 10 + usize::from(byte - b'0');
        }
        (index < u32::MAX as usize).then_some(index)
    })?
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
    Some(super::objects::native_function(
        method,
        value.to_bits() & PAYLOAD_MASK,
    ))
}

fn method(key: &JsString) -> Option<Method> {
    super::strings::with_ascii_key(key, method_named)?
}

fn method_named(name: &[u8]) -> Option<Method> {
    Some(match name {
        b"push" => push,
        b"pop" => pop,
        b"shift" => shift,
        b"unshift" => unshift,
        b"join" => join,
        b"toString" => to_string,
        b"slice" => slice,
        b"concat" => concat,
        b"indexOf" => index_of,
        b"includes" => includes,
        b"reverse" => reverse,
        b"forEach" => for_each,
        b"map" => map,
        b"filter" => filter,
        b"some" => some,
        b"every" => every,
        b"find" => find,
        b"findIndex" => find_index,
        b"findLast" => find_last,
        b"findLastIndex" => find_last_index,
        b"reduce" => reduce,
        b"reduceRight" => reduce_right,
        b"at" => at,
        b"lastIndexOf" => last_index_of,
        b"copyWithin" => copy_within,
        b"fill" => fill,
        b"flat" => flat,
        b"flatMap" => flat_map,
        b"keys" => keys,
        b"values" => values,
        b"entries" => entries,
        b"sort" => sort,
        b"toReversed" => to_reversed,
        b"toSorted" => to_sorted,
        b"splice" => splice,
        b"toSpliced" => to_spliced,
        b"with" => with_value,
        b"toLocaleString" => to_string,
        b"valueOf" => value_of,
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

/// Like [`relative`], but keeps the out-of-range answer: `at` and `with` need
/// `undefined`/a no-op for an index past either end instead of a clamp.
fn signed_index(value: f64, length: usize) -> Option<usize> {
    let number = to_number_value(value).trunc();
    let index = if number.is_nan() {
        0.0
    } else if number < 0.0 {
        length as f64 + number
    } else {
        number
    };
    (index >= 0.0 && index < length as f64).then_some(index as usize)
}

/// `deleteCount` for `splice` and `toSpliced`: a lone `start` removes the tail,
/// an explicit count is clamped to what is left.
fn delete_count(args: *const u8, start: usize, length: usize) -> usize {
    match arg_count(args) {
        0 => 0,
        1 => length - start,
        _ => {
            let number = to_number_value(arg(args, 1)).trunc();
            if number.is_nan() || number <= 0.0 {
                0
            } else {
                (number as usize).min(length - start)
            }
        }
    }
}

/// Replaces `remove` elements at `start` with `items` and returns the removed
/// ones, the shared core of `splice` and `toSpliced`.
fn splice_into(elements: &mut Vec<f64>, start: usize, remove: usize, items: Vec<f64>) -> Vec<f64> {
    let end = (start + remove).min(elements.len());
    elements.splice(start..end, items).collect()
}

/// The arguments from `splice`/`toSpliced` that are inserted.
fn inserted(args: *const u8) -> Vec<f64> {
    (2..arg_count(args)).map(|index| arg(args, index)).collect()
}

/// Copies the array's elements out so a callback may mutate it while we sort.
fn drain(env: i64) -> Vec<f64> {
    with_array(env, |elements| elements.clone()).unwrap_or_default()
}

/// `sort`'s comparison: the callback's sign, else the `ToString` order in
/// UTF-16 code units, which is what comparing two strings does.
fn compare(comparator: f64, lhs: f64, rhs: f64) -> Ordering {
    if comparator.to_bits() >> 48 != TAG_FUNCTION {
        return to_js_string(lhs).cmp(&to_js_string(rhs));
    }
    let result = to_number_value(call(comparator, &[lhs, rhs]));
    if result < 0.0 {
        Ordering::Less
    } else if result > 0.0 {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

/// `flat`'s depth: `undefined` is one level, `Infinity` flattens everything.
fn flatten_depth(value: f64) -> usize {
    if value.to_bits() >> 48 == TAG_UNDEFINED {
        return 1;
    }
    let number = to_number_value(value).trunc();
    if number.is_nan() || number <= 0.0 {
        0
    } else if number.is_infinite() || number >= usize::MAX as f64 {
        usize::MAX
    } else {
        number as usize
    }
}

/// Flattens `source` up to `depth` levels into `target`.
fn flatten_into(target: &mut Vec<f64>, source: &[f64], depth: usize) {
    for &value in source {
        match with_object(value, |object| object.elements.clone()).flatten() {
            Some(inner) if depth > 0 => flatten_into(target, &inner, depth - 1),
            _ => target.push(value),
        }
    }
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

extern "C" fn find_last(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let callback = arg(args, 0);
    let this = array_value(env);
    for index in (0..length(env)).rev() {
        let value = element(env, index);
        if value_truthy(call(callback, &[value, index as f64, this])) {
            return value;
        }
    }
    tagged_undefined()
}

extern "C" fn find_last_index(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let callback = arg(args, 0);
    let this = array_value(env);
    for index in (0..length(env)).rev() {
        if value_truthy(call(callback, &[element(env, index), index as f64, this])) {
            return index as f64;
        }
    }
    -1.0
}

extern "C" fn reduce_right(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let callback = arg(args, 0);
    let this = array_value(env);
    let length = length(env);
    let mut index = length as isize - 1;
    let mut accumulator = if arg_count(args) > 1 {
        arg(args, 1)
    } else if length > 0 {
        index -= 1;
        element(env, length - 1)
    } else {
        return tagged_undefined();
    };
    while index >= 0 {
        accumulator = call(
            callback,
            &[
                accumulator,
                element(env, index as usize),
                index as f64,
                this,
            ],
        );
        index -= 1;
    }
    accumulator
}

extern "C" fn at(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    match signed_index(arg(args, 0), length(env)) {
        Some(index) => element(env, index),
        None => tagged_undefined(),
    }
}

extern "C" fn last_index_of(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let search = arg(args, 0);
    let length = length(env);
    if length == 0 {
        return -1.0;
    }
    let position = arg(args, 1);
    let start = if position.to_bits() >> 48 == TAG_UNDEFINED {
        length - 1
    } else {
        let number = to_number_value(position).trunc();
        if number.is_nan() {
            return -1.0;
        }
        if number < 0.0 {
            match length.checked_sub((-number) as usize) {
                Some(shifted) => shifted,
                None => return -1.0,
            }
        } else {
            (number as usize).min(length - 1)
        }
    };
    for index in (0..=start).rev() {
        if values_strict_equal(element(env, index), search) {
            return index as f64;
        }
    }
    -1.0
}

extern "C" fn copy_within(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    with_array(env, |elements| {
        let length = elements.len();
        let target = relative(args, 0, length, 0);
        let start = relative(args, 1, length, 0);
        let end = relative(args, 2, length, length);
        let count = end.saturating_sub(start).min(length - target);
        if count > 0 {
            elements.copy_within(start..start + count, target);
        }
    });
    array_value(env)
}

extern "C" fn fill(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let value = arg(args, 0);
    with_array(env, |elements| {
        let length = elements.len();
        let start = relative(args, 1, length, 0);
        let end = relative(args, 2, length, length).max(start);
        elements[start..end].fill(value);
    });
    array_value(env)
}

extern "C" fn flat(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let depth = flatten_depth(arg(args, 0));
    let source = drain(env);
    let mut flattened = Vec::with_capacity(source.len());
    flatten_into(&mut flattened, &source, depth);
    tagged_object(new_array(flattened))
}

extern "C" fn flat_map(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let callback = arg(args, 0);
    let this = array_value(env);
    let mut mapped = Vec::with_capacity(length(env));
    for index in 0..length(env) {
        let value = call(callback, &[element(env, index), index as f64, this]);
        match with_object(value, |object| object.elements.clone()).flatten() {
            Some(inner) => mapped.extend(inner),
            None => mapped.push(value),
        }
    }
    tagged_object(new_array(mapped))
}

/// `array.keys()`: the indices as an array, since there is no iterator protocol.
extern "C" fn keys(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    let indices = (0..length(env)).map(|index| index as f64).collect();
    tagged_object(new_array(indices))
}

/// `array.values()`: a shallow copy of the elements.
extern "C" fn values(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    tagged_object(new_array(drain(env)))
}

/// `array.entries()`: `[index, value]` pairs, as an array of two-element arrays.
extern "C" fn entries(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    let pairs = with_array(env, |elements| {
        elements
            .iter()
            .enumerate()
            .map(|(index, &value)| tagged_object(new_array(vec![index as f64, value])))
            .collect()
    })
    .unwrap_or_default();
    tagged_object(new_array(pairs))
}

extern "C" fn sort(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let comparator = arg(args, 0);
    let mut elements = drain(env);
    elements.sort_by(|&lhs, &rhs| compare(comparator, lhs, rhs));
    with_array(env, |slot| *slot = elements);
    array_value(env)
}

extern "C" fn to_reversed(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    let mut elements = drain(env);
    elements.reverse();
    tagged_object(new_array(elements))
}

extern "C" fn to_sorted(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let comparator = arg(args, 0);
    let mut elements = drain(env);
    elements.sort_by(|&lhs, &rhs| compare(comparator, lhs, rhs));
    tagged_object(new_array(elements))
}

extern "C" fn splice(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let start = relative(args, 0, length(env), 0);
    let remove = delete_count(args, start, length(env));
    let items = inserted(args);
    let removed =
        with_array(env, |elements| splice_into(elements, start, remove, items)).unwrap_or_default();
    tagged_object(new_array(removed))
}

extern "C" fn to_spliced(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let mut elements = drain(env);
    let start = relative(args, 0, elements.len(), 0);
    let remove = delete_count(args, start, elements.len());
    let items = inserted(args);
    splice_into(&mut elements, start, remove, items);
    tagged_object(new_array(elements))
}

extern "C" fn with_value(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let mut elements = drain(env);
    if let Some(index) = signed_index(arg(args, 0), elements.len()) {
        elements[index] = arg(args, 1);
    }
    tagged_object(new_array(elements))
}

extern "C" fn value_of(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    array_value(env)
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

/// `Array.of(...)`: unlike [`sealion_array_construct`] a lone number is an
/// element, not a length.
extern "C" fn sealion_array_of(values: *const u8) -> u64 {
    let words = values.cast::<u64>();
    let count = unsafe { *words } as usize;
    let elements = (0..count)
        .map(|index| f64::from_bits(unsafe { *words.add(index + 1) }))
        .collect();
    new_array(elements) as u64
}

/// `Array.from(arrayLike)`: copies an array, splits a string into code points,
/// and indexes anything else by its `length` property.
extern "C" fn sealion_array_from(value: f64) -> f64 {
    if let Some(elements) = with_object(value, |object| object.elements.clone()).flatten() {
        return tagged_object(new_array(elements));
    }
    let characters = with_string(value, |text| {
        text.code_points()
            .map(|point| {
                let mut buffer = [0u16; 2];
                let units: &[u16] = point.encode_utf16(&mut buffer);
                tagged_string(JsString::from(units))
            })
            .collect::<Vec<f64>>()
    });
    if let Some(characters) = characters {
        return tagged_object(new_array(characters));
    }
    let length = to_number_value(get_property(value, "length"));
    let length = if length.is_finite() && length > 0.0 {
        length as usize
    } else {
        0
    };
    let elements = (0..length)
        .map(|index| get_property(value, &index.to_string()))
        .collect();
    tagged_object(new_array(elements))
}

pub(super) fn lookup(name: &str) -> Option<usize> {
    Some(match name {
        "sealion_array_new" => sealion_array_new as *const () as usize,
        "sealion_array_construct" => sealion_array_construct as *const () as usize,
        "sealion_array_is_array" => sealion_array_is_array as *const () as usize,
        "sealion_array_of" => sealion_array_of as *const () as usize,
        "sealion_array_from" => sealion_array_from as *const () as usize,
        _ => return None,
    })
}
