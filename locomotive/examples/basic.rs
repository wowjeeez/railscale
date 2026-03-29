use bytes::Bytes;
use memchr::memmem::Finder;
use locomotive::{HttpParser, HttpPipeline, TcpDestination, TcpSource};
use std::sync::Arc;
use train_track::{Pipeline, Service};

#[cfg(feature = "metrics-minimal")]
use std::time::Duration;

#[cfg(feature = "metrics-minimal")]
fn init_metrics() -> opentelemetry_sdk::metrics::SdkMeterProvider {
    use opentelemetry::global;
    use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
    use opentelemetry_stdout::MetricExporter;

    let exporter = MetricExporter::default();
    let reader = PeriodicReader::builder(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_interval(Duration::from_secs(10))
        .build();
    let provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .build();
    global::set_meter_provider(provider.clone());
    provider
}

#[tokio::main]
#[hotpath::main]
async fn main() {
    hotpath::tokio_runtime!();
    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter("warn")
        .init();

    #[cfg(feature = "metrics-minimal")]
    let _metrics = init_metrics();

    #[cfg(feature = "metrics-full")]
    let sampler_handle = {
        use train_track::sampler;
        let metrics_path = std::env::var("RAILSCALE_METRICS_FILE")
            .unwrap_or_else(|_| "/tmp/railscale-metrics.jsonl".to_string());
        Arc::new(sampler::start_sampler(&metrics_path, Duration::from_millis(100)))
    };

    let source = TcpSource::bind("127.0.0.1:8080").await.unwrap();

    let pipeline = Pipeline {
        source,
        parser_factory: || HttpParser::new(vec![]),
        pipeline: Arc::new(HttpPipeline::new(vec![
            (Finder::new(b"User-Agent"), Bytes::from_static(b"railscale/1.0")),
        ])),
        destination_factory: || TcpDestination::with_fixed_upstream("127.0.0.1:9090"),
        #[cfg(feature = "metrics-full")]
        sampler: Some(sampler_handle),
    };

    pipeline.run().await.unwrap();
}
