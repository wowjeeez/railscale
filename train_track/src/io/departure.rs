use bytes::Bytes;
use tokio::io::AsyncWrite;
use crate::RailscaleError;
use crate::io::destination::StreamDestination;

#[async_trait::async_trait]
pub trait Departure: Send {
    type Error: Into<RailscaleError>;
    async fn depart(&mut self, bytes: Bytes) -> Result<(), Self::Error>;
    async fn relay_response<W: AsyncWrite + Send + Unpin>(
        &mut self,
        client: &mut W,
    ) -> Result<u64, Self::Error>;
}

pub struct StreamDeparture<D>(D);

impl<D> StreamDeparture<D> {
    pub fn new(dest: D) -> Self {
        Self(dest)
    }
}

#[async_trait::async_trait]
impl<D: StreamDestination> Departure for StreamDeparture<D> {
    type Error = D::Error;

    async fn depart(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.0.write(bytes).await
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(
        &mut self,
        client: &mut W,
    ) -> Result<u64, Self::Error> {
        self.0.relay_response(client).await
    }
}

pub trait Transload: Departure {}

pub struct ChannelTransload {
    tx: tokio::sync::mpsc::Sender<Bytes>,
}

impl ChannelTransload {
    pub fn new(tx: tokio::sync::mpsc::Sender<Bytes>) -> Self {
        Self { tx }
    }
}

#[async_trait::async_trait]
impl Departure for ChannelTransload {
    type Error = RailscaleError;

    async fn depart(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.tx.send(bytes).await.map_err(|_| {
            RailscaleError::from(crate::ErrorKind::Io(
                std::io::Error::new(std::io::ErrorKind::BrokenPipe, "channel closed")
            ))
        })
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(
        &mut self,
        _client: &mut W,
    ) -> Result<u64, Self::Error> {
        Ok(0)
    }
}

impl Transload for ChannelTransload {}
