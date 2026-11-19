use std::collections::HashMap;

use boa_ast::{
    Declaration, Span, Spanned, Statement, StatementList, StatementListItem,
    expression::{
        Expression,
        access::{PropertyAccess, PropertyAccessField, SimplePropertyAccess},
        literal::LiteralKind,
        operator::{
            assign::{AssignOp, AssignTarget},
            binary::{ArithmeticOp, BinaryOp, BitwiseOp, LogicalOp, RelationalOp},
            unary::UnaryOp,
            update::{UpdateOp, UpdateTarget},
        },
    },
    statement::{Labelled, LabelledItem, iteration::ForLoopInitializer},
};
use boa_interner::Sym;
use cranelift_codegen::ir::{
    AbiParam, Block, FuncRef, Function, InstBuilder, MemFlags, SigRef, Signature, TrapCode,
    UserFuncName, Value, condcodes::FloatCC, condcodes::IntCC, types,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Module};
use cranelift_object::ObjectModule;

use crate::{
    Error, IrFunction,
    builtins::{self, Builtins},
    plan::{Program, ScopeId, ScopePlan, is_const, needs_tdz_check, node_key, resolve},
    value,
};

const MAX_ARGS: usize = 32;

pub fn make_js_signature<M: Module>(module: &M) -> Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::F64));
    sig.returns.push(AbiParam::new(types::F64));
    sig
}

pub fn make_calloc_signature<M: Module>(module: &M) -> Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

pub fn make_f64_unary_signature<M: Module>(module: &M) -> Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::F64));
    sig.returns.push(AbiParam::new(types::F64));
    sig
}

pub fn make_f64_binary_signature<M: Module>(module: &M) -> Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::F64));
    sig.params.push(AbiParam::new(types::F64));
    sig.returns.push(AbiParam::new(types::F64));
    sig
}

pub fn make_main_signature<M: Module>(module: &M) -> Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I32));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I32));
    sig
}

/// The libc entry points every compiled function may call.
pub struct Imports {
    pub calloc: FuncId,
    pub pow: FuncId,
    pub fmod: FuncId,
}

pub fn compile_functions(
    module: &mut ObjectModule,
    program: &Program,
    func_ids: &[FuncId],
    js_sig: &Signature,
    imports: &Imports,
    builtins: &Builtins,
) -> Result<Vec<IrFunction>, Error> {
    let mut raws = Vec::new();

    for plan in &program.functions {
        let mut func =
            Function::with_name_signature(UserFuncName::user(0, plan.id as u32), js_sig.clone());

        let calloc_ref = module.declare_func_in_func(imports.calloc, &mut func);
        let pow_ref = module.declare_func_in_func(imports.pow, &mut func);
        let fmod_ref = module.declare_func_in_func(imports.fmod, &mut func);
        let builtin_refs = builtins.declare_in(module, &mut func);
        let mut child_refs = HashMap::new();
        for &child in plan.children.iter().chain(plan.nested.values()) {
            let child_ref = module.declare_func_in_func(func_ids[child], &mut func);
            child_refs.insert(child, child_ref);
        }

        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut builder_ctx);
        let js_sig_ref = builder.import_signature(js_sig.clone());

        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);

        let env_param = builder.block_params(entry)[0];
        let args_param = builder.block_params(entry)[1];
        let self_param = builder.block_params(entry)[2];
        let root_scope = program.scope(plan.root_scope);
        let env = alloc_env(&mut builder, calloc_ref, root_scope.slot_count(), env_param);
        init_slots(&mut builder, env, root_scope);

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
        };

        if let Some(self_sym) = plan.self_binding {
            let slot = *root_scope
                .bindings
                .get(&self_sym)
                .ok_or_else(|| Error::Codegen("missing self binding slot".to_string()))?;
            builder
                .ins()
                .store(MemFlags::new(), self_param, env, (8 + 8 * slot) as i32);
        }

        if !plan.is_script {
            let undef = value::undefined(&mut builder);
            let len = builder
                .ins()
                .load(types::I64, MemFlags::new(), args_param, 0i32);
            for (i, param) in plan.params.iter().enumerate() {
                let slot = *root_scope
                    .bindings
                    .get(&param.sym)
                    .ok_or_else(|| Error::Codegen("missing parameter slot".to_string()))?;
                let idx = builder.ins().iconst(types::I64, i as i64);
                let present = builder.ins().icmp(IntCC::UnsignedLessThan, idx, len);
                let loaded =
                    builder
                        .ins()
                        .load(types::F64, MemFlags::new(), args_param, (8 + 8 * i) as i32);
                let arg = builder.ins().select(present, loaded, undef);
                let value = match &param.init {
                    Some(init) => compile_default(&mut builder, &ctx, arg, init)?,
                    None => arg,
                };
                builder
                    .ins()
                    .store(MemFlags::new(), value, env, (8 + 8 * slot) as i32);
            }
        }

        for &child_id in &plan.children {
            let child_plan = program.function(child_id);
            let sym = child_plan
                .binding_name
                .ok_or_else(|| Error::Codegen("missing function binding".to_string()))?;
            let slot = *root_scope
                .bindings
                .get(&sym)
                .ok_or_else(|| Error::Codegen("missing function slot".to_string()))?;
            let code = builder.ins().func_addr(types::I64, child_refs[&child_id]);
            let closure = new_closure(&mut builder, calloc_ref, code, env);
            builder
                .ins()
                .store(MemFlags::new(), closure, env, (8 + 8 * slot) as i32);
        }

        let mut loops = Vec::new();
        let out = compile_statements(&mut builder, &ctx, &plan.body, &mut loops)?;
        if !out.terminated {
            let ret = match out.last {
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
) -> Result<IrFunction, Error> {
    let mut func = Function::with_name_signature(UserFuncName::testcase("main"), main_sig.clone());
    let script_ref = module.declare_func_in_func(script_id, &mut func);

    let mut builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut func, &mut builder_ctx);

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);

    let zero = builder.ins().iconst(types::I64, 0);
    let self_value = value::undefined(&mut builder);
    let call = builder.ins().call(script_ref, &[zero, zero, self_value]);
    let result = builder.inst_results(call)[0];
    let result = value::to_number(&mut builder, result);
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

