use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

use sealion_engine::{CompileOptions, Engine, OptLevel};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn run_status_with_options(js: &str, options: &CompileOptions) -> std::process::ExitStatus {
    let engine = Engine::new();
    let mut ir = engine
        .compile_with_options(js, options)
        .unwrap_or_else(|e| panic!("compile failed: {e}\n{js}"));
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("sealion_test_{}_{}.bin", std::process::id(), n));
    ir.emit_binary(&path)
        .unwrap_or_else(|e| panic!("emit failed: {e}\n{}", ir.clif()));
    let status = Command::new(&path)
        .status()
        .unwrap_or_else(|e| panic!("run failed: {e}"));
    let _ = std::fs::remove_file(PathBuf::from(&path));
    status
}

fn run_status(js: &str) -> std::process::ExitStatus {
    run_status_with_options(js, &CompileOptions::default())
}

fn run_with_options(js: &str, options: &CompileOptions) -> i32 {
    run_status_with_options(js, options)
        .code()
        .unwrap_or_else(|| panic!("process terminated by signal"))
}

fn run(js: &str) -> i32 {
    run_status(js)
        .code()
        .unwrap_or_else(|| panic!("process terminated by signal"))
}

fn run_traps(js: &str) {
    let status = run_status(js);
    assert!(!status.success(), "expected runtime failure for {js}");
}

#[test]
fn arithmetic_precedence() {
    assert_eq!(run("1 + 2 * 3;"), 7);
}

#[test]
fn variables_and_assignment() {
    assert_eq!(run("var x = 10; x = x + 5; x;"), 15);
}

#[test]
fn if_else() {
    assert_eq!(run("var x = 0; if (1) { x = 3; } else { x = 4; } x;"), 3);
    assert_eq!(run("var x = 0; if (0) { x = 3; } else { x = 4; } x;"), 4);
}

#[test]
fn while_loop() {
    assert_eq!(
        run("var i = 0; var s = 0; while (i < 5) { s = s + i; i = i + 1; } s;"),
        10
    );
}

#[test]
fn function_call() {
    assert_eq!(run("function add(a, b) { return a + b; } add(2, 3);"), 5);
}

#[test]
fn recursion() {
    assert_eq!(
        run("function fact(n) { if (n <= 1) { return 1; } return n * fact(n - 1); } fact(5);"),
        120
    );
}

#[test]
fn function_hoisting() {
    assert_eq!(run("foo(); function foo() { return 42; }"), 42);
}

#[test]
fn closure_adder() {
    assert_eq!(
        run(
            "function makeAdder(x) { function add(y) { return x + y; } return add; } var add5 = makeAdder(5); add5(3);"
        ),
        8
    );
}

#[test]
fn closure_mutation() {
    assert_eq!(
        run(
            "function makeCounter() { var n = 0; function inc() { n = n + 1; return n; } return inc; } var c = makeCounter(); c(); c(); c();"
        ),
        3
    );
}

#[test]
fn logical_short_circuit() {
    assert_eq!(run("var x = 0; if (0 && (x = 1)) { x = 2; } x;"), 0);
    assert_eq!(run("var x = 0; if (1 || (x = 1)) { x = 2; } x;"), 2);
}

#[test]
fn compound_assignment_and_update() {
    assert_eq!(
        run("var x = 10; x += 5; x -= 2; x *= 3; x /= 2; x %= 7; x;"),
        5
    );
    assert_eq!(run("var x = 1; x++; ++x; x;"), 3);
}

#[test]
fn closures_as_values_and_arguments() {
    assert_eq!(
        run(
            "function apply(f, x) { return f(x); } function inc(y) { return y + 1; } apply(inc, 4);"
        ),
        5
    );
}

#[test]
fn nested_closure_captures_grandparent() {
    assert_eq!(
        run(
            "function outer(x) { function middle() { function inner() { return x; } return inner; } return middle(); } outer(7)();"
        ),
        7
    );
}

#[test]
fn return_inside_while() {
    assert_eq!(
        run("function f() { var i = 0; while (1) { i = i + 1; if (i > 3) { return i; } } } f();"),
        4
    );
}

#[test]
fn var_hoisting() {
    assert_eq!(run("x = 5; var x; x;"), 5);
}

#[test]
fn let_and_const() {
    assert_eq!(run("let x = 2; const y = 3; x + y;"), 5);
}

#[test]
fn unary_and_comparison() {
    assert_eq!(
        run("var x = 0; if (!0 && 3 >= 3 && 2 != 3) { x = 9; } x;"),
        9
    );
    assert_eq!(run("5 - -2;"), 7);
}

#[test]
fn missing_arguments_are_undefined() {
    assert_eq!(
        run("function f(a) { if (a === undefined) { return 7; } return 1; } f();"),
        7
    );
}

#[test]
fn undefined_literal_and_identity() {
    assert_eq!(run("undefined === undefined;"), 1);
    assert_eq!(run("var x; x === undefined;"), 1);
    assert_eq!(run("undefined === null;"), 0);
    assert_eq!(run("undefined === 0;"), 0);
    assert_eq!(run("var x = 0; if (undefined) { x = 1; } x;"), 0);
}

#[test]
fn null_literal_and_identity() {
    assert_eq!(run("null === null;"), 1);
    assert_eq!(run("null === 0;"), 0);
    assert_eq!(run("null !== undefined;"), 1);
    assert_eq!(run("var x = 0; if (null) { x = 1; } x;"), 0);
}

#[test]
fn void_operator() {
    assert_eq!(run("void 0 === undefined;"), 1);
    assert_eq!(run("var x = 0; void (x = 1); x;"), 1);
}

#[test]
fn undefined_implicit_returns() {
    assert_eq!(run("function f() { return; } f() === undefined;"), 1);
    assert_eq!(run("function f() {} f() === undefined;"), 1);
    assert_eq!(run("function f() { undefined; } f() === undefined;"), 1);
}

#[test]
fn functions_are_truthy() {
    assert_eq!(
        run("function f() { return 1; } var x = 6; if (f) { x = 5; } x;"),
        5
    );
}

#[test]
fn break_statement() {
    assert_eq!(
        run("var i = 0; while (1) { i = i + 1; if (i > 3) { break; } } i;"),
        4
    );
}

#[test]
fn continue_statement() {
    assert_eq!(
        run(
            "var i = 0; var s = 0; while (i < 5) { i = i + 1; if (i === 3) { continue; } s = s + i; } s;"
        ),
        12
    );
}

#[test]
fn nested_break_only_exits_inner_loop() {
    assert_eq!(
        run(
            "var i = 0; var s = 0; while (i < 3) { i = i + 1; var j = 0; while (1) { j = j + 1; if (j > 2) { break; } s = s + 10; } s = s + 1; } s;"
        ),
        63
    );
}

#[test]
fn break_outside_loop_is_rejected() {
    assert!(Engine::new().compile("break;").is_err());
    assert!(Engine::new().compile("continue;").is_err());
}

#[test]
fn extra_arguments_are_ignored() {
    assert_eq!(run("function f() { return 3; } f(1, 2, 3);"), 3);
}

#[test]
fn exposes_ir_and_object_api() {
    let engine = Engine::new();
    let mut ir = engine.compile("1 + 1;").unwrap();
    assert!(ir.clif().contains("__script"));
    let object = ir.emit_object().unwrap();
    assert!(!object.is_empty());
    assert!(ir.emit_object().is_err());
}

#[test]
fn exponentiation() {
    assert_eq!(run("2 ** 3;"), 8);
    assert_eq!(run("2 ** 2 ** 2;"), 16);
    assert_eq!(run("var x = 2; x **= 3; x;"), 8);
}

#[test]
fn ternary_operator() {
    assert_eq!(run("1 ? 2 : 3;"), 2);
    assert_eq!(run("0 ? 2 : 3;"), 3);
    assert_eq!(run("0 ? 1 : 0 ? 2 : 3;"), 3);
    assert_eq!(run("var x = 0; 0 ? (x = 1) : (x = 2); x;"), 2);
    assert_eq!(run("var x = 0; 1 ? (x = 1) : (x = 2); x;"), 1);
}

