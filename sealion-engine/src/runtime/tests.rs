use std::{
    mem::{align_of, offset_of, size_of},
    ptr::NonNull,
    sync::Arc,
};

use crate::{Engine, Error};

use super::{
    Caller, FuncType, HostFunc, Import, Instance, IntoFunc, Linker, Module, Store, Trap,
    VMArrayCallHostFuncContext, VMFuncRef, VMFunctionImport, ValRaw, ValueRet, ValueTyList,
    ValueType, WithoutCaller,
};

fn func_type(params: Vec<ValueType>, results: Vec<ValueType>) -> FuncType {
    FuncType::new(params, results)
}

fn simple_type(engine: &Engine) -> super::RegisteredType {
    engine
        .register_type(func_type(vec![ValueType::I32], vec![ValueType::I32]))
        .unwrap()
}

#[test]
fn val_raw_is_little_endian_and_sixteen_bytes() {
    assert_eq!(size_of::<ValRaw>(), 16);
    assert_eq!(align_of::<ValRaw>(), 16);

    assert_eq!(ValRaw::i32(-7).get_i32(), -7);
    assert_eq!(ValRaw::i64(-9).get_i64(), -9);
    assert_eq!(ValRaw::f32(1.5).get_f32(), 1.5);
    assert_eq!(ValRaw::f64(-2.25).get_f64(), -2.25);
    assert_eq!(ValRaw::v128(u128::MAX).get_v128(), u128::MAX);
    assert_eq!(
        ValRaw::funcref(None).get_funcref().map(|ptr| ptr.as_ptr()),
        None
    );
}

#[test]
fn typed_lists_roundtrip_through_slots() {
    let mut slots = vec![ValRaw::zero(); 4];
    let params = (7i32, -3i64, 2.5f64, true);
    <(i32, i64, f64, bool) as ValueTyList>::store(params, &mut slots);
    let loaded = unsafe { <(i32, i64, f64, bool) as ValueTyList>::load(&slots) };
    assert_eq!(loaded, (7, -3, 2.5, true));

    let results = (11u32, 12u64);
    <(u32, u64) as ValueRet>::store(results, &mut slots);
    let loaded = unsafe { <(u32, u64) as ValueRet>::load(&slots) };
    assert_eq!(loaded, (11, 12));
}

#[test]
fn function_types_are_interned_per_engine() {
    let engine = Engine::new();
    let first = engine
        .register_type(func_type(
            vec![ValueType::I32, ValueType::F64],
            vec![ValueType::I64],
        ))
        .unwrap();
    let second = engine
        .register_type(func_type(
            vec![ValueType::I32, ValueType::F64],
            vec![ValueType::I64],
        ))
        .unwrap();
    let other = simple_type(&engine);

    assert_eq!(first.index(), second.index());
    assert_eq!(first.ty(), second.ty());
    assert_ne!(first.index(), other.index());
}

#[test]
fn vm_records_have_identical_layouts() {
    assert_eq!(size_of::<VMFuncRef>(), size_of::<VMFunctionImport>());
    assert_eq!(
        offset_of!(VMFuncRef, compiled_call),
        offset_of!(VMFunctionImport, compiled_call)
    );
    assert_eq!(
        offset_of!(VMFuncRef, type_index),
        offset_of!(VMFunctionImport, type_index)
    );
    assert_eq!(
        offset_of!(VMFuncRef, vmctx),
        offset_of!(VMFunctionImport, vmctx)
    );
    assert_eq!(
        VMArrayCallHostFuncContext::FUNC_REF_OFFSET,
        offset_of!(VMArrayCallHostFuncContext, func_ref)
    );

    #[cfg(target_pointer_width = "64")]
    {
        assert_eq!(size_of::<VMFuncRef>(), 32);
        assert_eq!(size_of::<VMArrayCallHostFuncContext>(), 56);
        assert_eq!(VMArrayCallHostFuncContext::FUNC_REF_OFFSET, 8);
    }
}

#[test]
fn host_functions_marshal_typed_values_and_caller_data() {
    #[derive(Default)]
    struct State {
        calls: usize,
    }

    let engine = Engine::new();
    let add = HostFunc::wrap(
        &engine,
        |mut caller: Caller<'_, State>, (a, b): (i32, i32)| -> i32 {
            caller.data_mut().calls += 1;
            a.wrapping_add(b)
        },
    )
    .unwrap();
    let mut store = Store::new(&engine, State::default());

    let result: i32 = add.call_typed(&mut store, (20i32, 22i32)).unwrap();
    assert_eq!(result, 42);
    assert_eq!(store.data().calls, 1);

    let pair = HostFunc::wrap(
        &engine,
        |_: Caller<'_, State>, (a, b): (f64, f64)| -> (f64, bool) { (a * b, a > b) },
    )
    .unwrap();
    let result: (f64, bool) = pair.call_typed(&mut store, (2.5f64, 4.0f64)).unwrap();
    assert_eq!(result, (10.0, false));
    assert_eq!(store.data().calls, 1);
}

