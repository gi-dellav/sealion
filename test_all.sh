#!/usr/bin/env bash
# Build sealionjs, then compile & run every test in testerjs.
set -u

usage() {
    cat <<EOF
Usage: $(basename "$0") [-s] [-h]

Build sealionjs, then compile & run every test in testerjs.
At the end, report how many passed, how many compiled but failed
while running, and how many did not compile.

Options:
  -s   Skip the \`cargo install --path sealionjs\` phase and reuse the
       already installed sealionjs binary (it must exist).
  -h   Show this help and exit.
EOF
}

skip_install=0
while getopts ':sh' opt; do
    case "$opt" in
        s) skip_install=1 ;;
        h) usage; exit 0 ;;
        \?) usage >&2; exit 2 ;;
    esac
done

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
BIN="${CARGO_HOME:-$HOME/.cargo}/bin/sealionjs"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

if [ "$skip_install" -eq 1 ]; then
    echo "Skipping install (-s), using $BIN..."
else
    echo "Installing sealionjs..."
    if ! cargo install --path "$ROOT/sealionjs" --force >"$TMP/install.log" 2>&1; then
        cat "$TMP/install.log" >&2
        echo "cargo install failed" >&2
        exit 1
    fi
fi
[ -x "$BIN" ] || { echo "sealionjs binary not found at $BIN" >&2; exit 1; }

passed=0
run_failed=0
compile_failed=0

for src in "$ROOT"/testerjs/*.js; do
    name=$(basename "$src")
    out="$TMP/$name.bin"

    if ! "$BIN" "$src" -o "$out" >"$TMP/$name.compile" 2>&1; then
        ((compile_failed++))
        echo "MISS $name"
        continue
    fi

    if "$out" >"$TMP/$name.run" 2>&1; then
        ((passed++))
        echo "OK   $name"
    else
        ((run_failed++))
        echo "FAIL $name"
    fi
done

total=$((passed + run_failed + compile_failed))
echo
echo "total:    $total"
echo "passed:   $passed"
echo "run fail: $run_failed"
echo "compile:  $compile_failed"

[ $((run_failed + compile_failed)) -eq 0 ]
