#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

start_echo_upstream
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

# Single header value exceeding 32KB
huge_value=$(python3 -c "print('X' * 32768)")

response=$(send_raw_request "$PROXY_PORT" "GET / HTTP/1.1\r\nHost: example.com\r\nX-Huge: ${huge_value}\r\n\r\n")

# Proxy should either reject (431/400) or forward - just verify no crash
if echo "$response" | head -1 | grep -q "HTTP/"; then
    echo "PASS: proxy responded (status: $(echo "$response" | head -1))"
else
    echo "FAIL: proxy did not respond to oversized header"
    exit 1
fi
