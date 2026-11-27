#!/usr/bin/env bash
# Shared perfbench harness for one item directory.
# Expects <item>/<item>.js, <item>/<item>.c and <item>/<item>.py.
# Invoked by <item>/bench.sh.
#
# usage: harness.sh <item-dir>
# env:   RUNS (default 50), PYTHON_RUNS (default 10), CC
set -u

ITEM_DIR=${1:?usage: harness.sh <item-dir>}
ITEM=$(basename "$ITEM_DIR")
JS=$ITEM_DIR/$ITEM.js
PY=$ITEM_DIR/$ITEM.py
CSRC=$ITEM_DIR/$ITEM.c
BUILD=$ITEM_DIR/build
RUNS=${RUNS:-50}
PYTHON_RUNS=${PYTHON_RUNS:-10}

for f in "$JS" "$PY" "$CSRC"; do
    if [ ! -f "$f" ]; then
        echo "error: missing $f" >&2
        exit 1
    fi
done

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

have_sealion=1
have_node=1
have_python=1
command -v sealionjs >/dev/null 2>&1 || have_sealion=0
command -v node >/dev/null 2>&1 || have_node=0
command -v python3 >/dev/null 2>&1 || have_python=0

echo "== $ITEM"

echo "compiling C with $CC_BIN"
c_compile_start=$(now_ns)
"$CC_BIN" -O3 -fno-pic -no-pie -o "$BUILD/${ITEM}_c" "$CSRC" -lm || exit 1
c_compile_end=$(now_ns)

aot_ok=0
artifact_ok=0
sealion_compile_start=0
sealion_compile_end=0
artifact_compile_start=0
artifact_compile_end=0
if [ "$have_sealion" -eq 1 ]; then
    echo "compiling with sealionjs (AOT)"
    sealion_compile_start=$(now_ns)
    if sealionjs --no-pic "$JS" -o "$BUILD/${ITEM}_sealion" 2>"$BUILD/aot.log"; then
        aot_ok=1
    fi
    sealion_compile_end=$(now_ns)
    if [ "$aot_ok" -eq 0 ]; then
        echo "note: AOT rejected $ITEM.js, backends limited to JIT"
        head -2 "$BUILD/aot.log" 2>/dev/null | sed 's/^/       /'
    fi
    echo "compiling with sealionjs (JIT artifact)"
    artifact_compile_start=$(now_ns)
    if sealionjs --artifact "$BUILD/${ITEM}.artifact" "$JS" 2>"$BUILD/artifact.log"; then
        artifact_ok=1
    else
        echo "error: artifact compilation failed" >&2
        head -2 "$BUILD/artifact.log" 2>/dev/null | sed 's/^/       /' >&2
    fi
    artifact_compile_end=$(now_ns)
else
    echo "warning: sealionjs not found; skipping Sealion benchmarks" >&2
fi

awk -v c_start="$c_compile_start" -v c_end="$c_compile_end" \
    -v s_start="$sealion_compile_start" -v s_end="$sealion_compile_end" \
    -v a_start="$artifact_compile_start" -v a_end="$artifact_compile_end" \
    -v have_sealion="$have_sealion" -v aot_ok="$aot_ok" -v artifact_ok="$artifact_ok" 'BEGIN {
    printf "compile time  C: %.1f ms", (c_end - c_start) / 1000000
    if (have_sealion == 0) {
        printf "   Sealion: not installed\n"
        exit
    }
    if (aot_ok == 1) {
        printf "   Sealion AOT: %.1f ms", (s_end - s_start) / 1000000
    } else {
        printf "   Sealion AOT: n/a (dynamic runtime only)"
    }
    if (artifact_ok == 1) {
        printf "   Sealion (JIT artifact): %.1f ms", (a_end - a_start) / 1000000
    }
    printf "\n"
}'

jit_available=0
if [ "$have_sealion" -eq 1 ]; then
    printf '0;\n' > "$BUILD/jit_probe.js"
    sealionjs --no-cache --no-pic "$BUILD/jit_probe.js" >/dev/null 2>"$BUILD/jit_probe.log" || true
    if grep -q "in-process JIT" "$BUILD/jit_probe.log"; then
        jit_available=1
    fi
    rm -f "$BUILD/jit_probe.js" "$BUILD/jit_probe.log"
fi

if [ "$have_sealion" -eq 1 ] && [ "$jit_available" -eq 0 ]; then
    echo "warning: sealionjs was built without the jit feature; skipping JIT benchmark" >&2
    echo "         install with: cargo install --path sealionjs --features jit" >&2
