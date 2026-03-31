use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use coupler::ForwardTcp;

async fn start_echo_tcp() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap();
                stream.write_all(&buf[..n]).await.unwrap();
                stream.shutdown().await.unwrap();
            });
        }
    });
    (addr, handle)
}

#[tokio::test]
async fn forward_tcp_proxies_raw_bytes() {
    let (upstream_addr, _upstream) = start_echo_tcp().await;
    let cancel = CancellationToken::new();
    let flow = ForwardTcp::new("127.0.0.1:0", &upstream_addr.to_string()).await.unwrap();
    let proxy_addr = flow.local_addr();
    let cancel_clone = cancel.clone();
    let handle = tokio::spawn(async move {
        let _ = flow.run(cancel_clone).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut tcp = TcpStream::connect(proxy_addr).await.unwrap();
    tcp.write_all(b"raw bytes here").await.unwrap();
    let mut response = vec![0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(5), tcp.read(&mut response))
        .await.unwrap().unwrap();
    assert_eq!(&response[..n], b"raw bytes here");

    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
}

#[tokio::test]
async fn forward_tcp_builder_works() {
    let flow = ForwardTcp::builder()
        .bind("127.0.0.1:0")
        .upstream("127.0.0.1:1")
        .build()
        .await
        .unwrap();
    assert_ne!(flow.local_addr().port(), 0);
}

#[tokio::test]
async fn forward_tcp_builder_fails_without_upstream() {
    let result = ForwardTcp::builder()
        .bind("127.0.0.1:0")
        .build()
        .await;
    assert!(result.is_err());
}
