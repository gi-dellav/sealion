# RUNTIME.md — Exposing Rust APIs to Cranelift-Generated Code

This document distills the host-interface design of a production Cranelift-based
engine into a reusable blueprint. The snippets are adapted from that engine and
identifiers are normalized: `compiled_call` is the compiler's native entry point,
`array_call` is the universal value-slot entry point, and "module" means one
compiled unit. Exact upstream locations are listed in the source map at the end.

The goal is to let Rust code call compiled code, let compiled code call Rust
code, and make both directions type-safe, trap-safe, and cheap enough for
production use.

---

## 1. What "exposing a Rust API" actually means

A compiled function cannot call an arbitrary Rust closure. It can only call a raw
machine address with a fixed calling convention. Exposing a Rust API therefore
requires six pieces:

1. A stable address for the closure's state.
2. A stable `extern "C"` entry point for the closure.
3. A signature identity shared by the Rust side, the linker, and the compiler.
4. An entry point that matches the compiler's native calling convention.
5. A context pointer passed to every call.
6. A trap/error channel that does not rely on unwinding through compiled frames.

The reference engine solves this with **two calling conventions** and a
**function-reference record** that carries both entry points.

---

## 2. The two calling conventions

### 2.1 Native/compiled ABI

Compiled functions and compiler-generated bridging trampolines use the native ABI
of the target. The reference engine prepends two context arguments to every
signature:

1. **Callee context** — a special `ArgumentPurpose::VMContext` value. Compiled
   code can retrieve it with `special_param`.
2. **Caller context** — an ordinary pointer. This lets the callee reach the
   caller's runtime state, for example to access a caller-owned memory or store.

After those two pointers come the source-level parameters and results.

```rust
// Adapted from crates/cranelift/src/lib.rs.
fn blank_sig(isa: &dyn TargetIsa, call_conv: CallConv) -> ir::Signature {
    let pointer_type = isa.pointer_type();
    let mut sig = ir::Signature::new(call_conv);

    sig.params.push(ir::AbiParam::special(
        pointer_type,
        ir::ArgumentPurpose::VMContext,
    ));
    sig.params.push(ir::AbiParam::new(pointer_type));
    sig
}

fn compiled_call_signature(
    isa: &dyn TargetIsa,
    func_ty: &FuncType,
    call_conv: CallConv,
) -> ir::Signature {
    let mut sig = blank_sig(isa, call_conv);
    let cvt = |ty: &ValueType| ir::AbiParam::new(value_type(isa, *ty));
    sig.params.extend(func_ty.params().iter().map(&cvt));
    sig.returns.extend(func_ty.results().iter().map(&cvt));
    sig
}
```

The reference engine uses `CallConv::Tail` for compiled code so tail calls are
available, and switches to a different internal `CallConv` when a second code
generator must interoperate with the same runtime.

### 2.2 Array/slot ABI

The array ABI is signature-independent. Every callable has the same Rust
signature:

```rust
// Adapted from src/runtime/vm/vmcontext.rs.
pub type VMArrayCallNative = unsafe extern "C" fn(
    NonNull<VMOpaqueContext>, // callee context
    NonNull<VMContext>,       // caller context
    NonNull<ValRaw>,          // values: parameters in, results out
    usize,                    // capacity in ValRaw slots
) -> bool;                    // true = success, false = trap recorded
```

This is the **only** ABI the Rust side needs to know about. It is used for:

- the Rust closure entry point,
- direct host-to-host calls,
- host-to-compiled calls (`array_call` of a compiled function points at an
  array-to-compiled trampoline),
- compiled-to-host calls (`compiled_call` points at a compiled-to-array
  trampoline).

Each value occupies one 16-byte `ValRaw` slot, regardless of whether it is an
`i32`, `i64`, `f64`, pointer, or 16-byte vector. Slots are read and written in
little-endian order so the ABI is identical on every host:

```rust
// Adapted from crates/cranelift/src/compiler.rs.
let value_size = mem::size_of::<u128>();
let values_vec_len = cmp::max(ty.params().len(), ty.results().len());
let values_vec_byte_size = u32::try_from(value_size * values_vec_len).unwrap();

let slot = builder.func.create_sized_stack_slot(ir::StackSlotData::new(
    ir::StackSlotKind::ExplicitSlot,
    values_vec_byte_size,
    3, // 2^3-byte alignment; sufficient for 8-byte-aligned ValRaw
));

let flags = ir::MemFlagsData::new()
    .with_notrap()
    .with_endianness(ir::Endianness::Little);
```

