//! Host helpers for JavaScript object values.
//!
//! Compiled code stores an object as a NaN-boxed pointer to a [`JsObject`]: an
//! insertion-ordered own-property table, an optional dense element vector and a
//! `prototype` link to the object consulted when an own property is absent. A
//! [`JsObject`] with elements is an array, answered by `jit::arrays`; one with
//! entries is a `Map`. The kinds share one allocation so the tag is enough to
//! tell them apart. There are no property descriptors or garbage collector, so
//! objects leak for the lifetime of the process exactly like closure and
//! environment records.
//!
//! Property keys are canonicalised through `ToPropertyKey`, so `o[1]` and
//! `o["1"]` name the same property, and every accessor is total: a receiver
//! that is not an object yields `undefined` on read, `false` for `has` and
//! `delete`, and the written value on write. Nothing here throws, and a
//! prototype cycle is refused instead of raising a `TypeError`.
//!
//! Reads walk the prototype chain (`chain_get`); writes, `delete` and
//! `Object.hasOwn` stay on the receiver, as the specification requires. Arrays,
//! maps and strings answer their methods from native tables rather than from
//! real prototype objects, so `Array.prototype` is an ordinary object that users
//! can still extend. See `docs/internals/OBJECTS.md`.

use std::sync::OnceLock;

use sealion_string::JsString;

use super::arrays;
use super::maps;
use super::strings::{
    is_object, method_property, tagged_bool, tagged_null, tagged_string, tagged_undefined,
    to_js_string, with_string,
};
use crate::value::{PAYLOAD_MASK, TAG_NULL, TAG_OBJECT, TAG_STRING};

/// How far a property lookup may walk a prototype chain. Cycles are refused by
/// `Object.setPrototypeOf`/`__proto__`, so this only bounds a corrupt chain.
const MAX_PROTOTYPE_DEPTH: usize = 1000;

pub(super) struct JsObject {
    pub(super) properties: Vec<(JsString, f64)>,
    pub(super) elements: Option<Vec<f64>>,
    /// `Some(..)` when this object is a `Map`: insertion-ordered key/value
    /// pairs compared with SameValueZero.
    pub(super) entries: Option<Vec<(f64, f64)>>,
    /// `Some(..)` when this object is a `Date`: the instant in milliseconds
    /// since the epoch, `NaN` when invalid (a "time value" in the spec).
    pub(super) date: Option<f64>,
    /// The object consulted when an own property is absent, or `None` to end
    /// the chain (`Object.create(null)`).
    pub(super) prototype: Option<*mut JsObject>,
    /// Set when this object is the property bag of a function value, so that
    /// `Object.getPrototypeOf` can map it back to the function itself.
    pub(super) owner: Option<f64>,
    /// Own properties that exist but are not enumerable, so `for..in` and
    /// `Object.keys` skip them (`constructor` and a function's `prototype`).
    pub(super) hidden: Vec<JsString>,
}

fn alloc_object(prototype: Option<*mut JsObject>) -> *mut JsObject {
    Box::into_raw(Box::new(JsObject {
        properties: Vec::new(),
        elements: None,
        entries: None,
        date: None,
        prototype,
        owner: None,
        hidden: Vec::new(),
    }))
}

/// Allocates a `Date` holding `time`.
pub(super) fn new_date(time: f64) -> *mut JsObject {
    let pointer = alloc_object(Some(super::dates::date_prototype()));
    unsafe { (*pointer).date = Some(time) };
    pointer
}

/// The shared root prototype (`Object.prototype`), created on first use.
pub(super) fn object_prototype() -> *mut JsObject {
    static PROTOTYPE: OnceLock<usize> = OnceLock::new();
    *PROTOTYPE.get_or_init(|| alloc_object(None) as usize) as *mut JsObject
}

/// The shared `Array.prototype`, whose own prototype is `Object.prototype`.
fn array_prototype() -> *mut JsObject {
    static PROTOTYPE: OnceLock<usize> = OnceLock::new();
    *PROTOTYPE.get_or_init(|| alloc_object(Some(object_prototype())) as usize) as *mut JsObject
}

