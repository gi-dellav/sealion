# LOADER.md — Precompilation and Artifact Loading for a Cranelift Runtime

This document distills the ahead-of-time compilation and loading design of a
production Cranelift-based engine into a reusable blueprint. Snippets are adapted
and identifiers are normalized. The reference implementation uses ELF as its
container; the section names below are conceptual equivalents, with the
reference project's vendor prefix omitted. Upstream locations are listed in the
source map at the end.

The central idea is simple:

> Compile once into a **self-contained executable image plus metadata**, persist
> that image, and at load time map it, validate its compatibility, make it
> executable, decode its metadata, and register it. No parser, translator, or
> code generator is needed at load time.

---

## 1. Three related workflows

The reference engine exposes three ways to get a serialized artifact:

| Workflow | What it does | Uses compilation cache? |
|---|---|---|
| `Engine::precompile_module(input)` | Compiles input directly to artifact bytes | No |
| `Module::serialize()` | Serializes an already-compiled module's image | No |
| `Module::new(input)` | Compiles normally; may populate/use cache | Yes |

All three produce the same artifact format. The difference is only where the
image comes from.

A useful distinction:

- **Compile-time precompilation** is for build pipelines and cross-compilation.
  It may run on a machine that is not the deployment host.
- **Serialization after compilation** is for programs that both instantiate and
  persist a compiled unit, avoiding a second compilation.
- **Compilation caching** is an optimization for `Module::new`; it stores the
  same artifact bytes under a key derived from the engine configuration and the
  input.

Loading is deliberately stricter than compiling: a precompiled artifact may be
produced for another target, but it can only be loaded and executed by an engine
whose target and ABI-affecting settings are compatible with the host.

---

## 2. Pipeline at a glance

```text
                       compile side
input bytes
    |
    v
parse + validate + translate --------------------------> module metadata
    |                                                       |
    v                                                       |
per-function Cranelift IR                                   |
    |                                                       |
    v                                                       |
compile in parallel (optional inlining)                     |
    |                                                       |
    v                                                       |
pre-link: stable order, FuncKey -> index, reloc targets     |
    |                                                       |
    v                                                       |
append functions to ELF text section                        |
    |                                                       |
    +--> resolve intra-image relocations now                |
    +--> emit unwind/debug/trap/stack-map sections          |
    |                                                       |
    v                                                       v
serialize metadata into ELF section <-----------------------+
    |
    v
artifact bytes / owned mmap


                       load side
artifact bytes / file / external memory
    |
    v
copy or map into MmapVec
    |
    v
check engine/host compatibility
    |
    v
check artifact header, version, engine metadata
    |
    v
parse ELF sections, validate alignment and relocation policy
    |
    v
publish: read-only + executable + unwind registration
    |
    v
decode metadata, register types, build module object
    |
    v
register code in a store at instantiation time
    |
    v
look up function ranges from the serialized function table
```

---

## 3. Compile side

### 3.1 Engine configuration is part of the artifact

The engine owns the target triple, compiler flags, ISA flags, runtime tunables,
and feature bits. These affect code generation and must be recorded in the
artifact, because loading must reject an image built with incompatible settings.

```rust
// Adapted from the engine/compiler wiring.
struct EngineInner {
    config: Config,
    features: Features,
    tunables: Tunables,
    compiler: Option<Box<dyn Compiler>>,
    // ...
}

fn build_compiler(config: &Config, tunables: &mut Tunables)
    -> Result<(Config, Box<dyn Compiler>)>
{
    // Choose the Cranelift backend, apply required settings, and map
    // configuration onto compiler flags.
    // ...
}
```

The compiler handle is optional in engines that only load precompiled artifacts;
it is required for `precompile_module` and `Module::new`.

### 3.2 Translate once, collect bodies, compile them

The core build function is deliberately linear. It translates and validates the
input, extracts the function bodies, compiles them, pre-links them, emits the
object, and serializes metadata.

```rust
// Adapted from the module artifact builder.
pub(crate) fn build_module_artifacts<T: FinishedObject>(
    engine: &Engine,
    input: &[u8],
    dwarf_package: Option<&[u8]>,
    obj_state: &T::State,
) -> Result<(
    T,
    Option<(CompiledModuleInfo, CompiledFunctionsTable, ModuleTypes)>,
)> {
    let compiler = engine.try_compiler()?;
    let tunables = engine.tunables();

    // 1. Parse, validate, translate, and collect type information.
    let mut validator = Validator::new_with_features(engine.features());
    let mut types = ModuleTypesBuilder::new(&validator);
    let mut translation =
        Environment::new(tunables, &mut validator, &mut types).translate(input)?;
    prepare_translation(engine, compiler, &mut translation, &mut types);

    // 2. Take the function bodies out of the translation.
    let functions = mem::take(&mut translation.function_body_inputs);

    // 3. Build one compile job per function/trampoline and run them.
    let compile_inputs = CompileInputs::for_module(&types, &translation, functions);
    let unlinked = compile_inputs.compile(engine, &types)?;

    // 4. Flatten into final text-section order and record FuncKey -> index.
    let PreLinkOutput { compiled_funcs, indices, .. } = unlinked.pre_link();

    // 5. Create the object file and add engine metadata.
    let mut object = compiler.object(ObjectKind::Module)?;
    engine.append_compiler_info(&mut object)?;
    engine.append_bti(&mut object);

    // 6. Append code, resolve relocations, add data/unwind/debug sections.
    let (mut object, artifacts) = indices.link_and_append_code(
        object,
        engine,
        compiled_funcs,
        std::iter::once(translation).collect(),
        dwarf_package,
    )?;

    // 7. Serialize the runtime metadata section.
    let (info, index) = artifacts.unwrap_as_module_info();
    let types = types.finish();
    object.serialize_info(&(&info, &index, &types));

    // 8. Finish as Vec<u8> (precompile) or an owned mmap (normal compile).
    let result = T::finish_object(object, obj_state)?;
    Ok((result, Some((info, index, types))))
}
```

