# Mainline DHT from Rust — ground truth for global peer discovery

Phase 0 research note. Scope: **BitTorrent Mainline DHT (BEP 5) used only for global
peer discovery** under one fixed `LOBBY_INFOHASH`, via `announce_peer()` and
`get_peers()`, as required by `docs/SPEC_CS.md` §1, §2 and §3. Everything else
(transport, encryption, PeerId, NAT traversal, GossipSub) is libp2p's job and is out
of scope here.

**Every API claim below is verified in one of exactly two ways, stated per claim:**

* **[COMPILED]** — code using it was compiled and, where stated, executed.
* **[SOURCE]** — read in the unpacked crate under
  `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/<crate>-<version>/`.

docs.rs and prior knowledge were used only as leads. Toolchain: `rustc 1.95.0`,
`cargo 1.95.0`, `x86_64-pc-windows-msvc`. Measurements taken
2026-08-28, from a residential IPv4 line behind a consumer NAT,
public IPv4 `198.51.100.17`.

**Independent re-verification pass, 08:25–09:20 UTC.** Every load-bearing claim in
this note was checked a second time from scratch: the two ISO infohashes were
recomputed from the `.torrent` files on disk rather than reused; the `get_peers`,
announce round-trip, port-control, `RequestFilter` and KRPC probes were rebuilt and
re-run; both compiler-verified negatives were reproduced; every `[SOURCE]` line
number was re-read in the unpacked crate; and the BEP 5 quotes were re-fetched from
`bittorrent.org`. Where the second pass produced different numbers the ranges below
have been widened to cover both. Nothing was contradicted.

Probe sources (kept, not in the repo):
`<scratchpad>`
— `live/` (mainline 8.0.0), `dht7/` (dht 7.0.0), `n0probe/` (n0-mainline 0.6.0),
`krpc/` (hand-rolled KRPC), `cfg/` (config/socket probes), `negcheck*/` (dead crates),
`final/` (the recommended Cargo.toml, compiled and run).

---

## 1. Candidate evaluation

All version and date facts from the crates.io API (`GET /api/v1/crates/<name>`,
User-Agent header required, else 403).

| Crate | Ver | Date | License | BEP 5 client `announce_peer` + `get_peers`? | Sync / async | tokio | Verdict |
|---|---|---|---|---|---|---|---|
| **`mainline`** | **8.0.0** | 2026-08-04 | MIT | **Yes, both, as a first-class client API** | both (sync deprecated) | works, runtime-agnostic | **RECOMMENDED** |
| `dht` | 7.0.0 | 2026-08-05 | MIT | Yes, both, plus signed peers | both | works, runtime-agnostic | Viable alternative, less proven |
| `n0-mainline` | 0.6.0 | 2026-07-31 | MIT OR Apache-2.0 | Yes, both, plus signed peers behind a feature | async only | tokio-native (hard dep) | Viable alternative |
| `mtorrent-dht` | 0.5.4 | 2026-06-06 | Apache-2.0 | Peer *search* only; announce is implicit, not callable | async | **tokio `LocalSet` required** | Rejected |
| `magpie-bt-dht` | 0.1.3 | 2026-04-20 | Apache-2.0 OR MIT | Yes (`DhtRuntime::announce` / `find_peers`) | async | tokio | Rejected (too new, 132 downloads) |
| `bip_dht` | 0.6.0 | 2017-07-15 | MIT/Apache-2.0 | Yes, but only as `search(hash, announce: bool)` | sync, thread-based | no | Rejected (abandoned) |
| `mainline-dht` | 0.2.0 | 2021-10-30 | CC0-1.0 | Yes at message level | sync | no | **Rejected — does not compile** |

### 1.1 `mainline` 8.0.0 — the recommendation

Maintenance: 8 releases in the last 12 months, 142 241 recent downloads, 448 337
total, 17 reverse dependencies (it is the DHT under `pkarr` / Pubky).
Repository `https://github.com/pubky/mainline`. **License MIT.**
*Verification: crates.io API.*

Dependency surface is small and sane: `serde`, `serde_bencode`, `serde_bytes`,
`sha1_smol`, `crc`, `lru`, `ed25519-dalek 3.0.0-pre.1`, `getrandom 0.4`, `flume`,
`futures-lite`, `thiserror`, `tracing`, `dyn-clone`, `document-features`.
**No torrent library, no `tokio`.** *[SOURCE] `mainline-8.0.0/Cargo.toml`.*

Cargo features actually present *[SOURCE] `Cargo.toml`*:

```
default = ["full"]
full    = ["async"]
async   = ["node", "flume/async", "dep:futures-lite"]
node    = ["dep:flume"]
```

There is no `tokio`, no `quic`, no `server` feature. Do not invent any.

Public BEP 5 client API *[SOURCE] `src/dht.rs`, `src/async_dht.rs`; [COMPILED] used
in `live/`, `cfg/`, `final/`*:

```rust
// sync — present but every method carries #[deprecated(note = "use the async API via Dht::as_async() instead")]
impl Dht {
    pub fn get_peers(&self, info_hash: Id) -> GetIterator<Vec<SocketAddrV4>>;
    pub fn announce_peer(&self, info_hash: Id, port: Option<u16>) -> Result<Id, PutQueryError>;
}
// async — the supported path
impl Dht { pub fn as_async(self) -> AsyncDht; }
impl AsyncDht {
    pub fn get_peers(&self, info_hash: Id) -> GetStream<Vec<SocketAddrV4>>;   // impl futures Stream
    pub async fn announce_peer(&self, info_hash: Id, port: Option<u16>) -> Result<Id, PutQueryError>;
    pub async fn bootstrapped(&self) -> bool;
    pub async fn info(&self) -> Info;                 // .local_addr(), .id(), .dht_size_estimate()
    pub async fn find_node(&self, target: Id) -> Box<[Node]>;
}
```

`GetStream<T>` is `flume::r#async::RecvStream<'static, T>` and implements
`futures_core::Stream` — so `futures_lite::StreamExt::next()` works and it is
poll-driven by whatever executor you are on. *[SOURCE] `src/async_dht.rs:390-400`;
[COMPILED+RUN] under `#[tokio::main(flavor = "multi_thread")]`.*

**How it works under tokio.** `Dht::new()` spawns a plain OS thread
(`thread::Builder::new().name("Mainline Dht actor thread")`) that owns a blocking
`std::net::UdpSocket` with a 50 ms read timeout, and talks to the caller over
`flume` channels. Nothing blocks the tokio reactor; nothing needs a tokio feature.
*[SOURCE] `src/dht.rs:127-147`, `src/rpc/socket.rs:26-67`; [COMPILED+RUN].*

