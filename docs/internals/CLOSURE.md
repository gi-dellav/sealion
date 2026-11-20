# Closure compilation and runtime model

This document describes how `sealion-engine` 0.6.0 plans, materialises, calls and
stores closures. It covers the lexical planning pass that decides which bindings
must live in heap environments, the runtime layout of closures and environment
records, the Cranelift lowering, and the lifetime and limitations of the current
model.

A closure is not a separate JavaScript object type. It is an ordinary
NaN-boxed value (see [VALUE.md](VALUE.md)) tagged with `TAG_FUNCTION` and
carrying a pointer to a 16-byte runtime record:

```text
closure = { code: i64, env: *Environment }
```

The environment is a heap-allocated record whose first word points at the parent
environment. Name lookup is resolved at compile time into a fixed number of
parent-pointer loads plus a fixed slot offset; there is no runtime name lookup.

## 1. Compilation pipeline

Closures pass through three phases:

```text
oxc AST
  -> program plan (plan.rs)     lexical scopes, bindings, capture analysis, function table
  -> Cranelift IR (compiler.rs) environments, closure records, indirect calls
  -> object file / executable   (lib.rs)
```

The planning pass runs before any code is generated. This is what makes
environment depth and slot offsets compile-time constants.

| Phase | Entry point | Responsibility |
| --- | --- | --- |
| Planning | `build_program_plan` (`plan.rs:104`) | Build the scope tree, function plans and capture flags. |
| IR generation | `compile_functions` (`compiler.rs:92`) | Lower each function plan with a uniform ABI. |
| Main generation | `compile_main` (`compiler.rs:227`) | Call `__script` and translate the result / pending exception into an exit code. |
| Emission | `IrModule::emit_object`, `emit_binary` (`lib.rs:313`, `lib.rs:322`) | Emit the object and link with `cc -lm`. |

## 2. Lexical planning

### 2.1 Scope tree

`ScopePlan` (`plan.rs:32`) describes one lexical scope:

- `parent`: the enclosing lexical scope.
- `owner`: the function plan that owns the scope.
- `bindings`: `name -> slot` within the scope.
- `consts`: names that may not be assigned.
- `tdz`: slots that start in the temporal dead zone.
- `captured`: whether the scope may be observed by a nested function.

`let` and `const` declarations bind in the nearest enclosing block scope. A `{}`
block, a `switch` body, a catch clause and a `for` initializer with a lexical
declaration each create a scope. `var` declarations and `function` declarations
are hoisted to the root scope of their function (`plan.rs:290`, `plan.rs:299`,
`plan.rs:480`).

```js
function outer() {
  let a;          // binding in outer's root scope
  var b;          // hoisted to outer's root scope
  { let c; }      // binding in a nested block scope
  function inner() {} // name hoisted to outer's root scope
}
```

Beyond the JavaScript-visible behaviour, the important attribute is `captured`.
A scope with `captured == true` gets a runtime environment record; a scope with
`captured == false` keeps its bindings in Cranelift variables, which the backend
maps to machine registers and stack slots.

### 2.2 Capture analysis

The planner does not compute free variables. It uses a conservative "may be
captured" rule: creating a nested function marks the current scope and **every
lexical ancestor** as captured.

```rust
// plan.rs:147
fn mark_captured(&mut self, start: ScopeId) {
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &mut self.scopes[id.0];
        scope.captured = true;
        current = scope.parent;
    }
}
```

The rule is applied in two places:

- `add_nested_function` (`plan.rs:250`) handles function expressions and arrow
  functions. It builds the child plan with the current scope as parent and then
  calls `mark_captured(scope)`.
- `add_function_declaration` (`plan.rs:480`) hoists the name to the function
  root and builds the child with the root scope as parent, then calls
  `mark_captured(root)`.

Consequences:

- The captured set is upward-closed: if a scope has an environment, all of its
  ancestors have one too. This keeps the runtime environment chain contiguous.
- Sibling scopes are not affected.
- Capture is all-or-nothing per scope: all bindings in a captured scope live in
  the environment, even if the nested function never reads them.
