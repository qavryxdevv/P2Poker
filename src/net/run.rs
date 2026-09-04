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
use crate::protocol::constants::{
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

    // The table this client is forming or sitting at, and the mesh it is formed
    // on. One table at a time in this loop; multi-tabling is more than one
    // client, which is what §4.3's per-roster `peer_id` rule already allows and
    // what the lobby's own rules are written for.
    let mut table: Option<Formation> = None;
    // The hand in progress, if any.
    //
    // Started once, at the moment the roster settles. `TableReal` is emitted
    // from more than one place and fires again on later table traffic, so the
    // start has to be idempotent, or two `HAND_INIT`s would go out under one
    // seat and read as equivocation to everybody else.
    let mut hand: Option<crate::table::hand::Hand> = None;
    let mut hand_reported = false;
    // Said once while this client has reached nobody, and again if it is ever
    // alone a second time.
    let mut alone_said = false;
    // The last deck state told to the interface, so that `2m` chain stages do
    // not become `2m` identical lines in a player's log.
    let mut deck_reported: Option<(Option<u8>, bool)> = None;
    // Whether this hand's own cards have been handed to the interface.
    let mut cards_reported = false;
    // **The abort is a standing property, not an event.** `aborted()` stays
    // `Some` for the rest of the hand, and this block had no once-flag while
    // `deck_reported`, `cards_reported` and `turn_reported` all do -- so every
    // further event accepted before `next_hand_at` replaced the hand reprinted
    // the sentence. Measured in `split163641-10`: `far-n3` printed the void
    // four times, `far-n1` four, `far-n0` three. A reader counting seats from
    // a grep over-counts them, which is a bad property in the one message that
    // says a hand died.
    let mut abort_reported = false;
    // Hand events dropped for want of a draining reader, said when it moves.
    let mut inbox_dropped_said: u64 = 0;
    // The last turn told to the interface, so a stage per action does not
    // become a redraw per action.
    let mut turn_reported: Option<(Option<u8>, u64, u64, bool)> = None;
    // When the next hand may start. D-020's hold, and the only timer in this
    // loop that is about a person rather than about the network.
    let mut next_hand_at: Option<tokio::time::Instant> = None;
    // When this client stops waiting for its own player and acts for them.
    //
    // Version 1's whole answer to a stalled betting stage (D-015): there is no
    // `TIMEOUT_VOTE` and no `TIMEOUT_CERT`, so a seat that goes quiet is
    // answered by **its own client** folding for it and by nothing else. A
    // seat that goes quiet in a *cryptographic* stage has no answer here at
    // all, and that is `hand_deadline_ms`, which is not built.
    let mut act_by: Option<tokio::time::Instant> = None;

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
    // This client's own contribution to the stage now open, and the sequence
    // it belongs to.
    //
    // GossipSub has no history: a peer that joins the topic mesh after a
    // publish never sees it, and nothing re-sends. At two seats that rarely
    // bites, because both are meshed before either speaks. At three it is the
    // ordinary case - the last joiner missed the first two seats' `HAND_INIT`
    // and sat at "waiting for seats 0, 1" for ever, with no error anywhere and
    // every other line of its log identical to a healthy one.
    //
    // So this client re-sends what it already said, until the stage it said it
    // in has moved. **The stored bytes, never a re-signature**: re-signing
    // would change `emitted_at_unix_ms` and produce a second distinct body at
    // one slot, which is an equivocation proof against an honest peer
    // (`PROTOCOL.md` §5.2).
    // **Every** message this client has emitted for the hand in progress, not
    // just the newest. A peer that missed stage 0 is not helped by stage 1: it
    // is stuck at 0 and holding everything after it, and only the stage-0 bytes
    // release it. Capped, because it is a buffer and every buffer here is.
    let mut said: Vec<Vec<u8>> = Vec::new();
    let mut resend = tokio::time::interval(std::time::Duration::from_secs(5));
    resend.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Whether this table has ever dealt a hand.
    //
    // The two roads into the first hand fire again on **every later table
    // message**, and they are idempotent only because `hand.is_some()` guards
    // them. Between hands, and after the last one, `hand` is `None` and that
    // guard is open — so without this the next table message would deal hand
    // one again, on a table that has already played it, with stacks from the
    // roster rather than from the chain.
    let mut ever_dealt = false;
    // **Hand 1 waits for the Tox group to hold every seat**, bounded by
    // `GROUP_WAIT_MS`, after which it deals anyway — which is what this client
    // did before the gate existed. See `hand_one_may_open`.
    // **The boundary checkpoint's retained values, and the readmission set they
    // write** (§4.9). `boundaries` holds each retained hand's checkpoint — its
    // two stages and the comparison — and `readmitted` is `A`, read and cleared
    // at the next hand init and nowhere else.
    //
    // Held here rather than in `Hand`, because both outlive the hand they are
    // about: §4.9 admits a checkpoint-8 `STATE_HASH` of hand `k` until
    // `TERMINAL(k+1)` is fixed, and by then hand `k+1` is live.
    let mut boundaries = crate::table::boundary::Boundaries::new();
    // Which hand's `HAND_INIT` stage has already moved the previous hand's
    // checkpoint down. T47 fires once per hand and `dealt()` stays true after
    // it, so without this the transition would run on every later event.
    let mut crossed_for: Option<u64> = None;
    // Whether this node has said that its checkpoints agree. Said once; see the
    // `Took::Agreed` arm of `checkpoint_event`.
    let mut checkpoint_said = false;
    // **§6.3 step 1's freeze, and it is a latch.** `Some((k, sequence))` while
    // this peer has observed two distinct `state_hash` values at one checkpoint.
    // While it is set this client accepts no hand event and emits none: no card
    // opens, no action is applied, no chips move. §6.3: *"silently continuing is
    // what §15 forbids"*.
    //
    // **Released by exactly one thing** — a reconciliation stage that completes
    // over `R(c) ∪ W` with every value in it agreeing. Not a timer; not the
    // abort of the hand it froze in; not `TERMINAL(k)`; not a later hand's
    // checkpoint; not the table closing. The corpus lists those exclusions
    // because the previous revision left the release to be inferred and the
    // inference was wrong.
    let mut frozen: Option<(u64, u64)> = None;
    // Every occupied seat, which is the accepted set of a checkpoint stage and
    // of a reconciliation round. Refreshed where a boundary is opened.
    let mut roster_seats: Vec<u8> = Vec::new();
    // Said once: why the table has stopped. Cleared when the freeze is.
    let mut frozen_said = false;
    // Said once: why no reconciliation round could be opened. Its causes are
    // permanent ones only — a stage that has not closed yet is retried in
    // silence, because it is the ordinary case and not a fault.
    let mut no_round_said = false;
    // **Which roster seats have signed an event of a hand this client has not
    // reached, and the furthest each named.** Evidence that the table is
    // somewhere this client is not.
    let mut ahead: std::collections::HashMap<u8, u64> = std::collections::HashMap::new();
    // Set once that evidence is conclusive: `(the table's hand, this client's)`.
    // A latch, and terminal — see `note_a_hand_ahead`.
    let mut adrift: Option<(u64, u64)> = None;
    let mut adrift_said = false;
    // Said once: this client is on a TCP relay and the group is not filling.
    let mut udp_warned = false;
    // **A checkpoint copy that arrived before this client opened its own
    // boundary, held rather than dropped.**
    //
    // Peers finish a hand milliseconds apart and open their boundaries in that
    // order, so a faster seat's `STATE_HASH` routinely reaches a slower one
    // before there is a boundary to put it in. `checkpoint_event` answered that
    // with `return None` and the copy was gone — and the sender does not repeat
    // it on demand, because nothing tells it to. The slower seat then waits for
    // a value that was delivered and discarded, which reads from every side as
    // a message that never arrived.
    //
    // Bounded on both axes: two hands, and one copy per seat per hand. A hold
    // queue that grows is a way to be attacked, and a copy older than two hands
    // belongs to a boundary the store has already released anyway.
    let mut early_checkpoints: std::collections::HashMap<u64, Vec<Vec<u8>>> =
        std::collections::HashMap::new();
    // **The number nobody has: how many private addresses the real Amino DHT
    // hands this client.** `S1-AC`'s filter counts what it refuses, so the
    // figure arrives as a byproduct of the defence instead of needing a second
    // measurement pass. Taken once, as a shared handle, so the status line can
    // read it without borrowing the swarm.
    let bogons = swarm.behaviour().ipfs_kad.dropped();
    let mut bogons_said = 0u64;
    // How many §6.3 disputes this client has verified. Reported with the freeze,
    // because "none arrived" and "several arrived and agreed with me" are
    // different states and were the same silence.
    let mut disputes_seen = 0usize;
    // Group keys this client has already paired with an application key. One
    // pairing per peer per run; see `signer_of`.
    let mut taught: std::collections::HashSet<[u8; 32]> = std::collections::HashSet::new();
    let mut readmitted: Vec<u8> = Vec::new();
    let mut hand_one_held_since: Option<std::time::Instant> = None;
    // How many seats the group held when it last grew, and when that was. The
    // wait is on progress rather than on a deadline; see `hand_one_may_open`.
    // **`None` until a table exists, because a clock started at process launch
    // measures the wrong thing.** This used to be `(0, Instant::now())` at
    // start-up, so `GROUP_STALL_MS` had already elapsed for any client that had
    // been running two minutes before its table formed — which is the ordinary
    // case, not an exotic one — and the stall arm dealt hand 1 the instant the
    // roster ratified. Anchored on first use instead, exactly as `held_since`
    // is.
    let mut hand_one_progress: Option<(u64, std::time::Instant)> = None;
    let mut hand_one_forced_said = false;
    // Said once per node: the reason hand one cannot be built from the roster
    // this client holds. See `say_why_no_hand_one`.
    let mut why_no_hand_one_said = false;
    let mut table_topic: Option<gossipsub::IdentTopic> = None;
    // Empty unless this table's game traffic rides a Tox group (D-019), and
    // empty for ever in a build without `--features tox`. The lobby, the join
    // RPC and the ratification stay on the mesh either way; what moves is the
    // hand. See `net::toxsink` for why the cfg lives there and not here.
    let mut tox_sink = super::toxsink::TableSink::none();
    // Said once, when the group is really joined. On the founder that is
    // immediate; on a joiner it is after an invitation arrives **and** its chat
    // id matches the advertisement's, which is the one moment worth reporting -
    // before it, the hand has a transport that reaches nobody.
    let mut tox_group_said = false;
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
    let mut alive: std::collections::HashMap<
        PeerId,
        (std::time::Instant, Option<std::time::Duration>),
    > = std::collections::HashMap::new();
    let mut tox_refused_said: u64 = 0;
    let mut tox_invites_said: u64 = 0;
    // **How the re-send loop backs off.** `at` is the chain position it last
    // saw, and `ticks` counts five-second ticks since that position moved. A
    // table that is advancing re-sends almost nothing; a stuck one still gets
    // its first repeat after five seconds.
    // How many times the table's topic has been said again while forming.
    // Bounded, because the primitive is not free: a peer that has still not
    // heard after this many tries has a problem re-announcing will not fix.
    let mut table_announces: u32 = 0;
    let mut resend_at: u64 = u64::MAX;
    let mut resend_ticks: u32 = 0;

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

    // Whether the table this client is at has closed to newcomers. Drives
    // `dht_effort`, stops the lobby being polled by somebody who is not reading
    // it, and stops the advertisement of a table nobody can join.
    let mut table_closed = false;
    // A tournament that has once been full has started, and does not reopen.
    let mut tournament_started = false;
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
    macro_rules! hand_event {
        ($h:expr, $bytes:expr) => {{
            use crate::table::hand::Failed;
            let now = super::node::now_unix_ms();
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
                                publish_hand(sends, &mut swarm, &mut said, &tox_sink);
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
                                    if !hand_reported {
                                        hand_reported = true;
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
                                    if deck_reported != Some(deck) {
                                        deck_reported = Some(deck);
                                        let _ = events
                                            .send(NodeEvent::DeckProgress {
                                                hand_id: $h.hand_id(),
                                                shuffling: deck.0,
                                                ready: deck.1,
                                            })
                                            .await;
                                    }
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
                                    // A seat the table acted for, once.
                                    if let Some((seat, what)) = $h.take_certified_action() {
                                        let _ = events
                                            .send(NodeEvent::Warning(format!(
                                                "the table acted for seat {seat}: {what:?}"
                                            )))
                                            .await;
                                    }
                                    if let Some(why) =
                                        $h.aborted().filter(|_| !abort_reported)
                                    {
                                        abort_reported = true;
                                        act_by = None;
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
                                        let said = match why {
                                            crate::table::hand::Abort::BadShuffle { seat } => {
                                                format!(
                                                    "seat {seat}'s shuffle proof does not hold; the hand is void and every stack is restored"
                                                )
                                            }
                                            crate::table::hand::Abort::BadReveal { seat } => {
                                                format!(
                                                    "seat {seat}'s reveal proof does not hold against the committed deck; the hand is void and every stack is restored"
                                                )
                                            }
                                            crate::table::hand::Abort::Told { cause: 2 } => {
                                                "a peer proved a shuffle did not hold; the hand is void and every stack is restored".into()
                                            }
                                            crate::table::hand::Abort::Told { cause: 3 } => {
                                                "a peer proved a reveal share did not hold; the hand is void and every stack is restored".into()
                                            }
                                            _ => "a peer ended the hand on its own deadline; every stack is restored".into(),
                                        };
                                        let _ = events
                                            .send(NodeEvent::Warning(said))
                                            .await;
                                        next_hand_at = Some(
                                            tokio::time::Instant::now()
                                                + std::time::Duration::from_millis(800),
                                        );
                                    }
                                    let report =
                                        report_hand($h, &events, &mut turn_reported).await;
                                    if let Some(end) = report.ended {
                                        next_hand_at = Some(
                                            tokio::time::Instant::now() + end.pause(),
                                        );
                                    }
                                    act_by = hurry(report.clock.apply(act_by, $h.action_deadline()), autoplay);
                                    if let Some(cards) = $h.cards().filter(|_| !cards_reported) {
                                        cards_reported = true;
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
                                } else {
                                    let _ = events
                                        .send(NodeEvent::HandWaiting {
                                            hand_id: $h.hand_id(),
                                            seats: $h.waiting_for(),
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
                                        note_a_hand_ahead(
                                            $h,
                                            hand_id,
                                            seat,
                                            &mut ahead,
                                            &mut adrift,
                                        );
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
                                let _ = events
                                    .send(NodeEvent::Warning(format!("hand: {e}")))
                                    .await;
                                Some(gossipsub::MessageAcceptance::Reject)
                            }
                        }
        }};
    }
    loop {
        tokio::select! {
            event = SwarmStreamExt::select_next_some(&mut swarm) => {
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
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
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
                                let Some(f) = table.as_mut() else {
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
                                    tox_sink.tell(super::toxsink::Seat::Back(k));
                                }
                                match f.on_join_request(&request, &authenticated, ever_dealt, now) {
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
                                                    if let Some(t) = &table_topic {
                                                        let _ = swarm
                                                            .behaviour_mut()
                                                            .gossipsub
                                                            .publish(t.clone(), bytes);
                                                    }
                                                }
                                            }
                                        }
                                        report_roster(&events, f).await;
                                seat_on_tox(f, &tox_sink);
                                        if let Some(session) = f.session() {
                                            let _ = events.send(NodeEvent::TableReal {
                                                key: f.table_id(),
                                                session,
                                            }).await;
                                            // Only once every seat is taken.
                                            // A table that is still filling
                                            // must stay as findable as it was.
                                            table_closed = table_is_closed(f, &mut tournament_started);
                                            dht_effort(&mut swarm, table_closed);
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
                                            if !ever_dealt
                                                && hand_one_may_open(
                                                    &tox_sink,
                                                    &mut hand_one_held_since,
                                                    &mut hand_one_progress,
                                                    &mut hand_one_forced_said,
                                                    &events,
                                                )
                                                .await
                                            {
                                                say_why_no_hand_one(f, &mut why_no_hand_one_said, &events).await;
                                                if let Some(o) = opening_for_hand_one(f) {
                                                    ever_dealt = true;
                                                    begin_hand(
                                                        o,
                                                        &app_key,
                                                        &mut hand,
                                                                                            &mut said,
                                                        &mut swarm,
                                                        &events,
                                                        &tox_sink,
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
                                let Some(f) = table.as_mut() else { continue };
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
                                seat_on_tox(f, &tox_sink);
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
                                        table = None;
                                        // The Tox group goes with the table. Dropping the handle
                                        // tells the driver to leave and joins its thread, which
                                        // flushes what it still holds - the last message of a hand
                                        // is exactly what sits in that queue.
                                        tox_sink.clear();
                            tox_group_said = false;
                            table_announces = 0;
                                        if let Some(t) = table_topic.take() {
                                            let _ = swarm.behaviour_mut().gossipsub.unsubscribe(&t);
                                        }
                                        let _ = events
                                            .send(NodeEvent::JoinRefused { reason })
                                            .await;
                                    }
                                    Err(e) => {
                                        table = None;
                                        table_closed = false;
                        tournament_started = false;
                        // **Per table, not per process.** Its own doc says
                        // *whether this table has ever dealt a hand*, and it was
                        // set true and never set back — so after one hand at one
                        // table, the next table in the same process refused every
                        // stranger (`on_join_request` reads it as `started`),
                        // never opened hand 1, never re-said a ratification,
                        // never released a silent seat and was never
                        // re-advertised. Five mechanisms, all by not running.
                        ever_dealt = false;
                        hand = None;
                        hand_reported = false;
                        deck_reported = None;
                        cards_reported = false;
                        abort_reported = false;
                        turn_reported = None;
                        said.clear();
                        next_hand_at = None;
                        act_by = None;
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
                        table = None;
                        // The Tox group goes with the table. Dropping the handle
                        // tells the driver to leave and joins its thread, which
                        // flushes what it still holds - the last message of a hand
                        // is exactly what sits in that queue.
                        tox_sink.clear();
                            tox_group_said = false;
                            table_announces = 0;
                        if let Some(t) = table_topic.take() {
                            let _ = swarm.behaviour_mut().gossipsub.unsubscribe(&t);
                        }
                        table_closed = false;
                        tournament_started = false;
                        // **Per table, not per process.** Its own doc says
                        // *whether this table has ever dealt a hand*, and it was
                        // set true and never set back — so after one hand at one
                        // table, the next table in the same process refused every
                        // stranger (`on_join_request` reads it as `started`),
                        // never opened hand 1, never re-said a ratification,
                        // never released a silent seat and was never
                        // re-advertised. Five mechanisms, all by not running.
                        ever_dealt = false;
                        hand = None;
                        hand_reported = false;
                        deck_reported = None;
                        cards_reported = false;
                        abort_reported = false;
                        turn_reported = None;
                        said.clear();
                        next_hand_at = None;
                        act_by = None;
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
                    )) if table_topic
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
                        if table.is_none() {
                            let _ = swarm.behaviour_mut().gossipsub
                                .report_message_validation_result(
                                    &message_id,
                                    &propagation_source,
                                    gossipsub::MessageAcceptance::Ignore,
                                );
                            continue;
                        }
                        // The hand first, if there is one. A `HAND_INIT`
                        // decodes as neither a player list nor a ratification,
                        // so without this it would fall through to the
                        // formation and be refused as malformed.
                        if let Some(h) = hand.as_mut() {
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
                            cross_boundary_at_t47(h, &mut boundaries, &mut crossed_for);
                            let verdict: Option<gossipsub::MessageAcceptance> =
                                match checkpoint_event(
                                    &message.data,
                                    h,
                                    &mut boundaries,
                                    &mut readmitted,
                                    &app_key,
                                    &mut checkpoint_said,
                                    &mut frozen,
                                    &roster_seats,
                                    &mut no_round_said,
                                    &mut early_checkpoints,
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
                                        &mut boundaries,
                                        &mut frozen,
                                        &mut disputes_seen,
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
                                    None if frozen.is_some() => {
                                        Some(gossipsub::MessageAcceptance::Ignore)
                                    }
                                    None => hand_event!(h, &message.data),
                                    Some(out) => {
                                        if !out.is_empty() {
                                            publish_and_hear(
                                                out,
                                                h,
                                                &mut boundaries,
                                                &mut readmitted,
                                                &app_key,
                                                &mut checkpoint_said,
                                                &mut frozen,
                                                &roster_seats,
                                                &mut no_round_said,
                                                &mut early_checkpoints,
                                                &profile_dir,
                                                &events,
                                                &mut swarm,
                                                &mut said,
                                                &tox_sink,
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
                        let Some(f) = table.as_mut() else { continue };
                        let now = super::node::now_unix_ms();
                        let result = if joinwire::receive_player_list(&message.data).is_ok() {
                            f.on_player_list(&message.data, now)
                        } else {
                            f.on_table_ready(&message.data)
                        };
                        let refused = result.is_err();
                        match result {
                            Ok(sends) => {
                                if let Some(t) = &table_topic {
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
                                seat_on_tox(f, &tox_sink);
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
                                    if !ever_dealt
                                        && hand_one_may_open(
                                            &tox_sink,
                                            &mut hand_one_held_since,
                                            &mut hand_one_progress,
                                            &mut hand_one_forced_said,
                                            &events,
                                        )
                                        .await
                                    {
                                        say_why_no_hand_one(f, &mut why_no_hand_one_said, &events).await;
                                        if let Some(o) = opening_for_hand_one(f) {
                                            ever_dealt = true;
                                            begin_hand(
                                                o,
                                                &app_key,
                                                &mut hand,
                                                                            &mut said,
                                                &mut swarm,
                                                &events,
                                                &tox_sink,
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
                        ..
                    } if budget < CONNECTION_CEILING => {
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
                    SwarmEvent::OutgoingConnectionError { error, .. } => {
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
                            if !peer_has_our_topics(&swarm, &peer_id, &topics, table_topic.as_ref())
                            {
                                let mut which: Vec<&gossipsub::IdentTopic> =
                                    vec![&topics.lobby, &topics.lobby_chat];
                                which.extend(table_topic.as_ref());
                                announce_topics(&mut swarm, &which);
                            }
                            let _ = events
                                .send(NodeEvent::PokerPeer { peer: peer_id, gone: false })
                                .await;
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
                                    let first = match dialled_lobby.get(&peer) {
                                        Some(at) if at.elapsed() < REDIAL_AFTER => continue,
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
                                if swarm.dial(opts).is_ok() {
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
                        gossipsub::Event::Subscribed { peer_id, topic: t },
                    )) => {
                        // Somebody new on this table's topic. GossipSub
                        // delivers to whoever is on a topic when a message is
                        // published and to nobody afterwards, so a roster
                        // announced a moment before they arrived was never sent
                        // to them at all. Say it again.
                        if let (Some(f), Some(mine)) = (table.as_ref(), table_topic.as_ref()) {
                            if t == mine.hash() {
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
                                    tox_sink.try_broadcast(&bytes);
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
                        if t == topics.lobby.hash() {
                            if let Some(f) = table.as_mut().filter(|f| f.is_founder()) {
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
                        if t == topics.lobby.hash() || Some(&t) == table_topic.as_ref().map(|x| x.hash()).as_ref() {
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
                let closed = table
                    .as_ref()
                    .is_some_and(|f| table_is_closed(f, &mut tournament_started));
                if closed != table_closed {
                    table_closed = closed;
                    dht_effort(&mut swarm, closed);
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
                let now = super::node::now_unix_ms();
                match command {
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
                        let ad = match tox_sink.start(
                            &profile_dir,
                            super::toxsink::Role::Host,
                            &ad.table_name,
                            &nickname,
                            Vec::new(),
                        ) {
                            Ok(Some(mine)) => {
                                match tox_sink
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
                                        tox_sink.clear();
                                        tox_group_said = false;
                                        table_announces = 0;
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
                                        table_topic = Some(topic);
                                        table = Some(f);
                                        let _ = events.send(NodeEvent::Hosting { key }).await;
                                        report_params(&events, table.as_ref().unwrap()).await;
                                        report_roster(&events, table.as_ref().unwrap()).await;
                        if let Some(f) = table.as_ref() { seat_on_tox(f, &tox_sink); }
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
                        if table.is_some() {
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
                                match tox_sink.start(
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
                                let topic = joinrpc::table_topic(&key);
                                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic);
                                table_topic = Some(topic);
                                table = Some(f);
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
                                swarm.behaviour_mut().join.send_request(&founder, request);
                            }
                            Err(e) => {
                                table_closed = false;
                        tournament_started = false;
                        // **Per table, not per process.** Its own doc says
                        // *whether this table has ever dealt a hand*, and it was
                        // set true and never set back — so after one hand at one
                        // table, the next table in the same process refused every
                        // stranger (`on_join_request` reads it as `started`),
                        // never opened hand 1, never re-said a ratification,
                        // never released a silent seat and was never
                        // re-advertised. Five mechanisms, all by not running.
                        ever_dealt = false;
                        hand = None;
                        hand_reported = false;
                        deck_reported = None;
                        cards_reported = false;
                        abort_reported = false;
                        turn_reported = None;
                        said.clear();
                        next_hand_at = None;
                        act_by = None;
                        dht_effort(&mut swarm, false);
                        let _ = events.send(NodeEvent::LeftTable {
                                    why: format!("cannot ask to join: {e:?}"),
                                }).await;
                            }
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
                        let Some(h) = hand.as_mut() else {
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
                                publish_hand(sends, &mut swarm, &mut said, &tox_sink);
                                let report =
                                    report_hand(h, &events, &mut turn_reported).await;
                                if let Some(end) = report.ended {
                                    next_hand_at =
                                        Some(tokio::time::Instant::now() + end.pause());
                                }
                                act_by = hurry(report.clock.apply(act_by, h.action_deadline()), autoplay);
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
                        // The Tox group goes with the table. Dropping the handle
                        // tells the driver to leave and joins its thread, which
                        // flushes what it still holds - the last message of a hand
                        // is exactly what sits in that queue.
                        tox_sink.clear();
                            tox_group_said = false;
                            table_announces = 0;
                        if let Some(t) = table_topic.take() {
                            let _ = swarm.behaviour_mut().gossipsub.unsubscribe(&t);
                        }
                        table = None;
                        table_closed = false;
                        tournament_started = false;
                        // **Per table, not per process.** Its own doc says
                        // *whether this table has ever dealt a hand*, and it was
                        // set true and never set back — so after one hand at one
                        // table, the next table in the same process refused every
                        // stranger (`on_join_request` reads it as `started`),
                        // never opened hand 1, never re-said a ratification,
                        // never released a silent seat and was never
                        // re-advertised. Five mechanisms, all by not running.
                        ever_dealt = false;
                        hand = None;
                        hand_reported = false;
                        deck_reported = None;
                        cards_reported = false;
                        abort_reported = false;
                        turn_reported = None;
                        said.clear();
                        next_hand_at = None;
                        act_by = None;
                        dht_effort(&mut swarm, false);
                        let _ = events.send(NodeEvent::LeftTable {
                            why: "left the table".into(),
                        }).await;
                    }
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
            Some(item) = tox_sink.next() => {
                // **Learn who this group peer is, once, from a signature.**
                // The driver reports a sender by its group key and cannot get
                // further; the roster is keyed by application key. Pairing them
                // here — where the event is opened anyway — is what makes
                // D-019's removal able to find its target at all (`S1-I`), and
                // it costs one open per peer per run.
                if let (Some(gk), Some(h)) = (item.claimed, hand.as_ref()) {
                    if !taught.contains(&gk) {
                        if let Some(app) = signer_of(&item.bytes, h) {
                            taught.insert(gk);
                            tox_sink.tell(super::toxsink::Seat::KnownAs {
                                group_key: gk,
                                app_key: app,
                            });
                        }
                    }
                }
                // Nothing arrives while the link is down.
                if link_is_down() {
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
                if hand.is_none() {
                    if let Some(f) = table.as_mut() {
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
                                    if !tox_sink.try_broadcast(&bytes) {
                                        if let Some(t) = &table_topic {
                                            let _ = swarm
                                                .behaviour_mut()
                                                .gossipsub
                                                .publish(t.clone(), bytes);
                                        }
                                    }
                                }
                            }
                            report_roster(&events, f).await;
                            seat_on_tox(f, &tox_sink);
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
                if let Some(h) = hand.as_mut() {
                    cross_boundary_at_t47(h, &mut boundaries, &mut crossed_for);
                    match checkpoint_event(
                        &item.bytes,
                        h,
                        &mut boundaries,
                        &mut readmitted,
                        &app_key,
                        &mut checkpoint_said,
                        &mut frozen,
                        &roster_seats,
                        &mut no_round_said,
                        &mut early_checkpoints,
                        &profile_dir,
                        &events,
                    )
                    .await
                    {
                        // Not a checkpoint event: a dispute, or the hand's
                        // own — and the hand's own is not applied while §6.3
                        // step 1's freeze is latched.
                        None => {
                            if !dispute_event(
                                &item.bytes,
                                h,
                                &mut boundaries,
                                &mut frozen,
                                &mut disputes_seen,
                                &events,
                            )
                            .await
                                && frozen.is_none()
                            {
                                let _ = hand_event!(h, &item.bytes);
                            }
                        }
                        // Taken, and this peer's own `STATE_ACK` is due.
                        Some(out) if !out.is_empty() => {
                            publish_and_hear(
                                out,
                                h,
                                &mut boundaries,
                                &mut readmitted,
                                &app_key,
                                &mut checkpoint_said,
                                &mut frozen,
                                &roster_seats,
                                &mut no_round_said,
                                &mut early_checkpoints,
                                &profile_dir,
                                &events,
                                &mut swarm,
                                &mut said,
                                &tox_sink,
                            )
                            .await;
                        }
                        Some(_) => {}
                    }
                }
            }

            _ = resend.tick() => {
                if !tox_group_said {
                    if let Some(chat) = tox_sink.chat_id() {
                        tox_group_said = true;
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
                let dropped = tox_sink.inbox_dropped();
                if dropped > inbox_dropped_said {
                    inbox_dropped_said = dropped;
                    let _ = events
                        .send(NodeEvent::Warning(format!(
                            "{dropped} hand event(s) were received and dropped because this                              client was not draining; a seat that loses one diverges"
                        )))
                        .await;
                }

                let (refused, waiting, sent) = tox_sink.trouble();
                if waiting > 0 || refused > tox_refused_said {
                    tox_refused_said = refused;
                    // Which refusal, because the remedies have nothing in
                    // common: code 4 is this client's own group connection
                    // being down, decided before any peer is consulted, while
                    // code 5 comes from the peer loop.
                    let why = tox_sink.refused_why();
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
                let invites = tox_sink.invites_refused();
                if invites > tox_invites_said {
                    tox_invites_said = invites;
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
                if let (Some(t), true, true) =
                    (table_topic.as_ref(), table.is_some(), hand.is_none())
                {
                    let hash = t.hash();
                    let missing = poker_peers.iter().any(|p| {
                        !swarm.behaviour().gossipsub.all_peers().any(|(q, subs)| {
                            q == p && subs.contains(&&hash)
                        })
                    });
                    if missing && table_announces < MAX_TABLE_ANNOUNCES {
                        table_announces += 1;
                        announce_topics(&mut swarm, &[t]);
                    }
                }

                // **The third road into hand 1, and the gate needs it.** The
                // other two fire on a table message, and a table whose roster
                // has ratified may send none at all while it waits for the Tox
                // group to fill. Without this the wait could only end when
                // something else happened to arrive, which on a quiet table is
                // never. Idempotent for the same reason the other two are:
                // `ever_dealt`.
                if !ever_dealt {
                    if let Some(f) = table.as_ref() {
                        if hand_one_may_open(
                            &tox_sink,
                            &mut hand_one_held_since,
                            &mut hand_one_progress,
                            &mut hand_one_forced_said,
                            &events,
                        )
                        .await
                        {
                            say_why_no_hand_one(f, &mut why_no_hand_one_said, &events).await;
                            if let Some(o) = opening_for_hand_one(f) {
                                ever_dealt = true;
                                begin_hand(
                                    o,
                                    &app_key,
                                    &mut hand,
                                    &mut said,
                                    &mut swarm,
                                    &events,
                                    &tox_sink,
                                )
                                .await;
                            }
                        }
                    }
                }

                let Some(h) = hand.as_ref() else { continue };
                // A hand that is over is a hand nobody is waiting on.
                if h.over() || said.is_empty() {
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
                //   and the code is left alone: `said` is cleared between hands
                //   (see the `HAND_COMPLETE` note below), so what an advancing
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
                if here != resend_at {
                    resend_at = here;
                    resend_ticks = 0;
                }
                resend_ticks = resend_ticks.saturating_add(1);
                // 1, 2, 4, 8, ... ticks: a power of two and nothing between.
                let due = resend_ticks.is_power_of_two();
                if due {
                    let window = here.saturating_sub(RESEND_STAGES);
                    let recent: Vec<&Vec<u8>> = said
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
                    if tox_sink.is_on_tox() {
                        for out in recent {
                            tox_sink.try_broadcast(out);
                        }
                    }
                }
            }

            // A stage that nobody is completing. Polled rather than armed:
            // the answer changes every time a stage opens or closes, and the
            // cryptographic ones are bounded per stage rather than per hand.
            _ = stall.tick() => {
                let now = super::node::now_unix_ms();
                let Some(h) = hand.as_mut() else { continue };

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
                publish_hand(replayed, &mut swarm, &mut said, &tox_sink);
                let Some(h) = hand.as_mut() else { continue };

                // Say so first, if this client's own timer has run out on
                // somebody. A vote is not an accusation and does nothing
                // alone; only a complete set becomes a certificate, and only a
                // certificate moves anything. Where there are three seats or
                // more this is the answer, and the abort below is what happens
                // when it is not available — heads-up, where "unanimity" would
                // be the one opponent.
                match h.vote_on_timeouts(&app_key, now) {
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
                        if let Some(n) = h.take_cert_note() {
                            let _ = events.send(NodeEvent::Warning(n)).await;
                        }
                        publish_hand(sends, &mut swarm, &mut said, &tox_sink);
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

                let Some(h) = hand.as_mut() else { continue };
                if !h.may_abandon(now) {
                    continue;
                }
                act_by = None;
                // Which clock, taken before the abort resets the phase. Two
                // budgets can produce this abort and they send a reader to
                // completely different places -- see `Hand::expired_budget`.
                let budget = h.expired_budget(now);
                match h.abort_now(crate::table::hand::Abort::Deadline, &app_key, now) {
                    Ok(sends) => {
                        publish_hand(sends, &mut swarm, &mut said, &tox_sink);
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
                            && !tox_sink.is_on_tox()
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
                                "the hand ran out of time{which}; every stack is                                  restored{why}"
                            )))
                            .await;
                        // Straight on: an abort has nothing to look at, so
                        // D-020's hold has nothing to hold.
                        next_hand_at = Some(
                            tokio::time::Instant::now() + std::time::Duration::from_millis(800),
                        );
                    }
                    Err(e) => {
                        let _ = events
                            .send(NodeEvent::Warning(format!("the hand could not be ended: {e}")))
                            .await;
                    }
                }
            }

            // This client's own clock ran out on its own turn.
            () = async {
                match act_by {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            }, if act_by.is_some() => {
                act_by = None;
                let Some(h) = hand.as_mut() else { continue };
                let Some(turn) = h.turn().filter(|t| t.mine) else { continue };
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
                        publish_hand(sends, &mut swarm, &mut said, &tox_sink);
                        let _ = events
                            .send(NodeEvent::Warning(if autoplay.is_some() {
                                format!("autoplay: {action:?}")
                            } else {
                                format!("your clock ran out — {action:?} for you")
                            }))
                            .await;
                        let report = report_hand(h, &events, &mut turn_reported).await;
                        if let Some(end) = report.ended {
                            next_hand_at = Some(tokio::time::Instant::now() + end.pause());
                        }
                        act_by = hurry(report.clock.apply(act_by, h.action_deadline()), autoplay);
                    }
                    Err(e) => {
                        let _ = events
                            .send(NodeEvent::Warning(format!("the clock's own action: {e}")))
                            .await;
                    }
                }
            }

            // D-020's hold has run out: deal the next hand.
            () = async {
                match next_hand_at {
                    Some(at) => tokio::time::sleep_until(at).await,
                    // Nothing pending. `pending()` never completes, so this arm
                    // is simply not in the race — which is what an `Option`
                    // timer means in a `select!`.
                    None => std::future::pending().await,
                }
            }, if next_hand_at.is_some() => {
                next_hand_at = None;
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
                if frozen.is_none() {
                    frozen_said = false;
                }
                if let Some((k, _)) = frozen {
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
                    next_hand_at =
                        Some(tokio::time::Instant::now() + FROZEN_RECHECK);
                    if !frozen_said {
                        frozen_said = true;
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "no further hand is dealt: this peer is frozen at hand {k}'s checkpoint and section 6.3 releases that by a reconciliation round alone. {disputes_seen} dispute(s) verified from other seats; W is {:?}",
                                boundaries.contradicted(k)
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
                if let Some(h) = hand.as_ref() {
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
                        roster_seats = table
                            .as_ref()
                            .map(|f| f.roster().seats().iter().map(|e| e.seat).collect())
                            .unwrap_or_default();
                        let roster = roster_seats.clone();
                        if boundaries
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
                        let waiting = early_checkpoints.remove(&h.hand_id()).unwrap_or_default();
                        // Anything older than the boundary now opening is for a
                        // hand the store has released. Dropped here rather than
                        // left to grow.
                        early_checkpoints.retain(|k, _| *k > h.hand_id());
                        for held in waiting {
                            if let Some(next) = checkpoint_event(
                                &held,
                                h,
                                &mut boundaries,
                                &mut readmitted,
                                &app_key,
                                &mut checkpoint_said,
                                &mut frozen,
                                &roster_seats,
                                &mut no_round_said,
                                &mut early_checkpoints,
                                &profile_dir,
                                &events,
                            )
                            .await
                            {
                                if !next.is_empty() {
                                    publish_and_hear(
                                        next,
                                        h,
                                        &mut boundaries,
                                        &mut readmitted,
                                        &app_key,
                                        &mut checkpoint_said,
                                        &mut frozen,
                                        &roster_seats,
                                        &mut no_round_said,
                                        &mut early_checkpoints,
                                        &profile_dir,
                                        &events,
                                        &mut swarm,
                                        &mut said,
                                        &tox_sink,
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
                                boundaries.remember_own_event(h.hand_id(), &bytes);
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
                                    &mut boundaries,
                                    &mut readmitted,
                                    &app_key,
                                    &mut checkpoint_said,
                                    &mut frozen,
                                    &roster_seats,
                                    &mut no_round_said,
                                    &mut early_checkpoints,
                                    &profile_dir,
                                    &events,
                                    &mut swarm,
                                    &mut said,
                                    &tox_sink,
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
                if let Some((theirs, mine)) = adrift {
                    if !adrift_said {
                        adrift_said = true;
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "this client is out: the table is at hand {theirs} and this client reached only {mine}, so it has been dealing a hand nobody else has. No further hand is dealt here — but hand {mine}'s checkpoint has gone out, so the others are not held up by this."
                            )))
                            .await;
                    }
                    continue;
                }

                let next = hand.as_ref().and_then(|h| h.next_hand());
                if let (Some(h), Some(o)) = (hand.as_ref(), next.as_ref()) {
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
                hand = None;
                hand_reported = false;
                deck_reported = None;
                cards_reported = false;
                turn_reported = None;
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
                    for b in said.drain(..) {
                        match crate::net::chained::peek(&b, TABLE_FRAME_PEEK) {
                            Ok((EventType::HandComplete | EventType::HandAbort, _, _)) => {
                                terminal = Some(b)
                            }
                            Ok((EventType::StateHash | EventType::StateAck, _, _)) => {
                                checkpoint.push(b)
                            }
                            _ => {}
                        }
                    }
                    said.extend(terminal);
                    said.extend(checkpoint);
                }
                match next {
                    Some(mut opening) => {
                        // **Read here and cleared here, which is the whole of
                        // `A`'s life** (§4.9). A set read anywhere else is a set
                        // two peers can come to disagree about.
                        opening.readmitted = std::mem::take(&mut readmitted);
                        if !opening.readmitted.is_empty() {
                            let _ = events
                                .send(NodeEvent::Warning(format!(
                                    "seat(s) {:?} are heard again at this hand, and dealt in at the next",
                                    opening.readmitted
                                )))
                                .await;
                        }
                        begin_hand(
                            opening,
                            &app_key,
                            &mut hand,
                                    &mut said,
                            &mut swarm,
                            &events,
                            &tox_sink,
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
                    }
                }
            }

            _ = housekeeping.tick() => {
                let now = super::node::now_unix_ms();
                state.tick(now);

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
                if let Some(f) = table.as_ref() {
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
                        let (sent, refused, up) = tox_sink.invite_counts();
                        let (rejoins, join_fails, confirmed, founder_link) = tox_sink.join_trouble();
                        let (seen, want) = tox_sink.group_seen();
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "seats on the line: {}; tox self {}, group {seen} seen/{confirmed} confirmed/{want} wanted, tox friends up {up}, invites {sent} sent {refused} refused{}{}{}{}",
                                line.join(", "),
                                match tox_sink.tox_connection() {
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
                                } else if let Some(b) = boundaries.newest() {
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
                                match hand.as_ref() {
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
                    if !udp_warned && {
                        let (seen, want) = tox_sink.group_seen();
                        want > 0 && seen < want
                    } {
                        udp_warned = true;
                        let r = tox_sink.reach();
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "the table's group is not filling. Tox is {}; of {} known nodes, {} bootstrapped and {} TCP relays were accepted{}. Game traffic rides that group, so nothing can be dealt until it fills.",
                                match tox_sink.tox_connection() {
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
                    if !ever_dealt {
                        if let Some(topic) = table_topic.as_ref() {
                            for bytes in f.say_again(super::node::now_unix_ms()) {
                                // The same second path as the `Subscribed`
                                // repeat above, for the same reason (`S1-P`).
                                tox_sink.try_broadcast(&bytes);
                                let _ = swarm
                                    .behaviour_mut()
                                    .gossipsub
                                    .publish(topic.clone(), bytes);
                            }
                        }
                    }
                }

                // **Give back the seat of anybody who has stopped answering,
                // and only before the first hand.**
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
                if !ever_dealt {
                    if let Some(f) = table.as_mut().filter(|f| f.is_founder()) {
                        let silent: Vec<(u8, Vec<u8>, Option<[u8; 32]>)> = f
                            .roster()
                            .seats()
                            .iter()
                            .filter(|e| Some(e.seat) != f.my_seat())
                            .filter(|e| {
                                match PeerId::from_bytes(&e.peer_id) {
                                    // Answered, and recently enough.
                                    Ok(p) => alive
                                        .get(&p)
                                        .map(|(at, _)| {
                                            at.elapsed()
                                                >= std::time::Duration::from_millis(SEAT_SILENCE_MS)
                                        })
                                        // Seated and never once heard from. It
                                        // reached the join RPC over a connection,
                                        // so `ConnectionEstablished` recorded it;
                                        // no entry at all means that connection
                                        // and this client's memory of it are both
                                        // gone.
                                        .unwrap_or(true),
                                    // A `peer_id` that will not parse cannot be
                                    // pinged and cannot be judged. Left alone:
                                    // §4.3 admitted it, and refusing to seat it
                                    // is that check's business rather than this
                                    // one's.
                                    Err(_) => false,
                                }
                            })
                            // The Tox key travels with the seat, because it is
                            // read from the roster **before** the release takes
                            // the entry out of it. D-019's kick needs it and it
                            // is gone a line later.
                            .map(|e| (e.seat, e.peer_id.clone(), e.tox_key))
                            .collect();

                        for (seat, peer, tox_key) in silent {
                            match f.release_seat_before_the_first_hand(&peer, now) {
                                Ok(sends) if !sends.is_empty() => {
                                    let _ = events
                                        .send(NodeEvent::Warning(format!(
                                            "seat {seat} has answered nothing for {} s and the seat is free again",
                                            SEAT_SILENCE_MS / 1000
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
                                        tox_sink.tell(super::toxsink::Seat::Left(k));
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
                                                if let Some(tt) = &table_topic {
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
                                    seat_on_tox(f, &tox_sink);
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
                    if connected.is_empty() && !alone_said {
                        alone_said = true;
                        let _ = events
                            .send(NodeEvent::Warning(
                                "no other poker client has been reached yet — a table hosted now is one nobody can see. On one machine that is local discovery failing; across networks it is the relay."
                                    .into(),
                            ))
                            .await;
                    }
                    if !connected.is_empty() {
                        alone_said = false;
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
                if let Some(f) = table
                    .as_mut()
                    .filter(|f| f.is_founder() && !ever_dealt)
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
    for e in f.roster().seats() {
        if let Some(k) = e.tox_key {
            tox.tell(super::toxsink::Seat::Took(k));
        }
    }
}

async fn report_roster(events: &Events, f: &Formation) {
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
    let (action, grace, crypto, delay) = (20_000u32, 5_000u32, 30_000u32, 7_000u32);
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
            crate::protocol::constants::default_time_bank_ms(seats) as u64,
        ) as u32,
        time_bank_ms: crate::protocol::constants::default_time_bank_ms(seats),
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
    // No topic. A hand is Tox-only by instruction and by capacity, and the way
    // that rule is kept is by not handing the swarm's topic to anything on the
    // hand path -- see `publish_hand`.
    events: &Events,
    tox: &super::toxsink::TableSink,
) {
    use crate::table::hand::Hand;

    if hand.is_some() {
        return;
    }
    let now = super::node::now_unix_ms();
    let deadline = opening.crypto_step_timeout_ms;
    match Hand::open(opening, app_key, now, deadline) {
        Ok((h, sends)) => {
            // Through the same door as every other hand message, so that
            // `HAND_INIT` is remembered and re-sent like the rest. It is in
            // fact the one that goes missing: it is published the moment the
            // roster ratifies, which is before the last joiner has been
            // grafted into anybody's mesh for this topic.
            publish_hand(sends, swarm, said, tox);
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
/// **What it simulates, exactly, and the limit is the point of it.** The
/// application's messages stop in both directions while the process, the `Hand`,
/// the `Opening`, the chain position, the keys and the transport all survive —
/// which is what a brief outage does to a client and is emphatically *not* what
/// killing the process does. libp2p's own pings keep answering, deliberately:
/// that separates **did the hand recover** from **did the transport reconnect**,
/// and the second question already has an instrument (`-DropAt`) while the first
/// has never had one.
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
    let down = link_is_down();
    for crate::table::hand::Send::Broadcast(out) in sends {
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
/// **Two seats, because one is not evidence.** Each `Holding::AnotherHand`
/// carries a signature this hand has already verified against this table, so a
/// roster seat naming a hand this client has not reached is that seat's own word
/// that it is elsewhere. One seat could be mistaken or hostile; two independent
/// ones cannot both be, and it is the same floor §4.9 puts on a reconciliation
/// round for the same reason.
///
/// **Terminal, deliberately.** Nothing here can catch up: the openings of the
/// hands in between were derived from settlements this client never saw, and no
/// message carries one (`S1-Q`, closed by scope). Latching is therefore honest
/// where a retry would not be.
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
    if let Some(out) = adrift_now(mine, ahead) {
        *adrift = Some(out);
    }
}

/// The decision alone, so a test can reach it.
///
/// `note_a_hand_ahead` needs a whole `Hand` and a live table to be called; this
/// is the part that decides, and a test that had to rebuild the caller would
/// end up restating the rule instead of checking it.
fn adrift_now(mine: u64, ahead: &std::collections::HashMap<u8, u64>) -> Option<(u64, u64)> {
    let saying: Vec<u64> = ahead.values().copied().filter(|k| *k > mine).collect();
    let furthest = saying.iter().copied().max()?;
    // **Two seats, and two hands.** One seat running ahead is that peer's
    // problem; one hand of margin is what a table looks like while somebody is
    // still inside `Ended::pause`.
    (saying.len() >= 2 && furthest >= mine + ADRIFT_MARGIN).then_some((furthest, mine))
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

#[allow(clippy::too_many_arguments)]
async fn checkpoint_event(
    bytes: &[u8],
    h: &crate::table::hand::Hand,
    boundaries: &mut crate::table::boundary::Boundaries,
    readmitted: &mut Vec<u8>,
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

    let took = match half {
        checkwire::Kind::Hash => {
            let Ok(body) = crate::net::chained::payload::<checkwire::StateHash>(&opened, 512)
            else {
                return Some(Vec::new());
            };
            boundaries.on_state_hash(hand_id, seat, opened.event_hash, body.state_hash)
        }
        checkwire::Kind::Ack => {
            let Ok(body) = crate::net::chained::payload::<checkwire::StateAck>(&opened, 512) else {
                return Some(Vec::new());
            };
            boundaries.on_state_ack(hand_id, seat, opened.event_hash, body.checkpoint_hash)
        }
    };

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
async fn report_hand(
    h: &crate::table::hand::Hand,
    events: &Events,
    last: &mut Option<(Option<u8>, u64, u64, bool)>,
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
    );
    if *last == Some(now) {
        // Nothing new. In particular the clock is **not** re-armed: a mesh
        // redelivers, and a duplicate that reset the deadline every time would
        // be a clock that never runs out.
        return Report::default();
    }
    let board_changed = last.map(|(_, _, b, _)| b) != Some(now.2);
    *last = Some(now);

    if board_changed {
        let _ = events
            .send(NodeEvent::Board {
                hand_id,
                cards: h.board().iter().map(|c| c.index()).collect(),
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
                })
                .await;
            return Report {
                ended: None,
                clock: Clock::Start,
            };
        }
        Some(t) => {
            let _ = events
                .send(NodeEvent::NotYourTurn {
                    hand_id,
                    seat: Some(t.seat),
                })
                .await;
            return Report {
                ended: None,
                clock: Clock::Stop,
            };
        }
        None => {
            let _ = events
                .send(NodeEvent::NotYourTurn { hand_id, seat: None })
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

/// What a report says about the clock.
///
/// Three answers and not two, because "nothing changed" must not be confused
/// with "stop": a redelivered message reports nothing, and a clock that was
/// restarted on every redelivery would never run out.
#[derive(Default, PartialEq, Eq, Clone, Copy)]
enum Clock {
    /// It is newly this client's turn: start counting.
    Start,
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
            Clock::Start => Some(tokio::time::Instant::now() + timeout),
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

        // Three seats, one hand ahead: the showdown-pause skew, and the shape of
        // thirty-six of the thirty-seven real evictions.
        assert_eq!(
            adrift_now(mine, &at(&[(1, mine + 1), (2, mine + 1), (3, mine + 1)])),
            None,
            "three seats one hand ahead is a pause, not a divergence"
        );

        // Two hands is past any pause.
        assert_eq!(
            adrift_now(mine, &at(&[(1, mine + 2), (2, mine + 2)])),
            Some((mine + 2, mine)),
            "two hands behind is adrift"
        );

        // The one true positive on record: five hands, run212350-4.
        assert_eq!(
            adrift_now(6, &at(&[(1, 11), (2, 11), (3, 10)])),
            Some((11, 6)),
            "the case this mechanism exists for still fires"
        );

        // One seat is never the table, whatever the margin.
        assert_eq!(adrift_now(mine, &at(&[(1, mine + 9)])), None, "one seat is not the table");

        // And a table nobody is ahead of says nothing.
        assert_eq!(adrift_now(mine, &at(&[(1, mine), (2, mine - 1)])), None);
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
