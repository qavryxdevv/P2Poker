# NETWORK_STACK.md

Specification of the transport and discovery layer of `p2p-poker`.

**Status:** Phase 0 output, binding for Phase 7 (libp2p transport) and Phase 8
(Mainline DHT discovery + GossipSub lobby). No implementation exists yet.

**Authority order.** `docs/SPEC_CS.md` is the specification and wins over
everything here. `docs/DECISIONS.md` (D-001 … D-007) is binding owner decision and
outranks the research documents and any preference of this document. **D-007
corrects D-006** and wins over it: at two seats an action deadline is advisory and
a fold-effect timeout certificate is forbidden (§8.4, §15). The research
notes under `docs/research/` are the evidence base:
`LIBP2P.md`, `MAINLINE_DHT.md`, `NAT_AND_DISCOVERY.md` are the three this document
is built on; `MENTAL_POKER.md` is used only for measured per-hand byte counts.
All seven research documents exist — nothing was missing and nothing here is
invented to paper over a gap.

**Evidence rules.** Every load-bearing claim carries a `Verification:` line with
one of:

* **[COMPILED]** — code using it was compiled, and where stated executed, on this
  machine (`rustc 1.95.0`, `cargo 1.95.0`, `x86_64-pc-windows-msvc`). The probe
  written for *this* document is
  `…/scratchpad/probe-netstack/` and it builds and runs the entire behaviour stack
  specified in §5–§11.
* **[SOURCE]** — read in the unpacked crate under
  `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/<crate>-<version>/`.
* **[MEASURED]** — a live network measurement recorded in a research document,
  with the section reference.
* **[RESEARCH]** — carried over from a research document, which carries its own
  verification line.

docs.rs and recollection are leads, never evidence. Where something is unknown it
is written as **OPEN QUESTION** and carried to §13 rather than guessed
(`SPEC_CS.md` §36).

---

## 1. Layers, and the rule that constrains all of them

**Module this document governs** (`SPEC_CS.md` §23, interim mapping pending
`docs/ARCHITECTURE.md` in Phase 2): `src/net/` — `dht.rs`, `swarm.rs`, `lobby.rs`,
`streams.rs` — plus the `InMemoryTransport` of §1.3, which sits behind the same
upward trait. The other four specification documents name their own modules in
their own §1; the separation of transport, poker engine and cryptographic deck
that `SPEC_CS.md` §23 requires is the separation between those five mappings.

### 1.1 The stack

```
                        ┌────────────────────────────────────────┐
                        │ GUI            (egui/eframe, docs/GUI) │
                        └───────────────────┬────────────────────┘
                                            │  commands / view models
   ┌────────────────────────────────────────┴────────────────────────────────┐
   │ Lobby / matchmaking      table list, presence, chat, join flow          │
   ├─────────────────────────────────────────────────────────────────────────┤
   │ Poker state machine      NLHE rules, pots, showdown, tournament clock   │
   ├─────────────────────────────────────────────────────────────────────────┤
   │ Mental Poker             joint deck, shuffle proofs, decryption shares  │
   ├─────────────────────────────────────────────────────────────────────────┤
   │ Protocol / signed event log   canonical CBOR, Ed25519, hash chain       │
   ╞═════════════════════════════════════════════════════════════════════════╡
   │ P2P transport  ── THIS DOCUMENT ────────────────────────────────────────│
   │                                                                         │
   │   net/dht.rs      Mainline DHT (BEP 5): announce_peer / get_peers       │
   │   net/swarm.rs    libp2p swarm: QUIC+TCP, Noise/TLS, identify, ping,    │
   │                   AutoNAT v2, DCUtR, Circuit Relay v2 (client+server),  │
   │                   connection & memory limits, blocklist, mDNS, UPnP     │
   │   net/lobby.rs    GossipSub topics, snapshot request-response           │
   │   net/streams.rs  per-table direct streams (libp2p-stream)              │
   └───────────────────┬─────────────────────────────────────────────────────┘
                       │
              ┌────────┴────────┐
              │                 │
   BitTorrent Mainline DHT   libp2p (QUIC / TCP, GossipSub, relay)
   (discovery hints only)    (everything else)
              │                 │
              └────────┬────────┘
                       ▼
              other poker clients
```

The double line is a hard boundary. Everything below it moves bytes and
connections. Everything above it decides meaning.

### 1.2 The rule (`SPEC_CS.md` §1)

> *Síťová vrstva NESMÍ rozhodovat o pokerových pravidlech ani vytvářet karty.*
> The network layer must never decide a poker rule and never create a card.

Stated as prohibitions the implementation can be reviewed against. The network
layer must **not**:

1. create, derive, permute, encrypt, decrypt, or in any way produce a card, a
   deck, a shuffle, a permutation, or randomness that ends up in a card;
2. decide that an action is legal, that a betting round is over, that a street
   advances, that a hand is over, or who won;
3. decide that a player is absent, has timed out, has folded, or has forfeited.
   Transport-level connection loss is a **hint** the layers above may consider; the
   verdict is the D-006 timeout certificate, which is an application event signed
   by the other active players — and at two seats there is no such verdict at all,
   because D-007 makes the heads-up deadline advisory (§8.4). The prohibition is
   unchanged either way: the transport must not supply a verdict the layer above
   does not have;
4. reorder, deduplicate, filter, merge or "repair" application events on the basis
   of anything other than the bytes' own signatures and size caps. Ordering is the
   hash chain's job (`SPEC_CS.md` §13);
5. treat a `PeerId`, a connection, or a DHT record as an authorisation to act as a
   player (`SPEC_CS.md` §20 — "PeerId říká, s jakým socketem mluvíš, ne kdo hraje");
6. hold, forward, log or persist any secret from the mental-poker layer.

The network layer **must**:

1. bootstrap the Mainline DHT and the libp2p swarm;
2. publish and read `LOBBY_INFOHASH` presence hints;
3. carry the GossipSub lobby;
4. do NAT traversal (AutoNAT v2, DCUtR) and relay fallback (Circuit Relay v2);
5. give every peer connection confidentiality, integrity and a cryptographically
   proven `PeerId` (Noise or TLS 1.3, over QUIC or TCP);
6. deliver application messages to named peers, and report delivery facts
   (connected, disconnected, relayed, direct) upward without interpreting them;
7. enforce resource limits (§11) — the one place the network layer is allowed to
   drop a message on its own authority, and only for size, rate or malformation.

### 1.3 The consequence that makes it testable

Because the layer above only ever sees "bytes arrived from `PeerId` X" and
"connection to X is up/down/relayed", the whole poker and cryptographic stack can
run on `InMemoryTransport` with no DHT and no libp2p (`SPEC_CS.md` §24). The
interface between `net/` and everything above it is therefore a narrow trait, and
that trait — not the libp2p types — is what the rest of the program is written
against. Two more reasons the same boundary is required: `libp2p-stream` is
`0.4.0-alpha` and semver-exempt, and `mainline` may one day need replacing by a
hand-written KRPC client (`MAINLINE_DHT.md` §6 sizes that escape hatch at
1500–2400 lines and recommends against it).

> Verification: [RESEARCH] `LIBP2P.md` §7 (alpha containment),
> `MAINLINE_DHT.md` §6, §8.

#### 1.3.1 The upward trait, specified

This is the whole surface the layers above `net/` may use. **Nothing
libp2p-typed may appear above this trait** — no `Multiaddr`, no `SwarmEvent`, no
`StreamProtocol`, no `libp2p::PeerId`. `PeerId` above the boundary is our own
newtype over the 32-byte Ed25519 public key material that the libp2p handshake
proved, and the libp2p implementation converts at the boundary.

```rust
/// The complete transport surface. Implemented by the libp2p stack (§5–§11)
/// and by `InMemoryTransport` (§1.3.2), and by nothing else.
trait Transport {
    /// Deliver `bytes` to `peer`. Returns as soon as the bytes are queued;
    /// success is NOT a delivery receipt, and the caller must never treat it
    /// as one (§1.2 prohibition 3).
    fn send(&self, peer: PeerId, bytes: Bytes) -> Result<(), SendError>;

    /// The single event stream. Ordering is per-peer FIFO and nothing more:
    /// events from two different peers may interleave arbitrarily.
    fn events(&self) -> impl Stream<Item = TransportEvent>;

    /// A fact about a socket, never a fact about a player.
    fn connection_state(&self, peer: PeerId) -> ConnectionState;
}

enum ConnectionState { Direct, Relayed, Down }

enum TransportEvent {
    Connected    { peer: PeerId, state: ConnectionState },
    Disconnected { peer: PeerId },
    Message      { peer: PeerId, bytes: Bytes },
    ListenerClosed { reason: ListenerCloseReason },
}
```

Three rules that make the trait the boundary rather than a formality:

* `Message` carries **bytes**, never a decoded event. Decoding, canonicality and
  signature verification are the protocol layer's (`PROTOCOL.md` §4.0).
* `Disconnected` is a hint. It is never an input to a poker decision
  (§1.2 prohibition 3, §8.4, §10.4).
* `ListenerClosed` exists because a relay reservation denial closes a circuit
  listener with no automatic retry (§9.6); the layer above must be able to see it
  and re-issue `listen_on` with backoff.

#### 1.3.2 `InMemoryTransport` (`SPEC_CS.md` §24), specified

`SPEC_CS.md` §24 requires a simulated network that implements the same trait and
runs the whole poker and cryptographic stack with no DHT and no libp2p. It is the
only vehicle for §25's adversarial suite, and it is **not deferred**: the
in-memory path is the only one exercised before Phase 7 (§12), so an unspecified
harness blocks Phases 3–6.

**(i) A seeded, deterministic scheduler.** Determinism is a requirement, not a
nicety: `STATE_MACHINE.md` I22 asserts replay determinism, and an
under-determined harness produces flaky adversarial tests that get muted rather
than fixed. Concretely — one `u64` seed per run, printed in the failure output
and sufficient to reproduce the run byte-for-byte; a single logical clock advanced
only by the scheduler, never by wall time; every scheduling decision (which queued
message is delivered next, which injection fires) drawn from that seed; and no
use of thread scheduling, `Instant::now`, or any OS entropy source anywhere in
the harness.

**(ii) The eight injection modes `SPEC_CS.md` §24 names**, each parameterised and
each reproducible from the seed:

| Mode | What it does |
|---|---|
| delay | holds a message for a bounded number of logical ticks |
| duplicate | delivers a message twice, at different ticks |
| packet loss | drops a message entirely |
| reordered events | delivers a peer's messages out of the order it sent them |
| disconnect | emits `Disconnected` and drops subsequent sends to that peer |
| reconnect | emits `Connected` again, with a chosen `ConnectionState` |
| malicious packets | delivers arbitrary attacker-chosen bytes as a `Message`, including truncated, over-cap, non-canonical CBOR and invalid-signature bodies |
| conflicting messages | see (iii) |

**(iii) How a conflicting message is injected**, since it is the one mode with a
protocol meaning. The harness must be able to hand **two different
`SignedEvent`s for one
`(sender, table_id, hand_id, sequence, event_class)`** to two different
receivers — which is exactly the input the `EquivocationProof` predicate of
`PROTOCOL.md` §5.2 is defined over. The harness therefore needs the sender's
application signing key, because both copies must carry valid signatures; a test
that injects an invalidly signed second copy tests the signature check, not the
equivocation predicate, and the two must not be confused. Both copies carry
`chain_scope = 1`; the predicate is not defined over unchained traffic and the
harness must not attempt to produce a "conflict" there (§6.4, §7.4).

**(iv) The acceptance bar of `SPEC_CS.md` §24:** thousands of hands run
automatically, with no DHT and no libp2p process involved, each run reproducible
from its seed, and every `THREAT_MODEL.md` §5.5 cheater exercised against it.

---

## 2. Startup sequence

`SPEC_CS.md` §3 draws this chain:

```
start → load persistent libp2p keypair → bootstrap Mainline DHT
      → announce_peer(LOBBY_INFOHASH) → get_peers(LOBBY_INFOHASH)
      → dial peers over libp2p → subscribe GossipSub lobby topic
      → request table snapshot from several peers → receive available tables
```

The implementation follows it in that order, with two structural additions that
the spec's chain implies but does not draw: the libp2p listeners must be bound
**before** the announce (we announce the port we listen on), and the DHT and the
swarm run concurrently from that point on, not in sequence.

**Nothing in this sequence requires the client to be reachable from outside.**
Every step is an outbound operation. That is the property D-003 rests on:
*seeing* the lobby needs outbound connectivity only; *playing a hand* against
another unreachable peer is what needs §9.

### 2.1 Step table

| # | Step | Failure mode | What the client does |
|---|---|---|---|
| 0 | **Load or create the persistent libp2p Ed25519 keypair** from the portable profile directory next to the executable (`SPEC_CS.md` §22). Encoded with `Keypair::to_protobuf_encoding()`, read back with `from_protobuf_encoding`. On Windows the file is wrapped with DPAPI (`SPEC_CS.md` §21). | file missing → first run | generate a new keypair, write it, continue. **Never** generate a fresh identity when a keypair file exists but fails to decode — that silently forks the identity. Report the error and refuse to start, so a corrupted or foreign profile is visible rather than silently replaced. |
| 1 | **Load the cached DHT bootstrap list** and the per-peer DCUtR failure cache from the profile. | missing/corrupt | fall back to the compiled defaults; not fatal. |
| 2 | **Build the swarm** (§5) and `listen_on` `/ip4/0.0.0.0/udp/P/quic-v1`, `/ip4/0.0.0.0/tcp/P`, plus the `/ip6/::` equivalents. `P` is chosen once, persisted, reused every run. | port `P` in use | try `P` once, then fall back to an ephemeral port and persist the new value. Log it: a changed port invalidates the previous DHT record until the next announce. |
| 3 | **Start the Mainline DHT** on its **own** UDP socket, `.port(0)`, with a deny-all `RequestFilter` (§11.4). | socket bind failure | retry once on a different port; if it still fails, the client runs in **no-discovery mode**: the lobby is empty, direct-invite still works, and the GUI says so. |
| 4 | **Bootstrap the DHT.** | ~3 of 35 cold starts failed on the first attempt [MEASURED]; `router.bittorrent.com` is dead from this network, confirmed twice ~50 min apart | retry with exponential backoff (2 s, 5 s, 15 s, 60 s, then every 5 min). A failed bootstrap is **normal**, never fatal. Merge the cached node list with the compiled defaults — do not overwrite the cache with a bad session's routing table. |
| 5 | **`announce_peer(LOBBY_INFOHASH, Some(port))`** — see §3 and §4.4 for which port. Repeat every 10 minutes (§10). | announce error; or `Ok` while zero nodes actually stored it (the crate discards `stored_at`, `MAINLINE_DHT.md` §6a) | retry on the next cycle. Derive a health number by running `get_peers` right after the announce and counting responders that return **our own** address; that count is what the GUI's DHT indicator shows. |
| 6 | **`get_peers(LOBBY_INFOHASH)`** → `Vec<SocketAddrV4>` batches. First values arrive in **113–196 ms**; the full iterative query takes **2.4–2.9 s** [MEASURED]. | zero peers | not an error. Either nobody else is online or the lookup was unlucky. Retry on the 10-minute cycle and continue with mDNS and any cached peers. The GUI shows "no players found yet", never an error dialog. |
| 7 | **Dial the hints** (§4). Start dialling on the first batch — do not wait for the query to finish. | most candidates fail | expected. Bounded budget, concurrency cap, per-address timeout (§11.2). |
| 8 | **identify + AutoNAT v2 settle our external address.** Filter private/reserved addresses ourselves — AutoNAT v2 has no such guard and was measured confirming an RFC 1918 address as external [MEASURED]. | no confirmation | stay in `Reachability::Unknown`; continue. Re-announce with the corrected external port as soon as one is confirmed. |
| 9 | **Subscribe the GossipSub lobby topics** (§6). Can be done immediately after step 2; messages only flow once peers connect. | `SubscriptionError` | fatal configuration bug, not a runtime condition — fail loudly. |
| 10 | **Snapshot request to several peers** (§7) over `request-response`. | fewer than 2 usable responses | retry once with a fresh peer set, then proceed on live gossip alone and show "lobby syncing". |
| 11 | **Live GossipSub sync**, and the table list is displayed. | — | steady state. |

