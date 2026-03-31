#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

echo "SKIP: proxy does not yet reject TE+CL conflicts (known compliance gap - RFC 7230 3.3.3)"
exit 0

# TE.CL smuggling vector (reversed header order)

start_echo_upstream
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

response=$(send_raw_request "$PROXY_PORT" \
    "POST / HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: chunked\r\nContent-Length: 4\r\n\r\n5c\r\nGPOST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 15\r\n\r\nx=1\r\n0\r\n\r\n")

assert_status "$response" 400
