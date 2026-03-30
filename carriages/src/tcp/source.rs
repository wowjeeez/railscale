use std::net::SocketAddr;
use tokio::net::{TcpListener, UnixListener, tcp::{OwnedReadHalf, OwnedWriteHalf}};
use tokio::net::unix::{OwnedReadHalf as UnixReadHalf, OwnedWriteHalf as UnixWriteHalf};
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

pub struct SockSource {
    listener: UnixListener,
}

impl SockSource {
    pub fn bind(path: &str) -> Result<Self, std::io::Error> {
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path)?;
        info!(path = %path, "sock source bound");
        Ok(Self { listener })
    }
}

impl StreamSource for SockSource {
    type ReadHalf = UnixReadHalf;
    type WriteHalf = UnixWriteHalf;
    type Error = std::io::Error;

    async fn accept(&self) -> Result<(Self::ReadHalf, Self::WriteHalf), Self::Error> {
        let (stream, _) = self.listener.accept().await?;
        Ok(stream.into_split())
    }
}
