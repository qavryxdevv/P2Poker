//! The running node: the async loop that makes D-003 true.
//!
//! Everything else in `net` is a decision made in isolation and tested in
//! isolation. This is the part that has to be run to be believed, so it is kept
//! small and its pieces live elsewhere: the admission rules are
//! [`lobby`](super::lobby)'s, the per-node state is [`node`](super::node)'s, and
//! the relay budget is [`relay`]'s. What is here is the order
//! things happen in.
//!
//! # Discovery is libp2p's, and used to be BitTorrent's
//!
//! Until 2026-08-30 this node announced itself in the Mainline DHT under a fixed
//! infohash and dialled whoever else was there. It worked, and it could not be
//! made to work for the people who need it most: **a Mainline announcement can
//! say one thing, an IP and a port**, and a player behind a NAT does not have
//! one worth saying. They would appear in the lobby and be undialable, which is
//! the same as not appearing.
//!
//! What a NATed player has is a circuit address through a relay — a multiaddr,
//! several of them, changing as reservations come and go. A Kademlia provider
//! record carries exactly that, so the reachable player and the unreachable one
//! are found by the same mechanism.
//!
//! # The order, and why it is this one
//!
//! 1. **Reach the public network.** Nothing else can happen first: AutoNAT has
//!    nobody to ask whether this client is reachable, and a reservation cannot
//!    be requested of a relay this node has never met.
//! 2. **Find a relay and take a reservation.** Relay hosts advertise themselves
//!    under `/libp2p/relay`, which is where go-libp2p's own AutoRelay looks.
//!    That is the list; it is not a file and nothing about it is compiled in.
//! 3. **Announce, once there is an address to announce.** A provider record
//!    carries the swarm's external addresses and go-libp2p-kad-dht discards one
//!    that arrives with none, so a client with no circuit yet would be
//!    announcing itself to nobody.
//! 4. **Ask who else is there, every cycle**, and dial them.
//! 5. **Gossip.** A peer that completed a handshake joins the mesh, and the
//!    table advertisements arrive there. The DHT is the phone book; the mesh is
//!    what actually carries a lobby.
//!
//! Measured end to end on 2026-08-30, with multicast off on both sides and no
//! BitTorrent DHT in the binary at all: two clients behind one NAT, each holding
//! a reservation on a **different** public relay, found each other and agreed a
//! table — `TABLE FORMED session=f503d44809bfb094 seats=2` on both.
//!
//! # What failure looks like, and why most of it is not failure
//!
//! Most dials fail, addresses go stale, and a client behind a symmetric NAT
//! cannot be reached directly at all. None of that is an error condition:
//! D-003 is satisfied by the dials that succeed, and the rest is the ordinary
//! weather of an open network.

use std::time::Duration;

use libp2p::{
    futures::StreamExt as SwarmStreamExt, gossipsub, identity, ping, request_response,
    swarm::SwarmEvent, Multiaddr, PeerId,
};
use tokio::sync::mpsc;


use super::advert;
use super::formation::{Failed, Formation, Send};
use super::joinrpc;
use super::joinwire;
use super::portmap;
use super::relay;
use super::lobby::{LobbyStore, RateLimiter, TableAd, TableKind};
use super::node::{worth_parsing, Events, NodeCommand, NodeEvent, NodeState, REBROADCAST};
use crate::table::hand::Holding;
use super::swarm::{CONNECTION_CEILING, MAX_CONNECTIONS, MIN_CONNECTIONS};
use super::swarm::{self, NodeConfig, PokerBehaviourEvent, RelayRole, Topics};
use super::joinwire::DISPLAY_NAME_MAX;
use crate::protocol::constants::SNAPSHOT_MAX_ADS;
use crate::protocol::constants::{
    RETURN_GRACE_MS,
    AD_TTL_MS, HAND_DEADLINE_CAP_MS, LOBBY_MSG_MAX, MAX_SEATS,
};

/// The way on to the public libp2p network, when no player is reachable.
///
/// **This is a dependency on somebody else's machines, and it is deliberate.**
/// `NAT_AND_DISCOVERY.md` §3.3 rejects a hardcoded list of relays, and it is
/// right to: a relay sees who plays with whom. But D-004's layers 2 and 3 are
/// built on public relays, and without a way to reach one this client had none
/// at all — the status line read `relay: none` and it was the truth. Most
/// people are behind a NAT that blocks unsolicited traffic. A client they
/// cannot use is not a more decentralised client, it is an unused one.
///
/// Three things keep it honest:
///
/// * **A relay is a fallback, never the game.** DCUtR upgrades the connection
///   to a direct one as soon as it can, and a public relay's two minutes and
///   128 KiB are sized for exactly that: carry the introduction, then get out
///   of the way. A hand is played peer to peer or over a *volunteer's* relay
///   (D-002), never over these.
/// * **A name, not addresses.** One `/dnsaddr/` resolves through TXT records to
///   whatever the operators currently run — measured 2026-08-29, four nodes in
///   Amsterdam, New York, Singapore and Silicon Valley. Nothing is compiled in
///   that can go stale, and any node found through them that speaks the hop
///   protocol will do.
/// * **Volunteers come first.** This is consulted only after the DHT's own
///   relay swarm has been asked and answered with nobody.
const PUBLIC_ENTRY: &[&str] = &["/dnsaddr/bootstrap.libp2p.io"];

/// How many players from the lobby to reach for **per DHT answer**.
///
/// The connection budget is finite and a lobby key outlives the clients in it,
/// so dialling every provider at once fills the budget with the dead and leaves
/// none for the living. Whoever is not reached in one answer is reached in the
/// next.
///
/// **The old name said `CYCLE` and that was a lie about the code.** The counter
/// is declared inside the `FoundProviders` arm, so it resets on **every
/// response**, and one `get_providers` query draws one response per node that
/// answers — 707 of them in a measured 420-second run, not 7. This comment and
/// eight places in `NETWORK_STACK.md` all claimed a per-cycle budget the code
/// has never implemented, which made the crawl read sixty times slower than it
/// is. Renamed rather than re-explained: a name that needs a paragraph of
/// correction will mislead the next reader too.
///
/// `DECISIONS.md` rows written before 2026-09-02 call it `DIALS_PER_CYCLE`.
/// Those are dated records of what was believed then and are left alone.
const DIALS_PER_ANSWER: usize = 8;

/// How long before a lobby provider that did not answer a dial is tried again.
///
/// **The number this replaces was infinity, and that cost a table.** A peer was
/// recorded as dialled *before* the dial was issued and the outcome was never
/// read, so one failed attempt refused it for the life of the process — and the
/// first dial between two clients behind routers routinely fails, before either
/// has a relay reservation or a hole punched.
///
/// One minute matches `discover_timer`, so a provider is tried at most once per
/// discovery cycle however many answers name it. That keeps what the original
/// set was for — a record for a client that is long gone costs one dial a
/// minute, not one per answer — and gives a live peer that was unreachable at
/// 22.8 s a chance at 82.8 s rather than at never.
const REDIAL_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

/// How long before an unconfirmed lobby announcement is walked again.
///
/// Only while unconfirmed: once this client has seen its own record come back
/// from the network, `libp2p-kad` owns the republish at its own twelve-hour
/// interval and this stops. Five minutes is long enough that a slow walk is not
/// duplicated and short enough that a client whose first walk reached nobody is
/// not invisible for its whole session — which is what a joiner taking 386
/// seconds to find a founder looks like from the other side.
const REANNOUNCE_EVERY: std::time::Duration = std::time::Duration::from_secs(300);

/// How often a frozen peer looks again at whether it may deal.
///
/// A freeze is `§6.3`'s answer to two peers disagreeing at a boundary
/// checkpoint, and it ends when the round resolves to one value. Nothing tells
/// this loop that it ended, so the timer that would have opened the next hand
/// is pushed forward by this much instead of being thrown away, and the first
/// tick after the freeze lifts deals. Five seconds because that is already the
/// showdown pause, so a resumed table behaves like one that never stopped.
const FROZEN_RECHECK: std::time::Duration = std::time::Duration::from_secs(5);

/// How many providers already tried may be tried **again** in one answer.
///
/// **A separate count, because a first dial and a re-dial are not the same
/// purchase — and sharing one made the crawl worse.** `REDIAL_AFTER` entitles
/// every provider on the key to one attempt a minute, and a lobby of ghosts has
/// hundreds of them, so re-dials arrived at the front of answers and spent the
/// budget on peers this client had already failed to reach while peers it had
/// never tried at all waited.
///
/// Measured by replaying the committed loop over `split231719-2`'s own answer
/// stream: one shared count issues **637 dials and reaches 250 providers**,
/// where the loop before `REDIAL_AFTER` issued **304 and reached 304**. Two
/// counts keep the retry without paying for it out of discovery.
const REDIALS_PER_ANSWER: usize = 8;

/// What a provider waits after a dial **the far end failed**, rather than the
/// whole discovery cycle.
///
/// `REDIAL_AFTER` is right for a record that may simply be dead and wrong for
/// the first minute of a run, where a live peer's dial fails for two reasons
/// that both mend themselves in seconds: the DHT still serves a previous run's
/// circuit addresses while the founder's own announcement walk is still going,
/// and a circuit hop collides with a relay dial this client already has in
/// flight. Ten seconds is longer than either takes to clear and short enough to
/// fit six times into the minute the joiner would otherwise spend holding a
/// provider it could not act on.
///
/// Measured, `split122959-9`: every seat had the founder in a lobby answer
/// inside thirteen seconds, four of them took forty-five to sixty more to
/// connect to it, and the founder was offered again **20, 24, 27 and 31 times**
/// in that gap with the cooldown refusing every one.
const FAST_REDIAL_AFTER: std::time::Duration = std::time::Duration::from_secs(10);

/// How many fast second looks this process grants in its whole life.
///
/// **The bound is on the process and not on the provider, and that is the
/// entire safety argument.** A per-provider bound is inflatable: provider
/// records are unauthenticated and peer ids are free, so a peer that fills the
/// lobby with fakes earns a fresh allowance for each one and pins this client
/// at its redial ceiling for ever. A per-process bound cannot be renewed at
/// all: a flooder can **spend** it, once.
///
/// **Spending it is not free, and an earlier draft of this comment said it
/// was.** A unit is consumed precisely to buy a dial the loop would otherwise
/// have refused inside the minute, so consuming the budget *is* creating
/// traffic — at most one extra dial per unit, a measured median of 0.51.
/// Replaying the loop over the 642 usable logs on disk, the opening 120
/// seconds carry a median **1.67x** the dials they carry today, p90 2.47x, max
/// 3.90x, and the budget drains fully in 334 of 643. What makes that safe is
/// not that it is nothing but that it is **additive and once**: the worst a
/// flooder buys is the fifth more dials priced below, spent inside two minutes
/// and never renewed.
///
/// **A ceiling and not a rate, and the difference is the whole point.** Simply
/// shortening the cooldown for everyone is not an option: `REDIALS_PER_ANSWER`
/// is 8 and a 420-second run draws **707 answers**, so a lane every stale
/// record could hold would issue up to 5 656 re-dials where the committed loop
/// issues about 300. The budget is what keeps the worst case additive.
///
/// A hundred and twenty-eight against roughly 600 dials in a run is a fifth
/// more in the worst case, and the worst case is the only case it can produce.
///
/// **And it is the number the corpus asks for, which is the half of this that
/// was guessed first.** Replaying every nine- and ten-seat log on disk: the
/// founder's position in the order its providers are first offered is **median
/// 15, p90 99, max 192**, and it is beyond 128 in **24 of 521** joiner logs. So
/// a budget spent oldest-first reaches the founder in 95% of them, and in the
/// other 5% the lane is simply shut and the client behaves as it does today.
/// The lobbies themselves are much larger than that — median 74 providers
/// offered inside the opening two minutes, p90 288, max 454 — which is why the
/// budget is spent on rank rather than on size.
const FAST_REDIAL_BUDGET: u32 = 128;

/// How long after this process started the fast lane is open at all.
///
/// **The fault is the opening minute of a run and the budget should be spent
/// there.** `split122959-9` found the founder in a lobby answer at 1.9, 2.7,
/// 11.9 and 12.7 seconds on the four seats that then waited a full cycle for
/// it — so the founder's failed dial is among the earliest this client makes,
/// and a budget spent oldest-first reaches it. After this the lane is shut and
/// the crawl is exactly what it is today: a stale record that has been dead for
/// five minutes has earned no favours, and the client has by then either found
/// its table or has a different problem.
const FAST_REDIAL_WINDOW: std::time::Duration = std::time::Duration::from_secs(120);

/// The namespace relay hosts advertise themselves under, as a DHT key.
///
/// Derived, not copied: go-libp2p's routing discovery turns a namespace string
/// into `CIDv1(raw, sha2-256(ns))` and go-libp2p-kad-dht keys the provider
/// record on that CID's **multihash**, so the key is `0x12 0x20` — sha2-256,
/// thirty-two bytes — followed by the digest. Computing it here rather than
/// pasting thirty-four bytes means the derivation is visible and the test can
/// check it against the published value.
fn relay_namespace() -> libp2p::kad::RecordKey {
    namespace(b"/libp2p/relay")
}

/// The lobby, as a rendezvous on the public DHT.
///
/// Every client announces itself a provider of this one key and asks who else
/// is. That is the whole lobby: a place all clients agree on, where each finds
/// the others' addresses, after which table advertisements travel over gossip
/// exactly as they did before.
///
/// **Why this replaces announcing in Mainline.** A BitTorrent announcement can
/// say one thing — `IP:port` — and a player behind a NAT does not have one
/// worth saying. What they have is a circuit address through a relay, which is
/// a multiaddr and does not fit in four bytes and a port. A provider record
/// carries whatever addresses the node has, so the same mechanism serves the
/// reachable player and the unreachable one, and that is the difference between
/// a lobby most people can be seen in and a lobby only a minority can.
fn lobby_namespace() -> libp2p::kad::RecordKey {
    namespace(b"p2p-poker/main-lobby/v1")
}

/// A namespace string as the DHT key its provider records live under.
///
/// go-libp2p's routing discovery maps a namespace to `CIDv1(raw, sha2-256(ns))`
/// and go-libp2p-kad-dht keys the provider record on that CID's multihash, so
/// the key is `0x12 0x20` — sha2-256, thirty-two bytes — and then the digest.
/// Derived here rather than pasted, so the derivation is visible and testable.
fn namespace(ns: &[u8]) -> libp2p::kad::RecordKey {
    use sha2::{Digest, Sha256};
    let mut key = Vec::with_capacity(34);
    key.push(0x12);
    key.push(0x20);
    key.extend_from_slice(&Sha256::digest(ns));
    libp2p::kad::RecordKey::new(&key)
}

/// Where this node listens.
///
/// Port 0 by default, so the OS chooses and nothing collides with another
/// instance. QUIC is the transport peers are found on; TCP is there for the
/// networks that will not carry UDP at all.
///
/// **A fixed port is what makes a player reachable.** Everything else in this
/// module is about coping with not being reachable — relays, circuits, hole
/// punching — and all of it needs somebody, somewhere, who is. A player who
/// forwards one port on their router becomes that somebody: publicly dialable,
/// confirmed by AutoNAT, and then a relay for everyone else under D-002. With
/// an ephemeral port they cannot, because the number changes every start and no
/// forwarding rule can name it.
///
/// The same number is used for UDP and TCP. They are different sockets and do
/// not collide, and one number is one line in a router's configuration instead
/// of two.
///
/// # IPv6, and why it was missing
///
/// **This client bound `/ip4/0.0.0.0` and nothing else, on both transports, for
/// its whole life.** `NETWORK_STACK.md` §5.8 said discovery was IPv4-only and
/// gave the reason: the `mainline` crate's socket did
/// `unimplemented!("KrpcSocket does not support Ipv6")`. That crate was deleted
/// in `c7e6317` and the reason went with it — libp2p's QUIC and TCP transports
/// are both dual-stack — but the four IPv4 literals stayed, so the conclusion
/// survived its own premise. A player on an IPv6-only network could not be
/// reached by anybody, and neither could a player whose ISP gives out CGNAT on
/// v4 and a routable address on v6, which is the common case this is worth
/// most to.
///
/// **An IPv6 listener that fails is not an error.** A machine with no IPv6 at
/// all is ordinary — the machine this was written on is one — so the `/ip6/`
/// addresses are attempted and reported, never fatal. That is why this returns
/// two lists rather than one: the caller must be able to tell "could not bind,
/// carry on" from "could not bind, stop".
fn listen_addrs(port: u16) -> Vec<Multiaddr> {
    vec![
        format!("/ip4/0.0.0.0/udp/{port}/quic-v1")
            .parse()
            .expect("a literal"),
        format!("/ip4/0.0.0.0/tcp/{port}")
            .parse()
            .expect("a literal"),
    ]
}

/// The same two transports on IPv6, on the same port number.
///
/// `::` is the v6 wildcard. Whether it also accepts v4 traffic depends on the
/// host's `IPV6_V6ONLY` default and is not relied on here: the v4 listeners of
/// [`listen_addrs`] are bound in their own right, so a dual-stack socket would
/// merely be a duplicate and a v6-only socket loses nothing.
fn listen_addrs_v6(port: u16) -> Vec<Multiaddr> {
    vec![
        format!("/ip6/::/udp/{port}/quic-v1")
            .parse()
            .expect("a literal"),
        format!("/ip6/::/tcp/{port}")
            .parse()
            .expect("a literal"),
    ]
}

/// The QUIC port out of a listen address, if it has one.
///
/// This is what gets announced to Mainline, so it is read from what the swarm
/// actually bound rather than from what was asked for.
pub fn quic_port(addr: &Multiaddr) -> Option<u16> {
    use libp2p::multiaddr::Protocol;
    let mut udp = None;
    let mut is_quic = false;
    for p in addr.iter() {
        match p {
            Protocol::Udp(port) => udp = Some(port),
            Protocol::QuicV1 => is_quic = true,
            _ => {}
        }
    }
    if is_quic {
        udp
    } else {
        None
    }
}

/// Run a node until the caller drops the receiver.
///
/// Events go out on the channel rather than to a logger, so the GUI and a
/// headless run see the same stream.
/// A table this node is offering, if it is offering one.
///
/// The signing key **is** the table's identity, so it is held here and nowhere
/// else: a table whose key lived in two places would be a table two peers could
/// be handed two versions of.
pub struct Hosted {
    pub ad: TableAd,
    pub key: ed25519_dalek::SigningKey,
}

/// What one node needs to start, in one value.
///
/// These were positional arguments until `--autoplay` made an eighth. A caller
/// reads the field names; the alternative is eight positions of which two are a
/// `bool` and a `u16` next to each other.
pub struct Run {
    pub identity: identity::Keypair,
    pub app_key: ed25519_dalek::SigningKey,
    pub events: mpsc::Sender<NodeEvent>,
    pub commands: mpsc::Receiver<NodeCommand>,
    pub local_discovery: bool,
    pub port: u16,
    pub profile_dir: std::path::PathBuf,
    /// Seat this node plays itself, deciding after this long. `None` is a human.
    pub autoplay: Option<std::time::Duration>,
    /// `S1-BM`: never ask to be dealt back in. The client asks at every
    /// boundary where its seat is outside the roster with chips, on the
    /// player's behalf; this withholds the request, which is how a seat
    /// watches a table it has chips at without being dealt in, and how a
    /// measurement run holds a seat out on purpose.
    pub stay_out: bool,
}

/// **One table this client sits at, as the node loop runs it** (`D-043`,
/// stage 1). Every field was a local of the loop until 2026-09-12; the
/// comments are the ones those locals carried. The loop reads and writes
/// them as `t.field`, and its macros take the table as their first
/// argument -- a macro's bare names resolve where the macro is defined,
/// not where it is used. One table still; the driver underneath already
/// carries several (`D-042`), and the node's turn is the next stage.
struct TableRun {
    /// `D-043`: this slot's number, stable while the client sits here; the
    /// window's name for the table's state (`NodeEvent::AtTable`).
    slot: u8,
    /// When the slot was opened: one that never gets its table goes a minute
    /// later, once the window has turned elsewhere.
    opened_at: tokio::time::Instant,
    /// `S1-DV`: when each seat of the roster was first seen there by this
    /// client -- the grace a seat gets to join the table's group before its
    /// line being down reads as gone (`GROUP_JOIN_GRACE`).
    seat_since: std::collections::HashMap<u8, tokio::time::Instant>,
    /// The table this client is forming or sitting at, and the mesh it is formed
    /// on. One `TableRun` per table; the loop holds one of them until `D-043`'s
    /// stage 2 holds up to `MAX_TABLES`. (Until 2026-09-12 multi-tabling was
    /// meant to be more than one client, which §4.3's per-roster `peer_id`
    /// rule allows; the owner wants it in one.)
    table: Option<Formation>,
    /// The hand in progress, if any.
    ///
    /// Started once, at the moment the roster settles. `TableReal` is emitted
    /// from more than one place and fires again on later table traffic, so the
    /// start has to be idempotent, or two `HAND_INIT`s would go out under one
    /// seat and read as equivocation to everybody else.
    hand: Option<crate::table::hand::Hand>,
    hand_reported: bool,
    /// Said once while this client has reached nobody, and again if it is ever
    /// alone a second time.
    alone_said: bool,
    /// The last deck state told to the interface, so that `2m` chain stages do
    /// not become `2m` identical lines in a player's log.
    deck_reported: Option<(Option<u8>, bool)>,
    /// Whether this hand's own cards have been handed to the interface.
    cards_reported: bool,
    /// **The abort is a standing property, not an event.** `aborted()` stays
    /// `Some` for the rest of the hand, and this block had no once-flag while
    /// `deck_reported`, `cards_reported` and `turn_reported` all do -- so every
    /// further event accepted before `next_hand_at` replaced the hand reprinted
    /// the sentence. Measured in `split163641-10`: `far-n3` printed the void
    /// four times, `far-n1` four, `far-n0` three. A reader counting seats from
    /// a grep over-counts them, which is a bad property in the one message that
    /// says a hand died.
    abort_reported: bool,
    /// Hand events dropped for want of a draining reader, said when it moves.
    inbox_dropped_said: u64,
    /// `S1-DW`: the claims the table's group had refused when this client
    /// last said so.
    claims_refused_said: u64,
    /// `D-045`: the members removed by the table's word, and the times this
    /// client was itself removed, when last said.
    removed_said: u64,
    kicked_out_said: u64,
    /// `D-045`: the running hand's certified-out seats as last seen, by hand
    /// id -- a seat new to them is the table's word about it.
    required_seen: (u64, Vec<u8>),
    /// The last turn told to the interface, so a stage per action does not
    /// become a redraw per action.
    turn_reported: Option<HandReport>,
    /// When the next hand may start. D-020's hold, and the only timer in this
    /// loop that is about a person rather than about the network.
    next_hand_at: Option<tokio::time::Instant>,
    /// `S1-BM`: the boundary tick is two-phase. The timer fires at the
    /// terminal -- phase 1: the window, the checkpoint, this seat's sit-in
    /// request -- and the next hand is dealt from `deal_at` on, D-020's pause
    /// later, so a returning seat's request and checkpoint are on the wire for
    /// the whole pause instead of going out in the tick that deals over them
    /// (`run164337-3`: nine requests, no return).
    deal_at: Option<tokio::time::Instant>,
    /// The hold for a return in flight: the hand it is about, when it began,
    /// and whether it has been given up on.
    return_hold: Option<(u64, tokio::time::Instant, bool)>,
    /// The hand whose boundary phase 1 -- window, checkpoint, sit-in request --
    /// has run. Once per boundary: the arm fires at the terminal and again at
    /// `deal_at`, and on every re-arm the repair, the freeze and the boundary
    /// wait make, while `state_hash_event` seals afresh each call with a new
    /// timestamp and so a new event hash. Every receiver in `run170442-3`
    /// logged *put two different events in one checkpoint stage* at every
    /// boundary -- an equivocation by an honest peer, which D-009 rule 1
    /// forbids -- and `boundaries.open` re-inserted the stage and dropped the
    /// copies it had already heard. Keyed by hand id, so a re-opened hand
    /// (`S1-BS`) does not run its predecessor's boundary again either.
    boundary_done_for: Option<u64>,
    /// When this client stops waiting for its own player and acts for them.
    ///
    /// Version 1's whole answer to a stalled betting stage (D-015): there is no
    /// `TIMEOUT_VOTE` and no `TIMEOUT_CERT`, so a seat that goes quiet is
    /// answered by **its own client** folding for it and by nothing else. A
    /// seat that goes quiet in a *cryptographic* stage has no answer here at
    /// all, and that is `hand_deadline_ms`, which is not built.
    act_by: Option<tokio::time::Instant>,
    /// This client's own contribution to the stage now open, and the sequence
    /// it belongs to.
    ///
    /// GossipSub has no history: a peer that joins the topic mesh after a
    /// publish never sees it, and nothing re-sends. At two seats that rarely
    /// bites, because both are meshed before either speaks. At three it is the
    /// ordinary case - the last joiner missed the first two seats' `HAND_INIT`
    /// and sat at "waiting for seats 0, 1" for ever, with no error anywhere and
    /// every other line of its log identical to a healthy one.
    ///
    /// So this client re-sends what it already said, until the stage it said it
    /// in has moved. **The stored bytes, never a re-signature**: re-signing
    /// would change `emitted_at_unix_ms` and produce a second distinct body at
    /// one slot, which is an equivocation proof against an honest peer
    /// (`PROTOCOL.md` §5.2).
    /// **Every** message this client has emitted for the hand in progress, not
    /// just the newest. A peer that missed stage 0 is not helped by stage 1: it
    /// is stuck at 0 and holding everything after it, and only the stage-0 bytes
    /// release it. Capped, because it is a buffer and every buffer here is.
    said: Vec<Vec<u8>>,
    /// Whether this table has ever dealt a hand.
    ///
    /// The two roads into the first hand fire again on **every later table
    /// message**, and they are idempotent only because `hand.is_some()` guards
    /// them. Between hands, and after the last one, `hand` is `None` and that
    /// guard is open — so without this the next table message would deal hand
    /// one again, on a table that has already played it, with stacks from the
    /// roster rather than from the chain.
    ever_dealt: bool,
    /// **Hand 1 waits for the Tox group to hold every seat**, bounded by
    /// `GROUP_WAIT_MS`, after which it deals anyway — which is what this client
    /// did before the gate existed. See `hand_one_may_open`.
    /// **The boundary checkpoint's retained values, and the readmission set they
    /// write** (§4.9). `boundaries` holds each retained hand's checkpoint — its
    /// two stages and the comparison — and `readmitted` is `A`, read and cleared
    /// at the next hand init and nowhere else.
    ///
    /// Held here rather than in `Hand`, because both outlive the hand they are
    /// about: §4.9 admits a checkpoint-8 `STATE_HASH` of hand `k` until
    /// `TERMINAL(k+1)` is fixed, and by then hand `k+1` is live.
    boundaries: crate::table::boundary::Boundaries,
    /// Which hand's `HAND_INIT` stage has already moved the previous hand's
    /// checkpoint down. T47 fires once per hand and `dealt()` stays true after
    /// it, so without this the transition would run on every later event.
    crossed_for: Option<u64>,
    /// Whether this node has said that its checkpoints agree. Said once; see the
    /// `Took::Agreed` arm of `checkpoint_event`.
    checkpoint_said: bool,
    /// **§6.3 step 1's freeze, and it is a latch.** `Some((k, sequence))` while
    /// this peer has observed two distinct `state_hash` values at one checkpoint.
    /// While it is set this client accepts no hand event and emits none: no card
    /// opens, no action is applied, no chips move. §6.3: *"silently continuing is
    /// what §15 forbids"*.
    ///
    /// **Released by exactly one thing** — a reconciliation stage that completes
    /// over `R(c) ∪ W` with every value in it agreeing. Not a timer; not the
    /// abort of the hand it froze in; not `TERMINAL(k)`; not a later hand's
    /// checkpoint; not the table closing. The corpus lists those exclusions
    /// because the previous revision left the release to be inferred and the
    /// inference was wrong.
    frozen: Option<(u64, u64)>,
    /// Every occupied seat, which is the accepted set of a checkpoint stage and
    /// of a reconciliation round. Refreshed where a boundary is opened.
    roster_seats: Vec<u8>,
    /// Said once: why the table has stopped. Cleared when the freeze is.
    frozen_said: bool,
    /// When a vote was last reported as owed and not cast (`vote_state`).
    vote_state_said: u64,
    /// The hand whose "a certificate about the previous hand arrived too late
    /// to be kept" line has been said.
    late_cert_said: Option<u64>,
    /// `S1-CN`: the hand during which "a settlement of the previous hand arrived
    /// after the late path closed" has been said.
    late_settle_said: Option<u64>,
    /// `S1-CN`: the hand this client last ended by an abort that no late
    /// settlement closed, kept past the retained hand's own lifetime so a
    /// settlement of it arriving after retention ended is still counted as one
    /// the late path missed.
    unsettled_abort_here: Option<u64>,
    /// `S1-BS`: hand k, retained until hand k+1 leaves stage 0, so a
    /// certificate about it arriving late still banks its roster half and
    /// re-derives hand k+1.
    previous: Option<crate::table::hand::Hand>,
    /// `HAND_INIT`s of the next hand that arrived during this one: drained into
    /// the next hand's held queue when it opens, and read for whether its
    /// parent is contested before this client signs.
    next_inits: Vec<(u8, Vec<u8>)>,
    /// Everything else of the next hand that arrives before this client opens
    /// it (`S1-BW`), with the seat that signed it so no one seat can fill it.
    /// Kept apart from the `HAND_INIT`s because those decide the genesis and
    /// this must not reach that decision.
    next_early: Vec<(u8, Vec<u8>)>,
    /// What the pre-open buffer **lost**, as `(refused, evicted)`.
    ///
    /// **The open line reported what survived and never what was thrown away**,
    /// which is why it took two runs to see one bug: a frame refused at a full
    /// buffer and a frame that never arrived produced the same log. Cleared at
    /// each boundary with the buffers themselves.
    next_early_lost: (u32, u32),
    /// A re-derived opening for the running hand, applied by the stall tick
    /// outside the hand borrow.
    pending_repair: Option<crate::table::hand::Opening>,
    /// The hand whose boundary wait has been said.
    genesis_wait_said: Option<u64>,
    /// The hand a certificate banked late for, so a redelivery of that
    /// certificate after the hand was dropped is not reported as a repair
    /// missed.
    late_banked_for: Option<u64>,
    delayed_certs: Vec<(tokio::time::Instant, Vec<u8>)>,
    releasing_certs: bool,
    delay_said: bool,
    /// Said once: why no reconciliation round could be opened. Its causes are
    /// permanent ones only — a stage that has not closed yet is retried in
    /// silence, because it is the ordinary case and not a fault.
    no_round_said: bool,
    /// **Which roster seats have signed an event of a hand this client has not
    /// reached, and the furthest each named.** Evidence that the table is
    /// somewhere this client is not.
    ahead: std::collections::HashMap<u8, u64>,
    /// Set once that evidence is conclusive: `(the table's hand, this client's)`.
    /// A latch, and terminal — see `note_a_hand_ahead`.
    adrift: Option<(u64, u64)>,
    /// Said once: this client is on a TCP relay and the group is not filling.
    udp_warned: bool,
    /// **A checkpoint copy that arrived before this client opened its own
    /// boundary, held rather than dropped.**
    ///
    /// Peers finish a hand milliseconds apart and open their boundaries in that
    /// order, so a faster seat's `STATE_HASH` routinely reaches a slower one
    /// before there is a boundary to put it in. `checkpoint_event` answered that
    /// with `return None` and the copy was gone — and the sender does not repeat
    /// it on demand, because nothing tells it to. The slower seat then waits for
    /// a value that was delivered and discarded, which reads from every side as
    /// a message that never arrived.
    ///
    /// Bounded on both axes: two hands, and one copy per seat per hand. A hold
    /// queue that grows is a way to be attacked, and a copy older than two hands
    /// belongs to a boundary the store has already released anyway.
    early_checkpoints: std::collections::HashMap<u64, Vec<Vec<u8>>>,
    /// `S1-BM`: the same for section 4.10's boundary events. A sit-in request
    /// from a seat that ended hand k before this client did arrives before this
    /// client's window for k exists, and `boundary_event` used to consume it
    /// with no line (`split172616-9`: seat 3 asked, none of eight voters took
    /// it). Held under the same two bounds, admitted at phase 1.
    early_boundary: std::collections::HashMap<u64, Vec<Vec<u8>>>,
    /// How many §6.3 disputes this client has verified. Reported with the freeze,
    /// because "none arrived" and "several arrived and agreed with me" are
    /// different states and were the same silence.
    disputes_seen: usize,
    /// Group keys this client has already paired with an application key. One
    /// pairing per peer per run; see `signer_of`.
    taught: std::collections::HashSet<[u8; 32]>,
    /// `D-047`: the table's word about each seat out for good -- seat, key,
    /// the hand, the certificate -- carried in this client's lobby answers so
    /// the seat's own client is told.
    out_words: Vec<(u8, [u8; 32], u64, Vec<u8>)>,
    /// `D-047`: the keys out for good; a request from one is refused.
    out_keys: std::collections::BTreeSet<[u8; 32]>,
    /// `D-047`: this client's own seat has been told it is out.
    out_told: bool,
    readmitted: Vec<u8>,
    /// `S1-BM`: the evidence a return vote is cast on, by boundary and by
    /// subject. Written by `boundary_event` (the subject's `PLAYER_SIT_IN`)
    /// and `checkpoint_event` (its agreeing checkpoint-8 `STATE_HASH`), read
    /// by `vote_on_returns!` at the stall tick, released at the hand-over.
    sit_ins: SitIns,
    /// `S1-CR`: the unfinished session on record, if any, said once at start
    /// so the window can ask and a headless client can answer. `resuming` is
    /// set by `ResumeSession` and cleared when this seat is dealt in again,
    /// when the session is forgotten, or when the rejoin gives up.
    resume: Option<crate::storage::session::Record>,
    resuming: bool,
    resume_since_ms: u64,
    resume_last_peer_ms: Option<u64>,
    /// The members' `HAND_INIT` copies of the running table's hands, and the
    /// rest of that traffic, kept while this client holds no hand of it:
    /// `Opening::adopt` reads the first, the adopted hand replays the second.
    resume_inits: std::collections::BTreeMap<u64, Vec<Vec<u8>>>,
    resume_early: Vec<(u64, Vec<u8>)>,
    resume_said: Option<u64>,
    /// What this client joined by, for the record: the table key of the
    /// advert, the advert itself and its hash. `None` for a founder, whose
    /// table does not survive its own restart and gets no record.
    joined_key: Option<[u8; 32]>,
    joined_ad: Option<super::lobby::TableAd>,
    joined_advert_hash: Option<[u8; 32]>,
    /// `S1-CR`: the session the record on disk names, so the record is written
    /// when the table is set and again whenever the session changes under it
    /// (a roster change before the first deal settles the table again).
    recorded_session: Option<[u8; 32]>,
    recorded_refused_said: bool,
    /// `S1-CR`: when this client last said its ratification again over the
    /// group -- in answer to a copy after the table was set, or asking for
    /// the others' while it holds a roster and no session.
    ratification_echo_ms: u64,
    ratification_asked_ms: u64,
    ratification_asked_said: bool,
    /// `S1-CS`: one line per two seconds per seat of table chat.
    chat_limits: super::tabletalk::SeatLimiter,
    /// `S1-CS`: the group count the window was last told, so the felt can
    /// say *the players are joining the group* as they do and not on the
    /// thirty-second status line.
    carrier_reported: Option<(u64, u64)>,
    /// `D-033`: the hand whose card material the session record holds.
    material_recorded: Option<u64>,
    /// `D-033`: when the running hand was last said again to a seat back from a restart.
    hand_said_again_ms: u64,
    /// `D-033`: the stage the running hand is at, and since when, for the re-say above.
    stage_waiting: (u64, u64),
    /// `S1-CX`: the other seat's next hand, seen while this one is still
    /// inside a hand nothing was dealt in -- the hand id and the parent it
    /// carries -- for the stall tick to give this hand up on, if the parent
    /// is what this hand's own give-up opens at.
    give_up_for: Option<(u64, [u8; 32])>,
    hand_one_held_since: Option<std::time::Instant>,
    /// How many seats the group held when it last grew, and when that was. The
    /// wait is on progress rather than on a deadline; see `hand_one_may_open`.
    /// **`None` until a table exists, because a clock started at process launch
    /// measures the wrong thing.** This used to be `(0, Instant::now())` at
    /// start-up, so `GROUP_STALL_MS` had already elapsed for any client that had
    /// been running two minutes before its table formed — which is the ordinary
    /// case, not an exotic one — and the stall arm dealt hand 1 the instant the
    /// roster ratified. Anchored on first use instead, exactly as `held_since`
    /// is.
    hand_one_progress: Option<(u64, std::time::Instant)>,
    hand_one_forced_said: bool,
    /// Said once per node: the reason hand one cannot be built from the roster
    /// this client holds. See `say_why_no_hand_one`.
    why_no_hand_one_said: bool,
    table_topic: Option<gossipsub::IdentTopic>,
    /// Empty unless this table's game traffic rides a Tox group (D-019), and
    /// empty for ever in a build without `--features tox`. The lobby, the join
    /// RPC and the ratification stay on the mesh either way; what moves is the
    /// hand. See `net::toxsink` for why the cfg lives there and not here.
    tox_sink: super::toxsink::TableSink,
    /// Said once, when the group is really joined. On the founder that is
    /// immediate; on a joiner it is after an invitation arrives **and** its chat
    /// id matches the advertisement's, which is the one moment worth reporting -
    /// before it, the hand has a transport that reaches nobody.
    tox_group_said: bool,
    tox_refused_said: u64,
    tox_invites_said: u64,
    /// **How the re-send loop backs off.** `at` is the chain position it last
    /// saw, and `ticks` counts five-second ticks since that position moved. A
    /// table that is advancing re-sends almost nothing; a stuck one still gets
    /// its first repeat after five seconds.
    /// How many times the table's topic has been said again while forming.
    /// Bounded, because the primitive is not free: a peer that has still not
    /// heard after this many tries has a problem re-announcing will not fix.
    table_announces: u32,
    resend_at: u64,
    resend_ticks: u32,
    /// `D-041`: the last link reading told to the window per seat -- the ping,
    /// the group's word, and when -- so the stall tick says it again only on a
    /// change or every ten seconds.
    link_said: std::collections::HashMap<u8, (bool, Option<u64>, tokio::time::Instant)>,
    /// Whether the table this client is at has closed to newcomers. Drives
    /// `dht_effort`, stops the lobby being polled by somebody who is not reading
    /// it, and stops the advertisement of a table nobody can join.
    table_closed: bool,
    /// A tournament that has once been full has started, and does not reopen.
    tournament_started: bool,
    /// `D-042`: when the tournament ended here, so the table's group can be
    /// left `TOURNAMENT_LEAVE_GRACE` after -- long enough for the resend loop
    /// to give a slow seat the terminal message again, and no longer.
    table_over_at: Option<std::time::Instant>,
}

impl TableRun {
    fn new(
        slot: u8,
        profile_dir: &std::path::Path,
        resume: Option<crate::storage::session::Record>,
        tox_sink: super::toxsink::TableSink,
    ) -> TableRun {
        let _ = profile_dir;
        TableRun {
            slot,
            opened_at: tokio::time::Instant::now(),
            seat_since: std::collections::HashMap::new(),
            table: None,
            hand: None,
            hand_reported: false,
            alone_said: false,
            deck_reported: None,
            cards_reported: false,
            abort_reported: false,
            inbox_dropped_said: 0,
            claims_refused_said: 0,
            removed_said: 0,
            kicked_out_said: 0,
            required_seen: (0, Vec::new()),
            turn_reported: None,
            next_hand_at: None,
            deal_at: None,
            return_hold: None,
            boundary_done_for: None,
            act_by: None,
            said: Vec::new(),
            ever_dealt: false,
            boundaries: crate::table::boundary::Boundaries::new(),
            crossed_for: None,
            checkpoint_said: false,
            frozen: None,
            roster_seats: Vec::new(),
            frozen_said: false,
            vote_state_said: 0,
            late_cert_said: None,
            late_settle_said: None,
            unsettled_abort_here: None,
            previous: None,
            next_inits: Vec::new(),
            next_early: Vec::new(),
            next_early_lost: (0, 0),
            pending_repair: None,
            genesis_wait_said: None,
            late_banked_for: None,
            delayed_certs: Vec::new(),
            releasing_certs: false,
            delay_said: false,
            no_round_said: false,
            ahead: std::collections::HashMap::new(),
            adrift: None,
            udp_warned: false,
            early_checkpoints: std::collections::HashMap::new(),
            early_boundary: std::collections::HashMap::new(),
            disputes_seen: 0usize,
            taught: std::collections::HashSet::new(),
            out_words: Vec::new(),
            out_keys: std::collections::BTreeSet::new(),
            out_told: false,
            readmitted: Vec::new(),
            sit_ins: SitIns::default(),
            resume,
            resuming: false,
            resume_since_ms: 0,
            resume_last_peer_ms: None,
            resume_inits: std::collections::BTreeMap::new(),
            resume_early: Vec::new(),
            resume_said: None,
            joined_key: None,
            joined_ad: None,
            joined_advert_hash: None,
            recorded_session: None,
            recorded_refused_said: false,
            ratification_echo_ms: 0,
            ratification_asked_ms: 0,
            ratification_asked_said: false,
            chat_limits: super::tabletalk::SeatLimiter::default(),
            carrier_reported: None,
            material_recorded: None,
            hand_said_again_ms: 0,
            stage_waiting: (u64::MAX, 0),
            give_up_for: None,
            hand_one_held_since: None,
            hand_one_progress: None,
            hand_one_forced_said: false,
            why_no_hand_one_said: false,
            table_topic: None,
            tox_sink,
            tox_group_said: false,
            tox_refused_said: 0,
            tox_invites_said: 0,
            table_announces: 0,
            resend_at: u64::MAX,
            resend_ticks: 0,
            link_said: std::collections::HashMap::new(),
            table_closed: false,
            tournament_started: false,
            table_over_at: None,
        }
    }
}

pub async fn run(cfg: Run) -> Result<(), Box<dyn std::error::Error>> {
    let Run {
        identity,
        app_key,
        events,
        mut commands,
        local_discovery,
        port,
        profile_dir,
        autoplay,
        stay_out,
    } = cfg;
    // Advisory events are offered, not waited for. See `Events`.
    let events = Events::new(events);
    let mut swarm = swarm::build(NodeConfig {
        identity,
        // Capacity from the start; the ANNOUNCE is what AutoNAT gates (D-002).
        // `relay::Config` cannot be changed after the swarm is built, and
        // capacity nobody can reach costs nothing.
        // A player on the same wire should be found in a second, not in a
        // minute. Turned off only to prove the DHT path carries a lobby on its
        // own, which multicast would otherwise answer first.
        local_discovery,
        relay_role: RelayRole::Volunteer,
    })?;

    let topics = Topics::default();
    swarm.behaviour_mut().gossipsub.subscribe(&topics.lobby)?;
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&topics.lobby_chat)?;

    for addr in listen_addrs(port) {
        swarm.listen_on(addr)?;
    }
    // IPv6 beside IPv4, and a failure here is reported rather than fatal: a host
    // with no IPv6 stack is ordinary and must still be able to play.
    let mut v6 = 0usize;
    for addr in listen_addrs_v6(port) {
        match swarm.listen_on(addr.clone()) {
            Ok(_) => v6 += 1,
            Err(e) => {
                let _ = events
                    .send(NodeEvent::Warning(format!(
                        "no IPv6 listener on {addr}: {e}. This client is reachable over IPv4 only, which is what every build before this one was"
                    )))
                    .await;
            }
        }
    }
    if v6 > 0 {
        let _ = events
            .send(NodeEvent::Warning(format!(
                "listening on IPv6 as well as IPv4 ({v6} of 2 transports)"
            )))
            .await;
    }

    // Reach for the public network at once, rather than after the first
    // housekeeping tick. Two things depend on having *any* peer at all and both
    // are stuck without one: AutoNAT cannot decide whether this client is
    // reachable until somebody agrees to dial it back, and a reservation cannot
    // be asked of a relay this node has never met. Sitting on a home connection
    // with no other player running, the client could reach neither conclusion —
    // which is exactly what `relay: none` was reporting.
    // What this client knew last time.
    //
    // **Beside the compiled entry point, never instead of it.** A saved book
    // goes stale — a peer that answered yesterday may be off today, and a first
    // run has no book at all — so the DNS name is still dialled every start.
    // What the book buys is not being *dependent* on it: a client that has run
    // before can reach the network through peers it met itself, and the day
    // those four machines are unreachable is the day that matters.
    let remembered = super::peerbook::load(&profile_dir);
    if !remembered.is_empty() {
        let kad = &mut swarm.behaviour_mut().ipfs_kad;
        for (peer, addrs) in &remembered {
            for a in addrs {
                kad.add_address(peer, a.clone());
            }
        }
        let _ = events
            .send(NodeEvent::Warning(format!(
                "{} peer(s) remembered from last time",
                remembered.len()
            )))
            .await;
    }

    for entry in PUBLIC_ENTRY {
        match entry.parse::<libp2p::Multiaddr>() {
            Ok(addr) => {
                if let Err(e) = swarm.dial(addr) {
                    let _ = events
                        .send(NodeEvent::Warning(format!("{entry}: {e}")))
                        .await;
                }
            }
            Err(e) => {
                let _ = events
                    .send(NodeEvent::Warning(format!("{entry} is not an address: {e}")))
                    .await;
            }
        }
    }

    let mut state = NodeState::new();
    let my_peer_bytes = swarm.local_peer_id().to_bytes();
    // This node's own application key, used as the "from peer" when it files its
    // own advertisement. It is charged the same rate limit as anybody else,
    // which at one re-broadcast every thirty seconds is half the allowance.
    let my_app_key = app_key.verifying_key().to_bytes();
    // Until the interface says otherwise, eight characters of this player's own
    // key: stable, theirs, and never confusable with somebody else's.
    let mut nickname = crate::storage::profile::short_name(&my_app_key);

    // `D-043`: every table this client sits at, up to `MAX_TABLES`; the first
    // slot always exists, and `active` is the one the window is at.
    let mut tables: Vec<TableRun> = vec![TableRun::new(
        0,
        &profile_dir,
        crate::storage::session::load_recent(&profile_dir, super::node::now_unix_ms()),
        super::toxsink::TableSink::none(),
    )];
    let mut active: usize = 0;
    // The next slot's number, and the last `AtTable` the window was told.
    let mut next_slot: u8 = 1;
    let mut marked: Option<(u8, Option<[u8; 32]>)> = None;
    // The join requests in flight, by the table each was sent for.
    let mut join_pending: std::collections::HashMap<libp2p::request_response::OutboundRequestId, usize> =
        std::collections::HashMap::new();
    let first = &mut tables[0];
    // Arm the boundary: fire now for phase 1, deal after the pause.
    macro_rules! arm_boundary {
        ($t:ident, $pause:expr) => {{
            let now = tokio::time::Instant::now();
            $t.next_hand_at = Some(now);
            $t.deal_at = Some(now + $pause);
        }};
    }

    /// Bring this client's own turn forward, for a measurement run.
    ///
    /// **This is the only thing `--autoplay` changes about the clock**, and it
    /// can only ever make the deadline earlier: `min` against what the protocol
    /// derived, never a replacement for it. A measurement mode that could
    /// extend a deadline would be measuring a different table.
    fn hurry(
        at: Option<tokio::time::Instant>,
        autoplay: Option<std::time::Duration>,
    ) -> Option<tokio::time::Instant> {
        match (at, autoplay) {
            (Some(at), Some(d)) => Some(at.min(tokio::time::Instant::now() + d)),
            (at, _) => at,
        }
    }
    // When this hand may be given up on. The only terminus a stalled
    // CRYPTOGRAPHIC stage has in version 1: nobody votes and nobody certifies,
    // so each peer runs its own timer and the abort is buffered at a receiver
    // until that receiver's own timer agrees.
    let mut stall = tokio::time::interval(std::time::Duration::from_secs(2));
    stall.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut resend = tokio::time::interval(std::time::Duration::from_secs(5));
    resend.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // **fault-harness only.** `P2P_POKER_DELAY_CERTS_MS=<ms>` parks every
    // `TIMEOUT_CERT` this seat receives for that long before it is judged —
    // the shape `S1-BS` measured (the copies twenty-odd seconds late at one
    // seat), induced on purpose so the late roster repair is seen live. A
    // build without the feature never reads the variable.
    let delay_certs_ms: Option<u64> = if cfg!(feature = "fault-harness") {
        std::env::var("P2P_POKER_DELAY_CERTS_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v > 0)
    } else {
        None
    };
    // ... and only until this many seconds after the loop started, so a run
    // can park one hand's votes and copies and let the later ones through.
    let delay_certs_until: Option<std::time::Duration> = if cfg!(feature = "fault-harness") {
        std::env::var("P2P_POKER_DELAY_CERTS_UNTIL_S")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(std::time::Duration::from_secs)
    } else {
        None
    };
    let delay_since = tokio::time::Instant::now();
    let _ = PROCESS_STARTED.get_or_init(std::time::Instant::now);
    // **The number nobody has: how many private addresses the real Amino DHT
    // hands this client.** `S1-AC`'s filter counts what it refuses, so the
    // figure arrives as a byproduct of the defence instead of needing a second
    // measurement pass. Taken once, as a shared handle, so the status line can
    // read it without borrowing the swarm.
    let bogons = swarm.behaviour().ipfs_kad.dropped();
    let mut bogons_said = 0u64;
    if let Some(r) = first.resume.as_ref() {
        let _ = events
            .send(NodeEvent::UnfinishedSession {
                key: r.table_key,
                table_name: r.table_name.clone(),
                seat: r.my_seat,
                stack: r.my_stack,
                hand_id: r.hand_id,
            })
            .await;
    }
    // Write the session record: the table as joined, this seat, the boundary
    // reached. A no-op for a founder.
    macro_rules! remember_session {
        ($t:ident, $hand_id:expr, $terminal:expr, $stack:expr) => {
            remember_session!($t, $hand_id, $terminal, $stack, None::<[u8; 32]>)
        };
        ($t:ident, $hand_id:expr, $terminal:expr, $stack:expr, $kept:expr) => {{
            let kept: Option<[u8; 32]> = $kept;
            // `D-037`: the founder's record too -- its own table, advert and
            // hash, with the table key's seed and its last signed roster, so
            // that it can come back as the founder. A seat that joined records
            // what it joined under.
            let who = $t.table.as_ref().and_then(|f| {
                if f.is_founder() {
                    Some((f.table_id(), f.ad().clone(), f.advert_hash(), f))
                } else {
                    Some(($t.joined_key?, $t.joined_ad.as_ref()?.clone(), $t.joined_advert_hash?, f))
                }
            });
            if let Some((key, ad, advert_hash, f)) = who {
                if let (Some(session), Some(my_seat), Ok(advert)) =
                    (f.session(), f.my_seat(), super::advert::to_body_bytes(&ad))
                {
                    let record = crate::storage::session::Record {
                        version: crate::storage::session::RECORD_VERSION,
                        table_id: f.table_id(),
                        session_id: session,
                        table_key: key,
                        founder_peer_id: ad.founder_peer_id.clone(),
                        table_name: ad.table_name.clone(),
                        my_seat,
                        hand_id: $hand_id,
                        terminal: $terminal,
                        my_stack: $stack,
                        written_unix_ms: super::node::now_unix_ms(),
                        advert,
                        advert_hash,
                        ratification: f.my_ratification().map(<[u8]>::to_vec).unwrap_or_default(),
                        hand_secret: kept.unwrap_or([0u8; 32]),
                        secret_hand_id: if kept.is_some() { $hand_id } else { 0 },
                        founder_seed: f.table_seed().unwrap_or([0u8; 32]),
                        roster_list: if f.is_founder() {
                            f.my_list().map(<[u8]>::to_vec).unwrap_or_default()
                        } else {
                            Vec::new()
                        },
                    };
                    match crate::storage::session::save(&profile_dir, &record) {
                        Ok(()) => {
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "session record: hand #{}, seat {my_seat}, stack {}",
                                    $hand_id, $stack
                                )))
                                .await;
                        }
                        Err(e) => {
                            let _ = events
                                .send(NodeEvent::Warning(format!("the session record would not save: {e}")))
                                .await;
                        }
                    }
                    $t.recorded_session = f.session();
                }
            }
        }};
    }

    // `D-042`: the client's one Tox instance starts with the client, so the
    // first table it founds or joins finds the DHT warm and its relays up; a
    // table is a group on it. A failure here is a warning and not the end:
    // `start` tries again when a table comes.
    match first.tox_sink.boot(&profile_dir) {
        Ok(Some(mine)) => {
            let r = first.tox_sink.reach();
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "tox: this client's instance is up from the start, key {}; of {} known nodes, {} bootstrapped and {} TCP relays were accepted{}",
                    short_hash(&mine),
                    r.nodes,
                    r.booted,
                    r.relays,
                    if r.refreshed {
                        ", from a list refreshed on this start"
                    } else {
                        ", from the stored list"
                    }
                )))
                .await;
        }
        Ok(None) => {}
        Err(e) => {
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "tox: no instance at the client's start ({e}); the first table tries again"
                )))
                .await;
        }
    }
    // The last refusal count reported, so a steady state says nothing and a
    // rising one says it every five seconds.
    // When each peer last **answered**, and how long the round trip took. See
    // the `Ping` arm and `SEAT_SILENCE_MS`.
    //
    // The round trip is kept because it is the only thing that says how long a
    // seat takes to answer, and **that is not the same as how far away it is**.
    // Measured between four clients on one machine, where the wire is free:
    // `1 60ms, 2 342ms, 3 68ms`. None of that is network. It is how long each
    // peer's own loop took to get round to replying, which on a table doing
    // elliptic-curve work between hands is the quantity that actually decides
    // whether a seat answers promptly.
    //
    // Reported rather than acted on — nothing in the protocol reads it — and a
    // number nobody can see is a number nobody can use.
    //
    // `None` is *connected and not yet pinged*: the first ping is fifteen
    // seconds after the connection, and a sentinel duration would be
    // indistinguishable from the sub-millisecond answers a local peer gives.
    // `D-044`: the last answer each poker peer gave to the lobby's question
    // (D-040), by its peer id: the tables it named, and when on its own
    // clock. A joiner before the first hand reads its founder's: an answer
    // made after the table's advert that does not name the table is the
    // founder's own word that it offers the table no more -- it left, its
    // client lives on in the lobby answering every ping, its friendship
    // lingers, and the group may never have held it.
    let mut answers: std::collections::HashMap<Vec<u8>, (Vec<[u8; 32]>, u64)> =
        std::collections::HashMap::new();
    let mut alive: std::collections::HashMap<
        PeerId,
        (std::time::Instant, Option<std::time::Duration>),
    > = std::collections::HashMap::new();

    // Who mDNS has already told us about.
    //
    // It keeps telling us. The query goes out on a timer and every neighbour
    // answers it, so the same peer is "discovered" over and over for as long as
    // it is switched on. Announcing each answer costs a line in the log and,
    // through it, a repaint of the whole window — which on a machine with no
    // graphics driver is half a second of processor time for news that is not
    // news. Dialling each answer costs a connection attempt to a peer already
    // connected.
    //
    // Emptied by `Expired`, so a neighbour that goes away and comes back is
    // announced again. Without that half of it this would be a set that only
    // ever grows and a peer that could never be rediscovered.
    let mut seen_locally: std::collections::HashSet<libp2p::PeerId> =
        std::collections::HashSet::new();

    // Which connected peers are other poker clients, so a disconnection can say
    // whether it was one without asking `identify` about a peer already gone.
    let mut poker_peers: std::collections::HashSet<libp2p::PeerId> =
        std::collections::HashSet::new();
    // `D-040`, §7.5: the lobby questions this client has asked and not yet
    // had answered, by request id, with the nonce the answer must carry and
    // the peer it was asked of; and when each peer was last answered, so a
    // peer asking every second costs one answer in five.
    let mut snapshot_pending: std::collections::HashMap<
        libp2p::request_response::OutboundRequestId,
        ([u8; 32], libp2p::PeerId),
    > = std::collections::HashMap::new();
    let mut snapshot_answered: std::collections::HashMap<libp2p::PeerId, tokio::time::Instant> =
        std::collections::HashMap::new();

    // When each lobby provider was last dialled, so a record for a client that
    // is long gone is not dialled every minute for ever.
    //
    // **A map and not a set, because the set remembered an *attempt* and a
    // first dial to a NATed peer routinely fails.** Both ends of a table are
    // usually behind a router, and the first dial goes out before either has a
    // relay reservation or a hole punched; it fails, and until 2026-09-02 that
    // peer was refused for the life of the process.
    //
    // Measured, `split231719-2`: the joiner was offered the founder at 22.8 s
    // **in an answer containing one provider** — so the per-answer budget was
    // certainly not the reason — dialled it, and did not reach it. The founder
    // was offered again at 31.1, 38.4, 200.1 and 327.3 s and skipped every time
    // by `contains`. Contact came at **386.1 s**, and it came the other way:
    // the founder dialled in. The table then formed one second after the run's
    // own deadline, and no hand finished.
    //
    // `REDIAL_AFTER` keeps the whole of the protection the set was written for
    // — a dead record still costs at most one dial per interval — while letting
    // a live peer that was not reachable at 22.8 s be reached at 82.8 s. A peer
    // already connected costs nothing to re-dial:
    // `PeerCondition::DisconnectedAndNotDialing` makes it a no-op.
    let mut dialled_lobby: std::collections::HashMap<libp2p::PeerId, std::time::Instant> =
        std::collections::HashMap::new();

    // When this loop started, which is what `FAST_REDIAL_WINDOW` is measured
    // from. The node's own clock and not the wall: a run's opening minute is a
    // fact about this process, not about the time of day.
    let started = std::time::Instant::now();
    // **Both fault clocks are anchored here rather than at their first use.**
    // Each holds a `OnceLock` initialised on the first call, and
    // `mouth_is_shut` is only ever called from the two hand-send sites — so its
    // window would otherwise begin at the first hand, a minute and a half into
    // a run, and `-MuteAt 180` would mean a different moment in every run.
    // `link_is_down` is called from arms that fire immediately and so anchors
    // itself correctly today; it is armed here too, because a knob whose clock
    // depends on which arm happens to fire first is a knob that measures
    // something else the day an arm moves.
    let _ = link_is_down();
    let _ = mouth_is_shut();
    // **This site's own dials, by the `ConnectionId` its `DialOpts` carries.**
    // Five other subsystems dial these same peers by id — the join
    // request-response, `dcutr`, `autonat`'s dial-backs, both `kad`
    // behaviours and the mDNS arm — and `dcutr` alone spends three dials per
    // `AttemptsExceeded`, which one log on disk reaches 33 times. Without this
    // the crawl's budget would be emptied by failures the crawl never caused.
    // Entries are transient: every dial ends in an established connection or
    // an error, and all three arms remove theirs.
    let mut lobby_dials: std::collections::HashMap<
        libp2p::swarm::ConnectionId,
        libp2p::PeerId,
    > = std::collections::HashMap::new();
    // Providers entitled to one fast second look. Bounded by construction:
    // an entry is spent the moment the provider is re-dialled, and
    // `FAST_REDIAL_BUDGET` caps how many are ever granted.
    let mut fast_lane: std::collections::HashSet<libp2p::PeerId> =
        std::collections::HashSet::new();
    let mut fast_budget: u32 = FAST_REDIAL_BUDGET;

    // How many discovery cycles in a row this client has been comfortably under
    // its connection budget. Three, and the budget comes down a step.
    let mut comfortable: u8 = 0;
    // The budget as it stands. Kept here because `ConnectionLimits` has a
    // setter and no getter, so the only way to know the current value is to be
    // the one who set it.
    let mut budget: u32 = MAX_CONNECTIONS;
    // Connections held, counted here rather than asked of the swarm: there is
    // no accessor for it, and the two events that change it are already handled.
    let mut state_peers: u32 = 0;

    // The QUIC port, kept only so the router can be asked to open it once.
    // It used to be what got announced to Mainline as well; that is gone, and
    // opening a door in a NAT is worth doing whether or not anybody is told
    // about it through a BitTorrent swarm.
    let mut mapped_port: Option<u16> = None;


    // Peers already asked for a reservation because identify said they relay.
    // Asking twice is not harmful, only noisy, and the noise is a log line per
    // identify push — which arrives every time the peer's addresses change.
    // **When each relay was last asked, not merely that it was.**
    //
    // A set recorded the intent and never the outcome, so a relay that refused
    // once — or whose `listen_on` failed synchronously, which only warns — was
    // banned for the life of the process. That includes the relay whose
    // reservation later lapsed: `have_reservation` goes false again and the one
    // host that could restore it is the one host this client will never ask.
    // The same shape as `dialled_lobby`, found by the sweep `S1-AK` asked for.
    let mut asked_hops: std::collections::HashMap<libp2p::PeerId, std::time::Instant> =
        std::collections::HashMap::new();

    // Whether the public DHT has been asked who the relays are. Once is enough
    // to start: the answer arrives as providers, and each of those is then
    // dialled and asked for a reservation on its own account.
    let mut asked_public_dht = false;

    // Whether this client's own record is in the public lobby. Set once the
    // announcement has an address in it, because one without is discarded.
    // **Confirmed, not merely dispatched.**
    //
    // `start_providing` returning `Ok` means a query left this process; it says
    // nothing about whether any peer stored the record, which is the ordinary
    // failure when the walk happens seconds after the first reservation and the
    // routing table is at its thinnest. `NETWORK_STACK.md` §3 step 5 says so in
    // terms — *there is no repair … nothing widens it until the client
    // restarts* — and this is that repair.
    //
    // The confirmation costs nothing because it is already in the log: this
    // client provides the lobby key, so it appears in its own `get_providers`
    // answers. Seeing itself is proof the record reached somebody and came back.
    let mut in_public_lobby = false;
    let mut announced_at: Option<std::time::Instant> = None;

    // Whether this client has offered itself as a relay. Once only: the record
    // is republished by Kademlia on its own.
    let mut volunteering = false;

    // Whether any relay has ever been seen, so "no relay found" is a finding
    // about the network rather than about the first minute of a run.
    let mut seen_a_relay = false;

    // When this client last re-published its table for a newcomer, so a rush of
    // arrivals is one publish rather than one each.
    let mut last_lobby_shout: Option<tokio::time::Instant> = None;

    let mut have_reservation = false;
    // **Whether the reservation this client holds can carry a hand, and which
    // poker peers are reachable only through it.**
    //
    // Measured across two networks and it is the finding of that run: every
    // public relay found offered 131 072 bytes / 120 s, which `relay::adequate`
    // correctly calls *not enough*, and the client said so — once, at
    // reservation time, three thousand lines before the hand. When DCUtR then
    // failed for the one peer that mattered, the hand reached the deal, the
    // circuit hit its limit, publishing began to answer
    // `NoPeersSubscribedToTopic`, and the hand died at its deadline reporting
    // only *"the hand ran out of time"*.
    //
    // Both halves are needed. An inadequate reservation with every peer
    // hole-punched carries nothing of the game and is irrelevant; a relayed
    // peer on an unlimited reservation is fine. It is the conjunction that
    // ends hands, so the conjunction is what the deadline reports.
    let mut relay_inadequate = false;
    let mut relayed_peers: std::collections::HashSet<libp2p::PeerId> =
        std::collections::HashSet::new();
    // Counted so that "no relay" is reported as a finding rather than as
    // impatience: three cycles is three minutes of looking.
    let mut relay_searches: u32 = 0;
    // Checked often, acted on rarely. The listen address arrives a moment after
    // the swarm starts, so a timer whose period **is** the re-announce interval
    // misses its first tick and then says nothing for ten minutes — which is
    // what the first run of this binary did, silently.
    // The router, asked to open the port this client is listening on. PCP and
    // NAT-PMP, beside the UPnP already in the behaviour: a router that speaks
    // one of the three is a player who does not need a relay, and a hand over a
    // relay costs somebody else's bandwidth.
    //
    // Held so that renewing it is possible and so that dropping this task drops
    // the mapping — a client that exits without releasing leaves a hole in a
    // stranger's router until it expires.
    // The router, asked to open the port this client listens on: PCP and
    // NAT-PMP, beside the UPnP already in the behaviour. A router that speaks
    // one of the three is a player who does not need a relay, and a hand over a
    // relay costs somebody else's bandwidth.
    //
    // In **its own task**, because both protocols are UDP to an address that may
    // not answer and asking inside this loop stops the whole node while a router
    // that is not there fails to reply. It ends by itself when the event channel
    // closes, and hands the port back on the way out.

    let mut discover_timer = tokio::time::interval(Duration::from_secs(60));
    // Ten minutes. The table changes slowly and a file written every minute is
    // a file written six hundred times a day for no gain.
    let mut peerbook_timer = tokio::time::interval(Duration::from_secs(600));
    // Presence, on the same rhythm the table advertisements use and for the
    // same reason: three of these fit inside the time it takes to be forgotten,
    // so two lost messages do not empty a pane that should not be empty. The
    // rate is a wire value and is `PROTOCOL.md` §12's, not this module's.
    let mut presence_timer = tokio::time::interval(Duration::from_millis(
        crate::protocol::constants::PRESENCE_HEARTBEAT_MS,
    ));
    let mut housekeeping = tokio::time::interval(REBROADCAST);

    // **The hand's own handling, in one place and called from two.**
    //
    // A table's game traffic rides GossipSub or, under D-019, a Tox group — and
    // the answer to a hand event is the same either way: apply it, replay what
    // was held for a stage this client had not reached, publish what comes out,
    // and report what the interface needs. Only the *verdict* differs, and only
    // because GossipSub has one and Tox does not.
    //
    // A macro rather than a function because the body reads and writes a dozen
    // of this loop's own locals — `said`, `act_by`, `next_hand_at`, four
    // report-once flags — and threading them through a signature would mean a
    // struct refactor across a file whose most delicate property (one verdict,
    // one place to report it) was a day's debugging to arrive at. Expanding the
    // same tokens at both sites cannot make the two drift; two copies could.
    //
    // Every name it touches is declared above this point, which is what makes
    // `macro_rules!` hygiene resolve them to the loop's own bindings.
    // `S1-CW`: what every road that can end a hand does once its frames are
    // out -- the abort said once, the boundary armed, this client's clock
    // read. `hand_event!` below does it for an incoming event; the stall
    // tick's vote and its replay of held events can end the hand too, and
    // did so with nobody told: a certificate at a cryptographic stage ends
    // the hand before any card is out, and both survivors of a three-seat
    // table sat on the ended hand for the rest of the run.
    macro_rules! hand_may_have_ended {
        ($t:ident, $h:expr) => {{
            if let Some(why) = $h.aborted().filter(|_| !$t.abort_reported) {
                $t.abort_reported = true;
                $t.act_by = None;
                let _ = events.send(NodeEvent::Warning(abort_words(why))).await;
                arm_boundary!($t, std::time::Duration::from_millis(800));
            }
            let report = report_hand($h, &events, &mut $t.turn_reported).await;
            if let Some(end) = report.ended {
                arm_boundary!($t, end.pause());
            }
            $t.act_by = hurry(report.clock.apply($t.act_by, $h.action_deadline()), autoplay);
        }};
    }
    macro_rules! hand_event {
        ($t:ident, $h:expr, $bytes:expr) => {{
            use crate::table::hand::Failed;
            let now = super::node::now_unix_ms();
            // fault-harness: a certificate copy is parked, not judged (see
            // `delay_certs_ms`). Accepted for the mesh — it is a peer's
            // honest copy — and released by the stall tick when due.
            if delay_certs_ms.is_some()
                && !$t.releasing_certs
                && delay_certs_until.map_or(true, |u| delay_since.elapsed() < u)
                && matches!(
                    crate::net::chained::peek($bytes, TABLE_FRAME_PEEK),
                    Ok((
                        crate::protocol::messages::EventType::TimeoutCert
                            | crate::protocol::messages::EventType::TimeoutVote,
                        _,
                        _
                    ))
                )
            {
                // Votes too: a seat that holds every vote seals its own copy
                // and banks it, so the odd seat of `S1-BS` is one that missed
                // the votes as well as the copies.
                let ms = delay_certs_ms.unwrap_or(0);
                $t.delayed_certs.push((
                    tokio::time::Instant::now() + std::time::Duration::from_millis(ms),
                    $bytes.to_vec(),
                ));
                if !$t.delay_said {
                    $t.delay_said = true;
                    let _ = events
                        .send(NodeEvent::Warning(format!(
                            "fault-harness: every TIMEOUT_VOTE and TIMEOUT_CERT this seat receives is parked for {ms} ms before it is judged"
                        )))
                        .await;
                }
                Some(gossipsub::MessageAcceptance::Accept)
            } else {
                            match $h.on_event($bytes, &app_key, now) {
                            Ok(sends) => {
                                // Anything held for a stage this client had
                                // not reached is judged again now, because
                                // this event may have been the one that
                                // reached it. Without this a peer that ran
                                // ahead is parked for ever: the mesh does
                                // not re-send, and a hand of 2m+4 stages
                                // hears out-of-order messages as a matter
                                // of course rather than as an exception.
                                let (mut more, held_failures) = $h.replay_early(&app_key, now);
                                let mut sends = sends;
                                sends.append(&mut more);
                                for e in held_failures {
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "a held event was refused: {e}"
                                        )))
                                        .await;
                                }
                                publish_hand(sends, &mut swarm, &mut $t.said, &$t.tox_sink);
                                // `D-033`: the hand's card material goes into the
                                // record the moment the deck stage begins -- not on
                                // the next tick, which a stop at the deal beat by a
                                // second (`run110811-2`).
                                let material = $h.secret().and_then(|s| {
                                    if $t.material_recorded == Some($h.hand_id()) {
                                        return None;
                                    }
                                    Some(($h.hand_id(), $h.stack_at_boundary($h.my_seat()), s.keep()))
                                });
                                if let Some((hid, stack, kept)) = material {
                                    $t.material_recorded = Some(hid);
                                    remember_session!($t, hid, [0; 32], stack, Some(kept));
                                }
                                // Outside `dealt()`: a certificate is
                                // decided at cryptographic stages too, and
                                // gating the report on cards being out
                                // delayed every note until some later
                                // event happened to take another path.
                                if let Some(n) = $h.take_cert_note() {
                                    let _ = events.send(NodeEvent::Warning(n)).await;
                                }
                                // The prover's side of a shuffle context.
                                // It used to be emitted only on success and
                                // so never reached the error arm below; a
                                // refused proof now ends the hand with a
                                // `cause = 2` abort, which is an `Ok` with
                                // sends, so the refusal's own note arrives
                                // here too.
                                if let Some(n) = $h.take_shuffle_note() {
                                    let _ = events.send(NodeEvent::Warning(n)).await;
                                }
                                // The one condition this client cannot
                                // repair and must not hide.
                                if let Some(f) = $h.take_fork() {
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "this hand has forked: {f}"
                                        )))
                                        .await;
                                }
                                if $h.dealt() {
                                    if !$t.hand_reported {
                                        $t.hand_reported = true;
                                        let init = $h.init();
                                        let _ = events
                                            .send(NodeEvent::HandBegan {
                                                hand_id: init.hand_id,
                                                button: init.button_position,
                                                dealt_in: init.dealt_in.clone(),
                                            })
                                            .await;
                                    }
                                    let deck = ($h.shuffler(), $h.shuffled());
                                    if $t.deck_reported != Some(deck) {
                                        $t.deck_reported = Some(deck);
                                        let _ = events
                                            .send(NodeEvent::DeckProgress {
                                                hand_id: $h.hand_id(),
                                                shuffling: deck.0,
                                                ready: deck.1,
                                            })
                                            .await;
                                    }
                                } else {
                                    let _ = events
                                        .send(NodeEvent::HandWaiting {
                                            hand_id: $h.hand_id(),
                                            seats: $h.waiting_for(),
                                        })
                                        .await;
                                }
                                // `S1-CW`: from here on for a dealt hand and an
                                // undealt one alike. A certificate at a cryptographic
                                // stage ends the hand before any card is out, and
                                // everything below -- the abort said, the boundary
                                // armed, the clock stopped -- used to run only under
                                // `dealt()`: both survivors of a three-seat table sat
                                // on a hand that had ended, saying *waiting for seats*
                                // with nobody named, for the rest of the run
                                // (`run195623-3`, `run131730-3`).
                                // The cards, once and once only. They
                                // are read from a complete set of
                                // verified shares or not at all, so
                                // there is no partial state to report.
                                        // How the count stands. A vote that is
                                // never counted is the quietest way
                                // this machinery can fail: everybody
                                // says their clock ran out and nothing
                                // happens.
                                if let Some((subject, held, need, d)) = $h.take_tally() {
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "seat {subject} @{}: {held}/{need} agree",
                                            short_hash(&d)
                                        )))
                                        .await;
                                }
                                // **`S1-BB`: the carrier at the moment of
                                // the accusation, not at the moment of
                                // holding back.** One line per vote this
                                // client casts, whatever the bit says, so
                                // the sample is not conditioned on the
                                // lever the bit itself sets.
                                for (subject, mid, long_past) in $h.take_vote_carrier() {
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "voted about seat {subject}: mid-delivery {mid}, \
                                             long past {long_past}"
                                        )))
                                        .await;
                                }
                                // A seat the table acted for, once.
                                if let Some((seat, what)) = $h.take_certified_action() {
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "the table acted for seat {seat}: {what:?}"
                                        )))
                                        .await;
                                    // `D-045`, the owner's rule: a seat the table acted for
                                    // that the group has not heard from for `QUIET_LIMIT_S`
                                    // is a member that no longer plays, and the players' word removes it from the
                                    // group -- every member drops it, and the founder's kick
                                    // counts because every member allowed it on that word.
                                    // Not for good: a seat that comes back is invited again
                                    // (D-031). A seat merely slow, still heard, stays.
                                    let entry = $t.table.as_ref().and_then(|f| {
                                        f.roster()
                                            .seats()
                                            .iter()
                                            .find(|e| e.seat == seat)
                                            .map(|e| (e.app_public_key, e.tox_key))
                                    });
                                    if let Some((app, line)) = entry {
                                        let quiet = [
                                            $t.tox_sink.quiet_secs(&app),
                                            line.and_then(|k| $t.tox_sink.quiet_line(&k)),
                                        ]
                                        .into_iter()
                                        .flatten()
                                        .min();
                                        let held = $t.tox_sink.in_group(&app)
                                            || line.is_some_and(|k| $t.tox_sink.in_group_line(&k));
                                        if held && quiet.is_some_and(|q| q >= QUIET_LIMIT_S) {
                                            $t.tox_sink.tell(super::toxsink::Seat::Remove {
                                                app_key: Some(app),
                                                tox_key: line,
                                                for_good: false,
                                            });
                                            let _ = events
                                                .send(NodeEvent::Warning(format!(
                                                    "seat {seat}, acted for by the table and silent in its group for {} s, is removed from the group by the table's word (D-045)",
                                                    quiet.unwrap_or(0)
                                                )))
                                                .await;
                                        }
                                    }
                                }
                                remove_by_the_word!($t, $h);
                                if let Some(why) =
                                    $h.aborted().filter(|_| !$t.abort_reported)
                                {
                                    $t.abort_reported = true;
                                    $t.act_by = None;
                                    // **Which abort, and not one
                                    // sentence for all of them.** This
                                    // said "a peer ended the hand on
                                    // its own deadline" whatever
                                    // happened, and since `cause = 2`
                                    // exists that is sometimes simply
                                    // untrue: a hand ended by a proof
                                    // that does not hold ends in
                                    // seconds, names a seat, and has
                                    // nothing to do with anybody's
                                    // clock. A player told the wrong
                                    // reason looks for the wrong fault.
                                    let words = abort_words(why);
                                    let _ = events
                                        .send(NodeEvent::Warning(words))
                                        .await;
                                    arm_boundary!($t, std::time::Duration::from_millis(800));
                                }
                                let report =
                                    report_hand($h, &events, &mut $t.turn_reported).await;
                                if let Some(end) = report.ended {
                                    arm_boundary!($t, end.pause());
                                }
                                $t.act_by = hurry(report.clock.apply($t.act_by, $h.action_deadline()), autoplay);
                                if let Some(cards) = $h.cards().filter(|_| !$t.cards_reported) {
                                    $t.cards_reported = true;
                                    let _ = events
                                        .send(NodeEvent::CardsDealt {
                                            hand_id: $h.hand_id(),
                                            seats: $h.init().dealt_in.clone(),
                                        })
                                        .await;
                                    let _ = events
                                        .send(NodeEvent::HoleCards {
                                            hand_id: $h.hand_id(),
                                            cards: [cards[0].index(), cards[1].index()],
                                        })
                                        .await;
                                }
                                // **Reported before leaving, or this
                                // node forwards nothing.** Every arm of
                                // this branch returns to the top of the
                                // loop, and with `validate_messages()` set
                                // an unreported message is never passed on
                                // — so a peer that hears an event only
                                // through this one never hears it at all.
                                Some(gossipsub::MessageAcceptance::Accept)
                            }
                            // A peer one stage ahead. Held, not refused:
                            // GossipSub does not order two messages.
                            Err(Failed::NotYet) => {
                                if let Some(n) = $h.take_cert_note() {
                                    let _ = events
                                        .send(NodeEvent::Warning(format!("{n} | HELD")))
                                        .await;
                                }
                                // **What is said to the mesh depends on
                                // what was kept.** Accepting an event this
                                // client could not verify forwards it in
                                // this client's own name, and `NotYet` is
                                // reachable before any signature is
                                // checked — so unsigned junk used to be
                                // both relayed and stored, sixty-four at a
                                // time, evicting the genuine early events
                                // the queue exists for.
                                Some(match $h.hold($bytes.to_vec()) {
                                    Holding::Kept => gossipsub::MessageAcceptance::Accept,
                                    // Somebody else's hand, and verified:
                                    // the table identity and the signature
                                    // held, only the `hand_id` is not this
                                    // client's. Relayed, because a peer one
                                    // hand behind is exactly the
                                    // intermediary a peer one hand ahead
                                    // needs — it is simply not kept.
                                    Holding::AnotherHand { hand_id, seat } => {
                                        // `S1-CR`: a resumed bystander keeps a newer hand's
                                        // frames too, so a hand it adopted and cannot follow
                                        // is abandoned for the next one at the stall tick.
                                        // `D-038`: kept whether or not this client is resuming.
                                        // A seat that finds itself on a hand nobody else has
                                        // rejoins the table from these, and they are bounded --
                                        // two hand ids, one copy per seat, `RESUME_EARLY_CAP`
                                        // other frames -- so an ordinary table's pause skew costs
                                        // a few frames of memory and nothing else.
                                        if hand_id > $h.hand_id() {
                                            let _ = stash_for_resume($bytes, &mut $t.resume_inits, &mut $t.resume_early);
                                        }
                                        // `S1-CX`: the next hand's opening from a seat one
                                        // hand ahead, while nothing has been dealt here.
                                        if !$t.resuming && hand_id == $h.hand_id() + 1 && $h.street().is_none() && !$h.over() {
                                            if let Ok((crate::protocol::messages::EventType::HandInit, _, 0)) =
                                                crate::net::chained::peek($bytes, TABLE_FRAME_PEEK)
                                            {
                                                if let Ok(o) = crate::net::chained::open_in_hand(
                                                    $bytes,
                                                    crate::table::hand::FRAME_CAP,
                                                    crate::protocol::messages::EventType::HandInit,
                                                    &$h.table_id(),
                                                    hand_id,
                                                ) {
                                                    $t.give_up_for = Some((hand_id, o.envelope.previous_event_hash));
                                                }
                                            }
                                        }
                                        note_a_hand_ahead(
                                            $h,
                                            hand_id,
                                            seat,
                                            &mut $t.ahead,
                                            &mut $t.adrift,
                                        );
                                        // **A certificate about the hand just
                                        // finished, arriving during the next
                                        // one, was dropped here with no line**
                                        // — `S1-BS`: eight seats banked one,
                                        // the ninth never had it before its
                                        // hand ended, and whether a copy ever
                                        // reached it afterwards was
                                        // unobservable. Said once per hand;
                                        // the roster half it carries is what a
                                        // late roster repair would read.
                                        let kind = crate::net::chained::peek($bytes, TABLE_FRAME_PEEK)
                                            .ok()
                                            .map(|(k, _, _)| k);
                                        // `S1-BM`: the return pair about hand k
                                        // takes the same road as a late
                                        // certificate -- it is about hand k's
                                        // boundary, the retained hand judges it,
                                        // and a bank there re-derives this one.
                                        let what = match kind {
                                            Some(crate::protocol::messages::EventType::ReturnCert) => "RETURN_CERT",
                                            Some(crate::protocol::messages::EventType::ReturnVote) => "RETURN_VOTE",
                                            _ => "certificate",
                                        };
                                        if hand_id.saturating_add(1) == $h.hand_id()
                                            && matches!(
                                                kind,
                                                Some(
                                                    crate::protocol::messages::EventType::TimeoutCert
                                                        | crate::protocol::messages::EventType::ReturnCert
                                                        | crate::protocol::messages::EventType::ReturnVote
                                                )
                                            )
                                        {
                                            match $t.previous.as_mut() {
                                                // The hand it is about is still held: it
                                                // banks there, and a new bank re-derives
                                                // the running hand (`S1-BS`).
                                                Some(p) if p.hand_id() == hand_id => {
                                                    let now = super::node::now_unix_ms();
                                                    // Held there too: a copy the
                                                    // retained hand cannot judge yet
                                                    // (a voter-set shortfall, D-024
                                                    // point 5) is re-judged by its
                                                    // replay once another banks.
                                                    match p.on_event($bytes, &app_key, now) {
                                                        Err(Failed::NotYet) => {
                                                            let _ = p.hold($bytes.to_vec());
                                                        }
                                                        // `S1-BM`: the last return vote
                                                        // completes this client's own
                                                        // certificate, sealed by the
                                                        // retained hand; it still goes out.
                                                        Ok(sends) => {
                                                            publish_hand(sends, &mut swarm, &mut $t.said, &$t.tox_sink);
                                                        }
                                                        Err(_) => {}
                                                    }
                                                    let (more, _) = p.replay_early(&app_key, now);
                                                    publish_hand(more, &mut swarm, &mut $t.said, &$t.tox_sink);
                                                    if let Some(n) = p.take_cert_note() {
                                                        let _ = events
                                                            .send(NodeEvent::Warning(format!(
                                                                "hand #{hand_id} (over): {n}"
                                                            )))
                                                            .await;
                                                    }
                                                    if p.take_late_roster() {
                                                        $t.late_banked_for = Some(p.hand_id());
                                                        $t.pending_repair = p.next_hand();
                                                    }
                                                }
                                                _ => {
                                                    if $t.late_cert_said != Some($h.hand_id()) {
                                                        $t.late_cert_said = Some($h.hand_id());
                                                        // **What the node knows, and not a word more.**
                                                        // Banking is keyed on the subject digest and a
                                                        // hand can certify two seats, so "an earlier
                                                        // copy of this certificate" is more than the
                                                        // hand id can prove — the same false second
                                                        // clause `S1-BV` was opened for.
                                                        let line = if $t.late_banked_for == Some(hand_id) {
                                                            format!(
                                                                "a {what} about hand #{hand_id} from seat {seat:?} arrived during hand #{}; hand #{hand_id} is no longer held here, and a certificate about it did bank here before retention ended",
                                                                $h.hand_id()
                                                            )
                                                        } else {
                                                            format!(
                                                                "a {what} about hand #{hand_id} from seat {seat:?} arrived during hand #{}: hand #{hand_id} is no longer held here, so it cannot repair anything",
                                                                $h.hand_id()
                                                            )
                                                        };
                                                        let _ = events.send(NodeEvent::Warning(line)).await;
                                                    }
                                                }
                                            }
                                        } else if hand_id.saturating_add(1) == $h.hand_id()
                                            && kind == Some(crate::protocol::messages::EventType::HandComplete)
                                        {
                                            // **`S1-CN`: a settlement of the hand just finished, arriving
                                            // during the next one, and it is applied to nothing.** §4.10 says
                                            // `HAND_COMPLETE` wins over an abort because it is collective, and
                                            // `on_late_settlement` honours that for exactly as long as hand k
                                            // is this client's `hand` -- after a deadline abort, the 800 ms
                                            // `next_hand_at` below. The retained hand takes certificates only,
                                            // so from here on the copy is relayed and read by nobody, and hand
                                            // k+1 here stands on the abort's stacks while the emitters' stands
                                            // on the settlement's: a fork, and this seat is the one that
                                            // forked. Said once per hand, and only when hand k ended here by an
                                            // abort that no settlement closed -- a copy of a settlement this
                                            // client reached itself is the emitters' ordinary re-send of their
                                            // terminal and costs nothing.
                                            let unsettled_abort = match $t.previous.as_ref() {
                                                Some(p) if p.hand_id() == hand_id => {
                                                    p.aborted().is_some() && !p.late_settled()
                                                }
                                                _ => $t.unsettled_abort_here == Some(hand_id),
                                            };
                                            if unsettled_abort && $t.late_settle_said != Some($h.hand_id()) {
                                                $t.late_settle_said = Some($h.hand_id());
                                                let _ = events
                                                    .send(NodeEvent::Warning(format!(
                                                        "a settlement of hand #{hand_id} from seat {seat:?} arrived during hand #{}: not applied, because the late path closes 800 ms after an abort and hand #{hand_id} ended here by one; hand #{} was derived from the abort, which is a branch the settlement's emitters do not share",
                                                        $h.hand_id(),
                                                        $h.hand_id()
                                                    )))
                                                    .await;
                                            }
                                        } else if seat.is_some() {
                                            // The next hand's events, kept for it: they arrive
                                            // during this client's 800 ms pause at the boundary
                                            // and used to be dropped. The `HAND_INIT`s were the
                                            // first half of this (`S1-BS`); everything else was
                                            // still dropped, and that cost a bystander hand
                                            // #16's whole `DECK_INIT` stage and 283 seconds
                                            // inside it (`S1-BW`).
                                            match keep_for_next_hand(kind, hand_id, $h.hand_id()) {
                                                Keep::Init
                                                    if $t.next_inits.len() < NEXT_INITS_CAP
                                                        && seat.is_some_and(|s| {
                                                            $t.next_inits
                                                                .iter()
                                                                .filter(|(f, _)| *f == s)
                                                                .count()
                                                                < NEXT_INITS_PER_SEAT
                                                        })
                                                        && !$t.next_inits
                                                            .iter()
                                                            .any(|(_, b)| b[..] == $bytes[..]) =>
                                                {
                                                    if let Some(s) = seat {
                                                        $t.next_inits.push((s, $bytes.to_vec()));
                                                    }
                                                }
                                                Keep::Early => {
                                                    if let Some(s) = seat {
                                                        keep_early(
                                                            &mut $t.next_early,
                                                            s,
                                                            $bytes,
                                                            &mut $t.next_early_lost,
                                                        );
                                                    }
                                                }
                                                _ => {}
                                            }
                                        }
                                        gossipsub::MessageAcceptance::Accept
                                    }
                                    Holding::Malformed => {
                                        gossipsub::MessageAcceptance::Reject
                                    }
                                })
                            }
                            // Not a hand event at all - fall through to the
                            // formation, which is what it will be. The one
                            // way out of this match that reports nothing,
                            // because the formation handler below reports
                            // for it.
                            Err(Failed::Wire(joinwire::WireError::WrongType)) => None,
                            Err(e) => {
                                if let Some(n) = $h.take_shuffle_note() {
                                    let _ = events
                                        .send(NodeEvent::Warning(n))
                                        .await;
                                }
                                if let Some(n) = $h.take_settle_note() {
                                    let _ = events
                                        .send(NodeEvent::Warning(n))
                                        .await;
                                }
                                if let Some(n) = $h.take_cert_note() {
                                    let _ = events
                                        .send(NodeEvent::Warning(n))
                                        .await;
                                }
                                // `S1-CF`: two peers at one genesis with two
                                // dealt_in sets. Said before the refusal it
                                // explains, because "hand: seat 3: dealt_in
                                // differs" on its own reads as an ordinary
                                // stale frame and this is not one.
                                if let Some(n) = $h.take_dealt_note() {
                                    let _ = events
                                        .send(NodeEvent::Warning(n))
                                        .await;
                                }
                                let _ = events
                                    .send(NodeEvent::Warning(format!("hand: {e}")))
                                    .await;
                                Some(gossipsub::MessageAcceptance::Reject)
                            }
                        }
            }
        }};
    }
    // **Everything a table leaves behind, cleared in one place.**
    //
    // Leaving was four copies of the same list of assignments, and the comment
    // inside it records the last time this went wrong: `ever_dealt` was set true
    // and never set back, so the *next* table in the same process refused every
    // stranger, never opened hand 1, never re-said a ratification, never
    // released a silent seat and was never re-advertised — five mechanisms, all
    // by not running, all from one boolean nobody reset. That was fixed by
    // adding one line to each of the four copies, which is exactly the shape of
    // fix that leaves the next field to be found the same way.
    //
    // Twelve more were still being carried across. They matter because **the
    // next table restarts the hand-id sequence at 1** (`opening_for_hand_one` is
    // `Opening::from_formation(f, 1)`), so anything keyed by hand id meets the
    // previous table's record under the same key:
    //
    //  * `ahead` holds a raw hand id per seat and is filled by ORDINARY play —
    //    the `ADRIFT_MARGIN` note records 36 of 134 runs with a seat exactly one
    //    hand behind from `Ended::pause` skew, and says a table of four or more
    //    always has at least two. Within one table those age out as ids rise.
    //    At the next table every one of them is in the future again, so the
    //    first ordinary skew event at hand 1 can latch `adrift` on a client that
    //    never drifted. **`adrift` has no clearing site at all.**
    //  * `frozen` is released only by `RoundTook::Resolved` for the OLD hand id,
    //    whose evidence is the old table's signed bytes carrying the old
    //    `table_id`, which no seat at the new table can verify. And a frozen
    //    client emits nothing, so the seats waiting on its `STATE_HASH` wait for
    //    ever: **it stalls the new table, not just this peer.**
    //  * `boundaries` is admitted on the hand id alone — `Boundaries::holds`
    //    is `open.contains_key(&hand_id)` — and the table is only checked
    //    afterwards. A `Boundary` already carries a `table_id`; it is read when
    //    sealing outgoing events and never to refuse an incoming one.
    //
    // So this is a macro and not four more lines: there is one list, adding to
    // it is one edit, and a `leave_the_table!()` that compiles is a leave that
    // forgot nothing. `table` itself is not in here — two of the four sites have
    // already cleared it and one has never set it — and neither is `tox_sink`,
    // whose teardown must run before this and joins a thread.
    // `S1-BM`: cast this client's return votes on one hand's boundary, from
    // the evidence the node gathered for it, and publish what that seals.
    // The hand decides everything -- whether the boundary settled, whether
    // it holds a value of its own, whether each subject's evidence opens and
    // agrees -- and answers with nothing when there is nothing to say.
    macro_rules! vote_on_returns {
        ($t:ident, $h:expr) => {{
            let evidence = $t.sit_ins.complete($h.hand_id());
            if !evidence.is_empty() {
                match $h.vote_on_returns(&evidence, &app_key, super::node::now_unix_ms()) {
                    Ok(sends) => {
                        if !sends.is_empty() {
                            publish_hand(sends, &mut swarm, &mut $t.said, &$t.tox_sink);
                        }
                        if let Some(n) = $h.take_cert_note() {
                            let _ = events
                                .send(NodeEvent::Warning(format!("hand #{}: {n}", $h.hand_id())))
                                .await;
                        }
                    }
                    Err(e) => {
                        let _ = events
                            .send(NodeEvent::Warning(format!("a return vote would not seal: {e}")))
                            .await;
                    }
                }
            }
        }};
    }
    // `D-038`: this client is on a hand nobody else has -- a strict majority
    // of the seats still in the game are two or more hands past it -- so the
    // hand it holds will never end and every hand it would derive is its
    // own. It used to stop here for good: `adrift` was a terminus, one line
    // and no further hand, and the player sat at a table that had gone on
    // without them (`run172000-3`, after a 70 s outage). Now it does what a
    // seat back from a restart does (D-029, `S1-CR`): drops the branch,
    // keeps the table, the group and the keys, adopts the table's running
    // hand from the majority's copies as a bystander and asks to sit in at
    // that hand's boundary (D-028); D-032 counts the return. Nothing of the
    // table's rests on anything dropped here: no other seat signed a frame
    // of the branch, or the branch would be the table.
    // The player's own leave, from the window or from the fault knob below:
    // the group, the topic, the table, every local fact, the record.
    macro_rules! leave_table_now {
        ($t:ident) => {
            leave_table_now!($t, "left the table".to_string())
        };
        // `S1-DV`: with the reason the window shows -- a joiner taken back to
        // the lobby is told why.
        ($t:ident, $why:expr) => {{
            // The Tox group goes with the table: dropping the handle tells
            // the driver to leave and joins its thread, which flushes what it
            // still holds -- the last message of a hand sits in that queue.
            $t.tox_sink.clear();
            $t.tox_group_said = false;
            $t.table_announces = 0;
            if let Some(t) = $t.table_topic.take() {
                let _ = swarm.behaviour_mut().gossipsub.unsubscribe(&t);
            }
            $t.table = None;
            leave_the_table!($t);
            dht_effort(&mut swarm, false);
            // `S1-CR`: a seat that leaves by its own choice has no session to come back to.
            let _ = crate::storage::session::forget(&profile_dir);
            $t.resume = None;
            $t.resuming = false;
            let _ = events.send(NodeEvent::LeftTable { why: $why }).await;
        }};
    }
    // `D-045`, the owner's rule: a seat the table certified out of the hand
    // that the group has not heard from for `QUIET_LIMIT_S` is a member that
    // no longer plays, and the players' word removes it from the group --
    // every member drops it, and the founder's kick counts because every
    // member allowed it on that word. Not for good: a seat that comes back
    // is invited again (D-031). A seat merely slow, still heard, stays and
    // says so. Read at the certificate's own moment and again on the stall
    // tick; a hand that is over is not skipped, since a timeout certificate
    // ends the hand it certifies in.
    macro_rules! remove_by_the_word {
        ($t:ident, $h:expr) => {{
            let id = $h.hand_id();
            let cert: Vec<u8> = $h.certified_seats().to_vec();
            if $t.required_seen.0 != id {
                $t.required_seen = (id, Vec::new());
            }
            let fresh: Vec<u8> = cert.iter().copied().filter(|s| !$t.required_seen.1.contains(s)).collect();
            $t.required_seen.1 = cert;
            for seat in fresh {
                let entry = $t.table.as_ref().and_then(|f| {
                    f.roster()
                        .seats()
                        .iter()
                        .find(|e| e.seat == seat)
                        .map(|e| (e.app_public_key, e.tox_key))
                });
                if let Some((app, line)) = entry {
                    // `D-047`: the fourth absence is the last. Out of the table's
                    // group for good, never invited again, and out of the table:
                    // its chips leave at the boundary (the engine), so what the
                    // felt shows is a seat that left. Silence is no condition here:
                    // a seat certified four times over is out whether its line is
                    // bad or its play is.
                    let at_the_limit = $h.returns().get(usize::from(seat)).copied().unwrap_or(0)
                        >= crate::protocol::constants::MAX_RETURNS;
                    if at_the_limit {
                        // The word for the seat's own client, kept for this
                        // client's lobby answers; and no request from that key
                        // sits here again.
                        match $h.word_about(seat) {
                            Some(cert) => {
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "the word about seat {seat} is kept for its client's asking: {} bytes of hand #{}",
                                        cert.len(),
                                        $h.hand_id()
                                    )))
                                    .await;
                                $t.out_words.retain(|(s, _, _, _)| *s != seat);
                                $t.out_words.push((seat, app, $h.hand_id(), cert));
                            }
                            None => {
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "no certificate about seat {seat} is banked this hand: its client cannot be told by asking"
                                    )))
                                    .await;
                            }
                        }
                        $t.out_keys.insert(app);
                        $t.tox_sink.tell(super::toxsink::Seat::Remove {
                            app_key: Some(app),
                            tox_key: line,
                            for_good: true,
                        });
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "seat {seat} is out of the table for good after its fourth absence (D-047): removed from the table's group by the table's word, never to be invited again; its chips leave the table at the boundary"
                            )))
                            .await;
                        let _ = events.send(NodeEvent::SeatLeft { seat, quit: true }).await;
                        continue;
                    }
                    let quiet = [
                        $t.tox_sink.quiet_secs(&app),
                        line.and_then(|k| $t.tox_sink.quiet_line(&k)),
                    ]
                    .into_iter()
                    .flatten()
                    .min();
                    let held = $t.tox_sink.in_group(&app)
                        || line.is_some_and(|k| $t.tox_sink.in_group_line(&k));
                    if held && quiet.is_some_and(|q| q >= QUIET_LIMIT_S) {
                        $t.tox_sink.tell(super::toxsink::Seat::Remove {
                            app_key: Some(app),
                            tox_key: line,
                            for_good: false,
                        });
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "seat {seat}, certified out of the hand and silent in the table's group for {} s, is removed from the group by the table's word (D-045)",
                                quiet.unwrap_or(0)
                            )))
                            .await;
                    } else {
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "seat {seat} certified out of the hand, still heard by the table's group ({}): it stays a member (D-045)",
                                match quiet {
                                    Some(q) => format!("{q} s ago"),
                                    None => "no reading".to_string(),
                                }
                            )))
                            .await;
                    }
                }
            }
        }};
    }
    macro_rules! rejoin_from_copies {
        ($t:ident, $theirs:expr, $mine:expr) => {{
            let dead: u64 = $mine;
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "this client is on a hand nobody else has: the table is at hand #{} and this client reached only hand #{dead}. That hand is dropped and the table's running hand is adopted from the others' copies; this seat asks to sit in at its boundary (D-038)",
                    $theirs
                )))
                .await;
            $t.previous = None;
            $t.hand = None;
            $t.pending_repair = None;
            $t.give_up_for = None;
            $t.next_inits.clear();
            $t.next_early.clear();
            $t.resume_inits.retain(|k, _| *k > dead);
            $t.resume_early.retain(|(k, _)| *k > dead);
            $t.resume_said = None;
            $t.ahead.clear();
            $t.adrift = None;
            $t.frozen = None;
            $t.genesis_wait_said = None;
            $t.late_cert_said = None;
            $t.late_banked_for = None;
            $t.unsettled_abort_here = None;
            $t.late_settle_said = None;
            $t.next_hand_at = None;
            $t.deal_at = None;
            $t.return_hold = None;
            $t.boundary_done_for = None;
            $t.act_by = None;
            $t.stage_waiting = (u64::MAX, 0);
            $t.boundaries = crate::table::boundary::Boundaries::new();
            $t.early_checkpoints.clear();
            $t.early_boundary.clear();
            $t.crossed_for = None;
            $t.checkpoint_said = false;
            purge_hand_from_said(&mut $t.said, dead);
            $t.hand_reported = false;
            $t.deck_reported = None;
            $t.cards_reported = false;
            $t.turn_reported = None;
            $t.abort_reported = false;
            $t.resuming = true;
        }};
    }
    // `D-040`: ask one poker peer what it offers (§7.5). A fresh nonce per
    // question; the answer is matched on it.
    macro_rules! ask_lobby {
        ($peer:expr) => {{
            let p: libp2p::PeerId = $peer;
            let now = super::node::now_unix_ms();
            let mut nonce = [0u8; 32];
            let _ = crate::security::rng::fill(&mut nonce);
            if let Ok(bytes) = super::snapshot::ask(&app_key, nonce, now) {
                let id = swarm.behaviour_mut().snapshot.send_request(&p, bytes);
                snapshot_pending.insert(id, (nonce, p));
            }
        }};
    }
    macro_rules! leave_the_table {
        ($t:ident) => {{
            $t.table_closed = false;
            $t.tournament_started = false;
            $t.table_over_at = None;
            $t.ever_dealt = false;
            $t.hand = None;
            $t.late_cert_said = None;
            $t.previous = None;
            $t.next_inits.clear();
            $t.next_early.clear();
            $t.pending_repair = None;
            $t.genesis_wait_said = None;
            $t.late_banked_for = None;
            $t.hand_reported = false;
            $t.deck_reported = None;
            $t.cards_reported = false;
            $t.abort_reported = false;
            $t.turn_reported = None;
            $t.said.clear();
            $t.next_hand_at = None;
            $t.deal_at = None;
            $t.return_hold = None;
            $t.boundary_done_for = None;
            $t.act_by = None;
            // Keyed by hand id, and the next table starts again at 1.
            $t.boundaries = crate::table::boundary::Boundaries::new();
            $t.early_checkpoints.clear();
            $t.early_boundary.clear();
            $t.crossed_for = None;
            $t.checkpoint_said = false;
            $t.frozen = None;
            $t.link_said.clear();
            $t.seat_since.clear();
            $t.ahead.clear();
            $t.adrift = None;
            // **Its own doc says *cleared when the freeze is*, and the line
            // above clears the freeze.** Left behind, the next table's first
            // freeze would be silent — the one state that stops a client
            // dealing, with nothing said about why.
            $t.frozen_said = false;
            // Both hang off the freeze and the reconciliation round, which are
            // per table: `no_round_said` reports why no round could be opened,
            // and `disputes_seen` is counted so that "none arrived" and
            // "several arrived and agreed" are not the same silence.
            $t.no_round_said = false;
            $t.disputes_seen = 0;
            // Per table by their own meaning.
            $t.roster_seats.clear();
            $t.readmitted.clear();
            $t.sit_ins = SitIns::default();
            $t.resume_inits.clear();
            $t.resume_early.clear();
            $t.recorded_session = None;
            $t.hand_one_held_since = None;
            $t.hand_one_progress = None;
            $t.hand_one_forced_said = false;
            $t.why_no_hand_one_said = false;
        }};
    }

    loop {
        // `D-043`: the earliest own clock and the earliest deal among the
        // tables, read before the select so the timers own their instants
        // and borrow no table while the group's messages are awaited.
        let act_deadline = earliest(&tables, |x| x.act_by);
        let deal_deadline = earliest(&tables, |x| x.next_hand_at);
        tokio::select! {
            event = SwarmStreamExt::select_next_some(&mut swarm) => {
                // `D-043`: which table this event is for -- by its topic, by the
                // table its join request names, by the request its join answer
                // answers -- and the active table for everything else.
                let which = table_for_event(&tables, &event, active, &join_pending);
                let t = &mut tables[which];
                mark_table(&events, &mut marked, t).await;
                match event {
                    SwarmEvent::NewListenAddr { address, .. }
                        if address
                            .iter()
                            .any(|p| matches!(p, libp2p::multiaddr::Protocol::P2pCircuit)) =>
                    {
                        // A circuit address is external by construction: a relay
                        // accepted the reservation and handed back the address
                        // it will accept traffic for. Saying so is what makes
                        // this client findable, because `libp2p-kad` publishes
                        // a provider record carrying the swarm's **external**
                        // addresses and nothing else — a NATed player who never
                        // records one announces themselves to the lobby with no
                        // way to be reached, which is indistinguishable from not
                        // announcing at all.
                        //
                        // This does not weaken the rule that only AutoNAT may
                        // call this client publicly reachable. It is not a claim
                        // about this machine; it is a claim about a relay's
                        // address, made by that relay.
                        swarm.add_external_address(address.clone());
                        let _ = events.send(NodeEvent::Listening(address)).await;

                        // And join the lobby now, rather than waiting for the
                        // next discovery tick.
                        //
                        // This is the moment the announcement becomes possible:
                        // before it there is no external address and the record
                        // would be discarded, and the tick that used to carry it
                        // is a minute long and competes with a swarm that is
                        // never idle. Measured before this line existed: two
                        // clients ran for four hundred seconds, both took a
                        // reservation, and the lobby was asked once between
                        // them. Neither ever announced, so neither found the
                        // other.
                        if asked_public_dht && !in_public_lobby {
                            let kad = &mut swarm.behaviour_mut().ipfs_kad;
                            if kad.start_providing(lobby_namespace()).is_ok() {
                                in_public_lobby = true;
                                let _ = events.send(NodeEvent::Announced).await;
                            }
                            kad.get_providers(lobby_namespace());
                        }
                    }
                    SwarmEvent::NewListenAddr { address, .. } => {
                        // Announce only a port something is actually bound to.
                        if mapped_port.is_none() {
                            mapped_port = quic_port(&address);
                            // And ask the router to open it, once, in its own
                            // task. Started here rather than on a timer because
                            // this is the moment there is something to ask about.
                            if let Some(port) = mapped_port {
                                tokio::spawn(portmap::keep_open(port, events.clone()));
                            }
                        }
                        let _ = events.send(NodeEvent::Listening(address)).await;
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, connection_id, .. } => {
                        lobby_dials.remove(&connection_id);
                        state_peers += 1;
                        // A connection that has just been made is evidence the
                        // peer is there, and it is the only evidence available
                        // until the first ping fifteen seconds later. Without
                        // it a seat that joins and is swept a moment afterwards
                        // looks silent because nothing has asked it yet.
                        alive
                            .entry(peer_id)
                            .or_insert((std::time::Instant::now(), None));
                        let _ = events.send(NodeEvent::PeerConnected(peer_id)).await;
                    }

                    // **The answer to a ping, which this loop used to throw
                    // away.** `ping::Behaviour` has been in the swarm from the
                    // start and its events were matched by nothing, so the
                    // client asked every peer whether it was there fifteen
                    // times a minute and never looked at a single reply.
                    //
                    // A connection being up is not evidence that anybody is
                    // behind it: a half-open TCP or a NAT mapping that expired
                    // leaves a socket that looks perfectly well and answers
                    // nothing. Liveness has to be **asked for**, and this is the
                    // asking.
                    // **Only a success is matched, and a failure is
                    // deliberately not the opposite of one.** Silence is
                    // *nothing has come back lately*, which an unreliable link
                    // produces on its own; a failed ping is one datagram that
                    // did not make it. Treating the second as evidence that a
                    // player has gone would cost somebody their seat for a
                    // moment of packet loss, which is why `SEAT_SILENCE_MS` is
                    // six intervals of the first rather than one of the second.
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Ping(ping::Event {
                        peer,
                        result: Ok(rtt),
                        ..
                    })) => {
                        alive.insert(peer, (std::time::Instant::now(), Some(rtt)));
                        // `S1-CS`: the window shows each seat's link.
                        if let Some(seat) = seat_of_peer(t.table.as_ref(), &peer) {
                            // `D-035`: a ping answered from the lobby is not a seat
                            // at the table. Once the hand rides the group, the
                            // reading is the seat's membership there: a client that
                            // left the group gets no reading, whatever its lobby
                            // connection says, and the window's link goes stale.
                            // `D-041`: and the group's word rides with the figure.
                            let in_group = t.table
                                .as_ref()
                                .and_then(|f| {
                                    f.roster().seats().iter().find(|e| e.seat == seat).map(|e| e.app_public_key)
                                })
                                .is_some_and(|k| t.tox_sink.in_group(&k));
                            // `S1-DX`, the owner's rule: on a Tox table the felt's line
                            // is the group's alone -- the reading every two seconds on
                            // the stall tick -- and a libp2p ping says nothing there.
                            let at_the_table = !t.tox_sink.is_on_tox();
                            if at_the_table {
                                let _ = events
                                    .send(NodeEvent::SeatLink {
                                        seat,
                                        rtt_ms: Some(u64::try_from(rtt.as_millis()).unwrap_or(u64::MAX)),
                                        group: t.tox_sink.is_on_tox() && in_group,
                                        quiet_s: None,
                                    })
                                    .await;
                            }
                        }
                    }
                    SwarmEvent::ConnectionClosed {
                        peer_id,
                        num_established,
                        ..
                    } => {
                        // **`num_established` is what is left, not what closed.**
                        // A peer reached over both TCP and QUIC has two
                        // connections, and a relayed peer that hole-punches to a
                        // direct one has two for as long as the changeover
                        // takes. Treating either close as a departure removed a
                        // peer from `poker_peers` while it was still connected,
                        // undid its exemption from the connection cap, and
                        // reported it gone to the player. Measured on three
                        // clients on one machine: eleven departures and four for
                        // the other in four minutes, for two peers that never
                        // actually went anywhere, and seventeen
                        // direct-connection events for eight arrivals.
                        if num_established == 0 && poker_peers.remove(&peer_id) {
                            let _ = events
                                .send(NodeEvent::PokerPeer { peer: peer_id, gone: true })
                                .await;
                        }
                        state_peers = state_peers.saturating_sub(1);
                        let _ = events.send(NodeEvent::PeerDisconnected(peer_id)).await;
                        // `S1-CS`: a seat whose last connection closed is shown so.
                        if num_established == 0 {
                            if let Some(seat) = seat_of_peer(t.table.as_ref(), &peer_id) {
                                // `D-041`, `S1-DX`: on a Tox table a libp2p connection
                                // closing says nothing about the seat; the group's
                                // reading does, every two seconds, and this says nothing.
                                if !t.tox_sink.is_on_tox() {
                                    let _ = events
                                        .send(NodeEvent::SeatLink { seat, rtt_ms: None, group: false, quiet_s: None })
                                        .await;
                                }
                            }
                        }
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::RelayClient(
                        libp2p::relay::client::Event::ReservationReqAccepted {
                            relay_peer_id,
                            limit,
                            ..
                        },
                    )) => {
                        have_reservation = true;
                        let bytes = limit.as_ref().and_then(|l| l.data_in_bytes());
                        let seconds = limit
                            .as_ref()
                            .and_then(|l| l.duration())
                            .map(|d| d.as_secs());
                        // The decision `NAT_AND_DISCOVERY.md` asks for by name:
                        // compare the relay's own reported limits against what a
                        // hand actually costs, before anything is committed to
                        // this circuit.
                        let verdict = relay::adequate(
                            MAX_SEATS,
                            Duration::from_millis(HAND_DEADLINE_CAP_MS),
                            bytes,
                            limit.as_ref().and_then(|l| l.duration()),
                        );
                        let adequate = matches!(
                            verdict,
                            relay::Adequacy::Adequate | relay::Adequacy::Unlimited
                        );
                        relay_inadequate = !adequate;
                        let _ = events
                            .send(NodeEvent::Reserved {
                                relay: relay_peer_id,
                                bytes,
                                seconds,
                                adequate,
                            })
                            .await;
                    }
                    // `D-040`: a lobby question, or an answer to this client's.
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Snapshot(
                        request_response::Event::Message { peer, message, .. },
                    )) => {
                        let now = super::node::now_unix_ms();
                        match message {
                            request_response::Message::Request { request, channel, .. } => {
                                // One answer per peer in five seconds bounds what
                                // answering costs; a question is cheap to ask.
                                let recently = snapshot_answered
                                    .get(&peer)
                                    .is_some_and(|at| at.elapsed() < Duration::from_secs(5));
                                if recently {
                                    continue;
                                }
                                let Ok((ask, _asker)) = super::snapshot::open_ask(&request, now) else {
                                    // A question this client cannot read is not answered.
                                    continue;
                                };
                                snapshot_answered.insert(peer, tokio::time::Instant::now());
                                // This client's own table while it is open: the founder
                                // is the source of its advert, and a table that has dealt
                                // is not offered (D-037).
                                let mut adverts: Vec<Vec<u8>> = Vec::new();
                                // `D-043`: every table this client founded and has not dealt at.
                                for x in tables.iter() {
                                    if let Some(f) = x.table.as_ref().filter(|f| f.is_founder() && !x.ever_dealt) {
                                        if let Some(b) = f.current_advert() {
                                            adverts.push(b);
                                        }
                                    }
                                }
                                adverts.truncate(usize::from(ask.max_tables).min(SNAPSHOT_MAX_ADS));
                                let offered = adverts.len();
                                // `D-047`: the table's word about the seats out for good,
                                // from every table this client sits at, for their own
                                // clients -- the certificate, which they verify themselves.
                                let mut out: Vec<super::snapshot::OutWord> = Vec::new();
                                for x in tables.iter() {
                                    if let Some(f) = x.table.as_ref() {
                                        for (seat, app, hand_id, cert) in x.out_words.iter() {
                                            out.push(super::snapshot::OutWord {
                                                table_id: f.table_id(),
                                                app_key: *app,
                                                seat: *seat,
                                                hand_id: *hand_id,
                                                cert: minicbor::bytes::ByteVec::from(cert.clone()),
                                            });
                                        }
                                    }
                                }
                                out.truncate(super::snapshot::SNAPSHOT_MAX_OUT);
                                if let Ok(bytes) = super::snapshot::tell_out(&app_key, ask.nonce, adverts, false, out, now) {
                                    let _ = swarm.behaviour_mut().snapshot.send_response(channel, bytes);
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "lobby: answered {peer} with {offered} table(s) offered"
                                        )))
                                        .await;
                                }
                            }
                            request_response::Message::Response { request_id, response } => {
                                let Some((nonce, asked)) = snapshot_pending.remove(&request_id) else {
                                    continue;
                                };
                                if asked != peer {
                                    continue;
                                }
                                let (adverts, answered_at, out_words) = match super::snapshot::open_tell_out(&response, &nonce, now) {
                                    Ok(a) => a,
                                    Err(e) => {
                                        let _ = events
                                            .send(NodeEvent::Warning(format!(
                                                "the lobby answer from {peer} could not be read: {e:?}"
                                            )))
                                            .await;
                                        continue;
                                    }
                                };
                                // `D-047`: the table's word about this client's own seat,
                                // verified from the certificate's bytes against the roster --
                                // not the answerer's authority. Said once; a headless client
                                // leaves, a window holds the table for its player to close.
                                for w in out_words.iter().filter(|w| w.app_key == my_app_key) {
                                    for i in 0..tables.len() {
                                        let verdict: Option<String> = (|| {
                                            let t = &tables[i];
                                            let f = t.table.as_ref()?;
                                            if t.out_told || f.table_id() != w.table_id || f.my_seat() != Some(w.seat) {
                                                return None;
                                            }
                                            let roster = f.roster().clone();
                                            let (subjects, voters) = crate::table::hand::certificate_names(
                                                &w.cert,
                                                &w.table_id,
                                                w.hand_id,
                                                &|k| roster.seat_of(k),
                                            )?;
                                            if !subjects.contains(&w.seat)
                                                || voters.contains(&w.seat)
                                                || voters.len() < 2
                                                || voters.len() <= subjects.len()
                                            {
                                                return None;
                                            }
                                            Some(format!(
                                                "seat {} -- this client -- is out of the table for good after its fourth absence (D-047): the table's word, certified by seats {:?} at hand #{}, carried in the lobby answer of {peer}",
                                                w.seat, voters, w.hand_id
                                            ))
                                        })();
                                        if let Some(why) = verdict {
                                            let t = &mut tables[i];
                                            t.out_told = true;
                                            let _ = events.send(NodeEvent::Warning(why.clone())).await;
                                            let _ = events
                                                .send(NodeEvent::OutForGood { key: w.table_id, why: why.clone() })
                                                .await;
                                            if autoplay.is_some() {
                                                leave_table_now!(t, why);
                                            }
                                        }
                                    }
                                }
                                // **The responder is not trusted for anything** (§7.5):
                                // every advert goes through the checklist an advert heard
                                // over gossip goes through.
                                let before: std::collections::HashSet<[u8; 32]> =
                                    state.lobby.tables().map(|l| *l.key).collect();
                                let from = peer_bytes(&peer);
                                let mut named: Vec<[u8; 32]> = Vec::new();
                                for bytes in &adverts {
                                    match advert::receive(bytes, from, now, &mut state.limits, &mut state.lobby) {
                                        Ok(key) => {
                                            named.push(key);
                                            if let Some(held) = state.lobby.get(&key) {
                                                let name = held.ad.table_name.clone();
                                                let _ = events
                                                    .send(NodeEvent::TableSeen {
                                                        key,
                                                        ad: Box::new(held.ad.clone()),
                                                        params_hash: held.params_hash,
                                                        advert_hash: held.advert_hash,
                                                    })
                                                    .await;
                                                if !before.contains(&key) {
                                                    let _ = events
                                                        .send(NodeEvent::Warning(format!(
                                                            "lobby: table {} ({name}) heard by asking {peer}",
                                                            short_hash(&key)
                                                        )))
                                                        .await;
                                                }
                                            }
                                        }
                                        // Named all the same: a copy the store already holds
                                        // (`NotNewer`, the re-advert that came over the mesh a
                                        // moment earlier) or one this client's own limit refused
                                        // is still the founder saying the table is open. Not
                                        // counting it withdrew every table at the first tick
                                        // after its re-advert (`run192317-3`, both watchers at
                                        // 30.0 s).
                                        Err(advert::NotAccepted::NotTaken(_)) | Err(advert::NotAccepted::RateLimited) => {
                                            if let Some(key) = advert::table_key_of(bytes) {
                                                named.push(key);
                                            }
                                        }
                                        Err(e) => {
                                            let _ = events
                                                .send(NodeEvent::Warning(format!(
                                                    "an answer from {peer} carried an advert this client refused: {e:?}"
                                                )))
                                                .await;
                                        }
                                    }
                                }
                                // The other half of the answer: a founder that no longer
                                // names a table this client lists from it has withdrawn it.
                                // Only a table advertised BEFORE the answer, on the founder's
                                // own clock: an answer older than the advert was made before
                                // the table was set up and says nothing about it
                                // (`run192317-3`: the first question beat the hosting by a
                                // second, and its empty answer withdrew the table for 29 s).
                                let pb = peer.to_bytes();
                                answers.insert(pb.clone(), (named.clone(), answered_at));
                                let gone: Vec<([u8; 32], String)> = state
                                    .lobby
                                    .tables()
                                    .filter(|l| {
                                        l.held.ad.founder_peer_id == pb
                                            && !named.contains(l.key)
                                            && l.held.ad.timestamp_unix_ms < answered_at
                                    })
                                    .map(|l| (*l.key, l.held.ad.table_name.clone()))
                                    .collect();
                                for (key, name) in gone {
                                    state.lobby.remove(&key);
                                    let _ = events
                                        .send(NodeEvent::TableGone {
                                            key,
                                            why: "its founder no longer offers it".into(),
                                        })
                                        .await;
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "lobby: table {} ({name}) withdrawn: its founder {peer} no longer offers it",
                                            short_hash(&key)
                                        )))
                                        .await;
                                }
                            }
                        }
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Snapshot(
                        request_response::Event::OutboundFailure { request_id, .. },
                    )) => {
                        // Asked again at the next tick; nothing to say.
                        snapshot_pending.remove(&request_id);
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Join(
                        request_response::Event::Message { peer, message, .. },
                    )) => {
                        let now = super::node::now_unix_ms();
                        match message {
                            request_response::Message::Request { request, channel, .. } => {
                                // `peer` is what the **transport** authenticated,
                                // and it is what §4.3 compares the request's own
                                // claim against. The whole of `U17` — one node,
                                // one seat — rests on using this and not the
                                // value in the payload.
                                let authenticated = peer.to_bytes();
                                let Some(f) = t.table.as_mut() else {
                                    // No table here to join. Saying nothing is
                                    // right: an answer would confirm that this
                                    // client is a founder of something.
                                    continue;
                                };
                                // **A seat asking to join a table it already
                                // sits at has restarted**, and this is the only
                                // signal that says so. A client that is playing
                                // does not ask; one that asks has lost
                                // everything it held, its place in the table's
                                // Tox group among it — and nothing else notices,
                                // because `invited` records what the founder did
                                // rather than who is there, toxcore's friend
                                // connection outlives a short outage so no
                                // down-edge clears it, and a dead peer still
                                // resolves in the group's peer list so the group
                                // does not read as short.
                                //
                                // Read **before** the request is answered,
                                // because answering it is what changes the
                                // roster in every other case.
                                let returning = f
                                    .roster()
                                    .seats()
                                    .iter()
                                    .find(|e| e.peer_id == authenticated)
                                    .and_then(|e| e.tox_key);
                                if let Some(k) = returning {
                                    t.tox_sink.tell(super::toxsink::Seat::Back(k));
                                }
                                // `D-047`: a seat out of this table for good asks again --
                                // refused with the reason, whatever else the request says.
                                if let Ok((_, sender, _)) = super::joinwire::receive_join_request(&request) {
                                    if t.out_keys.contains(&sender) {
                                        if let Ok(bytes) = f.refuse_out(&request, now) {
                                            let _ = swarm.behaviour_mut().join.send_response(channel, bytes);
                                        }
                                        let _ = events
                                            .send(NodeEvent::Warning(
                                                "a seat out of this table for good asked to sit again and was refused (D-047)".into(),
                                            ))
                                            .await;
                                        continue;
                                    }
                                }
                                match f.on_join_request(&request, &authenticated, t.ever_dealt, now) {
                                    Ok(sends) => {
                                        // The channel is consumed by the one
                                        // reply and the rest of the sends go to
                                        // the table's topic. An earlier version
                                        // stopped at the reply, which threw away
                                        // the roster announcement and this
                                        // client's own ratification — the joiner
                                        // took its seat and then both sides sat
                                        // waiting for a message that had been
                                        // dropped on the floor here.
                                        let mut channel = Some(channel);
                                        for send in sends {
                                            match send {
                                                Send::Reply(bytes) => {
                                                    if let Some(c) = channel.take() {
                                                        let _ = swarm
                                                            .behaviour_mut()
                                                            .join
                                                            .send_response(c, bytes);
                                                    }
                                                }
                                                Send::Broadcast(bytes) => {
                                                    if let Some(t) = &t.table_topic {
                                                        let _ = swarm
                                                            .behaviour_mut()
                                                            .gossipsub
                                                            .publish(t.clone(), bytes);
                                                    }
                                                }
                                            }
                                        }
                                        report_roster(&events, f).await;
                                seat_on_tox(f, &t.tox_sink);
                                        if let Some(session) = f.session() {
                                            let _ = events.send(NodeEvent::TableReal {
                                                key: f.table_id(),
                                                session,
                                            }).await;
                                            // Only once every seat is taken.
                                            // A table that is still filling
                                            // must stay as findable as it was.
                                            t.table_closed = table_is_closed(f, &mut t.tournament_started);
                                            dht_effort(&mut swarm, t.table_closed);
                                            // The opening first, and the flag
                                            // only if there was one. Setting
                                            // the flag before asking closed the
                                            // guard for ever on a peer whose
                                            // roster had not ratified at that
                                            // instant, and that peer then never
                                            // dealt at all - which is exactly
                                            // what a three-seat run showed:
                                            // the table formed and one seat's
                                            // HAND_INIT never came.
                                            if !t.ever_dealt
                                                && !t.resuming && hand_one_may_open(
                                                    &t.tox_sink,
                                                    &mut t.hand_one_held_since,
                                                    &mut t.hand_one_progress,
                                                    &mut t.hand_one_forced_said,
                                                    &events,
                                                )
                                                .await
                                            {
                                                say_why_no_hand_one(f, &mut t.why_no_hand_one_said, &events).await;
                                                if let Some(o) = opening_for_hand_one(f) {
                                                    t.ever_dealt = true;
                                                    begin_hand(
                                                        o,
                                                        &app_key,
                                                        &mut t.hand,
                                                                                            &mut t.said,
                                                        &mut swarm,
                                                        &events,
                                                        &t.tox_sink,
                                                    )
                                                    .await;
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        // A structurally invalid request gets no
                                        // answer at all, which is what
                                        // `Formation` returns an error for.
                                        let _ = events.send(NodeEvent::Warning(
                                            format!("a join request was dropped: {e:?}"))).await;
                                    }
                                }
                            }
                            request_response::Message::Response { response, .. } => {
                                let Some(f) = t.table.as_mut() else { continue };
                                match f.on_join_answer(&response, now) {
                                    Ok(_) => {
                                        if let Some(seat) = f.my_seat() {
                                            let _ = events.send(NodeEvent::Seated {
                                                key: f.table_id(),
                                                seat,
                                            }).await;
                                        }
                                        report_params(&events, f).await;
                                        report_roster(&events, f).await;
                                seat_on_tox(f, &t.tox_sink);
                                    }
                                    // **`AlreadySeated` is not a refusal like the
                                    // others, and treating it like one is what
                                    // stopped a disconnected player ever coming
                                    // back.**
                                    //
                                    // Every other reason means *you are not at
                                    // this table*. This one means *you are
                                    // already at it* — the founder is holding
                                    // the seat and, since `S1-J`, answering with
                                    // the roster. Throwing the table away and
                                    // clearing the sink discards the Formation
                                    // and **destroys the Tox driver**, so the
                                    // client asks again thirty seconds later,
                                    // gets the same answer, and destroys it
                                    // again. Measured across seven runs: the
                                    // returning peer never entered the group,
                                    // never held a table, and the founder sat at
                                    // `tox friends up 2` of three while it sent
                                    // twenty-one invitations to the two it could
                                    // reach.
                                    //
                                    // Four other defects were found and fixed
                                    // looking for this one. Each was real; none
                                    // was the cause. What found it was asking
                                    // the returning client what it held, and
                                    // getting no answer at all.
                                    //
                                    // So: keep the table, keep the group, and
                                    // wait for the roster that is on its way.
                                    Err(Failed::Refused { reason, .. })
                                        if reason
                                            == crate::table::join::RejectReason::AlreadySeated
                                                .code() =>
                                    {
                                        let _ = events
                                            .send(NodeEvent::Warning(
                                                "this table already holds a seat for us; waiting for its roster"
                                                    .into(),
                                            ))
                                            .await;
                                    }
                                    // **A superseded acceptance is not a
                                    // failed table.** `admit_accept` refuses an
                                    // answer whose `request_hash` is not the one
                                    // this client is waiting for
                                    // (`table/join.rs:331-333`), which is
                                    // exactly what a *second* join request makes
                                    // of the founder's honest answer to the
                                    // first. Treating it as a teardown destroyed
                                    // a table that had already formed.
                                    //
                                    // Measured, `split215815-2`: the joiner
                                    // asked to join at 63.6 s and again at
                                    // 64.7 s, was seated over the table topic at
                                    // 64.9 s — *“2 seated … the table is set:
                                    // session af7beef3”* — and 0.0 s later the
                                    // late answer to the first request arrived
                                    // as `Accept(WrongRequest)` and took the
                                    // whole table with it. The seat held no
                                    // `Formation` for the next 193 seconds, and
                                    // when one was rebuilt it re-signed its own
                                    // `TABLE_READY` at a fresh clock, so its
                                    // `session_id` no longer matched the
                                    // founder's and neither side could ever
                                    // accept the other's — `take_ratification`
                                    // is first-copy-wins by design.
                                    //
                                    // Ignored like `AlreadySeated` above: the
                                    // request it answers is gone, and the table
                                    // this client actually holds was not built
                                    // by that answer.
                                    Err(Failed::Accept(
                                        crate::table::join::AcceptRefused::WrongRequest,
                                    )) => {
                                        let _ = events
                                            .send(NodeEvent::Warning(
                                                "an acceptance for a join request this client has already replaced; the table it holds is untouched"
                                                    .into(),
                                            ))
                                            .await;
                                    }
                                    Err(Failed::Refused { reason, .. }) => {
                                        t.table = None;
                                        // The Tox group goes with the table. Dropping the handle
                                        // tells the driver to leave and joins its thread, which
                                        // flushes what it still holds - the last message of a hand
                                        // is exactly what sits in that queue.
                                        t.tox_sink.clear();
                            t.tox_group_said = false;
                            t.table_announces = 0;
                                        if let Some(t) = t.table_topic.take() {
                                            let _ = swarm.behaviour_mut().gossipsub.unsubscribe(&t);
                                        }
                                        let _ = events
                                            .send(NodeEvent::JoinRefused { reason })
                                            .await;
                                        // `S1-CR`: the founder answered, and not with *already
                                        // seated*: the session this record names is gone.
                                        if t.resuming
                                            && reason != crate::table::join::RejectReason::AlreadySeated.code()
                                        {
                                            let _ = crate::storage::session::forget(&profile_dir);
                                            t.resume = None;
                                            t.resuming = false;
                                            let _ = events
                                                .send(NodeEvent::SessionGaveUp {
                                                    why: format!("the founder refused the rejoin (reason {reason})"),
                                                })
                                                .await;
                                        }
                                    }
                                    Err(e) => {
                                        t.table = None;
                        leave_the_table!(t);
                        dht_effort(&mut swarm, false);
                        let _ = events.send(NodeEvent::LeftTable {
                                            why: format!("the acceptance did not hold: {e:?}"),
                                        }).await;
                                    }
                                }
                            }
                        }
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Join(
                        request_response::Event::OutboundFailure { error, .. },
                    )) => {
                        // A join that goes unanswered has to say so. Silence
                        // here is a player looking at a button that appears to
                        // have done nothing.
                        t.table = None;
                        // The Tox group goes with the table. Dropping the handle
                        // tells the driver to leave and joins its thread, which
                        // flushes what it still holds - the last message of a hand
                        // is exactly what sits in that queue.
                        t.tox_sink.clear();
                            t.tox_group_said = false;
                            t.table_announces = 0;
                        if let Some(t) = t.table_topic.take() {
                            let _ = swarm.behaviour_mut().gossipsub.unsubscribe(&t);
                        }
                        leave_the_table!(t);
                        dht_effort(&mut swarm, false);
                        let _ = events.send(NodeEvent::LeftTable {
                            why: format!("the founder did not answer: {error}"),
                        }).await;
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                        gossipsub::Event::Message {
                            message,
                            propagation_source,
                            message_id,
                        },
                    )) if t.table_topic
                        .as_ref()
                        .is_some_and(|t| message.topic == t.hash()) =>
                    {
                        // The table mesh. Everything on it is either the
                        // founder's proposal or a seat's ratification of one,
                        // and which it is is decided by what it decodes as
                        // rather than by what was hoped for.
                        //
                        // **This arm must answer, exactly like the lobby's.**
                        // With `validate_messages()` set, GossipSub forwards
                        // nothing until the application reports a verdict, and
                        // this arm reported none — so every client was a black
                        // hole for its own table's traffic. Two seats did not
                        // notice, because they are directly meshed and delivery
                        // to a direct peer does not need forwarding; at ten
                        // seats a ratification that has to travel through a
                        // third seat simply never arrives, and the table never
                        // starts with nobody able to say why.
                        // No table here to judge it against. `Ignore` and not
                        // `Reject`: nothing about the message is known to be
                        // wrong, this client just cannot tell, and blaming a
                        // sender for that would cost an honest peer its score.
                        //
                        // Written as a guard rather than as a `match` yielding a
                        // verdict, because the `Some` arm's verdict was read by
                        // nothing — and a verdict computed and not reported is
                        // exactly the bug this whole path was repaired for.
                        if t.table.is_none() {
                            let _ = swarm.behaviour_mut().gossipsub
                                .report_message_validation_result(
                                    &message_id,
                                    &propagation_source,
                                    gossipsub::MessageAcceptance::Ignore,
                                );
                            continue;
                        }
                        // `S1-CS`: a line of table chat, from a seated key, to the
                        // window and nowhere else. Accepted for the mesh when it
                        // verified, ignored when the seat's budget is spent, refused
                        // when it is no seat's or another table's.
                        if let Ok((crate::protocol::messages::EventType::TableChat, _, _)) =
                            crate::net::chained::peek(&message.data, LOBBY_MSG_MAX.max(TABLE_FRAME_PEEK))
                        {
                            let now = super::node::now_unix_ms();
                            let verdict = match t.table.as_ref() {
                                Some(f) => match super::tabletalk::receive(
                                    &message.data,
                                    &f.table_id(),
                                    |k| f.roster().seat_of(k),
                                    now,
                                    &mut t.chat_limits,
                                ) {
                                    Ok(spoken) => {
                                        let _ = events
                                            .send(NodeEvent::TableSaid {
                                                seat: spoken.seat,
                                                nickname: spoken.nickname,
                                                text: spoken.text,
                                            })
                                            .await;
                                        gossipsub::MessageAcceptance::Accept
                                    }
                                    Err(super::tabletalk::NotHeard::TooMuch) => gossipsub::MessageAcceptance::Ignore,
                                    Err(_) => gossipsub::MessageAcceptance::Reject,
                                },
                                None => gossipsub::MessageAcceptance::Ignore,
                            };
                            let _ = swarm.behaviour_mut().gossipsub.report_message_validation_result(
                                &message_id,
                                &propagation_source,
                                verdict,
                            );
                            continue;
                        }
                        // The hand first, if there is one. A `HAND_INIT`
                        // decodes as neither a player list nor a ratification,
                        // so without this it would fall through to the
                        // formation and be refused as malformed.
                        if let Some(h) = t.hand.as_mut() {
                            // The hand, through the shared handling. The macro is
                            // defined above the loop and this is one of its two call
                            // sites; the other is the Tox group's. See there for why it
                            // is a macro, and for what the verdict is.
                            // **A checkpoint-8 `STATE_HASH` of a hand this
                            // client has finished is taken here and not by the
                            // hand**, which owns only its own stages and would
                            // refuse it for naming a position it has left. It
                            // is accepted for the mesh either way: it verified,
                            // and it is worth forwarding whether or not it
                            // agreed.
                            if link_is_down() {
                                continue;
                            }
                            cross_boundary_at_t47(h, &mut t.boundaries, &mut t.crossed_for);
                            let verdict: Option<gossipsub::MessageAcceptance> =
                                match checkpoint_event(
                                    &message.data,
                                    h,
                                    &mut t.boundaries,
                                    &mut t.readmitted,
                                    &mut t.sit_ins,
                                    &app_key,
                                    &mut t.checkpoint_said,
                                    &mut t.frozen,
                                    &t.roster_seats,
                                    &mut t.no_round_said,
                                    &mut t.early_checkpoints,
                                    &profile_dir,
                                    &events,
                                )
                                .await
                                {
                                    // A dispute is unchained and out of stage,
                                    // so the hand would refuse it; it is taken
                                    // here instead. Before the freeze test,
                                    // because a dispute is one of the things
                                    // that may **cause** the freeze.
                                    None if dispute_event(
                                        &message.data,
                                        h,
                                        &mut t.boundaries,
                                        &mut t.frozen,
                                        &mut t.disputes_seen,
                                        &events,
                                    )
                                    .await =>
                                    {
                                        Some(gossipsub::MessageAcceptance::Accept)
                                    }
                                    // **§6.3 step 1: no hand event is accepted
                                    // while frozen.** Ignored rather than
                                    // rejected — the sender is not at fault and
                                    // its message is not invalid; this receiver
                                    // has stopped, which is a different thing
                                    // and must not cost anybody peer score.
                                    None if t.frozen.is_some() => {
                                        Some(gossipsub::MessageAcceptance::Ignore)
                                    }
                                    // §4.10's hand boundary window, for the same
                                    // reason the checkpoint is taken above: it
                                    // belongs to the hand that has just ended
                                    // and the live one would answer it with
                                    // `WrongType` (`S1-BZ`).
                                    //
                                    // **Below the freeze, and a dispute is
                                    // above it.** A boundary event is a chained
                                    // event of a hand, so §6.3 step 1 covers it
                                    // like any other; a dispute is what may
                                    // *cause* the freeze and cannot be behind
                                    // it. A frozen table deals no further hand,
                                    // so there is nothing for a readmission to
                                    // be readmitted to.
                                    None if boundary_event(
                                        &message.data,
                                        h,
                                        &mut t.boundaries,
                                        &mut t.readmitted,
                                        &mut t.sit_ins,
                                        &mut t.early_boundary,
                                        &events,
                                    )
                                    .await =>
                                    {
                                        Some(gossipsub::MessageAcceptance::Accept)
                                    }
                                    None => hand_event!(t, h, &message.data),
                                    Some(out) => {
                                        if !out.is_empty() {
                                            publish_and_hear(
                                                out,
                                                h,
                                                &mut t.boundaries,
                                                &mut t.readmitted,
                                                &mut t.sit_ins,
                                                &app_key,
                                                &mut t.checkpoint_said,
                                                &mut t.frozen,
                                                &t.roster_seats,
                                                &mut t.no_round_said,
                                                &mut t.early_checkpoints,
                                                &profile_dir,
                                                &events,
                                                &mut swarm,
                                                &mut t.said,
                                                &t.tox_sink,
                                            )
                                            .await;
                                        }
                                        Some(gossipsub::MessageAcceptance::Accept)
                                    }
                                };
                            // The single exit. A hand event never leaves this
                            // branch without the mesh being told what became of
                            // it.
                            if let Some(v) = verdict {
                                let _ = swarm
                                    .behaviour_mut()
                                    .gossipsub
                                    .report_message_validation_result(
                                        &message_id,
                                        &propagation_source,
                                        v,
                                    );
                                continue;
                            }
                        }

                        // Nothing arrives while the link is down.
                        if link_is_down() {
                            continue;
                        }
                        if t.resuming && t.hand.is_none() {
                            let _ = stash_for_resume(&message.data, &mut t.resume_inits, &mut t.resume_early);
                        }
                        let Some(f) = t.table.as_mut() else { continue };
                        let now = super::node::now_unix_ms();
                        let result = if joinwire::receive_player_list(&message.data).is_ok() {
                            f.on_player_list(&message.data, now)
                        } else {
                            f.on_table_ready(&message.data)
                        };
                        let refused = result.is_err();
                        match result {
                            Ok(sends) => {
                                if let Some(t) = &t.table_topic {
                                    for send in sends {
                                        if let Send::Broadcast(bytes) = send {
                                            let _ = swarm
                                                .behaviour_mut()
                                                .gossipsub
                                                .publish(t.clone(), bytes);
                                        }
                                    }
                                }
                                report_roster(&events, f).await;
                                seat_on_tox(f, &t.tox_sink);
                                if let Some(session) = f.session() {
                                    let _ = events
                                        .send(NodeEvent::TableReal {
                                            key: f.table_id(),
                                            session,
                                        })
                                        .await;
                                    // The joiner's road to the same place. Both
                                    // sites start the hand, and `begin_hand` is
                                    // idempotent, because this one fires again
                                    // on every later table message.
                                    // See the note at the other road in.
                                    if !t.ever_dealt
                                        && !t.resuming && hand_one_may_open(
                                            &t.tox_sink,
                                            &mut t.hand_one_held_since,
                                            &mut t.hand_one_progress,
                                            &mut t.hand_one_forced_said,
                                            &events,
                                        )
                                        .await
                                    {
                                        say_why_no_hand_one(f, &mut t.why_no_hand_one_said, &events).await;
                                        if let Some(o) = opening_for_hand_one(f) {
                                            t.ever_dealt = true;
                                            begin_hand(
                                                o,
                                                &app_key,
                                                &mut t.hand,
                                                                            &mut t.said,
                                                &mut swarm,
                                                &events,
                                                &t.tox_sink,
                                            )
                                            .await;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "a table message was refused: {e:?}"
                                    )))
                                    .await;
                            }
                        }

                        // A signed message that failed a rule is the sender's
                        // fault and costs it peer score; one this client took is
                        // passed on.
                        let _ = swarm.behaviour_mut().gossipsub
                            .report_message_validation_result(
                                &message_id,
                                &propagation_source,
                                if refused {
                                    gossipsub::MessageAcceptance::Reject
                                } else {
                                    gossipsub::MessageAcceptance::Accept
                                },
                            );
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Dcutr(ev)) => {
                        match ev.result {
                            Ok(_) => {
                                relayed_peers.remove(&ev.remote_peer_id);
                                let _ = events
                                    .send(NodeEvent::HolePunched(ev.remote_peer_id))
                                    .await;
                            }
                            Err(_) => {
                                relayed_peers.insert(ev.remote_peer_id);
                                let _ = events
                                    .send(NodeEvent::StillRelayed(ev.remote_peer_id))
                                    .await;
                            }
                        }
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::AutonatClient(ev)) => {
                        // AutoNAT is the only thing that may decide this. A
                        // setting cannot: a user who is wrong about their own NAT
                        // would advertise a way through that is not one.
                        // ...and the address it confirmed has to be one the
                        // rest of the world could dial. `NAT_AND_DISCOVERY.md`
                        // §2.4(ii) calls this mitigation mandatory and upstream
                        // does not do it: there is no private-range check
                        // anywhere in `libp2p-autonat`'s v2 client. Two clients
                        // behind one router - a laptop and a virtual machine on
                        // it, which is how this gets tested - dial each other
                        // successfully on `192.168.x`, and without this guard
                        // both would conclude they are publicly reachable, both
                        // would volunteer as relays, and both would advertise a
                        // port that nobody outside the house can open.
                        let public = ev.result.is_ok() && reachable(&ev.tested_addr);

                        // A confirmed address is an **external** address, and
                        // saying so is not bookkeeping.
                        //
                        // `libp2p-relay` fills a reservation from the relay's
                        // external addresses and from nowhere else. A node that
                        // never records one volunteers as a relay, accepts the
                        // reservation, and hands back a list of no addresses —
                        // so the peer that reserved has a circuit it cannot tell
                        // anyone about, and the reservation is worse than a
                        // refusal because it looks like it worked. That is what
                        // `tests/relay_circuit.rs` found on the first run it was
                        // ever given, and it had been true since the relay was
                        // configured.
                        if public {
                            swarm.add_external_address(ev.tested_addr.clone());
                        } else {
                            swarm.remove_external_address(&ev.tested_addr);
                        }

                        if public != state.is_public() {
                            state.set_public(public);
                            let _ = events.send(NodeEvent::Reachability { public }).await;
                        }
                    }
                    // A relay that advertises the hop protocol and then says no.
                    // Reported rather than swallowed: measured against the four
                    // public `bootstrap.libp2p.io` nodes, all four advertise it
                    // and all four refuse, and a client that hid that would look
                    // identical to one whose request never went out.
                    SwarmEvent::ListenerClosed { reason: Err(e), addresses, .. } => {
                        // A circuit listener that closes is a reservation that
                        // is gone - refused now, or expired later. Either way
                        // the flag has to come back down, or one bad minute
                        // leaves this client believing it has a way in for the
                        // rest of the process and never looking for another.
                        if addresses.iter().any(|a| {
                            a.iter()
                                .any(|p| matches!(p, libp2p::multiaddr::Protocol::P2pCircuit))
                        }) {
                            have_reservation = false;
                        }
                        let _ = events
                            .send(NodeEvent::Warning(format!("relay said no: {e}")))
                            .await;
                    }
                    // A dial refused by this client's own limit, rather than
                    // by the far end. That is the cap being too tight for what
                    // this client is actually doing, so it gets room - up to a
                    // ceiling, because "raise it whenever anything is refused"
                    // is not a cap at all.
                    SwarmEvent::OutgoingConnectionError {
                        error: libp2p::swarm::DialError::Denied { .. },
                        connection_id,
                        ..
                    } if budget < CONNECTION_CEILING => {
                        lobby_dials.remove(&connection_id);
                        {
                            let raised = (budget + budget / 4).min(CONNECTION_CEILING);
                            budget = raised;
                            *swarm.behaviour_mut().conn_limits.limits_mut() =
                                super::swarm::connection_limits(raised);
                            comfortable = 0;
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "connection budget raised to {raised}"
                                )))
                                .await;
                        }
                    }
                    SwarmEvent::OutgoingConnectionError { error, connection_id, .. } => {
                        // **A second look, for this site's own dial and for a
                        // failure that is the far end's.** `Denied` never
                        // reaches here while the budget is below its ceiling —
                        // and 637 of the 644 logs on disk reach that ceiling, at a
                        // median of 24.4 s, so it very much reaches here after
                        // that. It is this client's own limiter refusing a
                        // connection that was already established, which makes
                        // the peer provably alive and the failure ours.
                        if let Some(peer) = lobby_dials.remove(&connection_id) {
                            if earns_a_fast_look(
                                the_failure_mends_itself(&error),
                                fast_budget,
                                started.elapsed() < FAST_REDIAL_WINDOW,
                            ) {
                                fast_budget = fast_budget.saturating_sub(1);
                                fast_lane.insert(peer);
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "a second look at {peer} in {}s after a failed dial ({fast_budget} left)",
                                        FAST_REDIAL_AFTER.as_secs()
                                    )))
                                    .await;
                            }
                        }
                        let _ = events
                            .send(NodeEvent::DialFailed { reason: error.to_string() })
                            .await;
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Identify(
                        libp2p::identify::Event::Received { peer_id, info, .. },
                    )) => {
                        // Identify is how a relay is recognised, rather than by
                        // being on a list. The hop protocol's name is a
                        // compile-time constant in `libp2p-relay` and is not
                        // namespaced per application, so a public node that
                        // offers it will serve this client the same as any
                        // other — which is what makes a list of relay addresses
                        // unnecessary (§3.3) even while an entry point is not.
                        // A peer on the public DHT is a way into it. Its
                        // addresses go into the public routing table so that
                        // `get_providers` has somewhere to start; without this
                        // the second Kademlia knows nobody and every query
                        // fails before it leaves the process.
                        if info
                            .protocols
                            .iter()
                            .any(|p| p.as_ref() == "/ipfs/kad/1.0.0")
                        {
                            for a in &info.listen_addrs {
                                if reachable(a) {
                                    swarm
                                        .behaviour_mut()
                                        .ipfs_kad
                                        .add_address(&peer_id, a.clone());
                                }
                            }
                            if !asked_public_dht {
                                // The first way in. A routing table with four
                                // entries can start a query but finishes very
                                // few, so the walk outwards comes first and the
                                // questions follow on the discovery timer.
                                asked_public_dht = true;
                                if let Err(e) = swarm.behaviour_mut().ipfs_kad.bootstrap() {
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "public DHT has no peers yet: {e}"
                                        )))
                                        .await;
                                }
                            }
                        }

                        // Another one of us, or a stranger on the same DHT?
                        if info.protocol_version == super::swarm::IDENTIFY_PROTOCOL
                            && poker_peers.insert(peer_id)
                        {
                            // Never subject to the cap. The whole point of a
                            // cap is to keep strangers from crowding out the
                            // people this client is here for, and a cap that
                            // could refuse a player would be doing the opposite.
                            swarm.behaviour_mut().conn_limits.bypass_peer_id(&peer_id);
                            // **Say again what topics this client is on.**
                            // GossipSub exchanges subscriptions once, when a
                            // connection is established, and measured on three
                            // clients on one machine that exchange is
                            // unreliable: the founder had both joiners
                            // connected and identified and knew of **no** peer
                            // subscribed to the lobby, so `publish` answered
                            // `NoPeersSubscribedToTopic` and the table never
                            // formed. Each joiner meanwhile saw exactly one of
                            // its two neighbours. A table formed in 33 s when
                            // the founder saw both and never when it did not,
                            // across nine runs.
                            //
                            // There is no per-peer "send my subscriptions"
                            // call, so this is the primitive that exists:
                            // dropping and retaking the topic re-announces it
                            // to everyone connected.
                            //
                            // **Only when this peer is not already known to be
                            // on the topic**, because the re-announce is not
                            // free: peers that receive the `UNSUBSCRIBE` prune
                            // this client from their mesh for that topic and
                            // have to re-graft it on a later heartbeat. Where
                            // the ordinary exchange worked — which is most of
                            // the time — there is nothing to repair and this
                            // does nothing.
                            // **Every topic this client holds, and the
                            // table's is one of them.** It checked the lobby
                            // hash alone, so a peer subscribed to the lobby and
                            // not to the table read as "already fine" and
                            // nothing was re-announced — and the table's topic
                            // is the one `TABLE_READY` travels on.
                            //
                            // Measured: a founder connected to both joiners,
                            // seated both over the join RPC (which is
                            // request-response and unaffected), and never
                            // received either ratification. `lobby topic: 0 of
                            // 2 subscribed` was in its own log, twice, next to
                            // two direct connections. The other two formed the
                            // table without it and played seventeen hands as
                            // seats [1, 2].
                            if !peer_has_our_topics(&swarm, &peer_id, &topics, t.table_topic.as_ref())
                            {
                                let mut which: Vec<&gossipsub::IdentTopic> =
                                    vec![&topics.lobby, &topics.lobby_chat];
                                which.extend(t.table_topic.as_ref());
                                announce_topics(&mut swarm, &which);
                            }
                            let _ = events
                                .send(NodeEvent::PokerPeer { peer: peer_id, gone: false })
                                .await;
                            // `D-040`: and ask it what it offers, now.
                            ask_lobby!(peer_id);
                        }

                        let relays = info.protocols.contains(&libp2p::relay::HOP_PROTOCOL_NAME);
                        seen_a_relay |= relays;
                        if relays
                            && !have_reservation
                            && !asked_hops
                                .get(&peer_id)
                                .is_some_and(|at| at.elapsed() < REDIAL_AFTER)
                        {
                            // Their own address, with their identity and the
                            // circuit suffix. Listening on that IS the
                            // reservation request; there is no separate call.
                            //
                            // Only a globally routable address: a relay
                            // advertising `192.168.x` is advertising its own
                            // network, and a reservation taken over an address
                            // this client cannot reach from outside would be a
                            // reservation nobody could use to reach it.
                            // Marked only once there is something to ask with.
                            // Identify arrives more than once and the first one
                            // often carries nothing but private addresses; a
                            // peer written off then would never be asked again,
                            // however reachable it turned out to be a second
                            // later.
                            if let Some(addr) = info.listen_addrs.iter().find(|a| reachable(a)) {
                                asked_hops.insert(peer_id, std::time::Instant::now());
                                let circuit = addr
                                    .clone()
                                    .with(libp2p::multiaddr::Protocol::P2p(peer_id))
                                    .with(libp2p::multiaddr::Protocol::P2pCircuit);
                                if let Err(e) = swarm.listen_on(circuit) {
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "no reservation from {peer_id}: {e}"
                                        )))
                                        .await;
                                }
                            }
                        }
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::IpfsKad(
                        libp2p::kad::Event::OutboundQueryProgressed { result, .. },
                    )) => {
                        use libp2p::kad::{GetProvidersOk, QueryResult};
                        // A query that fails is worth a line. Without one, a
                        // routing table too thin to answer anything looks
                        // exactly like a lobby with nobody in it.
                        match &result {
                            QueryResult::GetProviders(Err(e)) => {
                                let _ = events
                                    .send(NodeEvent::Warning(format!("DHT lookup failed: {e}")))
                                    .await;
                            }
                            QueryResult::StartProviding(r) => {
                                let _ = events
                                    .send(NodeEvent::Warning(match r {
                                        Ok(_) => "announced in the public lobby".to_owned(),
                                        Err(e) => format!("could not announce: {e}"),
                                    }))
                                    .await;
                            }
                            QueryResult::Bootstrap(Err(e)) => {
                                let _ = events
                                    .send(NodeEvent::Warning(format!("DHT bootstrap: {e}")))
                                    .await;
                            }
                            _ => {}
                        }
                        if let QueryResult::GetProviders(Ok(
                            GetProvidersOk::FinishedWithNoAdditionalRecord { .. },
                        )) = result
                        {
                            // Said out loud. An answer of nobody and a question
                            // never asked look the same in a log that only
                            // reports findings, and the first is a working lobby
                            // with no players in it.
                            if !in_public_lobby {
                                let _ = events
                                    .send(NodeEvent::Warning(
                                        "public lobby: nobody else yet".into(),
                                    ))
                                    .await;
                            }
                        }
                        if let QueryResult::GetProviders(Ok(
                            GetProvidersOk::FoundProviders { key, providers, .. },
                        )) = result
                        {
                            // The same handler for both keys: a relay and a
                            // fellow player are both peers to be dialled, and
                            // what differs is only what is said about them.
                            let lobby = key == lobby_namespace();
                            let _ = events
                                .send(NodeEvent::Warning(if lobby {
                                    format!("{} player(s) in the public lobby", providers.len())
                                } else {
                                    format!("{} relay(s) advertised in the DHT", providers.len())
                                }))
                                .await;
                            // **A few, not all.** Every provider was dialled
                            // every cycle, and a lobby key accumulates records
                            // from clients that are long gone - a dozen dead
                            // test profiles among them. With connections capped
                            // the flood filled the budget with strangers and
                            // left no room for the peer at this client's own
                            // table, which is how a table stopped forming at
                            // all. Whoever is not reached this cycle is reached
                            // the next one.
                            //
                            // **And the sentence above was false until it was
                            // measured.** A peer was recorded as dialled
                            // *before* the per-cycle budget was checked, so
                            // whoever the budget skipped was marked as reached
                            // and `dialled_lobby` refused it for ever after.
                            // With 140 providers on the lobby key and
                            // `DIALS_PER_ANSWER` = 8, that is 132 peers an answer
                            // written off without one dial attempt.
                            //
                            // Measured, `split205429-2`: the founder found the
                            // joiner in the lobby at 27.5 s among 140 providers
                            // and never dialled it, found it four more times
                            // over the next two minutes and never dialled it
                            // again, and published its table advert nine times
                            // out of nine into `NoPeersSubscribedToTopic`. The
                            // joiner ended `NO TABLE` while both nodes held a
                            // relay reservation and each knew the other's peer
                            // id. It reads as a NAT or relay failure and is
                            // neither.
                            //
                            // The budget is checked first now, and only a peer
                            // actually dialled is recorded — so a peer past the
                            // budget stays unknown and the next cycle takes it,
                            // which is what the paragraph above always claimed.
                            // Two counts, not one: see `REDIALS_PER_ANSWER`.
                            let mut fresh = 0usize;
                            let mut again = 0usize;
                            let me = *swarm.local_peer_id();
                            for peer in providers {
                                // `Some(true)` for a first dial, `Some(false)`
                                // for a re-dial, `None` for a relay — which
                                // keeps no book at all.
                                let mut charge: Option<bool> = None;
                                if lobby {
                                    let _ = events.send(NodeEvent::LobbyPeer(peer)).await;
                                    // **This client provides the lobby key too,
                                    // so it finds itself.** Fifteen times in one
                                    // measured run, each spending a dial slot on
                                    // a connection that cannot succeed.
                                    if peer == me {
                                        // **And this is the announce
                                        // confirmation.** The record came back
                                        // from the network, so somebody stored
                                        // it. Nothing else in the client can say
                                        // that.
                                        in_public_lobby = true;
                                        continue;
                                    }
                                    // Recently tried, so not again yet. Not
                                    // *ever* again: see `REDIAL_AFTER`.
                                    // **The cooldown a provider is on, which
                                    // is the whole of `S1-CB`.** A provider
                                    // whose own dial the far end failed waits
                                    // ten seconds rather than the full cycle,
                                    // once, and only while the process's budget
                                    // lasts.
                                    let waits = if fast_lane.contains(&peer) {
                                        FAST_REDIAL_AFTER
                                    } else {
                                        REDIAL_AFTER
                                    };
                                    let first = match dialled_lobby.get(&peer) {
                                        Some(at) if at.elapsed() < waits => continue,
                                        Some(_) => false,
                                        None => true,
                                    };
                                    if first {
                                        if fresh >= DIALS_PER_ANSWER {
                                            continue;
                                        }
                                    } else if again >= REDIALS_PER_ANSWER {
                                        continue;
                                    }
                                    charge = Some(first);
                                    // **The cooldown is stamped here and the
                                    // budget is not.** A provider whose
                                    // addresses `Bogonless` emptied is refused
                                    // by `Swarm::dial` synchronously and never
                                    // reaches the network, so charging it a slot
                                    // spends discovery on nothing — but stamping
                                    // it stops the same non-dial being retried
                                    // on every one of the four hundred answers a
                                    // run receives.
                                    dialled_lobby.insert(peer, std::time::Instant::now());
                                    // **And the entitlement is spent with the
                                    // stamp, for the reason the stamp is made
                                    // here.** It used to be spent only when
                                    // `Swarm::dial` returned `Ok`, and the
                                    // three refusals it returns *by value* --
                                    // `NoAddresses`, `Denied`,
                                    // `DialPeerConditionFalse` -- restamped the
                                    // cooldown without spending it. A provider
                                    // whose addresses the bogon filter empties
                                    // would then hold its ten-second lane for
                                    // the rest of the run, long past
                                    // `FAST_REDIAL_WINDOW`, because the window
                                    // gates only where a lane is *granted*.
                                    fast_lane.remove(&peer);
                                    // **Bounded, but never at the cost of the
                                    // one peer that matters.** This used to
                                    // `clear()`, which on a lobby larger than
                                    // the cap forgets the live peer along with
                                    // the ghosts and restarts the whole crawl.
                                    // Keeping the peers this client is connected
                                    // to bounds the set just as well and cannot
                                    // evict a mesh peer; the rest are ghosts, and
                                    // a ghost re-dialled occasionally costs one
                                    // slot and fails. Never fired in a measured
                                    // run — peak 318 against a cap of 512 — so
                                    // this is a guard for a public lobby, not a
                                    // fix for an observed fault.
                                    if dialled_lobby.len() > 512 {
                                        dialled_lobby.retain(|p, _| swarm.is_connected(p));
                                    }
                                }
                                // By peer id: the addresses came with the query
                                // and live in the routing table, and asking for
                                // them by hand would be asking a second time
                                // for what is already known.
                                let opts = libp2p::swarm::dial_opts::DialOpts::peer_id(peer)
                                    .condition(
                                        libp2p::swarm::dial_opts::PeerCondition::DisconnectedAndNotDialing,
                                    )
                                    .build();
                                // **The budget is spent only on a dial the swarm
                                // accepted.** `Swarm::dial` returns
                                // `NoAddresses`, `Denied` and
                                // `DialPeerConditionFalse` **by value** with no
                                // `SwarmEvent`
                                // (`libp2p-swarm-0.47.1/src/lib.rs:444-511`), and
                                // this call used to discard the `Result`. So a
                                // provider whose addresses the bogon filter
                                // emptied — 3 310 private addresses were refused
                                // in the measured run — cost a slot and a
                                // minute's silence for a dial that never
                                // happened, and left no trace anywhere: the
                                // asynchronous failure log is capped at twelve
                                // and was spent by 12.6 s.
                                let id = opts.connection_id();
                                if swarm.dial(opts).is_ok() {
                                    // **Only a dial the crawl itself decided to
                                    // make.** This loop walks the providers of
                                    // whichever key the query named, and
                                    // `charge` is `Some` only inside the lobby
                                    // branch above — so a provider of some other
                                    // key neither spends the budget nor earns
                                    // from it.
                                    // Only a dial the crawl itself decided to
                                    // make, and only one the swarm accepted:
                                    // this loop walks the providers of whichever
                                    // key the query named, and `charge` is
                                    // `Some` only inside the lobby branch above.
                                    // The entitlement was spent up there with
                                    // the stamp; what is recorded here is the
                                    // dial that will actually produce an event.
                                    if charge.is_some() {
                                        lobby_dials.insert(id, peer);
                                    }
                                    // Belt and braces. Every dial ends in an
                                    // established connection or an error and
                                    // all three arms remove their entry, so
                                    // this cannot grow — but a map that grows
                                    // only because of a bug nobody noticed is
                                    // the shape this row has already been
                                    // caught by once.
                                    if lobby_dials.len() > 1024 {
                                        lobby_dials.clear();
                                    }
                                    match charge {
                                        Some(true) => fresh += 1,
                                        Some(false) => again += 1,
                                        None => {}
                                    }
                                }
                            }
                        }
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Mdns(
                        libp2p::mdns::Event::Discovered(found),
                    )) => {
                        for (peer, addr) in found {
                            // The address is taken every time even for a peer
                            // already known: mDNS reports one entry per address,
                            // and a neighbour that gains one — a second adapter,
                            // a new lease — is offering a route this node does
                            // not have yet. It is only the *announcement* that
                            // is once per peer.
                            // **Not `add_explicit_peer`, and that call was the
                            // formation flake.** It was here to make delivery to
                            // a neighbour reliable and did the exact opposite.
                            //
                            // An explicit peer is excluded from the mesh: the
                            // heartbeat's graft filter is
                            // `!explicit_peers.contains(peer)`
                            // (`libp2p-gossipsub-0.49.5/src/behaviour.rs:2224`
                            // and `:2317`). So on a LAN, where every peer
                            // arrives by mDNS, **every peer was explicit and the
                            // mesh could never fill** — measured as
                            // `0 of 5 subscribed peers grafted`, for the whole
                            // life of a run, with all five speaking gossipsub.
                            //
                            // With `flood_publish(false)`, which §11 sets
                            // deliberately, `publish` then reaches nobody:
                            // `mesh_peers` is empty, and the top-up that would
                            // cover it filters on `!explicit_peers.contains`
                            // **again** (`:670`), so `recipient_peers` comes out
                            // empty and the call returns
                            // `NoPeersSubscribedToTopic` (`:783`). That is the
                            // `not published: NoPeersSubscribedToTopic` a
                            // founder logs seconds after hosting.
                            //
                            // The asymmetry is what hid it: `forward_msg` **does**
                            // include explicit peers (`:2740`), so everything
                            // this node relays for somebody else arrives, and
                            // only what it originates — its own table advert —
                            // goes nowhere. The table then forms only when a
                            // joiner pulls the advert by gossip, which is the
                            // "should have taken seconds, took minutes" symptom.
                            //
                            // An mDNS neighbour is an ordinary peer. The mesh is
                            // what it belongs in.
                            // **Every** address is dialled, and only the
                            // announcement is once per peer.
                            //
                            // The first version dialled only the first address
                            // a peer was ever seen at, which on a machine with
                            // more than one adapter is a coin toss: mDNS
                            // reports one entry per address, and if the one
                            // that arrived first was a Hyper-V or WSL adapter's
                            // the dial failed and the peer's real address was
                            // never tried. Two clients on one machine stopped
                            // connecting at all.
                            let _ = swarm.dial(addr);
                            if seen_locally.insert(peer) {
                                let _ = events.send(NodeEvent::LocalPeer(peer)).await;
                            }
                        }
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Mdns(
                        libp2p::mdns::Event::Expired(gone),
                    )) => {
                        // Forgotten, so the next answer from this peer counts as
                        // news again. Nothing else is done: an expired mDNS
                        // record says the advertisement lapsed, not that the
                        // connection is down, and dropping a live connection
                        // because a multicast packet went missing would be the
                        // worst possible reading of it.
                        for (peer, _addr) in gone {
                            seen_locally.remove(&peer);
                        }
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                        gossipsub::Event::Subscribed { peer_id, topic: subscribed },
                    )) => {
                        // Somebody new on this table's topic. GossipSub
                        // delivers to whoever is on a topic when a message is
                        // published and to nobody afterwards, so a roster
                        // announced a moment before they arrived was never sent
                        // to them at all. Say it again.
                        if let (Some(f), Some(mine)) = (t.table.as_ref(), t.table_topic.as_ref()) {
                            if subscribed == mine.hash() {
                                // **Said out loud, because the silent version of
                                // this could not be told from not firing at
                                // all.** A rejoining client sat at `ratified
                                // 1/4, 0 held` for a whole run — nothing
                                // refused, nothing waiting, and no way from
                                // either side's log to say whether the repeat
                                // went out, went out into an empty mesh, or was
                                // never triggered.
                                let mut sent = 0usize;
                                let mut failed: Option<String> = None;
                                for bytes in f.say_again(super::node::now_unix_ms()) {
                                    // **And over the table's Tox group, where
                                    // no duplicate cache applies.** `S1-P`: a
                                    // seat's own ratification must arrive
                                    // verbatim or not at all, so it cannot be
                                    // re-signed, so GossipSub refuses the repeat
                                    // for `duplicate_cache_time` = 120 s — which
                                    // is exactly the window a seat that missed
                                    // the single publish needs it in. The Tox
                                    // group has no content-addressed cache and
                                    // carries the same bytes.
                                    t.tox_sink.try_broadcast(&bytes);
                                    match swarm
                                        .behaviour_mut()
                                        .gossipsub
                                        .publish(mine.clone(), bytes)
                                    {
                                        Ok(_) => sent += 1,
                                        Err(e) => failed = Some(format!("{e:?}")),
                                    }
                                }
                                if sent > 0 || failed.is_some() {
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "{peer_id} joined this table's topic: said {sent} message(s) again{}",
                                            match &failed {
                                                Some(e) => format!(", and {e}"),
                                                None => String::new(),
                                            }
                                        )))
                                        .await;
                                }
                            }
                        }

                        // The first peer on the **lobby** topic, and this client
                        // is hosting a table nothing has heard yet.
                        //
                        // A client that founds a table before anybody is on the
                        // topic publishes into an empty mesh: not an error, and
                        // not a delivery. The table then waited for the next
                        // housekeeping tick — up to half a minute in which the
                        // founder's own lobby, and everybody else's, showed
                        // nothing. The failure was "no peers subscribed", so the
                        // repair is "publish when one subscribes", and it is the
                        // moment rather than a shorter timer.
                        if subscribed == topics.lobby.hash() {
                            if let Some(f) = t.table.as_mut().filter(|f| f.is_founder()) {
                                let now = super::node::now_unix_ms();
                                // For **every** arrival, not only the first.
                                //
                                // The condition here used to be "unless my own
                                // table is already in my own lobby", which is
                                // true from the first successful publish
                                // onwards — so the second player to subscribe,
                                // and every one after, was told nothing and
                                // waited up to half a minute for the next
                                // housekeeping tick. That is what "new tables do
                                // not appear reliably" was.
                                //
                                // Bounded to once every few seconds, because a
                                // burst of arrivals must not become a burst of
                                // publishes.
                                let quiet = last_lobby_shout
                                    .is_none_or(|at: tokio::time::Instant| {
                                        at.elapsed() >= Duration::from_secs(3)
                                    });
                                if quiet {
                                    last_lobby_shout = Some(tokio::time::Instant::now());
                                    if let Ok(bytes) = f.readvertise(now, AD_TTL_MS) {
                                        if swarm
                                            .behaviour_mut()
                                            .gossipsub
                                            .publish(topics.lobby.clone(), bytes.clone())
                                            .is_ok()
                                        {
                                            show_own_table(
                                                &bytes,
                                                &my_app_key,
                                                now,
                                                &mut state.limits,
                                                &mut state.lobby,
                                                &events,
                                            )
                                            .await;
                                        }
                                    }
                                }
                            }
                        }
                        // Only for the topics this client is actually in.
                        //
                        // The event fires for **every** topic a peer subscribes
                        // to, and since this node joined the public libp2p
                        // network that includes strangers announcing IPFS
                        // topics it has never heard of. The log filled with
                        // hundreds of "joined the lobby mesh" lines about peers
                        // who had done nothing of the kind, and the lines that
                        // mattered were pushed out of a bounded log before
                        // anyone could read them.
                        if subscribed == topics.lobby.hash()
                            || Some(&subscribed) == t.table_topic.as_ref().map(|x| x.hash()).as_ref()
                        {
                            let _ = events.send(NodeEvent::MeshPeer(peer_id)).await;
                        }
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                        gossipsub::Event::Message {
                            message,
                            propagation_source,
                            message_id,
                        },
                    )) => {
                        // **Every path out of here must end in a
                        // `report_message_validation_result`.** With
                        // `validate_messages()` set, GossipSub forwards nothing
                        // until the application answers — so a path that returns
                        // without answering makes this client a black hole: it
                        // takes adverts and relays none, invisibly, because
                        // delivery to *itself* still works and a two-node test
                        // still passes.
                        //
                        // The three answers are not interchangeable, and the
                        // distinction is the one the whole protocol turns on.
                        // `Reject` says *this sender is at fault* and costs it
                        // peer score; `Ignore` says *this client will not pass it
                        // on* and blames nobody.
                        let verdict = handle_gossip(
                            &message,
                            propagation_source,
                            &topics,
                            &mut state,
                            &events,
                        )
                        .await;

                        let _ = swarm
                            .behaviour_mut()
                            .gossipsub
                            .report_message_validation_result(
                                &message_id,
                                &propagation_source,
                                verdict,
                            );
                    }
                    _ => {}
                }
            }

            _ = presence_timer.tick() => {
                let now = super::node::now_unix_ms();
                if let Ok(bytes) = super::lobbytalk::presence(&app_key, &nickname, now) {
                    // A failure here is `NoPeersSubscribedToTopic`, which is the
                    // ordinary state of a client nobody has met yet — and, as
                    // of this writing, the state two clients stay in even when
                    // they are connected to each other. See NEXT.md.
                    let _ = swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(topics.lobby_chat.clone(), bytes);
                }
            }

            _ = peerbook_timer.tick() => {
                let peers = harvest(&mut swarm);
                if !peers.is_empty() {
                    if let Err(e) = super::peerbook::save(&profile_dir, &peers) {
                        let _ = events
                            .send(NodeEvent::Warning(format!("could not save peers: {e}")))
                            .await;
                    }
                }
            }

            _ = discover_timer.tick() => {
                // The cap, downwards. Raising it is an event; lowering it is a
                // habit, and it needs patience: a client that trimmed its budget
                // the moment it was under would spend every cycle refusing and
                // raising again. Three quiet cycles, then a step down.
                {
                    let held = state_peers;
                    if held + held / 4 < budget && budget > MIN_CONNECTIONS {
                        comfortable += 1;
                        if comfortable >= 3 {
                            comfortable = 0;
                            budget = (budget - budget / 8).max(MIN_CONNECTIONS);
                            *swarm.behaviour_mut().conn_limits.limits_mut() =
                                super::swarm::connection_limits(budget);
                        }
                    } else {
                        comfortable = 0;
                    }
                }

                // The roster moves between ticks - somebody sits, somebody
                // stands - so the answer is recomputed rather than remembered
                // from the moment it first became true.
                // `D-043`: every table's answer, and the DHT's effort follows any
                // closed one.
                let mut any_closed = false;
                let mut changed = false;
                for x in tables.iter_mut() {
                    let closed = x
                        .table
                        .as_ref()
                        .is_some_and(|f| table_is_closed(f, &mut x.tournament_started));
                    if closed != x.table_closed {
                        x.table_closed = closed;
                        changed = true;
                    }
                    any_closed |= closed;
                }
                if changed {
                    dht_effort(&mut swarm, any_closed);
                }

                // The public lobby: announce, then read.
                //
                // **Announce only with an address to announce.** A provider
                // record carries the swarm's external addresses, and
                // go-libp2p-kad-dht drops one that arrives with none —
                // `handlers.go`, `if len(pi.Addrs) < 1 { continue }`. A
                // client behind a NAT has no external address until a relay
                // gives it a circuit, so announcing before then is a packet
                // sent to be discarded, and the lobby it thinks it joined
                // has never heard of it.
                // Read even while playing. The first version stopped, on the
                // grounds that a seated player is not looking at the lobby —
                // and that is only true of somebody playing one table. A player
                // who wants a second and a third has to be able to see what is
                // on offer while the first is running, and a lobby that goes
                // blank the moment you sit down cannot do that.
                //
                // What stays off at a closed table is the part that serves other
                // people: Kademlia's server mode, and advertising a table
                // nobody can join.
                if asked_public_dht {
                    let reachable_here = swarm.external_addresses().next().is_some();
                    if reachable_here
                        && !in_public_lobby
                        && !announced_at.is_some_and(|at| at.elapsed() < REANNOUNCE_EVERY)
                    {
                        match swarm.behaviour_mut().ipfs_kad.start_providing(lobby_namespace())
                        {
                            Ok(_) => {
                                // Dispatched. `in_public_lobby` waits for this
                                // client to see its own record come back.
                                announced_at = Some(std::time::Instant::now());
                                let _ = events.send(NodeEvent::Announced).await;
                            }
                            Err(e) => {
                                let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "could not join the public lobby: {e}"
                                )))
                                .await;
                            }
                        }
                    }
                    // Read it every cycle either way: a client with nothing
                    // to announce can still see who is there, and a table it
                    // can reach is a table it can sit at.
                    swarm.behaviour_mut().ipfs_kad.get_providers(lobby_namespace());
                }

                // A relay is needed when this client cannot be reached
                // directly — and DCUtR needs one too, since it upgrades an
                // existing relayed connection and cannot start one. So one is
                // looked for whenever there is no reservation yet, whatever
                // AutoNAT has said so far.
                //
                // **Asking is all this does.** The reservation itself is taken
                // in the `identify` arm, from any peer that turns out to speak
                // the hop protocol — which is the same path a public relay and
                // a volunteer both arrive by, and which replaced a version that
                // built the circuit address by hand and got it wrong: without
                // `/p2p/<PeerId>` before `p2p-circuit` the transport refuses it
                // before a packet leaves, and reports the refusal as an empty
                // string. That path never worked, and it is gone rather than
                // repaired.
                if !have_reservation {
                    relay_searches += 1;
                    swarm.behaviour_mut().ipfs_kad.get_providers(relay_namespace());
                    if relay_searches >= 3 && !seen_a_relay {
                        let _ = events
                            .send(NodeEvent::NoRelayFound {
                                cycles: relay_searches,
                            })
                            .await;
                        // Reach for the public network again: an entry point
                        // that was unreachable a minute ago may not be now.
                        for entry in PUBLIC_ENTRY {
                            if let Ok(addr) = entry.parse::<libp2p::Multiaddr>() {
                                let _ = swarm.dial(addr);
                            }
                        }
                    }
                }

                // And offer the line to other people's games, but only once
                // AutoNAT says this client can actually be reached (D-002).
                // Advertising a relay from behind a NAT is the harm the role
                // separation exists to avoid.
                //
                // Under `/libp2p/relay`, the namespace every libp2p client
                // already looks in, rather than a swarm of our own: a relay is
                // not a poker thing, and a volunteer who is only findable by
                // poker clients helps nobody else and is found no sooner.
                if state.is_public()
                    && !volunteering
                    && swarm
                        .behaviour_mut()
                        .ipfs_kad
                        .start_providing(relay_namespace())
                        .is_ok()
                {
                    volunteering = true;
                }
            }

            Some(command) = commands.recv() => {
                // `D-043`: a table founded or joined while this client sits at
                // another opens a new slot, up to `MAX_TABLES`; everything else
                // goes to the active table.
                let opening = matches!(
                    command,
                    NodeCommand::CreateTable { .. } | NodeCommand::JoinTable { .. }
                );
                // The new slot takes the command; the active one stays the window's
                // until it turns there (`NodeCommand::Focus`).
                let mut opened: Option<usize> = None;
                if opening && tables[active].table.is_some() && tables.len() < MAX_TABLES {
                    let sink = tables[0].tox_sink.share();
                    tables.push(TableRun::new(next_slot, &profile_dir, None, sink));
                    next_slot = next_slot.saturating_add(1);
                    opened = Some(tables.len() - 1);
                    let _ = events
                        .send(NodeEvent::Warning(format!(
                            "another table: {} of {MAX_TABLES} slots open",
                            tables.len()
                        )))
                        .await;
                }
                let which = opened.unwrap_or(active);
                let mut close_slot = false;
                let t = &mut tables[which];
                mark_table(&events, &mut marked, t).await;
                let now = super::node::now_unix_ms();
                match command {
                    NodeCommand::SayAtTable(text) => {
                        // `S1-CS`: to the table's group, which is the closed set of
                        // its seats; the table's GossipSub topic only where the
                        // table has no group. Said to this client's own window
                        // directly, as the lobby's line is.
                        let Some(f) = t.table.as_ref() else {
                            let _ = events.send(NodeEvent::Warning("not at a table".into())).await;
                            continue;
                        };
                        let Some(seat) = f.my_seat() else {
                            let _ = events.send(NodeEvent::Warning("no seat to speak from yet".into())).await;
                            continue;
                        };
                        let now = super::node::now_unix_ms();
                        match super::tabletalk::say(&app_key, &f.table_id(), &nickname, &text, now) {
                            Ok(bytes) => {
                                if t.tox_sink.is_on_tox() {
                                    t.tox_sink.try_broadcast(&bytes);
                                } else if let Some(topic) = t.table_topic.as_ref() {
                                    let _ = swarm.behaviour_mut().gossipsub.publish(topic.clone(), bytes);
                                }
                                let _ = events
                                    .send(NodeEvent::TableSaid {
                                        seat,
                                        nickname: nickname.clone(),
                                        text: super::tabletalk::clip_line(&text),
                                    })
                                    .await;
                            }
                            Err(e) => {
                                let _ = events.send(NodeEvent::Warning(format!("not said: {e}"))).await;
                            }
                        }
                    }
                    NodeCommand::SayInLobby(text) => {
                        let now = super::node::now_unix_ms();
                        match super::lobbytalk::say(&app_key, &nickname, &text, now) {
                            Ok(bytes) => {
                                // Shown locally whatever the mesh does. A line
                                // that vanished because nobody was subscribed
                                // would look like a client that swallowed it.
                                let _ = swarm
                                    .behaviour_mut()
                                    .gossipsub
                                    .publish(topics.lobby_chat.clone(), bytes);
                                let _ = events
                                    .send(NodeEvent::LobbySaid {
                                        who: app_key.verifying_key().to_bytes(),
                                        nickname: nickname.clone(),
                                        text,
                                    })
                                    .await;
                            }
                            Err(e) => {
                                let _ = events
                                    .send(NodeEvent::Warning(format!("not said: {e}")))
                                    .await;
                            }
                        }
                    }
                    NodeCommand::CreateTable {
                        kind, name, seats, min_players, buyin, password,
                    } => {
                        // A fresh key per table, and that freshness is the only
                        // thing making two tables with the same players and the
                        // same rules different games (§4.3's `session_id`).
                        let seed = match crate::security::rng::secret_32() {
                            Ok(s) => s,
                            Err(e) => {
                                let _ = events.send(NodeEvent::Warning(
                                    format!("no randomness for a table key: {e}"))).await;
                                continue;
                            }
                        };
                        let table_key = ed25519_dalek::SigningKey::from_bytes(&seed);
                        let tournament = kind == TableKind::SitAndGo;
                        // A Sit-and-Go's structure is settled, so it ignores the
                        // three fields the dialog collects for a cash table. The
                        // password is dropped rather than carried: a rated table
                        // may not have one, and a Sit-and-Go that people are
                        // waiting to fill is the wrong place for a gate anyway.
                        let (ad, password) = if tournament {
                            (
                                TableAd::sng(
                                    seats,
                                    trim_to_bytes(&name, TABLE_NAME_MAX),
                                    app_key.verifying_key().to_bytes(),
                                    my_peer_bytes.clone(),
                                    now,
                                ),
                                None,
                            )
                        } else {
                            (
                                new_table(
                                    name,
                                    seats,
                                    min_players,
                                    buyin,
                                    password.is_some(),
                                    app_key.verifying_key().to_bytes(),
                                    my_peer_bytes.clone(),
                                    now,
                                ),
                                password,
                            )
                        };
                        // **fault-harness only.** `P2P_POKER_START_STACK=<chips>`
                        // starts every seat of this founder's Sit & Go with that
                        // many chips, so a tournament ENDS inside a run -- 300
                        // against blinds of 50/100 is a few hands. The advert
                        // carries it to the joiners as any start stack; the
                        // buy-in is the stack, both bounds, as §6 requires.
                        let ad = if cfg!(feature = "fault-harness") && tournament {
                            let mut ad = ad;
                            if let Some(stack) = std::env::var("P2P_POKER_START_STACK")
                                .ok()
                                .and_then(|v| v.parse::<u64>().ok())
                                .filter(|s| *s > 0)
                            {
                                ad.start_stack = stack;
                                ad.min_buyin = stack;
                                ad.max_buyin = stack;
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "fault-harness: every seat starts with {stack} chips"
                                    )))
                                    .await;
                            }
                            ad
                        } else {
                            ad
                        };
                        // A tournament pays every entrant the same stack, so the
                        // founder's own seat takes the start stack and not what
                        // the dialog offered — `SeatEntry::admissible` admits
                        // exactly one value there.
                        let buyin = if tournament { ad.start_stack } else { buyin };

                        // **The group first, and the advertisement second.**
                        // D-019: the table's game traffic rides a Tox group and
                        // the advertisement names it, so the group has to exist
                        // before the advert is signed — an advert amended after
                        // publication is a second advert, and §7.2 rule 7 would
                        // read the pair as a founder changing the table.
                        //
                        // A failure here is not a failure to found a table. The
                        // table forms on the mesh exactly as it did before and
                        // the advert names no group, which is what a build
                        // without the feature always says.
                        let ad = match t.tox_sink.start(
                            &profile_dir,
                            super::toxsink::Role::Host,
                            &ad.table_name,
                            &nickname,
                            Vec::new(),
                        ) {
                            Ok(Some(mine)) => {
                                match t.tox_sink
                                    // **This await blocks the loop, so it is
                                    // short.** The driver creates the group as
                                    // soon as it is asked, so the wait is
                                    // ordinarily milliseconds. It was briefly
                                    // derived from a transport wait in the
                                    // driver; that wait is gone (see the note
                                    // above `SWEEP_EVERY` in `tox::table`) and
                                    // so is the reason to make this long.
                                    .chat_id_ready(std::time::Duration::from_secs(5))
                                    .await
                                {
                                    Some(chat) => {
                                        let _ = events
                                            .send(NodeEvent::Warning(format!(
                                                "this table's traffic rides a Tox group, {}",
                                                short_hash(&chat)
                                            )))
                                            .await;
                                        ad.on_tox(mine, chat)
                                    }
                                    None => {
                                        // The group did not appear. Rather than
                                        // advertise a table whose group nobody
                                        // can check, the sink is given up and
                                        // the table rides the mesh.
                                        t.tox_sink.clear();
                                        t.tox_group_said = false;
                                        t.table_announces = 0;
                                        let _ = events
                                            .send(NodeEvent::Warning(
                                                "the Tox group did not come up; this table stays on the mesh"
                                                    .into(),
                                            ))
                                            .await;
                                        ad
                                    }
                                }
                            }
                            Ok(None) => ad,
                            Err(e) => {
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "no Tox for this table ({e}); it stays on the mesh"
                                    )))
                                    .await;
                                ad
                            }
                        };

                        match advert::publish(&ad, &table_key) {
                            Ok(bytes) => {
                                let hash = advert_hash_of(&bytes);
                                match Formation::found(
                                    app_key.clone(),
                                    table_key.clone(),
                                    ad.clone(),
                                    bytes.clone(),
                                    hash,
                                    password,
                                    my_peer_bytes.clone(),
                                    nickname.clone(),
                                    buyin,
                                ) {
                                    Ok(f) => {
                                        let key = f.table_id();
                                        let topic = joinrpc::table_topic(&key);
                                        let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic);
                                        t.table_topic = Some(topic);
                                        t.table = Some(f);
                                        let _ = events.send(NodeEvent::Hosting { key }).await;
                                        report_params(&events, t.table.as_ref().unwrap()).await;
                                        report_roster(&events, t.table.as_ref().unwrap()).await;
                        if let Some(f) = t.table.as_ref() { seat_on_tox(f, &t.tox_sink); }
                                        // Published first, shown second: a table
                                        // appears in its founder's own lobby when
                                        // the network has taken it, and not when
                                        // it was signed.
                                        match swarm
                                            .behaviour_mut()
                                            .gossipsub
                                            .publish(topics.lobby.clone(), bytes.clone())
                                        {
                                            Ok(_) => {
                                                show_own_table(
                                                    &bytes,
                                                    &my_app_key,
                                                    now,
                                                    &mut state.limits,
                                                    &mut state.lobby,
                                                    &events,
                                                )
                                                .await;
                                            }
                                            Err(e) => {
                                                // No mesh peer yet is the
                                                // ordinary case at start-up. The
                                                // table is real and will be
                                                // advertised again in thirty
                                                // seconds; it is not in anybody's
                                                // lobby until then, including
                                                // this one.
                                                let _ = events
                                                    .send(NodeEvent::Warning(format!(
                                                        "the table is not on the network yet: {e}"
                                                    )))
                                                    .await;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = events.send(NodeEvent::Warning(
                                            format!("the table does not form: {e:?}"))).await;
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = events.send(NodeEvent::Warning(
                                    format!("the advert does not publish: {e}"))).await;
                            }
                        }
                    }

                    NodeCommand::JoinTable { key, buyin, seat, password } => {
                        // **One join at a time, because the second one replaces
                        // the first and invalidates its answer.**
                        //
                        // `table = Some(f)` below overwrites a live `Formation`
                        // with a new one carrying a new nonce, so the founder's
                        // honest `JOIN_ACCEPT` for the earlier request then
                        // fails `admit_accept`'s `request_hash` check and used
                        // to tear the table down. The arm above no longer tears
                        // anything down; this stops the second request being
                        // made at all.
                        //
                        // It is not a hypothetical. `main.rs`'s headless mode
                        // re-issues `JoinTable` on every `TableSeen` while it
                        // has no seat, and the first one blocks this loop for
                        // about a second inside `tox_sink.start` — long enough
                        // for a second advert to queue behind it. Measured:
                        // *“asking to join”* at 63.6 s and again at 64.7 s, with
                        // the Tox warning printed twice, which is what two
                        // commands look like from the outside.
                        // `D-037`: the founder's own record was taken up by
                        // `ResumeSession`; the `JoinTable` the window and the
                        // headless client send after it has nobody to ask, and
                        // is not a fault.
                        if t.resume.as_ref().is_some_and(|r| r.founder_seed != [0u8; 32] && r.table_key == key)
                            && t.table.as_ref().is_some_and(|f| f.is_founder())
                        {
                            continue;
                        }
                        if t.table.is_some() {
                            let _ = events
                                .send(NodeEvent::Warning(
                                    "already joining or seated at a table; leave it before joining another"
                                        .into(),
                                ))
                                .await;
                            continue;
                        }
                        let Some(held) = state.lobby.get(&key).cloned() else {
                            let _ = events.send(NodeEvent::Warning(
                                "that table is no longer advertised".into())).await;
                            continue;
                        };
                        // `S1-DE`: a seat that sits down elsewhere is not resuming
                        // any more -- the recorded ratification is the old table's,
                        // and a resuming client adopts hands rather than deriving
                        // hand 1. The record stays until the new table's replaces it.
                        if t.resuming && t.resume.as_ref().is_some_and(|r| r.table_key != key) {
                            t.resuming = false;
                        }
                        t.joined_key = Some(key);
                        t.joined_ad = Some(held.ad.clone());
                        t.joined_advert_hash = Some(held.advert_hash);
                        // The founder's PeerId comes from the advert, which was
                        // signed by the table key. Dialling anything else would
                        // be taking routing advice from whoever spoke last.
                        let founder = match PeerId::from_bytes(&held.ad.founder_peer_id) {
                            Ok(p) => p,
                            Err(_) => {
                                let _ = events.send(NodeEvent::Warning(
                                    "that table advertises a peer id this client cannot read"
                                        .into())).await;
                                continue;
                            }
                        };
                        let nonce = match crate::security::rng::secret_32() {
                            Ok(n) => n,
                            Err(e) => {
                                let _ = events.send(NodeEvent::Warning(
                                    format!("no randomness for a join nonce: {e}"))).await;
                                continue;
                            }
                        };
                        // **The Tox side before the request is sealed.** What
                        // `Formation::join` returns is already signed, so a key
                        // added afterwards would be a field outside the
                        // signature - the one thing a receiver would be right to
                        // ignore.
                        //
                        // Only if the table says it is on Tox: the advert
                        // carries the founder's key and the group's chat id or
                        // it carries neither, and a joiner that started an
                        // instance for a table on the mesh would be a socket and
                        // a DHT presence for nothing.
                        let my_tox_key = match held.ad.founder_tox_key {
                            None => None,
                            Some(founder_tox) => {
                                match t.tox_sink.start(
                                    &profile_dir,
                                    super::toxsink::Role::Joiner {
                                        founder: founder_tox,
                                        // Passed through so the driver can check
                                        // the group it is invited into is the one
                                        // advertised: an invitation says nothing
                                        // about which group it is for.
                                        chat_id: held.ad.tox_chat_id,
                                    },
                                    &held.ad.table_name,
                                    &nickname,
                                    vec![founder_tox],
                                ) {
                                    Ok(k) => {
                                        match k {
                                            Some(_) => {
                                                let _ = events.send(NodeEvent::Warning(format!(
                                                    "this table's traffic is on a Tox group, {}; waiting to be invited",
                                                    held.ad.tox_chat_id.map(|c| short_hash(&c))
                                                        .unwrap_or_else(|| "unnamed".into())
                                                ))).await;
                                            }
                                            None => {
                                                let _ = events.send(NodeEvent::Warning(
                                                    "this table's traffic is on Tox and this build has none; the hand will not reach it"
                                                        .into())).await;
                                            }
                                        }
                                        k
                                    }
                                    Err(e) => {
                                        let _ = events.send(NodeEvent::Warning(format!(
                                            "no Tox for this table ({e}); the hand will not reach it"
                                        ))).await;
                                        None
                                    }
                                }
                            }
                        };
                        match Formation::join(
                            app_key.clone(),
                            held.ad.clone(),
                            held.advert_hash,
                            key,
                            my_peer_bytes.clone(),
                            nickname.clone(),
                            buyin,
                            seat,
                            password.as_deref(),
                            nonce,
                            now,
                            my_tox_key,
                        ) {
                            Ok((f, request)) => {
                                // `S1-CR`: a seat that restarted ratifies with the
                                // bytes it recorded, verbatim; a new ratification is
                                // a new `event_hash` and so a session identity nobody
                                // else computes (`run202634-3`).
                                let f = match t.resume.as_ref().filter(|r| t.resuming && !r.ratification.is_empty()) {
                                    Some(r) => f.with_recorded_ratification(r.ratification.clone()),
                                    None => f,
                                };
                                let topic = joinrpc::table_topic(&key);
                                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic);
                                t.table_topic = Some(topic);
                                t.table = Some(f);
                                // **Ask the DHT for the founder before asking
                                // the founder for a seat.**
                                //
                                // The advert carries `founder_peer_id` and no
                                // address, and `send_request` dials by peer id —
                                // so it can only succeed if the swarm already
                                // holds an address for that peer. Nothing put
                                // one there on purpose: they arrive as a
                                // by-product of the sixty-second lobby crawl,
                                // whenever a Kademlia response happens to carry
                                // the founder's record.
                                //
                                // That is the whole of the formation time.
                                // Measured, `split144400-10`, connection to the
                                // founder against seating: `n1` **2.2 s**, `n2`
                                // **97.5 s** — same machine, same binary, same
                                // run. Once connected, seating took 0.1–3.7 s on
                                // six of nine seats and the Tox group 10–24 s on
                                // all of them, so this one step is the variance.
                                //
                                // A targeted lookup is the cheapest fix there
                                // is: no wire change, no new field in the
                                // advert, one query whose whole purpose is to
                                // learn where a peer id lives. It runs beside
                                // the request rather than before it, because a
                                // swarm that already has the address should not
                                // wait for a walk it does not need — and
                                // `PeerCondition::DisconnectedAndNotDialing`
                                // makes the redundant case free.
                                if !swarm.is_connected(&founder) {
                                    swarm
                                        .behaviour_mut()
                                        .ipfs_kad
                                        .get_closest_peers(founder);
                                }
                                let asked = swarm.behaviour_mut().join.send_request(&founder, request);
                                join_pending.insert(asked, which);
                            }
                            Err(e) => {
                        leave_the_table!(t);
                        dht_effort(&mut swarm, false);
                        let _ = events.send(NodeEvent::LeftTable {
                                    why: format!("cannot ask to join: {e:?}"),
                                }).await;
                            }
                        }
                    }

                    NodeCommand::ResumeSession => {
                        let Some(r) = t.resume.as_ref() else {
                            let _ = events.send(NodeEvent::Warning("no unfinished session is on record".into())).await;
                            continue;
                        };
                        if t.table.is_some() {
                            let _ = events.send(NodeEvent::Warning("already at a table; leave it before rejoining another".into())).await;
                            continue;
                        }
                        // `D-037`: the founder's own record. There is no founder to
                        // ask, so the roster is rebuilt from the list it signed, the
                        // group is re-entered on a member's invitation, and the
                        // session is taken up from the members' ratifications like
                        // any returning seat's -- then the running hand is adopted
                        // and the seat comes back by D-028 or D-031.
                        if r.founder_seed != [0u8; 32] {
                            let (seed, list, ratification, advert_hash, stack, hand_id) = (
                                r.founder_seed,
                                r.roster_list.clone(),
                                r.ratification.clone(),
                                r.advert_hash,
                                r.my_stack,
                                r.hand_id,
                            );
                            let ad = match super::advert::from_body_bytes(&r.advert) {
                                Ok(ad) => ad,
                                Err(e) => {
                                    let _ = crate::storage::session::forget(&profile_dir);
                                    t.resume = None;
                                    let _ = events
                                        .send(NodeEvent::SessionGaveUp { why: format!("the recorded advert does not read: {e:?}") })
                                        .await;
                                    continue;
                                }
                            };
                            let now = super::node::now_unix_ms();
                            // The members' Tox keys, from the founder's own list:
                            // the friendships a member's invitation rides on.
                            let members: Vec<[u8; 32]> = joinwire::receive_player_list_at(&list)
                                .map(|(l, _, _)| l.roster.iter().filter(|e| e.seat != 0).filter_map(|e| e.tox_key).collect())
                                .unwrap_or_default();
                            if ad.founder_tox_key.is_some() {
                                match t.tox_sink.start(
                                    &profile_dir,
                                    super::toxsink::Role::Back { chat_id: ad.tox_chat_id },
                                    &ad.table_name,
                                    &nickname,
                                    members,
                                ) {
                                    Ok(_) => {
                                        let _ = events
                                            .send(NodeEvent::Warning(format!(
                                                "this table's traffic is on a Tox group, {}; waiting for a member to offer it again",
                                                ad.tox_chat_id.map(|c| short_hash(&c)).unwrap_or_else(|| "unnamed".into())
                                            )))
                                            .await;
                                    }
                                    Err(e) => {
                                        let _ = events
                                            .send(NodeEvent::Warning(format!("no Tox for this table ({e}); the hand will not reach it")))
                                            .await;
                                    }
                                }
                            }
                            match Formation::found_back(
                                app_key.clone(),
                                seed,
                                ad.clone(),
                                advert_hash,
                                &list,
                                (!ratification.is_empty()).then(|| ratification.clone()),
                                now,
                            ) {
                                Ok((f, sends)) => {
                                    let key = f.table_id();
                                    let topic = joinrpc::table_topic(&key);
                                    let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic);
                                    t.table_topic = Some(topic.clone());
                                    t.table = Some(f);
                                    // A table that is set is not advertised again.
                                    t.ever_dealt = true;
                                    t.resuming = true;
                                    t.resume_since_ms = now;
                                    t.resume_last_peer_ms = None;
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "rejoining {} as its founder (seat 0, stack {stack}, last at hand #{hand_id}) from the session record",
                                            ad.table_name
                                        )))
                                        .await;
                                    let _ = events.send(NodeEvent::Hosting { key }).await;
                                    report_params(&events, t.table.as_ref().unwrap()).await;
                                    report_roster(&events, t.table.as_ref().unwrap()).await;
                                    if let Some(f) = t.table.as_ref() {
                                        seat_on_tox(f, &t.tox_sink);
                                    }
                                    for send in sends {
                                        if let Send::Broadcast(bytes) = send {
                                            let _ = swarm.behaviour_mut().gossipsub.publish(topic.clone(), bytes.clone());
                                            t.tox_sink.try_broadcast(&bytes);
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = crate::storage::session::forget(&profile_dir);
                                    t.resume = None;
                                    let _ = events
                                        .send(NodeEvent::SessionGaveUp { why: format!("the founder's record does not rebuild the table: {e:?}") })
                                        .await;
                                }
                            }
                            continue;
                        }
                        // The recorded advert, back on offer under the hash a
                        // `JOIN_REQUEST` names, stale by construction: a table
                        // that has started is not advertised. `JoinTable` then
                        // finds it like any other and the ordinary road follows --
                        // *already seated*, the roster, the group, the ratification.
                        match super::advert::from_body_bytes(&r.advert) {
                            Ok(ad) => {
                                let now = super::node::now_unix_ms();
                                let params = super::advert::table_params_hash(&ad);
                                let _ = state.lobby.offer(r.table_key, ad.clone(), params, r.advert_hash, now);
                                t.resuming = true;
                                t.resume_since_ms = now;
                                t.resume_last_peer_ms = None;
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "rejoining {} (seat {}, stack {}, last at hand #{}) from the session record",
                                        ad.table_name, r.my_seat, r.my_stack, r.hand_id
                                    )))
                                    .await;
                                let _ = events
                                    .send(NodeEvent::TableSeen {
                                        key: r.table_key,
                                        ad: Box::new(ad),
                                        params_hash: params,
                                        advert_hash: r.advert_hash,
                                    })
                                    .await;
                            }
                            Err(e) => {
                                let _ = crate::storage::session::forget(&profile_dir);
                                t.resume = None;
                                let _ = events
                                    .send(NodeEvent::SessionGaveUp { why: format!("the recorded advert does not read: {e:?}") })
                                    .await;
                            }
                        }
                    }
                    NodeCommand::ForgetSession => {
                        let _ = crate::storage::session::forget(&profile_dir);
                        t.resume = None;
                        t.resuming = false;
                        // `S1-DE`: said as the record going, which is what the
                        // window acts on; a warning left its question standing.
                        let _ = events
                            .send(NodeEvent::SessionGaveUp { why: "forgotten at the player's word".into() })
                            .await;
                    }
                    NodeCommand::Focus(slot) => {
                        // `D-043`: the window turned to another of its tables.
                        if let Some(i) = tables.iter().position(|x| x.slot == slot) {
                            active = i;
                        }
                    }
                    NodeCommand::SetNickname(name) => {
                        // Bounded here as well as where it is chosen. §4.3's
                        // limit is on what a roster will take, and the roster is
                        // built here — a name that arrived by any other path
                        // must not be able to make a seat entry this client's
                        // own rules would refuse.
                        nickname = trim_to_bytes(&name, DISPLAY_NAME_MAX);
                        if nickname.trim().is_empty() {
                            nickname = crate::storage::profile::short_name(
                                &app_key.verifying_key().to_bytes(),
                            );
                        }
                    }

                    NodeCommand::Act(action) => {
                        let Some(h) = t.hand.as_mut() else {
                            let _ = events
                                .send(NodeEvent::Warning(
                                    "there is no hand to act in".into(),
                                ))
                                .await;
                            continue;
                        };
                        let now = super::node::now_unix_ms();
                        match h.act(action, &app_key, now) {
                            Ok(sends) => {
                                publish_hand(sends, &mut swarm, &mut t.said, &t.tox_sink);
                                let report =
                                    report_hand(h, &events, &mut t.turn_reported).await;
                                if let Some(end) = report.ended {
                                    arm_boundary!(t, end.pause());
                                }
                                t.act_by = hurry(report.clock.apply(t.act_by, h.action_deadline()), autoplay);
                            }
                            // The player's own engine refused it, which means
                            // the window offered something it should not have.
                            // Reported rather than swallowed: a button that
                            // does nothing is worse than one that explains.
                            Err(e) => {
                                let _ = events
                                    .send(NodeEvent::Warning(format!("that action: {e}")))
                                    .await;
                            }
                        }
                    }

                    NodeCommand::LeaveTable => {
                        close_slot = true;
                        leave_table_now!(t);
                    }
                }
                // `D-043`: a slot left while another table is open is closed.
                if close_slot && tables.len() > 1 {
                    tables.remove(which);
                    active = tables.iter().position(|x| x.table.is_some()).unwrap_or(0);
                }
                // And a slot that never got its table -- a join refused -- goes
                // once the window has turned elsewhere.
                if tables.len() > 1 {
                    let keep = tables[active].slot;
                    tables.retain(|x| {
                        x.table.is_some()
                            || x.slot == keep
                            || x.opened_at.elapsed() < std::time::Duration::from_secs(60)
                    });
                    active = tables.iter().position(|x| x.slot == keep).unwrap_or(0);
                }
            }

            // Say again what this client already said, while the stage it
            // said it in is still open.
            // **A message from the table's Tox group (D-019).**
            //
            // The same handling as the mesh's, through the same macro, because
            // the answer to a hand event does not depend on what carried it.
            //
            // The verdict is discarded, and that is the one real difference
            // between the two transports. GossipSub needs one because
            // `validate_messages()` withholds forwarding until the application
            // reports a verdict; a Tox group forwards nothing on this client's
            // behalf, so there is nobody to tell and nothing to withhold. The
            // macro still computes it, because computing it is what holds an
            // early event and refuses a malformed one.
            //
            // Inert in a build without the feature: `TableSink::next` is a
            // future that never resolves when there is no Tox table, so this
            // branch contributes nothing to the `select!`.
            Some((which, item)) = next_from_tables(&mut tables) => {
                let t = &mut tables[which];
                mark_table(&events, &mut marked, t).await;
                // **Learn who this group peer is, once, from a signature.**
                // The driver reports a sender by its group key and cannot get
                // further; the roster is keyed by application key. Pairing them
                // here — where the event is opened anyway — is what makes
                // D-019's removal able to find its target at all (`S1-I`), and
                // it costs one open per peer per run.
                if let (Some(gk), Some(h)) = (item.claimed, t.hand.as_ref()) {
                    if !t.taught.contains(&gk) {
                        // `S1-EE`: a frame signed by this client that arrives from a
                        // member is that member saying it again (D-033, a copy at a
                        // rejoin), never the member being this client. Paired, the
                        // member's exit would report this client's own seat gone,
                        // and the felt drew *left the table* behind the player's own
                        // cards with nothing ever clearing it.
                        if let Some(app) = signer_of(&item.bytes, h).filter(|a| *a != my_app_key) {
                            t.taught.insert(gk);
                            t.tox_sink.tell(super::toxsink::Seat::KnownAs {
                                group_key: gk,
                                app_key: app,
                            });
                            // `D-041`: said, because the felt's *on the line* rests on it.
                            let seat = t.table.as_ref().and_then(|f| f.roster().seat_of(&app));
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "group member {} is seat {:?} (application key {})",
                                    short_hash(&gk),
                                    seat,
                                    short_hash(&app)
                                )))
                                .await;
                        }
                    }
                }
                // Nothing arrives while the link is down.
                if link_is_down() {
                    continue;
                }
                // `S1-CR`: a ratification copy arriving after this client's
                // session is set is a seat that lacks the others' -- a restarted
                // client, whose gossipsub copy of ours is a Duplicate for two
                // minutes and whose Tox copy went out before it was in the group
                // (`run195623-3`: *ratified 1/3* for the rest of the run). Say
                // ours again over the group, at most once in five seconds.
                // `ever_dealt`: before the first deal every seat repeats its
                // ratification on the housekeeping tick anyway, and answering
                // those would be every seat echoing every seat.
                if let Some(f) = t.table.as_ref().filter(|_| t.ever_dealt) {
                    if f.session().is_some()
                        && matches!(
                            crate::net::chained::peek(&item.bytes, TABLE_FRAME_PEEK),
                            Ok((crate::protocol::messages::EventType::TableReady, _, _))
                        )
                    {
                        let now = super::node::now_unix_ms();
                        if now.saturating_sub(t.ratification_echo_ms) >= 5_000 {
                            t.ratification_echo_ms = now;
                            let mut n = 0usize;
                            for bytes in f.say_again(now) {
                                t.tox_sink.try_broadcast(&bytes);
                                n += 1;
                            }
                            // `D-033`: and the running hand -- every frame accepted
                            // and every frame said -- so a seat back from a restart
                            // can take the hand up where it stood.
                            // Once a minute at most: a seat back from a restart repeats its
                            // ratification every thirty seconds until it has dealt, and a
                            // dealt hand is a hundred kilobytes of frames.
                            if let Some(h) = t.hand.as_ref().filter(|_| now.saturating_sub(t.hand_said_again_ms) >= 60_000) {
                                t.hand_said_again_ms = now;
                                let hid = h.hand_id();
                                let mut frames = 0usize;
                                for b in h.transcript() {
                                    t.tox_sink.try_broadcast(b);
                                    frames += 1;
                                }
                                for b in t.said.iter() {
                                    if crate::net::chained::peek(b, TABLE_FRAME_PEEK).ok().map(|(_, h, _)| h) == Some(hid) {
                                        t.tox_sink.try_broadcast(b);
                                        frames += 1;
                                    }
                                }
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "and hand #{hid}'s {frames} frame(s), for a seat back from a restart (D-033)"
                                    )))
                                    .await;
                            }
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "a ratification copy arrived after the table was set; said {n} message(s) again over the group"
                                )))
                                .await;
                        }
                    }
                }
                // `S1-CS`: a line of table chat over the group -- the closed set
                // of the table's seats -- from a seated key, to the window.
                if let Ok((crate::protocol::messages::EventType::TableChat, _, _)) =
                    crate::net::chained::peek(&item.bytes, LOBBY_MSG_MAX.max(TABLE_FRAME_PEEK))
                {
                    if let Some(f) = t.table.as_ref() {
                        let now = super::node::now_unix_ms();
                        if let Ok(heard) = super::tabletalk::receive(
                            &item.bytes,
                            &f.table_id(),
                            |k| f.roster().seat_of(k),
                            now,
                            &mut t.chat_limits,
                        ) {
                            let _ = events
                                .send(NodeEvent::TableSaid {
                                    seat: heard.seat,
                                    nickname: heard.nickname,
                                    text: heard.text,
                                })
                                .await;
                        }
                    }
                    continue;
                }
                // **Formation traffic over the group, before there is a hand.**
                //
                // `S1-P`. Everything below this block is gated on a live `Hand`,
                // so until 2026-09-02 every Tox byte that arrived during
                // formation was discarded — and formation is exactly where the
                // GossipSub duplicate cache makes a repeat impossible. A seat's
                // `TABLE_READY` must arrive **verbatim** (`emitted_at_unix_ms`
                // is inside `EventBody`, so a re-signature is a different
                // `event_hash`, a different `session_id`, and `RatifiedTwice`
                // against an honest seat), and a verbatim repeat is what
                // `publish` refuses for two minutes.
                //
                // The group exists throughout: D-019 creates it **before** the
                // advert is signed, because the advert names it. So this is not
                // a new transport for the message — it is the transport the
                // table already has, carrying a message that had no second
                // chance on the other one.
                //
                // **What it does not fix**, stated because the row would
                // otherwise read as closed: `say_again` repeats only this
                // client's *own* ratification, and `Formation::ratified` stores
                // hashes rather than bytes, so no peer can repair a third
                // party's missing copy. A seat that never enters the group at
                // all is `S1-AA` shape (i) and is untouched by this.
                if t.hand.is_none() {
                    // `S1-CR`: the running table's hand traffic, kept for the
                    // adoption at the stall tick. The formation handler below
                    // refuses it quietly either way.
                    if t.resuming {
                        let _ = stash_for_resume(&item.bytes, &mut t.resume_inits, &mut t.resume_early);
                    }
                    // `S1-DX`: a ratification or a roster over the group teaches who
                    // the member that carried it is, before any hand does -- a
                    // signed message either way (`S1-I`), and the driver refuses a
                    // claim to a seat that is here and speaking (`S1-DW`).
                    if let (Some(gk), Some(f)) = (item.claimed, t.table.as_ref()) {
                        if !t.taught.contains(&gk) {
                            let signer = super::joinwire::receive_table_ready(&item.bytes, &f.table_id(), &f.genesis())
                                .ok()
                                .map(|(_, s, _)| s)
                                .or_else(|| super::joinwire::receive_player_list(&item.bytes).ok().map(|(_, s)| s));
                            // Only a signer that holds a seat: a roster is signed by the
                            // table's key, and pairing a member with that would stand in
                            // the way of the hand's traffic teaching the seat itself
                            // (`run135355-3`: *is seat None*).
                            if let Some(app) = signer.filter(|a| f.roster().seat_of(a).is_some()) {
                                t.taught.insert(gk);
                                t.tox_sink.tell(super::toxsink::Seat::KnownAs { group_key: gk, app_key: app });
                                let seat = f.roster().seat_of(&app);
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "group member {} is seat {:?} (application key {}), by its ratification",
                                        short_hash(&gk),
                                        seat,
                                        short_hash(&app)
                                    )))
                                    .await;
                            }
                        }
                    }
                    if let Some(f) = t.table.as_mut() {
                        let now = super::node::now_unix_ms();
                        let took = if joinwire::receive_player_list(&item.bytes).is_ok() {
                            f.on_player_list(&item.bytes, now)
                        } else {
                            f.on_table_ready(&item.bytes)
                        };
                        // A refusal here is not reported. The group carries hand
                        // traffic too, and during formation a peer may already
                        // be sending it; feeding that to the formation handler
                        // produces a refusal that means nothing.
                        if let Ok(sends) = took {
                            for send in sends {
                                if let super::formation::Send::Broadcast(bytes) = send {
                                    if !t.tox_sink.try_broadcast(&bytes) {
                                        if let Some(t) = &t.table_topic {
                                            let _ = swarm
                                                .behaviour_mut()
                                                .gossipsub
                                                .publish(t.clone(), bytes);
                                        }
                                    }
                                }
                            }
                            report_roster(&events, f).await;
                            seat_on_tox(f, &t.tox_sink);
                            if let Some(session) = f.session() {
                                // `TableReal` is idempotent and is emitted from
                                // more than one place already; see `begin_hand`.
                                let _ = events
                                    .send(NodeEvent::TableReal {
                                        key: f.table_id(),
                                        session,
                                    })
                                    .await;
                            }
                        }
                    }
                }
                if let Some(h) = t.hand.as_mut() {
                    cross_boundary_at_t47(h, &mut t.boundaries, &mut t.crossed_for);
                    match checkpoint_event(
                        &item.bytes,
                        h,
                        &mut t.boundaries,
                        &mut t.readmitted,
                        &mut t.sit_ins,
                        &app_key,
                        &mut t.checkpoint_said,
                        &mut t.frozen,
                        &t.roster_seats,
                        &mut t.no_round_said,
                        &mut t.early_checkpoints,
                        &profile_dir,
                        &events,
                    )
                    .await
                    {
                        // Not a checkpoint event: a dispute, or the hand's
                        // own — and the hand's own is not applied while §6.3
                        // step 1's freeze is latched.
                        None => {
                            // The same three-way dispatch as the gossipsub
                            // branch above, and in the same order: a dispute is
                            // unchained, out of stage and may itself cause the
                            // freeze, so it is above it; §4.10's window is a
                            // chained event of a hand and is below it, beside
                            // the hand's own.
                            if !dispute_event(
                                &item.bytes,
                                h,
                                &mut t.boundaries,
                                &mut t.frozen,
                                &mut t.disputes_seen,
                                &events,
                            )
                            .await
                                && t.frozen.is_none()
                                && !boundary_event(
                                    &item.bytes,
                                    h,
                                    &mut t.boundaries,
                                    &mut t.readmitted,
                                    &mut t.sit_ins,
                                    &mut t.early_boundary,
                                    &events,
                                )
                                .await
                            {
                                let _ = hand_event!(t, h, &item.bytes);
                            }
                        }
                        // Taken, and this peer's own `STATE_ACK` is due.
                        Some(out) if !out.is_empty() => {
                            publish_and_hear(
                                out,
                                h,
                                &mut t.boundaries,
                                &mut t.readmitted,
                                &mut t.sit_ins,
                                &app_key,
                                &mut t.checkpoint_said,
                                &mut t.frozen,
                                &t.roster_seats,
                                &mut t.no_round_said,
                                &mut t.early_checkpoints,
                                &profile_dir,
                                &events,
                                &mut swarm,
                                &mut t.said,
                                &t.tox_sink,
                            )
                            .await;
                        }
                        Some(_) => {}
                    }
                }
            }

            _ = resend.tick() => {
                for which in 0..tables.len() {
                    let t = &mut tables[which];
                    mark_table(&events, &mut marked, t).await;
                    if !t.tox_group_said {
                        if let Some(chat) = t.tox_sink.chat_id() {
                            t.tox_group_said = true;
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "in the table's Tox group {}; the hand rides it from here",
                                    short_hash(&chat)
                                )))
                                .await;
                        }
                    }
                    // **Say when the transport is behind, once it is.** A seat
                    // certified late for something it did say looks, from every log
                    // this client writes, like a seat that said nothing. The
                    // difference is here.
                    // **A dropped hand event is a seat diverging, so it is said
                    // the moment it is not zero.** It is not in the line below
                    // because that one is about the transport being behind, and
                    // this is about an event that reached this client and was
                    // thrown away inside it.
                    // `S1-DW`: a member said another seat's message again as its
                    // own first word and the group refused the pairing. Said, and
                    // the group is taught again so the member's own next word
                    // pairs it with itself.
                    let claims = t.tox_sink.claims_refused();
                    if claims > t.claims_refused_said {
                        t.claims_refused_said = claims;
                        t.taught.clear();
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "the table's group refused {claims} claim(s) by a member to be a seat that is here and speaking (S1-DW); the group is taught again"
                            )))
                            .await;
                    }
                    // `D-045`: said when the count grows.
                    let removed = t.tox_sink.removed();
                    if removed > t.removed_said {
                        t.removed_said = removed;
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "{removed} member(s) of the table's group removed here by the table's word (D-045)"
                            )))
                            .await;
                    }
                    let kicked = t.tox_sink.kicked_out();
                    if kicked > t.kicked_out_said {
                        t.kicked_out_said = kicked;
                        let _ = events
                            .send(NodeEvent::Warning(
                                "this client was removed from the table's group by the table's word; it holds no group now, and is invited again when it returns (D-045)".into(),
                            ))
                            .await;
                    }
                    let dropped = t.tox_sink.inbox_dropped();
                    if dropped > t.inbox_dropped_said {
                        t.inbox_dropped_said = dropped;
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "{dropped} hand event(s) were received and dropped because this                              client was not draining; a seat that loses one diverges"
                            )))
                            .await;
                    }

                    let (refused, waiting, sent) = t.tox_sink.trouble();
                    if waiting > 0 || refused > t.tox_refused_said {
                        t.tox_refused_said = refused;
                        // Which refusal, because the remedies have nothing in
                        // common: code 4 is this client's own group connection
                        // being down, decided before any peer is consulted, while
                        // code 5 comes from the peer loop.
                        let why = t.tox_sink.refused_why();
                        let named = [
                            (1usize, "group-not-found"),
                            (2, "too-long"),
                            (3, "empty"),
                            (4, "disconnected"),
                            (5, "fail-send"),
                        ]
                        .iter()
                        .filter(|(i, _)| why[*i] > 0)
                        .map(|(i, name)| format!("{} {name}", why[*i]))
                        .collect::<Vec<_>>();
                        let breakdown = if named.is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", named.join(", "))
                        };
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "the table's transport is behind: {waiting} message(s) queued, \
                                 {refused} fragment(s) refused of {} offered{breakdown}",
                                refused + sent
                            )))
                            .await;
                    }

                    // **And when a seat is outside the group, which is not the same
                    // thing.** A refused invitation leaves a player seated, counted
                    // in the roster and never dealt to; the transport counters above
                    // are all zero while it happens, because there is nothing wrong
                    // with the transport.
                    let invites = t.tox_sink.invites_refused();
                    if invites > t.tox_invites_said {
                        t.tox_invites_said = invites;
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "a seat is not in the table's group yet: {invites} invitation(s) refused so far"
                            )))
                            .await;
                    }

                    // **While a table is forming, keep saying what we subscribe
                    // to.** The per-peer check above fires once, when a peer
                    // arrives; a subscription lost after that is a subscription
                    // nothing repairs, and the window that matters is exactly the
                    // few seconds between seating and ratifying. A formed table
                    // that is playing does not need it and does not do it.
                    // **The table's topic only, and only a few times.** The
                    // first version said all three every five seconds for as long
                    // as a table was forming, and that churn cost more than it
                    // bought: every re-announce prunes this client from every
                    // peer's lobby mesh, and adverts started coming back
                    // `RateLimited` — at the founder, against its own.
                    //
                    // The lobby is not what is missing during formation. The
                    // table's topic is, and re-announcing that one disturbs only
                    // the seats already at the table.
                    if let (Some(topic), true, true) =
                        (t.table_topic.as_ref(), t.table.is_some(), t.hand.is_none())
                    {
                        let hash = topic.hash();
                        let missing = poker_peers.iter().any(|p| {
                            !swarm.behaviour().gossipsub.all_peers().any(|(q, subs)| {
                                q == p && subs.contains(&&hash)
                            })
                        });
                        if missing && t.table_announces < MAX_TABLE_ANNOUNCES {
                            t.table_announces += 1;
                            announce_topics(&mut swarm, &[topic]);
                        }
                    }

                    // **The third road into hand 1, and the gate needs it.** The
                    // other two fire on a table message, and a table whose roster
                    // has ratified may send none at all while it waits for the Tox
                    // group to fill. Without this the wait could only end when
                    // something else happened to arrive, which on a quiet table is
                    // never. Idempotent for the same reason the other two are:
                    // `ever_dealt`.
                    if !t.ever_dealt {
                        if let Some(f) = t.table.as_ref() {
                            if !t.resuming && hand_one_may_open(
                                &t.tox_sink,
                                &mut t.hand_one_held_since,
                                &mut t.hand_one_progress,
                                &mut t.hand_one_forced_said,
                                &events,
                            )
                            .await
                            {
                                say_why_no_hand_one(f, &mut t.why_no_hand_one_said, &events).await;
                                if let Some(o) = opening_for_hand_one(f) {
                                    t.ever_dealt = true;
                                    begin_hand(
                                        o,
                                        &app_key,
                                        &mut t.hand,
                                        &mut t.said,
                                        &mut swarm,
                                        &events,
                                        &t.tox_sink,
                                    )
                                    .await;
                                }
                            }
                        }
                    }

                    let Some(h) = t.hand.as_ref() else { continue };
                    // A hand that is over is a hand nobody is waiting on.
                    if h.over() || t.said.is_empty() {
                        continue;
                    }
                    // **Through whichever transport the table has, which this loop
                    // did not do and had to.** It published to the mesh only, so a
                    // table on Tox re-sent nothing at all - and a Tox group keeps no
                    // history exactly as GossipSub keeps none, which is the whole
                    // reason this loop exists.
                    //
                    // Measured at three seats, which is where it showed: one seat's
                    // messages did not reach the other two, it was certified late
                    // twice, its grace ran out and the roster dropped it. A seat
                    // lost to a transport that never repeated itself. Heads-up hid
                    // it, because two peers both in the group before the first hand
                    // have nothing to re-send.
                    // **Backed off, and narrowed.** Blindly repeating every event
                    // of the hand every five seconds is spam, and it is spam that
                    // grows with the table: at six seats `said` holds about fourteen
                    // messages, several of them nine kilobytes, so each client was
                    // pushing roughly fifty fragments per tick to five peers - three
                    // hundred deliveries a second across the group, without pause,
                    // for as long as the hand lasted. Measured at six seats: the
                    // deck chain completed and the betting then stalled with **no
                    // send refused by toxcore at all**, which is what a transport
                    // that is being drowned rather than blocked looks like.
                    //
                    // Two changes, both conservative:
                    //
                    // * **Back off while nothing moves, reset when it does.** A
                    //   stuck table gets its first repeat after five seconds, then
                    //   ten, twenty, forty - bounded, and it stops growing at the
                    //   hand's own deadline anyway.
                    //
                    //   **An advancing table re-sends on every tick, and this
                    //   sentence used to claim it re-sent nothing.** The reset puts
                    //   `resend_ticks` at 0 and the increment immediately after puts
                    //   it at 1, which **is** a power of two - so a position that
                    //   changes every tick is due every tick. That is not a defect
                    //   and the code is left alone: `said` is **drained and
                    //   refilled by kind** at every boundary - the block above
                    //   `match next` keeps the terminal and the checkpoint frames
                    //   and discards the stage ones. This said *"cleared between
                    //   hands"*, which a reader would have had to open the code to
                    //   disbelieve, and `S1-CD` acquired a false premise from it
                    //   that survived four independent designs. So what an advancing
                    //   table repeats is the last three stages of the hand it is
                    //   playing, which is exactly the peer that is one or two stages
                    //   behind. The wrong half was the claim, and a reader who
                    //   believed it would either mis-measure the bandwidth or
                    //   "correct" the code and delete a working safety net.
                    // * **Only the recent stages.** A peer more than a few stages
                    //   behind is not going to be caught up by repetition; that is
                    //   what a catch-up request is for, and it does not exist yet.
                    //   Repeating the whole hand on its behalf costs every other
                    //   seat the bandwidth.
                    let here = h.slot().sequence;
                    // **Keyed on the hand as well as the sequence** (`S1-BU`). A
                    // run of hands that all end at sequence 0 — a table waiting
                    // for one seat's `HAND_INIT`, sixty seconds a hand — left
                    // `here` at 0 across every boundary, so the counter never
                    // reset and the new hand's terminal and `HAND_INIT` went out
                    // at ticks 32, 64, 128: once a hand at best, then never.
                    let stamp = (h.hand_id() << 20) | (here & 0xF_FFFF);
                    if stamp != t.resend_at {
                        t.resend_at = stamp;
                        t.resend_ticks = 0;
                    }
                    t.resend_ticks = t.resend_ticks.saturating_add(1);
                    // 1, 2, 4, 8, ... ticks: a power of two and nothing between.
                    let due = t.resend_ticks.is_power_of_two();
                    if due {
                        let window = here.saturating_sub(RESEND_STAGES);
                        let recent: Vec<&Vec<u8>> = t.said
                            .iter()
                            .filter(|b| {
                                crate::net::chained::peek(b, TABLE_FRAME_PEEK)
                                    .map(|(_, _, seq)| seq >= window)
                                    .unwrap_or(true)
                            })
                            .collect();
                        // **Tox or nothing.** This re-send used to fall back to the
                        // per-table GossipSub topic when there was no Tox carrier,
                        // which put hand bytes on a circuit relay reserved for
                        // 131 072 bytes per 120 s — less than one hand, and the
                        // client's own log already called it *“NOT enough to carry
                        // a hand”*. The owner's instruction is that a hand never
                        // travels on libp2p; the capacity is the reason.
                        //
                        // **And it asks whether anything may leave at all, which it
                        // did not.** `link_is_down`'s own doc says it drops *every*
                        // table message this client would send; this site is the
                        // one that got away, so a seat with its link down went on
                        // broadcasting the last `RESEND_STAGES` stages on every
                        // power-of-two tick for the whole outage. The knob was
                        // therefore measuring something quieter than a link that is
                        // down, and every run taken with it — `split220636-9` among
                        // them, one of `S1-BW`'s two failed reproductions — was
                        // taken against a seat that was still speaking.
                        if t.tox_sink.is_on_tox() && !nothing_leaves() {
                            for out in recent {
                                t.tox_sink.try_broadcast(out);
                            }
                        }
                    }
                }
            }

            // A stage that nobody is completing. Polled rather than armed:
            // the answer changes every time a stage opens or closes, and the
            // cryptographic ones are bounded per stage rather than per hand.
            _ = stall.tick() => {
                for which in 0..tables.len() {
                    let t = &mut tables[which];
                    mark_table(&events, &mut marked, t).await;
                    let now = super::node::now_unix_ms();
                    // `D-042`, the owner's rule: when the tournament is over, the
                    // group it was played in is left and the friends it was played
                    // with go (after `FRIEND_LINGER`, at the driver). A little after
                    // the end, so the resend loop gives a slow seat the terminal
                    // message twice more; the table itself stays on the felt until
                    // the player leaves it. On this two-second tick: the thirty-second
                    // one missed a whole run's end (`run212648-3`).
                    if let Some(since) = t.table_over_at {
                        if since.elapsed() >= TOURNAMENT_LEAVE_GRACE && t.tox_sink.is_on_tox() {
                            t.tox_sink.clear();
                            let _ = events
                                .send(NodeEvent::Warning(
                                    "the tournament is over: this client left the table's group, and the friends it played with go once no other table needs them".into(),
                                ))
                                .await;
                        }
                    }
                    // fault-harness: `P2P_POKER_KICK_WITHOUT_WORD_AT=<s>`: the founder kicks its
                    // first other seat without the table's word, for measuring that no
                    // member honours it (`D-045`).
                    if kick_without_word_due() {
                        if let Some(f) = t.table.as_ref().filter(|f| f.is_founder()) {
                            let me = f.my_seat();
                            let target = f.roster().seats().iter().find(|e| Some(e.seat) != me).and_then(|e| e.tox_key);
                            if let Some(k) = target {
                                println!("fault-harness: kicking a seat without the table's word, as P2P_POKER_KICK_WITHOUT_WORD_AT asked");
                                t.tox_sink.tell(super::toxsink::Seat::KickWithoutWord(k));
                            }
                        }
                    }
                    // fault-harness: `P2P_POKER_LEAVE_TABLE_AT=<s>` leaves the table at
                    // that second, as the window's button would -- for measuring how
                    // the others' lobbies learn that a table is gone (D-040).
                    if t.table.is_some() && leave_table_due() {
                        println!("fault-harness: leaving the table, as P2P_POKER_LEAVE_TABLE_AT asked");
                        leave_table_now!(t);
                    }
                    // **Give back the seat of anybody who has stopped answering,
                    // and only before the first hand.** `S1-DV`: on the two-second
                    // tick, and by the group's word first -- a seat that left, timed
                    // out or fell silent in the group, or never joined it and lost
                    // its line -- with the ninety seconds of ping silence last.
                    // `-LeaveTableAt` before the deal had held the seat for the run,
                    // a client back in the lobby answering every ping.
                    //
                    // A player who sits down at a tournament and leaves before it
                    // fills is ordinary. Nothing used to notice: `LeaveTable` clears
                    // local state and tells the founder nothing, there is no message
                    // that could tell it — `PLAYER_LEAVE` is a boundary-window event
                    // of a chain that does not exist yet — and D-022's *held for two
                    // hands* is counted in hands, of which there are none. So the
                    // seat was held for ever and the replacement was refused with
                    // *the table is full*, measured twice in one run.
                    //
                    // **Liveness is asked for rather than assumed.** `alive` is the
                    // last time each peer answered a **ping**; a connection being up
                    // is not evidence that anybody is behind it. `SEAT_SILENCE_MS`
                    // is six ping intervals, so a seat is given back only by a peer
                    // that has missed every one of them.
                    //
                    // `!ever_dealt` is the whole of what makes this not an eviction,
                    // and it is checked here because `Formation` cannot see it.
                    if !t.ever_dealt {
                        // `S1-DV`: the group's word, by the Tox key each member's
                        // invitation came over (`patches/0033`): who quit or timed
                        // out since the last tick and, read below, who is present
                        // and how quiet. Taught by nobody, known from the join. (The
                        // topic's unsubscribe was tried first and read this client's
                        // own re-announcement, an unsubscribe and a subscribe a
                        // moment apart, as a leave: `run115253-3`.)
                        let gone_lines = t.tox_sink.take_gone_lines();
                        // How long each seat has been on the roster here.
                        let now_tick = tokio::time::Instant::now();
                        if let Some(f) = t.table.as_ref() {
                            let seats: Vec<u8> = f.roster().seats().iter().map(|e| e.seat).collect();
                            t.seat_since.retain(|s, _| seats.contains(s));
                            for s in seats {
                                t.seat_since.entry(s).or_insert(now_tick);
                            }
                        }
                        if let Some(f) = t.table.as_mut().filter(|f| f.is_founder()) {
                            let silent: Vec<(u8, Vec<u8>, Option<[u8; 32]>, String)> = f
                                .roster()
                                .seats()
                                .iter()
                                .filter(|e| Some(e.seat) != f.my_seat())
                                .filter_map(|e| {
                                    // Answered no ping for `SEAT_SILENCE_MS`. Seated and
                                    // never once heard from counts: it reached the join
                                    // RPC over a connection, so `ConnectionEstablished`
                                    // recorded it, and no entry at all means that
                                    // connection and this client's memory of it are both
                                    // gone. A `peer_id` that will not parse cannot be
                                    // pinged and cannot be judged: §4.3 admitted it.
                                    let by_ping = match PeerId::from_bytes(&e.peer_id) {
                                        Ok(p) => alive
                                            .get(&p)
                                            .map(|(at, _)| {
                                                at.elapsed()
                                                    >= std::time::Duration::from_millis(SEAT_SILENCE_MS)
                                            })
                                            .unwrap_or(true),
                                        Err(_) => false,
                                    };
                                    // `S1-DV`: the group's word about the line this
                                    // seat's invitation went over.
                                    let line = e.tox_key;
                                    let exit = line.and_then(|k| {
                                        gone_lines.iter().find(|(l, _)| *l == k).map(|(_, quit)| *quit)
                                    });
                                    let quiet = line
                                        .and_then(|k| t.tox_sink.quiet_line(&k))
                                        .is_some_and(|q| q >= QUIET_LIMIT_S);
                                    let never_in = line.is_some_and(|k| {
                                        !t.tox_sink.in_group_line(&k) && !t.tox_sink.friend_up(&k)
                                    }) && t
                                        .seat_since
                                        .get(&e.seat)
                                        .is_some_and(|since| since.elapsed() >= GROUP_JOIN_GRACE);
                                    let why = if let Some(quit) = exit {
                                        Some(if quit {
                                            "left the table before the first hand".to_string()
                                        } else {
                                            "timed out of the table's group before the first hand".to_string()
                                        })
                                    } else if quiet {
                                        Some(format!("has been silent in the table's group for {QUIET_LIMIT_S} s"))
                                    } else if never_in {
                                        Some(format!(
                                            "never joined the table's group and its line is down, {} s after sitting down",
                                            GROUP_JOIN_GRACE.as_secs()
                                        ))
                                    } else if by_ping {
                                        Some(format!("has answered nothing for {} s", SEAT_SILENCE_MS / 1000))
                                    } else {
                                        None
                                    };
                                    // The Tox key travels with the seat, because it is
                                    // read from the roster **before** the release takes
                                    // the entry out of it. D-019's kick needs it and it
                                    // is gone a line later.
                                    why.map(|w| (e.seat, e.peer_id.clone(), e.tox_key, w))
                                })
                                .collect();

                            for (seat, peer, tox_key, why) in silent {
                                match f.release_seat_before_the_first_hand(&peer, now) {
                                    Ok(sends) if !sends.is_empty() => {
                                        let _ = events
                                            .send(NodeEvent::Warning(format!(
                                                "seat {seat} {why} and the seat is free again"
                                            )))
                                            .await;
                                        // **D-019: out of the roster is out of the
                                        // group.** `Seat::Left` was constructed
                                        // NOWHERE — only its match arm existed — so
                                        // `Command::Unseated` was never sent and
                                        // the kick was dead twice over: nothing
                                        // asked for it, and `peer_for` could not
                                        // have answered if anything had (`S1-I`).
                                        //
                                        // It is asked for here, at the one place a
                                        // seat leaves a roster. Whether the driver
                                        // can find the peer is a second question:
                                        // the group-key pairing is learned from
                                        // signed hand traffic, and before hand one
                                        // there is none — so a seat released during
                                        // formation is removed from the roster,
                                        // which is the authority, and left in the
                                        // group until it is rebuilt. Said here
                                        // rather than discovered later.
                                        if let Some(k) = tox_key {
                                            t.tox_sink.tell(super::toxsink::Seat::Left(k));
                                        }
                                        // Only broadcasts come out of a release:
                                        // there is nobody to reply to, because
                                        // nothing asked. A `Reply` here would be a
                                        // response to a request that does not
                                        // exist, so it is dropped and said rather
                                        // than sent nowhere quietly.
                                        for s in sends {
                                            match s {
                                                Send::Broadcast(bytes) => {
                                                    if let Some(tt) = &t.table_topic {
                                                        let _ = swarm
                                                            .behaviour_mut()
                                                            .gossipsub
                                                            .publish(tt.clone(), bytes);
                                                    }
                                                }
                                                Send::Reply(_) => {
                                                    let _ = events
                                                        .send(NodeEvent::Warning(
                                                            "releasing a seat produced a reply, which has no request to answer"
                                                                .into(),
                                                        ))
                                                        .await;
                                                }
                                            }
                                        }
                                        seat_on_tox(f, &t.tox_sink);
                                        report_roster(&events, f).await;
                                    }
                                    Ok(_) => {}
                                    Err(e) => {
                                        let _ = events
                                            .send(NodeEvent::Warning(format!(
                                                "could not free seat {seat}: {e:?}"
                                            )))
                                            .await;
                                    }
                                }
                            }
                        }
                        // `S1-DV`, the other side: a joiner whose founder left the
                        // table by its own word, or has answered nothing for
                        // `SEAT_SILENCE_MS`, or said the roster again without this
                        // client, has nothing to sit at. Back to the lobby, and the
                        // note says which. Before this a joiner waited for ever.
                        let gone: Option<String> = t.table.as_ref().filter(|f| !f.is_founder()).and_then(|f| {
                            let founder_peer = PeerId::from_bytes(f.founder_peer_id()).ok();
                            let founder_line = f
                                .roster()
                                .seats()
                                .iter()
                                .find(|e| e.peer_id.as_slice() == f.founder_peer_id())
                                .and_then(|e| e.tox_key);
                            let by_ping = founder_peer.and_then(|p| alive.get(&p)).is_some_and(|(at, _)| {
                                at.elapsed() >= std::time::Duration::from_millis(SEAT_SILENCE_MS)
                            });
                            let exit = founder_line.and_then(|k| {
                                gone_lines.iter().find(|(l, _)| *l == k).map(|(_, quit)| *quit)
                            });
                            let quiet = founder_line
                                .and_then(|k| t.tox_sink.quiet_line(&k))
                                .is_some_and(|q| q >= QUIET_LIMIT_S);
                            let never_in = founder_line.is_some_and(|k| {
                                !t.tox_sink.in_group_line(&k) && !t.tox_sink.friend_up(&k)
                            }) && f
                                // Since this client's seat first appeared on the roster
                                // here -- not since the slot opened, which for a window
                                // is the client's start: a client long in the lobby read
                                // its founder as gone at the first tick after sitting down.
                                .my_seat()
                                .and_then(|s| t.seat_since.get(&s))
                                .is_some_and(|since| since.elapsed() >= GROUP_JOIN_GRACE);
                            // `D-044`: the founder's own word over the lobby's question
                            // (D-040): an answer from it, made after this table's advert,
                            // that does not name the table. The founder is in the lobby
                            // and offers it no more; seen within one asking (thirty
                            // seconds) whether or not the group ever held it.
                            // Only while the founder is not a confirmed member of this
                            // client's group: a table that dealt is answered without
                            // its advert too, and the founder deals only once the group
                            // holds everybody -- so a founder in the group that answers
                            // without the table has dealt, not left.
                            let withdrawn = !founder_line.is_some_and(|k| t.tox_sink.in_group_line(&k))
                                && answers
                                    .get(f.founder_peer_id())
                                    .is_some_and(|(named, at)| !named.contains(&f.table_id()) && *at > f.advert_time());
                            if f.released_before_the_first_hand() {
                                Some(
                                    "the founder gave this seat away before the first hand -- this client had answered nothing for a while, or had left -- so there is nothing to sit at; join again from the lobby"
                                        .to_string(),
                                )
                            } else if let Some(quit) = exit {
                                Some(if quit {
                                    "the founder left the table before the first hand: the table is gone".to_string()
                                } else {
                                    "the founder timed out of the table's group before the first hand: the table is gone"
                                        .to_string()
                                })
                            } else if withdrawn {
                                Some(
                                    "the founder no longer offers this table before the first hand: it left, or went on without this client; back to the lobby"
                                        .to_string(),
                                )
                            } else if quiet {
                                Some(format!(
                                    "the founder has been silent in the table's group for {QUIET_LIMIT_S} s before the first hand: the table is gone"
                                ))
                            } else if never_in {
                                Some(format!(
                                    "the founder never joined the table's group and its line is down, {} s after this client sat down: the table is gone",
                                    GROUP_JOIN_GRACE.as_secs()
                                ))
                            } else if by_ping {
                                Some(format!(
                                    "the founder has answered nothing for {} s before the first hand: the table is gone",
                                    SEAT_SILENCE_MS / 1000
                                ))
                            } else {
                                None
                            }
                        });
                        if let Some(why) = gone {
                            let _ = events.send(NodeEvent::Warning(why.clone())).await;
                            leave_table_now!(t, why);
                        }
                    }

                    // `D-045`: and on the tick, for a word the moment missed.
                    if let Some(h) = t.hand.as_ref() {
                        remove_by_the_word!(t, h);
                    }
                    // `D-047`: this seat itself, certified out for the fourth time, is
                    // out of the table; the client leaves, as D-032's heads-up client
                    // does at its fourth absence.
                    let out_myself = t.hand.as_ref().and_then(|h| {
                        let me = h.my_seat();
                        h.out_for_good().contains(&me).then_some(me)
                    });
                    if let Some(me) = out_myself {
                        if !t.out_told {
                            t.out_told = true;
                            let why = format!(
                                "seat {me} -- this client -- is out of the table for good after its fourth absence (D-047): the table certified it four times over"
                            );
                            let key = t.table.as_ref().map(|f| f.table_id()).unwrap_or([0; 32]);
                            let _ = events.send(NodeEvent::Warning(why.clone())).await;
                            let _ = events.send(NodeEvent::OutForGood { key, why: why.clone() }).await;
                            // A headless client leaves at once; a window holds the
                            // table until its player closes it.
                            if autoplay.is_some() {
                                leave_table_now!(t, why);
                            }
                        }
                    }
                    // `D-041`: every seat's link as the table's group knows it. On a
                    // Tox table the group carries the hand and the felt reads presence
                    // from it; a libp2p ping is a figure beside that reading, and a
                    // seat reached only through a relay never answers one -- the
                    // owner saw the far seat drawn *offline* through a whole game, and
                    // the far seat saw everybody so.
                    if t.tox_sink.is_on_tox() {
                        if let Some(f) = t.table.as_ref() {
                            let me = f.my_seat();
                            let fresh: Vec<(u8, bool, Option<u64>)> = f
                                .roster()
                                .seats()
                                .iter()
                                .filter(|e| Some(e.seat) != me)
                                .map(|e| {
                                    // `S1-DX`, the owner's rule: the line is the group's
                                    // alone. A seat is on it when the group holds it -- by
                                    // the application key the group taught (`known_as`) or
                                    // by the line its invitation went over (`patches/0033`)
                                    // -- and has heard from it within `QUIET_LIMIT_S`; the
                                    // figure is how long ago that was. No friend link and
                                    // no libp2p ping: a client in the lobby answers both.
                                    let by_app = t.tox_sink.in_group(&e.app_public_key);
                                    let by_line = e.tox_key.is_some_and(|k| t.tox_sink.in_group_line(&k));
                                    let quiet_s = [
                                        t.tox_sink.quiet_secs(&e.app_public_key),
                                        e.tox_key.and_then(|k| t.tox_sink.quiet_line(&k)),
                                    ]
                                    .into_iter()
                                    .flatten()
                                    .min();
                                    let group = (by_app || by_line) && quiet_s.map_or(true, |q| q < QUIET_LIMIT_S);
                                    (e.seat, group, quiet_s)
                                })
                                .collect();
                            for (seat, group, quiet_s) in fresh {
                                let (changed, moved, again) = match t.link_said.get(&seat) {
                                    Some((g, q, at)) => (
                                        *g != group,
                                        *q != quiet_s,
                                        at.elapsed() >= std::time::Duration::from_secs(10),
                                    ),
                                    None => (true, true, true),
                                };
                                if changed || moved || again {
                                    t.link_said.insert(seat, (group, quiet_s, tokio::time::Instant::now()));
                                    let _ = events
                                        .send(NodeEvent::SeatLink { seat, rtt_ms: None, group, quiet_s })
                                        .await;
                                }
                                if changed {
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "seat {seat} link: {}{}",
                                            if group { "on the line by the table's group" } else { "not on the line by the table's group" },
                                            match quiet_s {
                                                Some(q) => format!(", heard {q} s ago"),
                                                None => String::new(),
                                            }
                                        )))
                                        .await;
                                }
                            }
                        }
                    }
                    // fault-harness: parked certificate copies that are due.
                    if !t.delayed_certs.is_empty() {
                        let at_now = tokio::time::Instant::now();
                        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut t.delayed_certs)
                            .into_iter()
                            .partition(|(at, _)| *at <= at_now);
                        t.delayed_certs = later;
                        if !due.is_empty() {
                            t.releasing_certs = true;
                            if let Some(h) = t.hand.as_mut() {
                                for (_, b) in due {
                                    let _ = hand_event!(t, h, &b);
                                }
                            }
                            t.releasing_certs = false;
                        }
                    }
                    // `S1-CR`: the rejoin's clock. With no table in hand, a fresh
                    // advert of the recorded key (a hash the record does not hold)
                    // means the table exists; a Tox peer of the session answering
                    // means it may; ten minutes of neither and the record is dropped.
                    // Meanwhile the recorded advert is kept on offer, because the
                    // lobby's sweep expires it as the stale thing it is.
                    if t.resuming && t.table.is_none() {
                        if let Some(r) = t.resume.as_ref() {
                            let now = super::node::now_unix_ms();
                            if t.tox_sink.is_on_tox() || t.tox_sink.group_seen().0 > 0 {
                                t.resume_last_peer_ms = Some(now);
                            }
                            let advert_seen = state
                                .lobby
                                .get(&r.table_key)
                                .is_some_and(|h| h.advert_hash != r.advert_hash && h.received_at_ms >= t.resume_since_ms);
                            if crate::storage::session::give_up(now, advert_seen, t.resume_last_peer_ms, t.resume_since_ms) {
                                let _ = crate::storage::session::forget(&profile_dir);
                                t.resume = None;
                                t.resuming = false;
                                let _ = events
                                    .send(NodeEvent::SessionGaveUp {
                                        why: "no advertisement and no peer of the session for ten minutes".into(),
                                    })
                                    .await;
                            } else if let Ok(ad) = super::advert::from_body_bytes(&r.advert) {
                                let params = super::advert::table_params_hash(&ad);
                                let _ = state.lobby.offer(r.table_key, ad, params, r.advert_hash, now);
                            }
                        }
                    }
                    // `S1-CR`: the record's first write, once the table is set, and
                    // again if the session changes under it. Not while resuming:
                    // the record on disk is the one this client came back from,
                    // and this write replaced its hand and stack with hand #0 and
                    // the buy-in (`run202634-3`); the next boundary writes it.
                    // `D-037`: the founder's record too. The gate read `joined_key`
                    // alone, so the founder wrote nothing and came back to no
                    // record (`run160631-3`).
                    let founder_here = t.table.as_ref().is_some_and(|f| f.is_founder());
                    if (t.joined_key.is_some() || founder_here) && !t.resuming {
                        if let Some(f) = t.table.as_ref() {
                            if f.session().is_some() && f.session() != t.recorded_session {
                                let stack = f
                                    .my_seat()
                                    .and_then(|s| f.roster().seats().iter().find(|e| e.seat == s).map(|e| e.buyin))
                                    .unwrap_or(0);
                                remember_session!(t, 0, [0; 32], stack);
                            }
                        }
                    }
                    // `D-038`: a hand nobody else has, still waiting at some stage,
                    // never arms `next_hand_at`, so the latch is read here as well.
                    if let Some((theirs, mine)) = t.adrift {
                        rejoin_from_copies!(t, theirs, mine);
                    }
                    // `S1-CR`: a resumed bystander whose adopted hand did not follow --
                    // the table has moved on to a hand it holds a majority of copies
                    // for -- abandons it and adopts again; the hand it had was never
                    // one it could act in, and nothing of the table's rests on it.
                    if t.resuming {
                        let stuck = t.hand.as_ref().and_then(|h| {
                            let me = h.my_seat();
                            let bystander = !h.required().contains(&me) && !h.returned().contains(&me);
                            // `S1-CX`: a member's adopted hand in which nothing was dealt
                            // is abandoned for a newer hand a majority has opened -- the
                            // table has moved on (`run085603-2`: the returning seat sat in
                            // a hand the survivor had given up, while the survivor opened
                            // four more). Heads-up before `D-039`, any size since: a seat
                            // the table deals in adopts at stage 0, and a table that gave
                            // that hand up on its budget meanwhile has dealt on without it.
                            let undealt = h.street().is_none() && !h.over();
                            (bystander || undealt).then_some(h.hand_id())
                        });
                        if let (Some(current), Some(f)) = (stuck, t.table.as_ref()) {
                            let occupied = f.roster().len();
                            let newer = t.resume_inits
                                .iter()
                                .any(|(hid, copies)| *hid > current && copies.len() * 2 > occupied.saturating_sub(1));
                            if newer {
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "adopted hand #{current} did not follow the table, which has dealt on; abandoning it for the next hand's copies"
                                    )))
                                    .await;
                                t.previous = t.hand.take();
                            }
                        }
                    }
                    // `D-033`: the running hand's card material goes into the session
                    // record the moment the deck stage begins, so a restart inside the
                    // hand can take it up again and play it out.
                    let material = t.hand.as_ref().and_then(|h| {
                        let s = h.secret()?;
                        if t.material_recorded == Some(h.hand_id()) {
                            return None;
                        }
                        let stack = h.stack_at_boundary(h.my_seat());
                        Some((h.hand_id(), stack, s.keep()))
                    });
                    if let Some((hid, stack, kept)) = material {
                        t.material_recorded = Some(hid);
                        remember_session!(t, hid, [0; 32], stack, Some(kept));
                    }
                    // `D-033`: a seat back without its material cannot play the hand on;
                    // at its turn it folds, as the owner's rule says.
                    let folded = match t.hand.as_mut() {
                        Some(h) if !h.can_play_on() && h.turn().is_some_and(|t| t.mine) => {
                            let now = super::node::now_unix_ms();
                            Some(h.act(crate::poker::actions::Action::Fold, &app_key, now))
                        }
                        _ => None,
                    };
                    match folded {
                        Some(Ok(sends)) => {
                            publish_hand(sends, &mut swarm, &mut t.said, &t.tox_sink);
                            let _ = events
                                .send(NodeEvent::Warning(
                                    "folded: this seat's card material was lost with the client that stopped, so the hand cannot be played on (D-033)".into(),
                                ))
                                .await;
                        }
                        Some(Err(e)) => {
                            let _ = events.send(NodeEvent::Warning(format!("could not fold: {e}"))).await;
                        }
                        None => {}
                    }
                    // `D-035`: seats whose client left the table's group.
                    for (app, quit) in t.tox_sink.take_gone() {
                        // `S1-EE`: never about this client's own seat -- it is here.
                        if app == my_app_key {
                            continue;
                        }
                        if let Some(seat) = t.table.as_ref().and_then(|f| f.roster().seat_of(&app)) {
                            let _ = events.send(NodeEvent::SeatLeft { seat, quit }).await;
                            let _ = events
                                .send(NodeEvent::SeatLink { seat, rtt_ms: None, group: false, quiet_s: None })
                                .await;
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "seat {seat} left the table's group{}",
                                    if quit { " on purpose" } else { ": its connection timed out" }
                                )))
                                .await;
                        }
                    }
                    // `S1-CS`: the group count, the moment it changes.
                    if t.table.is_some() {
                        let group = t.tox_sink.group_seen();
                        if t.carrier_reported != Some(group) {
                            // `S1-DX`: a member arrived -- this client says its own list
                            // and ratification again over the group, so the newcomer is
                            // taught who this client is at once rather than at the next
                            // thirty-second repeat.
                            // `S1-EE`: after the first deal as well -- a seat back under a
                            // fresh group key (S1-EB) is paired by its own word, not by
                            // whatever it happens to relay first.
                            if group.0 > t.carrier_reported.map_or(0, |(seen, _)| seen) {
                                if let Some(f) = t.table.as_ref() {
                                    for bytes in f.say_again(super::node::now_unix_ms()) {
                                        t.tox_sink.try_broadcast(&bytes);
                                    }
                                }
                            }
                            t.carrier_reported = Some(group);
                            let _ = events
                                .send(NodeEvent::Carrier {
                                    seen: u16::try_from(group.0).unwrap_or(u16::MAX),
                                    want: u16::try_from(group.1).unwrap_or(u16::MAX),
                                })
                                .await;
                        }
                    }
                    // `S1-CX`: heads-up, give up a hand nothing was dealt in when the
                    // other seat has opened the next one from exactly that give-up.
                    // After a line outage the two seats were one hand apart and chased
                    // each other, each giving its hand up on the budget after the other
                    // had moved on (`run080531-2`); no chips are at stake before the
                    // deal, an abort's terminal follows from the genesis, and the other
                    // seat's copy names the parent, so the two open the same hand.
                    if let Some((next, parent)) = t.give_up_for.take() {
                        let mut given_up: Option<Result<(u64, Vec<crate::table::hand::Send>), (u64, crate::table::hand::Failed)>> = None;
                        if let (Some(h), Some(f)) = (t.hand.as_mut(), t.table.as_ref()) {
                            if f.roster().len() == 2
                                && h.hand_id() + 1 == next
                                && h.street().is_none()
                                && !h.over()
                                && h.genesis_if_given_up() == Some(parent)
                            {
                                let current = h.hand_id();
                                let now = super::node::now_unix_ms();
                                given_up = Some(
                                    h.abort_now(crate::table::hand::Abort::Deadline, &app_key, now)
                                        .map(|sends| (current, sends))
                                        .map_err(|e| (current, e)),
                                );
                            }
                        }
                        match given_up {
                            Some(Ok((current, sends))) => {
                                publish_hand(sends, &mut swarm, &mut t.said, &t.tox_sink);
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "gave up hand #{current}, in which nothing was dealt: the other seat has opened hand #{next} from that give-up, and this client opens it too"
                                    )))
                                    .await;
                            }
                            Some(Err((current, e))) => {
                                let _ = events
                                    .send(NodeEvent::Warning(format!("could not give hand #{current} up: {e}")))
                                    .await;
                            }
                            None => {}
                        }
                    }
                    // `S1-CR`: the recorded ratification did not fit the roster the
                    // founder said again; this client ratified anew, and the session
                    // it computes is then its own. Said once, so the log can tell
                    // this from the members' answer never arriving.
                    if t.resuming && !t.recorded_refused_said {
                        if let Some(f) = t.table.as_ref().filter(|f| f.recorded_ratification_refused()) {
                            t.recorded_refused_said = true;
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "the recorded ratification did not fit the roster the founder said again (serial {}); ratified anew, and the session this client computes may not be the table's",
                                    f.serial()
                                )))
                                .await;
                        }
                    }
                    // `S1-CR`: a resuming client in the group with a roster and no
                    // session asks for the ratifications the way the protocol allows
                    // -- by saying its own again -- and a member answers with its own.
                    if t.resuming && t.hand.is_none() {
                        if let Some(f) = t.table.as_ref() {
                            if f.session().is_none() && f.my_seat().is_some() && t.tox_sink.is_on_tox() {
                                let now = super::node::now_unix_ms();
                                if now.saturating_sub(t.ratification_asked_ms) >= 5_000 {
                                    t.ratification_asked_ms = now;
                                    for bytes in f.say_again(now) {
                                        t.tox_sink.try_broadcast(&bytes);
                                    }
                                    if !t.ratification_asked_said {
                                        t.ratification_asked_said = true;
                                        let _ = events
                                            .send(NodeEvent::Warning(
                                                "in the group with the roster and no session yet; saying my ratification again every five seconds until the others' come".into(),
                                            ))
                                            .await;
                                    }
                                }
                            }
                        }
                    }
                    // `S1-CR`: a resumed client with no hand adopts the running one
                    // from the members' own copies -- a strict majority of the
                    // occupied seats at one genesis, and stage 0 closed elsewhere
                    // (every seat's copy in, or a later stage of that hand seen), so
                    // the set that signed is the required set. Then it follows the
                    // hand as a bystander; at its settled boundary it asks to sit in
                    // (`S1-BM`).
                    if t.resuming && t.hand.is_none() {
                        if let Some(f) = t.table.as_ref() {
                            if f.session().is_some() {
                                let occupied = f.roster().len();
                                // `S1-CX`: at two seats the one other seat's opening is the
                                // whole table's, and a hand waiting at stage 0 for this seat
                                // has no later frame to show -- so its opening alone is enough.
                                // Before this, only a hand the survivor had already given up
                                // could be adopted (`run085603-2`).
                                let heads_up = occupied == 2;
                                let pick = t.resume_inits
                                    .iter()
                                    .rev()
                                    .find(|(_, copies)| copies.len() * 2 > occupied.saturating_sub(1))
                                    .map(|(h, c)| (*h, c.clone()));
                                if let Some((hid, copies)) = pick {
                                    // `D-039`: whether the set that signed is exact is asked
                                    // after the adoption, which reads the table's own word on
                                    // who is dealt in. A seat the table deals in signs the hand
                                    // as a member whatever stage 0 is at -- its copy may be the
                                    // one the table is waiting for. A bystander still needs
                                    // stage 0 closed elsewhere (every copy in, or a later frame
                                    // seen), because it follows the signers' set and nobody
                                    // waits for it.
                                    let exact = heads_up
                                        || copies.len() >= occupied
                                        || t.resume_early.iter().any(|(h, _)| *h == hid);
                                    if let Some(base) = crate::table::hand::Opening::from_formation(f, hid) {
                                        let now = super::node::now_unix_ms();
                                        match crate::table::hand::Opening::adopt_with_signers(base, &copies) {
                                            Ok((mut o, signers)) => {
                                                // `D-033`: a copy of this seat's own opening among
                                                // the table's means the previous life signed this
                                                // hand: it is taken up where it stood, not opened anew.
                                                let signed_before = signers.contains(&o.my_seat);
                                                let kept: Option<[u8; 32]> = t.resume
                                                    .as_ref()
                                                    .filter(|r| r.secret_hand_id == hid && r.hand_secret != [0u8; 32])
                                                    .map(|r| r.hand_secret);
                                                // `S1-CX`: heads-up there is no certificate to
                                                // come back by (D-007), and the one other seat
                                                // is the whole table. A returning seat that is
                                                // in the roster with chips signs the hand it
                                                // adopts and is dealt in, as a member; the
                                                // other seat's copy names what it plays for.
                                                if f.roster().len() == 2 {
                                                    let me = o.my_seat;
                                                    if o.seats.iter().any(|(s, _, st)| *s == me && *st > 0)
                                                        && !o.required.contains(&me)
                                                    {
                                                        o.required.push(me);
                                                        o.required.sort_unstable();
                                                    }
                                                }
                                                let deadline = o.crypto_step_timeout_ms;
                                                let seat = o.my_seat;
                                                // A member's opening goes out; a bystander's
                                                // `open` says nothing the table may hear.
                                                let member = o.required.contains(&seat);
                                                let opened = if !member && !exact {
                                                    Err(crate::table::hand::Failed::NotYet)
                                                } else if signed_before {
                                                    crate::table::hand::Hand::open_restoring(o, &app_key, now, deadline, kept.as_ref())
                                                        .map(|h| (h, Vec::new()))
                                                } else {
                                                    crate::table::hand::Hand::open(o, &app_key, now, deadline)
                                                };
                                                match opened {
                                                    Ok((mut h, opening_sends)) => {
                                                        if member && !signed_before {
                                                            publish_hand(opening_sends, &mut swarm, &mut t.said, &t.tox_sink);
                                                        }
                                                        for c in &copies {
                                                            match h.on_event(c, &app_key, now) {
                                                                // `D-039`: a member's stage 0 may close on the last
                                                                // copy, and what the hand then owes the next stage
                                                                // comes out here. Dropped, seat 3 of `run182312-4`
                                                                // never said its deck contribution and was certified
                                                                // out of the hand it had just been dealt into, while
                                                                // seat 1, whose stage 0 closed on a frame that came
                                                                // through the ordinary road, played on. A restored hand
                                                                // (D-033) says its part from `restore_done` below.
                                                                Ok(sends) if member && !signed_before => {
                                                                    publish_hand(sends, &mut swarm, &mut t.said, &t.tox_sink);
                                                                }
                                                                Ok(_) => {}
                                                                Err(e) => {
                                                                    let _ = events
                                                                        .send(NodeEvent::Warning(format!("the adopted hand #{hid} refused one of its copies: {e}")))
                                                                        .await;
                                                                }
                                                            }
                                                        }
                                                        let carried: Vec<Vec<u8>> = t.resume_early
                                                            .iter()
                                                            .filter(|(x, _)| *x == hid)
                                                            .map(|(_, b)| b.clone())
                                                            .collect();
                                                        let n = carried.len();
                                                        for b in carried {
                                                            let _ = h.hold(b);
                                                        }
                                                        let (more, failures) = h.replay_early(&app_key, now);
                                                        publish_hand(more, &mut swarm, &mut t.said, &t.tox_sink);
                                                        for e in failures {
                                                            let _ = events
                                                                .send(NodeEvent::Warning(format!("a held frame was refused by the adopted hand #{hid}: {e}")))
                                                                .await;
                                                        }
                                                        // `D-033`: the table's frames are in; whatever the
                                                        // stage still wants from this seat is made now.
                                                        if signed_before {
                                                            match h.restore_done(&app_key, now) {
                                                                Ok(sends) => publish_hand(sends, &mut swarm, &mut t.said, &t.tox_sink),
                                                                Err(e) => {
                                                                    let _ = events
                                                                        .send(NodeEvent::Warning(format!("the taken-up hand #{hid} could not go on: {e}")))
                                                                        .await;
                                                                }
                                                            }
                                                        }
                                                        let first = h.first_held().and_then(|b| {
                                                            let (kind, _, seq) = crate::net::chained::peek(b, TABLE_FRAME_PEEK).ok()?;
                                                            let o = crate::net::chained::open_in_hand(b, crate::table::hand::FRAME_CAP, kind, &h.table_id(), hid).ok()?;
                                                            Some(format!("{:?} at sequence {seq} chained from {}", kind, short_hash(&o.envelope.previous_event_hash)))
                                                        });
                                                        let _ = events
                                                            .send(NodeEvent::Warning(format!(
                                                                "adopted hand #{hid} is at sequence {} chained from {}, waiting for {:?} with {} frame(s) still held; the oldest held is {}",
                                                                h.slot().sequence,
                                                                short_hash(&h.slot().previous_event_hash),
                                                                h.waiting_for(),
                                                                h.held(),
                                                                first.unwrap_or_else(|| "none".into())
                                                            )))
                                                            .await;
                                                        let _ = events
                                                            .send(NodeEvent::Warning(format!(
                                                                "resumed at hand #{hid} as {} (seat {seat}) from {} copies, {n} frame(s) replayed; {}",
                                                                if member { "a member" } else { "a bystander" },
                                                                copies.len(),
                                                                if signed_before && h.can_play_on() {
                                                                    "this seat's previous life signed it, and with the kept card material it plays on where it stood (D-033)"
                                                                } else if signed_before {
                                                                    "this seat's previous life signed it; without its card material it can only follow and fold (D-033)"
                                                                } else if member {
                                                                    "this seat signed it and is dealt in"
                                                                } else {
                                                                    "it asks to sit in at this hand's boundary"
                                                                }
                                                            )))
                                                            .await;
                                                        let _ = events.send(NodeEvent::SessionResumed { hand_id: hid }).await;
                                                        t.hand = Some(h);
                                                        t.resume_inits.retain(|k, _| *k > hid);
                                                        t.resume_early.retain(|(k, _)| *k > hid);
                                                        t.hand_reported = false;
                                                        t.deck_reported = None;
                                                        t.cards_reported = false;
                                                        t.turn_reported = None;
                                                        t.abort_reported = false;
                                                        t.resume_said = None;
                                                    }
                                                    Err(crate::table::hand::Failed::NotYet) => {
                                                        if t.resume_said != Some(hid) {
                                                            t.resume_said = Some(hid);
                                                            let _ = events
                                                                .send(NodeEvent::Warning(format!(
                                                                    "hand #{hid}: {} copies, and they do not deal this seat in; stage 0 is still open there, so this seat follows once it closes or takes the next hand (D-039)",
                                                                    copies.len()
                                                                )))
                                                                .await;
                                                        }
                                                    }
                                                    Err(e) => {
                                                        if t.resume_said != Some(hid) {
                                                            t.resume_said = Some(hid);
                                                            let _ = events
                                                                .send(NodeEvent::Warning(format!("the adopted hand #{hid} would not open: {e}")))
                                                                .await;
                                                        }
                                                    }
                                                }
                                            }
                                            Err(crate::table::hand::Failed::NotYet) => {}
                                            Err(e) => {
                                                if t.resume_said != Some(hid) {
                                                    t.resume_said = Some(hid);
                                                    let _ = events
                                                        .send(NodeEvent::Warning(format!("hand #{hid} cannot be adopted from the copies held: {e}")))
                                                        .await;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // `S1-BM`: return votes, cast on the evidence the boundary
                    // gathered by the hand the boundary belongs to -- the live hand
                    // while it is still hand k, the retained one once k+1 has
                    // opened. A certificate that completes here banks its subject;
                    // on the retained hand `take_late_roster` below then re-derives
                    // hand k+1, which is READMISSION.md section 4's settled-path
                    // late-roster repair, on `S1-BS`'s road.
                    if let Some(h) = t.hand.as_mut() {
                        vote_on_returns!(t, h);
                    }
                    if let Some(p) = t.previous.as_mut() {
                        vote_on_returns!(t, p);
                    }
                    // `S1-BS`: the retained hand replays what it holds, and a
                    // certificate that banked there re-derives the running hand.
                    if let Some(p) = t.previous.as_mut() {
                        let _ = p.replay_early(&app_key, now);
                        if let Some(n) = p.take_cert_note() {
                            let _ = events
                                .send(NodeEvent::Warning(format!("hand #{} (over): {n}", p.hand_id())))
                                .await;
                        }
                        if p.take_late_roster() {
                            t.late_banked_for = Some(p.hand_id());
                            t.pending_repair = p.next_hand();
                        }
                    }
                    // Retention ends when the running hand leaves stage 0: from
                    // there its genesis is what the table chained from.
                    if t.hand.as_ref().is_some_and(|h| h.slot().sequence >= 1) {
                        t.previous = None;
                    }
                    if let Some(o) = t.pending_repair.take() {
                        if t.frozen.is_some() {
                            // Kept until the freeze lifts: a repair is not lost to it.
                            t.pending_repair = Some(o);
                        } else if t.adrift.is_none() {
                            // **And when this client has latched itself out the
                            // repair is dropped, deliberately.** The arm above
                            // keeps a repair through a freeze and says so; this
                            // one lets it go, because a latched client is about to
                            // drop the branch the repair is for and rejoin from the
                            // table's copies (`D-038`), which is the repair.
                            let reopened = reopen_hand(
                                o,
                                &mut t.hand,
                                &mut t.said,
                                &mut swarm,
                                &events,
                                &t.tox_sink,
                                &app_key,
                            )
                            .await;
                            if reopened {
                                // A timer armed about the old hand must not fire
                                // about the new one.
                                t.next_hand_at = None;
                                t.deal_at = None;
                                t.act_by = None;
                                t.hand_reported = false;
                                t.deck_reported = None;
                                t.cards_reported = false;
                                t.turn_reported = None;
                                t.abort_reported = false;
                            }
                        }
                    }
                    // A quiet hand speaks once the table is counted at its genesis:
                    // as many seats at this genesis as at any other, or nobody
                    // anywhere else.
                    if let Some(h) = t.hand.as_mut() {
                        // Whatever the sequence: a `HAND_INIT` at (k+1, 0) is as
                        // valid sent late, and a quiet hand whose stage 0 completed
                        // between two ticks still owes the table its copy.
                        if h.voice() == crate::table::hand::Voice::Quiet {
                            let foreign = h.foreign_genesis_named().map(|(_, s)| s.len()).unwrap_or(0);
                            let counted = h.counted_at_stage_zero().len();
                            if counted >= foreign.max(1) {
                                if let Some(s) = h.speak() {
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "hand #{}: signed after all — {counted} seat(s) counted at this genesis, {foreign} at another",
                                            h.hand_id()
                                        )))
                                        .await;
                                    publish_hand(vec![s], &mut swarm, &mut t.said, &t.tox_sink);
                                }
                            }
                        }
                    }
                    let Some(h) = t.hand.as_mut() else { continue };

                    // **A message parked on a clock has to be re-judged by a
                    // clock.** `replay_early` had exactly one caller in the tree —
                    // the `Ok` arm of `hand_event!` — so anything held with
                    // `Failed::NotYet` was reconsidered only when some *other*
                    // event happened to arrive and be accepted. Most held events
                    // are waiting for a stage this client has not reached, and for
                    // those an incoming event is the right trigger. A `HAND_ABORT`
                    // held because this receiver's own deadline has not passed is
                    // waiting for **time**, and nothing here made time a trigger.
                    //
                    // In `split173908-10` the fix above would have fired anyway,
                    // by luck: the eight stranded seats re-sent their votes for 293
                    // seconds and a duplicate vote returns `Ok`, which ran the
                    // replay. A quieter table has no such accident, and the whole
                    // point of the gate widening is that the moment it opens is a
                    // moment when nothing else is happening.
                    let (replayed, held_failures) = h.replay_early(&app_key, now);
                    for e in held_failures {
                        let _ = events
                            .send(NodeEvent::Warning(format!("a held event: {e}")))
                            .await;
                    }
                    publish_hand(replayed, &mut swarm, &mut t.said, &t.tox_sink);
                    // `S1-CW`: a held certificate replayed here can end the hand too.
                    hand_may_have_ended!(t, h);
                    let Some(h) = t.hand.as_mut() else { continue };

                    // **Ask before accusing.** `S1-BK`: the stage budget is 30 s
                    // and the carrier's blind repair ladder puts its attempts in
                    // the seconds T+3, T+5, T+9, T+17 and T+33, so a message whose
                    // early sends the wire refused arrives after the stage waiting
                    // for it has expired, and its sender is voted out for a message
                    // that was in flight. The carrier has a fast path — one round
                    // trip, 36 to 326 ms — but it fires only when the receiver can
                    // SEE a hole, which needs a later message to have arrived. A
                    // stage waiting on one seat has nothing later to reveal it.
                    //
                    // This client does know. So before it says anybody is late, it
                    // asks each seat it is waiting for to re-send. A request for a
                    // message the peer never sent is looked up in that peer's send
                    // array and quietly does nothing, so a seat that is genuinely
                    // silent is neither helped nor disturbed and gains nothing by
                    // stalling — which is what keeps `D-026` intact.
                    for seat in h.waiting_for() {
                        if let Some(key) = h.key_of(seat) {
                            t.tox_sink.nudge(key, seat);
                        }
                    }
                    // `D-033`: a seat this hand has waited on for twenty seconds that is
                    // back on the line -- after an outage longer than the carrier's memory
                    // -- holds none of this hand's frames since; say them again, its own
                    // with them, once a minute at most.
                    if t.stage_waiting.0 != h.slot().sequence {
                        t.stage_waiting = (h.slot().sequence, now);
                    }
                    if now.saturating_sub(t.stage_waiting.1) >= 20_000
                        && now.saturating_sub(t.hand_said_again_ms) >= 60_000
                    {
                        let back: Vec<u8> = h
                            .waiting_for()
                            .into_iter()
                            .filter(|s| {
                                // `S1-EB`: on a Tox table the line is the group's (D-041,
                                // S1-DX) -- the seat is a member the group has heard within
                                // fifteen seconds. The ping reading below never fires there,
                                // a Tox table pinging nobody, so this re-say had been dead
                                // on every Tox table since S1-DX.
                                let by_group = h.key_of(*s).is_some_and(|k| {
                                    t.tox_sink.in_group(&k) && t.tox_sink.quiet_secs(&k).is_none_or(|q| q < 15)
                                });
                                // The ping reading counts only where the hand rides libp2p:
                                // the lobby's swarm keeps pinging over a line the table has
                                // lost (run160251-2 said *back on the line* to a seat whose
                                // internet was gone).
                                by_group
                                    || (!t.tox_sink.is_on_tox()
                                        && t.table
                                            .as_ref()
                                            .and_then(|f| f.roster().seats().iter().find(|e| e.seat == *s).map(|e| e.peer_id.clone()))
                                            .and_then(|b| libp2p::PeerId::from_bytes(&b).ok())
                                            .and_then(|p| alive.get(&p).copied())
                                            .is_some_and(|(at, rtt)| rtt.is_some() && at.elapsed() < std::time::Duration::from_secs(15)))
                            })
                            .collect();
                        if !back.is_empty() {
                            t.hand_said_again_ms = now;
                            let hid = h.hand_id();
                            let mut frames = 0usize;
                            for b in h.transcript() {
                                t.tox_sink.try_broadcast(b);
                                frames += 1;
                            }
                            for b in t.said.iter() {
                                if crate::net::chained::peek(b, TABLE_FRAME_PEEK).ok().map(|(_, x, _)| x) == Some(hid) {
                                    t.tox_sink.try_broadcast(b);
                                    frames += 1;
                                }
                            }
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "hand #{hid} has waited on seat(s) {back:?} that are back on the line: said its {frames} frame(s) again (D-033)"
                                )))
                                .await;
                        }
                    }

                    // Say so first, if this client's own timer has run out on
                    // somebody. A vote is not an accusation and does nothing
                    // alone; only a complete set becomes a certificate, and only a
                    // certificate moves anything. Where there are three seats or
                    // more this is the answer, and the abort below is what happens
                    // when it is not available — heads-up, where "unanimity" would
                    // be the one opponent.
                    match h.vote_on_timeouts(&app_key, now, t.tox_sink.mid_delivery()) {
                        Ok(sends) if !sends.is_empty() => {
                            // Said out loud, because a table that is waiting on
                            // somebody looks exactly like one that is stuck, and
                            // this is the line that tells them apart. It is also
                            // the only visible sign the mechanism exists at all: a
                            // healthy table never reaches it, because every client
                            // acts for its own owner first.
                            let who: Vec<String> =
                                h.waiting_for().iter().map(|s| s.to_string()).collect();
                            let mine = h
                                .take_tally()
                                .map(|(s, held, need, d)| {
                                    format!("seat {s} @{}: {held}/{need} agree", short_hash(&d))
                                })
                                .unwrap_or_default();
                            // **And what was last heard from each of them**, which
                            // is the fact that tells a lost message from a
                            // divergence. `S1-BB`: a seat was certified out for
                            // letting its clock run when it had answered every
                            // prompt in the same millisecond it arrived, and the
                            // run could not say which of the two it was because
                            // nothing recorded where that seat was last heard.
                            //
                            // Equal to this client's own stage means the seat spoke
                            // at the very stage it is accused of ignoring, which is
                            // a divergence. Lower means it never spoke there, which
                            // is a message that did not arrive.
                            let at = h.stage_sequence();
                            let seen: Vec<String> = h
                                .waiting_for()
                                .iter()
                                .map(|s| match h.last_heard_at(*s) {
                                    Some(l) => format!("{s} last heard at stage {l}"),
                                    None => format!("{s} never heard this hand"),
                                })
                                .collect();
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "my clock has run out on seat {} — {mine}; I am at stage {at}, {}",
                                    who.join(", "),
                                    seen.join(", ")
                                )))
                                .await;
                            // The same reading as at the macro's site, because a
                            // vote cast on this tick must not go unsampled just
                            // because the tick reports differently (`S1-BB`).
                            for (subject, mid, long_past) in h.take_vote_carrier() {
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "voted about seat {subject}: mid-delivery {mid}, \
                                         long past {long_past}"
                                    )))
                                    .await;
                            }
                            if let Some(n) = h.take_cert_note() {
                                let _ = events.send(NodeEvent::Warning(n)).await;
                            }
                            publish_hand(sends, &mut swarm, &mut t.said, &t.tox_sink);
                            // `S1-CW`: this vote completed a certificate whose stage a
                            // peer's copy had opened, and the hand ended here -- before
                            // any card was out, at the client that voted last. Said and
                            // succeeded like a hand ended by any event; before this the
                            // founder of a three-seat table sat on an ended hand for the
                            // rest of the run (`run195623-3`, `run131730-3`).
                            hand_may_have_ended!(t, h);
                        }
                        Ok(_) => {
                            if let Some(n) = h.take_cert_note() {
                                let _ = events.send(NodeEvent::Warning(n)).await;
                            }
                        }
                        Err(e) => {
                            let _ = events
                                .send(NodeEvent::Warning(format!("the clock: {e}")))
                                .await;
                        }
                    }
                    // Two seats have signed this hand at a genesis this client
                    // does not hold. Said once per hand (`S1-BS`).
                    if let Some(n) = h.take_genesis_note() {
                        let _ = events.send(NodeEvent::Warning(n)).await;
                    }
                    // A vote that is owed and not cast, with every gate's value.
                    // Measured before this: eight seats waiting on one for 370 s,
                    // four votes, and no line saying what held the other four.
                    if let Some(line) = h.vote_state(now, t.tox_sink.mid_delivery()) {
                        if now.saturating_sub(t.vote_state_said) >= 30_000 {
                            t.vote_state_said = now;
                            let _ = events.send(NodeEvent::Warning(line)).await;
                        }
                    }

                    let Some(h) = t.hand.as_mut() else { continue };
                    if !h.may_abandon(now) {
                        continue;
                    }
                    t.act_by = None;
                    // Which clock, taken before the abort resets the phase. Two
                    // budgets can produce this abort and they send a reader to
                    // completely different places -- see `Hand::expired_budget`.
                    let budget = h.expired_budget(now);
                    match h.abort_now(crate::table::hand::Abort::Deadline, &app_key, now) {
                        Ok(sends) => {
                            publish_hand(sends, &mut swarm, &mut t.said, &t.tox_sink);
                            // **And name the cause when the transport is the cause.**
                            // A player reading "the hand ran out of time" looks for
                            // a slow opponent. Across two networks the opponent was
                            // not slow: the only path to it was a relay circuit
                            // limited to 128 KB and two minutes, which is less than
                            // one hand, and the hand stopped at the deal.
                            //
                            // **But only when the hand is actually on that relay.**
                            // D-019 puts a formed table's game traffic on a Tox
                            // group, and `publish_hand` honours it: Tox first, the
                            // per-table GossipSub topic only when there is no Tox
                            // carrier. So on a table that rides Tox this clause
                            // named a libp2p relay for bytes that never touched
                            // libp2p — and it was believed. Measured
                            // `split215815-2`: both seats printed it, both were on
                            // the Tox group with one peer confirmed, and the real
                            // failure was that the joiner lost a table it had
                            // already set (session `af7beef3` at 64.9 s) and formed
                            // a second one (`d648ec85`) at 388.5 s, so the two
                            // opened hand 1 at different genesis values and each
                            // waited for a seat that was playing another table.
                            // The relay sentence hid that for a whole reading.
                            //
                            // A diagnosis that can be right for the wrong reason is
                            // worse than none, so it now asks whether this table is
                            // on Tox before blaming the wire underneath it.
                            let stranded: Vec<&libp2p::PeerId> =
                                relayed_peers.intersection(&poker_peers).collect();
                            let why = if relay_inadequate
                                && !stranded.is_empty()
                                && !t.tox_sink.is_on_tox()
                            {
                                format!(
                                    ". {} of the poker peers here {} reachable only through a relay                                  whose reservation this client already reported as too small to                                  carry a hand - that is the likely cause, and it is not the                                  opponent being slow",
                                    stranded.len(),
                                    if stranded.len() == 1 { "is" } else { "are" }
                                )
                            } else {
                                String::new()
                            };
                            let which = budget
                                .map(|b| format!(" -- {b}"))
                                .unwrap_or_default();
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "the hand ran out of time{which}; every stack is restored{why}"
                                )))
                                .await;
                            // Straight on: an abort has nothing to look at, so
                            // D-020's hold has nothing to hold.
                            arm_boundary!(t, std::time::Duration::from_millis(800));
                        }
                        Err(e) => {
                            let _ = events
                                .send(NodeEvent::Warning(format!("the hand could not be ended: {e}")))
                                .await;
                        }
                    }
                }
            }

            // This client's own clock ran out on its own turn.
            () = tokio::time::sleep_until(
                act_deadline.map(|d| d.1).unwrap_or_else(tokio::time::Instant::now)
            ), if act_deadline.is_some() => {
                let which = act_deadline.map(|d| d.0).unwrap_or(active);
                let t = &mut tables[which];
                mark_table(&events, &mut marked, t).await;
                t.act_by = None;
                let Some(h) = t.hand.as_mut() else { continue };
                let Some(turn) = h.turn().filter(|t| t.mine) else { continue };
                // fault-harness: `P2P_POKER_STOP_ON_TURN_AFTER=<s>` stops this process
                // at its first own turn at or after that second -- a client that
                // dies while the table waits on it, which a TIMEOUT_CERT with a
                // fold effect lets the table play past. A death mid-shuffle stalls
                // the table to the hand deadline (D-015), which is why `S1-CR`'s
                // measurement asks for this one.
                if stop_on_turn_due(delay_since) {
                    println!("fault-harness: stopping at my turn, as P2P_POKER_STOP_ON_TURN_AFTER asked");
                    std::process::exit(0);
                }
                // Check when it is free, fold when it is not. Never call and
                // never raise: a client that put its owner's chips in while
                // they were away would be playing for them, and this is only
                // keeping the table moving.
                //
                // **`--autoplay` is the deliberate exception and calls**, which
                // is why it is a measurement flag and not a setting. Folding
                // ends hands early, and a run of hands that all end preflop
                // measures nothing about how long a hand takes; calling carries
                // every hand to a showdown, which is the long case and the one
                // worth knowing. It plays for you, on purpose, and it says so
                // in `--help`.
                let action = if turn.legal.can_check {
                    crate::poker::actions::Action::Check
                } else if autoplay.is_some() {
                    crate::poker::actions::Action::Call
                } else {
                    crate::poker::actions::Action::Fold
                };
                let now = super::node::now_unix_ms();
                match h.act(action, &app_key, now) {
                    Ok(sends) => {
                        publish_hand(sends, &mut swarm, &mut t.said, &t.tox_sink);
                        let _ = events
                            .send(NodeEvent::Warning(if autoplay.is_some() {
                                format!("autoplay: {action:?}")
                            } else {
                                format!("your clock ran out — {action:?} for you")
                            }))
                            .await;
                        let report = report_hand(h, &events, &mut t.turn_reported).await;
                        if let Some(end) = report.ended {
                            arm_boundary!(t, end.pause());
                        }
                        t.act_by = hurry(report.clock.apply(t.act_by, h.action_deadline()), autoplay);
                    }
                    Err(e) => {
                        let _ = events
                            .send(NodeEvent::Warning(format!("the clock's own action: {e}")))
                            .await;
                    }
                }
            }

            // D-020's hold has run out: deal the next hand.
            // `D-043`: the earliest deal among the tables; a slot with nothing
            // pending is simply not in the race.
            () = tokio::time::sleep_until(
                deal_deadline.map(|d| d.1).unwrap_or_else(tokio::time::Instant::now)
            ), if deal_deadline.is_some() => {
                let which = deal_deadline.map(|d| d.0).unwrap_or(active);
                let t = &mut tables[which];
                mark_table(&events, &mut marked, t).await;
                t.next_hand_at = None;
                // **Never succeed a hand that is not over.** This arm rested
                // on an invariant — the timer fires only about a hand that
                // ended — that a re-open (`S1-BS`) broke: a repair can put a
                // running hand under a timer armed about the old one, and
                // `next_hand()` is `None` for a running hand, which used to
                // end here with `hand = None` and a seat that had no hand for
                // the rest of the table. Found by the refuters, not the run.
                if t.hand.as_ref().is_some_and(|h| !h.over()) {
                    continue;
                }
                // **No further hand while frozen.** §6.3's freeze stops emission
                // as well as acceptance, and a peer that dealt on would be
                // building hand `k+1` on a state it has been told is contested.
                //
                // **The checkpoint below is not reached either, and for the
                // frozen peer that stays right — but the reason given here used
                // to be wrong and it was the same reason that broke the adrift
                // path.** *"There is nothing to checkpoint if no hand ran"* is
                // about hand `k+1`; the block below publishes hand **`k`'s**
                // hash, and that hand did run. What makes withholding it correct
                // *here* is narrower: a peer freezes because a value at that
                // checkpoint was already compared and disagreed, so its own copy
                // is out — and §6.3 step 2's dispute carries the complete signed
                // bytes as evidence in any case. The adrift latch had no such
                // argument and was moved below the checkpoint.
                // Reset where the latch is read, not where it is cleared:
                // `checkpoint_event` releases the freeze and has no business
                // knowing what this loop has said out loud.
                if t.frozen.is_none() {
                    t.frozen_said = false;
                }
                if let Some((k, _)) = t.frozen {
                    // **Re-armed, not consumed.** The line above cleared the
                    // timer before this branch was reached, and nothing puts it
                    // back: `*frozen = None` on reconciliation sends its warning
                    // and touches no timer, and the only other arming site fires
                    // when a hand *ends*, which cannot happen while none starts.
                    // So a peer that froze and then reconciled announced *the
                    // divergence reconciled* and then dealt nothing for the rest
                    // of its life.
                    //
                    // Clearing it was not wrong on its own — leaving it set makes
                    // this arm fire every tick — so the timer is pushed forward
                    // instead. The peer re-checks on a slow cadence while frozen
                    // and deals on the first tick after the freeze lifts, with no
                    // plumbing between `§6.3`'s reconciliation and this loop's
                    // locals.
                    t.next_hand_at =
                        Some(tokio::time::Instant::now() + FROZEN_RECHECK);
                    if !t.frozen_said {
                        t.frozen_said = true;
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "no further hand is dealt: this peer is frozen at hand {k}'s checkpoint and section 6.3 releases that by a reconciliation round alone. {} dispute(s) verified from other seats; W is {:?}",
                                t.disputes_seen,
                                t.boundaries.contradicted(k)
                            )))
                            .await;
                    }
                    continue;
                }
                // **Said when the roster moves, and only then.** Reported at
                // the derivation rather than at the end of the hand, because a
                // late certificate is banked in between and the earlier version
                // described a state the derivation never saw. But a table where
                // nobody leaves has nothing to explain, and two lines of
                // required sets, allowances and strike counts after every hand
                // are two lines a player has to read past to find out what
                // happened to theirs.
                // **This peer's own checkpoint-8 `STATE_HASH`, published once
                // the hand it is about has settled** (§4.9, §6.2 row 8). It is
                // the value every other seat compares against, and it is the
                // door back for a seat that missed this hand: §4.9's
                // readmission set is written by exactly this event agreeing.
                let phase1 = t.hand
                    .as_ref()
                    .is_some_and(|h| t.boundary_done_for != Some(h.hand_id()));
                if let Some(h) = t.hand.as_ref().filter(|_| phase1) {
                    // **§4.10's window, on BOTH terminal paths, and this is
                    // separate from the checkpoint below for exactly that
                    // reason** (`S1-BZ`). The block that follows is gated on
                    // `checkpoint8()`, which is `None` after an abort — §6.2
                    // row 8's aborted checkpoint is `S1-R` and is not built —
                    // so a window opened inside it existed on the settled path
                    // alone. **The abort path is the one a seat goes quiet on**,
                    // which is the whole population §4.10's window is for, so
                    // the half that was missing was the half that mattered.
                    //
                    // `Hand::terminal` answers §3.1's question on both paths and
                    // needs nothing the aborted checkpoint needs:
                    // `ABORT_TERMINAL(k)` is a function of `GENESIS(k)` alone.
                    if let Some(terminal) = h.terminal() {
                        let roster: Vec<u8> = t.table
                            .as_ref()
                            .map(|f| f.roster().seats().iter().map(|e| e.seat).collect())
                            .unwrap_or_default();
                        // **The window's set is `R(k)`, the roster of hand k, and
                        // not `P(k)`** (D-028). Only `PLAYER_SIT_IN` reads it, to
                        // refuse a request from a seat that is already dealt in --
                        // and a bystander in section 4.9's `A` signs stage 0 of
                        // hand k, so it is in `P(k)` without being in `R(k)`, and
                        // its request is exactly what a return certificate is
                        // about. `split174002-9`: eight voters refused it as
                        // deciding nothing. Opened here first, so the checkpoint's
                        // `open` below, which carries `P(k)`, finds it already open.
                        t.boundaries.open_window(
                            h.hand_id(),
                            terminal,
                            h.required(),
                            &roster,
                        );
                        // `S1-BM`: the boundary events that arrived before this
                        // window existed, admitted now -- the way early checkpoints
                        // are below. Through `boundary_event` itself, the one
                        // admission path, and before anything of this boundary is
                        // published, so a request already here is taken before
                        // the deal is decided.
                        let waiting = t.early_boundary.remove(&h.hand_id()).unwrap_or_default();
                        t.early_boundary.retain(|k, _| *k > h.hand_id());
                        if !waiting.is_empty() {
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "hand #{}: {} boundary event(s) arrived before the window opened here and are admitted now",
                                    h.hand_id(),
                                    waiting.len()
                                )))
                                .await;
                        }
                        for b in waiting {
                            let _ = boundary_event(
                                &b,
                                h,
                                &mut t.boundaries,
                                &mut t.readmitted,
                                &mut t.sit_ins,
                                &mut t.early_boundary,
                                &events,
                            )
                            .await;
                        }
                    }
                    if let Some((state, terminal)) = h.checkpoint8() {
                        // The stage, opened at the moment `TERMINAL(k)` is
                        // fixed. `P(k)` is a **snapshot** taken here and not a
                        // set read later: a checkpoint-8 `STATE_HASH` is itself
                        // a chain-`k` event and adds its sender to `P(k)`, so a
                        // set read after the stage opened would grow with its
                        // own contributions and never complete (§4.9).
                        //
                        // The accepted set is every occupied roster seat, which
                        // is §4.9's `L3`: a copy from outside `P(k)` is compared
                        // rather than rejected, and that copy agreeing is what
                        // writes the readmission set.
                        // Refreshed here, once a hand, because this is the one
                        // place that already needs it and a reconciliation round
                        // — the only other reader — can only open after a
                        // checkpoint this site opened.
                        t.roster_seats = t.table
                            .as_ref()
                            .map(|f| f.roster().seats().iter().map(|e| e.seat).collect())
                            .unwrap_or_default();
                        let roster = t.roster_seats.clone();
                        if t.boundaries
                            .open(
                                h.hand_id(),
                                h.table_id(),
                                terminal,
                                state,
                                &h.participants(),
                                &roster,
                            )
                            .is_none()
                        {
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "the boundary checkpoint of hand {} would not open: P(k) {:?} is not inside the roster {roster:?}",
                                    h.hand_id(),
                                    h.participants()
                                )))
                                .await;
                        }
                        // **The copies that arrived before this boundary
                        // existed, admitted now.**
                        //
                        // Held rather than dropped by `checkpoint_event`; see
                        // `early_checkpoints`. They go through the same one
                        // admission path as everything else — a second set of
                        // rules for the same event is how two paths come to
                        // disagree — and they go through it **before** this
                        // client publishes its own, so a stage that is already
                        // complete on arrival closes here instead of waiting
                        // for a repeat that nothing was going to send.
                        //
                        // Taken out of the map first, because the call needs
                        // `&mut` on it.
                        let waiting = t.early_checkpoints.remove(&h.hand_id()).unwrap_or_default();
                        // Anything older than the boundary now opening is for a
                        // hand the store has released. Dropped here rather than
                        // left to grow.
                        t.early_checkpoints.retain(|k, _| *k > h.hand_id());
                        for held in waiting {
                            if let Some(next) = checkpoint_event(
                                &held,
                                h,
                                &mut t.boundaries,
                                &mut t.readmitted,
                                &mut t.sit_ins,
                                &app_key,
                                &mut t.checkpoint_said,
                                &mut t.frozen,
                                &t.roster_seats,
                                &mut t.no_round_said,
                                &mut t.early_checkpoints,
                                &profile_dir,
                                &events,
                            )
                            .await
                            {
                                if !next.is_empty() {
                                    publish_and_hear(
                                        next,
                                        h,
                                        &mut t.boundaries,
                                        &mut t.readmitted,
                                        &mut t.sit_ins,
                                        &app_key,
                                        &mut t.checkpoint_said,
                                        &mut t.frozen,
                                        &t.roster_seats,
                                        &mut t.no_round_said,
                                        &mut t.early_checkpoints,
                                        &profile_dir,
                                        &events,
                                        &mut swarm,
                                        &mut t.said,
                                        &t.tox_sink,
                                    )
                                    .await;
                                }
                            }
                        }
                        match h.state_hash_event(&app_key, super::node::now_unix_ms()) {
                            Ok(Some(bytes)) => {
                                // Kept whole, because §6.3 step 2's dispute
                                // carries it as evidence: the complete signed
                                // bytes, so a peer whose own copies all agreed
                                // can **verify** a second value at this
                                // checkpoint rather than believe an accusation.
                                t.boundaries.remember_own_event(h.hand_id(), &bytes);
                                // **Published and heard.** A stage counts its
                                // own seat like every other, and neither
                                // GossipSub nor the Tox group delivers a message
                                // back to the peer that sent it — so a copy that
                                // is only published is a copy this peer's own
                                // stage never hears. Measured before this: every
                                // node's checkpoint waiting for exactly one
                                // seat, its own, for every hand of a run.
                                //
                                // Through `checkpoint_event`, which is the one
                                // admission path — a second one here would be a
                                // second set of rules for the same event — and
                                // it answers with this peer's `STATE_ACK` when
                                // the hash stage closes on this very copy, which
                                // then needs publishing and hearing in its turn.
                                publish_and_hear(
                                    bytes,
                                    h,
                                    &mut t.boundaries,
                                    &mut t.readmitted,
                                    &mut t.sit_ins,
                                    &app_key,
                                    &mut t.checkpoint_said,
                                    &mut t.frozen,
                                    &t.roster_seats,
                                    &mut t.no_round_said,
                                    &mut t.early_checkpoints,
                                    &profile_dir,
                                    &events,
                                    &mut swarm,
                                    &mut t.said,
                                    &t.tox_sink,
                                )
                                .await;
                            }
                            Ok(None) => {}
                            Err(e) => {
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "the boundary checkpoint would not seal: {e}"
                                    )))
                                    .await;
                            }
                        }
                    }
                }

                // `S1-BM`: this seat's own request to be dealt back in -- when
                // the hand settled and this seat is outside `R(k)` with chips,
                // once per boundary, at its own window slot on `TERMINAL(k)`.
                // Sent by the client on the player's behalf so nobody clicks
                // anything (READMISSION.md section 3), withheld by `--stay-out`.
                // Published and heard like the checkpoint: this client's own
                // window records that its seat spoke, and `A` gets the seat the
                // way it gets any other.
                if !stay_out && phase1 {
                    let asked = match t.hand.as_mut() {
                        Some(h) => h.sit_in_request(&app_key, super::node::now_unix_ms()),
                        None => Ok(None),
                    };
                    match asked {
                        Ok(Some(bytes)) => {
                            publish_hand(
                                vec![crate::table::hand::Send::Broadcast(bytes.clone())],
                                &mut swarm,
                                &mut t.said,
                                &t.tox_sink,
                            );
                            if let Some(h) = t.hand.as_ref() {
                                let _ = boundary_event(
                                    &bytes,
                                    h,
                                    &mut t.boundaries,
                                    &mut t.readmitted,
                                    &mut t.sit_ins,
                                    &mut t.early_boundary,
                                    &events,
                                )
                                .await;
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "hand #{}: seat {} (mine) is outside the roster with chips, so I asked to sit in at the next hand",
                                        h.hand_id(),
                                        h.my_seat()
                                    )))
                                    .await;
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "the sit-in request would not seal: {e}"
                                )))
                                .await;
                        }
                    }
                }

                if phase1 {
                    t.boundary_done_for = t.hand.as_ref().map(|h| h.hand_id());
                }

                // `S1-BM`: phase 2 is not due yet. The terminal fired this arm so
                // that the window, the checkpoint and the sit-in request above
                // went out at once; the deal itself waits for D-020's pause.
                if let Some(at) = t.deal_at {
                    if tokio::time::Instant::now() < at {
                        t.next_hand_at = Some(at);
                        continue;
                    }
                }
                // `S1-BM`: votes cast here as well as at the stall tick, so a
                // hold below is over as soon as the evidence is, not a tick later.
                if let Some(h) = t.hand.as_mut() {
                    vote_on_returns!(t, h);
                }
                // `S1-BM`: a return is in flight at this boundary -- a seat outside
                // the roster asked to sit in (this client, or one whose request
                // the window took) and no certificate about it has banked here --
                // so the next deal is held, up to `RETURN_GRACE_MS`. Dealt now,
                // hand k+1 would open at a genesis without the seat, and the
                // certificate a moment later could only re-open a stage 0 that a
                // heads-up hand leaves in a tenth of a second (`run164337-3`: nine
                // requests at nine boundaries, no return).
                if let Some(h) = t.hand.as_ref() {
                    let waiting_for = returning_seats(h, &t.sit_ins);
                    if !waiting_for.is_empty() {
                        let hid = h.hand_id();
                        let (since, gave_up) = match t.return_hold {
                            Some((id, s, g)) if id == hid => (s, g),
                            _ => {
                                let s = tokio::time::Instant::now();
                                t.return_hold = Some((hid, s, false));
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "hand #{hid}: holding the next deal for up to {} s while seat(s) {waiting_for:?} return; a certificate is owed first",
                                        RETURN_GRACE_MS / 1000
                                    )))
                                    .await;
                                (s, false)
                            }
                        };
                        if !gave_up {
                            // **A peer has dealt hand k+1 already -- its `HAND_INIT` is
                            // here -- so the boundary is over whatever this client
                            // thinks of it.** The hold ends and hand k+1 is followed from
                            // its first frame; a certificate that lands later still
                            // banks on the retained hand and re-derives k+1 from a stage
                            // 0 nobody has left. Without this the muted seat of
                            // `split172616-9` held six seconds after the table had dealt,
                            // opened hand #3 sixty-four buffered frames late, reached
                            // stage 37 and sat there for the rest of the run (`S1-BW`).
                            let peers_dealt = !t.next_inits.is_empty();
                            if !peers_dealt
                                && since.elapsed() < std::time::Duration::from_millis(RETURN_GRACE_MS)
                            {
                                t.next_hand_at = Some(
                                    tokio::time::Instant::now() + std::time::Duration::from_millis(250),
                                );
                                continue;
                            }
                            t.return_hold = Some((hid, since, true));
                            let line = if peers_dealt {
                                format!(
                                    "hand #{hid}: a peer has dealt hand #{} while seat(s) {waiting_for:?} were returning; following it, and the seat asks again at the next boundary",
                                    hid.saturating_add(1)
                                )
                            } else {
                                format!(
                                    "hand #{hid}: no return certificate within {} s for seat(s) {waiting_for:?}; dealing on, and the seat asks again at the next boundary",
                                    RETURN_GRACE_MS / 1000
                                )
                            };
                            let _ = events.send(NodeEvent::Warning(line)).await;
                        }
                    }
                }
                t.deal_at = None;

                // **This client is on a branch nobody shares.** It cannot
                // catch up and it must not deal on: a private tournament is
                // worse than no tournament, because it looks like one.
                //
                // # This test used to sit ABOVE the checkpoint, and that
                // # stranded every other seat at the table
                //
                // The block above publishes this peer's own checkpoint-8
                // `STATE_HASH` and says in its own words that it is *"the value
                // every other seat compares against, and … the door back for a
                // seat that missed this hand"*. The latch `continue`d before
                // reaching it, so a seat that fell behind **withheld the one
                // value the others needed to close hand `k`'s stage** — and
                // they sat at `checkpoint hand k waiting for [s]` for the rest
                // of the run. Measured across 134 runs: eight showed exactly
                // that, and it was read as a Tox delivery failure for a day
                // because the seat was in the group and 0 ms away.
                //
                // The hash is about a hand that has **already settled**.
                // Withholding it helps nobody and blocks everybody, and the
                // seat is going to stop dealing either way — which is all the
                // latch was ever for.
                //
                // `D-038`: and it is no longer a terminus. The checkpoint above
                // has gone out, so the others are not held up; this client
                // drops its branch and rejoins from the table's copies.
                if let Some((theirs, mine)) = t.adrift {
                    rejoin_from_copies!(t, theirs, mine);
                    continue;
                }

                // A repair the stall tick has not applied yet: the arm must
                // not succeed the hand it is about. A peer's abort arms this
                // timer 800 ms ahead and the tick is two seconds; succeeding
                // here dropped hand k and the tick's re-open then refused on
                // the id (found by reading, not by a run).
                if t.pending_repair.is_some() {
                    t.next_hand_at = Some(
                        tokio::time::Instant::now() + std::time::Duration::from_secs(2),
                    );
                    continue;
                }

                // `S1-BS`: never deal the next hand alone. More seats signed
                // this hand at one other genesis than were counted here, so
                // the hand this client would derive is on a branch nobody
                // shares: it waits — a late certificate about the previous
                // hand can still repair this one, and the adrift latch above
                // ends the wait when the table is two hands on.
                if let Some(h) = t.hand.as_ref() {
                    if h.slot().sequence == 0 {
                        if let Some((g, seats)) = h.foreign_genesis_named() {
                            let counted = h.counted_at_stage_zero();
                            if wait_at_boundary(seats.len(), counted.len()) {
                                if t.genesis_wait_said != Some(h.hand_id()) {
                                    t.genesis_wait_said = Some(h.hand_id());
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "hand #{} ended with no peer at its genesis: seat(s) {:?} hold {} and this client holds {} with {:?} counted; hand #{} is not dealt here until a certificate about hand #{} repairs it, or the table is two hands on and this seat rejoins it (D-038)",
                                            h.hand_id(),
                                            seats,
                                            short_hash(&g),
                                            short_hash(&h.genesis()),
                                            counted,
                                            h.hand_id().saturating_add(1),
                                            h.hand_id().saturating_sub(1)
                                        )))
                                        .await;
                                }
                                t.next_hand_at = Some(
                                    tokio::time::Instant::now() + std::time::Duration::from_secs(30),
                                );
                                continue;
                            }
                        }
                    }
                }
                // `S1-CR`: the boundary reached, on record; a busted seat has
                // nothing to come back to.
                if let Some(h) = t.hand.as_ref() {
                    let me = h.my_seat();
                    // The boundary stack, on both terminal paths. `stacks()` is empty
                    // after an abort, and reading it here forgot the session at every
                    // aborted boundary (`run193358-3`).
                    let stack = h.stack_at_boundary(me);
                    if stack == 0 && h.checkpoint8().is_some() {
                        let _ = crate::storage::session::forget(&profile_dir);
                        t.resume = None;
                        let _ = events
                            .send(NodeEvent::Warning(format!("hand #{}: seat {me} busted; the session record is forgotten", h.hand_id())))
                            .await;
                    } else {
                        remember_session!(t, h.hand_id(), h.terminal().unwrap_or([0; 32]), stack);
                    }
                }
                // `S1-CR`: a resumed client still outside the roster derives
                // nothing. Its `required` is the set that signed the copies it
                // adopted, exact when every copy came and not otherwise, and a
                // derivation from an inexact set is a genesis nobody shares; the
                // table's own copies of the next hand are the safe source until
                // a return certificate has put this seat back, when the
                // checkpoint that earned it has already said the set was exact.
                if t.resuming {
                    if let Some(h) = t.hand.as_ref() {
                        let me = h.my_seat();
                        if !h.required().contains(&me) && !h.returned().contains(&me) {
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "hand #{}: still outside the roster at the boundary; the next hand is adopted from the table's copies, not derived",
                                    h.hand_id()
                                )))
                                .await;
                            t.previous = t.hand.take();
                            continue;
                        }
                        t.resuming = false;
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "back in the roster from hand #{} on; deriving hands again",
                                h.hand_id().saturating_add(1)
                            )))
                            .await;
                    }
                }
                let next = t.hand.as_ref().and_then(|h| h.next_hand());
                // The derivation just made holds every certificate banked so
                // far, of either direction (`S1-BM`), so the late-roster flag a
                // bank on the live hand raised is spent here: re-derived from
                // the retained copy it would name the genesis this client
                // already holds and be refused as such, one log line later.
                if let Some(h) = t.hand.as_mut() {
                    let _ = h.take_late_roster();
                }
                if let (Some(h), Some(o)) = (t.hand.as_ref(), next.as_ref()) {
                    if o.required != h.required_now() {
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "the table changes: required {:?} -> {:?}. {}",
                                h.required_now(),
                                o.required,
                                h.roster_derivation()
                            )))
                            .await;
                    }
                }
                // `S1-CN`: remembered past retention, so a settlement of this
                // hand arriving after the retained copy is gone is still
                // counted as one the late path missed.
                if let Some(h) = t.hand.as_ref() {
                    t.unsettled_abort_here =
                        (h.aborted().is_some() && !h.late_settled()).then_some(h.hand_id());
                }
                // Retained, not dropped: a certificate about this hand that
                // arrives during the next one still banks here (`S1-BS`).
                t.previous = t.hand.take();
                t.hand_reported = false;
                t.deck_reported = None;
                t.cards_reported = false;
                t.turn_reported = None;
                t.abort_reported = false;
                // **All of hand k goes, except the one event hand k ended
                // with.** The five-second loop re-broadcasts this client's own
                // events because GossipSub keeps no history and a peer grafted
                // after a publish never sees it. That reason expires with the
                // hand for every event but the terminal: a peer that missed
                // `HAND_COMPLETE` or `HAND_ABORT` is stuck on hand k, and the
                // terminal is precisely what would free it. Clearing the lot
                // left it to time that peer's own stage deadline out instead.
                //
                // Everything else is an event of a hand nobody is playing —
                // refused by every peer, and, before the hold queue was made to
                // verify, stored by every peer.
                //
                // **And it kept the wrong one.** `said.pop()` keeps whatever
                // was pushed last, and by the time this runs the checkpoint
                // block above has already published — so the entry that
                // survived was a `STATE_HASH` or a `STATE_ACK` and **the
                // terminal was dropped**, which is the one thing the paragraph
                // above says must not be. Both are repairs and both are needed:
                // the terminal frees a peer stuck on hand `k`, and the
                // checkpoint hash is what §4.9's readmission set is written by.
                // So this retains by **kind** rather than by position.
                {
                    use crate::protocol::messages::EventType;
                    let mut terminal: Option<Vec<u8>> = None;
                    let mut checkpoint: Vec<Vec<u8>> = Vec::new();
                    for b in t.said.drain(..) {
                        match crate::net::chained::peek(&b, TABLE_FRAME_PEEK) {
                            Ok((EventType::HandComplete | EventType::HandAbort, _, _)) => {
                                terminal = Some(b)
                            }
                            // `S1-BM`: and this seat's own sit-in request, which
                            // the window at every peer takes until `HAND_INIT(k+1)`
                            // completes there.
                            Ok((
                                EventType::StateHash | EventType::StateAck | EventType::PlayerSitIn,
                                _,
                                _,
                            )) => {
                                checkpoint.push(b)
                            }
                            _ => {}
                        }
                    }
                    t.said.extend(terminal);
                    t.said.extend(checkpoint);
                }
                match next {
                    Some(mut opening) => {
                        // **Read here and cleared here, which is the whole of
                        // `A`'s life** (§4.9). A set read anywhere else is a set
                        // two peers can come to disagree about.
                        opening.readmitted = std::mem::take(&mut t.readmitted);
                        // `S1-BM`: the boundary before the retained hand's is over
                        // for good; hand k's evidence stays while `previous` does.
                        t.sit_ins.release_below(opening.hand_id.saturating_sub(1));
                        // **Said about other seats, and never about this
                        // one.** The `Bystander` arm writes `A` for any roster
                        // seat the checkpoint stage did not count, and this
                        // client's own seat qualifies whenever it did not sign
                        // the hand — so a muted seat announced its own return,
                        // three times, in `split125944-9`.
                        //
                        // **The set itself keeps that seat.** `open_with` builds
                        // `accepted` from `required ∪ readmitted` and decides
                        // from it whether this client seals its own `HAND_INIT`
                        // at stage 0. For a seat still in `required` that is the
                        // same either way; for a certified-out bystander —
                        // the only seat that ever gets here — `readmitted` is
                        // the whole of why it may sign, and taking it out would
                        // silence §4.9's route for the seat it was built for.
                        // So the filter is on the sentence, not on the set.
                        let spoken: Vec<_> = opening
                            .readmitted
                            .iter()
                            .copied()
                            .filter(|s| *s != opening.my_seat)
                            .collect();
                        if !spoken.is_empty() {
                            // **Two different facts, and the line used to assert
                            // one of them for both.** `A` is written from the
                            // checkpoint stage's `signed` set and `R(k+1)` is
                            // derived from `certified`, so a seat this client
                            // heard nothing from — but which nobody certified
                            // out — is in the roster of this hand *and* in `A`.
                            // For that seat "not dealt in" is false. Measured
                            // fifteen times across the corpus in its first half
                            // (in `required`, never signed); the second half has
                            // not co-occurred yet, and the sentence must not
                            // depend on that.
                            let (dealt, out): (Vec<_>, Vec<_>) = spoken
                                .into_iter()
                                .partition(|s| opening.required.contains(s));
                            if !out.is_empty() {
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "seat(s) {out:?} are heard again: they agree at hand #{}'s checkpoint, so their HAND_INIT is accepted while hand #{}'s stage 0 is open. The roster is monotone (D-024), so they are not dealt into it",
                                        opening.hand_id.saturating_sub(1),
                                        opening.hand_id
                                    )))
                                    .await;
                            }
                            if !dealt.is_empty() {
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "seat(s) {dealt:?} agreed with this client's checkpoint for hand #{} without being counted into it here; they are still in the roster and are dealt into hand #{}",
                                        opening.hand_id.saturating_sub(1),
                                        opening.hand_id
                                    )))
                                    .await;
                            }
                        }
                        // Quiet if two seats already signed this hand at one
                        // other genesis and none at this one (`S1-BS`).
                        let voice = quiet_if_contested(&opening, &t.next_inits);
                        begin_hand_with(
                            opening,
                            voice,
                            &mut t.next_inits,
                            &mut t.next_early,
                            &mut t.next_early_lost,
                            &app_key,
                            &mut t.hand,
                            &mut t.said,
                            &mut swarm,
                            &events,
                            &t.tox_sink,
                        )
                        .await;
                    }
                    // Fewer than two seats with chips, or fewer than two that
                    // took part in the hand just played. Either way there is no
                    // hand k+1 and that is the tournament's end condition
                    // rather than a fault.
                    None => {
                        let _ = events
                            .send(NodeEvent::Warning(
                                "the table has no next hand to deal".into(),
                            ))
                            .await;
                        // `D-042`: the tournament is over. Its group is left
                        // once the terminal message has had time to arrive
                        // everywhere; see the housekeeping tick.
                        if t.table_over_at.is_none() {
                            t.table_over_at = Some(std::time::Instant::now());
                        }
                    }
                }
            }

            _ = housekeeping.tick() => {
                let now = super::node::now_unix_ms();
                state.tick(now);
                // `D-040`, §7.5: every `AD_REBROADCAST_MS`, ask every poker peer
                // on the line what it offers. Bounded by the number of poker
                // peers, which is small, and by one question per peer per tick.
                {
                    let peers: Vec<libp2p::PeerId> = poker_peers
                        .iter()
                        .copied()
                        .filter(|p| swarm.is_connected(p))
                        .take(SNAPSHOT_ASKS_PER_TICK)
                        .collect();
                    for p in peers {
                        ask_lobby!(p);
                    }
                }

                // **Who is actually on the line**, said once per housekeeping
                // tick while there is a table.
                //
                // The owner's requirement, and it is not only about freeing
                // seats: *every peer must be pinged to check it is on the
                // line*. Acting on liveness and never showing it leaves a
                // player watching a stalled stage with no way to tell a seat
                // that is thinking from one that is gone — and this client has
                // been pinging four times a minute since the beginning and
                // discarding every reply.
                // `D-043`: what follows is every table's.
                for which in 0..tables.len() {
                    let t = &mut tables[which];
                    mark_table(&events, &mut marked, t).await;
                    if let Some(f) = t.table.as_ref() {
                        let mut line: Vec<String> = Vec::new();
                        for e in f.roster().seats() {
                            if Some(e.seat) == f.my_seat() {
                                continue;
                            }
                            let Ok(p) = PeerId::from_bytes(&e.peer_id) else {
                                line.push(format!("{} ?", e.seat));
                                continue;
                            };
                            line.push(match alive.get(&p) {
                                Some((at, Some(rtt)))
                                    if at.elapsed() < std::time::Duration::from_secs(30) =>
                                {
                                    format!("{} {}ms", e.seat, rtt.as_millis())
                                }
                                Some((at, None))
                                    if at.elapsed() < std::time::Duration::from_secs(30) =>
                                {
                                    format!("{} connected", e.seat)
                                }
                                Some((at, _)) => {
                                    format!("{} silent {}s", e.seat, at.elapsed().as_secs())
                                }
                                None => format!("{} never", e.seat),
                            });
                        }
                        if !line.is_empty() {
                            let (sent, refused, up) = t.tox_sink.invite_counts();
                            let (rejoins, join_fails, confirmed, founder_link) = t.tox_sink.join_trouble();
                            let (seen, want) = t.tox_sink.group_seen();
                            // The same numbers, as a fact rather than a sentence:
                            // the line below is advisory and may be dropped, and
                            // what the client tells its user must not be.
                            let _ = events
                                .send(NodeEvent::Carrier {
                                    seen: u16::try_from(seen).unwrap_or(u16::MAX),
                                    want: u16::try_from(want).unwrap_or(u16::MAX),
                                })
                                .await;
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "seats on the line: {}; tox self {}, group {seen} seen/{confirmed} confirmed/{want} wanted, tox friends up {up}, invites {sent} sent {refused} refused{}{}{}{}",
                                    line.join(", "),
                                    match t.tox_sink.tox_connection() {
                                        0 => "offline",
                                        1 => "tcp",
                                        _ => "udp",
                                    },
                                    // **Only while the table has not settled.** A
                                    // ratification count is the difference between
                                    // "they never arrive" and "they arrive and are
                                    // refused", and it is the number missing from
                                    // every line this client printed while it sat at
                                    // a table it could not start a hand at. Once
                                    // there is a session it is answered and saying
                                    // it every thirty seconds is noise.
                                    if f.session().is_none() {
                                        format!(
                                            ", ratified {}/{}, {} held",
                                            f.ratifiers().len(),
                                            f.roster().len(),
                                            f.held()
                                        )
                                    } else if let Some(b) = t.boundaries.newest() {
                                        // Where the boundary checkpoint has got to.
                                        // A stage that never closes is silent, and
                                        // so is one that closes and agrees.
                                        format!(
                                            ", checkpoint hand {} waiting for {:?}{}",
                                            b.hand_id(),
                                            b.waiting_for(),
                                            if b.ack_stage_complete() {
                                                ", agreed".to_string()
                                            } else if b.hash_stage_complete() {
                                                format!(
                                                    ", hashes in, acks waiting for {:?}",
                                                    b.acks_waiting_for()
                                                )
                                            } else {
                                                String::new()
                                            }
                                        )
                                    } else {
                                        String::new()
                                    },
                                    // **What the hand itself thinks, which nothing
                                    // used to say.**
                                    //
                                    // `Failed::NotYet` is not an error and is not
                                    // logged -- correctly, since an event for a
                                    // stage this client has not reached is ordinary
                                    // weather on a mesh that does not order. But it
                                    // means a client can fall behind in complete
                                    // silence, and in `split182531-10` that is
                                    // exactly what happened: zero dropped packets,
                                    // zero refusals, and `n0` still voted that seat
                                    // 1 was late while the other nine voted that
                                    // seat 7 was, because `n0` believed seat 8 was
                                    // on the clock and they believed seat 6 was.
                                    // Two subjects, both stuck at 8/9, and no
                                    // certificate possible from either.
                                    //
                                    // A held count and whose action this client is
                                    // waiting for would have said so in one line.
                                    // Printed only while a hand is open, and only
                                    // when there is something to say.
                                    match t.hand.as_ref() {
                                        Some(h) if !h.over() => {
                                            let held = h.held();
                                            let owed = h.waiting_for();
                                            if held > 0 || !owed.is_empty() {
                                                format!(", hand waiting for {owed:?}, {held} event(s) held")
                                            } else {
                                                String::new()
                                            }
                                        }
                                        _ => String::new(),
                                    },
                                    // **Said only when it changes.** A count that
                                    // repeats every thirty seconds is a number
                                    // nobody reads; one that appears when it moves
                                    // is the measurement (`S1-AC`).
                                    {
                                        let now = bogons.load(std::sync::atomic::Ordering::Relaxed);
                                        if now > bogons_said {
                                            let since = now - bogons_said;
                                            bogons_said = now;
                                            format!(", {since} private address(es) from the DHT refused ({now} this run)")
                                        } else {
                                            String::new()
                                        }
                                    },
                                    // **Said only when it is not zero.** A healthy
                                    // run never prints this, so its presence is the
                                    // whole message: `S1-AA` shape (i) happened and
                                    // was survived rather than sat through.
                                    if rejoins > 0 || join_fails > 0 {
                                        format!(
                                            ", group join restarted {rejoins} time(s), {join_fails} abandoned by toxcore, founder link {}",
                                            match founder_link {
                                                0 => "down",
                                                1 => "over a TCP relay",
                                                2 => "direct over UDP",
                                                _ => "n/a (this client is the founder)",
                                            }
                                        )
                                    } else {
                                        String::new()
                                    }
                                )))
                                .await;
                        }

                        // **When the group will not fill, say how far this client
                        // reached at all.**
                        //
                        // The owner's rule is that a TCP relay is a **fallback that
                        // must work**, not a fault to report — it is why the node
                        // list is refreshed over HTTPS and why every node is added
                        // to toxcore's relay list as well as its DHT. So the useful
                        // sentence is not *"UDP is blocked, check your firewall"*,
                        // which tells a player to fix what the client is supposed to
                        // survive. It is **which half of the fallback failed**.
                        //
                        // Every one of those calls was `let _ =`, so when an evening
                        // of runs died with `tox self tcp, tox friends up 0, invites
                        // 0 sent`, nothing could say whether zero relays had been
                        // added or all of them had and the friendships failed
                        // anyway. Those are different faults. `S1-U`.
                        if !t.udp_warned && {
                            let (seen, want) = t.tox_sink.group_seen();
                            want > 0 && seen < want
                        } {
                            t.udp_warned = true;
                            let r = t.tox_sink.reach();
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "the table's group is not filling. Tox is {}; of {} known nodes, {} bootstrapped and {} TCP relays were accepted{}. Game traffic rides that group, so nothing can be dealt until it fills.",
                                    match t.tox_sink.tox_connection() {
                                        0 => "offline",
                                        1 => "reachable only through a TCP relay",
                                        _ => "on UDP",
                                    },
                                    r.nodes,
                                    r.booted,
                                    r.relays,
                                    if r.refreshed {
                                        ", from a list refreshed on this start"
                                    } else {
                                        ", from the stored list"
                                    }
                                )))
                                .await;
                        }

                        // **Everything this seat has to say, said again, while the
                        // table has not settled.**
                        //
                        // A ratification is published once, into a mesh the last
                        // joiner may not be grafted into yet, and **nothing ever
                        // sends it again**: `say_again` fires on `Subscribed`, which
                        // for a peer that is already subscribed never fires a second
                        // time. So one lost `TABLE_READY` costs its receiver the
                        // whole tournament — it holds a full roster, cannot compute
                        // a `session_id` without every ratification, and
                        // `Opening::from_formation` then returns `None` for ever.
                        //
                        // Measured: a seat reporting `ratified 3/4, 0 held` for a
                        // whole run — nothing refused, nothing waiting, one
                        // ratification simply never delivered — while the other
                        // three played fifteen hands without it.
                        //
                        // Inside `duplicate_cache_time` (120 s) a seat's repeat is
                        // still refused as a duplicate and only the founder's
                        // re-signed roster goes out; after it, the repeat is carried.
                        // That makes the gap two minutes rather than for ever, which
                        // is the whole of what this can do without the wire decision
                        // `S1-P` leaves open.
                        // **`!ever_dealt`, not `session().is_none()`.** A seat
                        // whose own table has settled still holds a `TABLE_READY`
                        // that a neighbour may be missing — and the gate that used
                        // to stand here shut exactly when that seat became able to
                        // help. Measured: `n1` sat at `ratified 3/4, 0 held` for a
                        // whole run while three settled seats beside it held the
                        // copy it lacked and said nothing (`S1-P`).
                        //
                        // It still ends: a table that has dealt has no formation
                        // left to repair, and after that the repeat would be noise.
                        if !t.ever_dealt {
                            if let Some(topic) = t.table_topic.as_ref() {
                                for bytes in f.say_again(super::node::now_unix_ms()) {
                                    // The same second path as the `Subscribed`
                                    // repeat above, for the same reason (`S1-P`).
                                    t.tox_sink.try_broadcast(&bytes);
                                    let _ = swarm
                                        .behaviour_mut()
                                        .gossipsub
                                        .publish(topic.clone(), bytes);
                                }
                            }
                        }
                    }

                    // **Who this client can actually reach on the lobby topic.**
                    // A publish that returns `Ok` says only that it went somewhere.
                    // Measured on three clients on one machine, all discovered by
                    // mDNS within a second: `0 mesh peer(s) of 2` — both peers
                    // subscribed, neither grafted, so an advert reaches them only
                    // as an explicit peer or by gossip pull, and a table that
                    // should form in seconds took minutes. Nothing in the log
                    // distinguished that from "nobody is there" before this line.
                    {
                        let g = &swarm.behaviour().gossipsub;
                        let hash = topics.lobby.hash();
                        let mesh = g.mesh_peers(&hash).count();
                        let known = g
                            .all_peers()
                            .filter(|(_, subscribed)| subscribed.contains(&&hash))
                            .count();
                        // **Two different quantities, and the line used to read as
                        // if they were one.** `mesh` counts peers GRAFTED into the
                        // gossipsub mesh for this topic; `known` and `who` count and
                        // name the peers SUBSCRIBED to it. The message was
                        // `"{mesh} of {known} subscribed {who:?}"`, which reads as
                        // *none of the eight are subscribed* and then lists eight
                        // subscribers — the opposite of what it measures. A
                        // nine-seat run on the libp2p path was read that way, as a
                        // subscription failure, when what it says is that all eight
                        // are subscribed and **none is grafted**: adverts then
                        // arrive only by gossip pull, which is the "a table that
                        // should form in seconds took minutes" symptom above.
                        //
                        // Named, not counted. The founder reporting one subscriber
                        // while both joiners report two is the standing clue, and a
                        // count cannot say WHICH peer is missing.
                        let who: Vec<String> = g
                            .all_peers()
                            .filter(|(_, subscribed)| subscribed.contains(&&hash))
                            .map(|(p, _)| p.to_string().chars().rev().take(6).collect::<String>())
                            .collect();
                        // **Both sides of the comparison, named.** A table forms
                        // exactly when the founder sees every joiner subscribed to
                        // the lobby topic, and fails when it sees one of two while
                        // reporting both connected — measured, five runs. A count
                        // cannot say WHICH peer is connected and not subscribed,
                        // and that is the whole question.
                        // **Which protocol gossipsub thinks each subscriber
                        // speaks, because that decides whether it can be grafted at
                        // all.** `all_peers` — which `known` and `who` are built
                        // from — does not filter on `PeerKind`, while
                        // `get_random_peers_dynamic`, which is what the heartbeat
                        // fills the mesh from, requires `p.kind.is_gossipsub()`
                        // (`libp2p-gossipsub-0.49.5/src/behaviour.rs:3541` and
                        // `types.rs:166`). A peer whose protocol is not yet
                        // negotiated, or is Floodsub, or is NotSupported, therefore
                        // counts as subscribed here and is invisible to the mesh.
                        //
                        // That is two different failures with one appearance, which
                        // is the shape this project keeps paying for. Non-zero means
                        // the protocol; zero means backoff or score.
                        //
                        // **Compared as text, because `PeerKind` is not exported.**
                        // `peer_protocol()` returns `&PeerKind` publicly and the
                        // type is absent from the crate's `pub use` list
                        // (`lib.rs:119`), so the variants cannot be matched. The
                        // three names below are `types.rs:126–137`. The polarity is
                        // deliberate: they are the **healthy** kinds, so a library
                        // that renames one makes this line shout about every peer
                        // rather than fall silent about a broken one.
                        const GRAFTABLE: [&str; 3] = ["Gossipsubv1_2", "Gossipsubv1_1", "Gossipsub"];
                        let subscribed: std::collections::HashSet<libp2p::PeerId> = g
                            .all_peers()
                            .filter(|(_, subs)| subs.contains(&&hash))
                            .map(|(p, _)| *p)
                            .collect();
                        let not_gossipsub = g
                            .peer_protocol()
                            .filter(|(p, _)| subscribed.contains(p))
                            .filter(|(_, kind)| !GRAFTABLE.contains(&format!("{kind:?}").as_str()))
                            .count();
                        let connected: Vec<String> = poker_peers
                            .iter()
                            .map(|p| p.to_string().chars().rev().take(6).collect::<String>())
                            .collect();
                        // **A table nobody can see is worth saying out loud.**
                        // Measured: one run in five, the founder found no local peer
                        // at all — no mDNS discovery, no connection — while the two
                        // joiners found each other. Its log said `hosting T`, then
                        // `public lobby: nobody else yet`, and nothing else for the
                        // rest of the run. From the outside that is
                        // indistinguishable from a table waiting for players, and
                        // it is the opposite: the players are there and this client
                        // is alone.
                        if connected.is_empty() && !t.alone_said {
                            t.alone_said = true;
                            let _ = events
                                .send(NodeEvent::Warning(
                                    "no other poker client has been reached yet — a table hosted now is one nobody can see. On one machine that is local discovery failing; across networks it is the relay."
                                        .into(),
                                ))
                                .await;
                        }
                        if !connected.is_empty() {
                            t.alone_said = false;
                        }
                        if known > 0 || mesh > 0 || !connected.is_empty() {
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "lobby topic: {mesh} of {known} subscribed peers grafted ({not_gossipsub} cannot be, wrong or unnegotiated protocol); subscribed {who:?}; connected {connected:?}"
                                )))
                                .await;
                        }
                    }
                    // Nothing is dropped silently. "No warnings" and "the warnings
                    // were thrown away" must not look the same in a log.
                    let lost = events.take_dropped();
                    if lost > 0 {
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "{lost} advisory event(s) dropped: the log could not keep up, and the node did not wait for it"
                            )))
                            .await;
                    }
                    // Told to the interface as well, which keeps its own copy and
                    // cannot see this clock.
                    let _ = events.send(NodeEvent::Swept { now_ms: now }).await;

                    // Re-broadcast this node's own table.
                    //
                    // Through the `Formation`, which owns the table key and the
                    // advertisement together. It used to be published from a
                    // separate copy here, and the founder's own record of what it
                    // had signed then went stale: a joiner naming the copy it heard
                    // was refused with "the advertisement has expired", which is the
                    // defect the first two-instance run found and which no unit test
                    // could have, because it needs thirty seconds to appear.
                    // **A closed table is advertised too, and the sentence that
                    // used to stand here is why it was not.** It read: *a full
                    // tournament under way has nobody to attract — it is a closed
                    // group now — and a lobby listing it is a lobby listing a door
                    // that does not open.* True of strangers, and false of the one
                    // person who needs it most.
                    //
                    // The advert is **how a seat that was disconnected finds its way
                    // back**. Stop it and the advert expires at `AD_TTL_MS`, and a
                    // player whose client restarts ninety seconds later has no way
                    // to find the table at all: no advert, no address, nothing.
                    // Measured, twice — a client back after twenty seconds ran out
                    // its whole remaining life reporting `NO TABLE` while the table
                    // played thirty hands without it. D-022 gives that seat two
                    // hands of allowance and the transport gave it no way to spend
                    // them.
                    //
                    // **That reason is gone and so is the advertising, once a hand
                    // has been dealt.** The owner has ruled the restarting client
                    // out of scope — *“ten peer má smůlu”* — and a client whose link
                    // merely dropped never lost its table and never looks in the
                    // lobby for it. What the advert did keep open was a door for
                    // strangers: `S1-T`, where the paragraph above was wrong on its
                    // own terms. *“A stranger's `JOIN_REQUEST` is refused as the
                    // table is full”* is false for any table that started at
                    // `min_players_to_start` below `max_players` — this client's own
                    // default — and accepting one un-ratifies a table in the middle
                    // of a tournament. §7.3 withdraws a table's advert when it
                    // starts, `reason = 1`, and this is that withdrawal by omission.
                    if let Some(f) = t.table
                        .as_mut()
                        .filter(|f| f.is_founder() && !t.ever_dealt)
                    {
                        match f.readvertise(now, AD_TTL_MS) {
                            Ok(bytes) => {
                                let n = bytes.len();
                                match swarm
                                    .behaviour_mut()
                                    .gossipsub
                                    .publish(topics.lobby.clone(), bytes.clone())
                                {
                                    Ok(_) => {
                                        let _ = events.send(NodeEvent::Published { bytes: n }).await;
                                        // And into this node's own lobby, so a
                                        // founder sees the table it is sitting at.
                                        show_own_table(
                                            &bytes,
                                            &my_app_key,
                                            now,
                                            &mut state.limits,
                                            &mut state.lobby,
                                            &events,
                                        )
                                        .await;
                                    }
                                    Err(e) => {
                                        // No mesh peer yet is the ordinary case at
                                        // start-up, not a fault.
                                        let _ = events
                                            .send(NodeEvent::Warning(format!("not published: {e}")))
                                            .await;
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = events
                                    .send(NodeEvent::Warning(format!("own advert: {e:?}")))
                                    .await;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// `D-043`: which table a swarm event is for. A gossip message, a
/// subscription: the table whose topic it names. A join request: the table
/// whose id it names (the founder of several answers for the right one). A
/// join answer: the table whose request it answers. Everything else: the
/// active table, which is where a lobby event or a connection lands as it
/// always did.
fn table_for_event(
    tables: &[TableRun],
    event: &SwarmEvent<PokerBehaviourEvent>,
    active: usize,
    join_pending: &std::collections::HashMap<libp2p::request_response::OutboundRequestId, usize>,
) -> usize {
    let by_topic = |topic: &gossipsub::TopicHash| {
        tables
            .iter()
            .position(|t| t.table_topic.as_ref().is_some_and(|x| *topic == x.hash()))
    };
    let found = match event {
        SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(gossipsub::Event::Message {
            message,
            ..
        })) => by_topic(&message.topic),
        SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
            gossipsub::Event::Subscribed { topic, .. } | gossipsub::Event::Unsubscribed { topic, .. },
        )) => by_topic(topic),
        SwarmEvent::Behaviour(PokerBehaviourEvent::Join(request_response::Event::Message {
            message: request_response::Message::Request { request, .. },
            ..
        })) => super::joinwire::receive_join_request(request)
            .ok()
            .and_then(|(req, _, _)| {
                tables
                    .iter()
                    .position(|t| t.table.as_ref().is_some_and(|f| f.table_id() == req.table_id))
            }),
        SwarmEvent::Behaviour(PokerBehaviourEvent::Join(request_response::Event::Message {
            message: request_response::Message::Response { request_id, .. },
            ..
        })) => join_pending.get(request_id).copied(),
        _ => None,
    };
    found.unwrap_or(active).min(tables.len().saturating_sub(1))
}

/// `D-043`: tell the window which table what follows is about, when that
/// changes -- the slot's number and the table's key once it has one.
async fn mark_table(events: &Events, marked: &mut Option<(u8, Option<[u8; 32]>)>, t: &TableRun) {
    let key = t.table.as_ref().map(|f| f.table_id());
    if *marked != Some((t.slot, key)) {
        *marked = Some((t.slot, key));
        let _ = events.send(NodeEvent::AtTable { slot: t.slot, key }).await;
    }
}

/// `D-043`: the earliest of one timer across the tables, with its table.
fn earliest(
    tables: &[TableRun],
    at: impl Fn(&TableRun) -> Option<tokio::time::Instant>,
) -> Option<(usize, tokio::time::Instant)> {
    tables
        .iter()
        .enumerate()
        .filter_map(|(i, t)| at(t).map(|when| (i, when)))
        .min_by_key(|(_, when)| *when)
}

/// `D-043`: the next message from any table's group, with the table it came
/// from. A slot without a group never answers; a closed transport is waited
/// on for ever rather than reported, as one table's `next` always was.
async fn next_from_tables(
    tables: &mut [TableRun],
) -> Option<(usize, crate::table::transport::FromTable)> {
    let futures: Vec<_> = tables
        .iter_mut()
        .enumerate()
        .map(|(i, t)| Box::pin(async move { (i, t.tox_sink.next().await) }))
        .collect();
    let ((i, item), _, _) = futures::future::select_all(futures).await;
    match item {
        Some(item) => Some((i, item)),
        None => std::future::pending().await,
    }
}


/// Tell the interface what table this is.
///
/// Sent whenever this client joins one or founds one, because the window that
/// draws it must not have to guess a seat count.
async fn report_params(events: &Events, f: &Formation) {
    let ad = f.advert();
    let _ = events
        .send(NodeEvent::TableParams {
            key: f.table_id(),
            name: ad.table_name.clone(),
            seats: ad.max_players,
            needed: ad.min_players_to_start,
            small_blind: ad.small_blind,
            big_blind: ad.big_blind,
            action_ms: u64::from(ad.action_timeout_ms),
        })
        .await;
}

/// Tell the interface who is seated.
///
/// The whole roster every time rather than a difference: a difference is only
/// correct if the receiver never missed one, and this channel is bounded and
/// drops under load by design.
/// Tell the Tox driver which seats the roster now holds.
///
/// Called wherever the roster is reported, because the two answer the same
/// question: who is at this table. The founder adds each as a Tox friend and
/// invites it into the group; a seat with no Tox key is one this table cannot
/// reach that way and is passed over.
///
/// **Idempotent, and it has to be.** The roster is re-reported on every change,
/// and `Command::Seated` for a key already added is a `tox_friend_add_norequest`
/// that refuses and changes nothing. Diffing instead would mean keeping a
/// second copy of the roster in this loop to diff against, which is a second
/// answer to the same question.
fn seat_on_tox(f: &Formation, tox: &super::toxsink::TableSink) {
    if !tox.is_on_tox() {
        return;
    }
    // `D-044`: the whole roster, so the driver drops the seats it no longer
    // names as well as taking the new ones -- a joiner's driver had never
    // been told a seat was gone (`run130909-3`).
    let keys: Vec<[u8; 32]> = f.roster().seats().iter().filter_map(|e| e.tox_key).collect();
    // `D-045`: a roster with no seat is not the table's word (`run140845-3`).
    if keys.is_empty() {
        return;
    }
    tox.tell(super::toxsink::Seat::Roster(keys));
}

async fn report_roster(events: &Events, f: &Formation) {
    // `S1-DG`: the seat, whenever the roster is said and the formation knows
    // it. A seat that came back through *already seated* learned its number
    // from the founder's list and told the window nothing -- `Seated` went
    // out only with a fresh acceptance -- so the window had no hero and drew
    // the player's own cards face down while the hand's name beside them
    // read the cards it held. Said again with every roster; the window
    // notes it only when it changes.
    if let Some(seat) = f.my_seat() {
        let _ = events
            .send(NodeEvent::Seated {
                key: f.table_id(),
                seat,
            })
            .await;
    }
    let seats = f
        .roster()
        .seats()
        .iter()
        .map(|e| (e.seat, e.display_name.clone(), e.buyin))
        .collect();
    let _ = events
        .send(NodeEvent::Roster {
            key: f.table_id(),
            seats,
        })
        .await;
}

/// The `event_hash` of an advert this client just published.
///
/// Taken from the bytes rather than recomputed from the value, because it is the
/// bytes a joiner will name and the founder will look up.
fn advert_hash_of(bytes: &[u8]) -> [u8; 32] {
    use crate::protocol::messages::SignedEvent;
    match crate::protocol::serialization::from_canonical::<SignedEvent>(bytes, LOBBY_MSG_MAX) {
        Ok(signed) => crate::protocol::transcript::event_hash(&signed.body),
        // Unreachable for bytes this client produced a moment ago; a zero hash
        // simply makes every join request name an advert nobody signed, which
        // fails closed.
        Err(_) => [0u8; 32],
    }
}

/// §7.2's bound on a table name, in **bytes**.
pub const TABLE_NAME_MAX: usize = 64;

/// `D-043`: how many tables one client sits at, at most -- the owner's four.
pub const MAX_TABLES: usize = 4;

/// `D-042`: how long after a tournament's end its group is left. Two
/// rounds of the five-second resend loop, so a seat that missed the terminal
/// message hears it again before anybody is gone.
pub const TOURNAMENT_LEAVE_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

/// `S1-DT`: how long a member of the table's group may be silent before the
/// felt says it is off the line. Members ping each other every twelve
/// seconds, so a live one is never this quiet; a client that died is, long
/// before the library gives it up at 58 s.
pub const QUIET_LIMIT_S: u64 = 20;

/// Cut a string to at most `max` bytes, without cutting a character in half.
///
/// The obvious `chars().take(n)` counts the wrong thing and `&s[..n]` panics on
/// a boundary. This is the only correct shape and it is worth having once:
/// every bound the protocol states on a display field is a bound in bytes,
/// because that is what goes on the wire, while every bound a person has in mind
/// is in characters.
pub fn trim_to_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}


/// The advertisement for a table this client is founding.
///
/// `CUSTOM` and not a name of its own: §7.2 rule 3 admits exactly two preset
/// identifiers, and a name that asserts values nothing checks is how two clients
/// ship different tables under one identity.
#[allow(clippy::too_many_arguments)]
fn new_table(
    name: String,
    seats: u8,
    min_players: u8,
    buyin: u64,
    password_required: bool,
    founder_app_key: [u8; 32],
    founder_peer_id: Vec<u8>,
    now_ms: u64,
) -> TableAd {
    use super::lobby::{BlindSchedule, DECK_SUITE_V1};
    use crate::protocol::constants::hand_deadline_min_ms;

    let seats = seats.clamp(2, MAX_SEATS);
    // `D-034`: thirty seconds to decide, three for the network, no reserve.
    let (action, grace, crypto, delay) = (
        crate::protocol::constants::DECISION_MS,
        crate::protocol::constants::DECISION_GRACE_MS,
        30_000u32,
        7_000u32,
    );
    TableAd {
        game: 1,
        mode: 1,
        preset_id: "CUSTOM".into(),
        // Trimmed rather than refused: §7.2 caps it, and a table that will not
        // advertise because its name is long is a worse answer than one that
        // advertises under a shorter name.
        //
        // **By bytes, and the first version counted characters.** §7.2's bound
        // is 64 *bytes*; thirty-two characters of anything outside ASCII is up
        // to a hundred and twenty-eight of them, so a Czech or an emoji table
        // name passed this trim and was then refused by this client's own
        // admission rules — the table would have appeared in nobody's lobby,
        // including its founder's.
        table_name: trim_to_bytes(&name, TABLE_NAME_MAX),
        small_blind: 10,
        big_blind: 20,
        ante: 0,
        // The buy-in the founder chose is inside the range it advertises, which
        // is what `SeatEntry::admissible` checks its own seat against.
        min_buyin: buyin.min(200),
        max_buyin: buyin.max(2_000),
        start_stack: 0,
        players: 1,
        max_players: seats,
        min_players_to_start: min_players.clamp(2, seats),
        blind_schedule: BlindSchedule {
            mode: 1,
            every_n_hands: 20,
            first_small_blind: 10,
            small_blind_cap: 1_000,
        },
        action_timeout_ms: action,
        action_grace_ms: grace,
        crypto_step_timeout_ms: crypto,
        hand_deadline_ms: hand_deadline_min_ms(
            seats,
            action as u64,
            grace as u64,
            crypto as u64,
            delay as u64,
            0,
        ) as u32,
        // `D-034`: no reserve; the fold follows the thirty seconds and the grace.
        time_bank_ms: 0,
        join_deadline_ms: 120_000,
        hand_delay_ms: delay,
        button_rule: 1,
        odd_chip_rule: 1,
        showdown_policy: 1,
        password_required,
        deck_suite: DECK_SUITE_V1.into(),
        founder_app_key,
        founder_peer_id,
        timestamp_unix_ms: now_ms,
        expires_at_unix_ms: now_ms + AD_TTL_MS,
        // `TableAd::on_tox` fills these once the group exists; see there.
        founder_tox_key: None,
        tox_chat_id: None,
    }
}

/// Show this node its **own** table, once the network has actually taken it.
///
/// GossipSub does not deliver a message back to whoever published it, so the
/// founder's own advertisement never reached the founder's own lobby: a player
/// created a table and it appeared in everybody's list except theirs.
///
/// It is fed through `advert::receive` — the same path a stranger's advert takes,
/// with the same rate limiter and the same §7.2 admission — rather than inserted.
/// That is deliberate and it buys a real check: **this client's table appears in
/// this client's lobby only if this client's own advertisement would have been
/// accepted by anybody else.** An advert that its own author refuses is a table
/// nobody will ever see, and this is the cheapest possible place to find that out.
///
/// Called only after `publish` returned `Ok`, which is what the request was: a
/// table shows up once it is on the network, not once it is signed. `publish`
/// fails with `NoPeersSubscribedToTopic` when nothing heard it, and a table that
/// nothing heard is not in a lobby anywhere.
async fn show_own_table(
    bytes: &[u8],
    me: &[u8; 32],
    now: u64,
    limits: &mut RateLimiter,
    store: &mut LobbyStore,
    events: &Events,
) {
    match advert::receive(bytes, *me, now, limits, store) {
        Ok(key) => {
            if let Some(held) = store.get(&key) {
                let _ = events
                    .send(NodeEvent::TableSeen {
                        key,
                        ad: Box::new(held.ad.clone()),
                        params_hash: held.params_hash,
                        advert_hash: held.advert_hash,
                    })
                    .await;
            }
        }
        // `NotNewer` is the ordinary case on a re-broadcast this node has
        // already filed, and is not worth a line in anybody's log.
        Err(super::advert::NotAccepted::NotTaken(super::lobby::NotTaken::NotNewer)) => {}
        Err(e) => {
            // This client's own advert, refused by this client's own rules. It
            // is worth saying loudly, because every other client will refuse it
            // too and the table will simply never appear anywhere.
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "this client refuses its own advertisement: {e:?}"
                )))
                .await;
        }
    }
}

/// Judge one gossip message, and say what GossipSub should do with it.
///
/// Returns the acceptance rather than reporting it, so the answer is given in
/// one place and no branch can forget to give it.
///
/// # Why the topic is checked here, and was not checked at all
///
/// This node subscribes to two topics and the old arm read neither, so a chat
/// message was handed to the advert parser — which rejected it as malformed and
/// blamed the sender for speaking correctly on the other channel. That is
/// `PROTOCOL.md` §4.0 step 6, and it was missing.
async fn handle_gossip(
    message: &gossipsub::Message,
    from: libp2p::PeerId,
    topics: &Topics,
    state: &mut NodeState,
    events: &Events,
) -> gossipsub::MessageAcceptance {
    // Size first: free, the cap is the protocol's, and a message over it cannot
    // be a conforming one. `Reject`, because the sender chose the size.
    if !worth_parsing(message, LOBBY_MSG_MAX) {
        let _ = events
            .send(NodeEvent::TableRefused {
                reason: format!("over the lobby cap: {} bytes", message.data.len()),
            })
            .await;
        return gossipsub::MessageAcceptance::Reject;
    }

    if message.topic != topics.lobby.hash() {
        return if message.topic == topics.lobby_chat.hash() {
            let now = super::node::now_unix_ms();
            match super::lobbytalk::receive(&message.data, peer_bytes(&from), now, &mut state.limits)
            {
                Ok(super::lobbytalk::Heard::Here { who, nickname }) => {
                    let _ = events.send(NodeEvent::LobbyHere { who, nickname }).await;
                    gossipsub::MessageAcceptance::Accept
                }
                Ok(super::lobbytalk::Heard::Said {
                    who,
                    nickname,
                    text,
                }) => {
                    let _ = events
                        .send(NodeEvent::LobbySaid {
                            who,
                            nickname,
                            text,
                        })
                        .await;
                    gossipsub::MessageAcceptance::Accept
                }
                // Rate limiting is about this client's own budget and says
                // nothing about the message, so it is not forwarded and not
                // condemned. Everything else is a judgement.
                Err(super::lobbytalk::NotHeard::TooMuch) => gossipsub::MessageAcceptance::Ignore,
                Err(_) => gossipsub::MessageAcceptance::Reject,
            }
        } else {
            // A topic this node never subscribed to has no business arriving.
            gossipsub::MessageAcceptance::Reject
        };
    }

    // The sending peer, which is **not** the table key: a peer relaying somebody
    // else's advert is the ordinary case.
    let mut peer = [0u8; 32];
    let bytes = from.to_bytes();
    let take = bytes.len().min(32);
    peer[..take].copy_from_slice(&bytes[..take]);

    let now = super::node::now_unix_ms();
    match advert::receive(&message.data, peer, now, &mut state.limits, &mut state.lobby) {
        Ok(key) => {
            // The record the interface needs to draw a row and to build a
            // join request, taken from this node's own store rather than
            // re-derived — one place decides what was accepted.
            if let Some(held) = state.lobby.get(&key) {
                let _ = events
                    .send(NodeEvent::TableSeen {
                        key,
                        ad: Box::new(held.ad.clone()),
                        params_hash: held.params_hash,
                        advert_hash: held.advert_hash,
                    })
                    .await;
            }
            gossipsub::MessageAcceptance::Accept
        }
        Err(e) => {
            let _ = events
                .send(NodeEvent::TableRefused {
                    reason: format!("{e:?}"),
                })
                .await;
            // The same distinction as everywhere else: only a finding about the
            // **sender** costs it anything. A rate limit is about this client's
            // own budget and a full lobby is about its own memory; neither is
            // evidence the peer did wrong.
            match e {
                advert::NotAccepted::RateLimited | advert::NotAccepted::NotTaken(_) => {
                    gossipsub::MessageAcceptance::Ignore
                }
                _ => gossipsub::MessageAcceptance::Reject,
            }
        }
    }
}
/// The routing table, as something that can be written down.
///
/// Only addresses that will still mean something tomorrow, and only the first
/// few per peer: a public node advertises nine or more and this client can dial
/// two of those forms.
fn harvest(
    swarm: &mut libp2p::Swarm<super::swarm::PokerBehaviour>,
) -> Vec<(libp2p::PeerId, Vec<Multiaddr>)> {
    use super::peerbook::{worth_keeping, MAX_ADDRS_PER_PEER, MAX_PEERS};

    let mut out = Vec::new();
    for bucket in swarm.behaviour_mut().ipfs_kad.kbuckets() {
        for entry in bucket.iter() {
            let addrs: Vec<Multiaddr> = entry
                .node
                .value
                .iter()
                .filter(|a| worth_keeping(a))
                .take(MAX_ADDRS_PER_PEER)
                .cloned()
                .collect();
            if !addrs.is_empty() {
                out.push((*entry.node.key.preimage(), addrs));
            }
            if out.len() >= MAX_PEERS {
                return out;
            }
        }
    }
    out
}

/// Turn the public DHT down while a player is at a table, and up again after.
///
/// **Down, not off.** Leaving the network entirely would cost the table its
/// advertisement — a founder that stops re-broadcasting disappears from every
/// lobby and nobody else can join — and it would cost the relay reservation,
/// which took two to three minutes to obtain. Coming back would mean the whole
/// bootstrap again.
///
/// What is turned down is the part that is pure service to strangers: in server
/// mode this node answers Kademlia queries for the whole public network, and at
/// a table that is bandwidth taken from the game. Client mode keeps every query
/// this client makes and stops every query it answers. It is one field, and it
/// goes back up the moment the player stands.
fn dht_effort(swarm: &mut libp2p::Swarm<super::swarm::PokerBehaviour>, at_a_table: bool) {
    let kad = &mut swarm.behaviour_mut().ipfs_kad;
    if at_a_table {
        kad.set_mode(Some(libp2p::kad::Mode::Client));
    } else {
        // `None` is not "client": it restores libp2p's own rule, which is
        // server once there is a confirmed external address. Pinning it to
        // server here would make a NATed client claim to serve queries it
        // cannot be reached for.
        kad.set_mode(None);
    }
}

/// Start hand `k` and put this client's `HAND_INIT` on the table's mesh.
///
/// **Idempotent.** `TableReal` is emitted from more than one place and fires
/// again on later table traffic, and two `HAND_INIT`s under one seat would read
/// as equivocation to everybody else — one seat, two different events, one
/// stage — which is exactly the thing `table::stage` reports as a finding.
#[allow(clippy::too_many_arguments)]
async fn begin_hand(
    opening: crate::table::hand::Opening,
    app_key: &ed25519_dalek::SigningKey,
    hand: &mut Option<crate::table::hand::Hand>,
    said: &mut Vec<Vec<u8>>,
    swarm: &mut libp2p::Swarm<super::swarm::PokerBehaviour>,
    events: &Events,
    tox: &super::toxsink::TableSink,
) {
    let mut none: Vec<(u8, Vec<u8>)> = Vec::new();
    // **Hand one, which has no boundary behind it and therefore no buffer.**
    // What the loop holds for the hand after the running one belongs to the
    // loop and stays there; `reopen_hand` does not see it either, and for the
    // same reason: it re-opens the hand this client is already in, so the
    // buffer is still keyed on the hand after that one.
    let mut none_either: Vec<(u8, Vec<u8>)> = Vec::new();
    begin_hand_with(
        opening,
        crate::table::hand::Voice::Speak,
        &mut none,
        &mut none_either,
        // This path opens hand one, where nothing can have arrived early.
        &mut (0, 0),
        app_key,
        hand,
        said,
        swarm,
        events,
        tox,
    )
    .await;
}

/// How many of the next hand's `HAND_INIT`s one **seat** may leave here.
///
/// One for the hand it opens, one for a re-send. A seat may seal its
/// `HAND_INIT` at as many genesis values as it likes — each is a different
/// signed frame and the byte-identical dedup does not touch them — so without
/// a per-seat term one seat fills this buffer alone and the other nine are
/// starved out of the boundary they open on. `quiet_if_contested` survives that
/// (it folds by seat into a `BTreeSet`), the hand's stage 0 does not.
const NEXT_INITS_PER_SEAT: usize = 2;

/// How many of the next hand's `HAND_INIT`s are kept for it while this one
/// ends: **one per seat, and a little room for a re-send** — which is what this
/// constant has always said and, until `S1-CE`'s panel read the admission arm,
/// not what it did. The arm tested this total and a byte-identical dedup and
/// nothing else.
const NEXT_INITS_CAP: usize =
    (crate::protocol::constants::MAX_SEATS as usize) * NEXT_INITS_PER_SEAT;

/// How many of the next hand's other events are kept for it.
///
/// **A slot bound; the one that binds is the byte budget**
/// (`NEXT_EARLY_BYTES`). It is `MAX_SEATS` times the per-seat allowance so the
/// two are one rule counted twice, and `EARLY_CAP` is pinned above the sum of
/// both pre-open buffers by a `const` assertion — which until `S1-CD` held by
/// luck.
///
/// The arithmetic this doc used to carry — *sixteen `HAND_INIT`s and thirty-two
/// of these is forty-eight, which covers a full table's cryptographic stage* —
/// was wrong twice over and both halves are measured: a nine-handed table's run
/// before the first bet is **forty** frames, not thirty-two, and each slot cost
/// `FRAME_CAP` rather than its own type's cap.
const NEXT_EARLY_CAP: usize =
    (crate::protocol::constants::MAX_SEATS as usize) * NEXT_EARLY_PER_SEAT;

/// And how many of them one seat may contribute.
///
/// **The sibling buffer needs no such bound and this one does.** A seat has one
/// `HAND_INIT` per hand, so `NEXT_INITS_CAP` is bounded per seat by the
/// protocol; this buffer holds stage traffic, and a seat may sign as much of
/// that as it likes. Without a per-seat bound one seated peer fills all
/// thirty-two slots and the bystander loses the stage it was waiting for —
/// which is the fault this buffer exists to fix, re-created by the fix. Six is
/// a seat's own events across the few stages the boundary window can span.
const NEXT_EARLY_PER_SEAT: usize = 8;

/// The drain's invariant, which held by luck and is now pinned: everything both
/// pre-open buffers can hold must fit the queue they are poured into.
const _: () = assert!(
    NEXT_INITS_CAP + NEXT_EARLY_CAP <= crate::table::hand::EARLY_CAP,
    "the pre-open buffers can overflow Hand::early on the drain"
);

/// What an event of a hand this client has not opened yet is worth to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keep {
    /// A `HAND_INIT` of hand k+1. Kept apart because it decides which genesis
    /// this client opens k+1 at (`quiet_if_contested`), and that decision must
    /// rest on `HAND_INIT`s alone.
    Init,
    /// Any other event of hand k+1.
    Early,
    /// Nothing this client can use: another hand entirely, or unreadable.
    No,
}

/// **The boundary is a pause, and the table does not wait in it.**
///
/// A hand ends here, the next one is armed 800 ms later, and the seats that
/// have already opened it deal on. Everything of hand k+1 that arrives in
/// those 800 ms — or in the five-second showdown pause before them — reaches a
/// client whose `hand` is still k, where `hold` answers `AnotherHand`: worth
/// relaying, not worth holding.
///
/// **For a seat that is dealt in, dropping it costs nothing** — the table
/// cannot leave a stage without that seat's copy, so it repeats the stage
/// until the seat has caught up. **For a seat that is only following, it costs
/// the hand.** Nobody waits for a bystander, the senders' re-send covers the
/// last few stages only, and the stage it needed has gone by the time it opens.
/// Measured in `split125944-9`: `far-n0` opened hand #16 about two seconds
/// after the table dealt it, every `DECK_INIT` had already been dropped, and it
/// sat at stage 1 of that hand for the remaining 283 seconds of the run with no
/// terminus available to it (`S1-BW`).
///
/// So both are kept. The cap and the signature check are on the caller; this
/// decides only what kind of use the event is.
fn keep_for_next_hand(
    kind: Option<crate::protocol::messages::EventType>,
    hand_id: u64,
    mine: u64,
) -> Keep {
    use crate::protocol::messages::EventType;
    if hand_id != mine.saturating_add(1) {
        return Keep::No;
    }
    match kind {
        Some(EventType::HandInit) => Keep::Init,
        // **The four that decide a hand's fate are not carried into it.** A
        // terminal, a vote or a certificate replayed out of this queue would
        // end or narrow a hand from a frame the hand was opened without, and a
        // hand opened `Quiet` — which is withholding its own signature on
        // purpose — could emit a terminal from it. Each of the four has its own
        // road already: a `TIMEOUT_CERT` about the hand before this one is kept
        // by the late-certificate arm, and an abort, a settlement or a vote of
        // the hand about to open is re-sent by its own emitters, who are still
        // in that hand. What this buffer is for is the stage traffic nobody
        // re-sends to a seat that was not there, which is what a bystander
        // lost.
        Some(EventType::HandComplete)
        | Some(EventType::HandAbort)
        | Some(EventType::TimeoutVote)
        | Some(EventType::TimeoutCert) => Keep::No,
        // `S1-BM`: the return pair is about the boundary of the hand that
        // ended, never about the hand opening, and it has the late-certificate
        // arm for the copy that arrives during the next hand.
        Some(EventType::ReturnVote) | Some(EventType::ReturnCert) => Keep::No,
        // **§4.10's boundary window is not a stage of the hand and must not be
        // replayed into one.** The reachable case is narrow — a boundary event
        // carries the id of the hand that **ended**, so this arm is reached only
        // at a client a whole hand behind — and the disposition is the
        // specification's own: a boundary event that finds no open window *"is
        // rejected as out of stage; the seat re-emits at the next boundary"*.
        // Buffering it instead would replay it into `HAND_INIT(k+1)`'s hand,
        // where the only answer available is `WrongType`, which is the whole of
        // `S1-BZ` reintroduced through a queue.
        Some(k) if k.is_boundary() => Keep::No,
        Some(_) => Keep::Early,
        None => Keep::No,
    }
}

/// `S1-BM`: the evidence a return vote is cast on, gathered by the node because
/// its two halves arrive by two roads -- `boundary_event` takes the subject's
/// `PLAYER_SIT_IN` and `checkpoint_event` its checkpoint-8 `STATE_HASH` -- and
/// neither reaches a `Hand`. By the hand whose boundary it is, by subject. The
/// first copy of either half is the one kept, so a subject cannot swap its
/// evidence under a vote already cast; two boundaries at most, the retained
/// hand's and the live one's, and older ones go at the hand-over.
#[derive(Default)]
struct SitIns {
    by_hand: std::collections::BTreeMap<
        u64,
        std::collections::BTreeMap<u8, (Option<Vec<u8>>, Option<Vec<u8>>)>,
    >,
}

impl SitIns {
    fn slot(&mut self, hand_id: u64, seat: u8) -> &mut (Option<Vec<u8>>, Option<Vec<u8>>) {
        // Bounded before the entry is made: a third boundary evicts the oldest.
        if !self.by_hand.contains_key(&hand_id) && self.by_hand.len() >= 2 {
            if let Some(oldest) = self.by_hand.keys().next().copied() {
                self.by_hand.remove(&oldest);
            }
        }
        self.by_hand.entry(hand_id).or_default().entry(seat).or_default()
    }

    fn request(&mut self, hand_id: u64, seat: u8, bytes: &[u8]) {
        let e = self.slot(hand_id, seat);
        if e.0.is_none() {
            e.0 = Some(bytes.to_vec());
        }
    }

    fn checkpoint(&mut self, hand_id: u64, seat: u8, bytes: &[u8]) {
        let e = self.slot(hand_id, seat);
        if e.1.is_none() {
            e.1 = Some(bytes.to_vec());
        }
    }

    /// Every subject of this boundary with both halves in hand.
    fn complete(&self, hand_id: u64) -> Vec<crate::table::hand::ReturnEvidence> {
        self.by_hand
            .get(&hand_id)
            .map(|m| {
                m.iter()
                    .filter_map(|(seat, (r, c))| {
                        Some(crate::table::hand::ReturnEvidence {
                            seat: *seat,
                            request: r.clone()?,
                            checkpoint: c.clone()?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every seat that asked to sit in at this boundary, whether or not its
    /// checkpoint has arrived.
    fn requested(&self, hand_id: u64) -> Vec<u8> {
        self.by_hand
            .get(&hand_id)
            .map(|m| m.iter().filter(|(_, (r, _))| r.is_some()).map(|(s, _)| *s).collect())
            .unwrap_or_default()
    }

    /// Boundaries below `keep` are over for good.
    fn release_below(&mut self, keep: u64) {
        self.by_hand.retain(|k, _| *k >= keep);
    }
}

/// `S1-BM`: the seats whose return is in flight at this hand's boundary --
/// they asked to sit in (this client, or a seat whose request the window
/// took), they are outside the roster, and no certificate about them has
/// banked here. Empty means there is nothing to hold the deal for.
fn returning_seats(h: &crate::table::hand::Hand, sit_ins: &SitIns) -> Vec<u8> {
    // A boundary this client holds no checkpoint value of its own for -- an
    // abort's, or the late road's -- is one it cannot vote at, so there is
    // nothing to hold the deal for; the seat asks again at the next.
    if h.checkpoint8().is_none() {
        return Vec::new();
    }
    let mut v = sit_ins.requested(h.hand_id());
    if h.asked_to_sit_in() {
        v.push(h.my_seat());
    }
    v.sort_unstable();
    v.dedup();
    v.retain(|s| !h.returned().contains(s) && !h.required().contains(s));
    v
}

/// `S1-CS`: the seat a peer holds at this client's table, if any.
fn seat_of_peer(f: Option<&Formation>, peer: &PeerId) -> Option<u8> {
    let bytes = peer.to_bytes();
    f?.roster().seats().iter().find(|e| e.peer_id == bytes).map(|e| e.seat)
}

/// When this node loop started, for the fault switches that count from it.
static PROCESS_STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// fault-harness: whether `P2P_POKER_STOP_AT_OPEN_AFTER` names a second this
/// loop has reached. Read once; `false` in every build without the feature.
fn stop_at_open_due() -> bool {
    if !cfg!(feature = "fault-harness") {
        return false;
    }
    static AFTER: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    let after = AFTER.get_or_init(|| {
        std::env::var("P2P_POKER_STOP_AT_OPEN_AFTER")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
    });
    match (after, PROCESS_STARTED.get()) {
        (Some(s), Some(since)) => since.elapsed().as_secs() >= *s,
        _ => false,
    }
}

/// fault-harness: whether `P2P_POKER_LEAVE_TABLE_AT` names a second this loop
/// has reached. Read once; `false` in every build without the feature.
fn leave_table_due() -> bool {
    if !cfg!(feature = "fault-harness") {
        return false;
    }
    static AT: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    let at = AT.get_or_init(|| {
        std::env::var("P2P_POKER_LEAVE_TABLE_AT")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
    });
    // `S1-DV`: once. It used to fire on every tick past the second while the
    // client was seated, and a client that asked for its table again was
    // out again two seconds later.
    static FIRED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    match (at, PROCESS_STARTED.get()) {
        (Some(s), Some(since)) => {
            since.elapsed().as_secs() >= *s && !FIRED.swap(true, std::sync::atomic::Ordering::Relaxed)
        }
        _ => false,
    }
}

/// fault-harness: whether `P2P_POKER_KICK_WITHOUT_WORD_AT` names a second this loop
/// has reached, once; `false` in every build without the feature (`D-045`).
fn kick_without_word_due() -> bool {
    if !cfg!(feature = "fault-harness") {
        return false;
    }
    static AT: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    let at = AT.get_or_init(|| {
        std::env::var("P2P_POKER_KICK_WITHOUT_WORD_AT")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
    });
    static FIRED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    match (at, PROCESS_STARTED.get()) {
        (Some(s), Some(since)) => {
            since.elapsed().as_secs() >= *s && !FIRED.swap(true, std::sync::atomic::Ordering::Relaxed)
        }
        _ => false,
    }
}

/// How many poker peers one housekeeping tick asks about their tables.
const SNAPSHOT_ASKS_PER_TICK: usize = 32;

/// fault-harness: whether `P2P_POKER_STOP_AT_HAND` names the hand whose open
/// this is. Read once; `false` in every build without the feature.
fn stop_at_hand_due(hand_id: u64) -> bool {
    if !cfg!(feature = "fault-harness") {
        return false;
    }
    static AT: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    let at = AT.get_or_init(|| {
        std::env::var("P2P_POKER_STOP_AT_HAND")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
    });
    *at == Some(hand_id)
}

/// fault-harness: whether `P2P_POKER_STOP_ON_TURN_AFTER` names a second this
/// loop has reached. Read once; `false` in every build without the feature.
fn stop_on_turn_due(since: tokio::time::Instant) -> bool {
    if !cfg!(feature = "fault-harness") {
        return false;
    }
    static AFTER: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    let after = AFTER.get_or_init(|| {
        std::env::var("P2P_POKER_STOP_ON_TURN_AFTER")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
    });
    match after {
        Some(s) => since.elapsed().as_secs() >= *s,
        None => false,
    }
}

/// `S1-CR`: keep one frame of the running table for a client that holds no
/// hand of it -- a `HAND_INIT` under its hand id, anything else of a hand
/// beside it -- under two bounds: two hand ids of inits with one copy per
/// seat each, and `RESUME_EARLY_CAP` other frames, oldest out. Returns
/// whether the frame was a hand frame at all.
const RESUME_EARLY_CAP: usize = 1_024;

fn stash_for_resume(
    bytes: &[u8],
    inits: &mut std::collections::BTreeMap<u64, Vec<Vec<u8>>>,
    early: &mut Vec<(u64, Vec<u8>)>,
) -> bool {
    use crate::protocol::messages::EventType;
    let Ok((kind, hand_id, _)) = crate::net::chained::peek(bytes, TABLE_FRAME_PEEK) else {
        return false;
    };
    if hand_id == 0 {
        return false;
    }
    if kind == EventType::HandInit {
        if !inits.contains_key(&hand_id) {
            while inits.len() >= 2 {
                let lowest = *inits.keys().next().expect("non-empty");
                if lowest > hand_id {
                    return true;
                }
                inits.remove(&lowest);
                early.retain(|(h, _)| *h != lowest);
            }
        }
        let slot = inits.entry(hand_id).or_default();
        if slot.len() < usize::from(crate::protocol::constants::MAX_SEATS)
            && !slot.iter().any(|b| b[..] == bytes[..])
        {
            slot.push(bytes.to_vec());
        }
        return true;
    }
    if early.iter().any(|(h, b)| *h == hand_id && b[..] == bytes[..]) {
        return true;
    }
    if early.len() >= RESUME_EARLY_CAP {
        early.remove(0);
    }
    early.push((hand_id, bytes.to_vec()));
    true
}

/// `Quiet` when at least `FOREIGN_GENESIS_FLOOR` roster seats have already
/// signed this hand at one genesis that is not the one this client derived,
/// and no seat has signed it at this client's: the parent is contested
/// before the open, so this client's own signature waits for the table or
/// for a certificate (`S1-BS`).
fn quiet_if_contested(
    o: &crate::table::hand::Opening,
    inits: &[(u8, Vec<u8>)],
) -> crate::table::hand::Voice {
    use crate::protocol::messages::EventType;
    use crate::table::hand::{Voice, FOREIGN_GENESIS_FLOOR, FRAME_CAP};
    let seat_of = |key: &[u8; 32]| o.seats.iter().find(|(_, k, _)| k == key).map(|(s, _, _)| *s);
    let mut foreign: std::collections::BTreeMap<crate::poker::state::Hash, std::collections::BTreeSet<u8>> =
        std::collections::BTreeMap::new();
    let mut at_mine: std::collections::BTreeSet<u8> = std::collections::BTreeSet::new();
    for (_, b) in inits {
        let Ok(opened) =
            crate::net::chained::open_in_hand(b, FRAME_CAP, EventType::HandInit, &o.table_id, o.hand_id)
        else {
            continue;
        };
        if opened.envelope.sequence != 0 {
            continue;
        }
        let Some(seat) = seat_of(&opened.sender) else { continue };
        if seat == o.my_seat || !o.required.contains(&seat) {
            // A seat outside R — certified out of this hand — opens the hand
            // it thinks it is in; its word does not contest this one.
            continue;
        }
        if opened.envelope.previous_event_hash == o.genesis {
            at_mine.insert(seat);
        } else {
            foreign.entry(opened.envelope.previous_event_hash).or_default().insert(seat);
        }
    }
    let most = foreign.values().map(|s| s.len()).max().unwrap_or(0);
    if most >= FOREIGN_GENESIS_FLOOR && at_mine.is_empty() {
        Voice::Quiet
    } else {
        Voice::Speak
    }
}

/// Whether the boundary waits: more roster seats signed this hand at one
/// other genesis than were counted at this client's, this client's own seat
/// counted on its side. Ties deal on (§4.9's late-roster-repair box).
fn wait_at_boundary(named_elsewhere: usize, counted_here_without_me: usize) -> bool {
    named_elsewhere > counted_here_without_me + 1
}

/// The voice of a re-open: muted once this seat has signed the hand, or
/// once it was muted — a muted hand re-opened again must not speak, or one
/// seat would sign one hand twice on the second certificate.
fn reopen_voice(spoke: bool, was: crate::table::hand::Voice) -> crate::table::hand::Voice {
    use crate::table::hand::Voice;
    if spoke || was == Voice::Muted {
        Voice::Muted
    } else {
        Voice::Speak
    }
}

/// Drop this client's own events of one hand from the re-send list — a
/// re-opened hand must not put its first `HAND_INIT` beside its second.
fn purge_hand_from_said(said: &mut Vec<Vec<u8>>, hand_id: u64) {
    said.retain(|b| {
        crate::net::chained::peek(b, TABLE_FRAME_PEEK)
            .map(|(_, h, _)| h != hand_id)
            .unwrap_or(true)
    });
}

/// `S1-BS`: re-open the running hand at a re-derived opening — the same hand
/// id, a corrected genesis. Only from a stage 0 this client never left, and
/// only when the derivation names a different genesis; muted when this
/// client already signed the hand at the old one, so no seat ever signs one
/// hand twice. The held copies and the readmission set carry over. On any
/// refusal the old hand stays exactly as it was. Returns whether it re-opened.
async fn reopen_hand(
    mut opening: crate::table::hand::Opening,
    hand: &mut Option<crate::table::hand::Hand>,
    said: &mut Vec<Vec<u8>>,
    swarm: &mut libp2p::Swarm<super::swarm::PokerBehaviour>,
    events: &Events,
    tox: &super::toxsink::TableSink,
    app_key: &ed25519_dalek::SigningKey,
) -> bool {
    use crate::table::hand::{Hand, Voice};
    let Some(mut old) = hand.take() else { return false };
    if old.hand_id() != opening.hand_id || old.slot().sequence != 0 {
        let _ = events
            .send(NodeEvent::Warning(format!(
                "a certificate about hand #{} re-derives hand #{} at genesis {}, but hand #{} has left stage 0 here (sequence {}); not re-opened",
                opening.hand_id.saturating_sub(1),
                opening.hand_id,
                short_hash(&opening.genesis),
                old.hand_id(),
                old.slot().sequence
            )))
            .await;
        *hand = Some(old);
        return false;
    }
    if opening.genesis == old.genesis() {
        // The certificate re-derived what this client already holds.
        *hand = Some(old);
        return false;
    }
    if let Some((g, seats)) = old.foreign_genesis_named() {
        if g != opening.genesis {
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "a certificate about hand #{} re-derives hand #{} at genesis {}, but seat(s) {:?} hold {}: a certificate the table has is still missing here; not re-opening",
                    opening.hand_id.saturating_sub(1),
                    opening.hand_id,
                    short_hash(&opening.genesis),
                    seats,
                    short_hash(&g)
                )))
                .await;
            *hand = Some(old);
            return false;
        }
    }
    let voice = reopen_voice(old.spoke(), old.voice());
    let early = old.take_early();
    opening.readmitted = old.readmitted().to_vec();
    let was = (old.genesis(), old.required().to_vec());
    let now = super::node::now_unix_ms();
    let deadline = opening.crypto_step_timeout_ms;
    let (genesis, required) = (opening.genesis, opening.required.clone());
    match Hand::open_with(opening, app_key, now, deadline, voice) {
        Ok((mut h, sends)) => {
            purge_hand_from_said(said, h.hand_id());
            let carried = early.len();
            for b in early {
                let _ = h.hold(b);
            }
            let (more, failures) = h.replay_early(app_key, now);
            for e in failures {
                let _ = events
                    .send(NodeEvent::Warning(format!("a held event was refused: {e}")))
                    .await;
            }
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "hand #{} re-opens at genesis {} with seats {:?} (was {} with seats {:?}); {carried} held event(s) carried over; {}",
                    h.hand_id(),
                    short_hash(&genesis),
                    required,
                    short_hash(&was.0),
                    was.1,
                    match voice {
                        Voice::Muted => "muted: this seat signed the hand once already and follows the corrected one silently, rejoining at the next hand",
                        _ => "speaking: this seat signs the hand for the first time here",
                    }
                )))
                .await;
            publish_hand(sends, swarm, said, tox);
            publish_hand(more, swarm, said, tox);
            let _ = events
                .send(NodeEvent::HandWaiting {
                    hand_id: h.hand_id(),
                    seats: h.waiting_for(),
                })
                .await;
            *hand = Some(h);
            true
        }
        Err(e) => {
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "hand #{} could not be re-opened: {e}; the old one stands",
                    old.hand_id()
                )))
                .await;
            // Exactly as it was: the held queue goes back too.
            for b in early {
                let _ = old.hold(b);
            }
            *hand = Some(old);
            false
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn begin_hand_with(
    opening: crate::table::hand::Opening,
    voice: crate::table::hand::Voice,
    next_inits: &mut Vec<(u8, Vec<u8>)>,
    next_early: &mut Vec<(u8, Vec<u8>)>,
    lost: &mut (u32, u32),
    app_key: &ed25519_dalek::SigningKey,
    hand: &mut Option<crate::table::hand::Hand>,
    said: &mut Vec<Vec<u8>>,
    swarm: &mut libp2p::Swarm<super::swarm::PokerBehaviour>,
    events: &Events,
    tox: &super::toxsink::TableSink,
) {
    use crate::table::hand::{Hand, Voice};
    if hand.is_some() {
        next_inits.clear();
        next_early.clear();
        *lost = (0, 0);
        return;
    }
    let now = super::node::now_unix_ms();
    let deadline = opening.crypto_step_timeout_ms;
    match Hand::open_with(opening, app_key, now, deadline, voice) {
        Ok((mut h, sends)) => {
            if voice == Voice::Quiet {
                let _ = events
                    .send(NodeEvent::Warning(format!(
                        "hand #{}: two seats or more signed it at another genesis before this client opened it, and none at this one — this client's own HAND_INIT is withheld until the table is counted here or a certificate re-derives the hand",
                        h.hand_id()
                    )))
                    .await;
            }
            // The next hand's copies that arrived during the last one.
            // `HAND_INIT`s first: they are what stage 0 is waiting for, and the
            // hand's own queue evicts the oldest when it is full.
            let buffered: Vec<Vec<u8>> =
                std::mem::take(next_inits).into_iter().map(|(_, b)| b).collect();
            let carried = std::mem::take(next_early);
            // **Said out loud, because `S1-BW` cannot otherwise be measured.**
            // The buffer's whole effect is on a client that opens a hand later
            // than the table dealt it, and the old code dropped those frames
            // silently — so a run in which the fix worked and a run in which
            // nothing arrived early looked identical in the log. Only when
            // something was actually carried, so a healthy table says nothing.
            if !buffered.is_empty() || !carried.is_empty() || *lost != (0, 0) {
                let _ = events
                    .send(NodeEvent::Warning(format!(
                        "hand #{}: {} init(s) and {} early frame(s) had arrived before it opened here ({} refused, {} evicted)",
                        h.hand_id(),
                        buffered.len(),
                        carried.len(),
                        lost.0,
                        lost.1
                    )))
                    .await;
            }
            *lost = (0, 0);
            for b in buffered {
                let _ = h.hold(b);
            }
            for (_seat, b) in carried {
                let _ = h.hold(b);
            }
            let (more, failures) = h.replay_early(app_key, now);
            for e in failures {
                let _ = events
                    .send(NodeEvent::Warning(format!("a held event was refused: {e}")))
                    .await;
            }
            publish_hand(sends, swarm, said, tox);
            publish_hand(more, swarm, said, tox);
            // What this hand hangs off, said out loud. Two peers that opened
            // hand one from different views of the formation produce different
            // genesis values, and every message each sends is then "a different
            // parent" to the other — which reads, in every other line of the
            // log, as silence. This is the one line that tells them apart.
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "hand #{} opens at genesis {} with seats {:?}, level {} blinds {}/{}",
                    h.hand_id(),
                    short_hash(&h.genesis()),
                    h.required(),
                    h.level(),
                    h.small_blind(),
                    h.big_blind()
                )))
                .await;
            let _ = events
                .send(NodeEvent::HandWaiting {
                    hand_id: h.hand_id(),
                    seats: h.waiting_for(),
                })
                .await;
            // fault-harness: `P2P_POKER_STOP_AT_OPEN_AFTER=<s>` stops this process
            // at its first hand open at or after that second -- a client gone
            // while the next hand waits at stage 0 (`run080025-2`'s shape), which
            // is what the two-seat rule and the give-up convergence are about.
            if stop_at_open_due() {
                println!("fault-harness: stopping at the open of hand #{}, as P2P_POKER_STOP_AT_OPEN_AFTER asked", h.hand_id());
                std::process::exit(0);
            }
            // `P2P_POKER_STOP_AT_HAND=<k>`: the same stop keyed on the hand
            // number, so that several processes stop at the SAME hand --
            // a second counted from each process's own start straddles
            // their clocks when a hand opens near it (`run180731-4`).
            if stop_at_hand_due(h.hand_id()) {
                println!("fault-harness: stopping at the open of hand #{}, as P2P_POKER_STOP_AT_HAND asked", h.hand_id());
                std::process::exit(0);
            }
            *hand = Some(h);
        }
        Err(e) => {
            let _ = events
                .send(NodeEvent::Warning(format!("the hand could not start: {e}")))
                .await;
        }
    }
}

/// Publish what a hand produced, and remember it.
///
/// Remembering is the point: GossipSub has no history, so a peer that joins the
/// topic mesh after a publish never sees it. Everything this client says in a
/// hand is kept until the hand ends, and re-sent — **the stored bytes, never a
/// re-signature**, because re-signing would change `emitted_at_unix_ms` and
/// make a second distinct body at one slot, which is an equivocation proof
/// against an honest peer (`PROTOCOL.md` §5.2).
///
/// Capped, because it is a buffer and every buffer fed from a hand is bounded
/// where it is filled. Sixty-four is more messages than a hand of ten seats
/// produces before the deal, and dropping the oldest is the right direction:
/// the oldest is the one a peer is least likely to still be waiting for.
/// Put this client's own events on whichever transport the table has.
///
/// **One door for both.** D-019 moves a table's game traffic onto a Tox group
/// and leaves the lobby, the join RPC and the ratification on libp2p, so a node
/// may hold both at once — the table's mesh still carrying `PLAYER_LIST` and
/// `TABLE_READY` while the hand rides the group. Which one a hand event takes is
/// decided here and nowhere else: two call sites choosing separately is how the
/// same hand comes to be half on each.
///
/// `said` is appended either way, because the reason for it does not depend on
/// the transport. Neither GossipSub nor a Tox group keeps history, and a peer
/// that joined after a publish never sees it — measured on both, and on the Tox
/// side it killed a hand with neither end reporting anything wrong.
/// Is this peer known to subscribe to every topic this client holds?
///
/// GossipSub delivers to grafted mesh peers, and a peer it does not know is
/// subscribed is a peer it will not graft. The answer is what it *knows*, which
/// is a claim that arrives in a subscription message and can be missed.
fn peer_has_our_topics(
    swarm: &libp2p::Swarm<super::swarm::PokerBehaviour>,
    peer: &libp2p::PeerId,
    topics: &super::swarm::Topics,
    table: Option<&gossipsub::IdentTopic>,
) -> bool {
    let mut want = vec![topics.lobby.hash(), topics.lobby_chat.hash()];
    if let Some(t) = table {
        want.push(t.hash());
    }
    swarm
        .behaviour()
        .gossipsub
        .all_peers()
        .any(|(p, subscribed)| p == peer && want.iter().all(|h| subscribed.contains(&h)))
}

/// Say again what this client subscribes to, for the topics named.
///
/// There is no per-peer "send my subscriptions" call, so this is the primitive
/// that exists: dropping and retaking a topic re-announces it to everyone
/// connected.
///
/// **It is expensive and the list is explicit for that reason.** Peers that
/// receive the `UNSUBSCRIBE` prune this client from their mesh for that topic
/// and re-graft on a later heartbeat — so re-announcing the lobby churns every
/// peer's view of it. Measured, and it was a regression of mine: re-announcing
/// all three topics every five seconds while a table formed pushed the lobby
/// hard enough that adverts came back `RateLimited`, including at the founder
/// against its own. Say again only what is actually missing.
fn announce_topics(
    swarm: &mut libp2p::Swarm<super::swarm::PokerBehaviour>,
    which: &[&gossipsub::IdentTopic],
) {
    for t in which {
        let g = &mut swarm.behaviour_mut().gossipsub;
        let _ = g.unsubscribe(*t);
        let _ = g.subscribe(*t);
    }
}

/// How many times a forming table says its topic again before giving up.
///
/// Three, at five seconds apart. A peer that has not heard by then is not going
/// to, and the churn of saying it again costs every seat already there.
const MAX_TABLE_ANNOUNCES: u32 = 3;

/// How many stages back a re-send reaches.
///
/// A peer one stage behind is the ordinary case — the mesh does not order two
/// messages and a stage boundary crossing in flight is weather. Three is that
/// case with room, and a peer further back than three stages has lost enough
/// that repetition is the wrong tool.
const RESEND_STAGES: u64 = 3;

/// The cap for peeking at a message this client itself produced.
///
/// Its own bytes, so the bound is a formality — but a decoder with no bound is
/// a decoder with no bound, and `HAND_ABORT` is the largest thing it builds.
const TABLE_FRAME_PEEK: usize = crate::protocol::constants::HAND_ABORT_MAX;

/// **A link that is down on purpose**, for the one failure that is ordinary and
/// that nothing here had ever been made to face: a seated player on an
/// unreliable connection loses the line for a few seconds and gets it back.
///
/// `P2P_POKER_LINK_DOWN_AT=<s>` and `P2P_POKER_LINK_DOWN_FOR=<s>` drop every
/// table message this client would send and every one it would receive, for that
/// window, measured from the first call — which is the node loop starting.
///
/// **What it simulates, and what it does not -- the second half is `S1-CM`.**
/// The application's messages stop in both directions while the process, the
/// `Hand`, the `Opening`, the chain position, the keys and the transport all
/// survive, which is what a brief outage does to a client and is emphatically
/// *not* what killing the process does. libp2p's own pings keep answering,
/// deliberately: that separates **did the hand recover** from **did the
/// transport reconnect**, and the second question already has an instrument
/// (`-DropAt`).
///
/// **But the receive half discards what the transport has already delivered**,
/// and a real outage on the carrier the hand rides does not. Under D-019 a hand
/// event is a lossless group packet, and toxcore's ring holds one unacked for
/// `GC_CONFIRMED_PEER_TIMEOUT` = 58 s: the sender re-sends at 2, 4, 8, 16 and
/// 32 s (`gcc_resend_packets`), the receiver asks for the head of any gap with
/// `GR_ACK_REQ`, and only a head entry 58 s old drops the peer. So a seat whose
/// line is cut for less than that receives every frame it missed, late and in
/// order, the moment the line returns -- whereas this knob's receive side
/// `continue`s past a frame the ring has already acked, so the frame is gone
/// for good. The send half is closer: `publish_hand` parks what could not leave
/// in `said`, and the re-send arm puts it out once `nothing_leaves()` is false.
/// What this knob models is therefore a peer whose transport forgets what it
/// received while the line was down -- GossipSub's semantics, which has no
/// history -- and not the group's. The instrument for the group's semantics on
/// the receive side is patch 0016's `-Deaf`, which drops the packet before the
/// ring sees it: **did the hand recover** from a brief outage is measured with
/// that, under 58 s, and had not been measured at all until `S1-CM`.
///
/// No argument, so no caller has to thread a clock through six signatures to ask
/// it. Two locks, and the outer one is a compile-time absence: without
/// `--features fault-harness` this is the constant `false` and the environment
/// is never read.
#[cfg(feature = "fault-harness")]
fn link_is_down() -> bool {
    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    static WINDOW: OnceLock<Option<(u64, u64)>> = OnceLock::new();
    let start = START.get_or_init(std::time::Instant::now);
    let window = WINDOW.get_or_init(|| {
        let at = std::env::var("P2P_POKER_LINK_DOWN_AT")
            .ok()?
            .trim()
            .parse::<u64>()
            .ok()?;
        let dur = std::env::var("P2P_POKER_LINK_DOWN_FOR")
            .ok()?
            .trim()
            .parse::<u64>()
            .ok()?;
        Some((at, dur))
    });
    match window {
        Some((at, dur)) => {
            let s = start.elapsed().as_secs();
            s >= *at && s < at.saturating_add(*dur)
        }
        None => false,
    }
}

/// The constant `false`, in every build that did not ask for the harness.
#[cfg(not(feature = "fault-harness"))]
fn link_is_down() -> bool {
    false
}

/// **A seat that hears everything and is heard by nobody**, which is the one
/// fault shape this tree could not produce and the one `S1-BW` needs.
///
/// `P2P_POKER_MUTE_AT=<s>` and `P2P_POKER_MUTE_FOR=<s>` drop every **hand**
/// message this client would send, for that window, measured from the first
/// call. Nothing it receives is touched.
///
/// **Why the asymmetry is the whole instrument.** A seat is certified out when
/// it misses a decision deadline, and every other knob in this tree makes a
/// seat miss deadlines by cutting what reaches it — so by the time the table
/// has voted, the seat is hands behind and is no longer a bystander but a
/// stranger. `S1-BW`'s fix is about the seat that is certified out **and still
/// level with the table**: it keeps following, opens the next hand outside
/// `R`, and completes it from the buffer. `split125944-9` reached that state by
/// coincidence — its own certificates were parked — and this reaches it on
/// purpose, because silence in one direction costs a seat its seat without
/// costing it a single stage of what the table did next.
///
/// **Only the hand.** Formation traffic is untouched: a seat that could not
/// ratify would never be seated, and the state this exists to reach is on the
/// far side of that. It is also why this is not `link_is_down` with a flag —
/// that one is *an outage*, both directions and everything on them, and the
/// two answer different questions.
#[cfg(feature = "fault-harness")]
fn mouth_is_shut() -> bool {
    // **The clock is anchored here whichever form is in use.** In the on-turn
    // form this branch never reads `mute_start`, so without this line the
    // `OnceLock` behind it was first initialised inside `arm_the_mute` — at the
    // first action of the run — and `MUTE_AT` was then counted from **that**
    // rather than from the loop. Measured in `split184533-9`, which armed
    // hundreds of seconds late and certified nobody. It is the same trap
    // `S1-CC` records, in the code written to remove it.
    let _ = mute_start();
    match mute_window() {
        Some((at, dur)) => {
            if on_turn() {
                // Armed by `arm_the_mute`, at the first action this client
                // would publish at or after `at`. Before that it is silent
                // about nothing.
                match *mute_armed().lock().expect("the mute clock") {
                    Some(armed) => armed.elapsed().as_secs() < dur,
                    None => false,
                }
            } else {
                let s = mute_start().elapsed().as_secs();
                s >= at && s < at.saturating_add(dur)
            }
        }
        None => false,
    }
}

/// When this loop started, for the wall-clock form of the mute.
#[cfg(feature = "fault-harness")]
fn mute_start() -> &'static std::time::Instant {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now)
}

/// `(at, for)`, read once.
#[cfg(feature = "fault-harness")]
fn mute_window() -> Option<(u64, u64)> {
    static WINDOW: std::sync::OnceLock<Option<(u64, u64)>> = std::sync::OnceLock::new();
    *WINDOW.get_or_init(|| {
        let at = std::env::var("P2P_POKER_MUTE_AT").ok()?.trim().parse::<u64>().ok()?;
        let dur = std::env::var("P2P_POKER_MUTE_FOR").ok()?.trim().parse::<u64>().ok()?;
        Some((at, dur))
    })
}

/// Whether the window is measured from this seat's own turn.
#[cfg(feature = "fault-harness")]
fn on_turn() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("P2P_POKER_MUTE_ON_TURN").is_ok())
}

/// The instant the on-turn window was armed, if it has been.
#[cfg(feature = "fault-harness")]
fn mute_armed() -> &'static std::sync::Mutex<Option<std::time::Instant>> {
    static ARMED: std::sync::OnceLock<std::sync::Mutex<Option<std::time::Instant>>> =
        std::sync::OnceLock::new();
    ARMED.get_or_init(|| std::sync::Mutex::new(None))
}

/// **Arm the mute on the first action this client would publish**, at or after
/// `P2P_POKER_MUTE_AT`.
///
/// A mute measured in seconds costs the table nothing unless the seat happens
/// to be due to act inside it, and whether it is depends on where the button
/// was when the run started — which is why four runs of one recipe produced
/// four different outcomes (`S1-CD`). Armed here, the seat is silent across a
/// turn it certainly owed, which is what the table has to miss for a
/// certificate to form.
#[cfg(feature = "fault-harness")]
fn arm_the_mute(kind: crate::protocol::messages::EventType) {
    use crate::protocol::messages::EventType as E;
    if !on_turn() {
        return;
    }
    if !matches!(
        kind,
        E::ActionCheck | E::ActionCall | E::ActionBet | E::ActionRaise | E::ActionFold
    ) {
        return;
    }
    let Some((at, _)) = mute_window() else { return };
    if mute_start().elapsed().as_secs() < at {
        return;
    }
    let mut armed = mute_armed().lock().expect("the mute clock");
    if armed.is_none() {
        *armed = Some(std::time::Instant::now());
    }
}

/// Nothing to arm in a build without the harness.
#[cfg(not(feature = "fault-harness"))]
fn arm_the_mute(_kind: crate::protocol::messages::EventType) {}

/// The constant `false`, in every build that did not ask for the harness.
#[cfg(not(feature = "fault-harness"))]
fn mouth_is_shut() -> bool {
    false
}

/// **Every reason a hand's bytes do not leave this client**, in one place.
///
/// The two knobs are different faults — an outage is both directions, a mute is
/// one — and a send site cannot tell them apart or want to. A receive site
/// **can**: it asks `link_is_down` alone, because a muted seat is still
/// listening, and that difference is the instrument.
fn nothing_leaves() -> bool {
    link_is_down() || mouth_is_shut()
}

/// Send a hand's bytes, **over Tox and nowhere else**.
///
/// # The instruction, and the number behind it
///
/// D-019 puts a formed table's game traffic on a Tox group. The owner has since
/// stated it as an absolute: *never libp2p for playing a hand* — because a
/// libp2p circuit relay does not have the capacity for one, while the Tox
/// network and its fallback TCP relays do.
///
/// That is not a preference, it is the measured reservation. The public relays
/// this client obtains grant **131 072 bytes per 120 seconds**, and the client
/// already says so out loud in its own log: *“NOT enough to carry a hand”*.
///
/// # Why the parameter is gone rather than the branch
///
/// This used to take the per-table GossipSub topic and fall back to it whenever
/// there was no Tox carrier. Removing the argument is what makes the rule hold:
/// a function that cannot reach the swarm cannot be made to publish to it by a
/// later edit that looks harmless.
///
/// Formation traffic is untouched and still goes both ways — D-019 keeps the
/// roster and the ratification on libp2p, and `S1-P` added a Tox carrier beside
/// it rather than instead of it. This is about the hand.
///
/// # What happens when there is no Tox carrier
///
/// The bytes go into `said` and nothing leaves, exactly as when the link is
/// down, and the caller is told. A hand that cannot be sent is a hand that
/// stalls visibly; a hand pushed onto a channel that cannot carry it is one
/// that fails at the deal and blames the opponent.
fn publish_hand(
    sends: Vec<crate::table::hand::Send>,
    swarm: &mut libp2p::Swarm<super::swarm::PokerBehaviour>,
    said: &mut Vec<Vec<u8>>,
    tox: &super::toxsink::TableSink,
) {
    let _ = swarm;
    for crate::table::hand::Send::Broadcast(out) in sends {
        // **Arm the on-turn mute before asking whether anything may leave**,
        // so the window starts at the action this client owed rather than at a
        // second on the clock. A no-op in every other mode and in every build
        // without the harness.
        if let Ok((kind, _, _)) = crate::net::chained::peek(&out, TABLE_FRAME_PEEK) {
            arm_the_mute(kind);
        }
        let down = nothing_leaves();
        if down {
            // Nothing leaves. It is still put in `said`, because `said` is the
            // re-send buffer and a message that could not go out is exactly what
            // it exists to send later.
            if said.len() >= 64 {
                said.remove(0);
            }
            said.push(out);
            continue;
        }
        if tox.is_on_tox() {
            // A refusal here is a full channel, not a lost table: the driver is
            // behind, and the five-second re-send is what covers it.
            tox.try_broadcast(&out);
        }
        // No `else`. There is no second channel for a hand, by instruction and
        // by capacity. Without a Tox carrier the bytes wait in `said`, which is
        // what `said` is for.
        if said.len() >= 64 {
            said.remove(0);
        }
        said.push(out);
    }
}

/// Take one frame of the next hand into the pre-open buffer, evicting the
/// **highest** sequence held rather than refusing the arrival.
///
/// **The old rule was a guard, and a guard that fails drops the arrival.** The
/// admission arm read `next_early.len() < NEXT_EARLY_CAP && ...` and fell
/// through to `_ => {}`, so once the buffer was full every later frame of the
/// next hand was thrown away — and the buffer is structurally too small to
/// avoid that: at eight dealt-in seats the run before the first bet is 8
/// `DECK_INIT` + 8 `SHUFFLE_STEP` + 8 `SHUFFLE_PROOF` + 8 `DECK_COMMIT` + 8
/// `DEAL_PRIVATE` = **40 frames** against a cap of 32.
///
/// **Highest, because a client that is behind walks upward.** It resumes at the
/// stage it is stuck on and needs the lowest sequences first; the highest are
/// the ones their emitters are still covering under the ordinary five-second
/// re-send. Evicting by arrival — which is what `Hand::early` did — destroys
/// the contiguous run immediately above the cursor and guarantees the replay
/// stalls again at the next hole.
///
/// Measured, `split135059-9`: a bystander opened hand #2 with *"8 init(s) and
/// **32** early frame(s)"* — `NEXT_EARLY_CAP` exactly — replayed through stages
/// 0 to 17 and stalled at stage 18 one `DECK_COMMIT` short, about 101 bytes,
/// from a seat that had sealed the deck **18.4 s before that client opened the
/// hand**. The pre-open buffer was that frame's only possible route in.
///
/// The per-seat allowance is enforced first and against that seat's own
/// highest, so one loud seat cannot evict another's low sequences; the whole
/// buffer's cap is enforced second, against the highest anywhere.
fn keep_early(held: &mut Vec<(u8, Vec<u8>)>, seat: u8, bytes: &[u8], lost: &mut (u32, u32)) {
    let Ok((kind, _, arriving)) = crate::net::chained::peek(bytes, TABLE_FRAME_PEEK) else {
        return;
    };
    // **Refused at the door for being larger than its own type allows, and
    // this is what makes the budget below affordable.** Nothing between the
    // wire and here bounds a frame by its type — `open_in_hand` is given
    // `FRAME_CAP` and `check_envelope` never reads the payload's length — so
    // every slot was a sixteen-kilobyte slot and a `DECK_COMMIT` whose cap is
    // 160 bytes could hold one for a whole boundary.
    if bytes.len() > crate::table::hand::frame_ceiling(kind) {
        lost.0 = lost.0.saturating_add(1);
        return;
    }
    if held.iter().any(|(_, b)| b[..] == bytes[..]) {
        return;
    }
    // The held entry with the highest sequence, among `only` if given.
    let highest = |held: &[(u8, Vec<u8>)], only: Option<u8>| -> Option<(usize, u64)> {
        held.iter()
            .enumerate()
            .filter(|(_, (s, _))| only.is_none_or(|o| *s == o))
            .filter_map(|(i, (_, b))| {
                crate::net::chained::peek(b, TABLE_FRAME_PEEK)
                    .ok()
                    .map(|(_, _, q)| (i, q))
            })
            .max_by_key(|(_, q)| *q)
    };
    let mine = |held: &[(u8, Vec<u8>)]| -> (usize, usize) {
        held.iter()
            .filter(|(s, _)| *s == seat)
            .fold((0, 0), |(n, b), (_, v)| (n + 1, b + v.len()))
    };
    // **Loops, not `if`s, because the bound that binds is now the byte one**
    // and giving up one 544-byte `DECK_COMMIT` does not make room for an
    // 8 704-byte `SHUFFLE_PROOF`. Each pass removes an entry, so both
    // terminate.
    loop {
        let (n, b) = mine(held);
        if n < NEXT_EARLY_PER_SEAT
            && b + bytes.len() <= crate::table::hand::NEXT_EARLY_BYTES_PER_SEAT
        {
            break;
        }
        match highest(held, Some(seat)) {
            Some((i, q)) if q > arriving => {
                held.remove(i);
                lost.1 = lost.1.saturating_add(1);
            }
            _ => return,
        }
    }
    loop {
        let all: usize = held.iter().map(|(_, b)| b.len()).sum();
        if held.len() < NEXT_EARLY_CAP
            && all + bytes.len() <= crate::table::hand::NEXT_EARLY_BYTES
        {
            break;
        }
        match highest(held, None) {
            Some((i, q)) if q > arriving => {
                held.remove(i);
                lost.1 = lost.1.saturating_add(1);
            }
            _ => return,
        }
    }
    held.push((seat, bytes.to_vec()));
}

/// Whether a dial failure is one that **mends by itself**, so that trying
/// again in ten seconds is worth a slot.
///
/// **The question is transience, not blame, and this predicate was written
/// around the wrong one.** It used to ask whether the failure was *the far
/// end's rather than this client's*, and that principle is false for almost
/// everything it selects. Reconstructing the multi-line `dial failed:` records
/// across all 642 logs and keeping only those naming a peer offered as a lobby
/// provider — the population `charge.is_some()` admits — gives 1 552
/// `Transport` records, of which **1 479, 95%,** carry *"Response from
/// behaviour was canceled: oneshot canceled"*. That is not the far end: it is
/// `libp2p-relay` dialling the relay under the default
/// `DisconnectedAndNotDialing` condition and dropping the pending circuit on
/// any dial failure, so a circuit through a relay this client is **already
/// dialling** dies synchronously. Ours, both ends of it — and it mends in
/// seconds, when that relay is connected and the same address needs no dial at
/// all. Which is the whole of `FAST_REDIAL_AFTER`'s argument, and it survives
/// the correction untouched.
///
/// The list is closed and short on purpose, and it is **two** members rather
/// than the three it was written with. `Transport` covers that relay collision
/// and a stale address a later answer replaces; `WrongPeerId` is an address
/// that belongs to somebody else, which is a stale record and mends the same
/// way.
///
/// Everything excluded is excluded because it does **not** mend on its own.
/// `Denied` is this client's own connection limiter refusing a connection that
/// was **already established**: the limiter is saturated and stays saturated,
/// and counting it was the worst defect of this row's first design.
/// `LocalPeerId` is dialling ourselves and will be true for ever.
/// `NoAddresses` and `DialPeerConditionFalse` are refused by `Swarm::dial` **by
/// value** and never produce an event at all.
///
/// **One imprecision left in, deliberately.** Across the whole corpus 62% of
/// `Transport` records are *"Unsupported resolved address"* — this client's own
/// transport unable to dial a webtransport or webrtc-direct address, which
/// never mends in ten seconds. They do not reach the lane, but only because
/// they name relay-key providers that `charge.is_some()` excludes, not because
/// this predicate excludes them. Telling them apart needs matching on a
/// formatted error string, which is a worse dependency than the imprecision:
/// the cost of admitting one is a single extra dial out of a budget of 128.
///
/// **`Aborted` was in this list and is not, and it was wrong twice over.** It
/// is produced in exactly one place — `libp2p-swarm-0.47.1`
/// `connection/pool/task.rs:100-106`, when the `oneshot` held in
/// `PendingConnection.abort_notifier` is cancelled — and the only taker of that
/// notifier is `Pool::abort()`, reached only from `Pool::disconnect`, reached
/// only from `Swarm::disconnect_peer_id` and `ToSwarm::CloseConnection`.
/// Neither appears anywhere in this tree, and the one crate in the registry
/// that emits `CloseConnection` is `libp2p-allow-block-list`, which this client
/// does not use. So the arm was unreachable — and had it ever been reached it
/// would have meant **this client cancelled its own dial**, which is `Denied`'s
/// class and the opposite of the sentence that admitted it.
fn the_failure_mends_itself(error: &libp2p::swarm::DialError) -> bool {
    matches!(
        error,
        libp2p::swarm::DialError::Transport(_) | libp2p::swarm::DialError::WrongPeerId { .. }
    )
}

/// Whether this failure buys the provider a quicker second look.
///
/// Separate and pure because the budget is the whole safety argument of
/// `S1-CB`'s third design, and a bound that cannot be tested is a bound
/// nobody checks.
fn earns_a_fast_look(peers_own_fault: bool, budget: u32, opening: bool) -> bool {
    peers_own_fault && budget > 0 && opening
}

/// Eight hex characters of a hash, which is what a person can compare.
fn short_hash(h: &[u8; 32]) -> String {
    h[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// A checkpoint-8 `STATE_HASH` of a hand this receiver has **completed**, and
/// what it is worth (§4.9, §4.0 step 10b).
///
/// Returns whether the event was one — a `false` means the caller should pass
/// the bytes on to the hand as usual.
///
/// **It is not applied, chains nothing and completes no stage.** Step 10b:
/// such an event *"is not dropped for arriving late"*, and the one thing it
/// does is add its sender to the readmission set `A` **when its value agrees
/// with this receiver's own retained value**. That is `S1-O`'s door: a seat
/// outside `P(k)` is not dealt into `k+1` (§4.4) and cannot sign its way back
/// in, and this is how it signs.
///
/// **Agreement is the whole of the check, and it is enough.** A forged value is
/// refused by the comparison; a replayed one is idempotent, because entering a
/// set twice is entering it once. The signature, the table and the hand are all
/// verified by `open_in_hand`, which relaxes only the positional check — and
/// the position is exactly what makes this event stale.
///
/// A **disagreeing** value is a divergence and §6.3 is what answers it: the
/// freeze latches, a dispute goes out carrying this peer's own copy as evidence,
/// and a reconciliation round opens as soon as the checkpoint's own stage has
/// closed. This comment said *“that is not built”* until §6.3 was; a comment
/// that outlives what it describes is worse than none.
/// Notice that the table has gone on without this client, and latch it.
///
/// **The failure this exists for, measured.** A seat whose link was down for 75
/// seconds is certified out by the others, comes back with a perfectly healthy
/// transport — `group 3/3`, every seat answering pings — and then **deals a game
/// of its own**: `hand #5 opens at genesis c41cdc63` while the table is on hand
/// #10, three of its six hands on a genesis nobody else has, every one of them
/// timing out. The table is fine and plays on. The client is not, and nothing
/// told it so.
///
/// The owner's rule is that a peer this far gone is out — *“ten peer má smůlu a
/// bude muset být ostatními vyhozen ze hry”* — and the table already does its
/// half by certifying the seat away. This is the other half: **the client takes
/// itself out** instead of dealing a private tournament for the rest of the run.
///
/// **A strict majority of the seats still in the game, because two is not the
/// table.** Each `Holding::AnotherHand` carries a signature this hand has
/// already verified against this table, so a roster seat naming a hand this
/// client has not reached is that seat's own word that it is elsewhere. Two
/// seats used to be the floor, and two seats on a private branch of their own
/// -- aborting their way through hands nobody deals -- latched six healthy
/// seats out of a table of nine (`split111706-9`: `n0` at hand 6, `n6` at 3,
/// the six at hand 1 for the rest of the run). The table is wherever a strict
/// majority of the seats still in the game are, counted against this client's
/// own `required` and `returned` sets and nobody else: a seat certified out
/// that goes on dealing alone is not evidence of anything.
///
/// **A return, not a terminus (`D-038`).** Nothing here can catch up on its
/// own -- the openings of the hands in between were derived from settlements
/// this client never saw -- but the table's copies of its running hand are
/// evidence enough to adopt it (`S1-CR`), and the latch now hands over to
/// `rejoin_from_copies!`.
fn note_a_hand_ahead(
    h: &crate::table::hand::Hand,
    hand_id: u64,
    seat: Option<u8>,
    ahead: &mut std::collections::HashMap<u8, u64>,
    adrift: &mut Option<(u64, u64)>,
) {
    // A hand *behind* this client is an ordinary late delivery, and a seat with
    // no place in the roster is not evidence of anything.
    let Some(seat) = seat else { return };
    let mine = h.hand_id();
    if hand_id <= mine {
        return;
    }
    let furthest = ahead.entry(seat).or_insert(hand_id);
    *furthest = (*furthest).max(hand_id);
    if adrift.is_some() {
        return;
    }
    let me = h.my_seat();
    let others: std::collections::BTreeSet<u8> = h
        .required()
        .iter()
        .chain(h.returned().iter())
        .copied()
        .filter(|s| *s != me)
        .collect();
    let others: Vec<u8> = others.into_iter().collect();
    if let Some(out) = adrift_now(mine, ahead, &others) {
        *adrift = Some(out);
    }
}

/// The decision alone, so a test can reach it.
///
/// `note_a_hand_ahead` needs a whole `Hand` and a live table to be called; this
/// is the part that decides, and a test that had to rebuild the caller would
/// end up restating the rule instead of checking it.
fn adrift_now(mine: u64, ahead: &std::collections::HashMap<u8, u64>, others: &[u8]) -> Option<(u64, u64)> {
    // **A majority of the others, and two hands each.** One hand of margin is
    // what a table looks like while somebody is still inside `Ended::pause`;
    // a seat outside `others` is not in the game as this client knows it, and
    // its word about where the table is counts for nothing.
    let saying: Vec<u64> = others
        .iter()
        .filter_map(|s| ahead.get(s).copied())
        .filter(|k| *k >= mine.saturating_add(ADRIFT_MARGIN))
        .collect();
    let furthest = saying.iter().copied().max()?;
    (saying.len() * 2 > others.len()).then_some((furthest, mine))
}

/// How many hands ahead the table must be before this client calls itself out.
///
/// # This was one, and one is what an ordinary table looks like
///
/// **Measured over 134 table runs left on disk: 37 seats latched themselves
/// out, and 36 of them were exactly one hand behind.** The single remaining
/// case was five hands behind (`run212350-4`, hand 11 against 6) and is the one
/// this mechanism was written for.
///
/// The reason a margin of one is not evidence is in the code beside it:
/// `Ended::pause()` holds a finished hand at showdown for five seconds so a
/// player can see the cards. A peer that has already paid that pause and opened
/// hand `k+1` is a hand ahead of one still inside it, and on a table of four or
/// more there are always at least two such peers — which is the whole of the
/// `saying.len() >= 2` test. So the latch fired on ordinary progression skew,
/// permanently, and the seat it removed was healthy.
///
/// Two hands is not a tuned number. A seat one hand behind catches up when its
/// pause ends; a seat that is still behind after the table has played a further
/// hand is not going to. Nothing is lost by waiting, because the latch is about
/// refusing to *emit* into a hand nobody else has — and a seat that is one hand
/// behind and will catch up emits into a hand everybody else already finished,
/// which is late rather than divergent. §8.3's timeout certificate removes a
/// seat that really has stopped, and it does not need this to fire first.
/// And two hands is also the margin at which the table has demonstrably dealt
/// on without this seat, which is what `D-038`'s return rests on.
const ADRIFT_MARGIN: u64 = 2;

/// T47: hand `k+1`'s `HAND_INIT` stage has completed, so hand `k`'s checkpoint
/// moves down to the boundary slot.
///
/// **Moved, not dropped** — §4.9 keeps admitting `STATE_ACK` copies into it,
/// which is `N6`: *"the checkpoint-8 `STATE_ACK` stage is not closed by that
/// window"*. What the window bounds is the admission of a `STATE_HASH`, because
/// that is the only checkpoint-8 event that can grow `P(k)`, and hand `k`'s
/// record is released by `TERMINAL(k+1)` opening the next one.
///
/// `Hand::dealt` is T47's condition in the player's words: stage 0 complete,
/// every required seat heard and agreed. It stays true for the rest of the hand,
/// so the caller's `crossed_for` is what makes this happen once.
fn cross_boundary_at_t47(
    h: &crate::table::hand::Hand,
    boundaries: &mut crate::table::boundary::Boundaries,
    crossed_for: &mut Option<u64>,
) {
    if !h.dealt() || *crossed_for == Some(h.hand_id()) {
        return;
    }
    *crossed_for = Some(h.hand_id());
    boundaries.cross_boundary();
}

/// Put a checkpoint event on the table **and hear it here**, and the same for
/// whatever it makes due.
///
/// **A stage counts its own seat like every other, and the wire never gives a
/// message back to the peer that sent it** — neither GossipSub nor the Tox group
/// does. So a copy that is only published is a copy this peer's own stage never
/// hears, and the stage waits for its own seat for ever. Measured twice, once
/// per stage: every node's `STATE_HASH` stage *"waiting for [0]"* on the node
/// that is seat 0, and then, after the first half was fixed, every node's
/// `STATE_ACK` stage waiting for exactly the same seat.
///
/// Hearing goes through `checkpoint_event` rather than through a second
/// admission written here, because two admission paths for one event are two
/// sets of rules that can come to differ — and it is what makes this compose:
/// the `STATE_HASH` that completes the stage answers with this peer's
/// `STATE_ACK`, which needs publishing and hearing in its turn.
///
/// The loop is bounded rather than trusted to be two long.
#[allow(clippy::too_many_arguments)]
async fn publish_and_hear(
    first: Vec<u8>,
    h: &crate::table::hand::Hand,
    boundaries: &mut crate::table::boundary::Boundaries,
    readmitted: &mut Vec<u8>,
    sit_ins: &mut SitIns,
    app_key: &ed25519_dalek::SigningKey,
    checkpoint_said: &mut bool,
    frozen: &mut Option<(u64, u64)>,
    roster: &[u8],
    no_round_said: &mut bool,
    early: &mut std::collections::HashMap<u64, Vec<Vec<u8>>>,
    profile: &std::path::Path,
    events: &Events,
    // No topic, for the reason given on `publish_hand`: the way a hand stays
    // off libp2p is that nothing on the hand path is given the means to put it
    // there.
    swarm: &mut libp2p::Swarm<super::swarm::PokerBehaviour>,
    said: &mut Vec<Vec<u8>>,
    tox: &super::toxsink::TableSink,
) {
    let mut out = Some(first);
    for _ in 0..2 {
        let Some(bytes) = out.take() else { break };
        publish_hand(
            vec![crate::table::hand::Send::Broadcast(bytes.clone())],
            swarm,
            said,
            tox,
        );
        if let Some(next) = checkpoint_event(
            &bytes,
            h,
            boundaries,
            readmitted,
            sit_ins,
            app_key,
            checkpoint_said,
            frozen,
            roster,
            no_round_said,
            early,
            profile,
            events,
        )
        .await
        {
            if !next.is_empty() {
                out = Some(next);
            }
        }
    }
}

/// Which application key signed this table message, if it is one and it verifies.
///
/// **Used to bridge two key spaces, and it does it with a signature rather than
/// a guess.** The Tox driver knows a sender by its *group* key; the roster knows
/// a seat by its application key; `tox.h` maps neither to the other. Opening the
/// event answers it, and answers it with the only evidence that counts — which
/// is why the pairing is learned here, where the signature is checked, and not
/// in the driver, where nothing could check it. `S1-I`.
fn signer_of(bytes: &[u8], h: &crate::table::hand::Hand) -> Option<[u8; 32]> {
    let (kind, hand_id, _) = crate::net::chained::peek(bytes, TABLE_FRAME_PEEK).ok()?;
    crate::net::chained::open_in_hand(bytes, TABLE_FRAME_PEEK, kind, &h.table_id(), hand_id)
        .ok()
        .map(|o| o.sender)
}

/// §6.3 step 2, on the receiving side: a dispute is a **carrier**, and what
/// carries weight is what it contains.
///
/// **This is the half that makes the procedure reachable.** §6.3: *"a recipient
/// whose own copies all agreed did not observe the divergence and would
/// otherwise take no part in the procedure; the dispute is what puts a second,
/// independently verifying value at that checkpoint in front of it."* Without
/// it, a table where only two seats saw the mismatch can never satisfy §4.9's
/// floor of two on the reconciliation round, and the freeze is released by
/// nothing. This client emitted disputes and admitted none.
///
/// **The evidence is compared, never applied.** It occupies no slot, enters no
/// `stage_hash` and completes no stage — a `DISPUTE` is unchained and *"nothing
/// carried inside one ever becomes a chained event by being carried"*. So it
/// does not go through `checkpoint_event`, which would enter it into the stage.
///
/// **And it is an observation, not an instruction.** §6.3 calls the embedded
/// `STATE_HASH` *"an observation for step 1's purposes"*, and step 1's trigger
/// is observing **two distinct** values at one checkpoint. A dispute carrying a
/// value equal to this peer's own is therefore one value, not two, and changes
/// nothing — which is also what stops a dispute being a way to freeze a table
/// somebody else is playing.
///
/// Returns whether the bytes were a dispute at all.
async fn dispute_event(
    bytes: &[u8],
    h: &crate::table::hand::Hand,
    boundaries: &mut crate::table::boundary::Boundaries,
    frozen: &mut Option<(u64, u64)>,
    seen: &mut usize,
    events: &Events,
) -> bool {
    use crate::table::dispute;

    let table_id = h.table_id();
    let Ok((body, sender)) = dispute::receive(bytes, &table_id) else {
        return false;
    };
    // **Counted before anything else is decided.** A dispute that arrives and
    // adds nothing — because this peer had already seen the divergence
    // directly, which is the ordinary case at a small table — is silent, and
    // was indistinguishable from a dispute that never arrived. Measured: four
    // disputes went out and not one line said any had been read.
    *seen += 1;
    // `kind = 3` is D-014's cheat evidence and is not answered here; `kind = 2`
    // never reaches this point, `receive` having refused it.
    if body.kind != dispute::KIND_STATE_DIVERGENCE {
        return true;
    }
    let hand_id = body.hand_id;
    let Some(mine) = boundaries.own_value(hand_id) else {
        // A hand whose boundary this peer no longer holds. Verified and
        // relayed, and nothing to compare it against.
        return true;
    };
    let Some(evidence) = body.evidence.first() else {
        return true;
    };
    // Opened against the hand it names, exactly as a checkpoint-8 copy is, and
    // by the same function — the signature and the table do all the work, and
    // the position is what makes it stale rather than wrong.
    let Ok(opened) = crate::net::chained::open_in_hand(
        evidence,
        TABLE_FRAME_PEEK,
        crate::protocol::messages::EventType::StateHash,
        &table_id,
        hand_id,
    ) else {
        return true;
    };
    let Ok(carried) =
        crate::net::chained::payload::<crate::table::checkwire::StateHash>(&opened, 512)
    else {
        return true;
    };
    if carried.state_hash == mine {
        return true;
    }
    // Two distinct values at one checkpoint, one of them this peer's own.
    let Some(seat) = h.seat_of_key(&opened.sender) else {
        return true;
    };
    let fresh = boundaries.contradiction_from_a_dispute(hand_id, seat);
    if frozen.is_none() {
        *frozen = Some((hand_id, opened.envelope.sequence));
    }
    if fresh {
        let _ = events
            .send(NodeEvent::Warning(format!(
                "a dispute from {} carries seat {seat}'s end-of-hand state for hand {hand_id}, and it differs from this client's own: FROZEN at section 6.3 step 1, and seat {seat} joins the contradiction set",
                short_hash(&sender)
            )))
            .await;
    }
    true
}

/// §6.3 step 2, the moment this peer freezes: **declare, with the evidence**.
///
/// The dispute carries this peer's own `STATE_HASH` event whole, so a recipient
/// whose own copies all agreed can verify a second value at that checkpoint
/// rather than believe anybody — and §6.3 makes that load-bearing, because such
/// a recipient otherwise takes no part in the procedure and the reconciliation
/// stage's floor of two seats is then unreachable at a table where only two saw
/// the mismatch.
///
/// Step 3 is **not** here, and the first version of this had it here and was
/// wrong. See `try_to_reconcile`.
async fn dispute_answer(
    hand_id: u64,
    at_sequence: u64,
    boundaries: &crate::table::boundary::Boundaries,
    app_key: &ed25519_dalek::SigningKey,
    h: &crate::table::hand::Hand,
    events: &Events,
) -> Vec<u8> {
    let Some(own) = boundaries.own_event(hand_id).map(|b| b.to_vec()) else {
        // A peer that never published its own copy has no evidence to carry,
        // and a dispute without evidence is the accusation §6.3 deliberately
        // does not send.
        let _ = events
            .send(NodeEvent::Warning(format!(
                "no dispute is sent for hand {hand_id}: this client never published its own checkpoint copy, so it has no evidence to carry"
            )))
            .await;
        return Vec::new();
    };
    match crate::table::dispute::publish_state_divergence(
        h.table_id(),
        hand_id,
        at_sequence,
        own,
        app_key,
        super::node::now_unix_ms(),
    ) {
        Some(bytes) => {
            // **Said, because a dispute that is not sent and a dispute that is
            // sent and not heard look the same from here.** §6.3 step 2 is what
            // makes the reconciliation floor reachable at a table where only
            // two seats saw the mismatch, so whether it left is worth one line.
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "section 6.3 step 2: a dispute for hand {hand_id} goes out, carrying this client's own checkpoint copy as evidence"
                )))
                .await;
            bytes
        }
        None => {
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "the dispute for hand {hand_id} would not seal, so section 6.3 step 2 did not happen here"
                )))
                .await;
            Vec::new()
        }
    }
}

/// §6.3 step 3: open the reconciliation round when it **can** be opened, and put
/// this peer's re-derived value in it.
///
/// **Tried on every checkpoint event while frozen, and not once at the freeze.**
/// The round chains from the checkpoint's `stage_hash`, which exists only when
/// the stage has closed — and the copy that contradicts is usually not the last
/// one to arrive, so at the moment of the freeze there is nothing to chain from.
/// Measured: opened once at the freeze, the round never opened on any node of a
/// run, and the single `None` returned then reported the wrong cause as well.
///
/// Returns this peer's own round `STATE_HASH` to publish, or empty.
async fn try_to_reconcile(
    hand_id: u64,
    boundaries: &mut crate::table::boundary::Boundaries,
    roster: &[u8],
    app_key: &ed25519_dalek::SigningKey,
    said: &mut bool,
    events: &Events,
) -> Vec<u8> {
    use crate::table::boundary::NoRound;

    if boundaries.has_round(hand_id, 1) {
        return Vec::new();
    }
    match boundaries.open_round(hand_id, 1, roster) {
        Ok(()) => {
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "reconciliation round 1 for hand {hand_id} is open over R(c) union W; it is waiting for {:?} and needs one seat other than this to agree",
                    boundaries.round_waiting_for(hand_id, 1)
                )))
                .await;
            boundaries
                .get(hand_id)
                .and_then(|b| b.round_hash_event(1, app_key, super::node::now_unix_ms()))
                .unwrap_or_default()
        }
        // Not permanent: the stage closes when the last member of `P(k)` is
        // heard, and this is tried again on the next checkpoint event.
        Err(NoRound::StageNotClosed) => Vec::new(),
        Err(e) => {
            if !*said {
                *said = true;
                let _ = events
                    .send(NodeEvent::Warning(format!(
                        "no reconciliation round can open for hand {hand_id}: {}",
                        match e {
                            NoRound::BelowFloor =>
                                "R(c) union W is under section 4.9's floor of two, so this freeze is released by nothing",
                            NoRound::NoSuchBoundary =>
                                "this peer no longer holds that boundary",
                            _ => "the round is outside the band section 4.9 reserves",
                        }
                    )))
                    .await;
            }
            Vec::new()
        }
    }
}

/// §6.4's divergence report, written to the profile directory when a
/// reconciliation round completes carrying two values.
///
/// §6.4 says the client *"writes a reproducible divergence report to the profile
/// directory containing the full transcript, every peer's `STATE_HASH`, and its
/// own derived `PublicTableState`"*, and calls that report *"the bug report, and
/// ... the diagnostic material of §6.3 case (c)"*.
///
/// **Nothing wrote one.** §6.4 had already withdrawn a stronger claim — that the
/// evidence is deterministically adjudicable offline by a third party — on the
/// grounds that no reference engine exists, and kept a weaker one: *"the evidence
/// is preserved and is sufficient for a human ... to diagnose the divergence."*
/// That weaker claim was untrue as well, because nothing preserved anything: a
/// faulted table emitted one warning line and dropped its boundary on the next
/// prune.
///
/// **This writes what the client actually holds, and names what it does not.**
/// Two of §6.4's three items are not retained anywhere — the transcript is
/// dropped with the hand's slot store, and `PublicTableState` is hashed rather
/// than kept — so the file says so in a section of its own instead of quietly
/// omitting them. A report that looked complete and was not would be worse for
/// the human §6.4 is addressing than one that says where it stops.
///
/// Written atomically — temp file, `sync_all`, rename — because a report about a
/// disagreement is worth nothing if it can be half a file.
fn write_divergence_report(
    profile: &std::path::Path,
    hand_id: u64,
    round: u16,
    boundaries: &crate::table::boundary::Boundaries,
    h: &crate::table::hand::Hand,
) -> std::io::Result<std::path::PathBuf> {
    use std::io::Write;

    let hex = |b: &[u8]| -> String { b.iter().map(|x| format!("{x:02x}")).collect() };
    let table = hex(&h.table_id());
    let mut s = String::new();
    s.push_str("p2p-poker divergence report\n");
    s.push_str("PROTOCOL.md section 6.4, cause 4: the participants could not agree.\n");
    s.push_str("This table is closed. No further hand is dealt on it.\n\n");
    s.push_str(&format!("table        {table}\n"));
    s.push_str(&format!("hand         {hand_id}\n"));
    s.push_str("checkpoint   8 (the hand boundary, section 4.9)\n");
    s.push_str(&format!(
        "round        {round} (the reconciliation round of section 6.3 step 3)\n"
    ));
    s.push_str(&format!("this seat    {}\n", h.my_seat()));
    s.push_str(&format!(
        "written at   {} (unix ms, by this client's own clock)\n\n",
        super::node::now_unix_ms()
    ));

    s.push_str("-- what this peer derived --\n");
    match boundaries.own_value(hand_id) {
        Some(v) => s.push_str(&format!("own STATE_HASH   {}\n", hex(&v))),
        None => s.push_str("own STATE_HASH   (not retained: the boundary is already gone)\n"),
    }
    match boundaries.dissent(hand_id) {
        Some(v) => s.push_str(&format!("first differing  {}\n", hex(&v))),
        None => s.push_str("first differing  (none recorded)\n"),
    }
    s.push_str(&format!(
        "P(k), required   {:?}\nW, contradicted  {:?}\n\n",
        boundaries
            .get(hand_id)
            .map(|b| b.participants().to_vec())
            .unwrap_or_default(),
        boundaries.contradicted(hand_id)
    ));

    s.push_str("-- who was heard at the checkpoint, and the event each was heard saying --\n");
    let heard = boundaries.heard_at_checkpoint(hand_id);
    if heard.is_empty() {
        s.push_str("(nothing retained)\n");
    }
    for (seat, event) in heard {
        s.push_str(&format!("seat {seat}   event {}\n", hex(&event)));
    }
    s.push('\n');

    s.push_str(&format!(
        "-- the distinct values in reconciliation round {round}; two of these is the fault --\n"
    ));
    let values = boundaries.round_values(hand_id, round);
    if values.is_empty() {
        s.push_str("(nothing retained)\n");
    }
    for v in values {
        s.push_str(&format!("{}\n", hex(&v)));
    }
    s.push('\n');

    s.push_str("-- what section 6.4 asks for that this report does not carry --\n");
    s.push_str(
        "the full transcript of the hand:  not retained. The hand's slot store is\n\
         dropped when the hand ends, and nothing keeps a copy for this purpose.\n\
         every peer's STATE_HASH:          only the event hash of each is held, above;\n\
         the bodies are not retained past the stage that accepted them.\n\
         this peer's own PublicTableState: hashed at the boundary, never kept.\n\n\
         Those three are what a diagnosis would want most, and this client keeps\n\
         none of them. They are named here rather than omitted, so that a reader\n\
         knows the report is bounded and where it stops.\n",
    );

    let path = profile.join(format!(
        "divergence-{}-hand{hand_id}-round{round}.txt",
        &table[..16]
    ));
    let tmp = path.with_extension("txt.tmp");
    std::fs::create_dir_all(profile)?;
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(s.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Where a boundary event sat, against where §4.10 says it must.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowPosition {
    Right,
    /// The `sequence` is not `BOUNDARY_SEQUENCE_BASE + sender_seat`.
    NotTheSeatsSlot,
    /// The parent is not `TERMINAL(k)`.
    NotTheTerminal,
}

/// The two envelope rules §4.10 puts **in place of** §4.0 step 10a's chain
/// cursor, which `open_in_hand` relaxes for this type.
///
/// Pure, and lifted out of [`boundary_event`] so that it can be falsified
/// without a swarm: the whole of what makes the window safe is that these two
/// comparisons are exact.
///
/// **The seat is the input and the sequence is the thing compared.** Reading the
/// seat out of the sequence instead would let a sender nominate its own slot,
/// and a seat with two slots is a seat that can sign two boundary events for one
/// hand — the defect class §5.2 closed for `DISPUTE` and §4.8 for `TIMEOUT_VOTE`.
fn window_position(
    seat: crate::poker::state::SeatIdx,
    sequence: u64,
    parent: &crate::poker::state::Hash,
    terminal: &crate::poker::state::Hash,
) -> WindowPosition {
    if crate::table::seatwire::window_sequence(seat) != Some(sequence) {
        return WindowPosition::NotTheSeatsSlot;
    }
    if parent != terminal {
        return WindowPosition::NotTheTerminal;
    }
    WindowPosition::Right
}

/// One event of `PROTOCOL.md` §4.10's **hand boundary window**, taken here
/// rather than by the hand — `S1-BZ`.
///
/// Returns whether this frame was one: `false` means it is not a boundary type
/// at all and the caller carries on down its dispatch, `true` means it has been
/// disposed of, accepted or refused. That is `dispute_event`'s shape and not
/// `checkpoint_event`'s, because nothing here ever produces a reply — the window
/// is a fan of single-writer stages with no `stage_hash`, so there is no stage
/// for this client to complete and nothing to publish in answer.
///
/// **Why not `Hand::on_event`.** The window belongs to chain `k`, the hand that
/// has just ended, and closes at the acceptance of a complete `HAND_INIT(k+1)`.
/// For most of its life the live `Hand` is `k+1` and would refuse every one of
/// these for naming a hand it has left — which is exactly what it did: three
/// declared chained types answered with `Failed::Wire(WrongType)`, and the node
/// then handing them to the formation handler, which has no use for them either.
///
/// **The envelope rules are all here, and they are stricter than the cursor they
/// replace.** `open_in_hand` relaxes §4.0 step 10a's two positional comparisons
/// — this is the second of the document's exactly two exemptions from it — and
/// §4.10 replaces them with an exact identity and an exact parent:
/// `sequence == BOUNDARY_SEQUENCE_BASE + sender_seat`, and
/// `previous_event_hash == TERMINAL(k)` for every event of the window whichever
/// seat emits. The seat in that identity is read from the **sender's key** and
/// the sequence is then compared against it, never the other way round: reading
/// the seat out of the sequence would let a sender nominate the slot it
/// occupies, and one seat with two slots is the defect class §5.2 closed for
/// `DISPUTE` and §4.8 for `TIMEOUT_VOTE`.
async fn boundary_event(
    bytes: &[u8],
    h: &crate::table::hand::Hand,
    boundaries: &mut crate::table::boundary::Boundaries,
    readmitted: &mut Vec<u8>,
    sit_ins: &mut SitIns,
    early: &mut std::collections::HashMap<u64, Vec<Vec<u8>>>,
    events: &Events,
) -> bool {
    use crate::protocol::messages::EventType;
    use crate::table::boundary::WindowTook;
    use crate::table::seatwire;

    let Ok((kind, hand_id, sequence)) = crate::net::chained::peek(bytes, TABLE_FRAME_PEEK) else {
        return false;
    };
    if !kind.is_boundary() {
        // **And the band is refused to everybody else here.** §4.10: a receiver
        // *"rejects any other chained type at a `sequence >= BOUNDARY_SEQUENCE_BASE`"*.
        // §4.9's checkpoint band sits above this one and is not the window's to
        // refuse, so only the window's own ten slots are claimed.
        //
        // Refused by consuming it, which is what `true` means to the caller: a
        // `DECK_COMMIT` at seat 3's boundary slot must not go on to the hand,
        // where the slot comparison would reject it for a different reason and
        // report it as an ordinary stale frame.
        return seatwire::in_the_window(sequence);
    }
    // A window this peer does not hold: it has not opened, or T47 closed it.
    // §4.10 disposes of both the same way and the seat re-emits at the next
    // boundary. Consumed rather than passed on: the hand would answer a
    // boundary type with `WrongType`, which is the whole of `S1-BZ`.
    //
    // **The WINDOW store, not the checkpoint's.** They were one store and the
    // checkpoint's is opened on the settled path alone, so asking it here made
    // the window settled-path only too.
    if boundaries.window(hand_id).is_none() {
        // **Not yet, rather than not at all.** A hand id at or above the live
        // hand's names a boundary this client has not reached: the sender
        // ended hand k first, and its request is exactly what phase 1 will
        // want a moment later. Held under `early_checkpoints`' two bounds --
        // two hands, one copy per seat -- and admitted when the window opens.
        // A hand id below the live hand's is a window T47 closed: consumed,
        // and the seat re-emits at the next boundary (section 4.10).
        if hand_id >= h.hand_id() && (early.contains_key(&hand_id) || early.len() < 2) {
            let slot = early.entry(hand_id).or_default();
            if slot.len() < usize::from(crate::protocol::constants::MAX_SEATS)
                && !slot.iter().any(|b| b[..] == bytes[..])
            {
                slot.push(bytes.to_vec());
            }
        }
        return true;
    }
    let Ok(opened) = crate::net::chained::open_in_hand(
        bytes,
        TABLE_FRAME_PEEK,
        kind,
        &h.table_id(),
        hand_id,
    ) else {
        return true;
    };
    // The payload, under its own cap and never `FRAME_CAP`. Decoded before
    // anything is recorded so that a malformed body is refused rather than
    // filed: §4.10 gives two of the three a `reason` and `PLAYER_SIT_IN` an
    // empty one, and a type whose body does not decode is not that type.
    let reason = match kind {
        EventType::PlayerSitOut => {
            match crate::net::chained::payload::<seatwire::SitOut>(
                &opened,
                seatwire::BOUNDARY_EVENT_CAP,
            ) {
                Ok(b) => Some(b.reason),
                Err(_) => return true,
            }
        }
        EventType::PlayerLeave => {
            match crate::net::chained::payload::<seatwire::Leave>(
                &opened,
                seatwire::BOUNDARY_EVENT_CAP,
            ) {
                Ok(b) => Some(b.reason),
                Err(_) => return true,
            }
        }
        // No fields beyond the envelope, so there is nothing to decode and
        // nothing to check: the payload is whatever the emitter put there and
        // no rule in §4.10 reads it.
        _ => None,
    };
    let Some(seat) = h.seat_of_key(&opened.sender) else {
        return true;
    };
    let Some(terminal) = boundaries.window(hand_id).map(|w| w.terminal()) else {
        return true;
    };
    match window_position(seat, sequence, &opened.envelope.previous_event_hash, &terminal) {
        WindowPosition::Right => {}
        WindowPosition::NotTheSeatsSlot => {
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "seat {seat} sent a {} for hand {hand_id} at sequence {sequence}, and its \
                     only slot in the boundary window is {:?}: refused",
                    kind.name(),
                    seatwire::window_sequence(seat)
                )))
                .await;
            return true;
        }
        WindowPosition::NotTheTerminal => {
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "seat {seat} sent a {} for hand {hand_id} chained from something other than \
                     TERMINAL({hand_id}): refused",
                    kind.name()
                )))
                .await;
            return true;
        }
    }
    match boundaries.on_boundary_event(hand_id, seat, kind) {
        WindowTook::Took(EventType::PlayerSitIn) => {
            // §4.9's readmission set `A`, and the second thing that writes it —
            // the first is a checkpoint-8 `STATE_HASH` from outside `P(k)` whose
            // value agrees. Held by the node loop rather than by any hand,
            // because it is read and cleared at exactly one place: the next hand
            // init.
            //
            // **`A` is `accepted` and never `required`** (`P2`, §4.9). The seat
            // may sign stage 0 of the next hand; it does not thereby re-enter
            // the roster, because `next_hand` filters `self.open.required` in
            // both branches (`D-024`). The door back into the roster is
            // `S1-BM`'s return certificate, and this request is the first half
            // of the evidence it carries: kept here, beside the subject's
            // agreeing checkpoint from `checkpoint_event`, for the votes.
            if !readmitted.contains(&seat) {
                readmitted.push(seat);
                readmitted.sort_unstable();
            }
            sit_ins.request(hand_id, seat, bytes);
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "seat {seat} asked to sit in at the boundary of hand {hand_id}; it may sign \
                     the next hand's opening"
                )))
                .await;
        }
        WindowTook::Took(k) => {
            // Recorded and read by nobody yet, which is stated in the log rather
            // than implied by silence. A `PLAYER_LEAVE` removes no seat from
            // `roster_hash` and counts into no `P` (§3.1, §3.2); its chips leave
            // through `HAND_INIT`'s `n(11) ledger_delta`, inside a collective
            // body, and that is `S1-BM`'s.
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "seat {seat} sent a {} at the boundary of hand {hand_id}{}: recorded, and no \
                     seat leaves the roster of a table that has one",
                    k.name(),
                    match reason {
                        Some(r) => format!(" (reason {r})"),
                        None => String::new(),
                    }
                )))
                .await;
        }
        WindowTook::AlreadySpoke => {
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "seat {seat} has already spoken in the boundary window of hand {hand_id}; a \
                     second boundary event of any type is a stage violation and is refused"
                )))
                .await;
        }
        // Not worth a line each: a sit-in from a seat already in `P(k)` is what
        // a client that does not track its own participation sends, and a
        // sender that is not a seat of this table is refused everywhere else
        // too.
        WindowTook::DecidesNothing | WindowTook::NotASeat | WindowTook::Closed => {}
    }
    true
}

#[allow(clippy::too_many_arguments)]
/// Whether §4.9's parent rule applies to this frame, and whether it holds.
///
/// The mirror of [`window_position`] for §4.9, and deliberately the same shape:
/// the two sections state the same rule in the same words, and for a while only
/// §4.10 enforced it (`S1-CK`).
///
/// **Only round 0's hash half chains from `TERMINAL(k)`.** `state_ack_event`
/// chains from the checkpoint's own `stage_hash` and says so in its doc;
/// `round_hash_event` does the same for a reconciliation round. A rule scoped by
/// sequence alone would refuse every acknowledgement and every round, which is
/// what makes `NotInScope` a distinct answer rather than a `true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckpointPosition {
    /// Round 0's hash half, chained from `TERMINAL(k)` as §4.9 requires.
    Right,
    /// Round 0's hash half, chained from something else.
    NotTheTerminal,
    /// An acknowledgement or a reconciliation round: §4.9's parent rule is not
    /// about these, and they chain from the checkpoint's own `stage_hash`.
    NotInScope,
}

fn checkpoint_position(
    round: u16,
    half: crate::table::checkwire::Kind,
    parent: &crate::poker::state::Hash,
    terminal: &crate::poker::state::Hash,
) -> CheckpointPosition {
    if round != 0 || half != crate::table::checkwire::Kind::Hash {
        return CheckpointPosition::NotInScope;
    }
    if parent != terminal {
        return CheckpointPosition::NotTheTerminal;
    }
    CheckpointPosition::Right
}

async fn checkpoint_event(
    bytes: &[u8],
    h: &crate::table::hand::Hand,
    boundaries: &mut crate::table::boundary::Boundaries,
    readmitted: &mut Vec<u8>,
    sit_ins: &mut SitIns,
    app_key: &ed25519_dalek::SigningKey,
    checkpoint_said: &mut bool,
    frozen: &mut Option<(u64, u64)>,
    roster: &[u8],
    no_round_said: &mut bool,
    early: &mut std::collections::HashMap<u64, Vec<Vec<u8>>>,
    profile: &std::path::Path,
    events: &Events,
) -> Option<Vec<u8>> {
    use crate::table::boundary::{RoundTook, Took};
    use crate::table::checkwire;
    use crate::protocol::messages::EventType;

    let Ok((kind, hand_id, sequence)) = crate::net::chained::peek(bytes, TABLE_FRAME_PEEK) else {
        return None;
    };
    if kind != EventType::StateHash && kind != EventType::StateAck {
        return None;
    }
    // Only for a boundary this receiver still holds. §4.9's window, and the
    // store is what decides how long it is open — a hand that is live belongs
    // to the hand, which owns its own stages.
    //
    // **A copy for a boundary that is not open *yet* is held, not dropped.**
    // The two cases are opposite and this branch used to treat them alike: a
    // hand id **below** the window is a copy for a boundary the store has
    // released, and there is nothing to do with it; a hand id **at or above**
    // the live hand is a peer that finished before this one, and its copy is
    // exactly what the stage about to open will need. See `early_checkpoints`.
    if !boundaries.holds(hand_id) {
        // One copy per seat per hand, and two hands' worth in all: a hold
        // queue that grows is a way to be attacked. **The bound is checked
        // before the entry is made**, or `or_default` would insert an empty
        // slot for a third hand and the guard would then refuse to fill it.
        if hand_id >= h.hand_id() && (early.contains_key(&hand_id) || early.len() < 2) {
            let slot = early.entry(hand_id).or_default();
            if slot.len() < usize::from(crate::protocol::constants::MAX_SEATS)
                && !slot.iter().any(|b| b == bytes)
            {
                slot.push(bytes.to_vec());
            }
        }
        return None;
    }
    let (round, half) = checkwire::round_of(sequence)?;
    let expected = match half {
        checkwire::Kind::Hash => EventType::StateHash,
        checkwire::Kind::Ack => EventType::StateAck,
    };
    if kind != expected {
        // A `STATE_ACK` at the hash stage's sequence, or the reverse. §5.2.1's
        // slot key contains `event_type`, so this is not a slot either of them
        // owns.
        return Some(Vec::new());
    }

    let Ok(opened) = crate::net::chained::open_in_hand(
        bytes,
        TABLE_FRAME_PEEK,
        kind,
        &h.table_id(),
        hand_id,
    ) else {
        return Some(Vec::new());
    };
    let Some(seat) = h.seat_of_key(&opened.sender) else {
        return Some(Vec::new());
    };

    // **§4.9's parent, enforced** (`S1-CK`). *"**Parent.**
    // `previous_event_hash = TERMINAL(k)`, for every emitter and whatever the
    // order of arrival"* — the one thing every emitter must agree on before the
    // comparison starts. `open_in_hand` passes `position: None`, which is
    // §4.0 step 10a's exemption and relaxes **both** positional comparisons, so
    // without this the sender could chain its checkpoint from a stage it
    // invented. §4.10's window has had the same check since it was built
    // (`window_position`); this is the half that did not.
    //
    // **The scope is exact and the rest of the band must not move.** Only
    // round 0's HASH half chains from the terminal. `state_ack_event` chains
    // from the checkpoint's own `stage_hash` and says so in its doc, and
    // `round_hash_event` does the same for a reconciliation round — a gate
    // scoped by sequence alone would refuse every acknowledgement and every
    // round, which is 699 measured *the boundary checkpoint of hand N agreed*
    // lines regressed.
    //
    // **It cannot hide the divergence it sits next to.** `TERMINAL(k)` is a
    // `stage_hash` over the settlement's **event hashes**, while the value
    // checkpoint 8 compares is `state_hash` **inside** those events. Two peers
    // that settled the same hand heard the same frames and hold the same
    // terminal even when their `state_hash` differs, so this checks the
    // position and leaves the value to `on_state_hash` — which is the division
    // §4.9 itself draws. And a peer that aborted never arrives here at all:
    // `boundaries.open` is gated on `checkpoint8()`, `None` after an abort, so
    // it holds no boundary and the `holds` test above already returned.
    //
    // **Refused loudly.** A checkpoint frame that vanishes silently is the
    // shape of defect this register keeps filing, and a peer chaining from a
    // stage nobody else holds is precisely what §4.9 wants known.
    let Some(terminal) = boundaries.get(hand_id).map(|b| b.terminal()) else {
        // The `holds` test above passed, so this is unreachable; taking it as a
        // refusal rather than a panic keeps a receiver from dying on a race.
        return Some(Vec::new());
    };
    if checkpoint_position(round, half, &opened.envelope.previous_event_hash, &terminal)
        == CheckpointPosition::NotTheTerminal
    {
        let _ = events
            .send(NodeEvent::Warning(format!(
                "seat {seat}'s boundary checkpoint for hand {hand_id} is chained from a stage \
                 this client does not hold, so section 4.9's parent rule refuses it: it is not \
                 TERMINAL(k) here"
            )))
            .await;
        return Some(Vec::new());
    }

    // §6.3 step 3's re-derivation, in the round's own stage. Its `STATE_ACK`
    // half is not built: §6.3 reads the outcome off the **hash** stage — one
    // value resolves, two fault, *"and the engine needs no other signal"*.
    if round != 0 {
        if half != checkwire::Kind::Hash {
            return Some(Vec::new());
        }
        let Ok(body) = crate::net::chained::payload::<checkwire::StateHash>(&opened, 512) else {
            return Some(Vec::new());
        };
        match boundaries.on_round_hash(hand_id, round, seat, opened.event_hash, body.state_hash) {
            RoundTook::Resolved => {
                // The only thing that releases the freeze, and it took at least
                // one seat other than this one signing the same re-derived
                // value — which is the whole of what the floor `|R| >= 2` buys.
                *frozen = None;
                let _ = events
                    .send(NodeEvent::Warning(format!(
                        "the divergence at hand {hand_id} reconciled in round {round}: one value across R(c) union W, and the freeze is released"
                    )))
                    .await;
            }
            RoundTook::Unresolved => {
                // §6.4. The table is faulted, and the evidence is written down
                // — the only claim §6.4 still makes about diagnosis is that it
                // is *preserved*, and until now nothing preserved anything.
                let wrote = write_divergence_report(profile, hand_id, round, boundaries, h);
                let _ = events
                    .send(NodeEvent::Warning(format!(
                        "the divergence at hand {hand_id} did NOT reconcile in round {round}: two values, section 6.3 case (c), and this table deals no further hand. {}",
                        match &wrote {
                            Ok(p) => format!("The divergence report is at {}", p.display()),
                            Err(e) => format!("The divergence report could NOT be written: {e}"),
                        }
                    )))
                    .await;
            }
            _ => {}
        }
        return Some(Vec::new());
    }

    let mut hash_value: Option<crate::poker::state::Hash> = None;
    let took = match half {
        checkwire::Kind::Hash => {
            let Ok(body) = crate::net::chained::payload::<checkwire::StateHash>(&opened, 512)
            else {
                return Some(Vec::new());
            };
            hash_value = Some(body.state_hash);
            boundaries.on_state_hash(hand_id, seat, opened.event_hash, body.state_hash)
        }
        checkwire::Kind::Ack => {
            let Ok(body) = crate::net::chained::payload::<checkwire::StateAck>(&opened, 512) else {
                return Some(Vec::new());
            };
            boundaries.on_state_ack(hand_id, seat, opened.event_hash, body.checkpoint_hash)
        }
    };
    // `S1-BM`: the second half of a return's evidence -- the subject's own
    // signed checkpoint, whose value agrees here -- from ANY seat outside
    // `R(k)`, whether or not the checkpoint stage counted it. The `Bystander`
    // arm below sees only a seat outside `P(k)`, and a returning seat is
    // inside `P(k)`: it is in section 4.9's `A`, so it signed stage 0 of the
    // hand, which is what `A` is for. `split175316-9`: all eight voters took
    // its request and held the deal, and none could vote, because its copy
    // was `Counted` and the evidence never had its second half.
    if let Some(v) = hash_value {
        let outside = boundaries
            .window(hand_id)
            .is_some_and(|w| !w.required().contains(&seat));
        if outside
            && boundaries.own_value(hand_id) == Some(v)
            && !matches!(took, Took::Diverged { .. } | Took::Equivocation { .. } | Took::Uninvited)
        {
            sit_ins.checkpoint(hand_id, seat, bytes);
        }
    }

    match took {
        // §4.9's readmission set, written by exactly this: a copy from a seat
        // **outside** `P(k)` whose value agrees. A seat already in `P(k)` is
        // already in the next hand's required set and adding it would change
        // nothing and say something false in the log.
        Took::Bystander => {
            if !readmitted.contains(&seat) {
                readmitted.push(seat);
                readmitted.sort_unstable();
            }
            // `S1-BM`: the second half of a return's evidence -- the
            // subject's own signed checkpoint, whose value agreed here.
            sit_ins.checkpoint(hand_id, seat, bytes);
        }
        // The stage closed and every value in it agreed. One copy per seat, and
        // `Boundaries` returns this once.
        Took::AckIsDue => {
            if let Some(out) = boundaries
                .get(hand_id)
                .and_then(|b| b.state_ack_event(app_key, super::node::now_unix_ms()))
            {
                return Some(out);
            }
        }
        Took::Diverged {
            solitary_contradicted,
        } => {
            let already = frozen.is_some();
            // **The latch.** Set on the first observation and not moved by a
            // second: §6.3 freezes on *"two distinct `state_hash` values at one
            // checkpoint"*, and a third value is more of the same evidence
            // rather than a new event to freeze at.
            if !already {
                *frozen = Some((hand_id, sequence));
            }
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "seat {seat} holds a different end-of-hand state for hand {hand_id}: FROZEN at section 6.3 step 1 — no hand event is accepted or emitted from here{}",
                    if solitary_contradicted {
                        ", and this peer is alone in P(k), which is N1's second half"
                    } else {
                        ""
                    }
                )))
                .await;
            if !already {
                return Some(
                    dispute_answer(hand_id, sequence, boundaries, app_key, h, events).await,
                );
            }
        }
        Took::Equivocation { .. } => {
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "seat {seat} put two different events in one checkpoint stage of hand {hand_id}"
                )))
                .await;
        }
        // **Said once per node.** A checkpoint that agrees is the ordinary case
        // and a line per hand would be noise; but a stage that never ran is
        // also silent, and the two must not look the same in a log. So the
        // first one that closes says so, and after that only a divergence
        // speaks.
        Took::Agreed => {
            if !*checkpoint_said {
                *checkpoint_said = true;
                let _ = events
                    .send(NodeEvent::Warning(format!(
                        "the boundary checkpoint of hand {hand_id} agreed across P(k) = {:?}; checkpoints run from here and only a disagreement will say so",
                        boundaries.get(hand_id).map(|b| b.participants().to_vec()).unwrap_or_default()
                    )))
                    .await;
            }
        }
        Took::Counted | Took::Again | Took::Uninvited => {}
    }
    // **§6.3 step 3, tried here rather than at the freeze.** The round chains
    // from the checkpoint's `stage_hash` and the contradicting copy is usually
    // not the last to arrive, so at the freeze there is nothing to chain from.
    if matches!(*frozen, Some((k, _)) if k == hand_id) {
        let out = try_to_reconcile(hand_id, boundaries, roster, app_key, no_round_said, events).await;
        if !out.is_empty() {
            return Some(out);
        }
    }
    Some(Vec::new())
}

/// The first hand's opening, once the roster has ratified.
///
/// A named function rather than the expression inline, because both roads into
/// the first hand call it and an expression duplicated at two sites is an
/// expression that can come to differ at two sites.
fn opening_for_hand_one(f: &Formation) -> Option<crate::table::hand::Opening> {
    crate::table::hand::Opening::from_formation(f, 1)
}

/// Say once, per node, why hand one cannot be opened from this formation.
///
/// Once, because the three roads in are retried on every table message and a
/// reason repeated every thirty seconds is a reason nobody reads.
async fn say_why_no_hand_one(f: &Formation, said: &mut bool, events: &Events) {
    if *said {
        return;
    }
    if let Some(why) = crate::table::hand::Opening::why_not_from_formation(f) {
        *said = true;
        let _ = events
            .send(NodeEvent::Warning(format!("no hand can start here: {why}")))
            .await;
    }
}

/// How long a seated peer may answer nothing before the founder gives its seat
/// back, and **only before the first hand**.
///
/// **Derived from the ping cadence, not chosen.** `ping::Config`'s defaults in
/// `libp2p-ping-0.47.0` are a fifteen-second interval and a twenty-second
/// timeout (`handler.rs:65-66`), so a peer that is there answers roughly four
/// times a minute. Ninety seconds is six intervals: a peer has to miss every
/// one of them, which no ordinary loss does — the internet drops packets and
/// closes connections, and a rule that took a seat away for one lost datagram
/// would be worse than the problem.
///
/// It is also short enough that a table is not held for long by somebody who
/// has gone, which is the whole point: until this existed a tournament that
/// lost one player before it started could not be completed by anybody.
const SEAT_SILENCE_MS: u64 = 90_000;

/// `S1-DV`: how long a seat may sit on the roster outside the table's group
/// before its line being down reads as gone. The invitation dance takes ten
/// to fifteen seconds on one machine (D-042's measurements); forty leaves
/// room for a far seat and a slow relay.
const GROUP_JOIN_GRACE: std::time::Duration = std::time::Duration::from_secs(40);

/// How long hand 1 waits after the group **stops filling**.
///
/// **A fixed deadline is wrong for one of the two cases, which is why this one
/// is not fixed.** On one LAN, group entry runs on toxcore's own cadence —
/// `LAN_DISCOVERY_INTERVAL` is ten seconds
/// (`vendor/c-toxcore/toxcore/LAN_discovery.h:23`) — and lands at 10 to 40 s.
/// Across two machines on different subnets it is a different quantity
/// altogether: measured on a ten-seat table split five and five, seats entered
/// at **70, 73, 107, 143, 148, 155 and 246 seconds**, with a 91-second gap
/// between the last two. A sixty-second deadline covers none of that, and the
/// seat that arrived at 246 s was dealt into a hand it could not hear and
/// finished none of the four it opened.
///
/// So the wait is on **progress**, not on the clock: while seats are still
/// arriving the table keeps waiting, and it gives up only once nothing new has
/// joined for this long. Two minutes is longer than the widest gap measured.
const GROUP_STALL_MS: u64 = 120_000;

/// The wait's absolute ceiling, whatever the group is doing.
///
/// A table cannot be held for ever by a group that gains one member every
/// ninety seconds and never completes. Past this it deals — **which is exactly
/// what the client did before any of this existed**, so the worst case is the
/// old behaviour and nothing new can go wrong at the end of the wait.
const GROUP_WAIT_MAX_MS: u64 = 300_000;

/// May hand 1 open yet?
///
/// **Waiting is the whole of it; there is no new rule here.** Formation is now
/// about four times faster than it was (`S1-G`) while Tox group entry is
/// unchanged, so a seat reliably ratifies before the group holds everybody — and
/// a hand opened then reaches nobody, cannot be caught up, and ends at its own
/// deadline. Measured: a seat in the group from 15.1 s that received not one
/// fragment before 66.2 s while the founder played to hand 22.
///
/// After `GROUP_WAIT_MS` this returns `true` regardless, which is **exactly what
/// the client did before this gate existed**. So the worst case is unchanged and
/// the ordinary case is a table whose first hand everybody can hear.
///
/// `held_since` is `None` until the first refusal, so the clock starts when there
/// is something to wait for rather than when the process did.
async fn hand_one_may_open(
    tox: &super::toxsink::TableSink,
    held_since: &mut Option<std::time::Instant>,
    progress: &mut Option<(u64, std::time::Instant)>,
    said: &mut bool,
    events: &Events,
) -> bool {
    // **A table that owes itself a Tox carrier and has none opens nothing.**
    // `publish_hand` has no second channel by instruction (D-019's second
    // amendment), so such a client would deal into `said` and wait for events
    // it has no way to receive.
    if tox.wants_tox() && !tox.is_on_tox() {
        if !*said {
            *said = true;
            let _ = events
                .send(NodeEvent::Warning(
                    "this table's hand rides a Tox group and this client is not on one, so no hand is opened here"
                        .into(),
                ))
                .await;
        }
        return false;
    }
    if tox.group_is_complete() {
        return true;
    }
    let now = std::time::Instant::now();
    let since = *held_since.get_or_insert(now);

    // Still filling? Then keep waiting, however long it has already been.
    // Anchored on the first call for this table rather than at process start.
    let anchored = *progress.get_or_insert((tox.group_seen().0, now));
    if tox.group_seen().0 > anchored.0 {
        *progress = Some((tox.group_seen().0, now));
    }
    let anchored = progress.unwrap_or(anchored);

    // **A group that never started is not a group that stopped filling, and
    // both of the rules below could not tell them apart.**
    //
    // `progress` begins at `(0, now)`. If `seen` stays at zero it never moves,
    // so `stalled` becomes true at two minutes for a client that has not seen a
    // single other peer — and `too_long` does the same at five. Both then say
    // *"dealing anyway"*, and dealing to nobody is not a degraded table, it is
    // hands that no one will ever answer.
    //
    // Measured 2026-09-02, a four-seat run: `n3` sat at `group 0/3` for the
    // whole run with all three Tox friendships up and every seat 0 ms away on
    // the line, announced *"held 0 of 3 other seats and stopped filling …
    // dealing anyway"* at 135 s, and opened one hand that finished never. The
    // harness's own verdict was **"4 seat(s) opened hands and finished none —
    // they heard nobody"**.
    //
    // So the ceiling is not the last word: with no other seat in the group there
    // is nothing to deal *to*, and the honest move is to keep waiting and say
    // why. This cannot hold the **table** up — the other seats deal without this
    // one and §8.3's timeout certifies it out, which is exactly what happened to
    // a lagging seat in the run before this one. It holds up only this client's
    // production of hands nobody can hear.
    //
    // `want > 0` guards the no-Tox build, where `group_seen` answers `(0, 0)`
    // because there is no group to count.
    // **The floor tests the transport, not the driver's count.**
    //
    // `want` and `complete` are stored only inside the driver's sweep, once
    // every `SWEEP_EVERY` = 5 s, while the node loop calls `seat_on_tox` and
    // this function in the **same iteration** — so at the instant a table
    // ratifies, `want` is still the 0 it started at. `want > 0` was therefore
    // false exactly when the floor was needed, and the stall arm below dealt
    // anyway on any founder whose process had been up longer than
    // `GROUP_STALL_MS`.
    //
    // That is how `S1-AK`'s defect survived its own fix: making `complete`
    // honest about an empty roster moved the hole into the floor rather than
    // shutting it, and the run that appeared to prove the fix only did so
    // because the process was 45 seconds old.
    //
    // `is_on_tox()` is `inner.is_some()`, set synchronously in `start` and read
    // without asking the driver anything, so it cannot lag.
    let (seen, want) = tox.group_seen();
    if tox.is_on_tox() && seen == 0 {
        if !*said {
            *said = true;
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "not one of {want} other seats is in the table's group, so no hand is opened here: a hand dealt now would reach nobody. This client is isolated and the other seats will certify it out"
                )))
                .await;
        }
        return false;
    }

    let stalled = now.duration_since(anchored.1) >= std::time::Duration::from_millis(GROUP_STALL_MS);
    let too_long = now.duration_since(since) >= std::time::Duration::from_millis(GROUP_WAIT_MAX_MS);
    if stalled || too_long {
        // Said once, because it means the first hand is being dealt into a group
        // that does not hold everybody and somebody is about to sit through a
        // hand they cannot see.
        if !*said {
            *said = true;
            let _ = events
                .send(NodeEvent::Warning({
                    let (seen, want) = tox.group_seen();
                    let why = if too_long { "the wait's ceiling" } else { "no new seat for two minutes" };
                    format!(
                        "the table's group held {seen} of {want} other seats and stopped filling ({why}); dealing anyway"
                    )
                }))
                .await;
        }
        return true;
    }
    false
}

/// Tell the interface where the hand is, when it has moved.
///
/// One function rather than a block at each call site, because the two call
/// sites - an event arriving and this client acting - must say the same thing.
/// The comparison against the last report is what keeps a stage per action from
/// becoming a redraw per action.
/// Everything `report_hand` has told the window about a hand, as one key:
/// whose turn, the pot, the board's length, whether the hand is over, and
/// (`S1-CS`) every stack and every bet.
type HandReport = (Option<u8>, u64, u64, bool, Vec<u64>, Vec<u64>);

/// The sentence a player is told when a hand ends without a settlement.
///
/// **Which abort, and not one sentence for all of them.** This said "a peer
/// ended the hand on its own deadline" whatever happened, and since
/// `cause = 2` exists that is sometimes simply untrue: a hand ended by a
/// proof that does not hold ends in seconds, names a seat, and has nothing
/// to do with anybody's clock. A player told the wrong reason looks for the
/// wrong fault. A function since `S1-CW`: the stall tick's vote can end the
/// hand too, and says the same.
fn abort_words(why: crate::table::hand::Abort) -> String {
    use crate::table::hand::Abort;
    match why {
        Abort::BadShuffle { seat } => {
            format!("seat {seat}'s shuffle proof does not hold; the hand is void and every stack is restored")
        }
        Abort::BadReveal { seat } => format!(
            "seat {seat}'s reveal proof does not hold against the committed deck; the hand is void and every stack is restored"
        ),
        Abort::Told { cause: 2 } => {
            "a peer proved a shuffle did not hold; the hand is void and every stack is restored".into()
        }
        Abort::Told { cause: 3 } => {
            "a peer proved a reveal share did not hold; the hand is void and every stack is restored".into()
        }
        _ => "a peer ended the hand on its own deadline; every stack is restored".into(),
    }
}

async fn report_hand(
    h: &crate::table::hand::Hand,
    events: &Events,
    last: &mut Option<HandReport>,
) -> Report {
    let hand_id = h.hand_id();
    let turn = h.turn();
    // The hand being **over** is part of the key. Without it, a fold-out reads
    // as `(nobody to act, this pot, this board)` both when the fold is applied
    // and again when the settlement completes — the same tuple, so the second
    // was suppressed and `HandEnded` was never sent. The hand ended on one peer
    // and hung on the other, which is exactly what was measured.
    let now = (
        turn.as_ref().map(|t| t.seat),
        h.pot(),
        h.board().len() as u64,
        // `over`, not `betting_over`: an aborted hand has also ended, and a
        // key that could not tell the two apart would report the abort as
        // "nothing changed" and never send `HandEnded`.
        h.over(),
        // `S1-CS`: and every stack and every bet, so the window is told
        // after every action and not only when the turn changes.
        h.stacks(),
        h.bets(),
    );
    if last.as_ref() == Some(&now) {
        // Nothing new. In particular the clock is **not** re-armed: a mesh
        // redelivers, and a duplicate that reset the deadline every time would
        // be a clock that never runs out.
        return Report::default();
    }
    let board_changed = last.as_ref().map(|k| k.2) != Some(now.2);
    *last = Some(now);

    if board_changed {
        let _ = events
            .send(NodeEvent::Board {
                hand_id,
                cards: h.board().iter().map(|c| c.index()).collect(),
            })
            .await;
    }

    // `S1-CS`: the table as the engine has it, before the turn is announced.
    if let Some(street) = h.street() {
        let _ = events
            .send(NodeEvent::TableState {
                hand_id,
                street: street as u16,
                pot: h.pot(),
                to_act: turn.as_ref().map(|t| t.seat),
                stacks: h.stacks(),
                bets: h.bets(),
                folded: h.folded(),
            })
            .await;
    }

    match turn {
        Some(t) if t.mine => {
            let _ = events
                .send(NodeEvent::YourTurn {
                    hand_id,
                    street: t.street as u16,
                    to_call: t.to_call,
                    pot: t.pot,
                    can_check: t.legal.can_check,
                    can_call: t.legal.can_call,
                    can_bet: t.legal.can_bet,
                    can_raise: t.legal.can_raise,
                    min_raise_to: t.legal.min_raise_to,
                    max_raise_to: t.legal.max_raise_to,
                    elapsed_ms: turn_elapsed_ms(t.began_unix_ms),
                })
                .await;
            return Report {
                ended: None,
                clock: Clock::Start { began_unix_ms: t.began_unix_ms },
            };
        }
        Some(t) => {
            let _ = events
                .send(NodeEvent::NotYourTurn {
                    hand_id,
                    seat: Some(t.seat),
                    elapsed_ms: turn_elapsed_ms(t.began_unix_ms),
                })
                .await;
            return Report {
                ended: None,
                clock: Clock::Stop,
            };
        }
        None => {
            let _ = events
                .send(NodeEvent::NotYourTurn { hand_id, seat: None, elapsed_ms: 0 })
                .await;
            if h.over() {
                let seats = u8::try_from(h.stacks().len()).unwrap_or(0);
                let shown: Vec<Option<[u8; 2]>> = (0..seats)
                    .map(|s| h.shown(s).map(|c| [c[0].index(), c[1].index()]))
                    .collect();
                let anybody_showed = shown.iter().any(|s| s.is_some());
                let _ = events
                    .send(NodeEvent::HandEnded {
                        hand_id,
                        stacks: h.stacks(),
                        shown,
                    })
                    .await;
                return Report {
                    ended: Some(Ended { anybody_showed }),
                    clock: Clock::Stop,
                };
            }
        }
    }
    Report {
        ended: None,
        clock: Clock::Stop,
    }
}

/// What one report says.
#[derive(Default)]
struct Report {
    /// The hand ended, and whether anything was shown.
    ended: Option<Ended>,
    /// What to do with this client's own clock.
    clock: Clock,
}

/// `D-034`: how long ago a turn was given, for a window's countdown -- zero
/// when the stamp is unknown or from a clock ahead of this one.
fn turn_elapsed_ms(began_unix_ms: u64) -> u64 {
    if began_unix_ms == 0 {
        return 0;
    }
    super::node::now_unix_ms().saturating_sub(began_unix_ms)
}

/// What a report says about the clock.
///
/// Three answers and not two, because "nothing changed" must not be confused
/// with "stop": a redelivered message reports nothing, and a clock that was
/// restarted on every redelivery would never run out.
#[derive(Default, PartialEq, Eq, Clone, Copy)]
enum Clock {
    /// It is newly this client's turn: start counting -- from when the turn
    /// was given, on the giver's clock (`D-034`; zero when unknown).
    Start { began_unix_ms: u64 },
    /// It is not this client's turn: stop.
    Stop,
    /// Nothing changed. Leave the clock exactly as it is.
    #[default]
    Leave,
}

impl Clock {
    /// The new deadline, given the old one.
    fn apply(
        self,
        was: Option<tokio::time::Instant>,
        timeout: std::time::Duration,
    ) -> Option<tokio::time::Instant> {
        match self {
            // `D-034`: the thirty seconds run from the moment the turn was
            // given, on the clock of the seat that gave it, when that is
            // earlier than *now plus the timeout* -- a late delivery does not
            // add to them, and the fold lands where every other window's
            // countdown ends. Never more than the timeout, never less than
            // half a second, and the plain timeout when the stamp is unknown
            // or from a clock that is ahead of this one.
            Clock::Start { began_unix_ms } => {
                let now_unix = super::node::now_unix_ms();
                let left = if began_unix_ms == 0 || began_unix_ms > now_unix {
                    timeout
                } else {
                    let deadline = began_unix_ms.saturating_add(timeout.as_millis() as u64);
                    std::time::Duration::from_millis(deadline.saturating_sub(now_unix))
                        .min(timeout)
                        .max(std::time::Duration::from_millis(500))
                };
                Some(tokio::time::Instant::now() + left)
            }
            Clock::Stop => None,
            Clock::Leave => was,
        }
    }
}

impl Ended {
    /// How long to hold the screen before the next hand.
    ///
    /// D-020: about five seconds at a showdown, so a player can see what beat
    /// them. It is a **local** hold and not a stage — `HAND_INIT` is collective
    /// and already tolerates a seat that is late, so nothing has to agree about
    /// it and a client that wants none is still in protocol.
    fn pause(&self) -> std::time::Duration {
        if self.anybody_showed {
            std::time::Duration::from_secs(5)
        } else {
            // Everybody folded: nothing to read. A beat rather than nothing, so
            // the table does not jump straight into the next deal.
            std::time::Duration::from_millis(800)
        }
    }
}

/// A hand that has just ended, and the one thing the pause depends on.
struct Ended {
    /// Whether anything was put on the table. D-020 holds the screen so a
    /// player can read what beat them; a hand where everybody folded has
    /// nothing to read, and holding a blank table for five seconds is worse
    /// than not holding it.
    anybody_showed: bool,
}

/// Whether the table has stopped being open to anybody.
///
/// **Not "the roster ratified".** That was the first version and it was wrong:
/// a Sit-and-Go ratifies the roster it has, which for a table still filling is
/// two players out of ten, and a table that goes quiet while it is waiting is a
/// table nobody can find to join. The signal is that every seat is taken.
///
/// **And for a tournament it latches.** A Sit-and-Go that has been full has
/// started, and a seat that frees afterwards is somebody who busted, not a seat
/// on offer — so it must not go back on the market. A cash table is the
/// opposite: people arrive and leave between hands, and a free seat is exactly
/// what it wants to advertise. `latched` carries the tournament's answer; the
/// cash answer is computed fresh every time.
fn table_is_closed(f: &Formation, latched: &mut bool) -> bool {
    let full = f.session().is_some() && f.roster().len() as u8 >= f.advert().max_players;
    let tournament = f.advert().mode == super::lobby::Mode::TournamentSngPlayMoney.code();
    if tournament {
        *latched |= full;
        *latched
    } else {
        full
    }
}

/// A GossipSub source as the thirty-two bytes a rate limiter is keyed on.
///
/// The same reduction the advert path does inline. A peer id is a multihash and
/// is usually longer than a key; what matters here is only that one peer maps
/// to one budget, so a prefix is enough and a collision costs an honest peer a
/// share of a stranger's allowance rather than anything worse.
fn peer_bytes(from: &libp2p::PeerId) -> [u8; 32] {
    let mut peer = [0u8; 32];
    let bytes = from.to_bytes();
    let take = bytes.len().min(32);
    peer[..take].copy_from_slice(&bytes[..take]);
    peer
}

/// Whether an address is one the rest of the internet could dial.
///
/// A relay is only useful if the address it was reached on is reachable from
/// anywhere, because that address becomes part of this client's own circuit
/// address and is what other players will dial. Loopback and the private ranges
/// are somebody's own network; `0.0.0.0` is a wildcard bind and not an address
/// at all.
fn reachable(addr: &libp2p::Multiaddr) -> bool {
    use libp2p::multiaddr::Protocol;
    addr.iter().any(|p| match p {
        Protocol::Ip4(ip) => {
            !ip.is_loopback()
                && !ip.is_private()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_broadcast()
                && !ip.is_documentation()
        }
        // **IPv6's private ranges, spelled out because `std` will not do it on
        // stable.** `Ipv6Addr::is_unique_local` and `is_unicast_link_local` are
        // both unstable, and the v6 arm here used to test only loopback and
        // `::`, so `fc00::/7` — the exact counterpart of `10/8` and
        // `192.168/16` — read as *"the rest of the internet could dial this"*.
        //
        // That was harmless while the client bound no IPv6 socket at all. It
        // stopped being harmless in the same commit that added one: this
        // machine's own v6 addresses are `fdc9:6d69:…`, a ULA, and without
        // these two masks a LAN-only address would be offered as a relay
        // endpoint and, if anything ever confirmed it, published in a provider
        // record for strangers to fail to dial.
        Protocol::Ip6(ip) => {
            let first = ip.segments()[0];
            !ip.is_loopback()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && (first & 0xfe00) != 0xfc00  // fc00::/7, unique local
                && (first & 0xffc0) != 0xfe80  // fe80::/10, link-local unicast
                && ip.segments()[..2] != [0x2001, 0x0db8] // 2001:db8::/32, documentation
        }
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    /// **The rendezvous key, pinned to its bytes.** `NETWORK_STACK.md` §3.2.
    ///
    /// This is the one constant on which two independent implementations find
    /// each other or do not. `namespace` says it is *"derived here rather than
    /// pasted, so the derivation is visible and testable"* — it was visible and
    /// nothing tested it, so a change to the namespace string, the hash, or the
    /// multihash prefix would have moved the whole lobby somewhere else in
    /// silence, and every client would have kept working perfectly alone.
    ///
    /// The value is written out rather than recomputed by the test, because a
    /// test that recomputes the thing it checks passes whatever the code does.
    #[test]
    fn the_lobby_rendezvous_key_is_the_published_one() {
        assert_eq!(
            hex(lobby_namespace().to_vec().as_slice()),
            "12207e342925602a7c6558d6ac574207bcc56f7989e17ad52b7694f2c964d772c4b6",
            "sha2-256 of \"p2p-poker/main-lobby/v1\" under the 0x12 0x20 multihash prefix"
        );
        assert_eq!(
            hex(relay_namespace().to_vec().as_slice()),
            "1220245eebd20d2cd4c81b5d4ac27c73746279f436d62f3ef52c452a369e6ef7b610",
            "and the relay namespace is go-libp2p's own \"/libp2p/relay\""
        );
        // The shape, so a wrong prefix fails for the reason it is wrong rather
        // than as an opaque byte mismatch.
        let k = lobby_namespace().to_vec();
        assert_eq!(k.len(), 34, "a multihash: two bytes of prefix and a digest");
        assert_eq!(k[0], 0x12, "sha2-256");
        assert_eq!(k[1], 0x20, "thirty-two bytes of it");
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    use super::*;

    /// A table name is bounded in **bytes**, because that is what goes on the
    /// wire, and the first version counted characters.
    ///
    /// Thirty-two characters of Czech is up to sixty-four bytes and of emoji up
    /// to a hundred and twenty-eight, so a name a person would call short passed
    /// the trim and was then refused by this client's own §7.2 admission — the
    /// table appeared in nobody's lobby, its founder's included, and the only
    /// symptom was a warning about this client's own advertisement.
    #[test]
    fn a_name_is_trimmed_by_bytes_and_stays_text() {
        let cards = "\u{1F0A1}".repeat(40);
        let rs = "\u{159}".repeat(60);
        let xs = "x".repeat(200);
        for source in [
            "Riverside",
            "A table whose name is long enough that it will not fit in the field",
            cards.as_str(),
            rs.as_str(),
            xs.as_str(),
        ] {
            let cut = trim_to_bytes(source, TABLE_NAME_MAX);
            assert!(
                cut.len() <= TABLE_NAME_MAX,
                "{source:?} trimmed to {} bytes",
                cut.len()
            );
            assert!(source.starts_with(&cut), "the trim changed the text");
            // Still text. `is_char_boundary` is the whole point: a byte-wise cut
            // would have produced something that is not.
            assert!(std::str::from_utf8(cut.as_bytes()).is_ok());
        }
        assert_eq!(trim_to_bytes("Riverside", TABLE_NAME_MAX), "Riverside");
    }

    /// And the table this client founds passes this client's **own** admission
    /// rules, whatever name it is given. An advertisement its own author refuses
    /// is a table nobody will ever see.
    #[test]
    fn a_table_this_client_founds_is_one_it_would_accept() {
        const NOW: u64 = 1_700_000_000_000;
        let cards = "\u{1F0A1}".repeat(40);
        for name in [
            "Riverside",
            "A table whose name is very much longer than the sixty-four bytes allowed",
            cards.as_str(),
        ] {
            let ad = new_table(
                name.to_string(),
                6,
                2,
                1_000,
                false,
                [1u8; 32],
                vec![2u8; 38],
                NOW,
            );
            assert_eq!(
                super::super::lobby::admit(&ad, NOW),
                Ok(()),
                "this client would refuse its own table called {name:?}"
            );
        }
    }

    /// The announce carries a port, and announcing one nothing is bound to
    /// publishes an unreachable address to a hundred strangers every ten
    /// minutes. So the port is read from what the swarm actually bound.
    #[test]
    fn only_a_quic_listen_address_yields_a_port() {
        let quic: Multiaddr = "/ip4/192.168.1.5/udp/54321/quic-v1".parse().unwrap();
        assert_eq!(quic_port(&quic), Some(54321));

        let tcp: Multiaddr = "/ip4/192.168.1.5/tcp/54321".parse().unwrap();
        assert_eq!(quic_port(&tcp), None, "a TCP port is not the QUIC port");

        let bare_udp: Multiaddr = "/ip4/192.168.1.5/udp/54321".parse().unwrap();
        assert_eq!(quic_port(&bare_udp), None, "and UDP alone is not QUIC");
    }

    /// Port 0 by default: the OS chooses, so nothing collides with another
    /// instance.
    #[test]
    fn the_node_binds_ports_the_os_chooses() {
        let addrs = listen_addrs(0);
        assert_eq!(addrs.len(), 2);
        for a in &addrs {
            let s = a.to_string();
            assert!(
                s.contains("/udp/0/") || s.ends_with("/tcp/0"),
                "{s} does not bind port 0"
            );
        }
    }

    /// And a number when one is given, on both transports and the same one.
    ///
    /// The whole point is a router rule, and a rule names a number. Two
    /// different numbers would be two rules for no reason: UDP and TCP are
    /// different sockets and do not collide.
    #[test]
    fn a_fixed_port_is_used_on_both_transports() {
        let addrs = listen_addrs(4242);
        let shown: Vec<String> = addrs.iter().map(|a| a.to_string()).collect();
        assert!(shown.contains(&"/ip4/0.0.0.0/udp/4242/quic-v1".to_owned()), "{shown:?}");
        assert!(shown.contains(&"/ip4/0.0.0.0/tcp/4242".to_owned()), "{shown:?}");
    }

    /// A LAN address is not an address the rest of the internet can dial, and
    /// that is as true in IPv6 as in IPv4.
    ///
    /// The v6 arm of `reachable` tested loopback and `::` and nothing else,
    /// which was survivable only for as long as the client never bound a v6
    /// socket. It binds two now, and the addresses it gets on an ordinary home
    /// network are `fc00::/7` unique-local ones.
    #[test]
    fn a_private_ipv6_address_is_not_reachable_from_outside() {
        let yes = |s: &str| reachable(&s.parse::<Multiaddr>().expect("a literal"));
        // Real addresses this machine was measured to bind, 2026-09-02.
        assert!(!yes("/ip6/fdc9:6d69:ed51:0:2081:e457:503:2311/tcp/33774"), "a ULA is a LAN address");
        assert!(!yes("/ip6/fe80::1/tcp/1"), "link-local");
        assert!(!yes("/ip6/::1/tcp/1"), "loopback");
        assert!(!yes("/ip6/::/tcp/1"), "the wildcard bind is not an address");
        assert!(!yes("/ip6/2001:db8::1/tcp/1"), "the documentation range");
        assert!(!yes("/ip6/ff02::1/tcp/1"), "multicast");
        // And a real one still passes, or the client could never be reached
        // over IPv6 at all — which was the state this whole change is fixing.
        assert!(yes("/ip6/2606:4700:4700::1111/udp/443/quic-v1"), "a global unicast address");

        // The v4 arm, unchanged, restated so a future edit cannot quietly
        // loosen one family while tightening the other.
        assert!(!yes("/ip4/192.168.1.20/tcp/1"));
        assert!(!yes("/ip4/127.0.0.1/tcp/1"));
        // Not `203.0.113.4`: that is TEST-NET-3, and `reachable` refuses the
        // documentation ranges deliberately. `peerbook`'s own filter does not,
        // which is why its tests use it as a good address — two filters, two
        // jobs, and this one is stricter.
        assert!(yes("/ip4/1.1.1.1/tcp/1"));
    }

    /// **One hand behind is what an ordinary table looks like, and it used to be
    /// enough to evict yourself for ever.**
    ///
    /// `note_a_hand_ahead` latched as soon as two seats reported any higher hand
    /// id. Measured over 134 runs left on disk: 37 seats latched themselves out,
    /// **36 of them at a margin of exactly one**, and the single true positive
    /// was five hands behind. `Ended::pause()` holds a finished hand at showdown
    /// for five seconds so a player can see the cards, so on any table of four
    /// the seats that have already paid that pause are a hand ahead of the one
    /// still inside it — always, and at least two of them, which is exactly the
    /// `saying.len() >= 2` test. The latch was firing on the table working.
    #[test]
    fn one_hand_behind_is_not_adrift_and_two_is() {
        use std::collections::HashMap;
        let mine = 6u64;
        let at = |pairs: &[(u8, u64)]| -> HashMap<u8, u64> {
            pairs.iter().copied().collect()
        };
        let three = [1u8, 2, 3];
        let two = [1u8, 2];

        // Three seats, one hand ahead: the showdown-pause skew, and the shape of
        // thirty-six of the thirty-seven real evictions.
        assert_eq!(
            adrift_now(mine, &at(&[(1, mine + 1), (2, mine + 1), (3, mine + 1)]), &three),
            None,
            "three seats one hand ahead is a pause, not a divergence"
        );

        // Two hands is past any pause.
        assert_eq!(
            adrift_now(mine, &at(&[(1, mine + 2), (2, mine + 2)]), &two),
            Some((mine + 2, mine)),
            "two hands behind is adrift"
        );

        // The one true positive on record: five hands, run212350-4.
        assert_eq!(
            adrift_now(6, &at(&[(1, 11), (2, 11), (3, 10)]), &three),
            Some((11, 6)),
            "the case this mechanism exists for still fires"
        );

        // One seat of two is never the table, whatever the margin.
        assert_eq!(adrift_now(mine, &at(&[(1, mine + 9)]), &two), None, "one seat of two is not the table");

        // And a table nobody is ahead of says nothing.
        assert_eq!(adrift_now(mine, &at(&[(1, mine), (2, mine - 1)]), &two), None);
    }

    /// **`D-038`: two seats are not the table, a strict majority of the seats
    /// still in the game is.** `split111706-9` had two seats aborting their way
    /// through hands nobody dealt, and six healthy seats latched themselves out
    /// on their word. Broken deliberately: with the old `>= 2` rule the first
    /// assertion fails.
    #[test]
    fn a_minority_two_hands_ahead_is_not_the_table_and_a_majority_is() {
        use std::collections::HashMap;
        let mine = 1u64;
        let at = |pairs: &[(u8, u64)]| -> HashMap<u8, u64> {
            pairs.iter().copied().collect()
        };
        let eight: Vec<u8> = (1u8..=8).collect();

        // Two of eight, five and two hands on: a private branch, not the table.
        assert_eq!(adrift_now(mine, &at(&[(1, 6), (6, 3)]), &eight), None, "two of eight is a minority");
        // Four of eight is not a strict majority either.
        assert_eq!(adrift_now(mine, &at(&[(1, 3), (2, 3), (3, 3), (4, 3)]), &eight), None, "four of eight is half");
        // Five of eight, two hands on, is the table gone on without this seat.
        assert_eq!(
            adrift_now(mine, &at(&[(1, 3), (2, 3), (3, 3), (4, 3), (5, 4)]), &eight),
            Some((4, mine)),
            "five of eight two hands on is the table"
        );
        // A seat that is one hand on does not count towards the majority.
        assert_eq!(
            adrift_now(mine, &at(&[(1, 3), (2, 3), (3, 3), (4, 3), (5, 2)]), &eight),
            None,
            "a seat one hand on is inside the pause"
        );
        // A seat outside the game as this client knows it -- certified out, still
        // dealing alone -- is not evidence, however far it has got.
        assert_eq!(adrift_now(mine, &at(&[(9, 40), (10, 40), (11, 40)]), &eight), None, "strangers are not the table");
        // Heads-up the one other seat is the whole table.
        assert_eq!(adrift_now(mine, &at(&[(1, 3)]), &[1u8]), Some((3, mine)), "heads-up the other seat is the table");
        // And with nobody else in the game there is no table to be behind.
        assert_eq!(adrift_now(mine, &at(&[(1, 3)]), &[]), None);
    }

    /// **And IPv6, on the same port, on both transports.**
    ///
    /// `NETWORK_STACK.md` §5.8 called discovery IPv4-only and named the reason:
    /// the `mainline` crate could not do IPv6. That crate is gone and the four
    /// IPv4 literals stayed, so the client kept the limitation after losing the
    /// cause. A player on an IPv6-only network was unreachable by anybody, and
    /// nothing in the tree said so out loud — which is why this is a test and
    /// not a comment.
    #[test]
    fn ipv6_is_listened_on_too_and_on_the_same_port() {
        let shown: Vec<String> = listen_addrs_v6(4242).iter().map(|a| a.to_string()).collect();
        assert!(shown.contains(&"/ip6/::/udp/4242/quic-v1".to_owned()), "{shown:?}");
        assert!(shown.contains(&"/ip6/::/tcp/4242".to_owned()), "{shown:?}");

        // The same number as IPv4's, because a router rule names a number and
        // two numbers would be two rules for no reason.
        for a in listen_addrs(4242).iter().chain(listen_addrs_v6(4242).iter()) {
            let s = a.to_string();
            assert!(s.contains("/4242"), "{s} is not on the port that was asked for");
        }
    }

}

#[cfg(test)]
mod the_pre_open_buffer {
    use super::{keep_early, NEXT_EARLY_CAP, NEXT_EARLY_PER_SEAT};
    use crate::table::hand::{frame_ceiling, NEXT_EARLY_BYTES_PER_SEAT};
    use crate::protocol::messages::EventType;

    /// A real sealed frame of `kind` at `seq`, whose payload is `fill` bytes.
    ///
    /// Built with `seal` rather than by hand because everything under test
    /// reads the sequence, the type and the length out of the bytes it is
    /// given, and a hand-rolled envelope would test the fixture.
    fn at(kind: EventType, seq: u64, fill: usize, salt: u8) -> Vec<u8> {
        use crate::net::chained::{seal, Slot};
        #[derive(minicbor::Encode)]
        #[cbor(array)]
        struct Probe(#[n(0)] u8, #[cbor(n(1), with = "minicbor::bytes")] Vec<u8>);
        seal(
            kind,
            &Slot {
                table_id: [6u8; 32],
                hand_id: 2,
                sequence: seq,
                previous_event_hash: [9u8; 32],
            },
            &Probe(salt, vec![salt; fill]),
            &ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]),
            1_700_000_000_000,
            30_000,
            crate::table::hand::FRAME_CAP,
        )
        .expect("a sealed probe")
    }

    /// A small frame: `DECK_COMMIT` is the smallest cap in the pre-bet run.
    fn frame(seq: u64, salt: u8) -> Vec<u8> {
        at(EventType::DeckCommit, seq, 8, salt)
    }

    /// **The measurement that changed the unit.** Nothing between the wire and
    /// this buffer bounds a frame by its own type — `open_in_hand` is given
    /// `FRAME_CAP` and `check_envelope` never reads the payload's length — so
    /// every one of the old thirty-two slots was a sixteen-kilobyte slot and a
    /// `DECK_COMMIT` whose cap is 160 bytes could hold one for a whole
    /// boundary. Charging each frame its own ceiling is what makes eighty slots
    /// cost less than the old thirty-two.
    #[test]
    fn a_frame_larger_than_its_own_type_allows_is_refused_at_the_door() {
        let mut held: Vec<(u8, Vec<u8>)> = Vec::new();
        let mut lost = (0u32, 0u32);

        let fat = at(EventType::DeckCommit, 5, 2_000, 1);
        assert!(
            fat.len() > frame_ceiling(EventType::DeckCommit),
            "the fixture must exceed the ceiling to test it"
        );
        keep_early(&mut held, 1, &fat, &mut lost);
        assert!(held.is_empty(), "refused");
        assert_eq!(lost, (1, 0), "and counted, so a run can tell this from a loss");

        let ok = frame(5, 1);
        assert!(ok.len() <= frame_ceiling(EventType::DeckCommit));
        keep_early(&mut held, 1, &ok, &mut lost);
        assert_eq!(held.len(), 1, "a legal frame of the same type is admitted");
    }

    /// **The bytes bind before the slots.** A seat's share is one whole run to
    /// the first bet plus one spare frame of the largest type; eight
    /// `SHUFFLE_PROOF`s do not fit in it, and the eviction that follows is by
    /// sequence like every other.
    #[test]
    fn the_bytes_bind_before_the_slots_when_the_frames_are_large() {
        let mut held: Vec<(u8, Vec<u8>)> = Vec::new();
        let mut lost = (0u32, 0u32);
        for i in 0..NEXT_EARLY_PER_SEAT {
            keep_early(
                &mut held,
                3,
                &at(EventType::ShuffleProof, 100 + i as u64, 6_000, i as u8),
                &mut lost,
            );
        }
        let bytes: usize = held.iter().map(|(_, b)| b.len()).sum();
        assert!(
            held.len() < NEXT_EARLY_PER_SEAT,
            "the byte share bound before the slot count did: {} slots",
            held.len()
        );
        assert!(
            bytes <= NEXT_EARLY_BYTES_PER_SEAT,
            "{bytes} over the per-seat share"
        );
    }

    /// A full buffer gives up its highest sequence for a lower arrival. The old
    /// rule was a match guard, and a guard that fails drops the arrival — which
    /// is how a bystander came to open a hand holding exactly the first
    /// thirty-two frames to arrive and none of what it needed next.
    #[test]
    fn a_full_buffer_gives_up_its_highest_sequence_for_a_lower_arrival() {
        let mut held: Vec<(u8, Vec<u8>)> = Vec::new();
        let mut lost = (0u32, 0u32);
        // Ten seats at their full allowance: the slot cap is exactly that.
        for s in 0..10u8 {
            for i in 0..NEXT_EARLY_PER_SEAT {
                keep_early(
                    &mut held,
                    s,
                    &frame(100 + i as u64, s * 16 + i as u8),
                    &mut lost,
                );
            }
        }
        assert_eq!(held.len(), NEXT_EARLY_CAP, "full");

        keep_early(&mut held, 4, &frame(18, 200), &mut lost);
        assert!(
            held.iter().any(|(_, b)| b[..] == frame(18, 200)[..]),
            "the low arrival is in"
        );
        assert!(lost.1 >= 1, "and something was evicted for it");
        assert!(held.len() <= NEXT_EARLY_CAP, "the cap still binds");
    }

    /// The other half, and it is what makes the rule safe: a client that is
    /// behind needs the low sequences, so an arrival that is itself the highest
    /// is the one to drop.
    #[test]
    fn an_arrival_above_everything_held_is_the_one_dropped() {
        let mut held: Vec<(u8, Vec<u8>)> = Vec::new();
        let mut lost = (0u32, 0u32);
        for i in 0..NEXT_EARLY_PER_SEAT {
            keep_early(&mut held, 2, &frame(10 + i as u64, i as u8), &mut lost);
        }
        let before: Vec<Vec<u8>> = held.iter().map(|(_, b)| b.clone()).collect();

        keep_early(&mut held, 2, &frame(9_999, 200), &mut lost);
        let after: Vec<Vec<u8>> = held.iter().map(|(_, b)| b.clone()).collect();
        assert_eq!(before, after, "nothing moved and the arrival was dropped");
    }

    /// The per-seat allowance is enforced against **that seat's own** highest,
    /// so one loud seat cannot spend its eviction on another seat's low
    /// sequences. That is the exact-partition property: the aggregate budget is
    /// the sum of the shares, so no seat is ever evicted on another's behalf.
    #[test]
    fn a_seat_at_its_allowance_evicts_only_its_own() {
        let mut held: Vec<(u8, Vec<u8>)> = Vec::new();
        let mut lost = (0u32, 0u32);
        for i in 0..NEXT_EARLY_PER_SEAT {
            keep_early(&mut held, 3, &frame(500 + i as u64, i as u8), &mut lost);
        }
        keep_early(&mut held, 5, &frame(1, 200), &mut lost);

        keep_early(&mut held, 3, &frame(20, 201), &mut lost);
        assert!(
            held.iter().any(|(s, b)| *s == 5 && b[..] == frame(1, 200)[..]),
            "seat 5's low frame is untouched"
        );
        assert_eq!(
            held.iter().filter(|(s, _)| *s == 3).count(),
            NEXT_EARLY_PER_SEAT,
            "and seat 3's allowance still binds"
        );
        assert!(
            held.iter().any(|(s, b)| *s == 3 && b[..] == frame(20, 201)[..]),
            "with the low arrival in it"
        );
    }

    /// **A payload-maximal frame of every type this buffer carries must fit its
    /// own ceiling**, or the gate refuses a legal frame — and `Hand::hold`
    /// answers `Malformed`, which the node turns into a gossip `Reject`, so the
    /// client stops forwarding it too. A `const` assertion cannot check this:
    /// the envelope's size is a property of the encoder rather than of the
    /// constants, which is why `ENVELOPE_MAX` is the specification's 384 and
    /// not the 214 the fields count to.
    ///
    /// The sequence and `hand_id` are large on purpose: CBOR encodes small
    /// integers in one byte and large ones in nine, and a test that seals at
    /// sequence 0 measures the cheapest envelope there is.
    #[test]
    fn a_payload_maximal_frame_of_every_carried_type_fits_its_own_ceiling() {
        use crate::table::hand::{
            ACTION_CAP, BOARD_REVEAL_CAP, DECK_COMMIT_CAP, DECK_INIT_CAP, DEAL_PRIVATE_CAP,
            HAND_INIT_CAP, SHOWDOWN_MUCK_CAP, SHOWDOWN_REVEAL_CAP, SHUFFLE_PROOF_CAP,
            SHUFFLE_STEP_CAP,
        };
        use crate::net::chained::{seal, Slot};
        #[derive(minicbor::Encode)]
        #[cbor(array)]
        struct Fat(#[cbor(n(0), with = "minicbor::bytes")] Vec<u8>);

        // The payload cap counts the payload's own encoding, so a byte string
        // of `cap` bytes is slightly over it; `cap - 8` leaves room for the
        // CBOR header and still measures the worst case within a few bytes.
        for (kind, cap) in [
            (EventType::HandInit, HAND_INIT_CAP),
            (EventType::DeckInit, DECK_INIT_CAP),
            (EventType::ShuffleStep, SHUFFLE_STEP_CAP),
            (EventType::ShuffleProof, SHUFFLE_PROOF_CAP),
            (EventType::DeckCommit, DECK_COMMIT_CAP),
            (EventType::DealPrivate, DEAL_PRIVATE_CAP),
            (EventType::BoardReveal, BOARD_REVEAL_CAP),
            (EventType::ShowdownReveal, SHOWDOWN_REVEAL_CAP),
            (EventType::ShowdownMuck, SHOWDOWN_MUCK_CAP),
            (EventType::ActionBet, ACTION_CAP),
            // The four `hold` also gates but the pre-open buffer refuses by
            // kind. `hold` applies the ceiling to everything, and a certificate
            // rejected as `Malformed` would be rejected for the gossip mesh
            // too — which is a roster fault, not a buffering one.
            (EventType::TimeoutVote, crate::table::hand::TIMEOUT_VOTE_CAP),
            (EventType::TimeoutCert, crate::table::hand::TIMEOUT_CERT_CAP),
            (EventType::ReturnVote, crate::table::returnwire::RETURN_VOTE_CAP),
            (EventType::ReturnCert, crate::table::returnwire::RETURN_CERT_CAP),
            (EventType::HandComplete, crate::table::hand::HAND_COMPLETE_CAP),
            (EventType::StateHash, 512),
        ] {
            let wire = seal(
                kind,
                &Slot {
                    table_id: [6u8; 32],
                    hand_id: u64::MAX / 3,
                    sequence: u64::MAX / 5,
                    previous_event_hash: [9u8; 32],
                },
                &Fat(vec![7u8; cap - 8]),
                &ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]),
                u64::MAX / 7,
                u32::MAX,
                crate::table::hand::FRAME_CAP,
            )
            .expect("a sealed maximal frame");
            let ceiling = frame_ceiling(kind);
            assert!(
                wire.len() <= ceiling,
                "{kind:?}: a payload-maximal frame is {} bytes against a ceiling of \
                 {ceiling} — the gate would refuse a legal frame, and `hold` answers \
                 Malformed, which also stops it being forwarded",
                wire.len()
            );
        }
    }

    /// The same bytes twice are one entry. The five-second re-send puts every
    /// recent stage on the wire again at ticks 2, 4, 8, 16, 32, so without this
    /// a buffer of eighty is a buffer of a handful of distinct frames.
    #[test]
    fn a_byte_duplicate_takes_no_second_slot() {
        let mut held: Vec<(u8, Vec<u8>)> = Vec::new();
        let mut lost = (0u32, 0u32);
        keep_early(&mut held, 1, &frame(7, 0), &mut lost);
        keep_early(&mut held, 1, &frame(7, 0), &mut lost);
        assert_eq!(held.len(), 1);
        assert_eq!(lost, (0, 0), "a duplicate is neither refused nor an eviction");
    }
}

#[cfg(test)]
mod the_fast_lane {
    use super::{earns_a_fast_look, the_failure_mends_itself, FAST_REDIAL_BUDGET};
    use libp2p::swarm::DialError;

    /// **Which failures mend by themselves, and it is the closed list that
    /// matters.**
    ///
    /// The first shape of `S1-CB` counted every `OutgoingConnectionError`, and
    /// the one that made it net harmful was `Denied` — this client's own
    /// connection limiter refusing a connection that was **already
    /// established**. It is excluded because a saturated limiter stays
    /// saturated, not because of whose fault it is: 95% of the `Transport`
    /// failures this predicate *admits* are also this client's own, and they
    /// are admitted because they clear in seconds. The arm above swallows
    /// `Denied` only while the connection budget is below its ceiling, and 637
    /// of the 644 logs on disk reach that ceiling at a median of 24.4 s, so for
    /// most of every run it falls straight through to the counter.
    #[test]
    fn only_failures_that_mend_by_themselves_buy_a_second_look() {
        assert!(the_failure_mends_itself(&DialError::Transport(Vec::new())));
        assert!(the_failure_mends_itself(&DialError::WrongPeerId {
            obtained: libp2p::PeerId::random(),
            address: "/ip4/127.0.0.1/tcp/1".parse().unwrap(),
        }));

        // **`Denied`, which is what the paragraph above is about and what
        // nothing pinned.** It is not a hypothetical at this arm: the arm that
        // swallows it does so only while the connection budget is below
        // `CONNECTION_CEILING` = 320, and 637 of the 644 logs on disk reach that
        // ceiling, at a median of 24.4 s, 634 of them inside the 120 s the lane
        // is open. So for most of the window this predicate is the only thing
        // between this client's own limiter refusing an **already established**
        // connection and that peer being charged a failure for it.
        assert!(!the_failure_mends_itself(&DialError::Denied {
            cause: libp2p::swarm::ConnectionDenied::new(std::io::Error::other("our own limiter")),
        }));

        // **`Aborted` is this client cancelling its own dial, and it cannot
        // happen here at all.** Its only producer is the cancellation of
        // `PendingConnection.abort_notifier`, whose only taker is
        // `Pool::disconnect`, whose only callers are `Swarm::disconnect_peer_id`
        // and `ToSwarm::CloseConnection` — neither of which this tree contains.
        // It was admitted to the list as *"the connection dropped in flight"*,
        // which is the opposite of what it means.
        assert!(!the_failure_mends_itself(&DialError::Aborted));

        assert!(
            !the_failure_mends_itself(&DialError::LocalPeerId {
                address: "/ip4/127.0.0.1/tcp/1".parse().unwrap(),
            }),
            "dialling ourselves is not the peer's fault"
        );
        assert!(
            !the_failure_mends_itself(&DialError::NoAddresses),
            "no address is this client's own view, not a failure of the peer"
        );
        assert!(
            !the_failure_mends_itself(&DialError::DialPeerConditionFalse(
                libp2p::swarm::dial_opts::PeerCondition::Disconnected
            )),
            "a condition this client set is this client's"
        );
    }

    /// **The budget is on the process, and that is the whole safety argument.**
    ///
    /// A per-provider allowance is inflatable — provider records are
    /// unauthenticated and peer ids are free — so a flooder earns a fresh one
    /// per fake and pins this client at its redial ceiling for ever. Consuming
    /// a per-process budget only removes a benefit: at zero the lane is exactly
    /// today's behaviour, which is what this test pins.
    #[test]
    fn the_budget_is_the_process_s_and_running_out_is_todays_behaviour() {
        assert!(earns_a_fast_look(true, FAST_REDIAL_BUDGET, true));
        assert!(earns_a_fast_look(true, 1, true), "the last one is still granted");
        assert!(
            !earns_a_fast_look(true, 0, true),
            "and at zero a flooder has bought nothing but the absence of a favour"
        );
        assert!(
            !earns_a_fast_look(false, FAST_REDIAL_BUDGET, true),
            "a failure that is ours never reaches the budget at all"
        );
        assert!(
            !earns_a_fast_look(true, FAST_REDIAL_BUDGET, false),
            "and the lane is shut once the opening window has passed, whatever              is left in the budget"
        );
    }
}

#[cfg(test)]
mod ad_is_admissible {
    #[test]
    fn every_table_this_client_hosts_passes_this_client_s_own_gate() {
        for seats in 2..=crate::protocol::constants::MAX_SEATS {
            let ad = super::new_table(
                "T".into(),
                seats,
                seats,
                10_000,
                false,
                [7; 32],
                vec![1, 2, 3],
                1_700_000_000_000,
            );
            assert_eq!(
                crate::net::lobby::admit(&ad, 1_700_000_000_000),
                Ok(()),
                "the advert this client publishes at {seats} seats is one it would refuse"
            );
        }
    }
}

#[cfg(test)]
mod late_roster_tests {
    use super::*;

    /// A re-open must not put this seat's first `HAND_INIT` of the hand beside
    /// its second on the wire: the hand's own events leave the re-send list,
    /// the previous hand's terminal stays.
    #[test]
    fn a_re_open_purges_the_hand_from_the_resend_list_and_keeps_the_terminal() {
        use crate::protocol::messages::EventType;
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let seal = |kind: EventType, hand_id: u64| -> Vec<u8> {
            let slot = crate::net::chained::Slot {
                table_id: [1; 32],
                hand_id,
                sequence: 0,
                previous_event_hash: [2; 32],
            };
            crate::net::chained::seal(kind, &slot, &0u8, &key, 1_000, 30_000, 4096).unwrap()
        };
        let mut said = vec![
            seal(EventType::HandAbort, 7),
            seal(EventType::HandInit, 8),
            seal(EventType::TimeoutVote, 8),
            seal(EventType::StateHash, 7),
        ];
        purge_hand_from_said(&mut said, 8);
        let kinds: Vec<(EventType, u64)> = said
            .iter()
            .map(|b| {
                let (k, h, _) = crate::net::chained::peek(b, TABLE_FRAME_PEEK).unwrap();
                (k, h)
            })
            .collect();
        assert_eq!(kinds, vec![(EventType::HandAbort, 7), (EventType::StateHash, 7)]);
    }

    /// Ties deal on, with this client's own seat counted on its side.
    #[test]
    fn the_boundary_waits_only_for_a_strict_majority_elsewhere() {
        assert!(wait_at_boundary(7, 0));
        assert!(wait_at_boundary(3, 1));
        assert!(!wait_at_boundary(2, 1), "two against two: a tie deals on");
        assert!(!wait_at_boundary(2, 2));
        assert!(!wait_at_boundary(0, 0));
        assert!(!wait_at_boundary(1, 0), "one seat's word is one seat's word");
    }

    /// The boundary keeps the next hand whole, not only its `HAND_INIT`
    /// (`S1-BW`).
    #[test]
    fn the_boundary_keeps_every_kind_of_the_next_hand() {
        use crate::protocol::messages::EventType;
        assert_eq!(keep_for_next_hand(Some(EventType::HandInit), 8, 7), Keep::Init);
        assert_eq!(keep_for_next_hand(Some(EventType::DeckInit), 8, 7), Keep::Early);
        assert_eq!(keep_for_next_hand(Some(EventType::ShuffleStep), 8, 7), Keep::Early);
        assert_eq!(
            keep_for_next_hand(Some(EventType::HandAbort), 8, 7),
            Keep::No,
            "a terminal is not carried into the hand it would end"
        );
        assert_eq!(keep_for_next_hand(Some(EventType::HandComplete), 8, 7), Keep::No);
        assert_eq!(keep_for_next_hand(Some(EventType::TimeoutVote), 8, 7), Keep::No);
        assert_eq!(
            keep_for_next_hand(Some(EventType::TimeoutCert), 8, 7),
            Keep::No,
            "and a certificate has the late-certificate arm"
        );
        assert_eq!(
            keep_for_next_hand(Some(EventType::ReturnCert), 8, 7),
            Keep::No,
            "the return pair is the boundary's, not the next hand's (S1-BM)"
        );
        assert_eq!(keep_for_next_hand(Some(EventType::ReturnVote), 8, 7), Keep::No);
        assert_eq!(
            keep_for_next_hand(Some(EventType::DeckInit), 9, 7),
            Keep::No,
            "two hands ahead is not the hand this client is about to open"
        );
        assert_eq!(
            keep_for_next_hand(Some(EventType::TimeoutCert), 6, 7),
            Keep::No,
            "a hand behind belongs to the late-certificate arm, not to this buffer"
        );
        assert_eq!(keep_for_next_hand(None, 8, 7), Keep::No, "unreadable");
    }

    /// A seat that signed a hand, or was muted on it, never signs it again.
    #[test]
    fn a_re_open_speaks_only_for_a_seat_that_never_signed_the_hand() {
        use crate::table::hand::Voice;
        assert_eq!(reopen_voice(true, Voice::Speak), Voice::Muted);
        assert_eq!(reopen_voice(false, Voice::Muted), Voice::Muted);
        assert_eq!(reopen_voice(false, Voice::Quiet), Voice::Speak);
        assert_eq!(reopen_voice(false, Voice::Speak), Voice::Speak, "a non-member that never signed");
    }

    /// `S1-BM`: a return's evidence is two halves by two roads, the first copy
    /// of each is the one kept, and a boundary is released whole.
    #[test]
    fn return_evidence_needs_both_halves_and_keeps_the_first_copy() {
        let mut s = SitIns::default();
        s.request(7, 3, b"req-a");
        assert!(s.complete(7).is_empty(), "a request alone is not evidence");
        s.checkpoint(7, 3, b"chk-a");
        s.request(7, 3, b"req-b");
        s.checkpoint(7, 3, b"chk-b");
        let ev = s.complete(7);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].seat, 3);
        assert_eq!(ev[0].request, b"req-a".to_vec(), "the first copy is kept");
        assert_eq!(ev[0].checkpoint, b"chk-a".to_vec());
        s.checkpoint(8, 5, b"chk");
        s.request(9, 5, b"req");
        assert!(s.complete(7).is_empty(), "a third boundary evicted the oldest");
        s.release_below(9);
        assert!(!s.by_hand.contains_key(&8));
        assert!(s.by_hand.contains_key(&9));
        // A request alone is enough to hold the deal for; a checkpoint alone is not.
        assert_eq!(s.requested(9), vec![5]);
        s.checkpoint(9, 6, b"chk");
        assert_eq!(s.requested(9), vec![5], "seat 6 sent no request");
    }

    /// `S1-CR`: what a client with no hand keeps of the running table, and
    /// the two bounds on it.
    #[test]
    fn a_resuming_client_keeps_inits_by_hand_and_the_rest_bounded() {
        use crate::protocol::messages::EventType;
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let seal = |kind: EventType, hand_id: u64, tag: u16| -> Vec<u8> {
            let slot = crate::net::chained::Slot {
                table_id: [1; 32],
                hand_id,
                sequence: 0,
                previous_event_hash: [2; 32],
            };
            crate::net::chained::seal(kind, &slot, &tag, &key, 1_000, 30_000, 4096).unwrap()
        };
        let mut inits = std::collections::BTreeMap::new();
        let mut early = Vec::new();
        assert!(!stash_for_resume(b"not a frame", &mut inits, &mut early));
        assert!(stash_for_resume(&seal(EventType::HandInit, 5, 0), &mut inits, &mut early));
        assert!(stash_for_resume(&seal(EventType::HandInit, 5, 0), &mut inits, &mut early), "a duplicate is taken and not kept twice");
        assert_eq!(inits[&5].len(), 1);
        assert!(stash_for_resume(&seal(EventType::DeckInit, 5, 1), &mut inits, &mut early));
        assert_eq!(early.len(), 1);
        assert!(stash_for_resume(&seal(EventType::HandInit, 6, 0), &mut inits, &mut early));
        assert!(stash_for_resume(&seal(EventType::HandInit, 7, 0), &mut inits, &mut early));
        assert_eq!(inits.keys().copied().collect::<Vec<_>>(), vec![6, 7], "two hand ids, the lowest out");
        assert!(early.is_empty(), "and hand 5's other frames went with it");
        assert!(stash_for_resume(&seal(EventType::HandInit, 4, 0), &mut inits, &mut early));
        assert!(!inits.contains_key(&4), "a hand below the two held is not kept");
        for i in 0..(RESUME_EARLY_CAP as u16).saturating_add(10) {
            let _ = stash_for_resume(&seal(EventType::DeckInit, 7, i), &mut inits, &mut early);
        }
        assert_eq!(early.len(), RESUME_EARLY_CAP, "bounded, oldest out");
    }
}

/// §4.10's hand boundary window at the wire — the rules `boundary_event`
/// applies before anything reaches the store (`S1-BZ`).
#[cfg(test)]
mod the_boundary_window_at_the_wire {
    use super::*;
    use crate::protocol::constants::{BOUNDARY_SEQUENCE_BASE, MAX_SEATS};
    use crate::table::seatwire;

    const TERMINAL: crate::poker::state::Hash = [7u8; 32];
    const OTHER: crate::poker::state::Hash = [8u8; 32];

    #[test]
    fn every_seat_is_admitted_at_its_own_slot_and_at_no_other() {
        for seat in 0..MAX_SEATS {
            let mine = BOUNDARY_SEQUENCE_BASE + u64::from(seat);
            assert_eq!(
                window_position(seat, mine, &TERMINAL, &TERMINAL),
                WindowPosition::Right
            );
            for other in 0..MAX_SEATS {
                if other == seat {
                    continue;
                }
                assert_eq!(
                    window_position(seat, BOUNDARY_SEQUENCE_BASE + u64::from(other), &TERMINAL, &TERMINAL),
                    WindowPosition::NotTheSeatsSlot,
                    "seat {seat} was admitted at seat {other}'s slot"
                );
            }
        }
    }

    /// The fan's one parent. A window event whose parent is anything but
    /// `TERMINAL(k)` is refused even at the right slot — otherwise a peer could
    /// chain a boundary event from a stage it invented and the window would
    /// stop being a function of an agreed value.
    #[test]
    fn the_parent_is_the_terminal_and_nothing_else() {
        assert_eq!(
            window_position(3, BOUNDARY_SEQUENCE_BASE + 3, &OTHER, &TERMINAL),
            WindowPosition::NotTheTerminal
        );
        assert_eq!(
            window_position(3, BOUNDARY_SEQUENCE_BASE + 3, &[0u8; 32], &TERMINAL),
            WindowPosition::NotTheTerminal,
            "the zero hash is not a wildcard"
        );
    }

    /// **§4.9's parent rule, which §4.10 has enforced all along and §4.9 did
    /// not** (`S1-CK`).
    ///
    /// Both sections say the same thing in the same words — §4.10:
    /// *"`previous_event_hash == TERMINAL(k)` for every event of the window
    /// whichever seat emits"*; §4.9: *"**Parent.** `previous_event_hash =
    /// TERMINAL(k)`, for every emitter and whatever the order of arrival"* —
    /// and for a while only the first had code behind it.
    ///
    /// **The scope is the whole difficulty and is asserted here first.** Only
    /// round 0's hash half chains from the terminal. An acknowledgement chains
    /// from the checkpoint's own `stage_hash` (`Boundary::state_ack_event`
    /// says so and a test pins it), and so does a reconciliation round
    /// (`round_hash_event`). A rule scoped by sequence alone would refuse both
    /// and regress the 699 measured *the boundary checkpoint of hand N agreed*
    /// lines in the corpus — which is why `NotInScope` is a distinct answer
    /// rather than a `true`.
    ///
    /// **Why it cannot hide the divergence it sits beside**, asserted at the
    /// end: `TERMINAL(k)` is a `stage_hash` over the settlement's **event
    /// hashes**, while the value checkpoint 8 compares is `state_hash`
    /// **inside** those events. Two peers that settled the same hand heard the
    /// same frames, so they hold the same terminal even when their `state_hash`
    /// differs — the position and the value are independent, which is the
    /// division §4.9 draws.
    ///
    /// **To make this fail**: drop the `round != 0` half of the scope and the
    /// acknowledgement case goes red; drop the parent comparison and the first
    /// case does. Both were run.
    #[test]
    fn the_checkpoint_parent_is_the_terminal_and_only_at_round_zero() {
        use crate::table::checkwire::Kind;

        assert_eq!(
            checkpoint_position(0, Kind::Hash, &TERMINAL, &TERMINAL),
            CheckpointPosition::Right
        );
        assert_eq!(
            checkpoint_position(0, Kind::Hash, &OTHER, &TERMINAL),
            CheckpointPosition::NotTheTerminal,
            "a checkpoint chained from a stage this client does not hold"
        );
        assert_eq!(
            checkpoint_position(0, Kind::Hash, &[0u8; 32], &TERMINAL),
            CheckpointPosition::NotTheTerminal,
            "the zero hash is not a wildcard, exactly as in the window"
        );

        // The two halves the rule is NOT about, and refusing them is the
        // regression this scope exists to avoid.
        assert_eq!(
            checkpoint_position(0, Kind::Ack, &OTHER, &TERMINAL),
            CheckpointPosition::NotInScope,
            "an acknowledgement chains from the checkpoint's own stage_hash"
        );
        assert_eq!(
            checkpoint_position(1, Kind::Hash, &OTHER, &TERMINAL),
            CheckpointPosition::NotInScope,
            "and so does a reconciliation round"
        );

        // `open_in_hand` still admits any parent, and that is correct: §4.0
        // step 10a's exemption is what lets a late checkpoint be compared at
        // all. The check belongs above it, which is where it now is.
        const TABLE: crate::poker::state::Hash = [3u8; 32];
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let body = crate::table::checkwire::StateHash {
            checkpoint: crate::table::checkwire::BOUNDARY_CHECKPOINT,
            state_hash: [1u8; 32],
            transcript_head: TERMINAL,
        };
        let slot = crate::net::chained::Slot {
            table_id: TABLE,
            hand_id: 4,
            sequence: crate::table::checkwire::hash_sequence(0).expect("round 0"),
            previous_event_hash: OTHER,
        };
        let bytes = crate::net::chained::seal(
            crate::protocol::messages::EventType::StateHash,
            &slot,
            &body,
            &key,
            1,
            0,
            512,
        )
        .expect("seals");
        let opened = crate::net::chained::open_in_hand(
            &bytes,
            512,
            crate::protocol::messages::EventType::StateHash,
            &TABLE,
            4,
        )
        .expect("and opens: the decoder is not where the rule lives");
        assert_eq!(
            checkpoint_position(0, Kind::Hash, &opened.envelope.previous_event_hash, &TERMINAL),
            CheckpointPosition::NotTheTerminal,
            "the frame the decoder admits is the frame this rule refuses"
        );
    }

    /// The slot is checked **before** the parent, and a wrong slot is reported
    /// as a wrong slot: the two refusals have different meanings to whoever
    /// reads the log, and a seat at somebody else's slot with a correct parent
    /// is the more alarming of the two.
    #[test]
    fn a_wrong_slot_is_not_reported_as_a_wrong_parent() {
        assert_eq!(
            window_position(3, BOUNDARY_SEQUENCE_BASE + 4, &OTHER, &TERMINAL),
            WindowPosition::NotTheSeatsSlot
        );
    }

    /// §4.10: a receiver *"rejects any other chained type at a
    /// `sequence >= BOUNDARY_SEQUENCE_BASE`"*. The band's ten slots belong to
    /// the three types and to nothing else — and `boundary_event` consumes such
    /// a frame rather than passing it to the hand, which is what
    /// `in_the_window` decides for it.
    #[test]
    fn the_band_belongs_to_the_three_types_alone() {
        for seat in 0..MAX_SEATS {
            assert!(seatwire::in_the_window(BOUNDARY_SEQUENCE_BASE + u64::from(seat)));
        }
        // An ordinary stage index is not in the band, so an ordinary event
        // still reaches the hand — the one thing this must not break.
        for sequence in [0u64, 1, 12, 2_047] {
            assert!(!seatwire::in_the_window(sequence));
        }
    }

    /// A seat index the table does not have has no slot at all, so a sender
    /// claiming one cannot be admitted by arithmetic that happens to line up.
    #[test]
    fn a_seat_off_the_table_has_no_slot() {
        assert_eq!(
            window_position(
                MAX_SEATS,
                BOUNDARY_SEQUENCE_BASE + u64::from(MAX_SEATS),
                &TERMINAL,
                &TERMINAL
            ),
            WindowPosition::NotTheSeatsSlot
        );
    }
}