**Hard limitation: IPv4 only.** `KrpcSocket::new` does
`SocketAddr::V6(_) => unimplemented!("KrpcSocket does not support Ipv6")`.
*[SOURCE] `src/rpc/socket.rs:52-55`.* This matches reality — Mainline is an IPv4
network (BEP 32's IPv6 DHT is a separate table) — but it means **discovery is
IPv4-only** even though our libp2p layer can be dual-stack. Document it in
`NETWORK_STACK.md`.

### 1.2 `dht` 7.0.0

Same codebase lineage, different namespace: `repository = https://github.com/nuhvi/mainline`,
README title "Mainline". It is the author's newer crate name. Same MIT license.
**356 recent downloads, 2 reverse dependencies** — essentially unadopted so far.
*Verification: crates.io API; [SOURCE] `dht-7.0.0/README.md`.*

It has everything `mainline` has, plus an implementation of the proposed
"announce and lookup signed peers" BEP:

```rust
impl Dht     { pub fn  announce_signed_peer(&self, info_hash: Id, signer: &SigningKey) -> …;
               pub fn  get_signed_peers(&self, info_hash: Id) -> GetIterator<Vec<SignedAnnounce>>; }
impl AsyncDht{ pub async fn announce_signed_peer(&self, info_hash: Id, signer: &SigningKey) -> …;
               pub fn  get_signed_peers(&self, info_hash: Id) -> GetStream<Vec<SignedAnnounce>>; }
```

*[SOURCE] `dht-7.0.0/src/dht.rs:341,386`, `src/dht/async_dht.rs:189,237`,
`src/common/signed_announce.rs`; [COMPILED+RUN] in `dht7/`.* `SignedAnnounce` is
`{ key: [u8;32], timestamp: u64, signature: [u8;64] }`, ed25519 over
`encode_signable(info_hash, timestamp)`, with a 45 s timestamp tolerance
(`MAX_TIMESTAMP_TOLERANCE = 45 * 1000 * 1000` µs). Unlike `mainline` 8.0.0, its sync
API is **not** deprecated.

Measured on the live network (see §3.4) signed announces work but land on **2–3
storing nodes** versus **19–32** for a plain announce, because only the small
minority of Mainline nodes running this implementation understand the message.
That is a 10× loss of redundancy for a feature we do not need: the spec (§1, §3)
already says the DHT list is an unverified hint and identity is decided by the
libp2p handshake plus an application signature. **Do not adopt signed peers as the
primary discovery mechanism.**

### 1.3 `n0-mainline` 0.6.0

A tokio-native fork of the same lineage by n0-computer (the iroh people).
MIT OR Apache-2.0. 39 819 recent downloads but only 2 reverse dependencies —
the download count is the iroh CI, not independent adoption.
`get_peers` / `announce_peer` are both present and are **async-only**:

```rust
pub async fn get_peers(&self, info_hash: Id) -> Result<GetStream<Vec<SocketAddrV4>>, ActorShutdown>;
pub async fn announce_peer(&self, info_hash: Id, port: Option<u16>) -> Result<Id, PutQueryError>;
```

*[SOURCE] `n0-mainline-0.6.0/src/dht.rs:237,256`; [COMPILED+RUN] in `n0probe/`.*
Signed peers exist behind the non-default feature `unstable_signed_peers`
(the only non-default feature it has). It depends directly on `tokio 1.39` and
`noq-udp`. It has `port()` but **no `bind_address()` and no `request_timeout()`**
builder method — the timeout is adaptive with `MIN_REQUEST_TIMEOUT = 500 ms`.
*[SOURCE] `src/dht.rs:58-128`, `src/actor/socket.rs:26`.*

It is fast (see §3.3) but the shorter adaptive timeout made it noticeably less
robust in cold start here, and losing `bind_address` matters for a desktop app that
may need to pin an interface.

### 1.4 Rejected, with evidence

**`mtorrent-dht` 0.5.4** — the entire public control surface is three commands:

```rust
pub enum Command { AddNode { addr }, FindPeers { info_hash, callback, local_peer_port }, Shutdown }
```

*[SOURCE] `src/cmds.rs`.* There is no `announce_peer` you can call; announcing is a
side effect inside `run_search` driven by `local_peer_port`. Worse, the whole stack
is `!Send` — `Rc<Ctx>`, `Rc<Semaphore>` and `task::spawn_local` throughout, and the
crate's own doc example wraps it in `tokio::task::spawn_local`.
*[SOURCE] `src/processor.rs:36,129,336`, `src/queries.rs:33`, `src/lib.rs` doc example.*
That forces a tokio `LocalSet` and a dedicated single-threaded runtime next to the
libp2p swarm. Rejected.

**`magpie-bt-dht` 0.1.3** — technically the best-shaped API of the lot for us:
`DhtRuntime::announce(info_hash, port, private)` and
`DhtRuntime::find_peers(info_hash, private)`, and it explicitly "owns a shared UDP
socket via `magpie-bt-core`'s `UdpDemux`", which is the only crate here that even
contemplates port sharing. *[SOURCE] `src/runtime.rs:239,268`, `src/lib.rs` header.*
But: one release ever, 132 downloads, its own module doc still describes the crate
as shipping "the load-bearing data model and the wire codec only", and it is a
sub-crate of an unfinished BitTorrent engine. Against SPEC §28 (minimal, vetted
dependencies) this is not adoptable now. Worth re-checking in six months.

**`bip_dht` 0.6.0** — **it does still compile on Rust 1.95** *[COMPILED]*:

```rust
let b = bip_dht::DhtBuilder::with_router(bip_dht::Router::BitTorrent);
let _ = b.set_read_only(false);
```

builds clean, with `warning: the following packages contain code that will be
rejected by a future version of Rust: bitflags v0.4.0, net2 v0.2.39, nom v1.2.4`.
Last release 2017-07-15. It drags in `mio 0.5`, `chrono 0.2`, `nom 1.2`, `net2`, and
`start_mainline<H: Handshaker>` requires you to implement `bip_handshake::Handshaker`
— i.e. you must pretend to be a BitTorrent client. Announce is only reachable as
`search(hash, announce: bool)`. *[SOURCE] `src/builder.rs:56,117-182`.* Rejected.

**`mainline-dht` 0.2.0 — DOES NOT COMPILE.** *[COMPILED — failure reproduced]*:

```
error[E0554]: `#![feature]` may not be used on the stable release channel
error[E0599]: no method named `drain_filter` found for struct `Vec<NodeInfo>`
   --> mainline-dht-0.2.0/src/directory.rs:208:32
error[E0599]: no method named `drain_filter` found for struct `BTreeMap<K, V, A>`
   --> mainline-dht-0.2.0/src/table.rs:45:18
error: could not compile `mainline-dht` (lib) due to 6 previous errors
```

It uses `#![feature(drain_filter, btree_drain_filter, map_first_last, duration_constants)]`,
all of which are gone. Dead, and only revivable by forking. Rejected.

---

## 2. What BEP 5 actually says (the parts that bind us)

Fetched from `https://www.bittorrent.org/beps/bep_0005.html`, quoting the
mechanisms, not the prose:

* **Token.** `get_peers` always returns a `token`; `announce_peer` must present a
  token from a *recent* `get_peers` to the *same* node, and that node checks the
  token against the querying node's IP. The reference implementation uses
  `SHA1(IP ‖ secret)` with a **secret that changes every five minutes**, and
  **accepts tokens up to ten minutes old**.
* **Routing table freshness.** A node is "good" if it answered within the last
  **15 minutes**; buckets unchanged for **15 minutes** should be refreshed.
* **Announce lifetime.** BEP 5 **specifies no expiry at all** for stored peers, and
  no re-announce interval. That is a genuine gap in the spec: each implementation
  picks its own. Anything we say about TTL is therefore an empirical statement about
  the live network, not a protocol guarantee.
* Peers are stored as compact 6-byte `IP:port`; `implied_port=1` tells the storing
  node to use the UDP source port instead of the `port` argument.

---

## 3. Live measurements

### 3.1 Infohashes used

* **Debian** `481b6e3617be4c88f96cb25e47c9d8272130071e` —
  `debian-13.6.0-amd64-netinst.iso`, SHA-1 of the `info` dict of
  `https://cdimage.debian.org/debian-cd/current/amd64/bt-cd/debian-13.6.0-amd64-netinst.iso.torrent`
  (downloaded and hashed locally, not looked up).
* **Ubuntu** `01c137287d6f0ed05a56742dae794f632c79ff3d` —
  `ubuntu-24.04.4-desktop-amd64.iso`, from
  `https://releases.ubuntu.com/24.04/ubuntu-24.04.4-desktop-amd64.iso.torrent`,
  hashed the same way. Ubuntu is the busier of the two on the DHT.

### 3.2 UDP reachability from this machine (baseline, raw Python)

Bare KRPC `ping` from an ephemeral UDP port, and again bound to UDP/6881:

| Bootstrap node | Resolved | Replies (4 tries, 2 s timeout) | RTT |
|---|---|---|---|
| `router.bittorrent.com:6881` | 67.215.246.10 | **0/4** | — |
| `router.utorrent.com:6881` | 82.221.103.244 | **0/4** | — |
| `dht.transmissionbt.com:6881` | 87.98.162.88 / 212.129.33.59 | 4/4 | 21–26 ms |
| `dht.libtorrent.org:25401` | 185.157.221.247 | 4/4 | 31 ms |
| `relay.pkarr.org:6881` | 167.86.102.121 | 4/4 | 13 ms |

Repeated ~50 minutes later, identically: `router.bittorrent.com` 0/4 and
`router.utorrent.com` 0/4 again, `dht.transmissionbt.com` 4/4 at 24–25 ms,
`dht.libtorrent.org` 4/4 at 31 ms, `relay.pkarr.org` 4/4 at 14 ms. This is a stable
property of the network, not a transient blip.

**Correction made during the second verification pass:** an earlier draft of this note
said "two of the four `DEFAULT_BOOTSTRAP_NODES` are dead". That was wrong.
`router.utorrent.com` is dead but is **not** in the crate's default list (§4 prints
the real list: `router.bittorrent.com`, `dht.transmissionbt.com`,
`dht.libtorrent.org`, `relay.pkarr.org`). The accurate statement is:

