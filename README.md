# sealion
Experimental JS engine built on Cranelift

<img src="https://news.ucsc.edu/wp-content/uploads/2025/04/ronan-beats-humans-scaled.jpg" alt="Sealion" width="600">

## Try it out

Requires latest Rust toolchain.

1. Clone the repo
2. Run `cd sealionjs && cargo install --path .`
3. Run `sealionjs --help` to see CLI flags
4. Run `cd perfbench/loopmath && ./bench.sh` to run a simple perf benchmark

## Milestones

### Phase 1 ~ Toy runtime

- Must support AOT compilation on Linux (glibc) for both x86 and ARM64
- Must support CommonJS-style require() semantics
- Must run all tests in baseline/sicp/ (JS edition of SICP)
- Must run all tests in baseline/acorn/ (Acorn parser)
- Must have basic implementations of `abort`, `assert`, `console`, `fs`, `os`, `path`, `process`, `util` NodeJS modules

### Phase 2 ~ Basic runtime

- Must have basic `async` support
- Must have basic implementations of `events`, `http`, `https`, `url`, `fs/promise`
- Must pass test242 (ECMAScript standardized test suite)

### Phase 3 ~ Usable runtime

- Must pass all tests of the Express framework
- Must pass all tests of the Socket.IO library
- Must pass all tests of the Lodash library
- Must pass all tests of the tslib library
- Must pass all tests of the js-yaml library
- Must pass all tests of the fs-extra library
- Must pass all tests of the readdirp library

### Phase 4 ~ Production runtime

- Must pass all tests of the NextJS framework
- Must pass all tests of the AI SDK framework
- Must pass all tests of the Nitro framework
- Must pass all tests of the Nest framework
