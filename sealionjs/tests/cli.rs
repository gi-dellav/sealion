use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_sealionjs")
}

fn temp_path(name: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "sealionjs_cli_{}_{}_{}",
        std::process::id(),
        n,
        name
    ))
}

fn write_source(source: &str) -> PathBuf {
    let path = temp_path("input.js");
    fs::write(&path, source).unwrap();
    path
}

#[test]
fn runs_script_by_default() {
    let input = write_source("1 + 2;");
    let status = Command::new(bin()).arg(&input).status().unwrap();
    let _ = fs::remove_file(&input);
    assert_eq!(status.code(), Some(3));
}

#[test]
fn emits_and_runs_executable() {
    let input = write_source("function add(a, b) { return a + b; } add(4, 5);");
    let output = temp_path("program");
    let status = Command::new(bin())
        .arg("--output")
        .arg(&output)
        .arg(&input)
        .status()
        .unwrap();
    assert!(status.success());

    let run = Command::new(&output).status().unwrap();
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&output);
    assert_eq!(run.code(), Some(9));
}

#[test]
fn emits_object_file() {
    let input = write_source("while (0) {}");
    let object = temp_path("program.o");
    let status = Command::new(bin())
        .arg("--object")
        .arg(&object)
        .arg(&input)
        .status()
        .unwrap();
    let bytes = fs::read(&object).unwrap();
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&object);
    assert!(status.success());
    assert!(!bytes.is_empty());
}

#[test]
fn prints_ir_without_running() {
    let input = write_source("var x = 1; x;");
    let output = Command::new(bin())
        .arg("--emit-ir")
        .arg("--no-run")
        .arg(&input)
        .output()
        .unwrap();
    let _ = fs::remove_file(&input);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("__script"));
    assert!(stdout.contains("f64const"));
}

#[test]
fn runs_break_continue_and_undefined() {
    let input = write_source(
        "var i = 0; var s = 0;
         while (i < 5) { i = i + 1; if (i === 3) { continue; } if (i > 4) { break; } s = s + i; }
         if (s === undefined) { s = 0; }
         void null;
         s;",
    );
    let status = Command::new(bin()).arg(&input).status().unwrap();
    let _ = fs::remove_file(&input);
    assert_eq!(status.code(), Some(7));
}

#[test]
fn reports_errors() {
    let input = write_source("var = ;");
    let output = Command::new(bin()).arg(&input).output().unwrap();
    let _ = fs::remove_file(&input);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("sealionjs:"));
}

#[test]
fn runs_0_3_0_operators() {
    let input = write_source("var x = 1 ? (6 & 3) : 9; x |= 4; ~0 === -1 ? x : 0;");
    let status = Command::new(bin()).arg(&input).status().unwrap();
    let _ = fs::remove_file(&input);
    assert_eq!(status.code(), Some(6));
}