#[test]
fn comma_operator_sequences() {
    assert_eq!(run("var x = (1, 2); x;"), 2);
    assert_eq!(run("var x = 0; var y = (x = 5, 7); x + y;"), 12);
}

#[test]
fn nullish_coalescing() {
    assert_eq!(run("null ?? 5;"), 5);
    assert_eq!(run("undefined ?? 7;"), 7);
    assert_eq!(run("0 ?? 5;"), 0);
    assert_eq!(run("var x = 0; 1 ?? (x = 1); x;"), 0);
    assert_eq!(run("var x = 0; null ?? (x = 1); x;"), 1);
}

#[test]
fn bitwise_operators() {
    assert_eq!(run("1 & 3;"), 1);
    assert_eq!(run("1 | 4;"), 5);
    assert_eq!(run("5 ^ 1;"), 4);
    assert_eq!(run("1 << 4;"), 16);
    assert_eq!(run("(-8 >> 2) === -2;"), 1);
    assert_eq!(run("1 << 32;"), 1);
    assert_eq!(run("(~0) === -1;"), 1);
    assert_eq!(run("(~undefined) === -1;"), 1);
    assert_eq!(run("(-1 >>> 0) === 4294967295;"), 1);
    assert_eq!(run("(1e10 | 0) === 1410065408;"), 1);
}

#[test]
fn assignment_operators() {
    assert_eq!(run("var x = 7; x &= 3; x;"), 3);
    assert_eq!(run("var x = 3; x |= 8; x;"), 11);
    assert_eq!(run("var x = 6; x ^= 1; x;"), 7);
    assert_eq!(run("var x = 1; x <<= 4; x;"), 16);
    assert_eq!(run("var x = 64; x >>= 2; x;"), 16);
    assert_eq!(run("var x = -1; (x >>>= 28) === 15;"), 1);
    assert_eq!(run("var x = 2; x **= 3; x;"), 8);
    assert_eq!(run("var x = 10; x %= 3; x;"), 1);
}

#[test]
fn logical_assignment_operators() {
    assert_eq!(run("var x = 0; x ||= 5; x;"), 5);
    assert_eq!(run("var x = 1; x &&= 7; x;"), 7);
    assert_eq!(run("var x = null; x ??= 3; x;"), 3);
    assert_eq!(run("var x = 4; x ??= 3; x;"), 4);
    assert_eq!(run("var a = 1; a ||= (a = 9); a;"), 1);
    assert_eq!(run("var a = 0; a &&= (a = 9); a;"), 0);
}

#[test]
fn compound_assignment_reads_old_value_first() {
    assert_eq!(run("var x = 1; x += (x = 10); x;"), 11);
    assert_eq!(run("var x = 1; x *= (x = 3); x;"), 3);
}

#[test]
fn loose_equality() {
    assert_eq!(run("null == undefined;"), 1);
    assert_eq!(run("undefined == null;"), 1);
    assert_eq!(run("null != undefined;"), 0);
    assert_eq!(run("null === undefined;"), 0);
    assert_eq!(run("0 == null;"), 0);
    assert_eq!(run("undefined == 0;"), 0);
    assert_eq!(run("1 == 1;"), 1);
    assert_eq!(run("1 == 2;"), 0);
    assert_eq!(run("1 != 2;"), 1);
    assert_eq!(run("null == null;"), 1);
}

#[test]
fn block_scope_shadows() {
    assert_eq!(run("var x = 1; { let x = 2; x = 3; } x;"), 1);
    assert_eq!(run("let x = 1; { let x = 2; } x;"), 1);
    assert_eq!(run("var x = 1; { var x = 2; } x;"), 2);
}

#[test]
fn block_scope_does_not_leak() {
    assert!(Engine::new().compile("{ let y = 1; } y;").is_err());
    assert_eq!(run("var y = 0; { let y = 5; } y;"), 0);
}

#[test]
fn block_scope_nested_blocks() {
    assert_eq!(run("let x = 1; { let x = 2; { let x = 3; } } x;"), 1);
    assert_eq!(
        run("let x = 0; { let y = 1; x = y + 1; { let z = 5; x = x + z; } x = x + y; } x;"),
        8
    );
}

#[test]
fn block_scope_per_iteration_capture() {
    // Each iteration's `let` gets a fresh environment, so the closures keep
    // distinct values.
    assert_eq!(
        run(
            "var f = undefined; var g = undefined; var i = 0; while (i < 2) { let n = i; if (i === 0) { f = function () { return n; }; } else { g = function () { return n; }; } i = i + 1; } f() + g();"
        ),
        1
    );
}

#[test]
fn function_expressions() {
    assert_eq!(run("var f = function (a) { return a + 1; }; f(2);"), 3);
    assert_eq!(run("var f = function () { return 7; }; f();"), 7);
    assert_eq!(
        run("var add = function (a, b) { return a + b; }; add(2, 3);"),
        5
    );
}

#[test]
fn arrow_functions() {
    assert_eq!(run("var inc = (x) => x + 1; inc(4);"), 5);
    assert_eq!(run("var add = (a, b) => a + b; add(2, 3);"), 5);
    assert_eq!(run("var f = () => 9; f();"), 9);
    assert_eq!(
        run("var f = (x) => { var y = x * 2; return y + 1; }; f(5);"),
        11
    );
    assert_eq!(run("var f = x => x; f(3);"), 3);
}

#[test]
fn arrow_functions_capture_closures() {
    assert_eq!(
        run("function make(x) { return (y) => x + y; } var a = make(5); a(3);"),
        8
    );
    assert_eq!(
        run(
            "function counter() { var n = 0; return () => { n = n + 1; return n; }; } var c = counter(); c(); c(); c();"
        ),
        3
    );
    assert_eq!(
        run("function outer(x) { return () => { return () => x; }; } outer(7)()();"),
        7
    );
    assert_eq!(
        run("function apply(f, x) { return f(x); } apply((y) => y * 3, 4);"),
        12
    );
}

#[test]
fn arrow_function_inside_block_scope() {
    assert_eq!(
        run("var f = undefined; { let a = 4; f = () => a * 2; } f();"),
        8
    );
    assert_eq!(run("{ let a = 1; } var g = () => 2; g();"), 2);
}

#[test]
fn number_predicates() {
    assert_eq!(run("Number.isNaN(Number.NaN);"), 1);
    assert_eq!(run("Number.isNaN(0 / 0);"), 1);
    assert_eq!(run("Number.isNaN(1);"), 0);
    assert_eq!(run("Number.isNaN(undefined);"), 0);
    assert_eq!(run("Number.isNaN(null);"), 0);
    assert_eq!(run("isNaN(0 / 0);"), 1);
    assert_eq!(run("isNaN(4);"), 0);

    assert_eq!(run("Number.isFinite(1);"), 1);
    assert_eq!(run("Number.isFinite(Number.POSITIVE_INFINITY);"), 0);
    assert_eq!(run("Number.isFinite(Number.NaN);"), 0);
    assert_eq!(run("Number.isFinite(undefined);"), 0);
    assert_eq!(run("isFinite(0 - 1);"), 1);
    assert_eq!(run("isFinite(1 / 0);"), 0);

    assert_eq!(run("Number.isInteger(3);"), 1);
    assert_eq!(run("Number.isInteger(-3);"), 1);
    assert_eq!(run("Number.isInteger(3.5);"), 0);
    assert_eq!(run("Number.isInteger(Number.NaN);"), 0);
    assert_eq!(run("Number.isInteger(1 / 0);"), 0);
    assert_eq!(run("Number.isInteger(undefined);"), 0);

    assert_eq!(run("Number.isSafeInteger(9007199254740991);"), 1);
    assert_eq!(run("Number.isSafeInteger(9007199254740992);"), 0);
    assert_eq!(run("Number.isSafeInteger(0.5);"), 0);
}

