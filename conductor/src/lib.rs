use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;
use bytes::Bytes;
use memchr::memmem::Finder;
use carriage::http_v1::{HttpParser, HttpPipeline, HttpErrorResponder, HttpDeriverHook, ResponseParser};
use carriage::tcp::native::{TcpSource,TcpRouter};
use carriage::tcp::unix_sockets::{SockSource, TcpOverSockRouter};
use train_track::{Pipeline, Service, RailscaleError, CancellationToken, BufferLimits, StablingConfig};

#[cfg(feature = "metrics-full")]
use train_track::recorder::start_recorder;

pub struct NoRoute;
pub struct HasRoute;

enum Route {
    Tcp(String),
    Sock(String),
    Dynamic,
}

struct Config {
    headers: Vec<(Vec<u8>, Vec<u8>)>,
    route: Option<Route>,
    max_request_bytes: Option<usize>,
    inactivity_timeout: Option<Duration>,
    shutdown_token: Option<CancellationToken>,
    drain_timeout: Option<Duration>,
    #[cfg(feature = "metrics-full")]
    record_path: Option<String>,
}

impl Config {
    fn new() -> Self {
        Self {
            headers: Vec::new(),
            route: None,
            max_request_bytes: None,
            inactivity_timeout: None,
            shutdown_token: None,
            drain_timeout: None,
            #[cfg(feature = "metrics-full")]
            record_path: None,
        }
    }

    fn matchers(&self) -> Vec<(Finder<'static>, Bytes)> {
        self.headers
            .iter()
            .map(|(name, value)| {
                (Finder::new(&name).into_owned(), Bytes::from(value.clone()))
            })
            .collect()
    }

    fn buffer_limits(&self) -> BufferLimits {
        match self.max_request_bytes {
            Some(n) => BufferLimits {
                max_pre_route_bytes: n,
                max_post_route_bytes: n,
            },
            None => BufferLimits::default(),
        }
    }

    fn cancel_token(&self) -> CancellationToken {
        self.shutdown_token.clone().unwrap_or_default()
    }

    fn drain_duration(&self) -> Duration {
        self.drain_timeout.unwrap_or(Duration::from_secs(30))
    }
}

pub struct Conductor;

impl Conductor {
    pub fn tcp(addr: impl Into<String>) -> TcpBuilder {
        TcpBuilder {
            addr: addr.into(),
            config: Config::new(),
        }
    }

    pub fn sock(path: impl Into<String>) -> SockBuilder<NoRoute> {
        SockBuilder {
            path: path.into(),
            config: Config::new(),
            _state: PhantomData,
        }
    }
}

pub struct TcpBuilder {
    addr: String,
    config: Config,
}

impl TcpBuilder {
    pub fn route_tcp(mut self, addr: impl Into<String>) -> Self {
        self.config.route = Some(Route::Tcp(addr.into()));
        self
    }

    pub fn route_sock(mut self, path: impl Into<String>) -> Self {
        self.config.route = Some(Route::Sock(path.into()));
        self
    }

    pub fn route_dynamic(mut self) -> Self {
        self.config.route = Some(Route::Dynamic);
        self
    }

