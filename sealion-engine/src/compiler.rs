use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::{
    AbiParam, Block, FuncRef, Function, InstBuilder, MemFlags, SigRef, Signature, TrapCode,
    UserFuncName, Value, condcodes::FloatCC, condcodes::IntCC, types,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{DataId, FuncId, Module};
use cranelift_object::ObjectModule;
use oxc_ast::ast::{
    Argument, AssignmentExpression, AssignmentOperator, AssignmentTarget, BinaryExpression,
    BinaryOperator, BindingPattern, BlockStatement, CallExpression, ChainElement, ChainExpression,
    ConditionalExpression, DoWhileStatement, Expression, ForStatement, ForStatementInit,
    IfStatement, LabeledStatement, LogicalOperator, SimpleAssignmentTarget, Statement,
    StaticMemberExpression, SwitchStatement, TryStatement, UnaryExpression, UnaryOperator,
    UpdateExpression, UpdateOperator, VariableDeclaration, VariableDeclarationKind, WhileStatement,
};

use crate::{
    Error, IrFunction,
    builtins::{self, Builtins},
    plan::{
        Program, ScopeId, ScopePlan, is_const, needs_tdz_check, node_key, resolve, resolve_binding,
    },
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

pub fn make_nullary_f64_signature<M: Module>(module: &M) -> Signature {
    let mut sig = module.make_signature();
    sig.returns.push(AbiParam::new(types::F64));
    sig
}

pub fn make_i64_unary_signature<M: Module>(module: &M) -> Signature {
    let mut sig = module.make_signature();
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

/// The libc entry points every compiled function may call.
pub struct Imports {
    pub calloc: FuncId,
    pub pow: FuncId,
    pub fmod: FuncId,
    pub exception: DataId,
}

pub fn compile_functions<'a>(
    module: &mut ObjectModule,
    program: &'a Program<'a>,
    func_ids: &[FuncId],
    js_sig: &Signature,
    imports: &Imports,
    builtins: &'a Builtins,
) -> Result<Vec<IrFunction>, Error> {
    let numeric = analyze_numeric_bindings(program, builtins);
    let mut raws = Vec::new();

    for plan in &program.functions {
        let mut func =
            Function::with_name_signature(UserFuncName::user(0, plan.id as u32), js_sig.clone());

        let calloc_ref = module.declare_func_in_func(imports.calloc, &mut func);
        let pow_ref = module.declare_func_in_func(imports.pow, &mut func);
        let fmod_ref = module.declare_func_in_func(imports.fmod, &mut func);
        let exception_global = module.declare_data_in_func(imports.exception, &mut func);
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
        let exception = builder.ins().global_value(types::I64, exception_global);
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
        };

        let mut flow = Flow::new();
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
            let closure = new_closure(&mut builder, calloc_ref, code, ctx.env);
            store_var(&mut builder, &ctx, name, closure)?;
        }

        let out = compile_statements(&mut builder, &ctx, plan.body, &mut flow)?;
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
    exception_id: DataId,
) -> Result<IrFunction, Error> {
    let mut func = Function::with_name_signature(UserFuncName::testcase("main"), main_sig.clone());
    let script_ref = module.declare_func_in_func(script_id, &mut func);
    let exception_global = module.declare_data_in_func(exception_id, &mut func);

    let mut builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut func, &mut builder_ctx);

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);

    let exception = builder.ins().global_value(types::I64, exception_global);
    let zero = builder.ins().iconst(types::I64, 0);
    let self_value = value::undefined(&mut builder);
    let call = builder.ins().call(script_ref, &[zero, zero, self_value]);
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

/// True when a builtin call always returns a plain number.
fn numeric_builtin_call(
    program: &Program<'_>,
    builtins: &Builtins,
    scope: ScopeId,
    call: &CallExpression<'_>,
) -> bool {
    match &call.callee {
        Expression::StaticMemberExpression(access) => {
            let Expression::Identifier(id) = &access.object else {
                return false;
            };
            if resolve_binding(program, scope, id.name.as_str()).is_some() {
                return false;
            }
            let Some(namespace) = builtins.namespace(id.name.as_str()) else {
                return false;
            };
            let Some(name) = builtins.property(access.property.name.as_str()) else {
                return false;
            };
            builtins.has(namespace, name) && matches!(namespace, builtins::Namespace::Math)
        }
        Expression::Identifier(id)
            if resolve_binding(program, scope, id.name.as_str()).is_none() =>
        {
            matches!(builtins.global_function(id.name.as_str()), Some("Number"))
        }
        _ => false,
    }
}

fn numeric_property(
    program: &Program<'_>,
    builtins: &Builtins,
    scope: ScopeId,
    access: &StaticMemberExpression<'_>,
) -> bool {
    let Expression::Identifier(id) = &access.object else {
        return false;
    };
    if resolve_binding(program, scope, id.name.as_str()).is_some() {
        return false;
    }
    let Some(namespace) = builtins.namespace(id.name.as_str()) else {
        return false;
    };
    let Some(name) = builtins.property(access.property.name.as_str()) else {
        return false;
    };
    builtins.constant(namespace, name).is_some()
}

/// True when evaluating `expr` is guaranteed to produce an untagged number.
fn known_number_expr(
    program: &Program<'_>,
    builtins: &Builtins,
    numeric: &HashSet<(ScopeId, usize)>,
    scope: ScopeId,
    expr: &Expression<'_>,
) -> bool {
    match expr {
        Expression::NumericLiteral(_) => true,
        Expression::ParenthesizedExpression(paren) => {
            known_number_expr(program, builtins, numeric, scope, &paren.expression)
        }
        Expression::Identifier(id) => match resolve_binding(program, scope, id.name.as_str()) {
            Some(key) => numeric.contains(&key),
            None => builtins.global_constant(id.name.as_str()).is_some(),
        },
        Expression::BinaryExpression(binary) => {
            binary.operator.is_arithmetic() || binary.operator.is_bitwise()
        }
        Expression::UnaryExpression(unary) => matches!(
            unary.operator,
            UnaryOperator::UnaryPlus | UnaryOperator::UnaryNegation | UnaryOperator::BitwiseNot
        ),
        Expression::ConditionalExpression(conditional) => {
            known_number_expr(program, builtins, numeric, scope, &conditional.consequent)
                && known_number_expr(program, builtins, numeric, scope, &conditional.alternate)
        }
        Expression::AssignmentExpression(assign) => match assign.operator {
            AssignmentOperator::Assign => {
                known_number_expr(program, builtins, numeric, scope, &assign.right)
            }
            AssignmentOperator::LogicalAnd
            | AssignmentOperator::LogicalOr
            | AssignmentOperator::LogicalNullish => false,
            _ => true,
        },
        Expression::UpdateExpression(_) => true,
        Expression::SequenceExpression(sequence) => sequence
            .expressions
            .last()
            .is_some_and(|expr| known_number_expr(program, builtins, numeric, scope, expr)),
        Expression::CallExpression(call) => numeric_builtin_call(program, builtins, scope, call),
        Expression::StaticMemberExpression(access) => {
            numeric_property(program, builtins, scope, access)
        }
        _ => false,
    }
}

fn expr_is_number<'a>(ctx: &CodegenCtx<'a>, expr: &Expression<'a>) -> bool {
    known_number_expr(ctx.program, ctx.builtins, ctx.numeric, ctx.scope, expr)
}

fn binding_is_number(ctx: &CodegenCtx<'_>, name: &str) -> bool {
    resolve_binding(ctx.program, ctx.scope, name).is_some_and(|key| ctx.numeric.contains(&key))
}

fn number_value(builder: &mut FunctionBuilder, value: Value, known: bool) -> Value {
    if known {
        value
    } else {
        value::to_number(builder, value)
    }
}

/// The value a write stores is numeric for plain assignments when the RHS is
/// numeric, and always for arithmetic/bitwise compound assignments.
fn assignment_write_is_numeric(
    program: &Program<'_>,
    builtins: &Builtins,
    numeric: &HashSet<(ScopeId, usize)>,
    scope: ScopeId,
    op: AssignmentOperator,
    rhs: &Expression<'_>,
) -> bool {
    match op {
        AssignmentOperator::Assign
        | AssignmentOperator::LogicalAnd
        | AssignmentOperator::LogicalOr
        | AssignmentOperator::LogicalNullish => {
            known_number_expr(program, builtins, numeric, scope, rhs)
        }
        _ => true,
    }
}