#[test]
fn number_constants() {
    assert_eq!(run("Number.MAX_VALUE > 1e308;"), 1);
    assert_eq!(run("Number.MIN_VALUE > 0;"), 1);
    assert_eq!(run("Number.MIN_VALUE < 1e-300;"), 1);
    assert_eq!(run("Number.MAX_SAFE_INTEGER === 9007199254740991;"), 1);
    assert_eq!(run("Number.MIN_SAFE_INTEGER === -9007199254740991;"), 1);
    assert_eq!(run("Number.EPSILON > 0 && Number.EPSILON < 1e-15;"), 1);
    assert_eq!(run("Number.NaN === Number.NaN;"), 0);
    assert_eq!(run("Number.isNaN(Number.NaN);"), 1);
    assert_eq!(run("Number.NEGATIVE_INFINITY < 0;"), 1);
    assert_eq!(run("Number.POSITIVE_INFINITY > 1e308;"), 1);
    assert_eq!(run("Number.POSITIVE_INFINITY === 1 / 0;"), 1);
}

#[test]
fn math_rounding() {
    assert_eq!(run("Math.abs(-3);"), 3);
    assert_eq!(run("Math.abs(3);"), 3);
    assert_eq!(run("Math.ceil(1.2);"), 2);
    assert_eq!(run("Math.ceil(-1.2) === -1;"), 1);
    assert_eq!(run("Math.floor(1.8);"), 1);
    assert_eq!(run("Math.floor(-1.2) === -2;"), 1);
    assert_eq!(run("Math.round(1.5);"), 2);
    assert_eq!(run("Math.round(-1.5) === -1;"), 1);
    assert_eq!(run("Math.round(2.4);"), 2);
    assert_eq!(run("Math.round(-2.6) === -3;"), 1);
    assert_eq!(run("Math.trunc(1.7);"), 1);
    assert_eq!(run("Math.trunc(-1.7) === -1;"), 1);
    assert_eq!(run("Math.fround(1.337) === 1.3370000123977661;"), 1);
    assert_eq!(
        run("Math.abs(Math.fround(0.1) - 0.10000000149011612) < 1e-20;"),
        1
    );
}

#[test]
fn math_min_max() {
    assert_eq!(run("Math.min(3, 1);"), 1);
    assert_eq!(run("Math.max(3, 1);"), 3);
    assert_eq!(run("Math.min(5);"), 5);
    assert_eq!(run("Math.max(-1, -2, -3) === -1;"), 1);
    assert_eq!(run("Math.min(1, 2, -3, 4) === -3;"), 1);
    assert_eq!(run("Math.min() === Number.POSITIVE_INFINITY;"), 1);
    assert_eq!(run("Math.max() === Number.NEGATIVE_INFINITY;"), 1);
    assert_eq!(run("Number.isNaN(Math.max(1, 0 / 0));"), 1);
    assert_eq!(run("Number.isNaN(Math.min(0 / 0, 1));"), 1);
}

#[test]
fn math_powers_and_logs() {
    assert_eq!(run("Math.pow(2, 10) === 1024;"), 1);
    assert_eq!(run("Math.sqrt(16);"), 4);
    assert_eq!(run("Math.abs(Math.sqrt(2) * Math.sqrt(2) - 2) < 1e-15;"), 1);
    assert_eq!(run("Math.cbrt(27);"), 3);
    assert_eq!(run("Math.exp(0);"), 1);
    assert_eq!(run("Math.expm1(0);"), 0);
    assert_eq!(run("Math.round(Math.log(Math.E));"), 1);
    assert_eq!(run("Math.log1p(0);"), 0);
    assert_eq!(run("Math.log2(8);"), 3);
    assert_eq!(run("Math.log10(1000);"), 3);
    assert_eq!(run("Math.abs(Math.LN2 - Math.log(2)) < 1e-15;"), 1);
    assert_eq!(run("Math.abs(Math.LN10 - Math.log(10)) < 1e-15;"), 1);
    assert_eq!(run("Math.abs(Math.LOG2E * Math.LN2 - 1) < 1e-15;"), 1);
    assert_eq!(run("Math.abs(Math.LOG10E * Math.LN10 - 1) < 1e-15;"), 1);
    assert_eq!(run("Math.abs(Math.SQRT1_2 - Math.sqrt(0.5)) < 1e-15;"), 1);
    assert_eq!(run("Math.abs(Math.SQRT2 - Math.sqrt(2)) < 1e-15;"), 1);
    assert_eq!(run("Math.PI > 3.14 && Math.PI < 3.15;"), 1);
    assert_eq!(run("Math.E > 2.71 && Math.E < 2.72;"), 1);
}

#[test]
fn math_trigonometry() {
    assert_eq!(run("Math.sin(0);"), 0);
    assert_eq!(run("Math.cos(0);"), 1);
    assert_eq!(run("Math.tan(0);"), 0);
    assert_eq!(run("Math.asin(0);"), 0);
    assert_eq!(run("Math.acos(1);"), 0);
    assert_eq!(run("Math.atan(0);"), 0);
    assert_eq!(run("Math.round(Math.atan2(1, 1) * 4);"), 3);
    assert_eq!(run("Math.sinh(0);"), 0);
    assert_eq!(run("Math.cosh(0);"), 1);
    assert_eq!(run("Math.tanh(0);"), 0);
    assert_eq!(run("Math.asinh(0);"), 0);
    assert_eq!(run("Math.acosh(1);"), 0);
    assert_eq!(run("Math.atanh(0);"), 0);
    assert_eq!(run("Math.abs(Math.sin(Math.PI)) < 1e-15;"), 1);
    assert_eq!(
        run("Math.abs(Math.tanh(1) - Math.sinh(1) / Math.cosh(1)) < 1e-15;"),
        1
    );
}

#[test]
fn math_integer_helpers() {
    assert_eq!(run("Math.clz32(1);"), 31);
    assert_eq!(run("Math.clz32(0);"), 32);
    assert_eq!(run("Math.clz32(-1);"), 0);
    assert_eq!(run("Math.imul(3, 4);"), 12);
    assert_eq!(run("Math.imul(-1, 5) === -5;"), 1);
    assert_eq!(run("Math.imul(0xffffffff, 5) === -5;"), 1);
    assert_eq!(run("Math.sign(-3) === -1;"), 1);
    assert_eq!(run("Math.sign(3);"), 1);
    assert_eq!(run("Math.sign(0);"), 0);
    assert_eq!(run("Number.isNaN(Math.sign(0 / 0));"), 1);
}

#[test]
fn builtins_in_expressions() {
    assert_eq!(run("var x = -4; Math.abs(x) + Math.max(x, 2);"), 6);
    assert_eq!(
        run("function f(n) { return Math.floor(Math.sqrt(n)); } f(10);"),
        3
    );
    assert_eq!(
        run("var i = 0; var s = 0; while (i < 3) { s = s + Math.pow(2, i); i = i + 1; } s;"),
        7
    );
    assert_eq!(run("Math.max(Number.isNaN(1) ? 1 : 5, 2);"), 5);
}

#[test]
fn builtins_shadowing_and_errors() {
    assert_eq!(run("var Math = 7; Math;"), 7);
    assert!(Engine::new().compile("Math.abs;").is_err());
    assert!(Engine::new().compile("Math.nope(1);").is_err());
    assert!(Engine::new().compile("Number.PI;").is_err());
    assert!(Engine::new().compile("var m = Math;").is_err());
    assert!(Engine::new().compile("Math['abs'](1);").is_err());
}

#[test]
fn for_loop_basics() {
    assert_eq!(
        run("var s = 0; for (var i = 0; i < 5; i++) { s = s + i; } s;"),
        10
    );
    assert_eq!(
        run("var s = 0; for (let i = 0; i < 5; i++) { s = s + i; } s;"),
        10
    );
    assert_eq!(
        run("var s = 0; for (let i = 0; i < 4; i++) s = s + i; s;"),
        6
    );
    assert_eq!(run("let i = 9; for (let i = 0; i < 2; i++) {} i;"), 9);
    assert_eq!(
        run(
            "function fact(n) { let p = 1; for (let i = 2; i <= n; i++) { p = p * i; } return p; } fact(5);"
        ),
        120
    );
}

