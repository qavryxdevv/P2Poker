# NAT, Hole Punching and Peer Discovery — Phase 0 Reality Check

Scope: spec sections 1, 2, 3. Question under test: **can two players behind ordinary
home NAT find each other with no prior contact and no central lobby server?**

Every claim below carries a `Verification:` line. Two kinds of evidence are accepted:
**(a) compiled/executed** — code that built and ran on this machine, or
**(b) crate source** — read under `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/<crate>-<version>/`.
docs.rs and recollection are leads, never evidence.

Probe crates (not in the repo):
`…/scratchpad/probe-nat/` (pure-`std` STUN + KRPC, no dependencies) and
`…/scratchpad/probe-nat-p2p/` (libp2p 0.56.0).

---

## 0. Executive summary

| # | Question | Answer |
|---|---|---|
| 1 | NAT on this machine | **Endpoint-independent mapping, port-preserving, port-restricted filtering. Public IPv4, NOT CGNAT.** Best realistic case for hole punching. |
| 6 | DHT `IP:port` → libp2p dial | **Solved, measured.** `Swarm::dial` accepts a `Multiaddr` with no `/p2p` component; the QUIC/TLS handshake supplies the `PeerId`. No extra record needed. |
| 2 | AutoNAT | v2 works; **v1 is effectively dead weight**; v2 has no private-IP guard and will confirm a LAN address as "external". |
| 3 | Relay | **No public libp2p relay fleet exists.** Recommendation: self-hosted volunteer relays discovered through the DHT under a second infohash. |
| 4 | DCUtR success rate | **70% ± 7.1%** over 4.4M attempts (arXiv:2510.27500). ~0% when either side is symmetric. |
| 5 | Two clients, one LAN | Works, two independent ways (mDNS + direct dial). One real hazard: virtual adapters. |
| 7 | Heartbeat/TTL | Design given in §7. |

The single most important structural finding: **DCUtR only engages on a connection that
is already relayed.** No relay ⇒ no hole punch. This makes §3 the load-bearing design
decision of the whole network layer, not an optional fallback.

---

## 1. What NAT is this machine actually behind? (MEASURED)

### 1.1 Addresses

```
LAN adapter "LAN adapter" : 192.168.1.21/24, gateway 192.168.1.1
Hyper-V "Default Switch"      : 172.20.160.1/20        <- virtual, see §5.3
IPv6                          : fd00:0:0::/64 only — ULA, NOT globally routable
External IPv4                 : 198.51.100.17
```

`198.51.100.17` is in `198.51.100.0 – 198.51.100.255`, `RIPE-registered`, country CZ, type
`ASSIGNED PA`, remark *"This prefix is used for residential broadband ISP services."*

**Not CGNAT.** The reflexive address is a public, RIPE-registered residential address; it
is not in RFC 6598 `100.64.0.0/10`, nor RFC 1918. There is exactly one NAT between this
host and the Internet.

> Verification: (a) executed — `ipconfig`; `curl https://api.ipify.org`; RDAP query to
> `rdap.db.ripe.net/ip/198.51.100.17`; and `probe-nat` TEST 4.
> Note: `curl -6 https://api64.ipify.org` returned nothing — **no usable IPv6 on this host.**
> Do not design assuming IPv6 rescues the NAT problem here.

### 1.2 Mapping behaviour — the question that decides hole punching

One UDP socket, ten independent STUN servers on **seven distinct IP addresses**,
5 retries each (RFC 5389 Binding Request, XOR-MAPPED-ADDRESS parsed by hand):

```
local socket: 0.0.0.0:55214
stun.l.google.com:19302          74.125.250.129:19302   -> 198.51.100.17:55214
stun1.l.google.com:19302         74.125.250.129:19302   -> 198.51.100.17:55214
stun2.l.google.com:19302         74.125.250.129:19302   -> 198.51.100.17:55214
stun.cloudflare.com:3478         162.159.207.0:3478     -> 198.51.100.17:55214
stun.sipgate.net:3478            3.33.249.248:3478      -> 198.51.100.17:55214
stun.voip.blackberry.com:3478    20.93.239.171:3478     -> 198.51.100.17:55214
stun.nextcloud.com:443           46.225.95.169:443      -> timeout
stun.miwifi.com:3478             111.206.174.3:3478     -> 198.51.100.17:55214
stun.ekiga.net:3478              216.93.246.18:3478     -> 198.51.100.17:55214
stun.antisip.com:3478            5.39.72.109:3478       -> 198.51.100.17:55214

servers answering=9  distinct reflexive IPs=[198.51.100.17]  distinct ports=[55214]
local port 55214 -> external port 55214 : PORT PRESERVED
=> MAPPING: ENDPOINT-INDEPENDENT (RFC 4787 REQ-1 compliant, 'cone')
```

