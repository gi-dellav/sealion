use std::{
    fs,
    path::PathBuf,
    process::{Command, ExitCode},
    time::{SystemTime, UNIX_EPOCH},
};

use clap::Parser;
use sealion_engine::Engine;

/// Run or build the sealion 0.1.0 JavaScript subset.
#[derive(Debug, Parser)]
#[command(name = "sealionjs", version, about)]
struct Cli {
    /// JavaScript source file to compile.
    input: PathBuf,

    /// Write a native executable to this path instead of running it.
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Write a Cranelift object file to this path.
    #[arg(long, value_name = "PATH")]
    object: Option<PathBuf>,

    /// Print Cranelift IR to stdout.
    #[arg(long)]
    emit_ir: bool,

    /// Do not run the compiled program.
    #[arg(long)]
    no_run: bool,
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
    let source = fs::read_to_string(&cli.input)
        .map_err(|e| format!("failed to read {}: {e}", cli.input.display()))?;

    let engine = Engine::new();

    if cli.emit_ir {
        let ir = engine
            .compile(&source)
            .map_err(|e| format!("failed to compile {}: {e}", cli.input.display()))?;
        print!("{}", ir.clif());
    }

    if let Some(object_path) = &cli.object {
        let mut ir = engine
            .compile(&source)
            .map_err(|e| format!("failed to compile {}: {e}", cli.input.display()))?;
        let object = ir
            .emit_object()
            .map_err(|e| format!("failed to emit object file: {e}"))?;
        fs::write(object_path, object)
            .map_err(|e| format!("failed to write {}: {e}", object_path.display()))?;
    }

    if let Some(output_path) = &cli.output {
        let mut ir = engine
            .compile(&source)
            .map_err(|e| format!("failed to compile {}: {e}", cli.input.display()))?;
        ir.emit_binary(output_path)
            .map_err(|e| format!("failed to emit binary: {e}"))?;
    }

    if cli.no_run || cli.object.is_some() || cli.output.is_some() {
        return Ok(ExitCode::SUCCESS);
    }

    let mut ir = engine
        .compile(&source)
        .map_err(|e| format!("failed to compile {}: {e}", cli.input.display()))?;
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
