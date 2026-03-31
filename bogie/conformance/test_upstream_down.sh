#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

start_proxy "127.0.0.1:1"

response=$(curl -s -i --max-time 5 "http://127.0.0.1:${PROXY_PORT}/" || true)

assert_status "$response" 502
