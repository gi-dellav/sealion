use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use cranelift_codegen::ir::{
    Block, FuncRef, Function, InstBuilder, MemFlags, SigRef, Signature, UserFuncName, Value,
    condcodes::IntCC, types,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{DataId, FuncId, Module};
use cranelift_object::ObjectModule;
use oxc_ast::ast::BlockStatement;

use crate::{
    Error, IrFunction,
    builtins::Builtins,
    plan::{FunctionBodyPlan, Program, ScopeId},
    value,
};

mod array;
mod call;
mod chain;
mod class;
mod expr;
mod flow;
mod loops;
mod numeric;
mod object;
mod signature;
mod stmt;
mod support;

pub(crate) use signature::*;

use expr::{compile_default, compile_expr};
use flow::emit_exception_check;
use numeric::analyze_numeric_bindings;
use stmt::compile_statements;
use support::{
    alloc_env, declare_vars, hidden_value, init_local_slots, init_slots, new_closure, store_var,
};

/// Bytes per inline cache: the receiver shape and the slot index it served.
pub const INLINE_CACHE_SIZE: usize = 16;
/// The pool is sized from the module instead of being a fixed constant, so a
/// small program does not carry a large zeroed data section in its artifact.
pub const INLINE_CACHE_MIN_SLOTS: usize = 256;
pub const INLINE_CACHE_MAX_SLOTS: usize = 4096;

/// Eight access sites per distinct string literal, clamped. A module with more
/// statically named sites than this keeps the rest on the call path.
pub fn inline_cache_slots(strings: usize) -> usize {
    strings
        .saturating_mul(8)
        .clamp(INLINE_CACHE_MIN_SLOTS, INLINE_CACHE_MAX_SLOTS)
}

pub fn inline_cache_bytes(slots: usize) -> usize {
    slots * INLINE_CACHE_SIZE
}

#[derive(Clone)]
struct CodegenCtx<'a> {
    program: &'a Program<'a>,
    plan_id: usize,
    scope: ScopeId,
    env: Value,
    calloc_ref: FuncRef,
    pow_ref: FuncRef,
    fmod_ref: FuncRef,
    js_sig_ref: SigRef,
    child_refs: &'a HashMap<usize, FuncRef>,
    builtins: &'a Builtins,
    builtin_refs: &'a HashMap<&'static str, FuncRef>,
    vars: &'a HashMap<(ScopeId, usize), Variable>,
    numeric: &'a HashSet<(ScopeId, usize)>,
    exception: Value,
    strings: Option<value::StringOps>,
    objects: Option<value::ObjectOps>,
    arrays: Option<value::ArrayOps>,
    /// The address, length and interned-value cache slot of every literal.
    string_literals: Rc<HashMap<String, (Value, i64, Value)>>,
    /// The module's inline-cache pool base and the next free slot.
    inline_caches: Option<(Value, Rc<Cell<usize>>)>,
    /// How many cells that pool holds.
    inline_cache_slots: usize,
}

impl<'a> CodegenCtx<'a> {
    /// The same function, entered through another scope's environment.
    fn enter(&self, scope: ScopeId, env: Value) -> Self {
        Self {
            program: self.program,
            plan_id: self.plan_id,
            scope,
            env,
            calloc_ref: self.calloc_ref,
            pow_ref: self.pow_ref,
            fmod_ref: self.fmod_ref,
            js_sig_ref: self.js_sig_ref,
            child_refs: self.child_refs,
            builtins: self.builtins,
            builtin_refs: self.builtin_refs,
            vars: self.vars,
            numeric: self.numeric,
            exception: self.exception,
            strings: self.strings,
            objects: self.objects,
            arrays: self.arrays,
            string_literals: Rc::clone(&self.string_literals),
            inline_caches: self.inline_caches.clone(),
            inline_cache_slots: self.inline_cache_slots,
        }
    }

    /// The address of a fresh inline-cache cell, or `None` once the module's
    /// pool is exhausted (the access then stays on the call path).
    fn inline_cache(&self, builder: &mut FunctionBuilder) -> Option<Value> {
        let (base, next) = self.inline_caches.as_ref()?;
        let index = next.get();
        if index >= self.inline_cache_slots {
            return None;
        }
        next.set(index + 1);
        Some(
            builder
                .ins()
                .iadd_imm(*base, (index * INLINE_CACHE_SIZE) as i64),
        )
    }
}