**One of the four names in `DEFAULT_BOOTSTRAP_NODES` is dead from here — the
canonical `router.bittorrent.com`.** Both dead names in the table are the BitTorrent
Inc. routers; they resolve in DNS but answer nothing. The other three defaults all
answer 4/4. Outbound UDP is otherwise
unrestricted (4/4 to the other three, from both an ephemeral port and from 6881) and
the NAT is not blocking replies. This is the baseline against which every "it
failed" below has to be read, and it is the reason §8 insists on caching known-good
nodes between sessions.

### 3.3 `get_peers` against a busy public infohash

Each row is a **cold process start**: build client, bootstrap, one `get_peers`,
drain the stream to completion. Crate `mainline` 8.0.0, `#[tokio::main]`,
`request_timeout` default 2 s. *[COMPILED+RUN] `live/src/main.rs`.*

**Ubuntu infohash, `mainline` 8.0.0, 5 consecutive cold starts:**

| run | bootstrap | query wall time | nodes answering with values | raw peer records | **unique peers** |
|---|---|---|---|---|---|
| 1 | 2.51 s | 2.55 s | 34 | 1327 | **874** |
| 2 | 2.68 s | 2.63 s | 28 | 872 | **631** |
| 3 | 2.70 s | 2.52 s | 40 | 1149 | **746** |
| 4 | 2.89 s | 2.45 s | 50 | 1215 | **755** |
| 5 | 2.53 s | 2.45 s | 45 | 1157 | **768** |

**Debian infohash, `mainline` 8.0.0, 3 cold starts:** 269 / 260 / 275 unique peers,
query 2.58–2.82 s, first values at **165 / 141 / 146 ms**.

**Re-run 45 minutes later, same Ubuntu infohash, 3 fresh cold starts** (independent
verification pass):

| run | bootstrap | query wall time | nodes answering with values | raw peer records | **unique peers** | first value |
|---|---|---|---|---|---|---|
| 1 | 2.62 s | 2.54 s | 38 | 980 | **626** | 125.0 ms |
| 2 | 2.53 s | 2.49 s | 28 | 943 | **620** | 135.8 ms |
| 3 | 2.38 s | 2.39 s | 38 | 1034 | **680** | 121.2 ms |

Plus a fourth run under `RUST_LOG` (§3.6) at **734** unique peers, and the
recommended-`Cargo.toml` binary at **659**. So the honest figure is **620–880 unique
peers per cold lookup**, query wall time **2.4–2.6 s**, bootstrap **2.4–2.9 s**, in
every run across two sessions an hour apart.

**First-value latency is the number that matters for the lobby**: the first peers
arrive **113–165 ms** after the query is issued; the remaining ~2.5 s is the
iterative query fanning out. So a lobby can start dialling in well under a second
while discovery continues in the background.

**Head-to-head, same Ubuntu infohash, same machine, same minute:**

| Crate | bootstrap | `get_peers` wall | unique peers (5 runs) |
|---|---|---|---|
| `mainline` 8.0.0 | 2.51–2.89 s | 2.45–2.63 s | 631, 746, 755, 768, 874 |
| `n0-mainline` 0.6.0 | 1.0–2.0 s | 0.99–2.02 s | 811, 893, 931, 963, 978 |
| `dht` 7.0.0 | 0.6–1.3 s | 0.52–1.14 s | 0, 34, 740 |

`n0-mainline` is genuinely faster and found *more* peers. `dht` 7.0.0 is fastest and
by far the least stable — one of three runs returned **zero** peers and another
returned 34. The shorter adaptive timeouts buy latency at the cost of recall.

**Cold-start bootstrap failures.** Across roughly 35 process starts today,
**3 failed to bootstrap on the first attempt** (`bootstrapped=false`, 0 peers,
query giving up after one 2 s round). Every retry succeeded. With
`RUST_LOG=mainline=debug` a failing run shows `visited=107 responders=0` — every
first-round packet timed out. **The application must therefore treat a failed
bootstrap as normal and retry**, not as a fatal error. This is not a crate defect;
it is the state of the public bootstrap routers (§3.2).

### 3.4 `announce_peer` → `get_peers` round trip across two processes

Random 20-byte test infohash `5eddd13e97ad91981da84c6164e8cec866d56bc1`, so nothing
else on Earth had announced it.

**Process A** (`live.exe announce <ih> 45678 90`):

```
[t+44ms]    local_addr=0.0.0.0:6881  node_id=<redacted>…
[t+2.65s]   bootstrapped=true  dht_size_estimate=(5 791 263, 0.281)
[t+7.48s]   announce_peer OK  target=5eddd13e…  took=4.77s
holding for 90s ...
```

**Process B**, separate OS process, its own UDP socket on `0.0.0.0:58603`, its own
random node id, ~15 s later:

```
first peers batch at 113.3 ms (1 peers)
get_peers(5eddd13e…) DONE in 2.68s: responses_with_values=19  raw_peers=19  unique_peers=1
    peer 198.51.100.17:45678
```

Immediately repeated from a third fresh node (`0.0.0.0:51721`):

```
get_peers(5eddd13e…) DONE in 2.65s: responses_with_values=22  raw_peers=22  unique_peers=1
    peer 198.51.100.17:45678
```

**Result: the round trip works.** `announce_peer(ih, Some(45678))` stored
`198.51.100.17:45678` — our public IPv4 as seen by the storing nodes, and *our
chosen port*, not the DHT socket's port — and **19 then 22 independent third-party
nodes on the public internet handed that record back** to a different process
within ~113 ms of the query. Announce itself took **3.2–4.8 s** (it is an iterative
lookup followed by a write to the closest nodes).

