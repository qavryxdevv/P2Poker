//! The libp2p node: transports, discovery behaviours, and the table mesh.
//!
//! `docs/NETWORK_STACK.md` and `docs/research/LIBP2P.md`. Everything here is
//! transport. **This module must never decide a poker rule and never create a
//! card** (`SPEC_CS.md` §1).
//!
//! # Why this client runs a relay server at all (D-002)
//!
//! The owner's decision: a publicly reachable client volunteers to relay other
//! people's games on its own line. Without volunteers there are no relays, and
//! without relays two clients that are both behind symmetric NAT cannot reach
//! each other at all — which D-004 says must still work.
//!
//! It is **conditional on being publicly reachable** — but the condition is on
//! *advertising*, not on capacity, and the first draft of this module conflated
//! the two.
//!
//! The harm in a relay behind NAT is that it announces itself as a way through
//! and is not one, so a peer that picks it has spent an attempt on nothing. The
//! harm is in the announcement. Capacity that nobody can reach costs nothing and
//! is never used, and `relay::Config` cannot be changed after the swarm is
//! built — so a client that had to become a volunteer later would have to be
//! rebuilt.
//!
//! So the limits are always the volunteer ones, and **AutoNAT gates the announce
//! under the relay namespace** ([`super::run`]). A client behind NAT
//! is then exactly as useless as a relay as it would have been with zero limits,
//! and becomes useful the moment AutoNAT says it is reachable, with nothing to
//! rebuild.
//!
//! # The relay limits are set explicitly, and the defaults are the reason
//!
//! `relay::Config::default()` is **2 minutes and 128 KiB**. Those numbers are
//! sized for hole-punch coordination — a handful of packets to get two peers
//! talking directly — and a poker session is neither. A relayed table would be
//! cut off mid-hand, and the failure would look like a peer disconnecting rather
//! than like a limit being hit.
//!
//! So they are set here, deliberately and with the reasoning written down, and
//! they are set **generously but not without bound**: this client is lending its
//! own bandwidth to strangers and that has to have an edge.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher as _};
use std::time::Duration;

use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::{
    autonat, connection_limits, dcutr, gossipsub, identify, identity, kad, request_response,
    kad::store::MemoryStore, noise, ping, relay,
    swarm::NetworkBehaviour,
    tcp, tls, yamux, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};

use super::joinrpc::JoinCodec;
use super::snapshot::SnapshotCodec;
use crate::protocol::constants::{
    GOSSIP_MAX_TRANSMIT, IDLE_CONNECTION_TIMEOUT_MS, LOBBY_CHAT_TOPIC, LOBBY_TOPIC,
    MDNS_QUERY_INTERVAL_MS,
};

/// Whether this client offers its line to other people's games.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayRole {
    /// Volunteering (D-002), if and when AutoNAT says this client is reachable.
    ///
    /// Carries capacity from the start. Whether that capacity is ever *announced*
    /// is a separate decision made later, and it is the announcement that would
    /// do harm from behind a NAT.
    Volunteer,
    /// The user declined to relay at all.
    ///
    /// Zero capacity, so a reservation is refused even if this client turns out
    /// to be reachable. This is a preference, not a reachability finding.
    Declined,
}

/// How long a relayed circuit may live.
///
/// **The whole-hand cap, because anything less refuses itself.** The first
/// version was ten minutes, on the reasoning that it "covers a hand and then
/// some" — while [`adequate`](super::relay::adequate), the function that decides
/// whether a relay is usable, judges against `HAND_DEADLINE_CAP_MS`, an hour. So
/// every p2p-poker relay was declared inadequate by every p2p-poker client, and
/// the two halves of this project's own NAT story cancelled each other out.
/// Neither number was wrong alone; nothing compared them, and now a test does.
///
/// An hour is a long circuit to lend a stranger and it is the honest number: a
/// legal hand may take that long under §7.2's cap, and a circuit that dies
/// mid-street is the engineered abort `SPEC_CS.md` §19 treats as a security
/// problem rather than a hiccup. A volunteer who will not lend it declines
/// outright with [`RelayRole::Declined`], rather than by offering a circuit that
/// cannot carry a hand.
pub const RELAY_RESERVATION: Duration =
    Duration::from_millis(crate::protocol::constants::HAND_DEADLINE_CAP_MS);

/// How much data one relayed circuit may carry.
///
/// Derived from what a hand actually costs rather than picked: a full table's
/// `per_hand_bytes`, times the headroom the adequacy rule demands, times four
/// for the hands after the first. A literal would be a second number to keep in
/// step with the first, and those drift — which is exactly how the duration
/// above came to refuse itself.
pub const RELAY_MAX_CIRCUIT_BYTES: u64 =
    super::relay::per_hand_bytes(crate::protocol::constants::MAX_SEATS)
        * super::relay::HAND_HEADROOM
        * 4;

/// How many circuits one peer may hold on this client at once.
pub const RELAY_MAX_CIRCUITS_PER_PEER: usize = 4;

/// How many circuits this client carries at once, for everybody together -- the
/// number the first-run notice and the settings tell the player (`D-002`).
pub const RELAY_MAX_CIRCUITS: usize = 64;

/// `D-002`, `S1-FK`: whom this client's relay serves, and whether it serves at
/// all. Shared with the node loop, because `relay::Config` is fixed when the
/// swarm is built and neither of these is.
#[derive(Debug)]
pub struct RelayAdmission {
    /// Point 2: the peers `identify` named poker clients, kept by the node loop
    /// beside its own `poker_peers`.
    known: std::sync::RwLock<std::collections::HashSet<PeerId>>,
    /// Point 1 as the owner ruled it (2026-09-18): the player's switch in the
    /// settings, **on by default**. Off, the relay takes no reservation and opens
    /// no circuit; what is already open runs out on its own.
    on: std::sync::atomic::AtomicBool,
}

impl Default for RelayAdmission {
    fn default() -> Self {
        RelayAdmission { known: Default::default(), on: std::sync::atomic::AtomicBool::new(true) }
    }
}

impl RelayAdmission {
    /// A poker client this relay may reserve for.
    pub fn admit(&self, peer: PeerId) {
        if let Ok(mut known) = self.known.write() {
            known.insert(peer);
        }
    }

    /// A peer gone from this client.
    pub fn forget(&self, peer: &PeerId) {
        if let Ok(mut known) = self.known.write() {
            known.remove(peer);
        }
    }

    /// The player's switch; `false` stops new reservations and circuits at once.
    pub fn set_on(&self, on: bool) {
        self.on.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_on(&self) -> bool {
        self.on.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn reserves_for(&self, peer: &PeerId) -> bool {
        self.is_on() && self.known.read().is_ok_and(|known| known.contains(peer))
    }
}

/// `D-002`, `S1-FK`: the admission the node loop and the relay share.
pub type RelayAdmits = std::sync::Arc<RelayAdmission>;

/// `D-002` point 2, `S1-FK`: a **reservation** only for a peer `identify` named a
/// poker client, and only while the player's switch is on. Without it a client
/// with a confirmed external address was a relay for the whole libp2p world,
/// IPFS traffic included, on its player's line. Our own clients ask for a
/// reservation only once the relay's `identify` reached them (`run`, the
/// reservation request), and the relay's reading of theirs is on its way at the
/// same moment, so they are known when they ask.
///
/// **Circuits are gated on the switch alone**, where `NETWORK_STACK.md` §9.6
/// asked for the source too: a circuit can end only at a peer holding a
/// reservation here, which the gate above makes one of ours -- nobody else's
/// traffic can cross this client -- and a newcomer's first dial through a relay
/// it has never met opens the circuit half a round trip before the relay has its
/// `identify`, so a source gate would refuse exactly the new player trying to
/// reach a table.
struct PokerPeersOnly(RelayAdmits);

impl relay::RateLimiter for PokerPeersOnly {
    fn try_next(&mut self, peer: PeerId, _addr: &libp2p::Multiaddr, _now: web_time::Instant) -> bool {
        self.0.reserves_for(&peer)
    }
}

/// `D-002` point 1: no circuit through this client while the player has the
/// relay switched off.
struct WhileOn(RelayAdmits);

impl relay::RateLimiter for WhileOn {
    fn try_next(&mut self, _peer: PeerId, _addr: &libp2p::Multiaddr, _now: web_time::Instant) -> bool {
        self.0.is_on()
    }
}

/// How long a join request may go unanswered before it is a failure.
///
/// A founder that has to be dialled through a relay is slow, and a joiner that
/// gives up at five seconds gives up on a table it could have sat down at. Thirty
/// is long enough for a relayed round trip and short enough that a player is not
/// left looking at a button that appears to have done nothing.
pub const JOIN_RPC_TIMEOUT_MS: u64 = 30_000;
/// How long a lobby question (§7.5) waits for its answer. Short: the answer
/// is one advert or none, and an unanswered question is asked again at the
/// next housekeeping tick anyway.
pub const SNAPSHOT_RPC_TIMEOUT_MS: u64 = 10_000;


// ---------------------------------------------------------------------------
// The bogon filter (`S1-AC`)
// ---------------------------------------------------------------------------

/// Whether an address is one this client will let a **DHT record** send it to.
///
/// `NETWORK_STACK.md` §4.5 rule 2 says to drop RFC 1918, RFC 6598, loopback,
/// link-local, multicast, broadcast and the reserved ranges from DHT-derived
/// candidates, and gives the reason in its own words: *"a record in the public
/// DHT claiming a private address is either pollution or an attempt to make us
/// scan our own LAN."*
///
/// # This is not [`super::run::reachable`], and the difference matters
///
/// `reachable` asks *"could the rest of the internet dial this"* and answers
/// **no** for `/dns4/example.com/tcp/4001`, which has no IP component at all.
/// That is right where it is used — deciding what may be published as an
/// external address — and wrong here: a name is not a bogon, and dropping one
/// would cut the `/dnsaddr/bootstrap.libp2p.io` shape out of the routing table.
///
/// So this refuses an address only when it **carries an IP and that IP is
/// somebody's own network**. Anything it cannot judge, it keeps: a filter that
/// guesses is worse than one that admits what it does not know.
fn not_a_bogon(addr: &libp2p::Multiaddr) -> bool {
    use libp2p::multiaddr::Protocol;
    !addr.iter().any(|p| match p {
        Protocol::Ip4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_multicast()
                // RFC 6598, carrier-grade NAT: 100.64.0.0/10. Not `is_private`.
                || (ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1]))
        }
        Protocol::Ip6(ip) => {
            let first = ip.segments()[0];
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (first & 0xfe00) == 0xfc00 // fc00::/7, unique local
                || (first & 0xffc0) == 0xfe80 // fe80::/10, link-local
                || ip.segments()[..2] == [0x2001, 0x0db8] // documentation
        }
        _ => false,
    })
}

