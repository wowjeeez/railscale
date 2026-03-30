use std::sync::Arc;
use bytes::Bytes;
use memchr::memmem::Finder;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use train_track::*;
use carriages::*;

#[tokio::test]
async fn full_http_pipeline_end_to_end() {
    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();

    let upstream_join = tokio::spawn(async move {
        let (mut conn, _) = upstream_listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let n = conn.read(&mut buf).await.unwrap();
        buf.truncate(n);

        conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").await.unwrap();
        buf
    });

    let source = TcpSource::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = source.local_addr();

    let matchers = vec![
        (Finder::new(b"User-Agent"), Bytes::from_static(b"railscale/1.0")),
    ];

    let router = Arc::new(TcpRouter::fixed(upstream_addr.to_string()));
    let pipeline_proc = Arc::new(HttpPipeline::new(matchers));

    let proxy_join = tokio::spawn(async move {
        let pipeline = Pipeline {
            source,
            parser_factory: || HttpParser::new(vec![]),
            pipeline: pipeline_proc,
            router,
            #[cfg(feature = "metrics-full")]
            recorder: None,
        };

        pipeline.run().await
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = TcpStream::connect(proxy_addr).await.unwrap();
    client.write_all(
        b"GET / HTTP/1.1\r\nHost: example.com\r\nUser-Agent: curl/7.0\r\n\r\n"
    ).await.unwrap();
    client.shutdown().await.unwrap();

    let received_by_upstream = upstream_join.await.unwrap();
    let received_str = String::from_utf8_lossy(&received_by_upstream);

    assert!(received_str.contains("GET / HTTP/1.1"));
    assert!(received_str.contains("Host: example.com"));
    assert!(received_str.contains("User-Agent: railscale/1.0"));
    assert!(!received_str.contains("curl/7.0"));

    proxy_join.abort();
}
