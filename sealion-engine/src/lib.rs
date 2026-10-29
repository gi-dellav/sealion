//! A small JavaScript-to-native compiler for the 0.1.0 roadmap subset.
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
//! The 0.1.0 subset supports functions (named declarations), variables,
//! arithmetic, `if`/`else`, `while`, `return`, and closures. Values are
//! represented as `f64`; closures are heap pointers stored in `f64` values.
//! Block scoping, anonymous/arrow functions, objects, arrays, and strings are
//! intentionally deferred to later roadmap versions.

mod compiler;
mod error;
mod plan;

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

        let plans = build_program_plan(&script)?;

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
        let pow_sig = compiler::make_pow_signature(&module);
        let main_sig = compiler::make_main_signature(&module);

        let calloc_id = module.declare_function("calloc", Linkage::Import, &calloc_sig)?;
        let pow_id = module.declare_function("pow", Linkage::Import, &pow_sig)?;

        let mut func_ids = Vec::with_capacity(plans.len());
        for plan in &plans {
            let id = module.declare_function(&plan.name, Linkage::Local, &js_sig)?;
            func_ids.push(id);
        }

        let main_id = module.declare_function("main", Linkage::Export, &main_sig)?;

        let mut functions = compiler::compile_functions(
            &mut module,
            &plans,
            &func_ids,
            &js_sig,
            calloc_id,
            pow_id,
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
