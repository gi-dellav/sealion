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
fn string_inspection_methods() {
    assert_eq!(run(r#""abc".charAt(1) === "b" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abc".charAt() === "a" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abc".charAt(9) === "" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abc".charAt(-1) === "" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abc".charCodeAt(2);"#), 99);
    assert_eq!(run(r#"Number.isNaN("abc".charCodeAt(7)) ? 1 : 0;"#), 1);
    assert_eq!(run(r#""A".codePointAt(0);"#), 65);
    assert_eq!(run(r#""abc".at(-1) === "c" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abc".at(9) === undefined ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abcabc".indexOf("bc");"#), 1);
    assert_eq!(run(r#""abcabc".indexOf("bc", 2);"#), 4);
    assert_eq!(run(r#""abc".indexOf("z");"#), -1);
    assert_eq!(run(r#""abcabc".lastIndexOf("bc");"#), 4);
    assert_eq!(run(r#""abcabc".lastIndexOf("bc", 3);"#), 1);
    assert_eq!(run(r#""abc".includes("b") ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abc".includes("z") ? 1 : 0;"#), 0);
    assert_eq!(run(r#""abc".startsWith("ab") ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abc".startsWith("ab", 1) ? 1 : 0;"#), 0);
    assert_eq!(run(r#""abc".endsWith("bc") ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abc".endsWith("ab", 2) ? 1 : 0;"#), 1);
}

#[test]
fn string_trimming_and_case_methods() {
    assert_eq!(run(r#""  x  ".trim() === "x" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""  x  ".trimStart() === "x  " ? 1 : 0;"#), 1);
    assert_eq!(run(r#""  x  ".trimEnd() === "  x" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abc".toUpperCase() === "ABC" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""ABC".toLowerCase() === "abc" ? 1 : 0;"#), 1);
    assert_eq!(
        run(r#"var up = "abc".toUpperCase; up() === "ABC" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"var s = "  Ab  "; s.trim().toLowerCase() === "ab" ? 1 : 0;"#),
        1
    );
    assert_eq!(run(r#""abc".repeat(3) === "abcabcabc" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abc".repeat(0) === "" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""5".padStart(3, "0") === "005" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""5".padEnd(3) === "5  " ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abcdef".padStart(3, "0") === "abcdef" ? 1 : 0;"#), 1);
}

#[test]
fn string_slicing_and_joining_methods() {
    assert_eq!(run(r#""abcdef".slice(1, 3) === "bc" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abcdef".slice(-2) === "ef" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abcdef".slice(3, 1) === "" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abcdef".substring(3, 1) === "bc" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abcdef".substring(-2, 2) === "ab" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abcdef".substr(1, 2) === "bc" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""a-b-c".split("-").length;"#), 3);
    assert_eq!(run(r#""a-b-c".split("-")[1] === "b" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abc".split("").length;"#), 3);
    assert_eq!(run(r#""abc".split().length;"#), 1);
    assert_eq!(run(r#""a-b-c".split("-", 2).length;"#), 2);
    assert_eq!(run(r#""a".concat("b", 1) === "ab1" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abc".toString() === "abc" ? 1 : 0;"#), 1);
}

#[test]
fn string_replace_methods() {
    assert_eq!(run(r#""a-b".replace("-", "+") === "a+b" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""a-b-c".replace("-", "+") === "a+b-c" ? 1 : 0;"#), 1);
    assert_eq!(
        run(r#""a-b-c".replaceAll("-", "+") === "a+b+c" ? 1 : 0;"#),
        1
    );
    assert_eq!(run(r#""abc".replace("b", "[$&]") === "a[b]c" ? 1 : 0;"#), 1);
    assert_eq!(run(r#""abc".replace("z", "+") === "abc" ? 1 : 0;"#), 1);
    assert_eq!(
        run(r#""abc".replace("b", function (m) { return m.toUpperCase(); }) === "aBc" ? 1 : 0;"#),
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

#[test]
fn array_literals_length_and_indices() {
    assert_eq!(run("[];0;"), 0);
    assert_eq!(run("[1, 2, 3].length;"), 3);
    assert_eq!(run("var a = [10, 20, 30]; a[1];"), 20);
    assert_eq!(run(r#"var a = [10, 20]; a["1"];"#), 20);
    assert_eq!(run("var a = [1]; a[3] = 4; a.length;"), 4);
    assert_eq!(
        run("var a = [1, 2, 3]; a.length = 1; a[2] === undefined ? 1 : 0;"),
        1
    );
    assert_eq!(run("var a = [1, 2, 3]; a.length = 5; a.length;"), 5);
    assert_eq!(run("var a = [1, 2]; a[0]++; a[1] += 5; a[0] + a[1];"), 9);
    assert_eq!(run("var a = [1, 2]; 1 in a ? 1 : 0;"), 1);
    assert_eq!(run("var a = [1, 2]; 2 in a ? 1 : 0;"), 0);
    assert_eq!(
        run("var a = [1, 2]; delete a[0]; a[0] === undefined ? 1 : 0;"),
        1
    );
    assert_eq!(run("var a = [1, 2]; a === a ? 1 : 0;"), 1);
    assert_eq!(run("var a = []; a === [] ? 0 : 1;"), 1);
    assert_eq!(run("var r = 0; if ([]) { r = 1; } r;"), 1);
    assert_eq!(run("[1, , 3].length;"), 3);
    assert_eq!(run("var a = [[1, 2], [3]]; a[0][1] + a[1][0];"), 5);
}

#[test]
fn array_mutation_methods() {
    assert_eq!(run("var a = []; a.push(1, 2); a.length;"), 2);
    assert_eq!(run("var a = [1]; a.push(2); a[1];"), 2);
    assert_eq!(run("var a = [1, 2]; a.pop();"), 2);
    assert_eq!(run("var a = [1, 2]; a.pop(); a.length;"), 1);
    assert_eq!(run("var a = []; a.pop() === undefined ? 1 : 0;"), 1);
    assert_eq!(run("var a = [1, 2]; a.shift();"), 1);
    assert_eq!(run("var a = [1, 2]; a.shift(); a[0];"), 2);
    assert_eq!(run("var a = [3]; a.unshift(1, 2); a[0] + a[2];"), 4);
    assert_eq!(run("var a = [1, 2, 3]; a.reverse(); a[0];"), 3);
    assert_eq!(run("var a = [1, 2]; a.reverse() === a ? 1 : 0;"), 1);
}

#[test]
fn array_accessor_and_search_methods() {
    assert_eq!(run(r#"[1, 2, 3].join() === "1,2,3" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"[1, 2].join("-") === "1-2" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"[1, null, 3].join("-") === "1--3" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"String([1, 2]) === "1,2" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"[1, 2] + "" === "1,2" ? 1 : 0;"#), 1);
    assert_eq!(run("var a = [1, 2, 3, 4]; a.slice(1, 3).length;"), 2);
    assert_eq!(run("var a = [1, 2, 3, 4]; a.slice(-2)[0];"), 3);
    assert_eq!(run("var a = [1, 2]; a.concat([3, 4]).length;"), 4);
    assert_eq!(run("var a = [1, 2]; a.concat(3)[2];"), 3);
    assert_eq!(run("var a = [1, 2, 3]; a.indexOf(2);"), 1);
    assert_eq!(run("var a = [1, 2, 3]; a.indexOf(9);"), -1);
    assert_eq!(run("var a = [1, 2, 3]; a.indexOf(3, 0);"), 2);
    assert_eq!(run("var a = [1, 2, 3]; a.includes(2) ? 1 : 0;"), 1);
    assert_eq!(run("var a = [1, 2, 3]; a.includes(9) ? 0 : 1;"), 1);
    assert_eq!(run("var a = [NaN]; a.includes(NaN) ? 1 : 0;"), 1);
    assert_eq!(run(r#"var a = ["a", "b"]; a.indexOf("b");"#), 1);
}

#[test]
fn array_iteration_methods_call_closures() {
    assert_eq!(
        run("var s = 0; [1, 2, 3].forEach(function (x) { s = s + x; }); s;"),
        6
    );
    assert_eq!(
        run("var a = [1, 2, 3].map(function (x) { return x * 2; }); a[2];"),
        6
    );
    assert_eq!(
        run("var a = [1, 2, 3, 4].filter(function (x) { return x > 2; }); a.length;"),
        2
    );
    assert_eq!(
        run("[1, 2, 3].reduce(function (acc, x) { return acc + x; });"),
        6
    );
    assert_eq!(
        run("[1, 2].reduce(function (acc, x) { return acc + x; }, 10);"),
        13
    );
    assert_eq!(
        run("[1, 2, 3].some(function (x) { return x > 2; }) ? 1 : 0;"),
        1
    );
    assert_eq!(
        run("[1, 2, 3].every(function (x) { return x > 0; }) ? 1 : 0;"),
        1
    );
    assert_eq!(run("[1, 2, 3].find(function (x) { return x > 1; });"), 2);
    assert_eq!(
        run("[1, 2, 3].findIndex(function (x) { return x > 1; });"),
        1
    );
    assert_eq!(
        run("var plus = [1, 2].map(function (x) { return x + 1; }); plus[0] + plus[1];"),
        5
    );
}

#[test]
fn array_at_search_and_right_folds() {
    assert_eq!(run("[1, 2, 3].at(1);"), 2);
    assert_eq!(run("[1, 2, 3].at(-1);"), 3);
    assert_eq!(run("[1, 2, 3].at();"), 1);
    assert_eq!(run("[1, 2, 3].at(9) === undefined ? 1 : 0;"), 1);
    assert_eq!(run("[1, 2, 3].at(-9) === undefined ? 1 : 0;"), 1);
    assert_eq!(run("[1, 2, 1].lastIndexOf(1);"), 2);
    assert_eq!(run("[1, 2, 1].lastIndexOf(1, 1);"), 0);
    assert_eq!(run("[1, 2].lastIndexOf(3);"), -1);
    assert_eq!(run("[1, 2].lastIndexOf(2, -5);"), -1);
    assert_eq!(
        run(r#"["a", "b"].reduceRight(function (acc, x) { return acc + x; }) === "ba" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run("[1, 2, 3].reduceRight(function (acc, x) { return acc + x; }, 10);"),
        16
    );
    assert_eq!(
        run("[1, 2, 3].findLast(function (x) { return x < 3; });"),
        2
    );
    assert_eq!(
        run("[1, 2, 3].findLastIndex(function (x) { return x < 3; });"),
        1
    );
    assert_eq!(
        run("[1, 2].findLast(function (x) { return x > 5; }) === undefined ? 1 : 0;"),
        1
    );
}

#[test]
fn array_fill_copy_and_flatten_methods() {
    assert_eq!(run("[1, 2, 3].fill(0)[1];"), 0);
    assert_eq!(
        run(r#"[1, 2, 3].fill(0, 1).join() === "1,0,0" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run("[1, 2, 3].fill(0, 1, 2).join() === \"1,0,3\" ? 1 : 0;"),
        1
    );
    assert_eq!(run("var a = [1, 2, 3]; a.fill(0) === a ? 1 : 0;"), 1);
    assert_eq!(
        run(r#"[1, 2, 3, 4].copyWithin(0, 2).join() === "3,4,3,4" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"[1, 2, 3].copyWithin(1, 0).join() === "1,1,2" ? 1 : 0;"#),
        1
    );
    assert_eq!(run("[1, [2, [3]]].flat()[2][0];"), 3);
    assert_eq!(run("[1, [2, [3]]].flat().length;"), 3);
    assert_eq!(run("[1, [2, [3]]].flat(2).length;"), 3);
    assert_eq!(run("[1, [2, [3]]].flat(Infinity).length;"), 3);
    assert_eq!(run("[1, [2, [3]]].flat(0).length;"), 2);
    assert_eq!(
        run("[1, 2].flatMap(function (x) { return [x, x]; }).length;"),
        4
    );
    assert_eq!(run("[1, 2].flatMap(function (x) { return x; })[1];"), 2);
}

#[test]
fn array_sort_splice_and_copy_variants() {
    assert_eq!(run("[10, 9, 1].sort()[0];"), 1);
    assert_eq!(run("[10, 9, 1].sort()[2];"), 9);
    assert_eq!(
        run("[10, 9, 1].sort(function (a, b) { return b - a; })[0];"),
        10
    );
    assert_eq!(run("var a = [2, 1]; a.sort() === a ? 1 : 0;"), 1);
    assert_eq!(
        run(r#"var a = [1, 2, 3, 4]; a.splice(1, 2).join() === "2,3" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"var a = [1, 2, 3, 4]; a.splice(1, 2, 9); a.join() === "1,9,4" ? 1 : 0;"#),
        1
    );
    assert_eq!(run("var a = [1, 2, 3]; a.splice(1); a.length;"), 1);
    assert_eq!(run("var a = [1, 2, 3]; a.splice(-1)[0];"), 3);
    assert_eq!(run("[3, 1, 2].toReversed()[0];"), 2);
    assert_eq!(run("var a = [3, 1, 2]; a.toReversed(); a[0];"), 3);
    assert_eq!(run("[10, 9, 1].toSorted()[0];"), 1);
    assert_eq!(run("var a = [2, 1]; a.toSorted(); a[0];"), 2);
    assert_eq!(
        run(r#"[1, 2].toSpliced(0, 1, 7).join() === "7,2" ? 1 : 0;"#),
        1
    );
    assert_eq!(run("var a = [1, 2]; a.toSpliced(0, 1); a.length;"), 2);
    assert_eq!(run("[1, 2].with(0, 9)[0];"), 9);
    assert_eq!(run("var a = [1, 2]; a.with(-1, 9)[1] + a[1];"), 11);
    assert_eq!(run("[1, 2].with(5, 9).length;"), 2);
    assert_eq!(run("[7, 8].keys()[1];"), 1);
    assert_eq!(run("[7, 8].values()[1];"), 8);
    assert_eq!(run("[7, 8].keys().length;"), 2);
    assert_eq!(run("[7, 8].entries()[1][0] + [7, 8].entries()[1][1];"), 9);
    assert_eq!(run("var a = [1]; a.valueOf() === a ? 1 : 0;"), 1);
}

#[test]
fn map_constructor_and_methods() {
    assert_eq!(run("new Map().size;"), 0);
    assert_eq!(run("Map().size;"), 0);
    assert_eq!(run(r#"var m = new Map(); m.set("a", 1); m.get("a");"#), 1);
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.set("a", 2); m.get("a");"#),
        2
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.set("a", 2); m.size;"#),
        1
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.get("b") === undefined ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.has("a") ? 1 : 0;"#),
        1
    );
    assert_eq!(run(r#"var m = new Map(); m.has("a") ? 0 : 1;"#), 1);
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.delete("a") ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.delete("a"); m.has("a") ? 0 : 1;"#),
        1
    );
    assert_eq!(run(r#"var m = new Map(); m.delete("a") ? 0 : 1;"#), 1);
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.clear(); m.size;"#),
        0
    );
    assert_eq!(run("new Map().set(1, 2).get(1);"), 2);
    assert_eq!(
        run(r#"var m = new Map(); m.set(1, 2); m.set("1", 3); m.size;"#),
        2
    );
    assert_eq!(run("var m = new Map(); m.set(NaN, 5); m.get(NaN);"), 5);
    assert_eq!(
        run("var k = {}; var m = new Map(); m.set(k, 7); m.get(k);"),
        7
    );
    assert_eq!(
        run(r#"var m = new Map(); var set = m.set; set("a", 3); m.get("a");"#),
        3
    );
    assert_eq!(run("Array.isArray(new Map()) ? 0 : 1;"), 1);
}

#[test]
fn map_for_each_calls_closures() {
    assert_eq!(
        run(
            r#"var m = new Map(); m.set("a", 1); m.set("b", 2); var s = 0; m.forEach(function (v, k) { s = s + v; }); s;"#
        ),
        3
    );
    assert_eq!(
        run(
            r#"var m = new Map(); m.set("a", 1); m.set("b", 2); var keys = ""; m.forEach(function (v, k) { keys = keys + k; }); keys === "ab" ? 1 : 0;"#
        ),
        1
    );
    assert_eq!(
        run(
            r#"var m = new Map(); m.set("a", 1); m.forEach(function (v, k, map) { m.delete(k); }); m.size;"#
        ),
        0
    );
}

#[test]
fn map_iteration_accessors_return_arrays() {
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.set("b", 2); m.keys().length;"#),
        2
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.set("b", 2); m.keys()[1] === "b" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.set("b", 2); m.values()[0];"#),
        1
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.entries()[0][0] === "a" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.entries()[0][1];"#),
        1
    );
    assert_eq!(run("new Map().keys().length;"), 0);
    assert_eq!(run("new Map().entries().length;"), 0);
}

#[test]
fn array_constructor_and_method_references() {
    assert_eq!(run("Array.isArray([]) ? 1 : 0;"), 1);
    assert_eq!(run("Array.isArray({}) ? 0 : 1;"), 1);
    assert_eq!(run("Array.isArray(1) ? 0 : 1;"), 1);
    assert_eq!(run("Array().length;"), 0);
    assert_eq!(run("Array(1, 2, 3).length;"), 3);
    assert_eq!(run("Array(3).length;"), 3);
    assert_eq!(run("new Array(2).length;"), 2);
    assert_eq!(run("new Array(7, 8)[1];"), 8);
    assert_eq!(run("Array.of(7).length;"), 1);
    assert_eq!(run("Array.of(7)[0];"), 7);
    assert_eq!(run("Array.of(1, 2, 3).length;"), 3);
    assert_eq!(run("Array.of().length;"), 0);
    assert_eq!(run("Array.from([1, 2])[1];"), 2);
    assert_eq!(run("var a = [1]; Array.from(a) === a ? 0 : 1;"), 1);
    assert_eq!(run("Array.from('ab').length;"), 2);
    assert_eq!(run("Array.from('ab')[1] === 'b' ? 1 : 0;"), 1);
    assert_eq!(
        run("Array.from({length: 2, 0: 'x', 1: 'y'})[1] === 'y' ? 1 : 0;"),
        1
    );
    assert_eq!(run("var a = []; var push = a.push; push(4); a[0];"), 4);
}

#[test]
fn this_in_methods_and_arrows() {
    assert_eq!(
        run("var o = { x: 7, f: function () { return this.x; } }; o.f();"),
        7
    );
    assert_eq!(
        run("var o = { x: 7, f: function () { return this.x; } }; o['f']();"),
        7
    );
    assert_eq!(run("var o = { x: 2, f() { return this.x; } }; o.f();"), 2);
    assert_eq!(
        run("var o = { x: 1, f: function () { return this === o; } }; o.f() ? 1 : 0;"),
        1
    );
    assert_eq!(
        run(
            "var o = { x: 7, f: function () { return this.x; } }; var g = o.f; g() === undefined ? 1 : 0;"
        ),
        1
    );
    assert_eq!(
        run("var o = { x: 5, f: function () { var g = () => this.x; return g(); } }; o.f();"),
        5
    );
    assert_eq!(
        run("var o = { x: 3, f: function () { return () => this.x; } }; var g = o.f(); g();"),
        3
    );
}

#[test]
fn typeof_operator() {
    assert_eq!(run(r#"typeof 1 === "number" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof "s" === "string" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof true === "boolean" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof undefined === "undefined" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof null === "object" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof {} === "object" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof [1] === "object" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof function () {} === "function" ? 1 : 0;"#), 1);
    assert_eq!(
        run(r#"function f() {} typeof f === "function" ? 1 : 0;"#),
        1
    );
    assert_eq!(run(r#"typeof notDefined === "undefined" ? 1 : 0;"#), 1);
    assert_eq!(
        run(r#"typeof (alsoNotDefined) === "undefined" ? 1 : 0;"#),
        1
    );
    assert_eq!(run(r#"typeof Math === "object" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof Number === "function" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"var x; typeof x === "undefined" ? 1 : 0;"#), 1);
}

#[test]
fn array_callbacks_receive_index_and_array() {
    assert_eq!(
        run("[10, 20].map(function (x, i) { return x + i; })[1];"),
        21
    );
    assert_eq!(
        run("[1, 2, 3].map(function (x, i, arr) { return arr.length; })[0];"),
        3
    );
    assert_eq!(
        run("[1, 2, 3].filter(function (x, i) { return i > 0; }).length;"),
        2
    );
    assert_eq!(
        run(
            r#"var out = ""; [1, 2, 3].forEach(function (x, i) { out = out + i; }); out === "012" ? 1 : 0;"#
        ),
        1
    );
    assert_eq!(
        run("var a = [1, 2, 3]; a.reduce(function (acc, x) { return acc * x; });"),
        6
    );
    assert_eq!(
        run("[1, 2, 3].findIndex(function (x) { return x === 9; }) === -1 ? 1 : 0;"),
        1
    );
    assert_eq!(
        run("[1, 2, 3].some(function (x) { return x > 9; }) ? 0 : 1;"),
        1
    );
    assert_eq!(
        run("[1, 2, 3].every(function (x) { return x > 1; }) ? 0 : 1;"),
        1
    );
    assert_eq!(
        run("var a = [1, 2, 3]; a.map(function (x) { return [x, x * 2]; })[1][0];"),
        2
    );
    assert_eq!(
        run("[[1, 2], [3, 4]].map(function (p) { return p[0] + p[1]; })[1];"),
        7
    );
}

#[test]
fn array_methods_copy_and_compose() {
    assert_eq!(run("var a = [1, 2]; var b = a.slice(); b[0] = 9; a[0];"), 1);
    assert_eq!(
        run("var a = [1]; var b = a.concat([2]); a.length + b.length;"),
        3
    );
    assert_eq!(
        run(r#"var a = [1, 2, 3]; a.slice(1).join(",") === "2,3" ? 1 : 0;"#),
        1
    );
    assert_eq!(run("var a = [1, 2, 3]; a.slice(5).length;"), 0);
    assert_eq!(run("var a = [1, 2, 3]; a.slice(-2, -1)[0];"), 2);
    assert_eq!(
        run(
            r#"var a = [1, 2, 3]; a.map(function (x) { return x * x; }).join(",") === "1,4,9" ? 1 : 0;"#
        ),
        1
    );
    assert_eq!(
        run(r#"var a = [1, 2, 3]; a.join("") === "123" ? 1 : 0;"#),
        1
    );
    assert_eq!(run("var a = [1, 2, 3]; a.concat(a).length;"), 6);
    assert_eq!(
        run(r#"var a = ["a", "b"]; a.reverse(); a.join("") === "ba" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"var a = [5]; a.unshift(1, 2, 3); a.join(",") === "1,2,3,5" ? 1 : 0;"#),
        1
    );
    assert_eq!(run("[1, 2, 3].includes(3, 1) ? 1 : 0;"), 1);
    assert_eq!(run(r#"["a", "b", "c"].indexOf("c", 5) === -1 ? 1 : 0;"#), 1);
}

#[test]
fn array_indices_length_and_aliasing() {
    assert_eq!(run("var a = []; a[0] = 1; a.length;"), 1);
    assert_eq!(run("var a = [1, 2, 3]; a[10] = 5; a.length;"), 11);
    assert_eq!(run("var a = [1, 2, 3]; delete a[1]; a.length;"), 3);
    assert_eq!(
        run("var a = [1, 2, 3]; delete a[1]; a[1] === undefined ? 1 : 0;"),
        1
    );
    assert_eq!(
        run(r#"var a = [1, 2, 3]; a.length = 2; a.join(",") === "1,2" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"var a = [1, 2, 3]; a[1] = 9; a.join(",") === "1,9,3" ? 1 : 0;"#),
        1
    );
    assert_eq!(run("var a = [1, 2]; var b = a; b.push(3); a.length;"), 3);
    assert_eq!(run("[1, [2, [3]]][1][1][0];"), 3);
    assert_eq!(
        run("var a = [1, 2, 3]; a.reverse(); a[0] === 3 && a[2] === 1 ? 1 : 0;"),
        1
    );
    assert_eq!(
        run(r#"Array.isArray([1]) && !Array.isArray("x") ? 1 : 0;"#),
        1
    );
    assert_eq!(run("var a = [1, 2]; a.indexOf(1, 1) === -1 ? 1 : 0;"), 1);
}

#[test]
fn array_closures_capture_and_mutate() {
    assert_eq!(
        run("var a = []; function add(x) { a.push(x); } add(1); add(2); a.length;"),
        2
    );
    assert_eq!(
        run("var a = [1, 2]; var f = function () { return a.length; }; a.push(3); f();"),
        3
    );
    assert_eq!(
        run("var f = function () { var inner = [1]; inner.push(2); return inner; }; f().length;"),
        2
    );
    assert_eq!(
        run("var a = []; for (var i = 0; i < 5; i = i + 1) { a.push(i * i); } a[4];"),
        16
    );
    assert_eq!(
        run(
            "var a = [1, 2, 3, 4].filter(function (x) { return x > 2; }); a.map(function (x) { return x * 10; })[1];"
        ),
        40
    );
}

#[test]
fn map_values_hold_every_value_kind() {
    assert_eq!(run("new Map().size;"), 0);
    assert_eq!(run("new Map().set(1, 2).get(1);"), 2);
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1).set("b", 2); m.size;"#),
        2
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", [1, 2]); m.get("a")[1];"#),
        2
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", {n: 5}); m.get("a").n;"#),
        5
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("o", {n: 1}); m.get("o").n = 2; m.get("o").n;"#),
        2
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("f", function () { return 3; }); m.get("f")();"#),
        3
    );
    assert_eq!(run("var m = new Map(); m.set(true, 1); m.get(true);"), 1);
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", [1, 2]); m.get("a").push(3); m.get("a").length;"#),
        3
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set(1, "one"); typeof m.get(1) === "string" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.get("a") === 1 ? 1 : 0;"#),
        1
    );
}

#[test]
fn map_keys_use_same_value_zero_and_identity() {
    assert_eq!(
        run(r#"var m = new Map(); m.set(1, 1); m.set("1", 2); m.get(1);"#),
        1
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set(1, 1); m.set("1", 2); m.size;"#),
        2
    );
    assert_eq!(
        run("var m = new Map(); m.set(NaN, 1); m.set(NaN, 2); m.size;"),
        1
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set(0, "zero"); m.get(-0) === "zero" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("k", 1); m.get("K") === undefined ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run("var m = new Map(); var k = {}; m.set(k, 1); m.has(k) ? 1 : 0;"),
        1
    );
    assert_eq!(
        run("var m = new Map(); m.set({}, 1); m.has({}) ? 0 : 1;"),
        1
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.set("a", 2); m.size;"#),
        1
    );
    assert_eq!(
        run(
            r#"var m = new Map(); m.set("a", 1); m.delete("a"); m.get("a") === undefined ? 1 : 0;"#
        ),
        1
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.set("b", 2); m.delete("a"); m.size;"#),
        1
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.clear(); m.set("b", 2); m.get("b");"#),
        2
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); m.clear(); m.has("a") ? 0 : 1;"#),
        1
    );
}

#[test]
fn map_for_each_receives_map_and_preserves_order() {
    assert_eq!(
        run(
            r#"var m = new Map(); m.set("a", 1); m.set("b", 2); var out = ""; m.forEach(function (v, k) { out = out + k + v; }); out === "a1b2" ? 1 : 0;"#
        ),
        1
    );
    assert_eq!(
        run(
            r#"var m = new Map(); m.set("a", 1); m.set("b", 2); var t = 0; m.forEach(function (v) { t = t * 10 + v; }); t;"#
        ),
        12
    );
    assert_eq!(
        run(
            r#"var m = new Map(); m.set("a", 1); var same = 0; m.forEach(function (v, k, map) { same = map === m ? 1 : 0; }); same;"#
        ),
        1
    );
    assert_eq!(
        run("var m = new Map(); var c = 0; m.forEach(function () { c = c + 1; }); c;"),
        0
    );
    assert_eq!(
        run(
            "var count = 0; var m = new Map(); m.set(\"a\", 1); m.forEach(function () { count = count + 1; }); count;"
        ),
        1
    );
    assert_eq!(
        run(
            r#"var m = new Map(); m.set("a", 1); m.forEach(function (v, k, map) { map.set(k + "!", v * 10); }); m.size;"#
        ),
        2
    );
    assert_eq!(
        run(
            r#"var m = new Map(); m.set("a", 1); m.set("b", 2); m.forEach(function (v, k, map) { if (k === "a") { m.delete(k); } }); m.size;"#
        ),
        1
    );
}

#[test]
fn map_flows_through_functions_and_containers() {
    assert_eq!(
        run(
            r#"var m = new Map(); m.set("a", 1); function read(map, k) { return map.get(k); } read(m, "a");"#
        ),
        1
    );
    assert_eq!(
        run(r#"var m = new Map(); var o = {m: m}; o.m.set("a", 1); m.size;"#),
        1
    );
    assert_eq!(
        run(r#"var ms = [new Map()]; ms[0].set("a", 1); ms[0].get("a");"#),
        1
    );
    assert_eq!(
        run(
            r#"var outer = new Map(); outer.set("in", new Map()); outer.get("in").set("x", 5); outer.get("in").get("x");"#
        ),
        5
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", 1); var n = m; n.set("b", 2); m.size;"#),
        2
    );
    assert_eq!(run(r#"var m = new Map(); m === m ? 1 : 0;"#), 1);
    assert_eq!(
        run(r#"var m = new Map(); var s = m.size; m.set("a", 1); s;"#),
        0
    );
    assert_eq!(
        run(r#"var m = new Map(); m.set("a", {}); typeof m.get("a") === "object" ? 1 : 0;"#),
        1
    );
}

#[test]
fn this_rebinds_to_the_call_site_receiver() {
    assert_eq!(
        run(
            "var o = {v: 1, get: function () { return this.v; }}; var p = {v: 2, get: o.get}; p.get();"
        ),
        2
    );
    assert_eq!(
        run("var o = {x: 2, f: function () { return this.x; }}; ({x: 5, f: o.f}).f();"),
        5
    );
    assert_eq!(
        run(
            "var o = {x: 1, a: function () { return this.b(); }, b: function () { return this.x + 1; }}; o.a();"
        ),
        2
    );
    assert_eq!(
        run(
            "var o = {x: 3, f: function () { return this.x; }}; function run1(x) { return x.f(); } run1(o);"
        ),
        3
    );
    assert_eq!(
        run("var o = {inner: {x: 7, f: function () { return this.x; }}}; o.inner.f();"),
        7
    );
    assert_eq!(
        run("function f() { return {x: 3, get: function () { return this.x; }}; } f().get();"),
        3
    );
    assert_eq!(
        run("var o = {f: function () { return this; }}; o.f() === o ? 1 : 0;"),
        1
    );
    assert_eq!(
        run("var o = {x: 9}; o.get = function () { return this.x; }; o.get();"),
        9
    );
}

#[test]
fn this_mutates_and_chains_on_the_receiver() {
    assert_eq!(
        run(
            "var o = {n: 1, inc: function () { this.n = this.n + 1; return this.n; }}; o.inc(); o.inc();"
        ),
        3
    );
    assert_eq!(
        run(
            "var o = {n: 0, add: function (x) { this.n = this.n + x; return this; }}; o.add(1).add(2).n;"
        ),
        3
    );
    assert_eq!(
        run(
            "var o = {x: 1, f: function () { return this.x; }}; var c = 0; while (c < 2) { o.f(); c = c + 1; } o.x;"
        ),
        1
    );
    assert_eq!(
        run("var a = [1, 2, 3]; a.get = function () { return this.length; }; a.get();"),
        3
    );
    assert_eq!(
        run("var o = {x: 5, f: function () { return this.x + this.x; }}; o.f();"),
        10
    );
    assert_eq!(
        run(r#"var o = {x: 4}; o["f"] = function () { return this.x; }; o.f();"#),
        4
    );
}

#[test]
fn this_in_computed_and_optional_member_calls() {
    assert_eq!(
        run(r#"var o = {x: 6, f: function () { return this.x; }}; var k = "f"; o[k]();"#),
        6
    );
    assert_eq!(
        run(r#"var o = {x: 6, f: function () { return this.x; }}; var k = "f"; o?.[k]();"#),
        6
    );
    assert_eq!(
        run("var o = {x: 6, f: function () { return this.x; }}; o?.f();"),
        6
    );
    assert_eq!(run("var o = null; o?.f() === undefined ? 1 : 0;"), 1);

    assert_eq!(
        run(r#"var o = {d: {v: 3, f: function () { return this.v; }}}; var k = "d"; o[k].f();"#),
        3
    );
}

#[test]
fn this_is_undefined_for_plain_and_detached_calls() {
    assert_eq!(
        run("function f() { return this === undefined ? 1 : 0; } f();"),
        1
    );
    assert_eq!(
        run(
            "var o = {x: 7, f: function () { return this.x; }}; var g = o.f; g() === undefined ? 1 : 0;"
        ),
        1
    );
    assert_eq!(
        run(
            "var o = {f: function () { return typeof this; }}; var g = o.f; g() === \"undefined\" ? 1 : 0;"
        ),
        1
    );
    assert_eq!(
        run(
            "var o = {f: function () { return this === o ? 1 : 0; }}; var g = function () { return o.f(); }; g();"
        ),
        1
    );
    assert_eq!(run("var g = () => this; g() === undefined ? 1 : 0;"), 1);
    assert_eq!(
        run("var t = 0; [1].forEach(function () { t = this === undefined ? 1 : 0; }); t;"),
        1
    );
    assert_eq!(
        run(
            r#"var t = 0; var m = new Map(); m.set("a", 1); m.forEach(function () { t = this === undefined ? 1 : 0; }); t;"#
        ),
        1
    );
    assert_eq!(
        run("var o = {f: function () { return this; }}; var fs = [o.f]; fs[0]() === fs ? 1 : 0;"),
        1
    );
}

#[test]
fn this_in_nested_ordinary_functions_and_arrows() {
    assert_eq!(
        run(
            "var o = {x: 1, f: function () { function inner() { return this; } return inner() === undefined ? 1 : 0; }}; o.f();"
        ),
        1
    );
    assert_eq!(
        run(
            "var o = {x: 1, f: function () { var g = function () { return this === undefined ? 1 : 0; }; return g(); }}; o.f();"
        ),
        1
    );
    assert_eq!(
        run("var o = {x: 4, f: function () { return (() => this.x)(); }}; o.f();"),
        4
    );
    assert_eq!(
        run("var o = {x: 4, f: function () { var g = () => () => this.x; return g()(); }}; o.f();"),
        4
    );
    assert_eq!(
        run("var o = {x: 3, f: function () { return () => this.x; }}; var g = o.f(); g();"),
        3
    );
    assert_eq!(
        run(
            "var o = {x: 1, f: function () { return typeof this.x; }}; o.f() === \"number\" ? 1 : 0;"
        ),
        1
    );
    assert_eq!(
        run("var o = {f: function () { return typeof this; }}; o.f() === \"object\" ? 1 : 0;"),
        1
    );
    assert_eq!(
        run("var o = {x: 2, f: function () { return [1, 2].map(() => this.x).length; }}; o.f();"),
        2
    );
}

#[test]
fn typeof_covers_every_value_kind() {
    assert_eq!(run(r#"typeof 1 === "number" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof -1 === "number" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof NaN === "number" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof (0 / 0) === "number" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof "s" === "string" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof true === "boolean" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof (1 === 1) === "boolean" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof undefined === "undefined" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof null === "object" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof {} === "object" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof [] === "object" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof new Map() === "object" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof function () {} === "function" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof (() => 1) === "function" ? 1 : 0;"#), 1);
    assert_eq!(
        run(r#"var o = {f: function () {}}; typeof o.f === "function" ? 1 : 0;"#),
        1
    );
    assert_eq!(run(r#"typeof [].push === "function" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof new Map().get === "function" ? 1 : 0;"#), 1);
    assert_eq!(
        run(r#"var a = [1, "s"]; typeof a[1] === "string" ? 1 : 0;"#),
        1
    );
    assert_eq!(run(r#"typeof [1, 2] === typeof {} ? 1 : 0;"#), 1);
}

#[test]
fn typeof_builtin_globals_and_undeclared_names() {
    assert_eq!(run(r#"typeof Math === "object" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof Math.PI === "number" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof console === "object" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof Number === "function" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof String === "function" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof Boolean === "function" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof Array === "function" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof Map === "function" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof Object === "function" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof Date === "function" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof isNaN === "function" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof notDefined === "undefined" ? 1 : 0;"#), 1);
    assert_eq!(
        run(r#"typeof (alsoNotDefined) === "undefined" ? 1 : 0;"#),
        1
    );
    assert_eq!(run(r#"typeof typeof notDefined === "string" ? 1 : 0;"#), 1);
    assert_eq!(
        run(r#"var o = {}; typeof o.nope === "undefined" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"function f(a) { return typeof a; } f() === "undefined" ? 1 : 0;"#),
        1
    );
    assert_eq!(run(r#"var x; typeof x === "undefined" ? 1 : 0;"#), 1);
    assert_eq!(
        run(r#"function f() {} typeof f() === "undefined" ? 1 : 0;"#),
        1
    );
}

#[test]
fn typeof_composes_with_operators_and_control_flow() {
    assert_eq!(run(r#"(typeof 1) + 2 === "number2" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"typeof typeof 1 === "string" ? 1 : 0;"#), 1);
    assert_eq!(run(r#"var t = typeof 1; t === "number" ? 1 : 0;"#), 1);
    assert_eq!(
        run(r#"var o = {t: typeof 1}; o.t === "number" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"var t = 0; if (typeof 1 === "number") { t = 1; } t;"#),
        1
    );
    assert_eq!(
        run(r#"var r = 0; switch (typeof 1) { case "number": r = 1; break; default: r = 2; } r;"#),
        1
    );
    assert_eq!(
        run(r#"function f(a) { return typeof a; } f(1) === "number" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"function f(a) { return typeof a; } f("s") === "string" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"function f(a) { return typeof a; } f({}) === "object" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"function f(a) { return typeof a; } f(function () {}) === "function" ? 1 : 0;"#),
        1
    );
    assert_eq!(
        run(r#"var s = "x"; typeof s === "string" && typeof s.length === "number" ? 1 : 0;"#),
        1
    );
    assert_eq!(run(r#"typeof [1, 2].length === "number" ? 1 : 0;"#), 1);
}
