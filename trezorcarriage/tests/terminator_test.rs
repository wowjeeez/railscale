use std::sync::Arc;
use rustls::ServerConfig;
use rustls::crypto::ring::default_provider;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use train_track::StreamSource;
use trezorcarriage::TlsSource;

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
async fn tls_source_accepts_and_decrypts() {
    install_crypto();
    let (certs, key) = generate_test_certs();
    let client_config = make_client_config(&certs[0]);
    let server_config = make_server_config(certs, key);

    let source = TlsSource::bind("127.0.0.1:0", server_config).await.unwrap();
    let addr = source.local_addr();

    let server = tokio::spawn(async move {
        let (mut read_half, _write_half) = source.accept().await.unwrap();
        let mut buf = vec![0u8; 64];
        let n = read_half.read(&mut buf).await.unwrap();
        String::from_utf8(buf[..n].to_vec()).unwrap()
    });

    let connector = TlsConnector::from(client_config);
    let tcp = TcpStream::connect(addr).await.unwrap();
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let mut tls_stream = connector.connect(server_name, tcp).await.unwrap();
    tls_stream.write_all(b"hello tls").await.unwrap();
    tls_stream.shutdown().await.unwrap();

    let received = server.await.unwrap();
    assert_eq!(received, "hello tls");
}

#[tokio::test]
async fn tls_source_write_half_sends_encrypted() {
    install_crypto();
    let (certs, key) = generate_test_certs();
    let client_config = make_client_config(&certs[0]);
    let server_config = make_server_config(certs, key);

    let source = TlsSource::bind("127.0.0.1:0", server_config).await.unwrap();
    let addr = source.local_addr();

    let server = tokio::spawn(async move {
        let (_read_half, mut write_half) = source.accept().await.unwrap();
        write_half.write_all(b"response from server").await.unwrap();
        write_half.shutdown().await.unwrap();
    });

    let connector = TlsConnector::from(client_config);
    let tcp = TcpStream::connect(addr).await.unwrap();
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let mut tls_stream = connector.connect(server_name, tcp).await.unwrap();

    let mut buf = vec![0u8; 64];
    let n = tls_stream.read(&mut buf).await.unwrap();
    let received = String::from_utf8(buf[..n].to_vec()).unwrap();

    server.await.unwrap();
    assert_eq!(received, "response from server");
}

#[tokio::test]
async fn tls_source_rejects_non_tls_client() {
    install_crypto();
    let (certs, key) = generate_test_certs();
    let server_config = make_server_config(certs, key);

    let source = TlsSource::bind("127.0.0.1:0", server_config).await.unwrap();
    let addr = source.local_addr();

    let server = tokio::spawn(async move {
        source.accept().await
    });

    let mut tcp = TcpStream::connect(addr).await.unwrap();
    tcp.write_all(b"this is not tls data at all").await.unwrap();
    tcp.shutdown().await.unwrap();

    let result = server.await.unwrap();
    assert!(result.is_err());
}
