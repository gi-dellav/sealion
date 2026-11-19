#!/usr/bin/env bash
set -u

DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
RUNS=${RUNS:-50}
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

echo "compiling C with $CC_BIN"
"$CC_BIN" -O3 -o "$BUILD/loopmath_c" "$DIR/loopmath.c" -lm || exit 1

echo "compiling with sealionjs"
sealionjs "$DIR/loopmath.js" -o "$BUILD/loopmath_sealion" || exit 1

have_node=1
have_python=1
command -v node >/dev/null 2>&1 || have_node=0
command -v python3 >/dev/null 2>&1 || have_python=0

run_node_js() {
    node -e 'const fs = require("fs"); eval(fs.readFileSync(process.argv[1], "utf8") + "\nconsole.log(result);");' "$DIR/loopmath.js"
}

now_ns() {
    date +%s%N
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
        printf "%-10s %10.1f ms/run  (%d runs)\n", name, total / runs / 1000000, runs
    }'
}

echo
echo "checksums"
c_sum=$("$BUILD/loopmath_c")
printf "%-10s %s\n" "C" "$c_sum"

sealion_sum=0
"$BUILD/loopmath_sealion" >/dev/null 2>&1 || sealion_sum=$?
printf "%-10s %s\n" "Sealion" "$sealion_sum"

if [ "$have_node" -eq 1 ]; then
    js_sum=$(run_node_js)
    printf "%-10s %s\n" "NodeJS" "$js_sum"
else
    js_sum="n/a"
    printf "%-10s %s\n" "NodeJS" "not found, skipped"
fi

if [ "$have_python" -eq 1 ]; then
    py_sum=$(python3 "$DIR/loopmath.py")
    printf "%-10s %s\n" "Python" "$py_sum"
else
    py_sum="n/a"
    printf "%-10s %s\n" "Python" "not found, skipped"
fi

if [ "$c_sum" = "$sealion_sum" ] && { [ "$js_sum" = "n/a" ] || [ "$c_sum" = "$js_sum" ]; } && { [ "$py_sum" = "n/a" ] || [ "$c_sum" = "$py_sum" ]; }; then
    echo "checksums match"
else
    echo "checksum mismatch" >&2
fi

echo
echo "timings"
time_cmd "C" "$BUILD/loopmath_c"
time_cmd "Sealion" "$BUILD/loopmath_sealion"
if [ "$have_node" -eq 1 ]; then
    time_cmd "NodeJS" run_node_js
fi
if [ "$have_python" -eq 1 ]; then
    time_cmd "Python" python3 "$DIR/loopmath.py"
fi
