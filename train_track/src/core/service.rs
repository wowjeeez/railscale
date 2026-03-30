use std::pin::pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use bytes::Bytes;
use tokio::io::AsyncWriteExt;
use tokio::task::JoinSet;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{info, debug, warn};
#[cfg(feature = "log-raw")]
use tracing::trace;
#[cfg(feature = "log-raw")]
use std::pin::Pin;
#[cfg(feature = "log-raw")]
use std::task::{Context, Poll};
#[cfg(feature = "log-raw")]
use tokio::io::{self, AsyncWrite};
use crate::io::destination::StreamDestination;
use crate::io::router::DestinationRouter;
use crate::atom::frame::{Frame, ParsedData};
use crate::atom::parser::FrameParser;
use crate::core::pipeline::FramePipeline;
use crate::core::error_mapper::ErrorToBytes;
use crate::io::source::StreamSource;
use crate::io::batcher::BatchWriter;
use crate::{RailscaleError, ErrorKind, Phase};

#[cfg(feature = "metrics-full")]
use crate::recorder::{RecorderHandle, RequestEntry};

#[cfg(feature = "log-raw")]
struct TracingWriter<W> {
    inner: W,
}

#[cfg(feature = "log-raw")]
impl<W: AsyncWrite + Unpin> AsyncWrite for TracingWriter<W> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        trace!(
            direction = "<<",
            bytes = buf.len(),
            data = %String::from_utf8_lossy(buf),
            "raw"
        );
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

pub trait Service: Send + Sync {
    fn run(&self, cancel: CancellationToken) -> impl std::future::Future<Output = Result<(), RailscaleError>> + Send;
}

#[cfg(feature = "metrics-minimal")]
mod otel {
    use opentelemetry::global;
    use opentelemetry::metrics::{Counter, Histogram, UpDownCounter};
    use opentelemetry::KeyValue;

    pub(crate) struct OtelMetrics {
        connections_total: Counter<u64>,
        connections_active: UpDownCounter<i64>,
        connection_errors: Counter<u64>,
        connection_duration: Histogram<f64>,
        request_forward_duration: Histogram<f64>,
        routing_duration: Histogram<f64>,
        response_relay_duration: Histogram<f64>,
        response_bytes: Histogram<f64>,
        frames_parsed: Counter<u64>,
        bytes_passthrough: Counter<u64>,
        frame_parse_errors: Counter<u64>,
        destination_bytes_written: Counter<u64>,
        destination_write_errors: Counter<u64>,
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
                routing_duration: meter.f64_histogram("railscale.routing_duration_seconds")
                    .with_boundaries(latency_buckets.clone()).build(),
                response_relay_duration: meter.f64_histogram("railscale.response_relay_duration_seconds")
                    .with_boundaries(latency_buckets).build(),
                response_bytes: meter.f64_histogram("railscale.response_bytes")
                    .with_boundaries(size_buckets).build(),
                frames_parsed: meter.u64_counter("railscale.frames_parsed").build(),
                bytes_passthrough: meter.u64_counter("railscale.bytes_passthrough").build(),
                frame_parse_errors: meter.u64_counter("railscale.frame_parse_errors").build(),
                destination_bytes_written: meter.u64_counter("railscale.destination_bytes_written").build(),
                destination_write_errors: meter.u64_counter("railscale.destination_write_errors").build(),
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

        pub fn conn_error(&self, error_type: &'static str) {
            self.connection_errors.add(1, &[KeyValue::new("error_type", error_type)]);
        }

        pub fn forward_done(&self, duration: f64, frames: u64, passthrough_bytes: u64) {
            self.request_forward_duration.record(duration, &[]);
            self.frames_parsed.add(frames, &[]);
            self.bytes_passthrough.add(passthrough_bytes, &[]);
        }

        pub fn route_done(&self, duration: f64) {
            self.routing_duration.record(duration, &[]);
        }

