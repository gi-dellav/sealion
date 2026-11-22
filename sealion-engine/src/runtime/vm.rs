use std::{
    any::{Any, TypeId},
    marker::PhantomData,
    mem::offset_of,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use super::{slots::ValRaw, types::VMSharedTypeIndex};

#[repr(C)]
pub(crate) struct VMOpaqueContext {
    _private: [u8; 0],
}

#[repr(C)]
pub(crate) struct InstanceContext {
    pub(crate) imports: Vec<VMFunctionImport>,
}

#[repr(C)]
pub(crate) struct VMContext {
    pub(crate) store: NonNull<u8>,
    pub(crate) instance: *const InstanceContext,
    pub(crate) store_type: TypeId,
}

impl VMContext {
    pub(crate) fn new(
        store: NonNull<u8>,
        instance: *const InstanceContext,
        store_type: TypeId,
    ) -> Self {
        Self {
            store,
            instance,
            store_type,
        }
    }
}

pub(crate) type VMArrayCallFunction = unsafe extern "C" fn(
    NonNull<VMOpaqueContext>,
    NonNull<VMContext>,
    NonNull<ValRaw>,
    usize,
) -> bool;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct VMFuncRef {
    pub(crate) array_call: VMArrayCallFunction,
    pub(crate) compiled_call: Option<NonNull<u8>>,
    pub(crate) type_index: VMSharedTypeIndex,
    pub(crate) vmctx: NonNull<VMOpaqueContext>,
}

unsafe impl Send for VMFuncRef {}
unsafe impl Sync for VMFuncRef {}

impl VMFuncRef {
    pub(crate) const ARRAY_CALL_OFFSET: usize = offset_of!(Self, array_call);
    pub(crate) const COMPILED_CALL_OFFSET: usize = offset_of!(Self, compiled_call);
    pub(crate) const TYPE_INDEX_OFFSET: usize = offset_of!(Self, type_index);
    pub(crate) const VMCTX_OFFSET: usize = offset_of!(Self, vmctx);

    pub(crate) unsafe fn call_array(
        &self,
        caller_vmctx: NonNull<VMContext>,
        values: NonNull<ValRaw>,
        length: usize,
    ) -> bool {
        unsafe { (self.array_call)(self.vmctx, caller_vmctx, values, length) }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct VMFunctionImport {
    pub(crate) array_call: VMArrayCallFunction,
    pub(crate) compiled_call: NonNull<u8>,
    pub(crate) type_index: VMSharedTypeIndex,
    pub(crate) vmctx: NonNull<VMOpaqueContext>,
}

unsafe impl Send for VMFunctionImport {}
unsafe impl Sync for VMFunctionImport {}

impl VMFunctionImport {
    pub(crate) fn from_func_ref(func_ref: VMFuncRef) -> Option<Self> {
        Some(Self {
            array_call: func_ref.array_call,
            compiled_call: func_ref.compiled_call?,
            type_index: func_ref.type_index,
            vmctx: func_ref.vmctx,
        })
    }
}

pub(crate) struct StoreBox<T> {
    ptr: NonNull<T>,
    _marker: PhantomData<Box<T>>,
}

unsafe impl<T: Send> Send for StoreBox<T> {}
unsafe impl<T: Sync> Sync for StoreBox<T> {}

impl<T> StoreBox<T> {
    pub(crate) fn new(value: T) -> Self {
        let ptr = NonNull::from(Box::leak(Box::new(value)));
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    pub(crate) fn get(&self) -> NonNull<T> {
        self.ptr
    }

    pub(crate) fn as_ref(&self) -> &T {
        unsafe { self.ptr.as_ref() }
    }

    pub(crate) fn as_mut(&mut self) -> &mut T {
        unsafe { self.ptr.as_mut() }
    }
}

impl<T> Deref for StoreBox<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T> DerefMut for StoreBox<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

impl<T> Drop for StoreBox<T> {
    fn drop(&mut self) {
        unsafe {
            drop(Box::from_raw(self.ptr.as_ptr()));
        }
    }
}

pub(crate) const VM_ARRAY_CALL_HOST_FUNC_MAGIC: u32 = 0x5345_4148;

#[repr(C)]
pub(crate) struct VMArrayCallHostFuncContext {
    magic: u32,
    pub(crate) func_ref: VMFuncRef,
    host_state: Box<dyn Any + Send + Sync>,
}

impl VMArrayCallHostFuncContext {
    pub(crate) const FUNC_REF_OFFSET: usize = offset_of!(Self, func_ref);

    pub(crate) unsafe fn new(
        host_func: VMArrayCallFunction,
        type_index: VMSharedTypeIndex,
        host_state: Box<dyn Any + Send + Sync>,
    ) -> StoreBox<Self> {
        let context = StoreBox::new(Self {
            magic: VM_ARRAY_CALL_HOST_FUNC_MAGIC,
            func_ref: VMFuncRef {
                array_call: host_func,
                compiled_call: None,
                type_index,
                vmctx: NonNull::dangling(),
            },
            host_state,
        });

        let vmctx: NonNull<VMOpaqueContext> = context.get().cast();
        unsafe {
            context.get().as_mut().func_ref.vmctx = vmctx;
        }
        context
    }

    pub(crate) unsafe fn from_opaque(ptr: NonNull<VMOpaqueContext>) -> NonNull<Self> {
        let context = ptr.cast::<Self>();
        debug_assert_eq!(
            unsafe { context.as_ref().magic },
            VM_ARRAY_CALL_HOST_FUNC_MAGIC
        );
        context
    }

    pub(crate) fn host_state(&self) -> &(dyn Any + Send + Sync) {
        &*self.host_state
    }
}
