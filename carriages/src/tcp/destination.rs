use bytes::Bytes;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use train_track::{Frame, StreamDestination};
use crate::HttpFrame;

pub struct TcpDestination {
    upstream: Option<TcpStream>,
    fixed_addr: Option<String>,
    headers_sent: bool,
}

impl TcpDestination {
    pub fn new() -> Self {
        Self { upstream: None, fixed_addr: None, headers_sent: false }
    }

    pub fn with_fixed_upstream(addr: impl Into<String>) -> Self {
        Self { upstream: None, fixed_addr: Some(addr.into()), headers_sent: false }
    }
}


#[async_trait::async_trait]
impl StreamDestination for TcpDestination {
    type Frame = HttpFrame;
    type Error = std::io::Error;

    async fn provide(&mut self, routing_frame: &Self::Frame) -> Result<(), Self::Error> {
        let host = match &self.fixed_addr {
            Some(addr) => addr.clone(),
            None => {
                let line = routing_frame.as_bytes();
                extract_host(line).ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "no host in routing frame")
                })?
            }
        };
        let stream = TcpStream::connect(&host).await?;
        self.upstream = Some(stream);
        Ok(())
    }

    async fn write(&mut self, frame: Self::Frame) -> Result<(), Self::Error> {
        let upstream = self.upstream.as_mut().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotConnected, "not routed")
        })?;
        upstream.write_all(frame.as_bytes()).await?;
        upstream.write_all(b"\r\n").await
    }

    async fn write_raw(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        let upstream = self.upstream.as_mut().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotConnected, "not routed")
        })?;
        if !self.headers_sent {
            upstream.write_all(b"\r\n").await?;
            self.headers_sent = true;
        }
        upstream.write_all(&bytes).await
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(&mut self, client: &mut W) -> Result<u64, Self::Error> {
        let upstream = self.upstream.as_mut().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotConnected, "not routed")
        })?;
        upstream.shutdown().await?;
        tokio::io::copy(upstream, client).await
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