- A function declaration inside a block is deliberately treated as function
  scoped. Its plan's parent is the function root, not the block, so it cannot see
  that block's `let`/`const` bindings.
- A function with no nested functions at all keeps all locals in registers.

### 2.3 Function plans and stable AST identity

Each function or arrow gets a `FunctionPlan` (`plan.rs:64`) with its own root
scope, parameters, body and a table of nested functions:

- `children`: hoisted function declarations, evaluated at function entry.
- `nested`: function and arrow expressions, keyed by the address of the AST
  node.

The key is obtained with `node_key` (`plan.rs:100`):

```rust
pub fn node_key<T>(node: &T) -> usize {
    std::ptr::from_ref(node) as usize
}
```

Plans borrow the parsed AST, and the AST lives for the whole compilation, so node
addresses are stable between planning and code generation. `compile_closure`
uses the same key to find the child plan for a function expression:

```rust
// compiler.rs:2607
fn compile_closure(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'_>,
    key: usize,
) -> Result<Value, Error> {
    let child = *ctx
        .program
        .function(ctx.plan_id)
        .nested
        .get(&key)
        .ok_or_else(|| Error::Codegen("missing anonymous function plan".to_string()))?;
    let func_ref = *ctx
        .child_refs
        .get(&child)
        .ok_or_else(|| Error::Codegen("missing anonymous function reference".to_string()))?;
    let code = builder.ins().func_addr(types::I64, func_ref);
    Ok(new_closure(builder, ctx.calloc_ref, code, ctx.env))
}
```

Generated native function names are `__script` for the top level and `js_fn_N`
for nested functions. These names are only debug and linkage names; they are not
visible to JavaScript.

Named function expressions additionally record `self_binding`
(`plan.rs:80`). For example, `var f = function g() { ... }` creates a binding
named `g` in the function's own root scope. It is initialised from the callee
value passed to every call (section 4.2).

### 2.4 Binding resolution and environment depth

Two different lookups are used:

- `resolve_binding` (`plan.rs:668`) returns the scope that owns a name and its
  slot. `place` uses it to choose between environment and register storage, and
  the builtin/global recognisers use it to check whether a name is shadowed.
  `is_const` and `needs_tdz_check` walk the same scope tree separately for their
  compile-time diagnostics.
- `resolve` (`plan.rs:686`) returns `(depth, slot)`, where `depth` counts only
  scopes that have an environment:

```rust
// plan.rs:686
pub fn resolve(program: &Program<'_>, start: ScopeId, name: &str) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &program.scopes[id.0];
        if let Some(&slot) = scope.bindings.get(name) {
            return Some((depth, slot));
        }
        if scope.has_env() {
            depth += 1;
        }
        current = scope.parent;
    }
    None
}
```

At runtime `place` (`compiler.rs:1019`) starts from the current environment
pointer and follows the parent pointer exactly `depth` times. The slot is then
loaded or stored at offset `8 + 8 * slot`.

Because capture is upward-closed, a `depth` computed statically always matches
the runtime chain.

### 2.5 Temporal dead zone and constness

Parameters and `let`/`const` bindings are registered in the scope's `tdz` set
(`plan.rs:226`, `plan.rs:182`). Their slots are initialised with the
`TAG_UNINITIALIZED` value, and every read of a TDZ binding emits a trap check.
This applies inside closures too: a closure can capture a binding before its
initializer has run, but reading it through the closure traps until the
initializer executes.

`const` names are recorded in `consts` and assignment is rejected during code
generation by `is_const` (`plan.rs:641`).

## 3. Runtime representation

### 3.1 The tagged closure value

A function value uses the reserved quiet-NaN tag `0x7FFD` (see
[VALUE.md](VALUE.md)):

```text
63      48 47                                          0
+----------+--------------------------------------------+
|  0x7FFD  |            closure pointer (48 bits)        |
+----------+--------------------------------------------+
```

`new_closure` (`compiler.rs:766`) allocates and fills the record:

```rust
fn new_closure(
    builder: &mut FunctionBuilder,
    calloc_ref: FuncRef,
    code: Value,
    env: Value,
) -> Value {
    let ptr = call_calloc(builder, calloc_ref, 16);
    builder.ins().store(MemFlags::new(), code, ptr, 0i32);
    builder.ins().store(MemFlags::new(), env, ptr, 8i32);
    value::function(builder, ptr)
}
```

and `value::function` (`value.rs:59`) builds the tagged `f64`:

```rust
pub fn function(builder: &mut FunctionBuilder, ptr: Value) -> Value {
    let tag = builder
        .ins()
        .iconst(types::I64, (TAG_FUNCTION << 48) as i64);
    let bits = builder.ins().bor(tag, ptr);
    builder.ins().bitcast(types::F64, MemFlags::new(), bits)
}
```

The pointer is expected to fit in 48 bits, which is true of user-space addresses
on the supported 64-bit Linux targets. Calling `value::payload`
(`value.rs:68`) strips the tag again.

All those bits are a valid quiet NaN, so a closure can safely pass through code
that treats values as `f64`; however, the compiler always `ToNumber`-coerces
tagged values before arithmetic unless the static numeric analysis proves the
value is an untagged number.

### 3.2 Environment records

An environment record is allocated with `calloc(1, 8 + 8 * slots)`
(`alloc_env`, `compiler.rs:754`):

```text
environment:
  +0                      parent environment pointer (i64)
  +8                      slot 0 (f64, NaN-boxed)
  +16                     slot 1 (f64, NaN-boxed)
  ...
  +8 + 8*(slots-1)        slot slots-1
```

- `parent` is a raw environment pointer, not a JavaScript value.
- Every slot is an `f64` holding a NaN-boxed value.
- The script root environment has parent `0`; a function root environment has as
  parent the environment captured by its closure, not the caller's current
  environment. This is lexical, not dynamic, scoping.
- Slots are initialised to `undefined`, or to `TAG_UNINITIALIZED` when they are
  in the TDZ (`init_slots`, `compiler.rs:778`).

The closure record's `env` field points at the environment that was current when
the closure value was created. Multiple closures over the same activation share
the same record, so mutations are visible through all of them.

### 3.3 Worked memory picture

```js
function makeAdder(x) {
  function add(y) { return x + y; }
  return add;
}
var add5 = makeAdder(5);
```

After `makeAdder(5)` returns:

```text
add5 (NaN-boxed) ──> closure { code = &js_fn_add, env = E_make }
                                      |
E_make: parent = E_script             |
        slot x = 5  <-----------------+
E_script: parent = 0
        slot makeAdder = closure { code = &js_fn_makeAdder, env = E_script }
        slot add5      = the same value as add5 above
```

The `add` closure captures `E_make`, so it observes later writes to `x` made
through other closures over the same call.

## 4. Code generation

### 4.1 Uniform calling convention

Every JavaScript function, including the script, top-level declarations,
expressions and arrows, has the same signature (`make_js_signature`,
`compiler.rs:31`):

```text
(i64 env, i64 args, f64 callee) -> f64
```

- `env`: the environment captured by the closure being called.
- `args`: pointer to an argument block.
- `callee`: the function value itself. This is not `this`; `this` is not
  implemented. It exists so named function expressions can initialise their
  self binding without a separate lookup.
- Return: the function result as a NaN-boxed `f64`.

The argument block layout is:

```text
+0         argument count (i64)
+8         argument 0 (f64)
+16        argument 1 (f64)
...
+8+8*31    argument 31 (f64)
```

The block is `8 + 8 * MAX_ARGS = 264` bytes, with `MAX_ARGS = 32`
(`compiler.rs:29`, `compiler.rs:2444`).

### 4.2 Function entry

`compile_functions` (`compiler.rs:92`) emits the same prologue for every
function:

1. Read the exception state global (`compiler.rs:129`).
2. Declare a Cranelift variable for every binding owned by this function whose
   scope is not captured (`declare_vars`, `compiler.rs:791`).
3. If the root scope is captured, allocate a fresh root environment whose parent
   is the incoming `env` parameter and initialise its slots. Otherwise initialise
   the register variables and keep `env` as the incoming parameter
   (`compiler.rs:132`).
