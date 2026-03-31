#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

# RFC 7230 5.4: A client MUST send a Host header in an HTTP/1.1 request
# The proxy should still handle this gracefully (either forward or reject)

start_echo_upstream
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

# HTTP/1.1 request without Host header - send raw to bypass curl's auto-Host
response=$(send_raw_request "$PROXY_PORT" "GET / HTTP/1.1\r\n\r\n")

# Proxy should either return 400 (strict) or forward anyway (permissive)
# Either response is acceptable - just verify proxy doesn't crash
if echo "$response" | head -1 | grep -q "HTTP/"; then
    echo "PASS: proxy responded (status: $(echo "$response" | head -1))"
else
    echo "FAIL: proxy did not respond"
    exit 1
fi
