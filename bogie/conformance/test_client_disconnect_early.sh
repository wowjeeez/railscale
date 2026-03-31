#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

start_echo_upstream
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

# Send partial request then disconnect
echo -ne "GET / HTTP/1.1\r\nHost: ex" | nc -w 1 127.0.0.1 "$PROXY_PORT" 2>/dev/null || true

# Small delay, then verify proxy is still alive and accepting connections
sleep 0.5

response=$(curl -s -i --max-time 5 "http://127.0.0.1:${PROXY_PORT}/")

assert_status "$response" 200
