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

### Railscale mental model:

Everything in railscale is named after railway components. The proxy models itself as a train network — traffic is cargo moving through tracks, switches, and carriages.

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

