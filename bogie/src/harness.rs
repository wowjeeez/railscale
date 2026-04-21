use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use train_track::*;
use carriage::http_v1::*;
use carriage::tcp::native::*;
use rustls::crypto::ring::default_provider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use trezorcarriage::TlsSource;

pub struct TestUpstream {
    pub addr: SocketAddr,
    handle: JoinHandle<()>,
}

impl TestUpstream {
    pub async fn echo() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            loop {
                let (mut conn, _) = match listener.accept().await {
                    Ok(c) => c,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut tmp = vec![0u8; 4096];
                    loop {
                        match tokio::time::timeout(Duration::from_secs(2), conn.read(&mut tmp)).await
                        {
                            Ok(Ok(0)) => break,
                            Ok(Ok(n)) => buf.extend_from_slice(&tmp[..n]),
                            Ok(Err(_)) => break,
                            Err(_) => break,
                        }
                    }
                    let n = buf.len();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\nreceived {} bytes",
                        format!("received {} bytes", n).len(),
                        n
                    );
                    let _ = conn.write_all(response.as_bytes()).await;
                });
            }
        });
        Self { addr, handle }
    }

    pub async fn fixed_response(status: u16, reason: &'static str, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            loop {
                let (mut conn, _) = match listener.accept().await {
                    Ok(c) => c,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    let mut tmp = vec![0u8; 4096];
                    loop {
                        match tokio::time::timeout(Duration::from_secs(2), conn.read(&mut tmp))
                            .await
                        {
                            Ok(Ok(0)) => break,
                            Ok(Ok(_)) => break,
                            Ok(Err(_)) => break,
                            Err(_) => break,
                        }
                    }
                    let response = format!(
                        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\n\r\n{}",
                        status,
                        reason,
                        body.len(),
                        body
                    );
                    let _ = conn.write_all(response.as_bytes()).await;
                });
            }
        });
        Self { addr, handle }
    }
}

impl TestUpstream {
    pub async fn multi_response(status: u16, reason: &'static str, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            loop {
                let (mut conn, _) = match listener.accept().await {
                    Ok(c) => c,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    loop {
                        let mut request_buf = Vec::new();
                        loop {
                            match tokio::time::timeout(Duration::from_secs(5), conn.read(&mut buf)).await {
                                Ok(Ok(0)) => return,
                                Ok(Ok(n)) => {
                                    request_buf.extend_from_slice(&buf[..n]);
                                    if request_buf.windows(4).any(|w| w == b"\r\n\r\n") {
                                        break;
                                    }
                                }
                                Ok(Err(_)) => return,
                                Err(_) => return,
                            }
                        }
                        let response = format!(
                            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\n\r\n{}",
                            status, reason, body.len(), body
                        );
                        if conn.write_all(response.as_bytes()).await.is_err() {
                            return;
                        }
                    }
                });
            }
        });
        Self { addr, handle }
    }
}

impl Drop for TestUpstream {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub struct TestProxy {
    pub addr: SocketAddr,
    handle: JoinHandle<Result<(), RailscaleError>>,
}

impl TestProxy {
    pub async fn new(upstream_addr: &str) -> Self {
        let source = TcpSource::bind("127.0.0.1:0").await.unwrap();
        let addr = source.local_addr();
        let router = Arc::new(TcpRouter::fixed(upstream_addr.to_string()));
        let pipeline_proc = Arc::new(HttpPipeline::new(vec![]));

        let handle = tokio::spawn(async move {
            let pipeline = Pipeline {
                source,
                parser_factory: || HttpParser::new(vec![]),
                pipeline: pipeline_proc,
                router,
                error_responder: Some(Arc::new(HttpErrorResponder)),
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
        Self { addr, handle }
    }
}

impl TestProxy {
    pub async fn new_with_keepalive(upstream_addr: &str) -> Self {
        let source = TcpSource::bind("127.0.0.1:0").await.unwrap();
        let addr = source.local_addr();
        let router = Arc::new(TcpRouter::fixed(upstream_addr.to_string()));
        let pipeline_proc = Arc::new(HttpPipeline::new(vec![]));

        let handle = tokio::spawn(async move {
            let pipeline = Pipeline {
                source,
                parser_factory: || HttpParser::new(vec![]),
                pipeline: pipeline_proc,
                router,
                error_responder: Some(Arc::new(HttpErrorResponder)),
                buffer_limits: Default::default(),
                drain_timeout: Duration::from_secs(30),
                hook_factory: || HttpDeriverHook::new(),
                response_parser_factory: Some(|| ResponseParser::new()),
                response_pipeline: None,
                response_hook_factory: Some(|| HttpDeriverHook::new()),
                stabling_config: Some(StablingConfig::default()),
            turnout_name: "proxy".to_string(),
            capture_dir: None,
                #[cfg(feature = "metrics-full")]
                recorder: None,
            };
            pipeline.run(CancellationToken::new()).await
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        Self { addr, handle }
    }
}

impl Drop for TestProxy {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub async fn send_raw(addr: SocketAddr, data: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(data).await.unwrap();
    stream.shutdown().await.unwrap();
    let mut response = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut response)).await;
    response
}

pub fn assert_status(response: &[u8], expected: u16) {
    let text = String::from_utf8_lossy(response);
    let needle = format!(" {} ", expected);
    assert!(
        text.contains(&needle),
        "expected status {expected} in response: {text}"
    );
}

pub fn assert_body_contains(response: &[u8], needle: &str) {
    let text = String::from_utf8_lossy(response);
    let separator = "\r\n\r\n";
    let body = text
        .find(separator)
        .map(|pos| &text[pos + separator.len()..])
        .unwrap_or("");
    assert!(
        body.contains(needle),
        "expected body to contain \"{needle}\", got: {body}"
    );
}

pub fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

pub fn install_crypto() {
    let _ = default_provider().install_default();
}

pub fn test_tls_certs(hostname: &str) -> (Arc<ServerConfig>, Arc<ClientConfig>) {
    let certified_key =
        rcgen::generate_simple_self_signed(vec![hostname.to_string()]).unwrap();
    let cert_der = CertificateDer::from(certified_key.cert.der().to_vec());
    let key_der = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(
        certified_key.key_pair.serialize_der(),
    ));

    let server_config = Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der.clone()], key_der)
            .unwrap(),
    );