Two details are load-bearing:

- The function body inputs are **moved out** of the translation. Bodies are not
  serialized; only native code and metadata are.
- The metadata returned in memory is optional. On a fresh compile it can be
  passed directly to the module constructor; on a cache hit or deserialization it
  is decoded from the artifact's metadata section.

### 3.3 Stable function identity and deterministic order

Every compiled function/trampoline gets a `FuncKey`: a compact value identifying
its kind, namespace, and index. `FuncKey` is used for:

- relocations between compiled functions,
- the function-location table,
- reverse lookup from a program counter to a function,
- deterministic layout.

Compile jobs are parallel, but their outputs are sorted by `FuncKey` before
linking:

```rust
// Adapted from the compile driver.
let mut outputs = raw_outputs;
outputs.sort_unstable_by_key(|output| output.key);
Ok(UnlinkedCompileOutputs { outputs })
```

Prelinking then assigns each key its final index in the text section:

```rust
// Adapted from pre-link.
for output in self.outputs {
    let index = compiled_funcs.len();
    compiled_funcs.push((output.symbol, output.key, output.function.code));
    indices.indices.insert(output.key, index);
}
```

Deterministic order gives you:

- reproducible artifacts for identical inputs and settings,
- stable cache entries,
- binary-searchable function tables,
- predictable grouping of hot functions, cold trampolines, and builtins.

The reference engine groups functions by key kind, so all defined functions from
one module are adjacent and trampolines are not interleaved with hot code.

### 3.4 Parallel compilation and late-discovered dependencies

Function compilation is independent and may run in parallel. Some dependencies
are discovered only after initial compilation, for example builtin trampolines
referenced by a relocation. The driver collects those and compiles them in a
second pass before pre-linking.

If the compiler supports inlining, the driver first builds a call graph,
stratifies it, and compiles in an order that makes callees available to inlining
decisions. Inlining is an optimization; the artifact format does not depend on
it.

### 3.5 Resolve relocations during linking, not during loading

Cranelift emits machine relocations against `FuncKey` targets. At link time, the
object builder knows the final index of every function, so it can resolve all
intra-image relocations immediately:

```rust
// Adapted from the text-section builder.
pub fn append_func(
    &mut self,
    name: &str,
    compiled_func: &CompiledFunction,
    resolve_reloc_target: impl Fn(FuncKey) -> usize,
) -> (Option<SymbolId>, Range<u64>) {
    let body = compiled_func.buffer.data();
    let off = self.text.append(true, &body, compiled_func.alignment, &mut self.ctrl_plane);

    // ...
    for r in compiled_func.relocations() {
        let reloc_offset = off + u64::from(r.offset);
        let target = resolve_reloc_target(r.reloc_target);

        if self.text.resolve_reloc(reloc_offset, r.reloc, r.addend, target) {
            continue;
        }

        panic!("failed to resolve relocation: {r:?} -> {target}");
    }

    (symbol_id, off..off + body.len() as u64)
}
```

The text-section builder behaves like a tiny intra-object linker. It binds a
label for each function, records relocation uses, and emits the final machine
bytes. If a branch cannot reach its target, it inserts veneers or islands and
recomputes offsets.

The result is an artifact with **no load-time relocations**. This is a major
design win:

- the image can be mapped directly from disk and shared between processes,
- pages can remain clean and file-backed,
- loading is fast and predictable,
- W^X transitions are simpler,
- the text section can be made executable without modifying it.

The only tolerated relocations are in native debug sections, which are never
executed.

### 3.6 Write an object file, not an ad-hoc blob

The reference engine always emits ELF, even for targets whose native executable
format is different. ELF gives it a standard section table, alignment metadata,
symbols, and a well-tested writer through the `object` crate.

```rust
// Adapted from the object constructor.
fn object(&self, kind: ObjectKind) -> Result<Object<'static>> {
    let triple = self.triple();
    let (arch, flags) = match triple.architecture {
        X86_64 => (Architecture::X86_64, FileFlags(0)),
        Aarch64(_) => (Architecture::Aarch64, FileFlags(0)),
        // ... other supported targets ...
        _ => bail!("unsupported target architecture"),
    };

    let mut obj = Object::new(
        BinaryFormat::Elf,
        arch,
        match triple.endianness().unwrap() {
            Endianness::Little => object::Endianness::Little,
            Endianness::Big => object::Endianness::Big,
        },
    );

    obj.flags = FileFlags::Elf {
        os_abi: ELFOSABI_VENDOR,
        e_flags: flags | EF_VENDOR_MODULE,
        abi_version: 0,
    };

    Ok(obj)
}
```

