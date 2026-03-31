#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

# Verify proxy faithfully forwards response body content

start_upstream_with_body "the-exact-response-payload"
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

response=$(curl -s -i --max-time 5 "http://127.0.0.1:${PROXY_PORT}/")

assert_status "$response" 200
assert_body_contains "$response" "the-exact-response-payload"
