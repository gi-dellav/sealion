//! Host helpers for JavaScript object values.
//!
//! Compiled code stores an object as a NaN-boxed pointer to a [`JsObject`]: a
//! canonical [`Shape`] with a parallel slot array for the named properties, an
//! optional dense element vector and a `prototype` link to the object consulted
//! when an own property is absent. A [`JsObject`] with elements is an array,
//! answered by `jit::arrays`; one with entries is a `Map`. The kinds share one
//! allocation so the tag is enough to tell them apart. There are no property
//! descriptors or garbage collector, so objects — and shapes — leak for the
//! lifetime of the process exactly like closure and environment records.
//!
//! A statically named access site compares the receiver's shape with the one it
//! cached and indexes the slots itself; `sealion_object_get_cached` and
//! `sealion_object_set_cached` are the misses that refill it. See
//! `docs/internals/INLINE_CACHES.md`.
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

use std::mem::ManuallyDrop;
use std::sync::{Mutex, OnceLock};

use sealion_string::JsString;

use super::arrays;
use super::maps;
use super::strings::{
    is_object, method_property, string_pointer, tagged_bool, tagged_null, tagged_string,
    tagged_undefined, to_js_string, with_string,
};
use crate::value::{PAYLOAD_MASK, TAG_NULL, TAG_OBJECT, TAG_STRING};

/// How far a property lookup may walk a prototype chain. Cycles are refused by
/// `Object.setPrototypeOf`/`__proto__`, so this only bounds a corrupt chain.
const MAX_PROTOTYPE_DEPTH: usize = 1000;

/// `#[repr(C)]` freezes the field order: compiled access sites load `shape`
/// and `slots` at the offsets in [`crate::value`] rather than going through a
/// helper.
#[repr(C)]
pub(super) struct JsObject {
    /// The canonical property layout. Kept first so that the inline cache in
    /// `compiler::object` can load it at [`crate::value::OBJECT_SHAPE_OFFSET`].
    pub(super) shape: *const Shape,
    /// The value of `shape.names[i]` for every `i`, so a property read is a
    /// shape check and an indexed load rather than a scan of name/value pairs.
    pub(super) slots: Slots,
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

/// The own property values, parallel to `Shape::names`.
///
/// Held as a raw pointer, length and capacity rather than a `Vec` because a
/// compiled access site loads the data pointer with a single fixup-free load at
/// [`crate::value::OBJECT_SLOTS_OFFSET`], and a `Vec` hides its data pointer
/// behind a layout the standard library is free to change.
#[repr(C)]
pub(super) struct Slots {
    data: *mut f64,
    len: usize,
    capacity: usize,
}

impl Slots {
    const EMPTY: Slots = Slots {
        data: std::ptr::NonNull::<f64>::dangling().as_ptr(),
        len: 0,
        capacity: 0,
    };

    fn as_slice(&self) -> &[f64] {
        unsafe { std::slice::from_raw_parts(self.data, self.len) }
    }

    fn as_mut_slice(&mut self) -> &mut [f64] {
        unsafe { std::slice::from_raw_parts_mut(self.data, self.len) }
    }

    fn reserve(&mut self, extra: usize) {
        if self.len + extra <= self.capacity {
            return;
        }
        let capacity = (self.capacity * 2).max(4).max(self.len + extra);
        let layout =
            std::alloc::Layout::array::<f64>(capacity).expect("slot array size does not overflow");
        let data = unsafe {
            if self.capacity == 0 {
                std::alloc::alloc(layout)
            } else {
                let previous = std::alloc::Layout::array::<f64>(self.capacity)
                    .expect("slot array size does not overflow");
                std::alloc::realloc(self.data.cast(), previous, layout.size())
            }
        };
        if data.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        self.data = data.cast();
        self.capacity = capacity;
    }

    fn push(&mut self, value: f64) {
        self.reserve(1);
        unsafe { *self.data.add(self.len) = value };
        self.len += 1;
    }

