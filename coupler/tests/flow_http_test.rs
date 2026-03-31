use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use train_track::BufferLimits;
use coupler::ForwardHttp;

async fn start_echo_upstream() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                    n,
                    std::str::from_utf8(&buf[..n]).unwrap_or("binary")
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.shutdown().await.unwrap();
            });
        }
    });
    (addr, handle)
}

#[tokio::test]
async fn forward_http_new_proxies_request() {
    let (upstream_addr, _upstream) = start_echo_upstream().await;
    let cancel = CancellationToken::new();
    let flow = ForwardHttp::new("127.0.0.1:0", &upstream_addr.to_string()).await.unwrap();
    let proxy_addr = flow.local_addr();
    let cancel_clone = cancel.clone();
    let handle = tokio::spawn(async move {
        let _ = flow.run(cancel_clone).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut tcp = TcpStream::connect(proxy_addr).await.unwrap();
    tcp.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").await.unwrap();
    let mut response = vec![0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(5), tcp.read(&mut response))
        .await.unwrap().unwrap();
    let response_str = String::from_utf8_lossy(&response[..n]);
    assert!(response_str.contains("200 OK"), "expected 200 OK, got: {}", response_str);

    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
}

#[tokio::test]
async fn forward_http_builder_with_defaults() {
    let (upstream_addr, _upstream) = start_echo_upstream().await;
    let cancel = CancellationToken::new();
    let flow = ForwardHttp::builder()
        .bind("127.0.0.1:0")
        .upstream(&upstream_addr.to_string())
        .build()
        .await
        .unwrap();
    let proxy_addr = flow.local_addr();
    let cancel_clone = cancel.clone();
    let handle = tokio::spawn(async move {
        let _ = flow.run(cancel_clone).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut tcp = TcpStream::connect(proxy_addr).await.unwrap();
    tcp.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").await.unwrap();
    let mut response = vec![0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(5), tcp.read(&mut response))
        .await.unwrap().unwrap();
    let response_str = String::from_utf8_lossy(&response[..n]);
    assert!(response_str.contains("200 OK"), "expected 200 OK, got: {}", response_str);

    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
}

#[tokio::test]
async fn forward_http_builder_custom_buffer_limits() {
    let (upstream_addr, _upstream) = start_echo_upstream().await;
    let flow = ForwardHttp::builder()
        .bind("127.0.0.1:0")
        .upstream(&upstream_addr.to_string())
        .buffer_limits(BufferLimits { max_pre_route_bytes: 32 * 1024, max_post_route_bytes: 32 * 1024 })
        .drain_timeout(Duration::from_secs(1))
        .build()
        .await
        .unwrap();
    assert_ne!(flow.local_addr().port(), 0);
}