#[derive(Clone, Copy)]
struct CodegenCtx<'a> {
    program: &'a Program,
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
        }
    }
}

#[derive(Default)]
struct StmtOutcome {
    last: Option<Value>,
    terminated: bool,
}

fn call_calloc(builder: &mut FunctionBuilder, calloc_ref: FuncRef, size: i64) -> Value {
    let one = builder.ins().iconst(types::I64, 1);
    let size = builder.ins().iconst(types::I64, size);
    let call = builder.ins().call(calloc_ref, &[one, size]);
    builder.inst_results(call)[0]
}

fn alloc_env(
    builder: &mut FunctionBuilder,
    calloc_ref: FuncRef,
    slots: usize,
    parent: Value,
) -> Value {
    let size = 8 + 8 * slots as i64;
    let env = call_calloc(builder, calloc_ref, size);
    builder.ins().store(MemFlags::new(), parent, env, 0i32);
    env
}

fn new_closure(
    builder: &mut FunctionBuilder,
    calloc_ref: FuncRef,
    code: Value,
    env: Value,
) -> Value {
    let ptr = call_calloc(builder, calloc_ref, 16);
    builder.ins().store(MemFlags::new(), code, ptr, 0i32);
    builder.ins().store(MemFlags::new(), env, ptr, 8i32);
    value::function(builder, ptr)
}

fn init_slots(builder: &mut FunctionBuilder, env: Value, scope: &ScopePlan) {
    for slot in 0..scope.slot_count() {
        let initial = if scope.tdz.contains(&slot) {
            value::uninitialized(builder)
        } else {
            value::undefined(builder)
        };
        builder
            .ins()
            .store(MemFlags::new(), initial, env, (8 + 8 * slot) as i32);
    }
}

/// Evaluates a parameter initializer only when the argument is `undefined`.
fn compile_default(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    arg: Value,
    init: &Expression,
) -> Result<Value, Error> {
    let missing = value::is_undefined(builder, arg);
    let result_var = builder.declare_var(types::F64);
    let init_block = builder.create_block();
    let join_block = builder.create_block();

    builder.def_var(result_var, arg);
    builder
        .ins()
        .brif(missing, init_block, &[], join_block, &[]);

    builder.switch_to_block(init_block);
    let default = compile_expr(builder, ctx, init)?;
    builder.def_var(result_var, default);
    builder.ins().jump(join_block, &[]);

    builder.switch_to_block(join_block);
    Ok(builder.use_var(result_var))
}

/// The scope planned for a block-like node (`{ ... }`, `switch`, `for`).
fn node_scope(ctx: &CodegenCtx<'_>, node_key: usize) -> Result<ScopeId, Error> {
    ctx.program
        .function(ctx.plan_id)
        .blocks
        .get(&node_key)
        .copied()
        .ok_or_else(|| Error::Codegen("missing block scope".to_string()))
}

/// Enters a scope, allocating a fresh environment when it owns bindings.
fn enter_scope<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    scope: ScopeId,
) -> CodegenCtx<'a> {
    let plan = ctx.program.scope(scope);
    if plan.allocated() {
        let env = alloc_env(builder, ctx.calloc_ref, plan.slot_count(), ctx.env);
        init_slots(builder, env, plan);
        ctx.enter(scope, env)
    } else {
        ctx.enter(scope, ctx.env)
    }
}

/// Allocates a copy of `src`'s slots, used to give a `for` loop a fresh set of
/// bindings per iteration.
fn copy_env(
    builder: &mut FunctionBuilder,
    calloc_ref: FuncRef,
    parent: Value,
    src: Value,
    slots: usize,
) -> Value {
    let env = alloc_env(builder, calloc_ref, slots, parent);
    for slot in 0..slots {
        let offset = (8 + 8 * slot) as i32;
        let value = load_at(builder, src, offset);
        store_at(builder, env, offset, value);
    }
    env
}

/// Enters a block scope, allocating its environment when it owns bindings.
fn compile_block(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    block: &boa_ast::statement::Block,
    loops: &mut Vec<LoopTarget>,
) -> Result<StmtOutcome, Error> {
    let scope = node_scope(ctx, node_key(block))?;
    let inner = enter_scope(builder, ctx, scope);
    compile_statements(builder, &inner, block.statement_list(), loops)
}

struct LoopTarget {
    labels: Vec<Sym>,
    break_block: Block,
    continue_block: Option<Block>,
    /// `break;` without a label may target this entry (loops and `switch`).
    breakable: bool,
}

fn break_target(loops: &[LoopTarget], label: Option<Sym>) -> Result<Block, Error> {
    if let Some(label) = label {
        loops
            .iter()
            .rev()
            .find(|target| target.labels.contains(&label))
            .map(|target| target.break_block)
            .ok_or_else(|| Error::Scope(format!("unknown break label: {label:?}")))
    } else {
        loops
            .iter()
            .rev()
            .find(|target| target.breakable)
            .map(|target| target.break_block)
            .ok_or_else(|| Error::Scope("break statement outside of a loop".to_string()))
    }
}

fn continue_target(loops: &[LoopTarget], label: Option<Sym>) -> Result<Block, Error> {
    if let Some(label) = label {
        let target = loops
            .iter()
            .rev()
            .find(|target| target.labels.contains(&label))
            .ok_or_else(|| Error::Scope(format!("unknown continue label: {label:?}")))?;
        target
            .continue_block
            .ok_or_else(|| Error::Scope("continue label does not target a loop".to_string()))
    } else {
        loops
            .iter()
            .rev()
            .find_map(|target| target.continue_block)
            .ok_or_else(|| Error::Scope("continue statement outside of a loop".to_string()))
    }
}

fn call_f64_binary(
    builder: &mut FunctionBuilder,
    func_ref: FuncRef,
    lhs: Value,
    rhs: Value,
) -> Value {
    let call = builder.ins().call(func_ref, &[lhs, rhs]);
    builder.inst_results(call)[0]
}

fn call_pow(builder: &mut FunctionBuilder, pow_ref: FuncRef, lhs: Value, rhs: Value) -> Value {
    call_f64_binary(builder, pow_ref, lhs, rhs)
}

