use std::sync::Arc;
use bytes::Bytes;
use memchr::memmem::Finder;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use train_track::*;
use carriages::*;
use std::time::Duration;

async fn spawn_proxy_simple(upstream_addr: String) -> (std::net::SocketAddr, tokio::task::JoinHandle<Result<(), RailscaleError>>) {
    let source = TcpSource::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = source.local_addr();
    let router = Arc::new(TcpRouter::fixed(upstream_addr));
    let pipeline_proc = Arc::new(HttpPipeline::new(vec![]));

    let handle = tokio::spawn(async move {
        let pipeline = Pipeline {
            source,
            parser_factory: || HttpParser::new(vec![]),
            pipeline: pipeline_proc,
            router,
            error_responder: None,
            buffer_limits: Default::default(),
            drain_timeout: Duration::from_secs(30),
            #[cfg(feature = "metrics-full")]
            recorder: None,
        };
        pipeline.run(CancellationToken::new()).await
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (proxy_addr, handle)
}

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
            error_responder: None,
            buffer_limits: Default::default(),
            drain_timeout: Duration::from_secs(30),
            #[cfg(feature = "metrics-full")]
            recorder: None,
        };

        pipeline.run(CancellationToken::new()).await
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

#[tokio::test]
async fn malformed_http_request_returns_error() {
    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();

    let source = TcpSource::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = source.local_addr();
    let router = Arc::new(TcpRouter::fixed(upstream_addr.to_string()));
    let pipeline_proc = Arc::new(HttpPipeline::new(vec![]));

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
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    {
        let mut client = TcpStream::connect(proxy_addr).await.unwrap();
        client.write_all(b"no crlf here").await.unwrap();
        client.shutdown().await.unwrap();

        let mut response = Vec::new();
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client.read_to_end(&mut response),
        ).await;

        let response_str = String::from_utf8_lossy(&response);
        assert!(
            response_str.contains("400"),
            "expected 400 Bad Request (no routing frame), got: {response_str}"
        );
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!proxy_handle.is_finished());
    proxy_handle.abort();
}

#[tokio::test]
async fn client_disconnect_mid_request() {
    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();

    let (proxy_addr, proxy_handle) = spawn_proxy_simple(upstream_addr.to_string()).await;

    {
        let mut client = TcpStream::connect(proxy_addr).await.unwrap();
        client.write_all(b"GET / HTTP/1.1\r\nHost: exa").await.unwrap();
        drop(client);
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert!(!proxy_handle.is_finished());

    let upstream_listener2 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr2 = upstream_listener2.local_addr().unwrap();

    let (proxy_addr2, proxy_handle2) = spawn_proxy_simple(upstream_addr2.to_string()).await;

    let upstream_task = tokio::spawn(async move {
        let (mut conn, _) = upstream_listener2.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let n = conn.read(&mut buf).await.unwrap();
        buf.truncate(n);
        conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").await.unwrap();
    });

    let mut client2 = TcpStream::connect(proxy_addr2).await.unwrap();
    client2.write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await.unwrap();
    client2.shutdown().await.unwrap();

    let mut response = Vec::new();
    client2.read_to_end(&mut response).await.unwrap();
    let response_str = String::from_utf8_lossy(&response);
    assert!(response_str.contains("200 OK"));

    upstream_task.await.unwrap();
    proxy_handle.abort();
    proxy_handle2.abort();
}

#[tokio::test]
async fn large_body_streams_without_full_buffering() {
    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();

    let body_size: usize = 1024 * 1024;

    let upstream_task = tokio::spawn(async move {
        let (mut conn, _) = upstream_listener.accept().await.unwrap();

        let mut received = Vec::new();
        let mut buf = vec![0u8; 65536];
        loop {
            match tokio::time::timeout(
                std::time::Duration::from_secs(3),
                conn.read(&mut buf),
            ).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => received.extend_from_slice(&buf[..n]),
                Ok(Err(_)) => break,
                Err(_) => break,
            }
        }

        let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
        let _ = conn.write_all(response.as_bytes()).await;

        received
    });

    let (proxy_addr, proxy_handle) = spawn_proxy_simple(upstream_addr.to_string()).await;

    let body = vec![b'X'; body_size];
    let headers = format!(
        "POST /upload HTTP/1.1\r\nHost: example.com\r\nContent-Length: {}\r\n\r\n",
        body_size
    );

    let mut client = TcpStream::connect(proxy_addr).await.unwrap();
    client.write_all(headers.as_bytes()).await.unwrap();
    let _ = client.write_all(&body).await;
    let _ = client.shutdown().await;

    let mut response = Vec::new();
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.read_to_end(&mut response),
    ).await;

    let received = upstream_task.await.unwrap();
    let received_str = String::from_utf8_lossy(&received);
    assert!(received_str.contains("POST /upload HTTP/1.1"));
    assert!(received_str.contains("Host: example.com"));

    proxy_handle.abort();
}