struct NumericWalker<'a> {
    program: &'a Program<'a>,
    builtins: &'a Builtins,
    numeric: &'a HashSet<(ScopeId, usize)>,
    removed: &'a mut HashSet<(ScopeId, usize)>,
}

impl<'a> NumericWalker<'a> {
    fn statements(&mut self, plan_id: usize, scope: ScopeId, list: &[Statement<'a>]) {
        for stmt in list {
            self.statement(plan_id, scope, stmt);
        }
    }

    fn node_scope(&self, plan_id: usize, scope: ScopeId, key: usize) -> ScopeId {
        self.program
            .function(plan_id)
            .blocks
            .get(&key)
            .copied()
            .unwrap_or(scope)
    }

    fn variable_list(&mut self, scope: ScopeId, decl: &VariableDeclaration<'a>) {
        let lexical = decl.kind != VariableDeclarationKind::Var;
        for var in &decl.declarations {
            let init = &var.init;
            if lexical
                && let BindingPattern::BindingIdentifier(id) = &var.id
                && let Some(key) = resolve_binding(self.program, scope, id.name.as_str())
                && self.numeric.contains(&key)
            {
                let numeric = init.as_ref().is_some_and(|expr| {
                    known_number_expr(self.program, self.builtins, self.numeric, scope, expr)
                });
                if !numeric {
                    self.removed.insert(key);
                }
            }
            if let Some(expr) = init {
                self.expr(scope, expr);
            }
        }
    }

    fn statement(&mut self, plan_id: usize, scope: ScopeId, stmt: &Statement<'a>) {
        match stmt {
            Statement::BlockStatement(block) => {
                let inner = self.node_scope(plan_id, scope, node_key(&**block));
                self.statements(plan_id, inner, block.body.as_slice());
            }
            Statement::VariableDeclaration(decl) => {
                self.variable_list(scope, decl);
            }
            Statement::ExpressionStatement(expr) => self.expr(scope, &expr.expression),
            Statement::IfStatement(if_stmt) => {
                self.expr(scope, &if_stmt.test);
                self.statement(plan_id, scope, &if_stmt.consequent);
                if let Some(else_node) = &if_stmt.alternate {
                    self.statement(plan_id, scope, else_node);
                }
            }
            Statement::WhileStatement(while_stmt) => {
                self.expr(scope, &while_stmt.test);
                self.statement(plan_id, scope, &while_stmt.body);
            }
            Statement::DoWhileStatement(do_while) => {
                self.statement(plan_id, scope, &do_while.body);
                self.expr(scope, &do_while.test);
            }
            Statement::ForStatement(for_stmt) => {
                let mut loop_scope = scope;
                match &for_stmt.init {
                    Some(ForStatementInit::VariableDeclaration(decl))
                        if decl.kind != VariableDeclarationKind::Var =>
                    {
                        loop_scope = self.node_scope(plan_id, scope, node_key(&**for_stmt));
                        self.variable_list(loop_scope, decl);
                    }
                    Some(ForStatementInit::VariableDeclaration(decl)) => {
                        self.variable_list(loop_scope, decl);
                    }
                    Some(init) => {
                        if let Some(expr) = init.as_expression() {
                            self.expr(loop_scope, expr);
                        }
                    }
                    None => {}
                }
                if let Some(condition) = &for_stmt.test {
                    self.expr(loop_scope, condition);
                }
                if let Some(update) = &for_stmt.update {
                    self.expr(loop_scope, update);
                }
                self.statement(plan_id, loop_scope, &for_stmt.body);
            }
            Statement::SwitchStatement(switch_stmt) => {
                let inner = self.node_scope(plan_id, scope, node_key(&**switch_stmt));
                self.expr(scope, &switch_stmt.discriminant);
                for case in &switch_stmt.cases {
                    if let Some(condition) = &case.test {
                        self.expr(inner, condition);
                    }
                    self.statements(plan_id, inner, case.consequent.as_slice());
                }
            }
            Statement::ReturnStatement(ret) => {
                if let Some(target) = &ret.argument {
                    self.expr(scope, target);
                }
            }
            Statement::LabeledStatement(labelled) => {
                self.statement(plan_id, scope, &labelled.body);
            }
            Statement::ThrowStatement(throw) => {
                self.expr(scope, &throw.argument);
            }
            Statement::TryStatement(try_stmt) => {
                let inner = self.node_scope(plan_id, scope, node_key(&*try_stmt.block));
                self.statements(plan_id, inner, try_stmt.block.body.as_slice());
                if let Some(handler) = &try_stmt.handler {
                    let catch_scope = self.node_scope(plan_id, scope, node_key(&**handler));
                    self.statements(plan_id, catch_scope, handler.body.body.as_slice());
                }
                if let Some(finalizer) = &try_stmt.finalizer {
                    let finalizer_scope = self.node_scope(plan_id, scope, node_key(&**finalizer));
                    self.statements(plan_id, finalizer_scope, finalizer.body.as_slice());
                }
            }
            _ => {}
        }
    }

    fn chain_element(&mut self, scope: ScopeId, element: &ChainElement<'a>) {
        match element {
            ChainElement::CallExpression(call) => {
                self.expr(scope, &call.callee);
                for arg in &call.arguments {
                    if let Some(expr) = arg.as_expression() {
                        self.expr(scope, expr);
                    }
                }
            }
            ChainElement::ComputedMemberExpression(member) => {
                self.expr(scope, &member.object);
                self.expr(scope, &member.expression);
            }
            ChainElement::StaticMemberExpression(member) => {
                self.expr(scope, &member.object);
            }
            ChainElement::PrivateFieldExpression(member) => {
                self.expr(scope, &member.object);
            }
            _ => {}
        }
    }

    fn expr(&mut self, scope: ScopeId, expr: &Expression<'a>) {
        match expr {
            Expression::ParenthesizedExpression(paren) => {
                self.expr(scope, &paren.expression);
            }
            Expression::BinaryExpression(binary) => {
                self.expr(scope, &binary.left);
                self.expr(scope, &binary.right);
            }
            Expression::LogicalExpression(logical) => {
                self.expr(scope, &logical.left);
                self.expr(scope, &logical.right);
            }
            Expression::UnaryExpression(unary) => self.expr(scope, &unary.argument),
            Expression::ConditionalExpression(conditional) => {
                self.expr(scope, &conditional.test);
                self.expr(scope, &conditional.consequent);
                self.expr(scope, &conditional.alternate);
            }
            Expression::AssignmentExpression(assign) => {
                if let AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left
                    && let Some(key) = resolve_binding(self.program, scope, id.name.as_str())
                    && self.numeric.contains(&key)
                    && !assignment_write_is_numeric(
                        self.program,
                        self.builtins,
                        self.numeric,
                        scope,
                        assign.operator,
                        &assign.right,
                    )
                {
                    self.removed.insert(key);
                }
                self.expr(scope, &assign.right);
            }
            Expression::SequenceExpression(sequence) => {
                for expr in &sequence.expressions {
                    self.expr(scope, expr);
                }
            }
            Expression::CallExpression(call) => {
                self.expr(scope, &call.callee);
                for arg in &call.arguments {
                    if let Some(expr) = arg.as_expression() {
                        self.expr(scope, expr);
                    }
                }
            }
            Expression::ChainExpression(chain) => {
                self.chain_element(scope, &chain.expression);
            }
            Expression::StaticMemberExpression(simple) => {
                self.expr(scope, &simple.object);
            }
            Expression::ComputedMemberExpression(computed) => {
                self.expr(scope, &computed.object);
                self.expr(scope, &computed.expression);
            }
            Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => {}
            _ => {}
        }
    }
}

/// Greatest fixed point of bindings that always hold numbers. Only lexical
/// bindings are considered: `var` can be read as `undefined` before its
/// initializer, and parameters may be missing.
fn analyze_numeric_bindings<'a>(
    program: &'a Program<'a>,
    builtins: &'a Builtins,
) -> HashSet<(ScopeId, usize)> {
    let mut numeric = HashSet::new();
    for (index, scope) in program.scopes.iter().enumerate() {
        for &slot in &scope.tdz {
            numeric.insert((ScopeId(index), slot));
        }
    }
    for plan in &program.functions {
        for param in &plan.params {
            if let Some(key) = resolve_binding(program, plan.root_scope, param.name) {
                numeric.remove(&key);
            }
        }
    }

    loop {
        let mut removed = HashSet::new();
        for plan in &program.functions {
            let mut walker = NumericWalker {
                program,
                builtins,
                numeric: &numeric,
                removed: &mut removed,
            };
            for param in &plan.params {
                if let Some(init) = param.init {
                    walker.expr(plan.root_scope, init);
                }
            }
            walker.statements(plan.id, plan.root_scope, plan.body);
        }
        if removed.is_empty() {
            break;
        }
        for key in removed {
            numeric.remove(&key);
        }
    }
    numeric
}

