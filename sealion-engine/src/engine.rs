use std::{collections::HashMap, path::Path, str::FromStr};

#[cfg(feature = "jit")]
use std::sync::Arc;

use cranelift_codegen::{
    isa,
    settings::{self, Configurable},
};
use cranelift_module::{
    DataDescription, Linkage, Module as CraneliftModule, default_libcall_names,
};
use cranelift_object::{ObjectBuilder, ObjectModule};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_transformer::{
    ES2026Options, EnvOptions, TransformOptions, Transformer, TypeScriptOptions,
};
use target_lexicon::Triple;

use crate::{CompileOptions, Error, IrModule, builtins, compiler, plan::build_program_plan};

#[cfg(feature = "jit")]
use crate::{
    JitProgram,
    fnv::{FNV_OFFSET, fnv1a},
    jit,
    runtime::{FuncType, RegisteredType, RuntimeState},
};

#[cfg(all(feature = "jit", target_os = "linux", target_arch = "x86_64"))]
fn needs_dynamic_runtime(message: &str) -> bool {
    message.contains("dynamic runtime") || message.contains("string length")
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

#[derive(Clone)]
pub struct Engine {
    #[cfg(feature = "jit")]
    #[cfg_attr(not(test), allow(dead_code))]
    runtime: Arc<RuntimeState>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "jit")]
            runtime: Arc::new(RuntimeState::default()),
        }
    }

    #[cfg(feature = "jit")]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn register_type(&self, ty: FuncType) -> Result<RegisteredType, Error> {
        self.runtime.types.register(ty)
    }

    pub fn compile(&self, source: &str) -> Result<IrModule, Error> {
        self.compile_with_options(source, &CompileOptions::default())
    }

    pub fn compile_with_options(
        &self,
        source: &str,
        options: &CompileOptions,
    ) -> Result<IrModule, Error> {
        self.compile_with_options_internal(source, options, false, false)
    }

    #[cfg(feature = "jit")]
    pub fn compile_jit(&self, source: &str) -> Result<JitProgram, Error> {
        self.compile_jit_with_options(source, &CompileOptions::default())
    }

    #[cfg(all(feature = "jit", target_os = "linux", target_arch = "x86_64"))]
    pub fn compile_jit_with_options(
        &self,
        source: &str,
        options: &CompileOptions,
    ) -> Result<JitProgram, Error> {
        jit::check_target(options)?;
        let ir = self.compile_runtime_module(source, options)?;
        let (module, functions) = ir.into_jit_parts()?;
        Ok(JitProgram::from_parts(module, functions))
    }

    #[cfg(all(feature = "jit", not(all(target_os = "linux", target_arch = "x86_64"))))]
    pub fn compile_jit_with_options(
        &self,
        _source: &str,
        _options: &CompileOptions,
    ) -> Result<JitProgram, Error> {
        Err(Error::Unsupported(
            "JIT is only supported on Linux x86-64".to_string(),
        ))
    }

    /// Compile `source` directly to a precompiled artifact.
    ///
    /// This is the build-pipeline entry point: it does not execute or publish
    /// the module. The resulting bytes can be persisted and later loaded with
    /// [`Engine::deserialize`], [`Engine::deserialize_file`] or
    /// [`Engine::deserialize_raw`].
    #[cfg(feature = "jit")]
    pub fn precompile_module(&self, source: &str) -> Result<Vec<u8>, Error> {
        self.precompile_module_with_options(source, &CompileOptions::default())
    }

    /// Compile `source` to a precompiled artifact with explicit options.
    #[cfg(all(feature = "jit", target_os = "linux", target_arch = "x86_64"))]
    pub fn precompile_module_with_options(
        &self,
        source: &str,
        options: &CompileOptions,
    ) -> Result<Vec<u8>, Error> {
        jit::check_target(options)?;
        let mut ir = self.compile_runtime_module(source, options)?;
        ir.serialize()
    }

    #[cfg(all(feature = "jit", not(all(target_os = "linux", target_arch = "x86_64"))))]
    pub fn precompile_module_with_options(
        &self,
        _source: &str,
        _options: &CompileOptions,
    ) -> Result<Vec<u8>, Error> {
        Err(Error::Unsupported(
            "precompilation is only supported on Linux x86-64".to_string(),
        ))
    }

    /// Hash the settings that make two engines artifact-compatible.
    ///
    /// This is a cheaper check than loading an artifact. It covers the target,
    /// ABI-affecting compiler settings, the feature bits required by the image,
    /// and the engine version.
    #[cfg(feature = "jit")]
    pub fn precompile_compatibility_hash(&self, options: &CompileOptions) -> u64 {
        let mut hash = FNV_OFFSET;
        let target = options
            .target
            .clone()
            .unwrap_or_else(|| Triple::host().to_string());
        fnv1a(&mut hash, target.as_bytes());
        fnv1a(&mut hash, options.opt_level.as_str().as_bytes());
        fnv1a(
            &mut hash,
            options.target_cpu.as_deref().unwrap_or("").as_bytes(),
        );
        fnv1a(&mut hash, &[options.is_pic as u8]);
        fnv1a(&mut hash, &[options.unwind_info as u8]);
        fnv1a(&mut hash, &[options.preserve_frame_pointers as u8]);
        fnv1a(&mut hash, &[options.enable_alias_analysis as u8]);
        fnv1a(&mut hash, &[options.enable_verifier as u8]);
        fnv1a(&mut hash, env!("CARGO_PKG_VERSION").as_bytes());
        hash
    }

    /// Compile for the in-process runtime, preferring the static value model
    /// and falling back to the dynamic runtime only when the source needs it.
    #[cfg(all(feature = "jit", target_os = "linux", target_arch = "x86_64"))]
    fn compile_runtime_module(
        &self,
        source: &str,
        options: &CompileOptions,
    ) -> Result<IrModule, Error> {
        match self.compile_with_options_internal(source, options, true, false) {
            Err(Error::Unsupported(message)) if needs_dynamic_runtime(&message) => {
                self.compile_with_options_internal(source, options, true, true)
            }
            result => result,
        }
    }

    fn compile_with_options_internal(
        &self,
        source: &str,
        options: &CompileOptions,
        force_non_pic: bool,
        dynamic: bool,
    ) -> Result<IrModule, Error> {
        let mut effective = options.clone();
        if force_non_pic {
            effective.is_pic = false;
            // The in-process loader does not register unwind information.
            effective.unwind_info = false;
        }
        let options = &effective;
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

        let target = match &options.target {
            Some(target) => Triple::from_str(target)
                .map_err(|e| Error::Codegen(format!("invalid target triple '{target}': {e}")))?,
            None => Triple::host(),
        };
        let mut isa_builder =
            isa::lookup(target.clone()).map_err(|e| Error::Codegen(e.to_string()))?;
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
        let builtins = builtins::Builtins::new(&mut module, dynamic)?;

        let mut string_ids = HashMap::new();
        let mut key_ids = HashMap::new();
        if dynamic {
            for (index, text) in program.strings.iter().enumerate() {
                let name = format!("__string_{index}");
                let id = module.declare_data(&name, Linkage::Local, true, false)?;
                let mut data = DataDescription::new();
                data.define(text.as_bytes().to_vec().into_boxed_slice());
                data.set_align(1);
                module.define_data(id, &data)?;
                string_ids.insert(text.clone(), id);

                // One writable word per literal holds its interned value once
                // the program has materialised it, so a literal used in a loop
                // is resolved without probing the static string table again.
                let slot = module.declare_data(
                    &format!("__string_key_{index}"),
                    Linkage::Local,
                    true,
                    false,
                )?;
                let mut data = DataDescription::new();
                data.define_zeroinit(8);
                data.set_align(8);
                module.define_data(slot, &data)?;
                key_ids.insert(text.clone(), slot);
            }
        }

        let inline_cache_slots = compiler::inline_cache_slots(program.strings.len());
        let inline_caches = if dynamic {
            let id = module.declare_data("sealion_inline_caches", Linkage::Local, true, false)?;
            let mut data = DataDescription::new();
            data.define_zeroinit(compiler::inline_cache_bytes(inline_cache_slots));
            data.set_align(8);
            module.define_data(id, &data)?;
            Some(id)
        } else {
            None
        };

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
            inline_caches,
            inline_cache_slots,
        };
        let mut functions = compiler::compile_functions(
            &mut module,
            &program,
            &func_ids,
            &js_sig,
            &imports,
            &builtins,
            &string_ids,
            &key_ids,
            dynamic,
        )?;
        functions.push(compiler::compile_main(
            &mut module,
            func_ids[0],
            main_id,
            &main_sig,
            exception_id,
            &builtins,
            dynamic,
        )?);

        Ok(IrModule::new(
            functions,
            Some(module),
            options.is_pic,
            options.clone(),
            target,
            crate::fnv::source_checksum(source),
        ))
    }
}
