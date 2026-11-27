#!/usr/bin/env bash
exec "$(dirname "${BASH_SOURCE[0]}")/../harness.sh" "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