#[derive(Clone, Copy)]
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

fn init_slots(builder: &mut FunctionBuilder, env: Value, scope: &ScopePlan<'_>) {
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

/// Declares a register-backed local variable for every binding whose scope is
/// not captured by a closure.
fn declare_vars(
    builder: &mut FunctionBuilder,
    program: &Program<'_>,
    plan_id: usize,
) -> HashMap<(ScopeId, usize), Variable> {
    let mut vars = HashMap::new();
    for (index, scope) in program.scopes.iter().enumerate() {
        if scope.owner == plan_id && !scope.has_env() {
            for slot in 0..scope.slot_count() {
                vars.insert((ScopeId(index), slot), builder.declare_var(types::F64));
            }
        }
    }
    vars
}

fn init_local_slots(
    builder: &mut FunctionBuilder,
    vars: &HashMap<(ScopeId, usize), Variable>,
    scope_id: ScopeId,
    scope: &ScopePlan<'_>,
) {
    for slot in 0..scope.slot_count() {
        let initial = if scope.tdz.contains(&slot) {
            value::uninitialized(builder)
        } else {
            value::undefined(builder)
        };
        let var = vars[&(scope_id, slot)];
        builder.def_var(var, initial);
    }
}

/// Evaluates a parameter initializer only when the argument is `undefined`.
fn compile_default<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    arg: Value,
    init: &'a Expression<'a>,
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
    let default = compile_expr(builder, ctx, flow, init)?;
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
    if plan.has_env() {
        let env = alloc_env(builder, ctx.calloc_ref, plan.slot_count(), ctx.env);
        init_slots(builder, env, plan);
        ctx.enter(scope, env)
    } else {
        init_local_slots(builder, ctx.vars, scope, plan);
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
fn compile_block<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    block: &'a BlockStatement<'a>,
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let scope = node_scope(ctx, node_key(block))?;
    let inner = enter_scope(builder, ctx, scope);
    compile_statements(builder, &inner, block.body.as_slice(), flow)
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

fn break_target(loops: &[LoopTarget<'_>], label: Option<&str>) -> Result<(Block, usize), Error> {
    if let Some(label) = label {
        loops
            .iter()
            .rev()
            .find(|target| target.labels.contains(&label))
            .map(|target| (target.break_block, target.cleanup_depth))
            .ok_or_else(|| Error::Scope(format!("unknown break label: {label}")))
    } else {
        loops
            .iter()
            .rev()
            .find(|target| target.breakable)
            .map(|target| (target.break_block, target.cleanup_depth))
            .ok_or_else(|| Error::Scope("break statement outside of a loop".to_string()))
    }
}

fn continue_target(loops: &[LoopTarget<'_>], label: Option<&str>) -> Result<(Block, usize), Error> {
    if let Some(label) = label {
        let target = loops
            .iter()
            .rev()
            .find(|target| target.labels.contains(&label))
            .ok_or_else(|| Error::Scope(format!("unknown continue label: {label}")))?;
        target
            .continue_block
            .map(|block| (block, target.cleanup_depth))
            .ok_or_else(|| Error::Scope("continue label does not target a loop".to_string()))
    } else {
        loops
            .iter()
            .rev()
            .find_map(|target| {
                target
                    .continue_block
                    .map(|block| (block, target.cleanup_depth))
            })
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

#[derive(Clone, Copy)]
enum Place {
    Env { ptr: Value, offset: i32 },
    Local { var: Variable },
}

/// Resolves an identifier to either a slot in an environment record or a
/// register-backed local variable.
fn place(builder: &mut FunctionBuilder, ctx: &CodegenCtx<'_>, name: &str) -> Result<Place, Error> {
    let (home, slot) = resolve_binding(ctx.program, ctx.scope, name)
        .ok_or_else(|| Error::Scope(format!("unresolved identifier: {name}")))?;
    if ctx.program.scope(home).has_env() {
        let (depth, _) = resolve(ctx.program, ctx.scope, name)
            .ok_or_else(|| Error::Scope(format!("unresolved identifier: {name}")))?;
        let mut ptr = ctx.env;
        for _ in 0..depth {
            ptr = builder.ins().load(types::I64, MemFlags::new(), ptr, 0i32);
        }
        Ok(Place::Env {
            ptr,
            offset: (8 + 8 * slot) as i32,
        })
    } else {
        let var = *ctx
            .vars
            .get(&(home, slot))
            .ok_or_else(|| Error::Codegen("missing local variable".to_string()))?;
        Ok(Place::Local { var })
    }
}

fn load_at(builder: &mut FunctionBuilder, ptr: Value, offset: i32) -> Value {
    builder.ins().load(types::F64, MemFlags::new(), ptr, offset)
}

fn store_at(builder: &mut FunctionBuilder, ptr: Value, offset: i32, value: Value) {
    builder.ins().store(MemFlags::new(), value, ptr, offset);
}

fn load_place(builder: &mut FunctionBuilder, place: Place) -> Value {
    match place {
        Place::Env { ptr, offset } => load_at(builder, ptr, offset),
        Place::Local { var } => builder.use_var(var),
    }
}

fn store_place(builder: &mut FunctionBuilder, place: Place, value: Value) {
    match place {
        Place::Env { ptr, offset } => store_at(builder, ptr, offset, value),
        Place::Local { var } => builder.def_var(var, value),
    }
}

fn check_tdz(builder: &mut FunctionBuilder, value: Value) {
    let uninitialized = value::is_uninitialized(builder, value);
    builder
        .ins()
        .trapnz(uninitialized, TrapCode::unwrap_user(1));
}

fn load_var(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    name: &str,
) -> Result<Value, Error> {
    let place = place(builder, ctx, name)?;
    let value = load_place(builder, place);
    if needs_tdz_check(ctx.program, ctx.scope, name) {
        check_tdz(builder, value);
    }
    Ok(value)
}

fn store_var(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    name: &str,
    value: Value,
) -> Result<(), Error> {
    let place = place(builder, ctx, name)?;
    store_place(builder, place, value);
    Ok(())
}

fn has_exception(builder: &mut FunctionBuilder, ctx: &CodegenCtx<'_>) -> Value {
    let pending = builder
        .ins()
        .load(types::I64, MemFlags::new(), ctx.exception, 0i32);
    builder.ins().icmp_imm(IntCC::NotEqual, pending, 0)
}

fn take_exception(builder: &mut FunctionBuilder, ctx: &CodegenCtx<'_>) -> Value {
    let value = builder
        .ins()
        .load(types::F64, MemFlags::new(), ctx.exception, 8i32);
    let clear = builder.ins().iconst(types::I64, 0);
    builder
        .ins()
        .store(MemFlags::new(), clear, ctx.exception, 0i32);
    value
}

fn set_exception(builder: &mut FunctionBuilder, ctx: &CodegenCtx<'_>, value: Value) {
    let set = builder.ins().iconst(types::I64, 1);
    builder
        .ins()
        .store(MemFlags::new(), set, ctx.exception, 0i32);
    builder
        .ins()
        .store(MemFlags::new(), value, ctx.exception, 8i32);
}

/// Emits every finalizer between `from_depth` and `target_depth`, then jumps to
/// `continuation`. Returns false when a finalizer completes abruptly and
/// therefore overrides the original completion.
fn emit_unwind<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &Flow<'a>,
    from_depth: usize,
    target_depth: usize,
    continuation: Block,
    pending: bool,
) -> Result<bool, Error> {
    if from_depth <= target_depth {
        builder.ins().jump(continuation, &[]);
        return Ok(true);
    }

    let index = from_depth - 1;
    let cleanup = flow.cleanups[index].clone();
    let saved = if pending {
        Some(take_exception(builder, ctx))
    } else {
        None
    };
    let mut inner = Flow {
        loops: flow.loops[..cleanup.loop_depth].to_vec(),
        cleanups: flow.cleanups[..index].to_vec(),
        exception: cleanup.exception,
    };
    let finalizer_ctx = ctx.enter(cleanup.scope, cleanup.env);
    let after = builder.create_block();
    let out = compile_block(builder, &finalizer_ctx, cleanup.finalizer, &mut inner)?;
    if out.terminated {
        return Ok(false);
    }
    builder.ins().jump(after, &[]);
    builder.switch_to_block(after);
    if let Some(saved) = saved {
        set_exception(builder, ctx, saved);
    }
    emit_unwind(
        builder,
        ctx,
        flow,
        index,
        target_depth,
        continuation,
        pending,
    )
}

fn emit_pending_exception<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &Flow<'a>,
) -> Result<(), Error> {
    let depth = flow.cleanups.len();
    match flow.exception {
        Some(target) => {
            emit_unwind(
                builder,
                ctx,
                flow,
                depth,
                target.cleanup_depth,
                target.block,
                true,
            )?;
        }
        None => {
            let exit = builder.create_block();
            let reached = emit_unwind(builder, ctx, flow, depth, 0, exit, true)?;
            if reached {
                builder.switch_to_block(exit);
                let value = value::undefined(builder);
                builder.ins().return_(&[value]);
            }
        }
    }
    Ok(())
}

fn emit_exception_check<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &Flow<'a>,
) -> Result<(), Error> {
    let pending = has_exception(builder, ctx);
    let handler = builder.create_block();
    let ok = builder.create_block();
    builder.ins().brif(pending, handler, &[], ok, &[]);
    builder.switch_to_block(handler);
    emit_pending_exception(builder, ctx, flow)?;
    builder.switch_to_block(ok);
    Ok(())
}

fn compile_statements<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    list: &'a [Statement<'a>],
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let mut last = None;
    for stmt in list {
        let out = compile_statement(builder, ctx, stmt, flow)?;
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

fn compile_var_list<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    decl: &'a VariableDeclaration<'a>,
    flow: &mut Flow<'a>,
) -> Result<(), Error> {
    let lexical = decl.kind != VariableDeclarationKind::Var;
    for var in &decl.declarations {
        let BindingPattern::BindingIdentifier(id) = &var.id else {
            return Err(Error::Unsupported(
                "destructuring is not supported".to_string(),
            ));
        };
        let name = id.name.as_str();
        if let Some(init) = &var.init {
            let value = compile_expr(builder, ctx, flow, init)?;
            store_var(builder, ctx, name, value)?;
        } else if lexical {
            let value = value::undefined(builder);
            store_var(builder, ctx, name, value)?;
        }
    }
    Ok(())
}

fn compile_statement<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    stmt: &'a Statement<'a>,
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    match stmt {
        Statement::BlockStatement(block) => compile_block(builder, ctx, block, flow),
        Statement::VariableDeclaration(decl) => {
            compile_var_list(builder, ctx, decl, flow)?;
            Ok(StmtOutcome::default())
        }
        Statement::FunctionDeclaration(_)
        | Statement::EmptyStatement(_)
        | Statement::DebuggerStatement(_) => Ok(StmtOutcome::default()),
        Statement::ExpressionStatement(expr) => {
            let value = compile_expr(builder, ctx, flow, &expr.expression)?;
            Ok(StmtOutcome {
                last: Some(value),
                terminated: false,
            })
        }
        Statement::IfStatement(if_stmt) => compile_if(builder, ctx, if_stmt, flow),
        Statement::WhileStatement(while_stmt) => compile_while(builder, ctx, while_stmt, &[], flow),
        Statement::DoWhileStatement(do_while) => {
            compile_do_while(builder, ctx, do_while, &[], flow)
        }
        Statement::ForStatement(for_stmt) => compile_for(builder, ctx, for_stmt, &[], flow),
        Statement::SwitchStatement(switch_stmt) => {
            compile_switch(builder, ctx, switch_stmt, &[], flow)
        }
        Statement::LabeledStatement(labelled) => compile_labelled(builder, ctx, labelled, flow),
        Statement::BreakStatement(break_stmt) => {
            let label = break_stmt.label.as_ref().map(|label| label.name.as_str());
            let (target, depth) = break_target(&flow.loops, label)?;
            emit_unwind(
                builder,
                ctx,
                flow,
                flow.cleanups.len(),
                depth,
                target,
                false,
            )?;
            Ok(StmtOutcome {
                last: None,
                terminated: true,
            })
        }
        Statement::ContinueStatement(continue_stmt) => {
            let label = continue_stmt
                .label
                .as_ref()
                .map(|label| label.name.as_str());
            let (target, depth) = continue_target(&flow.loops, label)?;
            emit_unwind(
                builder,
                ctx,
                flow,
                flow.cleanups.len(),
                depth,
                target,
                false,
            )?;
            Ok(StmtOutcome {
                last: None,
                terminated: true,
            })
        }
        Statement::ReturnStatement(ret) => {
            let value = if let Some(target) = &ret.argument {
                compile_expr(builder, ctx, flow, target)?
            } else {
                value::undefined(builder)
            };
            let exit = builder.create_block();
            let reached = emit_unwind(builder, ctx, flow, flow.cleanups.len(), 0, exit, false)?;
            if reached {
                builder.switch_to_block(exit);
                builder.ins().return_(&[value]);
            }
            Ok(StmtOutcome {
                last: None,
                terminated: true,
            })
        }
        Statement::ThrowStatement(throw) => {
            let value = compile_expr(builder, ctx, flow, &throw.argument)?;
            set_exception(builder, ctx, value);
            emit_pending_exception(builder, ctx, flow)?;
            Ok(StmtOutcome {
                last: None,
                terminated: true,
            })
        }
        Statement::TryStatement(try_stmt) => compile_try(builder, ctx, try_stmt, flow),
        other => Err(Error::Unsupported(format!(
            "statement not supported: {other:?}"
        ))),
    }
}

fn compile_condition<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    expr: &'a Expression<'a>,
) -> Result<Value, Error> {
    match expr {
        Expression::ParenthesizedExpression(paren) => {
            compile_condition(builder, ctx, flow, &paren.expression)
        }
        Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
            let cond = compile_condition(builder, ctx, flow, &unary.argument)?;
            Ok(builder.ins().icmp_imm(IntCC::Equal, cond, 0))
        }
        Expression::BinaryExpression(binary) if binary.operator.is_compare() => {
            let op = binary.operator;
            let lhs = compile_expr(builder, ctx, flow, &binary.left)?;
            let rhs = compile_expr(builder, ctx, flow, &binary.right)?;
            let lhs = number_value(builder, lhs, expr_is_number(ctx, &binary.left));
            let rhs = number_value(builder, rhs, expr_is_number(ctx, &binary.right));
            let cc = match op {
                BinaryOperator::GreaterThan => FloatCC::GreaterThan,
                BinaryOperator::GreaterEqualThan => FloatCC::GreaterThanOrEqual,
                BinaryOperator::LessThan => FloatCC::LessThan,
                BinaryOperator::LessEqualThan => FloatCC::LessThanOrEqual,
                _ => unreachable!(),
            };
            Ok(builder.ins().fcmp(cc, lhs, rhs))
        }
        _ => {
            let value = compile_expr(builder, ctx, flow, expr)?;
            Ok(value::truthy(builder, value))
        }
    }
}

