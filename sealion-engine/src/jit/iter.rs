//! Iteration for `for..of` and `for..in`.
//!
//! The engine has no iterator protocol (no `Symbol.iterator`), so `for..of`
//! accepts the iterable shapes the runtime already has: arrays, strings (code
//! points, as the specification requires), maps (entry pairs) and array-likes
//! (anything with a numeric `length`). Anything else yields no iterations
//! rather than raising a `TypeError`, matching the rest of the runtime.
//!
//! A loop opens an [`Iterator`] once and then asks it for a value per step:
//! `sealion_iter_next` advances and reports whether there was a value, and
//! `sealion_iter_value` returns the one that was just produced. The length is
//! re-read every step, so a body that grows or shortens the array is visible.

use sealion_string::JsString;

use super::objects::{JsObject, get_property, new_array, object_pointer, tagged_object};
use super::strings::{tagged_bool, tagged_string, tagged_undefined, to_number_value, with_string};

enum Kind {
    Array,
    String,
    Map,
    ArrayLike,
}

struct Iterator {
    target: f64,
    index: usize,
    current: f64,
    kind: Kind,
}

fn kind_of(value: f64) -> Kind {
    if let Some(kind) = object_pointer(value).and_then(|pointer| {
        let object = unsafe { &*pointer };
        if object.elements.is_some() {
            Some(Kind::Array)
        } else if object.entries.is_some() {
            Some(Kind::Map)
        } else {
            None
        }
    }) {
        return kind;
    }
    if with_string(value, |_| ()).is_some() {
        return Kind::String;
    }
    Kind::ArrayLike
}

fn entry_at(value: f64, index: usize) -> Option<(f64, f64)> {
    let pointer = object_pointer(value)?;
    unsafe { &*pointer }.entries.as_ref()?.get(index).copied()
}

fn length(iterator: &Iterator) -> usize {
    match iterator.kind {
        Kind::Array => object_pointer(iterator.target)
            .and_then(|pointer| unsafe { &*pointer }.elements.as_ref().map(Vec::len))
            .unwrap_or(0),
        Kind::String => with_string(iterator.target, |text| text.len()).unwrap_or(0),
        Kind::Map => object_pointer(iterator.target)
            .and_then(|pointer| unsafe { &*pointer }.entries.as_ref().map(Vec::len))
            .unwrap_or(0),
        Kind::ArrayLike => {
            let length = to_number_value(get_property(iterator.target, "length"));
            if length.is_finite() && length > 0.0 {
                length as usize
            } else {
                0
            }
        }
    }
}

extern "C" fn sealion_iter_new(target: f64) -> u64 {
    let iterator = Iterator {
        target,
        index: 0,
        current: tagged_undefined(),
        kind: kind_of(target),
    };
    Box::into_raw(Box::new(iterator)) as u64
}

extern "C" fn sealion_iter_next(handle: i64) -> f64 {
    let iterator = unsafe { &mut *(handle as usize as *mut Iterator) };
    if iterator.index >= length(iterator) {
        iterator.current = tagged_undefined();
        return tagged_bool(false);
    }
    match iterator.kind {
        Kind::Array => {
            let value = object_pointer(iterator.target)
                .and_then(|pointer| {
                    unsafe { &*pointer }
                        .elements
                        .as_ref()?
                        .get(iterator.index)
                        .copied()
                })
                .unwrap_or_else(tagged_undefined);
            iterator.current = value;
            iterator.index += 1;
        }
        Kind::Map => {
            iterator.current = match entry_at(iterator.target, iterator.index) {
                Some((key, value)) => tagged_object(new_array(vec![key, value])),
                None => tagged_undefined(),
            };
            iterator.index += 1;
        }
        Kind::String => match with_string(iterator.target, |text| {
            let code_point = text.code_point_at(iterator.index);
            let mut units = [0u16; 2];
            let units = code_point.encode_utf16(&mut units);
            (JsString::from(&units[..]), code_point.code_unit_count())
        }) {
            Some((value, width)) => {
                iterator.current = tagged_string(value);
                iterator.index += width.max(1);
            }
            None => iterator.index += 1,
        },
        Kind::ArrayLike => {
            let key = iterator.index.to_string();
            iterator.current = get_property(iterator.target, &key);
            iterator.index += 1;
        }
    }
    tagged_bool(true)
}

extern "C" fn sealion_iter_value(handle: i64) -> f64 {
    let iterator = unsafe { &*(handle as usize as *mut Iterator) };
    iterator.current
}

/// The own and inherited enumerable string keys of `value`, in specification
/// order: array indices ascending, then named properties in insertion order,
/// with shadowed names appearing once.
pub(super) fn for_in_keys(value: f64) -> Vec<JsString> {
    let mut keys = Vec::new();
    let mut seen: Vec<JsString> = Vec::new();
    let mut current = object_pointer(value).or_else(|| super::functions::function_object(value));
    let mut depth = 0;
    while let Some(pointer) = current {
        if depth > 1000 {
            break;
        }
        let object: &JsObject = unsafe { &*pointer };
        if let Some(elements) = &object.elements {
            for index in 0..elements.len() {
                let key = JsString::from(index.to_string());
                if !seen.contains(&key) {
                    seen.push(key.clone());
                    keys.push(key);
                }
            }
        }
        for (key, _) in &object.properties {
            if object.hidden.contains(key) {
                continue;
            }
            if !seen.contains(key) {
                seen.push(key.clone());
                keys.push(key.clone());
            }
        }
        current = object.prototype;
        depth += 1;
    }
    keys
}

extern "C" fn sealion_object_for_in_keys(value: f64) -> f64 {
    let keys = for_in_keys(value);
    tagged_object(new_array(keys.into_iter().map(tagged_string).collect()))
}

pub(super) fn lookup(name: &str) -> Option<usize> {
    Some(match name {
        "sealion_iter_new" => sealion_iter_new as *const () as usize,
        "sealion_iter_next" => sealion_iter_next as *const () as usize,
        "sealion_iter_value" => sealion_iter_value as *const () as usize,
        "sealion_object_for_in_keys" => sealion_object_for_in_keys as *const () as usize,
        _ => return None,
    })
}
