#!/usr/bin/env bash
# Build sealionjs, then compile & run every test in testerjs.
set -u

usage() {
    cat <<EOF
Usage: $(basename "$0") [-s] [-h]

Build sealionjs, then run every test in this directory with the
dynamic (JIT) runtime -- no executables are written to disk.
sicp.js is the shared library, not a test, and is skipped.
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
    if ! cargo install --path "$ROOT/../../sealionjs" --force >"$TMP/install.log" 2>&1; then
        cat "$TMP/install.log" >&2
        echo "cargo install failed" >&2
        exit 1
    fi
fi
[ -x "$BIN" ] || { echo "sealionjs binary not found at $BIN" >&2; exit 1; }

passed=0
run_failed=0
compile_failed=0

for src in "$ROOT"/*.js; do
    name=$(basename "$src")
    [ "$name" = sicp.js ] && continue

    # Run through the JIT: sealionjs itself exits with the program's value
    # (truncated to 8 bits, exactly like a native binary's exit status).
    "$BIN" "$src" >"$TMP/$name.run" 2>"$TMP/$name.err"
    status=$?

    # Compile errors are reported as "sealionjs: ..." on stderr, before any
    # program output and with exit code 1; a program value of 1 looks the same
    # otherwise, so use the message to tell them apart.
    if grep -q '^sealionjs: ' "$TMP/$name.err"; then
        ((compile_failed++))
        echo "MISS $name"
        sed 's/^/     | /' "$TMP/$name.err" >&2
        continue
    fi

    # Exit code -1 (255 for the shell) is the only real failure; any other
    # code is the test's own computed result.
    if [ "$status" -eq 255 ]; then
        ((run_failed++))
        echo "FAIL $name (exit -1)"
        sed 's/^/     | /' "$TMP/$name.run" >&2
        sed 's/^/     | /' "$TMP/$name.err" >&2
    else
        ((passed++))
        echo "OK   $name (exit $status)"
    fi
done

total=$((passed + run_failed + compile_failed))
echo
echo "total:    $total"
echo "passed:   $passed"
echo "run fail: $run_failed"
echo "compile:  $compile_failed"

[ $((run_failed + compile_failed)) -eq 0 ]
