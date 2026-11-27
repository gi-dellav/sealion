//! Direct-call analysis for function declarations.
//!
//! A hoisted `function` declaration can be called directly only when its
//! binding is never read as a value, never assigned, and every call passes
//! explicit arguments. The same information feeds numeric parameter inference:
//! if all call sites pass numbers, the callee may treat its parameters as
//! untagged `f64`s.

use std::collections::{HashMap, HashSet};

use oxc_ast::ast::{
    Argument, AssignmentTarget, BindingPattern, ChainElement, Expression, ForStatementInit,
    ObjectPropertyKind, SimpleAssignmentTarget, Statement, VariableDeclaration,
};

use super::{FunctionBodyPlan, Program, ScopeId, node_key, resolve_binding};

/// One direct call site and the scope its arguments are evaluated in.
pub struct DirectCall<'a> {
    pub scope: ScopeId,
    pub args: &'a [Argument<'a>],
}

#[derive(Default)]
pub struct DirectCallInfo<'a> {
    /// Function-declaration bindings, keyed by the binding's `(scope, slot)`.
    pub bindings: HashMap<(ScopeId, usize), usize>,
    /// Direct call sites per callee plan.
    pub calls: HashMap<usize, Vec<DirectCall<'a>>>,
    /// Plans whose binding is only ever the callee of a direct call.
    pub safe: HashSet<usize>,
}

pub fn analyze<'a>(program: &Program<'a>) -> (DirectCallInfo<'a>, HashSet<(ScopeId, usize)>) {
    let mut bindings = HashMap::new();
    let mut duplicates = HashSet::new();
    for parent in &program.functions {
        for &child in &parent.children {
            let Some(name) = program.function(child).binding_name else {
                continue;
            };
            let Some(key) = resolve_binding(program, parent.root_scope, name) else {
                continue;
            };
            if bindings.insert(key, child).is_some() {
                duplicates.insert(key);
            }
        }
    }
    for key in duplicates {
        bindings.remove(&key);
    }

    let mut walker = Walker {
        program,
        bindings: &bindings,
        calls: HashMap::new(),
        escaped: HashSet::new(),
        initialized: HashSet::new(),
        unsafe_tdz: HashSet::new(),
        unstable_scopes: HashSet::new(),
        current_plan: 0,
        conservative: false,
    };
    for (id, plan) in program.functions.iter().enumerate() {
        walker.current_plan = id;
        walker.initialized.clear();
        for param in &plan.params {
            if let Some(init) = param.init {
                walker.expression(plan.root_scope, init);
            }
            if let Some(key) = resolve_binding(program, plan.root_scope, param.name) {
                walker.initialized.insert(key);
            }
        }
        let body = plan.body;
        let scope = plan.root_scope;
        match body {
            FunctionBodyPlan::Statements(statements) => walker.statements(id, scope, statements),
            FunctionBodyPlan::Expression(expression) => walker.expression(scope, expression),
        }
    }

    let calls = std::mem::take(&mut walker.calls);
    let escaped = std::mem::take(&mut walker.escaped);
    let unsafe_tdz = std::mem::take(&mut walker.unsafe_tdz);
    let unstable_scopes = std::mem::take(&mut walker.unstable_scopes);
    let conservative = walker.conservative;
    drop(walker);

    let mut safe = HashSet::new();
    let mut no_tdz = HashSet::new();
    if !conservative {
        for (&key, &plan) in &bindings {
            let _ = key;
            if escaped.contains(&plan) {
                continue;
            }
            if calls.get(&plan).is_none_or(|calls| calls.is_empty()) {
                continue;
            }
            safe.insert(plan);
        }
        for (index, scope) in program.scopes.iter().enumerate() {
            let scope_id = ScopeId(index);
            if scope.captured || unstable_scopes.contains(&scope_id) {
                continue;
            }
            for &slot in &scope.tdz {
                let key = (scope_id, slot);
                if !unsafe_tdz.contains(&key) {
                    no_tdz.insert(key);
                }
            }
        }
    }

    (
        DirectCallInfo {
            bindings,
            calls,
            safe,
        },
        no_tdz,
    )
}

struct Walker<'a, 'p> {
    program: &'p Program<'a>,
    bindings: &'p HashMap<(ScopeId, usize), usize>,
    calls: HashMap<usize, Vec<DirectCall<'a>>>,
    escaped: HashSet<usize>,
    /// Bindings definitely initialized at the current point in this plan.
    initialized: HashSet<(ScopeId, usize)>,
    /// Bindings read or written before their initializer.
    unsafe_tdz: HashSet<(ScopeId, usize)>,
    /// Scopes with non-sequential entry (currently `switch` bodies).
    unstable_scopes: HashSet<ScopeId>,
    current_plan: usize,
    conservative: bool,
}

