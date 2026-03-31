use std::sync::Arc;
use coupler::ForwardHttpToHttps;

fn test_client_config() -> Arc<rustls::ClientConfig> {
    rustls::crypto::ring::default_provider().install_default().ok();
    let cert_pem = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = rustls::pki_types::CertificateDer::from(cert_pem.cert.der().to_vec());
    let mut root_store = rustls::RootCertStore::empty();
    root_store.add(cert_der).unwrap();
    Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth(),
    )
}

#[tokio::test]
async fn forward_http_to_https_creates_with_new() {
    let config = test_client_config();
    let flow = ForwardHttpToHttps::new("127.0.0.1:0", "127.0.0.1:443", config).await.unwrap();
    assert_ne!(flow.local_addr().port(), 0);
}

#[tokio::test]
async fn forward_http_to_https_builder_works() {
    let config = test_client_config();
    let flow = ForwardHttpToHttps::builder()
        .bind("127.0.0.1:0")
        .upstream("127.0.0.1:443")
        .tls_config(config)
        .build()
        .await
        .unwrap();
    assert_ne!(flow.local_addr().port(), 0);
}

#[tokio::test]
async fn forward_http_to_https_builder_fails_without_tls_config() {
    let result = ForwardHttpToHttps::builder()
        .bind("127.0.0.1:0")
        .upstream("127.0.0.1:443")
        .build()
        .await;
    assert!(result.is_err());
}