/// A `NetworkBehaviour` that refuses to hand the swarm a private address.
///
/// # Why a wrapper, and why only around Kademlia
///
/// `NETWORK_STACK.md` §4.5's filter had somewhere to live under the old
/// Mainline discovery, where a candidate was a `SocketAddrV4` that application
/// code parsed. Under `libp2p-kad` the addresses never reach application code:
/// they ride inside the DHT messages, go into the routing table, and
/// `run.rs` dials with `DialOpts::peer_id(peer)`. The provider handler sees a
/// `HashSet<PeerId>` and nothing else. `S1-AC`.
///
/// **libp2p records the provenance the finding said the swarm does not.**
/// `DialOpts::peer_id(p).build()` sets `extend_addresses_through_behaviour:
/// true` and every address-carrying builder sets it `false` — including
/// `From<Multiaddr>`, which is what `swarm.dial(addr)` uses. `Swarm::dial`
/// appends what the behaviour returns **only** when that flag is true. So a
/// filter here touches the DHT path and is **structurally unable** to reach the
/// mDNS path, where a LAN peer is dialled on the `192.168.x` address this
/// project deliberately supports (§9.8, and §4.5's own warning that this filter
/// and §5.6's publish filter must never be merged).
///
/// # It counts what it drops, because that number does not exist
///
/// Nobody knows how many bogons the real Amino DHT hands this client. A filter
/// that counts produces the figure as a byproduct instead of requiring a second
/// measurement pass.
pub struct Bogonless<B> {
    inner: B,
    dropped: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// `S1-IZ`, in a binary built to be measured: the dials the inner behaviour
    /// asked for, by their connection id, until `net::run` reads each one's
    /// source as the swarm starts it.
    dials: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<libp2p::swarm::ConnectionId>>>,
}

/// `S1-IZ`: the most dial ids [`Bogonless`] holds for `net::run` to read. A dial
/// the swarm never starts is never read, so the set is bounded, and one that is
/// full is emptied rather than grown.
pub const DIALS_HELD_MAX: usize = 4096;

/// `S1-IZ`: how often the public DHT's behaviour bootstraps on its own --
/// **once an hour, where libp2p's default is every five minutes.**
///
/// A bootstrap is a walk to this client's own key and then one to a random key
/// in every farther bucket, some fifteen walks one after another, each allowed
/// the query timeout; it took seven to ten minutes and some 4 500 nodes, most
/// of them dials, and the next began five minutes after this one had -- so at
/// libp2p's default a client never stopped bootstrapping. Measured 2026-09-25,
/// side by side for sixteen minutes, an idle client and a two-seat table in each
/// arm: the bootstraps were 420 to 490 of some 670 nodes a minute a client
/// asked, and without the periodic one a settled client dialled 41 to 53 times
/// a minute against 488, held 14 to 15 connections against 60, and still met
/// every seat within two minutes, saw every table and dealt its hands. The
/// routing table is kept by this client's own walks -- the lobby's, the hour's,
/// the announcements -- and by every connection; the hourly bootstrap, its
/// buckets under `run::BOOTSTRAP_STEP_CAP`, is the guard against a table that
/// goes stale over a long session, which sixteen minutes cannot measure.
const PUBLIC_BOOTSTRAP_EVERY: Option<Duration> = Some(Duration::from_secs(60 * 60));

/// `S1-IZ`: the interval this client runs by -- in a binary built to be
/// measured, whatever `P2P_POKER_BOOTSTRAP_EVERY_S` says, 0 for none. A
/// bootstrap the routing table asks for when it holds too few peers is the
/// library's either way.
fn public_bootstrap_every() -> Option<Duration> {
    #[cfg(feature = "fault-harness")]
    if let Some(s) = std::env::var("P2P_POKER_BOOTSTRAP_EVERY_S").ok().and_then(|v| v.parse::<u64>().ok()) {
        return (s > 0).then(|| Duration::from_secs(s));
    }
    PUBLIC_BOOTSTRAP_EVERY
}

/// `S1-IT`: what becomes of the addresses a behaviour offers for a dial, after
/// whatever filter of its own it has: nothing this client has no transport for,
/// and a relay at every address any record gave it (`dialable`).
///
/// **Around the whole behaviour, because every part of it offers addresses.**
/// The first cut shaped only the public DHT's offers, and the bed's next run
/// still held *no transport for the address* in 812 of 969 failed dials of peers
/// that matter; the second added `identify`'s cache and the private DHT, and the
/// run after it held 855 of 1 359. `identify` hands every address a peer lists to
/// the swarm (`NewExternalAddrOfPeer`), the swarm hands it to every behaviour,
/// and each request-response behaviour offers it back for a dial -- a poker
/// client met once comes with every leg of its relay, WebTransport and
/// WebSocket among them, from six behaviours at once. So the shaping is done
/// once, on what all of them offer together ([`OnlyDialable`]).
#[derive(Clone)]
pub struct Offered {
    undialable: std::sync::Arc<std::sync::atomic::AtomicU64>,
    relays: std::sync::Arc<std::sync::Mutex<super::dialable::RelayBook>>,
    /// The peers a dial of which matters, as `net::run` tells it: the founder
    /// being asked for a seat, a player the lobby named.
    matters: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<PeerId>>>,
    /// And the relays the records route those peers through, for `net::run` to
    /// take: a circuit is given up when its relay is not reached, and this
    /// client's own connection limit was what refused 364 relays in one run.
    relays_that_matter: std::sync::Arc<std::sync::Mutex<Vec<PeerId>>>,
    /// The control, from the same binary (`offers_every_address`).
    offer_every_address: bool,
}

impl Default for Offered {
    fn default() -> Self {
        Self {
            undialable: Default::default(),
            relays: Default::default(),
            matters: Default::default(),
            relays_that_matter: Default::default(),
            offer_every_address: offers_every_address(),
        }
    }
}

impl Offered {
    /// How many addresses were not offered for want of a transport.
    pub fn undialable(&self) -> std::sync::Arc<std::sync::atomic::AtomicU64> {
        std::sync::Arc::clone(&self.undialable)
    }

    /// Whether a record has named this peer as somebody's relay.
    pub fn knows_relay(&self, peer: &PeerId) -> bool {
        self.relays.lock().unwrap_or_else(|held| held.into_inner()).knows(peer)
    }

    /// A dial of this peer matters. Bounded: the set is forgotten whole when it
    /// is full, and whoever still matters is said again at its next dial.
    pub fn matters(&self, peer: PeerId) {
        let mut matters = self.matters.lock().unwrap_or_else(|held| held.into_inner());
        if matters.len() >= PEERS_THAT_MATTER_MAX {
            matters.clear();
        }
        matters.insert(peer);
    }

    /// The relays that peers that matter were offered through since this was
    /// last asked, each once.
    pub fn take_relays_that_matter(&self) -> Vec<PeerId> {
        std::mem::take(&mut *self.relays_that_matter.lock().unwrap_or_else(|held| held.into_inner()))
    }

    /// Notes the relays a peer that matters is offered through.
    fn note_relays(&self, maybe_peer: Option<PeerId>, offered: &[libp2p::Multiaddr]) {
        let Some(peer) = maybe_peer else { return };
        if !self.matters.lock().unwrap_or_else(|held| held.into_inner()).contains(&peer) {
            return;
        }
        let mut wanted = self.relays_that_matter.lock().unwrap_or_else(|held| held.into_inner());
        for (relay, _) in offered.iter().filter_map(super::dialable::relay_of) {
            if wanted.len() < RELAYS_THAT_MATTER_MAX && !wanted.contains(&relay) {
                wanted.push(relay);
            }
        }
    }

    /// `already` is what the dial was given; `offered` what the behaviour adds.
    fn shape(&self, maybe_peer: Option<PeerId>, already: &[libp2p::Multiaddr], offered: Vec<libp2p::Multiaddr>) -> Vec<libp2p::Multiaddr> {
        let shaped = self.shape_addresses(maybe_peer, already, offered);
        self.note_relays(maybe_peer, already);
        self.note_relays(maybe_peer, &shaped);
        shaped
    }

    fn shape_addresses(&self, maybe_peer: Option<PeerId>, already: &[libp2p::Multiaddr], offered: Vec<libp2p::Multiaddr>) -> Vec<libp2p::Multiaddr> {
        let mut relays = self.relays.lock().unwrap_or_else(|held| held.into_inner());
        if self.offer_every_address {
            // The control still LEARNS, so that the lines about relays are said
            // in both kinds of run; it filters nothing and adds nothing.
            relays.learn(offered.iter().filter(|a| !super::dialable::lacks_transport(a)));
            return offered;
        }
        let before = offered.len();
        let mut kept: Vec<libp2p::Multiaddr> =
            offered.into_iter().filter(|a| !super::dialable::lacks_transport(a)).collect();
        let undialable = before - kept.len();
        if undialable > 0 {
            self.undialable
                .fetch_add(undialable as u64, std::sync::atomic::Ordering::Relaxed);
        }
        // **And the relay is dialled at every address the records gave it.**
        // What they say of a relay is read out of the circuits that go through
        // it, and offered when the relay itself is the peer being dialled --
        // which is the relay client's own dial, made with the one address of
        // one request and *extended through the behaviours*, that is, by this.
        relays.learn(kept.iter());
        if let Some(peer) = maybe_peer {
            for known in relays.addresses_of(&peer) {
                if !kept.contains(known) && !already.contains(known) {
                    kept.push(known.clone());
                }
            }
        }
        kept
    }
}

/// The most peers a dial of which is held to matter at once.
pub const PEERS_THAT_MATTER_MAX: usize = 1024;

