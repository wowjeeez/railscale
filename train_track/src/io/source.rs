use tokio::io::{AsyncRead, AsyncWrite};
use crate::RailscaleError;

pub trait StreamSource: Send {
    type ReadHalf: AsyncRead + Send + Unpin;
    type WriteHalf: AsyncWrite + Send + Unpin;
    type Error: Into<RailscaleError>;

    fn accept(&self) -> impl std::future::Future<Output = Result<(Self::ReadHalf, Self::WriteHalf), Self::Error>> + Send;
}
