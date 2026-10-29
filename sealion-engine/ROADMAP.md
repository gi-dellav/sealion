## 0.1.0

- [x] Functions
- [x] Variables
- [x] Arithmetic operators
- [x] if-else statements
- [x] While loops
- [x] `return` statements
- [x] Closures and Scopes

## 0.2.0

- [x] `undefined` type
- [x] `null` type
- [ ] `typeof` (moved to Later)
- [x] `void`
- [x] `break` statement
- [x] `continue` statement

## 0.3.0

- [x] Ternary operator (`? :`)
- [x] Comma operator (`,`)
- [x] Arithmetic operators: `+, -, *, /, %, **, ++, --`
- [x] Bitwise operators: `&, |, ^, ~, <<, >>, >>>`
- [x] Logical operators: `&&, ||, ??, !`
- [x] Comparison operators: `===, !==, ==, !=, <, >, <=, >=`
- [x] Assignment operators: `=, +=, -=, *=, /=, %=, **=, <<=, >>=, >>>=, &=, ^=, |=, &&=, ||=, ??=`

## 0.4.0

- [x] Block statements (`{ ... }`) — real block scoping, `let`/`const` shadow and do not leak
- [x] Anonymous functions — function and arrow expressions, capturing closures

## 0.5.0

- [ ] `Number.isNaN(value)` (+ alias `isNaN`)
- [ ] `Number.isFinite(value)` (+ alias `isFinite`)
- [ ] `Number.isInteger(value)`
- [ ] `Number.isSafeInteger(value)`
- [ ] `Number.MAX_VALUE, Number.MIN_VALUE, Number.MAX_SAFE_INTEGER, Number.MIN_SAFE_INTEGER, Number.EPSILON, Number.NaN, Number.NEGATIVE_INFINITY, Number.POSITIVE_INFINITY`
- [ ] `Math.abs(), Math.ceil(), Math.floor(), Math.round(), Math.trunc(), Math.fround()`
- [ ] `Math.min(a, b), Math.max(a, b)`
- [ ] `Math.pow(), Math.sqrt(), Math.cbrt(), Math.exp(), Math.expm1(), Math.log(), Math.log10(), Math.log1p(), Math.log2()`
- [ ] `Math.sin(), Math.cos(), Math.tan(), Math.asin(), Math.acos(), Math.atan(), Math.atan2(y, x), Math.sinh(), Math.cosh(), Math.tanh(), Math.asinh(), Math.acosh(), Math.atanh()`
- [ ] `Math.clz32(), Math.imul(), Math.sign()`
- [ ] `Math.PI, Math.E, Math.LN2, Math.LN10, Math.LOG2E, Math.LOG10E, Math.SQRT1_2, Math.SQRT2`

## 0.6.0

- [ ] `StaticArray(n)`, sealion-exclusive object for static arrays (like C's arrays; `a = Slice(10)` == `void *a[10]`); inside the engine, it's a static container of `number` values
- [ ] `Array`, JS's `array` type, based on sealion's StaticArray; the compiler offers a `--array-length` to define the static length, by default 1024
- [ ] `string`, which are based on sealion's StaticArray; the compiler offers a `--string-length` to define the static length, by default 1024
- [ ] `typeof` operand

## 0.7.0

- [ ] `switch, case, default`
- [ ] `for` loops
- [ ] `const` keyword

### Later

- [ ] `typeof`
- [ ] `Object` type
- [ ] `array` type
- [ ] `string` type

- [ ] `this` implementation
- [ ] `__proto__` implementation

- [ ] `Math` module
- [ ] `Date` module

- [ ] Error handling
