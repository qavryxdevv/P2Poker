# rust-libp2p 0.56.0 — ground truth for p2p-poker

Research phase 0. Every claim below carries a **Verification** line stating either
(a) *compiled* — code using it passed `cargo check` / `cargo run`, or
(b) *source* — read in the unpacked crate under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/<crate>-<version>/`.

docs.rs was **not** used as evidence anywhere in this document.

Toolchain used for all probes:

```
rustc 1.95.0 (59807616e 2026-04-14)
cargo 1.95.0 (f2d3ce0bd 2026-03-21)
host: x86_64-pc-windows-msvc
```

Probe crates (outside the repo):

| Probe | Path | Purpose |
| --- | --- | --- |
| `probe-libp2p` | `…/scratchpad/probe-libp2p/` | full poker behaviour stack, TCP+QUIC |
| `probe-libp2p-stream` | `…/scratchpad/probe-libp2p-stream/` | `libp2p-stream` + `request-response/cbor` |
| `probe-quic-only` | `…/scratchpad/probe-quic-only/` | QUIC-only stack, no `tcp` feature |

(`…/scratchpad/` = `<scratchpad>`)

---

## 1. Cargo features that actually exist in libp2p 0.56.0

`libp2p` 0.56.0 was published 2025-06-27 and is the newest non-yanked release.
`rust-version = "1.83.0"`, `edition = "2021"`.

The complete `[features]` list, verbatim from the crate manifest:

```
autonat, cbor, dcutr, dns, ecdsa, ed25519, floodsub, full, gossipsub, identify,
json, kad, macros, mdns, memory-connection-limits, metrics, noise, ping,
plaintext, pnet, quic, relay, rendezvous, request-response, rsa, secp256k1,
serde, tcp, tls, tokio, uds, upnp, wasm-bindgen, webrtc-websys, websocket,
websocket-websys, webtransport-websys, yamux
```

**Verification:** source — `libp2p-0.56.0/Cargo.toml`, `[features]` block. Independently
re-emitted by cargo itself in the negative test below.

### Verdict on the requested list

| Requested feature | Exists? | Note |
| --- | --- | --- |
| `quic` | **yes** | `dep:libp2p-quic`, `cfg(not(wasm32))` only |
| `noise` | **yes** | `dep:libp2p-noise` |
| `tls` | **yes** | `dep:libp2p-tls`, `cfg(not(wasm32))` only |
| `gossipsub` | **yes** | also wires `libp2p-metrics?/gossipsub` |
| `kad` | **yes** | also wires `libp2p-metrics?/kad` |
| `autonat` | **yes** | pulls libp2p-autonat with **default features `v1` + `v2`** |
| `dcutr` | **yes** | also wires `libp2p-metrics?/dcutr` |
| `relay` | **yes** | both server and client sides in one crate |
| `identify` | **yes** | |
| `ping` | **yes** | |
| `request-response` | **yes** | codecs behind extra `cbor` / `json` features |
| `macros` | **yes** | = `libp2p-swarm/macros`, gives `#[derive(NetworkBehaviour)]` |
| `tokio` | **yes** | fans out to swarm/mdns/tcp/dns/quic/upnp `?/tokio` |
| `dns` | **yes** | `cfg(not(wasm32))` only |
| `yamux` | **yes** | |
| `upnp` | **yes** | `cfg(not(wasm32))` only |
| `memory-connection-limits` | **yes** | `cfg(not(wasm32))` only |
| `metrics` | **yes** | Prometheus, `libp2p-metrics` |
| **`connection-limits`** | **NO — DOES NOT EXIST** | see below |

### The one that does not exist: `connection-limits`

There is no `connection-limits` cargo feature. `libp2p-connection-limits` is a
**non-optional** dependency of the umbrella crate, so `libp2p::connection_limits`
is always available with no feature gate at all:

```toml
[dependencies.libp2p-connection-limits]
version = "0.6.0"          # note: no `optional = true`
```

```rust
// libp2p-0.56.0/src/lib.rs:43 — no #[cfg(feature = ...)] above it
pub use libp2p_connection_limits as connection_limits;
```

Asking for it is a hard resolver error:

```
$ cargo check
error: failed to select a version for `libp2p`.
package `probe-libp2p` depends on `libp2p` with feature `connection-limits`
but `libp2p` does not have that feature.
 available features: autonat, cbor, dcutr, dns, ecdsa, ed25519, floodsub, full,
 gossipsub, identify, json, kad, macros, mdns, memory-connection-limits, metrics,
 noise, ping, plaintext, pnet, quic, relay, rendezvous, request-response, rsa,
 secp256k1, serde, tcp, tls, tokio, uds, upnp, wasm-bindgen, webrtc-websys,
 websocket, websocket-websys, webtransport-websys, yamux
```

`libp2p-allow-block-list` (→ `libp2p::allow_block_list`) is likewise non-optional
and always present — useful for us when banning a peer proven to have equivocated
(SPEC §14).

**Verification:** compiled — the error above is real cargo output from `probe-libp2p`;
plus source — `libp2p-0.56.0/Cargo.toml` and `src/lib.rs:38,43`.

> Trap for later: `memory-connection-limits` (with hyphens) is a feature,
> `connection-limits` is not, and the two module paths are
> `libp2p::memory_connection_limits` and `libp2p::connection_limits`. Both exist;
> only the first needs a feature.

---

## 2. The real `SwarmBuilder` API in 0.56, with a compiling stack

`SwarmBuilder` is a type-state builder. The phase order is fixed and each phase
carries "shortcut" methods that silently skip the phases in between:

