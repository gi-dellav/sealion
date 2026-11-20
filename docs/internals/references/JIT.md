# JIT.md — The In-Process JIT Step

This document covers the **JIT finishing step**: what changes when a
Cranelift-based AOT compiler is used inside the current process instead of
writing an artifact to disk. It assumes the front end, Cranelift translation,
per-function code generation, object emission, and metadata construction already
work. It does not re-explain those steps.

For the host-call ABI and function references, see `RUNTIME.md`. For the artifact
format, metadata schema, compatibility rules, and loader validation, see
`LOADER.md`.

---

## 1. JIT is the same compiler with a different finishing path

A useful mental model is:

```text
AOT:  input -> translate -> compile -> link -> object bytes -> persist
JIT:  input -> translate -> compile -> link -> object bytes -> executable memory
                                                               -> publish
                                                               -> register
```

The compiler work is identical. The difference is what implements the
`FinishedObject` contract:

| Concern | AOT | JIT |
|---|---|---|
| Object sink | `Vec<u8>` | Owned executable-capable `MmapVec` |
| Output | Serialized bytes | In-memory image |
| Compatibility check | At load time | Before compilation, against the native host |
| Publication | Later, in the loader | Immediately after object emission |
| Metadata | Decoded from the artifact | Reused from the compile step when possible |
| Module construction | After load | Immediately after publish |
| Cross-compilation | Allowed | Not allowed |
| Persistence | Native output | Optional (`Module::serialize`) |
| Cache | Artifact bytes | Same artifact bytes |

Both paths can use the same artifact format. That is what lets the reference
engine cache JIT output as if it were AOT output, and lets a JIT-compiled module
be serialized for a later process. In the reference engine, `Module::serialize`
simply returns the bytes of the in-memory image that was just published, so a
JIT-compiled unit can become the input to the AOT loader described in
`LOADER.md`.

The important design decision is:

> **JIT adds an executable-memory sink and an immediate publish/register step; it
> does not add a new compiler or a new artifact format.**

---

## 2. The in-process pipeline

The reference engine's JIT entry point is the same `CodeBuilder` used elsewhere.
A normal `Module::new` looks like this:

```rust
// Adapted from the module constructor.
pub fn new(engine: &Engine, input: impl AsRef<[u8]>) -> Result<Module> {
    CodeBuilder::new(engine)
        .input_bytes(input.as_ref(), None)?
        .compile_module()
}
```

The JIT-specific work happens in `compile_module` and `compile_cached`:

```rust
// Adapted from the in-process compile path.
pub fn compile_module(&self) -> Result<Module> {
    let custom_alignment = self.custom_alignment();

    let (code, info_and_types) = self.compile_cached(
        |engine, input, dwarf, state| {
            build_module_artifacts(engine, input, dwarf, state)
        },
        &custom_alignment,
    )?;

    Module::from_parts(self.engine, code, info_and_types)
}
```

The cached path has two outcomes:

```rust
// Adapted from the cached compile path.
self.engine
    .check_compatible_with_native_host()
    .context("compilation settings are not compatible with the native host")?;

let (code, info_and_types) = ModuleCacheEntry::new("runtime", engine.cache())
    .get_data_raw(
        &state,
        // Miss: compile and publish into executable memory.
        |state| {
            let (mmap, info) = build_artifacts(engine, input, dwarf, state)?;
            let code = publish_mmap(engine, mmap)?;
            Ok((code, info))
        },
        // Store artifact bytes in the cache.
        |_, (code, _)| Some(code.mmap().to_vec()),
        // Hit: load the artifact bytes exactly like the AOT loader.
        |engine, bytes| {
            let code = engine.load_code_bytes(&bytes, kind).ok()?;
            Some((code, None))
        },
    )?;
```

Notice the asymmetry:

- On a **cache miss**, the JIT compiles, publishes, and returns the in-memory
  `CodeMemory` plus the metadata it already has.
- On a **cache hit**, the JIT behaves like an AOT loader: it validates and loads
  artifact bytes. This is safe because the cache stores the same artifact format.
- A failed cache load becomes a miss and recompiles.

The JIT path also calls `check_compatible_with_native_host` before compiling. An
AOT compiler may cross-compile; an in-process JIT cannot.

### 2.1 What the JIT does not do

The reference engine's JIT is deliberately simple:

- It compiles the **entire module** synchronously before returning.
- There is **no tiering** and no on-stack replacement.
- There is **no lazy per-function compilation**; all functions and required
  trampolines are compiled up front.