4. Store the `callee` argument into the self binding, if this is a named function
   expression (`compiler.rs:159`).
5. Load parameters from the argument block. Missing arguments are `undefined`;
   default initializers run only when the resulting value is `undefined`
   (`compiler.rs:163`).
6. Create closures for hoisted function declarations and store them in their
   bindings (`compiler.rs:184`).
7. Compile the body and return its last value, or `undefined` (`compiler.rs:194`).

Step 6 is important: a function declaration does not execute anything at its
source position. Its closure is built once per activation of the enclosing
function, at entry, from the root environment.

Parameter loading demonstrates the bounds check and the uniform ABI:

```rust
// compiler.rs:163
if !plan.is_script {
    let undef = value::undefined(&mut builder);
    let len = builder
        .ins()
        .load(types::I64, MemFlags::new(), args_param, 0i32);
    for (i, param) in plan.params.iter().enumerate() {
        let idx = builder.ins().iconst(types::I64, i as i64);
        let present = builder.ins().icmp(IntCC::UnsignedLessThan, idx, len);
        let loaded = builder
            .ins()
            .load(types::F64, MemFlags::new(), args_param, (8 + 8 * i) as i32);
        let arg = builder.ins().select(present, loaded, undef);
        // ... default initializer, then store_var ...
    }
}
```

### 4.3 Creating a closure

Whenever a function expression or arrow is evaluated, the compiler emits:

1. `func_addr` for the generated native function.
2. A 16-byte `calloc`.
3. Stores of the code pointer and the current `ctx.env`.
4. The tagged `f64`.

The environment is captured by reference; the closure record contains a pointer
to the existing environment, never a copy. A closure may therefore outlive the
activation that created its environment; the environment record stays valid
because it is heap allocated.

Hoisted function declarations use the same `new_closure` helper, but are created
at function entry with the root environment.

### 4.4 Calling a closure

`compile_call_value` (`compiler.rs:2423`) implements dynamic dispatch:

```rust
// compiler.rs:2435
let callee_ptr = value::payload(builder, callee);
let code = builder
    .ins()
    .load(types::I64, MemFlags::new(), callee_ptr, 0i32);
let captured_env = builder
    .ins()
    .load(types::I64, MemFlags::new(), callee_ptr, 8i32);

let args_ptr = call_calloc(builder, ctx.calloc_ref, 8 + 8 * MAX_ARGS as i64);
let len = builder.ins().iconst(types::I64, args.len() as i64);
builder.ins().store(MemFlags::new(), len, args_ptr, 0i32);
// ... evaluate and store each argument ...

let call = builder
    .ins()
    .call_indirect(ctx.js_sig_ref, code, &[captured_env, args_ptr, callee]);
let result = builder.inst_results(call)[0];
emit_exception_check(builder, ctx, flow)?;
```

The relevant generated IR for a call looks like this (from a compiled `make(1)`
example):

```text
v19 = bitcast.i64 v18                 ; callee value
v20 = band_imm v19, 0xffff_ffff_ffff  ; strip TAG_FUNCTION
v21 = load.i64 v20                    ; closure.code
v22 = load.i64 v20+8                  ; closure.env
...
v25 = call fn0(v23, v24)              ; calloc(1, 264) for the args block
v26 = iconst.i64 1
store v26, v25                        ; args.len = 1
v27 = f64const 0x1.0000000000000p0
store v27, v25+8                      ; args[0] = 1
v28 = call_indirect sig, v21(v22, v25, v18)
v29 = load.i64 v3                     ; pending-exception flag
v30 = icmp_imm ne v29, 0
brif v30, block1, block2
```

Notes:

- There is no callable check. Calling a number or other non-function value
  interprets the masked bits as a pointer and can crash the process instead of
  raising a JavaScript `TypeError`.
- The callee is evaluated before the argument block is allocated.
- Every dynamically dispatched call allocates a fresh 264-byte argument block,
  even for zero-argument calls.
- Extra arguments are evaluated and stored but ignored by the callee; missing
  arguments become `undefined`.
- More than 32 arguments at one call site is rejected at compile time.
- After the call, the exception flag is checked. A `throw` in the callee returns
  `undefined` with the flag set and the caller unwinds through `finally` blocks.

