use std::time::Duration;
use tokio::net::{TcpStream, UnixStream};
use train_track::{DestinationRouter, ErrorKind, RailscaleError};
use crate::TcpDestination;
use crate::tcp::destination::TcpOverSockDestination;

pub struct TcpRouter {
    fixed_addr: Option<String>,
    inactivity_timeout: Option<Duration>,
}

impl TcpRouter {
    pub fn fixed(addr: impl Into<String>) -> Self {
        Self { fixed_addr: Some(addr.into()), inactivity_timeout: None }
    }

    pub fn from_routing_key() -> Self {
        Self { fixed_addr: None, inactivity_timeout: None }
    }

    pub fn with_inactivity_timeout(mut self, d: Duration) -> Self {
        self.inactivity_timeout = Some(d);
        self
    }
}

fn extract_host(request_line: &[u8]) -> Option<String> {
    let line = std::str::from_utf8(request_line).ok()?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        let uri = parts[1];
        if uri.contains("://") {
            uri.split("://").nth(1).map(|h| {
                h.split('/').next().unwrap_or(h).to_string()
            })
        } else {
            Some(uri.trim_start_matches('/').to_string())
        }
    } else {
        None
    }
}

#[async_trait::async_trait]
impl DestinationRouter for TcpRouter {
    type Destination = TcpDestination;

    async fn route(&self, routing_key: &[u8]) -> Result<Self::Destination, RailscaleError> {
        let host = match &self.fixed_addr {
            Some(addr) => addr.clone(),
            None => extract_host(routing_key).ok_or_else(|| {
                RailscaleError::from(ErrorKind::RoutingFailed("no host in routing key".into()))
            })?,
        };
        let stream = TcpStream::connect(&host).await?;
        let dest = TcpDestination::new(stream);
        match self.inactivity_timeout {
            Some(d) => Ok(dest.with_timeout(d)),
            None => Ok(dest),
        }
    }
}

pub struct TcpOverSockRouter {
    path: String,
    inactivity_timeout: Option<Duration>,
}

impl TcpOverSockRouter {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into(), inactivity_timeout: None }
    }

    pub fn with_inactivity_timeout(mut self, d: Duration) -> Self {
        self.inactivity_timeout = Some(d);
        self
    }
}

#[async_trait::async_trait]
impl DestinationRouter for TcpOverSockRouter {
    type Destination = TcpOverSockDestination;

    async fn route(&self, _routing_key: &[u8]) -> Result<Self::Destination, RailscaleError> {
        let stream = UnixStream::connect(&self.path).await?;
        let dest = TcpOverSockDestination::new(stream);
        match self.inactivity_timeout {
            Some(d) => Ok(dest.with_timeout(d)),
            None => Ok(dest),
        }
    }
}
