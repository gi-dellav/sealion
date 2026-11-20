//! Lexical planning: scope tree, bindings and the function table.
//!
//! Every `let`/`const` declaration opens a block scope; `var` declarations and
//! function declarations are hoisted to the root scope of their function.
//!
//! Scopes whose bindings may be captured by a closure get an environment
//! record: a `calloc`ed block whose first word points at the enclosing
//! environment, so name lookup walks a chain of environment pointers. Bindings
//! in scopes that no closure can capture are kept in machine registers instead.
//! Closures capture the environment that is current where the function
//! expression is evaluated.

use std::collections::{HashMap, HashSet};

use oxc_ast::ast::{
    BindingPattern, CatchClause, ChainElement, Expression, ForStatement, ForStatementInit,
    FormalParameters, Function, Statement, VariableDeclaration, VariableDeclarationKind,
};

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub usize);

#[derive(Debug, Clone)]
pub struct ParamPlan<'a> {
    pub name: &'a str,
    pub init: Option<&'a Expression<'a>>,
}

#[derive(Debug, Clone)]
pub struct ScopePlan<'a> {
    pub parent: Option<ScopeId>,
    /// The function plan that owns this scope.
    pub owner: usize,
    pub bindings: HashMap<&'a str, usize>,
    /// Bindings declared with `const`, which may not be assigned to.
    pub consts: HashSet<&'a str>,
    /// Slots that start in the temporal dead zone (`let`, `const`, parameters).
    pub tdz: HashSet<usize>,
    /// True when a nested function may capture bindings from this scope.
    /// Non-captured bindings can live in registers instead of an environment.
    pub captured: bool,
}

impl ScopePlan<'_> {
    /// True when the scope needs a runtime environment record.
    pub fn has_env(&self) -> bool {
        self.captured
    }

    pub fn slot_count(&self) -> usize {
        self.bindings.len()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FunctionBodyPlan<'a> {
    Statements(&'a [Statement<'a>]),
    Expression(&'a Expression<'a>),
}

#[derive(Debug, Clone)]
pub struct FunctionPlan<'a> {
    pub id: usize,
    pub name: String,
    pub root_scope: ScopeId,
    pub params: Vec<ParamPlan<'a>>,
    pub body: FunctionBodyPlan<'a>,
    /// Hoisted `function` declarations, evaluated at function entry.
    pub children: Vec<usize>,
    /// Function and arrow expressions, keyed by the address of the AST node.
    pub nested: HashMap<usize, usize>,
    /// Block scopes, keyed by the address of the block node itself.
    pub blocks: HashMap<usize, ScopeId>,
    pub is_script: bool,
    pub binding_name: Option<&'a str>,
    /// Inner name of a named function expression, bound in the function's own
    /// environment and initialised from the call's callee value.
    pub self_binding: Option<&'a str>,
}

pub struct Program<'a> {
    pub functions: Vec<FunctionPlan<'a>>,
    pub scopes: Vec<ScopePlan<'a>>,
}

impl<'a> Program<'a> {
    pub fn scope(&self, id: ScopeId) -> &ScopePlan<'a> {
        &self.scopes[id.0]
    }

    pub fn function(&self, id: usize) -> &FunctionPlan<'a> {
        &self.functions[id]
    }
}

/// Stable identity for an AST node: the plans borrow the parsed tree, so node
/// addresses do not change between planning and code generation.
pub fn node_key<T>(node: &T) -> usize {
    std::ptr::from_ref(node) as usize
}

pub fn build_program_plan<'a>(
    program: &'a oxc_ast::ast::Program<'a>,
) -> Result<Program<'a>, Error> {
    let mut planner = Planner {
        functions: Vec::new(),
        scopes: Vec::new(),
    };
    planner.build_function_plan(
        None,
        "__script".to_string(),
        Vec::new(),
        FunctionBodyPlan::Statements(program.body.as_slice()),
        true,
        None,
    )?;

    Ok(Program {
        functions: planner.functions,
        scopes: planner.scopes,
    })
}

struct Planner<'a> {
    functions: Vec<FunctionPlan<'a>>,
    scopes: Vec<ScopePlan<'a>>,
}

impl<'a> Planner<'a> {
    fn new_scope(&mut self, owner: usize, parent: Option<ScopeId>) -> ScopeId {
        let id = ScopeId(self.scopes.len());
        self.scopes.push(ScopePlan {
            parent,
            owner,
            bindings: HashMap::new(),
            consts: HashSet::new(),
            tdz: HashSet::new(),
            captured: false,
        });
        id
    }

