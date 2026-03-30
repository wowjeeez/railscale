use std::pin::pin;
use std::sync::Arc;
use std::time::Instant;
use bytes::Bytes;
use tokio_stream::StreamExt;
use tracing::warn;
use crate::io::destination::StreamDestination;
use crate::io::router::DestinationRouter;
use crate::atom::frame::{Frame, ParsedData};
use crate::atom::parser::FrameParser;
use crate::core::pipeline::FramePipeline;
use crate::io::source::StreamSource;
use crate::RailscaleError;

#[cfg(feature = "metrics-full")]
use std::sync::atomic::Ordering;
#[cfg(feature = "metrics-full")]
use crate::sampler::{RequestRecord, SamplerHandle};

pub trait Service: Send + Sync {
    fn run(&self) -> impl std::future::Future<Output = Result<(), RailscaleError>> + Send;
}

#[cfg(feature = "metrics-minimal")]
mod otel {
    use opentelemetry::global;
    use opentelemetry::metrics::{Counter, Histogram, UpDownCounter};

    pub(crate) struct OtelMetrics {
        connections_total: Counter<u64>,
        connections_active: UpDownCounter<i64>,
        connection_errors: Counter<u64>,
        connection_duration: Histogram<f64>,
        request_forward_duration: Histogram<f64>,
        upstream_connect_duration: Histogram<f64>,
        response_relay_duration: Histogram<f64>,
        response_bytes: Histogram<f64>,
        frames_parsed: Counter<u64>,
        bytes_passthrough: Counter<u64>,
    }

    impl OtelMetrics {
        pub fn new() -> Self {
            let meter = global::meter("railscale");
            let latency_buckets = vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0, 10.0];
            let size_buckets = vec![64.0, 256.0, 1024.0, 4096.0, 16384.0, 65536.0, 262144.0, 1048576.0];

            Self {
                connections_total: meter.u64_counter("railscale.connections_total").build(),
                connections_active: meter.i64_up_down_counter("railscale.connections_active").build(),
                connection_errors: meter.u64_counter("railscale.connection_errors").build(),
                connection_duration: meter.f64_histogram("railscale.connection_duration_seconds")
                    .with_boundaries(latency_buckets.clone()).build(),
                request_forward_duration: meter.f64_histogram("railscale.request_forward_duration_seconds")
                    .with_boundaries(latency_buckets.clone()).build(),
                upstream_connect_duration: meter.f64_histogram("railscale.upstream_connect_duration_seconds")
                    .with_boundaries(latency_buckets.clone()).build(),
                response_relay_duration: meter.f64_histogram("railscale.response_relay_duration_seconds")
                    .with_boundaries(latency_buckets).build(),
                response_bytes: meter.f64_histogram("railscale.response_bytes")
                    .with_boundaries(size_buckets).build(),
                frames_parsed: meter.u64_counter("railscale.frames_parsed").build(),
                bytes_passthrough: meter.u64_counter("railscale.bytes_passthrough").build(),
            }
        }

        pub fn conn_start(&self) {
            self.connections_active.add(1, &[]);
            self.connections_total.add(1, &[]);
        }

        pub fn conn_end(&self, duration: f64) {
            self.connections_active.add(-1, &[]);
            self.connection_duration.record(duration, &[]);
        }

        pub fn conn_error(&self) {
            self.connection_errors.add(1, &[]);
        }

        pub fn forward_done(&self, duration: f64, frames: u64, passthrough_bytes: u64) {
            self.request_forward_duration.record(duration, &[]);
            self.frames_parsed.add(frames, &[]);
            self.bytes_passthrough.add(passthrough_bytes, &[]);
        }

        pub fn upstream_connected(&self, duration: f64) {
            self.upstream_connect_duration.record(duration, &[]);
        }

        pub fn relay_done(&self, duration: f64, bytes: u64) {
            self.response_relay_duration.record(duration, &[]);
            self.response_bytes.record(bytes as f64, &[]);
        }
    }
}

#[cfg(not(feature = "metrics-minimal"))]
mod otel {
    pub(crate) struct OtelMetrics;

    impl OtelMetrics {
        pub fn new() -> Self { Self }
        #[inline(always)] pub fn conn_start(&self) {}
        #[inline(always)] pub fn conn_end(&self, _: f64) {}
        #[inline(always)] pub fn conn_error(&self) {}
        #[inline(always)] pub fn forward_done(&self, _: f64, _: u64, _: u64) {}
        #[inline(always)] pub fn upstream_connected(&self, _: f64) {}
        #[inline(always)] pub fn relay_done(&self, _: f64, _: u64) {}
    }
}

