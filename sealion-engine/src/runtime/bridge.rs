use cranelift_codegen::{
    ir::{
        self, AbiParam, ArgumentPurpose, InstBuilder, MemFlags, Signature, Type, condcodes::IntCC,
        types,
    },
    isa::{CallConv, TargetIsa},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};

use crate::Error;

use super::{
    slots::ValueType,
    types::FuncType,
    vm::{VMArrayCallHostFuncContext, VMFuncRef},
};

pub(crate) fn value_type(isa: &dyn TargetIsa, ty: ValueType) -> Type {
    match ty {
        ValueType::I32 => types::I32,
        ValueType::I64 => types::I64,
        ValueType::F32 => types::F32,
        ValueType::F64 => types::F64,
        ValueType::V128 => types::I8X16,
        ValueType::FuncRef | ValueType::ExternRef => isa.pointer_type(),
    }
}

pub(crate) fn blank_sig(isa: &dyn TargetIsa, call_conv: CallConv) -> Signature {
    let pointer_type = isa.pointer_type();
    let mut sig = Signature::new(call_conv);
    sig.params
        .push(AbiParam::special(pointer_type, ArgumentPurpose::VMContext));
    sig.params.push(AbiParam::new(pointer_type));
    sig
}

pub(crate) fn compiled_call_signature(
    isa: &dyn TargetIsa,
    func_ty: &FuncType,
    call_conv: CallConv,
) -> Signature {
    let mut sig = blank_sig(isa, call_conv);
    sig.params.extend(
        func_ty
            .params()
            .iter()
            .map(|ty| AbiParam::new(value_type(isa, *ty))),
    );
    sig.returns.extend(
        func_ty
            .results()
            .iter()
            .map(|ty| AbiParam::new(value_type(isa, *ty))),
    );
    sig
}

pub(crate) fn array_call_signature(isa: &dyn TargetIsa) -> Signature {
    let pointer_type = isa.pointer_type();
    let call_conv = CallConv::triple_default(isa.triple());
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(pointer_type));
    sig.params.push(AbiParam::new(pointer_type));
    sig.params.push(AbiParam::new(pointer_type));
    sig.params.push(AbiParam::new(pointer_type));
    sig.returns.push(AbiParam::new(types::I8));
    sig
}

pub(crate) fn raise_signature(isa: &dyn TargetIsa) -> Signature {
    let mut sig = Signature::new(CallConv::triple_default(isa.triple()));
    sig.params.push(AbiParam::new(isa.pointer_type()));
    sig
}

fn slot_address(builder: &mut FunctionBuilder, base: ir::Value, index: usize) -> ir::Value {
    builder.ins().iadd_imm(base, (index * 16) as i64)
}

fn spill_args(builder: &mut FunctionBuilder, base: ir::Value, args: &[ir::Value], flags: MemFlags) {
    for (index, arg) in args.iter().enumerate() {
        let address = slot_address(builder, base, index);
        builder.ins().store(flags, *arg, address, 0);
    }
}

fn load_values_from_array(
    builder: &mut FunctionBuilder,
    base: ir::Value,
    types: &[Type],
    flags: MemFlags,
) -> Vec<ir::Value> {
    types
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            let address = slot_address(builder, base, index);
            builder.ins().load(*ty, flags, address, 0)
        })
        .collect()
}

fn store_values_to_array(
    builder: &mut FunctionBuilder,
    base: ir::Value,
    values: &[ir::Value],
    flags: MemFlags,
) {
    for (index, value) in values.iter().enumerate() {
        let address = slot_address(builder, base, index);
        builder.ins().store(flags, *value, address, 0);
    }
}

fn slot_flags() -> MemFlags {
    MemFlags::new()
        .with_notrap()
        .with_endianness(ir::Endianness::Little)
}

fn allocate_slots(
    builder: &mut FunctionBuilder,
    func_ty: &FuncType,
    pointer_type: ir::Type,
) -> ir::Value {
    let count = func_ty.params().len().max(func_ty.results().len());
    let bytes = u32::try_from((count * 16).max(16)).unwrap_or(u32::MAX);
    let slot = builder.func.create_sized_stack_slot(ir::StackSlotData::new(
        ir::StackSlotKind::ExplicitSlot,
        bytes,
        4,
    ));
    builder.ins().stack_addr(pointer_type, slot, 0)
}