```
IdentityPhase
  → with_new_identity() | with_existing_identity(Keypair)
ProviderPhase
  → with_tokio()                                  [feature = "tokio"]
TcpPhase
  → with_tcp(tcp::Config, sec_upgrade, mux_upgrade) -> Result<_, SecUpgrade::Error>
  → (shortcut) with_quic() / with_quic_config(..)  — implies without_tcp()
QuicPhase<T>
  → with_quic() | with_quic_config(|libp2p_quic::Config| -> Config)
OtherTransportPhase<T>
  → with_other_transport(..) | with_dns()? | with_dns_config(cfg, opts)
DnsPhase<T>                                       [feature = "dns"]
  → with_dns() -> Result<_, std::io::Error>
WebsocketPhase<T>
RelayPhase<T>                                     [feature = "relay"]
  → with_relay_client(sec_upgrade, mux_upgrade) -> Result<_, _>
BandwidthMetricsPhase<T, R>
  → with_bandwidth_metrics(&mut Registry) | without_bandwidth_metrics()
BehaviourPhase<T, R>
  → with_behaviour(|&Keypair| -> R)                        when R = NoRelayBehaviour
  → with_behaviour(|&Keypair, relay::client::Behaviour| -> R)  when relay client used
SwarmPhase<T, B>
  → with_swarm_config(|libp2p_swarm::Config| -> Config)
  → (shortcut) build()
BuildPhase<T, B>
  → with_connection_timeout(Duration)   // default 10s
  → build() -> Swarm<B>
```

Two details that are easy to get wrong:

* `with_tcp` / `with_relay_client` take **function pointers, not values**:
  `libp2p_yamux::Config::default`, not `libp2p_yamux::Config::default()`.
  A tuple `(tls::Config::new, noise::Config::new)` means "offer both, negotiate".
* Once you use `with_relay_client`, the `with_behaviour` closure gains a **second
  argument** — the ready-made `relay::client::Behaviour`. You must store it in your
  behaviour struct or the relay transport is dead.

**Verification:** source — `libp2p-0.56.0/src/builder/phase/{identity,provider,tcp,quic,other_transport,dns,relay,bandwidth_metrics,behaviour,swarm,build}.rs`; and compiled, below.

### The probe that compiles (excerpt, `probe-libp2p/src/main.rs`)

```rust
use std::{collections::hash_map::DefaultHasher, hash::{Hash, Hasher as _}, time::Duration};
use libp2p::{
    autonat, connection_limits, dcutr, gossipsub, identify, identity, kad,
    kad::store::MemoryStore, multiaddr::Protocol, noise, ping, relay,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, tls, yamux, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};

#[derive(NetworkBehaviour)]
pub struct PokerBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub kademlia: kad::Behaviour<MemoryStore>,
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
    pub autonat_client: autonat::v2::client::Behaviour,
    pub autonat_server: autonat::v2::server::Behaviour,
    pub dcutr: dcutr::Behaviour,
    pub relay_client: relay::client::Behaviour,
    pub relay_server: relay::Behaviour,
    /// `connection_limits` has NO cargo feature: it is always compiled into `libp2p`.
    pub conn_limits: connection_limits::Behaviour,
    /// This one DOES require the `memory-connection-limits` feature.
    pub mem_limits: libp2p::memory_connection_limits::Behaviour,
    /// IGD/UPnP port mapping; `upnp` feature. Note the `tokio` sub-module.
    pub upnp: libp2p::upnp::tokio::Behaviour,
}

pub fn build_swarm(
    keypair: identity::Keypair,
) -> Result<Swarm<PokerBehaviour>, Box<dyn std::error::Error>> {
    let local_peer_id = PeerId::from(keypair.public());

    let swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            (tls::Config::new, noise::Config::new),   // function pointers!
            yamux::Config::default,
        )?
        .with_quic_config(|mut cfg| {
            cfg.handshake_timeout = Duration::from_secs(10);
            cfg.max_idle_timeout = 30_000;            // u32 milliseconds
            cfg.keep_alive_interval = Duration::from_secs(5);
            cfg
        })
        .with_dns()?
        .with_relay_client(
            (tls::Config::new, noise::Config::new),
            yamux::Config::default,
        )?
        .with_behaviour(|key, relay_client| {         // <-- 2 args because of relay
            let message_id_fn = |message: &gossipsub::Message| {
                let mut s = DefaultHasher::new();
                message.data.hash(&mut s);
                message.topic.hash(&mut s);
                gossipsub::MessageId::from(s.finish().to_be_bytes())
            };
            let gs_config = gossipsub::ConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(1))
                .validation_mode(gossipsub::ValidationMode::Strict)
                .validate_messages()
                .message_id_fn(message_id_fn)
                .max_transmit_size(256 * 1024)
                .mesh_n(8).mesh_n_low(6).mesh_n_high(12).mesh_outbound_min(3)
                .duplicate_cache_time(Duration::from_secs(120))
                .flood_publish(false)
                .build()?;
            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()), gs_config)?;

            let mut kad_cfg = kad::Config::new(StreamProtocol::new("/p2p-poker/kad/1"));
            kad_cfg.set_query_timeout(Duration::from_secs(60));
            let kademlia = kad::Behaviour::with_config(
                local_peer_id, MemoryStore::new(local_peer_id), kad_cfg);

            let identify = identify::Behaviour::new(
                identify::Config::new("/p2p-poker/1".into(), key.public())
                    .with_agent_version(format!("p2p-poker/{}", env!("CARGO_PKG_VERSION")))
                    .with_push_listen_addr_updates(true),
            );

            Ok(PokerBehaviour {
                gossipsub, kademlia, identify,
                ping: ping::Behaviour::new(ping::Config::new()),
                autonat_client: autonat::v2::client::Behaviour::new(
                    rand::rngs::OsRng,
                    autonat::v2::client::Config::default()
                        .with_probe_interval(Duration::from_secs(30))
                        .with_max_candidates(8),
                ),
                autonat_server: autonat::v2::server::Behaviour::new(rand::rngs::OsRng),
                dcutr: dcutr::Behaviour::new(local_peer_id),
                relay_client,
                relay_server: relay::Behaviour::new(local_peer_id, relay::Config::default()),
                conn_limits: connection_limits::Behaviour::new(
                    connection_limits::ConnectionLimits::default()
                        .with_max_pending_incoming(Some(32))
                        .with_max_established_incoming(Some(256))
                        .with_max_established_per_peer(Some(2)),
                ),
                mem_limits: libp2p::memory_connection_limits::Behaviour::with_max_percentage(0.25),
                upnp: libp2p::upnp::tokio::Behaviour::default(),
            })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    Ok(swarm)
}
```