#[test]
fn for_loop_optional_clauses() {
    assert_eq!(
        run("var i = 0; for (;;) { i = i + 1; if (i > 3) { break; } } i;"),
        4
    );
    assert_eq!(run("var i = 0; for (; i < 3; i++) {} i;"), 3);
    assert_eq!(run("var i = 0; for (; i < 3;) { i = i + 1; } i;"), 3);
    assert_eq!(
        run("var s = 0; for (var i = 0, j = 3; i < j; i++) { s = s + 1; } s;"),
        3
    );
}

#[test]
fn for_loop_control_flow() {
    assert_eq!(
        run("var s = 0; for (let i = 0; i < 5; i++) { if (i === 2) { continue; } s = s + i; } s;"),
        8
    );
    assert_eq!(
        run("var s = 0; for (let i = 0; i < 10; i++) { if (i === 3) { break; } s = s + i; } s;"),
        3
    );
    assert_eq!(
        run(
            "function f() { for (let i = 0; i < 10; i++) { if (i > 2) { return i; } } return 0; } f();"
        ),
        3
    );
    assert_eq!(
        run(
            "var s = 0; for (let i = 0; i < 3; i++) { for (let j = 0; j < 2; j++) { s = s + 1; } } s;"
        ),
        6
    );
    assert_eq!(
        run(
            "var s = 0; for (let i = 0; i < 3; i++) { let j = 0; while (1) { j = j + 1; if (j > 1) { break; } s = s + 10; } s = s + i; } s;"
        ),
        33
    );
}

#[test]
fn for_loop_per_iteration_bindings() {
    assert_eq!(
        run(
            "var f = undefined; var g = undefined; for (let i = 0; i < 2; i++) { if (i === 0) { f = function () { return i; }; } else { g = function () { return i; }; } } f() + g();"
        ),
        1
    );
    assert_eq!(
        run("var s = 0; for (let i = 0; i < 3; i++) { let n = i * 2; s = s + n; } s;"),
        6
    );
}

#[test]
fn for_loop_scope_does_not_leak() {
    assert!(
        Engine::new()
            .compile("for (let i = 0; i < 1; i++) {} i;")
            .is_err()
    );
    assert!(
        Engine::new()
            .compile("for (const i = 0; i < 1;) {} i;")
            .is_err()
    );
    assert_eq!(run("for (var i = 0; i < 1; i++) {} i;"), 1);
}

#[test]
fn switch_matches_and_falls_through() {
    assert_eq!(
        run("var x = 0; switch (2) { case 1: x = 1; break; case 2: x = 2; break; } x;"),
        2
    );
    assert_eq!(run("var x = 0; switch (9) { case 1: x = 1; break; } x;"), 0);
    assert_eq!(
        run(
            "var x = 0; switch (2) { case 1: x = 1; case 2: x = x + 2; case 3: x = x + 3; break; case 4: x = x + 4; } x;"
        ),
        5
    );
    assert_eq!(run("var x = 0; switch (1) { } x;"), 0);
}

#[test]
fn switch_default() {
    assert_eq!(
        run("var x = 0; switch (9) { case 1: x = 1; break; default: x = 7; } x;"),
        7
    );
    assert_eq!(
        run("var x = 0; switch (1) { case 1: x = 1; break; default: x = 7; } x;"),
        1
    );
    assert_eq!(
        run("var x = 0; switch (9) { case 1: x = 1; default: x = x + 7; case 2: x = x + 1; } x;"),
        8
    );
}

#[test]
fn switch_evaluation_order_and_strict_equality() {
    assert_eq!(
        run(
            "var n = 0; function next() { n = n + 1; return n; } switch (next()) { case 1: break; } n;"
        ),
        1
    );
    assert_eq!(
        run(
            "var n = 0; function t(v) { n = n + 1; return v; } switch (2) { case t(1): break; case t(2): break; case t(3): break; } n;"
        ),
        2
    );
    assert_eq!(
        run("var x = 0; switch (0) { case null: x = 1; break; default: x = 2; } x;"),
        2
    );
    assert_eq!(
        run("var x = 0; switch (undefined) { case undefined: x = 1; break; default: x = 2; } x;"),
        1
    );
}

#[test]
fn switch_control_flow() {
    assert_eq!(
        run(
            "var i = 0; var s = 0; while (i < 3) { i = i + 1; switch (i) { case 2: continue; default: s = s + i; } } s;"
        ),
        4
    );
    assert_eq!(
        run(
            "var i = 0; var s = 0; while (i < 3) { i = i + 1; switch (i) { case 2: break; default: s = s + i; } } s;"
        ),
        4
    );
    assert_eq!(
        run("function f(x) { switch (x) { case 1: return 10; default: return 20; } } f(1) + f(2);"),
        30
    );
}

#[test]
fn switch_shares_its_block_scope() {
    assert_eq!(
        run("switch (1) { case 1: let y = 5; var z = y + 1; break; } z;"),
        6
    );
    assert!(
        Engine::new()
            .compile("switch (1) { case 1: let y = 5; } y;")
            .is_err()
    );
    assert_eq!(
        run("let y = 1; switch (1) { case 1: let y = 2; var z = y; } z + y;"),
        3
    );
}

#[test]
fn const_cannot_be_reassigned() {
    assert_eq!(run("const x = 2; const y = 3; x * y;"), 6);
    assert!(Engine::new().compile("const x = 1; x = 2;").is_err());
    assert!(Engine::new().compile("const x = 1; x += 2;").is_err());
    assert!(Engine::new().compile("const x = 1; x++;").is_err());
    assert!(Engine::new().compile("const x = 1; --x;").is_err());
    assert!(Engine::new().compile("const x = 1; x ||= 2;").is_err());
    assert!(
        Engine::new()
            .compile("const c = 1; function f() { c = 2; }")
            .is_err()
    );
    assert!(
        Engine::new()
            .compile("function f() { const c = 1; c = 2; }")
            .is_err()
    );
}

#[test]
fn const_shadowing() {
    assert_eq!(run("const x = 1; { let x = 2; x = 3; } x;"), 1);
    assert_eq!(run("let x = 1; { const x = 2; } x = 5; x;"), 5);
    assert_eq!(
        run("const x = 1; function f(x) { x = x + 1; return x; } f(5) + x;"),
        7
    );
    assert_eq!(
        run("var s = 0; for (const i = 9; s < 1; s++) { s = s + i; } s;"),
        10
    );
}

#[test]
fn do_while_loops() {
    assert_eq!(run("var i = 0; do { i = i + 1; } while (i < 3); i;"), 3);
    assert_eq!(run("var i = 0; do { i = 1; } while (0); i;"), 1);
    assert_eq!(
        run(
            "var i = 0; var s = 0; do { i = i + 1; if (i === 2) { continue; } s = s + i; } while (i < 3); s;"
        ),
        4
    );
    assert_eq!(
        run("var i = 0; do { i = i + 1; if (i === 2) { break; } } while (1); i;"),
        2
    );
}

#[test]
fn labelled_break_and_continue() {
    assert_eq!(
        run(
            "var i = 0; var s = 0; outer: while (i < 5) { i = i + 1; var j = 0; inner: while (j < 5) { j = j + 1; if (j === 2) { continue outer; } s = s + 1; } } s;"
        ),
        5
    );
    assert_eq!(
        run(
            "var i = 0; outer: for (; i < 5; i = i + 1) { for (var j = 0; j < 5; j = j + 1) { if (j === 1) { break outer; } } } i;"
        ),
        0
    );
    assert_eq!(
        run(
            "var i = 0; var s = 0; outer: do { i = i + 1; if (i === 2) { continue outer; } s = s + i; } while (i < 3); s;"
        ),
        4
    );
    assert_eq!(
        run(
            "var s = 0; outer: for (var i = 0; i < 3; i = i + 1) { for (var j = 0; j < 3; j = j + 1) { if (j === 1) { continue outer; } s = s + 1; } } s;"
        ),
        3
    );
}

