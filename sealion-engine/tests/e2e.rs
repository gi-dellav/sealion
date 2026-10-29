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
