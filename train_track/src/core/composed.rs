use tokio::io::AsyncRead;
use tokio_stream::Stream;
use crate::RailscaleError;
use crate::atom::frame::ParsedData;
use crate::atom::parser::FrameParser;

#[async_trait::async_trait]
pub trait StreamTransform: Send {
    type Input: AsyncRead + Send + Unpin;
    type Output: AsyncRead + Send + Unpin;
    type Error: Into<RailscaleError> + Send;

    async fn transform(self, stream: Self::Input) -> Result<Self::Output, Self::Error>;
}

pub struct Composed<T: StreamTransform, P> {
    transform: Option<T>,
    parser_factory: fn() -> P,
}

impl<T: StreamTransform, P> Composed<T, P> {
    pub fn new(transform: T, parser_factory: fn() -> P) -> Self {
        Self { transform: Some(transform), parser_factory }
    }
}

impl<T, P> FrameParser<T::Input> for Composed<T, P>
where
    T: StreamTransform + 'static,
    P: FrameParser<T::Output> + Send + 'static,
    P::Frame: 'static,
    P::Error: From<T::Error> + Send,
{
    type Frame = P::Frame;
    type Error = P::Error;

    fn parse(&mut self, stream: T::Input) -> impl Stream<Item = Result<ParsedData<Self::Frame>, Self::Error>> + Send {
        let transform = self.transform.take().expect("Composed::parse called twice");
        let parser_factory = self.parser_factory;

        async_stream::stream! {
            let transformed = match transform.transform(stream).await {
                Ok(s) => s,
                Err(e) => {
                    yield Err(P::Error::from(e));
                    return;
                }
            };

            let mut parser = parser_factory();
            let inner_stream = parser.parse(transformed);
            tokio::pin!(inner_stream);

            while let Some(item) = tokio_stream::StreamExt::next(&mut inner_stream).await {
                yield item;
            }
        }
    }
}