Builtin calls such as `Math.sqrt(x)` are compiled inline or into libm calls and
do not use this closure-call path.

### 4.5 Environment-backed and register-backed access

`Place` (`compiler.rs:1011`) is the abstraction over both storage classes:

```rust
#[derive(Clone, Copy)]
enum Place {
    Env { ptr: Value, offset: i32 },
    Local { var: Variable },
}
```

`place` (`compiler.rs:1019`) decides based on the home scope:

```rust
// compiler.rs:1019
fn place(builder: &mut FunctionBuilder, ctx: &CodegenCtx<'_>, name: &str) -> Result<Place, Error> {
    let (home, slot) = resolve_binding(ctx.program, ctx.scope, name)
        .ok_or_else(|| Error::Scope(format!("unresolved identifier: {name}")))?;
    if ctx.program.scope(home).has_env() {
        let (depth, _) = resolve(ctx.program, ctx.scope, name)
            .ok_or_else(|| Error::Scope(format!("unresolved identifier: {name}")))?;
        let mut ptr = ctx.env;
        for _ in 0..depth {
            ptr = builder.ins().load(types::I64, MemFlags::new(), ptr, 0i32);
        }
        Ok(Place::Env {
            ptr,
            offset: (8 + 8 * slot) as i32,
        })
    } else {
        let var = *ctx
            .vars
            .get(&(home, slot))
            .ok_or_else(|| Error::Codegen("missing local variable".to_string()))?;
        Ok(Place::Local { var })
    }
}
```

Register-backed locals can only be accessed by the function that owns them,
because any nested function mentioning the binding would have marked the home
scope captured during planning.

### 4.6 Block scopes and per-iteration environments

`enter_scope` (`compiler.rs:864`) runs every time execution enters a scope:

```rust
// compiler.rs:864
fn enter_scope<'a>(
    builder: &mut FunctionBuilder,
    ctx: &CodegenCtx<'a>,
    scope: ScopeId,
) -> CodegenCtx<'a> {
    let plan = ctx.program.scope(scope);
    if plan.has_env() {
        let env = alloc_env(builder, ctx.calloc_ref, plan.slot_count(), ctx.env);
        init_slots(builder, env, plan);
        ctx.enter(scope, env)
    } else {
        init_local_slots(builder, ctx.vars, scope, plan);
        ctx.enter(scope, ctx.env)
    }
}
```

When a captured block body is inside a `while`, `do...while` or `for` body, the
allocation code is executed once per iteration. Each iteration therefore gets a
distinct environment record, which is what makes this work:

```js
var f = undefined;
var g = undefined;
var i = 0;
while (i < 2) {
  let n = i;
  if (i === 0) f = function () { return n; };
  else         g = function () { return n; };
  i = i + 1;
}
f() + g(); // 1
```

A `for` loop with a lexical initializer has an additional rule. The loop scope
is created once before the initializer, but at the end of each iteration its
slots are copied into a fresh environment before the update expression runs:

```rust
// compiler.rs:1657
builder.switch_to_block(step);
let step_ctx = if let Some((slots, _)) = fresh {
    let previous = builder.use_var(env_var);
    let env = copy_env(builder, ctx.calloc_ref, parent_env, previous, slots);
    builder.def_var(env_var, env);
    ctx.enter(ctx.scope, env)
} else {
    ctx
};
// ... compile update ...
builder.ins().jump(header, &[]);
```

`copy_env` (`compiler.rs:882`) allocates a new environment with the **original
parent** (saved before entering the loop scope), copies every slot, and stores
the new pointer in `env_var`. The header and body read `env_var`, so a closure
created in the body keeps the environment for its own iteration, while the update
expression mutates the copy used by the next iteration. `continue` jumps to the
step block, so it still performs the copy and the update.

If the loop scope is not captured, no copy is made and the bindings are updated
in place.

### 4.7 Named function expressions

For `var f = function g() { ... }`:

- The planner adds `g` to the function's root scope and records it as
  `self_binding`.
- At entry, the third ABI argument (the callee value) is stored into `g` before
  parameters are processed (`compiler.rs:159`).