impl<'a> Walker<'a, '_> {
    fn target(&self, scope: ScopeId, name: &str) -> Option<usize> {
        resolve_binding(self.program, scope, name).and_then(|key| self.bindings.get(&key).copied())
    }

    fn escape(&mut self, scope: ScopeId, name: &str) {
        if let Some(plan) = self.target(scope, name) {
            self.escaped.insert(plan);
        }
    }

    fn binding(&self, scope: ScopeId, name: &str) -> Option<(ScopeId, usize)> {
        resolve_binding(self.program, scope, name)
    }

    fn check_tdz(&mut self, scope: ScopeId, name: &str) {
        let Some(key) = self.binding(scope, name) else {
            return;
        };
        let home = self.program.scope(key.0);
        if home.owner != self.current_plan {
            return;
        }
        if home.tdz.contains(&key.1) && !self.initialized.contains(&key) {
            self.unsafe_tdz.insert(key);
        }
    }

    fn read(&mut self, scope: ScopeId, name: &str) {
        self.check_tdz(scope, name);
    }

    fn write(&mut self, scope: ScopeId, name: &str) {
        self.check_tdz(scope, name);
    }

    fn initialize(&mut self, scope: ScopeId, name: &str) {
        if let Some(key) = self.binding(scope, name) {
            self.initialized.insert(key);
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

    fn statements(&mut self, plan_id: usize, scope: ScopeId, list: &'a [Statement<'a>]) {
        for stmt in list {
            self.statement(plan_id, scope, stmt);
        }
    }

    fn variable_list(&mut self, scope: ScopeId, decl: &'a VariableDeclaration<'a>) {
        for var in &decl.declarations {
            if let Some(init) = &var.init {
                self.expression(scope, init);
            }
            match &var.id {
                BindingPattern::BindingIdentifier(id) => {
                    self.escape(scope, id.name.as_str());
                    if var.init.is_some() {
                        self.initialize(scope, id.name.as_str());
                    }
                }
                _ => self.conservative = true,
            }
        }
    }

    fn statement(&mut self, plan_id: usize, scope: ScopeId, stmt: &'a Statement<'a>) {
        match stmt {
            Statement::BlockStatement(block) => {
                let inner = self.node_scope(plan_id, scope, node_key(&**block));
                self.statements(plan_id, inner, block.body.as_slice());
            }
            Statement::VariableDeclaration(decl) => self.variable_list(scope, decl),
            Statement::FunctionDeclaration(_) => {}
            Statement::ExpressionStatement(expr) => self.expression(scope, &expr.expression),
            Statement::IfStatement(if_stmt) => {
                self.expression(scope, &if_stmt.test);
                self.statement(plan_id, scope, &if_stmt.consequent);
                if let Some(else_node) = &if_stmt.alternate {
                    self.statement(plan_id, scope, else_node);
                }
            }
            Statement::WhileStatement(while_stmt) => {
                self.expression(scope, &while_stmt.test);
                self.statement(plan_id, scope, &while_stmt.body);
            }
            Statement::DoWhileStatement(do_while) => {
                self.statement(plan_id, scope, &do_while.body);
                self.expression(scope, &do_while.test);
            }
            Statement::ForStatement(for_stmt) => {
                let mut loop_scope = scope;
                match &for_stmt.init {
                    Some(ForStatementInit::VariableDeclaration(decl))
                        if decl.kind != oxc_ast::ast::VariableDeclarationKind::Var =>
                    {
                        loop_scope = self.node_scope(plan_id, scope, node_key(&**for_stmt));
                        self.variable_list(loop_scope, decl);
                    }
                    Some(ForStatementInit::VariableDeclaration(decl)) => {
                        self.variable_list(loop_scope, decl);
                    }
                    Some(init) => {
                        if let Some(expr) = init.as_expression() {
                            self.expression(loop_scope, expr);
                        }
                    }
                    None => {}
                }
                if let Some(condition) = &for_stmt.test {
                    self.expression(loop_scope, condition);
                }
                if let Some(update) = &for_stmt.update {
                    self.expression(loop_scope, update);
                }
                self.statement(plan_id, loop_scope, &for_stmt.body);
            }
            Statement::SwitchStatement(switch_stmt) => {
                let inner = self.node_scope(plan_id, scope, node_key(&**switch_stmt));
                self.unstable_scopes.insert(inner);
                self.expression(scope, &switch_stmt.discriminant);
                for case in &switch_stmt.cases {
                    if let Some(condition) = &case.test {
                        self.expression(inner, condition);
                    }
                    self.statements(plan_id, inner, case.consequent.as_slice());
                }
            }
            Statement::ReturnStatement(ret) => {
                if let Some(target) = &ret.argument {
                    self.expression(scope, target);
                }
            }
            Statement::LabeledStatement(labelled) => {
                self.statement(plan_id, scope, &labelled.body);
            }
            Statement::ThrowStatement(throw) => self.expression(scope, &throw.argument),
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
            Statement::EmptyStatement(_)
            | Statement::DebuggerStatement(_)
            | Statement::BreakStatement(_)
            | Statement::ContinueStatement(_) => {}
            _ => self.conservative = true,
        }
    }

    fn expression(&mut self, scope: ScopeId, expr: &'a Expression<'a>) {
        match expr {
            Expression::Identifier(id) => {
                self.escape(scope, id.name.as_str());
                self.read(scope, id.name.as_str());
            }
            Expression::ParenthesizedExpression(paren) => self.expression(scope, &paren.expression),
            Expression::CallExpression(call) => {
                if let Expression::Identifier(id) = &call.callee
                    && let Some(plan) = self.target(scope, id.name.as_str())
                {
                    self.calls.entry(plan).or_default().push(DirectCall {
                        scope,
                        args: call.arguments.as_slice(),
                    });
                } else {
                    self.expression(scope, &call.callee);
                }
                for arg in &call.arguments {
                    if let Some(expr) = arg.as_expression() {
                        self.expression(scope, expr);
                    }
                }
            }
            Expression::AssignmentExpression(assign) => {
                match &assign.left {
                    AssignmentTarget::AssignmentTargetIdentifier(id) => {
                        self.escape(scope, id.name.as_str());
                        self.write(scope, id.name.as_str());
                    }
                    AssignmentTarget::StaticMemberExpression(access) => {
                        self.expression(scope, &access.object);
                    }
                    AssignmentTarget::ComputedMemberExpression(member) => {
                        self.expression(scope, &member.object);
                        self.expression(scope, &member.expression);
                    }
                    _ => self.conservative = true,
                }
                self.expression(scope, &assign.right);
            }
            Expression::UpdateExpression(update) => match &update.argument {
                SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                    self.escape(scope, id.name.as_str());
                    self.write(scope, id.name.as_str());
                }
                SimpleAssignmentTarget::StaticMemberExpression(access) => {
                    self.expression(scope, &access.object);
                }
                SimpleAssignmentTarget::ComputedMemberExpression(member) => {
                    self.expression(scope, &member.object);
                    self.expression(scope, &member.expression);
                }
                _ => self.conservative = true,
            },
            Expression::ArrayExpression(array) => {
                for element in &array.elements {
                    if let Some(expression) = element.as_expression() {
                        self.expression(scope, expression);
                    }
                }
            }
            Expression::NewExpression(new) => {
                self.expression(scope, &new.callee);
                for arg in &new.arguments {
                    if let Some(expression) = arg.as_expression() {
                        self.expression(scope, expression);
                    }
                }
            }
            Expression::ObjectExpression(object) => {
                for property in &object.properties {
                    let ObjectPropertyKind::ObjectProperty(property) = property else {
                        self.conservative = true;
                        continue;
                    };
                    if let Some(expression) = property.key.as_expression() {
                        self.expression(scope, expression);
                    }
                    self.expression(scope, &property.value);
                }
            }
            Expression::BinaryExpression(binary) => {
                self.expression(scope, &binary.left);
                self.expression(scope, &binary.right);
            }
            Expression::LogicalExpression(logical) => {
                self.expression(scope, &logical.left);
                self.expression(scope, &logical.right);
            }
            Expression::UnaryExpression(unary) => self.expression(scope, &unary.argument),
            Expression::ConditionalExpression(conditional) => {
                self.expression(scope, &conditional.test);
                self.expression(scope, &conditional.consequent);
                self.expression(scope, &conditional.alternate);
            }
            Expression::SequenceExpression(sequence) => {
                for expr in &sequence.expressions {
                    self.expression(scope, expr);
                }
            }
            Expression::ChainExpression(chain) => self.chain(scope, &chain.expression),
            Expression::StaticMemberExpression(simple) => self.expression(scope, &simple.object),
            Expression::ComputedMemberExpression(computed) => {
                self.expression(scope, &computed.object);
                self.expression(scope, &computed.expression);
            }
            Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => {}
            Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BigIntLiteral(_)
            | Expression::RegExpLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::ThisExpression(_) => {}
            Expression::TemplateLiteral(template) => {
                for expr in &template.expressions {
                    self.expression(scope, expr);
                }
            }
            _ => self.conservative = true,
        }
    }

    fn chain(&mut self, scope: ScopeId, element: &'a ChainElement<'a>) {
        match element {
            ChainElement::CallExpression(call) => {
                self.expression(scope, &call.callee);
                for arg in &call.arguments {
                    if let Some(expr) = arg.as_expression() {
                        self.expression(scope, expr);
                    }
                }
            }
            ChainElement::ComputedMemberExpression(member) => {
                self.expression(scope, &member.object);
                self.expression(scope, &member.expression);
            }
            ChainElement::StaticMemberExpression(member) => {
                self.expression(scope, &member.object);
            }
            ChainElement::PrivateFieldExpression(member) => {
                self.expression(scope, &member.object);
            }
            _ => self.conservative = true,
        }
    }
}
