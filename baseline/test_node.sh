#!/usr/bin/env bash
set -u

usage() {
    cat <<EOF
Usage: $(basename "$0") [-t seconds] [-d directory] [-q] [-h]

Run every .js file in the given directory with Node.js.

Options:
  -t seconds   Per-file timeout (default: 10, or \$TEST_TIMEOUT)
  -d directory Directory containing the tests (default: script directory)
  -q           Quiet; only print failures and the summary
  -h           Show this help and exit
EOF
}

timeout_seconds=${TEST_TIMEOUT:-10}
directory=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
quiet=0

while getopts ':t:d:qh' option; do
    case "$option" in
        t) timeout_seconds=$OPTARG ;;
        d) directory=$OPTARG ;;
        q) quiet=1 ;;
        h) usage; exit 0 ;;
        :) echo "$(basename "$0"): option -$OPTARG requires an argument" >&2; exit 2 ;;
        \?) echo "$(basename "$0"): unknown option -$OPTARG" >&2; usage >&2; exit 2 ;;
    esac
done

if ! command -v node >/dev/null 2>&1; then
    echo "node not found in PATH" >&2
    exit 1
fi
if [ ! -d "$directory" ]; then
    echo "not a directory: $directory" >&2
    exit 1
fi

shopt -s nullglob
files=("$directory"/*.js)
shopt -u nullglob

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

passed=0
failed=0
timed_out=0

for src in "${files[@]}"; do
    name=$(basename "$src")
    stdout_file="$tmp/$name.out"
    stderr_file="$tmp/$name.err"

    timeout --signal=KILL "${timeout_seconds}s" node "$src" >"$stdout_file" 2>"$stderr_file"
    status=$?

    if [ "$status" -eq 0 ]; then
        passed=$((passed + 1))
        if [ "$quiet" -eq 0 ]; then
            echo "OK    $name"
        fi
    elif [ "$status" -eq 137 ] || [ "$status" -eq 124 ]; then
        timed_out=$((timed_out + 1))
        echo "TIME  $name (${timeout_seconds}s)"
    else
        failed=$((failed + 1))
        echo "FAIL  $name (exit $status)"
        if [ -s "$stderr_file" ]; then
            sed -n '1,3p' "$stderr_file" | sed 's/^/      /'
        elif [ -s "$stdout_file" ]; then
            sed -n '1,3p' "$stdout_file" | sed 's/^/      /'
        fi
    fi
done

total=$((passed + failed + timed_out))
echo
echo "total:     $total"
echo "passed:    $passed"
echo "failed:    $failed"
echo "timed out: $timed_out"

[ $((failed + timed_out)) -eq 0 ]