fn call_fmod(builder: &mut FunctionBuilder, fmod_ref: FuncRef, lhs: Value, rhs: Value) -> Value {
    call_f64_binary(builder, fmod_ref, lhs, rhs)
}

/// Resolves an identifier to the environment holding it and the slot offset.
fn lvalue(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    sym: Sym,
) -> Result<(Value, i32), Error> {
    let (depth, slot) = resolve(ctx.program, ctx.scope, sym)
        .ok_or_else(|| Error::Scope(format!("unresolved identifier: {sym:?}")))?;
    let mut ptr = ctx.env;
    for _ in 0..depth {
        ptr = builder.ins().load(types::I64, MemFlags::new(), ptr, 0i32);
    }
    Ok((ptr, (8 + 8 * slot) as i32))
}

fn load_at(builder: &mut FunctionBuilder, ptr: Value, offset: i32) -> Value {
    builder.ins().load(types::F64, MemFlags::new(), ptr, offset)
}

fn store_at(builder: &mut FunctionBuilder, ptr: Value, offset: i32, value: Value) {
    builder.ins().store(MemFlags::new(), value, ptr, offset);
}

fn check_tdz(builder: &mut FunctionBuilder, value: Value) {
    let uninitialized = value::is_uninitialized(builder, value);
    let trap_block = builder.create_block();
    let continue_block = builder.create_block();
    builder
        .ins()
        .brif(uninitialized, trap_block, &[], continue_block, &[]);
    builder.switch_to_block(trap_block);
    builder.ins().trap(TrapCode::unwrap_user(1));
    builder.switch_to_block(continue_block);
}

fn load_var(builder: &mut FunctionBuilder, ctx: &CodegenCtx<'_>, sym: Sym) -> Result<Value, Error> {
    let (ptr, offset) = lvalue(builder, ctx, sym)?;
    let value = load_at(builder, ptr, offset);
    if needs_tdz_check(ctx.program, ctx.scope, sym) {
        check_tdz(builder, value);
    }
    Ok(value)
}

fn store_var(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    sym: Sym,
    value: Value,
) -> Result<(), Error> {
    let (ptr, offset) = lvalue(builder, ctx, sym)?;
    store_at(builder, ptr, offset, value);
    Ok(())
}

fn compile_statements(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    list: &StatementList,
    loops: &mut Vec<LoopTarget>,
) -> Result<StmtOutcome, Error> {
    let mut last = None;
    for item in list.statements() {
        let out = match item {
            StatementListItem::Statement(stmt) => compile_statement(builder, ctx, stmt, loops)?,
            StatementListItem::Declaration(decl) => compile_declaration(builder, ctx, decl)?,
        };
        if out.last.is_some() {
            last = out.last;
        }
        if out.terminated {
            return Ok(StmtOutcome {
                last,
                terminated: true,
            });
        }
    }
    Ok(StmtOutcome {
        last,
        terminated: false,
    })
}

fn compile_declaration(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    decl: &Declaration,
) -> Result<StmtOutcome, Error> {
    match decl {
        Declaration::FunctionDeclaration(_) => Ok(StmtOutcome::default()),
        Declaration::Lexical(lex) => {
            compile_var_list(builder, ctx, lex.variable_list(), true)?;
            Ok(StmtOutcome::default())
        }
        other => Err(Error::Unsupported(format!(
            "declaration not supported: {other:?}"
        ))),
    }
}

fn compile_var_list(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    list: &boa_ast::declaration::VariableList,
    lexical: bool,
) -> Result<(), Error> {
    for var in list.as_ref() {
        let sym = match var.binding() {
            boa_ast::declaration::Binding::Identifier(id) => id.sym(),
            boa_ast::declaration::Binding::Pattern(_) => {
                return Err(Error::Unsupported(
                    "destructuring is not supported".to_string(),
                ));
            }
        };
        if let Some(init) = var.init() {
            let value = compile_expr(builder, ctx, init)?;
            store_var(builder, ctx, sym, value)?;
        } else if lexical {
            let value = value::undefined(builder);
            store_var(builder, ctx, sym, value)?;
        }
    }
    Ok(())
}

fn compile_statement(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    stmt: &Statement,
    loops: &mut Vec<LoopTarget>,
) -> Result<StmtOutcome, Error> {
    match stmt {
        Statement::Block(block) => compile_block(builder, ctx, block, loops),
        Statement::Var(var) => {
            compile_var_list(builder, ctx, &var.0, false)?;
            Ok(StmtOutcome::default())
        }
        Statement::Empty | Statement::Debugger => Ok(StmtOutcome::default()),
        Statement::Expression(expr) => {
            let value = compile_expr(builder, ctx, expr)?;
            Ok(StmtOutcome {
                last: Some(value),
                terminated: false,
            })
        }
        Statement::If(if_stmt) => compile_if(builder, ctx, if_stmt, loops),
        Statement::WhileLoop(while_stmt) => compile_while(builder, ctx, while_stmt, &[], loops),
        Statement::DoWhileLoop(do_while) => compile_do_while(builder, ctx, do_while, &[], loops),
        Statement::ForLoop(for_stmt) => compile_for(builder, ctx, for_stmt, &[], loops),
        Statement::Switch(switch_stmt) => compile_switch(builder, ctx, switch_stmt, &[], loops),
        Statement::Labelled(labelled) => compile_labelled(builder, ctx, labelled, loops),
        Statement::Break(break_stmt) => {
            let target = break_target(loops, break_stmt.label())?;
            builder.ins().jump(target, &[]);
            Ok(StmtOutcome {
                last: None,
                terminated: true,
            })
        }
        Statement::Continue(continue_stmt) => {
            let target = continue_target(loops, continue_stmt.label())?;
            builder.ins().jump(target, &[]);
            Ok(StmtOutcome {
                last: None,
                terminated: true,
            })
        }
        Statement::Return(ret) => {
            let value = if let Some(target) = ret.target() {
                compile_expr(builder, ctx, target)?
            } else {
                value::undefined(builder)
            };
            builder.ins().return_(&[value]);
            Ok(StmtOutcome {
                last: None,
                terminated: true,
            })
        }
        other => Err(Error::Unsupported(format!(
            "statement not supported: {other:?}"
        ))),
    }
}

