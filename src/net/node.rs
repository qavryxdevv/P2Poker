//! The node's event loop: listen, announce, discover, dial, gossip.
//!
//! This is where [`swarm`](super::swarm)'s behaviours, [`dht`](super::dht)'s
//! global discovery and [`lobby`](super::lobby)'s admission rules are driven.
//! It is transport plumbing and decides no poker rule.
//!
//! # The two-step that D-003 asks for
//!
//! The acceptance criterion is that a client anywhere on the internet sees the
//! open tables, and it takes both discovery layers to get there:
//!
//! 1. **Kademlia** answers *who is there*. Every client announces itself a
//!    provider of one agreed key on the public libp2p DHT and asks who else is,
//!    and the record carries whatever addresses that client has — including a
//!    circuit through a relay, which is what a player behind a NAT has instead
//!    of an address of their own.
//! 2. **The handshake** settles who is actually there. A dial either completes
//!    a Noise or TLS handshake with a peer that speaks `/p2p-poker/1`, or it
//!    does not, and only then does the peer join the GossipSub mesh where
//!    adverts live.
//!
//! Most dials in step 2 fail, and that is the expected case rather than an
//! error: the provider list is a hint, and D-003 is satisfied by the ones that
//! succeed.
//!
//! # Why the loop owns the stores
//!
//! [`LobbyStore`] and [`RateLimiter`] are per-node state that only this loop
//! writes. Handing them out behind a lock would buy nothing — every write is on
//! this task — and would make the ordering of two writes a question somebody
//! could get wrong.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use libp2p::{gossipsub, Multiaddr, PeerId};

use super::lobby::{LobbyStore, RateLimiter};
use crate::protocol::constants::AD_REBROADCAST_MS;

/// What the interface asks the node to do.
///
/// One direction only. The paint loop never touches the swarm and the swarm
/// never touches a widget — `SPEC_CS.md` §33 — so everything the player does
/// arrives here as a value and everything the node learns goes back as a
/// [`NodeEvent`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeCommand {
    /// Say something in the lobby.
    ///
    /// The text is whatever was typed. It is trimmed and capped where it is
    /// sealed, not here: one place decides what fits on the wire.
    SayInLobby(String),
    /// Found a table and advertise it.
    CreateTable {
        /// Which game. A Sit-and-Go reads `seats` and `name` and nothing else
        /// — its structure is settled, and whether it carries the rated name
        /// follows from the seat count rather than from a separate choice.
        kind: crate::net::lobby::TableKind,
        name: String,
        seats: u8,
        min_players: u8,
        buyin: u64,
        password: Option<Vec<u8>>,
    },
    /// Ask to sit down at a table this client has seen advertised.
    JoinTable {
        /// The table key, which is the table's identity.
        key: [u8; 32],
        buyin: u64,
        seat: Option<u8>,
        password: Option<Vec<u8>>,
    },
    /// The name this client sits down under.
    ///
    /// A command rather than a parameter to `run`, because a player may change
    /// it while the node is running — and the node must have it, since it is
    /// the node that builds a `JOIN_REQUEST` and a roster entry.
    SetNickname(String),
    /// Stop hosting or stop waiting. Formation only; leaving a table that has
    /// started is a `PLAYER_LEAVE` and is not this.
    LeaveTable,
}