The buffer capacity is `max(parameter_count, result_count)` because parameters
are consumed before results are written.

### 2.3 Why two ABIs?

The native ABI is fast and lets compiled code call compiled code directly. It is
also type-specific, so it cannot be used through a single function-pointer type
or from dynamically typed host code.

The array ABI is uniform, so the runtime can hold a single function pointer type
for every callable, independent of its signature. The cost is that arguments must
be spilled to and reloaded from a slot buffer at each boundary.

The practical design is:

- compiled-to-compiled calls use the native ABI;
- any call that crosses the host/compiled boundary goes through a
  compiler-generated bridge on one side and an `array_call` pointer on the other.

---

## 3. Signature identity and type marshalling

The runtime must answer two questions quickly:

- At link time: does this host function have the signature this import requires?
- At call time: does this function reference have the signature this indirect
  call expects?

The reference engine gives every function type a canonical engine-wide
`VMSharedTypeIndex`. Types are interned once per `Engine`; modules and host
functions refer to the shared index instead of carrying structural types through
hot paths.

On the Rust side, a trait maps Rust types to value types and to slot
loads/stores. The important operations are:

```rust
// Simplified from src/runtime/func/typed.rs.
pub unsafe trait ValueTy: Send {
    fn valtype() -> ValueType;
    fn store(self, store: &mut AutoAssertNoGc<'_>, ptr: &mut MaybeUninit<ValRaw>)
        -> Result<()>;
    unsafe fn load(store: &mut AutoAssertNoGc<'_>, ptr: &ValRaw) -> Self;
}

unsafe impl ValueTy for i32 {
    fn valtype() -> ValueType { ValueType::I32 }

    fn store(self, _: &mut AutoAssertNoGc<'_>, ptr: &mut MaybeUninit<ValRaw>)
        -> Result<()>
    {
        ptr.write(ValRaw::i32(self));
        Ok(())
    }

    unsafe fn load(_: &mut AutoAssertNoGc<'_>, ptr: &ValRaw) -> Self {
        ptr.get_i32()
    }
}
```

A companion `ValueRet` trait does the same for results. Tuple implementations
load/store each element in order. This is what makes the public `Func::wrap`
surface ergonomic while the internal boundary stays uniformly slot-based.

The signature is registered when the host function is created, and the
registration is kept alive for as long as the function exists:

```rust
struct HostFuncState<F> {
    func: F,
    _ty: RegisteredType, // keeps the canonical type index alive
}
```

---

## 4. Creating a host callable

### 4.1 Public surface

The public API is intentionally small:

```rust
let add = Func::wrap(&mut store, |mut caller: Caller<'_, State>, a: i32, b: i32| -> i32 {
    caller.data_mut().calls += 1;
    a.wrapping_add(b)
});

let mut linker = Linker::new(engine);
linker.define("math", "add", add)?;
let instance = linker.instantiate(&mut store, &unit)?;
```

`Func::wrap` is a convenience layer over three internal steps:

1. `IntoFunc` turns the Rust closure into a `HostFunc`.
2. `HostFunc` builds and interns the signature.
3. The host context is inserted into the store as a function reference.

The linker also offers a one-step form, `Linker::func_wrap(module, name,
closure)`, which is sugar for `Func::wrap` followed by `Linker::define`. Both
forms produce the same `HostFunc` and the same function reference.

### 4.2 `IntoFunc` and typed-to-untyped adaptation

`IntoFunc` is implemented for functions and closures with up to 17 parameters,
with or without a leading `Caller`. Closures without `Caller` delegate to the
version with `Caller`, passing a dummy caller:

```rust
// Adapted from src/runtime/func.rs.
impl<T, F, A, R> IntoFunc<T, (Caller<'_, T>, A), R> for F
where
    F: Fn(Caller<'_, T>, A) -> R + Send + Sync + 'static,
    A: ValueTy,
    R: ValueRet,
    T: 'static,
{
    fn into_func(self, engine: &Engine) -> Result<HostFunc, OutOfMemory> {
        HostFunc::wrap(engine, move |caller: Caller<'_, T>, (a,)| self(caller, a))
    }
}
```

`HostFunc::wrap` constructs the signature, then closes over a function that
converts slots to typed parameters and typed results back to slots:

```rust
// Adapted from src/runtime/func.rs.
pub fn wrap<T, F, P, R>(engine: &Engine, func: F) -> Result<Self, OutOfMemory>
where
    F: Fn(Caller<'_, T>, P) -> R + Send + Sync + 'static,
    P: ValueTyList,
    R: ValueRet,
    T: 'static,
{
    let ty = R::func_type(engine, P::valtypes())?;

    let ctx = Self::vmctx_sync(engine, ty, move |mut caller, args| {
        let params = unsafe { Self::load_typed_params(caller.store.0, args) };
        let ret = func(caller.sub_caller(), params).into_fallible();
        unsafe { Self::store_typed_results(caller.store.0, ret, args) }
    })?;

    Ok(HostFunc::new_raw(engine, ctx, Asyncness::No))
}
```

No machine code is generated by `Func::wrap`. The host function is usable
immediately for host-to-host calls, and becomes callable from compiled code once
a module with a compatible bridging trampoline is available.

### 4.3 The host context

The context must be stable in memory because compiled code and function
references hold raw pointers to it. The reference engine allocates it with a
`StoreBox`, a box-like allocation whose address does not change when the
`HostFunc` value moves.

```rust
// Adapted from
// src/runtime/vm/vmcontext/vm_host_func_context.rs.
#[repr(C)]
pub struct VMArrayCallHostFuncContext {
    magic: u32,
    // padding on 64-bit targets
    pub(crate) func_ref: VMFuncRef,
    host_state: Box<dyn Any + Send + Sync>,
}
```

On a 64-bit target the layout is:

| Offset | Field | Size |
|---:|---|---:|
| 0 | `magic` | 4 |
| 4 | padding | 4 |
| 8 | `func_ref` | 32 |
| 40 | `host_state` (fat pointer) | 16 |
| 56 | total | 56 |

The `magic` value is a cheap runtime discriminator. It lets debug builds verify
that an opaque context really is a host-function context before casting. The
offset of `func_ref` is not hard-coded in the compiler; it is computed from the
same offset description used to generate the runtime and compiler layouts, and a
unit test asserts that the computed offset matches `offset_of!`.

The constructor fills in the embedded function reference:

```rust
// Adapted from the same file.
pub unsafe fn new(
    host_func: VMArrayCallNative,
    type_index: VMSharedTypeIndex,
    host_state: Box<dyn Any + Send + Sync>,
) -> Result<StoreBox<VMArrayCallHostFuncContext>, OutOfMemory> {
    let ctx = StoreBox::new(VMArrayCallHostFuncContext {
        magic: VM_ARRAY_CALL_HOST_FUNC_MAGIC,
        func_ref: VMFuncRef {
            array_call: NonNull::new(host_func as *mut u8).unwrap().cast().into(),
            type_index,
            compiled_call: None,
            vmctx: NonNull::dangling().into(),
        },
        host_state,
    })?;

    let vmctx = VMOpaqueContext::from_vm_array_call_host_func_context(ctx.get());
    unsafe { ctx.get().as_mut().func_ref.vmctx = vmctx.into() };
    Ok(ctx)
}
```

The context's own `func_ref.vmctx` points back to the context. This is the
callee context that compiled code will pass to the array ABI.

---

## 5. Function references and "hole filling"

### 5.1 `VMFuncRef`

A function reference is the runtime's universal callable handle:

```rust
// Adapted from crates/environ/src/vmtypes.rs.
#[repr(C)]
pub struct VMFuncRef {
    /// Always present. Calls the universal slot ABI.
    pub array_call: VmPtr<VMArrayCallFunction>,

    /// Present when this callable can be called from compiled code.
    /// Optional because a freshly created host function has no compiler
    /// available to generate its native-ABI bridge.
    pub compiled_call: Option<VmPtr<VMCompiledCallFunction>>,

    /// Canonical signature identity.
    pub type_index: VMSharedTypeIndex,

    /// Context passed as the callee context.
    pub vmctx: VmPtr<VMOpaqueContext>,
}
```

On 64-bit targets this is 32 bytes. `VMFunctionImport` has exactly the same
layout, except that `compiled_call` is a non-null pointer. The layout equivalence
is enforced by tests:

```rust
assert_eq!(size_of::<VMFunctionImport>(), size_of::<VMFuncRef>());
assert_eq!(
    offset_of!(VMFunctionImport, compiled_call),
    offset_of!(VMFuncRef, compiled_call),
);
```

This lets the runtime copy a function reference into an import table without
re-encoding it, and lets compiled code load the import's `compiled_call` pointer
at a fixed offset.

