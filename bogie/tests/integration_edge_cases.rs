use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::time::Duration;
use bogie::harness::*;

#[tokio::test]
async fn slow_upstream_response() {
    use std::sync::Arc;
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        let (mut conn, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let _ = tokio::time::timeout(Duration::from_secs(2), conn.read(&mut buf)).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let response = "HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\ndelayed";
        let _ = conn.write_all(response.as_bytes()).await;
    });

    let proxy = TestProxy::new(&addr.to_string()).await;
    let mut client = TcpStream::connect(proxy.addr).await.unwrap();
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
        .await
        .unwrap();
    let _ = client.shutdown().await;
    let mut response = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), client.read_to_end(&mut response)).await;
    assert_status(&response, 200);
    assert_body_contains(&response, "delayed");
    handle.abort();
}

#[tokio::test]
async fn many_headers_forwarded() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;

    let mut request = String::from("GET / HTTP/1.1\r\nHost: example.com\r\n");
    for i in 0..50 {
        request.push_str(&format!("X-Header-{}: value-{}\r\n", i, i));
    }
    request.push_str("\r\n");

    let response = send_raw(proxy.addr, request.as_bytes()).await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn large_header_value() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;

    let large_value: String = std::iter::repeat('X').take(4000).collect();
    let request = format!(
        "GET / HTTP/1.1\r\nHost: example.com\r\nX-Large: {}\r\n\r\n",
        large_value
    );

    let response = send_raw(proxy.addr, request.as_bytes()).await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn empty_response_body() {
    let upstream = TestUpstream::fixed_response(204, "No Content", "").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"DELETE /item HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    assert_status(&response, 204);
}

#[tokio::test]
async fn multiple_sequential_connections() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;

    for i in 0..5 {
        let response = send_raw(
            proxy.addr,
            b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n",
        )
        .await;
        assert_status(&response, 200);
        assert_body_contains(&response, "ok");
    }
}

#[tokio::test]
async fn partial_request_then_complete() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;

    let mut client = TcpStream::connect(proxy.addr).await.unwrap();
    client.write_all(b"GET / HTTP/1.1\r\n").await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    client.write_all(b"Host: example.com\r\n").await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    client.write_all(b"\r\n").await.unwrap();
    let _ = client.shutdown().await;

    let mut response = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(3), client.read_to_end(&mut response)).await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn headers_split_across_packets() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;

    let mut client = TcpStream::connect(proxy.addr).await.unwrap();
    // Send header name split mid-token
    client.write_all(b"GET / HTTP/1.1\r\nHo").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    client
        .write_all(b"st: example.com\r\nX-Cust")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    client.write_all(b"om: split-val\r\n\r\n").await.unwrap();
    let _ = client.shutdown().await;

    let mut response = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(3), client.read_to_end(&mut response)).await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn uri_with_query_string() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET /path?key=value&foo=bar HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn uri_with_fragment() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET /path#section HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn uri_with_encoded_chars() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET /path%20with%20spaces HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn client_write_byte_at_a_time() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;

    let request = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut client = TcpStream::connect(proxy.addr).await.unwrap();
    for byte in request.iter() {
        client.write_all(&[*byte]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let _ = client.shutdown().await;

    let mut response = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), client.read_to_end(&mut response)).await;
    assert_status(&response, 200);
}
