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
use crate::core::hook::{ConnectionHook, NoHook};
use crate::core::stabling::{Stabling, StablingConfig};
use crate::io::source::StreamSource;
use crate::io::batcher::BatchWriter;
use crate::{RailscaleError, ErrorKind, Phase};

#[cfg(feature = "capture")]
use crate::capture::{CaptureHandle, Direction};

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

impl Default for ConnectionResult {
    fn default() -> Self {
        Self {
            forward_duration: 0.0,
            connect_duration: 0.0,
            relay_duration: 0.0,
            frame_count: 0,
            response_bytes: 0,
            request_bytes: 0,
            routed: false,
        }
    }
}

impl ConnectionResult {
    fn accumulate(&mut self, other: &ConnectionResult) {
        self.forward_duration += other.forward_duration;
        self.connect_duration += other.connect_duration;
        self.relay_duration += other.relay_duration;
        self.frame_count += other.frame_count;
        self.response_bytes += other.response_bytes;
        self.request_bytes += other.request_bytes;
        self.routed |= other.routed;
    }
}

pub struct Pipeline<Src, Par, Pip, Rtr, Hook = NoHook, RPar = Par>
where
    Src: StreamSource,
    Par: FrameParser<Src::ReadHalf>,
    Pip: FramePipeline<Frame = Par::Frame>,
    Rtr: DestinationRouter,
    Hook: ConnectionHook<Par::Frame>,
{
    pub source: Src,
    pub parser_factory: fn() -> Par,
    pub pipeline: Arc<Pip>,
    pub router: Arc<Rtr>,
    pub error_responder: Option<Arc<dyn ErrorToBytes + Send + Sync>>,
    pub buffer_limits: BufferLimits,
    pub drain_timeout: Duration,
    pub hook_factory: fn() -> Hook,
    pub response_parser_factory: Option<fn() -> RPar>,
    pub response_pipeline: Option<Arc<Pip>>,
    pub response_hook_factory: Option<fn() -> Hook>,
    pub stabling_config: Option<StablingConfig>,
    #[cfg(feature = "metrics-full")]
    pub recorder: Option<Arc<RecorderHandle>>,
    pub turnout_name: String,
    pub capture_dir: Option<std::path::PathBuf>,
}

