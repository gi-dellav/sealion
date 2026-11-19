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

use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use boa_ast::{
    Declaration, Expression, Script, Span, Spanned, Statement, StatementList, StatementListItem,
    declaration::{Binding, LexicalDeclaration, VariableList},
    expression::access::{PropertyAccess, PropertyAccessField},
    function::{FormalParameterList, FunctionBody, FunctionDeclaration},
    statement::{LabelledItem, iteration::ForLoopInitializer},
};
use boa_interner::Sym;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub usize);

#[derive(Debug, Clone)]
pub struct ParamPlan {
    pub sym: Sym,
    pub init: Option<Expression>,
}

#[derive(Debug, Clone)]
pub struct ScopePlan {
    pub parent: Option<ScopeId>,
    /// The function plan that owns this scope.
    pub owner: usize,
    pub bindings: HashMap<Sym, usize>,
    /// Bindings declared with `const`, which may not be assigned to.
    pub consts: HashSet<Sym>,
    /// Slots that start in the temporal dead zone (`let`, `const`, parameters).
    pub tdz: HashSet<usize>,
    /// True when a nested function may capture bindings from this scope.
    /// Non-captured bindings can live in registers instead of an environment.
    pub captured: bool,
}

impl ScopePlan {
    /// True when the scope needs a runtime environment record.
    pub fn has_env(&self) -> bool {
        self.captured
    }

    pub fn slot_count(&self) -> usize {
        self.bindings.len()
    }
}

#[derive(Debug, Clone)]
pub struct FunctionPlan {
    pub id: usize,
    pub name: String,
    pub root_scope: ScopeId,
    pub params: Vec<ParamPlan>,
    pub body: Rc<StatementList>,
    /// Hoisted `function` declarations, evaluated at function entry.
    pub children: Vec<usize>,
    /// Function and arrow expressions, keyed by the span of the expression.
    pub nested: HashMap<Span, usize>,
    /// Block scopes, keyed by the address of the block node itself.
    pub blocks: HashMap<usize, ScopeId>,
    pub is_script: bool,
    pub binding_name: Option<Sym>,
    /// Inner name of a named function expression, bound in the function's own
    /// environment and initialised from the call's callee value.
    pub self_binding: Option<Sym>,
}

pub struct Program {
    pub functions: Vec<FunctionPlan>,
    pub scopes: Vec<ScopePlan>,
}

impl Program {
    pub fn scope(&self, id: ScopeId) -> &ScopePlan {
        &self.scopes[id.0]
    }

    pub fn function(&self, id: usize) -> &FunctionPlan {
        &self.functions[id]
    }
}

/// Stable identity for an AST node: the plans own the parsed tree, so node
/// addresses do not change between planning and code generation.
pub fn node_key<T>(node: &T) -> usize {
    std::ptr::from_ref(node) as usize
}

pub fn build_program_plan(script: &Script) -> Result<Program, Error> {
    let mut planner = Planner {
        functions: Vec::new(),
        scopes: Vec::new(),
    };
    planner.build_function_plan(
        None,
        "__script".to_string(),
        Vec::new(),
        script.statements().clone(),
        true,
        None,
    )?;

    Ok(Program {
        functions: planner.functions,
        scopes: planner.scopes,
    })
}

struct Planner {
    functions: Vec<FunctionPlan>,
    scopes: Vec<ScopePlan>,
}

impl Planner {
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

    fn add_binding(&mut self, scope: ScopeId, sym: Sym) -> usize {
        let bindings = &mut self.scopes[scope.0].bindings;
        if let Some(slot) = bindings.get(&sym).copied() {
            return slot;
        }
        let slot = bindings.len();
        bindings.insert(sym, slot);
        slot
    }

