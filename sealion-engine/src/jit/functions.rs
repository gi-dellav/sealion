//! Function values of the dynamic runtime: the closure record, its properties
//! and the `Function.prototype` methods.
//!
//! A function is a NaN-boxed pointer to a four-word record
//! `[entry point, environment, properties, constructible]`. Compiled code reads
//! only the first two words, so a call has the same cost as before; the third
//! word is an object allocated on first property access and the fourth marks
//! whether `new` may use the value (arrows and bound functions may not).
//!
//! `call`, `apply` and `bind` are native closures over the receiver, built the
//! same way as the array and string methods, so `var c = f.call` works. `bind`
//! returns a new function record whose entry point forwards to the target with
//! the bound arguments prepended.

use sealion_string::JsString;

use super::arrays::{arg, arg_count, is_array};
use super::objects::with_object;
use super::objects::{
    JsObject, chain_get, define_hidden_property, define_property_named, new_object, object_pointer,
    tagged_object,
};
use super::strings::{tagged_undefined, to_js_string};
use crate::value::{PAYLOAD_MASK, TAG_FUNCTION};

type Method = extern "C" fn(i64, *const u8, f64, f64) -> f64;
type JsCall = unsafe extern "C" fn(i64, *const u8, f64, f64) -> f64;

/// Argument blocks hold at most this many values (`compiler::call::MAX_ARGS`).
const MAX_ARGS: usize = 32;

/// `true` when `value` is a function (user closure, native method or bound one).
pub(super) fn is_function(value: f64) -> bool {
    value.to_bits() >> 48 == TAG_FUNCTION
}

fn record(value: f64) -> Option<*mut u64> {
    is_function(value).then(|| (value.to_bits() & PAYLOAD_MASK) as *mut u64)
}

/// The entry point and captured environment of the function behind `value`.
pub(super) fn code_and_env(value: f64) -> Option<(usize, i64)> {
    let record = record(value)?;
    Some((unsafe { *record } as usize, unsafe { *record.add(1) }
        as i64))
}

/// Allocates a function record. `constructible` decides whether `new` accepts
/// the value and whether it owns a `prototype` property.
pub(super) fn new_record(code: usize, env: u64, constructible: bool) -> f64 {
    let record = Box::into_raw(Box::new([code as u64, env, 0, u64::from(constructible)]));
    f64::from_bits((TAG_FUNCTION << 48) | (record as u64 & PAYLOAD_MASK))
}

pub(super) fn is_constructible(value: f64) -> bool {
    record(value)
        .map(|record| unsafe { *record.add(3) } != 0)
        .unwrap_or(false)
}

/// The own-property object of a function, allocated on first use.
pub(super) fn function_object(value: f64) -> Option<*mut JsObject> {
    let record = record(value)?;
    let slot = unsafe { record.add(2) };
    let pointer = unsafe { *slot };
    if pointer != 0 {
        return Some(pointer as usize as *mut JsObject);
    }
    let object = new_object();
    unsafe {
        *slot = object as u64;
        // `owner` lets `Object.getPrototypeOf` map the property bag back to
        // this function value.
        (*object).owner = Some(value);
    }
    Some(object)
}

/// Reads `value[key]` from a function's own properties.
pub(super) fn property(value: f64, key: &JsString) -> Option<f64> {
    chain_get(tagged_object(function_object(value)?), key)
}

/// Defines `value[key]` on a function's own properties.
pub(super) fn set_property(value: f64, key: &JsString, property: f64) {
    if let Some(object) = function_object(value) {
        define_property_named(object, key, property);
    }
}

/// The lazily created `prototype` object of a constructible function, carrying
/// a `constructor` property that points back at the function.
pub(super) fn ensure_prototype(value: f64) -> f64 {
    let Some(object) = function_object(value) else {
        return tagged_undefined();
    };
    let prototype = new_object();
    define_hidden_property(object, "prototype", tagged_object(prototype));
    define_hidden_property(prototype, "constructor", value);
    tagged_object(prototype)
}

