# Object representation and prototypes

This document describes how the dynamic (JIT) runtime represents JavaScript
objects, how property access works, and how prototypes are implemented. The
code lives in `sealion-engine/src/jit/objects.rs`; arrays and maps reuse the same
allocation from `jit/arrays.rs` and `jit/maps.rs`.

## 1. Bit layout of an object value

Objects participate in the NaN-boxing scheme described in
[`VALUE.md`](VALUE.md). An object is an `f64` whose 16-bit tag is
`TAG_OBJECT` (`0x7FFF`) and whose 48-bit payload is a pointer to a `JsObject`:

```text
+------------------------+-----------------------------------+
|  tag 0x7FFF (16 bits)  |  *mut JsObject (48-bit payload)   |
+------------------------+-----------------------------------+
```

Codegen never inspects the payload. Every read, write, `in` and `delete` is a
call to an `extern "C"` host helper declared in `builtins::Builtins::new` and
resolved by the JIT loader through `jit::objects::lookup`. The ABI is uniform:
`f64` in, `f64` out, with `sealion_object_new` returning the raw pointer bits
that codegen wraps with `value::object`.

## 2. The `JsObject` heap block

```rust
pub(super) struct JsObject {
    properties: Vec<(JsString, f64)>,       // insertion-ordered named properties
    pub(super) elements: Option<Vec<f64>>,  // dense index range: arrays
    pub(super) entries: Option<Vec<(f64, f64)>>, // key/value pairs: Map
    pub(super) prototype: Option<*mut JsObject>,
}
```

An object is one of three shapes, distinguished by the optional fields rather
than by a tag or a class pointer:

| Shape | `elements` | `entries` | `prototype` |
| --- | --- | --- | --- |
| Plain object (`{}`) | `None` | `None` | `Object.prototype` |
| Array (`[]`) | `Some` | `None` | `Array.prototype` |
| Map (`new Map()`) | `None` | `Some` | `Object.prototype` |

Named properties always live in `properties`. `elements` and `entries` only add
the shape-specific storage; both fall back to `properties` for ordinary keys.
Nothing is ever freed: there is no garbage collector, so objects leak for the
lifetime of the process exactly like closure environment records.

Property keys are canonicalised through `ToPropertyKey`
(`objects::property_key`): a string key is kept, every other value is converted
to its string form. `o[1]` and `o["1"]` therefore name the same property.

## 3. The root prototypes

Two singleton objects exist for the lifetime of the process, created lazily with
`OnceLock`:

```text
Object.prototype                 prototype: None
      ^
      |
Array.prototype                  prototype: Some(Object.prototype)
      ^
      |
[]  (new_array)                  prototype: Some(Array.prototype)

{}  (new_object)                 prototype: Some(Object.prototype)
new Map()                        prototype: Some(Object.prototype)
```

* `new_object()` chains to `Object.prototype`.
* `new_array()` chains to `Array.prototype`, which itself chains to
  `Object.prototype`.
* `new_map()` chains to `Object.prototype`.
* `Object.create(proto)` allocates a plain object whose `prototype` is `proto`;
  `Object.create(null)` ends the chain.
* `Object.prototype` itself has `prototype: None`.

The two roots are reachable from JavaScript as `Object.prototype` and
`Array.prototype`. `compiler::call::compile_property_access` recognises those
member expressions and emits a call to `sealion_object_prototype` or
`sealion_array_prototype`, so both names evaluate to the same singleton pointer
that `new_object`/`new_array` use.

## 4. Property access

### 4.1 Reads (`[[Get]]`)

`sealion_object_get(object, key)` evaluates, in order:

1. `string.length` for a string receiver — strings are primitives but answer
   `length` through the same code path.
2. `chain_get`: walk `object.prototype` until a value is found or the chain ends.
   At each object, `own_property` looks at:
   * `elements`: `"length"` and canonical index keys (`"0"`, not `"01"`). An
     index at or past the length is **absent**, so the walk continues into the
     prototype.
   * `entries`: the map `size`.
   * `properties`: the ordinary name/value table.
3. `__proto__`: return the receiver's prototype object, or `null`.
4. Virtual methods, in this order: `Map` methods, array methods, then
   `Object.prototype` methods. String methods are the last fallback for a string
   receiver. These are described in section 6.

A receiver that is not an object is total: the host helpers return `undefined`
instead of throwing.

### 4.2 Writes (`[[Set]]`)

`sealion_object_set(object, key, value)`:

1. `key === "__proto__"` replaces the prototype (section 5) and returns `value`.
2. `arrays::set_element` handles `length` and index writes.
3. Otherwise it updates an existing own property or appends a new one.

Writes never traverse the prototype chain: assigning to an inherited name
creates an own property on the receiver, shadowing the inherited value. There
are no setters or writable flags, so no `TypeError` can arise.

