#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
trap 'stop_tls_proxy; cleanup_certs' EXIT

generate_test_certs
start_tls_proxy "127.0.0.1:1" "$TEST_CERT" "$TEST_KEY"

RESPONSE=$(curl -s -i --cacert "$TEST_CERT" "https://127.0.0.1:$PROXY_TLS_PORT/" || true)
echo "$RESPONSE" | grep -q "502"