Exact `cargo check` output:

```
$ cargo check
    Checking probe-libp2p v0.1.0 (<scratchpad>)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.72s
```

Zero warnings, zero errors. And it actually runs and binds both transports:

```
$ cargo run
circuit dial addr: /ip4/203.0.113.7/udp/4001/quic-v1/p2p/12D3KooWDpJ7…/p2p-circuit/p2p/12D3KooWAhuY…
reservation listen addr: /ip4/203.0.113.7/udp/4001/quic-v1/p2p/12D3KooWDpJ7…/p2p-circuit
listen /ip4/192.168.1.21/tcp/18811
listen /ip4/127.0.0.1/tcp/18811
listen /ip4/172.20.160.1/tcp/18811
listen /ip4/192.168.1.21/udp/56534/quic-v1
```

**Verification:** compiled + executed.

### `#[derive(NetworkBehaviour)]` generated names

The derive generates `<StructName>Event` (here `PokerBehaviourEvent`) with one
variant per field, named in **PascalCase of the field name**: field `relay_client`
→ variant `RelayClient`, field `autonat_client` → `AutonatClient`. Matching:

```rust
SwarmEvent::Behaviour(PokerBehaviourEvent::Dcutr(dcutr::Event { remote_peer_id, result })) => ...
SwarmEvent::Behaviour(PokerBehaviourEvent::RelayClient(
    relay::client::Event::ReservationReqAccepted { relay_peer_id, renewal, limit })) => ...
```

**Verification:** compiled (both patterns are in `probe-libp2p/src/main.rs`).

### Persistent identity (SPEC §3)

`libp2p::identity::Keypair` has `generate_ed25519()`, `to_protobuf_encoding() ->
Result<Vec<u8>, DecodingError>` and `from_protobuf_encoding(&[u8])`. That protobuf
blob is what we persist to disk to get a stable `PeerId` across restarts.
`ed25519_from_bytes(impl AsMut<[u8]>)` exists if we prefer raw 32-byte seeds.

**Verification:** source — `libp2p-identity-0.2.14/src/keypair.rs:103,168,215,257`.

---

## 3. AutoNAT — 0.56 ships **both** v1 and v2

`libp2p-autonat` 0.15.0. Its own manifest:

```toml
[features]
default = ["v1", "v2"]
v1 = ["dep:libp2p-request-response", "dep:web-time", "dep:async-trait"]
v2 = ["dep:either", "dep:futures-bounded", "dep:thiserror", "dep:rand_core"]
```

The umbrella declares `libp2p-autonat` **without** `default-features = false`, so the
single `autonat` feature on `libp2p` gives you **v1 and v2 at once**.
**v2 needs no separate feature.**

```rust
// libp2p-autonat-0.15.0/src/lib.rs — the whole file
#[cfg(feature = "v1")] pub mod v1;
#[cfg(feature = "v2")] pub mod v2;
#[cfg(feature = "v1")] pub use v1::*;      // v1 is ALSO re-exported at the root
```

That last line is a footgun: `libp2p::autonat::Behaviour` is **v1**, silently. Always
write the version explicitly.

### Module paths

| What | Path |
| --- | --- |
| v1 (deprecated by upstream in favour of v2) | `libp2p::autonat::v1::{Behaviour, Config, Event, NatStatus, InboundProbeEvent, OutboundProbeEvent, ProbeId, ResponseError, DEFAULT_PROTOCOL_NAME}` |
| v1 aliased at root | `libp2p::autonat::Behaviour` etc. |
| v2 client | `libp2p::autonat::v2::client::{Behaviour, Config, Event}` |
| v2 server | `libp2p::autonat::v2::server::{Behaviour, Event}` |

v1's own doc comment says *"We recommend using v2 for new projects."*

### v2 API

```rust
// client
pub struct Behaviour<R = OsRng> where R: RngCore + 'static;
impl Behaviour<OsRng> { fn default() -> Self }          // = new(OsRng, Config::default())
impl<R> Behaviour<R> {
    pub fn new(rng: R, config: Config) -> Self;
    pub fn validate_addr(&mut self, addr: &Multiaddr);
}

pub struct Config { /* private */ }                      // Default: max_candidates 10, probe_interval 5s
impl Config {
    pub fn with_max_candidates(self, usize) -> Self;
    pub fn with_probe_interval(self, Duration) -> Self;
}

pub struct Event {                                       // a STRUCT, not an enum
    pub tested_addr: Multiaddr,
    pub bytes_sent: usize,
    pub server: PeerId,
    pub result: Result<(), autonat::v2::client::Error>,
}

// server
pub struct Behaviour<R = OsRng>;
impl Behaviour<OsRng> { fn default() -> Self }            // = new(OsRng)
impl<R> Behaviour<R> { pub fn new(rng: R) -> Self }
pub struct Event {                                       // also a struct
    pub all_addrs: Vec<Multiaddr>, pub tested_addr: Multiaddr,
    pub client: PeerId, pub data_amount: usize,
    pub result: Result<(), std::io::Error>,
}
```

The `OsRng` here is **`rand_core` 0.6's** `OsRng`, reachable as `rand::rngs::OsRng`
with `rand = "0.8"` (that is what the probe uses, and it compiles). This satisfies
SPEC §7's "use OS CSPRNG" for the AutoNAT nonces.

Wire protocols: `/libp2p/autonat/2/dial-request` and `/libp2p/autonat/2/dial-back`.

