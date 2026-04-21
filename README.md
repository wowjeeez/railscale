# Railscale

<p align="center">
  <img src="clean.png" alt="Railscale">
</p>

Insanely fast (really) streaming proxy/dns/packet capture/maybe even firewall service.
For now it supports http header remapping and routing.
Everything is named after railway things
- Very WIP

# Goals
- HTTP 1.0, HTTP1.1, HTTP2, HTTP3 support
- Multiplexed streams
- Keepalive support
- Load Balancing capabilities (reverse too)
- DNS over TLS
- TLS 1.3 support
- DynDNS
- Fully erasable types, 0 overhead abstractions
- Correctness
- Comfortable tailscale integration

# Normal usage
- Custom DNS
- Reverse proxy
- Packet capture/logging
- request matchers
- Basic firewall
- dev/tun mode maybe idk

# Metrics & Tracing
- OTEL
- Logging features

# Benchmarking
- Run the shell script

# Architecture
Railscale is very complex, (mainly because I wanted the entire network layer to be statically monomorphic) compared to what one would expect from a proxy. The reason being is that railscale doesnt buffer requests in memory, and is tuned for maximum performance.
The architecture is also designed to be extensible for basically any network related application.

**(And now claude also fucked it up a bit)**

### Railscale mental model

Everything in railscale is named after railway components. The proxy models itself as a train network — traffic is cargo moving through tracks, switches, and carriages.

## How it works

A request flows through the system like cargo through a rail network:

### Request path

```mermaid
flowchart TD
    Client([Client]) -->|TCP| SS["StreamSource\n<i>Station entrance</i>"]
    SS -->|raw bytes| FP["FrameParser\n<i>Cargo inspector</i>"]
    FP -->|typed Frames| CH["ConnectionHook\n<i>Checkpoint officer</i>"]
    CH -->|observed frames| FPL["FramePipeline\n<i>Assembly line</i>"]
    FPL --> RK{routing_key\nfound?}
    RK -->|new connection| DR["DestinationRouter\n<i>Route dispatcher</i>"]
    RK -->|pooled connection| ST["Stabling\n<i>Train depot</i>"]
    DR --> SD["StreamDestination\n<i>Terminal station</i>"]
    ST --> SD

    style SS fill:#2d4a2d,stroke:#4a7a4a
    style FP fill:#2d3a4a,stroke:#4a6a8a
    style CH fill:#4a3a2d,stroke:#8a6a4a
    style FPL fill:#2d3a4a,stroke:#4a6a8a
    style DR fill:#3a2d4a,stroke:#6a4a8a
    style ST fill:#3a2d4a,stroke:#6a4a8a
    style SD fill:#2d4a2d,stroke:#4a7a4a
```

**Concrete example (HTTP reverse proxy):**
- `StreamSource` → `TcpSource` listening on `:8080`
- `FrameParser` → `HttpParser` emits `HttpFrame` per request line, header, body chunk
- `ConnectionHook` → `HttpDeriverHook` extracts HTTP version, body framing, detects CL-TE smuggling
- `FramePipeline` → `HttpPipeline` strips hop-by-hop headers (Connection, TE, Upgrade...)
- `DestinationRouter` → `TcpRouter` connects to `10.0.0.5:80`
- `Stabling` → reuses pooled upstream connections

### Response path

```mermaid
flowchart TD
    SD["StreamDestination"] -->|upstream bytes| RP["ResponseParser\n<i>Return cargo inspector</i>"]
    RP -->|response frames| RPL["ResponsePipeline\n<i>Return assembly line</i>"]
    RPL --> BW["BatchWriter\n<i>Loading dock — 32KB chunks</i>"]
    BW -->|TCP| Client([Client])
    Client --> KA{keep-alive?}
    KA -->|yes| ST["Stabling\n<i>pool connection, loop</i>"]
    KA -->|no| CL["Close\n<i>teardown</i>"]

    style RP fill:#2d3a4a,stroke:#4a6a8a
    style RPL fill:#2d3a4a,stroke:#4a6a8a
    style BW fill:#4a3a2d,stroke:#8a6a4a
    style ST fill:#3a2d4a,stroke:#6a4a8a
```

### Abstraction map

