use std::io;
use bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_stream::Stream;
use train_track::{ParsedData, FrameParser};
use crate::frame::{TlsEncryptedFrame, TlsRecordType};

const TLS_HEADER_LEN: usize = 5;
const TLS_MAX_PAYLOAD: usize = 16384;

pub struct TlsParser;

impl TlsParser {
    pub fn new() -> Self {
        Self
    }
}

impl<S: AsyncRead + Send + Unpin + 'static> FrameParser<S> for TlsParser {
    type Frame = TlsEncryptedFrame;
    type Error = io::Error;

    fn parse(&mut self, stream: S) -> impl Stream<Item = Result<ParsedData<Self::Frame>, Self::Error>> + Send {
        async_stream::stream! {
            let mut reader = stream;
            loop {
                let mut header_buf = BytesMut::with_capacity(TLS_HEADER_LEN);
                while header_buf.len() < TLS_HEADER_LEN {
                    let n = reader.read_buf(&mut header_buf).await?;
                    if n == 0 {
                        if header_buf.is_empty() {
                            return;
                        }
                        yield Err(io::Error::new(io::ErrorKind::UnexpectedEof, "incomplete TLS header"));
                        return;
                    }
                }

                let record_type_byte = header_buf[0];
                let record_type = match TlsRecordType::from_u8(record_type_byte) {
                    Some(rt) => rt,
                    None => {
                        yield Err(io::Error::new(io::ErrorKind::InvalidData, format!("unknown TLS record type: {record_type_byte}")));
                        return;
                    }
                };

                let payload_len = ((header_buf[3] as usize) << 8) | (header_buf[4] as usize);
                if payload_len > TLS_MAX_PAYLOAD {
                    yield Err(io::Error::new(io::ErrorKind::InvalidData, format!("TLS payload length {payload_len} exceeds maximum {TLS_MAX_PAYLOAD}")));
                    return;
                }

                let mut payload_buf = BytesMut::with_capacity(payload_len);
                while payload_buf.len() < payload_len {
                    let n = reader.read_buf(&mut payload_buf).await?;
                    if n == 0 {
                        yield Err(io::Error::new(io::ErrorKind::UnexpectedEof, "incomplete TLS payload"));
                        return;
                    }
                }

                let payload = payload_buf.freeze();
                yield Ok(ParsedData::Parsed(TlsEncryptedFrame::new(payload, record_type)));
            }
        }
    }
}