fn compile_if<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    if_stmt: &'a IfStatement<'a>,
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let cond = compile_condition(builder, ctx, flow, &if_stmt.test)?;

    let then_block = builder.create_block();
    let else_block = builder.create_block();
    let join_block = builder.create_block();
    builder.ins().brif(cond, then_block, &[], else_block, &[]);

    builder.switch_to_block(then_block);
    let then_out = compile_statement(builder, ctx, &if_stmt.consequent, flow)?;
    if !then_out.terminated {
        builder.ins().jump(join_block, &[]);
    }

    builder.switch_to_block(else_block);
    let else_out = if let Some(else_node) = &if_stmt.alternate {
        compile_statement(builder, ctx, else_node, flow)?
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

fn compile_while<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    while_stmt: &'a WhileStatement<'a>,
    labels: &[&'a str],
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let header = builder.create_block();
    let body = builder.create_block();
    let exit = builder.create_block();

    builder.ins().jump(header, &[]);
    builder.switch_to_block(header);

    let cond = compile_condition(builder, ctx, flow, &while_stmt.test)?;
    builder.ins().brif(cond, body, &[], exit, &[]);

    builder.switch_to_block(body);
    flow.loops.push(LoopTarget {
        labels: labels.to_vec(),
        break_block: exit,
        continue_block: Some(header),
        breakable: true,
        cleanup_depth: flow.cleanups.len(),
    });
    let body_out = compile_statement(builder, ctx, &while_stmt.body, flow)?;
    flow.loops.pop();
    if !body_out.terminated {
        builder.ins().jump(header, &[]);
    }

    builder.switch_to_block(exit);
    Ok(StmtOutcome::default())
}

fn compile_do_while<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    do_while: &'a DoWhileStatement<'a>,
    labels: &[&'a str],
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let body_block = builder.create_block();
    let condition_block = builder.create_block();
    let exit = builder.create_block();

    builder.ins().jump(body_block, &[]);
    builder.switch_to_block(body_block);
    flow.loops.push(LoopTarget {
        labels: labels.to_vec(),
        break_block: exit,
        continue_block: Some(condition_block),
        breakable: true,
        cleanup_depth: flow.cleanups.len(),
    });
    let body_out = compile_statement(builder, ctx, &do_while.body, flow)?;
    flow.loops.pop();
    if !body_out.terminated {
        builder.ins().jump(condition_block, &[]);
    }

    builder.switch_to_block(condition_block);
    let cond = compile_condition(builder, ctx, flow, &do_while.test)?;
    builder.ins().brif(cond, body_block, &[], exit, &[]);

    builder.switch_to_block(exit);
    Ok(StmtOutcome::default())
}

/// Compiles a label chain. Labels attached to loops become extra names for the
/// loop target; any other labelled statement gets a break-only target.
fn compile_labelled<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    labelled: &'a LabeledStatement<'a>,
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let mut labels = vec![labelled.label.name.as_str()];
    let mut stmt = &labelled.body;
    while let Statement::LabeledStatement(inner) = stmt {
        labels.push(inner.label.name.as_str());
        stmt = &inner.body;
    }

    if matches!(stmt, Statement::FunctionDeclaration(_)) {
        return Err(Error::Unsupported(
            "labelled function declarations are not supported".to_string(),
        ));
    }

    match stmt {
        Statement::WhileStatement(while_stmt) => {
            compile_while(builder, ctx, while_stmt, &labels, flow)
        }
        Statement::DoWhileStatement(do_while) => {
            compile_do_while(builder, ctx, do_while, &labels, flow)
        }
        Statement::ForStatement(for_stmt) => compile_for(builder, ctx, for_stmt, &labels, flow),
        Statement::SwitchStatement(switch_stmt) => {
            compile_switch(builder, ctx, switch_stmt, &labels, flow)
        }
        stmt => {
            let join = builder.create_block();
            flow.loops.push(LoopTarget {
                labels,
                break_block: join,
                continue_block: None,
                breakable: false,
                cleanup_depth: flow.cleanups.len(),
            });
            let out = compile_statement(builder, ctx, stmt, flow)?;
            flow.loops.pop();
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
fn compile_for<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    for_stmt: &'a ForStatement<'a>,
    labels: &[&'a str],
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let parent_env = ctx.env;
    let mut ctx = *ctx;
    let mut fresh: Option<(usize, Value)> = None;
    let has_lexical_init = matches!(
        &for_stmt.init,
        Some(ForStatementInit::VariableDeclaration(decl))
            if decl.kind != VariableDeclarationKind::Var
    );
    if has_lexical_init {
        let scope = node_scope(&ctx, node_key(for_stmt))?;
        let plan = ctx.program.scope(scope);
        let slots = plan.slot_count();
        let has_env = plan.has_env();
        ctx = enter_scope(builder, &ctx, scope);
        if has_env {
            fresh = Some((slots, ctx.env));
        }
    }

    match &for_stmt.init {
        Some(ForStatementInit::VariableDeclaration(decl)) => {
            compile_var_list(builder, &ctx, decl, flow)?;
        }
        Some(init) => {
            let expr = init.as_expression().ok_or_else(|| {
                Error::Unsupported("for-loop initializer is not supported".to_string())
            })?;
            compile_expr(builder, &ctx, flow, expr)?;
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
    match &for_stmt.test {
        Some(condition) => {
            let cond = compile_condition(builder, &header_ctx, flow, condition)?;
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
    flow.loops.push(LoopTarget {
        labels: labels.to_vec(),
        break_block: exit,
        continue_block: Some(step),
        breakable: true,
        cleanup_depth: flow.cleanups.len(),
    });
    let body_out = compile_statement(builder, &body_ctx, &for_stmt.body, flow)?;
    flow.loops.pop();
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
    if let Some(update) = &for_stmt.update {
        compile_expr(builder, &step_ctx, flow, update)?;
    }
    builder.ins().jump(header, &[]);

    builder.switch_to_block(exit);
    Ok(StmtOutcome::default())
}

/// `switch (value) { case c: ... default: ... }` with fall-through.
fn compile_switch<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    switch_stmt: &'a SwitchStatement<'a>,
    labels: &[&'a str],
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let value = compile_expr(builder, ctx, flow, &switch_stmt.discriminant)?;
    let scope = node_scope(ctx, node_key(switch_stmt))?;
    let inner = enter_scope(builder, ctx, scope);

    let value_var = builder.declare_var(types::F64);
    builder.def_var(value_var, value);

    let cases = &switch_stmt.cases;
    let exit = builder.create_block();
    let case_blocks: Vec<Block> = cases.iter().map(|_| builder.create_block()).collect();
    let no_match = cases
        .iter()
        .position(|case| case.test.is_none())
        .map_or(exit, |i| case_blocks[i]);

    let tests: Vec<(usize, &Expression<'a>)> = cases
        .iter()
        .enumerate()
        .filter_map(|(i, case)| case.test.as_ref().map(|cond| (i, cond)))
        .collect();

    if tests.is_empty() {
        builder.ins().jump(no_match, &[]);
    }
    for (n, (i, condition)) in tests.iter().enumerate() {
        let is_last = n + 1 == tests.len();
        let matched_value = compile_expr(builder, &inner, flow, condition)?;
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

    flow.loops.push(LoopTarget {
        labels: labels.to_vec(),
        break_block: exit,
        continue_block: None,
        breakable: true,
        cleanup_depth: flow.cleanups.len(),
    });
    for (i, case) in cases.iter().enumerate() {
        builder.switch_to_block(case_blocks[i]);
        let out = compile_statements(builder, &inner, case.consequent.as_slice(), flow)?;
        if !out.terminated {
            let next = case_blocks.get(i + 1).copied().unwrap_or(exit);
            builder.ins().jump(next, &[]);
        }
    }
    flow.loops.pop();

    builder.switch_to_block(exit);
    Ok(StmtOutcome::default())
}

fn compile_try<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    try_stmt: &'a TryStatement<'a>,
    flow: &mut Flow<'a>,
) -> Result<StmtOutcome, Error> {
    let outer_exception = flow.exception;
    let base_cleanup = flow.cleanups.len();
    let base_loop = flow.loops.len();
    let has_finalizer = try_stmt.finalizer.is_some();
    if let Some(finalizer) = &try_stmt.finalizer {
        flow.cleanups.push(Cleanup {
            finalizer,
            loop_depth: base_loop,
            scope: ctx.scope,
            env: ctx.env,
            exception: outer_exception,
        });
    }

    let catch_entry = try_stmt.handler.as_ref().map(|_| builder.create_block());
    flow.exception = match catch_entry {
        Some(block) => Some(ExceptionTarget {
            block,
            cleanup_depth: base_cleanup + usize::from(has_finalizer),
        }),
        None => outer_exception,
    };

    let after = builder.create_block();
    let try_value = builder.declare_var(types::F64);
    let mut reaches_after = false;

    let try_out = compile_block(builder, ctx, &try_stmt.block, flow)?;
    if !try_out.terminated {
        let value = match try_out.last {
            Some(value) => value,
            None => value::undefined(builder),
        };
        builder.def_var(try_value, value);
        reaches_after |= emit_unwind(
            builder,
            ctx,
            flow,
            flow.cleanups.len(),
            base_cleanup,
            after,
            false,
        )?;
    }

    if let Some(handler) = &try_stmt.handler {
        let entry = catch_entry.ok_or_else(|| Error::Codegen("missing catch block".to_string()))?;
        builder.switch_to_block(entry);
        let catch_scope = node_scope(ctx, node_key(&**handler))?;
        let catch_ctx = enter_scope(builder, ctx, catch_scope);
        let exception = take_exception(builder, ctx);
        if let Some(param) = &handler.param {
            let BindingPattern::BindingIdentifier(id) = &param.pattern else {
                return Err(Error::Unsupported(
                    "destructuring catch parameters are not supported".to_string(),
                ));
            };
            store_var(builder, &catch_ctx, id.name.as_str(), exception)?;
        }

        flow.exception = outer_exception;
        let catch_out =
            compile_statements(builder, &catch_ctx, handler.body.body.as_slice(), flow)?;
        if !catch_out.terminated {
            let value = match catch_out.last {
                Some(value) => value,
                None => value::undefined(builder),
            };
            builder.def_var(try_value, value);
            reaches_after |= emit_unwind(
                builder,
                ctx,
                flow,
                flow.cleanups.len(),
                base_cleanup,
                after,
                false,
            )?;
        }
    }

    if has_finalizer {
        flow.cleanups.pop();
    }
    flow.exception = outer_exception;

    if reaches_after {
        builder.switch_to_block(after);
        Ok(StmtOutcome {
            last: Some(builder.use_var(try_value)),
            terminated: false,
        })
    } else {
        Ok(StmtOutcome {
            last: None,
            terminated: true,
        })
    }
}

fn compile_expr<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    expr: &'a Expression<'a>,
) -> Result<Value, Error> {
    match expr {
        Expression::NumericLiteral(lit) => Ok(builder.ins().f64const(lit.value)),
        Expression::BooleanLiteral(lit) => Ok(value::boolean(builder, lit.value)),
        Expression::NullLiteral(_) => Ok(value::null(builder)),
        Expression::StringLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::TemplateLiteral(_) => Err(Error::Unsupported(
            "string and bigint literals are not supported".to_string(),
        )),
        Expression::Identifier(id) => {
            let name = id.name.as_str();
            if is_free_global(ctx, name) {
                if ctx.builtins.is_undefined(name) {
                    return Ok(value::undefined(builder));
                }
                if let Some(constant) = ctx.builtins.global_constant(name) {
                    return Ok(builder.ins().f64const(constant));
                }
            }
            load_var(builder, ctx, name)
        }
        Expression::ParenthesizedExpression(paren) => {
            compile_expr(builder, ctx, flow, &paren.expression)
        }
        Expression::BinaryExpression(binary) => compile_binary(builder, ctx, flow, binary),
        Expression::LogicalExpression(logical) => {
            let lhs = compile_expr(builder, ctx, flow, &logical.left)?;
            compile_logical(builder, ctx, flow, lhs, &logical.right, logical.operator)
        }
        Expression::ConditionalExpression(conditional) => {
            compile_conditional(builder, ctx, flow, conditional)
        }
        Expression::UnaryExpression(unary) => compile_unary(builder, ctx, flow, unary),
        Expression::AssignmentExpression(assign) => compile_assign(builder, ctx, flow, assign),
        Expression::UpdateExpression(update) => compile_update(builder, ctx, update),
        Expression::SequenceExpression(sequence) => {
            let mut result = value::undefined(builder);
            for expr in &sequence.expressions {
                result = compile_expr(builder, ctx, flow, expr)?;
            }
            Ok(result)
        }
        Expression::CallExpression(call) => compile_call(builder, ctx, flow, call),
        Expression::FunctionExpression(func) => compile_closure(builder, ctx, node_key(&**func)),
        Expression::ArrowFunctionExpression(arrow) => {
            compile_closure(builder, ctx, node_key(&**arrow))
        }
        Expression::ChainExpression(chain) => compile_chain(builder, ctx, flow, chain),
        Expression::StaticMemberExpression(access) => compile_property_access(builder, ctx, access),
        other => Err(Error::Unsupported(format!(
            "expression not supported: {other:?}"
        ))),
    }
}

fn compile_binary<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    binary: &'a BinaryExpression<'a>,
) -> Result<Value, Error> {
    let lhs = compile_expr(builder, ctx, flow, &binary.left)?;
    let rhs = compile_expr(builder, ctx, flow, &binary.right)?;
    let op = binary.operator;
    if op.is_arithmetic() {
        compile_arithmetic(
            builder,
            ctx,
            op,
            lhs,
            rhs,
            expr_is_number(ctx, &binary.left),
            expr_is_number(ctx, &binary.right),
        )
    } else if op.is_bitwise() {
        Ok(compile_bitwise(builder, ctx, op, lhs, rhs))
    } else if op.is_equality() || op.is_compare() {
        compile_relational(
            builder,
            op,
            lhs,
            rhs,
            expr_is_number(ctx, &binary.left),
            expr_is_number(ctx, &binary.right),
        )
    } else {
        Err(Error::Unsupported(format!(
            "binary operator not supported: {op:?}"
        )))
    }
}

/// Compiles `cond ? if_true : if_false`, evaluating exactly one branch.
fn compile_conditional<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    conditional: &'a ConditionalExpression<'a>,
) -> Result<Value, Error> {
    let cond = compile_condition(builder, ctx, flow, &conditional.test)?;

    let result_var = builder.declare_var(types::F64);
    let true_block = builder.create_block();
    let false_block = builder.create_block();
    let join_block = builder.create_block();
    builder.ins().brif(cond, true_block, &[], false_block, &[]);

    builder.switch_to_block(true_block);
    let if_true = compile_expr(builder, ctx, flow, &conditional.consequent)?;
    builder.def_var(result_var, if_true);
    builder.ins().jump(join_block, &[]);

    builder.switch_to_block(false_block);
    let if_false = compile_expr(builder, ctx, flow, &conditional.alternate)?;
    builder.def_var(result_var, if_false);
    builder.ins().jump(join_block, &[]);

    builder.switch_to_block(join_block);
    Ok(builder.use_var(result_var))
}

/// The short-circuiting operators: `&&`, `||` and `??`.
fn compile_logical<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    lhs: Value,
    rhs_expr: &'a Expression<'a>,
    op: LogicalOperator,
) -> Result<Value, Error> {
    let (cond, rhs_when_true) = match op {
        LogicalOperator::And => (value::truthy(builder, lhs), true),
        LogicalOperator::Or => (value::truthy(builder, lhs), false),
        LogicalOperator::Coalesce => (value::is_nullish(builder, lhs), true),
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
    let rhs = compile_expr(builder, ctx, flow, rhs_expr)?;
    builder.def_var(result_var, rhs);
    builder.ins().jump(join_block, &[]);

    builder.switch_to_block(join_block);
    Ok(builder.use_var(result_var))
}

fn compile_arithmetic(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    op: BinaryOperator,
    lhs: Value,
    rhs: Value,
    lhs_numeric: bool,
    rhs_numeric: bool,
) -> Result<Value, Error> {
    let lhs = number_value(builder, lhs, lhs_numeric);
    let rhs = number_value(builder, rhs, rhs_numeric);
    let value = match op {
        BinaryOperator::Addition => builder.ins().fadd(lhs, rhs),
        BinaryOperator::Subtraction => builder.ins().fsub(lhs, rhs),
        BinaryOperator::Multiplication => builder.ins().fmul(lhs, rhs),
        BinaryOperator::Division => builder.ins().fdiv(lhs, rhs),
        BinaryOperator::Remainder => call_fmod(builder, ctx.fmod_ref, lhs, rhs),
        BinaryOperator::Exponential => call_pow(builder, ctx.pow_ref, lhs, rhs),
        other => {
            return Err(Error::Unsupported(format!(
                "arithmetic operator not supported: {other:?}"
            )));
        }
    };
    Ok(value)
}

/// Bitwise operators work on `ToInt32`/`ToUint32` views of the operands.
fn compile_bitwise(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    op: BinaryOperator,
    lhs: Value,
    rhs: Value,
) -> Value {
    let lhs = value::to_int32(builder, ctx.fmod_ref, lhs);
    let rhs = value::to_int32(builder, ctx.fmod_ref, rhs);

    let result = match op {
        BinaryOperator::BitwiseAnd => builder.ins().band(lhs, rhs),
        BinaryOperator::BitwiseOR => builder.ins().bor(lhs, rhs),
        BinaryOperator::BitwiseXOR => builder.ins().bxor(lhs, rhs),
        BinaryOperator::ShiftLeft => {
            let shift = builder.ins().band_imm(rhs, 31);
            builder.ins().ishl(lhs, shift)
        }
        BinaryOperator::ShiftRight => {
            let shift = builder.ins().band_imm(rhs, 31);
            builder.ins().sshr(lhs, shift)
        }
        BinaryOperator::ShiftRightZeroFill => {
            let shift = builder.ins().band_imm(rhs, 31);
            let shifted = builder.ins().ushr(lhs, shift);
            return value::from_uint32(builder, shifted);
        }
        _ => unreachable!(),
    };
    value::from_int32(builder, result)
}

fn compile_relational(
    builder: &mut FunctionBuilder,
    op: BinaryOperator,
    lhs: Value,
    rhs: Value,
    lhs_numeric: bool,
    rhs_numeric: bool,
) -> Result<Value, Error> {
    match op {
        BinaryOperator::Equality => Ok(value::loose_equal(builder, lhs, rhs)),
        BinaryOperator::StrictEquality => Ok(value::strict_equal(builder, lhs, rhs)),
        BinaryOperator::Inequality | BinaryOperator::StrictInequality => {
            let eq = if op == BinaryOperator::Inequality {
                value::loose_equal(builder, lhs, rhs)
            } else {
                value::strict_equal(builder, lhs, rhs)
            };
            let eq = value::truthy(builder, eq);
            let not = builder.ins().icmp_imm(IntCC::Equal, eq, 0);
            Ok(value::from_bool(builder, not))
        }
        BinaryOperator::GreaterThan
        | BinaryOperator::GreaterEqualThan
        | BinaryOperator::LessThan
        | BinaryOperator::LessEqualThan => {
            let lhs = number_value(builder, lhs, lhs_numeric);
            let rhs = number_value(builder, rhs, rhs_numeric);
            let cc = match op {
                BinaryOperator::GreaterThan => FloatCC::GreaterThan,
                BinaryOperator::GreaterEqualThan => FloatCC::GreaterThanOrEqual,
                BinaryOperator::LessThan => FloatCC::LessThan,
                BinaryOperator::LessEqualThan => FloatCC::LessThanOrEqual,
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

fn compile_unary<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    unary: &'a UnaryExpression<'a>,
) -> Result<Value, Error> {
    if unary.operator == UnaryOperator::Delete {
        return compile_delete(builder, ctx, flow, &unary.argument);
    }
    let target = compile_expr(builder, ctx, flow, &unary.argument)?;
    match unary.operator {
        UnaryOperator::UnaryNegation => {
            let target = number_value(builder, target, expr_is_number(ctx, &unary.argument));
            Ok(builder.ins().fneg(target))
        }
        UnaryOperator::UnaryPlus => Ok(number_value(
            builder,
            target,
            expr_is_number(ctx, &unary.argument),
        )),
        UnaryOperator::LogicalNot => {
            let cond = value::truthy(builder, target);
            let not = builder.ins().icmp_imm(IntCC::Equal, cond, 0);
            Ok(value::from_bool(builder, not))
        }
        UnaryOperator::BitwiseNot => {
            let int = value::to_int32(builder, ctx.fmod_ref, target);
            let inverted = builder.ins().bxor_imm(int, -1);
            Ok(value::from_int32(builder, inverted))
        }
        UnaryOperator::Void => Ok(value::undefined(builder)),
        other => Err(Error::Unsupported(format!(
            "unary operator not supported: {other:?}"
        ))),
    }
}

fn compile_delete<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    expr: &'a Expression<'a>,
) -> Result<Value, Error> {
    match expr {
        Expression::ParenthesizedExpression(paren) => {
            compile_delete(builder, ctx, flow, &paren.expression)
        }
        Expression::Identifier(id) => {
            let name = id.name.as_str();
            let deletable = resolve_binding(ctx.program, ctx.scope, name).is_none()
                && !ctx.builtins.is_global(name);
            Ok(value::boolean(builder, deletable))
        }
        Expression::StaticMemberExpression(_)
        | Expression::ComputedMemberExpression(_)
        | Expression::PrivateFieldExpression(_) => Err(Error::Unsupported(
            "delete of a property requires objects".to_string(),
        )),
        other => {
            compile_expr(builder, ctx, flow, other)?;
            Ok(value::boolean(builder, true))
        }
    }
}

fn compile_assign<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    assign: &'a AssignmentExpression<'a>,
) -> Result<Value, Error> {
    let name = match &assign.left {
        AssignmentTarget::AssignmentTargetIdentifier(id) => id.name.as_str(),
        _ => {
            return Err(Error::Unsupported(
                "only identifier assignment is supported".to_string(),
            ));
        }
    };
    if is_const(ctx.program, ctx.scope, name) {
        return Err(Error::Scope(
            "assignment to a constant variable".to_string(),
        ));
    }
    let place = place(builder, ctx, name)?;
    let tdz = needs_tdz_check(ctx.program, ctx.scope, name);
    let op = assign.operator;

    // The logical assignment operators only evaluate and store the right-hand
    // side when the current value calls for it.
    let short_circuit = match op {
        AssignmentOperator::LogicalAnd => Some((true, true)),
        AssignmentOperator::LogicalOr => Some((false, true)),
        AssignmentOperator::LogicalNullish => Some((true, false)),
        _ => None,
    };
    if let Some((rhs_when_truthy, use_truthy)) = short_circuit {
        let old = load_place(builder, place);
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
        let rhs = compile_expr(builder, ctx, flow, &assign.right)?;
        store_place(builder, place, rhs);
        builder.def_var(result_var, rhs);
        builder.ins().jump(join_block, &[]);

        builder.switch_to_block(join_block);
        return Ok(builder.use_var(result_var));
    }

    if op == AssignmentOperator::Assign {
        let rhs = compile_expr(builder, ctx, flow, &assign.right)?;
        if tdz {
            let current = load_place(builder, place);
            check_tdz(builder, current);
        }
        store_place(builder, place, rhs);
        return Ok(rhs);
    }

    // Compound assignment reads the old value before evaluating the right-hand
    // side, matching the evaluation order of the specification.
    let old = load_place(builder, place);
    if tdz {
        check_tdz(builder, old);
    }
    let rhs = compile_expr(builder, ctx, flow, &assign.right)?;
    let binary = op
        .to_binary_operator()
        .ok_or_else(|| Error::Unsupported(format!("assignment operator not supported: {op:?}")))?;
    let value = if binary.is_arithmetic() {
        compile_arithmetic(
            builder,
            ctx,
            binary,
            old,
            rhs,
            binding_is_number(ctx, name),
            expr_is_number(ctx, &assign.right),
        )?
    } else {
        compile_bitwise(builder, ctx, binary, old, rhs)
    };
    store_place(builder, place, value);
    Ok(value)
}

fn compile_update<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    update: &'a UpdateExpression<'a>,
) -> Result<Value, Error> {
    let name = match &update.argument {
        SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => id.name.as_str(),
        _ => {
            return Err(Error::Unsupported(
                "only identifier updates are supported".to_string(),
            ));
        }
    };
    if is_const(ctx.program, ctx.scope, name) {
        return Err(Error::Scope(
            "assignment to a constant variable".to_string(),
        ));
    }
    let old = load_var(builder, ctx, name)?;
    let old = number_value(builder, old, binding_is_number(ctx, name));
    let one = builder.ins().f64const(1.0);
    let new_value = match update.operator {
        UpdateOperator::Increment => builder.ins().fadd(old, one),
        UpdateOperator::Decrement => builder.ins().fsub(old, one),
    };
    let result = if update.prefix { None } else { Some(old) };
    store_var(builder, ctx, name, new_value)?;
    Ok(result.unwrap_or(new_value))
}

/// True when `name` is not bound in scope, i.e. it names a global builtin.
fn is_free_global(ctx: &CodegenCtx<'_>, name: &str) -> bool {
    resolve(ctx.program, ctx.scope, name).is_none()
}

/// Recognises `Math.member`, `Number.member` and `Object.member`.
fn builtin_member(
    ctx: &CodegenCtx<'_>,
    access: &StaticMemberExpression<'_>,
) -> Option<(builtins::Namespace, &'static str)> {
    let Expression::Identifier(id) = &access.object else {
        return None;
    };
    let name = id.name.as_str();
    if !is_free_global(ctx, name) {
        return None;
    }
    let namespace = ctx.builtins.namespace(name)?;
    let name = ctx.builtins.property(access.property.name.as_str())?;
    ctx.builtins
        .has(namespace, name)
        .then_some((namespace, name))
}

enum BuiltinCallee {
    Global(&'static str),
    Member(builtins::Namespace, &'static str),
}

/// Compiles `Math.foo(...)`, `Number.foo(...)`, `Object.foo(...)`, `Date.now()`
/// and the global conversion/predicate functions.
fn compile_builtin_call<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    callee: BuiltinCallee,
    args: &'a [Argument<'a>],
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
    let mut known_numbers = Vec::with_capacity(args.len());
    for arg in args {
        let Some(expr) = arg.as_expression() else {
            return Err(Error::Unsupported(
                "spread arguments are not supported".to_string(),
            ));
        };
        values.push(compile_expr(builder, ctx, flow, expr)?);
        known_numbers.push(expr_is_number(ctx, expr));
    }
    builtins::call(
        builder,
        ctx.builtin_refs,
        ctx.fmod_ref,
        namespace,
        name,
        &values,
        &known_numbers,
    )
}

/// Recognises a call to a builtin member or a free global builtin.
fn builtin_callee(ctx: &CodegenCtx<'_>, callee: &Expression<'_>) -> Option<BuiltinCallee> {
    match callee {
        Expression::StaticMemberExpression(access) => builtin_member(ctx, access)
            .map(|(namespace, name)| BuiltinCallee::Member(namespace, name)),
        Expression::Identifier(id) if is_free_global(ctx, id.name.as_str()) => ctx
            .builtins
            .global_function(id.name.as_str())
            .map(BuiltinCallee::Global),
        _ => None,
    }
}

fn compile_property_access(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    access: &StaticMemberExpression<'_>,
) -> Result<Value, Error> {
    let constant = builtin_member(ctx, access)
        .and_then(|(namespace, name)| ctx.builtins.constant(namespace, name));
    match constant {
        Some(value) => Ok(builder.ins().f64const(value)),
        None => Err(Error::Unsupported(
            "only builtin constants can be read as properties".to_string(),
        )),
    }
}

fn compile_call<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    call: &'a CallExpression<'a>,
) -> Result<Value, Error> {
    if let Some(builtin) = builtin_callee(ctx, &call.callee) {
        return compile_builtin_call(builder, ctx, flow, builtin, call.arguments.as_slice());
    }

    let callee = compile_expr(builder, ctx, flow, &call.callee)?;
    compile_call_value(builder, ctx, flow, callee, &call.arguments)
}

fn compile_call_value<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    callee: Value,
    args: &'a [Argument<'a>],
) -> Result<Value, Error> {
    if args.len() > MAX_ARGS {
        return Err(Error::Unsupported(format!(
            "calls with more than {MAX_ARGS} arguments are not supported"
        )));
    }

    let callee_ptr = value::payload(builder, callee);
    let code = builder
        .ins()
        .load(types::I64, MemFlags::new(), callee_ptr, 0i32);
    let captured_env = builder
        .ins()
        .load(types::I64, MemFlags::new(), callee_ptr, 8i32);

    let args_ptr = call_calloc(builder, ctx.calloc_ref, 8 + 8 * MAX_ARGS as i64);
    let len = builder.ins().iconst(types::I64, args.len() as i64);
    builder.ins().store(MemFlags::new(), len, args_ptr, 0i32);

    for (i, arg) in args.iter().enumerate() {
        let Some(expr) = arg.as_expression() else {
            return Err(Error::Unsupported(
                "spread arguments are not supported".to_string(),
            ));
        };
        let value = compile_expr(builder, ctx, flow, expr)?;
        builder
            .ins()
            .store(MemFlags::new(), value, args_ptr, (8 + 8 * i) as i32);
    }

    let call = builder
        .ins()
        .call_indirect(ctx.js_sig_ref, code, &[captured_env, args_ptr, callee]);
    let result = builder.inst_results(call)[0];
    emit_exception_check(builder, ctx, flow)?;
    Ok(result)
}

fn compile_chain<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    chain: &'a ChainExpression<'a>,
) -> Result<Value, Error> {
    let short = builder.create_block();
    let join = builder.create_block();
    let result = builder.declare_var(types::F64);
    let value = compile_chain_element(builder, ctx, flow, &chain.expression, short, result)?;
    builder.def_var(result, value);
    builder.ins().jump(join, &[]);

    builder.switch_to_block(short);
    let undefined = value::undefined(builder);
    builder.def_var(result, undefined);
    builder.ins().jump(join, &[]);

    builder.switch_to_block(join);
    Ok(builder.use_var(result))
}

fn compile_chain_element<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    element: &'a ChainElement<'a>,
    short: Block,
    result: Variable,
) -> Result<Value, Error> {
    match element {
        ChainElement::CallExpression(call) => {
            compile_chain_call(builder, ctx, flow, call, short, result)
        }
        ChainElement::StaticMemberExpression(access) => {
            compile_chain_static_member(builder, ctx, flow, access, short, result)
        }
        ChainElement::ComputedMemberExpression(member) => {
            compile_chain_computed_member(builder, ctx, flow, member, short, result)
        }
        other => Err(Error::Unsupported(format!(
            "optional chain element not supported: {other:?}"
        ))),
    }
}