**Reproduced independently 45 minutes later** on a second fresh random infohash
`25320ce93f9d0d7dafdc48c9247cc7c3e65f4506`, port `47001`. Announcer (its own process,
socket `0.0.0.0:6881`) bootstrapped in 2.63 s and `announce_peer` returned OK in
**4.46 s**. Two further separate processes, each with its own node id and its own
ephemeral UDP port (`0.0.0.0:59294`, `0.0.0.0:49887`), then looked it up:

```
lookup A: first values at 196.2 ms — DONE in 2.57s: responses_with_values=24 → 198.51.100.17:47001
lookup B: first values at 161.4 ms — DONE in 2.49s: responses_with_values=29 → 198.51.100.17:47001
```

Same result as the first pass: **the round trip works**, and across both experiments
a single announce lands on **19–32 independent storing nodes**.

Caveat stated plainly: both processes were behind the same NAT and share a public
IP. That does **not** weaken the DHT result — the record travelled through storing
nodes across the public internet and came back — but it proves nothing about NAT
traversal. Whether two peers on different networks can actually *connect* on that
IP:port is a libp2p/DCUtR/relay question, answered in the libp2p research note, not
here.

**Announce latency and success rate.** Five cold starts announcing port `45777` on a
fresh random infohash `91913cffa1f2b29e2bea595f4569deefcd993276`: **5 of 5
succeeded**, taking **3.03 / 4.65 / 4.67 / 4.86 / 4.66 s** (bootstrap 2.1–2.8 s of
that is separate and already counted). Looking that infohash up afterwards:

```
get_peers DONE in 2.62s: responses_with_values=54  raw_peers=54  unique_peers=1
    peer 198.51.100.17:45777        first value at 125.9 ms
```

**54** storing nodes after five announces, versus 19–32 after one. Repeat announces
widen the storing set instead of just refreshing it, and the address de-duplicates
correctly on the reader side. That is a further argument for a short re-announce
interval: it buys redundancy, not only freshness.

**Signed peers (`dht` 7.0.0), same experiment**, infohash `5016c0af…`:
`announce_signed_peer` succeeded, and a second process's `get_signed_peers`
returned the announce from **2** storing nodes (a same-process check on another
infohash saw 3). A plain `get_peers` on the same infohash returned **0** — signed
announces live in a separate namespace and are invisible to standard clients.
2–3 storing nodes versus 19–32 is the cost of using a BEP that is still a pull
request.

### 3.5 Announce lifetime measured on the live network

This is the one number BEP 5 refuses to define (§2), so it had to be measured.

**Sample A.** Random infohash `622b08ec85325af013897dda4e6506e7ee2db576`. One
`announce_peer(ih, Some(45999))` at 07:41:02 UTC; **the announcing process exited at
07:41:08** and never re-announced, so nothing refreshed the record. A fresh client
(new process, new node id, new UDP port each time) then polled every 5 minutes. The
count is `responses_with_values` — how many independent third-party nodes still had
the record and handed it back. *[COMPILED+RUN] `live/` driven by `ttl_probe.sh`;
raw log `probe-dht/ttl.log`.*

| t after announce | wall clock (UTC) | nodes still storing it | our record returned? |
|---|---|---|---|
| +5 min | 07:46:10 | 29 | yes |
| +10 min | 07:51:16 | 32 | yes |
| +15 min | 07:56:22 | 22 | yes |
| +20 min | 08:01:27 | 31 | yes |
| +25 min | 08:06:33 | 32 | yes |
| +30 min | 08:11:38 | 24 | yes |
| +35 min | 08:16:44 | 20 | yes |
| +40 min | 08:21:50 | 15 | yes |
| +45 min | 08:26:55 | **9** | yes |
| **+50 min** | 08:32:00 | **0** | **gone** |
| +56 min | 08:37:08 | 0 | gone |

An unscripted manual lookup at 08:31 UTC (≈ t+50) also returned 0, independently of
the polling script.

**Shape of the curve, and what it means.** The record does not sit flat and then fall
off a cliff at a fixed TTL, and it does not decay smoothly either. It holds a roughly
constant 22–32 storing nodes for the first ~30 minutes — the noise there is which
subset of the keyspace neighbourhood a given lookup happens to reach, not decay —
then falls monotonically 24 → 20 → 15 → 9 → 0 between t+30 and t+50.

Two things are worth separating here — what the data shows, and what I am inferring.

*Shown:* the storing set does not vanish at one instant, it drains over roughly
20 minutes. So the ~50 storing nodes are **not running the same fixed timer**; if
they were, they would all drop the record within one polling interval of each other.

*Inferred, and I cannot prove it from the wire:* the likeliest cause is
capacity-based eviction rather than expiry. `mainline`'s own storage side is an LRU
with no clock at all (§5) — a record is dropped only when something else pushes it
out — and if a meaningful share of the storing nodes behave that way, a record
survives exactly as long as it takes other people's announces to evict it. I did not
survey what the other implementations in that neighbourhood actually do, so treat the
mechanism as a well-supported hypothesis, not a verified fact. What matters
operationally is the same either way.

**Treat ~45 minutes as an observation from this network on this day, not a
guarantee.** Both samples were *private random* infohashes with essentially no
competing announces — the quietest possible corner of the keyspace. A publicly known
`LOBBY_INFOHASH` that many clients announce to will sit in a busier neighbourhood and
should be expected to evict faster, possibly much faster.

**Sample B — an independent replication that landed on the same answer.** A second
random infohash `2f59ec56c7dd6013e5b070a42762a20a496d1f8c`, announced once at
08:03:44 UTC on port `45888`, announcer exited immediately, polled every 3 minutes
and then every 5. *Raw logs `probe-dht/ttl2.log`, `probe-dht/ttl3.log`.*

| t after announce | +3 | +6 | +9 | +12 | +15 | +18 | +21 | +24 | +27 | +30 | +33 | +37 | +41 | +46 | **+51** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| nodes still storing it | 35 | 56 | 31 | 32 | 21 | 41 | 30 | 36 | 35 | 29 | 17 | 16 | 16 | **2** | **0** |

(The `+24 min` poll is the one that also returned a stranger's address under this
private random infohash — see §7.)

Two independent samples, different infohashes, announced 23 minutes apart, both
plateau in the 20–50 range for ~30 minutes and both reach zero between **t+50 and
t+51 minutes**. The agreement is close enough to treat **~45 min retrievable, dead by
~50 min** as the working figure for this network today — while remembering it is LRU
pressure, not a protocol timer, so it is not portable to a busy infohash.

**Consequence:** a client that announces once and then goes quiet disappears from the
lobby in well under an hour, with the storing-node count — i.e. the probability that
any given lookup finds us at all — already halved by t+40. The re-announce interval
adopted in §5 (10 minutes) sits far inside that, deliberately.

### 3.6 How many strangers learn about us (privacy measurement)

With `RUST_LOG=mainline::rpc::iterative_query=debug`, one `get_peers` on the Ubuntu
infohash reports (two separate cold runs, 07:xx and 08:28 UTC):

```
Done query id=01c137287d…  closest=468  visited=105  responders=74
Done query id=01c137287d…  closest=702  visited=176  responders=116
```

**105–176 distinct DHT nodes were contacted and told our IP address is asking for
that infohash**, of which 74–116 answered. Each run also issues a self-lookup to
populate the routing table, which visited another 132 and 140 nodes respectively —
those learn our IP but not what we are looking for. So the per-lookup disclosure
budget is **~100–180 strangers who learn (our IP, the infohash)** plus another
~130–140 who learn only our IP. See §7.

---

## 4. Socket and port control

