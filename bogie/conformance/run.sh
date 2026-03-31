#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PROJECT_ROOT/target}"

echo "=== Building reference binary ==="
cargo build --example basic -p carriage --release 2>&1

echo ""
echo "=== Running conformance tests ==="

passed=0
failed=0
failures=()

for test_script in "$SCRIPT_DIR"/test_*.sh; do
    test_name=$(basename "$test_script" .sh)
    if bash "$test_script" 2>&1; then
        echo "  PASS: $test_name"
        passed=$((passed + 1))
    else
        echo "  FAIL: $test_name"
        failed=$((failed + 1))
        failures+=("$test_name")
    fi
done

echo ""
echo "=== Results: $passed passed, $failed failed ==="

if [ ${#failures[@]} -gt 0 ]; then
    echo "Failures:"
    for f in "${failures[@]}"; do
        echo "  - $f"
    done
    exit 1
fi
