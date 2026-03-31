#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"

echo "Building TLS proxy example..."
cargo build -p bogie --example tls_proxy 2>/dev/null

PASS=0
FAIL=0
TOTAL=0

for test_script in "$SCRIPT_DIR"/test_tls_*.sh; do
    test_name=$(basename "$test_script" .sh)
    TOTAL=$((TOTAL + 1))
    echo -n "  $test_name ... "
    if bash "$test_script" >/dev/null 2>&1; then
        echo "PASS"
        PASS=$((PASS + 1))
    else
        echo "FAIL"
        FAIL=$((FAIL + 1))
    fi
done

echo ""
echo "Results: $PASS/$TOTAL passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