#[test]
fn labelled_blocks() {
    assert_eq!(
        run("var x = 0; block: { x = 1; break block; x = 2; } x;"),
        1
    );
    assert_eq!(run("var x = 0; a: b: { x = 1; break a; x = 2; } x;"), 1);
    assert_eq!(
        run("var x = 0; lab: switch (1) { case 1: x = 1; break lab; default: x = 2; } x;"),
        1
    );
    assert_eq!(
        run("var i = 0; while (i < 3) { i = i + 1; lab: { if (i === 2) { break; } } } i;"),
        2
    );
    assert!(Engine::new().compile("a: { continue a; }").is_err());
    assert!(Engine::new().compile("break nope;").is_err());
}

#[test]
fn debugger_is_a_noop() {
    assert_eq!(run("var x = 1; debugger; x;"), 1);
    assert_eq!(run("debugger; 2;"), 2);
}

#[test]
fn default_parameters() {
    assert_eq!(run("function f(a = 5) { return a; } f() + f(2);"), 7);
    assert_eq!(run("function f(a, b = a + 1) { return b; } f(2);"), 3);
    assert_eq!(run("function f(a = 5) { return a; } f(undefined);"), 5);
    assert_eq!(
        run("var n = 0; function f(a = (n = n + 1)) { return a; } f(1); f(); n;"),
        1
    );
    assert_eq!(
        run("var f = function (g = function () { return 7; }) { return g(); }; f();"),
        7
    );
    assert_eq!(run("function f(g = () => 8) { return g(); } f();"), 8);
    assert_eq!(run("function f(a, g = () => a) { return g(); } f(9);"), 9);
    assert_eq!(
        run("function f(a = 1, b = a + 1) { return a + b; } f();"),
        3
    );
    assert_eq!(run("function f(a = 5) { return a === null; } f(null);"), 1);
    assert_eq!(run("function f(a = 5) { return a === 0; } f(0);"), 1);
    assert_eq!(
        run("function f(a = 5) { return a === false; } f(false);"),
        1
    );
}

#[test]
fn default_parameters_have_a_tdz() {
    run_traps("function f(a = b, b = 1) { return a; } f();");
    run_traps("function f(a = a) { return a; } f();");
}

#[test]
fn let_and_const_have_a_tdz() {
    run_traps("x; let x = 1;");
    run_traps("let x = x;");
    run_traps("const x = x;");
    run_traps("x = 1; let x;");
    run_traps("let x = 1; { x; let x = 2; }");
    assert_eq!(run("let x; x === undefined;"), 1);
    assert_eq!(run("let x; x = 5; x;"), 5);
    assert_eq!(run("let x = 1; x;"), 1);
    assert_eq!(
        run("let x = 1; switch (x) { case 1: let x = 2; break; } x;"),
        1
    );
    assert_eq!(
        run("var r = 0; switch (1) { case 1: let x = 2; r = x; } r;"),
        2
    );
    run_traps("switch (1) { case 1: x; let x = 2; }");
}

#[test]
fn math_signed_zero() {
    assert_eq!(run("1 / Math.min(0, -0) === Number.NEGATIVE_INFINITY;"), 1);
    assert_eq!(run("1 / Math.min(-0, 0) === Number.NEGATIVE_INFINITY;"), 1);
    assert_eq!(run("1 / Math.max(0, -0) === Number.POSITIVE_INFINITY;"), 1);
    assert_eq!(run("1 / Math.max(-0, 0) === Number.POSITIVE_INFINITY;"), 1);
    assert_eq!(run("1 / Math.round(-0.4) === Number.NEGATIVE_INFINITY;"), 1);
    assert_eq!(run("1 / Math.round(-0.5) === Number.NEGATIVE_INFINITY;"), 1);
    assert_eq!(run("1 / Math.sign(-0) === Number.NEGATIVE_INFINITY;"), 1);
    assert_eq!(run("1 / Math.min(-0, -0) === Number.NEGATIVE_INFINITY;"), 1);
    assert_eq!(run("1 / Math.max(-0, -0) === Number.NEGATIVE_INFINITY;"), 1);
    assert_eq!(run("Number.isNaN(Math.min(NaN, -0));"), 1);
    assert_eq!(run("Number.isNaN(Math.max(-0, NaN));"), 1);
}

#[test]
fn math_arguments_are_coerced() {
    assert_eq!(run("Math.abs(null);"), 0);
    assert_eq!(run("Number.isNaN(Math.abs(undefined));"), 1);
    assert_eq!(run("Number.isNaN(Math.abs(function () {}));"), 1);
    assert_eq!(run("Math.min(null, 1);"), 0);
    assert_eq!(run("Number.isNaN(Math.min(undefined, 1));"), 1);
    assert_eq!(run("Math.max(null, -1);"), 0);
    assert_eq!(run("Math.round(null);"), 0);
    assert_eq!(run("Math.sign(null);"), 0);
    assert_eq!(run("Math.pow(null, 2);"), 0);
    assert_eq!(run("Math.atan2(null, 1);"), 0);
    assert_eq!(run("Math.floor(true);"), 1);
    assert_eq!(run("Math.abs(false);"), 0);
    assert_eq!(run("Math.sqrt(null);"), 0);
}

#[test]
fn arithmetic_and_relational_coercion() {
    assert_eq!(run("null + 1;"), 1);
    assert_eq!(run("null - 1 === -1;"), 1);
    assert_eq!(run("null * 3;"), 0);
    assert_eq!(run("null / 2;"), 0);
    assert_eq!(run("null % 2;"), 0);
    assert_eq!(run("null ** 2;"), 0);
    assert_eq!(run("null < 1;"), 1);
    assert_eq!(run("null >= 0;"), 1);
    assert_eq!(run("null <= 0;"), 1);
    assert_eq!(run("undefined < 1;"), 0);
    assert_eq!(run("Number.isNaN(undefined + 1);"), 1);
    assert_eq!(run("+null;"), 0);
    assert_eq!(run("1 / -null === Number.NEGATIVE_INFINITY;"), 1);
    assert_eq!(run("+true;"), 1);
    assert_eq!(run("+false;"), 0);
    assert_eq!(run("var x = null; x++; x;"), 1);
    assert_eq!(run("var x = null; var y = x++; y + x;"), 1);
    assert_eq!(run("var x = null; x += 2; x;"), 2);
    assert_eq!(run("var x = true; x++; x;"), 2);
    assert_eq!(run("true + 1;"), 2);
}

#[test]
fn global_nan_and_infinity() {
    assert_eq!(run("Number.isNaN(NaN);"), 1);
    assert_eq!(run("Infinity === Number.POSITIVE_INFINITY;"), 1);
    assert_eq!(run("-Infinity === Number.NEGATIVE_INFINITY;"), 1);
    assert_eq!(run("1 / Infinity;"), 0);
    assert_eq!(run("Number.isNaN(NaN + 1);"), 1);
    assert!(Engine::new().compile("undefined = 1;").is_err());
    assert!(Engine::new().compile("NaN = 1;").is_err());
    assert_eq!(run("{ let undefined = 5; undefined; }"), 5);
    assert_eq!(run("function f(undefined) { return undefined; } f(5);"), 5);
}

#[test]
fn booleans_are_distinct_and_coerce() {
    assert_eq!(run("true === 1;"), 0);
    assert_eq!(run("false === 0;"), 0);
    assert_eq!(run("true === true;"), 1);
    assert_eq!(run("false === false;"), 1);
    assert_eq!(run("true == 1;"), 1);
    assert_eq!(run("false == 0;"), 1);
    assert_eq!(run("true == 2;"), 0);
    assert_eq!(run("false == null;"), 0);
    assert_eq!(run("true + 1;"), 2);
}