*[SOURCE] `mainline-8.0.0/src/rpc/config.rs`, `src/rpc/socket.rs:38-66`,
`src/dht.rs:37-122`; [COMPILED+RUN] `cfg/src/main.rs`.*

`Config` exposes exactly these knobs, and `DhtBuilder` has one method each:

| Field / builder | Default | Meaning |
|---|---|---|
| `bootstrap: Option<Vec<SocketAddrV4>>` / `.bootstrap(&[T])` | `DEFAULT_BOOTSTRAP_NODES` | **replaces** the list |
| `.extra_bootstrap(&[T])` | — | appends to whatever is already set — **see footgun below** |
| `.no_bootstrap()` | — | sets an empty list (for a private testnet) |
| `port: Option<u16>` / `.port(u16)` | `None` | `None` ⇒ try 6881, fall back to an ephemeral port |
| `bind_address: Option<Ipv4Addr>` / `.bind_address(Ipv4Addr)` | `0.0.0.0` | IPv4 only |
| `request_timeout: Duration` / `.request_timeout(_)` | `DEFAULT_REQUEST_TIMEOUT` = **2 s** | per-request UDP timeout |
| `public_ip: Option<Ipv4Addr>` / `.public_ip(_)` | `None` | BEP 42 secure node id seed |
| `server_mode: bool` / `.server_mode()` | `false` (adaptive) | answer other people's queries |
| `server_settings: ServerSettings` | `MAX_INFO_HASHES=2000`, `MAX_PEERS=500`, `MAX_VALUES=1000` | storage caps when in server mode |

Verified at runtime *[COMPILED+RUN]*:

```
DEFAULT_BOOTSTRAP_NODES = ["router.bittorrent.com:6881", "dht.transmissionbt.com:6881",
                           "dht.libtorrent.org:25401", "relay.pkarr.org:6881"]
DEFAULT_REQUEST_TIMEOUT = 2s
pinned local_addr = 0.0.0.0:46881
bootstrapped(custom bootstrap = transmissionbt + libtorrent only) = true   (3 of 3 runs)
peers via custom bootstrap = 1073 / 936 / 1071
no_bootstrap node bootstrapped = false                                     (as designed)
```

So **yes: we fully control the port and the bootstrap list.**

> **Footgun, verified in source, contradicting the crate's own doc comment.**
> `extra_bootstrap`'s doc says "Add more bootstrap nodes to **default** bootstrapping
> nodes", but the body is
> `let mut bootstrap = self.0.bootstrap.clone().unwrap_or_default();` — if you have
> not called `.bootstrap()` first, `bootstrap` is `None`, `unwrap_or_default()` gives
> an **empty** vec, and your extras *replace* the defaults rather than augmenting
> them. *[SOURCE] `src/dht.rs:64-72`.* If we want defaults-plus-ours we must write
> `.bootstrap(DEFAULT_BOOTSTRAP_NODES).extra_bootstrap(&ours)` explicitly. Add a
> regression test for this when we wire it up.

### Can it share a UDP port with anything else? **No.**

`KrpcSocket::new` constructs its own `std::net::UdpSocket` from the config; there is
no constructor, no `From<UdpSocket>`, no trait, no injection point *[SOURCE]
`src/rpc/socket.rs:38-66`*. And the socket is exclusive — a second bind on the same
port fails *[COMPILED+RUN]*:

```
SECOND bind on 46881 FAILED: os error 10048 (only one usage of each socket address … is permitted)
```

`SO_REUSEADDR`/`SO_REUSEPORT` are never set. **Consequence for our architecture: the
DHT gets its own UDP socket, separate from libp2p's QUIC socket.** That is two UDP
ports and two NAT mappings per client. It also means the port we `announce_peer` is
*not* the port the DHT speaks from, so `implied_port` is wrong for us — we must pass
`Some(quic_port)` explicitly. Note the consequence honestly: the external port of
the QUIC mapping is what remote peers need, and behind a NAT that may differ from
the local port; the value to announce should come from libp2p's observed/external
address (AutoNAT / identify), not from `local_addr()`. Announcing a stale or wrong
port makes the DHT hint useless but is not a security problem — the spec already
treats the list as an unverified hint.

`mainline` never touches disk and never writes config, which suits the
portable-directory requirement in SPEC §22.

`AsyncDht::to_bootstrap()` returns `Vec<String>` of the **non-stale** routing-table
addresses (`routing_table.nodes().filter(|n| !n.is_stale()).map(address)`)
*[SOURCE] `src/common/routing_table.rs:129-134`, `src/dht.rs:560-562`*. Measured
after one bootstrap: **4 / 76 / 116** nodes across three runs, and **78** on a fourth
run during the verification pass *[COMPILED+RUN]*. The `4` is the outlier worth
noting: on a run where bootstrap barely succeeded there is almost nothing worth
caching, so the persisted list must be *merged* with the previous session's, not
overwritten with whatever this session happened to see.
Persist that list next to the profile and feed it back next launch — it is the
single best fix for the dead-bootstrap-router problem in §3.2.

### 4.1 Adaptive mode: the client promotes itself to a public DHT server

This is not obvious from the API and matters for a desktop app.
`Dht::client()` starts in "adaptive mode". On every routing-table refresh tick
(`REFRESH_TABLE_INTERVAL = 15 min`) the node runs:

```rust
fn try_switching_to_server_mode(&mut self) {
    if !self.server_mode() && !self.firewalled() {
        self.socket.server_mode = true;   // now answers unsolicited requests
    }
}
```

*[SOURCE] `src/rpc.rs:775-806`.* **There is no opt-out.** `DhtBuilder::server_mode()`
only forces it *on*; there is no `client_only()`. So a user on a public IP or with a
port-forward will, after ~15 minutes, silently begin answering strangers' `ping` /
`find_node` / `get_peers` / `announce_peer` and **storing other people's data** — up
to 2000 infohashes × 500 peers, plus 1000 immutable and 1000 mutable BEP 44 values.
For a poker client that is unexpected bandwidth, unexpected inbound traffic, and
unexpected third-party content on the user's machine.

Measured on this machine: `firewalled=true` persisted across every run
(`public=Some(198.51.100.17:64708)` was learned, but public addressability was never
confirmed), so **this NAT-ed client never promoted** — which will be the common case
for our users. *[COMPILED+RUN] `filter/src/main.rs`.*

The supported way to hard-block it is a deny-all `RequestFilter`, which compiles and
runs *[COMPILED+RUN]*:

```rust
use mainline::{Dht, RequestFilter, RequestSpecific, ServerSettings};

#[derive(Debug, Clone)]
struct DenyAll;

impl RequestFilter for DenyAll {
    fn allow_request(&self, _request: &RequestSpecific, _from: SocketAddrV4) -> bool { false }
}

let dht = Dht::builder()
    .port(0)
    .server_settings(ServerSettings {
        filter: Box::new(DenyAll),
        max_info_hashes: 1,
        max_peers_per_info_hash: 1,
        max_immutable_values: 1,
        max_mutable_values: 1,
    })
    .build()?
    .as_async();
```

`RequestFilter` is `Send + Sync + Debug + DynClone`; `ServerSettings` has all five
fields public and no `..Default::default()` shortcut is needed. Ship this filter, and
put a "contribute to the DHT" opt-in toggle in settings if we ever want to be good
citizens deliberately rather than by accident.

---

## 5. Announce TTL and re-announce interval

**What BEP 5 says:** nothing. No expiry, no interval. It only pins the *token*
lifetime — secret rotated every 5 min, tokens accepted up to 10 min old.