**Why v2 matters for us specifically:** v2's server always dials back over a *newly
allocated port*. v1 could dial back over the already-hole-punched port and report
"public" for a node that is not, which then makes DCUtR and relay decisions wrong.

### The address-confirmation chain (relevant to §5 below)

Verified in source, this is the actual data flow inside the swarm:

1. `identify` receives the remote's `observed_addr` and emits
   `ToSwarm::NewExternalAddrCandidate` (`libp2p-identify-0.47.0/src/behaviour.rs:370,374,383`).
2. The swarm turns that into `FromSwarm::NewExternalAddrCandidate`, delivered to
   every behaviour.
3. `autonat::v2::client` consumes it (`.../v2/client/behaviour.rs:108`), probes it,
   and on success emits `ToSwarm::ExternalAddrConfirmed` (`:202`).
4. `dcutr` also consumes `NewExternalAddrCandidate` (`libp2p-dcutr-0.14.1/src/behaviour.rs:340`)
   to fill its LRU of hole-punch candidate addresses.

So **identify is a hard prerequisite for both AutoNAT v2 and DCUtR** to have anything
to work with. Without identify in the behaviour, neither ever sees a candidate.

**Verification:** source for all of the above; compiled for the v2 client/server
construction and `Event` destructuring in `probe-libp2p`.

---

## 4. Circuit Relay v2

One crate, `libp2p-relay` 0.21.1, holds both sides.

| Role | Type |
| --- | --- |
| Relay **server** (we run one so CGNAT peers can be reached) | `libp2p::relay::Behaviour`, `libp2p::relay::Config`, `libp2p::relay::Event` |
| Relay **client** | `libp2p::relay::client::{Behaviour, Transport, Connection, Event, new}` |

Wire protocols, exact strings:

```rust
pub const HOP_PROTOCOL_NAME:  StreamProtocol = StreamProtocol::new("/libp2p/circuit/relay/0.2.0/hop");
pub const STOP_PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/libp2p/circuit/relay/0.2.0/stop");
```

`MAX_MESSAGE_SIZE` on the relay control protocol is 4096 bytes.

**Verification:** source — `libp2p-relay-0.21.1/src/protocol.rs:31-36`, `src/lib.rs`.

### Getting the client transport

Do **not** call `relay::client::new(peer_id)` yourself when using `SwarmBuilder` —
`with_relay_client(sec_upgrade, mux_upgrade)` constructs the transport, `OrTransport`s
it with everything before it, and hands you the `relay::client::Behaviour` as the
second closure argument. `relay::client::new(local_peer_id) -> (Transport, Behaviour)`
exists (`src/priv_client.rs:109`) and is the manual escape hatch.

### The multiaddr forms — verified byte-exactly

`Protocol::P2pCircuit` is multicodec **290**, string form **`p2p-circuit`**.

**Verification:** source — `multiaddr-0.18.2/src/protocol.rs:37,110,236,388,574,678`.

**Reservation (listen through a relay):**

```
/<relay transport addr>/p2p/<RELAY_PEER_ID>/p2p-circuit
```

built as

```rust
relay_addr.clone().with(Protocol::P2p(relay_peer_id)).with(Protocol::P2pCircuit)
```

then handed to `swarm.listen_on(that_addr)`. The relay client transport recognises
the trailing `p2p-circuit` with no destination and opens a HOP `RESERVE`.

**Dial another peer through a relay:**

```
/<relay transport addr>/p2p/<RELAY_PEER_ID>/p2p-circuit/p2p/<DEST_PEER_ID>
```

built as

```rust
relay_addr.clone()
    .with(Protocol::P2p(relay_peer_id))
    .with(Protocol::P2pCircuit)
    .with(Protocol::P2p(dest_peer_id))
```

The parser (`src/priv_client/transport.rs:266-300`) splits on `P2pCircuit`: every
`Protocol::P2p` **before** it is the relay's id, every one **after** it is the
destination's. More than one `p2p-circuit` in an address is rejected with
`Error::MultipleCircuitRelayProtocolsUnsupported` — no relay chaining.

Runtime proof (asserted string round-trip through `Multiaddr::to_string()` and
`str::parse::<Multiaddr>()`):

```
circuit dial addr: /ip4/203.0.113.7/udp/4001/quic-v1/p2p/12D3KooWDpJ7…/p2p-circuit/p2p/12D3KooWAhuY…
reservation listen addr: /ip4/203.0.113.7/udp/4001/quic-v1/p2p/12D3KooWDpJ7…/p2p-circuit
```

**Verification:** compiled + executed (`probe-libp2p`, `assert_eq!` on the round-trip).

### Client events

```rust
pub enum relay::client::Event {
    ReservationReqAccepted { relay_peer_id: PeerId, renewal: bool, limit: Option<Limit> },
    OutboundCircuitEstablished { relay_peer_id: PeerId, limit: Option<Limit> },
    InboundCircuitEstablished { src_peer_id: PeerId, limit: Option<Limit> },
}
```

`renewal: bool` distinguishes the first reservation from a refresh, and `limit`
carries the relay's advertised duration/byte caps. **A reservation is a lease, not a
permanent state** — expect periodic `ReservationReqAccepted { renewal: true, .. }`
and treat its absence as loss of reachability.

### Server config (the knobs for SPEC §4 anti-spam)

```rust
pub struct relay::Config {
    pub max_reservations: usize,
    pub max_reservations_per_peer: usize,
    pub reservation_duration: Duration,
    pub reservation_rate_limiters: Vec<Box<dyn RateLimiter>>,
    pub max_circuits: usize,
    pub max_circuits_per_peer: usize,
    pub max_circuit_duration: Duration,
    pub max_circuit_bytes: u64,
    pub circuit_src_rate_limiters: Vec<Box<dyn RateLimiter>>,
}
```

`max_circuit_bytes` is a hard cap on relayed traffic. A poker table played entirely
over a relay will hit it. Raise it deliberately on any relay we operate, and make
DCUtR upgrade a first-class success metric.

