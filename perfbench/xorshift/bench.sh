#!/usr/bin/env bash
PYTHON_RUNS=${PYTHON_RUNS:-3}
export PYTHON_RUNS
exec "$(dirname "${BASH_SOURCE[0]}")/../harness.sh" "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
