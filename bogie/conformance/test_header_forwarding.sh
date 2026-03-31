#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

# Upstream that echoes back received headers as the response body
start_echo_body_upstream
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

# Send a request with custom headers and verify proxy forwards them
response=$(curl -s -i --max-time 5 \
    -H "X-Custom-Header: test-value-123" \
    -H "X-Another: foo" \
    "http://127.0.0.1:${PROXY_PORT}/")

assert_status "$response" 200
# The echo-body upstream will echo the raw request including headers
assert_body_contains "$response" "X-Custom-Header"
