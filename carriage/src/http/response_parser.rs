use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncRead;
use tokio_stream::Stream;
use tokio_util::codec::FramedRead;
use train_track::{FrameParser, ParsedData};
use crate::http::response_codec::{ResponseCodec, ResponseCodecItem};
use crate::http_v1::HttpFrame;

pub struct ResponseParser;

impl ResponseParser {
    pub fn new() -> Self {
        Self
    }
}

struct ResponseFrameStream<T: AsyncRead + Unpin> {
    inner: FramedRead<T, ResponseCodec>,
    done: bool,
}

impl<T: AsyncRead + Unpin> Unpin for ResponseFrameStream<T> {}

impl<T: AsyncRead + Unpin> Stream for ResponseFrameStream<T> {
    type Item = Result<ParsedData<HttpFrame>, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }
        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(ResponseCodecItem::Frame(frame)))) => {
                if this.inner.decoder().is_response_complete() {
                    this.done = true;
                }
                Poll::Ready(Some(Ok(ParsedData::Parsed(frame))))
            }
            Poll::Ready(Some(Ok(ResponseCodecItem::Body(bytes)))) => {
                if this.inner.decoder().is_response_complete() {
                    this.done = true;
                }
                Poll::Ready(Some(Ok(ParsedData::Passthrough(bytes))))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S: AsyncRead + Send + Unpin> FrameParser<S> for ResponseParser {
    type Frame = HttpFrame;
    type Error = std::io::Error;

    fn parse(&mut self, stream: S) -> impl Stream<Item = Result<ParsedData<Self::Frame>, Self::Error>> + Send {
        ResponseFrameStream {
            inner: FramedRead::new(stream, ResponseCodec::new()),
            done: false,
        }
    }
}
