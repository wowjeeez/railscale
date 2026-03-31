use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io;
use tokio::net::TcpListener;
use rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;
use train_track::StreamSource;

pub struct TlsSource {
    listener: TcpListener,
    acceptor: TlsAcceptor,
}

impl TlsSource {
    pub async fn bind(addr: &str, config: Arc<ServerConfig>) -> Result<Self, io::Error> {
        let listener = TcpListener::bind(addr).await?;
        let acceptor = TlsAcceptor::from(config);
        Ok(Self { listener, acceptor })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.listener.local_addr().unwrap()
    }
}

impl StreamSource for TlsSource {
    type ReadHalf = tokio::io::ReadHalf<tokio_rustls::server::TlsStream<tokio::net::TcpStream>>;
    type WriteHalf = tokio::io::WriteHalf<tokio_rustls::server::TlsStream<tokio::net::TcpStream>>;
    type Error = io::Error;

    fn accept(&self) -> impl std::future::Future<Output = Result<(Self::ReadHalf, Self::WriteHalf), Self::Error>> + Send {
        async {
            let (tcp_stream, _addr) = self.listener.accept().await?;
            let tls_stream = self.acceptor.accept(tcp_stream).await?;
            Ok(tokio::io::split(tls_stream))
        }
    }
}