    fn add_lexical(&mut self, scope: ScopeId, decl: &LexicalDeclaration) -> Result<(), Error> {
        self.add_variable_list(scope, decl.variable_list())?;
        let is_const = decl.is_const();
        let mut tdz_slots = Vec::new();
        let mut consts = Vec::new();
        for var in decl.variable_list().as_ref() {
            if let Binding::Identifier(id) = var.binding() {
                let sym = id.sym();
                if let Some(slot) = self.scopes[scope.0].bindings.get(&sym).copied() {
                    tdz_slots.push(slot);
                }
                if is_const {
                    consts.push(sym);
                }
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
        params: Vec<ParamPlan>,
        body: StatementList,
        is_script: bool,
        self_binding: Option<Sym>,
    ) -> Result<usize, Error> {
        let id = self.functions.len();
        let root_scope = self.new_scope(id, parent_scope);
        self.functions.push(FunctionPlan {
            id,
            name,
            root_scope,
            params: Vec::new(),
            body: Rc::new(body),
            children: Vec::new(),
            nested: HashMap::new(),
            blocks: HashMap::new(),
            is_script,
            binding_name: None,
            self_binding,
        });

        for param in &params {
            let slot = self.add_binding(root_scope, param.sym);
            self.scopes[root_scope.0].tdz.insert(slot);
        }
        if let Some(sym) = self_binding {
            self.add_binding(root_scope, sym);
        }

        let body = Rc::clone(&self.functions[id].body);
        for param in &params {
            if let Some(init) = &param.init {
                self.scan_expression(id, root_scope, init)?;
            }
        }
        self.scan_statements(id, root_scope, &body)?;
        self.functions[id].params = params;
        Ok(id)
    }

    /// Builds a plan for a function or arrow expression nested in `scope`.
    fn add_nested_function(
        &mut self,
        plan_id: usize,
        scope: ScopeId,
        span: Span,
        params: &FormalParameterList,
        body: &FunctionBody,
        self_binding: Option<Sym>,
    ) -> Result<(), Error> {
        let params = collect_params(params)?;
        let name = format!("js_fn_{}", self.functions.len());
        let child = self.build_function_plan(
            Some(scope),
            name,
            params,
            body.statement_list().clone(),
            false,
            self_binding,
        )?;
        self.mark_captured(scope);
        self.functions[plan_id].nested.insert(span, child);
        Ok(())
    }

    fn scan_statements(
        &mut self,
        plan_id: usize,
        scope: ScopeId,
        list: &StatementList,
    ) -> Result<(), Error> {
        for item in list.statements() {
            match item {
                StatementListItem::Statement(stmt) => self.scan_statement(plan_id, scope, stmt)?,
                StatementListItem::Declaration(decl) => {
                    self.scan_declaration(plan_id, scope, decl)?;
                }
            }
        }
        Ok(())
    }

    fn scan_statement(
        &mut self,
        plan_id: usize,
        scope: ScopeId,
        stmt: &Statement,
    ) -> Result<(), Error> {
        match stmt {
            Statement::Block(block) => {
                let block_scope = self.new_scope(plan_id, Some(scope));
                self.functions[plan_id]
                    .blocks
                    .insert(node_key(block), block_scope);
                self.scan_statements(plan_id, block_scope, block.statement_list())?;
            }
            Statement::Var(var) => {
                let root = self.root_scope(plan_id);
                self.add_variable_list(root, &var.0)?;
                self.scan_initializers(plan_id, scope, &var.0)?;
            }
            Statement::Expression(expr) => self.scan_expression(plan_id, scope, expr)?,
            Statement::If(if_stmt) => {
                self.scan_expression(plan_id, scope, if_stmt.cond())?;
                self.scan_statement(plan_id, scope, if_stmt.body())?;
                if let Some(else_node) = if_stmt.else_node() {
                    self.scan_statement(plan_id, scope, else_node)?;
                }
            }
            Statement::WhileLoop(while_stmt) => {
                self.scan_expression(plan_id, scope, while_stmt.condition())?;
                self.scan_statement(plan_id, scope, while_stmt.body())?;
            }
            Statement::DoWhileLoop(do_while) => {
                self.scan_statement(plan_id, scope, do_while.body())?;
                self.scan_expression(plan_id, scope, do_while.cond())?;
            }
            Statement::ForLoop(for_stmt) => self.scan_for_loop(plan_id, scope, for_stmt)?,
            Statement::Switch(switch_stmt) => {
                let switch_scope = self.new_scope(plan_id, Some(scope));
                self.functions[plan_id]
                    .blocks
                    .insert(node_key(switch_stmt), switch_scope);
                self.scan_expression(plan_id, scope, switch_stmt.val())?;
                for case in switch_stmt.cases() {
                    if let Some(condition) = case.condition() {
                        self.scan_expression(plan_id, switch_scope, condition)?;
                    }
                    self.scan_statements(plan_id, switch_scope, case.body())?;
                }
            }
            Statement::Return(ret) => {
                if let Some(target) = ret.target() {
                    self.scan_expression(plan_id, scope, target)?;
                }
            }
            Statement::Labelled(labelled) => match labelled.item() {
                LabelledItem::Statement(stmt) => self.scan_statement(plan_id, scope, stmt)?,
                LabelledItem::FunctionDeclaration(_) => {
                    return Err(Error::Unsupported(
                        "labelled function declarations are not supported".to_string(),
                    ));
                }
            },
            Statement::Empty
            | Statement::Debugger
            | Statement::Break(_)
            | Statement::Continue(_) => {}
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
        for_stmt: &boa_ast::statement::iteration::ForLoop,
    ) -> Result<(), Error> {
        let mut loop_scope = scope;
        match for_stmt.init() {
            Some(ForLoopInitializer::Lexical(lex)) => {
                let inner = self.new_scope(plan_id, Some(scope));
                self.functions[plan_id]
                    .blocks
                    .insert(node_key(for_stmt), inner);
                self.add_lexical(inner, lex.declaration())?;
                self.scan_initializers(plan_id, inner, lex.declaration().variable_list())?;
                loop_scope = inner;
            }
            Some(ForLoopInitializer::Var(var)) => {
                let root = self.root_scope(plan_id);
                self.add_variable_list(root, &var.0)?;
                self.scan_initializers(plan_id, loop_scope, &var.0)?;
            }
            Some(ForLoopInitializer::Expression(expr)) => {
                self.scan_expression(plan_id, loop_scope, expr)?;
            }
            None => {}
        }
        if let Some(condition) = for_stmt.condition() {
            self.scan_expression(plan_id, loop_scope, condition)?;
        }
        if let Some(update) = for_stmt.final_expr() {
            self.scan_expression(plan_id, loop_scope, update)?;
        }
        self.scan_statement(plan_id, loop_scope, for_stmt.body())?;
        Ok(())
    }

    fn scan_declaration(
        &mut self,
        plan_id: usize,
        scope: ScopeId,
        decl: &Declaration,
    ) -> Result<(), Error> {
        match decl {
            Declaration::FunctionDeclaration(func) => {
                self.add_function_declaration(plan_id, func)?;
            }
            Declaration::Lexical(lex) => {
                self.add_lexical(scope, lex)?;
                self.scan_initializers(plan_id, scope, lex.variable_list())?;
            }
            other => {
                return Err(Error::Unsupported(format!(
                    "declaration not supported: {other:?}"
                )));
            }
        }
        Ok(())
    }

    fn add_function_declaration(
        &mut self,
        plan_id: usize,
        func: &FunctionDeclaration,
    ) -> Result<(), Error> {
        let root = self.root_scope(plan_id);
        self.add_binding(root, func.name().sym());

        let params = collect_params(func.parameters())?;
        let name = format!("js_fn_{}", self.functions.len());
        let child = self.build_function_plan(
            Some(root),
            name,
            params,
            func.body().statement_list().clone(),
            false,
            None,
        )?;
        self.mark_captured(root);
        self.functions[child].binding_name = Some(func.name().sym());
        self.functions[plan_id].children.push(child);
        Ok(())
    }

    fn add_variable_list(&mut self, scope: ScopeId, list: &VariableList) -> Result<(), Error> {
        for var in list.as_ref() {
            match var.binding() {
                Binding::Identifier(id) => {
                    self.add_binding(scope, id.sym());
                }
                Binding::Pattern(_) => {
                    return Err(Error::Unsupported(
                        "destructuring bindings are not supported".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn scan_initializers(
        &mut self,
        plan_id: usize,
        scope: ScopeId,
        list: &VariableList,
    ) -> Result<(), Error> {
        for var in list.as_ref() {
            if let Some(init) = var.init() {
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
        expr: &Expression,
    ) -> Result<(), Error> {
        match expr {
            Expression::FunctionExpression(func) => {
                self.add_nested_function(
                    plan_id,
                    scope,
                    func.span(),
                    func.parameters(),
                    func.body(),
                    func.name().map(|id| id.sym()),
                )?;
            }
            Expression::ArrowFunction(arrow) => {
                self.add_nested_function(
                    plan_id,
                    scope,
                    arrow.span(),
                    arrow.parameters(),
                    arrow.body(),
                    None,
                )?;
            }
            Expression::Parenthesized(paren) => {
                self.scan_expression(plan_id, scope, paren.expression())?;
            }
            Expression::Binary(binary) => {
                self.scan_expression(plan_id, scope, binary.lhs())?;
                self.scan_expression(plan_id, scope, binary.rhs())?;
            }
            Expression::Unary(unary) => self.scan_expression(plan_id, scope, unary.target())?,
            Expression::Conditional(conditional) => {
                self.scan_expression(plan_id, scope, conditional.condition())?;
                self.scan_expression(plan_id, scope, conditional.if_true())?;
                self.scan_expression(plan_id, scope, conditional.if_false())?;
            }
            Expression::Assign(assign) => {
                self.scan_expression(plan_id, scope, assign.rhs())?;
            }
            Expression::Call(call) => {
                self.scan_expression(plan_id, scope, call.function())?;
                for arg in call.args() {
                    self.scan_expression(plan_id, scope, arg)?;
                }
            }
            Expression::PropertyAccess(PropertyAccess::Simple(simple)) => {
                self.scan_expression(plan_id, scope, simple.target())?;
                if let PropertyAccessField::Expr(field) = simple.field() {
                    self.scan_expression(plan_id, scope, field)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn collect_params(params: &FormalParameterList) -> Result<Vec<ParamPlan>, Error> {
    let mut out = Vec::new();
    for param in params.as_ref() {
        if param.is_rest_param() {
            return Err(Error::Unsupported(
                "rest parameters are not supported".to_string(),
            ));
        }
        match param.variable().binding() {
            Binding::Identifier(id) => out.push(ParamPlan {
                sym: id.sym(),
                init: param.init().cloned(),
            }),
            Binding::Pattern(_) => {
                return Err(Error::Unsupported(
                    "destructuring parameters are not supported".to_string(),
                ));
            }
        }
    }
    Ok(out)
}

/// True when `sym` resolves to a `const` binding in scope from `start`.
pub fn is_const(program: &Program, start: ScopeId, sym: Sym) -> bool {
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &program.scopes[id.0];
        if scope.bindings.contains_key(&sym) {
            return scope.consts.contains(&sym);
        }
        current = scope.parent;
    }
    false
}

/// True when `sym` resolves to a binding that starts in the temporal dead zone.
pub fn needs_tdz_check(program: &Program, start: ScopeId, sym: Sym) -> bool {
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &program.scopes[id.0];
        if let Some(&slot) = scope.bindings.get(&sym) {
            return scope.tdz.contains(&slot);
        }
        current = scope.parent;
    }
    false
}

/// Resolves `sym` from `start`, returning the scope that owns the binding and
/// its slot.
pub fn resolve_binding(program: &Program, start: ScopeId, sym: Sym) -> Option<(ScopeId, usize)> {
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &program.scopes[id.0];
        if let Some(&slot) = scope.bindings.get(&sym) {
            return Some((id, slot));
        }
        current = scope.parent;
    }
    None
}

/// Resolves `sym` from `start`, returning how many environment pointers to
/// follow and the slot inside that environment.
pub fn resolve(program: &Program, start: ScopeId, sym: Sym) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &program.scopes[id.0];
        if let Some(&slot) = scope.bindings.get(&sym) {
            return Some((depth, slot));
        }
        if scope.has_env() {
            depth += 1;
        }
        current = scope.parent;
    }
    None
}
