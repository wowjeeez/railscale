use std::time::Duration;
use bytes::Bytes;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use train_track::StreamDestination;
use crate::tcp::dest::unixsocket::copy_with_inactivity_timeout;

pub struct TcpDestination {
    upstream: TcpStream,
    inactivity_timeout: Option<Duration>,
}

impl TcpDestination {
    pub fn new(stream: TcpStream) -> Self {
        Self { upstream: stream, inactivity_timeout: None }
    }

    pub fn with_timeout(mut self, d: Duration) -> Self {
        self.inactivity_timeout = Some(d);
        self
    }
}

#[async_trait::async_trait]
impl StreamDestination for TcpDestination {
    type Error = std::io::Error;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.upstream.write_all(&bytes).await
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(&mut self, client: &mut W) -> Result<u64, Self::Error> {
        match self.inactivity_timeout {
            Some(timeout) => copy_with_inactivity_timeout(&mut self.upstream, client, timeout).await,
            None => tokio::io::copy(&mut self.upstream, client).await,
        }
    }
}