fn compile_if(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    if_stmt: &boa_ast::statement::If,
    loops: &mut Vec<LoopTarget>,
) -> Result<StmtOutcome, Error> {
    let cond_value = compile_expr(builder, ctx, if_stmt.cond())?;
    let cond = value::truthy(builder, cond_value);

    let then_block = builder.create_block();
    let else_block = builder.create_block();
    let join_block = builder.create_block();
    builder.ins().brif(cond, then_block, &[], else_block, &[]);

    builder.switch_to_block(then_block);
    let then_out = compile_statement(builder, ctx, if_stmt.body(), loops)?;
    if !then_out.terminated {
        builder.ins().jump(join_block, &[]);
    }

    builder.switch_to_block(else_block);
    let else_out = if let Some(else_node) = if_stmt.else_node() {
        compile_statement(builder, ctx, else_node, loops)?
    } else {
        StmtOutcome::default()
    };
    if !else_out.terminated {
        builder.ins().jump(join_block, &[]);
    }

    if then_out.terminated && else_out.terminated {
        Ok(StmtOutcome {
            last: None,
            terminated: true,
        })
    } else {
        builder.switch_to_block(join_block);
        Ok(StmtOutcome::default())
    }
}

fn compile_while(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    while_stmt: &boa_ast::statement::iteration::WhileLoop,
    labels: &[Sym],
    loops: &mut Vec<LoopTarget>,
) -> Result<StmtOutcome, Error> {
    let header = builder.create_block();
    let body = builder.create_block();
    let exit = builder.create_block();

    builder.ins().jump(header, &[]);
    builder.switch_to_block(header);

    let cond_value = compile_expr(builder, ctx, while_stmt.condition())?;
    let cond = value::truthy(builder, cond_value);
    builder.ins().brif(cond, body, &[], exit, &[]);

    builder.switch_to_block(body);
    loops.push(LoopTarget {
        labels: labels.to_vec(),
        break_block: exit,
        continue_block: Some(header),
        breakable: true,
    });
    let body_out = compile_statement(builder, ctx, while_stmt.body(), loops)?;
    loops.pop();
    if !body_out.terminated {
        builder.ins().jump(header, &[]);
    }

    builder.switch_to_block(exit);
    Ok(StmtOutcome::default())
}

fn compile_do_while(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    do_while: &boa_ast::statement::iteration::DoWhileLoop,
    labels: &[Sym],
    loops: &mut Vec<LoopTarget>,
) -> Result<StmtOutcome, Error> {
    let body_block = builder.create_block();
    let condition_block = builder.create_block();
    let exit = builder.create_block();

    builder.ins().jump(body_block, &[]);
    builder.switch_to_block(body_block);
    loops.push(LoopTarget {
        labels: labels.to_vec(),
        break_block: exit,
        continue_block: Some(condition_block),
        breakable: true,
    });
    let body_out = compile_statement(builder, ctx, do_while.body(), loops)?;
    loops.pop();
    if !body_out.terminated {
        builder.ins().jump(condition_block, &[]);
    }

    builder.switch_to_block(condition_block);
    let cond_value = compile_expr(builder, ctx, do_while.cond())?;
    let cond = value::truthy(builder, cond_value);
    builder.ins().brif(cond, body_block, &[], exit, &[]);

    builder.switch_to_block(exit);
    Ok(StmtOutcome::default())
}

/// Compiles a label chain. Labels attached to loops become extra names for the
/// loop target; any other labelled statement gets a break-only target.
fn compile_labelled(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    labelled: &Labelled,
    loops: &mut Vec<LoopTarget>,
) -> Result<StmtOutcome, Error> {
    let mut labels = vec![labelled.label()];
    let mut item = labelled.item();
    while let LabelledItem::Statement(Statement::Labelled(inner)) = item {
        labels.push(inner.label());
        item = inner.item();
    }

    let stmt = match item {
        LabelledItem::Statement(stmt) => stmt,
        LabelledItem::FunctionDeclaration(_) => {
            return Err(Error::Unsupported(
                "labelled function declarations are not supported".to_string(),
            ));
        }
    };

    match stmt {
        Statement::WhileLoop(while_stmt) => compile_while(builder, ctx, while_stmt, &labels, loops),
        Statement::DoWhileLoop(do_while) => {
            compile_do_while(builder, ctx, do_while, &labels, loops)
        }
        Statement::ForLoop(for_stmt) => compile_for(builder, ctx, for_stmt, &labels, loops),
        Statement::Switch(switch_stmt) => compile_switch(builder, ctx, switch_stmt, &labels, loops),
        stmt => {
            let join = builder.create_block();
            loops.push(LoopTarget {
                labels,
                break_block: join,
                continue_block: None,
                breakable: false,
            });
            let out = compile_statement(builder, ctx, stmt, loops)?;
            loops.pop();
            if !out.terminated {
                builder.ins().jump(join, &[]);
            }
            builder.switch_to_block(join);
            Ok(StmtOutcome::default())
        }
    }
}

