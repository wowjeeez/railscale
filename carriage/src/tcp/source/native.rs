use std::net::SocketAddr;
use tokio::net::{TcpListener, tcp::{OwnedReadHalf, OwnedWriteHalf}};
use tracing::info;
use train_track::StreamSource;

pub struct TcpSource {
    listener: TcpListener,
}

impl TcpSource {
    pub async fn bind(addr: &str) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(addr).await?;
        info!(addr = %listener.local_addr().unwrap(), "tcp source bound");
        Ok(Self { listener })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.listener.local_addr().unwrap()
    }
}

impl StreamSource for TcpSource {
    type ReadHalf = OwnedReadHalf;
    type WriteHalf = OwnedWriteHalf;
    type Error = std::io::Error;

    async fn accept(&self) -> Result<(Self::ReadHalf, Self::WriteHalf), Self::Error> {
        let (stream, _) = self.listener.accept().await?;
        Ok(stream.into_split())
    }
}
