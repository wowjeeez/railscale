use bytes::Bytes;
use tokio::io::AsyncWrite;
use crate::atom::frame::Frame;
use crate::RailscaleError;


#[async_trait::async_trait]
pub trait StreamDestination: Send {
    type Frame: Frame;
    type Error: Into<RailscaleError>;

    async fn provide(&mut self, routing_frame: &Self::Frame) -> Result<(), Self::Error>;
    async fn write(&mut self, frame: Self::Frame) -> Result<(), Self::Error>;
    async fn write_raw(&mut self, bytes: Bytes) -> Result<(), Self::Error>;
    async fn relay_response<W: AsyncWrite + Send + Unpin>(&mut self, client: &mut W) -> Result<u64, Self::Error>;
}
