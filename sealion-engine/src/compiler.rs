use std::collections::HashMap;

use boa_ast::{
    Declaration, Statement, StatementList, StatementListItem,
    expression::{
        Expression,
        literal::LiteralKind,
        operator::{
            assign::{AssignOp, AssignTarget},
            binary::{ArithmeticOp, BinaryOp, BitwiseOp, LogicalOp, RelationalOp},
            unary::UnaryOp,
            update::{UpdateOp, UpdateTarget},
        },
    },
};
use boa_interner::Sym;
use cranelift_codegen::ir::{
    AbiParam, Block, FuncRef, Function, InstBuilder, MemFlags, SigRef, Signature, UserFuncName,
    Value, condcodes::FloatCC, condcodes::IntCC, types,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Module};
use cranelift_object::ObjectModule;

use crate::{
    Error, IrFunction,
    plan::{FunctionPlan, resolve},
    value,
};

const MAX_ARGS: usize = 32;

pub fn make_js_signature<M: Module>(module: &M) -> Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
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

pub fn compile_functions(
    module: &mut ObjectModule,
    plans: &[FunctionPlan],
    func_ids: &[FuncId],
    js_sig: &Signature,
    calloc_id: FuncId,
    pow_id: FuncId,
    fmod_id: FuncId,
) -> Result<Vec<IrFunction>, Error> {
    let mut raws = Vec::new();

    for plan in plans {
        let mut func =
            Function::with_name_signature(UserFuncName::user(0, plan.id as u32), js_sig.clone());

        let calloc_ref = module.declare_func_in_func(calloc_id, &mut func);
        let pow_ref = module.declare_func_in_func(pow_id, &mut func);
        let fmod_ref = module.declare_func_in_func(fmod_id, &mut func);
        let mut child_refs = HashMap::new();
        for &child in &plan.children {
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
        let env = alloc_env(&mut builder, calloc_ref, plan.slot_count(), env_param);

        let undef = value::undefined(&mut builder);
        for slot in 0..plan.slot_count() {
            builder
                .ins()
                .store(MemFlags::new(), undef, env, (8 + 8 * slot) as i32);
        }

        if !plan.is_script {
            let len = builder
                .ins()
                .load(types::I64, MemFlags::new(), args_param, 0i32);
            for i in 0..plan.params.len() {
                let offset = (8 + 8 * i) as i32;
                let idx = builder.ins().iconst(types::I64, i as i64);
                let present = builder.ins().icmp(IntCC::UnsignedLessThan, idx, len);
                let loaded = builder
                    .ins()
                    .load(types::F64, MemFlags::new(), args_param, offset);
                let arg = builder.ins().select(present, loaded, undef);
                builder.ins().store(MemFlags::new(), arg, env, offset);
            }
        }

        for &child_id in &plan.children {
            let child_plan = &plans[child_id];
            let sym = child_plan
                .binding_name
                .ok_or_else(|| Error::Codegen("missing function binding".to_string()))?;
            let slot = *plan
                .bindings
                .get(&sym)
                .ok_or_else(|| Error::Codegen("missing function slot".to_string()))?;
            let code = builder.ins().func_addr(types::I64, child_refs[&child_id]);
            let closure = new_closure(&mut builder, calloc_ref, code, env);
            builder
                .ins()
                .store(MemFlags::new(), closure, env, (8 + 8 * slot) as i32);
        }

        let ctx = CodegenCtx {
            plans,
            plan_id: plan.id,
            env,
            calloc_ref,
            pow_ref,
            fmod_ref,
            js_sig_ref,
        };

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
    let call = builder.ins().call(script_ref, &[zero, zero]);
    let result = builder.inst_results(call)[0];
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

struct CodegenCtx<'a> {
    plans: &'a [FunctionPlan],
    plan_id: usize,
    env: Value,
    calloc_ref: FuncRef,
    pow_ref: FuncRef,
    fmod_ref: FuncRef,
    js_sig_ref: SigRef,
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

struct LoopTarget {
    break_block: Block,
    continue_block: Block,
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
    let (depth, slot) = resolve(ctx.plans, ctx.plan_id, sym)
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

fn load_var(builder: &mut FunctionBuilder, ctx: &CodegenCtx<'_>, sym: Sym) -> Result<Value, Error> {
    let (ptr, offset) = lvalue(builder, ctx, sym)?;
    Ok(load_at(builder, ptr, offset))
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
            compile_var_list(builder, ctx, lex.variable_list())?;
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
        Statement::Block(block) => compile_statements(builder, ctx, block.statement_list(), loops),
        Statement::Var(var) => {
            compile_var_list(builder, ctx, &var.0)?;
            Ok(StmtOutcome::default())
        }
        Statement::Empty => Ok(StmtOutcome::default()),
        Statement::Expression(expr) => {
            let value = compile_expr(builder, ctx, expr)?;
            Ok(StmtOutcome {
                last: Some(value),
                terminated: false,
            })
        }
        Statement::If(if_stmt) => compile_if(builder, ctx, if_stmt, loops),
        Statement::WhileLoop(while_stmt) => compile_while(builder, ctx, while_stmt, loops),
        Statement::Break(_) => {
            let target = loops
                .last()
                .ok_or_else(|| Error::Scope("break statement outside of a loop".to_string()))?;
            builder.ins().jump(target.break_block, &[]);
            Ok(StmtOutcome {
                last: None,
                terminated: true,
            })
        }
        Statement::Continue(_) => {
            let target = loops
                .last()
                .ok_or_else(|| Error::Scope("continue statement outside of a loop".to_string()))?;
            builder.ins().jump(target.continue_block, &[]);
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
        break_block: exit,
        continue_block: header,
    });
    let body_out = compile_statement(builder, ctx, while_stmt.body(), loops)?;
    loops.pop();
    if !body_out.terminated {
        builder.ins().jump(header, &[]);
    }

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
        Expression::Identifier(id) => load_var(builder, ctx, id.sym()),
        Expression::Parenthesized(paren) => compile_expr(builder, ctx, paren.expression()),
        Expression::Binary(binary) => compile_binary(builder, ctx, binary),
        Expression::Conditional(conditional) => compile_conditional(builder, ctx, conditional),
        Expression::Unary(unary) => compile_unary(builder, ctx, unary),
        Expression::Assign(assign) => compile_assign(builder, ctx, assign),
        Expression::Update(update) => compile_update(builder, ctx, update),
        Expression::Call(call) => compile_call(builder, ctx, call),
        other => Err(Error::Unsupported(format!(
            "expression not supported: {other:?}"
        ))),
    }
}

fn compile_literal(builder: &mut FunctionBuilder, kind: &LiteralKind) -> Result<Value, Error> {
    let value = match kind {
        LiteralKind::Num(n) => builder.ins().f64const(*n),
        LiteralKind::Int(i) => builder.ins().f64const(*i as f64),
        LiteralKind::Bool(b) => {
            let n = if *b { 1.0 } else { 0.0 };
            builder.ins().f64const(n)
        }
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
    let cc = match op {
        RelationalOp::Equal => return Ok(value::loose_equal(builder, lhs, rhs)),
        RelationalOp::StrictEqual => return Ok(value::strict_equal(builder, lhs, rhs)),
        RelationalOp::NotEqual | RelationalOp::StrictNotEqual => {
            let eq = if op == RelationalOp::NotEqual {
                value::loose_equal(builder, lhs, rhs)
            } else {
                value::strict_equal(builder, lhs, rhs)
            };
            let zero = builder.ins().f64const(0.0);
            let cmp = builder.ins().fcmp(FloatCC::Equal, eq, zero);
            return Ok(value::from_bool(builder, cmp));
        }
        RelationalOp::GreaterThan => FloatCC::GreaterThan,
        RelationalOp::GreaterThanOrEqual => FloatCC::GreaterThanOrEqual,
        RelationalOp::LessThan => FloatCC::LessThan,
        RelationalOp::LessThanOrEqual => FloatCC::LessThanOrEqual,
        other => {
            return Err(Error::Unsupported(format!(
                "relational operator not supported: {other:?}"
            )));
        }
    };
    let cmp = builder.ins().fcmp(cc, lhs, rhs);
    Ok(value::from_bool(builder, cmp))
}

fn compile_unary(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    unary: &boa_ast::expression::operator::Unary,
) -> Result<Value, Error> {
    let target = compile_expr(builder, ctx, unary.target())?;
    match unary.op() {
        UnaryOp::Minus => Ok(builder.ins().fneg(target)),
        UnaryOp::Plus => Ok(target),
        UnaryOp::Not => {
            let cond = value::truthy(builder, target);
            let zero = builder.ins().f64const(0.0);
            let one = builder.ins().f64const(1.0);
            Ok(builder.ins().select(cond, zero, one))
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
    let (ptr, offset) = lvalue(builder, ctx, sym)?;
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
        store_at(builder, ptr, offset, rhs);
        return Ok(rhs);
    }

    // Compound assignment reads the old value before evaluating the right-hand
    // side, matching the evaluation order of the specification.
    let old = load_at(builder, ptr, offset);
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
    let old = load_var(builder, ctx, sym)?;
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

fn compile_call(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    call: &boa_ast::expression::Call,
) -> Result<Value, Error> {
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
        .call_indirect(ctx.js_sig_ref, code, &[captured_env, args_ptr]);
    Ok(builder.inst_results(call)[0])
}