    pub fn replace_header(mut self, name: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> Self {
        self.config.headers.push((name.into(), value.into()));
        self
    }

    pub fn max_request_bytes(mut self, n: usize) -> Self {
        self.config.max_request_bytes = Some(n);
        self
    }

    pub fn inactivity_timeout(mut self, d: Duration) -> Self {
        self.config.inactivity_timeout = Some(d);
        self
    }

    pub fn shutdown_token(mut self, token: CancellationToken) -> Self {
        self.config.shutdown_token = Some(token);
        self
    }

    pub fn drain_timeout(mut self, d: Duration) -> Self {
        self.config.drain_timeout = Some(d);
        self
    }

    #[cfg(feature = "metrics-full")]
    pub fn record(mut self, path: impl Into<String>) -> Self {
        self.config.record_path = Some(path.into());
        self
    }

    pub async fn run(self) -> Result<(), RailscaleError> {
        let source = TcpSource::bind(&self.addr).await?;
        let matchers = self.config.matchers();
        let buffer_limits = self.config.buffer_limits();
        let cancel = self.config.cancel_token();
        let drain = self.config.drain_duration();
        let route = self.config.route.unwrap_or(Route::Dynamic);
        let error_responder: Option<Arc<dyn train_track::ErrorToBytes + Send + Sync>> =
            Some(Arc::new(HttpErrorResponder));

        #[cfg(feature = "metrics-full")]
        let recorder = self.config.record_path.map(|p| Arc::new(start_recorder(&p)));

        macro_rules! build_router {
            (tcp $addr:expr) => {{
                let mut r = TcpRouter::fixed($addr);
                if let Some(d) = self.config.inactivity_timeout {
                    r = r.with_inactivity_timeout(d);
                }
                Arc::new(r)
            }};
            (sock $path:expr) => {{
                let mut r = TcpOverSockRouter::new($path);
                if let Some(d) = self.config.inactivity_timeout {
                    r = r.with_inactivity_timeout(d);
                }
                Arc::new(r)
            }};
            (dynamic) => {{
                let mut r = TcpRouter::from_routing_key();
                if let Some(d) = self.config.inactivity_timeout {
                    r = r.with_inactivity_timeout(d);
                }
                Arc::new(r)
            }};
        }

        match route {
            Route::Tcp(addr) => {
                let p = Pipeline {
                    source,
                    parser_factory: || HttpParser::new(vec![]),
                    pipeline: Arc::new(HttpPipeline::keepalive(matchers)),
                    router: build_router!(tcp addr),
                    error_responder,
                    buffer_limits,
                    drain_timeout: drain,
                    hook_factory: || HttpDeriverHook::new(),
                    response_parser_factory: Some(|| ResponseParser::new()),
                    response_pipeline: Some(Arc::new(HttpPipeline::keepalive(vec![]))),
                    response_hook_factory: Some(|| HttpDeriverHook::new()),
                    stabling_config: Some(StablingConfig::default()),
            turnout_name: "proxy".to_string(),
            capture_dir: None,
                    #[cfg(feature = "metrics-full")]
                    recorder,
                };
                p.run(cancel).await
            }
            Route::Sock(path) => {
                let p = Pipeline {
                    source,
                    parser_factory: || HttpParser::new(vec![]),
                    pipeline: Arc::new(HttpPipeline::keepalive(matchers)),
                    router: build_router!(sock path),
                    error_responder,
                    buffer_limits,
                    drain_timeout: drain,
                    hook_factory: || HttpDeriverHook::new(),
                    response_parser_factory: Some(|| ResponseParser::new()),
                    response_pipeline: Some(Arc::new(HttpPipeline::keepalive(vec![]))),
                    response_hook_factory: Some(|| HttpDeriverHook::new()),
                    stabling_config: Some(StablingConfig::default()),
            turnout_name: "proxy".to_string(),
            capture_dir: None,
                    #[cfg(feature = "metrics-full")]
                    recorder,
                };
                p.run(cancel).await
            }
            Route::Dynamic => {
                let p = Pipeline {
                    source,
                    parser_factory: || HttpParser::new(vec![]),
                    pipeline: Arc::new(HttpPipeline::keepalive(matchers)),
                    router: build_router!(dynamic),
                    error_responder,
                    buffer_limits,
                    drain_timeout: drain,
                    hook_factory: || HttpDeriverHook::new(),
                    response_parser_factory: Some(|| ResponseParser::new()),
                    response_pipeline: Some(Arc::new(HttpPipeline::keepalive(vec![]))),
                    response_hook_factory: Some(|| HttpDeriverHook::new()),
                    stabling_config: Some(StablingConfig::default()),
            turnout_name: "proxy".to_string(),
            capture_dir: None,
                    #[cfg(feature = "metrics-full")]
                    recorder,
                };
                p.run(cancel).await
            }
        }
    }
}

pub struct SockBuilder<S = NoRoute> {
    path: String,
    config: Config,
    _state: PhantomData<S>,
}

impl<S> SockBuilder<S> {
    pub fn replace_header(mut self, name: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> Self {
        self.config.headers.push((name.into(), value.into()));
        self
    }

    pub fn max_request_bytes(mut self, n: usize) -> Self {
        self.config.max_request_bytes = Some(n);
        self
    }

    pub fn inactivity_timeout(mut self, d: Duration) -> Self {
        self.config.inactivity_timeout = Some(d);
        self
    }

    pub fn shutdown_token(mut self, token: CancellationToken) -> Self {
        self.config.shutdown_token = Some(token);
        self
    }

    pub fn drain_timeout(mut self, d: Duration) -> Self {
        self.config.drain_timeout = Some(d);
        self
    }

    #[cfg(feature = "metrics-full")]
    pub fn record(mut self, path: impl Into<String>) -> Self {
        self.config.record_path = Some(path.into());
        self
    }
}

impl SockBuilder<NoRoute> {
    fn with_route(self, route: Route) -> SockBuilder<HasRoute> {
        SockBuilder {
            path: self.path,
            config: Config { route: Some(route), ..self.config },
            _state: PhantomData,
        }
    }

