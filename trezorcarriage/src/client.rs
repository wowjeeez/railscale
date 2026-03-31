use std::sync::Arc;
use std::time::Duration;
use bytes::Bytes;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use rustls::pki_types::ServerName;
use train_track::{StreamDestination, DestinationRouter, ErrorKind, RailscaleError};

pub struct TlsClientDestination {
    stream: tokio_rustls::client::TlsStream<TcpStream>,
    inactivity_timeout: Option<Duration>,
}

impl TlsClientDestination {
    pub fn new(stream: tokio_rustls::client::TlsStream<TcpStream>) -> Self {
        Self { stream, inactivity_timeout: None }
    }

    pub fn with_timeout(mut self, d: Duration) -> Self {
        self.inactivity_timeout = Some(d);
        self
    }
}

#[async_trait::async_trait]
impl StreamDestination for TlsClientDestination {
    type Error = std::io::Error;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.stream.write_all(&bytes).await
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(
        &mut self,
        client: &mut W,
    ) -> Result<u64, Self::Error> {
        match self.inactivity_timeout {
            Some(timeout) => {
                let mut buf = [0u8; 8192];
                let mut total: u64 = 0;
                use tokio::io::AsyncReadExt;
                loop {
                    tokio::select! {
                        result = self.stream.read(&mut buf) => {
                            let n = result?;
                            if n == 0 { break; }
                            client.write_all(&buf[..n]).await?;
                            total += n as u64;
                        }
                        _ = tokio::time::sleep(timeout) => {
                            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "inactivity timeout"));
                        }
                    }
                }
                Ok(total)
            }
            None => tokio::io::copy(&mut self.stream, client).await,
        }
    }
}

enum RoutingStrategy {
    Fixed(String),
    FromKey,
}

pub struct TlsClientRouter {
    config: Arc<rustls::ClientConfig>,
    strategy: RoutingStrategy,
    inactivity_timeout: Option<Duration>,
}

impl TlsClientRouter {
    pub fn fixed(addr: String, config: Arc<rustls::ClientConfig>) -> Self {
        Self { config, strategy: RoutingStrategy::Fixed(addr), inactivity_timeout: None }
    }

    pub fn from_routing_key(config: Arc<rustls::ClientConfig>) -> Self {
        Self { config, strategy: RoutingStrategy::FromKey, inactivity_timeout: None }
    }

    pub fn with_inactivity_timeout(mut self, d: Duration) -> Self {
        self.inactivity_timeout = Some(d);
        self
    }

    fn resolve_addr(&self, routing_key: &[u8]) -> Result<String, RailscaleError> {
        match &self.strategy {
            RoutingStrategy::Fixed(addr) => Ok(addr.clone()),
            RoutingStrategy::FromKey => {
                let key = std::str::from_utf8(routing_key).map_err(|_| {
                    RailscaleError::from(ErrorKind::RoutingFailed("invalid UTF-8 in routing key".into()))
                })?;
                let host = key.split('/').next().unwrap_or(key);
                if host.is_empty() {
                    return Err(RailscaleError::from(ErrorKind::RoutingFailed("empty host in routing key".into())));
                }
                if host.contains(':') {
                    Ok(host.to_string())
                } else {
                    Ok(format!("{host}:443"))
                }
            }
        }
    }

    fn resolve_sni(&self, addr: &str) -> Result<ServerName<'static>, RailscaleError> {
        let host = addr.split(':').next().unwrap_or(addr);
        ServerName::try_from(host.to_string()).map_err(|_| {
            RailscaleError::from(ErrorKind::RoutingFailed(format!("invalid SNI hostname: {host}")))
        })
    }
}

#[async_trait::async_trait]
impl DestinationRouter for TlsClientRouter {
    type Destination = TlsClientDestination;

    async fn route(&self, routing_key: &[u8]) -> Result<Self::Destination, RailscaleError> {
        let addr = self.resolve_addr(routing_key)?;
        let sni = self.resolve_sni(&addr)?;
        let tcp = TcpStream::connect(&addr).await?;
        let connector = TlsConnector::from(self.config.clone());
        let tls = connector.connect(sni, tcp).await?;
        let dest = TlsClientDestination::new(tls);
        match self.inactivity_timeout {
            Some(d) => Ok(dest.with_timeout(d)),
            None => Ok(dest),
        }
    }
}