pub(super) fn new_object() -> *mut JsObject {
    alloc_object(Some(object_prototype()))
}

pub(super) fn new_array(elements: Vec<f64>) -> *mut JsObject {
    Box::into_raw(Box::new(JsObject {
        properties: Vec::new(),
        elements: Some(elements),
        entries: None,
        date: None,
        prototype: Some(array_prototype()),
        owner: None,
        hidden: Vec::new(),
    }))
}

pub(super) fn new_map() -> *mut JsObject {
    Box::into_raw(Box::new(JsObject {
        properties: Vec::new(),
        elements: None,
        entries: Some(Vec::new()),
        date: None,
        prototype: Some(object_prototype()),
        owner: None,
        hidden: Vec::new(),
    }))
}

/// A native method bound to `env`: a closure record whose first two words
/// are `[entry point, receiver]` (see `jit::functions`).
pub(super) fn native_function(method: Method, env: u64) -> f64 {
    super::functions::new_record(method as usize, env, false)
}

/// Defines (or overwrites) the own data property `key` of `object`.
pub(super) fn define_property(object: *mut JsObject, key: &str, value: f64) {
    let key = JsString::from(key);
    let object = object_at(object);
    match object.properties.iter_mut().find(|(name, _)| *name == key) {
        Some(entry) => entry.1 = value,
        None => object.properties.push((key, value)),
    }
}

/// Defines an own property that reads back normally but is skipped by
/// enumeration, like a function's `prototype` or a prototype's `constructor`.
pub(super) fn define_hidden_property(object: *mut JsObject, key: &str, value: f64) {
    define_property(object, key, value);
    let key = JsString::from(key);
    let object = object_at(object);
    if !object.hidden.contains(&key) {
        object.hidden.push(key);
    }
}

pub(super) fn tagged_object(pointer: *mut JsObject) -> f64 {
    f64::from_bits((TAG_OBJECT << 48) | pointer as u64)
}

pub(super) fn object_pointer(value: f64) -> Option<*mut JsObject> {
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

/// The value of an own data property: dense array elements, the `length` of an
/// array, the `size` of a map, then the ordinary name/value table. An index at
/// or past an array's length is absent, so the chain keeps looking.
fn own_property(object: &JsObject, key: &JsString) -> Option<f64> {
    if let Some(elements) = &object.elements {
        if key == "length" {
            return Some(elements.len() as f64);
        }
        if let Some(index) = arrays::index_of_key(key) {
            return elements.get(index).copied();
        }
    }
    if let Some(entries) = &object.entries
        && key == "size"
    {
        return Some(entries.len() as f64);
    }
    object
        .properties
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| *value)
}

fn object_at(pointer: *mut JsObject) -> &'static mut JsObject {
    unsafe { &mut *pointer }
}

/// `[[Get]]`: walks the prototype chain looking for an own data property.
pub(super) fn chain_get(object: f64, key: &JsString) -> Option<f64> {
    let mut current = object_pointer(object);
    let mut depth = 0;
    while let Some(pointer) = current {
        if depth > MAX_PROTOTYPE_DEPTH {
            break;
        }
        let object = unsafe { &*pointer };
        if let Some(value) = own_property(object, key) {
            return Some(value);
        }
        current = object.prototype;
        depth += 1;
    }
    None
}

/// The object's prototype as a value: the prototype object, or `null`.
fn prototype_value(object: f64) -> Option<f64> {
    let pointer = object_pointer(object).or_else(|| super::functions::function_object(object))?;
    Some(match unsafe { (*pointer).prototype } {
        Some(prototype) => match unsafe { (*prototype).owner } {
            // A function's property bag stands in for the function itself, so
            // `Object.getPrototypeOf(B) === A` holds for `class B extends A`.
            Some(owner) => owner,
            None => tagged_object(prototype),
        },
        None => tagged_null(),
    })
}

