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
    /// What has happened, newest last.
    pub log: VecDeque<String>,
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
    /// The latest advertisement timestamp this client has seen.
    ///
    /// Used as the clock for this store's own expiry. Not this machine's clock:
    /// the two stores must agree about which adverts are alive, and a client
    /// whose clock ran fast would expire tables the node still held.
    pub newest_seen: u64,
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
    /// The table's name and shape, from the node rather than from the lobby.
    pub name: String,
    pub seats: u8,
    pub needed: u8,
    pub small_blind: u64,
    pub big_blind: u64,
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
                });
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
                self.log.push_back(match (shuffling, ready) {
                    (_, true) => format!("hand #{hand_id}: the deck is shuffled and sealed"),
                    (Some(s), _) => format!("hand #{hand_id}: seat {s} is shuffling"),
                    (None, false) => format!("hand #{hand_id}: the deck is being prepared"),
                });
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
                self.log.push_back(if to_call > 0 {
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
            }
            NodeEvent::Board { hand_id, cards } => {
                if let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) {
                    h.board = cards;
                }
            }
            NodeEvent::HandEnded {
                hand_id,
                stacks,
                shown,
            } => {
                if let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) {
                    h.stacks = stacks;
                    h.shown = shown;
                    h.over = true;
                    h.turn = None;
                    h.waiting_on = None;
                }
                self.log.push_back(format!("hand #{hand_id} is over"));
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
            NodeEvent::Swept { now_ms } => {
                self.last_sweep_ms = now_ms;
                // A player who has stopped saying they are here stops being
                // here. There is no goodbye message, because a client that is
                // switched off does not send one.
                self.players.retain(|_, (_, at)| {
                    now_ms.saturating_sub(*at) < crate::net::lobbytalk::PRESENCE_TTL_MS
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
                let now = ad.timestamp_unix_ms.max(self.newest_seen);
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
            NodeEvent::DialFailed { .. } => self.status.failed_dials += 1,
            NodeEvent::Warning(w) => self.note(w),

            NodeEvent::PortMapped { how, external } => {
                self.status.port_mapped = Some(how);
                self.note(format!("{how} opened port {external}"));
            }
            NodeEvent::Hosting { key } => {
                self.seated = Some(Seat {
                    key,
                    seat: Some(0),
                    hosting: true,
                    ..Default::default()
                });
                self.note(format!("hosting {}", short(&key)));
            }
            NodeEvent::Seated { key, seat } => {
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
            } => {
                let t = self.table(key);
                t.name = name;
                t.seats = seats;
                t.needed = needed;
                t.small_blind = small_blind;
                t.big_blind = big_blind;
            }
            NodeEvent::TableReal { key, session } => {
                self.table(key).session = Some(session);
                self.note(format!("the table is set: session {}", short(&session)));
            }
            NodeEvent::JoinRefused { reason } => {
                // The founder's claim, said as a claim. A rejection is never
                // proof of anything: §4.3 puts it plainly, and the founder may
                // simply not want this player.
                self.seated = None;
                self.note(format!("the founder says no: {}", refusal(reason)));
            }
            NodeEvent::LeftTable { why } => {
                self.seated = None;
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
    }

    /// The snapshot the panes read.
    pub fn view(&self) -> LobbyView {
        let mut v = LobbyView::from(&self.lobby, self.status.clone());
        v.selected = self.selected;
        v.log = self.log.iter().cloned().collect();
        v.me = self.me.clone();
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
}