    fn remove(&mut self, index: usize) {
        unsafe {
            std::ptr::copy(
                self.data.add(index + 1),
                self.data.add(index),
                self.len - index - 1,
            );
        }
        self.len -= 1;
    }
}

/// A canonical, shared property layout: the own property names of an object in
/// insertion order. Objects with the same names in the same order share one
/// `Shape`, so a compiled access site can compare a single pointer to decide
/// that a slot index it cached is still the right one.
///
/// Shapes are leaked: every distinct layout lives for the lifetime of the
/// process, like the objects and strings themselves. `names` never changes once
/// the shape exists, and the transition table is behind a mutex, which is what
/// makes a shape safe to share between the runtime's threads.
pub(super) struct Shape {
    names: Vec<JsString>,
    /// Layouts reachable by adding one property, memoised so that a constructor
    /// run in a loop reuses the same shape instead of creating a new one.
    transitions: Mutex<Vec<(JsString, *const Shape)>>,
}

// SAFETY: `names` and the transition list are only ever appended to, the
// transition list is reached through a mutex, and both a shape and everything
// it points at are leaked, so a shared reference is always valid.
unsafe impl Sync for Shape {}

impl Shape {
    fn new(names: Vec<JsString>) -> *const Shape {
        Box::into_raw(Box::new(Shape {
            names,
            transitions: Mutex::new(Vec::new()),
        }))
    }

    /// The slot of the own property named `key`.
    fn index_of(&self, key: &JsString) -> Option<usize> {
        self.names.iter().position(|name| name == key)
    }

    /// The shape of an object that also has `key`, appended as a new slot.
    fn transition(&self, key: &JsString) -> *const Shape {
        let mut transitions = self
            .transitions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(&(_, next)) = transitions.iter().find(|(name, _)| name == key) {
            return next;
        }
        let mut names = self.names.clone();
        names.push(key.clone());
        let next = Shape::new(names);
        transitions.push((key.clone(), next));
        next
    }

    /// The shape of an object that no longer has `key`, with the later slots
    /// shifted down. Deletes are rare, so the result is not memoised.
    fn without(&self, key: &JsString) -> *const Shape {
        Shape::new(
            self.names
                .iter()
                .filter(|name| *name != key)
                .cloned()
                .collect(),
        )
    }
}

const _: () = assert!(core::mem::offset_of!(JsObject, shape) == 0);
const _: () =
    assert!(core::mem::offset_of!(JsObject, slots) == crate::value::OBJECT_SLOTS_OFFSET as usize);
const _: () = assert!(core::mem::offset_of!(Slots, data) == 0);

/// The shape of an object with no own named properties.
fn empty_shape() -> *const Shape {
    static EMPTY: OnceLock<usize> = OnceLock::new();
    *EMPTY.get_or_init(|| Shape::new(Vec::new()) as usize) as *const Shape
}

impl JsObject {
    fn shape(&self) -> &Shape {
        unsafe { &*self.shape }
    }

    /// The own property names, in insertion order.
    pub(super) fn names(&self) -> &[JsString] {
        &self.shape().names
    }

    /// The value of the own property `key`, following no prototype link.
    fn get_named(&self, key: &JsString) -> Option<f64> {
        let index = self.shape().index_of(key)?;
        self.slots.as_slice().get(index).copied()
    }

    /// Defines `key`, appending a slot when the object does not have it yet.
    fn set_named(&mut self, key: &JsString, value: f64) -> usize {
        if let Some(index) = self.shape().index_of(key) {
            self.slots.as_mut_slice()[index] = value;
            return index;
        }
        let index = self.slots.len;
        self.shape = self.shape().transition(key);
        self.slots.push(value);
        index
    }

    /// Removes `key`, shifting the later slots down. `false` when absent.
    fn remove_named(&mut self, key: &JsString) -> bool {
        let Some(index) = self.shape().index_of(key) else {
            return false;
        };
        self.shape = self.shape().without(key);
        self.slots.remove(index);
        true
    }

    fn set_hidden(&mut self, key: &JsString) {
        if !self.hidden.iter().any(|name| name == key) {
            self.hidden.push(key.clone());
        }
    }