/// What the loop reports upwards, for the GUI and the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeEvent {
    /// A transport address this node is reachable on.
    Listening(Multiaddr),
    /// Somebody joined or left the network this client is on.
    ///
    /// **Any** peer: since this client speaks to the public libp2p DHT there are
    /// several hundred of them and almost none is a poker client. They are
    /// counted and never written to the log — a line each was hundreds a minute,
    /// and it buried everything a player might actually want to read.
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    /// A peer that turned out to be another **poker** client.
    ///
    /// Told apart by `identify`: the protocol version this client announces is
    /// its own, so a peer answering with it is running this software. That is
    /// what a player means by "peers", and it is what the header counts.
    PokerPeer { peer: PeerId, gone: bool },
    /// The DHT returned addresses. Most will not be poker clients.
    Discovered { hints: usize, dropped: usize },
    /// An advert was accepted into the lobby.
    ///
    /// The whole record travels, not just the key.
    ///
    /// It used to be the key alone, and the consequence was quiet and total:
    /// the interface keeps its own [`LobbyStore`](super::lobby::LobbyStore) and
    /// nothing ever put anything in it, so the table list was empty on every
    /// client no matter how many tables were being advertised. A key with no
    /// record is a row the list cannot draw.
    TableSeen {
        key: [u8; 32],
        /// Boxed: this is by far the largest variant, and every other event
        /// would otherwise be as big as this one.
        ad: Box<super::lobby::TableAd>,
        params_hash: [u8; 32],
        advert_hash: [u8; 32],
    },
    /// An advert was refused, with the reason as text for the log.
    TableRefused { reason: String },
    /// AutoNAT decided.
    Reachability { public: bool },
    /// The router opened a port, or would not.
    ///
    /// **Not** a statement about reachability. A router behind a carrier NAT
    /// confirms a mapping happily while the port stays shut from outside, so
    /// this says only that the door was opened — AutoNAT is still the only
    /// thing that may decide whether anybody can walk through it.
    PortMapped { how: &'static str, external: u16 },
    /// This client is hosting a table under this key.
    Hosting { key: [u8; 32] },
    /// The parameters of the table this client is at.
    ///
    /// Carried rather than looked up. The obvious source is this client's own
    /// lobby store, and for a founder it is the wrong one: a founder's table
    /// enters its own lobby only once the network has taken the advertisement,
    /// which is right for the *list* and useless for the table window — for the
    /// first thirty seconds, or for as long as this client is alone, the founder
    /// would be sitting at a table whose blinds and seat count it displayed as
    /// defaults it made up.
    TableParams {
        key: [u8; 32],
        name: String,
        seats: u8,
        needed: u8,
        small_blind: u64,
        big_blind: u64,
    },
    /// This client has a seat at a table being formed.
    Seated { key: [u8; 32], seat: u8 },
    /// The roster this client currently believes, newest first in the list.
    Roster {
        key: [u8; 32],
        seats: Vec<(u8, String, u64)>,
    },
    /// Every seat has ratified: the table is real and has a session identity.
    TableReal { key: [u8; 32], session: [u8; 32] },
    /// A hand has begun: every seat agreed on the same `HAND_INIT`.
    ///
    /// The first thing that crosses the wire after `session_id`, and the first
    /// time the table window has anything but "no hand in progress" to say.
    HandBegan {
        hand_id: u64,
        button: u8,
        dealt_in: Vec<u8>,
    },
    /// A hand is waiting for these seats to say the same thing this client did.
    ///
    /// Named rather than left as a spinner: "waiting for seat 3" is something a
    /// player can act on and a turning circle is not.
    HandWaiting { hand_id: u64, seats: Vec<u8> },
    /// How far the deck has got.
    ///
    /// Sent whenever the answer changes and not on every event, because the
    /// shuffle chain is `2m` stages and a player watching a table does not need
    /// to be told the same thing twice. `shuffling` is the seat whose turn it
    /// is; `ready` is whether the chain has closed and the deck is final.
    DeckProgress {
        hand_id: u64,
        shuffling: Option<u8>,
        ready: bool,
    },
    /// The founder refused. **Advisory** — a founder may lie, so the reason is
    /// carried as the claim it is.
    JoinRefused { reason: u16 },
    /// Formation was abandoned, either by this client or by a rule.
    LeftTable { why: String },
    /// Something in the transport went wrong and the node carried on.
    ///
    /// Separate from [`TableRefused`](NodeEvent::TableRefused), which is a
    /// finding about somebody else's advert. Reporting a failed DHT announce as
    /// a refused table would have told a user that a peer misbehaved when in
    /// fact this node had not finished starting up.
    Warning(String),
    /// This client's own record is in the public lobby.
    ///
    /// It used to carry the port announced to Mainline. There is no port now:
    /// a provider record carries multiaddrs, which is the whole reason for the
    /// change — a player behind a NAT has no port worth announcing and does
    /// have a circuit address.
    Announced,
    /// A dial did not complete.
    ///
    /// Reported rather than discarded, because *most dials fail* is a true
    /// statement about an open DHT and an indistinguishable statement about a
    /// node that is quietly broken. Without this line the two look identical in
    /// the log — which is how the first run of this binary looked like it was
    /// working.
    DialFailed { reason: String },
    /// A peer was found on the local network.
    LocalPeer(PeerId),
    /// The clock moved on, so adverts that have run out can go.
    ///
    /// The interface keeps its **own** copy of the lobby, and until this event
    /// existed that copy was swept only when a new advertisement arrived. When
    /// the last table's founder went away, nothing arrived — so the row for a
    /// table that no longer existed stayed on the screen until the client was
    /// restarted. Expiry is a function of time passing, and time passing has to
    /// be an event or it is not noticed.
    Swept { now_ms: u64 },
    /// Somebody is in the lobby, under this name.
    ///
    /// `who` is the player key and is the identity; `nickname` is decoration
    /// and two players may choose one. The interface shows both.
    LobbyHere { who: [u8; 32], nickname: String },
    /// Somebody said something in the lobby.
    LobbySaid {
        who: [u8; 32],
        nickname: String,
        text: String,
    },
    /// A peer was found in the public lobby, through the DHT.
    ///
    /// Separate from [`LocalPeer`](NodeEvent::LocalPeer) on purpose. The two
    /// paths are meant to be independent and one of them is much easier: on a
    /// single network multicast answers in a second, so a lobby that only ever
    /// worked over multicast would look exactly like one that worked. Saying
    /// which road a player arrived by is what makes the difference visible in
    /// an ordinary run instead of only in a test with multicast turned off.
    LobbyPeer(PeerId),
    /// A peer subscribed to a topic this node also holds.
    ///
    /// Worth its own line: until one does, `publish` returns
    /// `NoPeersSubscribedToTopic` and a table this node is offering reaches
    /// nobody. That is the ordinary state at start-up and is indistinguishable,
    /// in a log without this event, from an advert that is silently broken.
    MeshPeer(PeerId),
    /// This node's own advert went out.
    Published { bytes: usize },
    /// A relay accepted a reservation, and what it will carry.
    ///
    /// The limits are **read**, not assumed: the protocol reports the server's
    /// real ones back precisely so the decision can be made before committing,
    /// and `NAT_AND_DISCOVERY.md` says in so many words to use them.
    Reserved {
        relay: PeerId,
        bytes: Option<u64>,
        seconds: Option<u64>,
        adequate: bool,
    },
    /// No relay could be found, after looking for long enough that this is a
    /// finding rather than impatience.
    ///
    /// Reported because the alternative is a lobby listing tables nobody can sit
    /// at and a user who concludes the software is broken. If nobody anywhere is
    /// publicly reachable there is no game, and a client in that position has to
    /// say so.
    NoRelayFound { cycles: u32 },
    /// A relayed connection was upgraded to a direct one by hole punching.
    HolePunched(PeerId),
    /// Hole punching gave up. The connection stays relayed, which is a normal
    /// steady state and not an error — upstream allows three attempts and then
    /// stops.
    StillRelayed(PeerId),
}