/// The most relays waiting for `net::run` to take them.
pub const RELAYS_THAT_MATTER_MAX: usize = 256;

/// `S1-IT`'s control: `P2P_POKER_OFFER_EVERY_ADDRESS=1`, **in a binary built to
/// be measured and in no other**, switches off the row's two changes -- nothing
/// is dropped for want of a transport and no relay's addresses are added -- so
/// that a run with them and a run without differ in nothing else.
fn offers_every_address() -> bool {
    cfg!(feature = "fault-harness") && std::env::var_os("P2P_POKER_OFFER_EVERY_ADDRESS").is_some_and(|v| v == "1")
}

impl<B> Bogonless<B> {
    pub fn new(inner: B) -> Self {
        Self {
            inner,
            dropped: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            dials: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        }
    }

    /// A handle to the count, so the status line can read it without borrowing
    /// the swarm.
    pub fn dropped(&self) -> std::sync::Arc<std::sync::atomic::AtomicU64> {
        std::sync::Arc::clone(&self.dropped)
    }

    /// `S1-IZ`: whether the inner behaviour asked for the dial with this id --
    /// read once, as the swarm starts it. Always `false` in a player's build,
    /// which records nothing.
    pub fn asked_for(&self, id: libp2p::swarm::ConnectionId) -> bool {
        self.dials.lock().unwrap_or_else(|held| held.into_inner()).remove(&id)
    }
}

/// So every existing `swarm.behaviour_mut().ipfs_kad.get_providers(..)` call
/// site keeps working unchanged. The wrapper adds one method to the behaviour
/// trait's surface and nothing to the crate's.
impl<B> std::ops::Deref for Bogonless<B> {
    type Target = B;
    fn deref(&self) -> &B {
        &self.inner
    }
}

impl<B> std::ops::DerefMut for Bogonless<B> {
    fn deref_mut(&mut self) -> &mut B {
        &mut self.inner
    }
}

impl<B: libp2p::swarm::NetworkBehaviour> libp2p::swarm::NetworkBehaviour for Bogonless<B> {
    type ConnectionHandler = B::ConnectionHandler;
    type ToSwarm = B::ToSwarm;

    fn handle_pending_inbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        local: &libp2p::Multiaddr,
        remote: &libp2p::Multiaddr,
    ) -> Result<(), libp2p::swarm::ConnectionDenied> {
        self.inner.handle_pending_inbound_connection(id, local, remote)
    }

    fn handle_established_inbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        peer: PeerId,
        local: &libp2p::Multiaddr,
        remote: &libp2p::Multiaddr,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        self.inner
            .handle_established_inbound_connection(id, peer, local, remote)
    }

    /// **The one method that is not a delegation.**
    fn handle_pending_outbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[libp2p::Multiaddr],
        role: libp2p::core::Endpoint,
    ) -> Result<Vec<libp2p::Multiaddr>, libp2p::swarm::ConnectionDenied> {
        let offered = self
            .inner
            .handle_pending_outbound_connection(id, maybe_peer, addresses, role)?;
        let before = offered.len();
        let kept: Vec<libp2p::Multiaddr> =
            offered.into_iter().filter(not_a_bogon_ref).collect();
        let dropped = before - kept.len();
        if dropped > 0 {
            self.dropped
                .fetch_add(dropped as u64, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(kept)
    }

    fn handle_established_outbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        peer: PeerId,
        addr: &libp2p::Multiaddr,
        role: libp2p::core::Endpoint,
        port_use: libp2p::core::transport::PortUse,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        self.inner
            .handle_established_outbound_connection(id, peer, addr, role, port_use)
    }

    /// **The second method that is not a delegation** (`S1-FJ`). `libp2p-kad`
    /// fills this client's own provider record, when it answers a lookup of a
    /// key it provides, from every listen address the swarm reported -- the
    /// wildcard listeners' loopback, LAN, hypervisor-switch and ULA addresses,
    /// the topology `S1-Z` hid from `identify`. So an address this filter would
    /// refuse to dial is never reported to the inner behaviour as one it listens
    /// on: its record holds public addresses and relay circuits, and a stranger
    /// who asks for the lobby's providers learns no home network. mDNS is its
    /// own behaviour and keeps the LAN; `run` bootstraps the DHT itself, so the
    /// inner's bootstrap on a new listen address is not needed.
    fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm) {
        use libp2p::swarm::FromSwarm;
        match event {
            FromSwarm::NewListenAddr(e) if !not_a_bogon(e.addr) => {}
            FromSwarm::ExpiredListenAddr(e) if !not_a_bogon(e.addr) => {}
            other => self.inner.on_swarm_event(other),
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer: PeerId,
        id: libp2p::swarm::ConnectionId,
        event: libp2p::swarm::THandlerOutEvent<Self>,
    ) {
        self.inner.on_connection_handler_event(peer, id, event)
    }

    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>>
    {
        let polled = self.inner.poll(cx);
        // `S1-IZ`: which dials are this behaviour's, where a run is measured.
        if cfg!(feature = "fault-harness") {
            if let std::task::Poll::Ready(libp2p::swarm::ToSwarm::Dial { opts }) = &polled {
                let mut dials = self.dials.lock().unwrap_or_else(|held| held.into_inner());
                if dials.len() >= DIALS_HELD_MAX {
                    dials.clear();
                }
                dials.insert(opts.connection_id());
            }
        }
        polled
    }
}

/// fault-harness, `S1-IB`/`S1-ID`: set by `net::run` while the harness's
/// outage covers both lines (`tox::table::both_lines_dark`). Every connection
/// is refused while it is, inbound and outbound -- the lobby's transport as
/// dark as a machine whose internet has gone: its advert goes stale, its
/// search queue falls silent, and nobody reaches it.
#[cfg(feature = "fault-harness")]
pub static DARK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// No connection while the harness keeps this client dark; `Ok` in every other
/// build and at every other time.
fn dark_gate() -> Result<(), libp2p::swarm::ConnectionDenied> {
    #[cfg(feature = "fault-harness")]
    if DARK.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(libp2p::swarm::ConnectionDenied::new(std::io::Error::other(
            "fault-harness: this client's internet is gone",
        )));
    }
    Ok(())
}

/// `S1-IT`: a behaviour whose offered addresses pass through [`Offered`] --
/// worn by the WHOLE of this client's behaviour, so that what any part of it
/// offers for a dial is shaped once and alike. It is also where the harness's
/// dark outage refuses every connection ([`dark_gate`]), for the same reason.
pub struct OnlyDialable<B> {
    inner: B,
    offered: Offered,
}

/// The behaviour this client's swarm runs: all of [`PokerBehaviour`], offering
/// for a dial only what this client can dial.
pub type ShapedBehaviour = OnlyDialable<PokerBehaviour>;

impl<B> OnlyDialable<B> {
    pub fn new(inner: B) -> Self {
        Self { inner, offered: Offered::default() }
    }

    /// What is done to offered addresses, and the book of relays: for
    /// `net::run`, which reads it and tells it which dials matter.
    pub fn offered(&self) -> &Offered {
        &self.offered
    }
}

impl<B> std::ops::Deref for OnlyDialable<B> {
    type Target = B;
    fn deref(&self) -> &B {
        &self.inner
    }
}

impl<B> std::ops::DerefMut for OnlyDialable<B> {
    fn deref_mut(&mut self) -> &mut B {
        &mut self.inner
    }
}

impl<B: libp2p::swarm::NetworkBehaviour> libp2p::swarm::NetworkBehaviour for OnlyDialable<B> {
    type ConnectionHandler = B::ConnectionHandler;
    type ToSwarm = B::ToSwarm;

    fn handle_pending_inbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        local: &libp2p::Multiaddr,
        remote: &libp2p::Multiaddr,
    ) -> Result<(), libp2p::swarm::ConnectionDenied> {
        dark_gate()?;
        self.inner.handle_pending_inbound_connection(id, local, remote)
    }

    fn handle_established_inbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        peer: PeerId,
        local: &libp2p::Multiaddr,
        remote: &libp2p::Multiaddr,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        dark_gate()?;
        self.inner
            .handle_established_inbound_connection(id, peer, local, remote)
    }

    /// The one method that is not a delegation.
    fn handle_pending_outbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[libp2p::Multiaddr],
        role: libp2p::core::Endpoint,
    ) -> Result<Vec<libp2p::Multiaddr>, libp2p::swarm::ConnectionDenied> {
        dark_gate()?;
        let offered = self
            .inner
            .handle_pending_outbound_connection(id, maybe_peer, addresses, role)?;
        Ok(self.offered.shape(maybe_peer, addresses, offered))
    }

    fn handle_established_outbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        peer: PeerId,
        addr: &libp2p::Multiaddr,
        role: libp2p::core::Endpoint,
        port_use: libp2p::core::transport::PortUse,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        dark_gate()?;
        self.inner
            .handle_established_outbound_connection(id, peer, addr, role, port_use)
    }

    fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm) {
        self.inner.on_swarm_event(event)
    }

    fn on_connection_handler_event(
        &mut self,
        peer: PeerId,
        id: libp2p::swarm::ConnectionId,
        event: libp2p::swarm::THandlerOutEvent<Self>,
    ) {
        self.inner.on_connection_handler_event(peer, id, event)
    }

    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>>
    {
        self.inner.poll(cx)
    }
}

/// `filter` wants `&Multiaddr`; the predicate takes one too. A named function
/// rather than a closure so the borrow reads the same at both call sites.
fn not_a_bogon_ref(a: &libp2p::Multiaddr) -> bool {
    not_a_bogon(a)
}