Using a vendor-specific `os_abi` and `e_flags` bits gives you:

- a cheap way to distinguish your artifacts from ordinary ELF files,
- an object-kind marker (module vs component, or whatever your runtime has),
- a place to record target-specific variants.

The reference engine additionally *detects* precompiled input with
`os_abi == ELFOSABI_VENDOR`, `abi_version == 0`, and the expected kind bit in
`e_flags`.

### 3.7 Metadata is part of the ABI

The artifact must carry enough metadata to reconstruct runtime structures
without re-parsing the input. The reference engine serializes a tuple of:

- `CompiledModuleInfo` — module shape, imports, exports, function types, data
  ranges, debug ranges, names, checksum;
- `CompiledFunctionsTable` — map from `FuncKey` to `{start, length}` and optional
  source location;
- `ModuleTypes` — interned type definitions used by the unit.

The function location record is intentionally tiny:

```rust
// Adapted from the metadata definitions.
pub struct FunctionLoc {
    pub start: u32,   // offset from start of .text
    pub length: u32,  // body length in bytes
}
```

The complete module info is similarly small and serialization-friendly:

```rust
// Adapted from the metadata definitions.
pub struct CompiledModuleInfo {
    pub module: Module,                 // structural metadata
    pub meta: DebugMetadata,            // source/debug section ranges
    pub func_names: Vec<FunctionName>,  // sorted by function index
    pub checksum: SourceChecksum,       // optional integrity check
}
```

The function table is a data-oriented map, not a `HashMap`. It uses a
struct-of-arrays layout with ranges per function kind, dense or sparse indices,
and locations sorted by text offset. That gives:

- fast `FuncKey -> FunctionLoc` lookup,
- fast `text offset -> FuncKey` reverse lookup for backtraces and traps,
- low memory overhead because the table is deserialized into every loaded unit,
- a uniform representation for defined functions, trampolines, builtins, and
  startup functions.

### 3.8 Unwind, debug, and runtime metadata sections

The text section is not enough. A production artifact also carries:

| Section (conceptual) | Purpose |
|---|---|
| `.text` | Fully linked machine code |
| `.unwind` | Platform unwind tables (`.eh_frame`, or `.xdata`/`.pdata`) |
| `.engine.meta` | Artifact format version, engine version, compatibility metadata |
| `.module.info` | Serialized module/function/type metadata |
| `.rodata` | Read-only data referenced by compiled code |
| `.names` | Function names, optionally |
| `.debug.native` | Native DWARF, with relocations allowed |
| `.debug.source` | Debug info copied from the input format, optionally |
| `.addrmap` | Text offset -> source location map, optionally |
| `.stackmap` | Stack maps for GC safepoints, if needed |
| `.traps` | Text offset -> trap code |
| `.exceptions` | Exception-handler tables |
| `.frames` | Debug frame-state tables, optionally |
| `.source.bytecode` | Original input bytes, only for guest debugging |
| `.bti` | Whether branch-target identification is enabled |

Unwind information needs special layout care. On System V targets the reference
engine pads `.text` to a page boundary and places `.eh_frame` immediately after
it, because the generated FDEs use PC-relative encodings that assume this
relationship. On Windows it emits `.pdata` and `.xdata` with image-relative
addresses. If you copy this design, keep the same relationship between your text
section and your unwind section, or generate relocations that your loader can
apply safely.

---

## 4. Artifact format summary

### 4.1 Header

The container header should record at least:

- a magic or vendor ABI marker,
- an artifact/container format version,
- object kind (compiled unit, component/container, test object, etc.),
- target architecture and endianness,
- target-specific flags.

### 4.2 Engine metadata section

The reference engine stores a compact header followed by a serialized metadata
structure:

```text
+----------------+     +----------------+     +---------------------+
| format version | --> | string length  | --> | version string      |
+----------------+     +----------------+     +---------------------+
                                                       |
                                                       v
                                            +----------------------+
                                            | serialized metadata  |
                                            +----------------------+
```

The writer is straightforward:

```rust
// Adapted from engine metadata serialization.
const FORMAT_VERSION: u8 = 0;

fn append_engine_info(engine: &Engine, obj: &mut Object<'_>, metadata: &Metadata<'_>) {
    let section = obj.add_section(
        obj.segment_name(StandardSegment::Data).to_vec(),
        b".engine.meta".to_vec(),
        SectionKind::ReadOnlyData,
    );

    let mut data = Vec::new();
    data.push(FORMAT_VERSION);

    let version = engine.config().module_version.as_str();
    assert!(version.len() < 256);
    data.push(version.len() as u8);
    data.extend_from_slice(version.as_bytes());
    data.extend(postcard::to_allocvec(metadata).unwrap());

    obj.set_section_data(section, data, 1);
}
```

The compatibility metadata itself contains:

```rust
// Adapted from engine metadata.
pub struct Metadata<'a> {
    target: String,
    shared_flags: Vec<(&'a str, FlagValue<'a>)>,
    isa_flags: Vec<(&'a str, FlagValue<'a>)>,
    tunables: Tunables,
    features: u64,
}
```

Why include these groups?

- `target` catches architecture/OS mismatch.
- `shared_flags` catches code-generation settings that change ABI or runtime
  assumptions.
- `isa_flags` catches CPU feature assumptions and endianness/pointer-width
  variants.
- `tunables` catches runtime layout assumptions such as guard sizes, lazy table
  initialization, trap strategy, GC layout, and debug support.
- `features` catches language/runtime feature bits that the image requires.

### 4.3 Metadata section

The module info is serialized after the object is otherwise complete, because it
contains offsets into sections that are already laid out:

```rust
// Adapted from object metadata serialization.
pub fn serialize_info<T: Serialize>(&mut self, info: &T) {
    let section = self.obj.add_section(
        self.obj.segment_name(StandardSegment::Data).to_vec(),
        b".module.info".to_vec(),
        SectionKind::ReadOnlyData,
    );
    let data = postcard::to_allocvec(info).unwrap();
    self.obj.set_section_data(section, data, 1);
}
```

The reference engine uses `postcard` because it is compact, deterministic, and
supports borrowed slices. Any format works as long as you version it explicitly
and treat it as trusted input.

### 4.4 Versioning dimensions

Do not collapse all versioning into one number. The reference engine separately
tracks:

1. **Container format version** — the byte-level layout of the artifact header.
2. **Engine/module version** — a user-visible string, by default the full
   semantic version of the build.
3. **Metadata schema version** — implicit in the build version, but worth making
   explicit in a new design.
4. **Compiler identity** — target triple and flags.

The reference engine's default policy accepts artifacts only from the exact same
build version, unless the embedder selects a custom version or disables the
version check. That is a safe default: it makes cross-version artifacts fail
deterministically rather than producing undefined behavior.

---

## 5. Save side

### 5.1 Compile directly to bytes

```rust
// Adapted from the public precompile API.
pub fn precompile_module(&self, input: &[u8]) -> Result<Vec<u8>> {
    CodeBuilder::new(self)
        .input_bytes(input)?
        .compile_module_serialized()
}
```

The serialized path deliberately bypasses the compilation cache:

```rust
// Adapted from the code builder.
pub fn compile_module_serialized(&self) -> Result<Vec<u8>> {
    let input = self.get_input()?;
    let dwarf_package = self.get_dwarf_package();
    let (bytes, _) =
        build_module_artifacts(self.engine, &input, dwarf_package.as_deref(), &())?;
    Ok(bytes)
}
```

Because it does not check compatibility with the native host, this is the path
that can cross-compile.

### 5.2 Serialize an already-compiled module

```rust
// Adapted from the module serialization API.
pub fn serialize(&self) -> Result<Vec<u8>> {
    if !self.inner.serializable {
        bail!("this module cannot be serialized on its own");
    }
    Ok(self.engine_code().image().to_vec())
}
```

This returns the exact artifact image that produced the module. It is useful when
a program both instantiates and persists a unit; it avoids compiling twice.

### 5.3 Normal compilation with cache

The normal `Module::new` path builds the same artifacts, but goes through a cache
wrapper. On a miss it compiles, publishes the code, and stores the artifact
bytes. On a hit it loads the bytes as an artifact:

```rust
// Adapted from the cached compile path.
let (code, info_and_types) = ModuleCacheEntry::new("runtime", engine.cache())
    .get_data_raw(
        &state,
        // Miss: build and publish.
        |state| {
            let (mmap, info) = build_artifacts(engine, input, dwarf, state)?;
            let code = publish_mmap(engine, mmap)?;
            Ok((code, info))
        },
        // Serialize for the cache.
        |_, (code, _)| Some(code.mmap().to_vec()),
        // Hit: load the serialized artifact.
        |engine, bytes| {
            let kind = detect_object_kind(input);
            let code = engine.load_code_bytes(&bytes, kind).ok()?;
            Some((code, None))
        },
    )?;
```

A cache hit still goes through the full compatibility and structural validation
path. If loading fails, `.ok()?` turns the hit into a miss and recompiles. That
is the right failure mode: a corrupt or incompatible cache entry is a performance
problem, not a correctness problem.

### 5.4 End-to-end shape

```rust
// Build time, potentially for a different deployment target.
let mut config = Config::new();
config.target("x86_64-unknown-linux-gnu")?;
config.cranelift_opt_level(OptLevel::Speed);
let engine = Engine::new(&config)?;

let artifact = engine.precompile_module(&input)?;
std::fs::write("unit.art", &artifact)?;

// Deploy time, on a compatible host.
let engine = Engine::new(&Config::new())?;
let unit = unsafe { Module::deserialize_file(&engine, "unit.art")? };
let instance = Instance::new(&mut store, &unit, &imports)?;
```

If the deployment engine's target, ABI-affecting flags, or tunables are not
compatible with the artifact, `deserialize_file` returns an error before any of
the artifact's code is executed.

---

## 6. Load side

### 6.1 Entry points

The loader has three memory strategies:

| API shape | Memory strategy | Typical use |
|---|---|---|
| `deserialize(engine, bytes)` | Copy into a fresh mmap | Small artifacts, owned bytes |
| `deserialize_file(engine, path)` | Private file mapping | Large artifacts, fastest startup |
| `deserialize_raw(engine, memory)` | Caller-owned memory, no copy | Shared memory, custom allocator |

All of them are **unsafe** to expose to untrusted input. The artifact contains
executable code and metadata that is only lightly validated. The loader assumes
the bytes were produced by the same trusted compiler and have not been modified.

### 6.2 The load gate

The common load path performs four checks in order:

```rust
// Adapted from the engine load path.
pub(crate) fn load_code(
    &self,
    mmap: MmapVec,
    expected: ObjectKind,
) -> Result<Arc<CodeMemory>> {
    self.check_compatible_with_native_host()?;
    serialization::check_compatible(self, &mmap, expected)?;

    let mut code = CodeMemory::new(self, mmap)?;
    code.publish()?;

    Ok(Arc::new(code))
}
```

1. **Host compatibility** — the engine's target and ABI-affecting settings must be
   runnable on this host.
2. **Artifact compatibility** — header marker, format version, engine version,
   target, flags, tunables, and feature bits must match.
3. **Structural validation** — section layout, alignment, relocation policy, and
   expected sections.
4. **Publication** — make the image read-only, executable, and registered for
   unwinding/debugging.

### 6.3 Memory mapping and alignment

An artifact loaded from a file is mapped privately and writable first, because
the loader may need to normalize or patch pages. After validation, publication
removes write permission.

```rust
// Adapted from the mmap abstraction.
pub fn from_file(file: File) -> Result<MmapVec> {
    let file = Arc::new(file);
    let mmap = Mmap::from_file(Arc::clone(&file))?;
    let len = mmap.len();
    Ok(MmapVec::new_mmap(mmap, len))
}
```

For `deserialize(bytes)`, the loader copies the bytes into a new mmap:

```rust
// Adapted from the mmap abstraction.
pub fn from_slice_with_alignment(slice: &[u8], align: usize) -> Result<MmapVec> {
    let mut result = MmapVec::with_capacity_and_alignment(slice.len(), align)?;
    unsafe { result.as_mut_slice().copy_from_slice(slice) };
    Ok(result)
}
```

On platforms with virtual memory, the OS mapping is page-aligned, so the requested
alignment is bounded by the page size. On platforms without virtual memory, the
allocation must honor the requested alignment. If your code generator assumes a
minimum text alignment, enforce it in the loader or the generated code will be
wrong.

For file-backed loading, the file must not change for the lifetime of the loaded
module. Pages are faulted in lazily, so later writes to the file can change the
code that executes. Treat the file as immutable, or copy it into private memory.

### 6.4 Structural validation

The loader parses the ELF section table and records the ranges of every section it
cares about. It checks:

- every non-empty section is aligned to its declared alignment;
- there are no load-time relocations, except in native debug sections;
- the expected sections are present;
- the branch-protection marker section is present;
- there are no unexpected sections.

```rust
// Adapted from code-memory construction.
for section in sections {
    let data = section.data(endian, &mmap)?;
    let range = subslice_range(data, &mmap);

    let align = usize::try_from(section.sh_addralign(endian))?;
    if align != 0 && !data.is_empty() {
        let offset = data.as_ptr().addr() - mmap.as_ptr().addr();
        ensure!(offset % align == 0, "misaligned section");
    }

    if let Some(target) = reloc_section_target(&sections, section, endian)? {
        let target_name = section_name(endian, strings, target)?;
        ensure!(
            target_name.starts_with(".debug_"),
            "unexpected relocations in {target_name:?}",
        );
    }

    match name {
        ".text" => text = range,
        ".engine.meta" => { /* retained for compatibility checks */ }
        ".module.info" => info = range,
        ".unwind" => unwind = range,
        ".rodata" => data_range = range,
        ".names" => names = range,
        ".addrmap" => address_map = range,
        ".stackmap" => stack_maps = range,
        ".traps" => traps = range,
        ".exceptions" => exceptions = range,
        ".frames" => frames = range,
        ".source.bytecode" => source_bytes = range,
        ".bti" => enable_branch_protection = Some(data[0] != 0),
        // ... debug and symbol sections ...
        _ => bail!("unexpected section {name:?}"),
    }
}
```

Rejecting unknown sections is a deliberate safety and compatibility measure. It
prevents a malformed or newer artifact from being interpreted under older layout
assumptions.

### 6.5 Publication: W^X and unwind registration

Publication is the transition from "data in memory" to "executable code".

```rust
// Adapted from code-memory publication.
pub fn publish(&mut self) -> Result<()> {
    assert!(!self.published);
    self.published = true;

    if self.text().is_empty() {
        return Ok(());
    }

    unsafe {
        // Freeze the whole image first.
        if self.mmap.supports_virtual_memory() {
            self.mmap.make_readonly(0..self.mmap.len())?;
        }

        // Then make only the text range executable.
        if self.needs_executable {
            if !self.custom_publish()? {
                if !self.mmap.supports_virtual_memory() {
                    bail!("this target requires virtual memory");
                }
                self.mmap.make_executable(
                    self.text.clone(),
                    self.enable_branch_protection,
                )?;
            }
        }

        if !self.registered {
            self.register_unwind_info()?;
            self.register_debug_image()?;
            self.registered = true;
        }
    }

    Ok(())
}
```