impl NodeEvent {
    /// Whether this changes anything on screen other than a line in the log.
    ///
    /// The window is repainted when the node speaks, and on a machine with no
    /// graphics driver a repaint is not cheap: measured, one full redraw of the
    /// lobby costs about half a second of processor time under WARP against
    /// four milliseconds on a graphics card. The node says something two or
    /// three times a second — mDNS answers, DHT hints, dials that failed — and
    /// almost all of it is a log line nobody is reading at that instant.
    ///
    /// So the caller waits before repainting for those, and repaints at once
    /// for the rest. A seat filling, a roster ratifying, a hand starting: those
    /// are what a player is looking at, and they must never be held back.
    ///
    /// **Exhaustive on purpose — no wildcard.** A new variant will not compile
    /// until somebody decides which side it is on, which is the only way this
    /// stays true as the protocol grows. A wildcard here would silently make
    /// every future event slow, and the one that mattered would be the hand.
    pub fn changes_more_than_the_log(&self) -> bool {
        match self {
            // The table, and everything a player watches while sitting at it.
            Self::Hosting { .. }
            | Self::TableParams { .. }
            | Self::Seated { .. }
            | Self::Roster { .. }
            | Self::TableReal { .. }
            | Self::HandBegan { .. }
            | Self::HandWaiting { .. }
            | Self::DeckProgress { .. }
            | Self::JoinRefused { .. }
            | Self::LeftTable { .. }
            // The lobby list and the counters above it.
            | Self::TableSeen { .. }
            | Self::PokerPeer { .. }
            // The status line, which is a claim about whether this client can
            // play at all.
            | Self::Reachability { .. }
            | Self::Reserved { .. }
            | Self::NoRelayFound { .. }
            // Both change a pane a player is looking at, and a line of chat
            // that arrived three seconds ago is a line nobody answers.
            | Self::LobbyHere { .. }
            | Self::LobbySaid { .. } => true,

            // Log only. Chatty, repetitive, and worth a second's delay.
            Self::Listening(_)
            | Self::Discovered { .. }
            | Self::TableRefused { .. }
            | Self::PortMapped { .. }
            | Self::Warning(_)
            | Self::Announced { .. }
            | Self::DialFailed { .. }
            | Self::LocalPeer(_)
            | Self::LobbyPeer(_)
            // Counted only, and the count is not on the header.
            | Self::PeerConnected(_)
            | Self::PeerDisconnected(_)
            // Log only — and lazily on purpose. A sweep that removes nothing
            // must not cost a repaint, and one that removes a row can wait the
            // fraction of a second the lazy wake takes.
            | Self::Swept { .. }
            | Self::MeshPeer(_)
            | Self::Published { .. }
            | Self::HolePunched(_)
            | Self::StillRelayed(_) => false,
        }
    }
}

