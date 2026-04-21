use std::time::Duration;
use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use train_track::StreamDestination;


#[allow(dead_code)]
pub async fn copy_with_inactivity_timeout<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
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


pub struct TcpOverSockDestination {
    read_half: OwnedReadHalf,
    write_half: OwnedWriteHalf,
    inactivity_timeout: Option<Duration>,
}

impl TcpOverSockDestination {
    pub fn new(stream: UnixStream) -> Self {
        let (read_half, write_half) = stream.into_split();
        Self { read_half, write_half, inactivity_timeout: None }
    }

    pub fn with_timeout(mut self, d: Duration) -> Self {
        self.inactivity_timeout = Some(d);
        self
    }
}

#[async_trait::async_trait]
impl StreamDestination for TcpOverSockDestination {
    type Error = std::io::Error;
    type ResponseReader = OwnedReadHalf;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.write_half.write_all(&bytes).await
    }

    fn response_reader(&mut self) -> &mut OwnedReadHalf {
        &mut self.read_half
    }
}
