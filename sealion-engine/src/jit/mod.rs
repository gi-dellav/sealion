#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::str::FromStr;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use target_lexicon::Triple;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use crate::CompileOptions;
use crate::{Error, IrFunction};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod arch;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod arrays;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod artifact;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod dates;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod functions;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod iter;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod loader;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod maps;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod math;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod objects;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod os;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod strings;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub use artifact::{
    ARTIFACT_FORMAT_VERSION, BaseFixupRecord, EngineMetadata, FuncKey, FunctionInfo, FunctionLoc,
    ImportRecord, METADATA_SCHEMA_VERSION, Module, ModuleInfo,
};

pub struct JitProgram {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    module: artifact::Module,
    functions: Vec<IrFunction>,
}

impl JitProgram {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    pub(crate) fn from_parts(module: artifact::Module, functions: Vec<IrFunction>) -> Self {
        Self { module, functions }
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

    pub fn functions(&self) -> &[IrFunction] {
        &self.functions
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    pub fn module(&self) -> &artifact::Module {
        &self.module
    }

    /// The artifact bytes that produced this program.
    pub fn serialize(&self) -> Result<Vec<u8>, Error> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            self.module.serialize()
        }
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            Err(Error::Unsupported(
                "JIT is only supported on Linux x86-64".to_string(),
            ))
        }
    }

    pub fn run(&self) -> i32 {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            self.module.run()
        }
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            unreachable!("JIT is not supported on this platform")
        }
    }
}

pub(crate) fn supported() -> bool {
    cfg!(all(target_os = "linux", target_arch = "x86_64"))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) fn check_target(options: &CompileOptions) -> Result<(), Error> {
    let host = Triple::host();
    let requested = match &options.target {
        Some(target) => Triple::from_str(target)
            .map_err(|e| Error::Codegen(format!("invalid target triple '{target}': {e}")))?,
        None => host.clone(),
    };

    let compatible = requested.architecture == host.architecture
        && requested.operating_system == host.operating_system
        && requested.environment == host.environment
        && requested.binary_format == host.binary_format
        && requested.endianness().ok() == host.endianness().ok()
        && requested.pointer_width().ok() == host.pointer_width().ok();

    if !compatible {
        return Err(Error::Loader(format!(
            "in-process JIT cannot cross-compile: requested {requested}, host {host}"
        )));
    }

    Ok(())
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
pub(crate) fn load(artifact: &[u8]) -> Result<Module, Error> {
    artifact::load(artifact)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) fn build(
    object: &[u8],
    options: &CompileOptions,
    target: &Triple,
    source_checksum: u64,
) -> Result<Vec<u8>, Error> {
    artifact::build(object, options, target, source_checksum)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) fn build_module(
    object: &[u8],
    options: &CompileOptions,
    target: &Triple,
    source_checksum: u64,
) -> Result<Module, Error> {
    artifact::build_module(object, options, target, source_checksum)
}
