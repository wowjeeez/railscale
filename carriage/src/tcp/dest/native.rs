use std::time::Duration;
use bytes::Bytes;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use train_track::StreamDestination;

pub struct TcpDestination {
    read_half: OwnedReadHalf,
    write_half: OwnedWriteHalf,
    inactivity_timeout: Option<Duration>,
}

impl TcpDestination {
    pub fn new(stream: TcpStream) -> Self {
        let (read_half, write_half) = stream.into_split();
        Self { read_half, write_half, inactivity_timeout: None }
    }

    pub fn with_timeout(mut self, d: Duration) -> Self {
        self.inactivity_timeout = Some(d);
        self
    }
}

#[async_trait::async_trait]
impl StreamDestination for TcpDestination {
    type Error = std::io::Error;
    type ResponseReader = OwnedReadHalf;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.write_half.write_all(&bytes).await
    }

    fn response_reader(&mut self) -> &mut OwnedReadHalf {
        &mut self.read_half
    }
}