On Unix, `make_executable`:

- flushes the instruction cache,
- calls `mprotect(PROT_READ | PROT_EXEC)` on the text range,
- optionally adds `PROT_BTI` on AArch64 Linux,
- flushes the pipeline.

On Windows, the equivalent uses `VirtualProtect`/`FlushInstructionCache`. On all
platforms, the sequence is W^X: writable pages are made read-only before any page
becomes executable.

Unwind registration must happen before code can be unwound. The reference engine
passes the text base pointer and the unwind section range to a platform-specific
`UnwindRegistration`. On System V Unix that is `__register_frame`/libunwind with
`.eh_frame`; on Windows it is the `.pdata`/`.xdata` runtime-function table.

### 6.6 Decode metadata, register types, build the module

After publication, the loader decodes the metadata section and reconstructs the
runtime module:

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

    // Register the unit's type definitions in the engine and rewrite
    // module-local type indices to canonical engine-wide indices.
    let signatures = engine
        .register_and_canonicalize_types(&mut types, core::iter::once(&mut info.module))?;

    // Package the code image, type information, and function table.
    let code = EngineCode::new(code_memory, signatures, types.into())?;
    let index = Arc::new(index);

    Module::from_parts_raw(engine, code, info, index, true)
}
```

`EngineCode::new` also registers the text address range with the process-global
code registry used by trap/signal handling and backtrace lookup.

`from_parts_raw` then:

1. constructs the runtime module object from the metadata,
2. computes the low-level offsets for the instance context,
3. asks the instance allocator to validate the module shape,
4. stores the function table for later `FuncKey -> FunctionLoc` lookups.

At this point the module is loaded but not yet associated with a store.

### 6.7 Store registration

Code is registered with a store at instantiation time, not at deserialization
time. This allows one loaded module to be used by many stores and keeps
store-specific concerns (for example per-store breakpoint copies) out of the
shared image.

```rust
// Adapted from the store's module registry.
pub fn register(&mut self, module: &Module, engine: &Engine) -> Result<()> {
    ensure!(Engine::same(engine, module.engine()), "cross-engine use");

    if !self.modules.contains_key(&module.id()) {
        self.modules.insert(module.id(), module.clone())?;
    }

    // Create or reuse a store-local code handle for this image.
    // Simplified: the real registry memoizes this by the engine-code address.
    // Normally the handle shares the original CodeMemory; a debug build may
    // make a private copy so it can patch breakpoints per store.
    let store_code = StoreCode::new(engine, module.engine_code())?;
    self.loaded_code.insert(store_code.text_range(), LoadedCode {
        code: store_code,
        index: module.function_table().clone(),
        modules: Default::default(),
    })?;

    Ok(())
}
```

A function's executable address is then simply the store code's text base plus the
serialized function location:

```rust
// Adapted from function lookup.
pub fn function(&self, key: FuncKey) -> &[u8] {
    let range = self.module.compiled_module().function_range(key);
    &self.store_code.text()[range]
}
```

That is the payoff of the whole design: loading has already resolved all
relocations and recorded every function's range, so dispatch is a table lookup
and a pointer addition.

---

## 7. Relocations and code layout

### 7.1 Relocation model

Cranelift relocations name a target function by `FuncKey`, not by a symbol string:

```rust
// Adapted from the compiler's relocation record.
pub struct Relocation {
    pub reloc: binemit::Reloc,
    pub reloc_target: FuncKey,
    pub offset: binemit::CodeOffset,
    pub addend: binemit::Addend,
}
```

At link time, the `FuncKey -> index` map resolves every target. The text-section
builder applies the relocation to the final text bytes. This is why the artifact
does not need a dynamic linker or a relocation section.

### 7.2 Position independence

Even though relocations are resolved at compile time, generated code should still
be position-independent where the target requires it:

- relative branches are naturally position-independent;
- absolute addresses embedded in code must be avoided or converted to
  text-relative offsets;
- the text base must satisfy the alignment assumed by the code generator;
- veneers must be inserted if a branch target is out of range.

If your compiler emits absolute pointers into `.text` or `.rodata`, you either
need load-time relocation or a guarantee that the image is loaded at a fixed
address. The reference design avoids both by keeping code free of load-time
relocations.

### 7.3 Text/unwind layout

The reference engine's System V unwind writer assumes:

- `.text` is page-padded to a boundary,
- `.eh_frame` follows `.text`,
- FDE addresses use `DW_EH_PE_pcrel`.

If you change that layout, you must change the unwind encoding or add a
relocation. Keep the invariant explicit in your artifact format documentation.

---

## 8. Compatibility and versioning

### 8.1 What must match

| Category | Must match? | Why |
|---|---|---|
| Target architecture | Yes | Machine code is architecture-specific |
| Target OS | Yes for native execution | ABI, unwind format, page protections |
| Pointer width | Yes | Context layout, pointer-sized values |
| Endianness | Yes | Code and metadata encoding |
| ABI-affecting compiler flags | Yes | Calling convention, frame layout, stack probes |
| Runtime tunables affecting layout | Yes | Guard sizes, lazy initialization, trap strategy |
| Feature bits required by the image | Yes (image subset of host) | Runtime support must be present |
| Optimization level | No | Semantics-preserving |
| Debug info generation | No | Extra sections can be ignored |
| Address map generation | No | Optional metadata |
| Cache settings | No | Not part of the artifact |
| Parallel compilation | No | Scheduling only |

The reference engine's triple check compares architecture and operating system,
not vendor/environment. It does not have a separate pointer-width or endianness
field; those are implied by architecture names and by specific ISA flags. For a
new format, add explicit `pointer_width` and `endianness` fields. Relying on
architecture naming is fragile.

### 8.2 Compatibility metadata

```rust
// Adapted from engine metadata checks.
fn check_compatible(mut self, engine: &Engine) -> Result<()> {
    self.check_triple(engine)?;
    self.check_shared_flags(engine)?;
    self.check_isa_flags(engine)?;
    self.check_tunables(&engine.tunables())?;
    self.check_features(&engine.features())?;
    Ok(())
}