```mermaid
flowchart LR
    subgraph Orchestration
        P["Pipeline\n<i>Src, Par, Pip, Rtr, Hook, RPar</i>"]
    end

    subgraph "Frame transforms"
        SR["SwitchRail\n<i>Frame → Frame</i>\nsync transform\n\ne.g. IdentityRail"]
        T["Turnout\n<i>Frame → Option&lt;Frame&gt;</i>\nfilter + switch\n\ne.g. SimpleTurnout"]
        SR -.->|used by| T
    end

    subgraph Routing
        SH["Shunt\n<i>routing_key → Departure</i>\nasync connect\n\ne.g. RouterShunt"]
    end

    subgraph Transport
        DEP["Departure\n<i>Bytes → async send</i>\n\ne.g. StreamDeparture"]
        TL["Transload\n<i>Frame → Channel</i>\nside output\n\ne.g. ChannelTransload"]
        SHU["Shuttle\n<i>bidirectional link</i>\ntwo-way comms\n\ne.g. ShuttleLink"]
    end

    subgraph "Derive system"
        DF["DerivationFormula\n<i>Inspection rules</i>"] --> DS["DeriverSession\n<i>Accumulator</i>"]
        DS --> DE["DerivedEffect\n<i>Decision</i>"]
    end

    style SR fill:#2d4a2d,stroke:#4a7a4a
    style T fill:#2d4a2d,stroke:#4a7a4a
    style SH fill:#3a2d4a,stroke:#6a4a8a
    style DEP fill:#4a3a2d,stroke:#8a6a4a
    style TL fill:#4a3a2d,stroke:#8a6a4a
    style SHU fill:#4a3a2d,stroke:#8a6a4a
    style DF fill:#2d3a4a,stroke:#4a6a8a
    style DS fill:#2d3a4a,stroke:#4a6a8a
    style DE fill:#2d3a4a,stroke:#4a6a8a
```

### Derive system detail

```mermaid
flowchart LR
    M1["HeaderName\n('content-length')"] -->|"'42'"| S["DeriverSession"]
    M2["HeaderName\n('transfer-encoding')"] -->|"'chunked'"| S
    M3["RequestLineVersion"] -->|"'HTTP/1.1'"| S
    S --> E["DerivedEffect"]
    E --> BF["body_framing:\nChunked | ContentLength | UntilClose"]
    E --> CM["connection:\nKeepAlive | Close"]
    E --> CF["conflicts:\nCL+TE → reject 400"]

    style S fill:#2d3a4a,stroke:#4a6a8a
    style E fill:#4a3a2d,stroke:#8a6a4a
```

### Composing a proxy

There are three levels of abstraction for building a proxy, from high to low:

#### Level 1: Conductor (highest level)

```rust
// Simple reverse proxy with header replacement
Conductor::tcp("0.0.0.0:8080")
    .route_tcp("backend.internal:3000")
    .replace_header("User-Agent", "railscale/1.0")
    .max_request_bytes(16 * 1024 * 1024)
    .inactivity_timeout(Duration::from_secs(30))
    .run(cancel_token)
    .await?;

// Dynamic routing (routes to Host header value)
Conductor::tcp("0.0.0.0:8080")
    .route_dynamic()
    .run(cancel_token)
    .await?;

// Unix socket → TCP
Conductor::sock("/tmp/railscale.sock")
    .route_tcp("127.0.0.1:9090")
    .run(cancel_token)
    .await?;
```

#### Level 2: Coupler flows (protocol-aware)

```mermaid
flowchart LR
    subgraph ForwardHttp
        H1[HTTP] -->|OverTcp| H2[HTTP]
    end
    subgraph ForwardHttps
        S1[HTTPS] -->|OverTls| S2[HTTPS]
    end
    subgraph ForwardTls
        T1[TLS] -->|passthrough| T2[TLS]
    end
    subgraph ForwardHttpToHttps
        U1[HTTP] -->|OverTls| U2[HTTPS]
    end
    subgraph ForwardHttpsToHttp
        D1[HTTPS] -->|OverTcp| D2[HTTP]
    end
```

```rust
// HTTP → HTTP
ForwardHttp::builder()
    .bind("0.0.0.0:8080")
    .upstream("backend:3000")
    .buffer_limits(BufferLimits {
        max_pre_route_bytes: 10 * 1024 * 1024,
        max_post_route_bytes: 10 * 1024 * 1024,
    })
    .build().await?
    .run(cancel_token).await?;

// HTTPS → HTTPS (TLS termination + re-encryption)
ForwardHttps::builder()
    .bind("0.0.0.0:443")
    .upstream("backend:443")
    .tls_config(server_tls_config)
    .build().await?
    .run(cancel_token).await?;

// HTTP → HTTPS (upgrade)
ForwardHttpToHttps::new("0.0.0.0:80", "secure-backend:443").await?
    .run(cancel_token).await?;

// TLS passthrough (no decryption, SNI-based routing)
ForwardTls::new("0.0.0.0:443", "backend:443").await?
    .run(cancel_token).await?;
```