**Verification:** source — `libp2p-relay-0.21.1/src/behaviour.rs`; the server
`Behaviour::new(local_peer_id, Config::default())` call is compiled in `probe-libp2p`.

Several server `Event` variants (`ReservationReqAcceptFailed`, `ReservationReqDenyFailed`,
`CircuitReqDenyFailed`, `CircuitReqOutboundConnectFailed`, `CircuitReqAcceptFailed`)
are `#[deprecated]` upstream and will be replaced by internal logging — do not build
logic on them.

---

## 5. DCUtR

`libp2p-dcutr` 0.14.1. Tiny public surface:

```rust
pub use behaviour::{Behaviour, Error, Event};
pub use protocol::PROTOCOL_NAME;   // StreamProtocol::new("/libp2p/dcutr")

impl Behaviour { pub fn new(local_peer_id: PeerId) -> Self }

pub struct Event {                 // a struct, not an enum
    pub remote_peer_id: PeerId,
    pub result: Result<ConnectionId, dcutr::Error>,
}
```

`Ok(ConnectionId)` is the *new direct* connection. There is exactly this one event —
no separate "attempt started" signal.

**Verification:** source — `libp2p-dcutr-0.14.1/src/lib.rs`, `src/behaviour.rs:45-57,86-94`;
compiled — `Behaviour::new` and the `Event { remote_peer_id, result }` destructuring in `probe-libp2p`.

### Preconditions — both are real and both are hard

1. **An existing relayed connection is required.** In
   `handle_established_inbound_connection` / `handle_established_outbound_connection`
   the behaviour installs its real handler *only* when
   `is_relayed(addr)` — literally `addr.iter().any(|p| p == Protocol::P2pCircuit)`.
   For every non-relayed connection it installs `dummy::ConnectionHandler` and just
   bookkeeps. So DCUtR never initiates anything on a direct connection; it upgrades
   an existing `/p2p-circuit` connection in place.
   Source: `src/behaviour.rs:179,214,385`.

2. **Identify (or another candidate source) is required.** The addresses DCUtR offers
   the peer come from `Candidates`, an LRU(20) fed *exclusively* by
   `FromSwarm::NewExternalAddrCandidate` (`src/behaviour.rs:340`). `Candidates::add`
   drops any address that is itself relayed and appends `/p2p/<self>` if missing.
   With no identify in the behaviour, the candidate list stays empty and hole punching
   has nothing to propose.
   Source: `src/behaviour.rs:355-386`.

3. **Bounded retries.** `MAX_NUMBER_OF_UPGRADE_ATTEMPTS = 3`; after that the event is
   `Err(AttemptsExceeded(3))` and the connection stays relayed. Our UI/logic must
   treat "still relayed" as a normal steady state, not an error.
   Source: `src/behaviour.rs:45,127,136`.

### QUIC hole punching is implemented

`libp2p-quic` 0.13.1 has a real `hole_punching` module wired into the transport
(`src/transport.rs:53,84,324-391`, `src/hole_punching.rs`) with a
`hole_punch_attempts: HashMap<SocketAddr, oneshot::Sender<Connecting>>` table and a
`Connecting` race between the incoming hole-punched packet and the puncher future.
UDP hole punching is generally more successful than TCP simultaneous-open, which
reinforces the QUIC-first recommendation in §9.

**Verification:** source.

---

## 6. GossipSub

`libp2p-gossipsub` 0.49.5 (the umbrella pins `^0.49.0` and resolves to .5, a
2026-07-21 patch — we get it for free).

### Signing / authenticity

```rust
pub enum MessageAuthenticity {
    Signed(Keypair),   // signs with the libp2p identity keypair; author = its PeerId,
                       // sequence number is linearly increasing
    Author(PeerId),    // no signature
    RandomAuthor,      // no signature
    Anonymous,         // no author, no seqno, no signature
}
```

### Validation modes

```rust
pub enum ValidationMode {
    Strict,      // DEFAULT. author + seqno + valid signature required on every message
    Permissive,  // fields optional, validated if present
    Anonymous,   // author/seqno/signature must be ABSENT
    None,        // fields ignored; invalid signatures treated as valid
}
```

**Use `Signed` + `Strict`.** But note the SPEC §21 boundary carefully: GossipSub's
signature authenticates *the libp2p identity keypair that owns the PeerId*, i.e. "which
socket said this". SPEC §12 demands a **separate application signing keypair** over a
canonically serialised payload. Those are two independent signatures and both are
required — GossipSub's `Strict` mode is transport hygiene, **not** the table-advert
signature of SPEC §4. Do not let one substitute for the other.

### Manual validation (needed for §4 anti-spam)

`.validate_messages()` on the builder switches gossipsub to *not* forward a message
until the application says so. Then, per message:

```rust
swarm.behaviour_mut().gossipsub.report_message_validation_result(
    &message_id, &propagation_source,
    gossipsub::MessageAcceptance::Accept,   // | Reject (applies the P₄ score penalty) | Ignore
);
```

`Reject` is what we call after a table advert fails its *application* signature or
`expires_at` check — it costs the forwarder peer score. `Ignore` drops without penalty.

**Verification:** source — `src/behaviour.rs`, `src/types.rs:65-73`, `src/config.rs`;
compiled — the full `ConfigBuilder` chain and `report_message_validation_result` call
are in `probe-libp2p` and check clean.

### Subscribing

```rust
pub type IdentTopic  = Topic<IdentityHash>;   // topic string sent in the clear
pub type Sha256Topic = Topic<Sha256Hash>;     // topic string hashed on the wire

let topic = gossipsub::IdentTopic::new("/p2p-poker/lobby/1");
swarm.behaviour_mut().gossipsub.subscribe(&topic)?;   // Result<bool, SubscriptionError>
swarm.behaviour_mut().gossipsub.publish(topic.clone(), bytes)?;
```