        pub fn relay_done(&self, duration: f64, bytes: u64) {
            self.response_relay_duration.record(duration, &[]);
            self.response_bytes.record(bytes as f64, &[]);
        }

        pub fn parse_error(&self) {
            self.frame_parse_errors.add(1, &[]);
        }

        pub fn bytes_written(&self, bytes: u64) {
            self.destination_bytes_written.add(bytes, &[]);
        }

        pub fn write_error(&self) {
            self.destination_write_errors.add(1, &[]);
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
        #[inline(always)] pub fn conn_error(&self, _: &'static str) {}
        #[inline(always)] pub fn forward_done(&self, _: f64, _: u64, _: u64) {}
        #[inline(always)] pub fn route_done(&self, _: f64) {}
        #[inline(always)] pub fn relay_done(&self, _: f64, _: u64) {}
        #[inline(always)] pub fn parse_error(&self) {}
        #[inline(always)] pub fn bytes_written(&self, _: u64) {}
        #[inline(always)] pub fn write_error(&self) {}
    }
}

use otel::OtelMetrics;

const DEFAULT_MAX_BYTES: usize = 8 * 1024 * 1024;

pub struct BufferLimits {
    pub max_pre_route_bytes: usize,
    pub max_post_route_bytes: usize,
}

impl Default for BufferLimits {
    fn default() -> Self {
        Self {
            max_pre_route_bytes: DEFAULT_MAX_BYTES,
            max_post_route_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

#[cfg_attr(not(feature = "metrics-full"), allow(dead_code))]
struct ConnectionResult {
    forward_duration: f64,
    connect_duration: f64,
    relay_duration: f64,
    frame_count: u64,
    response_bytes: u64,
    request_bytes: u64,
    routed: bool,
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
    pub error_responder: Option<Arc<dyn ErrorToBytes + Send + Sync>>,
    pub buffer_limits: BufferLimits,
    pub drain_timeout: Duration,
    #[cfg(feature = "metrics-full")]
    pub recorder: Option<Arc<RecorderHandle>>,
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
        error_responder: Option<Arc<dyn ErrorToBytes + Send + Sync>>,
        buffer_limits: Arc<BufferLimits>,
        #[cfg(feature = "metrics-full")] recorder: Option<Arc<RecorderHandle>>,
        #[cfg(feature = "metrics-full")] start_time: Instant,
    ) {
        #[cfg(feature = "metrics-full")]
        if let Some(ref r) = recorder {
            r.conn_start();
        }
        otel.conn_start();

        let conn_start = Instant::now();
        let result = Self::do_connection(
            read_half, &mut write_half, parser_factory, pipeline, router, &otel,
            &error_responder, &buffer_limits,
            #[cfg(feature = "metrics-full")] &recorder,
        ).await;

        let total_duration = conn_start.elapsed().as_secs_f64();
        otel.conn_end(total_duration);

        #[cfg(feature = "metrics-full")]
        if let Some(ref r) = recorder {
            r.conn_end();
        }

        #[cfg_attr(not(feature = "metrics-full"), allow(unused_variables))]
        let (cr, result) = result;

        match &result {
            Ok(()) => {
                info!(
                    req_bytes = cr.request_bytes,
                    resp_bytes = cr.response_bytes,
                    frames = cr.frame_count,
                    total_ms = format_args!("{:.2}", total_duration * 1e3),
                    route_ms = format_args!("{:.2}", cr.connect_duration * 1e3),
                    forward_ms = format_args!("{:.2}", cr.forward_duration * 1e3),
                    relay_ms = format_args!("{:.2}", cr.relay_duration * 1e3),
                    "request complete"
                );
            }
            Err(e) => {
                let error_type = match &e.kind {
                    ErrorKind::RoutingFailed(_) => "routing",
                    ErrorKind::NoRoutingFrame => "no_routing_frame",
                    ErrorKind::ConnectionClosed => "connection_closed",
                    ErrorKind::Parse(_) => "parse",
                    ErrorKind::Io(_) => "io",
                    ErrorKind::BufferLimitExceeded => "buffer_limit_exceeded",
                };
                otel.conn_error(error_type);
                warn!(
                    error = %e,
                    error_type,
                    req_bytes = cr.request_bytes,
                    total_ms = format_args!("{:.2}", total_duration * 1e3),
                    "connection error"
                );
            }
        }

        #[cfg(feature = "metrics-full")]
        if let Some(ref r) = recorder {
            r.log_request(RequestEntry {
                t: start_time.elapsed().as_secs_f64(),
                total_us: (total_duration * 1e6) as u64,
                route_us: (cr.connect_duration * 1e6) as u64,
                forward_us: (cr.forward_duration * 1e6) as u64,
                relay_us: (cr.relay_duration * 1e6) as u64,
                req_bytes: cr.request_bytes,
                resp_bytes: cr.response_bytes,
                error: result.is_err(),
            });
            if cr.routed {
                r.upstream_close();
            }
        }
    }

    async fn write_error_response(
        write_half: &mut Src::WriteHalf,
        error_responder: &Option<Arc<dyn ErrorToBytes + Send + Sync>>,
        err: &RailscaleError,
    ) {
        if let Some(responder) = error_responder {
            let body = responder.error_bytes(err);
            let _ = write_half.write_all(&body).await;
            let _ = write_half.flush().await;
        }
    }

    async fn do_connection(
        read_half: Src::ReadHalf,
        write_half: &mut Src::WriteHalf,
        parser_factory: fn() -> Par,
        pipeline: Arc<Pip>,
        router: Arc<Rtr>,
        otel: &OtelMetrics,
        error_responder: &Option<Arc<dyn ErrorToBytes + Send + Sync>>,
        buffer_limits: &BufferLimits,
        #[cfg(feature = "metrics-full")] recorder: &Option<Arc<RecorderHandle>>,
    ) -> (ConnectionResult, Result<(), RailscaleError>) {
        let forward_start = Instant::now();
        let mut parser = parser_factory();
        let frames = parser.parse(read_half);
        let mut frames = pin!(frames);
        let mut frame_count: u64 = 0;
        let mut passthrough_bytes: u64 = 0;
        let mut request_bytes: u64 = 0;
        let mut pre_route_cumulative: usize = 0;

        let mut cr = ConnectionResult {
            forward_duration: 0.0,
            connect_duration: 0.0,
            relay_duration: 0.0,
            frame_count: 0,
            response_bytes: 0,
            request_bytes: 0,
            routed: false,
        };

        let mut pre_route_buf: Vec<Bytes> = Vec::new();
        let mut routing_key: Option<Bytes> = None;

        while let Some(result) = frames.next().await {
            match result {
                Ok(ParsedData::Passthrough(bytes)) => {
                    passthrough_bytes += bytes.len() as u64;
                    request_bytes += bytes.len() as u64;
                    pre_route_cumulative += bytes.len();
                    if pre_route_cumulative > buffer_limits.max_pre_route_bytes {
                        cr.frame_count = frame_count;
                        cr.request_bytes = request_bytes;
                        let err = RailscaleError::from(ErrorKind::BufferLimitExceeded).in_phase(Phase::Parse);
                        Self::write_error_response(write_half, error_responder, &err).await;
                        return (cr, Err(err));
                    }
                    pre_route_buf.push(bytes);
                }
                Ok(ParsedData::Parsed(frame)) => {
                    frame_count += 1;
                    let frame_len = frame.as_bytes().len();
                    request_bytes += frame_len as u64;
                    pre_route_cumulative += frame_len;
                    if pre_route_cumulative > buffer_limits.max_pre_route_bytes {
                        cr.frame_count = frame_count;
                        cr.request_bytes = request_bytes;
                        let err = RailscaleError::from(ErrorKind::BufferLimitExceeded).in_phase(Phase::Parse);
                        Self::write_error_response(write_half, error_responder, &err).await;
                        return (cr, Err(err));
                    }
                    let key = frame.routing_key().map(Bytes::copy_from_slice);
                    let processed = pipeline.process(frame);
                    pre_route_buf.push(processed.into_bytes());
                    if let Some(k) = key {
                        routing_key = Some(k);
                        break;
                    }
                }
                Err(e) => {
                    let err: RailscaleError = e.into();
                    let err = err.in_phase(Phase::Parse);
                    warn!(error = %err, "frame parse error");
                    otel.parse_error();
                    cr.frame_count = frame_count;
                    cr.request_bytes = request_bytes;
                    Self::write_error_response(write_half, error_responder, &err).await;
                    return (cr, Err(err));
                }
            }
        }

        let routing_key = match routing_key {
            Some(k) => k,
            None => {
                cr.frame_count = frame_count;
                cr.request_bytes = request_bytes;
                let err = RailscaleError::from(ErrorKind::NoRoutingFrame).in_phase(Phase::Parse);
                Self::write_error_response(write_half, error_responder, &err).await;
                return (cr, Err(err));
            }
        };

        debug!(
            routing_key = %String::from_utf8_lossy(&routing_key).trim(),
            "routing"
        );

        let mut post_route_cumulative: usize = 0;
        let max_post = buffer_limits.max_post_route_bytes;

        let ((dest_result, route_duration), post_route_buf) = tokio::join!(
            async {
                let route_start = Instant::now();
                let result = router.route(&routing_key).await;
                let duration = route_start.elapsed().as_secs_f64();
                otel.route_done(duration);
                (result, duration)
            },
            async {
                let mut buf: Vec<Bytes> = Vec::new();
                while let Some(result) = frames.next().await {
                    match result {
                        Ok(ParsedData::Passthrough(bytes)) => {
                            passthrough_bytes += bytes.len() as u64;
                            request_bytes += bytes.len() as u64;
                            post_route_cumulative += bytes.len();
                            if post_route_cumulative > max_post {
                                break;
                            }
                            buf.push(bytes);
                        }
                        Ok(ParsedData::Parsed(frame)) => {
                            frame_count += 1;
                            request_bytes += frame.as_bytes().len() as u64;
                            post_route_cumulative += frame.as_bytes().len();
                            if post_route_cumulative > max_post {
                                break;
                            }
                            let processed = pipeline.process(frame);
                            buf.push(processed.into_bytes());
                        }
                        Err(e) => {
                            warn!(error = %e.into(), "frame parse error");
                            otel.parse_error();
                            break;
                        }
                    }
                }
                buf
            }
        );

        // upstream may already be connected via tokio::join; dest is dropped without data
        if post_route_cumulative > max_post {
            cr.frame_count = frame_count;
            cr.request_bytes = request_bytes;
            let err = RailscaleError::from(ErrorKind::BufferLimitExceeded).in_phase(Phase::Forward);
            Self::write_error_response(write_half, error_responder, &err).await;
            return (cr, Err(err));
        }

        cr.connect_duration = route_duration;

        let mut dest = match dest_result {
            Ok(d) => d,
            Err(e) => {
                cr.frame_count = frame_count;
                cr.request_bytes = request_bytes;
                let err: RailscaleError = e.into();
                let err = err.in_phase(Phase::Routing);
                Self::write_error_response(write_half, error_responder, &err).await;
                return (cr, Err(err));
            }
        };
        cr.routed = true;
        #[cfg(feature = "metrics-full")]
        if let Some(r) = recorder {
            r.upstream_open();
        }

        let mut batcher = BatchWriter::new();
        for chunk in pre_route_buf.into_iter().chain(post_route_buf.into_iter()) {
            #[cfg(feature = "log-raw")]
            trace!(
                direction = ">>",
                bytes = chunk.len(),
                data = %String::from_utf8_lossy(&chunk),
                "raw"
            );
            batcher.push_bytes(chunk);
        }
        let total_len = batcher.len() as u64;
        match dest.write(batcher.take()).await {
            Ok(()) => {
                otel.bytes_written(total_len);
            }
            Err(e) => {
                otel.write_error();
                cr.frame_count = frame_count;
                cr.request_bytes = request_bytes;
                cr.forward_duration = forward_start.elapsed().as_secs_f64();
                let err: RailscaleError = e.into();
                return (cr, Err(err.in_phase(Phase::Forward)));
            }
        }

        let forward_duration = forward_start.elapsed().as_secs_f64();
        otel.forward_done(forward_duration, frame_count, passthrough_bytes);

        let relay_start = Instant::now();

        #[cfg(feature = "log-raw")]
        let mut tracing_writer = TracingWriter { inner: write_half };

        #[cfg(feature = "log-raw")]
        let response_bytes = match dest.relay_response(&mut tracing_writer).await {
            Ok(b) => b,
            Err(e) => {
                cr.frame_count = frame_count;
                cr.request_bytes = request_bytes;
                cr.forward_duration = forward_duration;
                let err: RailscaleError = e.into();
                return (cr, Err(err.in_phase(Phase::Relay)));
            }
        };
        #[cfg(not(feature = "log-raw"))]
        let response_bytes = match dest.relay_response(write_half).await {
            Ok(b) => b,
            Err(e) => {
                cr.frame_count = frame_count;
                cr.request_bytes = request_bytes;
                cr.forward_duration = forward_duration;
                let err: RailscaleError = e.into();
                return (cr, Err(err.in_phase(Phase::Relay)));
            }
        };
        let relay_duration = relay_start.elapsed().as_secs_f64();
        otel.relay_done(relay_duration, response_bytes);

        cr.forward_duration = forward_duration;
        cr.relay_duration = relay_duration;
        cr.frame_count = frame_count;
        cr.response_bytes = response_bytes;
        cr.request_bytes = request_bytes;

        (cr, Ok(()))
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
    async fn run(&self, cancel: CancellationToken) -> Result<(), RailscaleError> {
        let otel = Arc::new(OtelMetrics::new());
        let error_responder = self.error_responder.clone();
        let buffer_limits = Arc::new(BufferLimits {
            max_pre_route_bytes: self.buffer_limits.max_pre_route_bytes,
            max_post_route_bytes: self.buffer_limits.max_post_route_bytes,
        });
        #[cfg(feature = "metrics-full")]
        let recorder = self.recorder.clone();
        #[cfg(feature = "metrics-full")]
        let start_time = Instant::now();

        let mut join_set = JoinSet::new();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("shutdown signal received, draining connections");
                    break;
                }
                accept_result = self.source.accept() => {
                    let (read_half, write_half) = accept_result.map_err(Into::into)?;
                    let parser_factory = self.parser_factory;
                    let router = Arc::clone(&self.router);
                    let pipeline = Arc::clone(&self.pipeline);
                    let otel = Arc::clone(&otel);
                    let error_responder = error_responder.clone();
                    let buffer_limits = Arc::clone(&buffer_limits);
                    #[cfg(feature = "metrics-full")]
                    let recorder = recorder.clone();

                    join_set.spawn(Self::handle_connection(
                        read_half, write_half, parser_factory, pipeline, router,
                        otel, error_responder, buffer_limits,
                        #[cfg(feature = "metrics-full")] recorder,
                        #[cfg(feature = "metrics-full")] start_time,
                    ));
                }
            }
        }

        let drain_timeout = self.drain_timeout;
        tokio::select! {
            _ = async { while join_set.join_next().await.is_some() {} } => {
                info!("all connections drained");
            }
            _ = tokio::time::sleep(drain_timeout) => {
                warn!("drain timeout reached, aborting remaining connections");
                join_set.abort_all();
            }
        }

        Ok(())
    }
}