#### Level 3: Pipeline (full control)

Every higher-level API ultimately builds a `Pipeline` struct:

```rust
let pipeline = Pipeline {
    // Source: where connections come from
    source: TcpSource::bind("127.0.0.1:8080").await?,

    // Parser: how to parse raw bytes into frames
    parser_factory: || HttpParser::new(vec![]),

    // Pipeline: how to transform frames in-flight
    pipeline: Arc::new(HttpPipeline::new(vec![
        (Finder::new(b"User-Agent"), Bytes::from_static(b"railscale/1.0")),
        (Finder::new(b"X-Real-IP"), Bytes::from_static(b"10.0.0.1")),
    ])),

    // Router: where to send traffic
    router: Arc::new(TcpRouter::fixed("backend:3000")),

    // Hook: observe frames for validation (smuggling detection etc.)
    hook_factory: || HttpDeriverHook::new(),

    // Response handling (optional — None means raw passthrough)
    response_parser_factory: Some(|| ResponseParser::new()),
    response_pipeline: Some(Arc::new(HttpPipeline::keepalive(vec![]))),
    response_hook_factory: Some(|| HttpDeriverHook::new()),

    // Connection pooling
    stabling_config: Some(StablingConfig {
        max_idle_per_host: 16,
        max_total_idle: 128,
        idle_timeout: Duration::from_secs(120),
        enabled: true,
    }),

    // Error handling
    error_responder: Some(Arc::new(HttpErrorResponder)),
    buffer_limits: BufferLimits::default(),
    drain_timeout: Duration::from_secs(30),

    turnout_name: "proxy".to_string(),
    capture_dir: None,
};

pipeline.run(cancel_token).await?;
```

### Individual components

#### SwitchRail — frame-to-frame transform

```rust
// SwitchRail is a sync transform: Frame in → Frame out
pub trait SwitchRail: Send + Sync {
    type Input: Frame;
    type Output: Frame;
    fn switch(&self, input: Self::Input) -> Self::Output;
}

// Built-in: no-op pass-through
let rail = IdentityRail::<HttpFrame>::new();
let output = rail.switch(input); // unchanged

// Custom: add a header to every request
struct AddHeaderRail {
    name: Bytes,
    value: Bytes,
}

impl SwitchRail for AddHeaderRail {
    type Input = HttpFrame;
    type Output = HttpFrame;
    fn switch(&self, input: HttpFrame) -> HttpFrame {
        // inject header frame into stream
    }
}
```

#### Turnout — filter + transform

```rust
// Turnout wraps a SwitchRail and can drop frames (returns Option)
pub trait Turnout: Send + Sync {
    type Input: Frame;
    type Output: Frame;
    fn process(&self, input: Self::Input) -> Option<Self::Output>;
}

// SimpleTurnout = SwitchRail + filter closure
let turnout = SimpleTurnout::new(
    IdentityRail::new(),
    |frame: HttpFrame| {
        // drop frames matching a condition
        if frame.as_bytes().starts_with(b"X-Internal") {
            None // filtered out
        } else {
            Some(frame) // pass through
        }
    },
);

// FramePipelineAdapter = wrap any FramePipeline as a Turnout
let turnout = FramePipelineAdapter::new(
    HttpPipeline::new(vec![
        (Finder::new(b"Host"), Bytes::from_static(b"rewritten.host")),
    ])
);
```

#### ShuttleLink — chain turnouts

```rust
// ShuttleLink chains two Turnouts: A → B
let first = FramePipelineAdapter::new(HttpPipeline::new(header_matchers));
let second = FramePipelineAdapter::new(HttpPipeline::new(more_matchers));

let chain = ShuttleLink::new(first, second);
let output = chain.process(input)?; // runs through both
```

#### Shunt — connect to a destination

```rust
// Shunt creates an outbound connection from a routing key
#[async_trait]
pub trait Shunt: Send + Sync {
    type Input: Frame;
    type Departure: Departure;
    async fn connect(&self, routing_key: &[u8]) -> Result<Self::Departure, RailscaleError>;
}

// RouterShunt wraps any DestinationRouter
let shunt = RouterShunt::new(TcpRouter::fixed("backend:3000"));
let departure = shunt.connect(b"example.com").await?;
```

#### Departure & Transload — send data out