    fn is_hidden(&self, key: &JsString) -> bool {
        self.hidden.iter().any(|name| name == key)
    }
}

fn alloc_object(prototype: Option<*mut JsObject>) -> *mut JsObject {
    Box::into_raw(Box::new(JsObject {
        shape: empty_shape(),
        slots: Slots::EMPTY,
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
        shape: empty_shape(),
        slots: Slots::EMPTY,
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
        shape: empty_shape(),
        slots: Slots::EMPTY,
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
    object_at(object).set_named(&JsString::from(key), value);
}

/// Defines an own property from a key the caller already holds.
pub(super) fn define_property_named(object: *mut JsObject, key: &JsString, value: f64) {
    object_at(object).set_named(key, value);
}

/// Defines an own property that reads back normally but is skipped by
/// enumeration, like a function's `prototype` or a prototype's `constructor`.
pub(super) fn define_hidden_property(object: *mut JsObject, key: &str, value: f64) {
    let key = JsString::from(key);
    let object = object_at(object);
    object.set_named(&key, value);
    object.set_hidden(&key);
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
    object.get_named(key)
}

/// The slot of `key` when reading it uses the receiver's own slot array, i.e.
/// when it is neither an array element, an array `length`, nor a map `size`.
/// Only such reads may be cached by a compiled access site: every other answer
/// depends on the receiver's kind, its elements or its prototype chain.
fn slot_index(object: &JsObject, key: &JsString) -> Option<usize> {
    if object.elements.is_some() && (key == "length" || arrays::index_of_key(key).is_some()) {
        return None;
    }
    if object.entries.is_some() && key == "size" {
        return None;
    }
    object.shape().index_of(key)
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
            .names()
            .iter()
            .filter(|key| !object.is_hidden(key))
            .cloned(),
    );
    keys
}

/// Runs `f` with the `ToPropertyKey` form of `key`, borrowing the string when
/// the value already is one and converting a temporary otherwise.
fn with_property_key<R>(key: f64, f: impl FnOnce(&JsString) -> R) -> R {
    if let Some(pointer) = string_pointer(key) {
        let string = ManuallyDrop::new(unsafe { JsString::from_raw(pointer) });
        return f(&string);
    }
    let key = to_js_string(key);
    f(&key)
}

extern "C" fn sealion_object_new() -> u64 {
    new_object() as u64
}

fn get_value(object: f64, key: &JsString) -> f64 {
    if super::functions::is_function(object) {
        return function_get(object, key);
    }
    // Strings answer `length` here, so codegen has a single property path.
    if key == "length"
        && let Some(length) = with_string(object, |string| string.len() as f64)
    {
        return length;
    }
    if let Some(value) = chain_get(object, key) {
        return value;
    }
    // `__proto__` behaves like the accessor on `Object.prototype`.
    if key == "__proto__"
        && let Some(value) = prototype_value(object)
    {
        return value;
    }
    maps::method_property(object, key)
        .or_else(|| arrays::method_property(object, key))
        .or_else(|| super::dates::method_property(object, key))
        .or_else(|| object_method_property(object, key))
        .or_else(|| method_property(object, key))
        .unwrap_or_else(tagged_undefined)
}

extern "C" fn sealion_object_get(object: f64, key: f64) -> f64 {
    with_property_key(key, |key| get_value(object, key))
}

/// Reads `value[key]` through the ordinary property path, for host helpers
/// such as `Array.from` that index array-likes.
pub(super) fn get_property(value: f64, key: &str) -> f64 {
    get_value(value, &JsString::from(key))
}

fn set_value(object: f64, key: &JsString, value: f64) -> f64 {
    if super::functions::is_function(object) {
        super::functions::set_property(object, key, value);
        return value;
    }
    if key == "__proto__" {
        set_prototype(object, value);
        return value;
    }
    if arrays::set_element(object, key, value) {
        return value;
    }
    with_object(object, |object| {
        object.set_named(key, value);
    });
    value
}

extern "C" fn sealion_object_set(object: f64, key: f64, value: f64) -> f64 {
    with_property_key(key, |key| set_value(object, key, value))
}

fn has_value(object: f64, key: &JsString) -> bool {
    if super::functions::is_function(object) {
        return super::functions::property(object, key).is_some()
            || (key == "prototype" && super::functions::is_constructible(object))
            || super::functions::method_property(object, key).is_some();
    }
    chain_get(object, key).is_some()
        || arrays::has_element(object, key)
        || maps::method_property(object, key).is_some()
        || super::dates::method_property(object, key).is_some()
        || object_method_property(object, key).is_some()
        || (key == "__proto__" && is_object(object))
}

extern "C" fn sealion_object_has(object: f64, key: f64) -> f64 {
    tagged_bool(with_property_key(key, |key| has_value(object, key)))
}

extern "C" fn sealion_object_delete(object: f64, key: f64) -> f64 {
    with_property_key(key, |key| {
        if super::functions::is_function(object) {
            if let Some(properties) = super::functions::function_object(object) {
                with_object(tagged_object(properties), |properties| {
                    properties.remove_named(key);
                });
            }
            return tagged_bool(true);
        }
        arrays::delete_element(object, key);
        with_object(object, |object| {
            object.remove_named(key);
        });
        tagged_bool(true)
    })
}

/// Records that `shape`/`index` answered `cache`'s site, so the compiled fast
/// path can serve the next access with a pointer comparison.
fn fill_cache(cache: *mut u64, shape: *const Shape, index: usize) {
    unsafe {
        *cache = shape as u64;
        *cache.add(1) = index as u64;
    }
}

/// `object[key]` for a statically named key, on an inline-cache miss.
///
/// The compiled fast path compares the receiver's shape against the one this
/// left behind and only calls here when they differ. A hit on the receiver's
/// own slot array fills the cache; anything else (an array element, `length`,
/// `size`, a method table entry, a prototype hit) leaves it alone, so the site
/// keeps taking the slow path, which is always correct.
extern "C" fn sealion_object_get_cached(object: f64, key: f64, cache: *mut u64) -> f64 {
    with_property_key(key, |key| {
        if let Some(pointer) = object_pointer(object) {
            let receiver = unsafe { &*pointer };
            if let Some(index) = slot_index(receiver, key) {
                fill_cache(cache, receiver.shape, index);
                return receiver.slots.as_slice()[index];
            }
        }
        get_value(object, key)
    })
}

/// `object[key] = value` for a statically named key, on an inline-cache miss.
extern "C" fn sealion_object_set_cached(object: f64, key: f64, value: f64, cache: *mut u64) -> f64 {
    with_property_key(key, |key| {
        let result = set_value(object, key, value);
        if let Some(pointer) = object_pointer(object) {
            let receiver = unsafe { &*pointer };
            if let Some(index) = slot_index(receiver, key) {
                fill_cache(cache, receiver.shape, index);
            }
        }
        result
    })
}

type Method = extern "C" fn(i64, *const u8, f64, f64) -> f64;

/// The name of one of `Object.prototype`'s methods, if any.
fn object_method(key: &JsString) -> Option<Method> {
    super::strings::with_ascii_key(key, object_method_named)?
}

fn object_method_named(name: &[u8]) -> Option<Method> {
    Some(match name {
        b"hasOwnProperty" => has_own_property,
        b"isPrototypeOf" => is_prototype_of,
        b"propertyIsEnumerable" => property_is_enumerable,
        b"toString" => to_string,
        b"valueOf" => value_of,
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
        "sealion_object_get_cached" => sealion_object_get_cached as *const () as usize,
        "sealion_object_set" => sealion_object_set as *const () as usize,
        "sealion_object_set_cached" => sealion_object_set_cached as *const () as usize,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inline_cache_offsets_match_the_object_layout() {
        assert_eq!(core::mem::offset_of!(JsObject, shape), 0);
        assert_eq!(
            core::mem::offset_of!(JsObject, slots),
            crate::value::OBJECT_SLOTS_OFFSET as usize
        );
        assert_eq!(core::mem::offset_of!(Slots, data), 0);
    }

    #[test]
    fn slot_storage_grows_and_compacts() {
        let mut slots = Slots::EMPTY;
        assert!(slots.as_slice().is_empty());
        for value in 0..10 {
            slots.push(value as f64);
        }
        assert_eq!(
            slots.as_slice(),
            &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]
        );
        slots.as_mut_slice()[9] = 42.0;
        slots.remove(0);
        assert_eq!(
            slots.as_slice(),
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 42.0]
        );
    }

    #[test]
    fn objects_with_the_same_names_share_a_shape() {
        let first = new_object();
        let second = new_object();
        let a = JsString::from("a");
        let b = JsString::from("b");
        unsafe {
            (*first).set_named(&a, 1.0);
            (*first).set_named(&b, 2.0);
            (*second).set_named(&b, 3.0);
            (*second).set_named(&a, 4.0);
            // Insertion order is part of the shape, so these two differ ...
            assert_ne!((*first).shape, (*second).shape);
            let third = new_object();
            (*third).set_named(&a, 5.0);
            (*third).set_named(&b, 6.0);
            // ... and two objects built the same way share one.
            assert_eq!((*first).shape, (*third).shape);
            assert_eq!((*third).slots.as_slice(), &[5.0, 6.0]);
            assert_eq!((*third).names().len(), (*third).slots.len);
        }
    }

    #[test]
    fn removing_a_property_shifts_the_later_slots() {
        let object = new_object();
        let a = JsString::from("a");
        let b = JsString::from("b");
        let c = JsString::from("c");
        unsafe {
            (*object).set_named(&a, 1.0);
            (*object).set_named(&b, 2.0);
            (*object).set_named(&c, 3.0);
            assert_eq!((*object).slots.as_slice(), &[1.0, 2.0, 3.0]);
            assert!((*object).remove_named(&b));
            assert!(!(*object).remove_named(&b));
            assert_eq!((*object).get_named(&a), Some(1.0));
            assert_eq!((*object).get_named(&b), None);
            assert_eq!((*object).get_named(&c), Some(3.0));
            assert_eq!((*object).slots.as_slice(), &[1.0, 3.0]);
        }
    }
}
