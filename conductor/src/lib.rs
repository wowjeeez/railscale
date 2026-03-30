use std::sync::Arc;
use bytes::Bytes;
use memchr::memmem::Finder;
use carriages::{
    HttpParser, HttpPipeline,
    TcpSource, SockSource,
    TcpRouter, TcpOverSockRouter,
};
use train_track::{Pipeline, Service, RailscaleError};

#[cfg(feature = "metrics-full")]
use train_track::recorder::start_recorder;

enum Route {
    Tcp(String),
    Sock(String),
    Dynamic,
}

struct Config {
    headers: Vec<(Vec<u8>, Vec<u8>)>,
    route: Option<Route>,
    #[cfg(feature = "metrics-full")]
    record_path: Option<String>,
}

impl Config {
    fn new() -> Self {
        Self {
            headers: Vec::new(),
            route: None,
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
}

macro_rules! builder_methods {
    () => {
        pub fn route_tcp(mut self, addr: impl Into<String>) -> Self {
            self.config.route = Some(Route::Tcp(addr.into()));
            self
        }

        pub fn route_sock(mut self, path: impl Into<String>) -> Self {
            self.config.route = Some(Route::Sock(path.into()));
            self
        }

        pub fn replace_header(mut self, name: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> Self {
            self.config.headers.push((name.into(), value.into()));
            self
        }

        #[cfg(feature = "metrics-full")]
        pub fn record(mut self, path: impl Into<String>) -> Self {
            self.config.record_path = Some(path.into());
            self
        }
    };
}

pub struct Conductor;

impl Conductor {
    pub fn tcp(addr: impl Into<String>) -> TcpBuilder {
        TcpBuilder {
            addr: addr.into(),
            config: Config::new(),
        }
    }

    pub fn sock(path: impl Into<String>) -> SockBuilder {
        SockBuilder {
            path: path.into(),
            config: Config::new(),
        }
    }
}

pub struct TcpBuilder {
    addr: String,
    config: Config,
}

impl TcpBuilder {
    builder_methods!();

    pub fn route_dynamic(mut self) -> Self {
        self.config.route = Some(Route::Dynamic);
        self
    }

    pub async fn run(self) -> Result<(), RailscaleError> {
        let source = TcpSource::bind(&self.addr).await?;
        let matchers = self.config.matchers();
        let route = self.config.route.unwrap_or(Route::Dynamic);

        #[cfg(feature = "metrics-full")]
        let recorder = self.config.record_path.map(|p| Arc::new(start_recorder(&p)));

        match route {
            Route::Tcp(addr) => {
                let p = Pipeline {
                    source,
                    parser_factory: || HttpParser::new(vec![]),
                    pipeline: Arc::new(HttpPipeline::new(matchers)),
                    router: Arc::new(TcpRouter::fixed(addr)),
                    #[cfg(feature = "metrics-full")]
                    recorder,
                };
                p.run().await
            }
            Route::Sock(path) => {
                let p = Pipeline {
                    source,
                    parser_factory: || HttpParser::new(vec![]),
                    pipeline: Arc::new(HttpPipeline::new(matchers)),
                    router: Arc::new(TcpOverSockRouter::new(path)),
                    #[cfg(feature = "metrics-full")]
                    recorder,
                };
                p.run().await
            }
            Route::Dynamic => {
                let p = Pipeline {
                    source,
                    parser_factory: || HttpParser::new(vec![]),
                    pipeline: Arc::new(HttpPipeline::new(matchers)),
                    router: Arc::new(TcpRouter::from_routing_key()),
                    #[cfg(feature = "metrics-full")]
                    recorder,
                };
                p.run().await
            }
        }
    }
}

pub struct SockBuilder {
    path: String,
    config: Config,
}

impl SockBuilder {
    builder_methods!();

    pub async fn run(self) -> Result<(), RailscaleError> {
        let source = SockSource::bind(&self.path)?;
        let matchers = self.config.matchers();
        let route = self.config.route.expect("no route configured — call .route_tcp() or .route_sock()");

        #[cfg(feature = "metrics-full")]
        let recorder = self.config.record_path.map(|p| Arc::new(start_recorder(&p)));

        match route {
            Route::Tcp(addr) => {
                let p = Pipeline {
                    source,
                    parser_factory: || HttpParser::new(vec![]),
                    pipeline: Arc::new(HttpPipeline::new(matchers)),
                    router: Arc::new(TcpRouter::fixed(addr)),
                    #[cfg(feature = "metrics-full")]
                    recorder,
                };
                p.run().await
            }
            Route::Sock(path) => {
                let p = Pipeline {
                    source,
                    parser_factory: || HttpParser::new(vec![]),
                    pipeline: Arc::new(HttpPipeline::new(matchers)),
                    router: Arc::new(TcpOverSockRouter::new(path)),
                    #[cfg(feature = "metrics-full")]
                    recorder,
                };
                p.run().await
            }
            Route::Dynamic => panic!("route_dynamic() is only available on TcpBuilder"),
        }
    }
}