#[test]
fn global_boolean_and_number_functions() {
    assert_eq!(run("Number(true);"), 1);
    assert_eq!(run("Number(false);"), 0);
    assert_eq!(run("Number(null);"), 0);
    assert_eq!(run("Number.isNaN(Number(undefined));"), 1);
    assert_eq!(run("Number(3);"), 3);
    assert_eq!(run("Boolean(1);"), 1);
    assert_eq!(run("Boolean(0);"), 0);
    assert_eq!(run("Boolean(null);"), 0);
    assert_eq!(run("Boolean(undefined);"), 0);
    assert_eq!(run("Boolean(function () {});"), 1);
    assert_eq!(run("Boolean();"), 0);
    assert_eq!(run("Number.isNaN(Number());"), 1);
}

#[test]
fn global_is_nan_and_is_finite_coerce() {
    assert_eq!(run("isNaN(undefined);"), 1);
    assert_eq!(run("isNaN(null);"), 0);
    assert_eq!(run("isNaN(true);"), 0);
    assert_eq!(run("isFinite(null);"), 1);
    assert_eq!(run("isFinite(undefined);"), 0);
    assert_eq!(run("isFinite(true);"), 1);
    assert_eq!(run("Number.isNaN(undefined);"), 0);
    assert_eq!(run("Number.isFinite(null);"), 0);
}

#[test]
fn named_function_expressions() {
    assert_eq!(
        run(
            "var f = function fact(n) { if (n <= 1) { return 1; } return n * fact(n - 1); }; f(5);"
        ),
        120
    );
    assert_eq!(run("var f = function g() { return g === f; }; f();"), 1);
    assert_eq!(run("var f = function g() { return g; }; f === f();"), 1);
    assert_eq!(
        run(
            "function make() { return function inner(n) { if (n === 0) { return 1; } return inner(n - 1); }; } make()(3);"
        ),
        1
    );
    assert_eq!(
        run("var f = function g(a = g) { return a === g; }; f();"),
        1
    );
}

#[test]
fn object_is() {
    assert_eq!(run("Object.is(NaN, NaN);"), 1);
    assert_eq!(run("Object.is(0, -0);"), 0);
    assert_eq!(run("Object.is(-0, -0);"), 1);
    assert_eq!(run("Object.is(0, 0);"), 1);
    assert_eq!(run("Object.is(1, 2);"), 0);
    assert_eq!(run("Object.is(null, null);"), 1);
    assert_eq!(run("Object.is(null, undefined);"), 0);
    assert_eq!(run("Object.is(true, true);"), 1);
    assert_eq!(run("Object.is(true, 1);"), 0);
    assert_eq!(run("var f = function () {}; Object.is(f, f);"), 1);
    assert_eq!(run("Object.is(function () {}, function () {});"), 0);
    assert_eq!(run("Object.is();"), 1);
    assert!(Engine::new().compile("Math.is(1, 1);").is_err());
}

#[test]
fn math_hypot() {
    assert_eq!(run("Math.hypot(3, 4);"), 5);
    assert_eq!(run("Math.hypot();"), 0);
    assert_eq!(run("Math.hypot(5);"), 5);
    assert_eq!(run("Math.hypot(-3, -4);"), 5);
    assert_eq!(run("Math.hypot(3, 4, 12);"), 13);
    assert_eq!(run("Math.hypot(null, 3, 4);"), 5);
    assert_eq!(run("Math.hypot(true, 0);"), 1);
    assert_eq!(run("Math.hypot(3, 4) === Math.hypot(4, 3);"), 1);
    assert_eq!(run("Number.isNaN(Math.hypot(NaN, 1));"), 1);
    assert_eq!(run("Math.hypot(Infinity, NaN) === Infinity;"), 1);
    assert_eq!(run("1 / Math.hypot(-0) === Infinity;"), 1);
    assert_eq!(run("Math.hypot(1e308, 1e308) < Infinity;"), 1);
}

#[test]
fn math_random_and_date_now() {
    assert_eq!(run("var r = Math.random(); r >= 0 && r < 1;"), 1);
    assert_eq!(run("var r = Math.random(); r !== 1;"), 1);
    assert_eq!(run("Date.now() > 1000000000000;"), 1);
    assert_eq!(run("Date.now() % 1 === 0;"), 1);
}

#[test]
fn math_f16round() {
    assert_eq!(run("Math.f16round(1.5) === 1.5;"), 1);
    assert_eq!(run("Math.f16round(1.1) === 1.099609375;"), 1);
    assert_eq!(run("Math.f16round(2 ** -14) === 2 ** -14;"), 1);
    assert_eq!(run("Math.f16round(2 ** -25) === 0;"), 1);
    assert_eq!(run("Math.f16round(3 * 2 ** -25) === 2 ** -23;"), 1);
    assert_eq!(run("Math.f16round(1 + 2 ** -11) === 1;"), 1);
    assert_eq!(run("Math.f16round(1 + 3 * 2 ** -11) === 1 + 2 ** -9;"), 1);
    assert_eq!(run("Math.f16round(2049) === 2048;"), 1);
    assert_eq!(run("Math.f16round(65504) === 65504;"), 1);
    assert_eq!(run("Math.f16round(65519) === 65504;"), 1);
    assert_eq!(run("Math.f16round(65520) === Infinity;"), 1);
    assert_eq!(run("Math.f16round(-65520) === -Infinity;"), 1);
    assert_eq!(run("Math.f16round(2 ** 20) === Infinity;"), 1);
    assert_eq!(run("Math.f16round(-Infinity) === -Infinity;"), 1);
    assert_eq!(run("Number.isNaN(Math.f16round(NaN));"), 1);
    assert_eq!(run("Math.f16round(null) === 0;"), 1);
    assert_eq!(run("Math.f16round(1e-8) === 0;"), 1);
    assert_eq!(run("1 / Math.f16round(-0) === -Infinity;"), 1);
}

#[test]
fn delete_expressions() {
    assert_eq!(run("var x = 1; delete x;"), 0);
    assert_eq!(run("var x = 1; delete (x);"), 0);
    assert_eq!(run("var x = 1; delete x; x;"), 1);
    assert_eq!(run("delete 1;"), 1);
    assert_eq!(run("delete (1 + 2);"), 1);
    assert_eq!(run("delete missing;"), 1);
    assert_eq!(run("delete Math;"), 0);
    assert_eq!(run("delete undefined;"), 0);
    assert_eq!(run("delete null?.x;"), 1);
    assert_eq!(
        run("var x = 0; function f() { x = 5; return 1; } delete f(); x;"),
        5
    );
    assert_eq!(run("var x = 0; delete (x = 3); x;"), 3);
    assert!(Engine::new().compile("var o; delete o.x;").is_err());
}

#[test]
fn optional_chaining() {
    assert_eq!(run("var f; f?.();"), 0);
    assert_eq!(run("var f; f?.(1);"), 0);
    assert_eq!(run("var f; f?.() === undefined;"), 1);
    assert_eq!(run("var f = function (x) { return x + 1; }; f?.(1);"), 2);
    assert_eq!(
        run("var f = function (a, b) { return a + b; }; f?.(2, 3);"),
        5
    );
    assert_eq!(run("var f; var n = 0; f?.(n = 1); n;"), 0);
    assert_eq!(run("var n = null; n?.x;"), 0);
    assert_eq!(run("var n = null; n?.x === undefined;"), 1);
    assert_eq!(run("var n = null; n?.x.y === undefined;"), 1);
    assert_eq!(run("var n = null; n?.x();"), 0);
    assert_eq!(run("var n = null; n?.x?.();"), 0);
    assert_eq!(run("undefined?.[0] === undefined;"), 1);
    assert_eq!(run("var a = null; a?.[0] === undefined;"), 1);
    assert_eq!(run("var n = 0; var a = null; a?.[n = 1]; n;"), 0);
    assert_eq!(run("var a = 0; a?.[0] === undefined;"), 1);
    assert_eq!(run("Math?.PI === Math.PI;"), 1);
    assert_eq!(run("Date?.now() > 1000000000000;"), 1);
    assert_eq!(run("var f = null; f?.() ?? 7;"), 7);
    assert_eq!(run("var f = function () { return 1; }; f?.() + 1;"), 2);
    assert_eq!(run("(null)?.x === undefined;"), 1);
}