**Endpoint-independent mapping, and the external port equals the internal port.** Nine
servers at seven different IPs all observe the same `IP:port`. This is the friendly case:
a single reflexive candidate learned once is valid for every remote peer, which is
precisely the precondition DCUtR needs. Mapping was also stable on re-query after 3 s.

> Verification: (a) executed — `probe-nat/src/bin/filtering.rs`, section A, and
> `probe-nat/src/main.rs` TEST 1/TEST 3.

### 1.3 Filtering behaviour

RFC 5780 CHANGE-REQUEST against `stun.sipgate.net` (which does advertise
`OTHER-ADDRESS = 15.197.250.192:3479`, so it implements RFC 5780):

```
primed mapping = 198.51.100.17:62528
test II  (change IP + port): blocked/timeout
test III (change port)     : blocked/timeout
=> FILTERING: ADDRESS-AND-PORT-DEPENDENT (port-restricted cone)
```

Corroborated independently by an inbound-traffic census: joined the live Mainline DHT and
sprayed `find_node` widely so real nodes would learn of us, then counted packets arriving
from source IPs we had never contacted.

```
find_node sent          : 2406
distinct IPs contacted  : 793
replies from contacted  : 1361
UNSOLICITED packets from IPs we NEVER contacted: 0
```

793 distinct DHT nodes were contacted and 1361 replies came back, but **zero** packets
ever arrived from a stranger. Inbound is filtered by source address *and* port.

**Combined verdict: endpoint-independent mapping + address-and-port-dependent filtering
= classic "port-restricted cone" NAT.** Hole punching works, but *both* sides must
transmit outward first — which is exactly what DCUtR's synchronised simultaneous open
does. Unsolicited inbound is impossible, so a peer can never be reached cold.

> Verification: (a) executed — `probe-nat/src/bin/filtering.rs` section B;
> `probe-nat/src/bin/dht2.rs` section 2. An earlier version of the inbound probe reached
> only 6 IPs due to a bad index and was discarded as too weak; the numbers above are from
> the corrected rotating-cursor version.

### 1.4 Honest caveat about generalising this

This is **one** measurement of **one** network — a friendly one. It says nothing about
the user's opponents. The Czech residential market includes CGNAT deployments, and the
measurement campaign in §4 finds ~30% of hole punches fail in the wild. **Do not treat
this result as evidence that the relay path is optional.** It is evidence that *this*
machine will not be the one that fails.

---

## 6. THE BOOTSTRAP GAP: turning `IP:port` from the DHT into a libp2p dial

This is the load-bearing question, so it is answered before §2–§5.

### 6.1 The gap is real

A BEP 5 `get_peers` response carries `values`: a list of **compact peer info**, which is
**exactly 6 bytes — 4-byte IPv4 + 2-byte big-endian port**. There is no `PeerId`, no
transport tag, no protocol hint, no signature. Measured against two real public swarms:

```
== get_peers, closest-first, infohash debian-12.5.0-amd64-netinst.iso
   nodes asked=11 tokens=40 PEERS FOUND=5
      203.0.113.21:19107
      203.0.113.22:22433
      203.0.113.23:42899
      203.0.113.24:21539
      203.0.113.25:38464
== get_peers, closest-first, infohash ubuntu-24.04.1-desktop-amd64.iso
   nodes asked=8 tokens=24 PEERS FOUND=1
      203.0.113.26:31782
```

So the client gets an IP and a port and nothing else. libp2p normally wants
`/ip4/A.B.C.D/udp/P/quic-v1/p2p/12D3Koo…` — and the `PeerId` is the part the DHT cannot
give us.

> Verification: (a) executed — `probe-nat/src/bin/dht2.rs` section 1, hand-written
> bencode/KRPC over `std::net::UdpSocket`; (b) crate source — BEP 5 compact-peer layout
> re-derived from the wire bytes, not from documentation.

### 6.2 The resolution: dial without a PeerId and let the handshake name the peer

**`Swarm::dial` accepts a bare `Multiaddr` with no `/p2p` component.** The peer's identity
is then established by the QUIC/TLS (or Noise) handshake, and `ConnectionEstablished`
reports the `PeerId` that was cryptographically proven. Measured end to end:

```
LOCAL PEER ID: 12D3KooWQRHf…
DIALING /ip4/192.168.1.21/udp/63240/quic-v1  (contains /p2p PeerId component: false)
  dial() accepted the address without a PeerId
CONNECTED peer=12D3KooWN1VU… n=1
          addr="/ip4/192.168.1.21/udp/63240/quic-v1"
```

The dialer knew only an IP and a port, and finished holding a proven `PeerId`.

> Verification: (a) compiled and executed — `probe-nat-p2p` against libp2p 0.56.0.
> `swarm.dial(addr)` where `addr: Multiaddr` was parsed from a string with no `/p2p`.