    /// Marks a scope and every lexical ancestor as potentially captured by a
    /// closure, so their bindings stay in environment records.
    fn mark_captured(&mut self, start: ScopeId) {
        let mut current = Some(start);
        while let Some(id) = current {
            let scope = &mut self.scopes[id.0];
            scope.captured = true;
            current = scope.parent;
        }
    }

    fn add_binding(&mut self, scope: ScopeId, name: &'a str) -> usize {
        let bindings = &mut self.scopes[scope.0].bindings;
        if let Some(slot) = bindings.get(name).copied() {
            return slot;
        }
        let slot = bindings.len();
        bindings.insert(name, slot);
        slot
    }

    fn add_variables(
        &mut self,
        scope: ScopeId,
        decl: &'a VariableDeclaration<'a>,
    ) -> Result<(), Error> {
        let is_const = decl.kind == VariableDeclarationKind::Const;
        let mut tdz_slots = Vec::new();
        let mut consts = Vec::new();
        for var in &decl.declarations {
            let BindingPattern::BindingIdentifier(id) = &var.id else {
                return Err(Error::Unsupported(
                    "destructuring bindings are not supported".to_string(),
                ));
            };
            let name = id.name.as_str();
            let slot = self.add_binding(scope, name);
            if decl.kind != VariableDeclarationKind::Var {
                tdz_slots.push(slot);
            }
            if is_const {
                consts.push(name);
            }
        }
        let scope_plan = &mut self.scopes[scope.0];
        scope_plan.tdz.extend(tdz_slots);
        scope_plan.consts.extend(consts);
        Ok(())
    }

    fn root_scope(&self, plan_id: usize) -> ScopeId {
        self.functions[plan_id].root_scope
    }

    fn build_function_plan(
        &mut self,
        parent_scope: Option<ScopeId>,
        name: String,
        params: Vec<ParamPlan<'a>>,
        body: FunctionBodyPlan<'a>,
        is_script: bool,
        self_binding: Option<&'a str>,
    ) -> Result<usize, Error> {
        let id = self.functions.len();
        let root_scope = self.new_scope(id, parent_scope);
        self.functions.push(FunctionPlan {
            id,
            name,
            root_scope,
            params: Vec::new(),
            body,
            children: Vec::new(),
            nested: HashMap::new(),
            blocks: HashMap::new(),
            is_script,
            binding_name: None,
            self_binding,
        });

        for param in &params {
            let slot = self.add_binding(root_scope, param.name);
            self.scopes[root_scope.0].tdz.insert(slot);
        }
        if let Some(name) = self_binding {
            self.add_binding(root_scope, name);
        }

        for param in &params {
            if let Some(init) = param.init {
                self.scan_expression(id, root_scope, init)?;
            }
        }
        match body {
            FunctionBodyPlan::Statements(statements) => {
                self.scan_statements(id, root_scope, statements)?;
            }
            FunctionBodyPlan::Expression(expression) => {
                self.scan_expression(id, root_scope, expression)?;
            }
        }
        self.functions[id].params = params;
        Ok(id)
    }

    /// Builds a plan for a function or arrow expression nested in `scope`.
    fn add_nested_function(
        &mut self,
        plan_id: usize,
        scope: ScopeId,
        key: usize,
        params: &'a FormalParameters<'a>,
        body: FunctionBodyPlan<'a>,
        self_binding: Option<&'a str>,
    ) -> Result<(), Error> {
        let params = collect_params(params)?;
        let name = format!("js_fn_{}", self.functions.len());
        let child =
            self.build_function_plan(Some(scope), name, params, body, false, self_binding)?;
        self.mark_captured(scope);
        self.functions[plan_id].nested.insert(key, child);
        Ok(())
    }