#[test]
fn throw_catch_basics() {
    assert_eq!(run("try { throw 42; } catch (e) { e; }"), 42);
    assert_eq!(run("var x = 0; try { x = 1; } catch (e) { x = 2; } x;"), 1);
    assert_eq!(run("try { throw 1; } catch (e) { e + 1; }"), 2);
    assert_eq!(
        run("try { throw true; } catch (e) { e === true ? 1 : 0; }"),
        1
    );
    assert_eq!(
        run("try { throw null; } catch (e) { e === null ? 1 : 0; }"),
        1
    );
    assert_eq!(
        run("try { throw undefined; } catch (e) { e === undefined ? 1 : 0; }"),
        1
    );
    assert_eq!(
        run("var f = function () {}; try { throw f; } catch (e) { e === f ? 1 : 0; }"),
        1
    );
    assert_eq!(run("var x = 0; try { throw 1; } catch { x = 2; } x;"), 2);
    assert_eq!(run("var e = 1; try { throw 2; } catch (e) { } e;"), 1);
    assert_eq!(
        run("var f; try { throw 7; } catch (e) { f = function () { return e; }; } f();"),
        7
    );
    assert_eq!(
        run("function f() { throw 7; } try { f(); } catch (e) { e; }"),
        7
    );
    assert_eq!(
        run("function f() { throw 7; } function g() { f(); } try { g(); } catch (e) { e; }"),
        7
    );
    assert_eq!(run("try { } catch (e) { 5; } 3;"), 3);
    assert_eq!(run("try { throw 1; } catch (e) { } 3;"), 3);
    assert_eq!(
        run("function f() { try { throw 1; } catch (e) { return e + 1; } } f();"),
        2
    );
}

#[test]
fn uncaught_throw() {
    assert_eq!(run("throw 1;"), 1);
    assert_eq!(run("function f() { throw 1; } f();"), 1);
    assert_eq!(run("var x = 0; try { throw 1; } finally { x = 2; }"), 1);
}

#[test]
fn finally_semantics() {
    assert_eq!(run("var x = 0; try { x = 1; } finally { x = 2; } x;"), 2);
    assert_eq!(
        run("var x = 0; try { throw 1; } catch (e) { x = 3; } finally { x = 4; } x;"),
        4
    );
    assert_eq!(
        run("var x = 0; try { try { throw 1; } finally { x = 1; } } catch (e) { x = x + e; } x;"),
        2
    );
    assert_eq!(
        run(
            "var x = 0; try { try { throw 1; } finally { x = 1; throw 5; } } catch (e) { x = x + e; } x;"
        ),
        6
    );
    assert_eq!(
        run("var x = 0; function f() { try { return 1; } finally { x = 2; } } f(); x;"),
        2
    );
    assert_eq!(
        run("function f() { try { return 1; } finally { return 2; } } f();"),
        2
    );
    assert_eq!(
        run("var x = 0; function f() { try { return 1; } finally { return 2; } } x = f(); x;"),
        2
    );
    assert_eq!(
        run(
            "var x = 0; function f() { try { try { return 1; } finally { x = 2; } } finally { x = x + 3; } } f(); x;"
        ),
        5
    );
    assert_eq!(
        run(
            "var x = 0; function f() { try { throw 1; } finally { x = 2; } } try { f(); } catch (e) { x = x + e; } x;"
        ),
        3
    );
    assert_eq!(
        run(
            "var x = 0; function f() { throw 1; } function g() { try { f(); } finally { x = 2; } } try { g(); } catch (e) { x = x + e; } x;"
        ),
        3
    );
    assert_eq!(
        run(
            "var x = 0; function a() { try { throw 1; } finally { x = x + 1; } } function b() { try { a(); } finally { x = x + 10; } } try { b(); } catch (e) { x = x + e; } x;"
        ),
        12
    );
    assert_eq!(
        run("var x = 0; while (1) { try { break; } finally { x = 5; } } x;"),
        5
    );
    assert_eq!(
        run(
            "var x = 0; for (var i = 0; i < 3; i++) { try { if (i === 1) continue; x = x + 1; } finally { x = x + 10; } } x;"
        ),
        32
    );
    assert_eq!(
        run(
            "var x = 0; outer: for (var i = 0; i < 3; i++) { try { if (i === 1) break outer; x = x + 1; } finally { x = x + 10; } } x;"
        ),
        21
    );
    assert_eq!(
        run(
            "var x = 0; for (var i = 0; i < 2; i++) { try { continue; } finally { x = x + 1; } } x;"
        ),
        2
    );
    assert_eq!(
        run(
            "var x = 0; switch (1) { case 1: try { break; } finally { x = 3; } default: x = 9; } x;"
        ),
        3
    );
    assert_eq!(
        run(
            "var x = 0; function f() { try { return 1; } finally { x = 2; } } try { f(); } finally { x = x + 10; } x;"
        ),
        12
    );
    assert_eq!(
        run(
            "var x = 0; function f() { try { throw 1; } finally { x = 2; } } try { try { f(); } catch (e) { x = x + e; } } finally { x = x + 10; } x;"
        ),
        13
    );
    assert_eq!(
        run("var f; try { } finally { let y = 3; f = function () { return y; }; } f();"),
        3
    );
    assert_eq!(
        run("function f() { try { throw 1; } finally { return 7; } } f();"),
        7
    );
    assert_eq!(
        run(
            "var x = 0; function f() { try { return 1; } finally { throw 2; } } try { f(); } catch (e) { x = e; } x;"
        ),
        2
    );
    assert_eq!(
        run(
            "var x = 0; function f() { try { throw 1; } catch (e) { return e + 1; } finally { x = 9; } } var y = f(); x;"
        ),
        9
    );
}

#[test]
fn math_hypot_edge_cases() {
    assert_eq!(run("Math.hypot(0) === 0;"), 1);
    assert_eq!(run("1 / Math.hypot(0) === Infinity;"), 1);
    assert_eq!(run("Math.hypot(-Infinity) === Infinity;"), 1);
    assert_eq!(run("Math.hypot(Infinity, 1) === Infinity;"), 1);
    assert_eq!(run("Number.isNaN(Math.hypot(NaN, NaN));"), 1);
    assert_eq!(run("Number.isNaN(Math.hypot(undefined));"), 1);
    assert_eq!(run("Math.hypot(null, null) === 0;"), 1);
    assert_eq!(run("Math.hypot(true, false) === 1;"), 1);
    assert_eq!(run("Math.hypot(1e-320, 1e-320) > 0;"), 1);
    assert_eq!(run("Math.hypot(-3, 4) === 5;"), 1);
}

#[test]
fn math_random_and_date_now_more() {
    assert_eq!(
        run(
            "var i = 0; var ok = 1; while (i < 8) { var r = Math.random(); if (!(r >= 0 && r < 1)) { ok = 0; } i = i + 1; } ok;"
        ),
        1
    );
    assert_eq!(run("Date.now() > 0;"), 1);
    assert_eq!(run("Date.now() < 4102444800000;"), 1);
}

#[test]
fn math_f16round_edge_cases() {
    assert_eq!(run("Math.f16round(65519.999) === 65504;"), 1);
    assert_eq!(run("Math.f16round(-65519) === -65504;"), 1);
    assert_eq!(run("Math.f16round(65536) === Infinity;"), 1);
    assert_eq!(run("Math.f16round(2 ** -24) === 2 ** -24;"), 1);
    assert_eq!(run("Math.f16round(2 ** -25 + 2 ** -50) === 2 ** -24;"), 1);
    assert_eq!(run("1 / Math.f16round(-(2 ** -25)) === -Infinity;"), 1);
    assert_eq!(run("Math.f16round(1.0001) === 1;"), 1);
    assert_eq!(run("Math.f16round(1.0006) === 1.0009765625;"), 1);
    assert_eq!(run("Math.f16round(-1.1) === -1.099609375;"), 1);
    assert_eq!(run("Math.f16round(true) === 1;"), 1);
    assert_eq!(run("Math.f16round(null) === 0;"), 1);
    assert_eq!(run("Number.isNaN(Math.f16round(undefined));"), 1);
    assert_eq!(run("1 / Math.f16round(0) === Infinity;"), 1);
}

