use std::str::FromStr;

use target_lexicon::Triple;

use crate::{CompileOptions, Error, IrFunction};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod arch;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod loader;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod os;

pub struct JitProgram {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    image: loader::LoadedImage<os::LinuxMemory>,
    functions: Vec<IrFunction>,
}

impl JitProgram {
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

    pub fn run(&self) -> i32 {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            let main: unsafe extern "C" fn(i32, i64) -> i32 =
                unsafe { std::mem::transmute(self.image.main()) };
            unsafe { main(0, 0) }
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
        return Err(Error::Jit(format!(
            "in-process JIT cannot cross-compile: requested {requested}, host {host}"
        )));
    }

    Ok(())
}

pub(crate) fn load(object: &[u8], functions: Vec<IrFunction>) -> Result<JitProgram, Error> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let image = loader::load::<arch::X86_64, os::Linux>(object)?;
        Ok(JitProgram { image, functions })
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = (object, functions);
        Err(Error::Unsupported(
            "JIT is only supported on Linux x86-64".to_string(),
        ))
    }
}
