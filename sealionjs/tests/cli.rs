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

#[test]
fn runs_0_4_0_blocks_and_arrows() {
    let input = write_source("let x = 1; { let x = 2; } var inc = (n) => n + x; inc(3);");
    let status = Command::new(bin()).arg(&input).status().unwrap();
    let _ = fs::remove_file(&input);
    assert_eq!(status.code(), Some(4));
}

#[test]
fn runs_0_6_0_for_and_switch() {
    let input = write_source(
        "const limit = 4;
         var s = 0;
         for (let i = 0; i < limit; i++) { if (i === 2) { continue; } s = s + i; }
         switch (s) { case 4: s = s + 1; break; default: s = 0; }
         s;",
    );
    let status = Command::new(bin()).arg(&input).status().unwrap();
    let _ = fs::remove_file(&input);
    assert_eq!(status.code(), Some(5));
}

#[test]
fn rejects_const_reassignment() {
    let input = write_source("const c = 1; c = 2;");
    let output = Command::new(bin()).arg(&input).output().unwrap();
    let _ = fs::remove_file(&input);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("constant"));
}

#[test]
fn transpiles_typescript() {
    let input = temp_path("input.ts");
    fs::write(
        &input,
        "const add = (a: number, b: number): number => a + b;\nadd(2, 3);",
    )
    .unwrap();
    let status = Command::new(bin()).arg("-t").arg(&input).status().unwrap();
    let _ = fs::remove_file(&input);
    assert_eq!(status.code(), Some(5));
}

#[test]
fn accepts_es2026_transform() {
    let input = write_source("let x = 2 * 3; x;");
    let status = Command::new(bin())
        .arg("--es2026")
        .arg(&input)
        .status()
        .unwrap();
    let _ = fs::remove_file(&input);
    assert_eq!(status.code(), Some(6));
}

#[test]
fn supports_opt_level_flags() {
    for level in ["none", "speed", "speed_and_size"] {
        let input = write_source("1 + 2 * 3;");
        let status = Command::new(bin())
            .arg("--opt-level")
            .arg(level)
            .arg(&input)
            .status()
            .unwrap();
        let _ = fs::remove_file(&input);
        assert_eq!(status.code(), Some(7), "opt level {level}");
    }
}

#[test]
fn supports_short_opt_level_flag() {
    let input = write_source("1 + 2 * 3;");
    let status = Command::new(bin())
        .arg("-O")
        .arg("none")
        .arg(&input)
        .status()
        .unwrap();
    let _ = fs::remove_file(&input);
    assert_eq!(status.code(), Some(7));
}

#[test]
fn supports_cranelift_codegen_toggles() {
    let input = write_source("var x = 1; x + 2;");
    let status = Command::new(bin())
        .arg("--no-alias-analysis")
        .arg("--no-verifier")
        .arg("--preserve-frame-pointers")
        .arg(&input)
        .status()
        .unwrap();
    let _ = fs::remove_file(&input);
    assert_eq!(status.code(), Some(3));
}

#[test]
fn emits_object_without_unwind_info() {
    let input = write_source("while (0) {}");
    let object = temp_path("program.o");
    let status = Command::new(bin())
        .arg("--object")
        .arg(&object)
        .arg("--no-unwind-info")
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
fn emits_non_pic_object() {
    let input = write_source("while (0) {}");
    let object = temp_path("program.o");
    let status = Command::new(bin())
        .arg("--object")
        .arg(&object)
        .arg("--no-pic")
        .arg(&input)
        .status()
        .unwrap();
    let bytes = fs::read(&object).unwrap();
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&object);
    assert!(status.success());
    assert!(!bytes.is_empty());
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[test]
fn supports_explicit_target_and_cpu() {
    let input = write_source("1 + 2;");
    let status = Command::new(bin())
        .arg("--target")
        .arg("x86_64-unknown-linux-gnu")
        .arg("--target-cpu")
        .arg("x86-64")
        .arg(&input)
        .status()
        .unwrap();
    let _ = fs::remove_file(&input);
    assert_eq!(status.code(), Some(3));
}

#[test]
fn rejects_invalid_target() {
    let input = write_source("1;");
    let output = Command::new(bin())
        .arg("--target")
        .arg("not-a-target")
        .arg(&input)
        .output()
        .unwrap();
    let _ = fs::remove_file(&input);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("target triple"));
}

#[test]
fn rejects_invalid_target_cpu() {
    let input = write_source("1;");
    let output = Command::new(bin())
        .arg("--target-cpu")
        .arg("not-a-cpu")
        .arg(&input)
        .output()
        .unwrap();
    let _ = fs::remove_file(&input);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("target CPU"));
}

#[cfg(feature = "jit")]
#[test]
fn runs_in_process_jit_and_warns_on_ignored_flags() {
    let input = write_source("function f(a) { return a * 2; } f(21);");
    let output = Command::new(bin())
        .arg("--no-pic")
        .arg("--no-unwind-info")
        .arg(&input)
        .output()
        .unwrap();
    let _ = fs::remove_file(&input);
    assert_eq!(output.status.code(), Some(42));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--no-pic is ignored"));
    assert!(stderr.contains("--no-unwind-info is ignored"));
}

#[cfg(feature = "jit")]
#[test]
fn writes_and_runs_precompiled_artifact() {
    let input = write_source("function f(x) { return x * 2; } f(21);");
    let artifact = temp_path("program.art");

    let status = Command::new(bin())
        .arg("--artifact")
        .arg(&artifact)
        .arg(&input)
        .status()
        .unwrap();
    assert!(status.success());

    let run = Command::new(bin())
        .arg("--run-artifact")
        .arg(&artifact)
        .status()
        .unwrap();
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&artifact);
    assert_eq!(run.code(), Some(42));
}
