use std::sync::Arc;
use std::time::Duration;
use bytes::Bytes;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use rustls::ClientConfig;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use train_track::{StreamDestination, DestinationRouter, RailscaleError, ErrorKind};

pub struct TlsStreamDestination {
    upstream: TlsStream<TcpStream>,
    inactivity_timeout: Option<Duration>,
}

impl TlsStreamDestination {
    pub fn new(stream: TlsStream<TcpStream>) -> Self {
        Self {
            upstream: stream,
            inactivity_timeout: None,
        }
    }

    pub fn with_timeout(mut self, d: Duration) -> Self {
        self.inactivity_timeout = Some(d);
        self
    }

    pub fn upstream_mut(&mut self) -> &mut TlsStream<TcpStream> {
        &mut self.upstream
    }
}

#[async_trait::async_trait]
impl StreamDestination for TlsStreamDestination {
    type Error = std::io::Error;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.upstream.write_all(&bytes).await
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(
        &mut self,
        client: &mut W,
    ) -> Result<u64, Self::Error> {
        tokio::io::copy(&mut self.upstream, client).await
    }
}

pub struct TlsRouter {
    client_config: Arc<ClientConfig>,
    fixed_addr: Option<String>,
    sni_hostname: Option<String>,
    inactivity_timeout: Option<Duration>,
}

impl TlsRouter {
    pub fn fixed(
        addr: impl Into<String>,
        sni_hostname: impl Into<String>,
        client_config: Arc<ClientConfig>,
    ) -> Self {
        Self {
            client_config,
            fixed_addr: Some(addr.into()),
            sni_hostname: Some(sni_hostname.into()),
            inactivity_timeout: None,
        }
    }

    pub fn from_routing_key(client_config: Arc<ClientConfig>) -> Self {
        Self {
            client_config,
            fixed_addr: None,
            sni_hostname: None,
            inactivity_timeout: None,
        }
    }

    pub fn with_inactivity_timeout(mut self, d: Duration) -> Self {
        self.inactivity_timeout = Some(d);
        self
    }

    async fn connect(&self, addr: &str, sni_hostname: &str) -> Result<TlsStreamDestination, RailscaleError> {
        let tcp = TcpStream::connect(addr).await.map_err(|e| {
            RailscaleError::from(ErrorKind::RoutingFailed(format!("TCP connect to {addr}: {e}")))
        })?;

        let connector = TlsConnector::from(self.client_config.clone());
        let server_name = rustls::pki_types::ServerName::try_from(sni_hostname.to_string())
            .map_err(|e| RailscaleError::from(ErrorKind::RoutingFailed(format!("invalid SNI hostname '{sni_hostname}': {e}"))))?;

        let tls_stream = connector.connect(server_name, tcp).await.map_err(|e| {
            RailscaleError::from(ErrorKind::RoutingFailed(format!("TLS handshake with {addr}: {e}")))
        })?;

        let dest = TlsStreamDestination::new(tls_stream);
        match self.inactivity_timeout {
            Some(d) => Ok(dest.with_timeout(d)),
            None => Ok(dest),
        }
    }
}

#[async_trait::async_trait]
impl DestinationRouter for TlsRouter {
    type Destination = TlsStreamDestination;

    async fn route(&self, routing_key: &[u8]) -> Result<Self::Destination, RailscaleError> {
        match (&self.fixed_addr, &self.sni_hostname) {
            (Some(addr), Some(sni)) => self.connect(addr, sni).await,
            _ => {
                let key = std::str::from_utf8(routing_key).map_err(|e| {
                    RailscaleError::from(ErrorKind::RoutingFailed(format!("routing key not UTF-8: {e}")))
                })?;
                let (host, port) = match key.rfind(':') {
                    Some(pos) => (&key[..pos], &key[pos + 1..]),
                    None => (key, "443"),
                };
                let addr = format!("{host}:{port}");
                self.connect(&addr, host).await
            }
        }
    }
}
