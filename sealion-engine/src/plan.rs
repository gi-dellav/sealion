//! Lexical planning: scope tree, bindings and the function table.
//!
//! Every `let`/`const` declaration opens a block scope; `var` declarations and
//! function declarations are hoisted to the root scope of their function.
//!
//! At runtime each scope that owns bindings gets its own environment record: a
//! `calloc`ed block whose first word points at the enclosing environment, so
//! name lookup walks a chain of environment pointers. Scopes without bindings
//! share their parent's environment, which keeps the compiled code free of
//! useless allocations. Closures capture the environment that is current where
//! the function expression is evaluated.

use std::{collections::HashMap, rc::Rc};

use boa_ast::{
    Declaration, Expression, Script, Span, Spanned, Statement, StatementList, StatementListItem,
    declaration::{Binding, VariableList},
    expression::access::{PropertyAccess, PropertyAccessField},
    function::{FormalParameterList, FunctionBody, FunctionDeclaration},
};
use boa_interner::Sym;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeId(pub usize);

#[derive(Debug, Clone)]
pub struct ScopePlan {
    pub parent: Option<ScopeId>,
    pub bindings: HashMap<Sym, usize>,
    /// Root scopes of a function always own an environment, so that the
    /// function's own frame exists even when it declares nothing.
    pub function_root: bool,
}

impl ScopePlan {
    pub fn allocated(&self) -> bool {
        self.function_root || !self.bindings.is_empty()
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
    pub params: Vec<Sym>,
    pub body: Rc<StatementList>,
    /// Hoisted `function` declarations, evaluated at function entry.
    pub children: Vec<usize>,
    /// Function and arrow expressions, keyed by the span of the expression.
    pub nested: HashMap<Span, usize>,
    /// Block scopes, keyed by the address of the block node itself.
    pub blocks: HashMap<usize, ScopeId>,
    pub is_script: bool,
    pub binding_name: Option<Sym>,
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

pub fn build_program_plan(script: &Script, undefined_sym: Sym) -> Result<Program, Error> {
    let mut planner = Planner {
        functions: Vec::new(),
        scopes: Vec::new(),
    };
    let script_id = planner.build_function_plan(
        None,
        "__script".to_string(),
        Vec::new(),
        script.statements().clone(),
        true,
    )?;
    let root = planner.functions[script_id].root_scope;
    planner.add_binding(root, undefined_sym);

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
    fn new_scope(&mut self, parent: Option<ScopeId>, function_root: bool) -> ScopeId {
        let id = ScopeId(self.scopes.len());
        self.scopes.push(ScopePlan {
            parent,
            bindings: HashMap::new(),
            function_root,
        });
        id
    }

    fn add_binding(&mut self, scope: ScopeId, sym: Sym) {
        let bindings = &mut self.scopes[scope.0].bindings;
        if !bindings.contains_key(&sym) {
            let slot = bindings.len();
            bindings.insert(sym, slot);
        }
    }

    fn root_scope(&self, plan_id: usize) -> ScopeId {
        self.functions[plan_id].root_scope
    }

    fn build_function_plan(
        &mut self,
        parent_scope: Option<ScopeId>,
        name: String,
        params: Vec<Sym>,
        body: StatementList,
        is_script: bool,
    ) -> Result<usize, Error> {
        let id = self.functions.len();
        let root_scope = self.new_scope(parent_scope, true);
        self.functions.push(FunctionPlan {
            id,
            name,
            root_scope,
            params: params.clone(),
            body: Rc::new(body),
            children: Vec::new(),
            nested: HashMap::new(),
            blocks: HashMap::new(),
            is_script,
            binding_name: None,
        });

        for param in params {
            self.add_binding(root_scope, param);
        }

        let body = Rc::clone(&self.functions[id].body);
        self.scan_statements(id, root_scope, &body)?;
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
    ) -> Result<(), Error> {
        let params = collect_params(params)?;
        let name = format!("js_fn_{}", self.functions.len());
        let child = self.build_function_plan(
            Some(scope),
            name,
            params,
            body.statement_list().clone(),
            false,
        )?;
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
                let block_scope = self.new_scope(Some(scope), false);
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
            Statement::Return(ret) => {
                if let Some(target) = ret.target() {
                    self.scan_expression(plan_id, scope, target)?;
                }
            }
            Statement::Empty => {}
            Statement::Break(break_stmt) => reject_label(break_stmt.label(), "break")?,
            Statement::Continue(continue_stmt) => reject_label(continue_stmt.label(), "continue")?,
            other => {
                return Err(Error::Unsupported(format!(
                    "statement not supported: {other:?}"
                )));
            }
        }
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
                self.add_variable_list(scope, lex.variable_list())?;
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
        )?;
        self.functions[child].binding_name = Some(func.name().sym());
        self.functions[plan_id].children.push(child);
        Ok(())
    }

    fn add_variable_list(&mut self, scope: ScopeId, list: &VariableList) -> Result<(), Error> {
        for var in list.as_ref() {
            match var.binding() {
                Binding::Identifier(id) => self.add_binding(scope, id.sym()),
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
                )?;
            }
            Expression::ArrowFunction(arrow) => {
                self.add_nested_function(
                    plan_id,
                    scope,
                    arrow.span(),
                    arrow.parameters(),
                    arrow.body(),
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

fn reject_label(label: Option<Sym>, keyword: &str) -> Result<(), Error> {
    if label.is_some() {
        return Err(Error::Unsupported(format!(
            "labelled {keyword} is not supported"
        )));
    }
    Ok(())
}

fn collect_params(params: &FormalParameterList) -> Result<Vec<Sym>, Error> {
    let mut out = Vec::new();
    for param in params.as_ref() {
        if param.is_rest_param() {
            return Err(Error::Unsupported(
                "rest parameters are not supported".to_string(),
            ));
        }
        match param.variable().binding() {
            Binding::Identifier(id) => out.push(id.sym()),
            Binding::Pattern(_) => {
                return Err(Error::Unsupported(
                    "destructuring parameters are not supported".to_string(),
                ));
            }
        }
    }
    Ok(out)
}

/// Resolves `sym` from `start`, returning how many environment pointers to
/// follow and the slot inside that environment.
pub fn resolve(program: &Program, start: ScopeId, sym: Sym) -> Option<(usize, usize)> {
    let mut allocated = 0usize;
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &program.scopes[id.0];
        if scope.allocated() {
            allocated += 1;
        }
        if let Some(&slot) = scope.bindings.get(&sym) {
            return Some((allocated - 1, slot));
        }
        current = scope.parent;
    }
    None
}
