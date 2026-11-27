//! Host helpers for JavaScript object values.
//!
//! Compiled code stores an object as a NaN-boxed pointer to a [`JsObject`]: an
//! insertion-ordered own-property table plus an optional dense element vector.
//! A [`JsObject`] with elements is an array, answered by `jit::arrays`; the two
//! share one allocation so the tag is enough to tell them apart. There are no
//! prototypes, property descriptors or garbage collector, so objects leak for
//! the lifetime of the process exactly like closure and environment records.
//!
//! Property keys are canonicalised through `ToPropertyKey`, so `o[1]` and
//! `o["1"]` name the same property, and every accessor is total: a receiver
//! that is not an object yields `undefined` on read, `false` for `has` and
//! `delete`, and the written value on write. Nothing here throws.

use sealion_string::JsString;

use super::arrays;
use super::maps;
use super::strings::{
    is_object, method_property, tagged_bool, tagged_undefined, to_js_string, with_string,
};
use crate::value::{PAYLOAD_MASK, TAG_OBJECT, TAG_STRING};

pub(super) struct JsObject {
    properties: Vec<(JsString, f64)>,
    pub(super) elements: Option<Vec<f64>>,
    /// `Some(..)` when this object is a `Map`: insertion-ordered key/value
    /// pairs compared with SameValueZero.
    pub(super) entries: Option<Vec<(f64, f64)>>,
}

pub(super) fn new_object() -> *mut JsObject {
    Box::into_raw(Box::new(JsObject {
        properties: Vec::new(),
        elements: None,
        entries: None,
    }))
}

pub(super) fn new_array(elements: Vec<f64>) -> *mut JsObject {
    Box::into_raw(Box::new(JsObject {
        properties: Vec::new(),
        elements: Some(elements),
        entries: None,
    }))
}

pub(super) fn new_map() -> *mut JsObject {
    Box::into_raw(Box::new(JsObject {
        properties: Vec::new(),
        elements: None,
        entries: Some(Vec::new()),
    }))
}

pub(super) fn tagged_object(pointer: *mut JsObject) -> f64 {
    f64::from_bits((TAG_OBJECT << 48) | pointer as u64)
}

fn object_pointer(value: f64) -> Option<*mut JsObject> {
    if !is_object(value) {
        return None;
    }
    Some((value.to_bits() & PAYLOAD_MASK) as *mut JsObject)
}

/// Runs `f` on the object behind `value`, or returns `None` for non-objects.
///
/// The compiled program never frees an object, so the borrow is not tracked.
pub(super) fn with_object<R>(value: f64, f: impl FnOnce(&mut JsObject) -> R) -> Option<R> {
    let pointer = object_pointer(value)?;
    Some(f(unsafe { &mut *pointer }))
}

/// `ToPropertyKey`: strings are kept, every other value is converted to one of
/// its string forms.
fn property_key(key: f64) -> JsString {
    if key.to_bits() >> 48 == TAG_STRING {
        with_string(key, JsString::clone).unwrap_or_default()
    } else {
        to_js_string(key)
    }
}

fn own_property(object: f64, key: &JsString) -> Option<f64> {
    with_object(object, |object| {
        object
            .properties
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| *value)
    })
    .flatten()
}

extern "C" fn sealion_object_new() -> u64 {
    new_object() as u64
}

extern "C" fn sealion_object_get(object: f64, key: f64) -> f64 {
    let key = property_key(key);
    // Strings answer `length` here, so codegen has a single property path.
    if key == "length"
        && let Some(length) = with_string(object, |string| string.len() as f64)
    {
        return length;
    }
    if let Some(value) = arrays::element_property(object, &key) {
        return value;
    }
    if let Some(value) = own_property(object, &key) {
        return value;
    }
    if let Some(value) = maps::property(object, &key) {
        return value;
    }
    maps::method_property(object, &key)
        .or_else(|| arrays::method_property(object, &key))
        .or_else(|| method_property(object, &key))
        .unwrap_or_else(tagged_undefined)
}

extern "C" fn sealion_object_set(object: f64, key: f64, value: f64) -> f64 {
    let key = property_key(key);
    if arrays::set_element(object, &key, value) {
        return value;
    }
    with_object(object, |object| {
        match object.properties.iter_mut().find(|(name, _)| *name == key) {
            Some(entry) => entry.1 = value,
            None => object.properties.push((key, value)),
        }
    });
    value
}

extern "C" fn sealion_object_has(object: f64, key: f64) -> f64 {
    let key = property_key(key);
    let has = arrays::has_element(object, &key)
        || with_object(object, |object| {
            object.properties.iter().any(|(name, _)| *name == key)
        })
        .unwrap_or(false);
    tagged_bool(has)
}

extern "C" fn sealion_object_delete(object: f64, key: f64) -> f64 {
    let key = property_key(key);
    arrays::delete_element(object, &key);
    with_object(object, |object| {
        object.properties.retain(|(name, _)| *name != key);
    });
    tagged_bool(true)
}

pub(super) fn lookup(name: &str) -> Option<usize> {
    Some(match name {
        "sealion_object_new" => sealion_object_new as *const () as usize,
        "sealion_object_get" => sealion_object_get as *const () as usize,
        "sealion_object_set" => sealion_object_set as *const () as usize,
        "sealion_object_has" => sealion_object_has as *const () as usize,
        "sealion_object_delete" => sealion_object_delete as *const () as usize,
        _ => return None,
    })
}