#[test]
fn delete_more() {
    assert_eq!(run("function f(x) { return delete x; } f(1);"), 0);
    assert_eq!(run("function f() { } delete f;"), 0);
    assert_eq!(run("delete Number;"), 0);
    assert_eq!(run("delete NaN;"), 0);
    assert_eq!(run("delete Infinity;"), 0);
    assert_eq!(run("delete isNaN;"), 0);
    assert_eq!(run("delete Boolean;"), 0);
    assert_eq!(run("delete (void 0);"), 1);
    assert_eq!(run("delete (function () { });"), 1);
    assert_eq!(run("delete (((missing)));"), 1);
    assert_eq!(run("let x; delete x;"), 0);
    assert_eq!(run("var x = 0; var y = (x = 4); delete y; x;"), 4);
}

#[test]
fn optional_chaining_more() {
    assert_eq!(run("var f = function (x = 5) { return x; }; f?.();"), 5);
    assert_eq!(
        run("var f = function () { return function () { return 3; }; }; f?.()?.();"),
        3
    );
    assert_eq!(run("var n = null; n?.a.b.c === undefined;"), 1);
    assert_eq!(run("var a = null; a?.[0][1] === undefined;"), 1);
    assert_eq!(
        run("var x = 0; var f; function g() { x = 1; } f?.(g()); x;"),
        0
    );
    assert_eq!(
        run(
            "var x = 0; function g() { x = 1; return 1; } var f = function (a) { return a; }; f?.(g()); x;"
        ),
        1
    );
    assert_eq!(run("var f = null; f?.() || 5;"), 5);
    assert_eq!(run("Number?.MAX_VALUE === Number.MAX_VALUE;"), 1);
    assert_eq!(run("Object?.is(1, 1);"), 1);
    assert_eq!(run("Math?.sqrt(9);"), 3);
    assert_eq!(run("Math?.sqrt?.(9);"), 3);
    assert_eq!(run("(5)?.foo === undefined;"), 1);
    assert_eq!(run("delete null?.[0];"), 1);
    assert_eq!(
        run("var x = 0; var f; try { f?.((function () { throw 1; })()); } catch (e) { x = 1; } x;"),
        0
    );
    assert_eq!(
        run(
            "var x = 0; var f = function () { }; try { f?.((function () { throw 1; })()); } catch (e) { x = 1; } x;"
        ),
        1
    );
    assert_eq!(run("var f = function (a) { return a; }; f?.(1) ?? 9;"), 1);
}

#[test]
fn try_catch_finally_more() {
    assert_eq!(
        run(
            "var x = 0; for (var i = 0; i < 2; i++) { try { throw 1; } catch (e) { continue; } finally { x = x + 1; } } x;"
        ),
        2
    );
    assert_eq!(
        run("var x = 0; while (1) { try { throw 1; } catch (e) { break; } finally { x = 5; } } x;"),
        5
    );
    assert_eq!(
        run(
            "var x = 0; function f() { throw 1; } function g() { throw 2; } try { try { f(); } finally { g(); } } catch (e) { x = e; } x;"
        ),
        2
    );
    assert_eq!(
        run(
            "var x = 0; function h() { throw 9; } try { throw 1; } catch (e) { try { h(); } catch (inner) { x = inner; } } x;"
        ),
        9
    );
    assert_eq!(
        run(
            "var x = 0; function h() { throw 9; } try { try { throw 1; } finally { try { h(); } catch (e) { x = e; } } } catch (e) { x = x + e; } x;"
        ),
        10
    );
    assert_eq!(
        run(
            "function boom() { throw 4; } function f(a = boom()) { return a; } try { f(); } catch (e) { e; }"
        ),
        4
    );
    assert_eq!(
        run("var f = () => { throw 3; }; try { f(); } catch (e) { e; }"),
        3
    );
    assert_eq!(
        run(
            "function make() { return function () { throw 5; }; } try { make()(); } catch (e) { e; }"
        ),
        5
    );
    assert_eq!(
        run("function f() { let x = 1; try { return x; } finally { x = 2; } } f();"),
        1
    );
    assert_eq!(
        run("var x = 0; try { var y = 3; } catch (e) { } x = y; x;"),
        3
    );
    assert_eq!(run("try { throw 1; } catch (e) { e = 2; }"), 2);
    assert_eq!(
        run("var x = 0; try { } finally { x = 1; } try { throw 1; } catch (e) { x = x + e; } x;"),
        2
    );
    assert_eq!(
        run("try { throw 1; } catch (e) { try { throw e + 1; } catch (inner) { inner; } }"),
        2
    );
    assert_eq!(
        run(
            "var x = 0; label: for (var i = 0; i < 2; i++) { try { try { continue label; } finally { x = x + 1; } } finally { x = x + 10; } } x;"
        ),
        22
    );
    assert_eq!(
        run(
            "var x = 0; function f() { try { return 1; } finally { try { throw 2; } catch (e) { x = e; } } } var y = f(); x;"
        ),
        2
    );
    assert_eq!(
        run("var x = 0; try { throw 1; } catch (e) { x = e; throw e + 1; }"),
        1
    );
}

#[test]
fn numeric_analysis_with_new_constructs() {
    assert_eq!(
        run("let x = 1; try { throw 1; } catch (e) { x = null; } x + 1;"),
        1
    );
    assert_eq!(run("let x = 1; try { } finally { x = null; } x + 1;"), 1);
    assert_eq!(
        run("let x = 1; var f = function (a) { }; f?.(x = null); x + 1;"),
        1
    );
    assert_eq!(run("let x = 1; var f; f?.(x = null); x + 1;"), 2);
    assert_eq!(run("let x = 1; try { } finally { x = true; } x + 1;"), 2);
    assert_eq!(run("let x = 1; try { } finally { x = false; } x + 1;"), 1);
}

#[test]
fn supports_cranelift_optimization_levels() {
    let none = CompileOptions {
        opt_level: OptLevel::None,
        ..Default::default()
    };
    assert_eq!(run_with_options("1 + 2 * 3;", &none), 7);

    let size = CompileOptions {
        opt_level: OptLevel::SpeedAndSize,
        ..Default::default()
    };
    assert_eq!(run_with_options("1 + 2 * 3;", &size), 7);
}

#[test]
fn supports_cranelift_codegen_toggles() {
    let options = CompileOptions {
        enable_alias_analysis: false,
        enable_verifier: false,
        unwind_info: false,
        preserve_frame_pointers: true,
        ..Default::default()
    };
    assert_eq!(run_with_options("var x = 1; x + 2;", &options), 3);
}

#[test]
fn emits_non_pic_object() {
    let options = CompileOptions {
        is_pic: false,
        ..Default::default()
    };
    let mut ir = Engine::new()
        .compile_with_options("1 + 2;", &options)
        .unwrap();
    assert!(!ir.emit_object().unwrap().is_empty());
}

#[test]
fn accepts_explicit_host_target() {
    let options = CompileOptions {
        target: Some(target_lexicon::Triple::host().to_string()),
        ..Default::default()
    };
    assert_eq!(run_with_options("1 + 2;", &options), 3);
}

#[test]
fn cross_compiles_aarch64_object() {
    let options = CompileOptions {
        target: Some("aarch64-unknown-linux-gnu".to_string()),
        ..Default::default()
    };
    let mut ir = Engine::new()
        .compile_with_options("1 + 2;", &options)
        .unwrap();
    assert!(!ir.emit_object().unwrap().is_empty());
}

#[cfg(target_arch = "x86_64")]
#[test]
fn applies_x86_cpu_preset() {
    let options = CompileOptions {
        target_cpu: Some("x86-64".to_string()),
        ..Default::default()
    };
    assert_eq!(run_with_options("1 + 2;", &options), 3);
}

#[test]
fn rejects_invalid_target() {
    let options = CompileOptions {
        target: Some("not-a-target".to_string()),
        ..Default::default()
    };
    assert!(Engine::new().compile_with_options("1;", &options).is_err());
}