/// How often to re-publish this node's own adverts.
///
/// Half the advert TTL, so one lost re-broadcast does not expire a table. That
/// relationship is asserted at compile time in `protocol::constants` rather than
/// left to two numbers that happen to agree today.
pub const REBROADCAST: Duration = Duration::from_millis(AD_REBROADCAST_MS);

/// Milliseconds since the Unix epoch, or `0` if the clock is before it.
///
/// A local view and never canonical state (D-012): two honest peers may read
/// different values, so nothing derived from this may enter a hash.
pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The per-node state the loop owns.
pub struct NodeState {
    pub lobby: LobbyStore,
    pub limits: RateLimiter,
    /// Whether AutoNAT has confirmed this node is reachable, which is what D-002
    /// gates relay volunteering on.
    public: bool,

}

impl Default for NodeState {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeState {
    pub fn new() -> Self {
        NodeState {
            lobby: LobbyStore::new(),
            limits: RateLimiter::new(),
            public: false,
        }
    }

    pub fn set_public(&mut self, public: bool) {
        self.public = public;
    }

    pub fn is_public(&self) -> bool {
        self.public
    }

    /// Periodic housekeeping: expire adverts, forget idle rate windows.
    ///
    /// Returns how many tables were dropped.
    pub fn tick(&mut self, now_ms: u64) -> usize {
        self.limits.sweep(now_ms);
        self.lobby.expire(now_ms)
    }
}

/// Whether a GossipSub message is one this node should even parse.
///
/// Size first, because it is free and the cap is the protocol's. A message over
/// the cap cannot be a conforming one, so there is nothing to gain by looking
/// inside it.
pub fn worth_parsing(message: &gossipsub::Message, cap: usize) -> bool {
    message.data.len() <= cap
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::constants::{AD_TTL_MS, LOBBY_MSG_MAX};


    /// The size gate is free and the cap is the protocol's, so it runs before
    /// anything looks inside the message.
    #[test]
    fn an_oversized_message_is_not_parsed() {
        let msg = |len: usize| gossipsub::Message {
            source: None,
            data: vec![0u8; len],
            sequence_number: None,
            topic: gossipsub::IdentTopic::new("t").hash(),
        };
        assert!(worth_parsing(&msg(LOBBY_MSG_MAX), LOBBY_MSG_MAX));
        assert!(!worth_parsing(&msg(LOBBY_MSG_MAX + 1), LOBBY_MSG_MAX));
        assert!(worth_parsing(&msg(0), LOBBY_MSG_MAX));
    }

    /// One lost re-broadcast must not expire a table, which is why the interval
    /// is half the TTL and not merely shorter than it.
    #[test]
    fn a_lost_rebroadcast_does_not_expire_a_table() {
        assert!(
            REBROADCAST.as_millis() as u64 * 2 <= AD_TTL_MS,
            "two intervals must still fit inside the TTL"
        );
    }

    /// Housekeeping is one call, so no caller can do half of it.
    #[test]
    fn the_tick_expires_and_sweeps_together() {
        let mut state = NodeState::new();
        for i in 0..10u32 {
            let mut peer = [0u8; 32];
            peer[..4].copy_from_slice(&i.to_be_bytes());
            state.limits.admit_peer(peer, 0);
        }
        assert_eq!(state.limits.tracked().0, 10);

        state.tick(200_000);
        assert_eq!(state.limits.tracked(), (0, 0), "idle windows forgotten");
    }

    /// D-002's gate. It starts closed and is opened by measurement, never by a
    /// setting: a user who is wrong about their own NAT would otherwise
    /// advertise a way through that is not one.
    #[test]
    fn reachability_starts_closed() {
        let mut state = NodeState::new();
        assert!(!state.is_public(), "assume nothing until AutoNAT answers");
        state.set_public(true);
        assert!(state.is_public());
    }

    /// The clock is a local view and never canonical state (D-012). This test
    /// exists to hold the shape of that claim: the function returns a plain
    /// number that nothing hashes.
    #[test]
    fn the_clock_is_a_local_view() {
        let a = now_unix_ms();
        let b = now_unix_ms();
        assert!(b >= a);
        assert!(a > 1_600_000_000_000, "and it is a real wall clock");
    }
}

#[cfg(test)]
mod wake_tests {
    use super::*;