```rust
// Departure: async send bytes to a destination
#[async_trait]
pub trait Departure: Send {
    async fn depart(&mut self, bytes: Bytes) -> Result<(), Self::Error>;
    fn response_reader(&mut self) -> &mut Self::ResponseReader;
}

// StreamDeparture wraps any StreamDestination
let mut dep = StreamDeparture::new(TcpDestination::connect("backend:3000").await?);
dep.depart(request_bytes).await?;
let response = dep.response_reader(); // read upstream response

// ChannelTransload: send bytes to a channel (no response, for side output)
let (tx, mut rx) = tokio::sync::mpsc::channel(100);
let mut tap = ChannelTransload::new(tx);
tap.depart(captured_bytes).await?;
// elsewhere: rx.recv().await to consume
```

#### Stabling — connection pooling

```rust
let stabling = Stabling::new(StablingConfig {
    max_idle_per_host: 8,
    max_total_idle: 64,
    idle_timeout: Duration::from_secs(90),
    enabled: true,
});

// Try to reuse a pooled connection
if let Some(dest) = stabling.acquire(b"backend:3000") {
    // reuse existing TcpStream
} else {
    // create new connection via router
}

// After request completes, return connection to pool
stabling.release(Bytes::from("backend:3000"), dest);
```

#### ConnectionHook — observe & validate

```rust
pub trait ConnectionHook<F: Frame>: Send + 'static {
    fn on_frame(&mut self, frame: &F);                     // observe each frame
    fn validate(&self) -> Result<(), RailscaleError>;      // check after headers
    fn should_close_connection(&self) -> bool { false }     // keep-alive decision
    fn reset(&mut self) {}                                  // reset between requests
}

// NoHook: no validation
let pipeline = Pipeline { hook_factory: || NoHook, .. };

// HttpDeriverHook: full HTTP validation
// - detects CL-TE smuggling → 400
// - detects duplicate Content-Length conflicts → 400
// - determines keep-alive vs close
let pipeline = Pipeline { hook_factory: || HttpDeriverHook::new(), .. };
```

#### Custom DestinationRouter

```rust
// Route to different backends based on request path
struct PathRouter;

#[async_trait]
impl DestinationRouter for PathRouter {
    type Destination = TcpDestination;
    async fn route(&self, routing_key: &[u8]) -> Result<TcpDestination, RailscaleError> {
        let key = std::str::from_utf8(routing_key).unwrap_or("");
        let upstream = if key.starts_with("/api") {
            "api-backend:3000"
        } else {
            "web-backend:8080"
        };
        TcpDestination::connect(upstream).await
    }
}

// Route to a file (for logging/capture)
struct FileRouter;

#[async_trait]
impl DestinationRouter for FileRouter {
    type Destination = FileDestination;
    async fn route(&self, _key: &[u8]) -> Result<FileDestination, RailscaleError> {
        FileDestination::new(PathBuf::from("captured.bin")).map_err(Into::into)
    }
}
```

## Glossary

### Crates

| Name | Train meaning | Role |
|------|--------------|------|
| **railscale** | Rail + scale | The workspace — a scalable rail network |
| **train_track** | The track/rails | Core abstractions that everything runs on |
| **carriage** | Passenger/freight car | HTTP protocol (codec, parser, pipeline) + TCP transport |
| **trezorcarriage** | Armored vault car (trezor = safe 🇭🇺) | TLS handling — parsing, passthrough, termination |
| **conductor** | Train conductor | Orchestrator — assembles components into a running proxy |
| **coupler** | Device connecting cars | Joins protocols together (HTTP↔HTTPS, TCP↔TLS, etc.) |
| **bogie** | Wheeled truck under a car | Test harness + benchmarks |

### Core concepts (train_track)

| Name | Train meaning | Role |
|------|--------------|------|
| **Frame** | Structural skeleton of a car | Core data unit flowing through the system |
| **FrameParser** | Cargo inspector | Parses raw bytes into typed frames |
| **FramePipeline** | Assembly line on the railway | Processes frames through a sequence of transformations |
| **Pipeline** | The full transport route | High-level source → parse → route → destination config |
| **Service** | Scheduled train service | Main abstraction for running a proxy |
| **Turnout** | Track switch/junction | Routes frames to different destinations based on conditions |
| **SwitchRail** | Movable rail in a turnout | Trait for routing/switching decisions |
| **IdentityRail** | Straight track (no switch) | No-op pass-through rail |
| **Shunt** | Moving a car to another track | Routes frames with optional transformation |
| **RouterShunt** | Shunting with a route plan | Shunt implementation using a router |
| **Shuttle** | Back-and-forth train service | Bidirectional link for sending frames both ways |
| **ShuttleLink** | Connection for shuttle service | Channel pair for bidirectional communication |
| **Stabling** | Parking trains in a depot | Connection pooling/reuse |
| **Departure** | Train leaving the station | Outgoing data stream to a destination |
| **Transload** | Cargo transfer between trains | Transfer of data between formats/systems |
| **StreamSource** | Origin station | Incoming stream of connections |
| **StreamDestination** | Terminal station | Outgoing stream endpoint |
| **DestinationRouter** | Route dispatcher | Routes frames to the correct destination |
| **ConnectionHook** | Coupling point | Intercepts connection lifecycle events |

