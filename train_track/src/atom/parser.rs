use tokio::io::AsyncRead;
use tokio_stream::Stream;
use crate::atom::frame::{Frame, ParsedData};
use crate::RailscaleError;

pub trait FrameParser<S: AsyncRead + Send + Unpin>: Send {
    type Frame: Frame;
    type Error: Into<RailscaleError>;
    fn parse(&mut self, stream: S) -> impl Stream<Item = Result<ParsedData<Self::Frame>, Self::Error>> + Send;
}
