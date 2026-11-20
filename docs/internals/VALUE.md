# Value representation and management

This document describes how `sealion-engine` 0.6.0 represents and manipulates
JavaScript values. Every value is a single IEEE-754 `f64` using NaN boxing:
ordinary numbers are stored directly, and the reserved quiet-NaN payloads carry
the non-numeric values.

The representation is defined in `sealion-engine/src/value.rs`. Values flow
through Cranelift IR as `f64`, through the uniform calling convention as `f64`,
and through environment records and argument blocks as `f64`. There is no
separate boxed-value struct.

## 1. Bit layout

An `f64` is split into a 16-bit tag and a 48-bit payload:

```text
63                    48 47                                             0
+-----------------------+-------------------------------------------------+
|      tag (16 bits)    |                payload (48 bits)                |
+-----------------------+-------------------------------------------------+
```

Numbers use the normal IEEE-754 encoding, including both signs of zero, both
infinities and NaNs. The non-numeric values use tags `0x7FF9` through `0x7FFD`:

| Tag | Value | Payload |
| --- | --- | --- |
| `0x7FF9` | `undefined` | ignored |
| `0x7FFA` | `null` | ignored |
| `0x7FFB` | boolean | `0` or `1` in bit 0 |
| `0x7FFC` | uninitialized (TDZ) | ignored |
| `0x7FFD` | function/closure | 48-bit pointer to the closure record |

```rust
// value.rs:24
pub const TAG_UNDEFINED: u64 = 0x7FF9;
pub const TAG_NULL: u64 = 0x7FFA;
pub const TAG_BOOLEAN: u64 = 0x7FFB;
pub const TAG_UNINITIALIZED: u64 = 0x7FFC;
pub const TAG_FUNCTION: u64 = 0x7FFD;

const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;
```

All five tags are quiet NaNs: their mantissa has the quiet bit set. This means
the tagged values remain valid `f64`s and can be stored, copied and passed
around as raw bits without special handling. The two NaNs the FPU actually
produces, `0x7FF8_...` and `0xFFF8_...`, are left outside the tag range and are
treated as numbers, so an arithmetic result that is NaN stays a number.

The concrete bit patterns are:

```text
undefined       0x7FF9_0000_0000_0000
null            0x7FFA_0000_0000_0000
false           0x7FFB_0000_0000_0000
true            0x7FFB_0000_0000_0001
uninitialized   0x7FFC_0000_0000_0000
function        0x7FFD_0000_0000_0000 | closure_ptr
```

### Tag classification

A value is tagged when its top 16 bits lie in the unsigned interval
`[0x7FF9, 0x7FFD]`:

```rust
// value.rs:82
pub fn is_tagged(builder: &mut FunctionBuilder, value: Value) -> Value {
    let tag = tag_of(builder, value);
    let low = builder.ins().iconst(types::I64, TAG_UNDEFINED as i64);
    let high = builder.ins().iconst(types::I64, TAG_FUNCTION as i64);
    let ge = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, tag, low);
    let le = builder
        .ins()
        .icmp(IntCC::UnsignedLessThanOrEqual, tag, high);
    builder.ins().band(ge, le)
}
```

`tag_of` bitcasts the `f64` to `i64`, shifts right by 48 and compares. A
negative NaN such as `0xFFF8_...` has a tag above `0x7FFD` and is therefore
correctly classified as a number.

Tags `0x7FFE` and `0x7FFF` are unused. Adding a new heap value type (an object
or a string, for example) would need a new tag and an update to this range
check.

### The 48-bit payload assumption

The closure payload is a raw native pointer OR-ed with the tag. This relies on
user-space pointers fitting in the low 48 bits, which is the case on the
supported 64-bit Linux x86-64 and ARM64 targets. The payload mask is also used
verbatim when unwrapping a closure:

```rust
// value.rs:68
pub fn payload(builder: &mut FunctionBuilder, value: Value) -> Value {
    let bits = builder.ins().bitcast(types::I64, MemFlags::new(), value);
    builder.ins().band_imm(bits, PAYLOAD_MASK as i64)
}
```

There is no alignment requirement beyond what `calloc` provides. The top 16
bits of the pointer must be zero before the tag is OR-ed in.

## 2. Constructing values

Non-numeric constants are built by forming the bit pattern as an `i64` and
bitcasting it to `f64`:

```rust
// value.rs:35
pub fn undefined(builder: &mut FunctionBuilder) -> Value {
    let bits = builder
        .ins()
        .iconst(types::I64, (TAG_UNDEFINED << 48) as i64);
    builder.ins().bitcast(types::F64, MemFlags::new(), bits)
}

pub fn null(builder: &mut FunctionBuilder) -> Value {
    let bits = builder.ins().iconst(types::I64, (TAG_NULL << 48) as i64);
    builder.ins().bitcast(types::F64, MemFlags::new(), bits)
}
```

Booleans are similar, but they preserve the low bit:

```rust
// value.rs:136
pub fn from_bool(builder: &mut FunctionBuilder, value: Value) -> Value {
    let wide = builder.ins().uextend(types::I64, value);
    let payload = builder.ins().band_imm(wide, 1);
    let tag = builder.ins().iconst(types::I64, (TAG_BOOLEAN << 48) as i64);
    let bits = builder.ins().bor(tag, payload);
    builder.ins().bitcast(types::F64, MemFlags::new(), bits)
}
```

The `band_imm(..., 1)` guarantees a canonical boolean even if the input
condition is not already 0/1. `value::boolean` (`value.rs:47`) uses this helper
for boolean literals.

The uninitialized value is produced by `value::uninitialized` (`value.rs:52`) and
is only written into slots that are in the temporal dead zone:

```rust
// value.rs:52
pub fn uninitialized(builder: &mut FunctionBuilder) -> Value {
    let bits = builder
        .ins()
        .iconst(types::I64, (TAG_UNINITIALIZED << 48) as i64);
    builder.ins().bitcast(types::F64, MemFlags::new(), bits)
}
```

Function values use `value::function` (`value.rs:59`), which ORs the tag with the
closure pointer. See [CLOSURE.md](CLOSURE.md) for the record layout.

## 3. Classification and coercion

The helpers in `value.rs` are the semantic core of the engine. Code generation
composes them rather than inspecting tags ad hoc.

### 3.1 Tag and number predicates

| Helper | Meaning |
| --- | --- |
| `is_tagged` / `not_tagged` | Top 16 bits in the tag range. |
| `is_undefined` | Exact `TAG_UNDEFINED`. |
| `is_uninitialized` | Exact `TAG_UNINITIALIZED`, used for TDZ traps. |
| `is_boolean` (private) | Exact `TAG_BOOLEAN`. |
| `is_nan` | Untagged value with an unordered self-comparison. |
| `is_nullish` | `undefined` or `null`. |

`is_nan` has to exclude tagged values explicitly because tagged values are
themselves NaNs:

```rust
// value.rs:120
pub fn is_nan(builder: &mut FunctionBuilder, value: Value) -> Value {
    let unordered = builder.ins().fcmp(FloatCC::Unordered, value, value);
    let number = not_tagged(builder, value);
    builder.ins().band(unordered, number)
}
```

`is_finite` similarly requires `not_tagged`, then tests `abs(x) < Infinity`
(`value.rs:127`). `Number.isNaN` and `Number.isFinite` use these non-coercing
predicates; the global `isNaN` and `isFinite` first apply `ToNumber`.

### 3.2 Truthiness

`truthy` (`value.rs:147`) returns an untagged 0/1 condition:

- Tagged values: a function is truthy; a boolean is truthy when its low bit is
  1; `undefined`, `null` and `uninitialized` are false.
- Untagged values: ordered `value != 0.0`. This makes `+0`, `-0` and NaN false
  and every other number true.

```rust
// value.rs:147
pub fn truthy(builder: &mut FunctionBuilder, value: Value) -> Value {
    let tagged = is_tagged(builder, value);
    let tag = tag_of(builder, value);
    let fn_tag = builder.ins().iconst(types::I64, TAG_FUNCTION as i64);
    let is_fn = builder.ins().icmp(IntCC::Equal, tag, fn_tag);
    let is_bool = is_boolean(builder, value);
    let bits = bits_of(builder, value);
    let low = builder.ins().band_imm(bits, 1);
    let bool_true = builder.ins().icmp_imm(IntCC::NotEqual, low, 0);
    let bool_truthy = builder.ins().band(is_bool, bool_true);
    let tagged_truthy = builder.ins().bor(is_fn, bool_truthy);
    let zero = builder.ins().f64const(0.0);
    let num = builder.ins().fcmp(FloatCC::OrderedNotEqual, value, zero);
    builder.ins().select(tagged, tagged_truthy, num)
}
```

`OrderedNotEqual` is important: `UnorderedNotEqual` would make NaN truthy.
Tagged `false` is distinguished from numeric 0 by the tag check, so
`if (false)` and `if (0)` both take the false branch but by different paths.