    pub fn route_tcp(self, addr: impl Into<String>) -> SockBuilder<HasRoute> {
        self.with_route(Route::Tcp(addr.into()))
    }

    pub fn route_sock(self, sock_path: impl Into<String>) -> SockBuilder<HasRoute> {
        self.with_route(Route::Sock(sock_path.into()))
    }
}

impl SockBuilder<HasRoute> {
    pub async fn run(self) -> Result<(), RailscaleError> {
        let source = SockSource::bind(&self.path)?;
        let matchers = self.config.matchers();
        let buffer_limits = self.config.buffer_limits();
        let cancel = self.config.cancel_token();
        let drain = self.config.drain_duration();
        let route = self.config.route.expect("route must be set");
        let error_responder: Option<Arc<dyn train_track::ErrorToBytes + Send + Sync>> =
            Some(Arc::new(HttpErrorResponder));

        #[cfg(feature = "metrics-full")]
        let recorder = self.config.record_path.map(|p| Arc::new(start_recorder(&p)));

        match route {
            Route::Tcp(addr) => {
                let mut r = TcpRouter::fixed(addr);
                if let Some(d) = self.config.inactivity_timeout {
                    r = r.with_inactivity_timeout(d);
                }
                let p = Pipeline {
                    source,
                    parser_factory: || HttpParser::new(vec![]),
                    pipeline: Arc::new(HttpPipeline::keepalive(matchers)),
                    router: Arc::new(r),
                    error_responder,
                    buffer_limits,
                    drain_timeout: drain,
                    hook_factory: || HttpDeriverHook::new(),
                    response_parser_factory: Some(|| ResponseParser::new()),
                    response_pipeline: Some(Arc::new(HttpPipeline::keepalive(vec![]))),
                    response_hook_factory: Some(|| HttpDeriverHook::new()),
                    stabling_config: Some(StablingConfig::default()),
            turnout_name: "proxy".to_string(),
            capture_dir: None,
                    #[cfg(feature = "metrics-full")]
                    recorder,
                };
                p.run(cancel).await
            }
            Route::Sock(path) => {
                let mut r = TcpOverSockRouter::new(path);
                if let Some(d) = self.config.inactivity_timeout {
                    r = r.with_inactivity_timeout(d);
                }
                let p = Pipeline {
                    source,
                    parser_factory: || HttpParser::new(vec![]),
                    pipeline: Arc::new(HttpPipeline::keepalive(matchers)),
                    router: Arc::new(r),
                    error_responder,
                    buffer_limits,
                    drain_timeout: drain,
                    hook_factory: || HttpDeriverHook::new(),
                    response_parser_factory: Some(|| ResponseParser::new()),
                    response_pipeline: Some(Arc::new(HttpPipeline::keepalive(vec![]))),
                    response_hook_factory: Some(|| HttpDeriverHook::new()),
                    stabling_config: Some(StablingConfig::default()),
            turnout_name: "proxy".to_string(),
            capture_dir: None,
                    #[cfg(feature = "metrics-full")]
                    recorder,
                };
                p.run(cancel).await
            }
            Route::Dynamic => unreachable!(),
        }
    }
}
