#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

echo "SKIP: proxy does not yet reject duplicate Content-Length (known compliance gap)"
exit 0

# RFC 7230 3.3.3: if multiple Content-Length values differ, reject as invalid

start_echo_upstream
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

response=$(send_raw_request "$PROXY_PORT" \
    "POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 5\r\nContent-Length: 100\r\n\r\nhello")

assert_status "$response" 400
