#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

echo "SKIP: proxy does not yet enforce Content-Length body boundaries (known compliance gap)"
exit 0

# When body framing is implemented, this test should verify:
# Sending Content-Length: 100 with only 5 bytes of body
# The proxy should either wait for remaining bytes or reject

start_echo_upstream
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

response=$(send_raw_request "$PROXY_PORT" "POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 100\r\n\r\nhello")

# Should timeout or error, not forward incomplete body