/// `true` when following `start`'s chain reaches `target`, used to refuse a
/// prototype cycle (a `TypeError` in the specification, a no-op here).
fn reaches(start: *mut JsObject, target: *mut JsObject) -> bool {
    let mut current = Some(start);
    let mut depth = 0;
    while let Some(pointer) = current {
        if pointer == target {
            return true;
        }
        if depth > MAX_PROTOTYPE_DEPTH {
            return true;
        }
        current = unsafe { (*pointer).prototype };
        depth += 1;
    }
    false
}

/// `SetPrototypeOf`: an object or `null` replaces the prototype, anything else
/// is ignored, and a cycle is refused. A function's prototype chain lives on its
/// own property object, which is how `class B extends A` inherits the statics.
fn set_prototype(object: f64, value: f64) {
    let Some(pointer) =
        object_pointer(object).or_else(|| super::functions::function_object(object))
    else {
        return;
    };
    if value.to_bits() >> 48 == TAG_NULL {
        unsafe { (*pointer).prototype = None };
        return;
    }
    let prototype = match object_pointer(value).or_else(|| super::functions::function_object(value))
    {
        Some(pointer) => pointer,
        None => return,
    };
    if prototype == pointer || reaches(prototype, pointer) {
        return;
    }
    unsafe { (*pointer).prototype = Some(prototype) };
}

/// The own enumerable string keys of `object`: array indices in order, then the
/// named properties in insertion order.
fn own_keys(object: &JsObject) -> Vec<JsString> {
    let mut keys = Vec::new();
    if let Some(elements) = &object.elements {
        keys.extend((0..elements.len()).map(|index| JsString::from(index.to_string())));
    }
    keys.extend(
        object
            .properties
            .iter()
            .map(|(key, _)| key)
            .filter(|key| !object.hidden.contains(key))
            .cloned(),
    );
    keys
}

extern "C" fn sealion_object_new() -> u64 {
    new_object() as u64
}

extern "C" fn sealion_object_get(object: f64, key: f64) -> f64 {
    let key = property_key(key);
    if super::functions::is_function(object) {
        return function_get(object, &key);
    }
    // Strings answer `length` here, so codegen has a single property path.
    if key == "length"
        && let Some(length) = with_string(object, |string| string.len() as f64)
    {
        return length;
    }
    if let Some(value) = chain_get(object, &key) {
        return value;
    }
    // `__proto__` behaves like the accessor on `Object.prototype`.
    if key == "__proto__"
        && let Some(value) = prototype_value(object)
    {
        return value;
    }
    maps::method_property(object, &key)
        .or_else(|| arrays::method_property(object, &key))
        .or_else(|| super::dates::method_property(object, &key))
        .or_else(|| object_method_property(object, &key))
        .or_else(|| method_property(object, &key))
        .unwrap_or_else(tagged_undefined)
}

/// Reads `value[key]` through the ordinary property path, for host helpers
/// such as `Array.from` that index array-likes.
pub(super) fn get_property(value: f64, key: &str) -> f64 {
    sealion_object_get(value, tagged_string(JsString::from(key)))
}

extern "C" fn sealion_object_set(object: f64, key: f64, value: f64) -> f64 {
    let key = property_key(key);
    if super::functions::is_function(object) {
        super::functions::set_property(object, &key, value);
        return value;
    }
    if key == "__proto__" {
        set_prototype(object, value);
        return value;
    }
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
    if super::functions::is_function(object) {
        let has = super::functions::property(object, &key).is_some()
            || (key == "prototype" && super::functions::is_constructible(object))
            || super::functions::method_property(object, &key).is_some();
        return tagged_bool(has);
    }
    let has = chain_get(object, &key).is_some()
        || arrays::has_element(object, &key)
        || maps::method_property(object, &key).is_some()
        || super::dates::method_property(object, &key).is_some()
        || object_method_property(object, &key).is_some()
        || (key == "__proto__" && is_object(object));
    tagged_bool(has)
}