- There is **no dynamic linking of host symbols into code**; host calls go
  through runtime tables (see `RUNTIME.md`).
- After `Module::new` returns, no further compilation happens for that module.

This is a good default for a first JIT. Tiering, background compilation, lazy
functions, and code replacement are separate features that require versioned
code, safepoints, stack maps, and a deoptimization or patching story.

---

## 3. Emitting directly into executable-capable memory

### 3.1 The sink

The AOT path emits the object into a growable `Vec<u8>`. The JIT path emits it
into an owned memory mapping that can later be made executable.

```rust
// Adapted from the JIT object sink.
pub(crate) struct MmapVecWrapper(pub MmapVec);

impl FinishedObject for MmapVecWrapper {
    type State = CustomAlignment;

    fn finish_object(
        obj: ObjectBuilder<'_>,
        align: &CustomAlignment,
    ) -> Result<Self> {
        let mut result = ObjectMmap::default();
        result.alignment = align.alignment;
        obj.finish(&mut result)?;

        let mmap = result.mmap.expect("object writer did not reserve");
        assert_eq!(mmap.len(), result.len);
        Ok(MmapVecWrapper(mmap))
    }
}
```

`ObjectBuilder::finish` ultimately calls the `object` crate's `Object::emit`,
which only needs a `WritableBuffer`. The JIT sink implements that trait on top of
`MmapVec`:

```rust
// Adapted from the JIT object sink.
impl WritableBuffer for ObjectMmap {
    fn reserve(&mut self, additional: u64) -> Result<(), ()> {
        assert!(self.mmap.is_none(), "cannot reserve twice");
        self.mmap = Some(MmapVec::with_capacity_and_alignment(
            additional as usize,
            self.alignment,
        ).map_err(|_| ())?);
        Ok(())
    }

    fn write_zeros(&mut self, additional: u64) {
        self.len += additional as usize;
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        let mmap = self.mmap.as_mut().expect("write before reserve");
        unsafe {
            mmap.as_mut_slice()[self.len..][..bytes.len()].copy_from_slice(bytes);
        }
        self.len += bytes.len();
    }
}
```

The mapping is zero-initialized, so `write_zeros` only advances the cursor; it does
not need to write bytes. Three properties matter:

1. **Reserve once.** The object writer asks for the total size up front and does
   not grow afterward. The JIT allocates exactly that size.
2. **No intermediate copy.** The object bytes are written directly into the
   mapping that will become executable. There is no `Vec<u8> -> mmap` copy.
3. **Alignment is explicit.** The sink carries the engine's required code
   alignment. This matters for platforms where code must be aligned to a page or
   to a custom executable-memory granularity.

### 3.2 Why not compile to a `Vec` and copy?

Copying is acceptable for small modules, but it has two costs:

- an extra allocation and full-image copy at startup,
- a window where two mutable copies of the image exist, which complicates W^X
  and instruction-cache reasoning.

The direct-to-mmap sink avoids both. It also makes the JIT and AOT paths share
the exact same object writer; only the `FinishedObject` implementation differs.

### 3.3 Custom executable memory

Some platforms cannot allocate RW memory and later `mprotect` it to RX, or the
embedder wants to control executable memory itself. The reference engine exposes
a `CustomCodeMemory` hook:

```rust
// Adapted from the custom code-memory interface.
pub trait CustomCodeMemory: Send + Sync {
    fn required_alignment(&self) -> usize;
    fn publish_executable(&self, ptr: *const u8, len: usize) -> Result<()>;
    fn unpublish_executable(&self, ptr: *const u8, len: usize) -> Result<()>;
}
```

When configured, `CodeMemory::publish` calls the hook instead of `mprotect`, and
`Drop` calls `unpublish_executable` to restore the original permissions. The
loader/engine also uses `required_alignment()` when allocating the image.

If you add a JIT to an AOT compiler, this hook is worth designing in from the
start. It separates "where executable memory comes from" from "how the object is
written," which is useful for embedded targets, sandboxed runtimes, and
platforms with unusual W^X rules.

---

## 4. From image to executable code

After the object is emitted, the JIT does exactly what the AOT loader does, minus
the file/serialization step:

```rust
// Adapted from the JIT publish helper.
fn publish_mmap(engine: &Engine, mmap: MmapVec) -> Result<Arc<CodeMemory>> {
    let mut code = CodeMemory::new(engine, mmap)?;
    code.publish()?;
    Ok(Arc::new(code))
}
```

