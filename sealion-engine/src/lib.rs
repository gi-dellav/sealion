//! A small JavaScript-to-native compiler for the 0.4.0 roadmap subset.
//!
//! The engine parses JavaScript with `boa-parser`, lowers the AST to Cranelift IR,
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
//! The 0.4.0 subset supports functions (declarations, expressions and arrow
//! functions), closures, variables with block scoping, `if`/`else`, `while`,
//! `break`/`continue`, `return`, the `undefined`/`null` literals, and the
//! following operators:
//!
//! - arithmetic `+ - * / % **`, update `++ --`, and the comma operator,
//! - bitwise `& | ^ ~ << >> >>>`,
//! - logical `&& || ?? !` and the ternary `? :`,
//! - comparison `=== !== == != < > <= >=`,
//! - assignment `=` plus every compound form, including `&&=`, `||=` and `??=`.
//!
//! Values are NaN-boxed `f64`s: numbers are plain IEEE-754, while `undefined`,
//! `null` and closures live in reserved quiet-NaN tag ranges.
//! Consequences of the current encoding:
//!
//! - `===`/`!==` distinguish the tagged values, but booleans are still plain
//!   `0`/`1` numbers, so `true === 1` holds.
//! - `==`/`!=` implement strict equality plus `null == undefined`; the coercions
//!   that need strings or objects arrive with those types.
//! - Arithmetic on tagged values yields a NaN rather than coercing, so
//!   `undefined + 1` is not `NaN` by spec rules yet, while the bitwise operators
//!   do convert through `ToInt32` (`undefined | 0` is `0`).
//! - Bitwise results are signed 32-bit values except for `>>>`, as per the spec.
//! - String literals are rejected outright; strings arrive with the object work.
//!
//! Blocks are real lexical scopes: `let`/`const` shadow their enclosing scope
//! and disappear at the closing brace, while `var` and function declarations
//! hoist to the enclosing function. Each scope that owns bindings gets its own
//! `calloc`ed environment record linked to its parent, so a closure captures
//! whatever environment is current where the expression is evaluated (and a
//! `let` inside a loop body yields a fresh environment per iteration).
//!
//! Consequences of the current environment model:
//!
//! - Environments are never freed; a block that runs in a loop allocates once
//!   per iteration.
//! - There is no temporal dead zone: a block-scoped binding reads as
//!   `undefined` before its declaration executes.
//! - Named function expressions ignore their inner name, so recursion has to go
//!   through the assigned outer variable.
//! - `function` declarations hoist to the enclosing function even when written
//!   in a block, so they cannot see that block's `let`/`const` bindings.
//!
//! `switch`, objects, arrays, and strings are intentionally deferred to later
//! roadmap versions.

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

use boa_ast::scope::Scope;
use boa_interner::Interner;
use boa_parser::{Parser, Source};
use cranelift_codegen::{
    ir::Function,
    isa,
    settings::{self, Configurable},
};
use cranelift_module::{Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
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
        let mut interner = Interner::default();
        let scope = Scope::new_global();
        let mut parser = Parser::new(Source::from_bytes(source));
        let script = parser.parse_script(&scope, &mut interner)?;

        let undefined_sym = interner.get_or_intern("undefined");
        let program = build_program_plan(&script, undefined_sym)?;

        let triple = Triple::host();
        let isa_builder = isa::lookup(triple).map_err(|e| Error::Codegen(e.to_string()))?;
        let mut flag_builder = settings::builder();
        flag_builder
            .set("is_pic", "true")
            .map_err(|e| Error::Codegen(e.to_string()))?;
        let flags = settings::Flags::new(flag_builder);
        let isa = isa_builder
            .finish(flags)
            .map_err(|e| Error::Codegen(e.to_string()))?;

        let obj_builder = ObjectBuilder::new(isa, "sealion_module", default_libcall_names())?;
        let mut module = ObjectModule::new(obj_builder);

        let js_sig = compiler::make_js_signature(&module);
        let calloc_sig = compiler::make_calloc_signature(&module);
        let f64_binary_sig = compiler::make_f64_binary_signature(&module);
        let main_sig = compiler::make_main_signature(&module);

        let calloc_id = module.declare_function("calloc", Linkage::Import, &calloc_sig)?;
        let pow_id = module.declare_function("pow", Linkage::Import, &f64_binary_sig)?;
        let fmod_id = module.declare_function("fmod", Linkage::Import, &f64_binary_sig)?;

        let mut func_ids = Vec::with_capacity(program.functions.len());
        for plan in &program.functions {
            let id = module.declare_function(&plan.name, Linkage::Local, &js_sig)?;
            func_ids.push(id);
        }

        let main_id = module.declare_function("main", Linkage::Export, &main_sig)?;

        let mut functions = compiler::compile_functions(
            &mut module,
            &program,
            &func_ids,
            &js_sig,
            calloc_id,
            pow_id,
            fmod_id,
        )?;
        functions.push(compiler::compile_main(
            &mut module,
            func_ids[0],
            main_id,
            &main_sig,
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
