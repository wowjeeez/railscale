use criterion::{criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use train_track::*;
use carriage::http_v1::*;
use carriage::tcp::native::*;
use criterion::black_box;
use bogie::harness::*;

async fn setup() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>, tokio::task::JoinHandle<Result<(), RailscaleError>>) {
    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();

    let upstream_handle = tokio::spawn(async move {
        loop {
            let (mut conn, _) = match upstream_listener.accept().await {
                Ok(c) => c,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 65536];
                let _ = conn.read(&mut buf).await;
                let _ = conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").await;
            });
        }
    });

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
            error_responder: None,
            buffer_limits: Default::default(),
            drain_timeout: Duration::from_secs(30),
            hook_factory: || NoHook,
            response_parser_factory: None::<fn() -> HttpParser>,
            response_pipeline: None,
            response_hook_factory: None,
            stabling_config: None,
            turnout_name: "proxy".to_string(),
            capture_dir: None,
            #[cfg(feature = "metrics-full")]
            recorder: None,
        };
        pipeline.run(CancellationToken::new()).await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    (proxy_addr, upstream_handle, proxy_handle)
}

fn bench_pipeline_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("pipeline_throughput");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("simple_get_rps", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (proxy_addr, upstream_handle, proxy_handle) = setup().await;
                for _ in 0..100 {
                    let mut client = TcpStream::connect(proxy_addr).await.unwrap();
                    client.write_all(b"GET / HTTP/1.1\r\nHost: bench\r\n\r\n").await.unwrap();
                    client.shutdown().await.unwrap();
                    let mut resp = Vec::new();
                    let _ = tokio::time::timeout(Duration::from_secs(1), client.read_to_end(&mut resp)).await;
                }
                proxy_handle.abort();
                upstream_handle.abort();
            })
        })
    });

    group.finish();
}

fn bench_tls_pipeline(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let (proxy_addr, client_config) = rt.block_on(async {
        install_crypto();
        let upstream =
            Box::leak(Box::new(TestUpstream::fixed_response(200, "OK", "bench").await));
        let proxy =
            Box::leak(Box::new(TestTlsProxy::new(&upstream.addr.to_string()).await));
        (proxy.addr, proxy.client_config.clone())
    });

    c.bench_function("tls_termination_throughput", |b| {
        b.iter(|| {
            rt.block_on(async {
                let response = send_tls_raw(
                    proxy_addr,
                    client_config.clone(),
                    "localhost",
                    b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n",
                )
                .await;
                black_box(response);
            });
        })
    });
}

criterion_group!(benches, bench_pipeline_throughput, bench_tls_pipeline);
criterion_main!(benches);