Steps 3–7 and 9–11 run concurrently with the GUI; the GUI is never blocked
(`SPEC_CS.md` §33).

### 2.2 The network status the GUI must be given (`SPEC_CS.md` §22)

The transport layer publishes a single observable status struct:

* **DHT**: `NotStarted | Bootstrapping | Ready | Failed`, last successful announce
  time, storing-node health count (§2.1 step 5), number of unique candidates from
  the last lookup.
* **libp2p**: local `PeerId`, listen addresses, confirmed external addresses,
  reachability (`Unknown | Public | Private`), number of established connections,
  how many are relayed.
* **Lobby**: subscribed yes/no, GossipSub mesh peer count per topic, number of
  live table ads, time since last snapshot.
* **Relay**: whether we hold reservations and on which relays; whether we are
  *acting* as a relay and for how many peers (D-002 requires this to be visible);
  and for every table peer, whether the connection is direct or relayed (D-001).

"Still relayed after three DCUtR attempts" is a normal steady state, not an error
(`MAX_NUMBER_OF_UPGRADE_ATTEMPTS = 3` [SOURCE]) and must be displayed as such.

---

## 3. `LOBBY_INFOHASH`

### 3.1 Requirement

`SPEC_CS.md` §3: every installation ships the same fixed 20-byte infohash; the
client must not create a new lobby on each start. It must therefore be a compiled-in
constant. It must also be *verifiable* — anyone auditing the client should be able
to recompute it from first principles rather than trust a magic number.

### 3.2 Derivation

```
LOBBY_INFOHASH = first 20 bytes of SHA-256( ASCII("p2p-poker/mainline-lobby/v1") )
RELAY_INFOHASH = first 20 bytes of SHA-256( ASCII("p2p-poker/mainline-relay/v1") )
```

The input is the exact ASCII byte string, with **no** trailing newline, **no** NUL
terminator and no length prefix. The output is the first (most significant) 20
bytes of the 32-byte digest, in digest order.

| Constant | Derivation string | Value (hex, 20 bytes) |
|---|---|---|
| `LOBBY_INFOHASH` | `p2p-poker/mainline-lobby/v1` | `fd7c0d69433e32e425db3ca2b7d7718928739f01` |
| `RELAY_INFOHASH` | `p2p-poker/mainline-relay/v1` | `9c18d8c80f69de3aa079b2ef519bc4bbb67e1cc1` |

Reproduce with any tool:

```
$ printf '%s' 'p2p-poker/mainline-lobby/v1' | sha256sum | cut -c1-40
fd7c0d69433e32e425db3ca2b7d7718928739f01
$ printf '%s' 'p2p-poker/mainline-relay/v1' | sha256sum | cut -c1-40
9c18d8c80f69de3aa079b2ef519bc4bbb67e1cc1
```

> Verification: [COMPILED+RUN] `probe-netstack` computes both digests with `sha2`,
> truncates to 20 bytes, builds `mainline::Id` from them, and asserts equality
> against the pinned hex constants parsed by `Id::from_str`. Program output:
> `LOBBY_INFOHASH = fd7c0d69433e32e425db3ca2b7d7718928739f01`,
> `RELAY_INFOHASH = 9c18d8c80f69de3aa079b2ef519bc4bbb67e1cc1`.
> Independently reproduced with `sha256sum` and with `openssl dgst -sha256`.
> [SOURCE] `mainline-8.0.0/src/common/id.rs:31` (`Id::from_bytes`), `:165`
> (`From<[u8;20]>`), `:183` (`FromStr`, hex).

### 3.3 Why this construction

* **Not SHA-1.** A BitTorrent infohash is conventionally SHA-1 because it hashes a
  torrent `info` dictionary. We are not hashing a torrent; we need an arbitrary
  20-byte label. SHA-1 is collision-broken and using it here would invite the
  reader to think a security property is being claimed. Truncated SHA-256 makes it
  obvious that the value is a *namespace label*, not a commitment.
* **20 bytes is forced** by BEP 5 and by `mainline::Id` (`ID_SIZE = 20` [SOURCE]).
  Truncation is the only way to get 20 bytes out of SHA-256, and truncation of a
  hash function output is standard practice for a label.
* **Non-arbitrary and auditable.** A reviewer recomputes it in one shell command.
  Nobody has to trust that the constant was not chosen to sit next to something.
* **Version separation is free.** `v1` in the string means a future incompatible
  protocol version derives a different infohash and the two populations do not see
  each other at the DHT level at all. That is cheaper and more reliable than
  filtering incompatible peers after connecting.
* **Distinct relay namespace.** `RELAY_INFOHASH ≠ LOBBY_INFOHASH` so that looking
  for a relay does not enumerate players and vice versa
  (`NAT_AND_DISCOVERY.md` §3.4).

### 3.4 Distribution and integrity

* Both constants, and both derivation strings, are two-sided and are therefore
  defined in **`PROTOCOL.md` §13** together with the protocol names, topic names
  and size caps; §14 of this document no longer restates them. They are `const`,
  compiled in.
* A release build has **no** way to override them. A `--testnet <string>` flag may
  derive a *different* pair from a different derivation string for integration
  testing; when it is active the GUI must display a prominent, permanent "TESTNET"
  banner, because a client on a different infohash is invisible to everyone else
  and that must never be mistaken for "nobody is online".
* A unit test recomputes both constants from their derivation strings and asserts
  equality with the pinned hex. This is exactly the assertion the probe already
  runs, and it means a typo in the constant fails the build rather than silently
  splitting the network.

### 3.5 What a fixed public infohash costs — and it must be told to the user

`SPEC_CS.md` §3 explicitly requires documenting these limits, and forbids
answering them with a central server.

* **DHT records are unauthenticated.** Anybody may announce any `IP:port` under
  our infohash. There is no proof of possession beyond BEP 5's token, which only
  checks that the announcer's IP matches the one the storing node saw. The list is
  a hint (§4.1).
* **Records are short-lived and must be refreshed.** Measured on two independent
  private random infohashes: a single un-refreshed announce stayed retrievable for
  **~45 minutes** and was gone by **~50**, thinning from ~30 storing nodes at
  t+30 to 9 at t+45 [MEASURED]. This is LRU eviction, not a protocol TTL — BEP 5
  defines no expiry at all — so a *busy public* infohash will evict faster. Our
  own infohash's real lifetime is **unmeasured** and must be measured in Phase 8
  (§13, OQ-1).
* **Announcing publishes the player's IP.** A single `get_peers` was measured
  telling **105–176 distinct DHT nodes** that this IP is interested in this exact
  20-byte value, plus ~130–140 more that learn only the IP [MEASURED]. At a
  10-minute cadence that is roughly 150 disclosure events per day, to a rotating
  set of strangers, on a network that is heavily crawled.
* **Crawling is not hypothetical — it was observed.** During the TTL experiment a
  20-byte infohash generated locally, published nowhere but into the DHT, had a
  *stranger's* address announced under it within 24 minutes [MEASURED]. A public,
  hard-coded constant is a far easier target than that random one.
* **Therefore:** a one-time, plain-language consent screen before the first
  announce; a direct-invite mode that never announces; announce only while the
  user is actually looking for a game. None of these removes the disclosure. See
  `THREAT_MODEL.md`.

An epoch-rotating infohash (`SHA-256("…/v1" ‖ floor(day))`) would blunt historical
crawls at the cost of cross-version compatibility and a rotation race at midnight.
It is **not adopted**; carried as OQ-2.

> Verification: [MEASURED] `MAINLINE_DHT.md` §3.5, §3.6, §7; [SOURCE] BEP 5 has no
> expiry clause, `MAINLINE_DHT.md` §2.

---

## 4. The DHT-to-libp2p bridge

This is the load-bearing mechanism of the whole discovery story. Everything else
in this document assumes it works.

### 4.1 The rule it must obey

`SPEC_CS.md` §1, restated verbatim in force:

> The peer list from the Mainline DHT is **not verified**: anyone can write
> anything there. Treat it as a hint about **where to try connecting**, never as a
> claim about **who is there**. Identity is decided only by the libp2p handshake
> and by the signature on the application message.

Nothing in the bridge may weaken this. In particular the bridge must not add a
"signed DHT record" scheme to make the hint trustworthy — that would be a new
protocol where none is needed, and `dht 7.0.0`'s signed-peer variant was measured
reaching only **2–3 storing nodes versus 19–32** for a plain announce, a 10× loss
of redundancy for a property we do not need [MEASURED, `MAINLINE_DHT.md` §3.4].

### 4.2 What the DHT actually gives us

A BEP 5 `get_peers` response carries compact peer info: **exactly 6 bytes** —
4-byte IPv4 address plus 2-byte big-endian port. No `PeerId`, no transport tag, no
protocol hint, no signature. `mainline`'s API surfaces exactly that shape:

```rust
impl AsyncDht {
    pub fn get_peers(&self, info_hash: Id) -> GetStream<Vec<SocketAddrV4>>;
}
```

Each item yielded by the stream is the peer vector from **one responding node**,
so multiplicity across items is observable (see §4.6).

> Verification: [SOURCE] `mainline-8.0.0/src/async_dht.rs`; [MEASURED]
> `NAT_AND_DISCOVERY.md` §6.1, raw 6-byte records parsed off the wire by a
> hand-written KRPC probe.

### 4.3 The bridge, precisely

For each `SocketAddrV4 { ip, port }` that survives the filters of §4.5:

```rust
// 1. Synthesise a Multiaddr with NO /p2p component.
let addr: Multiaddr = Multiaddr::empty()
    .with(Protocol::Ip4(*hint.ip()))
    .with(Protocol::Udp(hint.port()))
    .with(Protocol::QuicV1);
// -> /ip4/203.0.113.7/udp/43117/quic-v1

// 2. Dial it as an unknown peer.
let opts = DialOpts::unknown_peer_id().address(addr).build();
// opts.get_peer_id() == None
swarm.dial(opts)?;
```

and, if and only if the QUIC dial fails, optionally the TCP form on the *same
numeric port* (§4.4):

```rust
let addr: Multiaddr = Multiaddr::empty()
    .with(Protocol::Ip4(*hint.ip()))
    .with(Protocol::Tcp(hint.port()));
// -> /ip4/203.0.113.7/tcp/43117
```

Identity is then established by the handshake, and `SwarmEvent::ConnectionEstablished`
reports the `PeerId` that was **cryptographically proven** by Noise or TLS 1.3.
Everything before that event was a guess; everything after it is a fact about the
socket (and still not a fact about the player — §4.7).

> Verification: [COMPILED+RUN] `probe-netstack` builds both multiaddrs, asserts the
> QUIC one contains no `Protocol::P2p` component, builds
> `DialOpts::unknown_peer_id().address(addr).build()` and asserts
> `get_peer_id() == None`. Output:
> `bridge quic = /ip4/203.0.113.7/udp/43117/quic-v1   tcp fallback = /ip4/203.0.113.7/tcp/43117   peer_id = None`.
> [COMPILED+RUN, `NAT_AND_DISCOVERY.md` §6.2] the same construction dialled a real
> peer end-to-end: the dialer knew only an IP and a port and finished holding a
> proven `PeerId`. Also recorded as verified in D-003.

### 4.4 Which port we announce, and the one-port rule

`mainline::Dht::announce_peer(info_hash, port: Option<u16>)`:

* `None` sets BEP 5 `implied_port`, so storing nodes record the **source port of
  our DHT packet**. That is the DHT socket's NAT-mapped port, which is **not** our
  QUIC port — the DHT cannot share libp2p's socket (`KrpcSocket` constructs its
  own `std::net::UdpSocket`, no injection point, `SO_REUSEADDR` never set; a second
  bind on the same port fails with OS error 10048 [SOURCE + COMPILED]).
* `Some(p)` records the literal `p`.

So we always announce `Some(external_quic_port)` and never `implied_port`.

**The one-port rule.** The client binds the *same numeric port* `P` for QUIC (UDP)
and for TCP. One DHT record then serves both transports, and a dialer can try
`/udp/P/quic-v1` first and `/tcp/P` as a fallback without a second record. If `P`
cannot be bound on both, only the QUIC port is announced and TCP remains reachable
only via addresses learned from `identify`, from the lobby, or from a relay. This
is a deliberate design choice with a cheap failure mode; it must be confirmed to
behave in Phase 7 (OQ-3).

**Learning the external port.** On the first pass the client announces its *local*
QUIC port, because nothing better is known yet. As soon as `identify`'s
`observed_addr` or AutoNAT v2 confirms an external address, the announce is
corrected and re-issued immediately. Announces repeat every 10 minutes, so a wrong
first announce self-corrects within one cycle. On the measured development NAT the
external port equals the local port (endpoint-independent mapping, port-preserving
[MEASURED]), so the first announce is already correct there; on other NATs it costs
other peers one dial timeout. That is acceptable: the DHT list is a hint, and dead
hints are expected.

> Verification: [SOURCE] `mainline-8.0.0/src/rpc/socket.rs:38-66`; [COMPILED]
> second-bind failure reproduced, `MAINLINE_DHT.md` §4; [MEASURED]
> `NAT_AND_DISCOVERY.md` §1.2 and §6.3 — an announce of port 41337 was read back
> from 12 nodes as `198.51.100.17:41337`, i.e. "the DHT stored the port we asked
> for and the IP it observed"; D-003 "Which port to announce".

### 4.5 Filtering candidates before dialling

Applied to **DHT-derived** candidates only:

1. **Deduplicate** across all responders, and against candidates already connected
   or already known to be dead this session.
2. **Drop bogons**: RFC 1918 (`10/8`, `172.16/12`, `192.168/16`), RFC 6598
   (`100.64/10`), loopback `127/8`, link-local `169.254/16`, multicast, broadcast,
   `0.0.0.0/8`, and reserved ranges. A record in the public DHT claiming a private
   address is either pollution or an attempt to make us scan our own LAN.
