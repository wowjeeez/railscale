use std::pin::Pin;
use std::task::{Context, Poll};
use bytes::Bytes;
use memchr::memmem::Finder;
use pin_project_lite::pin_project;
use tokio::io::AsyncRead;
use tokio_stream::Stream;
use tokio_util::codec::FramedRead;
use train_track::{FrameParser, ParsedData};
use crate::codec::HttpStreamingCodec;
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

impl<T: AsyncRead + Unpin> Stream for HttpFrameStream<T> {
    type Item = Result<ParsedData<HttpFrame>, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if *this.headers_done {
            let buf = this.inner.get_mut().read_buffer_mut();
            if !buf.is_empty() {
                let chunk = buf.split();
                return Poll::Ready(Some(Ok(ParsedData::Passthrough(chunk.freeze()))));
            }
            return Poll::Ready(None);
        }

        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                Poll::Ready(Some(Ok(ParsedData::Parsed(frame))))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => {
                *this.headers_done = true;
                let buf = this.inner.get_mut().read_buffer_mut();
                if !buf.is_empty() {
                    let chunk = buf.split();
                    return Poll::Ready(Some(Ok(ParsedData::Passthrough(chunk.freeze()))));
                }
                Poll::Ready(None)
            }
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