/// The behaviours this node runs.
#[derive(NetworkBehaviour)]
pub struct PokerBehaviour {
    /// The lobby and the table mesh.
    pub gossipsub: gossipsub::Behaviour,
    /// Peer routing. **Not** the global lobby — that is a provider record on
    /// the public Kademlia below, and lives in [`super::run`].
    pub kademlia: kad::Behaviour<MemoryStore>,
    /// The **public** Kademlia, where the relays are and where the lobby is.
    ///
    /// `kademlia` above speaks `/p2p-poker/kad/1`, which is right: this
    /// project's records are its own and have no business in anybody else's
    /// routing table. But it also means this client is invisible to, and blind
    /// to, the network where relays actually advertise themselves — and a peer
    /// behind a NAT with no relay cannot be reached at all.
    ///
    /// libp2p's answer to "where are the relays" is not a file. A relay host
    /// advertises itself in the DHT under the namespace `/libp2p/relay`, and
    /// AutoRelay looks it up there; that is the list, and it maintains itself.
    /// Reading it means speaking `/ipfs/kad/1.0.0`. Since `56b0b50` the lobby is
    /// a provider record on this same DHT (`run::lobby_namespace`), where it used
    /// to be a Mainline announce.
    ///
    /// **Automatic mode, not client mode.** This comment called it client mode —
    /// query, never answer — after `build` had moved to `set_mode(None)` in
    /// `56b0b50`: client until there is a confirmed external address, server
    /// after, because a node that answers nobody is added to nobody's routing
    /// table. The bandwidth argument for client mode is honoured where it
    /// matters, at a closed table (`run::dht_effort`).
    pub ipfs_kad: Bogonless<kad::Behaviour<MemoryStore>>,
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
    /// Asks other peers whether this client is reachable, which is what decides
    /// [`RelayRole`].
    pub autonat_client: autonat::v2::client::Behaviour<crate::security::rng::OsRng10>,
    /// Answers the same question for others.
    pub autonat_server: autonat::v2::server::Behaviour,
    /// Direct connection upgrade: the hole punch that turns a relayed
    /// connection into a direct one.
    pub dcutr: dcutr::Behaviour,
    pub relay_client: relay::client::Behaviour,
    pub relay_server: relay::Behaviour,
    pub conn_limits: connection_limits::Behaviour,
    pub mem_limits: libp2p::memory_connection_limits::Behaviour,
    /// Port mapping, for the router that will do it.
    pub upnp: libp2p::upnp::tokio::Behaviour,
    /// The join RPC: one request, one answer, on `/p2p-poker/join/1`.
    ///
    /// Request-response rather than the mesh, because a join happens between two
    /// peers who are not yet at a table together: there is no mesh to carry it,
    /// and an unanswered request has to become a timeout rather than silence.
    pub join: request_response::Behaviour<JoinCodec>,
    /// The lobby snapshot RPC (§7.5): one question, one answer, on
    /// `/p2p-poker/lobby-snapshot/1`.
    ///
    /// Asked of every poker peer, because the mesh's own subscription
    /// exchange happens once per peer and does not always happen at all
    /// (`S1-DK`, D-040): a table's advert must not depend on it.
    pub snapshot: request_response::Behaviour<SnapshotCodec>,
    /// Local-network discovery.
    ///
    /// Not an optimisation. Two clients on one LAN get each other's **external**
    /// address from the DHT and would have to dial it inwards through their own
    /// router, which is NAT hairpinning and which many routers simply do not do.
    /// The DHT is the wrong tool for a peer that is one hop away, and mDNS is
    /// the right one.
    /// Peers on the same wire, found by multicast.
    ///
    /// Optional, and the reason is evidence rather than tidiness. Two clients
    /// on one network find each other by mDNS in under a second, which is
    /// excellent for playing and useless for **proving** that anything else
    /// works: a test that dials through the DHT and then asserts two peers met
    /// is satisfied either way, and would keep passing after the DHT path
    /// broke. Turning it off is how the other path gets tested at all.
    pub mdns: Toggle<libp2p::mdns::tokio::Behaviour>,
}

/// The topics this node subscribes to.
/// How many connections this client will hold at once.
///
/// Measured before it was capped: **over four hundred**, nearly all of them
/// strangers on the public DHT that this client will never exchange a poker
/// message with. They cost sockets, file handles, keep-alives and — on a home
/// connection — a NAT table that other software has to share.
///
/// What the client actually needs at once: the handful of entry points, two or
/// three relays it holds reservations on, the seats at its tables, and enough
/// DHT peers for a lookup to converge. Eighty is comfortably above that sum and
/// far below four hundred.
///
/// **What it costs.** At the cap a new connection is refused, including one this
/// client wanted, so a lookup mid-flight can lose a hop and take longer.
/// Kademlia stores addresses rather than connections and dials on demand, so
/// the routing table is not what shrinks — the churn during a lookup is. Half
/// the budget is reserved against **incoming** connections, so a crowd of
/// strangers arriving cannot fill the table and leave no room for a relay this
/// client is trying to reach.
pub const MAX_CONNECTIONS: u32 = 80;

/// The floor and the ceiling the cap moves between.
///
/// The cap is not a fixed number, because a fixed one is either wasteful or
/// starving and there is no way to know which in advance. It starts at
/// [`MAX_CONNECTIONS`] and moves: **up** when a connection this client wanted
/// was refused by the limit, **down** while it has been comfortably under for a
/// while. The floor is what a client needs to function at all — entry points,
/// two or three relays, a table's seats and enough of a routing table to look
/// something up. The ceiling is where the cost stops being worth it.
pub const MIN_CONNECTIONS: u32 = 40;

/// The limits at one budget.
///
/// One function, called at start-up and again whenever the budget moves, so the
/// three limits that are derived from it cannot drift apart from each other —
/// `ConnectionLimits`'s setters consume `self` and it has no getters, so the
/// alternative was rebuilding it by hand in three places.
pub fn connection_limits(budget: u32) -> connection_limits::ConnectionLimits {
    connection_limits::ConnectionLimits::default()
        .with_max_pending_incoming(Some(32))
        .with_max_established(Some(budget))
        // Half against strangers arriving, so a crowd cannot fill the table and
        // leave no room for a relay this client is trying to reach.
        .with_max_established_incoming(Some(budget / 2))
        .with_max_established_per_peer(Some(2))
}

/// Where the budget may rise to, and no higher.
///
/// **`S1-IY`: 96, down from 320.** At 320 every client reached the ceiling
/// within seconds of starting -- 637 of 644 logs on the bed, the owner's own at
/// every start -- and then held about 300 connections, idle in the lobby or not:
/// an entry each in a player's router, three times the high-water mark of IPFS
/// Kubo's connection manager (96) and more than seven times the 40 that Brave and
/// IPFS Desktop run it at, for home routers' sake. What the higher number bought
/// had been taken over by `S1-IT`: the dials that matter -- a founder, a player
/// the lobby named, the relays their circuits go through -- and every poker
/// client are let through this limit, so what it governs now is strangers on the
/// DHT. Measured against a control from one binary before the change: tables of
/// two formed in every run at both numbers, on one machine and across two
/// networks in both directions, with as many hands dealt, while the connections
/// held fell from a mean of 239 to 75.
pub const CONNECTION_CEILING: u32 = 96;

/// What this client answers with when asked who it is.
///
/// Named rather than written twice: `identify` announces it, and it is what
/// tells another poker client apart from the several hundred strangers this node
/// shares a DHT with.
///
/// **It was written twice.** This was `pub const PROTOCOL_VERSION: &str` here
/// and `IDENTIFY_PROTOCOL` in `protocol::constants`, holding the same string —
/// and `PROTOCOL_VERSION` is also a `u16` in two other modules, so the name
/// carried two types and three meanings. One definition, under the name that
/// says what it is.
pub use crate::protocol::constants::IDENTIFY_PROTOCOL;

pub struct Topics {
    pub lobby: gossipsub::IdentTopic,
    pub lobby_chat: gossipsub::IdentTopic,
}

impl Default for Topics {
    fn default() -> Self {
        Topics {
            lobby: gossipsub::IdentTopic::new(LOBBY_TOPIC),
            lobby_chat: gossipsub::IdentTopic::new(LOBBY_CHAT_TOPIC),
        }
    }
}

/// What a node needs to know before it starts.
pub struct NodeConfig {
    pub identity: identity::Keypair,
    /// Whether this client is willing to relay at all (D-002).
    ///
    /// A **preference**, not a reachability finding. Reachability is AutoNAT's
    /// and it gates the announce rather than the capacity; see the module
    /// documentation for why the two were separated.
    pub relay_role: RelayRole,
    /// Whether to look for peers by multicast on the local network.
    ///
    /// True for a client, because a player on the same wire should be found in
    /// a second rather than in a minute. False when the point is to prove that
    /// discovery over the DHT works, since mDNS would answer first and the
    /// proof would be of nothing.
    pub local_discovery: bool,
    /// `D-002` (`S1-FK`): who may hold a reservation on this client's relay,
    /// and whether it relays at all. The node loop fills it and sets the switch
    /// from the player's settings; an empty set refuses everybody.
    pub relay_admits: RelayAdmits,
}

