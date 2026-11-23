use std::collections::HashSet;

use cranelift_codegen::ir::Value;
use cranelift_frontend::FunctionBuilder;
use oxc_ast::ast::{
    AssignmentOperator, AssignmentTarget, BinaryOperator, BindingPattern, CallExpression,
    ChainElement, Expression, ForStatementInit, Statement, StaticMemberExpression, UnaryOperator,
    VariableDeclaration, VariableDeclarationKind,
};

use super::CodegenCtx;
use crate::{
    builtins::{self, Builtins},
    plan::{FunctionBodyPlan, Program, ScopeId, node_key, resolve_binding},
    value,
};

pub(super) fn numeric_builtin_call(
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

pub(super) fn numeric_property(
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
pub(super) fn known_number_expr(
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
            if binary.operator == BinaryOperator::Addition {
                known_number_expr(program, builtins, numeric, scope, &binary.left)
                    && known_number_expr(program, builtins, numeric, scope, &binary.right)
            } else {
                binary.operator.is_arithmetic() || binary.operator.is_bitwise()
            }
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

pub(super) fn expr_is_number<'a>(ctx: &CodegenCtx<'a>, expr: &Expression<'a>) -> bool {
    known_number_expr(ctx.program, ctx.builtins, ctx.numeric, ctx.scope, expr)
}

pub(super) fn binding_is_number(ctx: &CodegenCtx<'_>, name: &str) -> bool {
    resolve_binding(ctx.program, ctx.scope, name).is_some_and(|key| ctx.numeric.contains(&key))
}

pub(super) fn number_value(
    builder: &mut FunctionBuilder,
    value: Value,
    known: bool,
    strings: Option<&value::StringOps>,
) -> Value {
    if known {
        value
    } else {
        value::to_number(builder, value, strings)
    }
}

/// The value a write stores is numeric for plain assignments when the RHS is
/// numeric, and always for arithmetic/bitwise compound assignments.
pub(super) fn assignment_write_is_numeric(
    program: &Program<'_>,
    builtins: &Builtins,
    numeric: &HashSet<(ScopeId, usize)>,
    scope: ScopeId,
    op: AssignmentOperator,
    rhs: &Expression<'_>,
) -> bool {
    match op {
        AssignmentOperator::Assign
        | AssignmentOperator::Addition
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
pub(super) fn analyze_numeric_bindings<'a>(
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
        let direct = program.direct_calls.safe.contains(&plan.id);
        for param in &plan.params {
            if let Some(key) = resolve_binding(program, plan.root_scope, param.name)
                && !direct
            {
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
            match plan.body {
                FunctionBodyPlan::Statements(statements) => {
                    walker.statements(plan.id, plan.root_scope, statements);
                }
                FunctionBodyPlan::Expression(expression) => {
                    walker.expr(plan.root_scope, expression);
                }
            }
        }

        // Parameters of directly called functions stay numeric only while
        // every call site passes a known number in that position.
        for &plan_id in &program.direct_calls.safe {
            let plan = program.function(plan_id);
            let Some(calls) = program.direct_calls.calls.get(&plan_id) else {
                continue;
            };
            for (index, param) in plan.params.iter().enumerate() {
                let Some(key) = resolve_binding(program, plan.root_scope, param.name) else {
                    continue;
                };
                if !numeric.contains(&key) || removed.contains(&key) {
                    continue;
                }
                let all_numeric = calls.iter().all(|call| {
                    call.args.get(index).is_some_and(|arg| {
                        arg.as_expression().is_some_and(|expr| {
                            known_number_expr(program, builtins, &numeric, call.scope, expr)
                        })
                    })
                });
                if !all_numeric {
                    removed.insert(key);
                }
            }
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
