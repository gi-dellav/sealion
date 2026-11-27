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

/// Where a class method finds `super`: the scope holding the class's hidden
/// slots, their indices and whether the method is static.
#[derive(Debug, Clone, Copy)]
pub struct SuperHome {
    pub scope: ScopeId,
    /// The parent constructor value (`super(...)` and static `super.x`).
    pub ctor_slot: usize,
    /// The parent's `prototype` object (instance `super.x`).
    pub proto_slot: usize,
    pub is_static: bool,
}

/// One method of a class: the closure that becomes `Class.prototype.<name>` or
/// `Class.<name>`.
#[derive(Debug, Clone)]
pub struct ClassMethodPlan<'a> {
    pub name: &'a str,
    pub plan: usize,
    pub is_static: bool,
}

/// The lowered form of a `class` node. The class itself is a function value (the
/// constructor); methods are assigned to its `prototype` object or to itself.
#[derive(Debug, Clone)]
pub struct ClassPlan<'a> {
    /// Scope holding the hidden parent slots; always an environment record so
    /// that the method closures can capture it.
    pub scope: ScopeId,
    pub parent: Option<&'a Expression<'a>>,
    /// The constructor plan, synthesised when the class declares none.
    pub ctor: usize,
    pub methods: Vec<ClassMethodPlan<'a>>,
    /// The class name, bound inside the class's own scope so that the methods
    /// and static initialisers can refer to it.
    pub name_binding: Option<&'a str>,
    pub ctor_slot: usize,
    pub proto_slot: usize,
}

#[derive(Debug, Clone)]
pub struct ScopePlan<'a> {
    pub parent: Option<ScopeId>,
    /// The function plan that owns this scope.
    pub owner: usize,
    pub bindings: HashMap<&'a str, usize>,
    /// Anonymous slots after the named bindings, used by lowered constructs such
    /// as the class `super` slots.
    pub extra: usize,
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
        self.bindings.len() + self.extra
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
    /// Class constructors and methods lowered inside this function. They are
    /// not hoisted: the class body creates their closures.
    pub members: Vec<usize>,
    /// Function and arrow expressions, keyed by the address of the AST node.
    pub nested: HashMap<usize, usize>,
    /// Block scopes, keyed by the address of the block node itself.
    pub blocks: HashMap<usize, ScopeId>,
    pub is_script: bool,
    pub binding_name: Option<&'a str>,
    /// Inner name of a named function expression, bound in the function's own
    /// environment and initialised from the call's callee value.
    pub self_binding: Option<&'a str>,
    /// The plan that lexically encloses this one, so that an arrow inside a
    /// method can still resolve `super`.
    pub parent: Option<usize>,
    /// Arrow functions inherit `this` and have no `prototype` of their own.
    pub is_arrow: bool,
    /// Set for a class constructor or method: where `super` is resolved.
    pub super_home: Option<SuperHome>,
    /// A synthesised derived-class constructor: forward `super(...arguments)`.
    pub forwards_super: bool,
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
    /// Lowered class nodes, keyed by the address of the `Class` node.
    pub classes: HashMap<usize, ClassPlan<'a>>,
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
