use bytes::Bytes;
use tokio::io::AsyncWrite;
use crate::{RailscaleError, StreamDestination};

pub trait MultiStreamCollection: Send + Sync + StreamDestination {
    type CollError: Into<RailscaleError>;
}

pub struct MultiDestination<C: MultiStreamCollection> {
    coll: C,
}

impl<C: MultiStreamCollection> MultiDestination<C> {
    pub fn new(coll: C) -> Self {
        Self { coll }
    }
}

#[async_trait::async_trait]
impl<C: MultiStreamCollection> StreamDestination for MultiDestination<C> {
    type Error = RailscaleError;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.coll.write(bytes).await.map_err(|e| e.into())
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(&mut self, client: &mut W) -> Result<u64, Self::Error> {
        self.coll.relay_response::<W>(client).await.map_err(|e| e.into())
    }
}