3. **Drop our own external address** (any address AutoNAT/identify has confirmed
   as ours) and any address on the local machine.
4. **Drop port 0** and, as a matter of hygiene, ports below 1024.

This filter is **not** the same as the publish filter of §5.6 and the two must
never be merged. Same-LAN peers are found by mDNS with a full `/p2p/<PeerId>`
address and are dialled on their RFC 1918 address deliberately
(`NAT_AND_DISCOVERY.md` §5.2). Conflating "never dial a private address" with
"never publish a private address" breaks the two-players-behind-one-router case,
which is a case this project explicitly cares about.

### 4.6 Hostile inputs: stranger, stale, attacker

| Input | What actually happens | Cost | Handling |
|---|---|---|---|
| **A real BitTorrent client** squatting on the port, or any non-libp2p listener | the transport handshake fails or the negotiated protocol is not ours | one handshake timeout | drop; remember the address as dead for the session; do not retry within the session. `LOBBY_INFOHASH` is public and anything may be listening on it. |
| **A libp2p node that is not a poker client** | the handshake succeeds and we get a proven `PeerId`, but `identify` reports a different protocol set | one connection | disconnect after `identify` unless the peer advertises `/p2p-poker/1`; never admit it to the relay allow-list (§9.6). |
| **Stale entry** — the peer went offline, or announced a port nobody can reach | dial timeout | one timeout slot | expected and normal. Bounded budget (§11.2). No retry storm. |
| **Wrong-port entry** — a NATed peer announced a port that is not its external port | dial timeout | as above | self-heals on that peer's next announce cycle. |
| **Attacker announces a third party's `IP:port`** (reflection) | we send a QUIC Initial / TCP SYN to an innocent host | small packet, no amplification beyond one handshake attempt per address per cycle | bounded dial budget, per-IP dial rate limit, no retries, deduplicate. This is inherent to BEP 5 — every BitTorrent client on earth has the same property. We reduce our contribution; we cannot remove it. |
| **Attacker floods the infohash with thousands of junk entries** (discovery DoS / eclipse attempt) | our candidate list is mostly junk, so real peers are found slowly or not at all | latency, wasted dials | (a) hard cap on candidates dialled per cycle; (b) prefer candidates returned by **more independent responding nodes** — `GetStream` yields one `Vec` per responder, so multiplicity is observable and a single Sybil writer is visible as a low-multiplicity set; (c) random sample from the remainder so a flooder cannot deterministically fill the sample; (d) keep a persistent list of peers that previously completed a poker handshake and dial those first. **None of this defeats a well-resourced flooder** — see §12. |
| **Attacker announces its own address and completes the handshake** | it is now a connected libp2p peer with a proven `PeerId` | one connection slot | this is *allowed*. The DHT was never an authorisation. The peer can now gossip, and everything it says is subject to application signature verification and to the lobby validation of §6.4. It cannot forge another player's table ad, and it cannot join a table it is not admitted to. |

### 4.7 What the bridge establishes, and what it does not

After `ConnectionEstablished` we know: **this socket holds the private key for this
`PeerId`.** That is all.

We do **not** know: that the peer is honest, that the peer is a distinct human,
that the peer runs an unmodified client, that the DHT record we dialled belonged to
them, or that they are entitled to a seat. `SPEC_CS.md` §20's three-identity
separation is what carries the rest:

| Identity | What it proves | Where it comes from |
|---|---|---|
| libp2p `PeerId` | which socket we are talking to | persistent Ed25519 keypair, proven in the transport handshake |
| poker application identity | who signed this event | a **separate** Ed25519 application keypair, over canonical CBOR (`SPEC_CS.md` §12) |
| table/session identity | which table and which session an event belongs to | `table_id` + per-session nonce (`SPEC_CS.md` §14, §20) |

A GossipSub message signature (§6.3) authenticates the *first* of those and only
the first. It is transport hygiene and is **never** accepted in place of the
application signature.

---

## 5. Transport configuration

### 5.1 Pinned versions

`libp2p = "0.56.0"` (published 2025-06-27, newest non-yanked), resolving
`libp2p-gossipsub 0.49.5`, `libp2p-relay 0.21.1`, `libp2p-dcutr 0.14.1`,
`libp2p-autonat 0.15.0`, `libp2p-quic 0.13.1`, `libp2p-swarm 0.47.1`,
`libp2p-core 0.43.2`, `libp2p-identity 0.2.14`, `libp2p-identify 0.47.0`,
`libp2p-mdns 0.48.0`, `libp2p-request-response 0.29.0`,
`libp2p-connection-limits 0.6.0`, `libp2p-allow-block-list 0.6.0`.
Plus `libp2p-stream = "0.4.0-alpha"` (not re-exported by the umbrella) and
`mainline = "=8.0.0"`.

**Never depend on `libp2p-identity`, `libp2p-core`, `libp2p-swarm`, `multiaddr` or
`futures` directly.** `libp2p-identity 0.3.0` exists and is outside the umbrella's
`^0.2.12` range; adding it links a second, incompatible `Keypair`/`PeerId` and
produces a type-mismatch error rather than a resolver error. Use the umbrella
re-exports. `libp2p-stream` is the one deliberate exception and it works because
its own ranges resolve to the same instances.

There is **no `connection-limits` cargo feature** — `libp2p-connection-limits` and
`libp2p-allow-block-list` are non-optional dependencies and
`libp2p::connection_limits` / `libp2p::allow_block_list` are always available.
`memory-connection-limits` (with hyphens) *is* a feature. Asking for
`connection-limits` is a hard resolver error.

> Verification: [RESEARCH+COMPILED] `LIBP2P.md` §1, §8, including the reproduced
> resolver error.

#### 5.1.1 Dependency register — the transport side (`SPEC_CS.md` §28)

`SPEC_CS.md` §28 requires a register with six columns for every crate the client
links. The permanent home is **`docs/DEPENDENCIES.md`**, generated from
`cargo metadata` and checked in CI so it cannot drift; until that document exists,
this section carries the transport side and `CRYPTOGRAPHY.md` §9 carries the
cryptographic side. **Neither is complete on its own**, and both are hand-written
and therefore subject to exactly the drift that `PHASE0_REVIEW.md` B-3 found.

| Crate | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `libp2p` (umbrella) | `0.56.0` | transport, encryption, NAT traversal, gossip, relay | `github.com/libp2p/rust-libp2p` | MIT | RUSTSEC-2022-0084 (resource-management DoS) patched at `>= 0.45.1`; pinned version is patched |
| `libp2p-core` | `0.43.2` | transport traits, upgrades | as above | MIT | RUSTSEC-2019-0004 patched `>= 0.8.1`, RUSTSEC-2022-0009 patched `>= 0.31.1`; both far below the pinned version |
| `libp2p-identity` | `0.2.14` | `PeerId`, `Keypair` | as above | MIT | no advisory in the local advisory database |
| `libp2p-swarm` `0.47.1`, `libp2p-swarm-derive` `0.35.1` | — | swarm driver, `#[derive(NetworkBehaviour)]` | as above | MIT | as above |
| `libp2p-quic` `0.13.1`, `libp2p-tcp` `0.44.1`, `libp2p-dns` `0.44.0` | — | base transports (§5.3) | as above | MIT | as above |
| `libp2p-noise` `0.46.1`, `libp2p-tls` `0.6.2`, `libp2p-yamux` `0.47.0` | — | security and muxer upgrades (mandatory for relay, §5.3) | as above | MIT | as above |
| `libp2p-gossipsub` | `0.49.5` | lobby topics (§6) | as above | MIT | as above |
| `libp2p-identify` `0.47.0`, `libp2p-ping` `0.47.0` | — | address candidates, liveness (§5.5) | as above | MIT | as above |
| `libp2p-autonat` `0.15.0`, `libp2p-dcutr` `0.14.1`, `libp2p-relay` `0.21.1` | — | reachability, hole punching, relay (§9) | as above | MIT | as above |
| `libp2p-request-response` | `0.29.0` | snapshot RPC (§7), join RPC (§8.4) | as above | MIT | as above |
| `libp2p-mdns` `0.48.0`, `libp2p-upnp` `0.5.0` | — | LAN discovery (§9.8), IGD mapping (§9.9) | as above | MIT | as above |
| `libp2p-connection-limits` `0.6.0`, `libp2p-memory-connection-limits` `0.5.0`, `libp2p-allow-block-list` `0.6.0` | — | resource limits and blocklist (§11) | as above | MIT | as above |
| **`libp2p-stream`** | **`0.4.0-alpha`** | per-table streams (§8.1) | as above | MIT | **unaudited and semver-exempt.** An alpha crate carries no stability guarantee; contained behind the §1.3 trait so replacing it is a one-file change |
| `mainline` | `=8.0.0` | Mainline DHT client (§3, §4, §11.4) | `github.com/pubky/mainline` | MIT | no advisory in the local advisory database; version-pinned with `=` because §11.4 depends on internals (`RequestFilter`, adaptive server mode) that are not semver-stable in practice |
| `web-time` | `1` | `Instant` in the `RateLimiter` signature (§9.6) | `github.com/daxpedda/web-time` | MIT OR Apache-2.0 | no advisory in the local advisory database |

The two entries a reader must not skip are **`libp2p-stream 0.4.0-alpha`** here
and **`ziffle 0.1.0`** in `CRYPTOGRAPHY.md` §9: those are the corpus's two
unaudited, semver-unstable dependencies, and they are flagged explicitly in both
places.

> Verification: [SOURCE] `license` and `repository` fields read from each crate's
> own `Cargo.toml` under
> `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`;
> advisories read from the local RustSec advisory database under
> `~/.cargo/advisory-db/crates/`. **No `cargo audit` run has been performed
> against the assembled transport tree** — the tree does not exist yet, there is
> no `Cargo.lock`, and the statements above are per-crate lookups, not a tree
> audit. The tree audit is `docs/DEPENDENCIES.md`'s CI job (D-2), Phase 7.

### 5.2 Features

```toml
libp2p = { version = "0.56.0", features = [
    "tokio", "macros",
    "quic", "tcp", "noise", "tls", "yamux", "dns",
    "gossipsub", "identify", "ping", "autonat", "dcutr", "relay",
    "request-response", "cbor",
    "mdns", "upnp", "memory-connection-limits",
    "ed25519", "serde",
] }
libp2p-stream = "0.4.0-alpha"
mainline = { version = "=8.0.0", default-features = false, features = ["async"] }
```

`kad` is **not** enabled in v1 — see §5.7.

### 5.3 Why QUIC first and TCP anyway

QUIC is the primary transport: UDP (so hole punching is possible at all — and
`libp2p-quic 0.13.1` has a real `hole_punching` module wired into the transport
[SOURCE]), 1-RTT handshake, native stream multiplexing with no head-of-line
blocking across per-table streams.

TCP stays in the build for three structural reasons, not as a preference:

1. UDP is blocked outright on many corporate, hotel and school networks. A
   QUIC-only client there reaches nothing, not even a relay, because the relay
   itself must be dialled over a base transport.
2. It costs almost nothing: `noise`, `tls` and `yamux` are **already mandatory**
   because `with_relay_client` needs a security upgrade and a muxer upgrade even
   when QUIC is the only base transport — a relayed connection is a stream through
   the relay, not a QUIC connection. This was proven by removing `yamux` from a
   QUIC-only probe and watching the build fail with
   `could not find yamux in libp2p … found an item that was configured out`.
3. Peers learned from the DHT or from a relay may be listening only on TCP.

Note honestly: TCP does **not** rescue discovery, because the Mainline DHT is UDP.
On a UDP-blocked network the client cannot discover anybody at all (§12).

> Verification: [RESEARCH+COMPILED] `LIBP2P.md` §9 (both the failing and passing
> states of the QUIC-only probe), §5 (`hole_punching` module).

### 5.4 The builder shape

Compiled and executed as written in `probe-netstack`:

```rust
let swarm = SwarmBuilder::with_existing_identity(keypair)
    .with_tokio()
    .with_tcp(
        tcp::Config::default().nodelay(true),
        (tls::Config::new, noise::Config::new),   // function pointers, not values
        yamux::Config::default,
    )?
    .with_quic_config(|mut c| {
        c.handshake_timeout   = Duration::from_secs(10);
        c.max_idle_timeout    = 30_000;           // u32 milliseconds
        c.keep_alive_interval = Duration::from_secs(5);
        c
    })
    .with_dns()?
    .with_relay_client((tls::Config::new, noise::Config::new), yamux::Config::default)?
    .with_behaviour(|key, relay_client| { /* §5.5 */ })?   // 2 args because of relay
    .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
    .build();
```

Two traps the type-state builder sets: the upgrade arguments are **function
pointers** (`noise::Config::new`, not `noise::Config::new()`), and once
`with_relay_client` is used the `with_behaviour` closure takes a **second
argument** — the ready-made `relay::client::Behaviour` — which must be stored in
the behaviour struct or the relay transport is dead.

`with_idle_connection_timeout` is set to 60 s deliberately: the default is much
shorter and silently closes lobby connections that are merely quiet. `ping` keeps
NAT mappings warm; consumer-router UDP mappings commonly expire in 30–120 s, so the
keep-alive interval must stay well under 30 s and `ping`'s default is suitable —
do not raise it.

### 5.5 The behaviour struct

```rust
#[derive(NetworkBehaviour)]
struct PokerBehaviour {
    gossipsub:      gossipsub::Behaviour,                     // §6
    identify:       identify::Behaviour,                      // prerequisite for AutoNAT v2 + DCUtR
    ping:           ping::Behaviour,                          // liveness, keep-alive
    autonat_client: autonat::v2::client::Behaviour,           // §9.2
    autonat_server: autonat::v2::server::Behaviour,
    dcutr:          dcutr::Behaviour,                         // §9.4
    relay_client:   relay::client::Behaviour,                 // from the builder
    relay_server:   relay::Behaviour,                         // §9.6, off by default
    snapshot:       request_response::cbor::Behaviour<SnapshotRequest, SnapshotResponse>,  // §7
    join:           request_response::cbor::Behaviour<JoinRequest, JoinResponse>,          // §8.4
    table_streams:  libp2p_stream::Behaviour,                 // §8
    conn_limits:    connection_limits::Behaviour,             // §11
    mem_limits:     libp2p::memory_connection_limits::Behaviour,
    blocklist:      libp2p::allow_block_list::Behaviour<libp2p::allow_block_list::BlockedPeers>,
    mdns:           libp2p::mdns::tokio::Behaviour,           // §9.8
    upnp:           libp2p::upnp::tokio::Behaviour,           // best effort
}
```

`#[derive(NetworkBehaviour)]` generates `PokerBehaviourEvent` with one variant per
field, named in PascalCase of the field name (`relay_client` → `RelayClient`).