extern "C" fn sealion_object_delete(object: f64, key: f64) -> f64 {
    let key = property_key(key);
    if super::functions::is_function(object) {
        if let Some(properties) = super::functions::function_object(object) {
            with_object(tagged_object(properties), |properties| {
                properties.properties.retain(|(name, _)| *name != key);
            });
        }
        return tagged_bool(true);
    }
    arrays::delete_element(object, &key);
    with_object(object, |object| {
        object.properties.retain(|(name, _)| *name != key);
    });
    tagged_bool(true)
}

type Method = extern "C" fn(i64, *const u8, f64, f64) -> f64;

/// The name of one of `Object.prototype`'s methods, if any.
fn object_method(key: &JsString) -> Option<Method> {
    Some(match key.to_std_string_lossy().as_str() {
        "hasOwnProperty" => has_own_property,
        "isPrototypeOf" => is_prototype_of,
        "propertyIsEnumerable" => property_is_enumerable,
        "toString" => to_string,
        "valueOf" => value_of,
        _ => return None,
    })
}

/// `true` when the chain from `object` includes `Object.prototype`, i.e. the
/// object inherits the methods of `Object.prototype`.
fn inherits_object_prototype(object: f64) -> bool {
    let target = object_prototype();
    let mut current = object_pointer(object);
    let mut depth = 0;
    while let Some(pointer) = current {
        if pointer == target {
            return true;
        }
        if depth > MAX_PROTOTYPE_DEPTH {
            return false;
        }
        current = unsafe { (*pointer).prototype };
        depth += 1;
    }
    false
}

/// The native closure for `object.method`, bound to the receiver that named it.
fn object_method_property(value: f64, key: &JsString) -> Option<f64> {
    if !is_object(value) || !inherits_object_prototype(value) {
        return None;
    }
    let method = object_method(key)?;
    Some(native_function(method, value.to_bits() & PAYLOAD_MASK))
}

fn receiver(env: i64) -> f64 {
    tagged_object(env as usize as *mut JsObject)
}

extern "C" fn has_own_property(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let key = property_key(arrays::arg(args, 0));
    let own = own_property(object_at(env as usize as *mut JsObject), &key).is_some();
    tagged_bool(own)
}

extern "C" fn is_prototype_of(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let target = arrays::arg(args, 0);
    let Some(mut current) = object_pointer(target) else {
        return tagged_bool(false);
    };
    let mut depth = 0;
    while let Some(prototype) = unsafe { (*current).prototype } {
        if prototype as usize == env as usize {
            return tagged_bool(true);
        }
        if depth > MAX_PROTOTYPE_DEPTH {
            break;
        }
        current = prototype;
        depth += 1;
    }
    tagged_bool(false)
}

extern "C" fn property_is_enumerable(env: i64, args: *const u8, _callee: f64, _this: f64) -> f64 {
    let key = property_key(arrays::arg(args, 0));
    let own = own_property(object_at(env as usize as *mut JsObject), &key).is_some();
    tagged_bool(own)
}

extern "C" fn to_string(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    tagged_string(to_js_string(receiver(env)))
}

extern "C" fn value_of(env: i64, _args: *const u8, _callee: f64, _this: f64) -> f64 {
    receiver(env)
}

extern "C" fn sealion_object_create(proto: f64) -> f64 {
    let prototype = if proto.to_bits() >> 48 == TAG_NULL {
        None
    } else if is_object(proto) {
        Some((proto.to_bits() & PAYLOAD_MASK) as *mut JsObject)
    } else {
        return tagged_undefined();
    };
    tagged_object(alloc_object(prototype))
}

extern "C" fn sealion_object_get_prototype(object: f64) -> f64 {
    prototype_value(object).unwrap_or_else(tagged_undefined)
}

extern "C" fn sealion_object_set_prototype(object: f64, proto: f64) -> f64 {
    set_prototype(object, proto);
    object
}

