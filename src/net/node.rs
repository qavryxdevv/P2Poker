//! The node's event loop: listen, announce, discover, dial, gossip.
//!
//! This is where [`swarm`](super::swarm)'s behaviours, [`run`](super::run)'s
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
    /// `D-043`: make this slot the active table -- the one an `Act`, a
    /// `SayAtTable` or a `LeaveTable` is for. The window sends it when the
    /// player turns to another of their tables.
    Focus(u8),
    /// Act on this client's own turn.
    ///
    /// The action is the player's, and it is checked twice: once here by the
    /// hand's own engine before anything is sealed, and again by every receiver
    /// against its own. The two are the same code, so a button that offered
    /// something illegal would fail at the first of them rather than reaching
    /// the wire.
    Act(crate::poker::actions::Action),
    /// Say something in the lobby.
    ///
    /// The text is whatever was typed. It is trimmed and capped where it is
    /// sealed, not here: one place decides what fits on the wire.
    SayInLobby(String),
    /// `S1-CS`: say something to the seats of this client's table, over
    /// the table's own group and nowhere else.
    SayAtTable(String),
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
    /// `S1-CR`: rejoin the unfinished session on record -- the node puts the
    /// recorded advert back on offer and says `TableSeen`; the caller then
    /// sits down at it with `JoinTable` like at any other table.
    ResumeSession,
    /// `S1-CR`: the unfinished session on record is not wanted; forget it.
    ForgetSession,
}

/// What the loop reports upwards, for the GUI and the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeEvent {
    /// `D-043`: what follows is about this table -- the slot's number, stable
    /// for as long as the client sits there, and the table's key once it has
    /// one. Sent whenever the node turns from one slot to another, before
    /// that slot's events; the window keeps one state per slot and routes by
    /// the last of these.
    AtTable { slot: u8, key: Option<[u8; 32]> },
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
    /// the interface keeps its own [`super::lobby::LobbyStore`] and
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
    /// A table this client listed is gone before its advert expired: its
    /// founder answered a lobby question (§7.5, D-040) without it.
    TableGone { key: [u8; 32], why: String },
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
        /// `S1-CS`: how long a seat has to decide, as the player sees it --
        /// the advert's `action_timeout_ms`; the grace and the time bank run
        /// on beyond it.
        action_ms: u64,
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
    /// `S1-CS`: the table as the engine has it after every action -- what
    /// each seat has behind and in front of it, the pot, the street, who
    /// has folded and whose turn it is. Sent whenever any of it changes,
    /// before the turn is announced, so the window never draws a turn on
    /// a table it has not been told about.
    TableState {
        hand_id: u64,
        street: u16,
        pot: u64,
        to_act: Option<u8>,
        stacks: Vec<u64>,
        bets: Vec<u64>,
        folded: Vec<bool>,
    },
    /// It is this client's turn, and this is what it may do.
    ///
    /// Sent when the answer changes and not on every event: the betting is a
    /// stage per action, and a window redrawing on each one would flicker
    /// through states nobody acted in.
    YourTurn {
        hand_id: u64,
        street: u16,
        to_call: u64,
        pot: u64,
        can_check: bool,
        can_call: bool,
        can_bet: bool,
        can_raise: bool,
        min_raise_to: u64,
        max_raise_to: u64,
        /// `D-034`: how long ago the turn was given, on the clock of the
        /// seat that gave it -- the window's countdown starts that far in,
        /// so a late delivery does not add to the thirty seconds.
        elapsed_ms: u64,
    },
    /// Somebody else is to act, or nobody is. `elapsed_ms` as for
    /// [`YourTurn`](NodeEvent::YourTurn): how long that seat has had it.
    NotYourTurn { hand_id: u64, seat: Option<u8>, elapsed_ms: u64 },
    /// A street opened and these are the cards on it.
    Board { hand_id: u64, cards: Vec<u8> },
    /// The hand is over.
    HandEnded {
        hand_id: u64,
        /// Final stacks by seat.
        stacks: Vec<u64>,
        /// What each seat showed, by seat; `None` for folded and mucked.
        shown: Vec<Option<[u8; 2]>>,
    },
    /// This client's own two cards, opened from a complete set of verified
    /// shares.
    ///
    /// It reaches the interface and goes no further: the channel is inside one
    /// process, and the whole point of the deal is that these two bytes exist
    /// here and nowhere else on the network. `card` values are deck indices in
    /// `0..=51` — the interface turns them into a rank and a suit.
    HoleCards { hand_id: u64, cards: [u8; 2] },
    /// Somebody's hole cards are dealt but not this client's to see.
    ///
    /// Sent so a table can draw backs at the other seats. It carries no card,
    /// because there is none to carry: those cards are one share short here and
    /// will stay that way unless their owner shows.
    CardsDealt { hand_id: u64, seats: Vec<u8> },
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
    /// **How many other seats this client can actually hear (`S1-BH`).**
    ///
    /// Emitted from housekeeping, which runs whatever else is happening — the
    /// sentence that says the same thing today lives inside `hand_one_may_open`
    /// behind an `!ever_dealt` gate, so it stops being said at the exact moment
    /// a mid-table silence would matter, and it is a `Warning`, which
    /// `is_advisory` marks droppable.
    ///
    /// **Advisory, and it has to be.** A client that has heard nobody must be
    /// able to say so rather than telling its user it is playing — measured, a
    /// seat that never entered the group was certified out and exited printing
    /// `TABLE FORMED seats=10` after 900 s at a table it had been removed from.
    /// But this arrives every housekeeping tick and the verdict needs four
    /// ticks of silence, so no single reading is load-bearing, and a
    /// non-advisory send on a full channel stops the whole `select!` — see
    /// `Events::send`.
    Carrier { seen: u16, want: u16 },
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
    /// `S1-CS`: a seat of this client's table said something. `seat` is
    /// what the window files it under and what a mute names.
    TableSaid {
        seat: u8,
        nickname: String,
        text: String,
    },
    /// `S1-CS`: how a seat's connection is doing -- the last ping's
    /// round trip, or `None` when its last connection closed -- and, `D-041`,
    /// whether the table's group holds the seat as a confirmed member right
    /// now. On a Tox table the group carries the hand, so `group` is the
    /// reading that says *on the line*; the ping is a figure beside it that a
    /// seat reached only through a relay never answers.
    SeatLink { seat: u8, rtt_ms: Option<u64>, group: bool },
    /// `D-035`: a seat's client left the table's group -- on purpose
    /// (`quit`) or by timing out. The seat is shown gone; heads-up a quit
    /// ends the game.
    SeatLeft { seat: u8, quit: bool },
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
    /// `S1-CR`: a session record was found at start. The window asks the
    /// player; a headless client answers with `ResumeSession` under
    /// `--resume` and otherwise says the record is there.
    UnfinishedSession {
        key: [u8; 32],
        table_name: String,
        seat: u8,
        stack: u64,
        hand_id: u64,
    },
    /// `S1-CR`: the rejoin opened a hand of the running table from the
    /// members' copies and follows it.
    SessionResumed { hand_id: u64 },
    /// `S1-CR`: the rejoin stopped and the record is gone.
    SessionGaveUp { why: String },
}