    fn scan_statements(
        &mut self,
        plan_id: usize,
        scope: ScopeId,
        list: &'a [Statement<'a>],
    ) -> Result<(), Error> {
        for stmt in list {
            self.scan_statement(plan_id, scope, stmt)?;
        }
        Ok(())
    }

    fn scan_statement(
        &mut self,
        plan_id: usize,
        scope: ScopeId,
        stmt: &'a Statement<'a>,
    ) -> Result<(), Error> {
        match stmt {
            Statement::BlockStatement(block) => {
                self.scan_block(plan_id, scope, block)?;
            }
            Statement::VariableDeclaration(decl) => {
                if decl.kind == VariableDeclarationKind::Var {
                    let root = self.root_scope(plan_id);
                    self.add_variables(root, decl)?;
                } else {
                    self.add_variables(scope, decl)?;
                }
                self.scan_initializers(plan_id, scope, decl)?;
            }
            Statement::FunctionDeclaration(func) => {
                self.add_function_declaration(plan_id, func)?;
            }
            Statement::ExpressionStatement(expr) => {
                self.scan_expression(plan_id, scope, &expr.expression)?;
            }
            Statement::IfStatement(if_stmt) => {
                self.scan_expression(plan_id, scope, &if_stmt.test)?;
                self.scan_statement(plan_id, scope, &if_stmt.consequent)?;
                if let Some(else_node) = &if_stmt.alternate {
                    self.scan_statement(plan_id, scope, else_node)?;
                }
            }
            Statement::WhileStatement(while_stmt) => {
                self.scan_expression(plan_id, scope, &while_stmt.test)?;
                self.scan_statement(plan_id, scope, &while_stmt.body)?;
            }
            Statement::DoWhileStatement(do_while) => {
                self.scan_statement(plan_id, scope, &do_while.body)?;
                self.scan_expression(plan_id, scope, &do_while.test)?;
            }
            Statement::ForStatement(for_stmt) => self.scan_for_loop(plan_id, scope, for_stmt)?,
            Statement::SwitchStatement(switch_stmt) => {
                let switch_scope = self.new_scope(plan_id, Some(scope));
                self.functions[plan_id]
                    .blocks
                    .insert(node_key(&**switch_stmt), switch_scope);
                self.scan_expression(plan_id, scope, &switch_stmt.discriminant)?;
                for case in &switch_stmt.cases {
                    if let Some(condition) = &case.test {
                        self.scan_expression(plan_id, switch_scope, condition)?;
                    }
                    self.scan_statements(plan_id, switch_scope, case.consequent.as_slice())?;
                }
            }
            Statement::ReturnStatement(ret) => {
                if let Some(target) = &ret.argument {
                    self.scan_expression(plan_id, scope, target)?;
                }
            }
            Statement::LabeledStatement(labelled) => {
                if matches!(&labelled.body, Statement::FunctionDeclaration(_)) {
                    return Err(Error::Unsupported(
                        "labelled function declarations are not supported".to_string(),
                    ));
                }
                self.scan_statement(plan_id, scope, &labelled.body)?;
            }
            Statement::ThrowStatement(throw) => {
                self.scan_expression(plan_id, scope, &throw.argument)?;
            }
            Statement::TryStatement(try_stmt) => {
                self.scan_block(plan_id, scope, &try_stmt.block)?;
                if let Some(handler) = &try_stmt.handler {
                    self.scan_catch(plan_id, scope, handler)?;
                }
                if let Some(finalizer) = &try_stmt.finalizer {
                    self.scan_block(plan_id, scope, finalizer)?;
                }
            }
            Statement::EmptyStatement(_)
            | Statement::DebuggerStatement(_)
            | Statement::BreakStatement(_)
            | Statement::ContinueStatement(_) => {}
            other => {
                return Err(Error::Unsupported(format!(
                    "statement not supported: {other:?}"
                )));
            }
        }
        Ok(())
    }

    /// A `for (init; cond; update)` loop. A lexical initializer opens a scope of
    /// its own that covers the condition, the update and the body.
    fn scan_for_loop(
        &mut self,
        plan_id: usize,
        scope: ScopeId,
        for_stmt: &'a ForStatement<'a>,
    ) -> Result<(), Error> {
        let mut loop_scope = scope;
        match &for_stmt.init {
            Some(ForStatementInit::VariableDeclaration(decl))
                if decl.kind != VariableDeclarationKind::Var =>
            {
                let inner = self.new_scope(plan_id, Some(scope));
                self.functions[plan_id]
                    .blocks
                    .insert(node_key(for_stmt), inner);
                self.add_variables(inner, decl)?;
                self.scan_initializers(plan_id, inner, decl)?;
                loop_scope = inner;
            }
            Some(ForStatementInit::VariableDeclaration(decl)) => {
                let root = self.root_scope(plan_id);
                self.add_variables(root, decl)?;
                self.scan_initializers(plan_id, loop_scope, decl)?;
            }
            Some(init) => {
                if let Some(expr) = init.as_expression() {
                    self.scan_expression(plan_id, loop_scope, expr)?;
                }
            }
            None => {}
        }
        if let Some(condition) = &for_stmt.test {
            self.scan_expression(plan_id, loop_scope, condition)?;
        }
        if let Some(update) = &for_stmt.update {
            self.scan_expression(plan_id, loop_scope, update)?;
        }
        self.scan_statement(plan_id, loop_scope, &for_stmt.body)?;
        Ok(())
    }

    fn scan_block(
        &mut self,
        plan_id: usize,
        scope: ScopeId,
        block: &'a oxc_ast::ast::BlockStatement<'a>,
    ) -> Result<ScopeId, Error> {
        let block_scope = self.new_scope(plan_id, Some(scope));
        self.functions[plan_id]
            .blocks
            .insert(node_key(block), block_scope);
        self.scan_statements(plan_id, block_scope, block.body.as_slice())?;
        Ok(block_scope)
    }

    fn scan_catch(
        &mut self,
        plan_id: usize,
        scope: ScopeId,
        handler: &'a CatchClause<'a>,
    ) -> Result<(), Error> {
        let catch_scope = self.new_scope(plan_id, Some(scope));
        self.functions[plan_id]
            .blocks
            .insert(node_key(handler), catch_scope);
        if let Some(param) = &handler.param {
            let BindingPattern::BindingIdentifier(id) = &param.pattern else {
                return Err(Error::Unsupported(
                    "destructuring catch parameters are not supported".to_string(),
                ));
            };
            self.add_binding(catch_scope, id.name.as_str());
        }
        self.scan_statements(plan_id, catch_scope, handler.body.body.as_slice())
    }

    fn scan_chain_element(
        &mut self,
        plan_id: usize,
        scope: ScopeId,
        element: &'a ChainElement<'a>,
    ) -> Result<(), Error> {
        match element {
            ChainElement::CallExpression(call) => {
                self.scan_expression(plan_id, scope, &call.callee)?;
                for arg in &call.arguments {
                    if let Some(expr) = arg.as_expression() {
                        self.scan_expression(plan_id, scope, expr)?;
                    }
                }
            }
            ChainElement::ComputedMemberExpression(member) => {
                self.scan_expression(plan_id, scope, &member.object)?;
                self.scan_expression(plan_id, scope, &member.expression)?;
            }
            ChainElement::StaticMemberExpression(member) => {
                self.scan_expression(plan_id, scope, &member.object)?;
            }
            ChainElement::PrivateFieldExpression(member) => {
                self.scan_expression(plan_id, scope, &member.object)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn add_function_declaration(
        &mut self,
        plan_id: usize,
        func: &'a Function<'a>,
    ) -> Result<(), Error> {
        let id = func
            .id
            .as_ref()
            .ok_or_else(|| Error::Unsupported("function declaration without a name".to_string()))?;
        let name = id.name.as_str();
        let root = self.root_scope(plan_id);
        self.add_binding(root, name);

        let params = collect_params(&func.params)?;
        let child_name = format!("js_fn_{}", self.functions.len());
        let body = func
            .body
            .as_ref()
            .ok_or_else(|| Error::Unsupported("function without a body".to_string()))?;
        let child = self.build_function_plan(
            Some(root),
            child_name,
            params,
            FunctionBodyPlan::Statements(body.statements.as_slice()),
            false,
            None,
        )?;
        self.mark_captured(root);
        self.functions[child].binding_name = Some(name);
        self.functions[plan_id].children.push(child);
        Ok(())
    }

    fn scan_initializers(
        &mut self,
        plan_id: usize,
        scope: ScopeId,
        decl: &'a VariableDeclaration<'a>,
    ) -> Result<(), Error> {
        for var in &decl.declarations {
            if let Some(init) = &var.init {
                self.scan_expression(plan_id, scope, init)?;
            }
        }
        Ok(())
    }

    /// Walks an expression looking for function/arrow expressions, which need a
    /// plan of their own. Unsupported nodes are left for code generation to
    /// reject with a better error.
    fn scan_expression(
        &mut self,
        plan_id: usize,
        scope: ScopeId,
        expr: &'a Expression<'a>,
    ) -> Result<(), Error> {
        match expr {
            Expression::FunctionExpression(func) => {
                let body = func
                    .body
                    .as_ref()
                    .ok_or_else(|| Error::Unsupported("function without a body".to_string()))?;
                self.add_nested_function(
                    plan_id,
                    scope,
                    node_key(&**func),
                    &func.params,
                    FunctionBodyPlan::Statements(body.statements.as_slice()),
                    func.id.as_ref().map(|id| id.name.as_str()),
                )?;
            }
            Expression::ArrowFunctionExpression(arrow) => {
                let body = match &arrow.body {
                    oxc_ast::ast::ArrowFunctionBody::FunctionBody(body) => {
                        FunctionBodyPlan::Statements(body.statements.as_slice())
                    }
                    expression => FunctionBodyPlan::Expression(expression.to_expression()),
                };
                self.add_nested_function(
                    plan_id,
                    scope,
                    node_key(&**arrow),
                    &arrow.params,
                    body,
                    None,
                )?;
            }
            Expression::ParenthesizedExpression(paren) => {
                self.scan_expression(plan_id, scope, &paren.expression)?;
            }
            Expression::BinaryExpression(binary) => {
                self.scan_expression(plan_id, scope, &binary.left)?;
                self.scan_expression(plan_id, scope, &binary.right)?;
            }
            Expression::LogicalExpression(logical) => {
                self.scan_expression(plan_id, scope, &logical.left)?;
                self.scan_expression(plan_id, scope, &logical.right)?;
            }
            Expression::UnaryExpression(unary) => {
                self.scan_expression(plan_id, scope, &unary.argument)?;
            }
            Expression::UpdateExpression(_) => {}
            Expression::ConditionalExpression(conditional) => {
                self.scan_expression(plan_id, scope, &conditional.test)?;
                self.scan_expression(plan_id, scope, &conditional.consequent)?;
                self.scan_expression(plan_id, scope, &conditional.alternate)?;
            }
            Expression::AssignmentExpression(assign) => {
                self.scan_expression(plan_id, scope, &assign.right)?;
            }
            Expression::SequenceExpression(sequence) => {
                for expr in &sequence.expressions {
                    self.scan_expression(plan_id, scope, expr)?;
                }
            }
            Expression::CallExpression(call) => {
                self.scan_expression(plan_id, scope, &call.callee)?;
                for arg in &call.arguments {
                    if let Some(expr) = arg.as_expression() {
                        self.scan_expression(plan_id, scope, expr)?;
                    }
                }
            }
            Expression::ChainExpression(chain) => {
                self.scan_chain_element(plan_id, scope, &chain.expression)?;
            }
            Expression::StaticMemberExpression(simple) => {
                self.scan_expression(plan_id, scope, &simple.object)?;
            }
            Expression::ComputedMemberExpression(computed) => {
                self.scan_expression(plan_id, scope, &computed.object)?;
                self.scan_expression(plan_id, scope, &computed.expression)?;
            }
            _ => {}
        }
        Ok(())
    }
}

fn collect_params<'a>(params: &'a FormalParameters<'a>) -> Result<Vec<ParamPlan<'a>>, Error> {
    if params.rest.is_some() {
        return Err(Error::Unsupported(
            "rest parameters are not supported".to_string(),
        ));
    }
    let mut out = Vec::new();
    for param in &params.items {
        let BindingPattern::BindingIdentifier(id) = &param.pattern else {
            return Err(Error::Unsupported(
                "destructuring parameters are not supported".to_string(),
            ));
        };
        out.push(ParamPlan {
            name: id.name.as_str(),
            init: param.initializer.as_deref(),
        });
    }
    Ok(out)
}

/// True when `name` resolves to a `const` binding in scope from `start`.
pub fn is_const(program: &Program<'_>, start: ScopeId, name: &str) -> bool {
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &program.scopes[id.0];
        if scope.bindings.contains_key(name) {
            return scope.consts.contains(name);
        }
        current = scope.parent;
    }
    false
}

/// True when `name` resolves to a binding that starts in the temporal dead zone.
pub fn needs_tdz_check(program: &Program<'_>, start: ScopeId, name: &str) -> bool {
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &program.scopes[id.0];
        if let Some(&slot) = scope.bindings.get(name) {
            return scope.tdz.contains(&slot);
        }
        current = scope.parent;
    }
    false
}

/// Resolves `name` from `start`, returning the scope that owns the binding and
/// its slot.
pub fn resolve_binding(
    program: &Program<'_>,
    start: ScopeId,
    name: &str,
) -> Option<(ScopeId, usize)> {
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &program.scopes[id.0];
        if let Some(&slot) = scope.bindings.get(name) {
            return Some((id, slot));
        }
        current = scope.parent;
    }
    None
}

/// Resolves `name` from `start`, returning how many environment pointers to
/// follow and the slot inside that environment.
pub fn resolve(program: &Program<'_>, start: ScopeId, name: &str) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &program.scopes[id.0];
        if let Some(&slot) = scope.bindings.get(name) {
            return Some((depth, slot));
        }
        if scope.has_env() {
            depth += 1;
        }
        current = scope.parent;
    }
    None
}
