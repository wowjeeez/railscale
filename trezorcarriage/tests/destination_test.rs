use std::sync::Arc;
use std::net::SocketAddr;
use bytes::Bytes;
use rustls::ServerConfig;
use rustls::crypto::ring::default_provider;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_rustls::{TlsAcceptor, TlsConnector};
use train_track::{StreamDestination, DestinationRouter};
use trezorcarriage::{TlsStreamDestination, TlsRouter};

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

async fn start_tls_echo_server(server_config: Arc<ServerConfig>) -> (JoinHandle<()>, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(server_config);

    let handle = tokio::spawn(async move {
        let (tcp_stream, _) = listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(tcp_stream).await.unwrap();
        let (mut read_half, mut write_half) = tokio::io::split(tls_stream);
        tokio::io::copy(&mut read_half, &mut write_half).await.unwrap();
        write_half.shutdown().await.unwrap();
    });

    (handle, addr)
}

async fn connect_tls_client(addr: SocketAddr, client_config: Arc<rustls::ClientConfig>) -> tokio_rustls::client::TlsStream<TcpStream> {
    let connector = TlsConnector::from(client_config);
    let tcp = TcpStream::connect(addr).await.unwrap();
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    connector.connect(server_name, tcp).await.unwrap()
}

#[tokio::test]
async fn destination_writes_to_upstream() {
    install_crypto();
    let (certs, key) = generate_test_certs();
    let client_config = make_client_config(&certs[0]);
    let server_config = make_server_config(certs, key);

    let (_server_handle, addr) = start_tls_echo_server(server_config).await;
    let tls_stream = connect_tls_client(addr, client_config).await;

    let mut dest = TlsStreamDestination::new(tls_stream);
    dest.write(Bytes::from_static(b"hello tls destination")).await.unwrap();
    let _reader = dest.response_reader();
}

#[tokio::test]
async fn router_fixed_connects_tls() {
    install_crypto();
    let (certs, key) = generate_test_certs();
    let client_config = make_client_config(&certs[0]);
    let server_config = make_server_config(certs, key);

    let (_server_handle, addr) = start_tls_echo_server(server_config).await;
    let router = TlsRouter::fixed(addr.to_string(), "localhost", client_config);

    let mut dest = router.route(b"ignored").await.unwrap();
    dest.write(Bytes::from_static(b"routed fixed")).await.unwrap();
    let _reader = dest.response_reader();
}

#[tokio::test]
async fn router_from_routing_key_extracts_host() {
    install_crypto();
    let (certs, key) = generate_test_certs();
    let client_config = make_client_config(&certs[0]);
    let server_config = make_server_config(certs, key);

    let (_server_handle, addr) = start_tls_echo_server(server_config).await;
    let router = TlsRouter::from_routing_key(client_config);

    let routing_key = format!("localhost:{}", addr.port());
    let mut dest = router.route(routing_key.as_bytes()).await.unwrap();
    dest.write(Bytes::from_static(b"routed by key")).await.unwrap();
    let _reader = dest.response_reader();
}