### 5.2 Why the native entry point may be absent

When a host function is created, the engine may not know which module will
eventually import it. The native entry point for a host function is not the Rust
closure; it is a **compiler-generated bridge** specialized to the function's
signature. That bridge is emitted as part of a module, once per distinct
signature used by that module.

Therefore the runtime uses a lazy fill protocol:

1. Create the host function with `compiled_call = None`.
2. Insert it into a store. The store copies the function reference into a stable
   arena and asks the module registry whether any registered module has a bridge
   for this signature.
3. If so, patch `compiled_call` immediately. If not, record the reference in a
   `with_holes` list.
4. When a new module is registered, revisit the hole list and fill any entries
   whose signature is now available.
5. Before instantiation copies imports into a module's context, every import
   that will be called from compiled code must have a non-null `compiled_call`.

```rust
// Adapted from src/runtime/store/func_refs.rs.
unsafe fn try_fill(func_ref: &mut VMFuncRef, modules: &ModuleRegistry) -> bool {
    func_ref.compiled_call = modules
        .compiled_to_array_trampoline(func_ref.type_index)
        .map(|f| f.into());
    func_ref.compiled_call.is_some()
}
```

The store records the pointer to its local copy, not the context's embedded copy.
That distinction matters: multiple stores can share one `HostFunc`, but each store
owns its own function-reference arena and may need to patch independently.

### 5.3 Pre-patching during linking

`InstancePre` performs the fill eagerly for the module being instantiated. It
type-checks all definitions first, then creates a `VMFuncRef` for each host
function whose signature has a bridge in that module:

```rust
// Adapted from src/runtime/instance.rs.
for item in &items {
    match item {
        Definition::Function(f) if f.func_ref().compiled_call.is_none() => {
            func_refs.push(VMFuncRef {
                compiled_call: module
                    .compiled_to_array_trampoline(f.sig_index())
                    .map(|f| f.into()),
                ..*f.func_ref()
            })?;
        }
        _ => {}
    }
}
```

If the module has no bridge for that signature, the hole remains. That is not
necessarily an error: the module cannot call that function, so the import may
never be used from compiled code. The error surfaces later if the runtime tries
to convert the reference into an import record.

---

## 6. Linking and instantiation

### 6.1 Definitions are keyed by name and typed separately

The linker stores definitions by `(module_name, field_name)` and keeps a small
type descriptor alongside each definition:

```rust
// Adapted from src/runtime/linker.rs.
pub(crate) enum Definition {
    Extern {
        item: Extern,
        ty: DefinitionType,
        engine: Engine,
    },
    HostFunc(Arc<HostFunc>),
}

pub(crate) enum DefinitionType {
    Func(VMSharedTypeIndex),
    Global(GlobalType),
    Table(TableType, u64),
    Memory(MemoryType, u64),
}
```

Import lookup is by name only. Type checking is a separate step, which keeps the
lookup path simple and lets the same definition be reused for multiple imports
with compatible types.

### 6.2 Type checking and import ordering

`InstancePre::new` walks the module's imports in order and resolves each one
through the linker. It then checks the definition's type against the import's
expected type using the canonical type index.

After type checking it performs the host-function pre-patch described above and
records how many host functions were seen. At instantiation time the runtime:

1. registers the module in the store's module registry;
2. fills any outstanding `compiled_call` holes;
3. converts each definition into its low-level import form;
4. copies the imports into the new instance's context.

The low-level function import is layout-identical to a function reference:

```rust
// Adapted from crates/environ/src/vmtypes.rs.
#[repr(C)]
pub struct VMFunctionImport {
    pub array_call: VmPtr<VMArrayCallFunction>,
    pub compiled_call: VmPtr<VMCompiledCallFunction>,
    pub type_index: VMSharedTypeIndex,
    pub vmctx: VmPtr<VMOpaqueContext>,
}
```

The instance context contains a contiguous array of these records. Compiled code
reaches import `i` by adding a precomputed offset to its own context pointer:

```rust
// Adapted from src/runtime/vm/instance.rs.
debug_assert_eq!(imports.functions.len(), module.num_imported_funcs);
unsafe {
    let offsets = instance.runtime_info.offsets();
    ptr::copy_nonoverlapping(
        imports.functions.as_ptr(),
        instance
            .vmctx_plus_offset_raw(offsets.imported_functions().begin())
            .as_ptr(),
        imports.functions.len(),
    );
}
```