use otel::OtelMetrics;

#[cfg_attr(not(feature = "metrics-full"), allow(dead_code))]
struct ConnectionResult {
    forward_duration: f64,
    connect_duration: f64,
    relay_duration: f64,
    frame_count: u64,
    response_bytes: u64,
    request_bytes: u64,
}

pub struct Pipeline<Src, Par, Pip, Rtr>
where
    Src: StreamSource,
    Par: FrameParser<Src::ReadHalf>,
    Pip: FramePipeline<Frame = Par::Frame>,
    Rtr: DestinationRouter,
{
    pub source: Src,
    pub parser_factory: fn() -> Par,
    pub pipeline: Arc<Pip>,
    pub router: Arc<Rtr>,
    #[cfg(feature = "metrics-full")]
    pub sampler: Option<Arc<SamplerHandle>>,
}

impl<Src, Par, Pip, Rtr> Pipeline<Src, Par, Pip, Rtr>
where
    Src: StreamSource + Sync,
    Par: FrameParser<Src::ReadHalf> + 'static,
    Pip: FramePipeline<Frame = Par::Frame> + 'static,
    Rtr: DestinationRouter + 'static,
    Rtr::Destination: 'static,
    <Rtr::Destination as StreamDestination>::Error: Send,
{
    async fn handle_connection(
        read_half: Src::ReadHalf,
        mut write_half: Src::WriteHalf,
        parser_factory: fn() -> Par,
        pipeline: Arc<Pip>,
        router: Arc<Rtr>,
        otel: Arc<OtelMetrics>,
        #[cfg(feature = "metrics-full")] sampler: Option<Arc<SamplerHandle>>,
        #[cfg(feature = "metrics-full")] start_time: Instant,
    ) {
        #[cfg(feature = "metrics-full")]
        if let Some(ref s) = sampler {
            s.shared().active_connections.fetch_add(1, Ordering::Relaxed);
        }
        otel.conn_start();

        let conn_start = Instant::now();
        let result = Self::do_connection(
            read_half, &mut write_half, parser_factory, pipeline, router, &otel,
        ).await;

        let total_duration = conn_start.elapsed().as_secs_f64();
        otel.conn_end(total_duration);

        #[cfg(feature = "metrics-full")]
        if let Some(ref s) = sampler {
            s.shared().active_connections.fetch_add(-1, Ordering::Relaxed);
        }

        match result {
            Ok(_cr) => {
                #[cfg(feature = "metrics-full")]
                if let Some(s) = sampler {
                    s.log_request(RequestRecord {
                        t: start_time.elapsed().as_secs_f64(),
                        total_us: (total_duration * 1e6) as u64,
                        connect_us: (_cr.connect_duration * 1e6) as u64,
                        forward_us: (_cr.forward_duration * 1e6) as u64,
                        relay_us: (_cr.relay_duration * 1e6) as u64,
                        frames: _cr.frame_count,
                        req_bytes: _cr.request_bytes,
                        resp_bytes: _cr.response_bytes,
                        error: false,
                    });
                }
            }
            Err(e) => {
                otel.conn_error();
                #[cfg(feature = "metrics-full")]
                if let Some(s) = sampler {
                    s.log_request(RequestRecord {
                        t: start_time.elapsed().as_secs_f64(),
                        total_us: (total_duration * 1e6) as u64,
                        connect_us: 0,
                        forward_us: 0,
                        relay_us: 0,
                        frames: 0,
                        req_bytes: 0,
                        resp_bytes: 0,
                        error: true,
                    });
                }
                warn!(error = %e, "connection error");
            }
        }
    }

    async fn do_connection(
        read_half: Src::ReadHalf,
        write_half: &mut Src::WriteHalf,
        parser_factory: fn() -> Par,
        pipeline: Arc<Pip>,
        router: Arc<Rtr>,
        otel: &OtelMetrics,
    ) -> Result<ConnectionResult, RailscaleError> {
        let forward_start = Instant::now();
        let mut parser = parser_factory();
        let frames = parser.parse(read_half);
        let mut frames = pin!(frames);
        let mut frame_count: u64 = 0;
        let mut passthrough_bytes: u64 = 0;
        let mut request_bytes: u64 = 0;

        let mut pre_route_buf: Vec<Bytes> = Vec::new();
        let mut routing_key: Option<Bytes> = None;

        while let Some(result) = frames.next().await {
            match result {
                Ok(ParsedData::Passthrough(bytes)) => {
                    passthrough_bytes += bytes.len() as u64;
                    request_bytes += bytes.len() as u64;
                    pre_route_buf.push(bytes);
                }
                Ok(ParsedData::Parsed(frame)) => {
                    frame_count += 1;
                    request_bytes += frame.as_bytes().len() as u64;
                    let key = frame.routing_key().map(Bytes::copy_from_slice);
                    let processed = pipeline.process(frame);
                    pre_route_buf.push(processed.into_bytes());
                    if let Some(k) = key {
                        routing_key = Some(k);
                        break;
                    }
                }
                Err(e) => {
                    warn!(error = %e.into(), "frame parse error");
                    return Err(RailscaleError::ConnectionClosed);
                }
            }
        }

        let routing_key = routing_key.ok_or(RailscaleError::NoRoutingFrame)?;

        let connect_start = Instant::now();

        let (dest_result, post_route_buf) = tokio::join!(
            router.route(&routing_key),
            async {
                let mut buf: Vec<Bytes> = Vec::new();
                while let Some(result) = frames.next().await {
                    match result {
                        Ok(ParsedData::Passthrough(bytes)) => {
                            passthrough_bytes += bytes.len() as u64;
                            request_bytes += bytes.len() as u64;
                            buf.push(bytes);
                        }
                        Ok(ParsedData::Parsed(frame)) => {
                            frame_count += 1;
                            request_bytes += frame.as_bytes().len() as u64;
                            let processed = pipeline.process(frame);
                            buf.push(processed.into_bytes());
                        }
                        Err(e) => {
                            warn!(error = %e.into(), "frame parse error");
                            break;
                        }
                    }
                }
                buf
            }
        );

        let connect_duration = connect_start.elapsed().as_secs_f64();
        otel.upstream_connected(connect_duration);

        let mut dest = dest_result?;

        for chunk in pre_route_buf {
            dest.write(chunk).await.map_err(Into::into)?;
        }

        for chunk in post_route_buf {
            dest.write(chunk).await.map_err(Into::into)?;
        }

        let forward_duration = forward_start.elapsed().as_secs_f64();
        otel.forward_done(forward_duration, frame_count, passthrough_bytes);

        let relay_start = Instant::now();
        let response_bytes = dest.relay_response(write_half).await.map_err(Into::into)?;
        let relay_duration = relay_start.elapsed().as_secs_f64();
        otel.relay_done(relay_duration, response_bytes);

        Ok(ConnectionResult {
            forward_duration,
            connect_duration,
            relay_duration,
            frame_count,
            response_bytes,
            request_bytes,
        })
    }
}

