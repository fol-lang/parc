#!/usr/bin/env bash
set -euo pipefail

output="$(mktemp "${TMPDIR:-/tmp}/parc-filtered-test.XXXXXX")"
trap 'rm -f "$output"' EXIT

set +e
"$@" 2>&1 | tee "$output"
status="${PIPESTATUS[0]}"
set -e

if test "$status" -ne 0; then
    exit "$status"
fi

if ! grep -Eq '^running [1-9][0-9]* tests?$' "$output"; then
    echo "filtered test command selected zero tests" >&2
    exit 1
fi