    /// The two kinds, named by hand rather than derived, so this test disagrees
    /// with the code when somebody changes the code's mind.
    #[test]
    fn a_seat_filling_is_not_a_log_line() {
        let peer = libp2p::PeerId::random();
        let now = [
            NodeEvent::Seated { key: [0; 32], seat: 3 },
            NodeEvent::Roster { key: [0; 32], seats: vec![] },
            NodeEvent::TableReal { key: [0; 32], session: [1; 32] },
            // Another poker client is a change to the header. A stranger on
            // the DHT is not, and there are several hundred of those.
            NodeEvent::PokerPeer { peer, gone: false },
            NodeEvent::Reachability { public: true },
        ];
        for e in now {
            assert!(e.changes_more_than_the_log(), "{e:?} must not be held back");
        }

        let later = [
            NodeEvent::PeerConnected(peer),
            NodeEvent::PeerDisconnected(peer),
            NodeEvent::LocalPeer(peer),
            NodeEvent::MeshPeer(peer),
            NodeEvent::DialFailed { reason: "no route".into() },
            NodeEvent::Discovered { hints: 68, dropped: 0 },
            NodeEvent::Warning("noise".into()),
        ];
        for e in later {
            assert!(!e.changes_more_than_the_log(), "{e:?} is only a log line");
        }
    }
}
