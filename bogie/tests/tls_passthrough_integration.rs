use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use rustls::pki_types::ServerName;
use tokio_rustls::TlsConnector;
use tokio_util::sync::CancellationToken;
use train_track::{Pipeline, BufferLimits, NoHook, Service};
use trezorcarriage::{TlsParser, TlsPassthroughPipeline, Passthrough};
use carriage::tcp::native::{TcpSource, TcpRouter};

use bogie::harness::*;

async fn start_passthrough_proxy(upstream_addr: &str) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let source = TcpSource::bind("127.0.0.1:0").await.unwrap();
    let addr = source.local_addr();
    let router = Arc::new(TcpRouter::fixed(upstream_addr.to_string()));
    let pipeline_proc = Arc::new(TlsPassthroughPipeline::<Passthrough>::new());
    let cancel = CancellationToken::new();

    let handle = tokio::spawn(async move {
        let pipeline = Pipeline {
            source,
            parser_factory: || TlsParser::new(),
            pipeline: pipeline_proc,
            router,
            error_responder: None,
            buffer_limits: BufferLimits::default(),
            drain_timeout: Duration::from_secs(5),
            hook_factory: || NoHook,
            response_parser_factory: None::<fn() -> TlsParser>,
            response_pipeline: None,
            response_hook_factory: None,
            stabling_config: None,
            turnout_name: "proxy".to_string(),
            capture_dir: None,
        };
        let _ = pipeline.run(cancel).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    (addr, handle)
}

#[ignore]
#[tokio::test]
async fn passthrough_preserves_connection() {
    install_crypto();
    let upstream = TestTlsUpstream::fixed_response("upstream.local", 200, "OK", "passthrough-ok").await;
    let (proxy_addr, _handle) = start_passthrough_proxy(&upstream.addr.to_string()).await;

    let tcp = TcpStream::connect(proxy_addr).await.unwrap();
    let connector = TlsConnector::from(upstream.client_config.clone());
    let server_name = ServerName::try_from("upstream.local".to_string()).unwrap();
    let mut tls = connector.connect(server_name, tcp).await.unwrap();

    tls.write_all(b"GET / HTTP/1.1\r\nHost: upstream.local\r\n\r\n")
        .await
        .unwrap();
    let mut response = vec![0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(5), tls.read(&mut response))
        .await
        .unwrap()
        .unwrap();
    let response_str = String::from_utf8_lossy(&response[..n]);
    assert!(
        response_str.contains("200 OK"),
        "expected 200 OK, got: {}",
        response_str
    );
    assert!(
        response_str.contains("passthrough-ok"),
        "expected body, got: {}",
        response_str
    );
}

#[ignore]
#[tokio::test]
async fn passthrough_sni_routing() {
    install_crypto();
    let upstream = TestTlsUpstream::fixed_response("routed.local", 200, "OK", "sni-routed").await;
    let (proxy_addr, _handle) = start_passthrough_proxy(&upstream.addr.to_string()).await;

    let tcp = TcpStream::connect(proxy_addr).await.unwrap();
    let connector = TlsConnector::from(upstream.client_config.clone());
    let server_name = ServerName::try_from("routed.local".to_string()).unwrap();
    let mut tls = connector.connect(server_name, tcp).await.unwrap();

    tls.write_all(b"GET / HTTP/1.1\r\nHost: routed.local\r\n\r\n")
        .await
        .unwrap();
    let mut response = vec![0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(5), tls.read(&mut response))
        .await
        .unwrap()
        .unwrap();
    let response_str = String::from_utf8_lossy(&response[..n]);
    assert!(
        response_str.contains("sni-routed"),
        "expected sni-routed body, got: {}",
        response_str
    );
}

#[ignore]
#[tokio::test]
async fn passthrough_concurrent_sni_routing() {
    install_crypto();
    let upstream =
        TestTlsUpstream::fixed_response("concurrent.local", 200, "OK", "concurrent-ok").await;
    let (proxy_addr, _handle) = start_passthrough_proxy(&upstream.addr.to_string()).await;

    let mut handles = Vec::new();
    for _ in 0..5 {
        let addr = proxy_addr;
        let config = upstream.client_config.clone();
        handles.push(tokio::spawn(async move {
            let tcp = TcpStream::connect(addr).await.unwrap();
            let connector = TlsConnector::from(config);
            let server_name =
                ServerName::try_from("concurrent.local".to_string()).unwrap();
            let mut tls = connector.connect(server_name, tcp).await.unwrap();
            tls.write_all(b"GET / HTTP/1.1\r\nHost: concurrent.local\r\n\r\n")
                .await
                .unwrap();
            let mut response = vec![0u8; 4096];
            let n = tokio::time::timeout(Duration::from_secs(5), tls.read(&mut response))
                .await
                .unwrap()
                .unwrap();
            let response_str = String::from_utf8_lossy(&response[..n]);
            assert!(
                response_str.contains("concurrent-ok"),
                "expected concurrent-ok, got: {}",
                response_str
            );
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
}