This is the key layout decision: **the compiler and runtime agree on a fixed
offset for every imported callable, and the runtime writes a complete
`{array_call, compiled_call, type_index, vmctx}` record there at instantiation.**

---

## 7. Compiled code calling a host function

### 7.1 Direct imported call

For a direct call to an import, code generation does three things:

1. Load the callee context from the import record.
2. Append `callee_context` and `caller_context` to the compiled-call arguments.
3. Load the import's `compiled_call` pointer and perform an indirect call with
   the native signature.

```rust
// Adapted from crates/cranelift/src/func_environ.rs.
fn direct_call_imported_function(
    /* ... */
    compiled_call_args: &[ir::Value],
    mut real_call_args: Vec<ir::Value>,
    caller_vmctx: ir::Value,
) -> ... {
    let vmctx = self.env.vmctx_val(&mut self.builder.cursor());
    let import_off = self.env.offsets.imported_functions().at(callee_index);

    let callee_vmctx = self
        .env
        .alias_regions
        .vm_function_import()
        .vmctx()
        .relative_to(import_off)
        .load(&mut self.builder.cursor(), vmctx);

    real_call_args.push(callee_vmctx);
    real_call_args.push(caller_vmctx);
    real_call_args.extend_from_slice(compiled_call_args);

    self.imported_call_inst(callee_index, sig_ref, &real_call_args)
}

fn imported_call_inst(
    /* ... */
) -> CallRets {
    let vmctx = self.caller_vmctx();
    let import_off = self.env.offsets.imported_functions().at(callee_index);
    let func_addr = self
        .env
        .alias_regions
        .vm_function_import()
        .compiled_call()
        .relative_to(import_off)
        .load(&mut self.builder.cursor(), vmctx);

    self.indirect_call_inst(sig_ref, func_addr, real_call_args)
}
```

For a host function, `compiled_call` is not the Rust closure. It is a
compiler-generated **compiled-to-array trampoline** whose native signature is the
import's signature.

### 7.2 Indirect calls

Indirect calls through a function reference first check the callee's canonical
type index against the expected index, then load both the native entry point and
the callee context from the reference:

```rust
// Adapted from crates/cranelift/src/func_environ.rs.
let caller_sig_id = self
    .env
    .module_interned_to_shared_ty(&mut self.builder.cursor(), interned_ty);

let callee_sig_id = self
    .env
    .alias_regions
    .vm_func_ref()
    .type_index()
    .trap_code(trap_code)
    .load(&mut self.builder.cursor(), funcref_ptr);

let matches = self
    .env
    .is_subtype(self.builder, callee_sig_id, caller_sig_id, interned_ty);
self.env
    .trapz(self.builder, matches, TRAP_BAD_SIGNATURE);
```

The reference engine compares interned type identities (with a subtype check
where its type system permits subtyping), not structural signatures. That makes
the check a load, a compare, and a trap.

### 7.3 The compiled-to-array bridge

The bridge is generated once per distinct signature in a module. Its body is
small and completely mechanical:

```rust
// Adapted from crates/cranelift/src/compiler.rs.
fn compile_compiled_to_array_trampoline(...) -> Result<CompiledFunctionBody, CompileError> {
    let native_sig = compiled_call_signature(isa, func_ty, call_conv);
    let array_sig = array_call_signature(isa);

    let args = builder.func.dfg.block_params(block0).to_vec();
    let callee_vmctx = args[0];
    let caller_vmctx = args[1];

    // Spill native parameters into 16-byte slots.
    let (args_base, args_len) = self.allocate_stack_array_and_spill_args(
        &mut alias_regions,
        func_ty,
        &mut builder,
        &args[2..],
    );
    let args_len = builder.ins().iconst(pointer_type, i64::from(args_len));

    // The host context embeds its function reference at a known offset.
    let func_ref_offset = alias_regions
        .offsets()
        .get_ptr_size()
        .vmarray_call_host_func_context_func_ref();

    let callee = alias_regions
        .vm_func_ref()
        .array_call()
        .relative_to(func_ref_offset.into())
        .load(&mut builder.cursor(), callee_vmctx);

    let callee_signature = builder.func.import_signature(array_sig);
    let call = self.call_indirect_host(
        &mut builder,
        HostCall::ArrayCall,
        callee_signature,
        callee,
        &[callee_vmctx, caller_vmctx, args_base, args_len],
    );

    // false means an error or trap was recorded on the Rust side.
    let succeeded = builder.func.dfg.inst_results(call)[0];
    self.raise_if_host_trapped(&mut builder, &mut alias_regions, caller_vmctx, succeeded);

    // Reload native results from the same slot buffer.
    let results = self.load_values_from_array(
        &mut alias_regions,
        func_ty.results(),
        &mut builder,
        args_base,
        args_len,
    );
    builder.ins().return_(&results);
    ...
}
```