#[test]
fn into_func_supports_closures_without_a_caller() {
    fn add(a: i32, b: i32) -> i32 {
        a.wrapping_add(b)
    }

    let engine = Engine::new();
    let host = <fn(i32, i32) -> i32 as IntoFunc<(), (i32, i32), i32, WithoutCaller>>::into_func(
        add, &engine,
    )
    .unwrap();
    let mut store = Store::new(&engine, ());
    let result: i32 = host.call_typed(&mut store, (9i32, 33i32)).unwrap();
    assert_eq!(result, 42);
}

#[test]
fn host_failures_use_the_thread_local_trap_channel() {
    let engine = Engine::new();
    let mut store = Store::new(&engine, ());

    let failing = HostFunc::wrap_fallible(&engine, |_: Caller<'_, ()>, ()| -> Result<i32, Trap> {
        Err(Trap::new("host failure"))
    })
    .unwrap();
    let error = failing
        .call_typed::<(), (), i32>(&mut store, ())
        .unwrap_err();
    match error {
        Error::Trap(message) => assert!(message.contains("host failure")),
        other => panic!("unexpected error: {other}"),
    }
    assert!(!super::has_unwind());

    let panicking = HostFunc::wrap(&engine, |_: Caller<'_, ()>, ()| -> i32 {
        panic!("host panic")
    })
    .unwrap();
    let error = panicking
        .call_typed::<(), (), i32>(&mut store, ())
        .unwrap_err();
    match error {
        Error::Trap(message) => assert!(message.contains("host panic")),
        other => panic!("unexpected error: {other}"),
    }
    assert!(!super::has_unwind());
}

#[test]
fn host_calls_reject_the_wrong_signature() {
    let engine = Engine::new();
    let host = HostFunc::wrap(&engine, |_: Caller<'_, ()>, (value,): (i32,)| -> i32 {
        value
    })
    .unwrap();
    let mut store = Store::new(&engine, ());
    assert!(matches!(
        host.call_typed::<(), (i64,), i64>(&mut store, (1i64,)),
        Err(Error::Host(_))
    ));
}

#[test]
fn holes_are_filled_when_a_module_is_registered() {
    let engine = Engine::new();
    let ty = simple_type(&engine);
    let host = Arc::new(
        HostFunc::wrap(&engine, |_: Caller<'_, ()>, (value,): (i32,)| -> i32 {
            value
        })
        .unwrap(),
    );
    let mut store = Store::new(&engine, ());

    let func_ref = store.register_host_func(&host);
    assert!(unsafe { func_ref.as_ref().compiled_call }.is_none());

    let mut module = Module::new(Vec::new());
    module.register_bridge(ty.index(), NonNull::dangling());
    store.register_module(&module);
    assert!(unsafe { func_ref.as_ref().compiled_call }.is_some());
}

#[test]
fn patches_are_isolated_per_store() {
    let engine = Engine::new();
    let ty = simple_type(&engine);
    let host = Arc::new(
        HostFunc::wrap(&engine, |_: Caller<'_, ()>, (value,): (i32,)| -> i32 {
            value
        })
        .unwrap(),
    );
    let mut first = Store::new(&engine, ());
    let mut second = Store::new(&engine, ());

    let first_ref = first.register_host_func(&host);
    let second_ref = second.register_host_func(&host);
    assert!(unsafe { first_ref.as_ref().compiled_call }.is_none());
    assert!(unsafe { second_ref.as_ref().compiled_call }.is_none());

    let mut module = Module::new(Vec::new());
    module.register_bridge(ty.index(), NonNull::dangling());
    first.register_module(&module);
    assert!(unsafe { first_ref.as_ref().compiled_call }.is_some());
    assert!(unsafe { second_ref.as_ref().compiled_call }.is_none());

    second.register_module(&module);
    assert!(unsafe { second_ref.as_ref().compiled_call }.is_some());
}