**What `mainline` 8.0.0 implements** *[SOURCE] `src/rpc/server/peers.rs`,
`src/rpc/server/tokens.rs`, `src/common/node.rs:15`, `src/rpc.rs:50-51`*:

* Its **storage side** (server mode) is an LRU, not a TTL: `PeersStore` is
  `LruCache<Id, LruCache<Id, SocketAddrV4>>` with `MAX_INFO_HASHES = 2000` and
  `MAX_PEERS = 500` per infohash. A peer record is evicted when it is the least
  recently used, **never on a clock**. `get_random_peers` returns a random subset
  capped at 10 per response.
* `TOKEN_ROTATE_INTERVAL = 300 s` (5 min) *[SOURCE] `src/common/node.rs:15`*, and
  `Tokens::validate` accepts the current *or* previous secret ⇒ effective token
  window 5–10 min. That matches BEP 5's *timing*, but note the construction differs:
  BEP 5's reference implementation is `SHA1(IP ‖ secret)`, whereas `mainline` uses
  **CRC-32/ISCSI (Castagnoli) over the address plus a 20-byte secret, giving a 4-byte
  token** (`SECRET_SIZE = 20`, `TOKEN_SIZE = 4`, `CASTAGNOLI`)
  *[SOURCE] `src/rpc/server/tokens.rs:12-14,49-56`*. BEP 5 explicitly leaves the
  construction undefined ("the implementation is not defined"), so this is conforming
  — but it is why the hand-rolled KRPC probe in §6 sees 8-byte tokens from other
  implementations and would see 4-byte ones from `mainline` servers. A hand-rolled
  client must treat the token as an opaque byte string of any length.
* `REFRESH_TABLE_INTERVAL = 15 min`, `PING_TABLE_INTERVAL = 5 min` for the routing
  table.
* **There is no automatic re-announce.** `announce_peer` is a one-shot query; no
  timer, no background task, nothing in the actor loop refreshes it.
  **The application owns the re-announce loop.**

**What the live network does:** measured in §3.5. A single announce, never refreshed,
was retrievable for **45 minutes and gone by 50**, with the number of storing nodes
already down from ~30 to 9 at t+45. There is no fixed TTL — it is LRU eviction, so a
busier keyspace neighbourhood evicts faster and the 45 minutes is an observation, not
a bound we can rely on.

**The rule we adopt:** re-announce `LOBBY_INFOHASH` **every 10 minutes** while the
client is running, on its own task, and treat a failed announce as retryable rather
than fatal. Rationale, now with the measurement behind it:

* **Upper bound from the network:** the record measurably starts thinning at ~t+30
  and is gone by t+50 (§3.5). Ten minutes gives us four to five refreshes inside the
  observed lifetime, so no single failed announce can make us disappear.
* **Upper bound from BEP 5:** tokens are accepted up to 10 minutes old and a node is
  "good" only if it answered within 15 minutes. A 10-minute cadence keeps us inside
  both windows.
* **It buys redundancy, not just freshness.** Five repeated announces put us on **54**
  storing nodes versus 19–32 for one (§3.4), and readers de-duplicate the address
  correctly. More refreshes ⇒ more independent nodes can answer for us ⇒ a higher
  chance any given lookup finds us.
* **Cost:** one iterative query per interval, ~3–5 s of background traffic. Negligible.

Also re-announce immediately after any network change (interface up, address change),
because the stored record is keyed to the IP the storing node saw. And note the
privacy cost this cadence implies: §7 counts it at roughly 150 disclosure events per
day.

---

## 6. If no crate were adequate: scoping our own KRPC client

The spec (§2) permits this and forbids pulling in a whole torrent library. We do
**not** need it — but it is worth knowing the size of the escape hatch, so here is a
*measured* estimate rather than a guess.

**Bencode crate: `serde_bencode` 0.2.4** (MIT, 876 k downloads). It is what
`mainline`, `dht` and `n0-mainline` all use. *[COMPILED+RUN]*. `bendy` 0.6.1
(BSD-3-Clause, enforced canonical encoding) is the alternative and is better
maintained (2025-11-10); it would be the choice if we ever needed canonicity, which
for KRPC we do not — but note KRPC replies from the wild are *not* canonical
bencode, so a strict decoder must be tolerant on input.

A **158-line** probe *[COMPILED+RUN] `krpc/src/main.rs`* already does: bencode
serialize of `ping` / `get_peers` / `announce_peer`, tolerant deserialize of
responses and errors, compact-node (26 B) and compact-peer (6 B) parsing, and a real
UDP round trip. Live output against `dht.transmissionbt.com:6881`:

```
ping      -> y="r"  rtt=22.2ms  responder_id_len=20
get_peers -> rtt=22.1ms  token=false  nodes=8  peers=0
  hop2 203.0.113.27:18263: token=Some(8) nodes=8 peers=0     (×4 of 6, 2 timed out)
```

Note the operationally important detail this surfaced: **the bootstrap routers do
not hand out tokens** (`token=false`), only real storage nodes do (`token=Some(8)`,
8 bytes). Any hand-rolled announce must therefore do at least a two-hop iterative
lookup before it can write anything.

What the 158 lines do **not** yet contain, and what makes the real cost:

| Part | Est. lines |
|---|---|
| KRPC codec: messages, errors, tolerant decode, `v`/`ip` handling | ~250 |
| Transaction table: tid allocation, correlation, timeout, retry | ~150 |
| Kademlia routing table: 160 buckets, k=8, good/questionable/bad, refresh | ~350 |
| Iterative lookup (α=3 concurrency, closest-set, termination) | ~250 |
| Token cache per node + `announce_peer` write phase | ~100 |
| Bootstrap, self-lookup, periodic table maintenance | ~150 |
| BEP 42 secure node id (CRC32C over IP) — needed or we get deprioritised | ~80 |
| Server side (answer `ping`/`find_node`/`get_peers`/`announce_peer`) — optional | ~250 |
| Rate limiting, message size caps, hardening per SPEC §27 | ~150 |
| Tests incl. a local testnet | ~400 |
| **Total, client-only** | **≈ 1500–1800 lines** |
| **Total, with server mode** | **≈ 2000–2400 lines** |

That is a month of careful work plus an indefinite tail of "why do 30 % of nodes
ignore us", to reproduce an MIT-licensed crate with 448 k downloads that already
does it. **Do not build this.** Keep the estimate on file as the contingency if
`mainline` is ever abandoned; the `net/dht.rs` module should expose a narrow trait
(`announce`, `lookup`) so that swap stays a one-file change.

---

## 6a. One genuine API gap in `mainline` 8.0.0 (SPEC §2 requires this be written down)

**Missing: the number of nodes that accepted our announce is unreachable.**

The crate computes it. `AsyncDht::put` returns
`PutOutcome { target: Id, stored_at: u32 }` — "Number of DHT nodes that acknowledged
storing the item" *[SOURCE] `src/rpc/put_query.rs:197-205`, `src/async_dht.rs:361-370`*.
But `announce_peer` throws it away:

```rust
pub async fn announce_peer(&self, info_hash: Id, port: Option<u16>) -> Result<Id, PutQueryError>
```

and the obvious workaround — calling `put()` directly — **does not compile**, because
the payload type of the variant is not exported. Verified with the compiler
*[COMPILED — failure reproduced]*:

```
error[E0422]: cannot find struct, variant or union type `AnnouncePeerRequestArguments` in crate `mainline`
  --> src\main.rs:14:58
   |
14 |  let req = PutRequestSpecific::AnnouncePeer(mainline::AnnouncePeerRequestArguments { … });
```