/// `for (init; condition; update) body`. A lexical initializer gets one
/// environment per iteration so each iteration's bindings are distinct.
fn compile_for(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    for_stmt: &boa_ast::statement::iteration::ForLoop,
    labels: &[Sym],
    loops: &mut Vec<LoopTarget>,
) -> Result<StmtOutcome, Error> {
    let parent_env = ctx.env;
    let mut ctx = *ctx;
    let mut fresh: Option<(usize, Value)> = None;
    if let Some(ForLoopInitializer::Lexical(_)) = for_stmt.init() {
        let scope = node_scope(&ctx, node_key(for_stmt))?;
        let plan = ctx.program.scope(scope);
        let slots = plan.slot_count();
        let allocated = plan.allocated();
        ctx = enter_scope(builder, &ctx, scope);
        if allocated {
            fresh = Some((slots, ctx.env));
        }
    }

    match for_stmt.init() {
        Some(ForLoopInitializer::Expression(expr)) => {
            compile_expr(builder, &ctx, expr)?;
        }
        Some(ForLoopInitializer::Var(var)) => compile_var_list(builder, &ctx, &var.0, false)?,
        Some(ForLoopInitializer::Lexical(lex)) => {
            compile_var_list(builder, &ctx, lex.declaration().variable_list(), true)?;
        }
        None => {}
    }

    let header = builder.create_block();
    let body_block = builder.create_block();
    let step = builder.create_block();
    let exit = builder.create_block();

    let env_var = builder.declare_var(types::I64);
    if let Some((_, env)) = fresh {
        builder.def_var(env_var, env);
    }
    builder.ins().jump(header, &[]);

    builder.switch_to_block(header);
    let per_iteration = fresh.is_some();
    let header_ctx = if per_iteration {
        ctx.enter(ctx.scope, builder.use_var(env_var))
    } else {
        ctx
    };
    match for_stmt.condition() {
        Some(condition) => {
            let value = compile_expr(builder, &header_ctx, condition)?;
            let cond = value::truthy(builder, value);
            builder.ins().brif(cond, body_block, &[], exit, &[]);
        }
        None => {
            builder.ins().jump(body_block, &[]);
        }
    }

    builder.switch_to_block(body_block);
    let body_ctx = if per_iteration {
        ctx.enter(ctx.scope, builder.use_var(env_var))
    } else {
        ctx
    };
    loops.push(LoopTarget {
        labels: labels.to_vec(),
        break_block: exit,
        continue_block: Some(step),
        breakable: true,
    });
    let body_out = compile_statement(builder, &body_ctx, for_stmt.body(), loops)?;
    loops.pop();
    if !body_out.terminated {
        builder.ins().jump(step, &[]);
    }

    builder.switch_to_block(step);
    let step_ctx = if let Some((slots, _)) = fresh {
        let previous = builder.use_var(env_var);
        let env = copy_env(builder, ctx.calloc_ref, parent_env, previous, slots);
        builder.def_var(env_var, env);
        ctx.enter(ctx.scope, env)
    } else {
        ctx
    };
    if let Some(update) = for_stmt.final_expr() {
        compile_expr(builder, &step_ctx, update)?;
    }
    builder.ins().jump(header, &[]);

    builder.switch_to_block(exit);
    Ok(StmtOutcome::default())
}

/// `switch (value) { case c: ... default: ... }` with fall-through.
fn compile_switch(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    switch_stmt: &boa_ast::statement::Switch,
    labels: &[Sym],
    loops: &mut Vec<LoopTarget>,
) -> Result<StmtOutcome, Error> {
    let value = compile_expr(builder, ctx, switch_stmt.val())?;
    let scope = node_scope(ctx, node_key(switch_stmt))?;
    let inner = enter_scope(builder, ctx, scope);

    let value_var = builder.declare_var(types::F64);
    builder.def_var(value_var, value);

    let cases = switch_stmt.cases();
    let exit = builder.create_block();
    let case_blocks: Vec<Block> = cases.iter().map(|_| builder.create_block()).collect();
    let no_match = cases
        .iter()
        .position(|case| case.is_default())
        .map_or(exit, |i| case_blocks[i]);

    let tests: Vec<(usize, &Expression)> = cases
        .iter()
        .enumerate()
        .filter_map(|(i, case)| case.condition().map(|cond| (i, cond)))
        .collect();

    if tests.is_empty() {
        builder.ins().jump(no_match, &[]);
    }
    for (n, (i, condition)) in tests.iter().enumerate() {
        let is_last = n + 1 == tests.len();
        let matched_value = compile_expr(builder, &inner, condition)?;
        let target = builder.use_var(value_var);
        let equal = value::strict_equal(builder, target, matched_value);
        let cond = value::truthy(builder, equal);
        let next = if is_last {
            no_match
        } else {
            builder.create_block()
        };
        builder.ins().brif(cond, case_blocks[*i], &[], next, &[]);
        if !is_last {
            builder.switch_to_block(next);
        }
    }

    loops.push(LoopTarget {
        labels: labels.to_vec(),
        break_block: exit,
        continue_block: None,
        breakable: true,
    });
    for (i, case) in cases.iter().enumerate() {
        builder.switch_to_block(case_blocks[i]);
        let out = compile_statements(builder, &inner, case.body(), loops)?;
        if !out.terminated {
            let next = case_blocks.get(i + 1).copied().unwrap_or(exit);
            builder.ins().jump(next, &[]);
        }
    }
    loops.pop();

    builder.switch_to_block(exit);
    Ok(StmtOutcome::default())
}

fn compile_expr(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    expr: &Expression,
) -> Result<Value, Error> {
    match expr {
        Expression::Literal(lit) => compile_literal(builder, lit.kind()),
        Expression::Identifier(id) => {
            if is_free_global(ctx, id.sym()) {
                if ctx.builtins.is_undefined(id.sym()) {
                    return Ok(value::undefined(builder));
                }
                if let Some(constant) = ctx.builtins.global_constant(id.sym()) {
                    return Ok(builder.ins().f64const(constant));
                }
            }
            load_var(builder, ctx, id.sym())
        }
        Expression::Parenthesized(paren) => compile_expr(builder, ctx, paren.expression()),
        Expression::Binary(binary) => compile_binary(builder, ctx, binary),
        Expression::Conditional(conditional) => compile_conditional(builder, ctx, conditional),
        Expression::Unary(unary) => compile_unary(builder, ctx, unary),
        Expression::Assign(assign) => compile_assign(builder, ctx, assign),
        Expression::Update(update) => compile_update(builder, ctx, update),
        Expression::Call(call) => compile_call(builder, ctx, call),
        Expression::FunctionExpression(func) => compile_closure(builder, ctx, func.span()),
        Expression::ArrowFunction(arrow) => compile_closure(builder, ctx, arrow.span()),
        Expression::PropertyAccess(access) => compile_property_access(builder, ctx, access),
        other => Err(Error::Unsupported(format!(
            "expression not supported: {other:?}"
        ))),
    }
}

fn compile_literal(builder: &mut FunctionBuilder, kind: &LiteralKind) -> Result<Value, Error> {
    let value = match kind {
        LiteralKind::Num(n) => builder.ins().f64const(*n),
        LiteralKind::Int(i) => builder.ins().f64const(*i as f64),
        LiteralKind::Bool(b) => value::boolean(builder, *b),
        LiteralKind::Null => value::null(builder),
        LiteralKind::Undefined => value::undefined(builder),
        LiteralKind::String(_) | LiteralKind::BigInt(_) => {
            return Err(Error::Unsupported(
                "string and bigint literals are not supported".to_string(),
            ));
        }
    };
    Ok(value)
}

