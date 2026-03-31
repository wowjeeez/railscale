use criterion::{criterion_group, criterion_main, black_box, Criterion};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;
use bogie::harness::*;
use coupler::{ForwardHttp, ForwardTcp, ForwardHttps};
use socket2::SockRef;
use tokio_rustls::TlsConnector;
use rustls::pki_types::ServerName;

async fn connect_linger0(addr: std::net::SocketAddr) -> TcpStream {
    let stream = TcpStream::connect(addr).await.unwrap();
    let sock = SockRef::from(&stream);
    sock.set_linger(Some(Duration::ZERO)).unwrap();
    stream
}

async fn send_tls_linger0(
    addr: std::net::SocketAddr,
    config: std::sync::Arc<rustls::ClientConfig>,
    hostname: &str,
    data: &[u8],
) -> Vec<u8> {
    let tcp = connect_linger0(addr).await;
    let connector = TlsConnector::from(config);
    let server_name = ServerName::try_from(hostname.to_string()).unwrap();
    let mut tls = connector.connect(server_name, tcp).await.unwrap();
    tls.write_all(data).await.unwrap();
    tls.shutdown().await.unwrap();
    let mut response = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(2), tls.read_to_end(&mut response)).await;
    response
}

fn bench_forward_http(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let proxy_addr = rt.block_on(async {
        let upstream = Box::leak(Box::new(
            TestUpstream::fixed_response(200, "OK", "ok").await,
        ));
        let flow = ForwardHttp::new("127.0.0.1:0", &upstream.addr.to_string())
            .await
            .unwrap();
        let addr = flow.local_addr();
        tokio::spawn(async move { let _ = flow.run(CancellationToken::new()).await; });
        tokio::time::sleep(Duration::from_millis(50)).await;
        addr
    });

    c.bench_function("coupler_forward_http", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut client = connect_linger0(proxy_addr).await;
                client.write_all(b"GET / HTTP/1.1\r\nHost: bench\r\n\r\n").await.unwrap();
                client.shutdown().await.unwrap();
                let mut resp = Vec::new();
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    client.read_to_end(&mut resp),
                ).await;
                black_box(resp);
            })
        })
    });
}

fn bench_forward_tcp(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let proxy_addr = rt.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut conn, _) = match listener.accept().await {
                    Ok(c) => c,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = match conn.read(&mut buf).await {
                        Ok(n) => n,
                        Err(_) => return,
                    };
                    let _ = conn.write_all(&buf[..n]).await;
                    let _ = conn.shutdown().await;
                });
            }
        });

        let flow = ForwardTcp::new("127.0.0.1:0", &upstream_addr.to_string())
            .await
            .unwrap();
        let addr = flow.local_addr();
        tokio::spawn(async move { let _ = flow.run(CancellationToken::new()).await; });
        tokio::time::sleep(Duration::from_millis(50)).await;
        addr
    });

    c.bench_function("coupler_forward_tcp", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut client = connect_linger0(proxy_addr).await;
                client.write_all(b"hello bench payload").await.unwrap();
                client.shutdown().await.unwrap();
                let mut resp = Vec::new();
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    client.read_to_end(&mut resp),
                ).await;
                black_box(resp);
            })
        })
    });
}

fn bench_forward_https(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let (proxy_addr, client_config) = rt.block_on(async {
        install_crypto();
        let (server_config, client_config) = test_tls_certs("localhost");
        let upstream = Box::leak(Box::new(
            TestUpstream::fixed_response(200, "OK", "ok").await,
        ));
        let flow = ForwardHttps::new(
            "127.0.0.1:0",
            &upstream.addr.to_string(),
            server_config,
        )
        .await
        .unwrap();
        let addr = flow.local_addr();
        tokio::spawn(async move { let _ = flow.run(CancellationToken::new()).await; });
        tokio::time::sleep(Duration::from_millis(50)).await;
        (addr, client_config)
    });

    c.bench_function("coupler_forward_https", |b| {
        b.iter(|| {
            rt.block_on(async {
                let response = send_tls_linger0(
                    proxy_addr,
                    client_config.clone(),
                    "localhost",
                    b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n",
                ).await;
                black_box(response);
            })
        })
    });
}

criterion_group!(benches, bench_forward_http, bench_forward_tcp, bench_forward_https);
criterion_main!(benches);
