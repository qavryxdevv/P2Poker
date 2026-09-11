//! Wiring between the GUI, the protocol and the engine.
//!
//! Owns the worker that parses, verifies, proves and applies, so that
//! `SPEC_CS.md` §33 holds: **cryptography never blocks the GUI event loop.**
//!
//! # The shape that keeps that true
//!
//! The node runs on a tokio task and pushes [`NodeEvent`]s down a channel. This
//! module drains the channel and folds each event into a plain snapshot; the
//! panes read the snapshot and nothing else. There is no lock the paint loop can
//! wait on and no future it can block on, because there is nothing shared to
//! wait for.
//!
//! A verification is 40 ms and a shuffle proof is 95 ms — measured, not
//! estimated. At sixty frames a second a frame is 16 ms, so a single proof on
//! the paint thread is six dropped frames and a hand's worth is a client that
//! looks crashed.

use std::collections::VecDeque;

use crate::gui::lobby::{LobbyView, NetworkStatus, RelayStatus};
use crate::net::lobby::LobbyStore;
use crate::net::node::NodeEvent;

/// `S1-CS`: the table window's view, derived here so it can be tested.
mod table;

/// The window's own stale-link threshold, for the opponent question.
fn app_link_stale_ms() -> u64 {
    table::LINK_STALE_MS
}

/// The founder's reason codes, §4.3, in the words a player can act on.
///
/// Two of the eight are never sent by this client because nothing in the corpus
/// gives them a mechanism; they are named here anyway, because another
/// implementation may send them and "reason 6" tells a player nothing.
fn refusal(code: u16) -> &'static str {
    match code {
        1 => "the table is full",
        2 => "that seat is taken",
        3 => "wrong password",
        4 => "the buy-in is out of range",
        5 => "the advertisement has expired",
        6 => "banned",
        7 => "capabilities do not match",
        8 => "already seated",
        _ => "no reason this client understands",
    }
}

/// Eight characters of a key, which is what a person can compare at a glance.
fn short(key: &[u8; 32]) -> String {
    key[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// How many log lines the status pane keeps.
///
/// Bounded because the events come from the network: a peer that reconnects in a
/// loop would otherwise grow this without limit, and an unbounded log is a
/// memory bug with an external trigger like any other.
pub const MAX_LOG_LINES: usize = 500;

/// How many lines of lobby chat to keep.
///
/// A pane, not a history. Nothing is stored between runs and nothing is
/// replayed to somebody who arrives later — GossipSub delivers to whoever is on
/// the topic when a line is published, and keeping more than fits on a screen
/// would be keeping a record this client has no business keeping.
pub const MAX_CHAT_LINES: usize = 200;

/// What this client knows about the hand in progress.
///
/// A local view, like everything else in this module. Nothing here enters a
/// hash: D-012 forbids canonical state derived from a per-receiver quantity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandInProgress {
    pub hand_id: u64,
    pub button: u8,
    pub dealt_in: Vec<u8>,
    /// The seat whose turn it is to shuffle, while the chain is running.
    pub shuffling: Option<u8>,
    /// Whether the chain has closed and the deck is final.
    pub deck_ready: bool,
    /// This client's own two cards, as deck indices, once they are readable.
    pub cards: Option<[u8; 2]>,
    /// The seats holding cards, so the table can draw backs at the others.
    pub holding: Vec<u8>,
    /// The board, as deck indices, as far as it has opened.
    pub board: Vec<u8>,
    /// What this client may do, if it is its turn.
    pub turn: Option<MyTurn>,
    /// Whose turn it is, when it is not this client's.
    pub waiting_on: Option<u8>,
    /// Final stacks, once the hand has been settled.
    pub stacks: Vec<u64>,
    /// What each seat showed at the showdown, by seat.
    pub shown: Vec<Option<[u8; 2]>>,
    /// Whether the hand is over.
    pub over: bool,
    /// `S1-CS`: what the engine has after every action -- behind, in
    /// front, the pot, the street, who folded. Empty until the first
    /// `TableState` of the hand.
    pub stacks_now: Vec<u64>,
    pub bets: Vec<u64>,
    pub folded: Vec<bool>,
    pub pot: u64,
    pub street: Option<u16>,
    /// What each seat gained at the settlement, by seat.
    pub won: Vec<u64>,
}

/// What this client may do on its own turn.
///
/// A copy of what the engine said, taken at one instant. The window draws from
/// this and never recomputes any of it: the only place a legal action is
/// decided is `poker::actions`, and a second opinion here would be a second
/// opinion that can be wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MyTurn {
    pub to_call: u64,
    pub pot: u64,
    pub can_check: bool,
    pub can_call: bool,
    pub can_bet: bool,
    pub can_raise: bool,
    pub min_raise_to: u64,
    pub max_raise_to: u64,
}

/// Everything the client knows, in the form the panes read it.
#[derive(Debug, Default)]
pub struct AppState {
    pub lobby: LobbyStore,
    pub status: NetworkStatus,
    /// What has happened, newest last. Capped at [`MAX_LOG_LINES`].
    pub log: VecDeque<String>,
    /// How many lines have **ever** been written to the log.
    ///
    /// Monotonic, and the only sound way to ask "what is new since I last
    /// looked" once the log has reached its cap. See `AppState::note`.
    pub emitted: u64,
    /// The table the user has selected in the list.
    pub selected: Option<[u8; 32]>,
    /// The table this client is at or forming, if any.
    pub seated: Option<Seat>,
    /// This player's own name, as they chose it. Display data, never an
    /// identifier — §4.3 says that of a name received from the network, and it
    /// is no less true of one's own.
    pub me: String,
    /// Who is in the lobby: player key to name and when they last said so.
    ///
    /// Keyed on the **key**, never the name: two players may choose one name,
    /// and a list keyed on names would let either of them evict the other.
    pub players: std::collections::BTreeMap<[u8; 32], (String, u64)>,
    /// What has been said in the lobby, newest last.
    pub chat: VecDeque<crate::gui::lobby::ChatLine>,
    /// The hand this client believes is in progress.
    pub hand: Option<HandInProgress>,
    /// Which seats the current hand is still waiting for, so the same sentence
    /// is not written to the log every time somebody else speaks.
    pub waiting_for: Vec<u8>,
    /// The last clock this client was told about, so presence can be aged.
    pub last_sweep_ms: u64,
    /// `S1-CR`: an unfinished session the node found on record at start, until
    /// the player answers or it resolves.
    pub unfinished: Option<Unfinished>,
    /// The latest advertisement timestamp this client has seen.
    ///
    /// Used as the clock for this store's own expiry. Not this machine's clock:
    /// the two stores must agree about which adverts are alive, and a client
    /// whose clock ran fast would expire tables the node still held.
    pub newest_seen: u64,
    /// `S1-CS`: the last stacks the engine or a settlement named, kept
    /// across the boundary so the next hand opens on them.
    pub last_stacks: Vec<u64>,
    /// `S1-CS`: how many times it has been this client's turn, so the raise
    /// control can tell a new turn from the one it is in.
    pub turns: u64,
    /// `S1-CS`: the hero's hand in words and odds, once both the cards and
    /// the board are known.
    pub strength: Option<crate::poker::strength::Strength>,
    /// `S1-CS`: whose decision clock is running, and since when on this
    /// machine's clock. A local view like everything else here; the
    /// window draws the fraction left from it and nothing is sent per
    /// frame.
    pub turn_seat: Option<u8>,
    pub turn_since: Option<std::time::Instant>,
    /// `S1-CS`: what the seats have said, newest last; capped like the lobby's.
    pub table_chat: VecDeque<TableLine>,
    /// `S1-CS`: seats this player does not want to hear. Local, never sent.
    pub muted: std::collections::BTreeSet<u8>,
    /// `S1-CS`: each seat's last link reading and when it arrived.
    pub links: std::collections::BTreeMap<u8, (Option<u64>, std::time::Instant)>,
    /// `S1-CS`: the join in progress, if one is.
    pub joining: Option<Joining>,
    /// `S1-CX`: the heads-up opponent this client cannot reach, if any.
    pub opponent_gone: Option<OpponentGone>,
    /// `D-032`: how many absences worth asking about ended with the
    /// opponent back; at `MAX_RETURNS` the next absence is final.
    pub opponent_returns: u8,
    /// `D-032`: the opponent's fourth absence; the game ends here, and
    /// nothing reachable undoes it.
    pub opponent_out: bool,
}

