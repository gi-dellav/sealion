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

- [x] `Number.isNaN(value)` (+ alias `isNaN`)
- [x] `Number.isFinite(value)` (+ alias `isFinite`)
- [x] `Number.isInteger(value)`
- [x] `Number.isSafeInteger(value)`
- [x] `Number.MAX_VALUE, Number.MIN_VALUE, Number.MAX_SAFE_INTEGER, Number.MIN_SAFE_INTEGER, Number.EPSILON, Number.NaN, Number.NEGATIVE_INFINITY, Number.POSITIVE_INFINITY`
- [x] `Math.abs(), Math.ceil(), Math.floor(), Math.round(), Math.trunc(), Math.fround()`
- [x] `Math.min(a, b), Math.max(a, b)`
- [x] `Math.pow(), Math.sqrt(), Math.cbrt(), Math.exp(), Math.expm1(), Math.log(), Math.log10(), Math.log1p(), Math.log2()`
- [x] `Math.sin(), Math.cos(), Math.tan(), Math.asin(), Math.acos(), Math.atan(), Math.atan2(y, x), Math.sinh(), Math.cosh(), Math.tanh(), Math.asinh(), Math.acosh(), Math.atanh()`
- [x] `Math.clz32(), Math.imul(), Math.sign()`
- [x] `Math.PI, Math.E, Math.LN2, Math.LN10, Math.LOG2E, Math.LOG10E, Math.SQRT1_2, Math.SQRT2`


## 0.6.0

- [x] `switch, case, default`
- [x] `for` loops
- [x] `const` keyword

## 0.7.0

- [x] Optional in-process JIT on Linux x86-64 (`jit` Cargo feature)
- [x] Precompiled artifacts: `Engine::precompile_module`, `IrModule::serialize`, `Engine::deserialize*`, and the `--artifact` / `--run-artifact` CLI flags
- [x] `Object`: own-property bags in the dynamic (JIT) runtime only — object literals, static and computed reads/writes, compound assignment, `++`/`--`, `delete`, `in`, optional chaining and `Object.is`
- [x] Object prototypes in the dynamic (JIT) runtime: `Object.prototype` and `Array.prototype`, `Object.create`, `Object.getPrototypeOf`, `Object.setPrototypeOf`, `Object.hasOwn`, `__proto__`, `Object.keys`/`values`/`entries` and `Object.prototype.hasOwnProperty`/`isPrototypeOf`/`propertyIsEnumerable`/`toString`/`valueOf`; no property descriptors, accessors or spread
- [x] `Array.prototype` — `at`, `concat`, `copyWithin`, `entries`, `every`, `fill`, `filter`, `find`, `findIndex`, `findLast`, `findLastIndex`, `flat`, `flatMap`, `forEach`, `includes`, `indexOf`, `join`, `keys`, `lastIndexOf`, `map`, `pop`, `push`, `reduce`, `reduceRight`, `reverse`, `shift`, `slice`, `some`, `sort`, `splice`, `toLocaleString`, `toReversed`, `toSorted`, `toSpliced`, `toString`, `unshift`, `values`, `valueOf` and `with`, plus `Array.isArray`, `Array.of` and `Array.from` (dynamic runtime only; `keys`/`values`/`entries` return arrays because there is no iterator protocol)
- [x] `Map` — `new Map()` with `size`, `get`, `set`, `has`, `delete`, `clear`, `forEach`, `keys`, `values` and `entries` (dynamic runtime only; `keys`/`values`/`entries` return arrays, no iterable argument)
- [x] `typeof` operand
- [x] `this` implementation
- [x] `String.prototype` — `charAt`, `charCodeAt`, `codePointAt`, `at`, `indexOf`, `lastIndexOf`, `includes`, `startsWith`, `endsWith`, `slice`, `substring`, `substr`, `split`, `trim`, `trimStart`, `trimEnd`, `toUpperCase`, `toLowerCase`, `repeat`, `padStart`, `padEnd`, `replace`, `replaceAll`, `concat`, `toString` and `valueOf` (dynamic runtime only; `replace` takes strings or a function, no regular expressions)


## 0.8.0

- [x] `String.fromCharCode` and `String.fromCodePoint` (dynamic runtime only; an out-of-range or non-integral code point is skipped instead of raising `RangeError`)
- [x] `Math` module — `Math` as an object of the dynamic runtime, so `var m = Math`, `var floor = Math.floor`, `Math['abs'](-1)` and `Object.keys(Math)` work; member calls keep using the inline/libm paths
- [x] `Date` module — `new Date(...)` (no argument, a time value, a string or the year/month fields), `Date()`, `Date.now`, `Date.parse`, `Date.UTC`, `Date.prototype` and the `get*`/`getUTC*`/`setTime`/`to*String` methods (dynamic runtime only; local time comes from the system clock, and an invalid date degrades to `"Invalid Date"`/`NaN` instead of raising `RangeError`)
- [x] `class` declarations and expressions — constructor, methods, static methods and `extends`
- [x] `super` — `super(...)`, `super.m()` and `super.x = v`, in methods and in arrows nested in them
- [x] `new` on user functions and classes — the receiver comes from `callee.prototype`, is passed as `this`, and is replaced only when the constructor returns an object
- [x] `instanceof`
- [x] `this` in constructors and methods, plus `Function.prototype.call`, `apply` and `bind`
- [x] Function values are object-like: a four-word record whose third word is a lazily allocated property bag, so `F.count = 1`, `C.m = fn` and `f.prototype` work, and `class B extends A` inherits the statics
- [x] Hidden (non-enumerable) properties, so `constructor` and a function's `prototype` stay out of `for..in` and `Object.keys`
- [x] `for..in` — enumerable string keys, own first then inherited, each name once, snapshotted at loop entry
- [x] `for..of` — arrays, strings (by code point), maps and array-likes, with a fresh binding per iteration for a `let`/`const` head
- [x] Functions return `undefined` when they fall off the end; only the script and an arrow expression body yield a trailing expression value

### 0.9.0

- [ ] `import` and `export` modules
- [ ] CommonJS modules via `require`

### 0.10.0

- [ ] Error handling
- [ ] Array destructuring
- [ ] Passing the `Acorn` and `SICP` baselines

### 0.11.0

- [ ] `using` keyword
- [ ] Class fields, accessors, private members and `new.target`

#### 0.11.0 (NodeJS modules)

0.11.0 must ship basic implementations of:

- [ ] `assert`
- [ ] `abort`
- [ ] `console`
- [ ] `fs`
- [ ] `os`
- [ ] `process`
- [ ] `path`
- [ ] `util`