/// Build the node.
///
/// **Must be called from inside a tokio runtime.** The UPnP behaviour spawns a
/// task in its constructor, so building outside one panics inside the library
/// rather than returning an error. That is a property of the dependency and not
/// of this function, and it is written here because the failure is a panic in
/// somebody else's file.
///
/// The builder is a type-state machine and the phase order is fixed:
/// identity → runtime → TCP → QUIC → DNS → relay client → behaviour → config.
/// Note that `with_relay_client` gives the behaviour closure a **second
/// argument**; that is not optional and is why the closure below takes two.
pub fn build(config: NodeConfig) -> Result<Swarm<ShapedBehaviour>, Box<dyn std::error::Error>> {
    let local_discovery = config.local_discovery;
    let local_peer_id = PeerId::from(config.identity.public());
    let relay_role = config.relay_role;
    let relay_admits = config.relay_admits.clone();

    let swarm = SwarmBuilder::with_existing_identity(config.identity)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            (tls::Config::new, noise::Config::new),
            yamux::Config::default,
        )?
        .with_quic_config(|mut cfg| {
            cfg.handshake_timeout = Duration::from_secs(10);
            cfg.max_idle_timeout = 30_000;
            cfg.keep_alive_interval = Duration::from_secs(5);
            cfg
        })
        .with_dns()?
        .with_relay_client((tls::Config::new, noise::Config::new), yamux::Config::default)?
        .with_behaviour(|key, relay_client| {
            let gossipsub = build_gossipsub(key)?;

            let mut kad_cfg = kad::Config::new(StreamProtocol::new("/p2p-poker/kad/1"));
            kad_cfg.set_query_timeout(Duration::from_secs(60));
            let kademlia = kad::Behaviour::with_config(
                local_peer_id,
                MemoryStore::new(local_peer_id),
                kad_cfg,
            );

            let mut ipfs_cfg = kad::Config::new(StreamProtocol::new("/ipfs/kad/1.0.0"));
            ipfs_cfg.set_query_timeout(Duration::from_secs(60));
            ipfs_cfg.set_periodic_bootstrap_interval(public_bootstrap_every());
            let mut ipfs_kad = kad::Behaviour::with_config(
                local_peer_id,
                MemoryStore::new(local_peer_id),
                ipfs_cfg,
            );
            // Automatic, not pinned to client.
            //
            // Client mode means never answering a query, which sounds polite and
            // starves the routing table: nobody adds a node they never hear
            // from, and a table built only from peers this node happened to
            // speak to first does not span the key space. Measured with it
            // pinned: two clients each announced themselves in the lobby, each
            // read the lobby a hundred and thirty times over ten minutes, each
            // found three or four other players — and never each other, because
            // their walks landed on different nodes.
            //
            // `None` restores libp2p's own rule: client until there is a
            // confirmed external address, server after. A client behind a NAT
            // gains one when a relay accepts its reservation, so the node that
            // starts answering queries is one that can actually be reached.
            ipfs_kad.set_mode(None);

            // **`identify` sent this machine's LAN topology to every stranger on
            // the public DHT**, and `NETWORK_STACK.md` §3.5 now says so.
            //
            // `hide_listen_addrs` defaults to false, so the message carries
            // `listen_addresses ∪ external_addresses`. The external half is
            // filtered — `run::reachable` gates what AutoNAT may add, which is
            // §5.6's publish filter doing its job — but the listen half is the
            // raw bound set. Measured here on 2026-09-02 that was
            // `/ip4/192.168.1.20`, `/ip4/172.27.224.1` (a Hyper-V "Default
            // Switch"), and two `fdc9:…` IPv6 ULAs. A peer on the Amino DHT that
            // never sees the lobby key still learned the player's home subnet
            // and which hypervisor they run.
            //
            // Hiding it costs a delay, not a capability. A genuinely public host
            // is advertised the moment AutoNAT confirms its address — the same
            // address, by the path that filters it — and the LAN case is served
            // by mDNS (§9.8), which does not go through `identify` at all. What
            // stops being advertised is precisely the set `reachable` would have
            // refused anyway.
            let identify = identify::Behaviour::new(
                identify::Config::new(IDENTIFY_PROTOCOL.into(), key.public())
                    .with_agent_version(format!("p2p-poker/{}", env!("CARGO_PKG_VERSION")))
                    .with_hide_listen_addrs(true)
                    .with_push_listen_addr_updates(true),
            );

            Ok(OnlyDialable::new(PokerBehaviour {
                gossipsub,
                kademlia,
                ipfs_kad: Bogonless::new(ipfs_kad),
                identify,
                ping: ping::Behaviour::new(ping::Config::new()),
                // Since `libp2p` 0.57 AutoNAT's client takes any generator, so it
                // is handed this crate's own OS handle; its server wants a
                // seedable one and seeds its own from the operating system when
                // built with `Default`. No exemption from `security::rng`'s scan
                // is left (there were two).
                autonat_client: autonat::v2::client::Behaviour::new(
                    crate::security::rng::OsRng10,
                    autonat::v2::client::Config::default()
                        .with_probe_interval(Duration::from_secs(30))
                        .with_max_candidates(8),
                ),
                autonat_server: autonat::v2::server::Behaviour::default(),
                dcutr: dcutr::Behaviour::new(local_peer_id),
                relay_client,
                relay_server: relay::Behaviour::new(local_peer_id, relay_config(relay_role, &relay_admits)),
                conn_limits: connection_limits::Behaviour::new(
                    connection_limits(MAX_CONNECTIONS),
                ),
                join: request_response::Behaviour::with_codec(
                    JoinCodec,
                    [(
                        super::joinrpc::protocol(),
                        request_response::ProtocolSupport::Full,
                    )],
                    request_response::Config::default()
                        .with_request_timeout(Duration::from_millis(JOIN_RPC_TIMEOUT_MS)),
                ),
                snapshot: request_response::Behaviour::with_codec(
                    SnapshotCodec,
                    [(
                        super::snapshot::protocol(),
                        request_response::ProtocolSupport::Full,
                    )],
                    request_response::Config::default()
                        .with_request_timeout(Duration::from_millis(SNAPSHOT_RPC_TIMEOUT_MS)),
                ),
                mem_limits: libp2p::memory_connection_limits::Behaviour::with_max_percentage(0.25),
                upnp: libp2p::upnp::tokio::Behaviour::default(),
                mdns: Toggle::from(if local_discovery {
                    Some(libp2p::mdns::tokio::Behaviour::new(
                        libp2p::mdns::Config {
                        query_interval: Duration::from_millis(MDNS_QUERY_INTERVAL_MS),
                        ..Default::default()
                    },
                        local_peer_id,
                    )?)
                } else {
                    None
                }),
            }))
        })?
        .with_swarm_config(|c| {
            c.with_idle_connection_timeout(Duration::from_millis(IDLE_CONNECTION_TIMEOUT_MS))
        })
        .build();

    Ok(swarm)
}

/// The GossipSub configuration.
///
/// **`ValidationMode::Strict` and `validate_messages()` together.** The first
/// requires every message to carry a signature, a sequence number and a source;
/// the second stops the behaviour from forwarding a message before this client
/// has judged it. Without the second, a message this client is about to reject
/// has already been passed on to its mesh peers in its name — which is how one
/// spammer's advert reaches the whole lobby through honest clients.
fn build_gossipsub(
    key: &identity::Keypair,
) -> Result<gossipsub::Behaviour, Box<dyn std::error::Error + Send + Sync>> {
    let mut behaviour = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(key.clone()),
        gossipsub_config()?,
    )?;
    // `D-055`: and the score, which `NETWORK_STACK.md` §6.7 decided was on from
    // day one and which nothing ever switched on.
    let (params, thresholds) = peer_score();
    behaviour
        .with_peer_score(params, thresholds)
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
    Ok(behaviour)
}

/// `D-055`: what a peer's behaviour is worth, and the score at which this
/// client stops listening to it.
///
/// **Only what this client can prove about the peer itself.** GossipSub's
/// scoring has five terms per topic and four of them are about *traffic rate*:
/// time in the mesh, first deliveries, the shortfall below an expected delivery
/// rate (P₃) and mesh failures (P₃b). They are written for a network whose
/// topics carry a steady stream, and this one's do not -- a table advertises
/// once every thirty seconds and a quiet lobby may say nothing for minutes. The
/// library's own defaults would put **every honest peer** of such a topic below
/// the graylist threshold on P₃ alone, which is the outcome `NETWORK_STACK.md`
/// §6.7 refused to risk (OQ-5) and the reason scoring stayed off entirely.
///
/// So every rate term is zero here and the only per-topic term is **P₄, the
/// invalid message count** -- messages this client itself judged and refused,
/// which for a peer of this build is none: the refusals that are about *this
/// client's own budget* are reported as `Ignore` and never reach the score
/// (`run.rs` keeps that distinction at every verdict, and a test holds it).
/// Beside it stand the library's own defaults for the terms that are not about
/// traffic at all: the IP colocation factor and the behaviour penalty.
///
/// **The arithmetic.** P₄ is `weight × count²` with `count` decaying by
/// `invalid_message_deliveries_decay` every second, so a peer sending `r`
/// refusable messages a second settles at `count ≈ r / (1 - decay)` = `2r`, and
/// its score at `-(2r)²`. Against the library's `graylist_threshold` of -80:
///
/// | refusals a second | settled score | |
/// |---|---|---|
/// | 1 in ten seconds | -0.04 | a disagreement between honest clients costs nothing |
/// | 1 | -4 | still heard |
/// | 4.5 | -81 | **graylisted**: its RPCs are dropped unread |
/// | 50 | -10 000 | gone at once |
///
/// A graylisted peer is not disconnected -- this is not a ban, and the score
/// decays -- but nothing it sends is parsed while it stays there, which is what
/// isolates a flooder from the work it is trying to make.
pub fn peer_score() -> (gossipsub::PeerScoreParams, gossipsub::PeerScoreThresholds) {
    let mut params = gossipsub::PeerScoreParams::default();
    // Two peers of this project on one address are a bed run, not a colocation
    // attack: every measured run this project has is ten clients on one
    // machine, and the default penalty would score them all into the graylist
    // the moment a run grew past ten.
    params.ip_colocation_factor_whitelist.insert("127.0.0.1".parse().expect("a literal address"));
    params.ip_colocation_factor_whitelist.insert("::1".parse().expect("a literal address"));
    (params, gossipsub::PeerScoreThresholds::default())
}

/// `D-055`: the per-topic score for every topic this client subscribes to.
///
/// See [`peer_score`]: P₁ to P₃b are off because they are about traffic rate
/// and this network's topics are quiet; P₄ is the only term, and it counts
/// nothing but what this client refused.
pub fn topic_score() -> gossipsub::TopicScoreParams {
    gossipsub::TopicScoreParams {
        topic_weight: 1.0,
        // P1, time in the mesh: off. The quantum stays non-zero because the
        // library validates it whether or not the weight is.
        time_in_mesh_weight: 0.0,
        time_in_mesh_quantum: Duration::from_secs(1),
        time_in_mesh_cap: 0.0,
        // P2, first deliveries: off. It rewards whoever is fastest, which on a
        // topic that carries an advert every thirty seconds is whoever happens
        // to be nearest the one table that spoke.
        first_message_deliveries_weight: 0.0,
        first_message_deliveries_decay: 0.0,
        first_message_deliveries_cap: 0.0,
        // P3 and P3b, the delivery-rate shortfall: off, and this is the one
        // that matters. The library's default expects twenty deliveries a
        // window from every mesh peer and squares the shortfall; a lobby of
        // quiet tables delivers none, so every honest peer would be graylisted
        // for the crime of having nothing to say.
        mesh_message_deliveries_weight: 0.0,
        mesh_message_deliveries_decay: 0.0,
        mesh_message_deliveries_cap: 0.0,
        mesh_message_deliveries_threshold: 0.0,
        mesh_message_deliveries_window: Duration::from_secs(0),
        mesh_message_deliveries_activation: Duration::from_secs(1),
        mesh_failure_penalty_weight: 0.0,
        mesh_failure_penalty_decay: 0.0,
        // P4: what this client refused. The only thing a peer is judged on.
        invalid_message_deliveries_weight: -1.0,
        invalid_message_deliveries_decay: 0.5,
    }
}