This maps perfectly onto spec §3's rule that *"Seznam peerů z Mainline DHT není ověřený …
Identitu rozhoduje až libp2p handshake"* — the untrusted DHT hint is only ever a hint, and
identity is decided by the handshake. **No signed blob is required, and none should be
added.** The architecture the spec already mandates is exactly the one that closes the gap.

### 6.3 Exact procedure

1. Persistent Ed25519 keypair → `PeerId` (spec §3, §21).
2. `swarm.listen_on("/ip4/0.0.0.0/udp/<P>/quic-v1")` — **pick `P` once, persist it, reuse
   it every run.** With this NAT the external port equals `P` (§1.2), so the DHT record
   stays valid across restarts and the mapping is what peers actually punch to.
3. Bootstrap Mainline DHT (§6.5), then `announce_peer(LOBBY_INFOHASH, port = P)` with
   `implied_port = 0`, so the DHT stores the QUIC port and not the KRPC source port.
4. `get_peers(LOBBY_INFOHASH)` → a set of `A.B.C.D:P'`.
5. For each, synthesise `"/ip4/A.B.C.D/udp/P'/quic-v1".parse::<Multiaddr>()` — **no
   `/p2p`** — and `swarm.dial(addr)`.
6. On `ConnectionEstablished`, take `peer_id` from the event. That is the authenticated
   identity. Feed it to Kademlia/GossipSub. Everything before this point was a guess.
7. Non-libp2p listeners on that port (real BitTorrent clients that share the infohash)
   simply fail the handshake and are dropped. Budget for that: `LOBBY_INFOHASH` is a
   public value and anything may be squatting on it.

**Critical detail:** step 3 must announce the **QUIC listen port**, not the DHT socket's
port. Verified that the DHT stores what you ask for:

```
our announced port was 41337; record says port 41337
-> the DHT stored the port WE ASKED FOR, and the IP it OBSERVED
```

`announce_peer` was accepted by 12 nodes and read back from 12 nodes as
`198.51.100.17:41337`.

> Verification: (a) executed — `probe-nat/src/bin/dht.rs` section 3, full
> announce→get_peers round trip on the live public DHT under a random test infohash.

### 6.4 Should we run one UDP socket or two?

Two, and accept the cost. A single socket multiplexing KRPC bencode and QUIC is possible
(they are distinguishable) but fragile. Two sockets means two NAT mappings; with
port-preserving endpoint-independent mapping that is harmless here, but on a symmetric
NAT the DHT-observed IP would still be correct even though the port is not — which is why
step 3 announces an explicitly chosen port rather than relying on `implied_port = 1`.

### 6.5 Bootstrap nodes — half the canonical list is dead (MEASURED)

```
router.bittorrent.com:6881       67.215.246.10:6881     NO RESPONSE
dht.transmissionbt.com:6881      87.98.162.88:6881      PONG  20ms  node_id=35034275
router.utorrent.com:6881         82.221.103.244:6881    NO RESPONSE
dht.libtorrent.org:25401         185.157.221.247:25401  PONG  31ms  node_id=1c11e01b
                                                        BEP42 ip=198.51.100.17:50429
router.bitcomet.com:6881         DNS FAIL (no such host)
dht.aelitis.com:6881             34.203.221.232:6881    NO RESPONSE
router.bittorrent.cloud:42069    DNS FAIL (no such host)
```

**Only 2 of 7 answered.** `router.bittorrent.com` and `router.utorrent.com` — the two most
commonly hardcoded — resolve but do not reply. Ship the working two, keep the dead ones as
low-priority extras in case they return, and **persist a routing-table cache to disk** so
that after the first successful run the client is not dependent on any of them. Treat a
bootstrap list as a cold-start crutch, not infrastructure.

Useful side effect: `dht.libtorrent.org` implements **BEP 42** and returned our external
`IP:port` in the `ip` field of the reply. That is a free second opinion on our reflexive
address, obtained without any STUN server.

> Verification: (a) executed — `probe-nat/src/bin/dht.rs` section 1, three retries each.

### 6.6 BEP 44 works, and is the right tool for §3 — but not for §6

Tested the immutable variant (`target = SHA1(bencode(v))`, no signature required):

```
bootstrap: 62 candidate nodes near target
nodes asked=22  write tokens for 'get'=28
'put' accepted by 10 node(s)
READ BACK v = "p2p-poker-test-blob-peerid-placeholder"
value read back from 10 node(s)
```

Arbitrary data storage on the public Mainline DHT **works from this machine**. A couple of
nodes answered `202 Server Error` to `get`, so support is not universal, but 10/10
readback is decisive.

Do **not** use it to solve §6 — §6.2 is simpler and needs no extra moving parts. Do use it
for **relay advertisement** (§3.4), where a record genuinely must carry more than
`IP:port`: a relay's full `/p2p/<PeerId>` multiaddr. BEP 44 mutable records (Ed25519
signed, BEP 44 §"Mutable items") let a relay operator republish under a stable key.

