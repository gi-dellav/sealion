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

use oxc_ast::ast::{Expression, Statement};

mod calls;
mod planner;
mod resolve;

pub use calls::DirectCallInfo;
pub use planner::build_program_plan;
pub use resolve::{is_const, needs_tdz_check, resolve, resolve_binding};

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
    /// Unique string literal contents, emitted as read-only data.
    pub strings: Vec<String>,
    /// Direct-call and escaping information for hoisted function declarations.
    pub direct_calls: DirectCallInfo<'a>,
    /// Bindings whose loads and stores cannot observe the temporal dead zone.
    pub no_tdz: HashSet<(ScopeId, usize)>,
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
