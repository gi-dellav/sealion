use std::{
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{SystemTime, UNIX_EPOCH},
};

use clap::{Parser, ValueEnum};
use sealion_engine::{CompileOptions, Engine, OptLevel};

/// Run or build the sealion 0.6.0 JavaScript subset.
#[derive(Debug, Parser)]
#[command(name = "sealionjs", version, about)]
struct Cli {
    /// JavaScript or TypeScript source file to compile.
    #[arg(required_unless_present = "run_artifact")]
    input: Option<PathBuf>,

    /// Write a native executable to this path instead of running it.
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Write a Cranelift object file to this path.
    #[arg(long, value_name = "PATH")]
    object: Option<PathBuf>,

    /// Write a precompiled artifact to this path instead of running.
    #[arg(long, value_name = "PATH", alias = "precompile", conflicts_with_all = ["output", "object"])]
    artifact: Option<PathBuf>,

    /// Load and run a precompiled artifact.
    #[arg(
        long,
        value_name = "PATH",
        alias = "load-artifact",
        conflicts_with = "artifact"
    )]
    run_artifact: Option<PathBuf>,

    /// Print Cranelift IR to stdout.
    #[arg(long)]
    emit_ir: bool,

    /// Do not run the compiled program.
    #[arg(long)]
    no_run: bool,

    /// Parse the input as TypeScript and remove type syntax.
    #[arg(short = 't', long)]
    ts: bool,

    /// Apply the ECMAScript 2026 transforms before compiling.
    #[arg(long)]
    es2026: bool,

    /// Set the Cranelift optimization level.
    #[arg(
        short = 'O',
        long,
        value_enum,
        value_name = "LEVEL",
        default_value = "speed"
    )]
    opt_level: OptLevelArg,

    /// Disable redundant-load optimizations with alias analysis.
    #[arg(long)]
    no_alias_analysis: bool,

    /// Disable the Cranelift IR verifier.
    #[arg(long)]
    no_verifier: bool,

    /// Compile for this target triple instead of the host.
    #[arg(long, value_name = "TRIPLE")]
    target: Option<String>,

    /// Enable a target-specific CPU preset (for example x86-64-v3 or znver4).
    #[arg(long, value_name = "PRESET")]
    target_cpu: Option<String>,

    /// Emit non-position-independent code.
    #[arg(long)]
    no_pic: bool,

    /// Do not generate unwind information.
    #[arg(long)]
    no_unwind_info: bool,

    /// Preserve frame pointers in generated functions.
    #[arg(long)]
    preserve_frame_pointers: bool,

    /// Do not read or write the on-disk artifact cache used by the JIT.
    #[arg(long)]
    no_cache: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OptLevelArg {
    None,
    Speed,
    #[value(name = "speed_and_size", alias = "speed-and-size")]
    SpeedAndSize,
}