### 3.3 Strict equality

`===` is bit equality when either operand is tagged, and IEEE-754 numeric
equality otherwise:

```rust
// value.rs:165
pub fn strict_equal(builder: &mut FunctionBuilder, lhs: Value, rhs: Value) -> Value {
    let lhs_bits = bits_of(builder, lhs);
    let rhs_bits = bits_of(builder, rhs);
    let bits_eq = builder.ins().icmp(IntCC::Equal, lhs_bits, rhs_bits);
    let num_eq = builder.ins().fcmp(FloatCC::Equal, lhs, rhs);
    let lhs_tagged = is_tagged(builder, lhs);
    let rhs_tagged = is_tagged(builder, rhs);
    let either_tagged = builder.ins().bor(lhs_tagged, rhs_tagged);
    let result = builder.ins().select(either_tagged, bits_eq, num_eq);
    from_bool(builder, result)
}
```

Consequences:

| Expression | Result | Reason |
| --- | --- | --- |
| `true === true` | `true` | Same tagged bits. |
| `true === 1` | `false` | One side tagged. |
| `null === null` | `true` | Same tagged bits. |
| `null === undefined` | `false` | Different tags. |
| `0 === -0` | `true` | Numeric comparison. |
| `NaN === NaN` | `false` | Numeric comparison. |
| `f === f` | `true` | Same closure pointer. |
| `f === g` | `false` unless same closure record | Closure identity. |

### 3.4 Loose equality

`==` is implemented only for the current value domain: numbers, booleans,
`undefined`, `null` and functions. The implementation (`value.rs:200`) is:

1. `strict_equal`.
2. `null == undefined` is true.
3. If either operand is a boolean, convert both with `boolean_to_number`
   (`true -> 1`, `false -> 0`, every other value unchanged) and compare as
   numbers.
4. Otherwise return the strict result.

| Expression | Result |
| --- | --- |
| `null == undefined` | `true` |
| `null == 0` | `false` |
| `undefined == 0` | `false` |
| `true == 1` | `true` |
| `false == 0` | `true` |
| `true == true` | `true` |
| `null == false` | `false` |
| `NaN == NaN` | `false` |
| `0 == -0` | `true` |
| `f == f` | `true` |

There is no `ToPrimitive`, no string comparison and no object coercion because
strings and objects do not exist yet.

### 3.5 SameValue and `Object.is`

`same_value` (`value.rs:220`) compares raw bits, except that any two untagged
NaNs are considered the same:

```rust
// value.rs:220
pub fn same_value(builder: &mut FunctionBuilder, lhs: Value, rhs: Value) -> Value {
    let lhs_bits = bits_of(builder, lhs);
    let rhs_bits = bits_of(builder, rhs);
    let bits_eq = builder.ins().icmp(IntCC::Equal, lhs_bits, rhs_bits);
    let lhs_nan = is_nan(builder, lhs);
    let rhs_nan = is_nan(builder, rhs);
    let both_nan = builder.ins().band(lhs_nan, rhs_nan);
    let result = builder.ins().bor(bits_eq, both_nan);
    from_bool(builder, result)
}
```

This is the ECMAScript `SameValue` algorithm for the current domain:

- `Object.is(0, -0)` is false.
- `Object.is(NaN, NaN)` is true.
- Tagged values compare by bits, so `Object.is(true, true)` is true and
  `Object.is(true, 1)` is false.
- Functions compare by closure identity.

### 3.6 ToNumber

`to_number` (`value.rs:233`) is the primitive `ToNumber` implemented by the
engine:

| Input | Result |
| --- | --- |
| number | unchanged |
| `undefined` | NaN |
| `null` | `0` |
| boolean | `1` or `0` |
| function | NaN |
| uninitialized | NaN (should be unreachable; TDZ traps first) |

```rust
// value.rs:233
pub fn to_number(builder: &mut FunctionBuilder, value: Value) -> Value {
    let bits = bits_of(builder, value);
    let tagged = is_tagged(builder, value);
    let low = builder.ins().band_imm(bits, 1);
    let bool_number = builder.ins().fcvt_from_uint(types::F64, low);
    let is_bool = is_boolean(builder, value);
    let is_null = tag_is(builder, value, TAG_NULL);
    let zero = builder.ins().f64const(0.0);
    let nan = builder.ins().f64const(f64::NAN);
    let non_bool = builder.ins().select(is_null, zero, nan);
    let tagged_number = builder.ins().select(is_bool, bool_number, non_bool);
    builder.ins().select(tagged, tagged_number, value)
}
```