extern "C" fn sealion_object_has_own(object: f64, key: f64) -> f64 {
    let key = property_key(key);
    let own = object_pointer(object)
        .map(|pointer| own_property(object_at(pointer), &key).is_some())
        .unwrap_or(false);
    tagged_bool(own)
}

extern "C" fn sealion_object_prototype() -> u64 {
    object_prototype() as u64
}

extern "C" fn sealion_array_prototype() -> u64 {
    array_prototype() as u64
}

/// `Object.keys`: the own enumerable string keys as an array.
extern "C" fn sealion_object_keys(object: f64) -> f64 {
    let keys = object_pointer(object)
        .map(|pointer| own_keys(object_at(pointer)))
        .unwrap_or_default();
    tagged_object(new_array(keys.into_iter().map(tagged_string).collect()))
}

/// `Object.values`: the values of those keys.
extern "C" fn sealion_object_values(object: f64) -> f64 {
    let values = object_pointer(object)
        .map(|pointer| {
            let object = object_at(pointer);
            own_keys(object)
                .iter()
                .filter_map(|key| own_property(object, key))
                .collect()
        })
        .unwrap_or_default();
    tagged_object(new_array(values))
}

/// `Object.entries`: `[key, value]` pairs.
extern "C" fn sealion_object_entries(object: f64) -> f64 {
    let entries = object_pointer(object)
        .map(|pointer| {
            let object = object_at(pointer);
            own_keys(object)
                .iter()
                .filter_map(|key| {
                    let value = own_property(object, key)?;
                    Some(tagged_object(new_array(vec![
                        tagged_string(key.clone()),
                        value,
                    ])))
                })
                .collect()
        })
        .unwrap_or_default();
    tagged_object(new_array(entries))
}

/// `value[key]` for a function value: its own properties, then its lazily
/// created `prototype`, then the `Function.prototype` methods.
fn function_get(value: f64, key: &JsString) -> f64 {
    if let Some(found) = super::functions::property(value, key) {
        return found;
    }
    if key == "prototype" && super::functions::is_constructible(value) {
        return super::functions::ensure_prototype(value);
    }
    super::functions::method_property(value, key).unwrap_or_else(tagged_undefined)
}

/// Allocates the object a `new` expression fills in, with `prototype` as its
/// prototype (falling back to `Object.prototype` for a non-object).
extern "C" fn sealion_object_new_instance(prototype: f64) -> f64 {
    let prototype = object_pointer(prototype).unwrap_or_else(object_prototype);
    tagged_object(alloc_object(Some(prototype)))
}

extern "C" fn sealion_instanceof(value: f64, constructor: f64) -> f64 {
    tagged_bool(super::functions::has_instance(value, constructor))
}

pub(super) fn lookup(name: &str) -> Option<usize> {
    Some(match name {
        "sealion_object_new" => sealion_object_new as *const () as usize,
        "sealion_object_get" => sealion_object_get as *const () as usize,
        "sealion_object_set" => sealion_object_set as *const () as usize,
        "sealion_object_has" => sealion_object_has as *const () as usize,
        "sealion_object_delete" => sealion_object_delete as *const () as usize,
        "sealion_object_create" => sealion_object_create as *const () as usize,
        "sealion_object_get_prototype" => sealion_object_get_prototype as *const () as usize,
        "sealion_object_set_prototype" => sealion_object_set_prototype as *const () as usize,
        "sealion_object_has_own" => sealion_object_has_own as *const () as usize,
        "sealion_object_prototype" => sealion_object_prototype as *const () as usize,
        "sealion_array_prototype" => sealion_array_prototype as *const () as usize,
        "sealion_object_keys" => sealion_object_keys as *const () as usize,
        "sealion_object_values" => sealion_object_values as *const () as usize,
        "sealion_object_entries" => sealion_object_entries as *const () as usize,
        "sealion_object_new_instance" => sealion_object_new_instance as *const () as usize,
        "sealion_instanceof" => sealion_instanceof as *const () as usize,
        _ => return None,
    })
}