### Data model

| Name | Train meaning | Role |
|------|--------------|------|
| **FramePhase** | Phase of the journey | Processing stage a frame is in |
| **PhasedFrame** | Car at a station | Frame tagged with its current phase |
| **PhasedBuffer** | Staging yard | Buffered frames organized by phase |
| **RawFrame** | Uninspected cargo | Unprocessed frame data |
| **ParsedData** | Inspected/sorted cargo | Frame data after parsing |
| **MatchAtom** | Cargo label | Smallest matchable unit in a frame |
| **DerivedEffect** | Routing decision from cargo inspection | Effect derived from frame analysis |
| **DerivationFormula** | Inspection rulebook | Formula for deriving effects from frames |

### HTTP (carriage)

| Name | Train meaning | Role |
|------|--------------|------|
| **HttpFrame** | HTTP cargo car | HTTP-specific frame representation |
| **HttpPhase** | HTTP journey stage | HTTP processing phases |
| **HttpStreamingCodec** | Cargo encoder/decoder | Encodes/decodes HTTP streams |
| **HttpParser** | HTTP cargo inspector | Parses HTTP protocol into frames |
| **HttpPipeline** | HTTP assembly line | Pipeline for HTTP frame processing |
| **HttpTurnout** | HTTP track switch | HTTP-specific routing |
| **HttpErrorResponder** | Error signal | Converts HTTP errors into responses |

### TLS (trezorcarriage)

| Name | Train meaning | Role |
|------|--------------|------|
| **TlsEncryptedFrame** | Sealed cargo | TLS-encrypted frame |
| **TlsParser** | TLS cargo inspector | Parses TLS record frames |
| **TlsPassthroughPipeline** | Sealed cargo express lane | Forwards TLS frames without decryption |
| **TlsPassthroughTurnout** | Sealed cargo switch | Routes TLS passthrough traffic |
| **TlsTerminationRail** | End-of-line track | Rail for TLS termination |
| **TlsSource** | Secure origin station | TLS connection source |
| **TlsStreamDestination** | Secure terminal station | TLS stream endpoint |
| **TlsClientDestination** | Client-side secure station | Outbound TLS connection to upstream |
| **TlsRouter** | Secure route dispatcher | Routes TLS connections |

### Forwarding (coupler)

| Name | Train meaning | Role |
|------|--------------|------|
| **OverTcp** | Shunt via main line | Shunt over TCP |
| **OverTls** | Shunt via secure line | Shunt over TLS |
| **OverUnix** | Shunt via depot track | Shunt over Unix socket |
| **ForwardHttp** | Send cargo via main line | Forward HTTP traffic |
| **ForwardHttps** | Send cargo via secure line | Forward HTTPS traffic |
| **ForwardTls** | Send sealed cargo | Forward raw TLS traffic |
| **ForwardHttpToHttps** | Upgrade cargo security | HTTP → HTTPS forwarding |
| **ForwardHttpsToHttp** | Downgrade cargo security | HTTPS → HTTP forwarding |

### Orchestration (conductor)

| Name | Train meaning | Role |
|------|--------------|------|
| **Conductor** | The conductor | Main API for building and running proxies |
| **TcpBuilder** | Main line builder | Builds TCP-based proxy configurations |
| **SockBuilder** | Depot track builder | Builds Unix socket-based configurations |

### Testing (bogie)

| Name | Train meaning | Role |
|------|--------------|------|
| **Harness** | Test rig equipment | Integration test harness |
| **Generators** | Cargo generators | Test data generators |

# Unorthodox networks
<p align="center">
  <img src="chaotic.png" alt="Chatoic">
</p>

- Bypass firewalls & ISP/gov restrictions
- Bypass strict corporate network policies & NAT
- Rotate proxies
- Posture spoofing
- SSH & RDP where they are explicitly forbidden
- Tor mesh
- NTLM gateway
- Interconnect multiple corporate machines conveniently
- Strict networks? Not when YOU ARE THE NETWORK
- Virtualization & usage in restricted/monitored environments
- *more cool shit here*
- UDP wireguard peering to tailscale