fn compile_binary(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    binary: &boa_ast::expression::operator::Binary,
) -> Result<Value, Error> {
    if let BinaryOp::Logical(op) = binary.op() {
        let lhs = compile_expr(builder, ctx, binary.lhs())?;
        return compile_logical(builder, ctx, lhs, binary.rhs(), op);
    }

    let lhs = compile_expr(builder, ctx, binary.lhs())?;
    let rhs = compile_expr(builder, ctx, binary.rhs())?;
    match binary.op() {
        BinaryOp::Arithmetic(arith) => compile_arithmetic(builder, ctx, arith, lhs, rhs),
        BinaryOp::Bitwise(bitwise) => Ok(compile_bitwise(builder, ctx, bitwise, lhs, rhs)),
        BinaryOp::Relational(rel) => compile_relational(builder, rel, lhs, rhs),
        BinaryOp::Comma => Ok(rhs),
        other => Err(Error::Unsupported(format!(
            "binary operator not supported: {other:?}"
        ))),
    }
}

/// Compiles `cond ? if_true : if_false`, evaluating exactly one branch.
fn compile_conditional(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    conditional: &boa_ast::expression::operator::Conditional,
) -> Result<Value, Error> {
    let cond_value = compile_expr(builder, ctx, conditional.condition())?;
    let cond = value::truthy(builder, cond_value);

    let result_var = builder.declare_var(types::F64);
    let true_block = builder.create_block();
    let false_block = builder.create_block();
    let join_block = builder.create_block();
    builder.ins().brif(cond, true_block, &[], false_block, &[]);

    builder.switch_to_block(true_block);
    let if_true = compile_expr(builder, ctx, conditional.if_true())?;
    builder.def_var(result_var, if_true);
    builder.ins().jump(join_block, &[]);

    builder.switch_to_block(false_block);
    let if_false = compile_expr(builder, ctx, conditional.if_false())?;
    builder.def_var(result_var, if_false);
    builder.ins().jump(join_block, &[]);

    builder.switch_to_block(join_block);
    Ok(builder.use_var(result_var))
}

/// The short-circuiting operators: `&&`, `||` and `??`.
fn compile_logical(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    lhs: Value,
    rhs_expr: &Expression,
    op: LogicalOp,
) -> Result<Value, Error> {
    let (cond, rhs_when_true) = match op {
        LogicalOp::And => (value::truthy(builder, lhs), true),
        LogicalOp::Or => (value::truthy(builder, lhs), false),
        LogicalOp::Coalesce => (value::is_nullish(builder, lhs), true),
    };

    let result_var = builder.declare_var(types::F64);
    let rhs_block = builder.create_block();
    let join_block = builder.create_block();

    builder.def_var(result_var, lhs);
    if rhs_when_true {
        builder.ins().brif(cond, rhs_block, &[], join_block, &[]);
    } else {
        builder.ins().brif(cond, join_block, &[], rhs_block, &[]);
    }

    builder.switch_to_block(rhs_block);
    let rhs = compile_expr(builder, ctx, rhs_expr)?;
    builder.def_var(result_var, rhs);
    builder.ins().jump(join_block, &[]);

    builder.switch_to_block(join_block);
    Ok(builder.use_var(result_var))
}

fn compile_arithmetic(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    op: ArithmeticOp,
    lhs: Value,
    rhs: Value,
) -> Result<Value, Error> {
    let lhs = value::to_number(builder, lhs);
    let rhs = value::to_number(builder, rhs);
    let value = match op {
        ArithmeticOp::Add => builder.ins().fadd(lhs, rhs),
        ArithmeticOp::Sub => builder.ins().fsub(lhs, rhs),
        ArithmeticOp::Mul => builder.ins().fmul(lhs, rhs),
        ArithmeticOp::Div => builder.ins().fdiv(lhs, rhs),
        ArithmeticOp::Mod => call_fmod(builder, ctx.fmod_ref, lhs, rhs),
        ArithmeticOp::Exp => call_pow(builder, ctx.pow_ref, lhs, rhs),
    };
    Ok(value)
}

/// Bitwise operators work on `ToInt32`/`ToUint32` views of the operands.
fn compile_bitwise(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    op: BitwiseOp,
    lhs: Value,
    rhs: Value,
) -> Value {
    let lhs = value::to_int32(builder, ctx.fmod_ref, lhs);
    let rhs = value::to_int32(builder, ctx.fmod_ref, rhs);

    let result = match op {
        BitwiseOp::And => builder.ins().band(lhs, rhs),
        BitwiseOp::Or => builder.ins().bor(lhs, rhs),
        BitwiseOp::Xor => builder.ins().bxor(lhs, rhs),
        BitwiseOp::Shl => {
            let shift = builder.ins().band_imm(rhs, 31);
            builder.ins().ishl(lhs, shift)
        }
        BitwiseOp::Shr => {
            let shift = builder.ins().band_imm(rhs, 31);
            builder.ins().sshr(lhs, shift)
        }
        BitwiseOp::UShr => {
            let shift = builder.ins().band_imm(rhs, 31);
            let shifted = builder.ins().ushr(lhs, shift);
            return value::from_uint32(builder, shifted);
        }
    };
    value::from_int32(builder, result)
}