Notice that the bridge does not know the concrete Rust closure type. It only
knows the function signature and the layout of the host context. That is what
lets one bridge serve every host function with the same signature.

### 7.4 Trap propagation from the bridge

The array ABI returns `true` for success and `false` for a recorded trap or
error. The bridge turns that into a call to the runtime's `raise` builtin, which
unwinds the compiled activation:

```rust
// Adapted from crates/cranelift/src/compiler.rs.
self.builder
    .ins()
    .brif(succeeded, continuation_block, &[], trapped_block, &[]);
...
builder.switch_to_block(trapped_block);
self.call_builtin(
    builder,
    alias_regions,
    vmctx,
    &[vmctx],
    BuiltinFunctionIndex::raise(),
    sig,
);
builder.ins().trap(TRAP_INTERNAL_ASSERT);
```

This is the crucial split: the Rust trampoline **never unwinds through compiled
frames**. It catches the error/panic, stores it in thread-local runtime state,
returns `false`, and lets compiled code call `raise` at a safe point.

---

## 8. The Rust host trampoline

The Rust entry point stored in `func_ref.array_call` is monomorphized over the
closure type. It has the exact array ABI:

```rust
// Adapted from src/runtime/func.rs.
unsafe extern "C" fn array_call_trampoline<T, F>(
    callee_vmctx: NonNull<VMOpaqueContext>,
    caller_vmctx: NonNull<VMContext>,
    args: NonNull<ValRaw>,
    args_len: usize,
) -> bool
where
    F: Fn(Caller<'_, T>, &mut [MaybeUninit<ValRaw>]) -> Result<()> + 'static,
    T: 'static,
{
    let run = |store: &mut dyn VMStore, instance: InstanceId| {
        let mut store = unsafe { store.unchecked_context_mut() };

        // Recover the host-state box from the callee context.
        let state = unsafe {
            let vmctx = VMArrayCallHostFuncContext::from_opaque(callee_vmctx);
            vmctx.as_ref().host_state()
        };

        // We control the Any value; this downcast is an invariant of the
        // signature used to build the context.
        let state = unsafe {
            debug_assert!(state.is::<HostFuncState<F>>());
            &*(state as *const _ as *const HostFuncState<F>)
        };

        let (gc_scope, ret) = {
            let gc_scope = store.0.enter_gc_lifo_scope();

            let mut args = NonNull::slice_from_raw_parts(args.cast(), args_len);
            let args = unsafe { args.as_mut() };

            let ret = (state.func)(
                Caller {
                    caller: Instance::from_runtime(instance, store.0),
                    store: store.as_context_mut(),
                },
                args,
            );

            (gc_scope, ret)
        };

        store.0.exit_gc_lifo_scope(gc_scope);
        ret
    };

    unsafe { vm::Instance::enter_host_from_compiled(caller_vmctx, run) }
}
```

The trampoline does five things:

1. Recovers the store and caller instance from `caller_vmctx`.
2. Recovers the typed host state from `callee_vmctx`.
3. Constructs `Caller` with a mutable store context and the caller instance.
4. Calls the user closure with the raw slot buffer.
5. Returns an ABI-safe `bool`; `enter_host_from_compiled` catches panics and records
   traps in thread-local state.

The `Caller` type is deliberately small:

```rust
pub struct Caller<'a, T: 'static> {
    pub(crate) store: StoreContextMut<'a, T>,
    caller: Instance,
}
```

`Caller` gives the host function:

- mutable access to the user's store data,
- the caller instance, so it can look up caller-side exports,
- a lifetime tied to the call, preventing the host function from retaining
  store borrows.

The caller context is not decoration. In the reference engine, host functions use
`Caller::get_export` to resolve caller-owned objects such as buffers or memory.
System-interface libraries rely on this to copy data between host and caller
address spaces without thread locals or global maps. If your runtime exposes
Rust APIs that receive pointers or handles into caller-owned storage, carry the
caller instance in the host context exactly as shown here.

