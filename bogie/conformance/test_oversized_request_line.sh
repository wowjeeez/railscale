#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

start_echo_upstream
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

# 16KB URI - well above typical 8KB limit
long_uri=$(python3 -c "print('/' + 'A' * 16000)")

response=$(send_raw_request "$PROXY_PORT" "GET ${long_uri} HTTP/1.1\r\nHost: example.com\r\n\r\n")

# Proxy should return 414 URI Too Long or 400 Bad Request, not crash
if echo "$response" | head -1 | grep -qE " (400|413|414) "; then
    echo "PASS: proxy rejected oversized URI"
elif echo "$response" | head -1 | grep -q "HTTP/"; then
    echo "WARN: proxy forwarded oversized URI (status: $(echo "$response" | head -1))"
else
    echo "FAIL: proxy did not respond to oversized URI"
    exit 1
fi