fn compile_chain_expr<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    expr: &'a Expression<'a>,
    short: Block,
    result: Variable,
) -> Result<Value, Error> {
    match expr {
        Expression::CallExpression(call) => {
            compile_chain_call(builder, ctx, flow, call, short, result)
        }
        Expression::StaticMemberExpression(access) => {
            compile_chain_static_member(builder, ctx, flow, access, short, result)
        }
        Expression::ChainExpression(chain) => compile_chain(builder, ctx, flow, chain),
        Expression::ComputedMemberExpression(member) => {
            compile_chain_computed_member(builder, ctx, flow, member, short, result)
        }
        Expression::PrivateFieldExpression(_) => Err(Error::Unsupported(
            "private member access requires objects".to_string(),
        )),
        other => compile_expr(builder, ctx, flow, other),
    }
}

fn compile_chain_static_member<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    access: &'a StaticMemberExpression<'a>,
    short: Block,
    result: Variable,
) -> Result<Value, Error> {
    if builtin_member(ctx, access).is_some() {
        return compile_property_access(builder, ctx, access);
    }

    let object = compile_chain_expr(builder, ctx, flow, &access.object, short, result)?;
    if !access.optional {
        return Ok(value::undefined(builder));
    }

    let ok = builder.create_block();
    let nullish = value::is_nullish(builder, object);
    builder.ins().brif(nullish, short, &[], ok, &[]);
    builder.switch_to_block(ok);
    Ok(value::undefined(builder))
}