impl From<OptLevelArg> for OptLevel {
    fn from(value: OptLevelArg) -> Self {
        match value {
            OptLevelArg::None => Self::None,
            OptLevelArg::Speed => Self::Speed,
            OptLevelArg::SpeedAndSize => Self::SpeedAndSize,
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("sealionjs: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    let engine = Engine::new();

    #[cfg(feature = "jit")]
    if let Some(path) = &cli.run_artifact {
        if cli.emit_ir || cli.object.is_some() || cli.output.is_some() {
            return Err("--run-artifact cannot be combined with compile-only flags".to_string());
        }
        let module = engine
            .deserialize_file(path)
            .map_err(|e| format!("failed to load {}: {e}", path.display()))?;
        return Ok(ExitCode::from((module.run() & 0xff) as u8));
    }
    #[cfg(not(feature = "jit"))]
    if cli.run_artifact.is_some() {
        return Err("--run-artifact requires the jit feature".to_string());
    }

    let input = cli
        .input
        .as_ref()
        .ok_or_else(|| "missing input source file".to_string())?;
    let source = fs::read_to_string(input)
        .map_err(|e| format!("failed to read {}: {e}", input.display()))?;

    let options = CompileOptions {
        typescript: cli.ts,
        es2026: cli.es2026,
        opt_level: cli.opt_level.into(),
        enable_alias_analysis: !cli.no_alias_analysis,
        enable_verifier: !cli.no_verifier,
        target: cli.target.clone(),
        target_cpu: cli.target_cpu.clone(),
        is_pic: !cli.no_pic,
        unwind_info: !cli.no_unwind_info,
        preserve_frame_pointers: cli.preserve_frame_pointers,
    };

    if cli.emit_ir {
        let ir = engine
            .compile_with_options(&source, &options)
            .map_err(|e| format!("failed to compile {}: {e}", input.display()))?;
        print!("{}", ir.clif());
    }

    #[cfg(feature = "jit")]
    if let Some(artifact_path) = &cli.artifact {
        let bytes = engine
            .precompile_module_with_options(&source, &options)
            .map_err(|e| format!("failed to precompile {}: {e}", input.display()))?;
        fs::write(artifact_path, bytes)
            .map_err(|e| format!("failed to write {}: {e}", artifact_path.display()))?;
        return Ok(ExitCode::SUCCESS);
    }
    #[cfg(not(feature = "jit"))]
    if cli.artifact.is_some() {
        return Err("--artifact requires the jit feature".to_string());
    }

    if let Some(object_path) = &cli.object {
        let mut ir = engine
            .compile_with_options(&source, &options)
            .map_err(|e| format!("failed to compile {}: {e}", input.display()))?;
        let object = ir
            .emit_object()
            .map_err(|e| format!("failed to emit object file: {e}"))?;
        fs::write(object_path, object)
            .map_err(|e| format!("failed to write {}: {e}", object_path.display()))?;
    }

    if let Some(output_path) = &cli.output {
        let mut ir = engine
            .compile_with_options(&source, &options)
            .map_err(|e| format!("failed to compile {}: {e}", input.display()))?;
        ir.emit_binary(output_path)
            .map_err(|e| format!("failed to emit binary: {e}"))?;
    }

    if cli.no_run || cli.object.is_some() || cli.output.is_some() {
        return Ok(ExitCode::SUCCESS);
    }

    #[cfg(feature = "jit")]
    if use_jit(&cli) {
        warn_ignored_jit_flags(&cli);
        let cache = if cli.no_cache {
            None
        } else {
            artifact_cache_path(&engine, &source, &options)
        };
        if let Some(path) = &cache
            && let Some(module) = load_cached_artifact(&engine, path)
        {
            return Ok(ExitCode::from((module.run() & 0xff) as u8));
        }
        let program = engine
            .compile_jit_with_options(&source, &options)
            .map_err(|e| format!("failed to compile {}: {e}", input.display()))?;
        if let Some(path) = &cache
            && let Ok(bytes) = program.serialize()
        {
            store_cached_artifact(path, &bytes);
        }
        return Ok(ExitCode::from((program.run() & 0xff) as u8));
    }

    let mut ir = engine
        .compile_with_options(&source, &options)
        .map_err(|e| format!("failed to compile {}: {e}", input.display()))?;
    let binary_path = temporary_executable_path();
    ir.emit_binary(&binary_path)
        .map_err(|e| format!("failed to emit binary: {e}"))?;

    let status = Command::new(&binary_path).status();
    let _ = fs::remove_file(&binary_path);

    let status = status.map_err(|e| format!("failed to run generated binary: {e}"))?;
    Ok(ExitCode::from(
        u8::try_from(status.code().unwrap_or(1)).unwrap_or(1),
    ))
}

#[cfg(feature = "jit")]
fn use_jit(cli: &Cli) -> bool {
    if !sealion_engine::jit_supported() {
        return false;
    }
    if cli.emit_ir || cli.object.is_some() || cli.output.is_some() {
        return false;
    }
    if cli.target.is_some() {
        eprintln!("sealionjs: warning: explicit --target disables the in-process JIT; using AOT");
        return false;
    }
    true
}

#[cfg(feature = "jit")]
fn warn_ignored_jit_flags(cli: &Cli) {
    if cli.no_pic {
        eprintln!(
            "sealionjs: warning: --no-pic is ignored by the in-process JIT; non-PIC code is always used"
        );
    }
    if cli.no_unwind_info {
        eprintln!(
            "sealionjs: warning: --no-unwind-info is ignored by the in-process JIT; unwind information is not registered"
        );
    }
}

#[cfg(feature = "jit")]
fn artifact_cache_path(engine: &Engine, source: &str, options: &CompileOptions) -> Option<PathBuf> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    options.typescript.hash(&mut hasher);
    options.es2026.hash(&mut hasher);
    options.enable_alias_analysis.hash(&mut hasher);
    options.enable_verifier.hash(&mut hasher);
    engine
        .precompile_compatibility_hash(options)
        .hash(&mut hasher);
    // Tie entries to this build so a rebuilt compiler never reuses stale code.
    if let Ok(path) = std::env::current_exe()
        && let Ok(metadata) = fs::metadata(path)
    {
        metadata.len().hash(&mut hasher);
        if let Ok(modified) = metadata.modified()
            && let Ok(elapsed) = modified.duration_since(UNIX_EPOCH)
        {
            elapsed.as_nanos().hash(&mut hasher);
        }
    }
    let key = hasher.finish();
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
    Some(
        base.join("sealion")
            .join("artifacts")
            .join(format!("{key:016x}.sealion")),
    )
}

#[cfg(feature = "jit")]
fn load_cached_artifact(engine: &Engine, path: &Path) -> Option<sealion_engine::Module> {
    let bytes = fs::read(path).ok()?;
    match engine.deserialize(&bytes) {
        Ok(module) => Some(module),
        Err(_) => {
            let _ = fs::remove_file(path);
            None
        }
    }
}

#[cfg(feature = "jit")]
fn store_cached_artifact(path: &Path, bytes: &[u8]) {
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let temp = path.with_extension(format!("tmp{}", std::process::id()));
    if fs::write(&temp, bytes).is_ok() {
        let _ = fs::rename(&temp, path);
    } else {
        let _ = fs::remove_file(&temp);
    }
}

fn temporary_executable_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut name = format!("sealionjs_{}_{}", std::process::id(), nanos);
    if cfg!(windows) {
        name.push_str(".exe");
    }
    std::env::temp_dir().join(name)
}
