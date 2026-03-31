#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

start_echo_upstream
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

# Generate a 1MB body
body_file=$(mktemp)
dd if=/dev/urandom bs=1024 count=1024 2>/dev/null | base64 > "$body_file"

response=$(curl -s -i --max-time 10 \
    -X POST \
    --data-binary "@${body_file}" \
    "http://127.0.0.1:${PROXY_PORT}/upload")

rm -f "$body_file"

assert_status "$response" 200
