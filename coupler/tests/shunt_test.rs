use bytes::Bytes;
use train_track::{Frame, Shunt};
use coupler::{OverTcp, OverUnix, OverTls};

struct TestFrame(Bytes);

impl Frame for TestFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { Some(&self.0) }
}

#[test]
fn over_tcp_fixed_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<OverTcp<TestFrame>>();
}

#[test]
fn over_tcp_from_routing_key_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    let _shunt = OverTcp::<TestFrame>::from_routing_key();
    assert_send_sync::<OverTcp<TestFrame>>();
}

#[tokio::test]
async fn over_tcp_fixed_connect_fails_with_refused() {
    let shunt = OverTcp::<TestFrame>::fixed("127.0.0.1:1");
    let result = shunt.connect(b"ignored").await;
    assert!(result.is_err());
}

#[test]
fn over_unix_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<OverUnix<TestFrame>>();
}

#[tokio::test]
async fn over_unix_connect_fails_with_no_socket() {
    let shunt = OverUnix::<TestFrame>::new("/tmp/nonexistent-railscale-test.sock");
    let result = shunt.connect(b"ignored").await;
    assert!(result.is_err());
}

#[test]
fn over_tls_fixed_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<OverTls<TestFrame>>();
}

#[tokio::test]
async fn over_tls_fixed_connect_fails_with_refused() {
    rustls::crypto::ring::default_provider().install_default().ok();
    let mut root_store = rustls::RootCertStore::empty();
    let cert_pem = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = rustls::pki_types::CertificateDer::from(cert_pem.cert.der().to_vec());
    root_store.add(cert_der).unwrap();
    let config = std::sync::Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth(),
    );
    let shunt = OverTls::<TestFrame>::fixed("127.0.0.1:1", config);
    let result = shunt.connect(b"ignored").await;
    assert!(result.is_err());
}
