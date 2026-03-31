use std::sync::Arc;
use std::time::Duration;
use rustls::ServerConfig;
use rustls::crypto::ring::default_provider;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsConnector;
use tokio_util::sync::CancellationToken;
use train_track::{Pipeline, Service, BufferLimits};
use trezorcarriage::TlsSource;
use carriage::HttpParser;
use carriage::HttpPipeline;
use carriage::tcp::native::TcpRouter;

fn install_crypto() {
    let _ = default_provider().install_default();
}

fn generate_test_certs() -> (Vec<rustls::pki_types::CertificateDer<'static>>, rustls::pki_types::PrivateKeyDer<'static>) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let cert_der = rustls::pki_types::CertificateDer::from(cert.cert.der().to_vec());
    let key_der = rustls::pki_types::PrivateKeyDer::from(
        rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der())
    );
    (vec![cert_der], key_der)
}

fn make_server_config(
    certs: Vec<rustls::pki_types::CertificateDer<'static>>,
    key: rustls::pki_types::PrivateKeyDer<'static>,
) -> Arc<ServerConfig> {
    Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap()
    )
}

fn make_client_config(cert: &rustls::pki_types::CertificateDer<'static>) -> Arc<rustls::ClientConfig> {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.add(cert.clone()).unwrap();
    Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth()
    )
}

#[tokio::test]
async fn tls_termination_to_plaintext_upstream() {
    install_crypto();

    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();

    let upstream_handle = tokio::spawn(async move {
        let (mut stream, _) = upstream_listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let _n = stream.read(&mut buf).await.unwrap();
        let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
    });

    let (certs, key) = generate_test_certs();
    let client_config = make_client_config(&certs[0]);
    let server_config = make_server_config(certs, key);

    let source = TlsSource::bind("127.0.0.1:0", server_config).await.unwrap();
    let proxy_addr = source.local_addr();
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let pipeline_handle = tokio::spawn(async move {
        let pipeline = Pipeline {
            source,
            parser_factory: || HttpParser::new(vec![]),
            pipeline: Arc::new(HttpPipeline::new(vec![])),
            router: Arc::new(TcpRouter::fixed(format!("127.0.0.1:{}", upstream_addr.port()))),
            error_responder: None,
            buffer_limits: BufferLimits::default(),
            drain_timeout: Duration::from_secs(5),
        };
        pipeline.run(cancel_clone).await
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let tcp_stream = TcpStream::connect(proxy_addr).await.unwrap();
    let connector = TlsConnector::from(client_config);
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let mut tls = connector.connect(server_name, tcp_stream).await.unwrap();

    tls.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").await.unwrap();

    let mut response = vec![0u8; 4096];
    let n = tls.read(&mut response).await.unwrap();
    let response_str = String::from_utf8_lossy(&response[..n]);

    assert!(response_str.contains("200 OK"), "expected 200 OK, got: {}", response_str);
    assert!(response_str.contains("OK"), "expected body 'OK', got: {}", response_str);

    cancel.cancel();
    let _ = pipeline_handle.await;
    let _ = upstream_handle.await;
}

#[tokio::test]
async fn tls_termination_preserves_request_body() {
    install_crypto();

    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();

    let upstream_handle = tokio::spawn(async move {
        let (mut stream, _) = upstream_listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let received = String::from_utf8_lossy(&buf[..n]);

        assert!(received.contains("hello world"), "upstream should receive body, got: {}", received);

        let response = "HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\nreceived";
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
    });

    let (certs, key) = generate_test_certs();
    let client_config = make_client_config(&certs[0]);
    let server_config = make_server_config(certs, key);

    let source = TlsSource::bind("127.0.0.1:0", server_config).await.unwrap();
    let proxy_addr = source.local_addr();
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let pipeline_handle = tokio::spawn(async move {
        let pipeline = Pipeline {
            source,
            parser_factory: || HttpParser::new(vec![]),
            pipeline: Arc::new(HttpPipeline::new(vec![])),
            router: Arc::new(TcpRouter::fixed(format!("127.0.0.1:{}", upstream_addr.port()))),
            error_responder: None,
            buffer_limits: BufferLimits::default(),
            drain_timeout: Duration::from_secs(5),
        };
        pipeline.run(cancel_clone).await
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let tcp_stream = TcpStream::connect(proxy_addr).await.unwrap();
    let connector = TlsConnector::from(client_config);
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let mut tls = connector.connect(server_name, tcp_stream).await.unwrap();

    let body = "hello world";
    let request = format!(
        "POST /data HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    tls.write_all(request.as_bytes()).await.unwrap();

    let mut response = vec![0u8; 4096];
    let n = tls.read(&mut response).await.unwrap();
    let response_str = String::from_utf8_lossy(&response[..n]);

    assert!(response_str.contains("200 OK"), "expected 200 OK, got: {}", response_str);
    assert!(response_str.contains("received"), "expected body 'received', got: {}", response_str);

    cancel.cancel();
    let _ = pipeline_handle.await;
    let _ = upstream_handle.await;
}
