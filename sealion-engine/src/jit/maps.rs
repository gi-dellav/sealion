//! Host helpers for JavaScript `Map` values.
//!
//! A map is a [`JsObject`] whose `entries` vector holds the insertion-ordered
//! key/value pairs; `size` and the methods are answered here, while named
//! properties fall through to the ordinary own-property table in `jit::objects`.
//! Keys are compared with SameValueZero, so `NaN` finds itself and objects are
//! compared by identity. There is no garbage collector: a map, its keys and its
//! values leak for the lifetime of the process.
//!
//! Methods are exposed as native closures exactly like the array ones: a
//! function record holding the entry point and the map pointer, so they use the
//! ordinary JavaScript calling convention and work through the same indirect
//! call path as a user function.

use sealion_string::JsString;

use super::arrays::{arg, call};
use super::objects::{JsObject, new_array, new_map, tagged_object, with_object};
use super::strings::{same_value_zero, tagged_bool, tagged_undefined};
use crate::value::PAYLOAD_MASK;

type Method = extern "C" fn(i64, *const u8, f64, f64) -> f64;

pub(super) fn is_map(value: f64) -> bool {
    with_object(value, |object| object.entries.is_some()).unwrap_or(false)
}

fn with_map<R>(env: i64, f: impl FnOnce(&mut Vec<(f64, f64)>) -> R) -> Option<R> {
    let object = unsafe { &mut *(env as usize as *mut JsObject) };
    object.entries.as_mut().map(f)
}

fn map_value(env: i64) -> f64 {
    tagged_object(env as usize as *mut JsObject)
}

fn entry_index(entries: &[(f64, f64)], key: f64) -> Option<usize> {
    entries
        .iter()
        .position(|(name, _)| same_value_zero(*name, key))
}

/// The native closure for `map.method`, or `None` when the key names no method.
pub(super) fn method_property(value: f64, key: &JsString) -> Option<f64> {
    if !is_map(value) {
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
        b"get" => get,
        b"set" => set,
        b"has" => has,
        b"delete" => delete,
        b"clear" => clear,
        b"forEach" => for_each,
        b"keys" => keys,
        b"values" => values,
        b"entries" => entries,
        _ => return None,
    })
}

extern "C" fn get(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let key = arg(args, 0);
    with_map(env, |entries| {
        entry_index(entries, key).map(|index| entries[index].1)
    })
    .flatten()
    .unwrap_or_else(tagged_undefined)
}

extern "C" fn set(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let key = arg(args, 0);
    let value = arg(args, 1);
    with_map(env, |entries| match entry_index(entries, key) {
        Some(index) => entries[index].1 = value,
        None => entries.push((key, value)),
    });
    // `map.set(key, value)` returns the map itself.
    map_value(env)
}

extern "C" fn has(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let key = arg(args, 0);
    let found = with_map(env, |entries| entry_index(entries, key).is_some()).unwrap_or(false);
    tagged_bool(found)
}

extern "C" fn delete(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let key = arg(args, 0);
    let removed = with_map(env, |entries| match entry_index(entries, key) {
        Some(index) => {
            entries.remove(index);
            true
        }
        None => false,
    })
    .unwrap_or(false);
    tagged_bool(removed)
}

extern "C" fn clear(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    with_map(env, Vec::clear);
    tagged_undefined()
}

/// Calls `callback(value, key, map)` for every entry in insertion order. The
/// entries are copied first so that the callback may mutate the map safely.
extern "C" fn for_each(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let callback = arg(args, 0);
    let this = map_value(env);
    let entries = with_map(env, |entries| entries.clone()).unwrap_or_default();
    for (key, value) in entries {
        call(callback, &[value, key, this]);
    }
    tagged_undefined()
}

/// `map.keys()`: the keys as an array in insertion order, since the runtime has
/// no iterator protocol.
extern "C" fn keys(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    let keys =
        with_map(env, |entries| entries.iter().map(|&(key, _)| key).collect()).unwrap_or_default();
    tagged_object(new_array(keys))
}

/// `map.values()`: the values as an array in insertion order.
extern "C" fn values(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    let values = with_map(env, |entries| {
        entries.iter().map(|&(_, value)| value).collect()
    })
    .unwrap_or_default();
    tagged_object(new_array(values))
}

/// `map.entries()`: `[key, value]` pairs, as an array of two-element arrays.
extern "C" fn entries(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    let pairs = with_map(env, |entries| {
        entries
            .iter()
            .map(|&(key, value)| tagged_object(new_array(vec![key, value])))
            .collect()
    })
    .unwrap_or_default();
    tagged_object(new_array(pairs))
}

extern "C" fn sealion_map_new() -> u64 {
    new_map() as u64
}

pub(super) fn lookup(name: &str) -> Option<usize> {
    Some(match name {
        "sealion_map_new" => sealion_map_new as *const () as usize,
        _ => return None,
    })
}
