#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

start_echo_upstream
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

# Build curl args with 50 custom headers
header_args=""
for i in $(seq 1 50); do
    header_args="$header_args -H \"X-Header-${i}: value-${i}\""
done

response=$(eval curl -s -i --max-time 5 $header_args "http://127.0.0.1:${PROXY_PORT}/")

assert_status "$response" 200
