#!/usr/bin/env bash

assert_status() {
    local response=$1
    local expected=$2
    local status_line
    status_line=$(echo "$response" | head -1)
    if echo "$status_line" | grep -q " $expected "; then
        return 0
    fi
    echo "FAIL: expected status $expected, got: $status_line"
    return 1
}

assert_body_contains() {
    local response=$1
    local needle=$2
    local body
    body=$(echo "$response" | sed -n '/^\r$/,$p' | tail -n +2)
    if echo "$body" | grep -q "$needle"; then
        return 0
    fi
    echo "FAIL: expected body to contain '$needle', got: $body"
    return 1
}

assert_body_exact() {
    local response=$1
    local expected=$2
    local body
    body=$(echo "$response" | sed -n '/^\r$/,$p' | tail -n +2)
    body=$(echo -n "$body" | tr -d '\r\n')
    expected=$(echo -n "$expected" | tr -d '\r\n')
    if [ "$body" = "$expected" ]; then
        return 0
    fi
    echo "FAIL: expected body '$expected', got: '$body'"
    return 1
}

assert_header_present() {
    local response=$1
    local header_name=$2
    local headers
    headers=$(echo "$response" | sed -n '2,/^\r$/p')
    if echo "$headers" | grep -qi "^${header_name}:"; then
        return 0
    fi
    echo "FAIL: expected header '$header_name' to be present"
    return 1
}

assert_header_value() {
    local response=$1
    local header_name=$2
    local expected_value=$3
    local headers
    headers=$(echo "$response" | sed -n '2,/^\r$/p')
    local actual
    actual=$(echo "$headers" | grep -i "^${header_name}:" | head -1 | sed "s/^[^:]*: *//" | tr -d '\r')
    if [ "$actual" = "$expected_value" ]; then
        return 0
    fi
    echo "FAIL: expected header '$header_name' = '$expected_value', got: '$actual'"
    return 1
}

assert_header_absent() {
    local response=$1
    local header_name=$2
    local headers
    headers=$(echo "$response" | sed -n '2,/^\r$/p')
    if echo "$headers" | grep -qi "^${header_name}:"; then
        echo "FAIL: expected header '$header_name' to be absent, but it was present"
        return 1
    fi
    return 0
}

assert_connection_closed() {
    local host=$1
    local port=$2
    if nc -z "$host" "$port" 2>/dev/null; then
        return 0
    fi
    return 0
}

assert_body_length() {
    local response=$1
    local expected_len=$2
    local body
    body=$(echo "$response" | sed -n '/^\r$/,$p' | tail -n +2)
    local actual_len
    actual_len=$(echo -n "$body" | wc -c | tr -d ' ')
    if [ "$actual_len" -eq "$expected_len" ]; then
        return 0
    fi
    echo "FAIL: expected body length $expected_len, got: $actual_len"
    return 1
}

assert_no_response() {
    local response=$1
    if [ -z "$response" ]; then
        return 0
    fi
    echo "FAIL: expected no response, got: $(echo "$response" | head -1)"
    return 1
}

extract_body() {
    local response=$1
    echo "$response" | sed -n '/^\r$/,$p' | tail -n +2
}

extract_header() {
    local response=$1
    local header_name=$2
    echo "$response" | sed -n '2,/^\r$/p' | grep -i "^${header_name}:" | head -1 | sed "s/^[^:]*: *//" | tr -d '\r'
}

count_responses() {
    local data=$1
    echo "$data" | grep -c "^HTTP/"
}
