use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::time::Duration;
use bogie::harness::*;

#[tokio::test]
async fn null_bytes_in_header_value() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET / HTTP/1.1\r\nHost: example.com\r\nX-Evil: val\x00ue\r\n\r\n",
    )
    .await;
    let text = String::from_utf8_lossy(&response);
    assert!(
        text.contains("HTTP/"),
        "proxy should respond (accept or reject), not crash"
    );
}

#[tokio::test]
async fn null_byte_in_uri() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET /path\x00evil HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    let text = String::from_utf8_lossy(&response);
    assert!(
        text.contains("HTTP/"),
        "proxy should respond (accept or reject), not crash"
    );
}

#[tokio::test]
async fn cr_without_lf_in_header() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET / HTTP/1.1\rHost: example.com\r\n\r\n",
    )
    .await;
    let text = String::from_utf8_lossy(&response);
    assert!(
        text.contains("HTTP/") || response.is_empty(),
        "proxy should handle bare CR gracefully"
    );
}

#[tokio::test]
async fn lf_without_cr_in_headers() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET / HTTP/1.1\nHost: example.com\n\n",
    )
    .await;
    let text = String::from_utf8_lossy(&response);
    assert!(
        text.contains("HTTP/") || text.contains("400") || response.is_empty(),
        "proxy should handle bare LF gracefully"
    );
}

#[tokio::test]
async fn extremely_long_header_name() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;

    let long_name: String = std::iter::repeat('X').take(8192).collect();
    let request = format!(
        "GET / HTTP/1.1\r\nHost: example.com\r\n{}: val\r\n\r\n",
        long_name
    );

    let response = send_raw(proxy.addr, request.as_bytes()).await;
    let text = String::from_utf8_lossy(&response);
    assert!(
        text.contains("HTTP/"),
        "proxy should respond to oversized header name"
    );
}

#[tokio::test]
async fn extremely_long_uri() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;

    let long_uri: String = std::iter::repeat('/').take(16384).collect();
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: example.com\r\n\r\n",
        long_uri
    );

    let response = send_raw(proxy.addr, request.as_bytes()).await;
    let text = String::from_utf8_lossy(&response);
    // Proxy should either reject (413/414/400) or forward
    assert!(
        text.contains("HTTP/") || response.is_empty(),
        "proxy should handle oversized URI"
    );
}

#[tokio::test]
async fn request_line_missing_version() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(proxy.addr, b"GET /\r\nHost: example.com\r\n\r\n").await;
    let text = String::from_utf8_lossy(&response);
    assert!(
        text.contains("HTTP/"),
        "proxy should respond to malformed request line"
    );
}

#[tokio::test]
async fn request_line_unknown_method() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"FAKEMETH / HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    let text = String::from_utf8_lossy(&response);
    assert!(
        text.contains("HTTP/"),
        "proxy should handle unknown methods (forward or reject)"
    );
}

#[tokio::test]
async fn header_without_colon() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET / HTTP/1.1\r\nHost: example.com\r\nInvalidHeader\r\n\r\n",
    )
    .await;
    let text = String::from_utf8_lossy(&response);
    assert!(
        text.contains("HTTP/") || response.is_empty(),
        "proxy should handle header without colon"
    );
}

#[tokio::test]
async fn header_with_space_before_colon() {
    // RFC 7230 3.2.4: No whitespace between header field-name and colon
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET / HTTP/1.1\r\nHost: example.com\r\nX-Bad : value\r\n\r\n",
    )
    .await;
    let text = String::from_utf8_lossy(&response);
    assert!(
        text.contains("HTTP/"),
        "proxy should handle space-before-colon header"
    );
}

#[tokio::test]
async fn empty_header_value() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET / HTTP/1.1\r\nHost: example.com\r\nX-Empty:\r\n\r\n",
    )
    .await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn duplicate_host_headers() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET / HTTP/1.1\r\nHost: example.com\r\nHost: evil.com\r\n\r\n",
    )
    .await;
    let text = String::from_utf8_lossy(&response);
    assert!(
        text.contains("HTTP/"),
        "proxy should handle duplicate Host headers"
    );
}

#[tokio::test]
async fn connection_flood_resilience() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;

    let mut handles = Vec::new();
    for _ in 0..50 {
        let addr = proxy.addr;
        handles.push(tokio::spawn(async move {
            let result = TcpStream::connect(addr).await;
            if let Ok(mut stream) = result {
                let _ = stream
                    .write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
                    .await;
                let _ = stream.shutdown().await;
                let mut buf = Vec::new();
                let _ = tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut buf))
                    .await;
            }
        }));
    }
    for h in handles {
        let _ = h.await;
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    let response = send_raw(
        proxy.addr,
        b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn zero_length_body_with_content_length() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 0\r\n\r\n",
    )
    .await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn request_line_with_absolute_uri() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET http://example.com/path HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    let text = String::from_utf8_lossy(&response);
    assert!(
        text.contains("HTTP/"),
        "proxy should handle absolute-form URI"
    );
}