`lib.rs` declares `mod common;` (private) and re-exports only
`rpc::messages::{MessageType, PutRequestSpecific, RequestSpecific}`. The payload
struct itself is defined at `src/common/messages.rs:156` as
`pub struct AnnouncePeerRequestArguments` but is re-exported nowhere — grepping
`lib.rs`, `rpc.rs` and `rpc/messages.rs` for the name returns nothing. So the variant
`PutRequestSpecific::AnnouncePeer(_)` is **unconstructible from outside the crate**
*[SOURCE] `src/lib.rs:10,26-32`, `src/common/messages.rs:156`; [COMPILED — failure
reproduced twice, 45 minutes apart]*.

Why we care: SPEC §22 requires a "DHT status" indicator in the GUI, and `stored_at`
is precisely the honest health signal — "announced to 14 nodes" versus "announced to
0". Right now `announce_peer` returns `Ok(target_id)` regardless of whether one node
or twenty stored the record.

**Minimal upstream addition** (per SPEC §2: extend the existing crate, do not write
our own protocol) — one non-breaking method:

```rust
/// Same as `announce_peer`, but reports how many nodes stored the record.
pub async fn announce_peer_detailed(&self, info_hash: Id, port: Option<u16>)
    -> Result<PutOutcome, PutQueryError>
```

or simply `pub use` the `*RequestArguments` structs. Until it lands, our
`net/dht.rs` reports announce success as a boolean and derives a health number by
doing a `get_peers(LOBBY_INFOHASH)` right after announcing and counting how many
responders returned *our own* address. Hold a local patch if we need it sooner.

---

## 7. Privacy consequences of a fixed public `LOBBY_INFOHASH`

This is not a footnote. It is the single largest privacy cost of the SPEC §3 design,
and SPEC §3 itself requires us to document it. Concretely, with numbers from §3:

**What we publish, by construction.** `announce_peer(LOBBY_INFOHASH, Some(port))`
writes the tuple `(public IPv4, port)` into the DHT, readable by anyone, with no
authentication and no access control. Our measurement: a second, unrelated process
retrieved `198.51.100.17:45678` from **19–32 independent nodes** within ~113 ms of
asking. **Anyone who knows `LOBBY_INFOHASH` — and it ships in every copy of the
binary, so everyone does — can enumerate the players.** There is no "unlisted" mode.

**Who learns it, and how many.** Per §3.6, a single `get_peers` contacts **105
distinct DHT nodes**; an `announce_peer` does the same iterative lookup and then
writes to the closest ~8–20. So each announce/lookup cycle tells on the order of
**100+ arbitrary internet hosts** that this IP is interested in this specific
20-byte value. Repeat every 10 minutes and that is ~150 disclosures per day, to a
rotating and self-selected set of strangers — Mainline is heavily crawled by
anti-piracy monitors, academic scanners and Sybil nodes, all of whom log exactly
this.

**Measured proof that strangers are watching, not just that they could.** During the
TTL experiment (§3.5, sample B) the infohash `2f59ec56c7dd…` was **20 random bytes
generated on this machine**. Nobody else on Earth had any reason to know it existed;
the only event that ever referenced it was our single `announce_peer`
UTC. Twenty-four minutes later, a lookup returned **two** peers:

```
--- t+24 min 08:28:22
get_peers(2f59ec56c7dd6013e5b070a42762a20a496d1f8c) DONE in 2.51s:
    responses_with_values=36  unique_peers=2
    peer 203.0.113.28:31786      <-- not us
    peer 198.51.100.17:45888       <-- us
```

An unrelated host announced itself under a secret we had published only into the DHT,
within half an hour of us publishing it. That is DHT crawling observed first-hand:
somebody is harvesting announce traffic near the keyspace and re-announcing under
whatever infohashes they see. (`203.0.113.28` appeared in one poll and was gone by
the next, so it is a single observation, not a persistent squatter — but one
observation is enough to settle the question of whether anyone is looking.) A
*publicly known, hard-coded* `LOBBY_INFOHASH` is a vastly easier target than a random
one, so treat this as a floor on the surveillance, not a ceiling. It is also a live
demonstration of the pollution problem: the DHT will hand our client addresses that
never ran this software.

**What an observer can derive, concretely:**

1. **The player roster.** Poll `get_peers(LOBBY_INFOHASH)` on a loop from a handful
   of vantage points and you get a near-complete list of IP addresses running this
   poker client, refreshed every few minutes. Each node returns only a random ≤10
   subset, so one query undercounts — but repeated polling converges, and running a
   few Sybil nodes near the infohash in the keyspace gives you the write stream
   directly.
2. **Session times.** Because a record dies without re-announce (§3.5) and we
   re-announce every 10 min, presence is a ~10-minute-resolution online/offline
   signal per IP. Over weeks that is a behavioural profile: when this person plays,
   how long, how often, which time zone.
3. **Geolocation and ISP** by trivial GeoIP on the IP, plus a reverse-DNS hostname
   in many cases.
4. **Linkage across time.** A residential IP is stable for days to months, so the
   same IP re-appearing links sessions. Combined with the libp2p PeerId (which the
   spec correctly makes persistent, SPEC §3), an observer who dials us gets a stable
   cryptographic identity **bound to a physical location**. That is a stronger link
   than either identifier alone.
5. **Target list for attack.** The roster doubles as a list of hosts to DoS, port
   scan, or attempt to de-anonymise. Poker adds motive: knowing which IP is at which
   table is the first step in targeted collusion or in DoSing an opponent who is
   about to act (SPEC §18 already lists DoS as out of the protocol's reach — this is
   how an attacker finds the target in the first place).
6. **Simple "is this person a poker player" answer.** For anyone who can observe
   the user's network (ISP, employer, household, a legal request), a single UDP
   packet carrying a well-known 20-byte constant is a clean, greppable signature.
   Announcing on `LOBBY_INFOHASH` is not steganographic in any way.

