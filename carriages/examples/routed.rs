use std::path::PathBuf;
use std::sync::Arc;
use bytes::Bytes;
use memchr::memmem::Finder;
use carriages::{HttpParser, HttpPipeline, TcpSource};
use train_track::{DestinationRouter, FileDestination, Pipeline, RailscaleError, Service};

struct FileRouter;

#[async_trait::async_trait]
impl DestinationRouter for FileRouter {
    type Destination = FileDestination;

    async fn route(&self, _routing_key: &[u8]) -> Result<Self::Destination, RailscaleError> {
        FileDestination::new(PathBuf::from("railscale.pcap")).map_err(Into::into)
    }
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
        router: Arc::new(FileRouter),
        #[cfg(feature = "metrics-full")]
        sampler: None,
    };

    pipeline.run().await.unwrap();
}
