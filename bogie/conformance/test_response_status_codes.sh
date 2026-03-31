#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

# Verify proxy forwards various upstream status codes faithfully

# 404
start_status_upstream 404 "Not Found"
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

response=$(curl -s -i --max-time 5 "http://127.0.0.1:${PROXY_PORT}/missing")
assert_status "$response" 404

cleanup
PIDS_TO_KILL=()

# 500
start_status_upstream 500 "Internal Server Error"
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

response=$(curl -s -i --max-time 5 "http://127.0.0.1:${PROXY_PORT}/")
assert_status "$response" 500

cleanup
PIDS_TO_KILL=()

# 301
start_upstream_raw "HTTP/1.1 301 Moved Permanently\r\nLocation: http://example.com/new\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

response=$(curl -s -i --max-time 5 -L0 "http://127.0.0.1:${PROXY_PORT}/old")
assert_status "$response" 301