- Every call to the closure passes the same closure value as the third argument,
  so `g` is always the function value itself. `g === f` is true when `f` holds
  the closure, and recursive calls work.
- A default parameter can refer to `g`, because the self binding is initialised
  before parameter defaults are evaluated.

Arrow functions use the same lowering without a self binding. They do not have
their own `this`, `arguments` or `new.target`; those features are not implemented.

## 5. End-to-end example

Source:

```js
function make(x) {
  return function (y) { return x + y; };
}
var f = make(1);
```

The top-level script has two captured bindings, `make` and `f`, so its root
environment has two slots and a parent. The hoisted closure for `make` is built
at script entry. The script IR (trimmed to the relevant instructions):

```text
; __script
block0(v0: i64, v1: i64, v2: f64):
    v4 = iconst.i64 1
    v5 = iconst.i64 24
    v6 = call fn0(v4, v5)              ; calloc(1, 24): parent + 2 slots
    store v0, v6                        ; env.parent = caller env (0 from main)
    v7 = iconst.i64 0x7ff9_0000_0000_0000
    v8 = bitcast.f64 v7
    store v8, v6+8                      ; slot make = undefined
    store v8, v6+16                     ; slot f = undefined
    v11 = func_addr.i64 fn_make
    v14 = call fn0(v12, v13)            ; calloc(1, 16)
    store v11, v14                      ; closure.code = &make
    store v6, v14+8                     ; closure.env = script env
    v16 = bor 0x7ffd_0000_0000_0000, v14
    v17 = bitcast.f64 v16
    store v17, v6+8                     ; bind make
    ...
    v18 = load.f64 v6+8                 ; callee: make
    v20 = band_imm bits(v18), 0xffff_ffff_ffff
    v21 = load.i64 v20                   ; code
    v22 = load.i64 v20+8                 ; captured env
    v25 = call fn0(...)                  ; args block (264 bytes)
    store 1, v25
    store 1.0, v25+8
    v28 = call_indirect sig, v21(v22, v25, v18)
    store v28, v6+16                     ; bind f
```

The generated `make` function receives the script environment as its `env`
parameter, allocates its own root environment for `x`, parented to the script
environment, stores the argument into the `x` slot after the TDZ check, and
creates the inner closure over its root environment:

```text
; js_fn_1 (make)
block0(v0: i64, v1: i64, v2: f64):
    v6 = call fn0(v4, v5)                ; calloc(1, 16): parent + x
    store v0, v6                          ; parent = script env captured by make's closure
    v8 = bitcast.f64 0x7ffc_0000_0000_0000
    store v8, v6+8                        ; x starts in the TDZ
    v10 = undefined
    v11 = load.i64 v1                     ; argument count
    v13 = icmp ult 0, v11
    v14 = load.f64 v1+8                   ; argument 0
    v15 = select v13, v14, v10
    store v15, v6+8                       ; x = argument
    v16 = func_addr.i64 fn_inner
    v19 = call fn0(v17, v18)              ; calloc(1, 16)
    store v16, v19                        ; closure.code = &inner
    store v6, v19+8                       ; inner captures make's env
    v22 = bitcast.f64 (0x7ffd_0000_0000_0000 | v19)
    return v22
```

The inner function has an uncaptured root (it contains no further functions), so
it keeps `y` in a register and uses the incoming environment directly:

```text
; js_fn_2 (inner)
block0(v0: i64, v1: i64, v2: f64):
    ; no root environment allocation: v0 is make's environment
    ...
    v13 = load.f64 v0+8                   ; x (depth 0 from this environment)
    ... trap if x is TAG_UNINITIALIZED ...
    ... load y from the argument block ...
    ... ToNumber(x), ToNumber(y), fadd ...
    return result
```

`x` is a parameter, and parameter bindings are deliberately excluded from the
numeric analysis (see [VALUE.md](VALUE.md)), so the addition still coerces both
operands even though `x` happens to hold a number here.

## 6. Semantics and corner cases

### Shared cells

Captured bindings are cells, not copies. Multiple closures over the same
environment observe each other's writes:

```js
function makeCounter() {
  var n = 0;
  return function () { n = n + 1; return n; };
}
var c = makeCounter();
c(); c(); c(); // 3
```

### Loop captures

- A captured `let`/`const` declared directly in a loop body (`while`,
  `do...while`, `for`) gets a new environment every time the body is entered.
- A captured lexical binding in a `for` initializer gets the per-iteration copy
  behaviour described in section 4.6.
- `var` bindings are function scoped and are never copied per iteration.

### Hoisting

- `function` declarations are visible before their source position because their
  closures are installed at function entry.
- All function declarations in a function are hoisted to its root, even when
  written inside a block, so they cannot observe that block's `let`/`const`
  bindings.
- `var` bindings exist from function entry with value `undefined`.
- A `let` captured before initialization has a valid environment and slot, but
  the slot contains `TAG_UNINITIALIZED`; reading it traps.

### Identity

Function values compare by the identity of their closure record. Evaluating a
function expression twice produces two distinct closure records, even for the
same native code:

```js
Object.is(function () {}, function () {}); // false
var f = function () {};
Object.is(f, f);                           // true
```

A hoisted function declaration produces one closure per activation of the
enclosing function.

### Recursion and defaults

```js
var fact = function f(n) {
  return n <= 1 ? 1 : n * f(n - 1);
};
fact(5); // 120
```

The self binding is what makes `f` available inside the expression. It is
initialised before default parameter expressions, so defaults may reference it.

### Missing features

The current closure model has no `this`, `arguments`, `new`, methods, classes,
generators, `async` functions, `eval`, `Function` constructor, `bind`, `call` or
`apply`. A function is only callable through the uniform call path.

## 7. Memory and lifetime

All runtime records are allocated with libc `calloc` and are never freed. There
is no garbage collector, reference counting, destructor or arena.

| Allocation | Size | Allocated when |
| --- | --- | --- |
| Closure record | 16 bytes | Each evaluation of a function/arrow expression; each hoisted declaration at enclosing entry. |
| Environment record | `8 + 8 * slots` bytes | Entry to a captured scope: once per call for a captured function root, once per execution for a captured block, once per iteration for captured loop bodies and `for` copies. |
| Argument block | 264 bytes | Every dynamically dispatched JavaScript call. |

For a long-running program, memory grows with:

- the number of captured function activations,
- the number of captured block entries and loop iterations,
- the number of closure values created,
- the total number of dynamic calls.

For the current test suite and CLI use (short-lived AOT programs) this is
acceptable. It is not suitable for a server process as-is. Any future object or
string heap will need a real allocation and collection strategy; the environment
and closure records should then become part of it.

## 8. Limitations and future directions

- Capture analysis is conservative. It never analyses free variables, so a
  function containing any nested function heap-allocates all of its locals. A
  free-variable pass could keep more bindings in registers.
- Environments are always heap allocated. There is no escape analysis or stack
  allocation for closures that do not escape.
- The third ABI argument is the callee, not `this`. Adding methods will require
  a receiver convention (an extra argument or a receiver field).
- Calls do not verify that the callee is a function. A callable check and a
  proper `TypeError` path are needed.
- The closure pointer relies on 48-bit user-space addresses, as assumed by the
  NaN-boxing scheme.
- There is no debug information mapping native code back to JavaScript source
  positions inside closures.

## 9. Tooling and tests

Inspect the generated IR with:

```sh
cd sealionjs
cargo run -- --emit-ir --no-run ../path/to/file.js
```

The closure-related end-to-end tests live in `sealion-engine/tests/e2e.rs`:

- `closure_adder`, `closure_mutation`, `closures_as_values_and_arguments`
- `nested_closure_captures_grandparent`
- `block_scope_per_iteration_capture`
- `arrow_functions_capture_closures`, `arrow_function_inside_block_scope`
- `for_loop_per_iteration_bindings`
- `default_parameters`, `default_parameters_have_a_tdz`
- `let_and_const_have_a_tdz`
- `named_function_expressions`

Run them with:

```sh
cargo test --manifest-path sealion-engine/Cargo.toml
```
