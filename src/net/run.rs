//! The running node: the async loop that makes D-003 true.
//!
//! Everything else in `net` is a decision made in isolation and tested in
//! isolation. This is the part that has to be run to be believed, so it is kept
//! small and its pieces live elsewhere: the admission rules are
//! [`lobby`](super::lobby)'s, the per-node state is [`node`](super::node)'s, and
//! the relay budget is [`relay`](super::relay)'s. What is here is the order
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
    futures::StreamExt as SwarmStreamExt, gossipsub, identity, request_response, swarm::SwarmEvent,
    Multiaddr, PeerId,
};
use tokio::sync::mpsc;

use std::collections::HashSet;

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

/// How many players from the lobby to reach for in one discovery cycle.
///
/// The connection budget is finite and a lobby key outlives the clients in it,
/// so dialling every provider at once fills the budget with the dead and leaves
/// none for the living. Whoever is not reached this cycle is reached the next.
const DIALS_PER_CYCLE: usize = 8;

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

pub async fn run(
    identity: identity::Keypair,
    app_key: ed25519_dalek::SigningKey,
    events: mpsc::Sender<NodeEvent>,
    mut commands: mpsc::Receiver<NodeCommand>,
    local_discovery: bool,
    port: u16,
    profile_dir: std::path::PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
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
    let mut table_topic: Option<gossipsub::IdentTopic> = None;

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

    // Lobby providers already tried, so a record for a client that is long gone
    // is dialled once rather than every minute for ever.
    let mut dialled_lobby: std::collections::HashSet<libp2p::PeerId> =
        std::collections::HashSet::new();

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
    let mut asked_hops: HashSet<libp2p::PeerId> = HashSet::new();

    // Whether the public DHT has been asked who the relays are. Once is enough
    // to start: the answer arrives as providers, and each of those is then
    // dialled and asked for a reservation on its own account.
    let mut asked_public_dht = false;

    // Whether this client's own record is in the public lobby. Set once the
    // announcement has an address in it, because one without is discarded.
    let mut in_public_lobby = false;

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
    // so two lost messages do not empty a pane that should not be empty.
    let mut presence_timer = tokio::time::interval(Duration::from_millis(
        super::lobbytalk::PRESENCE_EVERY_MS,
    ));
    let mut housekeeping = tokio::time::interval(REBROADCAST);

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
                        let _ = events.send(NodeEvent::PeerConnected(peer_id)).await;
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
                                match f.on_join_request(&request, &authenticated, now) {
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
                                            if !ever_dealt {
                                                if let Some(o) = opening_for_hand_one(f) {
                                                    ever_dealt = true;
                                                    begin_hand(
                                                        o,
                                                        &app_key,
                                                        &mut hand,
                                                                                            &mut said,
                                                        &mut swarm,
                                                        table_topic.as_ref(),
                                                        &events,
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
                                    }
                                    Err(Failed::Refused { reason, .. }) => {
                                        table = None;
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
                        hand = None;
                        hand_reported = false;
                        deck_reported = None;
                        cards_reported = false;
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
                        if let Some(t) = table_topic.take() {
                            let _ = swarm.behaviour_mut().gossipsub.unsubscribe(&t);
                        }
                        table_closed = false;
                        tournament_started = false;
                        hand = None;
                        hand_reported = false;
                        deck_reported = None;
                        cards_reported = false;
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
                            use crate::table::hand::Failed;
                            let now = super::node::now_unix_ms();
                            // **One verdict, one place to report it.** Every arm
                            // of this match used to leave by its own `continue`,
                            // and with `validate_messages()` set an arm that
                            // forgot to report first made this node a black hole
                            // for its own table's traffic — which is exactly what
                            // happened, to all three of them at once, and cost a
                            // day: two survivors of a dropped peer sat a stage
                            // apart because the bytes that would have closed the
                            // gap were at a neighbour that would not relay them.
                            //
                            // A missing report is not a mistake that can be
                            // tested for cheaply — it needs three real nodes with
                            // two of them unmeshed — so the shape is what
                            // prevents it. `None` is the one deliberate way out
                            // and it means *this was not a hand event*.
                            let verdict: Option<gossipsub::MessageAcceptance> =
                                match h.on_event(&message.data, &app_key, now) {
                                Ok(sends) => {
                                    // Anything held for a stage this client had
                                    // not reached is judged again now, because
                                    // this event may have been the one that
                                    // reached it. Without this a peer that ran
                                    // ahead is parked for ever: the mesh does
                                    // not re-send, and a hand of 2m+4 stages
                                    // hears out-of-order messages as a matter
                                    // of course rather than as an exception.
                                    let (mut more, held_failures) = h.replay_early(&app_key, now);
                                    let mut sends = sends;
                                    sends.append(&mut more);
                                    for e in held_failures {
                                        let _ = events
                                            .send(NodeEvent::Warning(format!(
                                                "a held event was refused: {e}"
                                            )))
                                            .await;
                                    }
                                    publish_hand(
                                        sends,
                                        table_topic.as_ref(),
                                        &mut swarm,
                                        &mut said,
                                    );
                                    // Outside `dealt()`: a certificate is
                                    // decided at cryptographic stages too, and
                                    // gating the report on cards being out
                                    // delayed every note until some later
                                    // event happened to take another path.
                                    if let Some(n) = h.take_cert_note() {
                                        let _ = events.send(NodeEvent::Warning(n)).await;
                                    }
                                    // The prover's side of a shuffle context.
                                    // It used to be emitted only on success and
                                    // so never reached the error arm below; a
                                    // refused proof now ends the hand with a
                                    // `cause = 2` abort, which is an `Ok` with
                                    // sends, so the refusal's own note arrives
                                    // here too.
                                    if let Some(n) = h.take_shuffle_note() {
                                        let _ = events.send(NodeEvent::Warning(n)).await;
                                    }
                                    // The one condition this client cannot
                                    // repair and must not hide.
                                    if let Some(f) = h.take_fork() {
                                        let _ = events
                                            .send(NodeEvent::Warning(format!(
                                                "this hand has forked: {f}"
                                            )))
                                            .await;
                                    }
                                    if h.dealt() {
                                        if !hand_reported {
                                            hand_reported = true;
                                            let init = h.init();
                                            let _ = events
                                                .send(NodeEvent::HandBegan {
                                                    hand_id: init.hand_id,
                                                    button: init.button_position,
                                                    dealt_in: init.dealt_in.clone(),
                                                })
                                                .await;
                                        }
                                        let deck = (h.shuffler(), h.shuffled());
                                        if deck_reported != Some(deck) {
                                            deck_reported = Some(deck);
                                            let _ = events
                                                .send(NodeEvent::DeckProgress {
                                                    hand_id: h.hand_id(),
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
                                        if let Some((subject, held, need, d)) = h.take_tally() {
                                            let _ = events
                                                .send(NodeEvent::Warning(format!(
                                                    "seat {subject} @{}: {held}/{need} agree",
                                                    short_hash(&d)
                                                )))
                                                .await;
                                        }
                                        // A seat the table acted for, once.
                                        if let Some((seat, what)) = h.take_certified_action() {
                                            let _ = events
                                                .send(NodeEvent::Warning(format!(
                                                    "the table acted for seat {seat}: {what:?}"
                                                )))
                                                .await;
                                        }
                                        if let Some(why) = h.aborted() {
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
                                            report_hand(h, &events, &mut turn_reported).await;
                                        if let Some(end) = report.ended {
                                            next_hand_at = Some(
                                                tokio::time::Instant::now() + end.pause(),
                                            );
                                        }
                                        act_by = report.clock.apply(act_by, h.action_deadline());
                                        if let Some(cards) = h.cards().filter(|_| !cards_reported) {
                                            cards_reported = true;
                                            let _ = events
                                                .send(NodeEvent::CardsDealt {
                                                    hand_id: h.hand_id(),
                                                    seats: h.init().dealt_in.clone(),
                                                })
                                                .await;
                                            let _ = events
                                                .send(NodeEvent::HoleCards {
                                                    hand_id: h.hand_id(),
                                                    cards: [cards[0].index(), cards[1].index()],
                                                })
                                                .await;
                                        }
                                    } else {
                                        let _ = events
                                            .send(NodeEvent::HandWaiting {
                                                hand_id: h.hand_id(),
                                                seats: h.waiting_for(),
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
                                    if let Some(n) = h.take_cert_note() {
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
                                    Some(match h.hold(message.data.clone()) {
                                        Holding::Kept => gossipsub::MessageAcceptance::Accept,
                                        // Somebody else's hand, and verified:
                                        // the table identity and the signature
                                        // held, only the `hand_id` is not this
                                        // client's. Relayed, because a peer one
                                        // hand behind is exactly the
                                        // intermediary a peer one hand ahead
                                        // needs — it is simply not kept.
                                        Holding::AnotherHand => {
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
                                    if let Some(n) = h.take_shuffle_note() {
                                        let _ = events
                                            .send(NodeEvent::Warning(n))
                                            .await;
                                    }
                                    if let Some(n) = h.take_settle_note() {
                                        let _ = events
                                            .send(NodeEvent::Warning(n))
                                            .await;
                                    }
                                    if let Some(n) = h.take_cert_note() {
                                        let _ = events
                                            .send(NodeEvent::Warning(n))
                                            .await;
                                    }
                                    let _ = events
                                        .send(NodeEvent::Warning(format!("hand: {e}")))
                                        .await;
                                    Some(gossipsub::MessageAcceptance::Reject)
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
                                    if !ever_dealt {
                                        if let Some(o) = opening_for_hand_one(f) {
                                            ever_dealt = true;
                                            begin_hand(
                                                o,
                                                &app_key,
                                                &mut hand,
                                                                            &mut said,
                                                &mut swarm,
                                                table_topic.as_ref(),
                                                &events,
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
                        if info.protocol_version == super::swarm::PROTOCOL_VERSION
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
                            let lobby_hash = topics.lobby.hash();
                            let already = swarm
                                .behaviour()
                                .gossipsub
                                .all_peers()
                                .any(|(p, subscribed)| {
                                    *p == peer_id && subscribed.contains(&&lobby_hash)
                                });
                            if !already {
                                for t in [&topics.lobby, &topics.lobby_chat] {
                                    let g = &mut swarm.behaviour_mut().gossipsub;
                                    let _ = g.unsubscribe(t);
                                    let _ = g.subscribe(t);
                                }
                            }
                            let _ = events
                                .send(NodeEvent::PokerPeer { peer: peer_id, gone: false })
                                .await;
                        }

                        let relays = info.protocols.contains(&libp2p::relay::HOP_PROTOCOL_NAME);
                        seen_a_relay |= relays;
                        if relays && !have_reservation && !asked_hops.contains(&peer_id) {
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
                                asked_hops.insert(peer_id);
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
                            let mut fresh = 0usize;
                            for peer in providers {
                                if lobby {
                                    let _ = events.send(NodeEvent::LobbyPeer(peer)).await;
                                    if !dialled_lobby.insert(peer) {
                                        continue;
                                    }
                                    if dialled_lobby.len() > 512 {
                                        dialled_lobby.clear();
                                    }
                                    fresh += 1;
                                    if fresh > DIALS_PER_CYCLE {
                                        continue;
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
                                let _ = swarm.dial(opts);
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
                            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer);
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
                                for bytes in f.say_again() {
                                    let _ = swarm
                                        .behaviour_mut()
                                        .gossipsub
                                        .publish(mine.clone(), bytes);
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
                    if reachable_here && !in_public_lobby {
                        match swarm.behaviour_mut().ipfs_kad.start_providing(lobby_namespace())
                        {
                            Ok(_) => {
                                in_public_lobby = true;
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
                        ) {
                            Ok((f, request)) => {
                                let topic = joinrpc::table_topic(&key);
                                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic);
                                table_topic = Some(topic);
                                table = Some(f);
                                swarm.behaviour_mut().join.send_request(&founder, request);
                            }
                            Err(e) => {
                                table_closed = false;
                        tournament_started = false;
                        hand = None;
                        hand_reported = false;
                        deck_reported = None;
                        cards_reported = false;
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
                                publish_hand(
                                    sends,
                                    table_topic.as_ref(),
                                    &mut swarm,
                                    &mut said,
                                );
                                let report =
                                    report_hand(h, &events, &mut turn_reported).await;
                                if let Some(end) = report.ended {
                                    next_hand_at =
                                        Some(tokio::time::Instant::now() + end.pause());
                                }
                                act_by = report.clock.apply(act_by, h.action_deadline());
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
                        if let Some(t) = table_topic.take() {
                            let _ = swarm.behaviour_mut().gossipsub.unsubscribe(&t);
                        }
                        table = None;
                        table_closed = false;
                        tournament_started = false;
                        hand = None;
                        hand_reported = false;
                        deck_reported = None;
                        cards_reported = false;
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
            _ = resend.tick() => {
                let (Some(h), Some(t)) = (hand.as_ref(), table_topic.as_ref()) else {
                    continue;
                };
                // A hand that is over is a hand nobody is waiting on.
                if h.over() || said.is_empty() {
                    continue;
                }
                for out in &said {
                    let _ = swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(t.clone(), out.clone());
                }
            }

            // A stage that nobody is completing. Polled rather than armed:
            // the answer changes every time a stage opens or closes, and the
            // cryptographic ones are bounded per stage rather than per hand.
            _ = stall.tick() => {
                let now = super::node::now_unix_ms();
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
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "my clock has run out on seat {} — {mine}",
                                who.join(", ")
                            )))
                            .await;
                        if let Some(n) = h.take_cert_note() {
                            let _ = events.send(NodeEvent::Warning(n)).await;
                        }
                        publish_hand(sends, table_topic.as_ref(), &mut swarm, &mut said);
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
                match h.abort_now(crate::table::hand::Abort::Deadline, &app_key, now) {
                    Ok(sends) => {
                        publish_hand(sends, table_topic.as_ref(), &mut swarm, &mut said);
                        // **And name the cause when the transport is the cause.**
                        // A player reading "the hand ran out of time" looks for
                        // a slow opponent. Across two networks the opponent was
                        // not slow: the only path to it was a relay circuit
                        // limited to 128 KB and two minutes, which is less than
                        // one hand, and the hand stopped at the deal.
                        let stranded: Vec<&libp2p::PeerId> =
                            relayed_peers.intersection(&poker_peers).collect();
                        let why = if relay_inadequate && !stranded.is_empty() {
                            format!(
                                ". {} of the poker peers here {} reachable only through a relay                                  whose reservation this client already reported as too small to                                  carry a hand - that is the likely cause, and it is not the                                  opponent being slow",
                                stranded.len(),
                                if stranded.len() == 1 { "is" } else { "are" }
                            )
                        } else {
                            String::new()
                        };
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "the hand ran out of time; every stack is restored{why}"
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
                let action = if turn.legal.can_check {
                    crate::poker::actions::Action::Check
                } else {
                    crate::poker::actions::Action::Fold
                };
                let now = super::node::now_unix_ms();
                match h.act(action, &app_key, now) {
                    Ok(sends) => {
                        publish_hand(sends, table_topic.as_ref(), &mut swarm, &mut said);
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "your clock ran out — {action:?} for you"
                            )))
                            .await;
                        let report = report_hand(h, &events, &mut turn_reported).await;
                        if let Some(end) = report.ended {
                            next_hand_at = Some(tokio::time::Instant::now() + end.pause());
                        }
                        act_by = report.clock.apply(act_by, h.action_deadline());
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
                // **Said when the roster moves, and only then.** Reported at
                // the derivation rather than at the end of the hand, because a
                // late certificate is banked in between and the earlier version
                // described a state the derivation never saw. But a table where
                // nobody leaves has nothing to explain, and two lines of
                // required sets, allowances and strike counts after every hand
                // are two lines a player has to read past to find out what
                // happened to theirs.
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
                if let Some(last) = said.pop() {
                    said.clear();
                    said.push(last);
                } else {
                    said.clear();
                }
                match next {
                    Some(opening) => {
                        begin_hand(
                            opening,
                            &app_key,
                            &mut hand,
                                    &mut said,
                            &mut swarm,
                            table_topic.as_ref(),
                            &events,
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
                                "lobby topic: {mesh} of {known} subscribed {who:?}; connected {connected:?}"
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
                // Not a table that has closed. A full tournament under way has
                // nobody to attract - it is a closed group now - and a lobby
                // listing it is a lobby listing a door that does not open.
                if let Some(f) = table
                    .as_mut()
                    .filter(|f| f.is_founder())
                    .filter(|_| !table_closed)
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
    topic: Option<&gossipsub::IdentTopic>,
    events: &Events,
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
            publish_hand(sends, topic, swarm, said);
            // What this hand hangs off, said out loud. Two peers that opened
            // hand one from different views of the formation produce different
            // genesis values, and every message each sends is then "a different
            // parent" to the other — which reads, in every other line of the
            // log, as silence. This is the one line that tells them apart.
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "hand #{} opens at genesis {} with seats {:?}",
                    h.hand_id(),
                    short_hash(&h.genesis()),
                    h.required()
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
fn publish_hand(
    sends: Vec<crate::table::hand::Send>,
    topic: Option<&gossipsub::IdentTopic>,
    swarm: &mut libp2p::Swarm<super::swarm::PokerBehaviour>,
    said: &mut Vec<Vec<u8>>,
) {
    for crate::table::hand::Send::Broadcast(out) in sends {
        if let Some(t) = topic {
            let _ = swarm.behaviour_mut().gossipsub.publish(t.clone(), out.clone());
        }
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

/// The first hand's opening, once the roster has ratified.
///
/// A named function rather than the expression inline, because both roads into
/// the first hand call it and an expression duplicated at two sites is an
/// expression that can come to differ at two sites.
fn opening_for_hand_one(f: &Formation) -> Option<crate::table::hand::Opening> {
    crate::table::hand::Opening::from_formation(f, 1)
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
        Protocol::Ip6(ip) => !ip.is_loopback() && !ip.is_unspecified(),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
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
