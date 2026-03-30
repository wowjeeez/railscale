use std::time::Duration;
use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpStream, UnixStream};
use train_track::StreamDestination;

async fn copy_with_inactivity_timeout<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    reader: &mut R,
    writer: &mut W,
    timeout: Duration,
) -> Result<u64, std::io::Error> {
    let mut buf = [0u8; 8192];
    let mut total: u64 = 0;
    loop {
        tokio::select! {
            result = reader.read(&mut buf) => {
                let n = result?;
                if n == 0 {
                    break;
                }
                writer.write_all(&buf[..n]).await?;
                total += n as u64;
            }
            _ = tokio::time::sleep(timeout) => {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "inactivity timeout"));
            }
        }
    }
    Ok(total)
}

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

pub struct TcpOverSockDestination {
    upstream: UnixStream,
    inactivity_timeout: Option<Duration>,
}

impl TcpOverSockDestination {
    pub fn new(stream: UnixStream) -> Self {
        Self { upstream: stream, inactivity_timeout: None }
    }

    pub fn with_timeout(mut self, d: Duration) -> Self {
        self.inactivity_timeout = Some(d);
        self
    }
}

#[async_trait::async_trait]
impl StreamDestination for TcpOverSockDestination {
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
