use tokio::net::TcpStream;
use train_track::{DestinationRouter, RailscaleError};
use crate::TcpDestination;

pub struct TcpRouter {
    fixed_addr: Option<String>,
}

impl TcpRouter {
    pub fn fixed(addr: impl Into<String>) -> Self {
        Self { fixed_addr: Some(addr.into()) }
    }

    pub fn from_routing_key() -> Self {
        Self { fixed_addr: None }
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
                RailscaleError::RoutingFailed("no host in routing key".into())
            })?,
        };
        let stream = TcpStream::connect(&host).await?;
        Ok(TcpDestination::new(stream))
    }
}