Arithmetic, relational comparison, `++`/`--`, unary `+`/`-`, bitwise conversion
and Math arguments all go through this helper unless the static numeric analysis
proves the operand is already an untagged number (section 5).

### 3.7 ToInt32 and unsigned conversion

`ToInt32` is `ToNumber`, then modulo `2**32`, truncated toward zero and
reinterpreted as a two's-complement 32-bit integer:

```rust
// value.rs:250
pub fn to_int32(builder: &mut FunctionBuilder, fmod_ref: FuncRef, value: Value) -> Value {
    let value = to_number(builder, value);
    let modulus = builder.ins().f64const(TWO_POW_32);
    let call = builder.ins().call(fmod_ref, &[value, modulus]);
    let remainder = builder.inst_results(call)[0];
    let wide = builder.ins().fcvt_to_sint_sat(types::I64, remainder);
    builder.ins().ireduce(types::I32, wide)
}
```

`TWO_POW_32` is `4_294_967_296.0` (`value.rs:31`). `fmod` (the libm call) yields
NaN for NaN and infinities. `fcvt_to_sint_sat` maps NaN to 0, exactly as the
spec requires. After `fmod`, the value is in `(-2**32, 2**32)`, so the saturating
conversion has no other observable effect.

Examples:

```text
ToInt32(1.9)          = 1
ToInt32(-1.9)         = -1
ToInt32(2**31)        = -2147483648
ToInt32(2**32 + 1)    = 1
ToInt32(NaN)          = 0
ToInt32(Infinity)     = 0
```

`from_int32` (`value.rs:259`) converts a signed 32-bit result back to `f64`.
`from_uint32` (`value.rs:263`) zero-extends first and is used by `>>>`.

## 4. Numbers, signed zero and NaN

All numeric values are IEEE-754 doubles. There are no integer or small-integer
representations:

- `+0` and `-0` are distinct bit patterns but compare equal with `===` and
  `==`, matching the spec. `Object.is` and the `Math` sign-sensitive helpers
  inspect the sign bit separately.
- NaN is represented as an untagged NaN. `NaN === NaN` is false, `Object.is`
  treats all untagged NaNs as equal, and `Number.isNaN` is true.
- `Infinity` and `-Infinity` are ordinary untagged infinities. Arithmetic that
  produces NaN produces the FPU's canonical `0x7FF8`/`0xFFF8` pattern, which is
  outside the tag range.
- `1 / -0` produces `-Infinity`; `Math.round(-0.4)` explicitly preserves `-0`,
  as does `Math.sign(-0)`, `Math.min(+0, -0)` and `Math.max(+0, -0)`.

The encoding assumes that arithmetic and libm never return a NaN whose payload
lands in `0x7FF9..0x7FFD`. The supported FPUs canonicalise generated NaNs to
`0x7FF8`/`0xFFF8`; this is an invariant of the backends rather than something
checked at runtime.

## 5. Static numeric analysis

Unboxing `ToNumber` at every arithmetic operation would be costly and
unnecessary for most code. `analyze_numeric_bindings` (`compiler.rs:650`) computes
the greatest fixed point of lexical bindings that are guaranteed to hold
untagged numbers. The result is a `HashSet<(ScopeId, usize)>` of binding slots.

### Initialisation

The analysis starts with every slot in every `tdz` set. Only `let` and `const`
slots are ever inserted: `var` bindings can be read as `undefined` before their
initializer, and parameters, function declaration names, self bindings and catch
parameters are not in a TDZ set. Parameters are then explicitly removed from the
set (`compiler.rs:662`), including parameters with defaults, because a missing
argument or a non-numeric default is possible.

### Refinement

A `NumericWalker` (`compiler.rs:424`) walks every function plan and removes a
binding when it can receive a value that is not provably numeric:

- A lexical binding with no initializer or a non-numeric initializer is removed.
- An assignment to a numeric binding removes it unless the write is numeric.
  Arithmetic/bitwise compound assignments always produce numbers; plain and
  logical assignments depend on the right-hand side.
- The walker does not descend into nested function bodies; each plan is visited
  separately in the outer loop.

`known_number_expr` (`compiler.rs:338`) defines the provable cases:

