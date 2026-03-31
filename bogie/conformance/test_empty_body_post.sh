#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

start_echo_upstream
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

# POST with Content-Length: 0 (empty body)
response=$(curl -s -i --max-time 5 -X POST \
    -H "Content-Length: 0" \
    "http://127.0.0.1:${PROXY_PORT}/")

assert_status "$response" 200