/// The configuration, separately, so a test can read it back.
///
/// `gossipsub::Behaviour::new` consumes the config and exposes none of it, so a
/// build that merely succeeds proves nothing about the settings. Splitting it
/// out is not tidiness: the settings below are the ones that were silently
/// weakened and committed, under a doc comment that asserted them.
pub fn gossipsub_config() -> Result<gossipsub::Config, Box<dyn std::error::Error + Send + Sync>> {
    // The message id is over the content, so two peers relaying one advert
    // produce one id and the duplicate cache actually suppresses it. It must
    // NOT include the sequence number: that is per-sender, so one advert
    // relayed by two peers would be two messages and the cache would suppress
    // neither.
    let message_id_fn = |message: &gossipsub::Message| {
        let mut s = DefaultHasher::new();
        message.data.hash(&mut s);
        message.topic.hash(&mut s);
        gossipsub::MessageId::from(s.finish().to_be_bytes())
    };

    let config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(1))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .validate_messages()
        .message_id_fn(message_id_fn)
        .max_transmit_size(GOSSIP_MAX_TRANSMIT)
        .mesh_n(8)
        .mesh_n_low(6)
        .mesh_n_high(12)
        .mesh_outbound_min(3)
        .duplicate_cache_time(Duration::from_secs(120))
        // Deliberately off: flood publishing sends every message to every known
        // peer of the topic rather than to the mesh, which turns one advert into
        // a fan-out proportional to the whole lobby.
        .flood_publish(false)
        .build()?;

    Ok(config)
}