| Expression | Why it is numeric |
| --- | --- |
| numeric literal | Untagged `f64` constant. |
| known numeric identifier | In the analysis set. |
| `NaN`, `Infinity` | Free global numeric constant. |
| arithmetic/bitwise binary | Arithmetic and bitwise results are always numbers. |
| unary `+`, `-`, `~` | Produce numbers. |
| conditional | Numeric when both branches are numeric. |
| assignment | Plain and arithmetic/bitwise compound assignments are numeric when the result is numeric; logical assignments are treated conservatively in expression position. |
| update `++`/`--` | Produces a number. |
| comma expression | Type of its last expression. |
| `Math.*(...)` | Builtin Math function results are numbers. |
| `Number(...)` | Conversion to a number. |
| `Math`/`Number` constant | Folded to an `f64` constant. |

The loop repeats until no more bindings are removed, because removing one
non-numeric binding can invalidate another binding initialised from it.

### Using the result

`number_value` (`compiler.rs:395`) is the wrapper used at arithmetic,
comparison and coercion sites:

```rust
fn number_value(builder: &mut FunctionBuilder, value: Value, known: bool) -> Value {
    if known {
        value
    } else {
        value::to_number(builder, value)
    }
}
```

The optimizer therefore keeps the two cases separate. Compare the generated
code for a `let` binding, which is inserted into the analysis set, with a `var`
binding, which is not:

```js
let x = 1;
x + 2;
```

```text
v6 = f64const 0x1.0000000000000p0
...
v10 = icmp eq tag(v6), 0x7ffc   ; TDZ check
trapnz v10, user1
v12 = fadd v6, v11              ; no ToNumber
```

```js
var x = 1;
x + 2;
```

```text
v6 = f64const 0x1.0000000000000p0
...
v30 = select tagged, ...        ; inline ToNumber(x)
v31 = fadd v30, v7
```

The TDZ check remains for the `let` binding even though the numeric analysis
knows the value is a number: the analysis tracks types, not initialization
state.

The analysis is soundness-critical. If a binding were marked numeric but could
receive a tagged value, skipping `ToNumber` would feed a NaN-boxed pointer into
floating-point arithmetic. The end-to-end suite includes assignments through
`try`/`catch`/`finally` and optional calls to exercise the removal rules
(`numeric_analysis_with_new_constructs` in `sealion-engine/tests/e2e.rs:1523`).

## 6. Where values live

| Storage | Representation | Managed by |
| --- | --- | --- |
| Cranelift IR value | `types::F64`; tag is just bits | Cranelift register allocator. |
| Register-backed binding | Cranelift `Variable` of type `F64` | Backend, register/stack allocation. |
| Environment slot | `f64` at `env + 8 + 8*slot` | Heap record allocated with `calloc`. |
| Argument block entry | `f64` at `args + 8 + 8*i` | Heap record allocated per call. |
| Closure record | `i64` code, `i64` environment pointer | Heap record allocated with `calloc`. |
| Return value | `f64` | Calling convention. |

The code and environment pointers inside a closure record are raw machine words,
not JavaScript values. The closure value itself is the tagged `f64`. Environment
records are described in detail in [CLOSURE.md](CLOSURE.md).

## 7. Calling convention

Every JavaScript function has one signature (`compiler.rs:31`):

```text
(i64 env, i64 args, f64 callee) -> f64
```

- `env` is the captured environment pointer, not a JS value.
- `args` points to a block whose first word is the argument count and whose
  following words are `f64` arguments.
- `callee` is the function value itself, used for named function expression
  self bindings. It is not `this`.
- The return value is a NaN-boxed `f64`.

`compile_call_value` (`compiler.rs:2423`) allocates the argument block with
`calloc(1, 8 + 8 * 32)`, stores the argument count at offset 0 and each argument
starting at offset 8, then performs an indirect call through the closure record.
At most 32 arguments are supported per call site (`MAX_ARGS`, `compiler.rs:29`).
Arguments beyond a function's parameter count are evaluated and stored but
ignored; parameters beyond the supplied count become `undefined`.

This uniform ABI is what makes values and closures first-class: a function can
be passed as an `f64`, returned, stored in an environment, compared with `===`
and called through a closure record without any tag-specific call sequence.

The `main` wrapper (`compile_main`, `compiler.rs:227`) calls `__script` with
`env = 0`, `args = 0` and `callee = undefined`. It then:

- returns exit code 1 if the exception flag is set;
- otherwise applies `ToNumber` to the script result and uses
  `fcvt_to_sint_sat.i32` as the process exit code.

Since there is no output API in the engine, this exit code is the primary way
the outside world observes a compiled program.

## 8. Exceptions as a side channel