impl<Src, Par, Pip, Rtr, Hook, RPar> Pipeline<Src, Par, Pip, Rtr, Hook, RPar>
where
    Src: StreamSource + Sync,
    Par: FrameParser<Src::ReadHalf> + 'static,
    Pip: FramePipeline<Frame = Par::Frame> + 'static,
    Rtr: DestinationRouter + 'static,
    Rtr::Destination: Sync + 'static,
    <Rtr::Destination as StreamDestination>::Error: Send,
    Hook: ConnectionHook<Par::Frame>,
    RPar: for<'a> FrameParser<&'a mut <Rtr::Destination as StreamDestination>::ResponseReader, Frame = Par::Frame> + 'static,
    for<'a> <RPar as FrameParser<&'a mut <Rtr::Destination as StreamDestination>::ResponseReader>>::Error: Send,
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
        hook_factory: fn() -> Hook,
        response_parser_factory: Option<fn() -> RPar>,
        response_pipeline: Option<Arc<Pip>>,
        response_hook_factory: Option<fn() -> Hook>,
        stabling: Option<Arc<Stabling<Rtr::Destination>>>,
        #[cfg(feature = "metrics-full")] recorder: Option<Arc<RecorderHandle>>,
        #[cfg(feature = "metrics-full")] start_time: Instant,
        #[cfg(feature = "capture")] capture: CaptureHandle,
    ) {
        #[cfg(feature = "metrics-full")]
        if let Some(ref r) = recorder {
            r.conn_start();
        }
        otel.conn_start();

        let conn_start = Instant::now();
        let result = Self::do_connection(
            read_half, &mut write_half, parser_factory, pipeline, router, &otel,
            &error_responder, &buffer_limits, hook_factory,
            response_parser_factory, &response_pipeline, &response_hook_factory,
            &stabling,
            #[cfg(feature = "metrics-full")] &recorder,
            #[cfg(feature = "capture")] &capture,
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

    #[allow(clippy::too_many_arguments)]
    async fn do_connection(
        read_half: Src::ReadHalf,
        write_half: &mut Src::WriteHalf,
        parser_factory: fn() -> Par,
        pipeline: Arc<Pip>,
        router: Arc<Rtr>,
        otel: &OtelMetrics,
        error_responder: &Option<Arc<dyn ErrorToBytes + Send + Sync>>,
        buffer_limits: &BufferLimits,
        hook_factory: fn() -> Hook,
        response_parser_factory: Option<fn() -> RPar>,
        response_pipeline: &Option<Arc<Pip>>,
        response_hook_factory: &Option<fn() -> Hook>,
        stabling: &Option<Arc<Stabling<Rtr::Destination>>>,
        #[cfg(feature = "metrics-full")] recorder: &Option<Arc<RecorderHandle>>,
        #[cfg(feature = "capture")] capture: &CaptureHandle,
    ) -> (ConnectionResult, Result<(), RailscaleError>) {
        let mut parser = parser_factory();
        let frames = parser.parse(read_half);
        let mut frames = pin!(frames);
        let mut total_cr = ConnectionResult::default();
        let mut first_cycle = true;
        #[cfg(feature = "capture")]
        let connection_id = capture.next_connection_id();

        loop {
            let forward_start = Instant::now();
            let mut hook = (hook_factory)();
            let mut frame_count: u64 = 0;
            let mut passthrough_bytes: u64 = 0;
            let mut request_bytes: u64 = 0;
            let mut pre_route_cumulative: usize = 0;

            let mut cr = ConnectionResult::default();

            let mut pre_route_buf: Vec<Bytes> = Vec::new();
            let mut routing_key: Option<Bytes> = None;

            let mut client_closed = false;

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
                            total_cr.accumulate(&cr);
                            return (total_cr, Err(err));
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
                            total_cr.accumulate(&cr);
                            return (total_cr, Err(err));
                        }
                        hook.on_frame(&frame);
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
                        if !first_cycle && matches!(err.kind, ErrorKind::ConnectionClosed) {
                            client_closed = true;
                            break;
                        }
                        let err = err.in_phase(Phase::Parse);
                        warn!(error = %err, "frame parse error");
                        otel.parse_error();
                        cr.frame_count = frame_count;
                        cr.request_bytes = request_bytes;
                        Self::write_error_response(write_half, error_responder, &err).await;
                        total_cr.accumulate(&cr);
                        return (total_cr, Err(err));
                    }
                }
            }

            if client_closed || (routing_key.is_none() && !first_cycle && pre_route_buf.is_empty()) {
                total_cr.accumulate(&cr);
                return (total_cr, Ok(()));
            }

            let routing_key = match routing_key {
                Some(k) => k,
                None => {
                    cr.frame_count = frame_count;
                    cr.request_bytes = request_bytes;
                    let err = RailscaleError::from(ErrorKind::NoRoutingFrame).in_phase(Phase::Parse);
                    Self::write_error_response(write_half, error_responder, &err).await;
                    total_cr.accumulate(&cr);
                    return (total_cr, Err(err));
                }
            };

            debug!(
                routing_key = %String::from_utf8_lossy(&routing_key).trim(),
                "routing"
            );

            let mut post_route_cumulative: usize = 0;
            let max_post = buffer_limits.max_post_route_bytes;

            let acquired = stabling.as_ref().and_then(|s| s.acquire(&routing_key));
            #[cfg_attr(not(feature = "metrics-full"), allow(unused_variables))]
            let need_route = acquired.is_none();

            let ((dest_result, route_duration), post_route_buf) = tokio::join!(
                async {
                    if let Some(dest) = acquired {
                        return (Ok(dest), 0.0);
                    }
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
                                hook.on_frame(&frame);
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

            if post_route_cumulative > max_post {
                cr.frame_count = frame_count;
                cr.request_bytes = request_bytes;
                let err = RailscaleError::from(ErrorKind::BufferLimitExceeded).in_phase(Phase::Forward);
                Self::write_error_response(write_half, error_responder, &err).await;
                total_cr.accumulate(&cr);
                return (total_cr, Err(err));
            }

            if let Err(err) = hook.validate() {
                cr.frame_count = frame_count;
                cr.request_bytes = request_bytes;
                let err = err.in_phase(Phase::Parse);
                Self::write_error_response(write_half, error_responder, &err).await;
                total_cr.accumulate(&cr);
                return (total_cr, Err(err));
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
                    total_cr.accumulate(&cr);
                    return (total_cr, Err(err));
                }
            };
            cr.routed = true;
            #[cfg(feature = "metrics-full")]
            if need_route {
                if let Some(r) = recorder {
                    r.upstream_open();
                }
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
            let request_data = batcher.take();
            #[cfg(feature = "capture")]
            capture.send(Direction::Request, request_data.clone(), connection_id);
            let total_len = request_data.len() as u64;
            match dest.write(request_data).await {
                Ok(()) => {
                    otel.bytes_written(total_len);
                }
                Err(e) => {
                    otel.write_error();
                    cr.frame_count = frame_count;
                    cr.request_bytes = request_bytes;
                    cr.forward_duration = forward_start.elapsed().as_secs_f64();
                    let err: RailscaleError = e.into();
                    total_cr.accumulate(&cr);
                    return (total_cr, Err(err.in_phase(Phase::Forward)));
                }
            }

            let forward_duration = forward_start.elapsed().as_secs_f64();
            otel.forward_done(forward_duration, frame_count, passthrough_bytes);

            let relay_start = Instant::now();

            let (response_bytes, request_close) = match response_parser_factory {
                Some(rpf) => {
                    let mut response_hook = response_hook_factory.map(|f| (f)());
                    let mut resp_parser = (rpf)();
                    let response_reader = dest.response_reader();
                    let resp_stream = resp_parser.parse(response_reader);
                    let mut resp_stream = pin!(resp_stream);

                    let mut resp_bytes: u64 = 0;
                    let mut response_batcher = BatchWriter::new();
                    let mut relay_err = None;

                    while let Some(result) = resp_stream.next().await {
                        match result {
                            Ok(ParsedData::Parsed(frame)) => {
                                if let Some(ref mut rh) = response_hook {
                                    rh.on_frame(&frame);
                                }
                                let processed = match response_pipeline {
                                    Some(rp) => rp.process(frame),
                                    None => frame,
                                };
                                let bytes = processed.into_bytes();
                                resp_bytes += bytes.len() as u64;
                                #[cfg(feature = "log-raw")]
                                trace!(
                                    direction = "<<",
                                    bytes = bytes.len(),
                                    data = %String::from_utf8_lossy(&bytes),
                                    "raw"
                                );
                                response_batcher.push_bytes(bytes);
                            }
                            Ok(ParsedData::Passthrough(bytes)) => {
                                resp_bytes += bytes.len() as u64;
                                #[cfg(feature = "log-raw")]
                                trace!(
                                    direction = "<<",
                                    bytes = bytes.len(),
                                    data = %String::from_utf8_lossy(&bytes),
                                    "raw"
                                );
                                response_batcher.push_bytes(bytes);
                            }
                            Err(e) => {
                                if response_batcher.len() > 0 {
                                    let _ = write_half.write_all(&response_batcher.take()).await;
                                }
                                relay_err = Some(e);
                                break;
                            }
                        }
                        if response_batcher.len() > 32768 {
                            let batch = response_batcher.take();
                            #[cfg(feature = "capture")]
                            capture.send(Direction::Response, batch.clone(), connection_id);
                            if let Err(e) = write_half.write_all(&batch).await {
                                let err: RailscaleError = e.into();
                                cr.frame_count = frame_count;
                                cr.request_bytes = request_bytes;
                                cr.forward_duration = forward_duration;
                                cr.response_bytes = resp_bytes;
                                total_cr.accumulate(&cr);
                                return (total_cr, Err(err.in_phase(Phase::Relay)));
                            }
                        }
                    }

                    if let Some(e) = relay_err {
                        cr.frame_count = frame_count;
                        cr.request_bytes = request_bytes;
                        cr.forward_duration = forward_duration;
                        cr.response_bytes = resp_bytes;
                        let err: RailscaleError = e.into();
                        total_cr.accumulate(&cr);
                        return (total_cr, Err(err.in_phase(Phase::Relay)));
                    }

                    if response_batcher.len() > 0 {
                        let batch = response_batcher.take();
                        #[cfg(feature = "capture")]
                        capture.send(Direction::Response, batch.clone(), connection_id);
                        if let Err(e) = write_half.write_all(&batch).await {
                            let err: RailscaleError = e.into();
                            cr.frame_count = frame_count;
                            cr.request_bytes = request_bytes;
                            cr.forward_duration = forward_duration;
                            cr.response_bytes = resp_bytes;
                            total_cr.accumulate(&cr);
                            return (total_cr, Err(err.in_phase(Phase::Relay)));
                        }
                    }
                    let _ = write_half.flush().await;

                    let should_close = hook.should_close_connection()
                        || response_hook.as_ref().map_or(false, |rh| rh.should_close_connection());

                    (resp_bytes, should_close)
                }
                None => {
                    let response_reader = dest.response_reader();

                    #[cfg(feature = "log-raw")]
                    let resp_bytes = {
                        let mut tracing_writer = TracingWriter { inner: &mut *write_half };
                        match tokio::io::copy(response_reader, &mut tracing_writer).await {
                            Ok(b) => b,
                            Err(e) => {
                                cr.frame_count = frame_count;
                                cr.request_bytes = request_bytes;
                                cr.forward_duration = forward_duration;
                                let err: RailscaleError = e.into();
                                total_cr.accumulate(&cr);
                                return (total_cr, Err(err.in_phase(Phase::Relay)));
                            }
                        }
                    };
                    #[cfg(not(feature = "log-raw"))]
                    let resp_bytes = match tokio::io::copy(response_reader, write_half).await {
                        Ok(b) => b,
                        Err(e) => {
                            cr.frame_count = frame_count;
                            cr.request_bytes = request_bytes;
                            cr.forward_duration = forward_duration;
                            let err: RailscaleError = e.into();
                            total_cr.accumulate(&cr);
                            return (total_cr, Err(err.in_phase(Phase::Relay)));
                        }
                    };

                    (resp_bytes, true)
                }
            };

            let relay_duration = relay_start.elapsed().as_secs_f64();
            otel.relay_done(relay_duration, response_bytes);

            cr.forward_duration = forward_duration;
            cr.relay_duration = relay_duration;
            cr.frame_count = frame_count;
            cr.response_bytes = response_bytes;
            cr.request_bytes = request_bytes;
            total_cr.accumulate(&cr);

            first_cycle = false;

            if request_close || response_parser_factory.is_none() {
                break;
            }

            if let Some(s) = stabling {
                s.release(Bytes::copy_from_slice(&routing_key), dest);
            }
        }

        (total_cr, Ok(()))
    }
}