### 4.3 `in` and `delete`

* `sealion_object_has` walks the prototype chain (`chain_get(...).is_some()`)
  and also reports the virtual array/map/`Object.prototype` methods. `delete`
  only ever removes **own** properties, matching the specification: deleting an
  inherited property returns `true` and leaves the prototype untouched.

## 5. `__proto__`, `Object.create`, `setPrototypeOf`

`Object.setPrototypeOf(object, proto)` and `object.__proto__ = proto` funnel
through `set_prototype`:

* an object value replaces `prototype`,
* `null` clears it (`prototype: None`),
* any other value is ignored,
* a cycle is refused. `reaches(proto, object)` walks the candidate chain; if it
  reaches the receiver the assignment is a no-op instead of a `TypeError`.

`Object.getPrototypeOf` returns the prototype object or `null`; a non-object
receiver yields `undefined`. `Object.create(proto)` allocates a plain object
chained to `proto` (or to nothing for `null`).

`__proto__` is handled inline in `get`/`set` rather than as a real accessor on
`Object.prototype`. Consequently `Object.create(null).__proto__` still reads as
`null`, whereas the specification would report `undefined`; `Object.create`
remains the reliable API.

Every lookup and cycle check is bounded by `MAX_PROTOTYPE_DEPTH = 1000`, so a
corrupt chain cannot hang the runtime.

## 6. Built-in methods are virtual, not prototype properties

Arrays, maps and strings answer their methods from native tables keyed by name
(`arrays::method_property`, `maps::method_property`,
`strings::method_property`). Each hit allocates a two-word closure record:

```text
[ entry point , receiver ]     tagged as TAG_FUNCTION
```

The record uses the ordinary JavaScript calling convention `(env, args, callee,
this)`, so `var push = a.push` keeps working and `this` is the receiver bound at
the moment the method was read. These virtual methods are consulted **after**
the prototype chain, which means a user assignment to `Array.prototype.push`
creates a real own property on the singleton and shadows the built-in.

`Object.prototype`'s own methods are implemented the same way for every object
whose chain includes `Object.prototype`:

| Method | Behaviour |
| --- | --- |
| `hasOwnProperty(key)` | own data property only |
| `propertyIsEnumerable(key)` | same test, since every own data property is enumerable |
| `isPrototypeOf(value)` | walks `value`'s chain looking for the receiver |
| `toString()` | `"[object Object]"` (arrays override it with `join`) |
| `valueOf()` | the receiver |

`Object.create(null)` removes `Object.prototype` from the chain, so it has none
of these methods (`o.toString === undefined`).

Virtual built-ins are invisible to own-key enumeration: `Object.hasOwn(
Array.prototype, "push")` is `false` even though `[].push` resolves.

## 7. Enumeration

`Object.keys`, `Object.values` and `Object.entries` enumerate own properties
only:

* array indices first, in order, then named properties in insertion order
  (`own_keys`),
* inherited properties are never included,
* `length`/`size` are not enumerable keys.

`Object.keys` returns a real array (chained to `Array.prototype`), so it supports
`.length`, `.map`, and so on.

## 8. Object literals

`compiler::object.rs` compiles `{ ... }` by calling `sealion_object_new` and then
`sealion_object_set` for each property in source order, so insertion order and
evaluation order both match the specification. `{__proto__: proto}` needs no
special case at the AST level: it compiles to a `set` on the `"__proto__"` key,
which `sealion_object_set` interprets as a prototype assignment. Object spread
and getter/setter literals are still rejected at compile time.

## 9. Registering a new helper

Adding an object helper takes four edits:

1. Write the `extern "C" fn sealion_…` in `jit/objects.rs` and add it to
   `objects::lookup`.
2. Declare the symbol with the right signature in `builtins::Builtins::new`
   (inside the `dynamic` block) so the AOT backend does not emit it.
3. Expose the member name through `OBJECT_INLINE` (or `ARRAY_INLINE`) so that
   `builtin_member` recognises `Object.name`.
4. Add a match arm to `builtins::dispatch::call` that calls the `FuncRef`.

## 10. Known gaps

* No property descriptors, writable/enumerable/configurable flags, accessors,
  getters or setters (except the built-in `__proto__` shorthand).
* No `Function.prototype`, no `new` on user functions: user functions have no
  `.prototype` and cannot be constructors.
* Built-in array/map/string methods live in native tables, so they are not
  visible as own properties of `Array.prototype`/`Map.prototype`.
* No garbage collection; prototype singletons and every allocated object live
  forever.
* `Object.create(proto)` returns `undefined` for a non-object, non-`null`
  argument instead of throwing a `TypeError`; prototype cycles are refused
  silently for the same reason — the runtime has no exception path out of a host
  helper yet.