impl NodeEvent {
    /// Whether losing this event costs a line of log rather than correctness.
    ///
    /// **This is what keeps the node's own clock out of the hands of whatever
    /// is drawing its log.** Every event used to leave with `send().await`, so
    /// a full channel stopped the node's whole `select!` — its timers with it.
    /// Measured on a busy DHT: the thirty-second housekeeping tick did not fire
    /// for three minutes and the table advertisement went out once in a
    /// two-hundred-second run, which is a table that takes minutes to form for
    /// reasons that have nothing to do with the network.
    ///
    /// The division is by consequence, not by importance. Everything here is a
    /// rendering of state the node already holds: drop one and a line is
    /// missing. Everything not here is something the receiver's own fold
    /// depends on — a seat, a roster, a card, a turn — and losing one would
    /// leave the interface describing a hand that is not being played.
    pub fn is_advisory(&self) -> bool {
        matches!(
            self,
            NodeEvent::Listening(_)
                | NodeEvent::PeerConnected(_)
                | NodeEvent::PeerDisconnected(_)
                | NodeEvent::PokerPeer { .. }
                | NodeEvent::Discovered { .. }
                | NodeEvent::DialFailed { .. }
                | NodeEvent::LobbyPeer(_)
                | NodeEvent::LocalPeer(_)
                | NodeEvent::Reachability { .. }
                | NodeEvent::PortMapped { .. }
                | NodeEvent::Announced
                | NodeEvent::Swept { .. }
                | NodeEvent::TableRefused { .. }
                | NodeEvent::Warning(_)
                // **A periodic reading, and dropping one costs nothing.**
                //
                // This was NOT advisory at first, on the reasoning that what a
                // client tells its user must not be droppable. That is right
                // for a one-shot event and wrong for this one: it arrives every
                // housekeeping tick with the same value, and the verdict it
                // feeds needs `DEAF_MS` = four ticks of silence to mature, so a
                // dropped reading delays nothing that can be seen. Waiting for
                // it, on the other hand, is what the comment on `Events::send`
                // warns about in so many words — a non-advisory send on a full
                // channel stops the whole `select!`, timers included, and that
                // has already cost this project a run where housekeeping did
                // not fire for three minutes.
                | NodeEvent::Carrier { .. }
        )
    }
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
            Self::AtTable { .. }
            | Self::Hosting { .. }
            | Self::TableParams { .. }
            | Self::Seated { .. }
            | Self::Roster { .. }
            | Self::TableReal { .. }
            | Self::HandBegan { .. }
            | Self::TableState { .. }
            | Self::HandWaiting { .. }
            | Self::DeckProgress { .. }
            | Self::HoleCards { .. }
            | Self::CardsDealt { .. }
            | Self::YourTurn { .. }
            | Self::NotYourTurn { .. }
            | Self::Board { .. }
            | Self::HandEnded { .. }
            | Self::JoinRefused { .. }
            | Self::LeftTable { .. }
            // The lobby list and the counters above it.
            | Self::TableSeen { .. }
            | Self::TableGone { .. }
            | Self::PokerPeer { .. }
            // The status line, which is a claim about whether this client can
            // play at all.
            | Self::Reachability { .. }
            | Self::Reserved { .. }
            | Self::NoRelayFound { .. }
            // Both change a pane a player is looking at, and a line of chat
            // that arrived three seconds ago is a line nobody answers.
            | Self::LobbyHere { .. }
            | Self::LobbySaid { .. }
            | Self::TableSaid { .. }
            | Self::SeatLink { .. }
            | Self::SeatLeft { .. }
            // `S1-CR`: the question about an unfinished game, and its answer.
            | Self::UnfinishedSession { .. }
            | Self::SessionResumed { .. }
            | Self::SessionGaveUp { .. } => true,