/// The relay server's limits.
///
/// See the module documentation: the library's defaults are two minutes and
/// 128 KiB, sized for hole-punch coordination, and a relayed poker table would
/// be cut off mid-hand.
///
/// When this client is not publicly reachable the limits are set to zero
/// circuits, which is the honest form of "not a relay": it does not advertise a
/// way through that it cannot provide.
///
/// A volunteer's reservations go only to the peers in `admits` ([`PokerPeersOnly`],
/// `S1-FK`), and its circuits only while the player's switch is on ([`WhileOn`]),
/// on top of the crate's own per-peer and per-address limiters.
pub fn relay_config(role: RelayRole, admits: &RelayAdmits) -> relay::Config {
    match role {
        RelayRole::Volunteer => {
            let mut c = relay::Config {
                max_reservations: 128,
                max_reservations_per_peer: 4,
                reservation_duration: RELAY_RESERVATION,
                max_circuits: RELAY_MAX_CIRCUITS,
                max_circuits_per_peer: RELAY_MAX_CIRCUITS_PER_PEER,
                max_circuit_duration: RELAY_RESERVATION,
                max_circuit_bytes: RELAY_MAX_CIRCUIT_BYTES,
                ..Default::default()
            };
            c.reservation_rate_limiters.push(Box::new(PokerPeersOnly(admits.clone())));
            c.circuit_src_rate_limiters.push(Box::new(WhileOn(admits.clone())));
            c
        }
        RelayRole::Declined => relay::Config {
            max_reservations: 0,
            max_reservations_per_peer: 0,
            max_circuits: 0,
            max_circuits_per_peer: 0,
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    /// **The bogon filter accepts everything it cannot judge, and refuses only
    /// an address that carries somebody's own network.** `S1-AC`.
    ///
    /// The table is the test. A predicate like this fails by being one range
    /// too wide — cutting the routing table down — or one range too narrow,
    /// which is the attack `NETWORK_STACK.md` §4.5 names: *"an attempt to make
    /// us scan our own LAN."*
    #[test]
    fn a_dht_record_cannot_send_this_client_to_a_private_address() {
        use super::not_a_bogon;
        let a = |s: &str| s.parse::<libp2p::Multiaddr>().expect("a literal");

        // Refused: somebody's own network, in both families.
        for bad in [
            "/ip4/192.168.1.20/tcp/4001",
            "/ip4/10.0.0.5/udp/4001/quic-v1",
            "/ip4/172.16.0.20/tcp/22",
            "/ip4/127.0.0.1/tcp/4001",
            "/ip4/169.254.1.1/tcp/4001",
            "/ip4/0.0.0.0/tcp/4001",
            "/ip4/255.255.255.255/tcp/4001",
            "/ip4/203.0.113.4/tcp/4001",     // TEST-NET-3, documentation
            "/ip4/224.0.0.1/tcp/4001",       // multicast
            "/ip4/100.64.0.1/tcp/4001",      // RFC 6598, carrier-grade NAT
            "/ip4/100.127.255.255/tcp/4001", // the top of that /10
            "/ip6/::1/tcp/4001",
            "/ip6/fd12:3456:789a:1::1/tcp/4001", // a ULA, as a home router hands out
            "/ip6/fe80::1/tcp/4001",
            "/ip6/2001:db8::1/tcp/4001",
            "/ip6/ff02::1/tcp/4001",
        ] {
            assert!(!not_a_bogon(&a(bad)), "{bad} should have been refused");
        }

        // Kept: a real address, in both families.
        for good in [
            "/ip4/1.1.1.1/tcp/4001",
            "/ip4/104.131.131.82/udp/4001/quic-v1",
            "/ip4/100.63.255.255/tcp/4001", // just below RFC 6598
            "/ip4/100.128.0.1/tcp/4001",    // just above it
            "/ip6/2606:4700:4700::1111/udp/443/quic-v1",
        ] {
            assert!(not_a_bogon(&a(good)), "{good} should have been kept");
        }

        // **Kept because it cannot be judged, which is the whole design.**
        // `run::reachable` answers *no* for these — rightly, where it is used —
        // and using it here would have cut names and the bootstrap shape out of
        // the routing table.
        for unjudgeable in [
            "/dns4/bootstrap.example.com/tcp/4001",
            "/dnsaddr/bootstrap.libp2p.io",
        ] {
            assert!(
                not_a_bogon(&a(unjudgeable)),
                "{unjudgeable} has no IP to judge and must be kept"
            );
        }

        // A relay circuit carries the relay's own address, so it is judged on
        // that — and a circuit through a LAN relay is refused, which is right:
        // a DHT record should not be sending anybody to a relay on our subnet.
        assert!(not_a_bogon(&a(
            "/ip4/147.75.87.27/tcp/4001/p2p/12D3KooWLZ18MNzm9xu7GZRQ16c6k3CZLN529CbUqrYaWNqyYko9/p2p-circuit"
        )));
        assert!(!not_a_bogon(&a(
            "/ip4/192.168.1.5/tcp/4001/p2p/12D3KooWLZ18MNzm9xu7GZRQ16c6k3CZLN529CbUqrYaWNqyYko9/p2p-circuit"
        )));
    }

    /// `S1-FJ`: the wrapped behaviour is told of the addresses this client
    /// listens on only where a stranger may learn them -- a public address or a
    /// circuit through a public relay -- and of nothing on the home network, so
    /// the provider record `libp2p-kad` fills from its listen set holds none.
    #[test]
    fn the_wrapped_dht_never_learns_a_home_address_it_listens_on() {
        use libp2p::core::transport::ListenerId;
        use libp2p::swarm::{ExpiredListenAddr, FromSwarm, NetworkBehaviour, NewListenAddr};

        /// Records every listen address it is told of, and does nothing else.
        #[derive(Default)]
        struct Heard(Vec<(bool, libp2p::Multiaddr)>);
        impl NetworkBehaviour for Heard {
            type ConnectionHandler = libp2p::swarm::dummy::ConnectionHandler;
            type ToSwarm = std::convert::Infallible;
            fn handle_established_inbound_connection(
                &mut self,
                _: libp2p::swarm::ConnectionId,
                _: PeerId,
                _: &libp2p::Multiaddr,
                _: &libp2p::Multiaddr,
            ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
                Ok(libp2p::swarm::dummy::ConnectionHandler)
            }
            fn handle_established_outbound_connection(
                &mut self,
                _: libp2p::swarm::ConnectionId,
                _: PeerId,
                _: &libp2p::Multiaddr,
                _: libp2p::core::Endpoint,
                _: libp2p::core::transport::PortUse,
            ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
                Ok(libp2p::swarm::dummy::ConnectionHandler)
            }
            fn on_swarm_event(&mut self, event: FromSwarm) {
                match event {
                    FromSwarm::NewListenAddr(e) => self.0.push((true, e.addr.clone())),
                    FromSwarm::ExpiredListenAddr(e) => self.0.push((false, e.addr.clone())),
                    _ => {}
                }
            }
            fn on_connection_handler_event(
                &mut self,
                _: PeerId,
                _: libp2p::swarm::ConnectionId,
                e: libp2p::swarm::THandlerOutEvent<Self>,
            ) {
                match e {}
            }
            fn poll(
                &mut self,
                _: &mut std::task::Context<'_>,
            ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>> {
                std::task::Poll::Pending
            }
        }

        let a = |s: &str| s.parse::<libp2p::Multiaddr>().expect("a literal");
        let mut b = super::Bogonless::new(Heard::default());
        let id = ListenerId::next();
        let home = [
            "/ip4/192.168.0.10/tcp/50390",
            "/ip4/127.0.0.1/udp/58742/quic-v1",
            "/ip4/172.20.0.1/tcp/50390",
            "/ip6/fd00::1/tcp/50391",
            "/ip6/::1/tcp/50391",
        ];
        let public = [
            "/ip4/1.1.1.1/tcp/4001",
            "/ip4/147.75.87.27/tcp/4001/p2p/12D3KooWLZ18MNzm9xu7GZRQ16c6k3CZLN529CbUqrYaWNqyYko9/p2p-circuit",
        ];
        for s in home.iter().chain(public.iter()) {
            let addr = a(s);
            b.on_swarm_event(FromSwarm::NewListenAddr(NewListenAddr { listener_id: id, addr: &addr }));
            b.on_swarm_event(FromSwarm::ExpiredListenAddr(ExpiredListenAddr { listener_id: id, addr: &addr }));
        }
        let told: Vec<String> = b.0.iter().map(|(_, x)| x.to_string()).collect();
        for s in home {
            assert!(!told.iter().any(|t| t == s), "{s} reached the DHT's listen set");
        }
        for s in public {
            assert_eq!(told.iter().filter(|t| *t == s).count(), 2, "{s}: its listening and its end both reach it");
        }
    }

    /// The counter starts at zero and the handle is shared, so a status line can
    /// read it without borrowing the swarm.
    #[test]
    fn the_filter_counts_what_it_drops() {
        use std::sync::atomic::Ordering;
        let b = super::Bogonless::new(());
        let handle = b.dropped();
        assert_eq!(handle.load(Ordering::Relaxed), 0);
        // Same allocation, not a copy: the point of handing out an Arc.
        b.dropped().fetch_add(3, Ordering::Relaxed);
        assert_eq!(handle.load(Ordering::Relaxed), 3);
    }

    /// `S1-IT`: of what a record offers for a player behind a relay, this client
    /// is handed only what it can dial -- and when the relay client then dials
    /// the relay with the ONE address its first request named, the dial is
    /// extended with every address the records gave that relay.
    ///
    /// Measured, `split190546-9`: a relay named at fifteen addresses, its
    /// WebTransport one first, the relay dialled there -- *Unsupported resolved
    /// address* -- and fifteen circuit attempts *canceled*, in five seats' logs.
    ///
    /// The breaks that must make this fail: offer the WebTransport circuit;
    /// offer the relay's dial nothing of what was learned.
    #[test]
    fn a_player_behind_a_relay_is_dialled_only_where_this_client_can_speak() {
        use libp2p::swarm::{FromSwarm, NetworkBehaviour};
        use std::sync::atomic::Ordering;

        /// Offers the same addresses for whoever is dialled, as a DHT record does.
        struct Offers(Vec<libp2p::Multiaddr>);
        impl NetworkBehaviour for Offers {
            type ConnectionHandler = libp2p::swarm::dummy::ConnectionHandler;
            type ToSwarm = std::convert::Infallible;
            fn handle_pending_outbound_connection(
                &mut self,
                _: libp2p::swarm::ConnectionId,
                _: Option<PeerId>,
                _: &[libp2p::Multiaddr],
                _: libp2p::core::Endpoint,
            ) -> Result<Vec<libp2p::Multiaddr>, libp2p::swarm::ConnectionDenied> {
                Ok(self.0.clone())
            }
            fn handle_established_inbound_connection(
                &mut self,
                _: libp2p::swarm::ConnectionId,
                _: PeerId,
                _: &libp2p::Multiaddr,
                _: &libp2p::Multiaddr,
            ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
                Ok(libp2p::swarm::dummy::ConnectionHandler)
            }
            fn handle_established_outbound_connection(
                &mut self,
                _: libp2p::swarm::ConnectionId,
                _: PeerId,
                _: &libp2p::Multiaddr,
                _: libp2p::core::Endpoint,
                _: libp2p::core::transport::PortUse,
            ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
                Ok(libp2p::swarm::dummy::ConnectionHandler)
            }
            fn on_swarm_event(&mut self, _: FromSwarm) {}
            fn on_connection_handler_event(&mut self, _: PeerId, _: libp2p::swarm::ConnectionId, e: libp2p::swarm::THandlerOutEvent<Self>) {
                match e {}
            }
            fn poll(
                &mut self,
                _: &mut std::task::Context<'_>,
            ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>> {
                std::task::Poll::Pending
            }
        }

        let a = |s: &str| s.parse::<libp2p::Multiaddr>().expect("a literal");
        let relay: PeerId = "12D3KooWLZ18MNzm9xu7GZRQ16c6k3CZLN529CbUqrYaWNqyYko9".parse().unwrap();
        let player = PeerId::random();
        let through = |leg: &str| a(&format!("{leg}/p2p/{relay}/p2p-circuit/p2p/{player}"));
        // The record as a node gave it: the relay's WebTransport address first.
        let record = vec![
            through("/ip4/147.75.87.27/udp/4001/quic-v1/webtransport"),
            through("/ip4/147.75.87.27/tcp/4001"),
            through("/dns4/relay.example.org/tcp/443/wss"),
            through("/ip4/147.75.87.27/udp/4001/quic-v1"),
            through("/ip4/192.168.1.5/tcp/4001"),
        ];
        let mut b = super::OnlyDialable::new(super::Bogonless::new(Offers(record)));
        let id = libp2p::swarm::ConnectionId::new_unchecked(1);
        let dialer = libp2p::core::Endpoint::Dialer;

        let offered = b.handle_pending_outbound_connection(id, Some(player), &[], dialer).expect("not denied");
        assert_eq!(offered, [through("/ip4/147.75.87.27/tcp/4001"), through("/ip4/147.75.87.27/udp/4001/quic-v1")]);
        assert_eq!(b.offered().undialable().load(Ordering::Relaxed), 2, "WebTransport and secure WebSocket");
        assert_eq!(b.dropped().load(Ordering::Relaxed), 1, "and the home network's address is the other filter's");
        assert!(b.offered().knows_relay(&relay) && !b.offered().knows_relay(&player));

        // The relay client's dial of the relay: one address, that of its first
        // request. It leaves here with the other one the records gave.
        b.inner.inner.0.clear();
        let given = [a("/ip4/147.75.87.27/tcp/4001")];
        let offered = b.handle_pending_outbound_connection(id, Some(relay), &given, dialer).expect("not denied");
        assert_eq!(offered, [a("/ip4/147.75.87.27/udp/4001/quic-v1")], "every address but the one the dial already has");
        // And nobody else's dial gains an address from the book.
        let offered = b.handle_pending_outbound_connection(id, Some(PeerId::random()), &[], dialer).expect("not denied");
        assert!(offered.is_empty(), "{offered:?}");
    }

    use super::*;

    fn keypair() -> identity::Keypair {
        identity::Keypair::generate_ed25519()
    }

    /// The stack builds. It is a type-state builder with a fixed phase order and
    /// a closure whose arity depends on an earlier phase, so "it compiles" is a
    /// real result rather than a formality.
    #[tokio::test]
    async fn the_node_builds() {
        let swarm = build(NodeConfig {
            identity: keypair(),
            local_discovery: false,
            relay_role: RelayRole::Declined,
            relay_admits: RelayAdmits::default(),
        })
        .expect("the stack builds");
        assert_eq!(swarm.connected_peers().count(), 0);
    }

    /// A user who declines to relay accepts nothing, whatever AutoNAT later
    /// says. That is a preference and it outranks a measurement.
    #[test]
    fn a_client_that_declined_accepts_no_reservations() {
        let off = relay_config(RelayRole::Declined, &RelayAdmits::default());
        assert_eq!(off.max_reservations, 0);
        assert_eq!(off.max_circuits, 0);
        assert_eq!(off.max_circuits_per_peer, 0);
    }

    /// `S1-FK`, D-002 point 2: a volunteer reserves a slot only for a poker
    /// client the node has named -- a stranger is refused by the first limiter
    /// that says no, whatever the crate's own limiters allow -- and a peer is
    /// admitted the moment the node adds it. Circuits keep the crate's limiters.
    #[test]
    fn a_volunteer_reserves_only_for_poker_peers() {
        let admits = RelayAdmits::default();
        let mut c = relay_config(RelayRole::Volunteer, &admits);
        let addr: libp2p::Multiaddr = "/ip4/1.1.1.1/tcp/4001".parse().expect("a literal");
        let stranger = PeerId::random();
        let ours = PeerId::random();
        admits.admit(ours);
        let now = web_time::Instant::now();
        let reserves = |c: &mut relay::Config, peer: PeerId| {
            c.reservation_rate_limiters.iter_mut().all(|l| l.try_next(peer, &addr, now))
        };
        assert!(!reserves(&mut c, stranger), "a stranger holds no reservation here");
        assert!(reserves(&mut c, ours), "a poker peer does");
        let circuit = |c: &mut relay::Config, peer: PeerId| {
            c.circuit_src_rate_limiters.iter_mut().all(|l| l.try_next(peer, &addr, now))
        };
        assert!(circuit(&mut c, stranger), "a circuit's source is not asked who it is");
        assert_eq!(
            c.circuit_src_rate_limiters.len(),
            relay::Config::default().circuit_src_rate_limiters.len() + 1,
            "circuit sources keep the crate's own limiters, and the switch"
        );
        // The player's switch, off: nothing new, for anybody (D-002 point 1).
        admits.set_on(false);
        assert!(!reserves(&mut c, ours), "no reservation, even for one of ours");
        assert!(!circuit(&mut c, ours), "and no circuit");
        admits.set_on(true);
        assert!(reserves(&mut c, ours), "on again, at once");
        admits.forget(&ours);
        assert!(!reserves(&mut c, ours), "a peer gone from this client is forgotten");
    }

    /// The relay this client offers must pass the test this client applies.
    ///
    /// The first version failed it — 600 seconds offered against an hour
    /// demanded — so every p2p-poker relay was refused by every p2p-poker
    /// client and the two halves of the NAT story cancelled out. Neither number
    /// was wrong on its own; nothing compared them.
    #[test]
    fn our_own_relay_is_one_our_own_client_would_accept() {
        let c = relay_config(RelayRole::Volunteer, &RelayAdmits::default());
        assert_eq!(
            crate::net::relay::adequate(
                crate::protocol::constants::MAX_SEATS,
                Duration::from_millis(crate::protocol::constants::HAND_DEADLINE_CAP_MS),
                Some(c.max_circuit_bytes),
                Some(c.max_circuit_duration),
            ),
            crate::net::relay::Adequacy::Adequate,
            "a relay this client would not use is a relay it should not offer"
        );
    }

    /// The library defaults are sized for hole-punch coordination — two minutes
    /// and 128 KiB — and a relayed table would be cut off mid-hand. The point of
    /// this test is that the numbers are *chosen*, so a library bump that moved
    /// the defaults cannot silently move ours.
    #[test]
    fn the_relay_limits_are_ours_and_not_the_librarys() {
        let ours = relay_config(RelayRole::Volunteer, &RelayAdmits::default());
        let theirs = relay::Config::default();

        assert!(
            ours.max_circuit_duration > theirs.max_circuit_duration,
            "the default duration is sized for a hole punch, not a hand"
        );
        assert!(
            ours.max_circuit_bytes > theirs.max_circuit_bytes,
            "the default byte budget is sized for a hole punch, not a hand"
        );
        assert_eq!(ours.max_circuit_duration, RELAY_RESERVATION);
        assert_eq!(ours.max_circuit_bytes, RELAY_MAX_CIRCUIT_BYTES);
    }

    /// Lending a stranger this client's bandwidth has to have an edge.
    #[test]
    fn a_volunteer_relay_is_still_bounded() {
        let c = relay_config(RelayRole::Volunteer, &RelayAdmits::default());
        assert!(c.max_circuits > 0 && c.max_circuits <= 256);
        assert!(c.max_circuits_per_peer <= c.max_circuits);
        assert!(c.max_reservations_per_peer <= c.max_reservations);
        assert!(
            c.max_circuit_bytes <= 64 * 1024 * 1024,
            "generous, not unbounded"
        );
    }

    /// The two lobby topics are distinct, or chat and adverts arrive on one
    /// channel and every advert parser sees chat.
    #[test]
    fn the_topics_are_distinct() {
        let t = Topics::default();
        assert_ne!(t.lobby.hash(), t.lobby_chat.hash());
        assert_eq!(t.lobby.to_string(), LOBBY_TOPIC);
    }

    /// The settings this module's documentation asserts, asserted.
    ///
    /// **This test exists because these exact values were silently changed and
    /// committed**, under the doc comment that claims them: `Strict` became
    /// `Permissive`, `validate_messages` was dropped, `flood_publish(false)` was
    /// dropped, and the per-sender sequence number was added to the message id.
    /// `gossipsub::Behaviour::new` consumes the config and exposes nothing, so
    /// the build test that existed could not have noticed any of it.
    ///
    /// Each line below is a security property and not a preference:
    ///
    /// * `Strict` requires every message to carry a signature, a source and a
    ///   sequence number. `Permissive` accepts an unsigned one.
    /// * `validate_messages` stops the behaviour forwarding a message before
    ///   this client has judged it — without it, a message this client is about
    ///   to reject has already gone to its mesh peers in its name.
    /// * `flood_publish(false)` keeps a publish to the mesh. On, it goes to
    ///   every known peer of the topic, turning one advert into a fan-out the
    ///   size of the lobby.
    #[test]
    fn the_gossip_settings_are_the_ones_the_documentation_claims() {
        let config = gossipsub_config().expect("the config builds");

        assert!(
            matches!(config.validation_mode(), gossipsub::ValidationMode::Strict),
            "Permissive accepts an unsigned message"
        );
        assert!(
            config.validate_messages(),
            "without this, a message this client is about to reject has already \
             been forwarded in its name"
        );
        assert!(
            !config.flood_publish(),
            "flood publishing turns one advert into a fan-out the size of the lobby"
        );
        assert_eq!(config.max_transmit_size(), GOSSIP_MAX_TRANSMIT);
        assert_eq!(config.mesh_n(), 8);
        assert_eq!(config.mesh_n_low(), 6);
        assert_eq!(config.mesh_n_high(), 12);
    }

    /// The message id is over the content, so two peers relaying one advert
    /// produce one id and the duplicate cache suppresses it. An id over
    /// arrival-order state would make every relayed copy a fresh message.
    #[test]
    fn one_message_has_one_id_wherever_it_arrives_from() {
        let key = keypair();
        assert!(build_gossipsub(&key).is_ok());

        // The property, stated directly on the hash the config installs.
        let id = |data: &[u8], topic: &str| {
            let mut s = DefaultHasher::new();
            data.hash(&mut s);
            gossipsub::IdentTopic::new(topic).hash().hash(&mut s);
            s.finish()
        };
        assert_eq!(id(b"advert", LOBBY_TOPIC), id(b"advert", LOBBY_TOPIC));
        assert_ne!(id(b"advert", LOBBY_TOPIC), id(b"advert", LOBBY_CHAT_TOPIC));
        assert_ne!(id(b"advert", LOBBY_TOPIC), id(b"other", LOBBY_TOPIC));
    }

    /// The transmit cap is the protocol's, not the library's example value.
    #[test]
    fn the_transmit_cap_is_the_protocols() {
        assert_eq!(GOSSIP_MAX_TRANSMIT, 65_536);
    }

    /// `S1-IZ`: the dials the wrapped behaviour asks for are known by their
    /// connection id, once each -- which is how `net::run` tells the public
    /// DHT's dials from every other behaviour's in a measured run.
    ///
    /// The break that must make this fail: a `poll` that records nothing.
    #[test]
    fn the_wrapped_dhts_dials_are_known_by_their_id() {
        use libp2p::swarm::{dial_opts::DialOpts, FromSwarm, NetworkBehaviour, ToSwarm};

        /// Asks for each dial it holds, one a poll, and does nothing else.
        struct Dials(Vec<DialOpts>);
        impl NetworkBehaviour for Dials {
            type ConnectionHandler = libp2p::swarm::dummy::ConnectionHandler;
            type ToSwarm = std::convert::Infallible;
            fn handle_established_inbound_connection(
                &mut self,
                _: libp2p::swarm::ConnectionId,
                _: PeerId,
                _: &libp2p::Multiaddr,
                _: &libp2p::Multiaddr,
            ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
                Ok(libp2p::swarm::dummy::ConnectionHandler)
            }
            fn handle_established_outbound_connection(
                &mut self,
                _: libp2p::swarm::ConnectionId,
                _: PeerId,
                _: &libp2p::Multiaddr,
                _: libp2p::core::Endpoint,
                _: libp2p::core::transport::PortUse,
            ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
                Ok(libp2p::swarm::dummy::ConnectionHandler)
            }
            fn on_swarm_event(&mut self, _: FromSwarm) {}
            fn on_connection_handler_event(
                &mut self,
                _: PeerId,
                _: libp2p::swarm::ConnectionId,
                e: libp2p::swarm::THandlerOutEvent<Self>,
            ) {
                match e {}
            }
            fn poll(
                &mut self,
                _: &mut std::task::Context<'_>,
            ) -> std::task::Poll<ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>> {
                match self.0.pop() {
                    Some(opts) => std::task::Poll::Ready(ToSwarm::Dial { opts }),
                    None => std::task::Poll::Pending,
                }
            }
        }

        let first = DialOpts::peer_id(PeerId::random()).build();
        let second = DialOpts::peer_id(PeerId::random()).build();
        let (one, two) = (first.connection_id(), second.connection_id());
        let mut b = super::Bogonless::new(Dials(vec![first, second]));
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        while b.poll(&mut cx).is_ready() {}
        assert!(b.asked_for(one));
        assert!(b.asked_for(two));
        assert!(!b.asked_for(one), "read once");
        assert!(
            !b.asked_for(DialOpts::peer_id(PeerId::random()).build().connection_id()),
            "a dial it never asked for is not its"
        );
    }

    /// `S1-IZ`: the public DHT bootstraps on its own once an hour, not at
    /// libp2p's five minutes -- and only a binary built to be measured can be
    /// told another interval.
    ///
    /// The breaks that must make this fail: the library's default back; the
    /// knob read in a player's build.
    #[test]
    fn the_public_dht_bootstraps_once_an_hour() {
        assert_eq!(PUBLIC_BOOTSTRAP_EVERY, Some(Duration::from_secs(3_600)));
        let src = include_str!("swarm.rs");
        let code = &src[..src.find("\n#[cfg(test)]\nmod tests {").expect("the tests")];
        assert!(code.contains("ipfs_cfg.set_periodic_bootstrap_interval(public_bootstrap_every());"), "the interval is set");
        let knob = code.find("fn public_bootstrap_every() -> Option<Duration> {").expect("the knob");
        assert_eq!(
            code[knob..].lines().nth(1).map(str::trim),
            Some("#[cfg(feature = \"fault-harness\")]"),
            "only a measured build is told another interval"
        );
    }
    /// `D-055`: the score judges a peer on what this client refused of it, and
    /// on nothing about how much it talks.
    ///
    /// Every rate term is zero **on purpose**. The library's defaults expect
    /// twenty deliveries a window from each mesh peer and square the shortfall
    /// (P3); a lobby of quiet tables delivers none, so the defaults would
    /// graylist every honest peer for having nothing to say -- which is why
    /// `NETWORK_STACK.md` §6.7 left scoring off rather than risk it (OQ-5).
    #[test]
    fn the_peer_score_counts_refusals_and_not_traffic() {
        let p = topic_score();
        assert_eq!(p.time_in_mesh_weight, 0.0, "P1 off");
        assert_eq!(p.first_message_deliveries_weight, 0.0, "P2 off");
        assert_eq!(p.mesh_message_deliveries_weight, 0.0, "P3 off: the one that would graylist a quiet honest peer");
        assert_eq!(p.mesh_failure_penalty_weight, 0.0, "P3b off");
        assert!(p.invalid_message_deliveries_weight < 0.0, "P4 is the whole of it");
        p.validate().expect("the library accepts it");

        let (params, thresholds) = peer_score();
        params.validate().expect("the library accepts the peer params");
        thresholds.validate().expect("and the thresholds");
        assert!(
            params.ip_colocation_factor_whitelist.contains(&"127.0.0.1".parse().unwrap()),
            "a bed run is ten clients on one machine, not a colocation attack"
        );

        // The arithmetic the doc comment claims: a peer sending `r` refusable
        // messages a second settles at a count of `r / (1 - decay)` and scores
        // `weight * count^2`. Graylisting takes a stream, not a disagreement.
        let settled = |r: f64| {
            let count = r / (1.0 - p.invalid_message_deliveries_decay);
            p.topic_weight * p.invalid_message_deliveries_weight * count * count
        };
        assert!(settled(0.1) > thresholds.gossip_threshold,
            "one refusal in ten seconds costs an honest neighbour nothing: {}", settled(0.1));
        assert!(settled(1.0) > thresholds.graylist_threshold,
            "and one a second is still heard: {}", settled(1.0));
        assert!(settled(8.0) < thresholds.graylist_threshold,
            "a flooder is graylisted: {}", settled(8.0));
    }

}