**`identify` is a hard prerequisite, not decoration.** The address-candidate chain
is: `identify` receives the remote's `observed_addr` and emits
`ToSwarm::NewExternalAddrCandidate`; the swarm turns that into
`FromSwarm::NewExternalAddrCandidate`; `autonat::v2::client` consumes it, probes
it, and on success emits `ToSwarm::ExternalAddrConfirmed`; `dcutr` consumes the
same event to fill its LRU(20) of hole-punch candidates. Without `identify` in the
behaviour, neither AutoNAT v2 nor DCUtR ever sees a candidate and both are inert.

**`libp2p::autonat::Behaviour` silently means v1.** The crate re-exports v1 at the
module root. Only `autonat::v2::*` paths may appear in our source; enforce it as a
review rule.

> Verification: [COMPILED+RUN] `probe-netstack` builds exactly this struct with
> every field constructed as specified, subscribes both lobby topics, registers the
> table stream protocol, and binds both listeners. `cargo check` finishes clean;
> `cargo run` prints `ALL OK`. [SOURCE] the candidate chain, `LIBP2P.md` §3 with
> file:line references into `libp2p-identify-0.47.0`, `libp2p-autonat-0.15.0` and
> `libp2p-dcutr-0.14.1`.

### 5.6 Persistent identity, and the publish filter

`identity::Keypair::to_protobuf_encoding() -> Result<Vec<u8>, DecodingError>` and
`from_protobuf_encoding(&[u8])` are the persistence pair; a round trip preserves
the `PeerId` (68 bytes for Ed25519, measured). Two installations must never share a
keypair (`SPEC_CS.md` §3); the file lives in the portable profile directory beside
the executable and is DPAPI-wrapped on Windows (`SPEC_CS.md` §21, §22).

**Publish filter** (distinct from the dial filter of §4.5): before an address is
announced to the DHT or advertised in the lobby it must be a globally routable
address. AutoNAT v2 has **no** private-address guard — `only_global_ips` exists in
v1 and has no v2 equivalent, and v2 was measured confirming
`/ip4/192.168.1.21/udp/55598/quic-v1` as an external address because the dial-back
came from a LAN peer [MEASURED]. So the filter is ours to implement, and it must
also drop known-virtual adapter ranges: a Hyper-V "Default Switch" address was
measured being advertised, dialled, and burning a full handshake timeout
[MEASURED].

> Verification: [COMPILED+RUN] keypair round trip in `probe-netstack`
> (`persistent identity round-trip OK (68 bytes)`); [SOURCE]
> `libp2p-identity-0.2.14/src/keypair.rs`; [MEASURED] `NAT_AND_DISCOVERY.md`
> §2.4(ii), §5.3.

### 5.7 libp2p Kademlia is not enabled in v1

`kad` composes cleanly (it was compiled in both `LIBP2P.md`'s probe and an earlier
revision of `probe-netstack`), but it is **not** shipped in v1. `SPEC_CS.md` §1
names the Mainline DHT as the discovery mechanism; a second DHT is a second eclipse
surface, a second maintenance burden, and has no mandated role here. Peer-set
resilience comes instead from: the 10-minute DHT re-lookup, the listen addresses
learned through `identify` from already-connected peers, mDNS on the LAN, and the
GossipSub mesh itself. Revisit only if Phase 8 measures poor lobby connectivity
(OQ-4).

### 5.8 IPv4-only discovery

`mainline`'s `KrpcSocket::new` does
`SocketAddr::V6(_) => unimplemented!("KrpcSocket does not support Ipv6")` [SOURCE].
Mainline is an IPv4 network; BEP 32's IPv6 DHT is a separate table nobody
implements widely. So the libp2p layer may be dual-stack, but **discovery is
IPv4-only**. A client on an IPv6-only network cannot find anybody through the DHT.
This was not measurable here — the development machine has no usable IPv6 at all
[MEASURED] — and is carried as a limitation, not solved (§12).

---

## 6. The GossipSub lobby

### 6.1 Topics

| Topic (`IdentTopic`) | Carries | Publisher | Cadence |
|---|---|---|---|
| `/p2p-poker/lobby/1` | `LOBBY_TABLE_AD`, `LOBBY_TABLE_REMOVE`, player presence heartbeat | table founder / each client | ad every 30 s, presence every 40 s (§10) |
| `/p2p-poker/lobby-chat/1` | lobby chat (`SPEC_CS.md` §22 requires chat) | any client | user-driven, rate-limited |

Chat is a **separate topic** so that chat volume can never crowd out table
discovery, so it can be muted without losing the lobby, and so its scoring and rate
limits are independent. `IdentTopic` (topic string on the wire in the clear) is
used because the strings are public constants anyway and debuggability is worth
more than a hash here.

Per-table traffic is **never** on a lobby topic (§8.3).

### 6.2 Configuration

```rust
gossipsub::ConfigBuilder::default()
    .validation_mode(gossipsub::ValidationMode::Strict)
    .validate_messages()
    .message_id_fn(content_hash_of(data, topic))
    .max_transmit_size(65_536)                       // = the crate default; see below
    .heartbeat_interval(Duration::from_secs(1))
    .mesh_n(8).mesh_n_low(6).mesh_n_high(12).mesh_outbound_min(3)
    .duplicate_cache_time(Duration::from_secs(120))
    .flood_publish(false)
    .build()?
```

| Setting | Value | Why |
|---|---|---|
| `MessageAuthenticity` | `Signed(libp2p keypair)` | every message carries author + seqno + signature |
| `validation_mode` | `Strict` | reject anything unsigned or missing author/seqno |
| `validate_messages()` | on | **mandatory**: nothing is forwarded until the application has validated it (`SPEC_CS.md` §4 anti-spam) |
| `message_id_fn` | hash of `(data, topic)` | **mandatory**: the default id is `source ‖ seqno`, so one peer can republish byte-identical content forever under new seqnos and it is never deduplicated |
| `max_transmit_size` | **65 536** | this is a two-sided protocol constant — a peer configured at 64 KiB rejects a 256 KiB frame — so it is pinned at the crate default, which maximises interoperability and minimises the DoS surface. Nothing we publish comes near it (§6.5). |
| `mesh_n / low / high` | 8 / 6 / 12 | denser than the 6/5/12 default; the lobby is small and latency-sensitive |
| `mesh_outbound_min` | 3 | raises the cost of an eclipse by inbound-only Sybils |
| `duplicate_cache_time` | 120 s | must exceed the 30 s ad re-broadcast interval with margin |
| `flood_publish` | **false** | the default `true` sends every publish to *every* known peer in the topic, not just the mesh — an amplification lever we do not want on a public lobby |

Subscription flooding is bounded with
`Behaviour::new_with_subscription_filter` and a `MaxCountSubscriptionFilter` /
`WhitelistSubscriptionFilter` over our two topic names, so a peer cannot subscribe
us into thousands of junk topics.

> Verification: [COMPILED+RUN] the whole builder chain plus `subscribe` on both
> topics in `probe-netstack`; [RESEARCH+SOURCE] `LIBP2P.md` §6 for the defaults and
> the `message_id_fn` trap.

### 6.3 Two signatures, never one

GossipSub's `Strict` mode authenticates **the libp2p identity keypair that owns
the `PeerId`** — i.e. *which socket said this*. `SPEC_CS.md` §12 requires a
**separate application signing keypair** over canonically serialised bytes, and
`SPEC_CS.md` §4 requires the table advertisement itself to be signed. These are two
independent signatures and both are required. `ValidationMode::Strict` must never
be allowed to create the impression that a message is application-authentic.

Concretely: a peer relaying somebody else's validly-signed table ad is normal and
correct — the gossip signature will be the *forwarder's*, and the application
signature will be the *founder's*. Validation looks at the application signature.

### 6.4 Validation and the accept/reject decision

For every received lobby message, in this order, before anything is forwarded:

1. size cap (§6.5) — over-size is rejected at the transport by
   `max_transmit_size`; the application re-checks its own per-type cap, which for
   a lobby chat message is `LOBBY_CHAT_MAX` and for an advert is `TABLE_AD_MAX`;
2. deterministic CBOR decode into the declared schema; any trailing bytes,
   non-canonical encoding, unknown required field, or collection over its cap is a
   parse failure (`SPEC_CS.md` §16, §27);
3. `protocol_version` matches;
4. **the envelope is unchained.** `chain_scope == 0`, and `table_id`, `hand_id`
   and `sequence` carry the unchained sentinels of `PROTOCOL.md` §2.3/§2.4 —
   `table_id = ZERO32`, `hand_id = 0xFFFF_FFFF_FFFF_FFFF`,
   `previous_event_hash = ZERO32`, `sequence = 0`. Anything else on a lobby topic
   is rejected. Every lobby message type is unchained
   (`LOBBY_TABLE_AD`, `LOBBY_TABLE_REMOVE`, `LOBBY_PLAYER_PRESENCE`,
   `LOBBY_CHAT`), so a chained envelope arriving here is either a bug or an
   attempt to make lobby traffic collide with the chain namespace that
   `PROTOCOL.md` §5.2's equivocation predicate is defined over. The table an
   advert concerns is named in its **payload** and by
   `sender_public_key == table_public_key`, never in the envelope;
5. application signature verifies against the declared public key;
6. for a table ad, the signing key **is** the ad's `table_public_key`;
7. `timestamp` is not more than a small skew allowance in the future;
   `expires_at > timestamp` and `expires_at` is not more than ~5 minutes ahead of
   local time (otherwise a malicious peer pins a table in every lobby forever);
8. per-peer, per-type rate budget not exceeded (§6.6).

Then:

```rust
gossipsub.report_message_validation_result(&message_id, &propagation_source, acceptance);
```

| Outcome | `MessageAcceptance` | Effect |
|---|---|---|
| all checks pass | `Accept` | forwarded to the mesh |
| signature invalid, schema invalid, expired, version mismatch, over cap | `Reject` | not forwarded; applies the topic P₄ invalid-message penalty **if** topic score params are configured (§6.7) |
| valid but uninteresting (duplicate we already hold, table we already have at a newer timestamp) | `Ignore` | not forwarded, no penalty |

### 6.5 Size caps

| Message | Constant | Application cap | Note |
|---|---|---|---|
| `LOBBY_TABLE_AD` | `TABLE_AD_MAX` | 1 024 B | **payload** cap. The fields of `SPEC_CS.md` §4 plus a custom-parameter block total under 500 B at worst case, so this is over 2× headroom |
| a **complete signed** `LOBBY_TABLE_AD` | `TABLE_AD_SIGNED_MAX` | **1 536 B** | payload (≤ 1 024) + envelope + the 64-byte signature. This is the cap applied wherever a whole signed advert is **forwarded or embedded** rather than freshly parsed: the snapshot elements of §7.3, and `JOIN_ACCEPT`'s `advert_event` (`PROTOCOL.md` §4.3). The two caps bound different objects, and conflating them is what produced the 2 560 / 1 024 conflict `PHASE0_REVIEW.md` C-1 found |
| `LOBBY_TABLE_REMOVE` | — | 256 B | |
| presence heartbeat | — | 256 B | |
| lobby chat message | `LOBBY_CHAT_MAX` | 2 048 B | payload cap. The message type is `PROTOCOL.md` §4/§7's `0x0106 LOBBY_CHAT`; its `display_name` ≤ 32 B and `text` ≤ 512 B are display-only strings under `PROTOCOL.md` §9.4's string rules |
| any lobby message | `LOBBY_MSG_MAX` | **8 192 B** hard ceiling | anything larger is a protocol error regardless of type |

The 8 KiB application ceiling sits far below the 64 KiB transport ceiling, so the
transport limit is a backstop and never the operative limit. A lobby snapshot does
**not** go over gossip — see §7.

### 6.6 Rate limits (application-level, `SPEC_CS.md` §4)

Per remote `PeerId`, token buckets refilled continuously:

| Budget | Rate | Burst |
|---|---|---|
| table ads (any table) | 1 per 10 s | 4 |
| distinct `table_id`s advertised by one identity | 4 concurrently | — |
| presence heartbeats | 1 per 20 s | 2 |
| chat messages | 1 per 2 s | 5 |
| total lobby bytes from one peer | 8 KiB/s | 64 KiB |

Exceeding a budget produces `Ignore` for the excess (not `Reject` — the message may
be perfectly valid, we simply refuse to spend on it), and repeated excess
downgrades the peer locally: stop dialling it, then disconnect. Proven protocol
violations (an invalid signature on a message the peer originated, or an
`EquivocationProof` in the `PROTOCOL.md` §5.2 sense — two conflicting **chained**
events, `SPEC_CS.md` §14) go further and land the peer in
`allow_block_list::Behaviour<BlockedPeers>` via `block_peer`. The lobby founder
contradiction of §7.4 is **not** such a proof and never reaches `block_peer`.

### 6.7 Peer scoring — honest status

`Behaviour::with_peer_score(PeerScoreParams, PeerScoreThresholds)` exists and is
callable; `set_topic_params` installs per-topic parameters
[SOURCE `libp2p-gossipsub-0.49.5/src/behaviour.rs:922,954`]. It compiles and runs
with the defaults in `probe-netstack`.

But **the defaults do almost nothing for us**: `PeerScoreParams::default()` has
`topics: HashMap::new()` [SOURCE `src/peer_score/params.rs:164-183`], so the
per-topic terms — including P₄, `invalid_message_deliveries_weight`, the term a
`Reject` actually feeds — contribute **zero** until a `TopicScoreParams` is
installed for each topic. Only `app_specific_weight`, IP-colocation
(`-5.0` above 10 peers per IP) and the behaviour penalty are live by default.

So: scoring is enabled from day one with defaults, which buys IP-colocation and
behaviour penalties, and the §6.6 application rate limits are what actually
controls spam in v1. Deriving safe `TopicScoreParams` for our topics requires
knowing real message rates and mesh sizes, which we do not have. Getting them wrong
is worse than not setting them — badly tuned scoring graylists honest peers.
Carried as OQ-5, to be set in Phase 8 against measured traffic.

---

## 7. Snapshot sync for a newly joined client

`SPEC_CS.md` §3: *a newly connected client requests a snapshot of already-existing
tables from several peers over libp2p, and only then continues with live GossipSub
synchronisation.*

### 7.1 Mechanism

`request_response::cbor::Behaviour<SnapshotRequest, SnapshotResponse>` on protocol
`/p2p-poker/lobby-snapshot/1`. This is a genuine one-shot RPC: timeouts,
correlation and size caps come free, and CBOR aligns with the canonical
serialisation of `SPEC_CS.md` §12. It is **not** a gossip message, for two reasons:
a lobby with hundreds of tables blows past any sane gossip frame, and a snapshot is
addressed to one peer, not broadcast.

```rust
let codec = request_response::cbor::codec::Codec::<SnapshotRequest, SnapshotResponse>::default()
    .set_request_size_maximum(1024)              // SNAPSHOT_REQ_MAX
    .set_response_size_maximum(256 * 1024);      // SNAPSHOT_RESP_MAX = 262 144
let snapshot = request_response::Behaviour::with_codec(
    codec,
    [(StreamProtocol::new("/p2p-poker/lobby-snapshot/1"), request_response::ProtocolSupport::Full)],
    request_response::Config::default()
        .with_request_timeout(Duration::from_secs(15))
        .with_max_concurrent_streams(8),
);
```

