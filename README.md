<p align="center">
  <img src="logo.png" alt="Railscale" width="200">
</p>

# Railscale

A scriptable, Tailscale-native network service. Lua scripts define **carriages** — independent Tailscale nodes, each with its own hostname and forwarding pipeline. Railscale reads the scripts and brings them to life on the tailnet.

## Architecture

```mermaid
graph TB
    LUA[Lua Scripts] -->|define| RS[Railscale Engine]
    RS -->|spawns| C1
    RS -->|spawns| C2
    RS -->|spawns| C3
    RS -->|spawns| DNS

    subgraph Tailnet
        C1[carriage-alpha.tailnet.ts.net]
        C2[carriage-beta.tailnet.ts.net]
        C3[carriage-gamma.tailnet.ts.net]
        DNS[dns.tailnet.ts.net]
    end

    C1 -->|forwards to| U1[upstream:8080]
    C2 -->|forwards to| U2[upstream:443]
    C3 -->|forwards to| U3[upstream:5432]
    DNS -->|resolves via| U4[upstream DNS]

    style Tailnet fill:#1a1a2e,color:#fff
    style LUA fill:#2d4a22,color:#fff
```

## Carriage Pipeline

Each carriage spawned by a Lua script runs a four-stage pipeline:

```mermaid
graph LR
    A[Ingress] -->|TCP / HTTP / TLS| B(CarriageListener)
    B -->|raw stream| C(DataFrameProducer)
    C -->|parsed frames| D(FrameConductor)
    D -->|inspected frames| E(DisembarkStrategy)
    E -->|forwarded| F[Upstream Target]
```

| Stage | Trait | Role |
|-------|-------|------|
| **Listen** | `CarriageListener` | Accept connections on this carriage's tailnet address |
| **Parse** | `DataFrameProducer` | Read and buffer frames from the connection |
| **Inspect** | `FrameConductor` | Evaluate frames — pass, reject, or transform |
| **Forward** | `DisembarkStrategy` | Write frames to the upstream destination |

## Key Concepts

- **Lua scripts** -- the source of truth. They declare carriages, their hostnames, ingress types, and forwarding targets. Railscale is the runtime that executes them.
- **Carriage** -- a self-contained Tailscale service with its own hostname and forwarding pipeline, defined in Lua
- **DNS Server** -- a separate Tailscale service for DNS forwarding, also defined in Lua
- **rsnet** -- Rust Tailscale integration; each carriage gets its own `rsnet` instance
