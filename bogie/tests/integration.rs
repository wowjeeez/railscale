use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use bogie::harness::*;

#[tokio::test]
async fn simple_get_200() {
    let upstream = TestUpstream::fixed_response(200, "OK", "hello").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(proxy.addr, b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    assert_status(&response, 200);
    assert_body_contains(&response, "hello");
}

#[tokio::test]
async fn post_with_content_length() {
    let upstream = TestUpstream::fixed_response(200, "OK", "accepted").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(
        proxy.addr,
        b"POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 5\r\n\r\nhello",
    )
    .await;
    assert_status(&response, 200);
    assert_body_contains(&response, "accepted");
}

#[tokio::test]
async fn malformed_request_returns_400() {
    let upstream = TestUpstream::echo().await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let response = send_raw(proxy.addr, b"not http at all").await;
    assert_status(&response, 400);
}

#[tokio::test]
async fn upstream_down_returns_502() {
    let proxy = TestProxy::new("127.0.0.1:1").await;
    let response = send_raw(proxy.addr, b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    assert_status(&response, 502);
}

#[tokio::test]
async fn large_body_streams() {
    let upstream = TestUpstream::fixed_response(200, "OK", "uploaded").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let body = vec![b'X'; 1024 * 1024];
    let headers = format!(
        "POST /upload HTTP/1.1\r\nHost: example.com\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    let mut client = TcpStream::connect(proxy.addr).await.unwrap();
    client.write_all(headers.as_bytes()).await.unwrap();
    let _ = client.write_all(&body).await;
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
async fn concurrent_clients() {
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    let mut handles = Vec::new();
    for _ in 0..10 {
        let addr = proxy.addr;
        handles.push(tokio::spawn(async move {
            let response =
                send_raw(addr, b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
            assert_status(&response, 200);
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
}

#[tokio::test]
async fn client_disconnect_mid_request() {
    let upstream = TestUpstream::fixed_response(200, "OK", "alive").await;
    let proxy = TestProxy::new(&upstream.addr.to_string()).await;
    {
        let mut client = TcpStream::connect(proxy.addr).await.unwrap();
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: exa")
            .await
            .unwrap();
        drop(client);
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let response = send_raw(proxy.addr, b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    assert_status(&response, 200);
    assert_body_contains(&response, "alive");
}

#[cfg(unix)]
#[tokio::test]
async fn unix_socket_get() {
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::net::UnixStream;
    use train_track::*;
    use carriage::http_v1::*;
    use carriage::tcp::unix_sockets::*;
    use carriage::tcp::native::*;

    let upstream = TestUpstream::fixed_response(200, "OK", "unix-ok").await;
    let sock_path = format!("/tmp/bogie-test-{}.sock", std::process::id());
    let source = SockSource::bind(&sock_path).unwrap();
    let router = Arc::new(TcpRouter::fixed(upstream.addr.to_string()));
    let pipeline_proc = Arc::new(HttpPipeline::new(vec![]));

    let sock_path_clone = sock_path.clone();
    let proxy_handle = tokio::spawn(async move {
        let pipeline = Pipeline {
            source,
            parser_factory: || HttpParser::new(vec![]),
            pipeline: pipeline_proc,
            router,
            error_responder: Some(Arc::new(HttpErrorResponder)),
            buffer_limits: Default::default(),
            drain_timeout: Duration::from_secs(30),
            #[cfg(feature = "metrics-full")]
            recorder: None,
        };
        pipeline.run(CancellationToken::new()).await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
        .await
        .unwrap();
    client.shutdown().await.unwrap();
    let mut response = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(2), client.read_to_end(&mut response)).await;
    let response_str = String::from_utf8_lossy(&response);
    assert!(response_str.contains("200 OK"));
    assert!(response_str.contains("unix-ok"));
    proxy_handle.abort();
    let _ = std::fs::remove_file(&sock_path_clone);
}
