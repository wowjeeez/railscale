use std::marker::PhantomData;
use bytes::Bytes;
use tokio::io::AsyncWrite;
use crate::{Frame, RailscaleError, StreamDestination};

pub trait MultiStreamCollection<T: Frame> : Send + Sync + StreamDestination<Frame=T> {
    type Error: Into<RailscaleError>;
}

pub struct MultiDestination<T: Frame, C: MultiStreamCollection<T>> {
    _p: PhantomData<T>,
    coll: C
}

impl <T: Frame, C: MultiStreamCollection<T>> MultiDestination<T, C> {
    pub fn new(coll: C) -> Self {
        Self { _p: PhantomData, coll }
    }
}

#[async_trait::async_trait]
impl<T: Frame + Sync, C: MultiStreamCollection<T>> StreamDestination for MultiDestination<T, C> {
    type Frame = T;
    type Error = RailscaleError;

    async fn provide(&mut self, routing_frame: &Self::Frame) -> Result<(), Self::Error> {
        self.coll.provide(routing_frame).await.map_err(|e| e.into())
    }

    async fn write(&mut self, frame: Self::Frame) -> Result<(), Self::Error> {
        self.coll.write(frame).await.map_err(|e| e.into())
    }

    async fn write_raw(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.coll.write_raw(bytes).await.map_err(|e| e.into())
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(&mut self, client: &mut W) -> Result<u64, Self::Error> {
        self.coll.relay_response::<W>(client).await.map_err(|e| e.into())
    }
}