Use `IdentTopic` for the well-known lobby topic (debuggable, and the string is public
anyway). Consider `Sha256Topic` for per-table topics if we ever add them.

### Message size limit

**Default `max_transmit_size` = 65536 bytes** (`ProtocolConfig::default()`,
`src/protocol.rs:86`), and it is a *transmit and receive* cap.

Change it globally or per topic:

```rust
gossipsub::ConfigBuilder::default()
    .max_transmit_size(256 * 1024)
    .max_transmit_size_for_topic(512 * 1024, topic_hash)
```

**Both sides must agree.** A peer with the default 64 KiB will reject our 256 KiB
frame. This is a protocol-version-level decision — pin it in the spec constants, do
not tune it per build.

For SPEC §3's "request a snapshot of existing tables from several peers", note that
this size limit is one more reason snapshot fetch must be a **direct request-response
stream, not a gossip message**: a lobby with hundreds of tables will blow past any
sane gossip frame.

### Mesh parameters worth setting

Defaults (`TopicMeshConfig::default()`, `src/config.rs:80-89`):
`mesh_n = 6`, `mesh_n_low = 5`, `mesh_n_high = 12`, `mesh_outbound_min = 2`.
Other relevant defaults: `heartbeat_interval = 1s`, `history_length = 5`,
`history_gossip = 3`, `duplicate_cache_time = 60s`, `fanout_ttl = 60s`,
`flood_publish = true`, `gossip_lazy = 6`, `max_ihave_length = 5000`.

Recommended for a small global lobby:

| Param | Value | Why |
| --- | --- | --- |
| `mesh_n` / `_low` / `_high` | 8 / 6 / 12 | slightly denser mesh; a poker lobby is small and latency-sensitive |
| `mesh_outbound_min` | 3 | raises the cost of an eclipse attack by inbound-only sybils |
| `validate_messages()` | on | mandatory for §4 spam control |
| `validation_mode` | `Strict` | reject unsigned |
| `message_id_fn` | hash of `(data, topic)` | **important** — the default id is `source ‖ seqno`, so one peer can republish identical content under new seqnos forever and it is never deduplicated |
| `duplicate_cache_time` | 120s | must exceed advert re-broadcast interval |
| `flood_publish` | `false` | default `true` sends every publish to *every* known peer in the topic, not just the mesh — an amplification lever we do not want on a public lobby |

Peer scoring (`PeerScoreParams`, `PeerScoreThresholds`, `TopicScoreParams`,
`score_parameter_decay`) is exported and is the proper long-term answer to lobby
spam. Not configured in the probe; flagged as Phase-1 work.

Also available and worth using: `MaxCountSubscriptionFilter` / `WhitelistSubscriptionFilter`
via `Behaviour::new_with_subscription_filter`, to stop a peer subscribing us to
thousands of junk topics.

---

## 7. Direct per-table streams: `libp2p-stream` vs `request-response` vs hand-written

### Does `libp2p-stream` 0.4.0-alpha work with libp2p 0.56.0? **Yes.**

Its manifest depends on `libp2p-core ^0.43.1`, `libp2p-swarm ^0.47.0`,
`libp2p-identity ^0.2.12` — exactly the ranges the 0.56.0 umbrella resolves into
(0.43.2 / 0.47.1 / 0.2.14). Same crate instances, so the types unify.

```
$ cargo check
    Checking libp2p-stream v0.4.0-alpha
    Checking libp2p v0.56.0
    Checking probe-libp2p-stream v0.1.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 53.44s
```

**Verification:** compiled — `probe-libp2p-stream` composes `libp2p_stream::Behaviour`
and `libp2p::request_response::cbor::Behaviour` in one `#[derive(NetworkBehaviour)]`
struct; plus source — `libp2p-stream-0.4.0-alpha/Cargo.toml`.

Note it is **not** re-exported by the umbrella; it is a separate direct dependency.

### The three options

**A. `libp2p-stream` 0.4.0-alpha** — raw `AsyncRead + AsyncWrite` streams, driven
outside the swarm loop:

```rust
use libp2p_stream as stream;

pub const TABLE_PROTO: StreamProtocol = StreamProtocol::new("/p2p-poker/table/1");

let mut control = swarm.behaviour().streams.new_control();   // Control: Clone
let mut incoming = control.accept(TABLE_PROTO)?;             // IncomingStreams: Stream<Item=(PeerId, Stream)>

tokio::spawn(async move {
    while let Some((peer, mut s)) = incoming.next().await {
        tokio::spawn(async move { /* run the table protocol on `s` */ });
    }
});

// outbound
let s: libp2p::Stream = control.open_stream(peer, TABLE_PROTO).await?;
```

- `type ToSwarm = ()` — it emits no swarm events at all. All I/O happens in tasks.
- `Control` is `Clone` and `Send`, so table logic lives in its own task, cleanly
  separated from the swarm loop. This maps directly onto SPEC §23's `net/streams.rs`.
- `open_stream` **auto-dials** if not connected.
- `OpenStreamError` is `#[non_exhaustive]` — a wildcard match arm is mandatory
  (found the hard way: `error[E0004]: non-exhaustive patterns: Err(_) not covered`).
- **Backpressure caveat, from the README:** *"we will drop streams if your application
  falls behind in processing these incoming streams"*. The `.next()` loop must never
  block; hand off to a spawned task immediately, as above.
- Dropping `IncomingStreams` de-registers the protocol and later inbound negotiations
  fail.
- **It is `-alpha`.** Semver-exempt; a 0.4.0 → 0.5.0 bump can break us.

**B. `request-response`** — framed request/response with a codec:

```rust
snapshot: request_response::cbor::Behaviour<SnapshotRequest, SnapshotResponse>,
// = request_response::Behaviour<cbor::codec::Codec<Req, Resp>>

request_response::cbor::Behaviour::new(
    [(SNAPSHOT_PROTO, request_response::ProtocolSupport::Full)],
    request_response::Config::default()
        .with_request_timeout(Duration::from_secs(15))
        .with_max_concurrent_streams(16),
)
```