fn compile_chain_computed_member<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    member: &'a oxc_ast::ast::ComputedMemberExpression<'a>,
    short: Block,
    result: Variable,
) -> Result<Value, Error> {
    let object = compile_chain_expr(builder, ctx, flow, &member.object, short, result)?;
    if member.optional {
        let ok = builder.create_block();
        let nullish = value::is_nullish(builder, object);
        builder.ins().brif(nullish, short, &[], ok, &[]);
        builder.switch_to_block(ok);
    }
    let _ = compile_expr(builder, ctx, flow, &member.expression)?;
    Ok(value::undefined(builder))
}

fn compile_chain_call<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    flow: &mut Flow<'a>,
    call: &'a CallExpression<'a>,
    short: Block,
    result: Variable,
) -> Result<Value, Error> {
    if let Some(builtin) = builtin_callee(ctx, &call.callee) {
        return compile_builtin_call(builder, ctx, flow, builtin, call.arguments.as_slice());
    }

    let callee = compile_chain_expr(builder, ctx, flow, &call.callee, short, result)?;
    if call.optional {
        let ok = builder.create_block();
        let nullish = value::is_nullish(builder, callee);
        builder.ins().brif(nullish, short, &[], ok, &[]);
        builder.switch_to_block(ok);
    }
    compile_call_value(builder, ctx, flow, callee, &call.arguments)
}

/// Materialises a function or arrow expression as a closure over the current
/// environment.
fn compile_closure(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    key: usize,
) -> Result<Value, Error> {
    let child = *ctx
        .program
        .function(ctx.plan_id)
        .nested
        .get(&key)
        .ok_or_else(|| Error::Codegen("missing anonymous function plan".to_string()))?;
    let func_ref = *ctx
        .child_refs
        .get(&child)
        .ok_or_else(|| Error::Codegen("missing anonymous function reference".to_string()))?;
    let code = builder.ins().func_addr(types::I64, func_ref);
    Ok(new_closure(builder, ctx.calloc_ref, code, ctx.env))
}