Exceptions are a value, but they do not travel through the normal return
channel. `lib.rs:243` defines a 16-byte zero-initialised global data object,
`sealion_exception`:

```text
+0    pending flag (i64)
+8    exception value (f64, NaN-boxed)
```

- `throw` evaluates its argument, stores the value at `+8` and sets the flag
  (`set_exception`, `compiler.rs:1113`).
- A `catch` block calls `take_exception` (`compiler.rs:1102`), which loads the
  value and clears the flag.
- `has_exception` (`compiler.rs:1095`) loads the flag. Every dynamically
  dispatched call checks it after returning, then unwinds through the
  appropriate `finally` blocks (`emit_exception_check`, `compiler.rs:1205`).
- If an exception reaches `main`, the process exits with status 1.

Because the state is a single global, exception propagation is not thread-safe.
The engine is single-threaded. Any value in the current domain can be thrown:
`undefined`, `null`, booleans, numbers and closures are all valid exception
values.

## 9. Memory management and aliasing

Primitives are copied by value as `f64`s. There is no allocation for numbers,
booleans, `null` or `undefined`. The only engine-managed allocations are:

- closure records (16 bytes),
- environment records (`8 + 8 * slots` bytes),
- argument blocks (264 bytes).

All three are allocated with `calloc` and are never freed. There is no garbage
collector, no reference counting and no ownership tracking. This has two
consequences:

- A closure can safely outlive the activation that created it; the environment
  record remains valid for the lifetime of the process.
- Memory grows monotonically with closure creation, captured-scope entry and
  dynamic calls. A long-running program that creates closures in a loop will
  leak. The engine is currently aimed at short-lived AOT programs and tests.

Environment slots are mutable cells. Copying a closure value copies the `f64`
pointer, so all copies share the same closure record and the same environment.
There is no structural copy of environments, even at closure creation. The one
deliberate copy is the `for` loop's per-iteration environment
(`copy_env`, `compiler.rs:882`), which is required by the specification.

## 10. Current limits and future work

The value domain is deliberately narrow:

- No strings, objects, arrays, symbols, BigInts or regular expressions. String
  and BigInt literals are rejected at compile time, as are template literals.
- No `typeof`, `instanceof`, `in`, property objects or prototypes. Property
  access is limited to the compile-time `Math`, `Number`, `Object` and `Date`
  namespaces.
- No function objects as property containers: `f.name`, `f.length` and similar
  are not implemented.
- No callable check. Calling a non-function value strips the tag and treats the
  low 48 bits as a pointer. That is undefined behaviour in the compiled program
  and normally crashes.
- No canonicalisation of NaN payloads. The engine treats every untagged NaN as
  the same value for `Object.is` and as false for comparison, but the raw payload
  is preserved.
- The 48-bit pointer, fixed tag range and FPU NaN canonicalisation assumptions
  are not checked at runtime.
- `ToNumber`, `ToString`, `ToPrimitive` and the equality algorithms only cover
  the current primitive domain. Adding strings and objects will require extending
  them, adding tags or an object header, and introducing a real heap and garbage
  collector.

## 11. Source map and verification

| Topic | Source |
| --- | --- |
| Tags and bit helpers | `sealion-engine/src/value.rs` |
| Truthiness, equality, coercions | `sealion-engine/src/value.rs:120` onward |
| Numeric analysis | `sealion-engine/src/compiler.rs:285`, `compiler.rs:650` |
| Calling convention | `sealion-engine/src/compiler.rs:31`, `compiler.rs:2423` |
| Argument block and closure calls | `sealion-engine/src/compiler.rs:2423` |
| Exception state | `sealion-engine/src/lib.rs:243`, `compiler.rs:1095` |
| Closure and environment layout | [CLOSURE.md](CLOSURE.md) |

Inspect the lowered representation with:

```sh
cd sealionjs
cargo run -- --emit-ir --no-run ../path/to/file.js
```

Relevant end-to-end tests in `sealion-engine/tests/e2e.rs`:

- `undefined_literal_and_identity`, `null_literal_and_identity`
- `loose_equality`, `switch_evaluation_order_and_strict_equality`
- `booleans_are_distinct_and_coerce`
- `global_nan_and_infinity`, `number_constants`, `object_is`
- `arithmetic_and_relational_coercion`, `math_arguments_are_coerced`
- `global_is_nan_and_is_finite_coerce`
- `math_signed_zero`
- `numeric_analysis_with_new_constructs`

Run the suite with:

```sh
cargo test --manifest-path sealion-engine/Cargo.toml
```