fn compile_relational(
    builder: &mut FunctionBuilder,
    op: RelationalOp,
    lhs: Value,
    rhs: Value,
) -> Result<Value, Error> {
    match op {
        RelationalOp::Equal => Ok(value::loose_equal(builder, lhs, rhs)),
        RelationalOp::StrictEqual => Ok(value::strict_equal(builder, lhs, rhs)),
        RelationalOp::NotEqual | RelationalOp::StrictNotEqual => {
            let eq = if op == RelationalOp::NotEqual {
                value::loose_equal(builder, lhs, rhs)
            } else {
                value::strict_equal(builder, lhs, rhs)
            };
            let eq = value::truthy(builder, eq);
            let not = builder.ins().icmp_imm(IntCC::Equal, eq, 0);
            Ok(value::from_bool(builder, not))
        }
        RelationalOp::GreaterThan
        | RelationalOp::GreaterThanOrEqual
        | RelationalOp::LessThan
        | RelationalOp::LessThanOrEqual => {
            let lhs = value::to_number(builder, lhs);
            let rhs = value::to_number(builder, rhs);
            let cc = match op {
                RelationalOp::GreaterThan => FloatCC::GreaterThan,
                RelationalOp::GreaterThanOrEqual => FloatCC::GreaterThanOrEqual,
                RelationalOp::LessThan => FloatCC::LessThan,
                RelationalOp::LessThanOrEqual => FloatCC::LessThanOrEqual,
                _ => unreachable!(),
            };
            let cmp = builder.ins().fcmp(cc, lhs, rhs);
            Ok(value::from_bool(builder, cmp))
        }
        other => Err(Error::Unsupported(format!(
            "relational operator not supported: {other:?}"
        ))),
    }
}

fn compile_unary(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    unary: &boa_ast::expression::operator::Unary,
) -> Result<Value, Error> {
    let target = compile_expr(builder, ctx, unary.target())?;
    match unary.op() {
        UnaryOp::Minus => {
            let target = value::to_number(builder, target);
            Ok(builder.ins().fneg(target))
        }
        UnaryOp::Plus => Ok(value::to_number(builder, target)),
        UnaryOp::Not => {
            let cond = value::truthy(builder, target);
            let not = builder.ins().icmp_imm(IntCC::Equal, cond, 0);
            Ok(value::from_bool(builder, not))
        }
        UnaryOp::Tilde => {
            let int = value::to_int32(builder, ctx.fmod_ref, target);
            let inverted = builder.ins().bxor_imm(int, -1);
            Ok(value::from_int32(builder, inverted))
        }
        UnaryOp::Void => Ok(value::undefined(builder)),
        other => Err(Error::Unsupported(format!(
            "unary operator not supported: {other:?}"
        ))),
    }
}

fn compile_assign(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    assign: &boa_ast::expression::operator::Assign,
) -> Result<Value, Error> {
    let sym = match assign.lhs() {
        AssignTarget::Identifier(id) => id.sym(),
        _ => {
            return Err(Error::Unsupported(
                "only identifier assignment is supported".to_string(),
            ));
        }
    };
    if is_const(ctx.program, ctx.scope, sym) {
        return Err(Error::Scope(
            "assignment to a constant variable".to_string(),
        ));
    }
    let (ptr, offset) = lvalue(builder, ctx, sym)?;
    let tdz = needs_tdz_check(ctx.program, ctx.scope, sym);
    let op = assign.op();

    // The logical assignment operators only evaluate and store the right-hand
    // side when the current value calls for it.
    let short_circuit = match op {
        AssignOp::BoolAnd => Some((true, true)),
        AssignOp::BoolOr => Some((false, true)),
        AssignOp::Coalesce => Some((true, false)),
        _ => None,
    };
    if let Some((rhs_when_truthy, use_truthy)) = short_circuit {
        let old = load_at(builder, ptr, offset);
        if tdz {
            check_tdz(builder, old);
        }
        let cond = if use_truthy {
            value::truthy(builder, old)
        } else {
            value::is_nullish(builder, old)
        };

        let result_var = builder.declare_var(types::F64);
        let rhs_block = builder.create_block();
        let join_block = builder.create_block();
        builder.def_var(result_var, old);
        if rhs_when_truthy {
            builder.ins().brif(cond, rhs_block, &[], join_block, &[]);
        } else {
            builder.ins().brif(cond, join_block, &[], rhs_block, &[]);
        }

        builder.switch_to_block(rhs_block);
        let rhs = compile_expr(builder, ctx, assign.rhs())?;
        store_at(builder, ptr, offset, rhs);
        builder.def_var(result_var, rhs);
        builder.ins().jump(join_block, &[]);

        builder.switch_to_block(join_block);
        return Ok(builder.use_var(result_var));
    }

    if op == AssignOp::Assign {
        let rhs = compile_expr(builder, ctx, assign.rhs())?;
        if tdz {
            let current = load_at(builder, ptr, offset);
            check_tdz(builder, current);
        }
        store_at(builder, ptr, offset, rhs);
        return Ok(rhs);
    }

    // Compound assignment reads the old value before evaluating the right-hand
    // side, matching the evaluation order of the specification.
    let old = load_at(builder, ptr, offset);
    if tdz {
        check_tdz(builder, old);
    }
    let rhs = compile_expr(builder, ctx, assign.rhs())?;
    let value = match op {
        AssignOp::Add => compile_arithmetic(builder, ctx, ArithmeticOp::Add, old, rhs)?,
        AssignOp::Sub => compile_arithmetic(builder, ctx, ArithmeticOp::Sub, old, rhs)?,
        AssignOp::Mul => compile_arithmetic(builder, ctx, ArithmeticOp::Mul, old, rhs)?,
        AssignOp::Div => compile_arithmetic(builder, ctx, ArithmeticOp::Div, old, rhs)?,
        AssignOp::Mod => compile_arithmetic(builder, ctx, ArithmeticOp::Mod, old, rhs)?,
        AssignOp::Exp => compile_arithmetic(builder, ctx, ArithmeticOp::Exp, old, rhs)?,
        AssignOp::And => compile_bitwise(builder, ctx, BitwiseOp::And, old, rhs),
        AssignOp::Or => compile_bitwise(builder, ctx, BitwiseOp::Or, old, rhs),
        AssignOp::Xor => compile_bitwise(builder, ctx, BitwiseOp::Xor, old, rhs),
        AssignOp::Shl => compile_bitwise(builder, ctx, BitwiseOp::Shl, old, rhs),
        AssignOp::Shr => compile_bitwise(builder, ctx, BitwiseOp::Shr, old, rhs),
        AssignOp::Ushr => compile_bitwise(builder, ctx, BitwiseOp::UShr, old, rhs),
        other => {
            return Err(Error::Unsupported(format!(
                "assignment operator not supported: {other:?}"
            )));
        }
    };
    store_at(builder, ptr, offset, value);
    Ok(value)
}