> Verification: (a) executed — `probe-nat/src/bin/bep44.rs`, hand-rolled SHA-1 + bencode,
> live public DHT.

---

## 2. libp2p AutoNAT — how a node learns it is not reachable

libp2p 0.56.0 ships **both** versions; they are separate behaviours and can coexist
(`libp2p::autonat::Behaviour` for v1, `libp2p::autonat::v2::client::Behaviour` /
`autonat::v2::server::Behaviour` for v2).

> Verification: (a) compiled — all four type paths appear in the `#[derive(NetworkBehaviour)]`
> struct of `probe-nat-p2p/src/main.rs`, which builds clean against libp2p 0.56.0.

### 2.1 Protocol identifiers observed on the wire

From an `identify` exchange with a real peer:

```
/libp2p/autonat/1.0.0
/libp2p/autonat/2/dial-request
/libp2p/autonat/2/dial-back
/libp2p/circuit/relay/0.2.0/hop
/libp2p/circuit/relay/0.2.0/stop
```

> Verification: (a) executed — `identify::Event::Received` in the two-instance run.

### 2.2 Timing — v1

`autonat::Config::default()` (crate source, `libp2p-autonat-0.15.0/src/v1/behaviour.rs`):

| field | default | meaning |
|---|---|---|
| `boot_delay` | **15 s** | delay before the first probe |
| `retry_interval` | **90 s** | retry while status is `Unknown` / below max confidence |
| `refresh_interval` | **15 min** | re-test after max confidence reached |
| `confidence_max` | **3** | probes needed to be confident |
| `throttle_server_period` | 90 s | cannot re-use the same peer as server sooner |
| `timeout` | 30 s | per request |
| `only_global_ips` | **true** | ignore peers observed at private addresses |

So a v1 verdict takes **15 s at the earliest, and realistically ~15 s + 3 × 90 s ≈ 5 minutes**
to reach full confidence. `NatStatus` is `Public(Multiaddr) | Private | Unknown`, readable
via `Behaviour::nat_status()` and `Behaviour::confidence()`.

> Verification: (b) crate source — `Default for Config`; enum at line 112.
> (a) compiled — `swarm.behaviour().autonat_v1.nat_status()` / `.confidence()` compile and run.

### 2.3 Timing — v2

`autonat::v2::client::Config::default()`: `max_candidates = 10`, `probe_interval = 5 s`.
Far more responsive. v2 also fixes v1's central flaw — the server **always dials back over
a freshly allocated port**, which removes the false positives v1 suffered when the
client-server connection itself ran over a hole-punched port. It adds a DoS guard by
requiring the client to send more data (`DATA_LEN_LOWER_BOUND = 30_000` …
`DATA_LEN_UPPER_BOUND = 100_000` bytes) than the dial-back costs the server.

> Verification: (b) crate source — `v2/client/behaviour.rs` `Default for Config`;
> `v2/protocol.rs` lines 14–15; rationale in the `//!` docs of `src/v2.rs`.

### 2.4 Two findings that will bite

**(i) v1 stayed `Unknown` forever in our run.**

```
autonat v1 nat_status: Unknown
autonat v1 confidence: 0
```

Zero `StatusChanged` events in 45 s across both instances. Cause: `only_global_ips: true`
means LAN peers are refused as probe servers, and our test had only LAN peers.
Consequence: **on a LAN-only or freshly-started swarm, v1 tells you nothing.** Combined
with the ~5-minute confidence time, v1 earns its keep only in a large public swarm.
**Recommendation: use AutoNAT v2, and do not gate startup on v1.**

**(ii) v2 has no private-address guard and produced a false positive.**

```
EXTERNAL ADDR CONFIRMED /ip4/192.168.1.21/udp/55598/quic-v1
AUTONAT v2 CLIENT EVENT tested_addr=/ip4/192.168.1.21/udp/55598/quic-v1 result=Ok(())
EXTERNAL ADDR CONFIRMED /ip4/192.168.1.21/tcp/10554
```

v2 confirmed a **private RFC 1918 address** as an external address, because the dial-back
came from a peer on the same LAN. Grepping `v2/client/behaviour.rs` and
`v2/server/behaviour.rs` for `global` / `is_private` / `loopback` returns **no matches** —
there is no equivalent of v1's `only_global_ips`.

**Mitigation (must implement):** filter address candidates ourselves before trusting
`ExternalAddrConfirmed` — discard RFC 1918, RFC 6598, loopback and link-local — unless we
deliberately want the LAN path (§5). Do not publish a private address to the DHT.

> Verification: (a) executed — quoted events; (b) crate source — the absence of any
> global-IP check in both v2 modules.

### 2.5 What the node does with the answer

`Public` → announce the confirmed address, offer to act as a relay for others (§3.4).
`Private` → stop advertising direct addresses, obtain a relay reservation, listen on
`/p2p/<relay>/p2p-circuit`, and let DCUtR try to upgrade each relayed connection.

