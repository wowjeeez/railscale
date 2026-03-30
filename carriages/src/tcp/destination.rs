use bytes::Bytes;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use train_track::StreamDestination;

pub struct TcpDestination {
    upstream: TcpStream,
}

impl TcpDestination {
    pub fn new(stream: TcpStream) -> Self {
        Self { upstream: stream }
    }
}

#[async_trait::async_trait]
impl StreamDestination for TcpDestination {
    type Error = std::io::Error;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.upstream.write_all(&bytes).await
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(&mut self, client: &mut W) -> Result<u64, Self::Error> {
        self.upstream.shutdown().await?;
        tokio::io::copy(&mut self.upstream, client).await
    }
}
