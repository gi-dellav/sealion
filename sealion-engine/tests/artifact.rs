#![cfg(all(feature = "jit", target_os = "linux", target_arch = "x86_64"))]

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use sealion_engine::{CompileOptions, Engine, OptLevel};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn temp_path(name: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "sealion_artifact_{}_{}_{}",
        std::process::id(),
        n,
        name
    ))
}

#[test]
fn round_trip_precompile_and_deserialize() {
    let engine = Engine::new();
    let source = "function add(a, b) { return a + b; } add(19, 23);";
    let artifact = engine.precompile_module(source).unwrap();
    assert_eq!(&artifact[..4], b"\x7fELF");

    let module = engine.deserialize(&artifact).unwrap();
    assert_eq!(module.run(), 42);
    assert_eq!(module.run(), 42);
    assert_eq!(module.serialize().unwrap(), artifact);
}

#[test]
fn deserializes_from_file_and_raw_memory() {
    let engine = Engine::new();
    let artifact = engine.precompile_module("6 * 7;").unwrap();

    let path = temp_path("unit.art");
    fs::write(&path, &artifact).unwrap();
    let module = engine.deserialize_file(&path).unwrap();
    let _ = fs::remove_file(&path);
    assert_eq!(module.run(), 42);

    let raw = engine.deserialize_raw(&artifact).unwrap();
    assert_eq!(raw.run(), 42);
}

#[test]
fn jit_output_can_be_serialized_and_reloaded() {
    let engine = Engine::new();
    let program = engine
        .compile_jit("function f(x) { return x + 1; } f(41);")
        .unwrap();
    assert_eq!(program.run(), 42);

    let artifact = program.serialize().unwrap();
    let module = engine.deserialize(&artifact).unwrap();
    assert_eq!(module.run(), 42);
}

#[test]
fn artifact_is_deterministic() {
    let engine = Engine::new();
    let source = "var x = 3; function f() { return x; } f();";
    let first = engine.precompile_module(source).unwrap();
    let second = engine.precompile_module(source).unwrap();
    assert_eq!(first, second);
}

#[test]
fn exposes_function_table() {
    let engine = Engine::new();
    let artifact = engine
        .precompile_module("function add(a, b) { return a + b; } add(4, 5);")
        .unwrap();
    let module = engine.deserialize(&artifact).unwrap();

    let main = module.function_by_name("main").expect("main function");
    let script = module
        .function_by_name("__script")
        .expect("script function");
    assert!(module.functions().len() >= 2);

    let main_loc = module.function_location(main).unwrap();
    let script_loc = module.function_location(script).unwrap();
    assert!(main_loc.length > 0);
    assert!(script_loc.length > 0);
    assert_eq!(
        module.function_at_offset(main_loc.start as usize),
        Some(main)
    );
    assert_eq!(
        module.function_at_offset(script_loc.start as usize),
        Some(script)
    );
    assert_eq!(
        module.function_at_address(module.function_address(main).unwrap()),
        Some(main)
    );
    assert!(!module.function(main).unwrap().is_empty());
    assert_eq!(module.entry_offset() as u32, main_loc.start);
}

#[test]
fn accepts_different_optimization_level() {
    let engine = Engine::new();
    let options = CompileOptions {
        opt_level: OptLevel::None,
        ..CompileOptions::default()
    };
    let artifact = engine
        .precompile_module_with_options("1 + 2 * 3;", &options)
        .unwrap();
    assert_eq!(engine.deserialize(&artifact).unwrap().run(), 7);
}

#[test]
fn rejects_corrupt_and_truncated_artifacts() {
    let engine = Engine::new();
    let artifact = engine.precompile_module("40 + 2;").unwrap();

    assert!(engine.deserialize(&artifact[..artifact.len() / 2]).is_err());
    assert!(engine.deserialize(&[]).is_err());
    assert!(engine.deserialize(b"not an artifact").is_err());

    let mut bad_magic = artifact.clone();
    bad_magic[0] = 0;
    assert!(engine.deserialize(&bad_magic).is_err());
}

#[test]
fn rejects_artifacts_from_another_engine_version() {
    let engine = Engine::new();
    let mut artifact = engine.precompile_module("1;").unwrap();
    let current = env!("CARGO_PKG_VERSION").as_bytes();
    let replacement = vec![b'9'; current.len()];
    let mut replaced = false;
    for window in artifact.windows(current.len()).enumerate() {
        if window.1 == current {
            let start = window.0;
            artifact[start..start + current.len()].copy_from_slice(&replacement);
            replaced = true;
            break;
        }
    }
    assert!(replaced, "artifact did not contain the engine version");
    let error = engine.deserialize(&artifact).unwrap_err().to_string();
    assert!(error.contains("engine version"), "{error}");
}

#[test]
fn rejects_cross_compilation_at_precompile_time() {
    let engine = Engine::new();
    let options = CompileOptions {
        target: Some("aarch64-unknown-linux-gnu".to_string()),
        ..CompileOptions::default()
    };
    let error = engine
        .precompile_module_with_options("1;", &options)
        .unwrap_err()
        .to_string();
    assert!(error.contains("cross-compile"), "{error}");
}

#[test]
fn published_text_is_not_writable() {
    let engine = Engine::new();
    let artifact = engine.precompile_module("1;").unwrap();
    let module = engine.deserialize(&artifact).unwrap();
    let entry = module.entry();

    let maps = fs::read_to_string("/proc/self/maps").unwrap();
    let line = maps
        .lines()
        .find(|line| {
            let mut fields = line.split_whitespace();
            let Some(range) = fields.next() else {
                return false;
            };
            let Some((start, end)) = range.split_once('-') else {
                return false;
            };
            let Ok(start) = usize::from_str_radix(start, 16) else {
                return false;
            };
            let Ok(end) = usize::from_str_radix(end, 16) else {
                return false;
            };
            (start..end).contains(&entry)
        })
        .expect("entry mapping");
    let perms = line.split_whitespace().nth(1).unwrap();
    assert!(
        perms.starts_with("r-x") || perms.starts_with("r--"),
        "{perms}"
    );
    assert!(!perms.starts_with("rwx"), "{perms}");
}

#[test]
fn serializes_a_normally_compiled_module() {
    let engine = Engine::new();
    let mut module = engine
        .compile("function add(a, b) { return a + b; } add(20, 22);")
        .unwrap();
    let artifact = module.serialize().unwrap();
    let loaded = engine.deserialize(&artifact).unwrap();
    assert_eq!(loaded.run(), 42);
}

#[test]
fn string_helpers_survive_precompile_roundtrip() {
    let engine = Engine::new();
    let artifact = engine
        .precompile_module(r#"var s = "hello"; s + "!" === "hello!" ? s.length : 0;"#)
        .unwrap();
    let module = engine.deserialize(&artifact).unwrap();
    assert_eq!(module.run(), 5);
}