---

## 3. Circuit Relay v2 — the honest tension with spec §3

### 3.1 DCUtR *requires* a relay. This is not a fallback.

`libp2p-dcutr-0.14.1/src/behaviour.rs` installs the real protocol handler only when the
connection is relayed:

```rust
fn handle_established_inbound_connection(
    &mut self, connection_id: ConnectionId, peer: PeerId,
    local_addr: &Multiaddr, remote_addr: &Multiaddr,
) -> Result<THandler<Self>, ConnectionDenied> {
    if is_relayed(local_addr) {
        …                       // handler::relayed::Handler
```

with `type ConnectionHandler = Either<handler::relayed::Handler, dummy::ConnectionHandler>;`
— a non-relayed connection gets `dummy::ConnectionHandler`, i.e. nothing. Also
`MAX_NUMBER_OF_UPGRADE_ATTEMPTS: u8 = 3`, and the relayed handler's stream budget is
`FuturesSet::new(Duration::from_secs(10), 1)`.

**Consequence: no relay ⇒ no DCUtR ⇒ no hole punch ⇒ two NATed players never meet.**
The relay is on the critical path for *establishing* every NAT-to-NAT connection, even
those that end up fully direct. Any design that treats relays as an optional extra is wrong.

> Verification: (b) crate source — `src/behaviour.rs` lines 45, 152, 168–180.

### 3.2 There is no public relay fleet

There is no maintained, publicly advertised list of open Circuit Relay v2 servers that a
third-party application may use. Protocol Labs runs relays for IPFS, but they are sized
and reserved for IPFS traffic and offer no availability commitment to outside projects.
The libp2p documentation's answer to "where do I get a relay" is *run one*
(`libp2p/go-libp2p-relay-daemon`, `js-libp2p-relay-server`).

> Verification: web search of libp2p/IPFS docs, specs repo, and forums returned relay
> *implementations* and the v2 spec, and **no list of public relays**. This is a negative
> result: absence of evidence found, stated as such.

### 3.3 The tension, stated plainly

Spec §3 says **"Nevytvářej skrytý centrální fallback server."** Hardcoding a list of
third-party relay addresses into every client would violate that in substance even if not
in form:

- every NATed player's *first* contact with any other player would traverse machines we
  neither own nor control;
- those operators would see the full social graph of who plays with whom, and when;
- if the list dies, the network dies — the definition of a hidden central dependency;
- it silently transfers the project's availability to a third party who never agreed to it.

A hardcoded relay list is a central fallback server wearing a costume. **Reject it.**

### 3.4 Recommendation: DHT-discovered volunteer relays

**Any peer that AutoNAT v2 confirms as publicly reachable enables
`relay::Behaviour` (the server side) and advertises itself in the DHT under a second,
distinct infohash `RELAY_INFOHASH ≠ LOBBY_INFOHASH`.**

Mechanics:

1. Peer starts, runs AutoNAT v2, filtered per §2.4(ii) so private addresses cannot qualify.
2. If confirmed public **and** the user has left "act as relay" enabled (default on, one
   checkbox to disable — see §3.6), it starts `relay::Behaviour::new(pid, relay::Config::default())`.
3. It publishes a **BEP 44 mutable** record keyed by its Ed25519 identity containing its
   full multiaddr *including* `/p2p/<PeerId>` (§6.6 proved BEP 44 works here), and
   `announce_peer(RELAY_INFOHASH, quic_port)` so relays are findable by plain `get_peers`
   as well. The `IP:port` alone is enough to dial by §6.2; the BEP 44 blob is an
   optimisation that lets a client know the `PeerId` before dialling.
4. A NATed peer does `get_peers(RELAY_INFOHASH)`, dials a few, takes a reservation on 2–3
   of them, and listens on `/ip4/…/p2p/<relay>/p2p-circuit`.
   (`Protocol::P2pCircuit` exists in the `multiaddr` crate and parses from `"p2p-circuit"`.)
5. It publishes those circuit addresses to the lobby. Peers reach it via the circuit,
   then DCUtR upgrades to direct. The relay carries only the handshake.

Why this satisfies §3: the relay set is *emergent* and *self-healing*. No address is
compiled in. Any sufficiently-connected player can serve. If every relay disappears, new
ones appear as soon as one publicly-reachable player starts the app. Nobody is structurally
privileged — a relay is just a player whose NAT happens to be permissive.

> Verification: (a) compiled — `relay::Behaviour`, `relay::client::Behaviour`,
> `.with_relay_client(noise::Config::new, yamux::Config::default)` all build in
> `probe-nat-p2p`; both `/libp2p/circuit/relay/0.2.0/hop` and `/stop` were observed
> advertised via `identify` at runtime. (b) crate source — `Protocol::P2pCircuit` in
> `multiaddr/src/protocol.rs:110,236`.

