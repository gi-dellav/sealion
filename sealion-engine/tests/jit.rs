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

#[test]
fn object_literals_and_property_reads() {
    assert_eq!(run("var o = {a: 1}; o.a;"), 1);
    assert_eq!(run("({a: 1, b: 2}).b;"), 2);
    assert_eq!(run("var a = 3; var o = {a}; o.a;"), 3);
    assert_eq!(run("var o = {n: 1, n: 2}; o.n;"), 2);
    assert_eq!(run(r#"var o = {a: 7}; o["a"];"#), 7);
    assert_eq!(run(r#"var k = "x"; var o = {[k]: 5}; o.x;"#), 5);
    assert_eq!(run("var o = {1: 4}; o[1];"), 4);
    assert_eq!(run(r#"var o = {1: 4}; o["1"];"#), 4);
    assert_eq!(run("var o = {a: {b: 9}}; o.a.b;"), 9);
    assert_eq!(run("var o = {}; o.none === undefined ? 1 : 2;"), 1);
    assert_eq!(
        run("var o = {f: function (x) { return x + 1; }}; o.f(4);"),
        5
    );
}

#[test]
fn object_property_writes_updates_and_delete() {
    assert_eq!(run("var o = {}; o.a = 1; o.a;"), 1);
    assert_eq!(run("var o = {}; o.a = 1; o.a += 2; o.a;"), 3);
    assert_eq!(run("var o = {n: 1}; o.n++; o.n;"), 2);
    assert_eq!(run("var o = {n: 5}; o.n--;"), 5);
    assert_eq!(run("var o = {n: 5}; ++o.n;"), 6);
    assert_eq!(run("var o = {n: 5}; o.n *= 2; o.n;"), 10);
    assert_eq!(run("var o = {a: 1}; 'a' in o ? 1 : 2;"), 1);
    assert_eq!(run("var o = {a: 1}; delete o.a; 'a' in o ? 1 : 2;"), 2);
    assert_eq!(
        run("var o = {}; o.m = 1; delete o.m; o.m === undefined ? 1 : 2;"),
        1
    );
}

#[test]
fn object_identity_truthiness_and_chaining() {
    assert_eq!(run("var o = {}; o === o ? 1 : 2;"), 1);
    assert_eq!(run("var o = {}; o === {} ? 1 : 2;"), 2);
    assert_eq!(run("var o = {}; Object.is(o, o) ? 1 : 0;"), 1);
    assert_eq!(run("var o = {}; Object.is(o, {}) ? 1 : 0;"), 0);
    assert_eq!(run("var r = 0; if ({}) { r = 1; } else { r = 2; } r;"), 1);
    assert_eq!(run("var o = null; o?.a === undefined ? 1 : 2;"), 1);
    assert_eq!(run("var o = {a: 4}; o?.a;"), 4);
    assert_eq!(run("var o = {a: 1}; o.a.x === undefined ? 1 : 2;"), 1);
}

#[test]
fn objects_stringify_and_flow_through_functions() {
    assert_eq!(
        run(r#"var o = {a: 1}; String(o) === "[object Object]" ? 1 : 2;"#),
        1
    );
    assert_eq!(
        run(r#"var o = {}; o + "" === "[object Object]" ? 1 : 2;"#),
        1
    );
    assert_eq!(run("function f(o) { return o.a; } f({a: 8});"), 8);
    assert_eq!(
        run("var o = {n: 1}; function g() { return o.n; } g(); o.n = 2; g();"),
        2
    );
    assert_eq!(run("var o = {n: 1}; console.log(o, o.n); 0;"), 0);
}

#[test]
fn unsupported_object_shapes_are_rejected() {
    for source in [
        "var o = {...{a: 1}}; 0;",
        "var o = {get a() { return 1; }}; 0;",
        "var o = {__proto__: null}; 0;",
    ] {
        assert!(
            Engine::new().compile_jit(source).is_err(),
            "expected a rejection for {source}"
        );
    }
}

#[test]
fn direct_calls_respect_reassignment_and_escapes() {
    assert_eq!(
        run(
            "function f(x) { return x + 1; } var g = f; f = function (x) { return x * 10; }; f(5) + g(1);"
        ),
        52
    );
}

#[test]
fn numeric_parameters_stay_correct_with_mixed_calls() {
    assert_eq!(
        run(
            r#"function add(a, b) { return a + b; } add(1, 2) + (add("a", "b") === "ab" ? 10 : 0);"#
        ),
        13
    );
}

#[test]
fn missing_and_default_arguments_keep_their_values() {
    assert_eq!(
        run("function f(a, b) { return b; } f(1) === undefined ? 1 : 0;"),
        1
    );
    assert_eq!(run("function f(a = 5) { return a; } f();"), 5);
}

#[test]
fn initialized_loop_bindings_compile_without_tdz_checks() {
    let ir = Engine::new()
        .compile(
            "function f(n) { let acc = 0.0; for (let i = 1; i <= n; i = i + 1) { acc = acc + Math.sqrt(i); } return acc; } f(10);",
        )
        .unwrap();
    let clif = ir.clif();
    assert!(!clif.contains("trapnz"), "{clif}");
    assert!(clif.contains("fcmp le"), "{clif}");
}

#[test]
fn numeric_comparison_conditions_match_js_semantics() {
    assert_eq!(
        run("var r = 0; if (NaN < 1) { r = 1; } else { r = 2; } r;"),
        2
    );
    assert_eq!(run("NaN !== NaN ? 1 : 0;"), 1);
    assert_eq!(run("-0 === 0 ? 1 : 0;"), 1);
    assert_eq!(run("-0 < 0 ? 1 : 0;"), 0);
}