#[tokio::test]
async fn concurrent_clients_through_proxy() {
    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();

    let upstream_task = tokio::spawn(async move {
        loop {
            let accept_result = upstream_listener.accept().await;
            let (mut conn, _) = match accept_result {
                Ok(c) => c,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = conn.read(&mut buf).await.unwrap_or(0);
                if n == 0 { return; }
                let request = String::from_utf8_lossy(&buf[..n]);

                let id = request.lines()
                    .find(|l| l.starts_with("X-Client-Id:"))
                    .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
                    .unwrap_or_default();

                let body = format!("client-{id}");
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                let _ = conn.write_all(response.as_bytes()).await;
            });
        }
    });

    let (proxy_addr, proxy_handle) = spawn_proxy_simple(upstream_addr.to_string()).await;

    let client_count = 10;
    let mut handles = Vec::new();

    for i in 0..client_count {
        let addr = proxy_addr;
        handles.push(tokio::spawn(async move {
            let mut client = TcpStream::connect(addr).await.unwrap();
            let request = format!(
                "GET / HTTP/1.1\r\nHost: example.com\r\nX-Client-Id: {i}\r\n\r\n"
            );
            client.write_all(request.as_bytes()).await.unwrap();
            client.shutdown().await.unwrap();

            let mut response = Vec::new();
            client.read_to_end(&mut response).await.unwrap();
            let response_str = String::from_utf8_lossy(&response).to_string();
            assert!(
                response_str.contains(&format!("client-{i}")),
                "client {i} got wrong response: {response_str}"
            );
            i
        }));
    }

    let mut results = Vec::new();
    for h in handles {
        results.push(h.await.unwrap());
    }
    results.sort();
    assert_eq!(results, (0..client_count).collect::<Vec<_>>());

    proxy_handle.abort();
    upstream_task.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn unix_socket_end_to_end() {
    use tokio::net::UnixStream;

    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();

    let upstream_task = tokio::spawn(async move {
        let (mut conn, _) = upstream_listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let n = conn.read(&mut buf).await.unwrap();
        buf.truncate(n);
        conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").await.unwrap();
        buf
    });

    let sock_path = format!("/tmp/railscale-test-{}.sock", std::process::id());
    let source = SockSource::bind(&sock_path).unwrap();
    let router = Arc::new(TcpRouter::fixed(upstream_addr.to_string()));
    let pipeline_proc = Arc::new(HttpPipeline::new(vec![]));

    let sock_path_clone = sock_path.clone();
    let proxy_handle = tokio::spawn(async move {
        let pipeline = Pipeline {
            source,
            parser_factory: || HttpParser::new(vec![]),
            pipeline: pipeline_proc,
            router,
            error_responder: None,
            buffer_limits: Default::default(),
            drain_timeout: Duration::from_secs(30),
            #[cfg(feature = "metrics-full")]
            recorder: None,
        };
        pipeline.run(CancellationToken::new()).await
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    client.write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await.unwrap();
    client.shutdown().await.unwrap();

    let mut response = Vec::new();
    client.read_to_end(&mut response).await.unwrap();
    let response_str = String::from_utf8_lossy(&response);
    assert!(response_str.contains("200 OK"));
    assert!(response_str.contains("ok"));

    let received = upstream_task.await.unwrap();
    let received_str = String::from_utf8_lossy(&received);
    assert!(received_str.contains("GET / HTTP/1.1"));

    proxy_handle.abort();
    let _ = std::fs::remove_file(&sock_path_clone);
}

#[tokio::test]
async fn upstream_connection_refused() {
    let source = TcpSource::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = source.local_addr();
    let router = Arc::new(TcpRouter::fixed("127.0.0.1:1".to_string()));
    let pipeline_proc = Arc::new(HttpPipeline::new(vec![]));

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
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = TcpStream::connect(proxy_addr).await.unwrap();
    client.write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await.unwrap();
    client.shutdown().await.unwrap();

    let mut response = Vec::new();
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        client.read_to_end(&mut response),
    ).await;

    let response_str = String::from_utf8_lossy(&response);
    assert!(
        response_str.contains("502"),
        "expected 502 Bad Gateway, got: {response_str}"
    );

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!proxy_handle.is_finished());
    proxy_handle.abort();
}
