use std::sync::Arc;
use rustls::crypto::ring::default_provider;
use train_track::DestinationRouter;
use trezorcarriage::TlsClientRouter;

fn install_crypto() {
    let _ = default_provider().install_default();
}

fn test_client_config() -> Arc<rustls::ClientConfig> {
    let mut root_store = rustls::RootCertStore::empty();
    let cert_pem = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = rustls::pki_types::CertificateDer::from(cert_pem.cert.der().to_vec());
    root_store.add(cert_der).unwrap();
    Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth(),
    )
}

#[test]
fn tls_client_router_fixed_is_send_sync() {
    install_crypto();
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<TlsClientRouter>();
    let config = test_client_config();
    let _router = TlsClientRouter::fixed("localhost:443".into(), config);
}

#[test]
fn tls_client_router_from_routing_key_is_send_sync() {
    install_crypto();
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<TlsClientRouter>();
    let config = test_client_config();
    let _router = TlsClientRouter::from_routing_key(config);
}

#[tokio::test]
async fn tls_client_router_fixed_route_fails_with_connection_refused() {
    install_crypto();
    let config = test_client_config();
    let router = TlsClientRouter::fixed("127.0.0.1:1".into(), config);
    let result = router.route(b"ignored").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn tls_client_router_from_routing_key_fails_with_bad_key() {
    install_crypto();
    let config = test_client_config();
    let router = TlsClientRouter::from_routing_key(config);
    let result = router.route(b"\xff\xff").await;
    assert!(result.is_err());
}
