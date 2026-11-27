//! A small JavaScript-to-native compiler for the 0.6.0 roadmap subset.
//!
//! The engine parses JavaScript and TypeScript with `oxc-parser`, lowers the AST to Cranelift IR,
//! and can emit either a native object file or a linked executable.
//!
//! ```
//! use sealion_engine::Engine;
//!
//! let mut module = Engine::new().compile("1 + 2;").unwrap();
//! let clif = module.clif();
//! assert!(clif.contains("__script"));
//! ```
//!
//! [`CompileOptions`] enables TypeScript parsing, the ECMAScript 2026
//! transforms, and Cranelift code-generation settings before the AST is
//! lowered.
//!
//! With the `jit` feature on Linux x86-64, `Engine::compile_jit` compiles the
//! same object image and publishes it in process as a `JitProgram`.
//! `Engine::precompile_module` produces a self-contained ELF artifact with
//! resolved internal relocations and runtime metadata; it can be persisted and
//! loaded later with `Engine::deserialize`, `Engine::deserialize_file` or
//! `Engine::deserialize_raw`, or serialized from an already-compiled module
//! with `IrModule::serialize`. Cranelift traps such as temporal-dead-zone
//! violations abort the process; trap recovery is not implemented. The AOT
//! object and executable APIs are unchanged.
//!
//! The 0.6.0 subset supports functions (declarations, expressions, named
//! expressions and arrow functions), closures, variables (`var`, `let` and
//! `const`) with block scoping and a temporal dead zone, default parameters,
//! `if`/`else`, `while`, `do...while`, `for`, `switch`, labelled statements,
//! labelled `break`/`continue`, `debugger`, `return`, `try`/`catch`/`finally`
//! with primitive exceptions, `throw`, the `undefined`/`null` literals, the
//! `Math`, `Number`, `Object.is` and `Date.now` builtins, the global
//! `NaN`/`Infinity` values, optional chaining (`?.` and `?.()`), `delete` on
//! identifiers and non-references, and the following operators:
//!
//! The dynamic (JIT) runtime additionally supports string values, objects and
//! arrays: object and array literals, static and computed property access,
//! property assignment, `delete`, `in`, `Object.is`, `Array.isArray`, `Array(n)`
//! and the common `Array.prototype` methods (`push`, `pop`, `shift`, `unshift`,
//! `join`, `slice`, `concat`, `indexOf`, `includes`, `reverse`, `forEach`, `map`,
//! `filter`, `some`, `every`, `find`, `findIndex` and `reduce`). Objects have no
//! prototypes, `this`, accessors or spread, and nothing is garbage collected.
//!
//! - arithmetic `+ - * / % **`, update `++ --`, and the comma operator,
//! - bitwise `& | ^ ~ << >> >>>`,
//! - logical `&& || ?? !` and the ternary `? :`,
//! - comparison `=== !== == != < > <= >=`,
//! - assignment `=` plus every compound form, including `&&=`, `||=` and `??=`.
//!
//! Values are NaN-boxed `f64`s: numbers are plain IEEE-754, while `undefined`,
//! `null`, booleans, uninitialized bindings and closures live in reserved
//! quiet-NaN tag ranges. Consequences of the current encoding:
//!
//! - `===`/`!==` distinguish the tagged values, so `true === 1` is false.
//! - `==`/`!=` implement strict equality, `null == undefined`, boolean to
//!   number coercion, and string-to-number comparison.
//! - Arithmetic, relational operators, `++`/`--` and Math calls apply
//!   `ToNumber` to their operands (`null + 1` is `1`, `undefined + 1` is NaN).
//! - Bitwise operators convert through `ToInt32` (`undefined | 0` is `0`).
//! - Bitwise results are signed 32-bit values except for `>>>`, as per the spec.
//! - The dynamic (JIT) runtime stores strings as tagged pointers to
//!   `sealion-string` values, so literals, concatenation, comparison, `.length`,
//!   `String(value)` and `console.log` work there. The AOT emitter still rejects
//!   string values because it cannot link the host string runtime.
//!
//! Blocks are real lexical scopes: `let`/`const` shadow their enclosing scope
//! and disappear at the closing brace, while `var` and function declarations
//! hoist to the enclosing function. Scopes that a closure may capture get a
//! `calloc`ed environment record linked to its parent; all other bindings stay
//! in machine registers. A captured `let` in a loop body therefore gets a fresh
//! environment per iteration, while an uncaptured one does not.
//!
//! Consequences of the current environment model:
//!
//! - Captured environments are never freed; a captured block that runs in a
//!   loop allocates once per iteration.
//! - `let`/`const` bindings and parameters start in the temporal dead zone;
//!   reading one before its declaration runs traps instead of yielding
//!   `undefined`.
//! - Named function expressions can refer to themselves by name.
//! - `function` declarations hoist to the enclosing function even when written
//!   in a block, so they cannot see that block's `let`/`const` bindings.
//!
//! `Math`, `Number`, `Object` and `Date` are namespaces, not objects: every
//! member is either a compile-time constant or a builtin function, so
//! `Math.PI` folds to a literal, `Math.sqrt` becomes a libm call and
//! `Math.clz32` a few instructions. Consequences:
//!
//! - The namespaces cannot be used as values (`var m = Math` is rejected), and
//!   only the dot notation reaches them.
//! - A local binding named after a namespace or builtin shadows it for reads,
//!   but `Math.abs(x)` still resolves to the builtin.
//! - `isNaN`/`isFinite` coerce their argument; `Number.isNaN`/`Number.isFinite`
//!   do not. `Boolean(value)` and `Number(value)` are also available.
//! - `Math.round` rounds halves towards `+Infinity`; `Math.min`/`Math.max` keep
//!   `-0`/`NaN` semantics, `Math.fround` rounds through an `f32`, and
//!   `Math.f16round` rounds to half precision. `Math.hypot` scales its
//!   operands, and `Math.random` uses `drand48`.
//! - `Date.now()` returns the current epoch time in milliseconds.
//!
//! Optional chains short-circuit to `undefined` for nullish values. Without
//! objects, non-builtin property reads inside a chain evaluate to `undefined`;
//! `delete property` is still rejected.
//!
//! `throw` accepts any current value. An uncaught exception unwinds every
//! `finally` block and makes the process exit with status `1`.
//!
//! `const` bindings are immutable: assigning to one (or updating it with
//! `++`/`--`) is rejected at compile time. `switch` compares with `===`,
//! evaluates the discriminant once, evaluates case expressions in source order
//! until one matches, and falls through from the matched clause on. A `let` or
//! `const` declared inside a `switch` belongs to the scope of the whole
//! `switch` body.
//!
//! A `for (init; condition; update)` loop opens a scope for its lexical
//! initializer and, like the specification, copies that scope's bindings once
//! per iteration, so a closure created in the body sees the iteration's own
//! value. `continue` still runs the update expression. `do...while` evaluates
//! its condition after the body, and labelled `break`/`continue` can target
//! outer loops or labelled blocks.
//!
//! `undefined` is a non-writable global and `debugger` is accepted as a no-op.
//! `for..of`, `for..in` and prototype-based inheritance are intentionally
//! deferred to later roadmap versions.

mod builtins;
mod compiler;
mod engine;
mod error;
mod fnv;
mod ir;
#[cfg(feature = "jit")]
mod jit;
mod options;
mod plan;
#[cfg(feature = "jit")]
mod runtime;
mod value;

pub use engine::Engine;
pub use error::Error;
pub use ir::{IrFunction, IrModule};
pub use options::{CompileOptions, OptLevel};

#[cfg(feature = "jit")]
pub use jit::JitProgram;
#[cfg(all(feature = "jit", target_os = "linux", target_arch = "x86_64"))]
pub use jit::{
    ARTIFACT_FORMAT_VERSION, BaseFixupRecord, EngineMetadata, FuncKey, FunctionInfo, FunctionLoc,
    ImportRecord, METADATA_SCHEMA_VERSION, Module, ModuleInfo,
};

#[cfg(feature = "jit")]
pub fn jit_supported() -> bool {
    jit::supported()
}

pub fn compile(source: &str) -> Result<IrModule, Error> {
    Engine::new().compile(source)
}
