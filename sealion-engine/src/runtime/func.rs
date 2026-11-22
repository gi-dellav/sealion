use std::{
    any::TypeId,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr::NonNull,
};

use crate::{Engine, Error};

use super::{
    linker::Instance,
    slots::{ValRaw, ValueRet, ValueTy, ValueTyList},
    store::{Store, StoreContextMut, StoreInner},
    trap::{Trap, UnwindReason, record_unwind, take_unwind},
    types::{FuncType, RegisteredType, VMSharedTypeIndex},
    vm::{StoreBox, VMArrayCallHostFuncContext, VMContext, VMFuncRef, VMOpaqueContext},
};

type HostFuncClosure<T> =
    Box<dyn Fn(Caller<'_, T>, &mut [ValRaw]) -> Result<(), Trap> + Send + Sync>;

pub(crate) struct HostFuncState<T: 'static> {
    func: HostFuncClosure<T>,
    _ty: RegisteredType,
}

pub(crate) struct HostFunc {
    ctx: StoreBox<VMArrayCallHostFuncContext>,
    ty: RegisteredType,
    data_type: TypeId,
}

impl HostFunc {
    pub(crate) fn wrap<T, F, P, R>(engine: &Engine, func: F) -> Result<Self, Error>
    where
        T: 'static,
        F: Fn(Caller<'_, T>, P) -> R + Send + Sync + 'static,
        P: ValueTyList,
        R: ValueRet,
    {
        let ty = engine.register_type(FuncType::new(P::valtypes(), R::valtypes()))?;
        let state = HostFuncState {
            func: Box::new(move |caller, slots| {
                let params = unsafe { P::load(slots) };
                let result = func(caller, params);
                R::store(result, slots);
                Ok(())
            }),
            _ty: ty.clone(),
        };
        let ctx = unsafe {
            VMArrayCallHostFuncContext::new(array_call_trampoline::<T>, ty.index(), Box::new(state))
        };
        Ok(Self {
            ctx,
            ty,
            data_type: TypeId::of::<T>(),
        })
    }

    pub(crate) fn wrap_fallible<T, F, P, R>(engine: &Engine, func: F) -> Result<Self, Error>
    where
        T: 'static,
        F: Fn(Caller<'_, T>, P) -> Result<R, Trap> + Send + Sync + 'static,
        P: ValueTyList,
        R: ValueRet,
    {
        let ty = engine.register_type(FuncType::new(P::valtypes(), R::valtypes()))?;
        let state = HostFuncState {
            func: Box::new(move |caller, slots| {
                let params = unsafe { P::load(slots) };
                let result = func(caller, params)?;
                R::store(result, slots);
                Ok(())
            }),
            _ty: ty.clone(),
        };
        let ctx = unsafe {
            VMArrayCallHostFuncContext::new(array_call_trampoline::<T>, ty.index(), Box::new(state))
        };
        Ok(Self {
            ctx,
            ty,
            data_type: TypeId::of::<T>(),
        })
    }

    pub(crate) fn func_ref(&self) -> &VMFuncRef {
        &self.ctx.func_ref
    }

    pub(crate) fn type_index(&self) -> VMSharedTypeIndex {
        self.ty.index()
    }

    pub(crate) fn registered_type(&self) -> &RegisteredType {
        &self.ty
    }

    pub(crate) fn data_type(&self) -> TypeId {
        self.data_type
    }

    pub(crate) fn call_typed<T: 'static, P, R>(
        &self,
        store: &mut Store<T>,
        params: P,
    ) -> Result<R, Error>
    where
        P: ValueTyList,
        R: ValueRet,
    {
        if TypeId::of::<T>() != self.data_type {
            return Err(Error::Host(
                "host function belongs to a different store data type".to_string(),
            ));
        }
        let ty = self.ty.ty();
        if P::valtypes().as_slice() != ty.params() || R::valtypes().as_slice() != ty.results() {
            return Err(Error::Host(
                "host function called with the wrong signature".to_string(),
            ));
        }

        let capacity = ty.params().len().max(ty.results().len());
        let mut slots = store.take_slot_storage();
        slots.clear();
        slots.resize(capacity, ValRaw::zero());
        params.store(&mut slots);

        let instance = Instance::empty();
        let mut vmctx = store.vmctx(&instance);
        let values = NonNull::new(slots.as_mut_ptr()).expect("value buffer is not null");
        let ok = unsafe {
            self.func_ref()
                .call_array(NonNull::from(&mut vmctx), values, capacity)
        };

        if ok {
            let result = unsafe { R::load(&slots) };
            store.return_slot_storage(slots);
            Ok(result)
        } else {
            store.return_slot_storage(slots);
            let error = take_unwind()
                .map(UnwindReason::into_error)
                .unwrap_or_else(|| Error::Trap("host call failed without recording a trap".into()));
            Err(error)
        }
    }
}

pub(crate) struct Caller<'a, T: 'static> {
    pub(crate) store: StoreContextMut<'a, T>,
    caller: Instance,
}

impl<'a, T> Caller<'a, T> {
    pub(crate) fn new(store: StoreContextMut<'a, T>, caller: Instance) -> Self {
        Self { store, caller }
    }

    pub(crate) fn data(&self) -> &T {
        self.store.data()
    }

    pub(crate) fn data_mut(&mut self) -> &mut T {
        self.store.data_mut()
    }