### 3.5 The default relay limits will break a poker session — tune them

`relay::Config::default()` (crate source, `libp2p-relay-0.21.1/src/behaviour.rs`):

| field | default | consequence |
|---|---|---|
| `max_circuit_duration` | **120 s** | a relayed circuit is **killed after 2 minutes** |
| `max_circuit_bytes` | **131072 (128 KiB)** | and after 128 KiB |
| `max_reservations` | 128 | per relay |
| `max_reservations_per_peer` | 4 | |
| `reservation_duration` | 3600 s | reservation, not circuit |
| `max_circuits` | 16 | concurrently |
| `max_circuits_per_peer` | 4 | |

Plus rate limiters: 1 reservation per peer per 2 min (30/h), 1 per IP per min (60/h).

**These defaults are deliberately hostile to using a relay as a transport.** They are sized
to carry a DCUtR handshake and nothing more. This is upstream telling us the intended
design: relay to meet, then punch.

Two implications:

- **When DCUtR succeeds**, defaults are fine — leave them. Do not raise them "just in case";
  generous limits invite free-riding on volunteer players' bandwidth.
- **When DCUtR fails** (~30%, §4), the session must survive on the relay. 120 s / 128 KiB
  will not carry a poker hand, let alone a Sit-and-Go. Options, in preference order:
  1. Accept a **separate, explicitly configured** relay profile with longer limits, offered
     by volunteering players who opt in to "relay full sessions". The client asks for a
     reservation and *reads the returned `Limit`* — the protocol reports the server's real
     limits back to the client (`relay::protocol::Limit` exposes `duration()` and
     `data_in_bytes()`), so the client can tell before committing whether a circuit will
     survive a hand. **Use this; do not assume.**
  2. If no adequate relay is available, **refuse to seat the player** with an honest message
     rather than starting a hand that will drop mid-street. Spec §19 already makes
     mid-hand disconnect a security problem, not merely an annoyance — a circuit that dies
     at 120 s is an *engineered* abort attack against ourselves.
- Mental-poker traffic is small (commitments, shuffle proofs, decryption shares), but a
  Bayer-Groth proof for a 52-card deck is not tiny. **Measure real per-hand bytes in
  Phase 5 and check it against whatever `Limit` real relays hand back**; do not guess now.

> Verification: (b) crate source — `Default for Config` in `libp2p-relay-0.21.1/src/behaviour.rs`;
> `pub struct Limit { duration, data_in_bytes }` with public accessors in `src/protocol.rs:39-52`.

### 3.6 Privacy and abuse duties this creates

Volunteering as a relay means carrying strangers' traffic from your home IP. This must be
disclosed in the UI, and it must be switchable off. Keep the default rate limiters. Note
also that relaying reveals to the relay operator *that* two peers are talking, though not
what they say (the circuit carries an end-to-end encrypted libp2p connection, and the poker
layer is separately signed and encrypted per spec §12/§20).

---

## 4. DCUtR success rate in the wild

**Headline: 70% ± 7.1%.** From *"Challenging Tribal Knowledge — Large Scale Measurement
Campaign on Decentralized NAT Traversal"*, arXiv:2510.27500 (submitted 31 Oct 2025):

- **> 4.4 million** traversal attempts, **85,000+** distinct networks, **167** countries.
- Hole-punching stage success: **70% ± 7.1%**.
- **TCP and QUIC are statistically indistinguishable, both ≈70%** — refuting the common
  assumption that UDP punching is easier. (Do not choose QUIC *for punching reasons*;
  choose it for 0-RTT, stream multiplexing and no head-of-line blocking.)
- **97.6%** of successful connections succeed on the **first** attempt. So retries buy
  little: if the first punch fails, plan for the relay rather than hammering.
- Success is **independent of relay characteristics** — good news for §3.4, since it means
  a volunteer relay on a home connection is not worse at brokering than a datacenter one.

Earlier data: the Protocol Labs `punchr` campaign (Dec 2022 – Jan 2023) collected 6.25M
results from 154 clients against 47,000 peers and reported 60–90%.

### 4.1 Failure modes

1. **Symmetric NAT (endpoint-dependent mapping) on either side — the classic killer.**
   rust-libp2p maintainers, Mar 2025: *"There are situations in which hole punching will
   not work, most notably when one of the nodes is behind a symmetric NAT."* The external
   port cannot be predicted, so the peer has no valid address to punch to. Our machine is
   *not* symmetric (§1.2), but an opponent may be.
2. **Regional variance is severe.** In the same discussion one user reported **1–10%**
   success on their networks and another **zero success across ~10 people**, both
   attributed to symmetric NAT prevalence. A 70% global average can be near-0% for a
   specific pair of players. **This is the empirical case for never shipping without a
   working relay path.**
3. **Failure is sticky.** Maintainers note success is highly peer-dependent: if a punch
   to a given peer fails once, it typically fails repeatedly. Cache that verdict per peer
   and go straight to relay next time rather than re-paying the DCUtR latency.