`CodeMemory::new` parses the ELF section table and records section ranges. For the
full structural validation list, see `LOADER.md` §6.4. The JIT should not skip
these checks even though it just produced the image: they are cheap relative to
compilation, and they catch internal inconsistencies before code becomes
executable.

`CodeMemory::publish` performs the actual transition:

1. Freeze the whole image read-only.
2. Make only the text range executable, applying branch-protection flags where
   supported.
3. Flush the instruction cache.
4. Register unwind information.
5. Register a native debug image if enabled.

For the full publication logic and platform details, see `LOADER.md` §6.5. The
JIT-specific point is that **publication happens during `Module::new`**, not at
some later load step.

### 4.1 Unpublishing and patching

The reference engine supports temporary unpublishing for controlled code edits,
such as debugger breakpoints:

```rust
// Adapted from code-memory publication.
pub fn unpublish(&mut self) -> Result<()> {
    assert!(self.published);
    self.published = false;

    if self.text().is_empty() {
        return Ok(());
    }

    self.mmap.make_readwrite(0..self.mmap.len())?;
    // Deliberately keep unwind registration in place.
    Ok(())
}

pub fn text_mut(&mut self) -> &mut [u8] {
    assert!(!self.published);
    unsafe { &mut self.mmap.as_mut_slice()[self.text.clone()] }
}
```

This is not a general hot-swap mechanism. The safety conditions are strict:

- no active stack frame may still be executing in the region being modified,
- any PC values into the region must remain valid,
- the overall image size cannot change,
- the edit must preserve instruction validity at patch points.

For a normal JIT, publication is one-way until the code is unloaded. Treat
unpublish/patch as a specialized debugger feature, not as a tiering mechanism.

---

## 5. Module construction and code registration

### 5.1 Reuse the metadata you just built

The JIT does not need to decode its own metadata section. `build_module_artifacts`
already returns `(CompiledModuleInfo, CompiledFunctionsTable, ModuleTypes)`, and
the JIT passes them straight to module construction:

```rust
// Adapted from module construction.
pub(crate) fn from_parts(
    engine: &Engine,
    code_memory: Arc<CodeMemory>,
    info_and_types: Option<(CompiledModuleInfo, CompiledFunctionsTable, ModuleTypes)>,
) -> Result<Self> {
    let (mut info, index, mut types) = match info_and_types {
        Some(values) => values,
        None => postcard::from_bytes(code_memory.module_info())?,
    };

    let signatures = engine
        .register_and_canonicalize_types(&mut types, core::iter::once(&mut info.module))?;

    let code = EngineCode::new(code_memory, signatures, types.into())?;
    let index = Arc::new(index);

    Module::from_parts_raw(engine, code, info, index, true)
}
```

On a cache hit, `info_and_types` is `None` and the metadata is decoded from the
artifact. On a JIT miss, it is `Some` and the JIT avoids a serialize/decode round
trip.

### 5.2 Register the code range for traps and backtraces

`EngineCode::new` registers the text range with the process-global code registry.
That registry is how signal handlers and backtrace code map a PC back to a code
image when there is no contextual store available.

```rust
// Adapted from engine-code construction.
pub fn new(
    mmap: Arc<CodeMemory>,
    signatures: TypeCollection,
    types: Types,
) -> Result<EngineCode, OutOfMemory> {
    crate::module::register_code(&mmap, mmap.raw_addr_range())?;
    Ok(EngineCode {
        original_code: mmap,
        signatures,
        types,
    })
}

impl Drop for EngineCode {
    fn drop(&mut self) {
        crate::module::unregister_code(self.original_code.raw_addr_range());
    }
}
```

The global registry is a `RwLock`-protected range map. Registration and
unregistration must be paired exactly. If registration fails, the code image must
be discarded rather than retained in a module; otherwise the process could
execute code that the trap machinery cannot identify. In the reference flow the
image has already been published at this point, so the `CodeMemory` is dropped,
which releases the mapping and its registrations.

### 5.3 Store registration

Code is shared at the engine/module level and registered with a store at
instantiation time. Normally the store uses the same `CodeMemory` as the module:

```rust
// Adapted from store-code construction.
pub fn new(engine: &Engine, engine_code: &Arc<EngineCode>) -> Result<Self> {
    #[cfg(feature = "debug")]
    let code = if engine.tunables().debug_guest {
        // Per-store copy so breakpoints can be patched independently.
        let mut private_copy = engine_code.original_code.deep_clone(engine)?;
        private_copy.publish()?;
        crate::module::register_code(
            &engine_code.original_code,
            private_copy.raw_addr_range(),
        )?;
        StoreCodeStorage::Private(Box::new(private_copy))
    } else {
        StoreCodeStorage::Shared(engine_code.original_code.clone())
    };

    #[cfg(not(feature = "debug"))]
    let code = StoreCodeStorage::Shared(engine_code.original_code.clone());

    Ok(StoreCode(code))
}
```

This is why the same module can run in many stores without duplicating machine
code. The exception is guest debugging, where a store may deliberately make a
private copy so it can patch breakpoints without affecting other stores.

For the runtime side of instantiation, including how host function references are
patched and copied into the instance context, see `RUNTIME.md` §5 and §6. In
particular, a newly JIT-compiled module can fill outstanding `compiled_call`
holes in host function references when it is registered.

---

## 6. Memory lifetime and unloading

The ownership chain is:

```text
Module
  -> Arc<EngineCode>
       -> Arc<CodeMemory>
            -> MmapVec (the actual allocation)
            -> unwind/debug registrations
```

Consequences:

- All stores and instances that use a module keep the module alive.
- The machine code stays at a stable address for as long as any user exists.
- Dropping the last `Module`/`EngineCode` unregisters the code range and releases
  the mapping.
- `StoreCode` may share that mapping or own a private copy.
- `Func` values hold a store reference; the store holds its module registry; the
  registry holds the module. Code cannot be freed while a callable is live.

`CodeMemory`'s `Drop` implementation releases unwind and debug registrations
before the underlying mapping is destroyed:

```rust
// Adapted from code-memory destruction.
impl Drop for CodeMemory {
    fn drop(&mut self) {
        if let Some(mem) = self.custom_code_memory.as_ref() {
            if self.published && self.needs_executable {
                let text = self.text();
                mem.unpublish_executable(text.as_ptr(), text.len())
                    .expect("executable memory unpublish failed");
            }
        }

        // These registrations implicitly refer to the mmap.
        let _ = self.unwind_registration.take();
        let _ = self.debug_registration.take();
    }
}
```

For a JIT, this lifetime discipline is as important as compilation itself. A
use-after-free here is not a normal memory bug; it is execution of unmapped or
reused pages.

---

## 7. Concurrency and parallelism

### 7.1 Parallel function compilation

The engine compiles independent functions and trampolines in parallel when
configured:

```rust
// Adapted from the engine's parallel helper.
pub(crate) fn run_maybe_parallel<A, B, E, F>(
    &self,
    input: Vec<A>,
    f: F,
) -> Result<Vec<B>, E>
where
    F: Fn(A) -> Result<B, E> + Send + Sync,
{
    if self.config().parallel_compilation {
        return input
            .into_par_iter()
            .map(|a| f(a))
            .collect::<Vec<Result<B, E>>>()
            .into_iter()
            .collect();
    }

    input.into_iter().map(|a| f(a)).collect()
}
```

Two details are worth copying:

- Results are materialized in input order before propagating an error, so the
  reported error is deterministic regardless of thread scheduling.
- The parallel path falls back to sequential compilation when the feature or
  configuration disables it.

After parallel compilation, **linking and object emission are single-threaded**
for a given module. The function outputs are sorted by stable key (see
`LOADER.md` §3.3), then appended to one text section. This avoids concurrent
mutation of the object writer and gives deterministic layout.

### 7.2 Concurrent modules

Different modules can be compiled and published concurrently. Each has its own
mapping and object writer. The shared state is:

- the engine's type/signature registry,
- the compilation cache,
- the process-global code range registry,
- the engine's profiling agent.

Those shared structures must be thread-safe. The reference engine uses locks or
atomic data structures around registration and cache access. Executable code
itself is immutable after publication, so execution does not need a lock.

### 7.3 Cache concurrency

A compilation cache must handle two threads compiling the same input at once.
The simple approach is:

- both compile,
- one wins the cache insert,
- the loser either keeps its result or discards it.

More elaborate implementations can use per-key locks or a "single flight" map.
Either way, a cache entry must be written atomically and must be revalidated on
read (see `LOADER.md` §9).

---

## 8. JIT configuration knobs

