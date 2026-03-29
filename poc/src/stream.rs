use std::pin::Pin;
use std::task::{Context, Poll};
use bytes::{Bytes, BytesMut};
use memchr::memmem::Finder;
use pin_project_lite::pin_project;
use tokio::io::AsyncRead;
use tokio_stream::Stream;
use tokio_util::codec::FramedRead;
use crate::codec::{HttpFrame, HttpStreamingCodec};

#[derive(Debug)]
pub enum MappedFrame {
    Header(Bytes),
    Body(BytesMut),
}

pin_project! {
    pub struct MappedHttpStream<T: AsyncRead> {
        #[pin]
        inner: FramedRead<T, HttpStreamingCodec>,
        headers_done: bool,
    }
}

impl<T: AsyncRead> MappedHttpStream<T> {
    pub fn from(stream: T, replace: Vec<(Finder<'static>, Bytes)>) -> Self {
        Self {
            inner: FramedRead::new(stream, HttpStreamingCodec::new(replace)),
            headers_done: false,
        }
    }
}

impl<T: AsyncRead + Unpin> Stream for MappedHttpStream<T> {
    type Item = Result<MappedFrame, tokio::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if *this.headers_done {
            let buf = this.inner.read_buffer_mut();
            if !buf.is_empty() {
                let chunk = buf.split();
                return Poll::Ready(Some(Ok(MappedFrame::Body(chunk))));
            }
            return Poll::Ready(None);
        }

        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(HttpFrame::HeadLine(line)))) => {
                Poll::Ready(Some(Ok(MappedFrame::Header(line))))
            }
            Poll::Ready(Some(Ok(HttpFrame::EndOfHeaders))) => {
                *this.headers_done = true;
                let buf = this.inner.read_buffer_mut();
                if !buf.is_empty() {
                    let chunk = buf.split();
                    return Poll::Ready(Some(Ok(MappedFrame::Body(chunk))));
                }
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