fi

run_node_js() {
    node -e 'const fs = require("fs"); eval(fs.readFileSync(process.argv[1], "utf8") + "\nconsole.log(result);");' "$JS"
}

declare -A TOTALS

time_cmd() {
    local name=$1
    local runs=$2
    shift 2
    local total=0
    local worst=0
    local start end i dur
    for ((i = 0; i < runs; i++)); do
        start=$(now_ns)
        "$@" >/dev/null 2>&1
        end=$(now_ns)
        dur=$((end - start))
        total=$((total + dur))
        if [ "$dur" -gt "$worst" ]; then
            worst=$dur
        fi
    done
    TOTALS[$name]=$total
    awk -v name="$name" -v total="$total" -v worst="$worst" -v runs="$runs" 'BEGIN {
        printf "%-16s %10.1f ms/run  worst %10.1f ms  (%d runs)\n", name, total / runs / 1000000, worst / 1000000, runs
    }'
}

echo
echo "checksums"
c_sum=$("$BUILD/${ITEM}_c")
printf "%-16s %s\n" "C" "$c_sum"

aot_sum="n/a"
if [ "$aot_ok" -eq 1 ]; then
    aot_sum=0
    "$BUILD/${ITEM}_sealion" >/dev/null 2>&1 || aot_sum=$?
    printf "%-16s %s\n" "Sealion AOT" "$aot_sum"
else
    printf "%-16s %s\n" "Sealion AOT" "not compiled, skipped"
fi

jit_sum="n/a"
if [ "$artifact_ok" -eq 1 ] && [ "$jit_available" -eq 1 ]; then
    jit_sum=0
    sealionjs --run-artifact "$BUILD/${ITEM}.artifact" >/dev/null 2>&1 || jit_sum=$?
    printf "%-16s %s\n" "Sealion JIT" "$jit_sum"
elif [ "$artifact_ok" -eq 0 ]; then
    printf "%-16s %s\n" "Sealion JIT" "artifact build failed, skipped"
else
    printf "%-16s %s\n" "Sealion JIT" "not enabled, skipped"
fi

node_sum="n/a"
if [ "$have_node" -eq 1 ]; then
    node_sum=$(run_node_js)
    printf "%-16s %s\n" "NodeJS" "$node_sum"
else
    printf "%-16s %s\n" "NodeJS" "not found, skipped"
fi

py_sum="n/a"
if [ "$have_python" -eq 1 ]; then
    py_sum=$(python3 "$PY")
    printf "%-16s %s\n" "Python" "$py_sum"
else
    printf "%-16s %s\n" "Python" "not found, skipped"
fi

checks_ok=1
for sum in "$aot_sum" "$jit_sum" "$node_sum" "$py_sum"; do
    if [ "$sum" != "n/a" ] && [ "$sum" != "$c_sum" ]; then
        checks_ok=0
    fi
done
if [ "$checks_ok" -eq 1 ]; then
    echo "checksums match"
else
    echo "checksum mismatch" >&2
fi

echo
echo "timings (AOT runs the prebuilt executable; JIT loads a precompiled artifact, so parse and codegen are excluded)"
time_cmd "C" "$RUNS" "$BUILD/${ITEM}_c"
if [ "$aot_ok" -eq 1 ]; then
    time_cmd "Sealion AOT" "$RUNS" "$BUILD/${ITEM}_sealion"
fi
if [ "$artifact_ok" -eq 1 ] && [ "$jit_available" -eq 1 ]; then
    time_cmd "Sealion JIT" "$RUNS" sealionjs --run-artifact "$BUILD/${ITEM}.artifact"
fi
if [ "$have_node" -eq 1 ]; then
    time_cmd "NodeJS" "$RUNS" run_node_js
fi
if [ "$have_python" -eq 1 ]; then
    time_cmd "Python" "$PYTHON_RUNS" python3 "$PY"
fi

c_total=${TOTALS[C]:-0}
if [ "$c_total" -gt 0 ]; then
    echo
    echo "relative to C"
    for name in "C" "Sealion AOT" "Sealion JIT" "NodeJS" "Python"; do
        total=${TOTALS[$name]:-}
        [ -n "$total" ] || continue
        awk -v name="$name" -v total="$total" -v c_total="$c_total" 'BEGIN {
            printf "%-16s %6.2fx\n", name, total / c_total
        }'
    done
fi