impl Seat {
    /// **Whether this client can still hear the table it is sitting at.**
    ///
    /// `S1-BH`: a seat is certified out precisely *because* it cannot hear the
    /// group, and the certificate that removes it is a hand event, so it rides
    /// that same group — the one party who needs it is the one party guaranteed
    /// not to get it. Measured: a seat that never entered the group was struck
    /// out and exited printing `TABLE FORMED seats=10` after 900 s at a table
    /// it had been removed from.
    ///
    /// **Nothing has to be sent to fix that.** Every peer that CAN hear the
    /// group receives its own certificate and applies it — measured, within two
    /// seconds. The only peer left uninformed is the one that hears nothing,
    /// and that peer can see its own silence. It just never looked.
    pub fn deaf(&self, now_ms: u64) -> bool {
        self.silent_since
            .is_some_and(|at| now_ms.saturating_sub(at) >= crate::protocol::constants::DEAF_MS)
    }

    /// Sitting at a table that has formed **and** can be heard.
    pub fn playing(&self, now_ms: u64) -> bool {
        self.session.is_some() && !self.deaf(now_ms)
    }
}

/// Where this client is sitting, as the panes read it.
///
/// A **local view** like everything else here: it is what this client believes
/// about a table being formed, and none of it is canonical until `session` is
/// filled in — which happens only when every seat has ratified the roster.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seat {
    /// The table key, which is the table's identity.
    pub key: [u8; 32],
    /// This client's seat, once the founder has given it one.
    pub seat: Option<u8>,
    /// Whether this client founded the table.
    pub hosting: bool,
    /// Who is seated: seat, name, buy-in.
    pub roster: Vec<(u8, String, u64)>,
    /// The session identity, once the table is real. `None` means the table has
    /// **not** started, whatever else is filled in.
    pub session: Option<[u8; 32]>,
    /// **How many other seats this client can hear, and since when it could
    /// hear none (`S1-BH`).**
    ///
    /// `heard` is the last carrier reading. `silent_since` is the clock of the
    /// first reading of zero that was not followed by a better one — cleared
    /// the moment anybody is heard, so a transient cannot latch. A client that
    /// has been at zero for `DEAF_MS` is not playing, whatever its session says,
    /// and `playing()` is where that judgement is made once.
    pub heard: Option<u16>,
    pub silent_since: Option<u64>,
    /// `S1-CS`: how many other seats the carrier wants to hear before the
    /// table can deal, from the same reading as `heard`.
    pub group_want: Option<u16>,
    /// The table's name and shape, from the node rather than from the lobby.
    pub name: String,
    pub seats: u8,
    pub needed: u8,
    pub small_blind: u64,
    pub big_blind: u64,
    /// `S1-CS`: how long a seat has to decide, from the table's params.
    pub action_ms: u64,
}

/// `S1-CS`: a line said at the table, filed under the seat that said it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableLine {
    pub seat: u8,
    pub who: String,
    pub said: String,
}

/// `S1-CS`: a join this client asked for and has not heard the end of.
///
/// Kept here so the window can say *connecting* with a clock on it, and
/// so the reason it ended -- a seat, a refusal, the founder not answering,
/// or nothing at all inside [`JOIN_WAIT_MS`] -- reaches the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Joining {
    pub key: [u8; 32],
    pub name: String,
    pub buyin: u64,
    pub password: Option<Vec<u8>>,
    pub since: std::time::Instant,
    /// Why it failed, once it has.
    pub failed: Option<String>,
}

/// How long a join may go unanswered before the window calls it failed.
/// Longer than the node's own request timeout plus the time a founder has
/// been measured to take to become dialable.
pub const JOIN_WAIT_MS: u64 = 90_000;

/// `S1-CX`: a heads-up opponent this client cannot reach, since when,
/// whether the log has said so and whether the player has answered.
///
/// D-007: at two seats nobody can fold a hand for an absent player and
/// the remedy is to leave the table -- so the window asks, once per
/// episode, and a headless client writes the same sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpponentGone {
    pub since: std::time::Instant,
    pub said: bool,
    pub dismissed: bool,
}

/// How long an opponent must be unreachable before it is said: longer
/// than a reconnection takes, shorter than a player's patience.
pub const OPPONENT_GONE_MS: u64 = 15_000;

