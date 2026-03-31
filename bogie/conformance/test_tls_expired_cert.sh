#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/helpers/common.sh"
trap 'stop_tls_proxy; cleanup_certs' EXIT

start_echo_upstream
generate_expired_cert
start_tls_proxy "127.0.0.1:$UPSTREAM_PORT" "$EXPIRED_CERT" "$EXPIRED_KEY"

if curl -s --cacert "$EXPIRED_CERT" "https://127.0.0.1:$PROXY_TLS_PORT/" 2>/dev/null; then
    exit 1
fi
exit 0