fn string_ops(refs: &HashMap<&'static str, FuncRef>) -> Option<value::StringOps> {
    Some(value::StringOps {
        from_utf8: *refs.get("sealion_string_from_utf8")?,
        key: *refs.get("sealion_string_key")?,
        add: *refs.get("sealion_string_add")?,
        strict_equal: *refs.get("sealion_string_strict_equal")?,
        loose_equal: *refs.get("sealion_string_loose_equal")?,
        same_value: *refs.get("sealion_string_same_value")?,
        truthy: *refs.get("sealion_string_truthy")?,
        to_number: *refs.get("sealion_string_to_number")?,
        to_string: *refs.get("sealion_string_to_string")?,
        compare: *refs.get("sealion_string_compare")?,
    })
}

fn array_ops(refs: &HashMap<&'static str, FuncRef>) -> Option<value::ArrayOps> {
    Some(value::ArrayOps {
        new: *refs.get("sealion_array_new")?,
    })
}

fn object_ops(refs: &HashMap<&'static str, FuncRef>) -> Option<value::ObjectOps> {
    Some(value::ObjectOps {
        new: *refs.get("sealion_object_new")?,
        get: *refs.get("sealion_object_get")?,
        set: *refs.get("sealion_object_set")?,
        get_cached: *refs.get("sealion_object_get_cached")?,
        set_cached: *refs.get("sealion_object_set_cached")?,
        has: *refs.get("sealion_object_has")?,
        delete: *refs.get("sealion_object_delete")?,
        new_instance: *refs.get("sealion_object_new_instance")?,
        set_prototype: *refs.get("sealion_object_set_prototype")?,
        instance_of: *refs.get("sealion_instanceof")?,
        for_in_keys: *refs.get("sealion_object_for_in_keys")?,
        iter_new: *refs.get("sealion_iter_new")?,
        iter_next: *refs.get("sealion_iter_next")?,
        iter_value: *refs.get("sealion_iter_value")?,
    })
}

#[derive(Default)]
struct StmtOutcome {
    last: Option<Value>,
    terminated: bool,
}

#[derive(Clone)]
struct LoopTarget<'a> {
    labels: Vec<&'a str>,
    break_block: Block,
    continue_block: Option<Block>,
    /// `break;` without a label may target this entry (loops and `switch`).
    breakable: bool,
    cleanup_depth: usize,
}

#[derive(Clone, Copy)]
struct ExceptionTarget {
    block: Block,
    cleanup_depth: usize,
}

#[derive(Clone)]
struct Cleanup<'a> {
    finalizer: &'a BlockStatement<'a>,
    loop_depth: usize,
    scope: ScopeId,
    env: Value,
    exception: Option<ExceptionTarget>,
}

struct Flow<'a> {
    loops: Vec<LoopTarget<'a>>,
    cleanups: Vec<Cleanup<'a>>,
    exception: Option<ExceptionTarget>,
}

impl<'a> Flow<'a> {
    fn new() -> Self {
        Self {
            loops: Vec::new(),
            cleanups: Vec::new(),
            exception: None,
        }
    }
}

#[derive(Clone, Copy)]
enum Place {
    Env { ptr: Value, offset: i32 },
    Local { var: Variable },
}

/// The libc entry points every compiled function may call.
pub struct Imports {
    pub calloc: FuncId,
    pub pow: FuncId,
    pub fmod: FuncId,
    pub exception: DataId,
    /// The writable pool of per-site inline caches; `None` without the dynamic
    /// runtime, where property access does not exist.
    pub inline_caches: Option<DataId>,
    /// Cells in that pool, from [`inline_cache_slots`].
    pub inline_cache_slots: usize,
}