impl<Src, Par, Pip, Rtr> Service for Pipeline<Src, Par, Pip, Rtr>
where
    Src: StreamSource + Sync + 'static,
    Src::ReadHalf: Send + 'static,
    Src::WriteHalf: Send + 'static,
    Par: FrameParser<Src::ReadHalf> + 'static,
    Par::Error: Send,
    Pip: FramePipeline<Frame = Par::Frame> + 'static,
    Rtr: DestinationRouter + 'static,
    Rtr::Destination: 'static,
    <Rtr::Destination as StreamDestination>::Error: Send,
{
    async fn run(&self) -> Result<(), RailscaleError> {
        let otel = Arc::new(OtelMetrics::new());
        #[cfg(feature = "metrics-full")]
        let sampler = self.sampler.clone();
        #[cfg(feature = "metrics-full")]
        let start_time = Instant::now();

        loop {
            let (read_half, write_half) = self.source.accept().await.map_err(Into::into)?;
            let parser_factory = self.parser_factory;
            let router = Arc::clone(&self.router);
            let pipeline = Arc::clone(&self.pipeline);
            let otel = Arc::clone(&otel);
            #[cfg(feature = "metrics-full")]
            let sampler = sampler.clone();

            tokio::spawn(Self::handle_connection(
                read_half, write_half, parser_factory, pipeline, router,
                otel,
                #[cfg(feature = "metrics-full")] sampler,
                #[cfg(feature = "metrics-full")] start_time,
            ));
        }
    }
}
