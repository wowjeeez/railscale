use bytes::Bytes;
use tokio::io::AsyncWrite;
use crate::RailscaleError;

#[async_trait::async_trait]
pub trait StreamDestination: Send {
    type Error: Into<RailscaleError>;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error>;
    async fn relay_response<W: AsyncWrite + Send + Unpin>(&mut self, client: &mut W) -> Result<u64, Self::Error>;
}