#[allow(clippy::too_many_arguments)]
pub fn compile_functions<'a>(
    module: &mut ObjectModule,
    program: &'a Program<'a>,
    func_ids: &[FuncId],
    js_sig: &Signature,
    imports: &Imports,
    builtins: &'a Builtins,
    string_ids: &HashMap<String, DataId>,
    key_ids: &HashMap<String, DataId>,
    dynamic: bool,
) -> Result<Vec<IrFunction>, Error> {
    let numeric = analyze_numeric_bindings(program, builtins);
    let mut raws = Vec::new();
    let next_inline_cache = Rc::new(Cell::new(0));

    for plan in &program.functions {
        let mut func =
            Function::with_name_signature(UserFuncName::user(0, plan.id as u32), js_sig.clone());

        let calloc_ref = module.declare_func_in_func(imports.calloc, &mut func);
        let pow_ref = module.declare_func_in_func(imports.pow, &mut func);
        let fmod_ref = module.declare_func_in_func(imports.fmod, &mut func);
        let exception_global = module.declare_data_in_func(imports.exception, &mut func);
        let builtin_refs = builtins.declare_in(module, &mut func);
        let mut child_refs = HashMap::new();
        for &child in plan
            .children
            .iter()
            .chain(plan.nested.values())
            .chain(plan.members.iter())
        {
            let child_ref = module.declare_func_in_func(func_ids[child], &mut func);
            child_refs.insert(child, child_ref);
        }

        let string_data: Vec<(
            String,
            cranelift_codegen::ir::GlobalValue,
            i64,
            cranelift_codegen::ir::GlobalValue,
        )> = string_ids
            .iter()
            .map(|(text, &id)| {
                let data = module.declare_data_in_func(id, &mut func);
                let slot = module.declare_data_in_func(key_ids[text], &mut func);
                (text.clone(), data, text.len() as i64, slot)
            })
            .collect();
        let inline_cache_data = imports
            .inline_caches
            .map(|id| module.declare_data_in_func(id, &mut func));

        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut builder_ctx);
        let js_sig_ref = builder.import_signature(js_sig.clone());

        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);

        let env_param = builder.block_params(entry)[0];
        let args_param = builder.block_params(entry)[1];
        let self_param = builder.block_params(entry)[2];
        let this_param = builder.block_params(entry)[3];
        let exception = builder.ins().global_value(types::I64, exception_global);
        let string_literals: HashMap<String, (Value, i64, Value)> = string_data
            .into_iter()
            .map(|(text, data, len, slot)| {
                let pointer = builder.ins().global_value(types::I64, data);
                let slot = builder.ins().global_value(types::I64, slot);
                (text, (pointer, len, slot))
            })
            .collect();
        let string_literals = Rc::new(string_literals);
        let inline_caches = inline_cache_data.map(|data| {
            let base = builder.ins().global_value(types::I64, data);
            (base, Rc::clone(&next_inline_cache))
        });
        let strings = if dynamic {
            Some(
                string_ops(&builtin_refs)
                    .ok_or_else(|| Error::Codegen("missing string runtime".to_string()))?,
            )
        } else {
            None
        };
        let objects = if dynamic {
            Some(
                object_ops(&builtin_refs)
                    .ok_or_else(|| Error::Codegen("missing object runtime".to_string()))?,
            )
        } else {
            None
        };
        let arrays = if dynamic {
            Some(
                array_ops(&builtin_refs)
                    .ok_or_else(|| Error::Codegen("missing array runtime".to_string()))?,
            )
        } else {
            None
        };
        let root_scope = program.scope(plan.root_scope);
        let vars = declare_vars(&mut builder, program, plan.id);
        let env = if root_scope.has_env() {
            let env = alloc_env(&mut builder, calloc_ref, root_scope.slot_count(), env_param);
            init_slots(&mut builder, env, root_scope);
            env
        } else {
            init_local_slots(&mut builder, &vars, plan.root_scope, root_scope);
            env_param
        };

        let ctx = CodegenCtx {
            program,
            plan_id: plan.id,
            scope: plan.root_scope,
            env,
            calloc_ref,
            pow_ref,
            fmod_ref,
            js_sig_ref,
            child_refs: &child_refs,
            builtins,
            builtin_refs: &builtin_refs,
            vars: &vars,
            numeric: &numeric,
            exception,
            strings,
            objects,
            arrays,
            string_literals,
            inline_caches,
            inline_cache_slots: imports.inline_cache_slots,
        };

        let mut flow = Flow::new();
        if root_scope.bindings.contains_key("this") {
            store_var(&mut builder, &ctx, "this", this_param)?;
        }
        if let Some(self_name) = plan.self_binding {
            store_var(&mut builder, &ctx, self_name, self_param)?;
        }

        if !plan.is_script {
            let undef = value::undefined(&mut builder);
            let len = builder
                .ins()
                .load(types::I64, MemFlags::new(), args_param, 0i32);
            for (i, param) in plan.params.iter().enumerate() {
                let idx = builder.ins().iconst(types::I64, i as i64);
                let present = builder.ins().icmp(IntCC::UnsignedLessThan, idx, len);
                let loaded =
                    builder
                        .ins()
                        .load(types::F64, MemFlags::new(), args_param, (8 + 8 * i) as i32);
                let arg = builder.ins().select(present, loaded, undef);
                let value = match &param.init {
                    Some(init) => compile_default(&mut builder, &ctx, &mut flow, arg, init)?,
                    None => arg,
                };
                store_var(&mut builder, &ctx, param.name, value)?;
            }
        }

        for &child_id in &plan.children {
            let child_plan = program.function(child_id);
            let name = child_plan
                .binding_name
                .ok_or_else(|| Error::Codegen("missing function binding".to_string()))?;
            let code = builder.ins().func_addr(types::I64, child_refs[&child_id]);
            let closure = new_closure(&mut builder, calloc_ref, code, ctx.env, true);
            store_var(&mut builder, &ctx, name, closure)?;
        }

        // A synthesised derived-class constructor forwards its arguments to the
        // parent constructor, which initialises the receiver `new` allocated.
        if plan.forwards_super
            && let Some(home) = plan.super_home
        {
            let parent = hidden_value(&mut builder, &ctx, home.scope, home.ctor_slot)?;
            let callable = value::is_function(&mut builder, parent);
            let forward = builder.create_block();
            let skip = builder.create_block();
            builder.ins().brif(callable, forward, &[], skip, &[]);
            builder.switch_to_block(forward);
            let payload = value::payload(&mut builder, parent);
            let code = builder
                .ins()
                .load(types::I64, MemFlags::new(), payload, 0i32);
            let captured = builder
                .ins()
                .load(types::I64, MemFlags::new(), payload, 8i32);
            builder.ins().call_indirect(
                js_sig_ref,
                code,
                &[captured, args_param, parent, this_param],
            );
            emit_exception_check(&mut builder, &ctx, &flow)?;
            builder.ins().jump(skip, &[]);
            builder.switch_to_block(skip);
        }

        let out = match plan.body {
            FunctionBodyPlan::Statements(statements) => {
                compile_statements(&mut builder, &ctx, statements, &mut flow)?
            }
            FunctionBodyPlan::Expression(expression) => StmtOutcome {
                last: Some(compile_expr(&mut builder, &ctx, &mut flow, expression)?),
                terminated: false,
            },
        };
        if !out.terminated {
            // Only the script (whose result is the process exit code) and an
            // arrow expression body fall off the end with a value; an ordinary
            // function returns `undefined`, so `new` keeps the receiver it
            // allocated unless the constructor returns an object of its own.
            let implicit = match plan.body {
                FunctionBodyPlan::Statements(_) if !plan.is_script => None,
                _ => out.last,
            };
            let ret = match implicit {
                Some(value) => value,
                None => value::undefined(&mut builder),
            };
            builder.ins().return_(&[ret]);
        }

        builder.seal_all_blocks();
        builder.finalize();

        raws.push(IrFunction {
            name: plan.name.clone(),
            function: func.clone(),
        });

        let mut context = module.make_context();
        context.func = func;
        module.define_function(func_ids[plan.id], &mut context)?;
    }

    Ok(raws)
}