fn compile_update(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    update: &boa_ast::expression::operator::Update,
) -> Result<Value, Error> {
    let sym = match update.target() {
        UpdateTarget::Identifier(id) => id.sym(),
        _ => {
            return Err(Error::Unsupported(
                "only identifier updates are supported".to_string(),
            ));
        }
    };
    if is_const(ctx.program, ctx.scope, sym) {
        return Err(Error::Scope(
            "assignment to a constant variable".to_string(),
        ));
    }
    let old = load_var(builder, ctx, sym)?;
    let old = value::to_number(builder, old);
    let one = builder.ins().f64const(1.0);
    let (new_value, result) = match update.op() {
        UpdateOp::IncrementPre => (builder.ins().fadd(old, one), None),
        UpdateOp::IncrementPost => (builder.ins().fadd(old, one), Some(old)),
        UpdateOp::DecrementPre => (builder.ins().fsub(old, one), None),
        UpdateOp::DecrementPost => (builder.ins().fsub(old, one), Some(old)),
    };
    store_var(builder, ctx, sym, new_value)?;
    Ok(result.unwrap_or(new_value))
}

/// True when `sym` is not bound in scope, i.e. it names a global builtin.
fn is_free_global(ctx: &CodegenCtx<'_>, sym: Sym) -> bool {
    resolve(ctx.program, ctx.scope, sym).is_none()
}

/// Recognises `Math.member`, `Number.member` and `Object.member`.
fn builtin_member(
    ctx: &CodegenCtx<'_>,
    access: &SimplePropertyAccess,
) -> Option<(builtins::Namespace, &'static str)> {
    let Expression::Identifier(id) = access.target() else {
        return None;
    };
    if !is_free_global(ctx, id.sym()) {
        return None;
    }
    let namespace = ctx.builtins.namespace(id.sym())?;
    let PropertyAccessField::Const(field) = access.field() else {
        return None;
    };
    let name = ctx.builtins.property(field.sym())?;
    ctx.builtins
        .has(namespace, name)
        .then_some((namespace, name))
}

enum BuiltinCallee {
    Global(&'static str),
    Member(builtins::Namespace, &'static str),
}

/// Compiles `Math.foo(...)`, `Number.foo(...)`, `Object.foo(...)` and the
/// global conversion/predicate functions.
fn compile_builtin_call(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    callee: BuiltinCallee,
    args: &[Expression],
) -> Result<Value, Error> {
    let (namespace, name) = match callee {
        BuiltinCallee::Global(name) => (None, name),
        BuiltinCallee::Member(namespace, name) => {
            if ctx.builtins.constant(namespace, name).is_some() {
                return Err(Error::Unsupported(format!("{name} is not a function")));
            }
            (Some(namespace), name)
        }
    };

    let mut values = Vec::with_capacity(args.len());
    for arg in args {
        values.push(compile_expr(builder, ctx, arg)?);
    }
    builtins::call(
        builder,
        ctx.builtin_refs,
        ctx.fmod_ref,
        namespace,
        name,
        &values,
    )
}

/// Recognises a call to a builtin member or a free global builtin.
fn builtin_callee(ctx: &CodegenCtx<'_>, callee: &Expression) -> Option<BuiltinCallee> {
    match callee {
        Expression::PropertyAccess(PropertyAccess::Simple(access)) => builtin_member(ctx, access)
            .map(|(namespace, name)| BuiltinCallee::Member(namespace, name)),
        Expression::Identifier(id) if is_free_global(ctx, id.sym()) => ctx
            .builtins
            .global_function(id.sym())
            .map(BuiltinCallee::Global),
        _ => None,
    }
}

fn compile_property_access(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    access: &PropertyAccess,
) -> Result<Value, Error> {
    let constant = match access {
        PropertyAccess::Simple(simple) => builtin_member(ctx, simple)
            .and_then(|(namespace, name)| ctx.builtins.constant(namespace, name)),
        _ => None,
    };
    match constant {
        Some(value) => Ok(builder.ins().f64const(value)),
        None => Err(Error::Unsupported(
            "only Math and Number constants can be read as properties".to_string(),
        )),
    }
}

fn compile_call(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    call: &boa_ast::expression::Call,
) -> Result<Value, Error> {
    if let Some(builtin) = builtin_callee(ctx, call.function()) {
        return compile_builtin_call(builder, ctx, builtin, call.args());
    }

    if call.args().len() > MAX_ARGS {
        return Err(Error::Unsupported(format!(
            "calls with more than {MAX_ARGS} arguments are not supported"
        )));
    }

    let callee = compile_expr(builder, ctx, call.function())?;
    let callee_ptr = value::payload(builder, callee);
    let code = builder
        .ins()
        .load(types::I64, MemFlags::new(), callee_ptr, 0i32);
    let captured_env = builder
        .ins()
        .load(types::I64, MemFlags::new(), callee_ptr, 8i32);

    let args_ptr = call_calloc(builder, ctx.calloc_ref, 8 + 8 * MAX_ARGS as i64);
    let len = builder.ins().iconst(types::I64, call.args().len() as i64);
    builder.ins().store(MemFlags::new(), len, args_ptr, 0i32);

    for (i, arg) in call.args().iter().enumerate() {
        let value = compile_expr(builder, ctx, arg)?;
        builder
            .ins()
            .store(MemFlags::new(), value, args_ptr, (8 + 8 * i) as i32);
    }

    let call = builder
        .ins()
        .call_indirect(ctx.js_sig_ref, code, &[captured_env, args_ptr, callee]);
    Ok(builder.inst_results(call)[0])
}

/// Materialises a function or arrow expression as a closure over the current
/// environment.
fn compile_closure(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    span: Span,
) -> Result<Value, Error> {
    let child = *ctx
        .program
        .function(ctx.plan_id)
        .nested
        .get(&span)
        .ok_or_else(|| Error::Codegen("missing anonymous function plan".to_string()))?;
    let func_ref = *ctx
        .child_refs
        .get(&child)
        .ok_or_else(|| Error::Codegen("missing anonymous function reference".to_string()))?;
    let code = builder.ins().func_addr(types::I64, func_ref);
    Ok(new_closure(builder, ctx.calloc_ref, code, ctx.env))
}