pub(crate) fn compile_compiled_to_array<M: Module>(
    module: &mut M,
    name: &str,
    func_ty: &FuncType,
    raise: Option<FuncId>,
) -> Result<FuncId, Error> {
    let isa = module.isa();
    let pointer_type = isa.pointer_type();
    let native_sig = compiled_call_signature(isa, func_ty, CallConv::Tail);
    let array_sig = array_call_signature(isa);
    let ir_results: Vec<Type> = func_ty
        .results()
        .iter()
        .map(|ty| value_type(isa, *ty))
        .collect();
    let func_id = module.declare_function(name, Linkage::Local, &native_sig)?;

    let mut func = ir::Function::with_name_signature(ir::UserFuncName::user(0, 0), native_sig);
    let raise_ref = raise.map(|raise| module.declare_func_in_func(raise, &mut func));

    let mut builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut func, &mut builder_ctx);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);

    let params = builder.block_params(entry).to_vec();
    let callee_vmctx = params[0];
    let caller_vmctx = params[1];
    let native_args = &params[2..];
    let flags = slot_flags();
    let base = allocate_slots(&mut builder, func_ty, pointer_type);
    spill_args(&mut builder, base, native_args, flags);

    let count = builder.ins().iconst(
        pointer_type,
        func_ty.params().len().max(func_ty.results().len()) as i64,
    );
    let array_call_offset =
        VMArrayCallHostFuncContext::FUNC_REF_OFFSET + VMFuncRef::ARRAY_CALL_OFFSET;
    let callee = builder
        .ins()
        .load(pointer_type, flags, callee_vmctx, array_call_offset as i32);
    let array_sig = builder.import_signature(array_sig);
    let call = builder.ins().call_indirect(
        array_sig,
        callee,
        &[callee_vmctx, caller_vmctx, base, count],
    );
    let succeeded = builder.inst_results(call)[0];

    if let Some(raise_ref) = raise_ref {
        let failed = builder.create_block();
        let continuation = builder.create_block();
        builder
            .ins()
            .brif(succeeded, continuation, &[], failed, &[]);
        builder.switch_to_block(failed);
        builder.ins().call(raise_ref, &[caller_vmctx]);
        builder.ins().trap(ir::TrapCode::unwrap_user(2));
        builder.switch_to_block(continuation);
    } else {
        builder.ins().trapz(succeeded, ir::TrapCode::unwrap_user(2));
    }

    let results = load_values_from_array(&mut builder, base, &ir_results, flags);
    builder.ins().return_(&results);
    builder.seal_all_blocks();
    builder.finalize();

    let mut context = module.make_context();
    context.func = func;
    module.define_function(func_id, &mut context)?;
    Ok(func_id)
}

pub(crate) fn compile_array_to_compiled<M: Module>(
    module: &mut M,
    name: &str,
    func_ty: &FuncType,
    compiled: FuncId,
) -> Result<FuncId, Error> {
    let isa = module.isa();
    let array_sig = array_call_signature(isa);
    let ir_params: Vec<Type> = func_ty
        .params()
        .iter()
        .map(|ty| value_type(isa, *ty))
        .collect();
    let func_id = module.declare_function(name, Linkage::Local, &array_sig)?;

    let mut func = ir::Function::with_name_signature(ir::UserFuncName::user(0, 0), array_sig);
    let compiled_ref = module.declare_func_in_func(compiled, &mut func);

    let mut builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut func, &mut builder_ctx);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);

    let params = builder.block_params(entry).to_vec();
    let callee_vmctx = params[0];
    let caller_vmctx = params[1];
    let values_ptr = params[2];
    let values_len = params[3];

    let required = func_ty.params().len().max(func_ty.results().len());
    let enough = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        values_len,
        required as i64,
    );
    builder.ins().trapz(enough, ir::TrapCode::unwrap_user(3));

    let flags = slot_flags();
    let mut args = load_values_from_array(&mut builder, values_ptr, &ir_params, flags);
    args.insert(0, caller_vmctx);
    args.insert(0, callee_vmctx);

    let call = builder.ins().call(compiled_ref, &args);
    let results = builder.inst_results(call).to_vec();
    store_values_to_array(&mut builder, values_ptr, &results, flags);

    let success = builder.ins().iconst(types::I8, 1);
    builder.ins().return_(&[success]);
    builder.seal_all_blocks();
    builder.finalize();

    let mut context = module.make_context();
    context.func = func;
    module.define_function(func_id, &mut context)?;
    Ok(func_id)
}
