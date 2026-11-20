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
    str::FromStr,
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
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_transformer::{
    ES2026Options, EnvOptions, TransformOptions, Transformer, TypeScriptOptions,
};
use target_lexicon::Triple;

use plan::build_program_plan;

/// Cranelift optimization level for [`CompileOptions::opt_level`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptLevel {
    /// Minimize compile time by disabling most optimizations.
    None,
    /// Generate the fastest possible code.
    #[default]
    Speed,
    /// Like [`OptLevel::Speed`], but also reduce code size.
    SpeedAndSize,
}

impl OptLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Speed => "speed",
            Self::SpeedAndSize => "speed_and_size",
        }
    }
}

/// Language, edition and code-generation options for
/// [`Engine::compile_with_options`].
#[derive(Debug, Clone)]
pub struct CompileOptions {
    /// Parse the source as TypeScript and remove type syntax before lowering.
    pub typescript: bool,
    /// Apply the ECMAScript 2026 transforms before lowering.
    pub es2026: bool,
    /// Cranelift optimization level.
    pub opt_level: OptLevel,
    /// Enable redundant-load optimizations with alias analysis.
    pub enable_alias_analysis: bool,
    /// Run the Cranelift IR verifier during compilation.
    pub enable_verifier: bool,
    /// Target triple to compile for; defaults to the host.
    pub target: Option<String>,
    /// Target-specific CPU preset, for example `x86-64-v3` or `znver4`.
    pub target_cpu: Option<String>,
    /// Emit position-independent code.
    pub is_pic: bool,
    /// Generate unwind information.
    pub unwind_info: bool,
    /// Preserve frame pointers in generated functions.
    pub preserve_frame_pointers: bool,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            typescript: false,
            es2026: false,
            opt_level: OptLevel::Speed,
            enable_alias_analysis: true,
            enable_verifier: true,
            target: None,
            target_cpu: None,
            is_pic: true,
            unwind_info: true,
            preserve_frame_pointers: false,
        }
    }
}

fn set_flag(
    builder: &mut settings::Builder,
    name: &str,
    value: impl std::fmt::Display,
) -> Result<(), Error> {
    let value = value.to_string();
    builder.set(name, &value).map_err(|e| {
        Error::Codegen(format!(
            "failed to set Cranelift setting '{name}' to '{value}': {e}"
        ))
    })
}

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
        self.compile_with_options(source, &CompileOptions::default())
    }

    pub fn compile_with_options(
        &self,
        source: &str,
        options: &CompileOptions,
    ) -> Result<IrModule, Error> {
        let allocator = Allocator::default();
        let source_type = if options.typescript {
            SourceType::ts()
        } else {
            SourceType::script()
        };
        let parsed = Parser::new(&allocator, source, source_type).parse();
        if let Some(error) = parsed.diagnostics.first() {
            return Err(Error::Parse(error.to_string()));
        }
        if parsed.fatal_error {
            return Err(Error::Parse("parser aborted".to_string()));
        }

        let mut program = parsed.program;

        if options.typescript || options.es2026 {
            let semantic = SemanticBuilder::new()
                .with_excess_capacity(2.0)
                .build(&program);
            if let Some(error) = semantic.diagnostics.first() {
                return Err(Error::Parse(error.to_string()));
            }

            let transform_options = TransformOptions {
                typescript: TypeScriptOptions::default(),
                env: EnvOptions {
                    es2026: ES2026Options {
                        explicit_resource_management: options.es2026,
                    },
                    ..EnvOptions::default()
                },
                ..TransformOptions::default()
            };
            let source_path = Path::new(if options.typescript {
                "input.ts"
            } else {
                "input.js"
            });
            let transformed = Transformer::new(&allocator, source_path, &transform_options)
                .build_with_scoping(semantic.semantic.into_scoping(), &mut program);
            if let Some(error) = transformed.diagnostics.first() {
                return Err(Error::Parse(error.to_string()));
            }
        }

        let program = build_program_plan(&program)?;

        let triple = match &options.target {
            Some(target) => Triple::from_str(target)
                .map_err(|e| Error::Codegen(format!("invalid target triple '{target}': {e}")))?,
            None => Triple::host(),
        };
        let mut isa_builder = isa::lookup(triple).map_err(|e| Error::Codegen(e.to_string()))?;
        let mut flag_builder = settings::builder();
        set_flag(&mut flag_builder, "is_pic", options.is_pic)?;
        set_flag(&mut flag_builder, "opt_level", options.opt_level.as_str())?;
        set_flag(
            &mut flag_builder,
            "enable_alias_analysis",
            options.enable_alias_analysis,
        )?;
        set_flag(
            &mut flag_builder,
            "enable_verifier",
            options.enable_verifier,
        )?;
        set_flag(&mut flag_builder, "unwind_info", options.unwind_info)?;
        set_flag(
            &mut flag_builder,
            "preserve_frame_pointers",
            options.preserve_frame_pointers,
        )?;
        if let Some(cpu) = &options.target_cpu {
            isa_builder
                .enable(cpu)
                .map_err(|e| Error::Codegen(format!("invalid target CPU '{cpu}': {e}")))?;
        }
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
            is_pic: options.is_pic,
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
    is_pic: bool,
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
        command.arg("-o").arg(path).arg(&object_path).arg("-lm");
        #[cfg(target_os = "linux")]
        if !self.is_pic {
            command.arg("-no-pie");
        }
        let output = command.output()?;

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