    let mut root_store = RootCertStore::empty();
    root_store.add(cert_der).unwrap();
    let client_config = Arc::new(
        ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth(),
    );

    (server_config, client_config)
}

pub struct TestTlsProxy {
    pub addr: SocketAddr,
    pub client_config: Arc<ClientConfig>,
    handle: JoinHandle<Result<(), RailscaleError>>,
}

impl TestTlsProxy {
    pub async fn new(upstream_addr: &str) -> Self {
        install_crypto();
        let (server_config, client_config) = test_tls_certs("localhost");
        let source = TlsSource::bind("127.0.0.1:0", server_config).await.unwrap();
        let addr = source.local_addr();
        let router = Arc::new(TcpRouter::fixed(upstream_addr.to_string()));
        let pipeline_proc = Arc::new(HttpPipeline::new(vec![]));

        let handle = tokio::spawn(async move {
            let pipeline = Pipeline {
                source,
                parser_factory: || HttpParser::new(vec![]),
                pipeline: pipeline_proc,
                router,
                error_responder: Some(Arc::new(HttpErrorResponder)),
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
        Self {
            addr,
            client_config,
            handle,
        }
    }
}

impl Drop for TestTlsProxy {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub async fn send_tls_raw(
    addr: SocketAddr,
    client_config: Arc<ClientConfig>,
    hostname: &str,
    data: &[u8],
) -> Vec<u8> {
    let tcp = TcpStream::connect(addr).await.unwrap();
    let connector = TlsConnector::from(client_config);
    let server_name = ServerName::try_from(hostname.to_string()).unwrap();
    let mut tls = connector.connect(server_name, tcp).await.unwrap();
    tls.write_all(data).await.unwrap();
    tls.shutdown().await.unwrap();
    let mut response = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(2), tls.read_to_end(&mut response)).await;
    response
}

pub struct TestTlsUpstream {
    pub addr: SocketAddr,
    pub client_config: Arc<ClientConfig>,
    handle: JoinHandle<()>,
}

impl TestTlsUpstream {
    pub async fn fixed_response(
        hostname: &str,
        status: u16,
        reason: &'static str,
        body: &'static str,
    ) -> Self {
        install_crypto();
        let (server_config, client_config) = test_tls_certs(hostname);
        let acceptor = TlsAcceptor::from(server_config);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            loop {
                let (tcp_stream, _) = match listener.accept().await {
                    Ok(c) => c,
                    Err(_) => break,
                };
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let mut tls_stream = match acceptor.accept(tcp_stream).await {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    let mut buf = vec![0u8; 4096];
                    let _ = tokio::time::timeout(
                        Duration::from_secs(2),
                        tls_stream.read(&mut buf),
                    )
                    .await;
                    let response = format!(
                        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\n\r\n{}",
                        status,
                        reason,
                        body.len(),
                        body
                    );
                    let _ = tls_stream.write_all(response.as_bytes()).await;
                    let _ = tls_stream.shutdown().await;
                });
            }
        });
        Self {
            addr,
            client_config,
            handle,
        }
    }
}

impl Drop for TestTlsUpstream {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
