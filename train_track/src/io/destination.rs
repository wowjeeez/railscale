use bytes::Bytes;
use tokio::io::AsyncRead;
use crate::RailscaleError;

#[async_trait::async_trait]
pub trait StreamDestination: Send {
    type Error: Into<RailscaleError>;
    type ResponseReader: AsyncRead + Send + Unpin;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error>;
    fn response_reader(&mut self) -> &mut Self::ResponseReader;
}
