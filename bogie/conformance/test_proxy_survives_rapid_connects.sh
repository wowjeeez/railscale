#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
source "$SCRIPT_DIR/helpers/assertions.sh"

start_echo_upstream
start_proxy "127.0.0.1:${UPSTREAM_PORT}"

# Rapid-fire 20 connections, mix of complete and incomplete requests
for i in $(seq 1 10); do
    (echo -ne "GET / HTTP/1.1\r\nHost: example.com\r\n\r\n" | nc -w 1 127.0.0.1 "$PROXY_PORT" >/dev/null 2>&1) &
done

# Also some that disconnect immediately
for i in $(seq 1 10); do
    (echo -ne "" | nc -w 0 127.0.0.1 "$PROXY_PORT" >/dev/null 2>&1) &
done

wait

sleep 0.5

# Verify proxy still healthy
response=$(curl -s -i --max-time 5 "http://127.0.0.1:${PROXY_PORT}/")

assert_status "$response" 200
