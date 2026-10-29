use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

use sealion_engine::Engine;

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn run(js: &str) -> i32 {
    let engine = Engine::new();
    let mut ir = engine
        .compile(js)
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
        .code()
        .unwrap_or_else(|| panic!("process terminated by signal"))
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