These are the settings most relevant to an in-process JIT:

| Setting | Why it matters for JIT |
|---|---|
| `strategy` | Selects the code generator (for example a fast baseline vs an optimizing backend) |
| `cranelift_opt_level` | Trade compile time for generated-code quality |
| `parallel_compilation` | Uses multiple threads for function compilation |
| `cache` | Reuses artifact bytes across processes or runs |
| `debug_info` | Emits native DWARF for JIT code |
| `debug_symbols` | Emits symbol names for profiling/debugging |
| `native_unwind_info` | Enables backtraces and unwinding through JIT frames |
| `profiler` | Registers JIT code with perf/jitdump/VTune/etc. |
| `with_custom_code_memory` | Supplies custom executable-memory management |
| `generate_address_map` | Records PC-to-source mappings for profiling/tracing |
| `guest_debug` | Enables per-store code copies and frame-state metadata |

Two JIT-specific rules:

1. **The target must match the native host.** `Module::new` checks this before
   compiling. Cross-compilation belongs to the AOT path.
2. **The cache is optional but should be validated.** A JIT cache hit is just an
   AOT load; if validation fails, fall back to compiling.

The reference engine also exposes a hash of the compile environment. It is useful
for deciding whether two engines are likely to produce/load the same artifacts,
but it is not a substitute for the artifact's own compatibility checks.

---

## 9. Debugging, profiling, and observability

A JIT has to make its generated code visible to the rest of the process.

### 9.1 Unwind and backtraces

`CodeMemory::publish` registers the unwind section with the platform unwinder.
Without this, a trap or panic cannot produce a correct backtrace through JIT
frames. The registration is held by `CodeMemory` and released on drop.

### 9.2 Native debug info

When native debug info is enabled, publication also registers a GDB/JIT image.
The reference engine currently copies the image for this registration, then hands
it to the platform debugger interface.

### 9.3 Profiling agents

The engine holds a `ProfilingAgent`. When a module is constructed, the profiler is
told about the code image and can map text addresses back to function names:

```rust
// Adapted from profiler registration.
profiler.register_module(self.engine_code.image(), &|addr| {
    let key = self.func_by_text_offset(addr)?;
    let name = self.func_name(key)?;
    Some(demangle_function_name(name))
});
```

This is how JIT code shows up in `perf`, VTune, jitdump, or similar tools.

### 9.4 PC lookup and traps

The global code registry and the module's function table provide two different
lookups:

- **global registry:** PC -> code image, usable from a signal handler without a
  store;
- **module function table:** text offset -> `FuncKey`, used for traps,
  backtraces, and profiling names.

Registration must happen before the first execution and unregistration must
happen after the last execution. The `Arc` ownership chain enforces the latter.

### 9.5 Debugging and patching

With guest debugging enabled, each store gets a private copy of the code image so
breakpoints can be patched without affecting other stores. The patching path uses
`unpublish`, writes the modified instruction bytes, and `publish` again. This is
a specialized feature with strict safety requirements; it is not a general JIT
code-replacement mechanism.

---

## 10. Failure handling and security

### 10.1 Compile failure

If compilation or object emission fails before publication:

- the `MmapVec` is dropped,
- no code is registered,
- no executable permission is applied.

If `CodeMemory::new` or `publish` fails:

- the partially constructed `CodeMemory` is dropped,
- custom executable memory is unpublished,
- unwind/debug registrations are released.

The important invariants are: **do not make code executable until the image is
structurally valid, and do not retain or register executable code unless
registration succeeds.**

### 10.2 W^X

The JIT must never leave a page both writable and executable. The reference
sequence is:

1. allocate writable memory,
2. write the image,
3. make the image read-only,
4. make only the text range executable,
5. flush the instruction cache.

If your platform cannot do this with `mprotect`, use a custom code-memory hook
that provides an equivalent W^X transition.

### 10.3 Instruction cache coherence

On architectures with non-coherent instruction caches, publishing executable code
without flushing the cache can execute stale instructions. The reference engine
routes this through the platform's executable-memory implementation. Do not skip
it, even if the code was just written by the current thread.

### 10.4 Trust boundary

JIT input is usually trusted code, but it may still be attacker-influenced in a
plugin or multi-tenant system. The JIT compiler must treat translation and code
generation as a security boundary if the input is not fully trusted. Artifact
loading is a separate trust boundary; see `LOADER.md` §10.

### 10.5 Executable memory lifetime

