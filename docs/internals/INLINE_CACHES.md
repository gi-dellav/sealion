# Constant keys, object shapes and inline caches

This document describes the three optimisations that keep property access in
the dynamic (JIT) runtime off the slow path:

1. **Constant keys.** A source-level property name is interned once per module
   instead of being re-decoded at every evaluation.
2. **Shapes and slots.** An object's named properties live in a slot array
   described by a shared, canonical *shape*, so a read is a pointer comparison
   and an indexed load instead of a scan of name/value pairs.
3. **Inline caches.** A statically named access site remembers the shape it last
   served and the slot that shape gave the key, and serves a same-shape receiver
   inline.

Together with allocation-free method-name dispatch (section 6) these turned the
`perfbench/objprops` case from ~102x the C time to ~27x, and `perfbench/charmix`
from ~48x to ~28x.

The code lives in `sealion-engine/src/compiler/object.rs` (codegen),
`sealion-engine/src/jit/objects.rs` (the object model) and
`sealion-engine/src/jit/strings.rs` (key interning). The object model itself is
described in [`OBJECTS.md`](OBJECTS.md); the value tags in
[`VALUE.md`](VALUE.md).

## 1. Why the old path was slow

Every access was an unconditional host call that:

* rebuilt the key with `sealion_string_from_utf8` — a UTF-8 validation plus a
  hash probe of the static string table, per access;
* cloned the `JsString` key (a refcount bump) to own it for the duration;
* scanned `Vec<(JsString, f64)>` comparing full string contents, entry by entry;
* and on a miss, decoded the key into a fresh `std::String` for each method
  table it tried.

A loop body of the form `o.a = o.a + 1` therefore allocated, hashed and compared
strings on every iteration, even though neither the receiver's layout nor the
key changed.

## 2. Constant keys

Each distinct string literal of a module gets two objects in the module's data
section (see `engine.rs`):

| data | contents |
| --- | --- |
| `__string_N` | the literal's bytes, for `sealion_string_from_utf8` |
| `__string_key_N` | one writable word, zero until the literal is resolved |

`compiler::expr::compile_string_data` emits

```text
key = call sealion_string_key(slot, bytes, len)
```

and the host helper (`jit::strings::sealion_string_key`) returns the word when it
is non-zero, otherwise interns the text once, stores the tagged value in the
word and returns it. The word lives in the artifact, is rebased by the loader
like any other data, and stays valid across processes, so
`--artifact`/`--run-artifact` behaves the same as an in-process JIT run.

The second and every later evaluation of a literal — including every static
property key — is one indirect load and no hashing, and a name that is *not* in
the static string table (`"b"`, for instance) is allocated once per module
instead of once per access.

## 3. Shapes and slots

```rust
#[repr(C)]
pub(super) struct JsObject {
    shape: *const Shape,      // offset 0
    slots: Slots,             // offset 8 (see below)
    elements: Option<Vec<f64>>,
    entries: Option<Vec<(f64, f64)>>,
    date: Option<f64>,
    prototype: Option<*mut JsObject>,
    owner: Option<f64>,
    hidden: Vec<JsString>,
}

#[repr(C)]
pub(super) struct Slots { data: *mut f64, len: usize, capacity: usize }

pub(super) struct Shape { names: Vec<JsString>, transitions: Mutex<...> }
```

* `Shape::names[i]` is the name of the own property stored in `slots[i]`; the
  two arrays are always the same length and always in insertion order, so
  enumeration order is unchanged from the earlier name/value vector.
* Shapes are **canonical**: adding a property walks a memoised transition table,
  so two objects built by the same sequence of adds share one `Shape` pointer.
  That is what lets a cache hit mean "this receiver has this key at this slot".
* Shapes are **leaked** and immutable after construction; only the transition
  table is mutable, and it is behind a `Mutex` because shapes are shared across
  the runtime's threads.
* `Slots` is a raw array rather than a `Vec` on purpose: a compiled access site
  loads its data pointer at a fixed offset, and `Vec` hides its pointer behind a
  layout the standard library may change (`#[repr(C)]` plus a `const` assertion
  in `jit/objects.rs` pins the two offsets that codegen relies on).

Removing a property builds a fresh, uncached shape with the later slots shifted
down. A delete therefore invalidates every cache entry that named the old shape —
the object's shape pointer changes, so the next access misses and refills.

## 4. The inline cache

The cells live in one writable data object, `sealion_inline_caches`, declared per
module. The pool is sized from the module — `compiler::inline_cache_slots` takes
eight cells per distinct string literal, clamped to 256..=4096 — so a small
program does not carry a large zeroed data section in its artifact. Each
statically named access site claims one cell at compile time:

```text
cell = [ shape: *const Shape, index: u64 ]
```

For `o.a` as a *read* (`compiler::object::get_named`):

```text
    if (tag(o) != TAG_OBJECT) goto slow
    shape  = load(o + OBJECT_SHAPE_OFFSET)
    cached = load(cell)
    if (shape != cached) goto slow
    values = load(o + OBJECT_SLOTS_OFFSET)
    index  = load(cell + 8)
    v      = load(values + 8 * index)
    goto join(v)
slow:
    k = call sealion_string_key(...)            # section 2
    v = call sealion_object_get_cached(o, k, cell)
join(v):
```

A write is the same guard with a store and `sealion_object_set_cached`. Two
details keep this correct without a runtime check on the fast path:

* the tag check keeps strings, functions and plain numbers out of the shape
  load;
* a cell is only ever filled by a host helper that found the key in the
  receiver's **own slots**, so a hit needs no prototype, element or method-table
  lookup — and the shape's `names`/`slots` invariant guarantees the index is in
  range.

The host helpers (`sealion_object_get_cached`, `sealion_object_set_cached`)
otherwise run the ordinary `[[Get]]`/`[[Set]]` path, then call `fill_cache` when
`slot_index` reports an own slot. Array elements, `length`, map `size`, virtual
methods and prototype hits never fill the cache, so those keys simply stay on the
call path forever — correct, just not cached. A computed key (`o[k]`) has no
cell: it is only known at run time, so it always takes the call path.

When a module has more statically named sites than the pool has cells, the extra
sites are compiled as plain calls; nothing else changes.
`accesses_past_the_inline_cache_pool_still_work` covers that case.

## 5. Invariants worth remembering

* `shape.names.len() == slots.len` for every object, at all times. All mutation
  goes through `JsObject::set_named` / `JsObject::remove_named`.
* `shape` is never null and never reused for a different name list taken out of
  insertion order.
* A cache cell is written only by the helper for the site that owns it, and only
  with `(receiver.shape, own slot index)`.
* Shapes and their name lists outlive every object that points at them.

## 6. Method names without decoding

`String`, `Array`, `Map`, `Object.prototype`, `Function.prototype` and `Date`
answer method names from native tables. The name used to be decoded with
`to_std_string_lossy()`, which allocates a `std::String` on every *miss*. It is
now matched as a byte slice:

```rust
pub(super) fn with_ascii_key<R>(key: &JsString, f: impl FnOnce(&[u8]) -> R) -> Option<R>
```

which borrows the bytes of a Latin1 key, copies the code units of a short UTF-16
key into a stack buffer, and returns `None` for anything that cannot name a
method (non-ASCII, or longer than the longest method name). Each table is then a
`match` over byte-string literals, which the compiler lowers to a length check
and a handful of byte comparisons.

`arrays::index_of_key` — the canonical-index parser, called for every array read
— parses the key's own bytes instead of decoding it into a `String`, and
`dates::method_property` checks the receiver *before* looking at the key, so a
non-`Date` no longer pays for the key at all.

## 7. Where to look, and what is tested

| Topic | Source |
| --- | --- |
| Constant-key cache | `jit/strings.rs:sealion_string_key`, `compiler/expr.rs:compile_string_data` |
| Shape, slots, transitions | `jit/objects.rs` (`Shape`, `Slots`, `JsObject`) |
| Inline-cache codegen | `compiler/object.rs:get_named`, `set_named` |
| Cache pool | `compiler/mod.rs:INLINE_CACHE_SLOTS`, `engine.rs` |
| Cached host path | `jit/objects.rs:sealion_object_get_cached`, `sealion_object_set_cached` |
| Byte-keyed method tables | `jit/strings.rs:with_ascii_key`, `jit/{arrays,maps,objects,functions,dates}.rs` |

Unit tests in `jit/objects.rs` pin the layout offsets that codegen depends on,
the slot array's growth and compaction, shape canonicalisation and slot shifting
on delete. Behavioural coverage lives in `sealion-engine/tests/jit.rs`:

- `inline_cached_property_accesses_stay_correct` — a site that alternates shapes,
  a cached own property followed by a prototype hit, growth past the initial slot
  capacity, `__proto__`, arrays and plain objects at one site, compound
  assignment, and a member loop head.
- `method_names_are_matched_without_decoding_the_key` — a key built from code
  units, a non-ASCII key, a non-canonical index (`"01"`) and `a[0]`/`a["0"]`.