The size caps are **mandatory overrides**: the codec defaults are 1 MiB request /
10 MiB response [SOURCE `libp2p-request-response-0.29.0/src/cbor.rs:78`], far too
generous for a lobby snapshot and a free memory-exhaustion lever. The request
payload is ~45 B and capped at 128 B by `PROTOCOL.md` §9.3, so 1 024 B is envelope
headroom and nothing more; slack in a request cap is DoS surface, not safety
margin.

> Verification: [COMPILED+RUN] exactly this construction, for both the snapshot and
> the join protocol, in `probe-netstack`; [SOURCE] `with_codec` at
> `libp2p-request-response-0.29.0/src/lib.rs:395`, the setters at `src/cbor.rs:97,103`.

### 7.2 Who is asked, and how many

* **K = 4** peers, with a floor of 2 usable responses to consider the snapshot done.
* Chosen from peers that have completed `identify` and advertise `/p2p-poker/1`.
* Prefer peers in **distinct IPv4 /24 prefixes** and, where known, discovered from
  **different DHT responders** — a cheap and partial defence against being handed
  four Sybils by one flooder.
* Ask all K concurrently. 15 s timeout each.
* If fewer than 2 respond, retry once with a fresh set, then continue on live
  gossip alone and show "lobby syncing" in the GUI. Never block the UI.
* Re-run a snapshot after any period of >5 minutes with zero lobby traffic, and
  after reconnecting from a full disconnect.

### 7.3 What a snapshot contains

`SnapshotResponse { protocol_version, ads: Vec<Vec<u8>> }` where each element is a
complete, independently signed `LOBBY_TABLE_AD` **exactly as it was gossiped** —
the responder forwards the original signed bytes and never re-serialises,
re-signs or summarises them. Caps: at most **128** ads (`SNAPSHOT_MAX_ADS`), at
most **262 144 B** total (`SNAPSHOT_RESP_MAX`), each ad's **payload** ≤ 1 024 B
(`TABLE_AD_MAX`) and each **complete signed** ad ≤ 1 536 B
(`TABLE_AD_SIGNED_MAX`). The count and the total are chosen together:
`128 × 1 536 = 196 608 B`, which leaves room for the array and envelope overhead
inside 262 144 B.

A snapshot carries **no game state**, no hand history, no player secrets, no
cryptographic material. It is a set of table advertisements and nothing else.

### 7.4 Conflict resolution

1. **Every ad is validated independently**, by the rules of §6.4. An ad's validity
   is a property of the ad, never of who sent it.
2. **Union, not vote.** The result is the union of all valid ads from all
   responders. An ad reported by one peer is exactly as valid as one reported by
   all four. *Counting is never evidence.*
3. **Per `table_id`, keep the validly signed ad with the highest `timestamp`.** An
   ad with a lower `timestamp` for a `table_id` we already hold is discarded, which
   also blunts replay of stale ads (`SPEC_CS.md` §14).
4. **Two different validly signed adverts from the same `table_public_key` with
   the same `timestamp_unix_ms` and different bodies are a founder
   contradiction.** The table is marked `EQUIVOCATION`, both are retained as
   evidence, and it is never joinable. This is **not** an `EquivocationProof` in
   the `PROTOCOL.md` §5.2 sense — lobby messages are unchained
   (`chain_scope = 0`, §6.4), and §5.2's predicate is defined only over chained
   events — and it **must not be called one**. It is a local lobby-hygiene rule
   with no chip and no blocklist consequence beyond refusing the table; in
   particular it does not feed `block_peer` (§11.5) and it does not attribute
   anything under D-005. The meaning it does have comes from the *founder's own
   two signatures*, not from any count of peers.
5. **An ad signed by any key other than its `table_public_key` is discarded.**
6. Local eviction then proceeds by §10.3 (relative freshness), not by what a
   snapshot said.

### 7.5 Why a snapshot peer cannot become an authority

A responder's only powers are **omission** and **delay**. It cannot:

* create a table ad — it lacks the founder's application key;
* alter a table ad — the signature covers the canonical bytes;
* revoke a table ad — a `LOBBY_TABLE_REMOVE` must be signed by the founder, and a
  table also disappears by TTL with no cooperation at all (§10.3);
* make us prefer its ads — nothing in the merge is weighted by sender;
* learn anything about a hand — snapshots carry no game state.

Omission is bounded by asking K independent peers and by live gossip converging
within seconds afterwards. It is **not eliminated**: a client whose entire peer set
is controlled by one adversary sees whatever that adversary chooses to show. That
is an eclipse attack, it is a **liveness and visibility** failure rather than an
integrity failure — the adversary still cannot forge a table or a hand — and it is
**not solved** at this layer (§12).

---

## 8. Per-table transport

`SPEC_CS.md` §1: *the poker game state is sent only between the participants of
that table, over direct libp2p streams, never over the Mainline DHT.*

### 8.1 Mechanism

`libp2p-stream` on protocol `/p2p-poker/table/1`: long-lived, bidirectional, both
sides push. `request-response` cannot express server push and one substream per
event would be wasteful, and a hand-written `NetworkBehaviour` +
`ConnectionHandler` is explicitly not warranted (`SPEC_CS.md` §2 asks for a minimal
addition over an existing crate, not a new protocol).

```rust
const TABLE_PROTOCOL: StreamProtocol = StreamProtocol::new("/p2p-poker/table/1");

let mut control  = swarm.behaviour().table_streams.new_control();   // Control: Clone + Send
let mut incoming = control.accept(TABLE_PROTOCOL)?;                 // Stream<Item = (PeerId, Stream)>
// outbound: control.open_stream(peer, TABLE_PROTOCOL).await?   — auto-dials if not connected
```

Three operational rules that follow from the crate:

* **Hand every accepted stream to a spawned task immediately.** The crate's own
  README warns it will *drop* streams if the application falls behind on the
  incoming iterator.
* **Dropping `IncomingStreams` de-registers the protocol** and later inbound
  negotiations fail — hold it for the process lifetime.
* `OpenStreamError` is `#[non_exhaustive]`; a wildcard arm is mandatory.

The crate is **`0.4.0-alpha` and semver-exempt**. It is contained behind the
`net/streams.rs` trait (§1.3) so that replacing it is a one-file change.

> Verification: [COMPILED+RUN] `new_control()` and `accept(TABLE_PROTOCOL)` in
> `probe-netstack`; [RESEARCH] `LIBP2P.md` §7 for the backpressure and
> de-registration behaviour and the alpha risk.

### 8.2 Topology: full mesh, and it is a security requirement

Every participant of a table maintains a direct stream to **every other**
participant. `n(n−1)/2` connections; 45 for a ten-seat table, 1 for heads-up.

A star through the table founder would be cheaper and is **rejected**: it would
make the founder a relay of other players' signed events, which (a) lets the
founder selectively withhold or delay events, and (b) weakens equivocation
detection, because equivocation is detected precisely by two participants
comparing what the *same signer* sent them. If everything passes through one node,
that node chooses what everyone compares. `SPEC_CS.md` §14 requires equivocation to
produce a cryptographic proof; that requires every participant to receive every
event from its own signer over a path the signer chose.

A peer **may** additionally forward a signed event it received, as a resilience
measure when a direct pair link is broken. Forwarding never changes validity: the
signature and the hash chain decide, and a forwarded event is accepted on exactly
the same terms as a directly received one.

### 8.3 What must never carry table traffic

| Channel | Why not |
|---|---|
| **Mainline DHT** | it stores 6 bytes of `IP:port`, is world-readable and unauthenticated, and has no confidentiality of any kind. `SPEC_CS.md` §1 forbids it outright. |
| **GossipSub lobby topic** | it is world-readable by every client in the lobby, which would publish table traffic to non-participants, and a hand's **table-wide** shuffle traffic (18 KB heads-up, 54 KB six-handed [RESEARCH `MENTAL_POKER.md` §5.1]) would flood a topic sized for 1 KiB ads. The table-wide figure is the right one here, because a broadcast topic carries every seat's traffic to everyone; it is the wrong one against a per-circuit relay cap, which is the error §9.5 corrects. |
| **A relay, as an authority** | permitted as a byte pipe (D-001) and never as a participant: it holds no key share, sees no plaintext, arbitrates nothing. |

### 8.4 Framing, admission and membership

* **Join** goes over `request-response` on `/p2p-poker/join/1`
  (`JOIN_REQUEST` / `JOIN_ACCEPT` / `JOIN_REJECT`, `SPEC_CS.md` §16),
  **`JOIN_REQ_MAX` = 4 096 B request and `JOIN_RESP_MAX` = 16 384 B response**,
  20 s timeout. The two are deliberately asymmetric: `JOIN_REQUEST`'s payload cap
  is 512 B (`PROTOCOL.md` §9.3) so 4 096 B is envelope headroom only, while
  `JOIN_ACCEPT` embeds a complete signed advert (≤ `TABLE_AD_SIGNED_MAX` = 1 536 B)
  plus a ten-entry roster and needs the room. `PLAYER_LIST` and `TABLE_READY` do
  **not** travel on this RPC — see `PROTOCOL.md` §4.3.
* **Framing on the table stream is ours**, not the transport's: `u32` big-endian
  length prefix followed by a deterministic-CBOR body. Max frame
  **`TABLE_FRAME_MAX` = 262 144 bytes** (256 KiB). The binding case is **not** the
  shuffle: it is `DISPUTE`, whose four evidence entries of `MAX_EMBEDDED_EVENT` =
  32 768 B each plus envelope exceed 128 KiB on their own, and `HAND_ABORT`'s
  80 000 B behind it (`PROTOCOL.md` §9.3). A 131 072 B frame cannot carry the
  protocol's own evidence-bearing message, which is why the earlier 128 KiB
  figure — sized against `ShuffleProof<52>` at 5 547 B and `MaskedDeck<52>` at
  3 432 B [RESEARCH `MENTAL_POKER.md` §5.1] — was wrong: the shuffle objects are
  an order of magnitude smaller than the binding case and never set this bound.
  256 KiB remains a hard bound for the fuzzer (`SPEC_CS.md` §27).
* **Membership gate.** A table stream from a `PeerId` that is not an admitted
  participant of that `table_id` is closed immediately, before any body is read.
  Membership comes from the signed `PLAYER_LIST` / `TABLE_READY` of the application
  protocol, never from the transport.
* **Connection loss is a hint, not a verdict.** When a table stream drops, the
  network layer reports the fact and nothing more. Whether that seat is absent is
  decided above, by the D-006 timeout certificate signed by the other still-active
  players — **which at two seats is a single player, so D-007 makes it advisory;
  the transport layer's behaviour is unchanged either way**. At `n = 2` a
  heads-up action deadline is a UI countdown that produces no signed state
  transition at all (D-007 point 1, `PROTOCOL.md` §8.3), and the transport must
  not invent one to fill the gap. At `n ≥ 3` an action timeout is an auto
  check/fold and never an abort, and no timeout of any kind ends the tournament or
  the cash game. Only a client that is gone or withholding decryption shares
  reaches the D-005 abort path. The transport must not shortcut any of that.

---

## 9. NAT traversal and reachability

### 9.1 The structural finding that orders everything else

**DCUtR only engages on a connection that is already relayed.**
`libp2p-dcutr-0.14.1` installs its real handler only when `is_relayed(addr)` —
literally `addr.iter().any(|p| p == Protocol::P2pCircuit)` — and installs
`dummy::ConnectionHandler` otherwise. So: no relay ⇒ no DCUtR ⇒ no coordinated hole
punch. A relay is on the critical path for *establishing* many NAT-to-NAT
connections even when the resulting connection ends up fully direct.

> Verification: [SOURCE] `libp2p-dcutr-0.14.1/src/behaviour.rs:179,214,385`
> (`handle_established_{in,out}bound_connection`), `:45` for
> `MAX_NUMBER_OF_UPGRADE_ATTEMPTS = 3`, `:340,355-386` for the candidate LRU fed
> only by `FromSwarm::NewExternalAddrCandidate`.

This does **not** contradict D-004 layer 1: blind *mutual dialling* needs no relay
at all, because the DHT delivers both addresses to both peers symmetrically. Layer
1 is not DCUtR; it is two clients dialling each other at the same time. DCUtR is
layer 2, and layer 2 needs a relay.

### 9.2 AutoNAT: v2 only

`libp2p 0.56.0`'s single `autonat` feature enables **both** v1 and v2 (the
sub-crate's `default = ["v1","v2"]` and the umbrella does not disable defaults).
We use **v2 only**:

* v1 stayed `Unknown` with `confidence: 0` for the entire measured run, because
  `only_global_ips: true` refuses LAN peers as probe servers, and its confidence
  time is `boot_delay 15 s + 3 × retry_interval 90 s ≈ 5 minutes` [MEASURED +
  SOURCE]. Startup must never gate on it.
* v2's `probe_interval` default is 5 s with `max_candidates` 10, and its server
  **always dials back over a freshly allocated port**, which removes v1's false
  "public" verdicts on already-hole-punched ports.
* v2 has **no private-address guard** and was measured confirming an RFC 1918
  address as external. Filtering is ours (§5.6). This is mandatory, not advisory:
  publishing a private address to the DHT poisons the lobby for everyone.

Configuration used: `Config::default().with_probe_interval(30 s).with_max_candidates(8)`.
Both the client and the server behaviour are enabled — a publicly reachable client
answering AutoNAT probes for others costs almost nothing and the network needs
servers to exist.

> Verification: [COMPILED+RUN] both behaviours constructed in `probe-netstack`;
> [MEASURED + SOURCE] `NAT_AND_DISCOVERY.md` §2.2–§2.4, `LIBP2P.md` §3.

### 9.3 Reachability classes and what each can do

| Class | Sees the lobby | Dialable | Can play | Notes |
|---|---|---|---|---|
| Public IP / port-forwarded | yes | yes | yes | can volunteer as a relay (D-002); materially helps everyone |
| Full-cone / endpoint-independent NAT | yes | after a punch | yes — layers 1, 2 | the measured development machine: endpoint-independent mapping, port-preserving, address-and-port-dependent filtering [MEASURED] |
| Port-restricted cone NAT | yes | after a punch | yes — layers 1, 2 | both sides must transmit outward first |
| Symmetric NAT (one side) | yes | no | usually yes, via the other side's punch | |
| **Symmetric NAT (both sides)** | yes (layer 3) | no | **only over a raised-limit relay (D-002); otherwise no** | the case that genuinely fails, stated as D-004 requires |
| CGNAT | yes | no | as symmetric, usually | commonly symmetric; no port forward possible |
| **UDP blocked entirely** | **no** | no | only by direct invite over TCP | the Mainline DHT is UDP-only, so discovery dies with it. Not solved (§12). |
| IPv6-only | **no** | — | direct invite only | `mainline` is IPv4-only [SOURCE] |

### 9.4 DCUtR, with the honest numbers

`libp2p-dcutr 0.14.1`. Public surface is `Behaviour::new(local_peer_id)` and a
single `Event { remote_peer_id, result: Result<ConnectionId, Error> }` — there is
no "attempt started" signal.

