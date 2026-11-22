use std::{
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use cranelift_codegen::ir::Function;
use cranelift_object::ObjectModule;
use target_lexicon::Triple;

use crate::{CompileOptions, Error};

#[cfg(feature = "jit")]
use crate::jit;

pub struct IrFunction {
    pub name: String,
    pub function: Function,
}

#[cfg_attr(
    not(all(feature = "jit", target_os = "linux", target_arch = "x86_64")),
    allow(dead_code)
)]
pub struct IrModule {
    functions: Vec<IrFunction>,
    module: Option<ObjectModule>,
    is_pic: bool,
    options: CompileOptions,
    target: Triple,
    source_checksum: u64,
}

impl IrModule {
    pub(crate) fn new(
        functions: Vec<IrFunction>,
        module: Option<ObjectModule>,
        is_pic: bool,
        options: CompileOptions,
        target: Triple,
        source_checksum: u64,
    ) -> Self {
        Self {
            functions,
            module,
            is_pic,
            options,
            target,
            source_checksum,
        }
    }

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

    #[cfg(all(feature = "jit", target_os = "linux", target_arch = "x86_64"))]
    pub(crate) fn into_functions(self) -> Vec<IrFunction> {
        self.functions
    }

    /// Serialize this compiled module as a self-contained artifact.
    ///
    /// The image is pre-linked and its internal relocations are resolved, so
    /// loading it does not need a compiler or a linker.
    #[cfg(all(feature = "jit", target_os = "linux", target_arch = "x86_64"))]
    pub fn serialize(&mut self) -> Result<Vec<u8>, Error> {
        let object = self.emit_object()?;
        jit::build(&object, &self.options, &self.target, self.source_checksum)
    }

    #[cfg(all(feature = "jit", not(all(target_os = "linux", target_arch = "x86_64"))))]
    pub fn serialize(&mut self) -> Result<Vec<u8>, Error> {
        Err(Error::Unsupported(
            "artifact serialization is only supported on Linux x86-64".to_string(),
        ))
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
