use bytes::Bytes;
use tokio::io::AsyncRead;
use crate::RailscaleError;
use crate::io::destination::StreamDestination;

#[async_trait::async_trait]
pub trait Departure: Send {
    type Error: Into<RailscaleError>;
    type ResponseReader: AsyncRead + Send + Unpin;

    async fn depart(&mut self, bytes: Bytes) -> Result<(), Self::Error>;
    fn response_reader(&mut self) -> &mut Self::ResponseReader;
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
    type ResponseReader = D::ResponseReader;

    async fn depart(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.0.write(bytes).await
    }

    fn response_reader(&mut self) -> &mut D::ResponseReader {
        self.0.response_reader()
    }
}

pub trait Transload: Departure {}

pub struct ChannelTransload {
    tx: tokio::sync::mpsc::Sender<Bytes>,
    empty: tokio::io::Empty,
}

impl ChannelTransload {
    pub fn new(tx: tokio::sync::mpsc::Sender<Bytes>) -> Self {
        Self { tx, empty: tokio::io::empty() }
    }
}

#[async_trait::async_trait]
impl Departure for ChannelTransload {
    type Error = RailscaleError;
    type ResponseReader = tokio::io::Empty;

    async fn depart(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.tx.send(bytes).await.map_err(|_| {
            RailscaleError::from(crate::ErrorKind::Io(
                std::io::Error::new(std::io::ErrorKind::BrokenPipe, "channel closed")
            ))
        })
    }

    fn response_reader(&mut self) -> &mut tokio::io::Empty {
        &mut self.empty
    }
}

impl Transload for ChannelTransload {}