Measured success in the wild: **70 % ± 7.1 %** over more than 4.4 million attempts
across 85 000+ networks and 167 countries (arXiv:2510.27500). **TCP and QUIC are
statistically indistinguishable, both ≈70 %** — so QUIC is chosen for 1-RTT,
multiplexing and no head-of-line blocking, *not* because UDP punches better.
**97.6 %** of successes happen on the **first** attempt, so retries buy almost
nothing; plan for the relay instead of hammering. Failure is **sticky and
peer-specific** — cache the verdict per peer and go straight to relay next time.
Regional variance is severe: individual reporters saw 1–10 %, and one saw zero
across ten people, both attributed to symmetric NAT prevalence. **A 70 % global
average can be near zero for a specific pair of players.**

`MAX_NUMBER_OF_UPGRADE_ATTEMPTS = 3`, then `Err(AttemptsExceeded(3))` and the
connection stays relayed. "Still relayed" is a normal steady state and must be
shown as such (§2.2), never as an error.

> Verification: [RESEARCH] `NAT_AND_DISCOVERY.md` §4 and §4.1 (paper fetched;
> a second, similar-titled arXiv identifier surfaced in search and was *not* used
> because it could not be fetched); [SOURCE] the crate constants and API.

### 9.5 Circuit Relay v2 — permitted, and split in two

**D-001 governs: relays are permitted.** A relay carries an already end-to-end
encrypted, mutually authenticated libp2p stream. It cannot read a poker event
(the transport terminates at the peers), cannot forge or alter one (every event is
application-signed and every receiver re-validates independently), cannot become an
authority (it is not a party to any table session and holds no key share), and
cannot decrypt a card (it holds no share of the joint deck key). A malicious relay
is at worst a network adversary already in the threat model.

Its real costs, which `THREAT_MODEL.md` must carry: **metadata exposure** (who
talks to whom, when, how much, for how long), a **liveness dependency** (if every
reachable relay is down, a CGNAT-only client cannot connect — the UI must say "no
direct route and no relay available", never disguise it), and a **DoS lever** (a
relay operator can drop a specific peer; a relayed connection loss is handled
exactly like any other disconnect, per D-005/D-006).

**The two roles must never be blurred** (D-001 addendum):

| Role | Who can serve it | Limits |
|---|---|---|
| **Rendezvous for a hole punch** — carry the DCUtR coordination, then get out of the way | any public relay, including the many public kubo nodes that enable `Swarm.RelayService` by default | the defaults are *sized for exactly this*: `max_circuit_duration` 2 min, `max_circuit_bytes` 128 KiB, `reservation_duration` 1 h, `max_reservations` 128, `max_circuits` 16, `max_circuits_per_peer` 4 — identical in kubo and in `rust-libp2p` |
| **Carrying a whole session** when DCUtR fails on both ends | only a relay whose operator raised its own limits — i.e. a D-002 volunteer poker client | the **2-minute `max_circuit_duration`** is what makes a public relay unusable for a session, not the byte cap. A session lasts far longer than two minutes, so a public IPFS relay **will** reset the connection mid-hand, which is an *engineered abort attack against ourselves* under `SPEC_CS.md` §19. |

**The byte arithmetic, corrected.** An earlier revision of this row compared
*table-wide* shuffle traffic (~18 KB heads-up, ~54 KB six-handed) against
`max_circuit_bytes`, which is a **per-circuit** cap. A table is a full mesh
(§8.2), so one circuit connects exactly one pair and carries only that peer's own
step and proof:

| Quantity | Value | Basis |
|---|---:|---|
| `ShuffleProof<52>` + `MaskedDeck<52>`, one shuffler | 8 979 B | 5 547 + 3 432, measured [RESEARCH `MENTAL_POKER.md` §5.1] |
| Shuffle traffic over **one circuit**, **one direction**, per hand | **8 979 B** | that peer's own step and proof; **independent of `n`** |
| Hands per direction against a 131 072 B public-relay budget, shuffle only | **~14** | 131 072 / 8 979 |
| The same including the signed event stream | **~10 hands** | order-of-magnitude only; measure under OQ-7 |
| A relayed peer's **total** per-hand outbound at an `n`-seat table | `(n−1) × 8 979 B` | 44 895 B at six seats — a bandwidth figure, spread over `n−1` separate budgets, **never** a single cap |
| The binding public-relay limit | **`max_circuit_duration` = 120 s**, not the byte cap | a session lasts far longer than two minutes |

So the public-relay byte budget is *comfortable* for several hands and the
duration is what breaks; the failure mode is unchanged but the reason in the
earlier text was wrong.

Two further facts to reconcile, because the research documents differ in emphasis
and D-001 outranks:

* `NAT_AND_DISCOVERY.md` §3.2 records, as a negative result, that **no curated
  public list** of open Circuit Relay v2 servers exists for third-party
  applications, and rejects hardcoding third-party relay addresses as "a central
  fallback server wearing a costume".
* D-001's addendum records that public relays nonetheless **exist in quantity**,
  because kubo enables the relay service by default on every publicly reachable
  node, and that no shipped list is needed to find them.

Both are true and they are about different things: relays are abundant, a curated
*list* is not. D-001 permits shipping a bootstrap list; this document does not use
one, because we have a better source that costs nothing and is self-healing:

**Relay candidate sources, in priority order:**

1. Peers we are already connected to whose `identify` reports
   `/libp2p/circuit/relay/0.2.0/hop` — free, and observed working at runtime
   [MEASURED `NAT_AND_DISCOVERY.md` §2.1].
2. `get_peers(RELAY_INFOHASH)` — the D-002 volunteer pool, dialled by the same
   bridge as §4. Emergent, self-healing, nothing compiled in, nobody structurally
   privileged.
3. A compiled-in bootstrap relay list — **permitted by D-001**, not shipped in v1,
   and if ever added it must be visibly labelled and always outranked by runtime
   discovery.

The client **must read the `Limit` the relay returns** rather than assume:
`relay::client::Event::{ReservationReqAccepted, OutboundCircuitEstablished,
InboundCircuitEstablished}` all carry `limit: Option<Limit>` with public
`duration()` / `data_in_bytes()` accessors. A circuit whose advertised limits
cannot carry a hand must not be used to seat a player; refuse with an honest
message instead of starting a hand that will drop mid-street.

A reservation is a **lease, not a state**: expect periodic
`ReservationReqAccepted { renewal: true, .. }` and treat its absence as loss of
reachability.

Multiaddr forms (byte-exact, `Protocol::P2pCircuit` is multicodec 290, string
`p2p-circuit`):

```
reservation (listen):  /<relay transport addr>/p2p/<RELAY_ID>/p2p-circuit
dial through a relay:  /<relay transport addr>/p2p/<RELAY_ID>/p2p-circuit/p2p/<DEST_ID>
```

More than one `p2p-circuit` in an address is rejected —
`Error::MultipleCircuitRelayProtocolsUnsupported`, no relay chaining.

Several relay **server** `Event` variants (`ReservationReqAcceptFailed`,
`ReservationReqDenyFailed`, `CircuitReqDenyFailed`, `CircuitReqOutboundConnectFailed`,
`CircuitReqAcceptFailed`) are `#[deprecated]` upstream — build no logic on them.

> Verification: [SOURCE] `libp2p-relay-0.21.1/src/protocol.rs:31-36,39-52`,
> `src/behaviour.rs` (`impl Default for Config`), `src/priv_client/transport.rs:266-300`;
> [COMPILED+RUN] the multiaddr round trips in `LIBP2P.md` §4; [RESEARCH] D-001
> addendum for the kubo defaults, `MENTAL_POKER.md` §5.1 for the per-hand bytes.

### 9.6 Acting as a relay (D-002)

A client that AutoNAT v2 confirms as publicly reachable **may** run the Circuit
Relay v2 **server** with raised limits, so a relayed connection can carry a whole
session rather than only a hole-punch coordination. This is the only way to get an
unlimited relay without the project operating infrastructure.

**Off by default.** Enabled only by an explicit, visible setting, with a first-run
disclosure in plain language: strangers' poker traffic will cross the user's
connection, at the user's bandwidth cost. The disclosure must state the ceilings
**in concrete terms**, because the raised limits below are large and a user who
agrees to "help relay" is agreeing to these numbers: *up to 128 circuits open at
the same time, each allowed to carry up to 1 GiB and to stay open for up to an hour,
and up to 64 peers holding a reservation through you.*
Slot and bandwidth ceilings are user-visible and user-settable, and the network
status panel shows how many peers are currently being relayed.

**The trap: Circuit Relay v2 is not protocol-selective.** `HOP_PROTOCOL_NAME` and
`STOP_PROTOCOL_NAME` are compile-time constants
(`/libp2p/circuit/relay/0.2.0/hop`, `/stop`) and cannot be renamed, and the
`Behaviour` decides accept-or-deny purely on resource limits and rate limiters —
there is no application ACL hook. Left alone, enabling the server makes the user an
**open relay for the entire libp2p network**, IPFS traffic included.

**The usable hook** is `Config::reservation_rate_limiters` and
`Config::circuit_src_rate_limiters`. The module `behaviour::rate_limiter` is
`pub(crate)`, but the trait itself is **re-exported at the crate root**
(`libp2p-relay-0.21.1/src/lib.rs:42`), so `libp2p::relay::RateLimiter` is a public,
nameable, implementable trait:

```rust
pub trait RateLimiter: Send {
    fn try_next(&mut self, peer: PeerId, addr: &Multiaddr, now: Instant) -> bool;
}
```

The crate also carries a blanket implementation
`impl<T: FnMut(PeerId, &Multiaddr, Instant) -> bool + Send> RateLimiter for T`
(`src/behaviour/rate_limiter.rs:56`), so a closure would coerce — but **a named
type is used instead**, per D-002's 2026-08-28 correction: admission control holds
shared mutable state (the admitted-peer set, each peer's tier, and that peer's
live-circuit count), and a closure would have to capture all of it awkwardly. The
earlier justification for the closure — that the trait could not be named from
outside — was simply false, and it is the false *reason*, not the working code,
that these documents were carrying.

The third parameter is `web_time::Instant`, so a `web-time = "1"` dependency is
needed just to write the signature. **Both** vectors must be gated: one governs
who may reserve a slot to become reachable through us, the other who may open a
circuit through us.

```rust
struct PokerPeersOnly {
    known: Arc<Mutex<HashSet<PeerId>>>,   // + tier and live-circuit counts per peer
}

impl libp2p::relay::RateLimiter for PokerPeersOnly {
    fn try_next(&mut self, peer: PeerId, _addr: &Multiaddr, _now: web_time::Instant) -> bool { … }
}

let mut relay_cfg = relay::Config {
    max_circuit_duration:      Duration::from_secs(3600),
    max_circuit_bytes:         1 << 30,        // 1 GiB
    max_reservations:          64,
    max_reservations_per_peer: 4,              // crate default
    reservation_duration:      Duration::from_secs(3600),   // crate default
    max_circuits:              128,
    max_circuits_per_peer:     9,              // = MAX_SEATS − 1
    ..Default::default()
};
relay_cfg.reservation_rate_limiters.push(Box::new(PokerPeersOnly::new(&admitted)));
relay_cfg.circuit_src_rate_limiters.push(Box::new(PokerPeersOnly::new(&admitted)));
```

**Every scalar field is stated, so no other document restates a subset.** The
`..Default::default()` therefore fills exactly two things — the crate's own
per-peer and per-IP rate-limiter vectors — and that is deliberate: our
`PokerPeersOnly` is pushed *alongside* them, never in place of them. Two of the
stated fields are corrections rather than choices:

* **`max_circuits_per_peer = 9` is forced.** A relayed peer at a ten-seat table
  needs one circuit per table-mate (§8.2 full mesh, `MAX_SEATS − 1`). The crate
  default of 4 refuses the fifth, so the D-002 relay as previously written could
  not carry the case it exists for.
* **`max_circuits = 128`** lets one volunteer serve roughly 14 relayed peers at
  full tables (128 / 9), which is the number the consent disclosure above is about.

> Verification: [SOURCE] `libp2p-relay-0.21.1/src/lib.rs:42` (the `RateLimiter`
> re-export), `src/behaviour/rate_limiter.rs:38` (the trait), `:56` (the blanket
> impl), `src/behaviour.rs:124-166` (`impl Default for Config`, giving
> `max_reservations 128`, `max_reservations_per_peer 4`, `reservation_duration`
> 1 h, `max_circuits 16`, `max_circuits_per_peer 4`, `max_circuit_duration` 2 min,
> `max_circuit_bytes: 1 << 17`), `src/behaviour/handler.rs:415-450` (the limits
> handed to each circuit). [COMPILED+RUN] `probe-netstack` builds a `relay::Config`
> with both rate-limiter vectors populated over a shared admitted-peer set,
> `relay::Behaviour::new` accepts it, and the swarm builds and runs. D-002
> independently verified the raised limits print back correctly. **The probe used
> the closure form; the named-type form above has not itself been compiled** —
> it is the same trait and the blanket impl proves the signature, but that is an
> inference, and Phase 7 must compile it.

#### Settling the open decision: relay admission

`DECISIONS.md` leaves this open and names `NETWORK_STACK.md` as the place to settle
it: admit by **`identify` protocol name**, or by **lobby presence**?

**Decision: admit by `identify` protocol name, with escalation by lobby presence.**

* **Tier A — admitted.** The peer's `identify` response lists `/p2p-poker/1`. It
  gets a reservation and is refused a **third** concurrent live circuit, plus a
  per-peer byte ceiling well below the global one.
* **Tier B — escalated.** The peer has additionally been seen publishing a
  validly-signed lobby message, or is a participant of a table we are also in. It
  gets the full raised limits, up to 9 concurrent live circuits.
* **Everyone else is refused**, which keeps generic IPFS traffic off the user's
  line — the thing D-002 actually requires.

**The per-tier circuit ceiling is not a `Config` field.** `max_circuits_per_peer`
is a single global number and is pinned at 9 above, because a Tier B peer at a
ten-seat table needs all nine. The 2-versus-9 distinction is therefore enforced
**inside the `PokerPeersOnly` implementation of `libp2p::relay::RateLimiter`**,
which sees the requesting `PeerId` and can count that peer's live circuits: Tier A
is refused beyond 2, Tier B beyond 9. A reader must not think the `Config` field
does it — the `Config` cannot express a per-tier ceiling at all, and assuming it
can is how a Tier A stranger ends up with nine circuits.

Why not lobby presence alone: it is **circular** for exactly the user D-002 exists
to help. A brand-new client behind CGNAT cannot appear in the lobby until it has
connectivity, and it cannot get connectivity without a relay. Requiring lobby
presence for admission locks out the case the feature was built for. D-002's open
question — *can a never-before-seen CGNAT peer obtain a reservation at all?* — is
answered **yes**, deliberately.