/// `S1-CR`: what the window asks about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unfinished {
    pub key: [u8; 32],
    pub table_name: String,
    pub seat: u8,
    pub stack: u64,
    pub hand_id: u64,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one event from the node into the snapshot.
    ///
    /// Every arm is a local view. **Nothing here enters a hash** — D-012 forbids
    /// canonical state derived from a per-receiver quantity, and every value in
    /// this struct is exactly that: how many peers *this* client has, what *this*
    /// client heard, when.
    pub fn apply(&mut self, event: NodeEvent) {
        match event {
            NodeEvent::Listening(addr) => {
                self.status.listening.push(addr.to_string());
                self.note(format!("listening on {addr}"));
            }
            // Counted, not written down. This client shares a DHT with several
            // hundred strangers and a line each was hundreds a minute in the
            // client log, which buried every line a player might have wanted.
            NodeEvent::PeerConnected(_) => self.status.peers += 1,
            NodeEvent::PeerDisconnected(_) => {
                self.status.peers = self.status.peers.saturating_sub(1);
            }
            NodeEvent::PokerPeer { peer, gone } => {
                if gone {
                    self.status.lobby_peers = self.status.lobby_peers.saturating_sub(1);
                    self.note(format!("a player left the network: {peer}"));
                } else {
                    self.status.lobby_peers += 1;
                    self.note(format!("another poker client: {peer}"));
                }
            }
            NodeEvent::Discovered { hints, dropped } => {
                self.note(format!("{hints} peers found, {dropped} unusable"));
            }
            NodeEvent::Announced => {
                self.status.dht_announced = true;
                self.note("this client is listed in the public lobby".into());
            }
            NodeEvent::Reachability { public } => {
                self.status.public = Some(public);
                self.note(if public {
                    "this client is reachable from the internet".into()
                } else {
                    "this client is behind NAT".into()
                });
            }
            NodeEvent::Reserved {
                relay,
                bytes,
                seconds,
                adequate,
            } => {
                self.status.relay = Some(RelayStatus {
                    peer: relay.to_string(),
                    adequate,
                });
                self.note(format!(
                    "relay {relay}: {} bytes / {} s, {}",
                    bytes.map(|b| b.to_string()).unwrap_or_else(|| "no limit".into()),
                    seconds.map(|s| s.to_string()).unwrap_or_else(|| "no limit".into()),
                    if adequate {
                        "enough for a table"
                    } else {
                        "NOT enough to carry a hand"
                    }
                ));
            }
            NodeEvent::NoRelayFound { cycles } => {
                self.note(format!(
                    "no relay after {cycles} searches; if nobody anywhere is reachable there is no game"
                ));
            }
            NodeEvent::HolePunched(p) => self.note(format!("{p} is now a direct connection")),
            NodeEvent::StillRelayed(p) => self.note(format!("{p} stays relayed")),
            NodeEvent::UnfinishedSession { key, table_name, seat, stack, hand_id } => {
                self.note(format!(
                    "an unfinished game is on record: {table_name}, seat {seat}, stack {stack}, last at hand #{hand_id}"
                ));
                self.unfinished = Some(Unfinished { key, table_name, seat, stack, hand_id });
            }
            NodeEvent::SessionResumed { hand_id } => {
                self.unfinished = None;
                self.note(format!("back at the table: following hand #{hand_id}"));
            }
            NodeEvent::SessionGaveUp { why } => {
                self.unfinished = None;
                self.note(format!("the unfinished game is gone: {why}"));
            }
            NodeEvent::LocalPeer(p) => self.note(format!("found {p} on this network")),
            NodeEvent::LobbyPeer(p) => self.note(format!("found {p} in the public lobby")),
            NodeEvent::LobbyHere { who, nickname } => {
                // Noted when somebody arrives, not every time they say they are
                // still here: presence repeats every thirty seconds and a line
                // each time would bury everything else.
                let arrived = !self.players.contains_key(&who);
                if arrived {
                    self.note(format!(
                        "{nickname} ({}) is in the lobby",
                        crate::storage::profile::short_name(&who)
                    ));
                }
                self.players.insert(who, (nickname, self.last_sweep_ms));
            }
            NodeEvent::LobbySaid {
                who,
                nickname,
                text,
            } => {
                // Somebody who speaks is somebody who is here, so a client that
                // joined between two presence messages still sees them in the
                // list rather than only in the chat.
                self.players.insert(who, (nickname.clone(), self.last_sweep_ms));
                if self.chat.len() >= MAX_CHAT_LINES {
                    self.chat.pop_front();
                }
                self.chat.push_back(crate::gui::lobby::ChatLine {
                    // The name **and** the key, because a name is decoration
                    // and two players may choose one.
                    who: format!("{nickname} ({})", crate::storage::profile::short_name(&who)),
                    said: text,
                });
            }
            NodeEvent::TableSaid { seat, nickname, text } => {
                if self.table_chat.len() >= MAX_CHAT_LINES {
                    self.table_chat.pop_front();
                }
                self.table_chat.push_back(TableLine {
                    seat,
                    who: format!("{nickname} (seat {seat})"),
                    said: text,
                });
            }
            NodeEvent::SeatLink { seat, rtt_ms } => {
                self.links.insert(seat, (rtt_ms, std::time::Instant::now()));
                if self.heads_up_opponent() == Some(seat) {
                    self.opponent_reachable(rtt_ms.is_some());
                }
            }
            NodeEvent::HandBegan {
                hand_id,
                button,
                dealt_in,
            } => {
                self.hand = Some(HandInProgress {
                    hand_id,
                    button,
                    dealt_in,
                    // `HAND_INIT` completing is what starts the chain, so at
                    // this instant nobody has shuffled yet. The first
                    // `DeckProgress` names the seat that is up.
                    shuffling: None,
                    deck_ready: false,
                    cards: None,
                    holding: Vec::new(),
                    board: Vec::new(),
                    turn: None,
                    waiting_on: None,
                    stacks: Vec::new(),
                    shown: Vec::new(),
                    over: false,
                    stacks_now: Vec::new(),
                    bets: Vec::new(),
                    folded: Vec::new(),
                    pot: 0,
                    street: None,
                    won: Vec::new(),
                });
                self.strength = None;
                self.waiting_for.clear();
                self.note(format!("hand #{hand_id} has begun"));
            }
            NodeEvent::DeckProgress {
                hand_id,
                shuffling,
                ready,
            } => {
                if let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) {
                    h.shuffling = shuffling;
                    h.deck_ready = ready;
                }
                self.note(match (shuffling, ready) {
                    (_, true) => format!("hand #{hand_id}: the deck is shuffled and sealed"),
                    (Some(s), _) => format!("hand #{hand_id}: seat {s} is shuffling"),
                    (None, false) => format!("hand #{hand_id}: the deck is being prepared"),
                });
            }
            NodeEvent::TableState {
                hand_id,
                street,
                pot,
                to_act,
                stacks,
                bets,
                folded,
            } => {
                if let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) {
                    h.stacks_now = stacks.clone();
                    h.bets = bets;
                    h.folded = folded;
                    h.pot = pot;
                    h.street = Some(street);
                    if h.turn.is_none() {
                        h.waiting_on = to_act;
                    }
                }
                self.last_stacks = stacks;
                self.clock_for(to_act);
            }
            NodeEvent::YourTurn {
                hand_id,
                street,
                to_call,
                pot,
                can_check,
                can_call,
                can_bet,
                can_raise,
                min_raise_to,
                max_raise_to,
            } => {
                let _ = street;
                if let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) {
                    h.turn = Some(MyTurn {
                        to_call,
                        pot,
                        can_check,
                        can_call,
                        can_bet,
                        can_raise,
                        min_raise_to,
                        max_raise_to,
                    });
                    h.waiting_on = None;
                }
                self.turns = self.turns.saturating_add(1);
                let me = self.seated.as_ref().and_then(|s| s.seat);
                self.clock_for(me);
                self.note(if to_call > 0 {
                    format!("hand #{hand_id}: your turn — {to_call} to call")
                } else {
                    format!("hand #{hand_id}: your turn")
                });
            }
            NodeEvent::NotYourTurn { hand_id, seat } => {
                if let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) {
                    h.turn = None;
                    h.waiting_on = seat;
                }
                self.clock_for(seat);
                // Logged, because "whose turn is it" is the question a player
                // asks of a table that appears to be doing nothing — and a
                // table that is doing nothing because it is waiting for
                // somebody looks identical to one that is stuck.
                if let Some(seat) = seat {
                    self.log
                        .push_back(format!("hand #{hand_id}: waiting for seat {seat}"));
                }
            }
            NodeEvent::Board { hand_id, cards } => {
                let named: Vec<String> = cards
                    .iter()
                    .filter_map(|i| crate::poker::state::Card::from_index(*i).ok())
                    .map(|c| c.to_string())
                    .collect();
                if let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) {
                    h.board = cards;
                }
                self.refresh_strength();
                if !named.is_empty() {
                    self.log
                        .push_back(format!("hand #{hand_id}: board {}", named.join(" ")));
                }
            }
            NodeEvent::HandEnded {
                hand_id,
                stacks,
                shown,
            } => {
                if let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) {
                    // `S1-CS`: what each seat gained is the settlement's figure
                    // over the engine's last one -- what the chips fly with.
                    let before: &[u64] = if h.stacks_now.is_empty() {
                        &self.last_stacks
                    } else {
                        &h.stacks_now
                    };
                    let won: Vec<u64> = stacks
                        .iter()
                        .enumerate()
                        .map(|(i, s)| s.saturating_sub(before.get(i).copied().unwrap_or(*s)))
                        .collect();
                    h.won = won;
                    h.stacks = stacks.clone();
                    h.shown = shown;
                    h.over = true;
                    h.turn = None;
                    h.waiting_on = None;
                    h.bets = vec![0; h.bets.len()];
                }
                self.last_stacks = stacks;
                self.clock_for(None);
                self.note(format!("hand #{hand_id} is over"));
            }
            NodeEvent::CardsDealt { hand_id, seats } => {
                if let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) {
                    h.holding = seats;
                }
            }
            NodeEvent::HoleCards { hand_id, cards } => {
                if let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) {
                    h.cards = Some(cards);
                }
                self.refresh_strength();
                self.log
                    .push_back(format!("hand #{hand_id}: your cards are dealt"));
            }
            NodeEvent::HandWaiting { hand_id, seats } => {
                // Said once per set, not once per arriving copy: a table
                // waiting for one seat would otherwise write a line every time
                // anybody else spoke.
                if self.waiting_for != seats {
                    let who: Vec<String> = seats.iter().map(|s| s.to_string()).collect();
                    self.note(format!(
                        "hand #{hand_id} is waiting for seat{} {}",
                        if seats.len() == 1 { "" } else { "s" },
                        who.join(", ")
                    ));
                    self.waiting_for = seats;
                }
            }
            NodeEvent::Carrier { seen, want } => {
                // `S1-CX`: the group's count is not a reading of the opponent's
                // link either way. It went on *seeing* a seat that had stopped
                // (`run080025-2`), and it is empty for the ten to sixty seconds
                // the other seat takes to join a freshly set table's group --
                // which is not an opponent out of reach. A closed connection and
                // a stale link start an episode; a ping ends one.
                let now = self.last_sweep_ms;
                if let Some(s) = self.seated.as_mut() {
                    s.heard = Some(seen);
                    s.group_want = Some(want);
                    if seen > 0 || want == 0 {
                        s.silent_since = None;
                    } else if s.silent_since.is_none() {
                        s.silent_since = Some(now);
                    }
                }
            }
            NodeEvent::Swept { now_ms } => {
                self.last_sweep_ms = now_ms;
                self.tick_opponent();
                // A player who has stopped saying they are here stops being
                // here. There is no goodbye message, because a client that is
                // switched off does not send one.
                self.players.retain(|_, (_, at)| {
                    now_ms.saturating_sub(*at) < crate::protocol::constants::PRESENCE_TTL_MS
                });
                let gone = self.lobby.expire(now_ms);
                if gone > 0 {
                    // Said only when something went. A line every half minute
                    // saying nothing happened is a line that hides the ones
                    // that matter.
                    self.note(format!(
                        "{gone} table{} expired",
                        if gone == 1 { "" } else { "s" }
                    ));
                }
            }
            NodeEvent::MeshPeer(p) => self.note(format!("{p} joined the lobby mesh")),
            NodeEvent::Published { bytes } => self.note(format!("advertised, {bytes} bytes")),
            NodeEvent::TableSeen {
                key,
                ad,
                params_hash,
                advert_hash,
            } => {
                let name = ad.table_name.clone();
                // Offered rather than inserted, so this store applies §7.2's
                // rules 6 and 7 for itself. It is a **second** copy of the
                // lobby — the node has its own — and a copy that took the
                // node's word would drift from it silently the first time a
                // message was dropped, which this channel is allowed to do.
                // **A stranger's clock is not this client's clock.**
                //
                // This was `ad.timestamp_unix_ms.max(self.newest_seen)`, so the
                // value that decides what has expired was taken from the
                // advertisement being offered. §7.2 rule 5 accepts a timestamp
                // up to `MAX_CLOCK_SKEW_MS` = 120 s in the future, and
                // `AD_TTL_MS` is 90 s — **120 > 90 is the whole bug**. One
                // advert signed with `timestamp_unix_ms = now + 120 s` passes
                // `lobby::admit`, sets this clock 120 s ahead, and the
                // `expire(now)` on the next line then drops every record whose
                // `received_at_ms` is not within 90 s of it — which is every
                // honest table. Reproduced: three tables in the list, one such
                // advert, and the list held the attacker's table alone. The
                // node's own store is unharmed because it ages on the real
                // clock, so the two copies simply disagree. Honest founders
                // re-broadcast and come back within a cycle, but an attacker
                // sending one of these every 30 s — far under
                // `MAX_ADS_PER_PEER_PER_MIN` = 20 — keeps the user's list empty
                // for as long as it cares to.
                //
                // `newest_seen` exists because this layer has no wall clock of
                // its own; it is fed events. But it *is* fed a real one, in
                // `Swept { now_ms }`, so use that and let the advert's claim
                // count only before the first sweep has arrived. Monotone
                // either way, which is what `offer` and `expire` need.
                let now = if self.last_sweep_ms > 0 {
                    self.last_sweep_ms.max(self.newest_seen)
                } else {
                    ad.timestamp_unix_ms.max(self.newest_seen)
                };
                self.newest_seen = now;
                self.lobby.expire(now);
                match self.lobby.offer(key, *ad, params_hash, advert_hash, now) {
                    Ok(()) => self.note(format!(
                        "table {} ({name})",
                        crate::gui::lobby::short_key(&key)
                    )),
                    // Not newer is the ordinary case: a table re-broadcasts
                    // every thirty seconds and this client already has it.
                    Err(crate::net::lobby::NotTaken::NotNewer) => {}
                    Err(e) => self.note(format!("table {}: {e:?}", crate::gui::lobby::short_key(&key))),
                }
            }
            NodeEvent::TableRefused { reason } => self.note(format!("advert refused: {reason}")),
            // Kept out of the log by default. Most dials fail on an open DHT and
            // a log full of them buries the lines that mean something — but the
            // count is carried, because "most dials fail" and "this client is
            // broken" look identical without one.
            NodeEvent::DialFailed { reason } => {
                self.status.failed_dials += 1;
                // **The first few, in full.** Kept out of the log entirely
                // before this, which meant a client that could not reach the
                // machine it was sitting next to looked exactly like one that
                // had nothing to reach. The window that matters is the first
                // half-minute — a table forming — and after that the count
                // alone is the right amount of noise.
                if self.status.failed_dials <= 12 {
                    self.note(format!("dial failed: {reason}"));
                }
            }
            NodeEvent::Warning(w) => self.note(w),

            NodeEvent::PortMapped { how, external } => {
                self.status.port_mapped = Some(how);
                self.note(format!("{how} opened port {external}"));
            }
            NodeEvent::Hosting { key } => {
                self.forget_the_table();
                self.seated = Some(Seat {
                    key,
                    seat: Some(0),
                    hosting: true,
                    ..Default::default()
                });
                self.note(format!("hosting {}", short(&key)));
            }
            NodeEvent::Seated { key, seat } => {
                // `S1-CS`: a seat is the end of the join that asked for it.
                if self.joining.as_ref().is_some_and(|j| j.key == key) {
                    self.joining = None;
                }
                self.table(key).seat = Some(seat);
                self.note(format!("seat {seat} at {}", short(&key)));
            }
            NodeEvent::Roster { key, seats } => {
                let n = seats.len();
                self.table(key).roster = seats;
                self.note(format!("{n} seated"));
            }
            NodeEvent::TableParams {
                key,
                name,
                seats,
                needed,
                small_blind,
                big_blind,
                action_ms,
            } => {
                let t = self.table(key);
                t.name = name;
                t.seats = seats;
                t.needed = needed;
                t.small_blind = small_blind;
                t.big_blind = big_blind;
                t.action_ms = action_ms;
            }
            NodeEvent::TableReal { key, session } => {
                self.table(key).session = Some(session);
                self.note(format!("the table is set: session {}", short(&session)));
            }
            NodeEvent::JoinRefused { reason } => {
                // The founder's claim, said as a claim. A rejection is never
                // proof of anything: §4.3 puts it plainly, and the founder may
                // simply not want this player.
                self.forget_the_table();
                self.seated = None;
                if let Some(j) = self.joining.as_mut() {
                    j.failed = Some(format!("the founder says no: {}", refusal(reason)));
                }
                self.note(format!("the founder says no: {}", refusal(reason)));
            }
            NodeEvent::LeftTable { why } => {
                self.forget_the_table();
                self.seated = None;
                if let Some(j) = self.joining.as_mut() {
                    j.failed = Some(why.clone());
                }
                self.note(why);
            }
        }
    }

    /// This client's record for a table, created if there is not one yet.
    ///
    /// The three table events are a stream and their order is not guaranteed:
    /// a seat can be ratified from the founder's roster announcement on the mesh
    /// **before** the answer to the join RPC arrives, and it does exactly that on
    /// a fast local network. An earlier version dropped anything that arrived
    /// before the seat, so a table that had genuinely formed was reported as no
    /// table at all — by the client that was sitting at it.
    ///
    /// A record for a different table replaces this one. One table per client
    /// here; multi-tabling is more than one client, which is what §4.3's
    /// per-roster rules are written for.
    fn table(&mut self, key: [u8; 32]) -> &mut Seat {
        match &self.seated {
            Some(s) if s.key == key => {}
            _ => {
                // `S1-CS`: a hand from another table is not this table's.
                self.forget_the_table();
                self.seated = Some(Seat {
                    key,
                    ..Default::default()
                })
            }
        }
        self.seated.as_mut().expect("just set")
    }

    /// Whether this client is at a table that has actually started.
    ///
    /// Not `seated.is_some()`: a seat with no session is a table still being
    /// formed, and treating the two as one is how a client deals a hand at a
    /// table nobody has ratified.
    pub fn at_a_real_table(&self) -> bool {
        self.seated.as_ref().is_some_and(|s| s.session.is_some())
    }

    fn note(&mut self, line: String) {
        if self.log.len() >= MAX_LOG_LINES {
            self.log.pop_front();
        }
        self.log.push_back(line);
        // Counted, and this is not bookkeeping for its own sake. A reader that
        // works out what is new from `log.len()` gets nothing at all once the
        // log is at capacity, because a pop and a push leave the length where
        // it was. The headless client did exactly that, and the effect was that
        // every line routed through here stopped being printed the moment five
        // hundred lines had gone by - a few minutes of an ordinary lobby. It is
        // how a table that was playing perfectly well looked like a stalled one
        // for three runs of the two-process test.
        self.emitted = self.emitted.saturating_add(1);
    }

    /// `S1-CS`: nothing of a hand outlives the table it was played at. The
    /// window reopened used to show the last game's cards at the next
    /// table, because the hand was never cleared.
    fn forget_the_table(&mut self) {
        self.opponent_gone = None;
        self.opponent_returns = 0;
        self.opponent_out = false;
        self.clock_for(None);
        self.table_chat.clear();
        self.muted.clear();
        self.links.clear();
        self.hand = None;
        self.waiting_for.clear();
        self.last_stacks.clear();
        self.strength = None;
    }

    /// `S1-CX`: the other seat, when this table is heads-up and set.
    fn heads_up_opponent(&self) -> Option<u8> {
        let s = self.seated.as_ref()?;
        if s.session.is_none() || s.roster.len() != 2 {
            return None;
        }
        let me = s.seat?;
        s.roster.iter().map(|(n, _, _)| *n).find(|n| *n != me)
    }

    /// A reading about the heads-up opponent: reachable clears the episode,
    /// unreachable starts one if none is open.
    fn opponent_reachable(&mut self, reachable: bool) {
        // `D-032`: the fourth absence is final.
        if self.opponent_out {
            return;
        }
        if reachable {
            // `D-032`: an absence worth asking about that ended is a return.
            if self
                .opponent_gone
                .take()
                .is_some_and(|g| g.since.elapsed().as_millis() as u64 >= OPPONENT_GONE_MS)
            {
                self.opponent_returns = self.opponent_returns.saturating_add(1);
                self.note(format!(
                    "your opponent is back: return {} of {} (D-032)",
                    self.opponent_returns,
                    crate::protocol::constants::MAX_RETURNS
                ));
            }
        } else if self.opponent_gone.is_none() {
            self.opponent_gone = Some(OpponentGone {
                since: std::time::Instant::now(),
                said: false,
                dismissed: false,
            });
        }
    }

    /// Called every frame and on every sweep: an opponent unreachable for
    /// `OPPONENT_GONE_MS` is said once, in D-007's words.
    pub fn tick_opponent(&mut self) {
        // A link reading gone stale is an opponent this client cannot reach,
        // whether or not the connection was ever seen to close.
        if let Some(opponent) = self.heads_up_opponent() {
            let stale = self
                .links
                .get(&opponent)
                .is_some_and(|(_, at)| at.elapsed().as_millis() as u64 > app_link_stale_ms());
            if stale {
                self.opponent_reachable(false);
            }
        }
        let due = self
            .opponent_gone
            .as_ref()
            .filter(|g| !g.said && g.since.elapsed().as_millis() as u64 >= OPPONENT_GONE_MS)
            .map(|g| g.since.elapsed().as_secs());
        if let Some(secs) = due {
            if let Some(g) = self.opponent_gone.as_mut() {
                g.said = true;
            }
            // `D-032`: three returns and no more -- the fourth absence ends the
            // game, and the one thing left to do is leave.
            if self.opponent_returns >= crate::protocol::constants::MAX_RETURNS {
                self.opponent_out = true;
                self.note(format!(
                    "your opponent has been unreachable for {secs} s, for the {}th time; {} returns are the limit (D-032): the game ends here -- leave the table",
                    self.opponent_returns + 1,
                    crate::protocol::constants::MAX_RETURNS
                ));
            } else {
                self.note(format!(
                    "your opponent has been unreachable for {secs} s; heads-up, nobody can fold a hand for them (D-007): wait for them, or leave the table"
                ));
            }
        }
    }

    /// The player chose to wait: the question is not asked again for this
    /// episode. A new one -- the opponent back, then gone again -- asks anew.
    pub fn dismiss_opponent_gone(&mut self) {
        if let Some(g) = self.opponent_gone.as_mut() {
            g.dismissed = true;
        }
    }

    /// How long the opponent has been unreachable, once it is worth asking
    /// about and until the player has answered.
    pub fn opponent_gone_for_s(&self) -> Option<u64> {
        self.opponent_gone
            .as_ref()
            .filter(|g| !g.dismissed && g.since.elapsed().as_millis() as u64 >= OPPONENT_GONE_MS)
            .map(|g| g.since.elapsed().as_secs())
    }

    /// `S1-CS`: the window asked to sit down; the small window says so until
    /// a seat comes, a refusal comes, or the wait runs out.
    pub fn begin_join(&mut self, key: [u8; 32], name: String, buyin: u64, password: Option<Vec<u8>>) {
        self.joining = Some(Joining {
            key,
            name,
            buyin,
            password,
            since: std::time::Instant::now(),
            failed: None,
        });
    }

    /// The join is tried again, from now.
    pub fn retry_join(&mut self) {
        if let Some(j) = self.joining.as_mut() {
            j.since = std::time::Instant::now();
            j.failed = None;
        }
    }

    /// Called every frame: a join nobody has answered inside `JOIN_WAIT_MS`
    /// is called failed, once.
    pub fn tick_join(&mut self) {
        if let Some(j) = self.joining.as_mut() {
            if j.failed.is_none() && j.since.elapsed().as_millis() as u64 >= JOIN_WAIT_MS {
                j.failed = Some(format!(
                    "no answer from the table in {} s; it may be gone, or its host may be unreachable from here",
                    JOIN_WAIT_MS / 1_000
                ));
            }
        }
    }

    /// `S1-CS`: the decision clock runs for the seat to act, from the moment
    /// this client learned it was that seat's turn; a seat that was already
    /// on the clock keeps its start.
    fn clock_for(&mut self, seat: Option<u8>) {
        if seat != self.turn_seat {
            self.turn_seat = seat;
            self.turn_since = seat.map(|_| std::time::Instant::now());
        }
    }

    /// The hero's hand in words and odds, from the cards this client opened
    /// and the board it verified; `None` until the cards are known.
    fn refresh_strength(&mut self) {
        use crate::poker::state::Card;
        self.strength = self.hand.as_ref().and_then(|h| {
            let cards = h.cards?;
            let hole = [Card::from_index(cards[0]).ok()?, Card::from_index(cards[1]).ok()?];
            let board: Vec<Card> = h.board.iter().filter_map(|i| Card::from_index(*i).ok()).collect();
            Some(crate::poker::strength::strength(hole, &board))
        });
    }

    /// The snapshot the panes read.
    pub fn view(&self) -> LobbyView {
        let mut v = LobbyView::from(&self.lobby, self.status.clone());
        v.selected = self.selected;
        v.log = self.log.iter().cloned().collect();
        v.me = self.me.clone();
        v.unfinished = self.unfinished.clone();
        v.joining = self.joining.as_ref().map(|j| crate::gui::lobby::JoiningView {
            name: j.name.clone(),
            elapsed_s: j.since.elapsed().as_secs(),
            failed: j.failed.clone(),
        });
        v.chat = self.chat.iter().cloned().collect();
        // Name and key together, because a name is decoration. Sorted by name
        // so the pane does not reshuffle every time somebody says they are
        // still here.
        v.seated = {
            let mut who: Vec<String> = self
                .players
                .iter()
                .map(|(k, (name, _))| {
                    format!("{name} ({})", crate::storage::profile::short_name(k))
                })
                .collect();
            who.sort();
            who
        };
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::PeerId;

    fn peer() -> PeerId {
        PeerId::from(libp2p::identity::Keypair::generate_ed25519().public())
    }

    /// A seat is not a table. Until every seat has ratified there is no session
    /// identity, and a client that treated the two as one would deal a hand at a
    /// table nobody agreed to.
    /// The three table events arrive in whatever order the network gives them,
    /// and on a fast local network the ratification really does beat the answer
    /// to the join request. An earlier version dropped everything that arrived
    /// before the seat, and a client sitting at a formed table reported no
    /// table at all.
    #[test]
    fn the_table_events_may_arrive_in_any_order() {
        let orders: [&[NodeEvent]; 3] = [
            &[
                NodeEvent::Seated { key: [7u8; 32], seat: 1 },
                NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "a".into(), 1), (1, "b".into(), 1)] },
                NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] },
            ],
            &[
                NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] },
                NodeEvent::Seated { key: [7u8; 32], seat: 1 },
                NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "a".into(), 1), (1, "b".into(), 1)] },
            ],
            &[
                NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "a".into(), 1), (1, "b".into(), 1)] },
                NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] },
                NodeEvent::Seated { key: [7u8; 32], seat: 1 },
            ],
        ];
        for order in orders {
            let mut s = AppState::new();
            for e in order {
                s.apply(e.clone());
            }
            let seat = s.seated.as_ref().expect("a seat");
            assert_eq!(seat.seat, Some(1));
            assert_eq!(seat.roster.len(), 2);
            assert_eq!(seat.session, Some([9u8; 32]));
            assert!(s.at_a_real_table());
        }
    }

    /// A record for another table replaces this one rather than merging into
    /// it. Two tables' rosters in one record is two tables nobody is at.
    #[test]
    fn another_table_replaces_this_one() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 1 });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::Seated { key: [8u8; 32], seat: 4 });
        let seat = s.seated.as_ref().unwrap();
        assert_eq!(seat.key, [8u8; 32]);
        assert_eq!(seat.seat, Some(4));
        assert_eq!(seat.session, None, "the old session followed us to a new table");
    }

    #[test]
    fn a_seat_without_a_session_is_not_a_table() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Hosting { key: [7u8; 32] });
        assert!(s.seated.is_some());
        assert!(!s.at_a_real_table(), "a table with one seat has not started");

        s.apply(NodeEvent::Roster {
            key: [7u8; 32],
            seats: vec![(0, "Alice".into(), 1_000), (1, "Bob".into(), 1_000)],
        });
        assert!(!s.at_a_real_table(), "a roster is a proposal, not a table");

        s.apply(NodeEvent::TableReal {
            key: [7u8; 32],
            session: [9u8; 32],
        });
        assert!(s.at_a_real_table());
    }

    /// A refusal clears the seat and reaches the log as a **claim**.
    #[test]
    fn a_refusal_is_reported_as_a_claim() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated {
            key: [7u8; 32],
            seat: 3,
        });
        s.apply(NodeEvent::JoinRefused { reason: 3 });
        assert!(s.seated.is_none());
        let line = s.log.back().unwrap();
        assert!(line.contains("says no"), "{line}");
        assert!(line.contains("wrong password"), "{line}");
    }

    /// Every reason code §4.3 allocates has words, including the two this client
    /// never sends. Another implementation may send them, and "reason 6" tells a
    /// player nothing at all.
    #[test]
    fn every_reason_code_has_words() {
        for code in 1..=8u16 {
            assert_ne!(refusal(code), "no reason this client understands");
        }
        assert_eq!(refusal(0), "no reason this client understands");
        assert_eq!(refusal(9), "no reason this client understands");
    }

    #[test]
    fn the_peer_count_follows_the_connections() {
        let mut s = AppState::new();
        let a = peer();
        let b = peer();

        s.apply(NodeEvent::PeerConnected(a));
        s.apply(NodeEvent::PeerConnected(b));
        assert_eq!(s.status.peers, 2);

        s.apply(NodeEvent::PeerDisconnected(a));
        assert_eq!(s.status.peers, 1);
    }

    /// A disconnection this client never saw a connection for must not take the
    /// count below zero — which on a `usize` is not a negative number but a very
    /// large one, and would render as "connected to 18446744073709551615 peers".
    #[test]
    fn a_stray_disconnection_does_not_wrap_the_count() {
        let mut s = AppState::new();
        s.apply(NodeEvent::PeerDisconnected(peer()));
        assert_eq!(s.status.peers, 0);
    }

    /// The events come from the network, so the log is bounded like everything
    /// else that does. A peer reconnecting in a loop would otherwise grow it
    /// without limit.
    #[test]
    fn the_log_is_bounded() {
        let mut s = AppState::new();
        for _ in 0..(MAX_LOG_LINES * 3) {
            // A **poker** peer, because an ordinary DHT connection deliberately
            // writes no line at all any more: there are several hundred of them
            // and a line each buried everything else.
            s.apply(NodeEvent::PokerPeer {
                peer: peer(),
                gone: false,
            });
        }
        assert_eq!(s.log.len(), MAX_LOG_LINES);
    }

    /// And it keeps the newest, because the oldest line is the least useful one
    /// when something has just gone wrong.
    #[test]
    fn the_log_keeps_the_newest() {
        let mut s = AppState::new();
        for i in 0..(MAX_LOG_LINES + 5) {
            s.apply(NodeEvent::Warning(format!("line {i}")));
        }
        assert_eq!(s.log.back().unwrap(), &format!("line {}", MAX_LOG_LINES + 4));
        assert!(!s.log.front().unwrap().contains("line 0"));
    }

    /// The relay verdict reaches the status pane as a verdict and not as a
    /// number, because "16 MiB" tells a player nothing and "cannot carry a hand"
    /// tells them everything.
    #[test]
    fn an_inadequate_relay_is_recorded_but_does_not_take_the_headline() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Reserved {
            relay: peer(),
            bytes: Some(131_072),
            seconds: Some(120),
            adequate: false,
        });
        // The verdict is kept, because seating depends on it...
        assert!(!s.status.relay.as_ref().unwrap().adequate);
        // ...and it is in the log, where a player can go looking...
        assert!(s.log.back().unwrap().contains("NOT enough"));
        // ...but it is not the headline. With no peers yet the headline is
        // still "Looking for peers", which is the plainest true thing: a relay
        // and nobody to play is not a relay problem.
        let line = s.view().status.summary();
        assert!(!line.contains("cannot carry a hand"), "{line}");
        assert_eq!(line, "Looking for peers");
    }

    /// A table nobody re-advertises must leave the interface's own lobby, and
    /// nothing else was ever going to tell it to.
    ///
    /// The sweep used to happen only inside the handler for an **incoming**
    /// advert, so the last table on a quiet network stayed on screen until the
    /// client was restarted — which is exactly the state a player is in when
    /// the founder closes their client.
    #[test]
    fn a_table_that_nobody_re_advertises_leaves_the_screen() {
        use crate::protocol::constants::AD_TTL_MS;

        let mut s = AppState::new();
        let now = 1_700_000_000_000u64;
        let ad = crate::net::lobby::TableAd::rated_sng(
            "Riverside".into(),
            [9u8; 32],
            vec![1, 2, 3],
            now,
        );
        let key = [7u8; 32];
        s.apply(NodeEvent::TableSeen {
            key,
            ad: Box::new(ad),
            params_hash: [1u8; 32],
            advert_hash: [2u8; 32],
        });
        assert_eq!(s.view().tables.len(), 1, "the table arrived");

        // Time passes and nobody says it again.
        s.apply(NodeEvent::Swept {
            now_ms: now + AD_TTL_MS + 1,
        });
        assert_eq!(s.view().tables.len(), 0, "and it is gone from the screen");
    }

    /// One advert with a future timestamp must not empty the screen.
    ///
    /// §7.2 rule 5 admits a timestamp up to `MAX_CLOCK_SKEW_MS` = 120 s ahead
    /// and `AD_TTL_MS` is 90 s, so an advert signed 120 s in the future used to
    /// carry this copy's expiry clock 120 s with it and drop every honest record
    /// in one call — leaving the attacker's table alone in the user's list. Its
    /// own re-broadcast rate limit is 20 a minute, so keeping the list empty
    /// cost one message every 30 s.
    #[test]
    fn a_future_advert_does_not_sweep_the_screen() {
        use crate::protocol::constants::MAX_CLOCK_SKEW_MS;

        let mut s = AppState::new();
        let now = 1_700_000_000_000u64;

        fn see(s: &mut AppState, name: &str, key: u8, at: u64) {
            s.apply(NodeEvent::TableSeen {
                key: [key; 32],
                ad: Box::new(crate::net::lobby::TableAd::rated_sng(
                    name.into(),
                    [key; 32],
                    vec![1, 2, 3],
                    at,
                )),
                params_hash: [1u8; 32],
                advert_hash: [key; 32],
            });
        }

        // A real clock arrives first, as it does in the running client: the
        // sweep is what tells this layer what time it is.
        s.apply(NodeEvent::Swept { now_ms: now });
        see(&mut s, "honest one", 1, now);
        see(&mut s, "honest two", 2, now);
        assert_eq!(s.view().tables.len(), 2, "both honest tables are on screen");

        // And now one signed as far ahead as the rules allow.
        see(&mut s, "far ahead", 3, now + MAX_CLOCK_SKEW_MS);

        assert_eq!(
            s.view().tables.len(),
            3,
            "a future timestamp expired the honest tables"
        );
        assert!(
            s.newest_seen <= now,
            "a stranger's timestamp moved this client's clock to {}",
            s.newest_seen
        );
    }

    /// A client that has heard nobody stops claiming to be playing — and a
    /// client that hears somebody again says so at once.
    ///
    /// `S1-BH`. The measured case: a seat that never entered the table's Tox
    /// group was certified out by the other nine, logged **not one mention of
    /// it** — the certificate is a hand event and rides the very group it
    /// cannot hear — and exited after 900 s printing `TABLE FORMED seats=10`.
    /// It had the fact all along: it prints `group 0 seen/0 confirmed/9 wanted`
    /// every housekeeping tick. It simply never acted on its own measurement.
    #[test]
    fn a_client_that_hears_nobody_does_not_claim_to_be_playing() {
        use crate::protocol::constants::DEAF_MS;

        let mut s = AppState::new();
        let now = 1_700_000_000_000u64;
        s.apply(NodeEvent::Swept { now_ms: now });
        s.apply(NodeEvent::Hosting { key: [7u8; 32] });
        s.apply(NodeEvent::TableReal {
            key: [7u8; 32],
            session: [3u8; 32],
        });

        let seat = |s: &AppState| s.seated.clone().expect("this client is seated");
        assert!(
            seat(&s).playing(now),
            "a table that has just formed is being played at"
        );

        // Nobody in the group, and the clock moves.
        s.apply(NodeEvent::Carrier { seen: 0, want: 9 });
        assert!(
            seat(&s).playing(now),
            "one reading of zero is a transient, not a verdict"
        );

        let later = now + DEAF_MS;
        assert!(
            !seat(&s).playing(later),
            "after DEAF_MS of hearing nobody this client still claimed to be playing"
        );
        assert!(seat(&s).deaf(later), "and it can say why");

        // And one voice is enough to take it all back.
        s.apply(NodeEvent::Carrier { seen: 1, want: 9 });
        assert!(
            seat(&s).playing(later),
            "a client that can hear the table again is playing at it again"
        );
    }

    #[test]
    fn reachability_reaches_the_status() {
        let mut s = AppState::new();
        assert_eq!(s.status.public, None, "unknown until AutoNAT answers");
        s.apply(NodeEvent::Reachability { public: false });
        assert_eq!(s.status.public, Some(false));
        assert!(s.view().status.summary().contains("no relay"));
    }

    /// The whole struct is a local view. Nothing in it enters a hash, which is
    /// D-012, and the test exists so that a later editor who adds a field is
    /// reminded rather than trusted.
    #[test]
    fn the_snapshot_is_a_local_view() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Discovered {
            hints: 7,
            dropped: 3,
        });
        // Two clients that heard different things hold different snapshots, and
        // that is correct rather than a fault.
        let mut other = AppState::new();
        other.apply(NodeEvent::Discovered {
            hints: 2,
            dropped: 0,
        });
        assert_ne!(s.log, other.log);
    }

    /// `S1-CS`: the table window reopened showed the last game. A hand that
    /// was in progress when the player left the table -- or was refused a
    /// seat, or sat down somewhere else -- is nobody's hand any more, and a
    /// window drawn from it shows cards from a table this client is not at.
    #[test]
    fn the_hand_does_not_follow_the_player_to_the_next_table() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 1 });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::HandBegan { hand_id: 4, button: 0, dealt_in: vec![0, 1] });
        s.apply(NodeEvent::CardsDealt { hand_id: 4, seats: vec![0, 1] });
        s.apply(NodeEvent::HoleCards { hand_id: 4, cards: [12, 13] });
        s.apply(NodeEvent::HandWaiting { hand_id: 4, seats: vec![0] });
        assert!(s.hand.is_some(), "a hand is in progress");

        s.apply(NodeEvent::LeftTable { why: "left the table".into() });
        assert!(s.hand.is_none(), "the hand left with the table");
        assert!(s.waiting_for.is_empty(), "and so did the seats it was waiting for");

        // Sitting down somewhere else, with a hand still on record from
        // before, is the same thing from the other side.
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 1 });
        s.apply(NodeEvent::HandBegan { hand_id: 5, button: 0, dealt_in: vec![0, 1] });
        s.apply(NodeEvent::Seated { key: [8u8; 32], seat: 3 });
        assert!(s.hand.is_none(), "a hand from another table is not this table's");

        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 3] });
        s.apply(NodeEvent::JoinRefused { reason: 1 });
        assert!(s.hand.is_none(), "a refused seat has no hand");
    }

    /// `S1-CS`: the decision clock runs for the seat to act and for nobody
    /// else, restarts when the turn moves, and stops with the hand.
    #[test]
    fn the_clock_runs_for_the_seat_to_act() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2] });
        assert_eq!(s.turn_seat, None);
        s.apply(NodeEvent::TableState {
            hand_id: 1, street: 0, pot: 30, to_act: Some(2), stacks: vec![1_000; 3], bets: vec![0, 10, 20], folded: vec![false; 3],
        });
        assert_eq!(s.turn_seat, Some(2));
        let started = s.turn_since.expect("a clock is running");
        s.apply(NodeEvent::NotYourTurn { hand_id: 1, seat: Some(2) });
        assert_eq!(s.turn_since, Some(started), "the same seat keeps its start");
        s.apply(NodeEvent::NotYourTurn { hand_id: 1, seat: Some(1) });
        assert_eq!(s.turn_seat, Some(1));
        assert!(s.turn_since.is_some());
        s.apply(NodeEvent::YourTurn {
            hand_id: 1, street: 0, to_call: 20, pot: 50, can_check: false, can_call: true, can_bet: false, can_raise: true, min_raise_to: 40, max_raise_to: 1_000,
        });
        assert_eq!(s.turn_seat, Some(0), "our own turn is our own clock");
        s.apply(NodeEvent::HandEnded { hand_id: 1, stacks: vec![1_000; 3], shown: vec![None; 3] });
        assert_eq!(s.turn_seat, None);
        assert_eq!(s.turn_since, None);
    }

    /// `S1-CS`: what the seats say reaches the pane under their seat, is
    /// bounded like the lobby's, and leaves with the table.
    #[test]
    fn table_lines_are_filed_under_the_seat_and_bounded() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        for i in 0..(MAX_CHAT_LINES + 5) {
            s.apply(NodeEvent::TableSaid { seat: 2, nickname: "Carol".into(), text: format!("line {i}") });
        }
        assert_eq!(s.table_chat.len(), MAX_CHAT_LINES);
        let last = s.table_chat.back().unwrap();
        assert_eq!(last.seat, 2);
        assert!(last.who.contains("Carol") && last.who.contains("seat 2"));
        assert_eq!(last.said, format!("line {}", MAX_CHAT_LINES + 4));
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: Some(120) });
        assert_eq!(s.links.get(&2).map(|(r, _)| *r), Some(Some(120)));
        s.apply(NodeEvent::LeftTable { why: "left the table".into() });
        assert!(s.table_chat.is_empty() && s.links.is_empty(), "the chat and the links went with the table");
    }

    /// `S1-CS`: a join ends with a seat, or with the reason it did not.
    #[test]
    fn a_join_ends_with_a_seat_or_a_reason() {
        let mut s = AppState::new();
        s.begin_join([7u8; 32], "Riverside".into(), 1_000, None);
        assert!(s.view().joining.as_ref().is_some_and(|j| j.failed.is_none() && j.name == "Riverside"));
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 3 });
        assert!(s.joining.is_none(), "a seat is the end of the join");

        s.begin_join([8u8; 32], "Elsewhere".into(), 1_000, None);
        s.apply(NodeEvent::JoinRefused { reason: 1 });
        assert!(s.joining.as_ref().unwrap().failed.as_deref().unwrap().contains("the table is full"));

        s.retry_join();
        assert!(s.joining.as_ref().unwrap().failed.is_none());
        s.apply(NodeEvent::LeftTable { why: "the founder did not answer".into() });
        assert!(s.joining.as_ref().unwrap().failed.as_deref().unwrap().contains("did not answer"));

        // And a join nobody answers at all fails on the clock.
        s.retry_join();
        s.joining.as_mut().unwrap().since = std::time::Instant::now() - std::time::Duration::from_millis(JOIN_WAIT_MS + 1);
        s.tick_join();
        assert!(s.joining.as_ref().unwrap().failed.as_deref().unwrap().contains("no answer"));
        s.joining = None;
        assert!(s.view().joining.is_none());
    }

    /// `S1-CX`: a heads-up opponent that cannot be reached is said once and
    /// asked about until the player answers; a reading that reaches them ends
    /// the episode, and a new one asks anew.
    #[test]
    fn a_heads_up_opponent_that_cannot_be_reached_is_said_and_asked_about() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        s.apply(NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "me".into(), 1_000), (1, "them".into(), 1_000)] });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None });
        assert!(s.opponent_gone.is_some(), "the closed connection opens an episode");
        assert_eq!(s.opponent_gone_for_s(), None, "not worth asking about yet");
        s.tick_opponent();
        assert!(!s.log.iter().any(|l| l.contains("unreachable")), "nor saying");

        s.opponent_gone.as_mut().unwrap().since = std::time::Instant::now() - std::time::Duration::from_millis(OPPONENT_GONE_MS + 1_000);
        assert!(s.opponent_gone_for_s().is_some_and(|secs| secs >= 16));
        s.tick_opponent();
        let line = s.log.back().unwrap().clone();
        assert!(line.contains("unreachable") && line.contains("D-007"), "{line}");
        s.tick_opponent();
        assert_eq!(s.log.iter().filter(|l| l.contains("unreachable")).count(), 1, "said once");

        s.dismiss_opponent_gone();
        assert_eq!(s.opponent_gone_for_s(), None, "the player chose to wait");
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(40) });
        assert!(s.opponent_gone.is_none(), "a ping ends the episode");
        s.apply(NodeEvent::Carrier { seen: 0, want: 1 });
        assert!(s.opponent_gone.is_none(), "an empty group is the other seat still joining it, not an opponent out of reach");
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None });
        assert!(s.opponent_gone.as_ref().is_some_and(|g| !g.dismissed), "a closed connection is a new episode, asked anew");
        s.apply(NodeEvent::Carrier { seen: 1, want: 1 });
        assert!(s.opponent_gone.is_some(), "a seat the group merely sees is not one this client can reach");
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(30) });
        assert!(s.opponent_gone.is_none(), "a ping is");
        // And a reading gone stale -- no ping for longer than the window doubts --
        // starts an episode by itself.
        s.links.insert(1, (Some(30), std::time::Instant::now() - std::time::Duration::from_millis(app_link_stale_ms() + 1_000)));
        s.tick_opponent();
        assert!(s.opponent_gone.is_some(), "a stale link is an unreachable opponent");
    }

    /// At three seats the certificate does the work (D-023) and no question is
    /// asked, whatever the links say.
    #[test]
    fn a_three_seat_table_asks_no_such_question() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        s.apply(NodeEvent::Roster {
            key: [7u8; 32],
            seats: vec![(0, "a".into(), 1_000), (1, "b".into(), 1_000), (2, "c".into(), 1_000)],
        });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None });
        s.apply(NodeEvent::Carrier { seen: 0, want: 2 });
        assert!(s.opponent_gone.is_none());
    }

    /// `D-032`: three returns and no more. Each absence worth asking about
    /// that ends is a return; a blip shorter than the question is not; at the
    /// fourth absence the question is final and nothing reachable undoes it.
    #[test]
    fn a_heads_up_opponent_may_return_three_times() {
        use crate::protocol::constants::MAX_RETURNS;
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        s.apply(NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "me".into(), 1_000), (1, "them".into(), 1_000)] });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        for n in 1..=MAX_RETURNS {
            s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None });
            s.opponent_gone.as_mut().unwrap().since = std::time::Instant::now() - std::time::Duration::from_millis(OPPONENT_GONE_MS + 1_000);
            s.tick_opponent();
            assert!(!s.opponent_out, "absence {n} is not the last");
            s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(40) });
            assert_eq!(s.opponent_returns, n, "return {n} counted");
            assert!(s.log.back().unwrap().contains("D-032"), "and said");
        }
        // A blip shorter than the question does not count.
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(40) });
        assert_eq!(s.opponent_returns, MAX_RETURNS, "a blip is no return");
        // The fourth absence is final.
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None });
        s.opponent_gone.as_mut().unwrap().since = std::time::Instant::now() - std::time::Duration::from_millis(OPPONENT_GONE_MS + 1_000);
        s.tick_opponent();
        assert!(s.opponent_out, "the fourth absence ends the game");
        let line = s.log.back().unwrap().clone();
        assert!(line.contains("D-032") && line.contains("limit"), "{line}");
        assert!(s.table_view().opponent_out, "the window is told");
        assert!(s.table_view().opponent_gone_s.is_some(), "and still shows how long");
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(40) });
        assert!(s.opponent_out, "and nothing reachable undoes it");
        assert_eq!(s.opponent_returns, MAX_RETURNS);
    }
}