Never free or reuse a code mapping while a thread could still be executing in it.
Use reference counting, join/quiesce execution, or a safepoint mechanism. The
reference engine relies on the `Arc` chain from stores/instances to the module and
code memory.

---

## 11. Adding a JIT to an existing AOT compiler

If your AOT compiler already produces an object image, the JIT port is mostly
plumbing:

### 11.1 Minimal changes

1. Make the object sink generic over `FinishedObject` (or equivalent).
2. Add a sink that writes into an executable-capable memory mapping.
3. Add an in-process compile entry point that:
   - checks the target is the native host,
   - translates and compiles,
   - emits into the memory sink,
   - validates the image,
   - publishes it,
   - constructs the module with in-memory metadata.
4. Register the text range with the trap/backtrace registry.
5. Wire module/store registration so host function references can be patched.
6. Add lifetime management (module -> code -> mapping).
7. Add optional cache, profiler, and debug hooks.

### 11.2 What to reuse unchanged

- Cranelift translation and per-function compilation.
- Function keys and deterministic output ordering.
- Relocation resolution and object emission.
- Artifact metadata schema.
- Compatibility metadata and flag checks.
- Host-call ABI and trampoline generation (`RUNTIME.md`).
- Artifact section layout and loader validation (`LOADER.md`).

### 11.3 What to add or change

| Area | AOT behavior | JIT behavior |
|---|---|---|
| Target check | Optional at compile time | Required before compile |
| Output sink | Growable byte vector | Executable-capable mapping |
| Metadata | Serialize then decode on load | Reuse in-memory metadata |
| Publication | Deferred to loader | Immediate |
| Registration | At load/instantiate | At compile/module construction |
| Cache | Optional | Optional, same bytes |
| Profiling | At load | At module construction |
| Debug copies | At load/store registration | At store registration |
| Unloading | When module drops | When module drops; must unregister |

### 11.4 Tests worth writing

- **Equivalence:** compile the same input AOT and JIT; compare observable
  behavior, function ranges, relocations, and metadata.
- **W^X:** after publication, text is executable and not writable.
- **I-cache:** execute newly JIT-compiled code immediately on targets with
  non-coherent caches.
- **Traps/backtraces:** traps in JIT code produce correct trap codes and
  backtraces.
- **Host calls:** JIT code calls host functions with the array/native ABI;
  traps propagate correctly (`RUNTIME.md`).
- **Hole filling:** host functions created before and after module compilation
  become callable; indirect calls type-check.
- **Concurrency:** compile/publish/drop modules from multiple threads while
  other threads execute code.
- **Unload:** dropping the last module/instance unregisters the code range and
  releases executable memory.
- **Cache:** miss, hit, corrupt hit, incompatible target, concurrent insert.
- **OOM:** allocation failure during object emission/publication leaves no
  executable artifacts behind.
- **Debug:** breakpoint patching on a per-store copy does not affect other
  stores.

---

## 12. Reference source map

Paths beginning with `src/` are relative to the reference runtime crate;
`crates/cranelift` and `crates/environ` are the compiler and environment crates.

| Topic | Location |
|---|---|
| JIT entry point and in-process module creation | `src/runtime/module.rs`, `src/compile/runtime.rs` |
| Cached compile, hit/miss behavior, publish helper | `src/compile/runtime.rs` |
| Object writer and metadata assembly | `src/compile.rs`, `crates/environ/src/compile/module_artifacts.rs` |
| Executable-capable object sink | `src/compile/runtime.rs` |
| Mmap allocation, alignment, file mapping | `src/runtime/vm/mmap_vec.rs` |
| Section parsing, publication, W^X, unwind/debug registration | `src/runtime/code_memory.rs` |
| Custom executable-memory hook | `src/runtime/code_memory.rs`, `src/config.rs` |
| Engine-code creation, global code registration, drop | `src/runtime/code.rs` |
| Module construction and metadata reuse | `src/runtime/module.rs` |
| Store-local code handle and private debug copies | `src/runtime/code.rs` |
| Parallel compilation helper | `src/engine.rs` |
| Compilation cache key | `src/compile/code_builder.rs` |
| Profiling registration | `src/runtime/instantiate.rs`, `src/profiling_agent.rs` |
| Runtime host-call ABI and trampolines | `src/runtime/func.rs`, `crates/cranelift/src/compiler.rs` |
| Trap/backtrace code registry | `src/runtime/module/registry.rs` |