            // Log only. Chatty, repetitive, and worth a second's delay.
            //
            // The carrier reading arrives on every housekeeping tick with the
            // same value nearly every time, and the verdict it feeds -- whether
            // this client is deaf to its table -- takes DEAF_MS to mature, so a
            // tick's delay in repainting it changes nothing a player can see.
            Self::Carrier { .. }
            | Self::Listening(_)
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

/// The node's end of the event channel, with a rule about waiting.
///
/// **The node must not block on its own log.** Every event used to go out with
/// `send().await`, and a full channel stops the whole `select!` — timers
/// included. Measured on a busy DHT: the housekeeping tick did not fire for
/// three minutes and the table advertisement went out once in a
/// two-hundred-second run, so a third player who missed that one publish waited
/// the rest of the run to see the table.
///
/// So an advisory event is offered and dropped if there is no room, and only an
/// event the receiver's fold depends on is waited for. Nothing is dropped
/// silently: the count is carried and reported, because "no warnings" and "the
/// warnings were thrown away" must not look the same.
#[derive(Clone)]
pub struct Events {
    tx: tokio::sync::mpsc::Sender<NodeEvent>,
    dropped: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl Events {
    pub fn new(tx: tokio::sync::mpsc::Sender<NodeEvent>) -> Self {
        Events {
            tx,
            dropped: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Offered or waited for, by [`NodeEvent::is_advisory`].
    pub async fn send(&self, event: NodeEvent) -> Result<(), ()> {
        if event.is_advisory() {
            if self.tx.try_send(event).is_err() {
                self.dropped
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            return Ok(());
        }
        self.tx.send(event).await.map_err(|_| ())
    }

    /// Resolves when every receiver is gone, so a background task that only
    /// reports can stop when there is nobody left to report to.
    pub async fn closed(&self) {
        self.tx.closed().await
    }

    /// How many advisory events have been dropped, taken rather than read.
    pub fn take_dropped(&self) -> u64 {
        self.dropped
            .swap(0, std::sync::atomic::Ordering::Relaxed)
    }
}