fn check_triple(&self, engine: &Engine) -> Result<()> {
    let module_target = Triple::from_str(&self.target)?;
    let engine_target = engine.target();

    if module_target.architecture != engine_target.architecture {
        bail!("artifact was compiled for architecture '{}'",
              module_target.architecture);
    }
    if module_target.operating_system != engine_target.operating_system {
        bail!("artifact was compiled for operating system '{}'",
              module_target.operating_system);
    }
    Ok(())
}
```

The flag checks fall into three buckets:

- **Required values** — for example, frame pointers preserved, stack probes
  enabled, a specific libcall calling convention.
- **Must be disabled** — features that change generated-code behavior in ways the
  runtime does not support.
- **Semantics-preserving and ignored** — optimization level, register allocator,
  NaN canonicalization, Spectre mitigations, and similar knobs.

Unknown flags are rejected. That is important: silently ignoring a new
ABI-affecting flag produces hard-to-debug corruption.

Tunables are checked field by field. The reference engine strictly checks things
like memory reservation/guard sizes, trap strategy, lazy table initialization,
GC heap layout, debug support, and inlining settings. It deliberately ignores
runtime-only settings that do not affect the compiled image, such as growth
reservation, GC zeal, and address-map generation.

Feature bits are checked as a subset: the artifact may require fewer features
than the host provides, but not more.

### 8.3 Version policy

A safe default is:

- accept artifacts only from the exact same build version;
- provide an explicit opt-out for controlled deployments;
- include the version string in the artifact header, not only in metadata;
- reject unknown container format versions before parsing further.

The reference engine's default version string is the full semantic version of the
build. A custom string or `None` can be configured. The important property is
that an incompatible artifact fails **deterministically and safely**, before any
of its code is executed.

---

## 9. Caching

### 9.1 Cache key

A cache key must include everything that changes the artifact:

```rust
// Adapted from the engine compile-environment hash.
impl Hash for EngineCompileEnv<'_> {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        if let Some(compiler) = self.0.compiler() {
            compiler.triple().hash(hasher);
            compiler.flags().hash(hasher);
            compiler.isa_flags().hash(hasher);
        }

        self.0.tunables().hash(hasher);
        self.0.features().hash(hasher);

        // Prevent accidental reuse across build versions.
        self.0.config().module_version.hash(hasher);
    }
}
```

The input bytes themselves are hashed by the cache entry, not by this structure.
The net key is therefore approximately:

```text
hash(input) + hash(target, compiler flags, ISA flags, tunables, features, version)
```

The reference engine also exposes this configuration hash publicly as
`precompile_compatibility_hash`. That lets a build system or deployment
controller compare two engines and decide whether an artifact produced by one is
expected to load in the other, without attempting a load.

### 9.2 Cache values and validation

The cache value is the artifact bytes. A cache hit must still:

1. copy or map the bytes,
2. run the same compatibility checks as any other load,
3. parse and validate the section layout,
4. publish the code,
5. decode metadata.

If any step fails, treat it as a miss and recompile. This makes stale, corrupt,
or cross-target cache entries harmless.

### 9.3 Operational rules

- Write cache entries atomically (temp file + rename).
- Include a checksum or authenticated digest if the cache is shared.
- Never share a cache directory between different target triples unless the key
  includes the triple.
- Version the cache directory or key with the engine version.
- Bound the cache size and evict deterministically.
- Consider separate directories for debug and release configurations.

---

## 10. Security and operational concerns

### 10.1 Deserialization is a trust boundary

The reference loader explicitly documents that deserialization is unsafe and must
not be exposed to arbitrary input. The artifact contains machine code and
metadata that is only lightly validated. An attacker who can modify the bytes can
execute arbitrary code.

If you need to load artifacts from an untrusted or semi-trusted source:

- sign artifacts at build time and verify signatures before loading;
- authenticate the metadata and the code together;
- reject artifacts whose signer or build identity is not allowed;
- consider encrypting artifacts at rest if confidentiality matters;
- never rely on structural validation as a security boundary.

### 10.2 File-backed artifacts must be immutable

`deserialize_file` maps the file and faults pages in lazily. If another process
modifies the file while the module is alive, the executing code can change. Treat
artifact files as immutable, or copy them into private memory before publication.

### 10.3 W^X and instruction cache coherence

- Map writable during loading only if you must write.
- Freeze the image read-only before making text executable.
- Flush the instruction cache after making text executable.
- Use platform-specific branch-protection flags if your target supports them.
- Do not leave writable and executable aliases of the same pages.

### 10.4 Unwind and debug registration

Register unwind information before any code can throw or trap. If registration
fails, unpublish the image and return an error; do not leave executable code with
no unwind metadata.

### 10.5 Metadata ordering

The reference engine publishes code before decoding the metadata section. That is
acceptable because the artifact is trusted and because the code cannot execute
until a module is instantiated. If your loader decodes metadata first, you get
better error messages for malformed metadata and can avoid publishing anything
until the metadata is known-good. For a new design, prefer:

1. structural validation,
2. metadata decode and validation,
3. publication,
4. registration.

This ordering makes failures cheaper and safer.

### 10.6 Cross-compilation

Precompilation may run on a different host than execution. Keep the compile-side
configuration explicit and record it in the artifact. Test the artifact on the
deployment target, including:

- pointer width and endianness,
- CPU feature assumptions,
- unwind format,
- executable-memory permissions,
- branch protection,
- calling convention details.

---

## 11. Design checklist

### 11.1 Minimum viable artifact writer

- [ ] Choose a container (ELF via `object` is a good default).
- [ ] Define a vendor header marker and object-kind flags.
- [ ] Define a format version and a version string.
- [ ] Define stable function keys and deterministic compile order.
- [ ] Resolve all intra-image relocations at link time.
- [ ] Emit `.text`, unwind information, and a metadata section.
- [ ] Optionally emit names, source debug info, address maps, stack maps, traps,
      exceptions, and frame tables.
- [ ] Serialize compatibility metadata and module/function/type metadata.
- [ ] Add a source checksum if replay or integrity checking matters.

### 11.2 Minimum viable loader

- [ ] Accept bytes, a file, or external memory.
- [ ] Check engine/host compatibility before touching code.
- [ ] Check artifact header, format version, and version string.
- [ ] Parse sections and validate alignment.
- [ ] Reject unexpected sections and load-time relocations.
- [ ] Decode and validate metadata.
- [ ] Make the image read-only, then text executable.
- [ ] Flush instruction cache and register unwind information.
- [ ] Register types and build the runtime module object.
- [ ] Register code with a store at instantiation time.
- [ ] Look up function ranges from the serialized table.

### 11.3 Tests worth writing

- Round-trip: compile, serialize, deserialize, instantiate, call every export.
- Cross-version: older/newer artifacts must fail deterministically.
- Cross-target: wrong architecture, OS, pointer width, endianness.
- Flag mismatch: ABI-affecting flag changes must be rejected; optimization-only
  changes must be accepted.
- Corrupt sections: truncated header, bad alignment, unknown section, metadata
  truncation, bad type indices.
- W^X: text is not writable after publication.
- Unwind: backtraces work through loaded code.
- File mutation: document and test the immutable-file requirement.
- Cache: miss, hit, corrupt hit, cross-target hit, version change.

### 11.4 Recommended metadata schema additions

The reference format is compact but relies on architecture names for some
properties. For a new runtime, make these explicit:

- `pointer_width: u8`
- `endianness: Little | Big`
- `metadata_schema_version: u32`
- `compiler_build_id` or a hash of the compiler configuration
- `abi_version` for the host interface
- `object_kind`
- `code_alignment`
- `unwind_format`
- `feature_bits` with a documented compatibility rule

This costs a few bytes and removes a class of subtle cross-target failures.

---

## 12. Reference source map

Paths beginning with `src/` are relative to the reference runtime crate;
`crates/cranelift` and `crates/environ` are the compiler and environment crates.

| Topic | Location |
|---|---|
| Engine compiler handle and AOT entry point | `src/engine.rs` |
| Module artifact build pipeline | `src/compile.rs` |
| Serialized-compile and cached-compile paths | `src/compile/code_builder.rs`, `src/compile/runtime.rs` |
| Object file construction and section/relocation layout | `crates/environ/src/compile/mod.rs`, `crates/cranelift/src/obj.rs` |
| Function and module metadata structures | `crates/environ/src/module_artifacts.rs` |
| Module structural metadata | `crates/environ/src/module.rs` |
| Compiled function representation and relocations | `crates/cranelift/src/compiled_function.rs` |
| Per-function compile and append-code driver | `crates/cranelift/src/compiler.rs` |
| Engine compatibility metadata and checks | `src/engine/serialization.rs` |
| Module deserialization and construction | `src/runtime/module.rs` |
| ELF section parsing, publication, unwind registration | `src/runtime/code_memory.rs` |
| Mmap/copy/external-memory strategies | `src/runtime/vm/mmap_vec.rs` |
| Store-level module/code registration | `src/runtime/module/registry.rs`, `src/runtime/code.rs` |
| Runtime module object and function ranges | `src/runtime/instantiate.rs` |