4. **CGNAT** — commonly symmetric, and no port forward is possible. Relay-only.
5. `MAX_NUMBER_OF_UPGRADE_ATTEMPTS = 3` then `AttemptsExceeded` (crate source, §3.1).

> Verification: arXiv:2510.27500 (fetched); rust-libp2p Discussion #5910 (fetched);
> FOSDEM 2023 *"Hole punching in the wild"*. Crate constant: (b) crate source.
> Note: a search result also surfaced "arXiv:2604.12484" with a similar title; that
> identifier is not consistent with a real arXiv numbering for a paper I could fetch, so
> **no number from it is used here.** Only 2510.27500 is cited.

---

## 5. Two clients behind the same router (MEASURED)

This has bitten this user before, so it was tested directly: two processes on this host,
same LAN, same NAT.

### 5.1 It works, by two independent mechanisms

**(i) Direct dial of the LAN address** — connected in well under a second, no mDNS needed:

```
DIALING /ip4/192.168.1.21/udp/63240/quic-v1  (contains /p2p PeerId component: false)
CONNECTED peer=12D3KooWN1VU… n=1
```

**(ii) mDNS** — `libp2p::mdns::tokio::Behaviour` found the peer on all interfaces:

```
MDNS DISCOVERED 12D3KooWN1VU… at /ip4/192.168.1.21/udp/63240/quic-v1/p2p/12D3KooWN1VU…
MDNS DISCOVERED 12D3KooWN1VU… at /ip4/192.168.1.21/tcp/10531/p2p/12D3KooWN1VU…
```

mDNS service `_p2p._udp.local`, multicast `224.0.0.251`; `mdns::Config::default()` is
`ttl = 6 min`, `query_interval = 5 min`, `enable_ipv6 = false`. **The 5-minute default
query interval is too slow for a lobby** — two players starting the app a minute apart
would wait. Lower it to ~10–20 s. mDNS records carry the full `/p2p/<PeerId>`, so unlike
the DHT path no handshake-identity step is needed.

> Verification: (a) compiled and executed — `probe-nat-p2p` run script `run2.ps1`,
> logs `A.log` / `B.log`; (b) crate source — `libp2p-mdns-0.48.0/src/lib.rs:48-56` and
> `Default for Config`.

### 5.2 What actually guarantees it

Do not rely on NAT hairpinning. The guarantee comes from **never needing the router at
all**: both peers advertise their RFC 1918 addresses in `identify.listen_addrs` and in
mDNS, and dial each other directly on the L2 segment. Confirmed — the remote's
`listen_addrs` included `/ip4/192.168.1.21/…`, and `observed_addr` was
`/ip4/192.168.1.21/udp/55598/quic-v1`, i.e. the LAN address, not the public one.

Therefore: **keep mDNS enabled, and do not filter private addresses out of the *dialling*
path** — only out of what gets *published* to the DHT (§2.4(ii)). Those are different
filters and conflating them is exactly how this case breaks.

If the two players are on the same LAN but the app only knew them through the DHT, both
would announce the *same* public IP `198.51.100.17` under `LOBBY_INFOHASH` with different
ports; dialling that public IP:port from inside would require hairpinning, which many
consumer routers do badly. **mDNS is the reliable path for this case; treat it as
mandatory, not decorative.**

### 5.3 Real hazard found: virtual adapters poison the address set

The Hyper-V "Default Switch" address `172.20.160.1` was advertised, dialled, and timed out:

```
DIAL FAIL peer=Some(PeerId("12D3KooWN1VU…"))
  err=Failed to negotiate transport protocol(s):
  [(/ip4/172.20.160.1/udp/63240/quic-v1/p2p/12D3KooWN1VU…: Handshake with the remote timed out.)]
```

The peer is genuinely listening on that address, but it is on an isolated virtual switch,
so the dial cannot complete and burns a full handshake timeout. On a developer machine
(Hyper-V, WSL, Docker, VPN adapters — this host has three OpenVPN adapters too) this adds
several dead candidates to every dial. **Mitigation:** dial candidates concurrently and
take the first success (libp2p already races them), cap per-address timeout, and consider
excluding known-virtual interface prefixes from what we *publish*.

---

## 7. Heartbeat and TTL for lobby liveness (spec §1)

Three independent expiry layers; none may rely on a departing peer announcing anything.

### 7.1 Mainline DHT layer

Announced peers expire on their own; BitTorrent implementations conventionally hold them
for ~15 min. **Re-`announce_peer(LOBBY_INFOHASH)` every 10 minutes**, and re-run
`get_peers` on the same cadence to refresh the candidate set. This also keeps the NAT
mapping for the DHT socket alive.