For host-to-host calls, the runtime still enters through a synthetic caller
context and the same trampoline path. This keeps one code path for all calls and
ensures traps are handled uniformly.

---

## 9. Calling compiled code from Rust

The reverse direction is symmetric. `Func::call` checks the argument count and
types, packs values into a `ValRaw` buffer, and invokes the function reference's
`array_call`:

```rust
// Adapted from src/runtime/func.rs.
pub fn call(
    &self,
    mut store: impl AsContextMut,
    params: &[Val],
    results: &mut [Val],
) -> Result<()> {
    let mut store = store.as_context_mut();
    self.call_impl_check_args(&mut store, params, results)?;
    unsafe { self.call_impl_do_call(&mut store, params, results) }
}

unsafe fn call_impl_do_call<T>(/* ... */) -> Result<()> {
    let ty = self.load_ty(store.0);
    let values_vec_size = params.len().max(ty.results().len());
    let mut values_vec = store.0.take_slot_storage();
    values_vec.resize_with(values_vec_size, || ValRaw::v128(0))?;

    for (arg, slot) in params.iter().cloned().zip(&mut values_vec) {
        *slot = arg.to_raw(&mut *store)?;
    }

    self.call_unchecked(
        &mut *store,
        core::ptr::slice_from_raw_parts_mut(values_vec.as_mut_ptr(), values_vec_size),
    )?;
    ...
}
```

`call_unchecked_raw` enters the runtime's trap-catching scope and invokes the
function reference's array entry point:

```rust
// Adapted from src/runtime/func.rs.
invoke_compiled_and_catch_traps(store, uncaught_exception, |caller, vm| unsafe {
    VMFuncRef::array_call(func_ref, vm, caller, params_and_returns)
})
```

For a compiled function, `array_call` points at an **array-to-compiled**
trampoline. That trampoline:

1. validates that the slot buffer is large enough,
2. loads native arguments from the slots,
3. inserts the callee and caller contexts,
4. calls the compiled body with the native ABI,
5. stores native results back into the slots,
6. returns `true`, or `false` after recording a trap.

```rust
// Adapted from crates/cranelift/src/compiler.rs.
let (vmctx, caller_vmctx, values_vec_ptr, values_vec_len) = {
    let params = builder.func.dfg.block_params(block0);
    (params[0], params[1], params[2], params[3])
};

let required_capacity = cmp::max(func_sig.params().len(), func_sig.results().len());
let enough_capacity = builder.ins().icmp_imm_u(
    ir::condcodes::IntCC::UnsignedGreaterThanOrEqual,
    values_vec_len,
    i64::from(u32::try_from(required_capacity).unwrap()),
);
builder.ins().trapz(enough_capacity, TRAP_INTERNAL_ASSERT);

let mut args = self.load_values_from_array(
    &mut alias_regions,
    func_sig.params(),
    &mut builder,
    values_vec_ptr,
    values_vec_len,
);
args.insert(0, caller_vmctx);
args.insert(0, vmctx);

builder.ins().try_call(callee, &args, exception_table);

// normal return:
self.store_values_to_array(
    &mut alias_regions,
    &mut builder,
    func_sig.results(),
    &normal_return_values,
    values_vec_ptr,
    values_vec_len,
);
let true_return = builder.ins().iconst(ir::types::I8, 1);
builder.ins().return_(&[true_return]);
```

Typed calls (`TypedFunc::call`) use the same path but avoid the dynamic `Val`
checks and place parameters/results in a stack-allocated union instead of a
heap-allocated value vector.

---

## 10. Error, panic, and trap handling

The error channel is part of the ABI, not an afterthought.

- Every array-ABI call returns a `bool` success flag.
- Other host-ABI entry points, such as builtins that must return a value, use a
  distinguished sentinel value instead.
- `enter_host_from_compiled` wraps the Rust closure in
  `catch_unwind_and_record_trap`.
- On `Err` or panic, it converts the failure into a `UnwindReason`, stores it in
  the current thread's runtime state, and returns the sentinel.
- The compiled bridge sees the sentinel and calls the `raise` builtin.
- `raise` unwinds or transfers control to the correct handler; the host-side
  call returns an `Err` to the embedder.

This design has two important properties:

1. Rust destructors run normally on the host side because the panic is caught
   before returning to compiled code.
2. Compiled frames are unwound only by the runtime's own `raise` machinery, not
   by the platform unwinder crossing a language boundary.