pub fn compile_main(
    module: &mut ObjectModule,
    script_id: FuncId,
    main_id: FuncId,
    main_sig: &Signature,
    exception_id: DataId,
    builtins: &Builtins,
    dynamic: bool,
) -> Result<IrFunction, Error> {
    let mut func = Function::with_name_signature(UserFuncName::testcase("main"), main_sig.clone());
    let script_ref = module.declare_func_in_func(script_id, &mut func);
    let exception_global = module.declare_data_in_func(exception_id, &mut func);
    let builtin_refs = if dynamic {
        builtins.declare_in(module, &mut func)
    } else {
        HashMap::new()
    };
    let strings = if dynamic {
        string_ops(&builtin_refs)
    } else {
        None
    };

    let mut builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut func, &mut builder_ctx);

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);

    let exception = builder.ins().global_value(types::I64, exception_global);
    let zero = builder.ins().iconst(types::I64, 0);
    let self_value = value::undefined(&mut builder);
    let call = builder
        .ins()
        .call(script_ref, &[zero, zero, self_value, self_value]);
    let result = builder.inst_results(call)[0];

    let pending = builder
        .ins()
        .load(types::I64, MemFlags::new(), exception, 0i32);
    let pending = builder.ins().icmp_imm(IntCC::NotEqual, pending, 0);
    let failed = builder.create_block();
    let ok = builder.create_block();
    builder.ins().brif(pending, failed, &[], ok, &[]);

    builder.switch_to_block(failed);
    builder.ins().store(MemFlags::new(), zero, exception, 0i32);
    let failure = builder.ins().iconst(types::I32, 1);
    builder.ins().return_(&[failure]);

    builder.switch_to_block(ok);
    let result = value::to_number(&mut builder, result, strings.as_ref());
    let code = builder.ins().fcvt_to_sint_sat(types::I32, result);
    builder.ins().return_(&[code]);

    builder.seal_all_blocks();
    builder.finalize();

    let raw = IrFunction {
        name: "main".to_string(),
        function: func.clone(),
    };

    let mut context = module.make_context();
    context.func = func;
    module.define_function(main_id, &mut context)?;

    Ok(raw)
}
