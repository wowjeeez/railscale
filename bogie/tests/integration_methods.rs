use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use bogie::harness::*;

#[tokio::test]
async fn head_request_200() {
    let upstream = TestUpstream::fixed_response(200, "OK", "hello").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(proxy.addr, b"HEAD / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn put_with_body() {
    let upstream = TestUpstream::fixed_response(200, "OK", "updated").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"PUT /resource HTTP/1.1\r\nHost: example.com\r\nContent-Length: 6\r\n\r\nupdate",
    )
    .await;
    assert_status(&response, 200);
    assert_body_contains(&response, "updated");
}

#[tokio::test]
async fn delete_request() {
    let upstream = TestUpstream::fixed_response(204, "No Content", "").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"DELETE /resource HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    assert_status(&response, 204);
}

#[tokio::test]
async fn options_request() {
    let upstream = TestUpstream::fixed_response(200, "OK", "").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"OPTIONS / HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn patch_with_body() {
    let upstream = TestUpstream::fixed_response(200, "OK", "patched").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"PATCH /resource HTTP/1.1\r\nHost: example.com\r\nContent-Length: 13\r\n\r\n{\"field\":\"v\"}",
    )
    .await;
    assert_status(&response, 200);
    assert_body_contains(&response, "patched");
}

#[tokio::test]
async fn custom_headers_forwarded_to_upstream() {
    let upstream = TestUpstream::echo().await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET / HTTP/1.1\r\nHost: example.com\r\nX-Custom: test-val\r\nX-Request-Id: abc-123\r\n\r\n",
    )
    .await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn binary_body_forwarded() {
    let upstream = TestUpstream::fixed_response(200, "OK", "received").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;

    let mut request = Vec::new();
    request.extend_from_slice(b"POST /upload HTTP/1.1\r\nHost: example.com\r\nContent-Length: 256\r\n\r\n");
    for i in 0u8..=255 {
        request.push(i);
    }

    let mut client = TcpStream::connect(proxy.addr).await.unwrap();
    client.write_all(&request).await.unwrap();
    let _ = client.shutdown().await;
    let mut response = Vec::new();
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.read_to_end(&mut response),
    )
    .await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn empty_body_post() {
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
async fn response_status_404_forwarded() {
    let upstream = TestUpstream::fixed_response(404, "Not Found", "not here").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET /missing HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    assert_status(&response, 404);
    assert_body_contains(&response, "not here");
}

#[tokio::test]
async fn response_status_500_forwarded() {
    let upstream = TestUpstream::fixed_response(500, "Internal Server Error", "oops").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    assert_status(&response, 500);
    assert_body_contains(&response, "oops");
}

#[tokio::test]
async fn response_status_301_forwarded() {
    let upstream = TestUpstream::fixed_response(301, "Moved Permanently", "").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET /old HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    assert_status(&response, 301);
}

#[tokio::test]
async fn http10_request() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ten").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"GET / HTTP/1.0\r\nHost: example.com\r\n\r\n",
    )
    .await;
    assert_status(&response, 200);
}
