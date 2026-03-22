use bytes::{Bytes, BytesMut};
use tokio::io::AsyncRead;
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, FramedRead};

use crate::carriage::gate::{Stamp, TicketGate};
use crate::carriage::manifest::Manifest;

#[derive(Debug)]
pub enum BufferedField {
    /// Known protocol-level fields with static keys (e.g., "tls_version", "content_type")
    KeyValue(&'static str, &'static str),
    /// Dynamic key-value pairs from protocol headers (e.g., HTTP headers, TLS extensions)
    Header(String, String),
    /// Standalone protocol values (e.g., HTTP method, path)
    Attribute(String),
    /// Raw buffered bytes when structured parsing isn't applicable
    Bytes(Bytes),
}

#[derive(Debug)]
pub enum TicketField {
    Buffered(BufferedField),
    Passthrough(Bytes),
    Boundary,
}

pub trait PassengerDecoder: Decoder<Item = TicketField, Error = std::io::Error> {
    fn with_predicate(buffer_predicate: fn(&[u8]) -> bool) -> Self;
}

pub struct TicketPipeline<R: AsyncRead + Unpin, D: PassengerDecoder> {
    framed: FramedRead<R, D>,
    buffered_fields: Vec<BufferedField>,
    stamps: Stamp,
}

impl<R: AsyncRead + Unpin, D: PassengerDecoder> TicketPipeline<R, D> {
    pub fn new(reader: R, buffer_predicate: fn(&[u8]) -> bool) -> Self {
        Self {
            framed: FramedRead::new(reader, D::with_predicate(buffer_predicate)),
            buffered_fields: Vec::new(),
            stamps: Stamp::new(),
        }
    }

    pub async fn process_metadata<M, G>(&mut self, gate: &G) -> Result<(), G::Error>
    where
        M: Manifest,
        G: TicketGate<M>,
    {
        while let Some(result) = self.framed.next().await {
            match result {
                Ok(TicketField::Buffered(field)) => {
                    self.buffered_fields.push(field);
                }
                Ok(TicketField::Passthrough(_)) => {
                    // streamed through, nothing to hold
                }
                Ok(TicketField::Boundary) => {
                    break;
                }
                Err(_e) => {
                    return Err(todo!("map io error to gate error"));
                }
            }
        }
        Ok(())
    }

    pub fn buffered_fields(&self) -> &[BufferedField] {
        &self.buffered_fields
    }

    pub fn stamps(&self) -> &Stamp {
        &self.stamps
    }

    pub fn into_body_stream(self) -> R {
        self.framed.into_inner()
    }
}