The trade-off, stated plainly: **an `identify` protocol name is self-asserted and
free to forge.** Anyone can run a libp2p node that advertises `/p2p-poker/1` and be
relayed. So the ACL is **not a security boundary**; it is a *scope limiter*. What
actually bounds abuse is the resource ceiling — total reservations, per-peer
circuits, per-peer and global byte and bandwidth caps, and the default rate
limiters (1 reservation per peer per 2 min, 1 per IP per min) which stay in place
alongside our closures. The user-visible setting must say this honestly: "relay
only for this application" means "only for peers claiming to run this application".

**A race that must be handled.** The admission closure runs when the reservation
request arrives, and it can arrive before `identify` has completed on that
connection, in which case an honest peer is denied. And a denied reservation is not
retried for us: on a reservation error the client handler sets its reservation
state back to `None` and forwards the error to the transport listener, which does
`self.close(Err(Error::Reservation(e)))` and emits `TransportEvent::ListenerClosed`
[SOURCE `libp2p-relay-0.21.1/src/priv_client/handler.rs:292-298,515-517`,
`src/priv_client/transport.rs:408,331-341`]. So **the application must observe
`SwarmEvent::ListenerClosed` for a circuit listener and re-issue `listen_on` with
backoff itself.** Whether the race actually occurs in practice, and with what
frequency, is unmeasured — carried as OQ-6.

### 9.7 The four layers of D-004, mapped onto mechanisms

D-004 requires the lobby to be visible **even if every client is behind NAT**.

| Layer | Mechanism | What it needs | Gives |
|---|---|---|---|
| **0** | Mainline DHT lookup and announce | outbound UDP only; the DHT is millions of publicly reachable BitTorrent nodes we only ever talk to outbound, and storing nodes record the source address they actually saw. `Dht::client()` participates without serving. | *knowing other players exist and where they appear from* — survives an all-NAT world unconditionally |
| **1** | **Mutual dialling** | both peers announce and both call `get_peers`, so both learn the other's external address at roughly the same time; both dial, and the outbound packets open each NAT mapping. **No relay, no coordination server** — the DHT delivered the information symmetrically. Needs endpoint-independent mapping on at least one side, and the announced port to be the *external* QUIC port (§4.4). | a direct connection, with nothing but the DHT |
| **2** | DCUtR over a public relay | a relay for the coordination only; public relays are adequate and free here | a direct connection when blind mutual dialling does not converge |
| **3** | **Lobby gossip over a public relay** | a public relay's 128 KiB / 2 min budget — ample for a few hundred bytes per table ad, and the 2-minute reset is survivable because the client simply reconnects | **lobby visibility with no punch succeeding anywhere.** This is the floor under D-003 and what makes the requirement unconditional |
| **4** | A D-002 relay with raised limits | a volunteering publicly reachable poker client | *playing a hand* when both players are unreachable and no punch worked |

Layer 1 was directly supported by measurement on the development machine: one local
UDP socket against nine STUN servers on seven distinct IPs returned the **same
external `IP:port`** every time, and the external port equalled the local port —
endpoint-independent mapping, port-preserving, on a public (non-CGNAT) residential
address [MEASURED]. One router is not a survey, but that is the common home case
and the case layer 1 is built for.

**The case that genuinely fails**, stated as D-004 requires rather than hidden:
**symmetric NAT on both ends**. A different external port per destination defeats
address prediction, so layers 1 and 2 fail. Those players still see the lobby
(layer 3) and can still play if a raised-limit relay is available (layer 4). If
neither is available, they cannot play.

### 9.8 mDNS and the same-router case

Two clients behind the same router is a case this project explicitly cares about,
and the DHT path handles it **badly**: both announce the *same* public IP with
different ports, so dialling that public `IP:port` from inside requires NAT
hairpinning, which many consumer routers do poorly.

`libp2p::mdns::tokio::Behaviour` is therefore **mandatory, not decorative**. It was
measured discovering the LAN peer on all interfaces, and mDNS records carry the
full `/p2p/<PeerId>`, so no handshake-identity step is needed for that path.
Service `_p2p._udp.local`, multicast `224.0.0.251`. The default
`query_interval` is **5 minutes**, far too slow for a lobby — two players starting
a minute apart would wait — so it is lowered to **15 s**.

The guarantee comes from never needing the router at all: both peers advertise
their RFC 1918 addresses in `identify.listen_addrs` and in mDNS and dial each other
directly on the L2 segment. This is why the dial filter (§4.5) and the publish
filter (§5.6) must stay separate.

Hazard measured on this developer machine: a Hyper-V "Default Switch" address
`172.20.160.1` was advertised, dialled, and burned a full handshake timeout because
the virtual switch is isolated. Dial candidates concurrently (libp2p already races
them), cap the per-address timeout, and exclude known-virtual interface prefixes
from what we *publish*.

> Verification: [MEASURED+COMPILED] `NAT_AND_DISCOVERY.md` §5.1–§5.3;
> [COMPILED] `mdns::Config { query_interval: 15 s }` in `probe-netstack`.

### 9.9 UPnP

`libp2p::upnp::tokio::Behaviour` (IGD port mapping) is enabled as a best-effort
nicety: when it works, the client becomes publicly reachable and helps everyone.
It is never depended on. `libp2p-upnp 0.6.0` exists but is outside the umbrella's
`^0.5.0` range; staying on 0.5.0 is fine because AutoNAT + DCUtR + relay is the
real path.

---

## 10. Heartbeat, TTL, and self-healing expiry

Three independent expiry layers. **None** may depend on a departing peer announcing
anything — a crashed or killed client must vanish without cooperation
(`SPEC_CS.md` §1).

### 10.1 Mainline DHT layer

| Parameter | Value | Basis |
|---|---|---|
| Re-announce `LOBBY_INFOHASH` | **every 10 min** | measured lifetime of an un-refreshed record: retrievable ~45 min, gone by ~50, thinning from t+30 [MEASURED, two independent samples]. 10 min gives 4–5 refreshes inside that window. |
| Also re-announce | immediately on any network change (interface up, address change), and immediately after the external address is first confirmed | the stored record is keyed to the IP the storing node saw |
| Re-run `get_peers` | same 10-minute cadence | refreshes the candidate set |
| BEP 5 token window | 5–10 min (secret rotated every 5 min, previous accepted) | 10 min sits inside it |
| "Good node" window | 15 min | 10 min sits inside it |

Nothing in `mainline` re-announces for us: `announce_peer` is a one-shot query with
no timer and no background task. The application owns the loop. Repeated announces
also *widen* the storing set rather than merely refreshing it — five announces
landed on **54** storing nodes versus 19–32 for one [MEASURED] — so a short cadence
buys redundancy, not only freshness.

**The 45-minute figure is an observation on one network on one day, on private
random infohashes in the quietest possible corner of the keyspace.** Our public
`LOBBY_INFOHASH` will sit in a busier neighbourhood and should be expected to evict
faster, because the mechanism is LRU pressure and not a protocol timer. Re-measure
in Phase 8 and shorten the interval if the real figure is materially under 45 min
(OQ-1).

### 10.2 libp2p connection layer

* `ping::Behaviour` on every connection gives liveness.
* `with_idle_connection_timeout(60 s)` — the default is much shorter and silently
  closes quiet lobby connections.
* Keep-alive must stay **well under 30 s**: consumer-router UDP mappings commonly
  expire in 30–120 s. `ping`'s default interval is suitable; do not raise it. QUIC
  `keep_alive_interval` is set to 5 s and `max_idle_timeout` to 30 s.
* Relay reservations are leases: absence of a periodic
  `ReservationReqAccepted { renewal: true, .. }` means loss of reachability
  through that relay, and the client re-reserves or picks another.

### 10.3 Lobby application layer — the authoritative one

| Item | TTL | Re-broadcast |
|---|---|---|
| `LOBBY_TABLE_AD` | **90 s** | every **30 s** by the table founder (3× margin against gossip jitter and one missed beat) |
| Player presence | **120 s** | every **40 s** |

Rules:

* On expiry, drop the entry locally. **TTL expiry is the only cleanup path.**
  `LOBBY_TABLE_REMOVE` is an optimisation for the polite case and never a
  precondition for cleanup, and no `TABLE_CLOSE` is required.
* **No peer may revoke another peer's advert.** A `LOBBY_TABLE_REMOVE` is
  accepted only when it is signed by the advert's own `table_public_key`; a
  removal signed by anything else is discarded exactly like any other
  wrongly-signed lobby message (§6.4 step 6). This is what makes the abandoned-
  formation case of `PROTOCOL.md` §4.3 safe: when a founder disappears before
  `TABLE_READY` completes, nobody holds the table key, so nobody can revoke the
  advert — and nobody needs to. It leaves every lobby by `AD_TTL_MS` (90 s) with
  no cooperation from anybody, and a client holding a `JOIN_ACCEPT` for a table
  whose advert has expired must stop displaying that table as joinable.
* The signed `expires_at` is what the lobby honours — never a peer's word that a
  table is gone.
* **Clock skew is an attack surface.** Use *relative* freshness — age since local
  receipt — for eviction, and treat `expires_at` only as an **upper bound** on how
  long we are willing to hold an entry at all. Reject any ad whose `expires_at` is
  more than ~5 minutes ahead of local time and any whose `timestamp` is in the
  future beyond a small skew allowance; otherwise a malicious peer pins a table in
  every lobby forever.
* Reconcile by `(table_id, timestamp)`, keeping the newest validly signed ad per
  `table_id`; a lower `timestamp` for a `table_id` we already hold is discarded,
  which also blunts replay of stale ads (`SPEC_CS.md` §14).

### 10.4 What these TTLs do **not** govern

A *seated* player's absence. That is D-005/D-006/D-007 protocol state — absent
seat, auto check/fold, timeout certificate, hand abort with signed attribution, and
at two seats none of that machinery at all (§8.4) — decided
above this layer by signed events, never by a network timer. The lobby TTLs govern
only *lobby visibility*: whether a table and a player still appear in the list.
Conflating the two would let a network hiccup fold a hand, which §1.2 rule 3
forbids.

---

## 11. Resource limits

`SPEC_CS.md` §17 and §27: assume a fully modified adversary who can craft arbitrary
packets, and bound every message and every collection.

### 11.1 libp2p connection and memory limits

```rust
connection_limits::ConnectionLimits::default()
    .with_max_pending_incoming(Some(32))
    .with_max_pending_outgoing(Some(64))
    .with_max_established_incoming(Some(128))
    .with_max_established_outgoing(Some(128))
    .with_max_established(Some(192))
    .with_max_established_per_peer(Some(2))
```

plus `memory_connection_limits::Behaviour::with_max_percentage(0.25)` — refuse new
connections once the process holds a quarter of system memory.

`with_max_established_per_peer(2)` allows exactly the relayed-plus-direct pair that
exists during a DCUtR upgrade, and no more.

> Verification: [COMPILED+RUN] in `probe-netstack`; [SOURCE] the six setter names at
> `libp2p-connection-limits-0.6.0/src/lib.rs:188-224`.

### 11.2 Discovery budget

| Budget | Value |
|---|---|
| DHT candidates dialled per `get_peers` cycle | ≤ 64 |
| Concurrent dials from DHT hints | ≤ 8 |
| Per-address dial timeout | 10 s |
| Retries per DHT-derived address within a session | 0 (the next cycle is the retry) |
| Dials to the same `/24` per cycle | ≤ 4 |
| Peers asked for a snapshot | 4 |
| Bootstrap retry backoff | 2 s, 5 s, 15 s, 60 s, then 5 min |

### 11.3 Message size caps

Every value here is normative in **`PROTOCOL.md` §13** and is reproduced, not
defined, in this table. Where the two ever differ, `PROTOCOL.md` §13 is right and
this table is stale.

| Where | Constant | Cap | Source of the number |
|---|---|---|---|
| GossipSub transmit/receive | `GOSSIP_MAX_TRANSMIT` | 65 536 B | crate default, pinned as a protocol constant (§6.2) |
| Any lobby message (application) | `LOBBY_MSG_MAX` | 8 192 B | §6.5 |
| `LOBBY_TABLE_AD` payload | `TABLE_AD_MAX` | 1 024 B | §6.5 |
| A complete signed advert, forwarded or embedded | `TABLE_AD_SIGNED_MAX` | 1 536 B | §6.5, §7.3 |
| Lobby chat payload | `LOBBY_CHAT_MAX` | 2 048 B | §6.5 |
| Snapshot request | `SNAPSHOT_REQ_MAX` | 1 024 B | §7.1 |
| Snapshot response | `SNAPSHOT_RESP_MAX` | 262 144 B, ≤ 128 ads | §7.1, §7.3, overriding the 10 MiB codec default |
| Ads in one snapshot | `SNAPSHOT_MAX_ADS` | 128 | §7.3; `128 × 1 536 = 196 608 B` is what makes the response cap fit |
| Join request | `JOIN_REQ_MAX` | 4 096 B | §8.4 |
| Join response | `JOIN_RESP_MAX` | 16 384 B | §8.4 |
| Table stream frame | `TABLE_FRAME_MAX` | 262 144 B | sized by `DISPUTE`, not by the shuffle (§8.4) |
| One embedded evidence element | `MAX_EMBEDDED_EVENT` | 32 768 B | `PROTOCOL.md` §9.3 (`DISPUTE`, `HAND_ABORT`) |
| Relay control protocol | — | 4 096 B | `MAX_MESSAGE_SIZE`, [SOURCE] `libp2p-relay-0.21.1/src/protocol.rs:36`; the crate's own constant, not ours |

Every one of these parsers is a fuzz target (`SPEC_CS.md` §27): no input may crash,
allocate unboundedly, read out of bounds, or bypass schema validation. Collections
are length-capped **before** allocation, not after.

### 11.4 Mainline DHT limits — including one that is not obvious

`mainline`'s `Dht::client()` starts in **adaptive mode**, and on every routing-table
refresh tick (`REFRESH_TABLE_INTERVAL = 15 min`) runs:

```rust
fn try_switching_to_server_mode(&mut self) {
    if !self.server_mode() && !self.firewalled() {
        self.socket.server_mode = true;   // now answers unsolicited requests
    }
}
```

**There is no opt-out API** — `DhtBuilder::server_mode()` only forces it *on*, and
there is no `client_only()`. So a user on a public IP or with a port forward will,
after ~15 minutes, silently begin answering strangers' `ping` / `find_node` /
`get_peers` / `announce_peer` and **storing other people's data** — up to 2000
infohashes × 500 peers plus 1000 immutable and 1000 mutable BEP 44 values. For a
poker client that is unexpected inbound traffic, unexpected bandwidth, and
unexpected third-party content on the user's machine.

The supported hard block is a deny-all `RequestFilter`, which compiles and runs:

```rust
#[derive(Debug, Clone)]
struct DenyAll;
impl RequestFilter for DenyAll {
    fn allow_request(&self, _r: &RequestSpecific, _from: SocketAddrV4) -> bool { false }
}

Dht::builder()
    .port(0)                                   // never 6881 — do not fingerprint as a torrent client
    .request_timeout(Duration::from_millis(2500))
    .server_settings(ServerSettings {
        filter: Box::new(DenyAll),
        max_info_hashes: 1, max_peers_per_info_hash: 1,
        max_immutable_values: 1, max_mutable_values: 1,
    })
    .build()?
    .as_async()
```