Note the privacy cost the spec already requires us to document (§3): announcing under a
public infohash publishes the player's IP as *someone interested in this specific value*,
and public infohashes are continuously crawled. This is inherent, is not a reason for a
central server, and must be surfaced in the UI.

### 7.2 libp2p connection layer

`ping::Behaviour` gives liveness on every open connection. Set
`with_idle_connection_timeout` deliberately — the probe used 30 s; the default is much
shorter and will silently close lobby connections that are merely quiet. Keep NAT mappings
warm on any path we intend to reuse: UDP mappings on consumer routers commonly expire in
30–120 s, so the keep-alive interval must be **well under 30 s** (ping default interval is
suitable; do not raise it).

### 7.3 Lobby application layer (the authoritative one)

`TABLE_ADVERTISEMENT` already carries `timestamp` and `expires_at` and is signed (§4).
That signed `expires_at` is what the lobby honours — never a peer's word that a table is gone.

- Table advert TTL: **90 s**, re-broadcast by the table owner every **30 s** (3× margin
  against GossipSub jitter and one missed beat).
- Player presence TTL: **120 s**, heartbeat every **40 s**.
- On expiry, drop the entry locally. No `TABLE_CLOSE` required — `TABLE_CLOSE` is an
  optimisation for the polite case, never a precondition for cleanup. A crashed client
  must vanish without cooperation, which is exactly what spec §1 demands.
- Reject any advert whose `expires_at` is more than **~5 minutes** ahead of local time, and
  any whose `timestamp` is in the future beyond a small skew allowance. Otherwise a
  malicious peer pins a table in every lobby forever — cheap spam, and §4 requires
  anti-spam.
- Clock skew is a real attack surface here. Prefer *relative* freshness (age since receipt)
  for local eviction and use `expires_at` only as an upper bound on how long we are willing
  to hold the entry at all.
- New client join order stays as §3 mandates: snapshot request to several peers first,
  then live GossipSub. Reconcile by `(table_id, timestamp)`, keeping the newest validly
  signed advert per `table_id`; a lower `timestamp` for a `table_id` we already hold is
  discarded, which also blunts replay of stale adverts (§14).

---

## 8. Verified inventory for the network layer

libp2p **0.56.0**, all features confirmed present by `cargo add` and by a clean
`cargo build --release`:
`autonat, dcutr, dns, gossipsub, identify, kad, macros, mdns, noise, ping, quic, relay, tcp, tokio, yamux`.

Resolved sub-crate versions: `libp2p-autonat 0.15.0`, `libp2p-relay 0.21.1`,
`libp2p-dcutr 0.14.1`, `libp2p-mdns 0.48.0`, `libp2p-identify 0.47.0`, `libp2p-kad 0.48.0`,
`libp2p-gossipsub 0.49.5`, `libp2p-quic 0.13.1`, `libp2p-swarm 0.47.1`, `libp2p-core 0.43.2`.

Builder chain that compiles:

```rust
SwarmBuilder::with_existing_identity(kp)
    .with_tokio()
    .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
    .with_quic()
    .with_relay_client(noise::Config::new, yamux::Config::default)?
    .with_behaviour(|key, relay_client| { /* … */ })?
    .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(30)))
    .build();
```

> Verification: (a) compiled and executed — `probe-nat-p2p`, rustc 1.95.0,
> `x86_64-pc-windows-msvc`.

For Mainline DHT, spec §2 permits writing a minimal KRPC client rather than pulling in a
torrent library. **The probes here demonstrate that is entirely tractable**: bencode +
`ping` / `find_node` / `get_peers` / `announce_peer` / BEP 44 `put`/`get`, all working
against the live DHT, in pure `std` with **zero dependencies**, in a few hundred lines
(`probe-nat/src/bin/dht.rs`, `dht2.rs`, `bep44.rs`). Recommend that route over adopting
`mainline 8.0.0` unless async integration proves painful — fewer dependencies, and spec
§28 asks for exactly that.

---

## 9. Open risks

1. **We have measured one friendly network.** The 70%/30% split is the number that matters
   for other players, not our own clean cone NAT result.
2. **The relay design is the project's real single point of failure.** If too few players
   are publicly reachable, the volunteer pool is thin and NATed players cannot meet at all.
   Worth measuring in Phase 8: what fraction of real peers AutoNAT v2 confirms as public.
3. **Relay default limits (120 s / 128 KiB) cannot carry a hand.** Resolve before Phase 8,
   with real per-hand byte counts from Phase 5.
4. **Public `LOBBY_INFOHASH` is crawlable and squattable.** Expect junk peers; the
   handshake filters them, but budget the dial attempts.
5. **AutoNAT v2 false positives on private addresses** — must be filtered by us (§2.4).
6. **Only 2 of 7 canonical DHT bootstrap nodes are alive.** Persist a routing cache.
7. **No usable IPv6 on this host**, so no measurement of the IPv6 path, which would
   otherwise sidestep NAT entirely. Worth re-testing on a network that has it.
