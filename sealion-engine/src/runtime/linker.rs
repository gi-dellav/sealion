use std::{any::TypeId, collections::HashMap, ptr::NonNull, sync::Arc};

use crate::{Engine, Error};

use super::{
    func::HostFunc,
    store::Store,
    types::{RegisteredType, VMSharedTypeIndex},
    vm::{InstanceContext, VMFuncRef, VMFunctionImport},
};

#[derive(Clone, Debug)]
pub(crate) struct Import {
    pub(crate) module: String,
    pub(crate) name: String,
    pub(crate) ty: RegisteredType,
}

#[derive(Clone, Default)]
pub(crate) struct Module {
    imports: Vec<Import>,
    bridges: HashMap<VMSharedTypeIndex, NonNull<u8>>,
}

impl Module {
    pub(crate) fn new(imports: Vec<Import>) -> Self {
        Self {
            imports,
            bridges: HashMap::new(),
        }
    }

    pub(crate) fn imports(&self) -> &[Import] {
        &self.imports
    }

    pub(crate) fn register_bridge(&mut self, index: VMSharedTypeIndex, bridge: NonNull<u8>) {
        self.bridges.insert(index, bridge);
    }

    pub(crate) fn bridges(&self) -> &HashMap<VMSharedTypeIndex, NonNull<u8>> {
        &self.bridges
    }

    fn compiled_to_array_trampoline(&self, index: VMSharedTypeIndex) -> Option<NonNull<u8>> {
        self.bridges.get(&index).copied()
    }
}

#[derive(Clone)]
pub(crate) enum Definition {
    HostFunc(Arc<HostFunc>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DefinitionType {
    Func(VMSharedTypeIndex),
}

impl Definition {
    fn ty(&self) -> DefinitionType {
        match self {
            Self::HostFunc(func) => DefinitionType::Func(func.type_index()),
        }
    }

    fn host_func(&self) -> &Arc<HostFunc> {
        match self {
            Self::HostFunc(func) => func,
        }
    }
}

pub(crate) struct Linker {
    engine: Engine,
    definitions: HashMap<(String, String), Definition>,
}

impl Linker {
    pub(crate) fn new(engine: &Engine) -> Self {
        Self {
            engine: engine.clone(),
            definitions: HashMap::new(),
        }
    }

    pub(crate) fn engine(&self) -> &Engine {
        &self.engine
    }

    pub(crate) fn define(&mut self, module: &str, name: &str, func: Arc<HostFunc>) {
        self.definitions.insert(
            (module.to_string(), name.to_string()),
            Definition::HostFunc(func),
        );
    }

    pub(crate) fn instantiate_pre<T: 'static>(
        &self,
        store: &mut Store<T>,
        module: &Module,
    ) -> Result<InstancePre, Error> {
        let mut imports = Vec::with_capacity(module.imports().len());
        for import in module.imports() {
            let key = (import.module.clone(), import.name.clone());
            let definition = match self.definitions.get(&key) {
                Some(definition) => definition,
                None => {
                    return Err(Error::Linker(format!(
                        "unknown import '{}::{}'",
                        import.module, import.name
                    )));
                }
            };

            let expected = DefinitionType::Func(import.ty.index());
            if definition.ty() != expected {
                return Err(Error::Linker(format!(
                    "type mismatch for import '{}::{}'",
                    import.module, import.name
                )));
            }

            let host = definition.host_func();
            if host.data_type() != TypeId::of::<T>() {
                return Err(Error::Linker(format!(
                    "store data type does not match import '{}::{}'",
                    import.module, import.name
                )));
            }
            let mut func_ref = unsafe { *store.register_host_func(host).as_ptr() };
            if func_ref.compiled_call.is_none() {
                func_ref.compiled_call = module.compiled_to_array_trampoline(func_ref.type_index);
            }
            imports.push(func_ref);
        }

        Ok(InstancePre {
            module: module.clone(),
            imports,
        })
    }

    pub(crate) fn instantiate<T: 'static>(
        &self,
        store: &mut Store<T>,
        module: &Module,
    ) -> Result<Instance, Error> {
        self.instantiate_pre(store, module)?.instantiate(store)
    }
}

pub(crate) struct InstancePre {
    module: Module,
    imports: Vec<VMFuncRef>,
}

impl InstancePre {
    pub(crate) fn instantiate<T: 'static>(self, store: &mut Store<T>) -> Result<Instance, Error> {
        store.register_module(&self.module);

        let mut imports = Vec::with_capacity(self.imports.len());
        for func_ref in self.imports {
            let compiled_call = func_ref.compiled_call.ok_or_else(|| {
                Error::Linker(
                    "host function has no compiled entry point for this module".to_string(),
                )
            })?;
            imports.push(VMFunctionImport {
                array_call: func_ref.array_call,
                compiled_call,
                type_index: func_ref.type_index,
                vmctx: func_ref.vmctx,
            });
        }

        Ok(Instance::from_imports(imports))
    }
}

#[derive(Clone)]
pub(crate) struct Instance {
    ptr: *const InstanceContext,
    owner: Option<Arc<InstanceContext>>,
}

impl Instance {
    pub(crate) fn empty() -> Self {
        Self {
            ptr: std::ptr::null(),
            owner: None,
        }
    }

    fn from_imports(imports: Vec<VMFunctionImport>) -> Self {
        let owner = Arc::new(InstanceContext { imports });
        Self {
            ptr: Arc::as_ptr(&owner),
            owner: Some(owner),
        }
    }

    pub(crate) unsafe fn from_context_ptr(ptr: *const InstanceContext) -> Self {
        Self { ptr, owner: None }
    }

    pub(crate) fn context_ptr(&self) -> *const InstanceContext {
        self.ptr
    }

    pub(crate) fn imports(&self) -> &[VMFunctionImport] {
        if self.ptr.is_null() {
            &[]
        } else {
            unsafe { &(*self.ptr).imports }
        }
    }
}