It also means a host function must never return a value that collides with the
sentinel. The reference engine enforces this with the `HostResultHasUnwindSentinel`
trait: every ABI-visible return type has an explicit sentinel that no valid value
can produce.

For stores with GC, the trampoline also enters a GC LIFO scope around the host
call so roots are scoped correctly. If your runtime has tracing GC, replicate
this: the host boundary is a root-set boundary.

---

## 11. Design checklist for your runtime

### 11.1 Minimum set of types

| Concept | Suggested shape |
|---|---|
| Canonical signature | Interned index, not a structural type, in hot paths |
| Function reference | `{ array_call, compiled_call, signature_id, context }` |
| Import record | Same layout as function reference, `compiled_call` non-null |
| Host context | Stable allocation with magic, embedded function reference, boxed closure |
| Slot value | 16 bytes, 8-byte aligned, little-endian encoding |
| Caller handle | Store borrow + caller instance/context |
| Trap record | Thread-local state plus ABI sentinel |

### 11.2 Invariants

- A host context must never move after its address is published.
- A function reference's `context` must outlive every compiled call that can use
  it. Store arenas or reference counting are the usual answers.
- `compiled_call` must be non-null before an import record is copied into an
  instance context.
- Every function reference used in an indirect call must carry the canonical
  signature identity expected by the call site.
- Slot buffers must have capacity for `max(params, results)`.
- Host code must not unwind through compiled frames.
- The caller context must be valid for the entire call; do not store it in the
  host state.

### 11.3 Performance notes

- Pre-patch `compiled_call` during linking so the hot import path is a load and
  an indirect call.
- Make the array ABI's slot buffer stack-allocated in generated bridges.
- Use `extern "C"` and a fixed slot layout; do not pass Rust trait objects or
  `Result` across the boundary.
- Intern signatures once per engine. Comparing pointers or small indices is much
  cheaper than structural comparison.
- Keep the host trampoline monomorphized per closure type, but erase that type
  behind `Box<dyn Any>` in the context so the function reference stays uniform.
- Reuse the slot buffer across host calls in the store, as the reference engine
  does with its per-store value storage.

### 11.4 Common pitfalls

- **Forgetting the caller context.** Host functions frequently need caller-owned
  state (memory, tables, store data); without it you will end up with thread
  locals or global maps.
- **Patching only at creation time.** A host function may be created before the
  module that provides its bridge. Keep a hole list and fill it on module
  registration.
- **Treating `array_call` as the compiled entry point.** For compiled functions
  it is an array-to-compiled trampoline, not the function body. The native entry
  point is a separate pointer.
- **Letting panics cross the boundary.** Always catch and convert to a sentinel.
- **Assuming a single store.** Host functions can be shared across stores; keep
  store-local function references and contexts distinct.
- **Using native-endian slots.** Define one endianness for the ABI and stick to
  it; otherwise cross-compilation and serialization become fragile.

---

## 12. Reference source map

Paths beginning with `src/` are relative to the reference runtime crate;
`crates/cranelift` and `crates/environ` are the compiler and environment crates.

| Topic | Location |
|---|---|
| Public `Func`/`Caller` surface, `IntoFunc`, host trampoline | `src/runtime/func.rs` |
| Typed value traits and typed calls | `src/runtime/func/typed.rs` |
| Array ABI type, `VMFuncRef`, `ValRaw` | `src/runtime/vm/vmcontext.rs` |
| Host context layout | `src/runtime/vm/vmcontext/vm_host_func_context.rs` |
| Function reference/import layouts | `crates/environ/src/vmtypes.rs` |
| Layout offset calculation | `crates/environ/src/vmoffsets.rs` |
| Store-local function-reference arena and hole filling | `src/runtime/store/func_refs.rs` |
| Linker definitions, import lookup, `InstancePre` | `src/runtime/linker.rs`, `src/runtime/instance.rs` |
| Import-table initialization in the instance context | `src/runtime/vm/instance.rs` |
| Compiled signatures and array ABI | `crates/cranelift/src/lib.rs` |
| Compiled-to-array bridge | `crates/cranelift/src/compiler.rs` |
| Array-to-compiled bridge | `crates/cranelift/src/compiler.rs` |
| Direct/indirect imported calls and signature checks | `crates/cranelift/src/func_environ.rs` |
| Trap/error ABI and host-entry wrapper | `src/runtime/vm/traphandlers.rs` |
| Opaque context pointer types | `src/runtime/vm/vmcontext.rs` |
