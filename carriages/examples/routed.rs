use bytes::Bytes;
use memchr::memmem::Finder;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWrite;
use carriages::{HttpFrame, HttpParser, HttpPipeline, TcpDestination, TcpSource};
use train_track::{
    FileDestination,
    MatchStrategy, MemchrDomainMatcher, Pipeline,
    RailscaleError, RouterDestination, Service, StreamDestination,
};

enum Destination {
    Tcp(TcpDestination),
    File(FileDestination<HttpFrame>),
}

#[async_trait::async_trait]
impl StreamDestination for Destination {
    type Frame = HttpFrame;
    type Error = RailscaleError;

    async fn provide(&mut self, routing_frame: &Self::Frame) -> Result<(), Self::Error> {
        match self {
            Self::Tcp(d) => d.provide(routing_frame).await.map_err(Into::into),
            Self::File(d) => d.provide(routing_frame).await.map_err(Into::into),
        }
    }

    async fn write(&mut self, frame: Self::Frame) -> Result<(), Self::Error> {
        match self {
            Self::Tcp(d) => d.write(frame).await.map_err(Into::into),
            Self::File(d) => d.write(frame).await.map_err(Into::into),
        }
    }

    async fn write_raw(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        match self {
            Self::Tcp(d) => d.write_raw(bytes).await.map_err(Into::into),
            Self::File(d) => d.write_raw(bytes).await.map_err(Into::into),
        }
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(&mut self, client: &mut W) -> Result<u64, Self::Error> {
        match self {
            Self::Tcp(d) => d.relay_response(client).await.map_err(Into::into),
            Self::File(d) => d.relay_response(client).await.map_err(Into::into),
        }
    }
}

type Router = RouterDestination<HttpFrame, Destination, MemchrDomainMatcher<Destination>>;

fn build_router() -> Router {
    let matchers = vec![
        MemchrDomainMatcher::new(
            MatchStrategy::contains("ferris.rustcrab"),
            || Destination::Tcp(TcpDestination::with_fixed_upstream("httpbin.org:80")),
        ),
        MemchrDomainMatcher::new(
            MatchStrategy::contains(""),
            || Destination::File(
                FileDestination::new(PathBuf::from("railscale.pcap")).expect("open capture file")
            ),
        ),
    ];
    RouterDestination::new(matchers)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter("info")
        .init();

    let source = TcpSource::bind("127.0.0.1:8080").await.unwrap();

    let pipeline = Pipeline {
        source,
        parser_factory: || HttpParser::new(vec![]),
        pipeline: Arc::new(HttpPipeline::new(vec![
            (Finder::new(b"User-Agent"), Bytes::from_static(b"railscale/1.0")),
        ])),
        destination_factory: build_router,
        #[cfg(feature = "metrics-full")]
        sampler: None,
    };

    pipeline.run().await.unwrap();
}