`send_request(&peer, req) -> OutboundRequestId`, `send_response(channel, resp)`,
and `Codec::set_request_size_maximum(u64)` / `set_response_size_maximum(u64)`.
Events arrive in the swarm loop as `request_response::Event::Message { message, .. }`
with `Message::{Request { request, channel, .. }, Response { response, .. }}`.
It is strictly request→response: no server push, and every exchange opens a fresh
substream.

**C. Hand-written `NetworkBehaviour` + `ConnectionHandler`.** Full control; also by
far the most code, and the `ConnectionHandler` state machine is the part of
rust-libp2p most likely to churn between releases. SPEC §2 says *"napiš minimální
doplněk nad existující crate, ne vlastní protokol"* — a hand-rolled handler is not
warranted here.

### Recommendation

**Use both B and A, for different jobs:**

| Job | Tool | Reason |
| --- | --- | --- |
| Lobby snapshot fetch (SPEC §3: "request a snapshot from several peers") | **`request-response` + `cbor`** | genuinely one-shot RPC; timeouts, size caps and correlation come free; deterministic CBOR aligns with SPEC §12 |
| `JOIN_REQUEST` / `JOIN_ACCEPT` handshake | **`request-response` + `cbor`** | same shape |
| Per-table event log (SPEC §1: "stav hry se posílá pouze mezi účastníky přes přímé libp2p streamy") | **`libp2p-stream`** | long-lived, bidirectional, both sides push (`BOARD_REVEAL`, `TIMEOUT`, `ACTION_*`); request-response cannot express server push, and one substream per event is wasteful |

Do **not** use option C.

**Risk to manage:** `libp2p-stream` is alpha. Contain it behind a small internal trait
in `net/streams.rs` (open a table stream, accept table streams) so that swapping it
for a hand-written handler later is a single-file change. Frame the table protocol
ourselves with an explicit length prefix + deterministic CBOR body — do not rely on
the transport for framing. That framing code is ours regardless of which option wins,
so the trait boundary costs almost nothing.

---

## 8. Do the sub-crates have newer versions than 0.56.0 pins?

Resolved tree from `Cargo.lock` vs newest non-yanked on crates.io (checked
2026-08-28 via the crates.io API with a User-Agent header):

| Crate | umbrella range | resolved | newest | matters? |
| --- | --- | --- | --- | --- |
| libp2p-gossipsub | ^0.49.0 | **0.49.5** | 0.49.5 | no — we already get it |
| libp2p-relay | ^0.21.0 | **0.21.1** | 0.21.1 | no |
| libp2p-dcutr | ^0.14.0 | **0.14.1** | 0.14.1 | no |
| libp2p-swarm | ^0.47.0 | **0.47.1** | 0.47.1 | no |
| libp2p-core | ^0.43.1 | **0.43.2** | 0.43.2 | no |
| libp2p-quic | ^0.13.0 | **0.13.1** | 0.13.1 | no |
| libp2p-tcp | ^0.44.0 | **0.44.1** | 0.44.1 | no |
| libp2p-tls | ^0.6.2 | **0.6.2** | 0.6.2 | no |
| libp2p-noise | ^0.46.1 | **0.46.1** | 0.46.1 | no |
| libp2p-identity | ^0.2.12 | **0.2.14** | **0.3.0** | **yes, as a trap** |
| libp2p-upnp | ^0.5.0 | **0.5.0** | **0.6.0** | minor |
| libp2p-kad / identify / ping / request-response / autonat / dns / metrics / yamux / connection-limits / memory-connection-limits / swarm-derive | — | latest | latest | no |

Every semver-compatible patch released since 2025-06-27 is picked up automatically.
The umbrella is *not* holding us back on anything we use — with two exceptions:

**`libp2p-identity` 0.3.0 (2026-07-28) is a hard trap.** The umbrella pins `^0.2.12`.
If our own `Cargo.toml` adds `libp2p-identity = "0.3"` directly, cargo links **two
different crate instances**, and `identity::Keypair` from one will not be the same
type as the `Keypair` the `SwarmBuilder` wants — a confusing type-mismatch error, not
a resolver error. **Never depend on `libp2p-identity` directly.** Always go through
the umbrella's re-export: `libp2p::identity::*`, `libp2p::PeerId`. Same rule for
`libp2p-core`, `libp2p-swarm`, `multiaddr`, and `futures` (`libp2p::futures`).
`libp2p-stream` is the one deliberate exception, and it works precisely because its
own range is `^0.2.12` and so shares our instance.

**`libp2p-upnp` 0.6.0** exists but is outside `^0.5.0`. UPnP is a best-effort nicety
for us (AutoNAT + DCUtR + relay is the real path), so staying on 0.5.0 is fine.

**Verification:** `cargo tree` / `Cargo.lock` of `probe-libp2p` (compiled), plus the
crates.io API for newest versions.

---

## 9. Is a QUIC-only stack viable?

### What a QUIC-only stack actually needs

`probe-quic-only` uses `default-features = false` and no `tcp`. First attempt with
only `["tokio","macros","quic","dcutr","relay","identify","gossipsub","autonat"]`:

```
error: could not find `yamux` in `libp2p`
note: found an item that was configured out
   --> libp2p-0.56.0/src/lib.rs:142:25
    |
140 | #[cfg(feature = "yamux")]
142 | pub use libp2p_yamux as yamux;
```

**This is the decisive finding: `with_relay_client` needs a security upgrade and a
multiplexer upgrade even when the only base transport is QUIC.** A relayed connection
is a *stream* through the relay, not a QUIC connection, so it must be
Noise/TLS-encrypted and yamux-multiplexed by hand. Adding `"noise", "yamux"` makes it
compile:

```
    Checking libp2p-quic v0.13.1
    Checking libp2p v0.56.0
    Checking probe-quic-only v0.1.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 26.94s
```