#[test]
fn linking_checks_types_and_publishes_import_records() {
    let engine = Engine::new();
    let ty = simple_type(&engine);
    let host = Arc::new(
        HostFunc::wrap(&engine, |_: Caller<'_, ()>, (value,): (i32,)| -> i32 {
            value.wrapping_add(1)
        })
        .unwrap(),
    );

    let mut linker = Linker::new(&engine);
    linker.define("math", "inc", host.clone());

    let import = Import {
        module: "math".to_string(),
        name: "inc".to_string(),
        ty: ty.clone(),
    };
    let mut module = Module::new(vec![import.clone()]);
    module.register_bridge(ty.index(), NonNull::dangling());

    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &module).unwrap();
    assert_eq!(instance.imports().len(), 1);
    let record = instance.imports()[0];
    assert_eq!(record.type_index, ty.index());
    assert_eq!(
        record.array_call as usize,
        host.func_ref().array_call as usize
    );
    assert_eq!(record.vmctx, host.func_ref().vmctx);

    let other_type = engine
        .register_type(func_type(vec![ValueType::I64], vec![ValueType::I64]))
        .unwrap();
    let mismatch = Module::new(vec![Import {
        module: "math".to_string(),
        name: "inc".to_string(),
        ty: other_type,
    }]);
    assert!(matches!(
        linker.instantiate(&mut store, &mismatch),
        Err(Error::Linker(_))
    ));

    let missing = Module::new(vec![Import {
        module: "math".to_string(),
        name: "missing".to_string(),
        ty: ty.clone(),
    }]);
    assert!(matches!(
        linker.instantiate(&mut store, &missing),
        Err(Error::Linker(_))
    ));

    let no_bridge = Module::new(vec![Import {
        module: "math".to_string(),
        name: "inc".to_string(),
        ty: ty.clone(),
    }]);
    let mut fresh_store = Store::new(&engine, ());
    assert!(matches!(
        linker.instantiate(&mut fresh_store, &no_bridge),
        Err(Error::Linker(_))
    ));
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod jit {
    use cranelift_codegen::{
        ir::{AbiParam, InstBuilder, UserFuncName, types},
        isa::{self, CallConv},
        settings::{self, Configurable},
    };
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use cranelift_module::{FuncId, Linkage, Module as CraneliftModule, default_libcall_names};
    use cranelift_object::{ObjectBuilder, ObjectModule};
    use target_lexicon::Triple;

    use crate::{CompileOptions, Engine};

    use super::{
        Caller, FuncType, HostFunc, Instance, Module, Store, VMFuncRef, ValRaw, ValueType,
        func_type,
    };
    use crate::runtime::bridge::{
        compile_array_to_compiled, compile_compiled_to_array, compiled_call_signature,
    };
    use crate::runtime::{VMArrayCallFunction, VMContext, VMOpaqueContext};

    fn object_module() -> ObjectModule {
        let mut flags = settings::builder();
        flags.set("is_pic", "false").unwrap();
        flags.set("opt_level", "none").unwrap();
        let flags = settings::Flags::new(flags);
        let isa = isa::lookup(Triple::host()).unwrap().finish(flags).unwrap();
        let builder = ObjectBuilder::new(isa, "runtime_test", default_libcall_names()).unwrap();
        let mut module = ObjectModule::new(builder);
        define_main(&mut module);
        module
    }

    fn define_main(module: &mut ObjectModule) {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I32));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I32));
        let id = module
            .declare_function("main", Linkage::Export, &sig)
            .unwrap();

        let mut func = cranelift_codegen::ir::Function::with_name_signature(
            UserFuncName::testcase("main"),
            sig,
        );
        let mut context = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let zero = builder.ins().iconst(types::I32, 0);
        builder.ins().return_(&[zero]);
        builder.seal_all_blocks();
        builder.finalize();

        let mut context = module.make_context();
        context.func = func;
        module.define_function(id, &mut context).unwrap();
    }

    fn define_add_target(module: &mut ObjectModule, func_ty: &FuncType) -> FuncId {
        let sig = compiled_call_signature(module.isa(), func_ty, CallConv::Tail);
        let id = module
            .declare_function("target", Linkage::Export, &sig)
            .unwrap();

        let mut func = cranelift_codegen::ir::Function::with_name_signature(
            UserFuncName::testcase("target"),
            sig,
        );
        let mut context = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let params = builder.block_params(entry).to_vec();
        let result = builder.ins().iadd(params[2], params[3]);
        builder.ins().return_(&[result]);
        builder.seal_all_blocks();
        builder.finalize();

        let mut context = module.make_context();
        context.func = func;
        module.define_function(id, &mut context).unwrap();
        id
    }

    fn finish(module: ObjectModule) -> crate::jit::Module {
        let object = module.finish().emit().unwrap();
        let artifact =
            crate::jit::build(&object, &CompileOptions::default(), &Triple::host(), 0).unwrap();
        crate::jit::load(&artifact).unwrap()
    }

    fn function_address(module: &crate::jit::Module, name: &str) -> usize {
        let key = module.function_by_name(name).unwrap();
        module.function_address(key).unwrap()
    }

    #[test]
    fn compiled_bridge_calls_the_rust_host_trampoline() {
        #[derive(Default)]
        struct State {
            calls: usize,
        }

        let engine = Engine::new();
        let ty = engine
            .register_type(func_type(
                vec![ValueType::I32, ValueType::I32],
                vec![ValueType::I32],
            ))
            .unwrap();

        let mut module = object_module();
        compile_compiled_to_array(&mut module, "host_bridge", ty.ty(), None).unwrap();
        let jit = finish(module);
        let bridge = function_address(&jit, "host_bridge");

        let host = std::sync::Arc::new(
            HostFunc::wrap(
                &engine,
                |mut caller: Caller<'_, State>, (a, b): (i32, i32)| -> i32 {
                    caller.data_mut().calls += 1;
                    a.wrapping_add(b)
                },
            )
            .unwrap(),
        );
        let mut store = Store::new(&engine, State::default());
        let mut runtime_module = Module::new(Vec::new());
        runtime_module.register_bridge(
            ty.index(),
            std::ptr::NonNull::new(bridge as *mut u8).unwrap(),
        );
        store.register_module(&runtime_module);

        let func_ref = unsafe { *store.register_host_func(&host).as_ptr() };
        let compiled: unsafe extern "C" fn(
            std::ptr::NonNull<VMOpaqueContext>,
            std::ptr::NonNull<VMContext>,
            i32,
            i32,
        ) -> i32 = unsafe { std::mem::transmute(func_ref.compiled_call.unwrap()) };
        let mut vmctx = store.vmctx(&Instance::empty());
        let result =
            unsafe { compiled(func_ref.vmctx, std::ptr::NonNull::from(&mut vmctx), 20, 22) };
        assert_eq!(result, 42);
        assert_eq!(store.data().calls, 1);
    }

    #[test]
    fn array_bridge_calls_a_compiled_function() {
        let engine = Engine::new();
        let ty = engine
            .register_type(func_type(
                vec![ValueType::I32, ValueType::I32],
                vec![ValueType::I32],
            ))
            .unwrap();

        let mut module = object_module();
        let target = define_add_target(&mut module, ty.ty());
        compile_array_to_compiled(&mut module, "array_bridge", ty.ty(), target).unwrap();
        let jit = finish(module);
        let bridge = function_address(&jit, "array_bridge");
        let array_call: VMArrayCallFunction = unsafe { std::mem::transmute(bridge as *mut u8) };

        let func_ref = VMFuncRef {
            array_call,
            compiled_call: None,
            type_index: ty.index(),
            vmctx: std::ptr::NonNull::dangling(),
        };
        let mut store = Store::new(&engine, ());
        let mut vmctx = store.vmctx(&Instance::empty());
        let mut slots = [ValRaw::i32(20), ValRaw::i32(22)];
        let ok = unsafe {
            func_ref.call_array(
                std::ptr::NonNull::from(&mut vmctx),
                std::ptr::NonNull::new(slots.as_mut_ptr()).unwrap(),
                slots.len(),
            )
        };
        assert!(ok);
        assert_eq!(slots[0].get_i32(), 42);
    }

    #[test]
    fn compiled_bridge_handles_empty_signatures() {
        #[derive(Default)]
        struct State {
            calls: usize,
        }

        let engine = Engine::new();
        let ty = engine.register_type(func_type(vec![], vec![])).unwrap();

        let mut module = object_module();
        compile_compiled_to_array(&mut module, "empty_bridge", ty.ty(), None).unwrap();
        let jit = finish(module);
        let bridge = function_address(&jit, "empty_bridge");

        let host = std::sync::Arc::new(
            HostFunc::wrap(&engine, |mut caller: Caller<'_, State>, ()| {
                caller.data_mut().calls += 1;
            })
            .unwrap(),
        );
        let mut store = Store::new(&engine, State::default());
        let mut runtime_module = Module::new(Vec::new());
        runtime_module.register_bridge(
            ty.index(),
            std::ptr::NonNull::new(bridge as *mut u8).unwrap(),
        );
        store.register_module(&runtime_module);

        let func_ref = unsafe { *store.register_host_func(&host).as_ptr() };
        let compiled: unsafe extern "C" fn(
            std::ptr::NonNull<VMOpaqueContext>,
            std::ptr::NonNull<VMContext>,
        ) = unsafe { std::mem::transmute(func_ref.compiled_call.unwrap()) };
        let mut vmctx = store.vmctx(&Instance::empty());
        unsafe {
            compiled(func_ref.vmctx, std::ptr::NonNull::from(&mut vmctx));
        }
        assert_eq!(store.data().calls, 1);
    }
}