**What it does *not* leak:** nothing about cards, stacks, table membership, or game
state — all of that is SPEC-mandated to travel over libp2p between table
participants only, never through the DHT. And a DHT record is not an identity claim:
anyone can announce any IP:port under our infohash (there is no proof of possession
beyond the token's IP check), so the list is a hint, exactly as SPEC §1 says. That
cuts both ways — an attacker can also **pollute** the lobby with thousands of junk
entries and make discovery slow and expensive, which is a DoS worth a bounded
dial-attempt budget in `net/dht.rs`.

**Mitigations that are honest** (none of them eliminates the disclosure; the SPEC
forbids a central fallback, and rightly):

* **Tell the user, before the first announce.** A one-time, plain-language consent
  screen: "joining the lobby publishes your IP address to a public network; anyone
  can see that this computer is running this poker client." SPEC §3 requires that
  this be documented; making it a UI element is the honest version.
* **Offline / direct-invite mode.** Let a user play without ever announcing, joining
  a table by an out-of-band ticket (multiaddr + table key). Discovery is then zero
  and the privacy cost is zero. This should exist from day one.
* **Announce only while actually looking for a game**, not for the whole time the
  app is open, and stop announcing the moment the user is seated. Fewer records,
  shorter presence trail.
* **Do not put anything except IP:port in the DHT.** No nicknames, no PeerId, no
  table metadata. That is already the design; keep it that way.
* **VPN/Tor** is the user's option, and we should say so, but note it plainly:
  Mainline is UDP and Tor is not, so Tor does not work for the DHT — a VPN does, at
  the cost of trusting the VPN.
* Optional later: derive a rotating infohash from a coarse epoch
  (`SHA1("p2p-poker/lobby/v1" ‖ floor(day))`) so that a historical crawl of one
  constant does not index the whole user base forever. This costs
  cross-version-compat and does not stop a live observer, so it is an improvement,
  not a fix, and it must be a deliberate decision — the SPEC currently mandates one
  fixed constant.

---

## 8. RECOMMENDATION

**Use `mainline = "=8.0.0"`, async API only, in a dedicated `net/dht.rs` module
behind a narrow internal trait.**

Why, in order of weight:

1. It is the only candidate that is simultaneously **(a)** maintained (8 releases in
   12 months, 142 k recent downloads, 17 reverse deps, MIT), **(b)** a real BEP 5
   *client* with `announce_peer` and `get_peers` as first-class methods, **(c)**
   runtime-agnostic so it does not fight libp2p's tokio runtime, and **(d)** free of
   any torrent-library baggage — its whole dependency set is serde + bencode + sha1 +
   crc + lru + ed25519 + flume + tracing.
2. **It is proven end-to-end on this machine, twice, in two sessions an hour apart**:
   620–880 unique peers per cold lookup on a public infohash in ~2.5 s with first
   values at 113–196 ms, and a two-process announce→lookup round trip on two
   different fresh random infohashes, each returning our own `IP:port` from 19–32
   independent third-party nodes.
3. `n0-mainline` 0.6.0 is the credible second choice and was measurably *faster*
   (816–978 peers, ~1–2 s). Reasons it is second, not first: it hard-depends on
   tokio (fine for us, but a coupling), has no `bind_address` or `request_timeout`
   control, its shorter adaptive timeout was less robust on cold start here, and it
   is at `0.x` with 2 reverse deps. **Re-evaluate at 1.0.** `dht` 7.0.0 is third:
   same author, newer, but 356 recent downloads and a run that returned zero peers.
4. `mtorrent-dht`, `magpie-bt-dht`, `bip_dht`, `mainline-dht` are rejected with the
   evidence in §1.4, including a compiler-verified negative for `mainline-dht`.
5. **Do not write our own KRPC client.** §6 sizes it at 1500–2400 lines to
   reimplement an MIT crate. Keep the trait boundary so we *could*.

Binding implementation rules that fall out of the measurements:

* **Async API only.** Every sync method on `Dht` is `#[deprecated]` in 8.0.0; use
  `Dht::builder()…build()?.as_async()`.
* **Separate UDP socket.** The DHT cannot share libp2p's QUIC port (§4, verified).
  Bind it with `.port(0)` — do **not** default to 6881, which fingerprints us as a
  BitTorrent client and collides with any real torrent client on the machine.
* **Announce `Some(external_quic_port)`, never `implied_port`.** The port the DHT
  speaks from is not the port peers must dial.
* **Re-announce every 10 minutes** on its own task, and immediately after any
  network change (§5). Nothing in the crate does this for us, and BEP 5 defines no
  TTL — but measured twice on two random infohashes, an un-refreshed record is
  **retrievable for ~45 min and gone by ~50** (§3.5), thinning steadily from t+30.
  Ten minutes gives four refreshes inside that window and also widens the storing
  set (5 announces ⇒ 54 nodes vs 19–32 for one).
* **Retry bootstrap.** ~3 of 35 cold starts failed on the first attempt (§3.3);
  one of the four default bootstrap routers — the canonical `router.bittorrent.com`
  — is dead from this network, confirmed twice ~50 min apart (§3.2).
  Ship a hard-coded extra bootstrap list, cache good nodes from the last session on
  disk (`AsyncDht::to_bootstrap()` returns exactly that list), and feed them back
  with `.bootstrap(DEFAULT_BOOTSTRAP_NODES).extra_bootstrap(&cached)` — mind the
  `extra_bootstrap` footgun in §4.
* **Treat every returned address as hostile input** (SPEC §1): cap the dial budget,
  de-duplicate, drop private/reserved ranges, and let the libp2p handshake plus the
  application signature decide identity. The DHT never sees game state.
* **IPv4-only discovery** (§1.1). Say so in `NETWORK_STACK.md`.
* **Surface the privacy cost in the UI before the first announce** (§7), and ship a
  no-announce direct-invite mode.

### Verified `Cargo.toml` snippet

Compiled **and executed** as written, at `probe-dht/final/` *[COMPILED+RUN]*:

```toml
[dependencies]
# BEP 5 Mainline DHT — global peer discovery only. MIT. No tokio dependency of its own;
# it runs its UDP loop on a dedicated OS thread and talks over flume channels.
# default-features = false drops nothing we use: the "async" feature already implies "node".
mainline = { version = "=8.0.0", default-features = false, features = ["async"] }

# for StreamExt::next() over mainline::async_dht::GetStream
futures-lite = "2.6"

# our own runtime (shared with libp2p)
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time"] }
```

### Verified skeleton for `src/net/dht.rs`

Compiled and run as written; only the placeholder infohash is a stand-in.

```rust
use std::net::SocketAddrV4;
use std::time::Duration;

use futures_lite::StreamExt;
use mainline::{async_dht::AsyncDht, Dht, Id};

/// Fixed lobby infohash — a project constant shipped in every client (SPEC §3).
const LOBBY_INFOHASH_HEX: &str = "…20 bytes of hex…";
/// See docs/research/MAINLINE_DHT.md §5. BEP 5 defines no TTL; 10 min sits inside
/// the token window (10 min) and the good-node window (15 min).
const REANNOUNCE_INTERVAL: Duration = Duration::from_secs(10 * 60);

fn build_dht() -> std::io::Result<AsyncDht> {
    Ok(Dht::builder()
        .port(0)                                        // never 6881: do not look like a torrent client
        .request_timeout(Duration::from_millis(2500))
        .build()?
        .as_async())
}

async fn discover(dht: &AsyncDht, ih: Id) -> Vec<SocketAddrV4> {
    let mut out = Vec::new();
    let mut stream = dht.get_peers(ih);
    while let Some(batch) = stream.next().await {
        out.extend(batch);                              // first batch lands in ~113–165 ms
    }
    out.sort();
    out.dedup();
    out
}

async fn announce_loop(dht: AsyncDht, ih: Id, external_quic_port: u16) {
    loop {
        if let Err(e) = dht.announce_peer(ih, Some(external_quic_port)).await {
            tracing::warn!(%e, "DHT announce failed; will retry");
        }
        tokio::time::sleep(REANNOUNCE_INTERVAL).await;
    }
}
```

---

## 9. Open items for later phases

* Measure a **real** two-network round trip (this machine + a VPS) once the libp2p
  layer exists; §3.4 proves the DHT record, not NAT traversal.
* Decide the actual `LOBBY_INFOHASH` value and how it is derived (a hash of a
  version string is the obvious choice, and makes protocol-version separation free).
* Decide whether an epoch-rotating infohash (§7) is worth the compatibility cost.
* Re-check `magpie-bt-dht` and `n0-mainline` in ~6 months; both could overtake.
* Add a regression test for the `extra_bootstrap` footgun (§4).
* **Re-measure the announce lifetime against the real `LOBBY_INFOHASH` once it
  exists.** §3.5 measured two *private random* infohashes with essentially no
  competing announces; a publicly known constant that many clients announce to lives
  in a busier part of the keyspace and will be evicted faster. If the measured
  lifetime there is materially under 45 min, shorten the 10-minute re-announce
  interval accordingly.
* File the `announce_peer_detailed` / `pub use *RequestArguments` request upstream at
  `github.com/pubky/mainline` (§6a), and carry a local patch if the GUI's DHT health
  indicator needs `stored_at` before it lands.
