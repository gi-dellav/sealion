#!/usr/bin/env bash
set -u

DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
RUNS=${RUNS:-10}
BUILD="$DIR/build"

mkdir -p "$BUILD"

if [ -n "${CC:-}" ]; then
    CC_BIN=$CC
elif command -v cc >/dev/null 2>&1; then
    CC_BIN=cc
elif command -v gcc >/dev/null 2>&1; then
    CC_BIN=gcc
else
    echo "error: no C compiler found" >&2
    exit 1
fi

now_ns() {
    date +%s%N
}

echo "compiling C with $CC_BIN"
c_compile_start=$(now_ns)
"$CC_BIN" -O3 -o "$BUILD/loopmath_c" "$DIR/loopmath.c" -lm || exit 1
c_compile_end=$(now_ns)

echo "compiling with sealionjs (AOT)"
sealion_compile_start=$(now_ns)
sealionjs "$DIR/loopmath.js" -o "$BUILD/loopmath_sealion" || exit 1
sealion_compile_end=$(now_ns)

awk -v c_start="$c_compile_start" -v c_end="$c_compile_end" \
    -v s_start="$sealion_compile_start" -v s_end="$sealion_compile_end" 'BEGIN {
    printf "compile time  C: %.1f ms   Sealion AOT: %.1f ms\n", \
        (c_end - c_start) / 1000000, (s_end - s_start) / 1000000
}'

printf '0;\n' > "$BUILD/jit_probe.js"
jit_available=0
sealionjs --no-pic "$BUILD/jit_probe.js" >/dev/null 2>"$BUILD/jit_probe.log" || true
if grep -q "in-process JIT" "$BUILD/jit_probe.log"; then
    jit_available=1
fi
rm -f "$BUILD/jit_probe.js" "$BUILD/jit_probe.log"

if [ "$jit_available" -eq 0 ]; then
    echo "warning: sealionjs was built without the jit feature; skipping JIT benchmark" >&2
    echo "         install with: cargo install --path sealionjs --features jit" >&2
fi

have_node=1
have_python=1
command -v node >/dev/null 2>&1 || have_node=0
command -v python3 >/dev/null 2>&1 || have_python=0

run_node_js() {
    node -e 'const fs = require("fs"); eval(fs.readFileSync(process.argv[1], "utf8") + "\nconsole.log(result);");' "$DIR/loopmath.js"
}

time_cmd() {
    local name=$1
    shift
    local total=0
    local start end i
    for ((i = 0; i < RUNS; i++)); do
        start=$(now_ns)
        "$@" >/dev/null 2>&1
        end=$(now_ns)
        total=$((total + end - start))
    done
    awk -v name="$name" -v total="$total" -v runs="$RUNS" 'BEGIN {
        printf "%-12s %10.1f ms/run  (%d runs)\n", name, total / runs / 1000000, runs
    }'
}

echo
echo "checksums"
c_sum=$("$BUILD/loopmath_c")
printf "%-12s %s\n" "C" "$c_sum"

aot_sum=0
"$BUILD/loopmath_sealion" >/dev/null 2>&1 || aot_sum=$?
printf "%-12s %s\n" "Sealion AOT" "$aot_sum"

jit_sum="n/a"
if [ "$jit_available" -eq 1 ]; then
    jit_sum=0
    sealionjs "$DIR/loopmath.js" >/dev/null 2>&1 || jit_sum=$?
    printf "%-12s %s\n" "Sealion JIT" "$jit_sum"
else
    printf "%-12s %s\n" "Sealion JIT" "not enabled, skipped"
fi

if [ "$have_node" -eq 1 ]; then
    js_sum=$(run_node_js)
    printf "%-12s %s\n" "NodeJS" "$js_sum"
else
    js_sum="n/a"
    printf "%-12s %s\n" "NodeJS" "not found, skipped"
fi

if [ "$have_python" -eq 1 ]; then
    py_sum=$(python3 "$DIR/loopmath.py")
    printf "%-12s %s\n" "Python" "$py_sum"
else
    py_sum="n/a"
    printf "%-12s %s\n" "Python" "not found, skipped"
fi

checks_ok=1
[ "$c_sum" = "$aot_sum" ] || checks_ok=0
if [ "$jit_available" -eq 1 ] && [ "$c_sum" != "$jit_sum" ]; then
    checks_ok=0
fi
if [ "$have_node" -eq 1 ] && [ "$c_sum" != "$js_sum" ]; then
    checks_ok=0
fi
if [ "$have_python" -eq 1 ] && [ "$c_sum" != "$py_sum" ]; then
    checks_ok=0
fi
if [ "$checks_ok" -eq 1 ]; then
    echo "checksums match"
else
    echo "checksum mismatch" >&2
fi

echo
echo "timings (JIT includes parse, codegen and load; AOT runs the prebuilt executable)"
time_cmd "C" "$BUILD/loopmath_c"
time_cmd "Sealion AOT" "$BUILD/loopmath_sealion"
if [ "$jit_available" -eq 1 ]; then
    time_cmd "Sealion JIT" sealionjs "$DIR/loopmath.js"
fi
if [ "$have_node" -eq 1 ]; then
    time_cmd "NodeJS" run_node_js
fi
if [ "$have_python" -eq 1 ]; then
    time_cmd "Python" python3 "$DIR/loopmath.py"
fi
