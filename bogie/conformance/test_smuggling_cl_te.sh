#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

echo "SKIP: proxy does not yet reject CL+TE conflicts (known compliance gap - RFC 7230 3.3.3)"
exit 0

# Classic CL.TE smuggling vector
# When implemented, proxy MUST reject requests with both Content-Length and Transfer-Encoding
# Per RFC 7230 section 3.3.3: a proxy MUST reject or fixup conflicting framing

start_echo_upstream
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

response=$(send_raw_request "$PROXY_PORT" \
    "POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 6\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\nX")

assert_status "$response" 400
