use tokio::net::UnixListener;
use tracing::info;
use train_track::StreamSource;
use tokio::net::unix::{OwnedReadHalf as UnixReadHalf, OwnedWriteHalf as UnixWriteHalf};

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