impl<Src, Par, Pip, Rtr, Hook, RPar> Service for Pipeline<Src, Par, Pip, Rtr, Hook, RPar>
where
    Src: StreamSource + Sync + 'static,
    Src::ReadHalf: Send + 'static,
    Src::WriteHalf: Send + 'static,
    Par: FrameParser<Src::ReadHalf> + 'static,
    Par::Error: Send,
    Pip: FramePipeline<Frame = Par::Frame> + 'static,
    Rtr: DestinationRouter + 'static,
    Rtr::Destination: Sync + 'static,
    <Rtr::Destination as StreamDestination>::Error: Send,
    Hook: ConnectionHook<Par::Frame>,
    RPar: for<'a> FrameParser<&'a mut <Rtr::Destination as StreamDestination>::ResponseReader, Frame = Par::Frame> + 'static,
    for<'a> <RPar as FrameParser<&'a mut <Rtr::Destination as StreamDestination>::ResponseReader>>::Error: Send,
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

        let stabling: Option<Arc<Stabling<Rtr::Destination>>> = self.stabling_config.as_ref().map(|config| {
            Arc::new(Stabling::new(StablingConfig {
                max_idle_per_host: config.max_idle_per_host,
                max_total_idle: config.max_total_idle,
                idle_timeout: config.idle_timeout,
                enabled: config.enabled,
            }))
        });

        if let Some(ref s) = stabling {
            let s = Arc::clone(s);
            let c = cancel.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(30));
                loop {
                    tokio::select! {
                        _ = c.cancelled() => break,
                        _ = interval.tick() => s.reap_expired(),
                    }
                }
            });
        }

        #[cfg(feature = "capture")]
        let capture_handle = {
            let dir = self.capture_dir.clone().unwrap_or_else(|| std::path::PathBuf::from("."));
            let (handle, _task) = CaptureHandle::spawn(self.turnout_name.clone(), dir);
            handle
        };

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
                    let hook_factory = self.hook_factory;
                    let response_parser_factory = self.response_parser_factory;
                    let response_pipeline = self.response_pipeline.clone();
                    let response_hook_factory = self.response_hook_factory;
                    let stabling = stabling.clone();
                    #[cfg(feature = "metrics-full")]
                    let recorder = recorder.clone();
                    #[cfg(feature = "capture")]
                    let capture = capture_handle.clone();

                    join_set.spawn(Self::handle_connection(
                        read_half, write_half, parser_factory, pipeline, router,
                        otel, error_responder, buffer_limits, hook_factory,
                        response_parser_factory, response_pipeline, response_hook_factory,
                        stabling,
                        #[cfg(feature = "metrics-full")] recorder,
                        #[cfg(feature = "metrics-full")] start_time,
                        #[cfg(feature = "capture")] capture,
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
