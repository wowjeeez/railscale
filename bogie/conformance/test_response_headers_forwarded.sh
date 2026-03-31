#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

# Verify proxy forwards upstream response headers to client

start_upstream_with_headers "X-Upstream-Custom: upstream-value-42\r\nX-Request-Id: req-abc-123\r\n" "ok"
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

response=$(curl -s -i --max-time 5 "http://127.0.0.1:${PROXY_PORT}/")

assert_status "$response" 200
assert_header_present "$response" "X-Upstream-Custom"
assert_header_present "$response" "X-Request-Id"
