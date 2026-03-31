use std::sync::Arc;
use coupler::ForwardHttps;

fn test_server_config() -> Arc<rustls::ServerConfig> {
    rustls::crypto::ring::default_provider().install_default().ok();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = rustls::pki_types::CertificateDer::from(cert.cert.der().to_vec());
    let key_der = rustls::pki_types::PrivateKeyDer::try_from(cert.key_pair.serialize_der()).unwrap();
    Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .unwrap(),
    )
}

#[tokio::test]
async fn forward_https_creates_with_new() {
    let config = test_server_config();
    let flow = ForwardHttps::new("127.0.0.1:0", "127.0.0.1:1", config).await.unwrap();
    assert_ne!(flow.local_addr().port(), 0);
}

#[tokio::test]
async fn forward_https_builder_works() {
    let config = test_server_config();
    let flow = ForwardHttps::builder()
        .bind("127.0.0.1:0")
        .upstream("127.0.0.1:1")
        .tls_config(config)
        .build()
        .await
        .unwrap();
    assert_ne!(flow.local_addr().port(), 0);
}

#[tokio::test]
async fn forward_https_builder_fails_without_tls_config() {
    let result = ForwardHttps::builder()
        .bind("127.0.0.1:0")
        .upstream("127.0.0.1:1")
        .build()
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn forward_https_builder_fails_without_bind() {
    let config = test_server_config();
    let result = ForwardHttps::builder()
        .upstream("127.0.0.1:1")
        .tls_config(config)
        .build()
        .await;
    assert!(result.is_err());
}
