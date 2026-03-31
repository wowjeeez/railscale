#!/usr/bin/env bash

PIDS_TO_KILL=()

cleanup() {
    if [ ${#PIDS_TO_KILL[@]} -gt 0 ]; then
        for pid in "${PIDS_TO_KILL[@]}"; do
            kill "$pid" 2>/dev/null
        done
        wait 2>/dev/null || true
    fi
}
trap cleanup EXIT

free_port() {
    python3 -c 'import socket; s=socket.socket(); s.bind(("",0)); print(s.getsockname()[1]); s.close()'
}

wait_for_port() {
    local port=$1
    local max_attempts=${2:-50}
    local attempt=0
    while ! nc -z 127.0.0.1 "$port" 2>/dev/null; do
        attempt=$((attempt + 1))
        if [ "$attempt" -ge "$max_attempts" ]; then
            echo "FAIL: port $port not ready after $max_attempts attempts"
            return 1
        fi
        sleep 0.1
    done
}

start_echo_upstream() {
    local port
    port=$(free_port)
    (while true; do
        echo -ne "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok" | nc -l 127.0.0.1 "$port" >/dev/null 2>&1
    done) &
    PIDS_TO_KILL+=($!)
    UPSTREAM_PORT=$port
    wait_for_port "$port"
}

start_upstream_with_body() {
    local body=$1
    local port
    port=$(free_port)
    local body_len=${#body}
    (while true; do
        echo -ne "HTTP/1.1 200 OK\r\nContent-Length: ${body_len}\r\nConnection: close\r\n\r\n${body}" | nc -l 127.0.0.1 "$port" >/dev/null 2>&1
    done) &
    PIDS_TO_KILL+=($!)
    UPSTREAM_PORT=$port
    wait_for_port "$port"
}

start_upstream_with_headers() {
    local extra_headers=$1
    local body=${2:-"ok"}
    local port
    port=$(free_port)
    local body_len=${#body}
    (while true; do
        echo -ne "HTTP/1.1 200 OK\r\n${extra_headers}Content-Length: ${body_len}\r\nConnection: close\r\n\r\n${body}" | nc -l 127.0.0.1 "$port" >/dev/null 2>&1
    done) &
    PIDS_TO_KILL+=($!)
    UPSTREAM_PORT=$port
    wait_for_port "$port"
}

start_upstream_raw() {
    local raw_response=$1
    local port
    port=$(free_port)
    (while true; do
        echo -ne "$raw_response" | nc -l 127.0.0.1 "$port" >/dev/null 2>&1
    done) &
    PIDS_TO_KILL+=($!)
    UPSTREAM_PORT=$port
    wait_for_port "$port"
}

start_slow_upstream() {
    local delay_secs=$1
    local port
    port=$(free_port)
    (while true; do
        nc -l 127.0.0.1 "$port" >/dev/null 2>&1
        sleep "$delay_secs"
        echo -ne "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nslow" | nc -l 127.0.0.1 "$port" >/dev/null 2>&1
    done) &
    PIDS_TO_KILL+=($!)
    UPSTREAM_PORT=$port
    wait_for_port "$port"
}

start_echo_body_upstream() {
    local port
    port=$(free_port)
    python3 -c "
import socket, threading

def handle(conn):
    data = b''
    while True:
        chunk = conn.recv(4096)
        if not chunk:
            break
        data += chunk
        if b'\r\n\r\n' in data:
            header_end = data.index(b'\r\n\r\n') + 4
            headers = data[:header_end].decode('latin-1')
            cl = 0
            for line in headers.split('\r\n'):
                if line.lower().startswith('content-length:'):
                    cl = int(line.split(':',1)[1].strip())
            body = data[header_end:]
            while len(body) < cl:
                chunk = conn.recv(4096)
                if not chunk:
                    break
                body += chunk
            resp_body = body
            resp = b'HTTP/1.1 200 OK\r\nContent-Length: ' + str(len(resp_body)).encode() + b'\r\nConnection: close\r\n\r\n' + resp_body
            conn.sendall(resp)
            break
    conn.close()

s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(('127.0.0.1', $port))
s.listen(32)
while True:
    conn, _ = s.accept()
    threading.Thread(target=handle, args=(conn,), daemon=True).start()
" &
    PIDS_TO_KILL+=($!)
    UPSTREAM_PORT=$port
    wait_for_port "$port"
}

start_status_upstream() {
    local status=$1
    local reason=$2
    local port
    port=$(free_port)
    (while true; do
        echo -ne "HTTP/1.1 ${status} ${reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n" | nc -l 127.0.0.1 "$port" >/dev/null 2>&1
    done) &
    PIDS_TO_KILL+=($!)
    UPSTREAM_PORT=$port
    wait_for_port "$port"
}

send_raw_request() {
    local port=$1
    local raw_data=$2
    local timeout=${3:-5}
    echo -ne "$raw_data" | nc -w "$timeout" 127.0.0.1 "$port" 2>/dev/null || true
}

start_proxy() {
    local upstream_addr=$1
    local port
    port=$(free_port)
    PROXY_PORT=$port
    "$CARGO_TARGET_DIR/release/examples/basic" "127.0.0.1:${port}" "$upstream_addr" &
    PIDS_TO_KILL+=($!)
    wait_for_port "$port"
}

generate_test_certs() {
    CERT_DIR=$(mktemp -d)
    openssl req -x509 -newkey rsa:2048 -nodes \
        -keyout "$CERT_DIR/key.pem" \
        -out "$CERT_DIR/cert.pem" \
        -days 1 \
        -subj '/CN=localhost' 2>/dev/null
    TEST_CERT="$CERT_DIR/cert.pem"
    TEST_KEY="$CERT_DIR/key.pem"
    export TEST_CERT TEST_KEY CERT_DIR
}

generate_expired_cert() {
    EXPIRED_DIR=$(mktemp -d)
    openssl req -x509 -newkey rsa:2048 -nodes \
        -keyout "$EXPIRED_DIR/key.pem" \
        -out "$EXPIRED_DIR/cert.pem" \
        -days 0 \
        -subj '/CN=localhost' 2>/dev/null
    EXPIRED_CERT="$EXPIRED_DIR/cert.pem"
    EXPIRED_KEY="$EXPIRED_DIR/key.pem"
    export EXPIRED_CERT EXPIRED_KEY EXPIRED_DIR
}

start_tls_proxy() {
    local upstream_addr="$1"
    local cert="$2"
    local key="$3"
    TLS_PROXY_PID=""
    local output
    output=$(mktemp)
    cargo run -p bogie --example tls_proxy -- "$cert" "$key" "$upstream_addr" "127.0.0.1:0" > "$output" 2>&1 &
    TLS_PROXY_PID=$!
    PIDS_TO_KILL+=("$TLS_PROXY_PID")
    local attempts=0
    while [ $attempts -lt 50 ]; do
        if grep -q 'LISTENING:' "$output" 2>/dev/null; then
            break
        fi
        sleep 0.1
        attempts=$((attempts + 1))
    done
    PROXY_TLS_PORT=$(grep 'LISTENING:' "$output" | sed 's/LISTENING://')
    rm -f "$output"
    export PROXY_TLS_PORT TLS_PROXY_PID
}

stop_tls_proxy() {
    if [ -n "$TLS_PROXY_PID" ]; then
        kill "$TLS_PROXY_PID" 2>/dev/null
        wait "$TLS_PROXY_PID" 2>/dev/null
    fi
}

cleanup_certs() {
    [ -n "$CERT_DIR" ] && rm -rf "$CERT_DIR"
    [ -n "$EXPIRED_DIR" ] && rm -rf "$EXPIRED_DIR"
}
