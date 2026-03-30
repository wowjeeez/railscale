use std::pin::Pin;
use std::task::{Context, Poll};
use bytes::Bytes;
use memchr::memmem::Finder;
use pin_project_lite::pin_project;
use tokio::io::AsyncRead;
use tokio_stream::Stream;
use tokio_util::codec::FramedRead;
use train_track::{FrameParser, ParsedData};
use crate::http::codec::HttpStreamingCodec;
use crate::HttpFrame;

pub struct HttpParser {
    matchers: Vec<(Finder<'static>, Bytes)>,
}

impl HttpParser {
    pub fn new(matchers: Vec<(Finder<'static>, Bytes)>) -> Self {
        Self { matchers }
    }
}

pin_project! {
    struct HttpFrameStream<T: AsyncRead> {
        #[pin]
        inner: FramedRead<T, HttpStreamingCodec>,
        headers_done: bool,
    }
}

fn drain_buffer<T: AsyncRead + Unpin>(
    inner: &mut FramedRead<T, HttpStreamingCodec>,
) -> Option<ParsedData<HttpFrame>> {
    let buf = inner.read_buffer_mut();
    if !buf.is_empty() {
        let chunk = buf.split();
        Some(ParsedData::Passthrough(chunk.freeze()))
    } else {
        None
    }
}

impl<T: AsyncRead + Unpin> Stream for HttpFrameStream<T> {
    type Item = Result<ParsedData<HttpFrame>, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        let inner = this.inner.as_mut().get_mut();

        if *this.headers_done {
            return match drain_buffer(inner) {
                Some(data) => Poll::Ready(Some(Ok(data))),
                None => Poll::Ready(None),
            };
        }

        match Pin::new(&mut *inner).poll_next(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if frame.is_end_of_headers() {
                    *this.headers_done = true;
                    let separator = Bytes::from_static(b"\r\n");
                    let buf = inner.read_buffer_mut();
                    if buf.is_empty() {
                        return Poll::Ready(Some(Ok(ParsedData::Passthrough(separator))));
                    }
                    let body = buf.split().freeze();
                    let mut combined = bytes::BytesMut::with_capacity(2 + body.len());
                    combined.extend_from_slice(&separator);
                    combined.extend_from_slice(&body);
                    return Poll::Ready(Some(Ok(ParsedData::Passthrough(combined.freeze()))));
                }
                Poll::Ready(Some(Ok(ParsedData::Parsed(frame))))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S: AsyncRead + Send + Unpin> FrameParser<S> for HttpParser {
    type Frame = HttpFrame;
    type Error = std::io::Error;

    fn parse(&mut self, stream: S) -> impl Stream<Item = Result<ParsedData<Self::Frame>, Self::Error>> + Send {
        let codec = HttpStreamingCodec::new(self.matchers.clone());
        HttpFrameStream {
            inner: FramedRead::new(stream, codec),
            headers_done: false,
        }
    }
}
