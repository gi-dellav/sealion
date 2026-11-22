#![cfg(all(feature = "jit", target_os = "linux", target_arch = "x86_64"))]

use sealion_engine::{CompileOptions, Engine};

fn run(js: &str) -> i32 {
    let program = Engine::new()
        .compile_jit(js)
        .unwrap_or_else(|e| panic!("JIT compile failed: {e}\n{js}"));
    program.run()
}

#[test]
fn jit_is_supported_on_this_target() {
    assert!(sealion_engine::jit_supported());
}

#[test]
fn arithmetic_and_control_flow() {
    assert_eq!(run("1 + 2 * 3;"), 7);
    assert_eq!(run("5 % 3;"), 2);
    assert_eq!(run("(1 << 4) | 3;"), 19);
    assert_eq!(
        run("var i = 0; var s = 0; while (i < 5) { s = s + i; i = i + 1; } s;"),
        10
    );
    assert_eq!(
        run(
            "var x = 2; var r = 0; switch (x) { case 1: r = 1; break; case 2: r = 4; break; default: r = 9; } r;"
        ),
        4
    );
}

#[test]
fn functions_and_closures() {
    assert_eq!(run("function add(a, b) { return a + b; } add(4, 5);"), 9);
    assert_eq!(
        run(
            "function makeAdder(x) { function add(y) { return x + y; } return add; } var add5 = makeAdder(5); add5(3);"
        ),
        8
    );
    assert_eq!(
        run(
            "function outer(x) { function middle() { function inner() { return x; } return inner; } return middle(); } outer(7)();"
        ),
        7
    );
}

#[test]
fn calls_libm_and_libc() {
    assert_eq!(run("Math.sqrt(16);"), 4);
    assert_eq!(run("Math.pow(2, 8);"), 256);
    assert_eq!(run("Math.sin(0) + Math.log(1) + Math.floor(2.9);"), 2);
    assert!(run("Math.random() >= 0 ? 1 : 0;") == 1);
    assert_eq!(run("Date.now() > 0 ? 1 : 0;"), 1);
}

#[test]
fn exceptions_use_in_process_state() {
    assert_eq!(run("try { throw 7; } catch (e) { e; }"), 7);
    assert_eq!(run("throw 5;"), 1);
}

#[test]
fn runs_more_than_once() {
    let program = Engine::new().compile_jit("var x = 4; x * 2;").unwrap();
    assert_eq!(program.run(), 8);
    assert_eq!(program.run(), 8);
}

#[test]
fn exposes_ir_and_functions() {
    let program = Engine::new().compile_jit("1 + 1;").unwrap();
    assert!(program.clif().contains("__script"));
    assert!(!program.functions().is_empty());
}

#[test]
fn overrides_pic_for_jit() {
    let options = CompileOptions {
        is_pic: true,
        ..CompileOptions::default()
    };
    let program = Engine::new()
        .compile_jit_with_options("2 + 3;", &options)
        .unwrap();
    assert_eq!(program.run(), 5);
}

#[test]
fn rejects_cross_compilation() {
    let target = "aarch64-unknown-linux-gnu";
    let options = CompileOptions {
        target: Some(target.to_string()),
        ..CompileOptions::default()
    };
    match Engine::new().compile_jit_with_options("1 + 1;", &options) {
        Ok(_) => panic!("expected cross-compilation to be rejected"),
        Err(error) => assert!(error.to_string().contains("cannot cross-compile")),
    }
}

#[test]
fn strings_flow_through_the_dynamic_runtime() {
    assert_eq!(run(r#""hello" === "hello" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""hello" === "world" ? 1 : 0;"#), 0);
    assert_eq!(run(r#"var s = "abc"; s.length;"#), 3);
    assert_eq!(run(r#""a" + "b" === "ab" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""n=" + 42 === "n=42" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""5" - 2;"#), 3);
    assert_eq!(run(r#""10" < "9" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""10" < 9 ? 1 : 0;"#), 0);
    assert_eq!(run(r#"Number("42") + 1;"#), 43);
    assert_eq!(run(r#"String(123) === "123" ? 1 : 0;"#), 1);
    assert_eq!(
        run(r#"var r = 0; if ("") { r = 1; } else { r = 2; } r;"#),
        2
    );
    assert_eq!(
        run(r#"var r = 0; if ("x") { r = 1; } else { r = 2; } r;"#),
        1
    );
    assert_eq!(run(r#"Object.is("a", "a") ? 1 : 0;"#), 1);
    assert_eq!(
        run(r#"var name = "world"; `hello ${name}` === "hello world" ? 1 : 0;"#),
        1
    );
}

#[test]
fn strings_capture_and_cross_calls() {
    assert_eq!(
        run(
            r#"function make() { var s = "captured"; return function () { return s; }; } make()() === "captured" ? 1 : 0;"#
        ),
        1
    );
    assert_eq!(
        run(r#"function id(s) { return s; } id("value") === "value" ? 1 : 0;"#),
        1
    );
}

#[test]
fn console_log_accepts_strings_and_other_values() {
    assert_eq!(
        run(
            r#"var s = "world"; console.log("hello", s); console.log(42, true, null, undefined); 0;"#
        ),
        0
    );
}
