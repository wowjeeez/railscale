use std::sync::Arc;
use std::time::Duration;
use rustls::crypto::ring::default_provider;
use train_track::{Pipeline, CancellationToken, BufferLimits, NoHook, Service};
use carriage::http_v1::{HttpParser, HttpPipeline, HttpErrorResponder};
use carriage::tcp::native::TcpRouter;
use trezorcarriage::TlsSource;

#[tokio::main]
async fn main() {
    let _ = default_provider().install_default();
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage: tls_proxy <cert.pem> <key.pem> <upstream_addr> [listen_addr]");
        std::process::exit(1);
    }
    let cert_path = &args[1];
    let key_path = &args[2];
    let upstream_addr = &args[3];
    let listen_addr = args.get(4).map(|s| s.as_str()).unwrap_or("127.0.0.1:0");

    let cert_pem = std::fs::read(cert_path).unwrap();
    let key_pem = std::fs::read(key_path).unwrap();
    let certs: Vec<_> = rustls_pemfile::certs(&mut cert_pem.as_slice())
        .map(|r| r.unwrap())
        .collect();
    let key = rustls_pemfile::private_key(&mut key_pem.as_slice())
        .unwrap()
        .unwrap();

    let server_config = Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap(),
    );

    let source = TlsSource::bind(listen_addr, server_config).await.unwrap();
    let port = source.local_addr().port();
    println!("LISTENING:{port}");

    let pipeline = Pipeline {
        source,
        parser_factory: || HttpParser::new(vec![]),
        pipeline: Arc::new(HttpPipeline::new(vec![])),
        router: Arc::new(TcpRouter::fixed(upstream_addr.to_string())),
        error_responder: Some(Arc::new(HttpErrorResponder)),
        buffer_limits: BufferLimits::default(),
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
    pipeline.run(CancellationToken::new()).await.unwrap();
}
