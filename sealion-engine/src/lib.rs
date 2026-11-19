//! A small JavaScript-to-native compiler for the 0.6.0 roadmap subset.
//!
//! The engine parses JavaScript with `oxc-parser`, lowers the AST to Cranelift IR,
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
//! - `==`/`!=` implement strict equality, `null == undefined`, and boolean to
//!   number coercion; string/object coercions arrive with those types.
//! - Arithmetic, relational operators, `++`/`--` and Math calls apply
//!   `ToNumber` to their operands (`null + 1` is `1`, `undefined + 1` is NaN).
//! - Bitwise operators convert through `ToInt32` (`undefined | 0` is `0`).
//! - Bitwise results are signed 32-bit values except for `>>>`, as per the spec.
//! - String literals are rejected outright; strings arrive with the object work.
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
//! Objects, arrays, strings and `for..in`/`for..of` loops are intentionally
//! deferred to later roadmap versions.

mod builtins;
mod compiler;
mod error;
mod plan;
mod value;

pub use error::Error;

use std::{
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use cranelift_codegen::{
    ir::Function,
    isa,
    settings::{self, Configurable},
};
use cranelift_module::{DataDescription, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use target_lexicon::Triple;

use plan::build_program_plan;

pub struct Engine;

impl Default for Engine {
    fn default() -> Self {
        Self
    }
}

impl Engine {
    pub fn new() -> Self {
        Self
    }

    pub fn compile(&self, source: &str) -> Result<IrModule, Error> {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::script()).parse();
        if let Some(error) = parsed.errors.first() {
            return Err(Error::Parse(error.to_string()));
        }
        if parsed.panicked {
            return Err(Error::Parse("parser aborted".to_string()));
        }

        let program = build_program_plan(&parsed.program)?;

        let triple = Triple::host();
        let isa_builder = isa::lookup(triple).map_err(|e| Error::Codegen(e.to_string()))?;
        let mut flag_builder = settings::builder();
        flag_builder
            .set("is_pic", "true")
            .map_err(|e| Error::Codegen(e.to_string()))?;
        flag_builder
            .set("opt_level", "speed")
            .map_err(|e| Error::Codegen(e.to_string()))?;
        let flags = settings::Flags::new(flag_builder);
        let isa = isa_builder
            .finish(flags)
            .map_err(|e| Error::Codegen(e.to_string()))?;

        let obj_builder = ObjectBuilder::new(isa, "sealion_module", default_libcall_names())?;
        let mut module = ObjectModule::new(obj_builder);
        let builtins = builtins::Builtins::new(&mut module)?;

        let js_sig = compiler::make_js_signature(&module);
        let calloc_sig = compiler::make_calloc_signature(&module);
        let f64_binary_sig = compiler::make_f64_binary_signature(&module);
        let main_sig = compiler::make_main_signature(&module);

        let calloc_id = module.declare_function("calloc", Linkage::Import, &calloc_sig)?;
        let pow_id = module.declare_function("pow", Linkage::Import, &f64_binary_sig)?;
        let fmod_id = module.declare_function("fmod", Linkage::Import, &f64_binary_sig)?;
        let exception_id = module.declare_data("sealion_exception", Linkage::Local, true, false)?;
        let mut exception_data = DataDescription::new();
        exception_data.define_zeroinit(16);
        exception_data.set_align(8);
        module.define_data(exception_id, &exception_data)?;

        let mut func_ids = Vec::with_capacity(program.functions.len());
        for plan in &program.functions {
            let id = module.declare_function(&plan.name, Linkage::Local, &js_sig)?;
            func_ids.push(id);
        }

        let main_id = module.declare_function("main", Linkage::Export, &main_sig)?;

        let imports = compiler::Imports {
            calloc: calloc_id,
            pow: pow_id,
            fmod: fmod_id,
            exception: exception_id,
        };
        let mut functions = compiler::compile_functions(
            &mut module,
            &program,
            &func_ids,
            &js_sig,
            &imports,
            &builtins,
        )?;
        functions.push(compiler::compile_main(
            &mut module,
            func_ids[0],
            main_id,
            &main_sig,
            exception_id,
        )?);

        Ok(IrModule {
            functions,
            module: Some(module),
        })
    }
}

pub struct IrFunction {
    pub name: String,
    pub function: Function,
}

pub struct IrModule {
    functions: Vec<IrFunction>,
    module: Option<ObjectModule>,
}

impl IrModule {
    pub fn functions(&self) -> &[IrFunction] {
        &self.functions
    }

    pub fn clif(&self) -> String {
        let mut out = String::new();
        for function in &self.functions {
            out.push_str(&format!(
                "; {}\n{}\n\n",
                function.name,
                function.function.display()
            ));
        }
        out
    }

    pub fn emit_object(&mut self) -> Result<Vec<u8>, Error> {
        let module = self
            .module
            .take()
            .ok_or_else(|| Error::Codegen("module has already been emitted".to_string()))?;
        let product = module.finish();
        product.emit().map_err(|e| Error::Codegen(e.to_string()))
    }

    pub fn emit_binary(&mut self, path: &Path) -> Result<(), Error> {
        let object = self.emit_object()?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let object_path =
            std::env::temp_dir().join(format!("sealion_{}_{}.o", std::process::id(), nanos));
        std::fs::write(&object_path, object)?;

        let mut command = Command::new("cc");
        let output = command
            .arg("-o")
            .arg(path)
            .arg(&object_path)
            .arg("-lm")
            .output()?;

        let _ = std::fs::remove_file(&object_path);

        if !output.status.success() {
            return Err(Error::Linker(
                String::from_utf8_lossy(&output.stderr).into(),
            ));
        }
        Ok(())
    }
}

pub fn compile(source: &str) -> Result<IrModule, Error> {
    Engine::new().compile(source)
}