    pub(crate) fn engine(&self) -> &Engine {
        self.store.engine()
    }

    pub(crate) fn instance(&self) -> &Instance {
        &self.caller
    }

    unsafe fn from_vmctx(vmctx: NonNull<VMContext>) -> Self {
        let context = unsafe { vmctx.as_ref() };
        assert_eq!(
            context.store_type,
            TypeId::of::<T>(),
            "host function called with the wrong store data type"
        );
        let store = unsafe { context.store.cast::<StoreInner<T>>().as_mut() };
        let instance = unsafe { Instance::from_context_ptr(context.instance) };
        Self {
            store: StoreContextMut::new(store),
            caller: instance,
        }
    }
}

unsafe extern "C" fn array_call_trampoline<T: 'static>(
    callee_vmctx: NonNull<VMOpaqueContext>,
    caller_vmctx: NonNull<VMContext>,
    args: NonNull<ValRaw>,
    args_len: usize,
) -> bool {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let context = unsafe { VMArrayCallHostFuncContext::from_opaque(callee_vmctx) };
        let state = unsafe { context.as_ref().host_state() }
            .downcast_ref::<HostFuncState<T>>()
            .expect("host function state has the wrong type");
        let caller = unsafe { Caller::from_vmctx(caller_vmctx) };
        let slots = unsafe { std::slice::from_raw_parts_mut(args.as_ptr(), args_len) };
        (state.func)(caller, slots)
    }));

    match result {
        Ok(Ok(())) => true,
        Ok(Err(trap)) => {
            record_unwind(UnwindReason::Trap(trap));
            false
        }
        Err(payload) => {
            record_unwind(UnwindReason::Panic(payload));
            false
        }
    }
}

pub(crate) struct WithCaller;
pub(crate) struct WithoutCaller;

pub(crate) trait IntoFunc<T, P, R, M> {
    fn into_func(self, engine: &Engine) -> Result<HostFunc, Error>;
}

impl<T, F, R> IntoFunc<T, (), R, WithCaller> for F
where
    T: 'static,
    F: Fn(Caller<'_, T>) -> R + Send + Sync + 'static,
    R: ValueRet,
{
    fn into_func(self, engine: &Engine) -> Result<HostFunc, Error> {
        HostFunc::wrap(engine, move |caller: Caller<'_, T>, ()| self(caller))
    }
}

impl<T, F, R> IntoFunc<T, (), R, WithoutCaller> for F
where
    T: 'static,
    F: Fn() -> R + Send + Sync + 'static,
    R: ValueRet,
{
    fn into_func(self, engine: &Engine) -> Result<HostFunc, Error> {
        HostFunc::wrap(engine, move |_caller: Caller<'_, T>, ()| self())
    }
}

macro_rules! impl_into_func_with_caller {
    ($(($ty:ident, $var:ident)),+) => {
        impl<T, Func, R, $($ty),+> IntoFunc<T, ($($ty,)+), R, WithCaller> for Func
        where
            T: 'static,
            Func: Fn(Caller<'_, T>, $($ty),+) -> R + Send + Sync + 'static,
            $($ty: ValueTy,)+
            R: ValueRet,
        {
            fn into_func(self, engine: &Engine) -> Result<HostFunc, Error> {
                HostFunc::wrap(
                    engine,
                    move |caller: Caller<'_, T>, ($($var,)+): ($($ty,)+)| {
                        self(caller, $($var),+)
                    },
                )
            }
        }
    };
}

macro_rules! impl_into_func_without_caller {
    ($(($ty:ident, $var:ident)),+) => {
        impl<T, Func, R, $($ty),+> IntoFunc<T, ($($ty,)+), R, WithoutCaller> for Func
        where
            T: 'static,
            Func: Fn($($ty),+) -> R + Send + Sync + 'static,
            $($ty: ValueTy,)+
            R: ValueRet,
        {
            fn into_func(self, engine: &Engine) -> Result<HostFunc, Error> {
                HostFunc::wrap(
                    engine,
                    move |_caller: Caller<'_, T>, ($($var,)+): ($($ty,)+)| {
                        self($($var),+)
                    },
                )
            }
        }
    };
}

macro_rules! impl_into_funcs {
    ($(($ty:ident, $var:ident)),+) => {
        impl_into_func_with_caller!($(($ty, $var)),+);
        impl_into_func_without_caller!($(($ty, $var)),+);
    };
}

impl_into_funcs!((A, a));
impl_into_funcs!((A, a), (B, b));
impl_into_funcs!((A, a), (B, b), (C, c));
impl_into_funcs!((A, a), (B, b), (C, c), (D, d));
impl_into_funcs!((A, a), (B, b), (C, c), (D, d), (E, e));
impl_into_funcs!((A, a), (B, b), (C, c), (D, d), (E, e), (F, f));
impl_into_funcs!((A, a), (B, b), (C, c), (D, d), (E, e), (F, f), (G, g));
impl_into_funcs!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h)
);
impl_into_funcs!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i)
);
impl_into_funcs!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j)
);
impl_into_funcs!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k)
);
impl_into_funcs!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l)
);
impl_into_funcs!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l),
    (M, m)
);
impl_into_funcs!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l),
    (M, m),
    (N, n)
);
impl_into_funcs!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l),
    (M, m),
    (N, n),
    (O, o)
);
impl_into_funcs!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l),
    (M, m),
    (N, n),
    (O, o),
    (P, p)
);