`libp2p-tcp` appears in that `Cargo.lock` only as a dev-dependency of `libp2p`;
`cargo tree -i libp2p-tcp` returns *"nothing to print"* and it is never compiled.
So the transport really is QUIC-only. (`libp2p-tls` **is** compiled regardless —
libp2p-quic depends on it unconditionally for the QUIC TLS handshake.)

**Verification:** compiled — both the failing and the passing state of `probe-quic-only`.

### Should we ship QUIC-only? **No. Keep TCP as a fallback.**

QUIC-first, unambiguously: it is UDP, so hole punching works (`libp2p-quic` has a real
`hole_punching` module, §5), the handshake is 1-RTT, and there is no head-of-line
blocking across our per-table streams.

But TCP must stay in the build, for reasons that are structural rather than
preferential:

1. **UDP is blocked outright on many corporate/hotel/school networks and by some
   ISP CPE.** A QUIC-only client on such a network reaches nothing — not even a relay,
   since the relay itself must be dialled over the base transport. TCP+443-shaped
   traffic is the last resort that keeps such a player in the game. This is exactly
   SPEC §1's "musí fungovat i pro klienty za NAT a CGNAT".
2. **We pay almost nothing for it.** `noise`, `tls` and `yamux` are already mandatory
   for the relay client (proven above), so adding `tcp` costs one small crate and one
   extra `listen_on`. There is no scenario where dropping `tcp` meaningfully shrinks
   the build.
3. **Public bootstrap/relay infrastructure is heterogeneous.** Any peer we learn about
   from the Mainline DHT may only be listening on TCP.

**Concrete policy:** listen on `/ip4/0.0.0.0/udp/0/quic-v1` **and**
`/ip4/0.0.0.0/tcp/0` (plus the `/ip6/::` equivalents); dial QUIC first where an
address offers both; treat TCP as a degraded-but-working path. Verified working —
`probe-libp2p` binds both simultaneously in one swarm at runtime (output in §2).

---

## Open items / risks for later phases

1. **`libp2p-stream` is `0.4.0-alpha`** and semver-exempt. Contained behind an
   internal trait in `net/streams.rs` (§7). Re-evaluate before 1.0.
2. **`max_transmit_size` is a two-sided protocol constant.** Changing it is a
   protocol-version break. Freeze it in the shared constants module next to
   `LOBBY_INFOHASH`.
3. **The default GossipSub `message_id_fn` is `source ‖ seqno`** and does not
   deduplicate identical content. Override it (§6) or lobby spam is nearly free.
4. **`libp2p::autonat::Behaviour` silently means v1.** Enforce a lint or a review rule
   that only `autonat::v2::*` paths appear in our source.
5. **Relay `max_circuit_bytes` will cut off a long relayed table session.** Decide the
   value for any relay we run, and surface "still relayed after 3 DCUtR attempts" in
   the UI as a real state.
6. **GossipSub signing ≠ application signing.** SPEC §12/§21 require our own Ed25519
   application keypair over canonical CBOR. Do not let `ValidationMode::Strict` create
   false confidence.
7. **Untested here:** actual two-hosts-behind-different-NATs connectivity
   (SPEC §36 explicitly asks for this measurement). Everything above is API ground
   truth plus single-host runtime; the real hole-punch success rate is a separate
   experiment.

---

## Recommended dependency block

This exact block was compiled as written, in `…/scratchpad/probe-final/`, against a
single `#[derive(NetworkBehaviour)]` struct holding all thirteen behaviours
(gossipsub, kad, identify, ping, autonat v2 client, autonat v2 server, dcutr,
relay client, relay server, connection_limits, memory_connection_limits, upnp,
libp2p-stream, request-response/cbor):

```
$ cargo check
    Checking libp2p-stream v0.4.0-alpha
    Checking libp2p v0.56.0
    Checking probe-final v0.1.0 (...\scratchpad\probe-final)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 00s
```

**Verification:** compiled. The same behaviour set minus `libp2p-stream` also *runs*
(`probe-libp2p`, output in §2).

```toml
[dependencies]
libp2p = { version = "0.56.0", features = [
    # runtime + derive
    "tokio", "macros",
    # transports & upgrades  (noise+yamux are NOT optional: the relay client needs them)
    "quic", "tcp", "noise", "tls", "yamux", "dns",
    # protocols
    "gossipsub", "kad", "identify", "ping", "autonat", "dcutr", "relay",
    "request-response", "cbor",
    # NAT / limits / observability
    "upnp", "metrics", "memory-connection-limits",
    # identity
    "ed25519", "serde",
    # NOTE: there is NO "connection-limits" feature. `libp2p::connection_limits`
    # and `libp2p::allow_block_list` are always compiled in.
] }

# Direct per-table byte streams. NOT re-exported by the umbrella.
# Depends on libp2p-core ^0.43.1 / libp2p-swarm ^0.47.0 / libp2p-identity ^0.2.12,
# i.e. exactly the instances libp2p 0.56.0 resolves -> types unify.
# ALPHA: semver-exempt, keep behind an internal trait.
libp2p-stream = "0.4.0-alpha"

tokio = { version = "1", features = ["full"] }
futures = "0.3"
# rand 0.8 for `rand::rngs::OsRng`, which is rand_core 0.6's OsRng —
# the exact type libp2p-autonat 0.15 wants.
rand = "0.8"
serde = { version = "1", features = ["derive"] }

# Do NOT add these as direct dependencies — use the umbrella re-exports instead
# (libp2p::identity, libp2p::PeerId, libp2p::core, libp2p::swarm, libp2p::multiaddr,
#  libp2p::futures). libp2p-identity 0.3.0 exists but is OUTSIDE the umbrella's
# ^0.2.12 range and would link a second, incompatible copy of `Keypair`/`PeerId`.
# libp2p-identity = "0.3"   # <-- BREAKS TYPE UNIFICATION
```
