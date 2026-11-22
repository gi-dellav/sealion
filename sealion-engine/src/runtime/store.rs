use std::{any::TypeId, collections::HashMap, ptr::NonNull, sync::Arc};

use crate::Engine;

use super::{
    func::HostFunc,
    linker::{Instance, Module},
    slots::ValRaw,
    types::VMSharedTypeIndex,
    vm::{StoreBox, VMContext, VMFuncRef},
};

#[derive(Default)]
pub(crate) struct ModuleRegistry {
    bridges: HashMap<VMSharedTypeIndex, NonNull<u8>>,
}

impl ModuleRegistry {
    pub(crate) fn insert(&mut self, index: VMSharedTypeIndex, bridge: NonNull<u8>) {
        self.bridges.insert(index, bridge);
    }

    pub(crate) fn compiled_to_array_trampoline(
        &self,
        index: VMSharedTypeIndex,
    ) -> Option<NonNull<u8>> {
        self.bridges.get(&index).copied()
    }
}

#[derive(Default)]
struct FuncRefs {
    refs: Vec<StoreBox<VMFuncRef>>,
    with_holes: Vec<usize>,
}

impl FuncRefs {
    fn register(&mut self, func_ref: VMFuncRef, modules: &ModuleRegistry) -> NonNull<VMFuncRef> {
        let index = self.refs.len();
        let mut func_ref = StoreBox::new(func_ref);
        if !try_fill(&mut func_ref, modules) {
            self.with_holes.push(index);
        }
        let ptr = func_ref.get();
        self.refs.push(func_ref);
        ptr
    }

    fn fill_holes(&mut self, modules: &ModuleRegistry) {
        self.with_holes
            .retain(|&index| !try_fill(&mut self.refs[index], modules));
    }
}

fn try_fill(func_ref: &mut VMFuncRef, modules: &ModuleRegistry) -> bool {
    func_ref.compiled_call = modules.compiled_to_array_trampoline(func_ref.type_index);
    func_ref.compiled_call.is_some()
}

pub(crate) struct Store<T> {
    inner: StoreInner<T>,
}

pub(crate) struct StoreInner<T> {
    pub(crate) engine: Engine,
    data: T,
    func_refs: FuncRefs,
    host_func_refs: HashMap<usize, (NonNull<VMFuncRef>, Arc<HostFunc>)>,
    modules: ModuleRegistry,
    slot_storage: Vec<ValRaw>,
}

impl<T: 'static> Store<T> {
    pub(crate) fn new(engine: &Engine, data: T) -> Self {
        Self {
            inner: StoreInner {
                engine: engine.clone(),
                data,
                func_refs: FuncRefs::default(),
                host_func_refs: HashMap::new(),
                modules: ModuleRegistry::default(),
                slot_storage: Vec::new(),
            },
        }
    }

    pub(crate) fn engine(&self) -> &Engine {
        &self.inner.engine
    }

    pub(crate) fn data(&self) -> &T {
        &self.inner.data
    }

    pub(crate) fn data_mut(&mut self) -> &mut T {
        &mut self.inner.data
    }

    pub(crate) fn as_context(&self) -> StoreContext<'_, T> {
        StoreContext::new(&self.inner)
    }

    pub(crate) fn as_context_mut(&mut self) -> StoreContextMut<'_, T> {
        StoreContextMut::new(&mut self.inner)
    }

    pub(crate) fn into_data(self) -> T {
        self.inner.data
    }

    pub(crate) fn register_host_func(&mut self, host: &Arc<HostFunc>) -> NonNull<VMFuncRef> {
        let key = host.func_ref().vmctx.as_ptr() as usize;
        if let Some((func_ref, _)) = self.inner.host_func_refs.get(&key) {
            return *func_ref;
        }
        let func_ref = self
            .inner
            .func_refs
            .register(*host.func_ref(), &self.inner.modules);
        self.inner
            .host_func_refs
            .insert(key, (func_ref, Arc::clone(host)));
        func_ref
    }

    pub(crate) fn register_module(&mut self, module: &Module) {
        for (&index, &bridge) in module.bridges() {
            self.inner.modules.insert(index, bridge);
        }
        self.inner.func_refs.fill_holes(&self.inner.modules);
    }

    pub(crate) fn vmctx(&mut self, instance: &Instance) -> VMContext
    where
        T: 'static,
    {
        self.inner.vmctx(instance)
    }

    pub(crate) fn take_slot_storage(&mut self) -> Vec<ValRaw> {
        self.inner.take_slot_storage()
    }

    pub(crate) fn return_slot_storage(&mut self, slots: Vec<ValRaw>) {
        self.inner.return_slot_storage(slots);
    }
}

impl<T: 'static> StoreInner<T> {
    pub(crate) fn vmctx(&mut self, instance: &Instance) -> VMContext {
        VMContext::new(
            NonNull::from(&mut *self).cast(),
            instance.context_ptr(),
            TypeId::of::<T>(),
        )
    }

    pub(crate) fn take_slot_storage(&mut self) -> Vec<ValRaw> {
        std::mem::take(&mut self.slot_storage)
    }

    pub(crate) fn return_slot_storage(&mut self, mut slots: Vec<ValRaw>) {
        slots.clear();
        self.slot_storage = slots;
    }
}

pub(crate) struct StoreContext<'a, T> {
    inner: &'a StoreInner<T>,
}

impl<'a, T> StoreContext<'a, T> {
    pub(crate) fn new(inner: &'a StoreInner<T>) -> Self {
        Self { inner }
    }

    pub(crate) fn data(&self) -> &T {
        &self.inner.data
    }

    pub(crate) fn engine(&self) -> &Engine {
        &self.inner.engine
    }
}

pub(crate) struct StoreContextMut<'a, T> {
    inner: &'a mut StoreInner<T>,
}

impl<'a, T> StoreContextMut<'a, T> {
    pub(crate) fn new(inner: &'a mut StoreInner<T>) -> Self {
        Self { inner }
    }

    pub(crate) fn data(&self) -> &T {
        &self.inner.data
    }

    pub(crate) fn data_mut(&mut self) -> &mut T {
        &mut self.inner.data
    }

    pub(crate) fn engine(&self) -> &Engine {
        &self.inner.engine
    }

    pub(crate) fn as_context(&self) -> StoreContext<'_, T> {
        StoreContext::new(self.inner)
    }
}