Ship this filter. A "contribute to the DHT" opt-in toggle may be offered later, but
it must be a deliberate choice, never an accident.

Two more binding rules from the same research: use the **async API only** (every
sync method on `Dht` is `#[deprecated]` in 8.0.0), and beware
`extra_bootstrap` — its doc comment claims it adds to the defaults but the body is
`self.0.bootstrap.clone().unwrap_or_default()`, so without a prior `.bootstrap()`
call your extras **replace** the defaults. Write
`.bootstrap(DEFAULT_BOOTSTRAP_NODES).extra_bootstrap(&cached)` explicitly and add a
regression test.

> Verification: [SOURCE] `mainline-8.0.0/src/rpc.rs:775-806`, `src/dht.rs:64-72`,
> `src/rpc/server/{peers,tokens}.rs`; [COMPILED+RUN] the `DenyAll` filter and the
> builder, `MAINLINE_DHT.md` §4, §4.1.

### 11.5 Blocklist

`libp2p::allow_block_list::Behaviour<BlockedPeers>` with `block_peer(PeerId)` for
peers with a **proven** protocol violation: an invalid signature on a message they
originated, or an `EquivocationProof` as `PROTOCOL.md` §5.2 defines it — two
conflicting **chained** events under one key (`SPEC_CS.md` §14). Unchained lobby
and join traffic is outside that predicate and can never produce such a proof, so
the §7.4 founder contradiction refuses the table and stops there. Blocking is
local, is persisted in the profile, and is never applied on suspicion or on
rate-limit overrun alone — the milder responses of §6.6 come first.

---

## 12. What this layer explicitly does **not** solve

Per `SPEC_CS.md` §18 and its closing paragraph, and D-004/D-005's requirement that
real limits be stated rather than hidden. **No claim is made here that the system
makes cheating impossible.**

**Belongs to a higher layer, by design (§1.2):** deck creation, shuffling and
shuffle proofs; card secrecy; board reveal timing; poker rule legality; pot and
side-pot arithmetic; showdown; who won; the hash-chain transcript; replay and
equivocation detection; the disconnect/abort handling of D-005 and the timeout
certificate of D-006. The network layer carries the bytes and nothing else.

**Nothing above is exercised over libp2p before Phase 7.** Every one of Phases
3–6 runs the poker and cryptographic stack on the `InMemoryTransport` of §1.3.2
and on nothing else, so an unspecified or under-determined harness does not merely
delay testing — it **blocks Phases 3–6**, and it is the only place
`SPEC_CS.md` §25's eleven malicious peers can be run at all before there is a
network.

**Not solved by anybody, and the honest list:**

1. **Symmetric NAT on both ends, with no raised-limit relay available.** Those
   players see the lobby but cannot play. Real, stated in D-004, not fixed here.
2. **UDP blocked entirely.** The Mainline DHT is UDP-only, so discovery dies with
   it and the client sees an empty lobby. TCP rescues the *transport*, not the
   *discovery*. Only direct invite works.
3. **IPv6-only networks.** `mainline` is IPv4-only [SOURCE]. Unmeasured — this
   machine had no usable IPv6 at all.
4. **Eclipse.** A client whose entire peer set is adversarial sees whatever that
   adversary shows it. The mitigations in §4.6 and §7.2 raise the cost; they do not
   close it. The adversary still cannot forge a table ad or a hand.
5. **Discovery-layer DoS / infohash pollution.** Anyone can flood `LOBBY_INFOHASH`
   with junk. We bound our own exposure; we cannot clean the DHT.
6. **Privacy of participation.** A fixed public infohash publishes the player's IP
   to ~100–180 strangers per lookup cycle, is continuously crawled (observed
   first-hand, §3.5), and yields a roster, session times, geolocation and long-term
   linkage to a persistent `PeerId`. Mitigations exist (consent screen, direct
   invite, announce only while looking for a game, VPN — Tor does not work, the DHT
   is UDP); **none of them eliminates it**.
7. **Relay metadata.** A relay learns who talks to whom, when and how much (D-001).
8. **Relay availability.** If no relay is reachable, a CGNAT-only client cannot
   connect. The UI reports it honestly; the layer does not fix it.
9. **Sybil and multi-account.** Identities are free. Nothing here counts humans.
10. **Collusion out of band, endpoint malware, screen sharing, physical coercion,
    traffic analysis, deliberate disconnection.** `SPEC_CS.md` §18 lists these as
    outside the protocol's reach and this document does not claim otherwise.
11. **Generalisation of the NAT measurements.** One friendly residential network
    was measured. The 70 % ± 7.1 % DCUtR figure is the number that matters for other
    players, and it can be near zero for a specific pair.
12. **Real two-network connectivity has never been tested.** Everything in §4 and
    §9 is API ground truth plus single-host runtime plus one live DHT round trip
    between two processes behind the same NAT. The D-003 / D-004 acceptance test —
    two cold clients on unrelated networks, neither told anything about the other,
    both seeing the same open tables, and the D-004 variant with both behind NAT and
    the D-002 relay disabled — is Phase 8 work and is the criterion this design is
    judged by. It has **not** been passed yet.

    **The acceptance test runs on a `CUSTOM` two-seat table**, not on
    `RATED_SNG_POKERTH_V1`. That preset is fully specified but pins `seats = 10`
    and `min_players_to_start = 10`, while `SPEC_CS.md` §32 requires two-player
    heads-up as the first supported mode and §1.3 scopes the MVP at `nlhe/2-6`; it
    is therefore **not playable by the MVP** and becomes playable when `nlhe/7-10`
    lands (`PROTOCOL.md` §13, `STATE_MACHINE.md` §9.5). Judging D-003 and D-004 by
    a configuration the MVP cannot ship would test something nobody can run.

    Two consequences of that choice must be stated rather than discovered later:
    at two seats the deadline machinery does not apply at all (D-007, §8.4), so
    the acceptance test exercises the transport in exactly the mode where a stalled
    peer has no in-protocol remedy; and a two-seat full mesh is one circuit, which
    is the least demanding case for §9.5's relay arithmetic and therefore proves
    the least about it.

---

## 13. Open questions carried forward

| # | Question | Blocks | Owner phase |
|---|---|---|---|
| **OQ-1** | What is the real announce lifetime of the *public* `LOBBY_INFOHASH`? The measured ~45 min is from private random infohashes in an empty keyspace neighbourhood; a busy public constant will evict faster under LRU pressure. If materially shorter, shorten the 10-minute re-announce. | §10.1 interval | 8 |
| **OQ-2** | Adopt an epoch-rotating infohash for privacy? Costs cross-version compatibility and adds a midnight rotation race; does not stop a live observer. Currently **not** adopted. | §3.5 | later |
| **OQ-3** | Does the one-port rule (same numeric port for QUIC/UDP and TCP) hold in practice, and does the QUIC-then-TCP fallback on a single DHT hint behave? | §4.4 | 7 |
| **OQ-4** | Ship libp2p Kademlia after all, if Phase 8 measures poor lobby connectivity from the DHT alone? Costs a second eclipse surface. | §5.7 | 8 |
| **OQ-5** | `TopicScoreParams` values for both lobby topics. Defaults leave the per-topic terms — including the invalid-message penalty a `Reject` feeds — at zero. Requires measured message rates and mesh sizes; badly tuned scoring graylists honest peers. | §6.7 | 8 |
| **OQ-6** | How often does the relay admission race actually deny an honest peer (reservation request arriving before `identify` completes)? A denial closes the circuit listener with no automatic retry, so our own `listen_on` backoff must cover it. | §9.6 | 7 |
| **OQ-7** | What is the real per-hand byte count over **one** relayed circuit, per direction, measured against the `Limit` a real relay actually returns? The estimate is ~8 979 B of shuffle plus the signed event stream, order ~10 KB per hand, against a 131 072 B public-relay budget — comfortable, so the binding public-relay limit is the 120 s `max_circuit_duration` and not the byte cap (§9.5). The measurement is what gives the "refuse to seat rather than start a hand that will drop" rule a number. | §9.5 | 5 → 8 |
| **OQ-8** | What fraction of real peers does AutoNAT v2 confirm as publicly reachable? This sizes the D-002 volunteer relay pool, which is the project's real single point of failure. | §9.6, §12.8 | 8 |
| **OQ-9** | Upstream `mainline`: `announce_peer_detailed` returning `PutOutcome { stored_at }`, or `pub use` of the `*RequestArguments` structs, so the GUI's DHT health indicator has a real number instead of a `get_peers`-and-count workaround. Hold a local patch if needed sooner. | §2.1 step 5 | 8 |
| **OQ-10** | Licence for the project. Unrelated to this document but still open in `DECISIONS.md`. | publication | — |

---

## 14. Constants

**Every two-sided constant is defined in `PROTOCOL.md` §13 and is not restated
here. Changing any of them is a protocol-version change.** That includes every
value this section used to carry: `PROTOCOL_VERSION`, the protocol and topic
strings, the two derivation strings and their infohashes, and every size, TTL and
interval that both sides must agree on. The names in `PROTOCOL.md` §13 are the
names a Rust `constants` module carries, and no value has two names anywhere in
the corpus. Earlier revisions of this document and of `PROTOCOL.md` gave several
of these values two different names and two different numbers; that is
`PHASE0_REVIEW.md` C-1, and one home is the fix.

What remains here is the genuinely **local** tuning — values a peer may change
without breaking interoperability, because no other peer parses or depends on
them.

| Local constant | Value | Where |
|---|---|---|
| GossipSub `mesh_n` / `mesh_n_low` / `mesh_n_high` / `mesh_outbound_min` | 8 / 6 / 12 / 3 | §6.2 |
| GossipSub `heartbeat_interval` | 1 s | §6.2 |
| GossipSub `duplicate_cache_time` | 120 s | §6.2 — must exceed the 30 s ad re-broadcast interval with margin |
| GossipSub `flood_publish` | `false` | §6.2 |
| Per-peer lobby rate budgets (ads, presence, chat, bytes) | §6.6's table | §6.6 |
| DHT candidates dialled per cycle | ≤ 64 | §11.2 |
| Concurrent dials from DHT hints | ≤ 8 | §11.2 |
| Per-address dial timeout | 10 s | §11.2 |
| Dials to the same `/24` per cycle | ≤ 4 | §11.2 |
| Bootstrap retry backoff | 2 s, 5 s, 15 s, 60 s, then 5 min | §2.1 step 4, §11.2 |
| `connection_limits` (pending in/out, established in/out, total, per peer) | 32 / 64 / 128 / 128 / 192 / 2 | §11.1 |
| `memory_connection_limits` share of system memory | 0.25 | §11.1 |
| Relay `Config` (D-002 volunteer) | §9.6's block | §9.6 — local, because a relay's limits are its operator's choice; a client reads the `Limit` the relay actually returns rather than assuming |

A note on the boundary, because it is not obvious in one case: `GOSSIP_MAX_TRANSMIT`
*looks* like local tuning and is not. A peer configured at 64 KiB rejects a larger
frame outright, so the value is two-sided and lives in `PROTOCOL.md` §13. The mesh
parameters beside it genuinely are local: a denser mesh costs only its owner.

---

## 15. Decision register — where each binding decision is honoured

| Decision | Sections |
|---|---|
| **D-001** relays permitted; costs documented; two roles never blurred | §9.5, §9.7, §12.7, §12.8 |
| **D-002** publicly reachable client volunteers as relay, off by default, admission via the rate-limiter closures; open question settled | §9.6 |
| **D-003** global lobby visibility is the acceptance criterion; the bridge is the load-bearing step; announce `Some(external_quic_port)` | §2, §4, §4.4, §12.12 |
| **D-004** lobby visible even if every client is behind NAT; four layers; symmetric-both-ends stated | §9.7, §9.3, §12.1 |
| **D-005** absent seat, mid-hand abort, forfeiture — **not** a transport concern | §1.2 rule 3, §8.4, §10.4 |
| **D-006** action timeout is auto check/fold, never an abort; timeout certificate; no timeout ends the game — **as corrected by D-007** | §1.2 rule 3, §8.4, §10.4 |
| **D-007** corrects D-006: at `n = 2` the action deadline is advisory, a fold-effect timeout certificate is forbidden, and no document may claim the certificate protects a two-seat table. The transport layer's behaviour is unchanged — connection loss stays a hint at every table size — and the transport must not invent a substitute verdict | §8.4, §1.2 rule 3, §10.4, §12.12 |

---

## 16. Objections to the fix plan

`PHASE0_FIXPLAN.md` is applied as written throughout this document. One ruling is
applied under objection, recorded here so the corpus stays consistent and the
disagreement is visible rather than silently resolved in one file.

### 16.1 A-8 — `max_circuit_bytes` is per circuit, but **not** per direction

**The ruling.** A-8 states that *"`max_circuit_bytes` is per circuit and per
direction"*, and derives from that a public-relay budget of ~14 hands per
direction (131 072 / 8 979). §9.5 above carries those numbers verbatim, as
instructed.

**The objection.** The "per direction" half does not hold in
`libp2p-relay 0.21.1`. A circuit is relayed by a single `CopyFuture` holding one
`bytes_sent: u64` counter, and **both** directions increment that one counter
before it is compared against the cap:

```rust
// src/copy_future.rs:41-48, 78, 88-104
if this.max_circuit_bytes > 0 && this.bytes_sent > this.max_circuit_bytes { … }
let src_status = match forward_data(&mut this.src, &mut this.dst, cx) { … this.bytes_sent += i … };
let dst_status = match forward_data(&mut this.dst, &mut this.src, cx) { … this.bytes_sent += i … };
```

The cap is therefore per circuit and **bidirectional**. A-8's citation
(`src/behaviour.rs`, `impl Default for Config`; `src/behaviour/handler.rs`)
establishes the default value and that the value is handed to each circuit; it
does not reach the accounting, which lives in `src/copy_future.rs`.

**What changes if the objection is upheld.** Over one relayed circuit between two
seats, each seat sends its own step and proof once per hand, so the circuit
carries `2 × 8 979 = 17 958 B` of shuffle per hand, not 8 979 B. The
public-relay budget is then **~7 hands**, not ~14, and the "including the signed
event stream" figure roughly halves with it. Every other number in A-8 is
unaffected: `8 979 B` per shuffler is correct, `(n−1) × 8 979 B` as a relayed
peer's total per-hand outbound is correct, and — decisively — **the ruling's
conclusion is correct either way**: at 7 hands as at 14, the byte cap is not what
binds, and the 120 s `max_circuit_duration` still is. The correction changes a
comfort margin, not a design decision.

**Why it is still worth recording.** OQ-7 asks for this number to be measured
against a real relay's returned `Limit`. Whoever runs that measurement will read
the accounting as per-direction, measure one direction, and conclude there is
twice the headroom there is. `SPEC_CS.md` §36 forbids carrying a claim stronger
than its evidence, and "per direction" is one such claim, small as its
consequence is here.