/// The `Function.prototype` method named by `key`, bound to `value`.
pub(super) fn method_property(value: f64, key: &JsString) -> Option<f64> {
    if !is_function(value) {
        return None;
    }
    let method = method(key)?;
    let record = Box::into_raw(Box::new([
        method as usize as u64,
        value.to_bits() & PAYLOAD_MASK,
        0,
        0,
    ]));
    Some(f64::from_bits(
        (TAG_FUNCTION << 48) | (record as u64 & PAYLOAD_MASK),
    ))
}

fn method(key: &JsString) -> Option<Method> {
    super::strings::with_ascii_key(key, method_named)?
}

fn method_named(name: &[u8]) -> Option<Method> {
    Some(match name {
        b"call" => call,
        b"apply" => apply,
        b"bind" => bind,
        b"toString" => to_string,
        _ => return None,
    })
}

fn receiver(env: i64) -> f64 {
    f64::from_bits((TAG_FUNCTION << 48) | (env as u64 & PAYLOAD_MASK))
}

/// Calls `target` with an explicit `this` and argument list.
fn invoke(target: f64, this: f64, values: &[f64]) -> f64 {
    let Some((code, env)) = code_and_env(target) else {
        return tagged_undefined();
    };
    let mut block = [0u64; 1 + MAX_ARGS];
    let count = values.len().min(MAX_ARGS);
    block[0] = count as u64;
    for (index, value) in values.iter().take(count).enumerate() {
        block[index + 1] = value.to_bits();
    }
    let function: JsCall = unsafe { std::mem::transmute(code) };
    unsafe { function(env, block.as_ptr().cast::<u8>(), target, this) }
}

extern "C" fn call(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let target = receiver(env);
    let this = arg(args, 0);
    let values: Vec<f64> = (1..arg_count(args)).map(|index| arg(args, index)).collect();
    invoke(target, this, &values)
}

/// The elements of an array-like argument, or nothing when it has no length.
fn list_items(value: f64) -> Vec<f64> {
    if is_array(value) {
        return with_object(value, |object| object.elements.clone())
            .flatten()
            .unwrap_or_default();
    }
    let length = super::strings::to_number_value(super::objects::get_property(value, "length"));
    if !(length.is_finite() && length > 0.0) {
        return Vec::new();
    }
    let length = (length as usize).min(MAX_ARGS);
    (0..length)
        .map(|index| super::objects::get_property(value, &index.to_string()))
        .collect()
}

extern "C" fn apply(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let target = receiver(env);
    let this = arg(args, 0);
    let values = list_items(arg(args, 1));
    invoke(target, this, &values)
}

struct Bound {
    target: f64,
    this: f64,
    args: Vec<f64>,
}

extern "C" fn bound_entry(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let bound = unsafe { &*(env as usize as *const Bound) };
    let mut values = bound.args.clone();
    values.extend((0..arg_count(args)).map(|index| arg(args, index)));
    invoke(bound.target, bound.this, &values)
}

extern "C" fn bind(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let target = receiver(env);
    let bound = Box::into_raw(Box::new(Bound {
        target,
        this: arg(args, 0),
        args: (1..arg_count(args)).map(|index| arg(args, index)).collect(),
    }));
    new_record(bound_entry as *const () as usize, bound as u64, false)
}

extern "C" fn to_string(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    let target = receiver(env);
    super::strings::tagged_string(to_js_string(target))
}

/// `value instanceof constructor`: `true` when the prototype chain of `value`
/// contains `constructor.prototype`.
pub(super) fn has_instance(value: f64, constructor: f64) -> bool {
    let Some(mut current) = object_pointer(value) else {
        return false;
    };
    let prototype = super::objects::get_property(constructor, "prototype");
    let Some(target) = object_pointer(prototype) else {
        return false;
    };
    let mut depth = 0;
    loop {
        let Some(pointer) =
            with_object(tagged_object(current), |object| object.prototype).flatten()
        else {
            return false;
        };
        if pointer == target {
            return true;
        }
        if depth > 1000 {
            return false;
        }
        current = pointer;
        depth += 1;
    }
}
