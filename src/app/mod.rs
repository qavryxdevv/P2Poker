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
use crate::net::node::{NodeCommand, NodeEvent};

/// `S1-CS`: the table window's view, derived here so it can be tested.
mod table;
mod tablelog;

/// The window's own stale-link threshold, for the opponent question.
fn app_link_stale_ms() -> u64 {
    table::LINK_STALE_MS
}

/// The founder's reason codes, §4.3, in the words a player can act on.
///
/// Two of the eight are never sent by this client because nothing in the corpus
/// gives them a mechanism; they are named here anyway, because another
/// implementation may send them and "reason 6" tells a player nothing.
/// `S1-EK`: where the window client writes every line of its log, once told.
/// The headless client prints the same lines; the window had nowhere to
/// keep them, and a report of *the window closed, I do not know why* could
/// not be read afterwards.
static LOG_FILE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// `S1-EK`: write the log to this file from now on -- appended, started
/// afresh past eight megabytes.
pub fn log_to(path: std::path::PathBuf) {
    let _ = LOG_FILE.set(path);
}

/// `D-053`: how large the client's log file may grow, in bytes.
pub const LOG_FILE_MAX: u64 = 1_048_576;

/// `D-053`: what survives a trim -- the **newest** half. The file used to be
/// emptied when it grew past its bound, so the lines a reader wanted (the ones
/// just before they looked) were exactly the ones thrown away.
pub const LOG_FILE_KEEP: u64 = LOG_FILE_MAX / 2;

/// `D-053`: whether a line belongs in the client's log file.
///
/// **The owner, 2026-09-14**: *the log is very spammy and it swells fast; keep
/// what matters -- certifications, security incidents from a rogue peer -- and
/// hold the file to a megabyte.*
///
/// It is a **drop list, not a keep list**, and that is the whole design: a line
/// nobody thought of is kept rather than silently lost, so the next fault to be
/// diagnosed from this file is still in it. What is dropped is the lobby's own
/// bookkeeping -- peers appearing and disappearing on the mesh, answers to
/// lobby questions, dials, relays, addresses -- which is nearly all of the
/// volume and none of the meaning: the table's traffic does not ride any of it,
/// and the status line already says what the lobby amounts to.
///
/// The **keep list sits above it** so that a line which matters is never
/// dropped for happening to hold a word the drop list names: anything a
/// decision tagged, every certificate, and every word about a member that was
/// cut off, removed, kicked or put out.
pub fn worth_logging(line: &str) -> bool {
    /// Kept whatever else the line says.
    const KEEP: [&str; 12] = [
        "(D-0",
        "certif",
        "the table's word",
        "flood",
        "not safe",
        "cut off",
        "out of the table for good",
        "no seat of this table",
        "removed from the group",
        "a kick",
        "stranger",
        "diverg",
    ];
    /// The lobby's bookkeeping, which is the volume.
    ///
    /// Two lines that look like this list are deliberately **not** in it:
    /// *could not join the public lobby* and *no other poker client has been
    /// reached yet* are this client saying it cannot be played with, which is
    /// the first thing a reader of this file wants to know.
    const DROP: [&str; 8] = [
        "joined the lobby mesh",
        // The space is load-bearing: without it this also matches *could not
        // jo-IN THE PUBLIC LOBBY*, which is the opposite of chatter.
        " in the public lobby",
        "lobby: answered",
        "is now a direct connection",
        "dial failed",
        "relay said no",
        "from the DHT refused",
        "NOT enough to carry a hand",
    ];
    KEEP.iter().any(|k| line.contains(k)) || !DROP.iter().any(|d| line.contains(d))
}

/// `D-053`: hold the file to [`LOG_FILE_MAX`] by keeping its newest
/// [`LOG_FILE_KEEP`] bytes, cut at a line boundary so no half line survives.
fn trim_log(path: &std::path::Path, len: u64) {
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let from = usize::try_from(len.saturating_sub(LOG_FILE_KEEP)).unwrap_or(0);
    let tail = &bytes[from.min(bytes.len())..];
    let cut = tail.iter().position(|b| *b == b'\n').map_or(0, |i| i + 1);
    let _ = std::fs::write(path, &tail[cut.min(tail.len())..]);
}

fn log_line(line: &str) {
    let Some(path) = LOG_FILE.get() else {
        return;
    };
    // `D-053`: the lobby's bookkeeping stays out of the file.
    if !worth_logging(line) {
        return;
    }
    // `D-053`: and a line saying itself over and over is counted, not repeated.
    // Written out when a different line comes, so a loop costs one line plus a
    // number instead of a megabyte. A client that exits mid-fold loses the
    // count and never a line.
    static LAST: std::sync::Mutex<Option<(String, u64)>> = std::sync::Mutex::new(None);
    // The lock is held to the end of this function on purpose: the fold, the
    // trim and the append are then one step, so two threads cannot both find
    // the file over its bound and cut it twice, and no line can be appended
    // between a trim's read and its write and go with it. A panic elsewhere
    // must not stop the client logging, so a poisoned lock is taken anyway.
    let mut last = LAST.lock().unwrap_or_else(|held| held.into_inner());
    let folded = match last.as_mut() {
        Some((prev, n)) if prev == line => {
            *n += 1;
            return;
        }
        Some((prev, n)) => {
            let folded = (*n > 0).then_some(*n);
            *prev = line.to_owned();
            *n = 0;
            folded
        }
        None => {
            *last = Some((line.to_owned(), 0));
            None
        }
    };
    use std::io::Write;
    if let Ok(m) = std::fs::metadata(path) {
        if m.len() > LOG_FILE_MAX {
            trim_log(path, m.len());
        }
    }
    let file = std::fs::OpenOptions::new().create(true).append(true).open(path);
    if let Ok(mut f) = file {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let secs = (now / 1000) % 86_400;
        let at = format!("{:02}:{:02}:{:02}.{:03}", secs / 3600, (secs / 60) % 60, secs % 60, now % 1000);
        if let Some(n) = folded {
            let _ = writeln!(f, "{at} the line above repeated {n} more time(s)");
        }
        let _ = writeln!(f, "{at} {line}");
    }
}

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
        9 => "this seat was removed after its fourth absence; the game at this table is over for good (D-047)",
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
    /// `S1-DG`: every stack as the hand began -- what a hand ended without a
    /// settlement restores. The last boundary's figures, taken when the hand
    /// began; the table's own reports move `last_stacks` during the hand.
    pub start_stacks: Vec<u64>,
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
    /// `D-052`: the pots the hand settled into -- the main pot first, then
    /// each side pot -- so a winner's badge can say which one it took and
    /// whether it was shared.
    pub pots: Vec<crate::net::node::PotEnd>,
    /// What each seat did last this betting round, by seat: the badge beside
    /// it. A new street clears every word but *fold* and *all in*, as PokerTH's
    /// engine does.
    pub acted: Vec<Option<crate::gui::table::SeatAct>>,
    /// The blinds of this hand are in the table's log.
    pub blinds_said: bool,
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

/// `D-043`: one of this client's tables, as the lobby lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotView {
    pub slot: u8,
    pub name: String,
    pub turn: bool,
    pub active: bool,
}

/// `S1-FG`: where this client is at a table the lobby offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Here {
    /// A seat at it, in this slot: its window is this client's.
    Seated(u8),
    /// A join to it under way.
    Joining,
}

/// `D-043`: one table's state, for a slot the window is not looking at.
/// The same fields `AppState` holds for the active slot; see `swap_slot`.
#[derive(Debug, Default)]
pub struct TableApp {
    pub seated: Option<Seat>,
    pub hand: Option<HandInProgress>,
    pub waiting_for: Vec<u8>,
    /// `D-058`: the seats the node said a hand's cryptographic stage has stood
    /// on for a moment, and which hand; empty once it moved on.
    pub stands: (u64, Vec<u8>),
    /// `S1-FM`: the hand the seats in `waiting_for` are owed for.
    pub waiting_hand: u64,
    /// `D-058`: the last hand this window heard end -- one it may never have
    /// held, called off at its opening before the deal.
    pub last_ended: Option<u64>,
    /// `S1-FL`: when the last hand ended here, by hand -- the window about the
    /// place waits from the hand's end, not from the node's word about it.
    pub hand_ended_at: Option<(u64, std::time::Instant)>,
    pub last_stacks: Vec<u64>,
    pub turns: u64,
    pub strength: Option<crate::poker::strength::Strength>,
    pub turn_seat: Option<u8>,
    pub turn_since: Option<std::time::Instant>,
    pub table_chat: VecDeque<TableLine>,
    pub muted: std::collections::BTreeSet<u8>,
    pub links: std::collections::BTreeMap<u8, (Option<u64>, bool, Option<u64>, std::time::Instant)>,
    pub opponent_gone: Option<OpponentGone>,
    pub opponent_was_reachable: bool,
    pub out_for_good: Option<String>,
    pub ever_on_line: bool,
    pub certified: std::collections::BTreeSet<u8>,
    pub opponent_returns: u8,
    pub opponent_out: bool,
    pub gone: std::collections::BTreeSet<u8>,
    pub left_for_good: std::collections::BTreeSet<u8>,
    pub opponent_left: bool,
    pub table_log: VecDeque<crate::gui::table::LogLine>,
    pub turn_warned: u64,
    pub sitting_out: bool,
    pub away: std::collections::BTreeSet<u8>,
    pub finished: Option<(crate::gui::table::Finish, std::time::Instant)>,
    pub show_choice: Option<(u64, std::time::Instant)>,
    pub out_flooded: bool,
    pub unsafe_note: Option<(String, u64)>,
    pub rejoin: Option<Rejoin>,
    pub waits: std::collections::BTreeMap<u8, SeatWait>,
}

/// Everything the client knows, in the form the panes read it.
#[derive(Debug, Default)]
pub struct AppState {
    /// `D-043`: the other tables this client sits at, by slot; the active
    /// slot's state is this struct's own fields.
    pub background: std::collections::BTreeMap<u8, TableApp>,
    /// The slot whose state the fields hold and the window shows.
    pub active_slot: u8,
    /// The slot the node's last `AtTable` named: where the next event lands.
    pub current_slot: u8,
    /// Every slot's table key, once it has one.
    pub slot_keys: std::collections::BTreeMap<u8, Option<[u8; 32]>>,
    /// The slots where it is this client's turn.
    pub turn_at: std::collections::BTreeSet<u8>,
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
    /// `D-058`: the seats the node said a hand's cryptographic stage has stood
    /// on for a moment, and which hand; empty once it moved on.
    pub stands: (u64, Vec<u8>),
    /// `S1-FM`: the hand the seats in `waiting_for` are owed for.
    pub waiting_hand: u64,
    /// `D-058`: the last hand this window heard end -- one it may never have
    /// held, called off at its opening before the deal.
    pub last_ended: Option<u64>,
    /// `S1-FL`: when the last hand ended here, by hand -- the window about the
    /// place waits from the hand's end, not from the node's word about it.
    pub hand_ended_at: Option<(u64, std::time::Instant)>,
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
    pub links: std::collections::BTreeMap<u8, (Option<u64>, bool, Option<u64>, std::time::Instant)>,
    /// `S1-CS`: the join in progress, if one is.
    pub joining: Option<Joining>,
    /// `S1-FG`: the lobby's word that a table asked for is one this client is at
    /// already. Shown only while that is still so (`view`).
    pub already_at: Option<[u8; 32]>,
    /// `S1-CX`: the heads-up opponent this client cannot reach, if any.
    pub opponent_gone: Option<OpponentGone>,
    /// `S1-EC`: whether the opponent has been on the line once at this
    /// table. Before that, *not on the line* is a seat still joining the
    /// group, not an absence: a game used to begin with a return spent and
    /// the question asked while the other seat was handshaking.
    pub opponent_was_reachable: bool,
    /// `D-047`: this seat is out of the table for good, and why; the window
    /// says so and holds the table until the player closes it.
    pub out_for_good: Option<String>,
    /// `S1-EH`: whether any other seat has been on the line at this table
    /// once -- before that, nobody reachable is a group still forming.
    pub ever_on_line: bool,
    /// `S1-EI`: the seats the table certified out of the running hand.
    pub certified: std::collections::BTreeSet<u8>,
    /// `D-032`: how many absences worth asking about ended with the
    /// opponent back; at `MAX_RETURNS` the next absence is final.
    pub opponent_returns: u8,
    /// `D-032`: the opponent's fourth absence; the game ends here, and
    /// nothing reachable undoes it.
    pub opponent_out: bool,
    /// `D-035`: seats whose client left the table's group, until seen there
    /// again.
    pub gone: std::collections::BTreeSet<u8>,
    pub left_for_good: std::collections::BTreeSet<u8>,
    /// `D-035`: the heads-up opponent quit the table on purpose; the game is
    /// over, and the one thing left to do is leave.
    pub opponent_left: bool,
    /// The table's history, PokerTH's *Log* panel: the hand's header, the
    /// blinds, every action, the streets, the showdown and the winners.
    pub table_log: VecDeque<crate::gui::table::LogLine>,
    /// The turn (`turns`) PokerTH's three-second warning was played for.
    pub turn_warned: u64,
    /// `D-049`: this client's seat sits out -- its own clock ran out, and the
    /// node checks or folds at once on every turn until the player is back.
    pub sitting_out: bool,
    /// `D-049`: the seats the table's group says sit out, as the last link
    /// reading of each had it -- only ever said of a seat on the line.
    pub away: std::collections::BTreeSet<u8>,
    /// The place this player finished the tournament in -- out of chips, or
    /// the winner -- and when the deciding hand ended.
    pub finished: Option<(crate::gui::table::Finish, std::time::Instant)>,
    /// `D-050`: this client's hand waits at the showdown for the player, who
    /// may show it: the hand, and until when.
    pub show_choice: Option<(u64, std::time::Instant)>,
    /// `D-051`: this seat is out for good for flooding the table's group, as
    /// against `D-047`'s fourth absence.
    pub out_flooded: bool,
    /// `D-051`: why this table is not safe, as the node last said it, with a
    /// serial the window closes by -- the question comes back when the node
    /// says it again.
    pub unsafe_note: Option<(String, u64)>,
    /// `D-057`: this client's own way back to the table, while it is under way.
    pub rejoin: Option<Rejoin>,
    /// `D-058`: the other seats this table waits on, or waited on and is taking
    /// back, by seat.
    pub waits: std::collections::BTreeMap<u8, SeatWait>,
    /// The sounds owed since the window last played them, in order.
    pub sound_cues: Vec<crate::sound::Cue>,
    /// How many games this client has sat down to since it started: PokerTH's
    /// *Game: N*.
    pub games: u32,
    /// What this player keeps about other players, by key: the stars under a
    /// name and the note behind them. Loaded by the window from the profile.
    pub notes: crate::storage::notes::Notes,
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
    /// Which game of this client's session this table is -- PokerTH's
    /// *Game: N* -- counted when the table is set; 0 before.
    pub game_no: u32,
    /// The application key behind each seat, for the ratings and notes this
    /// player keeps: never the name (`PROTOCOL.md` §4.3).
    pub keys: std::collections::BTreeMap<u8, [u8; 32]>,
    /// The small blind of the last hand and how many times it has gone up in
    /// this game, for PokerTH's blind raise sound.
    pub blinds_seen: (u64, u32),
    /// `D-059`: how many times each seat has made this table wait and the
    /// table's cryptographic step, by the node's count -- for the table's log
    /// and the wait panel.
    pub patience: std::collections::BTreeMap<u8, (u8, u64)>,
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
    /// `S1-DE`: this join is the rejoin of the game on record. The window
    /// says so, and a failed one offers to forget the record.
    pub rejoin: bool,
    /// `S1-DE`: the node said the session is gone, so there is nothing to
    /// try again; the window says why, and its one button closes it.
    pub gone: bool,
}

/// How long a join may go unanswered before the window calls it failed.
/// Longer than the node's own request timeout plus the time a founder has
/// been measured to take to become dialable.
pub const JOIN_WAIT_MS: u64 = 90_000;

/// `D-057`: this client's own way back to the table, as far as it has come --
/// from the moment its line went, the table's group fell silent or its seat
/// was certified out, until it is dealt in again.
#[derive(Debug, Clone)]
pub struct Rejoin {
    pub since: std::time::Instant,
    /// The library said the Tox network is unreachable, rather than only the
    /// table's group falling silent.
    pub line_lost: bool,
    /// The network is back, by the library's word.
    pub network_back: bool,
    /// A seat of the table can be reached from here again, and since when.
    pub reached: bool,
    pub reached_at: Option<std::time::Instant>,
    /// This client's seat was certified out of a hand while it was away.
    pub certified_out: bool,
    /// Following the table's running hand as a bystander: which.
    pub following: Option<u64>,
    /// Asked to sit in at that hand's end.
    pub asked: Option<u64>,
    /// Dealt in again, or never out: when.
    pub back_at: Option<std::time::Instant>,
    /// `S1-FR`: this client came back after a restart, from its session record.
    pub restarted: bool,
}

impl Rejoin {
    fn begin() -> Rejoin {
        Rejoin {
            since: std::time::Instant::now(),
            line_lost: false,
            network_back: false,
            reached: false,
            reached_at: None,
            certified_out: false,
            following: None,
            asked: None,
            back_at: None,
            restarted: false,
        }
    }
}

/// `D-058`: a seat this table waits on -- off the line in a hand that deals it
/// in -- or goes on without, or takes back: the votes about it, the hand it was
/// certified out of, and, if it comes back, its sit-in and the votes on its
/// return.
///
/// **Said while the table is not being played, and never over a hand that is**
/// (the owner, 2026-09-14, three times). The felt shows a wait while the hand
/// stands on the seat, and a seat certified out until the table is played
/// again ([`AppState::wait_views`]); a seat on its way back is said at its seat
/// and in the table's log.
#[derive(Debug, Clone)]
pub struct SeatWait {
    pub since: std::time::Instant,
    /// How the votes about it stand, while the hand stands on it: held, needed.
    pub votes: Option<(u8, u8)>,
    /// The hand it was certified out of, and whether that hand was played on
    /// after it -- the turn at a seat on the line -- which is where saying so
    /// ends; otherwise the next hand's cards end it.
    pub certified_in: Option<u64>,
    pub played_on: bool,
    /// The certificate acted for it at its turn: the turn this window still
    /// shows at the seat is spent until the turn moves.
    pub turn_spent: bool,
    /// Asked to sit in at the end of this hand.
    pub asked: Option<u64>,
    /// How the votes on its return stand: held, needed.
    pub return_votes: Option<(u8, u8)>,
    /// The latest hand its return was heard at: a return that goes quiet is
    /// not said past the hand after it.
    pub return_hand: Option<u64>,
}

/// `D-058`: what a hand stands on at a seat -- its turn, or a cryptographic
/// stage it owes. Anything less is a hand the others are still playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stands {
    Turn,
    Stage,
}

/// `D-058`: whether the hand stands on `seat`, and how. Its turn -- one no
/// certificate has acted on yet -- while it is off the line or the votes about
/// it run; or a stage: the node's word that the stage has stood on it for a
/// moment (heard or not), and before the deal the hand's own word the instant
/// the seat is off the line. A free function, so the sweep can ask it inside
/// its retain and the felt outside.
fn stands_on(seat: u8, w: &SeatWait, turn: Option<u8>, off: bool, stands: &[u8], waiting: &[u8]) -> Option<Stands> {
    if turn == Some(seat) && !w.turn_spent && (off || w.votes.is_some()) {
        return Some(Stands::Turn);
    }
    if stands.contains(&seat) || (off && waiting.contains(&seat)) {
        return Some(Stands::Stage);
    }
    None
}

impl SeatWait {
    /// A wait that began `ago` before now: the node's word about a stage comes
    /// after its moment, and the clock on the panel counts from the stall.
    fn begun(ago: std::time::Duration) -> SeatWait {
        let mut w = SeatWait::begin();
        w.since = std::time::Instant::now().checked_sub(ago).unwrap_or(w.since);
        w
    }

    fn begin() -> SeatWait {
        SeatWait {
            since: std::time::Instant::now(),
            votes: None,
            certified_in: None,
            played_on: false,
            turn_spent: false,
            asked: None,
            return_votes: None,
            return_hand: None,
        }
    }

    /// On its way back: it asked to sit in, or the seats vote on its return.
    fn returning(&self) -> bool {
        self.asked.is_some() || self.return_votes.is_some()
    }

    /// Heard on its way back, at hand `hand`.
    fn heard_return(&mut self, hand: Option<u64>) {
        if let Some(k) = hand {
            self.return_hand = Some(self.return_hand.map_or(k, |r| r.max(k)));
        }
    }
}

/// How long *Back in the game* stays over the felt.
const REJOIN_BACK_SHOWN: std::time::Duration = std::time::Duration::from_secs(4);

/// How long after the table is reachable again a seat never put out counts as
/// back in the game.
const REJOIN_SETTLES: std::time::Duration = std::time::Duration::from_secs(6);

/// `S1-CX`: a heads-up opponent this client cannot reach, since when,
/// whether the log has said so and whether the player has answered.
///
/// D-007: at two seats nobody can fold a hand for an absent player and
/// the remedy is to leave the table -- so the window asks, and asks again
/// every `OPPONENT_ASK_AGAIN_MS` while the player waits (D-046), and a
/// headless client writes the same sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpponentGone {
    pub since: std::time::Instant,
    pub said: bool,
    /// `D-046`: when the player last chose to wait; `None` while the
    /// question stands.
    pub dismissed_at: Option<std::time::Instant>,
    /// `D-034`: the opponent is on the line but long past their time to
    /// decide -- D-007's other case, asked about with the same question and
    /// said as what it is. Ends when they act; no absence and no return.
    pub slow: bool,
    /// `S1-EI`: not one opponent but everybody else, at a bigger table --
    /// heads-up's question with more chairs, since nobody can certify
    /// anybody alone (D-036).
    pub alone: bool,
}

/// How long an opponent must be unreachable before it is said: longer
/// than a reconnection takes, shorter than a player's patience.
pub const OPPONENT_GONE_MS: u64 = 15_000;

/// `D-046`: how long a player's *Wait* holds before the question about an
/// opponent still out of reach is asked again -- the owner's "a few tens of
/// seconds", so that a wait is never for ever.
pub const OPPONENT_ASK_AGAIN_MS: u64 = 30_000;

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
    /// `D-043`: every event lands on the slot the node's last `AtTable`
    /// named. The active slot's state is this struct's own fields, as it
    /// always was; another slot's is swapped in for the event and out
    /// again, so the rest of this file never learns there is more than one.
    pub fn apply(&mut self, event: NodeEvent) {
        if let NodeEvent::AtTable { slot, key } = event {
            self.current_slot = slot;
            self.slot_keys.insert(slot, key);
            if slot != self.active_slot && !self.background.contains_key(&slot) {
                self.background.insert(slot, TableApp::default());
            }
            return;
        }
        match &event {
            NodeEvent::YourTurn { .. } => {
                self.turn_at.insert(self.current_slot);
            }
            NodeEvent::NotYourTurn { .. } | NodeEvent::HandEnded { .. } | NodeEvent::LeftTable { .. } => {
                self.turn_at.remove(&self.current_slot);
            }
            _ => {}
        }
        let left = matches!(event, NodeEvent::LeftTable { .. });
        let slot = self.current_slot;
        if slot == self.active_slot || !self.background.contains_key(&slot) {
            self.apply_here(event);
            return;
        }
        self.swap_slot(slot);
        self.apply_here(event);
        self.swap_slot(slot);
        if left {
            self.background.remove(&slot);
            self.slot_keys.remove(&slot);
        }
    }

    /// Exchange the fields with a background slot's state.
    fn swap_slot(&mut self, slot: u8) {
        let Some(other) = self.background.get_mut(&slot) else {
            return;
        };
        std::mem::swap(&mut self.seated, &mut other.seated);
        std::mem::swap(&mut self.hand, &mut other.hand);
        std::mem::swap(&mut self.waiting_for, &mut other.waiting_for);
        std::mem::swap(&mut self.stands, &mut other.stands);
        std::mem::swap(&mut self.waiting_hand, &mut other.waiting_hand);
        std::mem::swap(&mut self.last_ended, &mut other.last_ended);
        std::mem::swap(&mut self.hand_ended_at, &mut other.hand_ended_at);
        std::mem::swap(&mut self.last_stacks, &mut other.last_stacks);
        std::mem::swap(&mut self.turns, &mut other.turns);
        std::mem::swap(&mut self.strength, &mut other.strength);
        std::mem::swap(&mut self.turn_seat, &mut other.turn_seat);
        std::mem::swap(&mut self.turn_since, &mut other.turn_since);
        std::mem::swap(&mut self.table_chat, &mut other.table_chat);
        std::mem::swap(&mut self.muted, &mut other.muted);
        std::mem::swap(&mut self.links, &mut other.links);
        std::mem::swap(&mut self.opponent_gone, &mut other.opponent_gone);
        std::mem::swap(&mut self.opponent_was_reachable, &mut other.opponent_was_reachable);
        std::mem::swap(&mut self.out_for_good, &mut other.out_for_good);
        std::mem::swap(&mut self.ever_on_line, &mut other.ever_on_line);
        std::mem::swap(&mut self.certified, &mut other.certified);
        std::mem::swap(&mut self.opponent_returns, &mut other.opponent_returns);
        std::mem::swap(&mut self.opponent_out, &mut other.opponent_out);
        std::mem::swap(&mut self.gone, &mut other.gone);
        std::mem::swap(&mut self.left_for_good, &mut other.left_for_good);
        std::mem::swap(&mut self.opponent_left, &mut other.opponent_left);
        std::mem::swap(&mut self.table_log, &mut other.table_log);
        std::mem::swap(&mut self.turn_warned, &mut other.turn_warned);
        std::mem::swap(&mut self.sitting_out, &mut other.sitting_out);
        std::mem::swap(&mut self.away, &mut other.away);
        std::mem::swap(&mut self.finished, &mut other.finished);
        std::mem::swap(&mut self.show_choice, &mut other.show_choice);
        std::mem::swap(&mut self.out_flooded, &mut other.out_flooded);
        std::mem::swap(&mut self.unsafe_note, &mut other.unsafe_note);
        std::mem::swap(&mut self.rejoin, &mut other.rejoin);
        std::mem::swap(&mut self.waits, &mut other.waits);
    }

    /// `D-043`: turn to another of this client's tables: its state becomes
    /// the fields, the fields become its background entry.
    pub fn switch_to(&mut self, slot: u8) {
        if slot == self.active_slot || !self.background.contains_key(&slot) {
            return;
        }
        self.swap_slot(slot);
        if let Some(was_active) = self.background.remove(&slot) {
            self.background.insert(self.active_slot, was_active);
        }
        self.active_slot = slot;
    }

    /// `D-043`: the felt of one slot, the active one or another. `D-058`: the
    /// seats the table waits on are read afresh first, every frame.
    pub fn table_view_of(&mut self, slot: u8) -> crate::gui::table::TableView {
        if slot == self.active_slot || !self.background.contains_key(&slot) {
            self.tick_waits();
            return self.table_view();
        }
        self.swap_slot(slot);
        self.tick_waits();
        let view = self.table_view();
        self.swap_slot(slot);
        view
    }

    /// `D-043`: every slot this client sits at -- the number, the table's
    /// name, whether it is this client's turn there, whether it is the active
    /// one -- the active slot first.
    pub fn slots(&self) -> Vec<SlotView> {
        let mut out = Vec::new();
        if let Some(s) = self.seated.as_ref() {
            out.push(SlotView {
                slot: self.active_slot,
                name: s.name.clone(),
                turn: self.turn_at.contains(&self.active_slot),
                active: true,
            });
        }
        for (slot, other) in self.background.iter() {
            if let Some(s) = other.seated.as_ref() {
                out.push(SlotView {
                    slot: *slot,
                    name: s.name.clone(),
                    turn: self.turn_at.contains(slot),
                    active: false,
                });
            }
        }
        out
    }

    fn apply_here(&mut self, event: NodeEvent) {
        match event {
            // Taken in `apply`, before anything lands anywhere.
            NodeEvent::AtTable { .. } => {}
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
            // `S1-EH`: this client's own line to the Tox network, said on change.
            NodeEvent::ToxLine { how } => {
                let before = self.status.tox.replace(how);
                // Noted past the client's start only: the library says *offline*
                // until the DHT answers, which is nobody's line going away.
                if how == "offline" && before.is_some_and(|b| b != "offline") {
                    self.note("the Tox network is unreachable from here: the table's hands ride it".into());
                    // `D-057`: this client's own line went.
                    if let Some(r) = self.rejoin_begins() {
                        r.line_lost = true;
                        r.network_back = false;
                        r.reached = false;
                        r.back_at = None;
                    }
                } else if how != "offline" && before == Some("offline") && self.ever_on_line {
                    self.note(format!("the Tox network is reachable again ({how})"));
                    if let Some(r) = self.rejoin.as_mut() {
                        r.network_back = true;
                    }
                }
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
            NodeEvent::SessionResumed { hand_id, member } => {
                self.unfinished = None;
                self.note(format!("back at the table: following hand #{hand_id}"));
                // `D-057`: in the table's group and caught up with its hand --
                // dealt in it, or following it to sit in at its end.
                let r = self.rejoin.get_or_insert_with(Rejoin::begin);
                r.network_back = true;
                r.reached = true;
                r.reached_at.get_or_insert_with(std::time::Instant::now);
                if member {
                    r.back_at.get_or_insert_with(std::time::Instant::now);
                } else {
                    r.certified_out = true;
                    r.following = Some(hand_id);
                    r.back_at = None;
                }
            }
            NodeEvent::TableReach { reachable } => {
                // `D-057`: nobody at the table answers -- this client's own line,
                // most likely, at three seats or more; at two the question about
                // the opponent (`D-046`) is the word, and this panel stays out.
                if reachable {
                    if let Some(r) = self.rejoin.as_mut() {
                        r.network_back = true;
                        r.reached = true;
                        r.reached_at.get_or_insert_with(std::time::Instant::now);
                    }
                } else if self.seated.as_ref().is_some_and(|s| s.roster.len() >= 3) {
                    if let Some(r) = self.rejoin_begins() {
                        r.reached = false;
                        r.reached_at = None;
                        r.back_at = None;
                    }
                }
            }
            NodeEvent::TimeoutVotes { seat, held, need } => {
                if self.seated.as_ref().and_then(|s| s.seat) != Some(seat) {
                    self.waits.entry(seat).or_insert_with(SeatWait::begin).votes = Some((held, need));
                }
            }
            NodeEvent::ReturnVotes { seat, held, need } => {
                if self.seated.as_ref().and_then(|s| s.seat) != Some(seat) {
                    let hand = self.hand.as_ref().map(|h| h.hand_id);
                    let w = self.waits.entry(seat).or_insert_with(SeatWait::begin);
                    w.heard_return(hand);
                    w.return_votes = Some((held, need));
                }
            }
            // `D-059`: a seat made the table wait, and the table waits less for it
            // at a cryptographic step from now on.
            NodeEvent::SeatWaited { seat, waits, step_ms } => {
                if let Some(s) = self.seated.as_mut() {
                    s.patience.insert(seat, (waits, step_ms));
                }
                let name = self.seat_name(seat);
                let cuts = crate::protocol::constants::MAX_PATIENCE_CUTS;
                let next_s = crate::table::hand::patience_ms(step_ms, waits) / 1_000;
                self.log_table(
                    crate::gui::table::LogKind::SitOut,
                    if waits <= cuts {
                        format!("{name} made the table wait ({waits} of {cuts}): {next_s} s at a step from now on")
                    } else {
                        format!("{name} made the table wait again: {next_s} s at a step")
                    },
                );
            }
            NodeEvent::SitInAsked { seat, hand_id } if self.seated.as_ref().and_then(|s| s.seat) != Some(seat) => {
                // `D-058`: another seat on its way back -- said at its seat, and
                // once per boundary in the table's log.
                let name = self.seat_name(seat);
                let w = self.waits.entry(seat).or_insert_with(SeatWait::begin);
                let fresh = w.asked != Some(hand_id);
                w.heard_return(Some(hand_id));
                w.asked = Some(hand_id);
                if fresh {
                    self.log_table(crate::gui::table::LogKind::SitOut, format!("{name} asks to sit in after hand #{hand_id}"));
                }
            }
            NodeEvent::SitInAsked { hand_id, .. } => {
                if let Some(r) = self.rejoin.as_mut() {
                    r.network_back = true;
                    r.reached = true;
                    r.certified_out = true;
                    r.asked = Some(hand_id);
                }
            }
            NodeEvent::SessionGaveUp { why } => {
                self.unfinished = None;
                // `S1-DE`: a rejoin in progress ends here, with the reason
                // where the player is looking; there is nothing to try again.
                if let Some(j) = self.joining.as_mut().filter(|j| j.rejoin) {
                    j.failed = Some(why.clone());
                    j.gone = true;
                }
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
                // PokerTH's *lobby chat notification*: somebody else named this
                // player.
                if !self.me.is_empty()
                    && nickname != self.me
                    && text.to_lowercase().contains(&self.me.to_lowercase())
                {
                    self.sound_cues.push(crate::sound::Cue::LobbyChatNotify);
                }
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
                // `D-054`: the name is kept as the name, and nothing else. It
                // used to be written into one string with the seat --
                // *"{nickname} (seat {seat})"* -- and a name is whatever the
                // other client typed, so a seat could call itself *Alice (seat
                // 0)* and read as another player's line. The seat is drawn by
                // the window from this field, which the signature decided.
                self.table_chat.push_back(TableLine {
                    seat,
                    who: nickname,
                    said: text,
                });
            }
            NodeEvent::SeatLink { seat, rtt_ms, group, quiet_s, away } => {
                // `D-049`: the seat's word that it sits out, as the group has
                // it -- never kept for a seat off the line, so nothing of it
                // outlives the seat's presence.
                let away = away && group;
                let was_away = self.away.contains(&seat);
                if !group {
                    // Off the line: the word is not kept, and nothing is said
                    // about it -- the seat has not come back, it is not there.
                    self.away.remove(&seat);
                } else if away != was_away && self.seated.as_ref().is_some_and(|s| s.seat != Some(seat)) {
                    if away {
                        self.away.insert(seat);
                    } else {
                        self.away.remove(&seat);
                    }
                    let name = self.seat_name(seat);
                    self.log_table(
                        crate::gui::table::LogKind::SitOut,
                        if away { format!("{name} sits out") } else { format!("{name} is back") },
                    );
                }
                // `D-041`: on the line when the table's group holds the seat,
                // or when a ping answered; the group is the reading that
                // matters where the hand rides it.
                let reachable = rtt_ms.is_some() || group;
                if group {
                    self.ever_on_line = true;
                }
                self.links.insert(seat, (rtt_ms, group, quiet_s, std::time::Instant::now()));
                // `D-035`: a reading from the table's group is a seat back in it.
                if reachable {
                    self.gone.remove(&seat);
                }
                if self.heads_up_opponent() == Some(seat) {
                    self.opponent_reachable(reachable);
                }
            }
            // `D-035`: a seat's client left the table's group, or the table's word
            // removed it. `S1-FQ`: gone from the felt, and no more -- the carrier
            // says *on purpose* of a client that merely rejoins the group (the
            // owner killed a heads-up opponent twice and was told the game was
            // over), so only the table's removal is for good, and only the seat's
            // own word (`SeatLeftTable`) is a player who left.
            NodeEvent::SeatLeft { seat, quit, removed } => {
                // `S1-EE`: never the player's own seat -- the player is here, and no
                // reading about their own seat would ever clear it.
                if self.seated.as_ref().and_then(|s| s.seat) == Some(seat) {
                    self.note(format!("a seat-left about this seat itself ({seat}) is ignored (S1-EE)"));
                    return;
                }
                self.gone.insert(seat);
                if removed {
                    self.left_for_good.insert(seat);
                }
                self.note(if removed {
                    format!("seat {seat} is out of the table for good: removed by the table's word")
                } else if quit {
                    format!("seat {seat} left the table's group -- the carrier says on purpose, which a client rejoining the group says too")
                } else {
                    format!("seat {seat} left the table's group: its connection timed out")
                });
            }
            // `S1-FQ`, the owner's word (2026-09-15): *game over with "opponent
            // left" only provably, when the last opponent actively leaves the
            // table* -- the seat's own signed word, and nothing else.
            NodeEvent::SeatLeftTable { seat } => {
                if self.seated.as_ref().and_then(|s| s.seat) == Some(seat) {
                    return;
                }
                // `S1-EJ`: read before the seat is marked, since a seat left for
                // good is no longer in the game the reading is about.
                let was_opponent = self.heads_up_opponent() == Some(seat);
                self.gone.insert(seat);
                self.left_for_good.insert(seat);
                let name = self.seat_name(seat);
                self.log_table(crate::gui::table::LogKind::SitOut, format!("{name} left the table"));
                self.note(format!("seat {seat} left the table: its own word"));
                if was_opponent && !self.opponent_out {
                    self.opponent_out = true;
                    self.opponent_left = true;
                    self.opponent_gone = Some(OpponentGone {
                        since: std::time::Instant::now()
                            .checked_sub(std::time::Duration::from_millis(OPPONENT_GONE_MS))
                            .unwrap_or_else(std::time::Instant::now),
                        said: true,
                        dismissed_at: None,
                        slow: false,
                        alone: false,
                    });
                    self.note("your opponent left the table (D-035): its own word -- the game is over, leave the table".into());
                }
            }
            NodeEvent::HandBegan {
                hand_id,
                button,
                dealt_in,
                small_blind,
                big_blind,
            } => {
                // `D-050`: a new hand waits for nobody's word about the last.
                self.show_choice = None;
                // The hand's own blinds, as its init derived them from the
                // tournament's schedule: the status bar showed the starting
                // ones for the whole game (the owner, 2026-09-13).
                if let Some(s) = self.seated.as_mut() {
                    if big_blind > 0 {
                        let (seen, level) = s.blinds_seen;
                        if big_blind > seen {
                            if seen > 0 {
                                s.blinds_seen = (big_blind, level.saturating_add(1));
                                if let Some(cue) = crate::sound::Cue::blinds_raised(level.saturating_add(1)) {
                                    self.sound_cues.push(cue);
                                }
                            } else {
                                s.blinds_seen = (big_blind, 0);
                            }
                        }
                        s.small_blind = small_blind;
                        s.big_blind = big_blind;
                    }
                }
                self.hand = Some(HandInProgress {
                    hand_id,
                    button,
                    dealt_in,
                    start_stacks: self.last_stacks.clone(),
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
                    pots: Vec::new(),
                    acted: Vec::new(),
                    blinds_said: false,
                });
                self.strength = None;
                self.waiting_for.clear();
                self.stands = (0, Vec::new());
                self.certified.clear();
                // `D-057`: dealt in again after the way back.
                let me = self.seated.as_ref().and_then(|s| s.seat);
                if let (Some(r), Some(me)) = (self.rejoin.as_mut(), me) {
                    let dealt = self.hand.as_ref().is_some_and(|h| h.dealt_in.contains(&me));
                    if dealt && r.reached && (r.certified_out || r.following.is_some() || r.asked.is_some()) {
                        r.back_at.get_or_insert_with(std::time::Instant::now);
                    }
                }
                // `D-058`: another seat dealt in again after its return, said under
                // the hand's header; and the votes to act for a seat were the hand
                // just over's.
                let mut back_in: Vec<u8> = Vec::new();
                if let Some(h) = self.hand.as_ref() {
                    self.waits.retain(|seat, w| {
                        w.votes = None;
                        if w.returning() && h.dealt_in.contains(seat) {
                            back_in.push(*seat);
                            return false;
                        }
                        true
                    });
                }
                self.log_hand_began(hand_id);
                for seat in back_in {
                    let name = self.seat_name(seat);
                    self.log_table(crate::gui::table::LogKind::SitOut, format!("{name} is back in the game"));
                }
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
                self.log_blinds(hand_id);
                self.clock_for(to_act, 0);
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
                elapsed_ms,
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
                self.clock_for(me, elapsed_ms);
                self.note(if to_call > 0 {
                    format!("hand #{hand_id}: your turn — {to_call} to call")
                } else {
                    format!("hand #{hand_id}: your turn")
                });
            }
            NodeEvent::NotYourTurn { hand_id, seat, elapsed_ms } => {
                if let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) {
                    h.turn = None;
                    h.waiting_on = seat;
                }
                self.clock_for(seat, elapsed_ms);
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
                let before = self
                    .hand
                    .as_ref()
                    .filter(|h| h.hand_id == hand_id)
                    .map(|h| h.board.len())
                    .unwrap_or(0);
                if let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) {
                    h.board = cards;
                }
                self.log_street(hand_id, before);
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
                pots,
                gained,
            } => {
                // `S1-DG`: a hand that ended without a settlement -- the
                // deadline, a certificate -- restores every stack to the
                // hand's start, and the engine reports no stacks for it. Those
                // are the stacks this window held at the last boundary; taking
                // the empty list showed every seat's buy-in until the next
                // deal.
                let restored = stacks.is_empty();
                let stacks = if restored {
                    self.hand
                        .as_ref()
                        .filter(|h| h.hand_id == hand_id && !h.start_stacks.is_empty())
                        .map(|h| h.start_stacks.clone())
                        .unwrap_or_else(|| self.last_stacks.clone())
                } else {
                    stacks
                };
                if let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) {
                    // `S1-ER`: what each seat gained is the **engine's** own
                    // figure -- it read it off the stacks the settlement
                    // replaced, which is the only place both sides of it are
                    // certain. `S1-CS` had the window subtract the last state
                    // it was told from this one, and the node reported a state
                    // after the settlement in the very call that ends the hand,
                    // so every difference came out zero: no winner marked at
                    // any seat, no chips flying and no line in the table's log.
                    // The difference stays as the fallback, for a hand whose
                    // engine named no gain.
                    let before: &[u64] = if h.stacks_now.is_empty() {
                        &self.last_stacks
                    } else {
                        &h.stacks_now
                    };
                    let won: Vec<u64> = if restored {
                        vec![0; stacks.len()]
                    } else if gained.iter().any(|g| *g > 0) {
                        gained
                    } else {
                        stacks
                            .iter()
                            .enumerate()
                            .map(|(i, s)| s.saturating_sub(before.get(i).copied().unwrap_or(*s)))
                            .collect()
                    };
                    h.won = won;
                    // `D-052`: which pot each winner took, for the badge.
                    h.pots = pots;
                    h.stacks = stacks.clone();
                    h.shown = shown;
                    h.over = true;
                    h.turn = None;
                    h.waiting_on = None;
                    h.bets = vec![0; h.bets.len()];
                }
                self.last_stacks = stacks;
                self.clock_for(None, 0);
                // `D-058`: nothing stands on anybody in a hand that is over -- and
                // the hand is over, whether this window ever held it or not.
                self.waiting_for.clear();
                if self.stands.0 == hand_id {
                    self.stands.1.clear();
                }
                self.last_ended = Some(self.last_ended.map_or(hand_id, |e| e.max(hand_id)));
                self.hand_ended_at = Some((hand_id, std::time::Instant::now()));
                // A hand called off after a seat was certified out of it leaves
                // nothing moving on the felt until the next hand's cards -- said,
                // even where the hand had been played on in between.
                if restored {
                    for w in self.waits.values_mut() {
                        if w.certified_in == Some(hand_id) {
                            w.played_on = false;
                        }
                    }
                }
                if !restored {
                    self.log_showdown(hand_id);
                }
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
                // `D-057`, the owner's word: the way back is over the moment this
                // client holds its cards again -- nothing over a hand it plays.
                if self.hand.as_ref().is_some_and(|h| h.hand_id == hand_id) {
                    self.rejoin = None;
                }
                self.refresh_strength();
                self.sound_cues.push(crate::sound::Cue::DealTwoCards);
                self.log
                    .push_back(format!("hand #{hand_id}: your cards are dealt"));
            }
            // `S1-EI`: the table certified a seat out of the running hand.
            NodeEvent::SeatCertified { seat, hand_id } => {
                self.certified.insert(seat);
                // `D-057`: this client's own seat, put out of the hand while away.
                if self.seated.as_ref().and_then(|s| s.seat) == Some(seat) {
                    if let Some(r) = self.rejoin_begins() {
                        r.certified_out = true;
                    }
                } else {
                    // `D-058`: another: the table goes on without it, and says so over
                    // the felt until the table is played again -- the turn at a seat
                    // on the line, or the next hand's cards -- and in the log. The
                    // hand is the certificate's own, by the node's word: this window
                    // may hold no hand of that number (a certificate at the hand's
                    // opening, before the deal), or hear of it after the hand ended.
                    let me = self.seated.as_ref().and_then(|s| s.seat);
                    let live = self.hand.as_ref().is_some_and(|h| !h.over && h.hand_id == hand_id);
                    let live_turn = live
                        && self.turn_seat.is_some_and(|t| {
                            t != seat && (Some(t) == me || self.links.get(&t).is_some_and(|(_, group, _, _)| *group))
                        });
                    let turn_here = self.turn_seat == Some(seat);
                    // The certificate answers the stall: nothing stands on the seat.
                    self.waiting_for.retain(|s| *s != seat);
                    if self.stands.0 == hand_id {
                        self.stands.1.retain(|s| *s != seat);
                    }
                    let w = self.waits.entry(seat).or_insert_with(SeatWait::begin);
                    if !w.returning() {
                        w.votes = None;
                        w.certified_in = Some(hand_id);
                        w.played_on = live_turn;
                        w.turn_spent = turn_here;
                    }
                    let name = self.seat_name(seat);
                    self.log_table(
                        crate::gui::table::LogKind::SitOut,
                        format!("{name} is certified out of this hand; the table plays on without it"),
                    );
                }
                self.note(format!(
                    "seat {seat} certified out of this hand: the hand goes on among the seats on the line"
                ));
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
                self.waiting_hand = hand_id;
            }
            // `S1-FL`: a seat's place in the tournament, decided at the boundary.
            NodeEvent::Finished { hand_id, seat, place, tied, players_left, over } => {
                self.seat_finished(hand_id, seat, usize::from(place), tied, usize::from(players_left), over);
            }
            // `S1-FG`: the node did not start the join asked for. At a table this
            // client sits at, the lobby says so and the join goes; otherwise the
            // join window says why -- and its Cancel gives up whatever the node
            // still holds of that table.
            NodeEvent::JoinNotStarted { key, why, already_here } => {
                let seated_here = matches!(self.at_table(&key), Some(Here::Seated(_)));
                if self.joining.as_ref().is_some_and(|j| j.key == key) {
                    if already_here && seated_here {
                        self.joining = None;
                        self.already_at = Some(key);
                    } else if let Some(j) = self.joining.as_mut() {
                        j.failed = Some(why.clone());
                    }
                }
                self.note(why);
            }
            // `D-058`: the seats a hand's cryptographic stage stands on, by the
            // node's word after its moment -- before the deal as after it, so no
            // hand of this window's is asked for. Each gets its entry now, the
            // clock counting from the stall: the sweep, thirty seconds apart,
            // used to be the only thing writing them.
            NodeEvent::StageStands { hand_id, seats } => {
                let me = self.seated.as_ref().and_then(|s| s.seat);
                for seat in seats.iter().filter(|s| Some(**s) != me) {
                    self.waits.entry(*seat).or_insert_with(|| {
                        SeatWait::begun(std::time::Duration::from_millis(crate::net::node::STAGE_STANDS_AFTER_MS))
                    });
                }
                self.stands = (hand_id, seats);
                self.turn_moved();
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
                self.tick_waits();
                // `D-057`: *Back in the game* for a moment, then nothing; and no
                // way back at a table this client no longer plays at.
                // Back in the table's group with nothing more to do -- the seat
                // was never put out -- is back in the game once no catching-up
                // has followed for a moment (a founder's adoption comes seconds
                // after it is reached, `fe181646-3`).
                if let Some(r) = self.rejoin.as_mut() {
                    let settled = r.reached_at.is_some_and(|at| at.elapsed() >= REJOIN_SETTLES);
                    if settled && !r.certified_out && r.following.is_none() && r.asked.is_none() {
                        r.back_at.get_or_insert_with(std::time::Instant::now);
                    }
                }
                if self.rejoin.as_ref().is_some_and(|r| r.back_at.is_some_and(|at| at.elapsed() >= REJOIN_BACK_SHOWN))
                    // `D-057`, the owner's word: not over a hand it holds cards in.
                    || (self.rejoin.as_ref().is_some_and(|r| r.back_at.is_some()) && self.holds_cards())
                    || self.finished.is_some()
                    || self.out_for_good.is_some()
                {
                    self.rejoin = None;
                }
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
            // `D-040`: withdrawn by its founder's own answer, so it leaves the
            // screen now rather than when its advert would have expired.
            NodeEvent::TableGone { key, why } => {
                if self.lobby.remove(&key) {
                    self.note(format!("table {} gone: {why}", crate::gui::lobby::short_key(&key)));
                }
            }
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
                // `S1-FR`: a founder back from its record is on its way back.
                let restarted = self.joining.as_ref().is_some_and(|j| j.key == key && j.rejoin);
                self.forget_the_table();
                self.seated = Some(Seat {
                    key,
                    seat: Some(0),
                    hosting: true,
                    ..Default::default()
                });
                self.note(format!("hosting {}", short(&key)));
                if restarted {
                    self.rejoin = Some(Rejoin { restarted: true, ..Rejoin::begin() });
                }
            }
            NodeEvent::Seated { key, seat } => {
                // `S1-FR`: read before the join is ended -- a seat taken up again
                // from the session record, after a restart.
                let restarted = self.joining.as_ref().is_some_and(|j| j.key == key && j.rejoin);
                // `S1-CS`: a seat is the end of the join that asked for it.
                if self.joining.as_ref().is_some_and(|j| j.key == key) {
                    self.joining = None;
                }
                // `S1-DG`: said with every roster now; noted when it changes.
                let t = self.table(key);
                if t.seat != Some(seat) {
                    t.seat = Some(seat);
                    self.note(format!("seat {seat} at {}", short(&key)));
                }
                // `S1-FR`, the owner (2026-09-15): the window of a client back after
                // a restart said only *the players are joining its group (0 of 1
                // in)* -- the words for a table before its first hand -- and nothing
                // about its own way back. The way back is shown from the seat on,
                // as after a lost line (`D-057`).
                if restarted && self.rejoin.is_none() {
                    self.rejoin = Some(Rejoin { restarted: true, ..Rejoin::begin() });
                }
            }
            NodeEvent::Roster { key, seats } => {
                let n = seats.len();
                let t = self.table(key);
                // PokerTH's *network game notification*: somebody sat down
                // while the table fills; the table set says the rest.
                let grew = t.session.is_none() && !t.roster.is_empty() && n > t.roster.len();
                t.roster = seats;
                if grew {
                    self.sound_cues.push(crate::sound::Cue::PlayerConnected);
                }
                self.note(format!("{n} seated"));
            }
            NodeEvent::RosterKeys { key, keys } => {
                self.table(key).keys = keys.into_iter().collect();
            }
            NodeEvent::SeatActed {
                hand_id,
                seat,
                action,
                put_in,
                total,
                all_in,
                by_table,
            } => {
                self.seat_acted(hand_id, seat, action, put_in, total, all_in, by_table);
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
                // Said once per session: a seat back from a restart hears the
                // table set again with every ratification copy the members
                // answer with, and the log read *the table is set* twenty
                // times over (`run163147-3`).
                let t = self.table(key);
                if t.session != Some(session) {
                    t.session = Some(session);
                    if t.game_no == 0 {
                        self.games = self.games.saturating_add(1);
                        let games = self.games;
                        self.table(key).game_no = games;
                        // PokerTH's *network game notification*: the game is
                        // ready to start.
                        self.sound_cues.push(crate::sound::Cue::OnlineGameReady);
                    }
                    self.note(format!("the table is set: session {}", short(&session)));
                }
            }
            NodeEvent::JoinRefused { reason } => {
                // The founder's claim, said as a claim. A rejection is never
                // proof of anything: §4.3 puts it plainly, and the founder may
                // simply not want this player.
                self.forget_the_table();
                self.seated = None;
                if let Some(j) = self.joining_of_this_slot() {
                    j.failed = Some(format!("the founder says no: {}", refusal(reason)));
                }
                self.note(format!("the founder says no: {}", refusal(reason)));
            }
            // `D-050`: the showdown waits for this player's word, or no longer.
            NodeEvent::ShowdownChoice { hand_id, open_ms } => {
                self.show_choice =
                    open_ms.map(|ms| (hand_id, std::time::Instant::now() + std::time::Duration::from_millis(ms)));
            }
            // `D-049`: this client's seat sits out, or the player is back.
            NodeEvent::SittingOut { on } => {
                if self.sitting_out != on {
                    self.sitting_out = on;
                    self.note(if on {
                        "sitting out: your clock ran out; your seat checks or folds at once until you are back".to_string()
                    } else {
                        "back at the table".to_string()
                    });
                }
            }
            // `D-047`: this seat is out of the table for good. The table is held
            // for the window to say so; leaving is the player's click.
            NodeEvent::OutForGood { key: _, why, flooded } => {
                self.out_for_good = Some(why.clone());
                self.out_flooded = flooded;
                self.note(why);
            }
            // `D-051`: a seat this client cut off for flooding the table's group.
            NodeEvent::SeatFlooded { seat } => {
                let name = self.seat_name(seat);
                let line = format!("{name} flooded the table's connection with junk traffic and is cut off for good");
                self.log_table(crate::gui::table::LogKind::SitOut, line.clone());
                self.note(line);
            }
            // `D-051`: the table is not safe, or safe again.
            NodeEvent::TableUnsafe { why } => {
                self.unsafe_note = why.map(|w| {
                    let serial = self.unsafe_note.as_ref().map_or(1, |(_, n)| n + 1);
                    (w, serial)
                });
                if let Some((w, _)) = self.unsafe_note.as_ref() {
                    let line = format!("This table is not safe: {w}");
                    self.log_table(crate::gui::table::LogKind::SitOut, line.clone());
                    self.note(line);
                }
            }
            NodeEvent::LeftTable { why } => {
                self.forget_the_table();
                self.seated = None;
                if let Some(j) = self.joining_of_this_slot() {
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
        log_line(&line);
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
        self.opponent_was_reachable = false;
        self.out_for_good = None;
        self.ever_on_line = false;
        self.certified.clear();
        self.opponent_returns = 0;
        self.opponent_out = false;
        self.gone.clear();
        self.left_for_good.clear();
        self.opponent_left = false;
        self.clock_for(None, 0);
        self.table_chat.clear();
        self.muted.clear();
        self.links.clear();
        self.hand = None;
        self.waiting_for.clear();
        self.stands = (0, Vec::new());
        self.waiting_hand = 0;
        self.last_ended = None;
        self.hand_ended_at = None;
        self.last_stacks.clear();
        self.strength = None;
        self.table_log.clear();
        self.turn_warned = 0;
        self.sitting_out = false;
        self.away.clear();
        self.finished = None;
        self.show_choice = None;
        self.out_flooded = false;
        self.unsafe_note = None;
        self.rejoin = None;
        self.waits.clear();
    }

    /// `S1-FL`: a seat's place, by the node's word at the boundary (the owner,
    /// 2026-09-13: out of chips, the window says the place; the last seat
    /// holding chips has won and is congratulated; either way the window waits
    /// `FINISH_WINDOW_DELAY_MS` after the hand that decided it). The window
    /// used to work the place out itself, from its own copies of the stacks --
    /// which a client back from a restart did not hold, so the tie between
    /// seats busted in one hand was read off the buy-ins -- and only for a
    /// hand it was dealt into, so a seat blinded out while sitting out was
    /// told nothing (the owner, 2026-09-15: *not reliable*).
    fn seat_finished(&mut self, hand_id: u64, seat: u8, place: usize, tied: bool, players_left: usize, over: bool) {
        let me = self.seated.as_ref().and_then(|s| s.seat);
        let name = self.seat_name(seat);
        let words = crate::gui::table::ordinal(place);
        let said = if tied { format!("tied for {words} place") } else { format!("in {words} place") };
        if Some(seat) == me && self.finished.is_none() {
            // From the hand's end: the deciding hand is looked at first.
            let at = self
                .hand_ended_at
                .filter(|(id, _)| *id == hand_id)
                .map(|(_, at)| at)
                .unwrap_or_else(std::time::Instant::now);
            // Out of chips while others play on: soon, before the next hand is
            // under way. The winner, or the loser of the last hand: the ten
            // seconds to look at the hand that ended the tournament.
            let delay = if over { crate::gui::table::FINISH_WINDOW_DELAY_MS } else { crate::gui::table::BUST_WINDOW_DELAY_MS };
            self.finished = Some((crate::gui::table::Finish { place, tied, players_left, show_in_ms: delay }, at));
            if place == 1 && over {
                self.note("you won the tournament".to_string());
            } else {
                self.note(format!("out of chips: finished {said}"));
            }
        }
        if place == 1 && over {
            // The winner's line is the log's own, *wins game N!*, when the
            // showdown wrote it; a tournament ended beside an absent seat's
            // chips wrote none.
            if !self.table_log.iter().rev().take(12).any(|l| matches!(l.kind, crate::gui::table::LogKind::GameWin)) {
                let game = self.seated.as_ref().map(|s| s.game_no).unwrap_or(0).max(1);
                self.log_table(crate::gui::table::LogKind::GameWin, format!("{name} wins game {game}!"));
            }
        } else {
            self.log_table(crate::gui::table::LogKind::SitOut, format!("{name} finished {said}"));
        }
    }

    /// `S1-CX`: the other seat, when this table is heads-up and set.
    fn heads_up_opponent(&self) -> Option<u8> {
        let s = self.seated.as_ref()?;
        if s.session.is_none() {
            return None;
        }
        let me = s.seat?;
        // `S1-EJ`: the seats still in the game -- not left for good, not out of
        // chips -- and not how many the table was founded for: a table founded
        // for more is heads-up once the others are gone (the owner: the question
        // never came at a table with dead seats).
        // `S1-FO`: and while a hand is under way, a seat that began it with chips
        // or plays in it is in the game -- an all-in seat has nothing behind
        // and plays on. The owner's game (2026-09-15): one seat all in at a hand
        // of three read as out, the third seat as the only opponent, and the
        // table's removal of that third seat as *your opponent left the table,
        // the game is over*.
        let under_way = self.hand.as_ref().filter(|h| !h.over);
        let live: Vec<u8> = s
            .roster
            .iter()
            .filter(|(n, _, stack)| {
                let i = usize::from(*n);
                *n != me
                    && !self.left_for_good.contains(n)
                    && (self.last_stacks.get(i).copied().unwrap_or(*stack) > 0
                        || under_way.is_some_and(|h| {
                            h.start_stacks.get(i).is_some_and(|c| *c > 0)
                                || (h.dealt_in.contains(n) && !h.folded.get(i).copied().unwrap_or(false))
                        }))
            })
            .map(|(n, _, _)| *n)
            .collect();
        (live.len() == 1).then(|| live[0])
    }

    /// A reading about the heads-up opponent: reachable clears the episode,
    /// unreachable starts one if none is open.
    fn opponent_reachable(&mut self, reachable: bool) {
        // `D-032`: the fourth absence is final.
        if self.opponent_out {
            return;
        }
        if reachable {
            self.opponent_was_reachable = true;
            // `D-032`: an absence worth asking about that ended is a return.
            if self
                .opponent_gone
                .take()
                .is_some_and(|g| !g.slow && g.since.elapsed().as_millis() as u64 >= OPPONENT_GONE_MS)
            {
                self.opponent_returns = self.opponent_returns.saturating_add(1);
                self.note(format!(
                    "your opponent is back: return {} of {} (D-032)",
                    self.opponent_returns,
                    crate::protocol::constants::MAX_RETURNS
                ));
            }
        // `S1-EJ`: or any seat was -- the opponent may have become the opponent
        // only when the others left for good, having been on the line before
        // it was one (S1-EC's guard is against a group still forming).
        } else if self.opponent_gone.is_none() && (self.opponent_was_reachable || self.ever_on_line) {
            // `S1-EC`: only an opponent that has been on the line can be out of
            // reach; before that the seat is still joining (S1-CX said so for
            // the group's count, and this reading needed the same rule).
            self.opponent_gone = Some(OpponentGone {
                since: std::time::Instant::now(),
                said: false,
                dismissed_at: None,
                slow: false,
                alone: false,
            });
        }
    }

    /// Called every frame and on every sweep: an opponent unreachable for
    /// `OPPONENT_GONE_MS` is said once, in D-007's words.
    /// `S1-EI`: at three seats or more, whether every other seat is off the
    /// line at once, once one was on it -- nobody can certify anybody alone,
    /// so it is heads-up's question (D-007) with more chairs.
    fn alone_at_table(&self) -> bool {
        let Some(s) = self.seated.as_ref() else {
            return false;
        };
        if s.session.is_none() || s.roster.len() < 3 || !self.ever_on_line {
            return false;
        }
        let Some(me) = s.seat else {
            return false;
        };
        let others: Vec<u8> = s.roster.iter().map(|(n, _, _)| *n).filter(|n| *n != me).collect();
        !others.is_empty()
            && others
                .iter()
                .all(|n| self.links.get(n).is_some_and(|(_, group, _, _)| !*group))
    }

    /// `D-058`: the seats this table waits on, goes on without, or takes back,
    /// read afresh before every frame of the felt and on every sweep.
    ///
    /// **Said while the table is not being played, and never over a hand that
    /// is** (the owner, 2026-09-14, four times: *Waiting for* had stood over a
    /// table dealing on without a seat that had left; then *coming back* over a
    /// live hand; then, once that was gone, the certificate's word went at once
    /// and the table stood still and silent until the next hand; then the panel
    /// came *not reliably*, and went before the next hand was dealt). A seat off
    /// the line is kept while a running hand deals it in; whatever the hand
    /// stands on is kept from the moment it does; certified out, a seat is kept
    /// until the table is played again -- the turn at a seat on the line, or the
    /// next hand's cards; on its way back it is kept, for its seat to say, until
    /// it is dealt in again and not past the hand after the one its return was
    /// last heard at.
    ///
    /// **Before every frame**, from [`AppState::table_view_of`]: the sweep is
    /// thirty seconds apart, and while it was the only thing writing these
    /// entries a stall that began after one was said at the next, or never.
    pub fn tick_waits(&mut self) {
        let me = self.seated.as_ref().and_then(|s| s.seat);
        let roster: Vec<u8> = self.seated.as_ref().map(|s| s.roster.iter().map(|(n, _, _)| *n).collect()).unwrap_or_default();
        if self.finished.is_some() || self.out_for_good.is_some() || roster.is_empty() {
            self.waits.clear();
            return;
        }
        for a in self.absent_seats() {
            self.waits.entry(a.seat).or_insert_with(SeatWait::begin);
        }
        // Whatever the hand stands on, from the moment it does: the node's word
        // about a stage, the hand's own word before the deal about a seat off
        // the line, the turn at a seat off the line.
        let standing: Vec<u8> = self
            .stands
            .1
            .iter()
            .copied()
            .chain(self.waiting_for.iter().copied().filter(|s| self.link_off(*s)))
            .chain(self.turn_seat.filter(|t| self.link_off(*t)))
            .filter(|s| Some(*s) != me)
            .collect();
        for seat in standing {
            self.waits.entry(seat).or_insert_with(SeatWait::begin);
        }
        let hand = self.hand.as_ref().map(|h| (h.hand_id, h.over, h.dealt_in.clone()));
        let cards_out = self.cards_out_in();
        let (left, links, turn) = (&self.left_for_good, &self.links, self.turn_seat);
        let (stands, waiting) = (&self.stands.1, &self.waiting_for);
        self.waits.retain(|seat, w| {
            if Some(*seat) == me {
                return false;
            }
            if w.returning() {
                return roster.contains(seat)
                    && !left.contains(seat)
                    && match &hand {
                        Some((id, _, _)) => *id <= *w.return_hand.get_or_insert(*id) + 1,
                        None => true,
                    };
            }
            // The table goes on without it: until it is played again, whatever
            // the roster says of the seat meanwhile.
            if w.certified_in.is_some_and(|k| !w.played_on && cards_out.is_none_or(|id| id <= k)) {
                return true;
            }
            let off = links.get(seat).is_some_and(|(_, group, _, _)| !*group);
            if stands_on(*seat, w, turn, off, stands, waiting).is_some() {
                return true;
            }
            // Off the line in a running hand that deals it in: kept, so its clock
            // counts from the first moment should the hand come to stand on it.
            roster.contains(seat)
                && hand.as_ref().is_some_and(|(_, over, dealt)| !*over && dealt.contains(seat))
                && (off || w.votes.is_some())
        });
    }

    /// `S1-FM`: the stall the felt shows is the table's first hand, still
    /// opening -- by the node's word about its stage, or by the hand's own before
    /// this window has held any hand of this table.
    fn first_hand_opening(&self) -> bool {
        (!self.stands.1.is_empty() && self.stands.0 == 1) || (self.waiting_hand == 1 && self.hand.is_none())
    }

    /// A reading of the table's group that says the seat is off the line. No
    /// reading at all is neither: the seats of a table still forming are not
    /// waited on over the felt (`S1-EI`'s rule).
    fn link_off(&self, seat: u8) -> bool {
        self.links.get(&seat).is_some_and(|(_, group, _, _)| !*group)
    }

    /// The hand whose cards are out -- this client's, or anybody's -- if the
    /// running hand's are.
    fn cards_out_in(&self) -> Option<u64> {
        self.hand.as_ref().filter(|h| h.cards.is_some() || !h.holding.is_empty()).map(|h| h.hand_id)
    }

    /// `D-058`: every seat the hand stands on, and every seat the table goes on
    /// without while it is not being played, as the felt shows them -- and no
    /// other: a hand the others are playing is not covered, and a seat on its
    /// way back is said at its seat.
    pub fn wait_views(&self) -> Vec<crate::gui::table::WaitView> {
        use crate::gui::table::StepState::{Done, Later, Now};
        use crate::gui::table::{RejoinView, WaitView};
        let Some(seated) = self.seated.as_ref() else {
            return Vec::new();
        };
        // `S1-EI`'s rule: a set table whose group has held a seat once.
        if seated.session.is_none() || !self.ever_on_line {
            return Vec::new();
        }
        // This client's own way back is the only panel while it is under way.
        if self.rejoin.is_some() {
            return Vec::new();
        }
        let state = |done: bool, now: bool| if done { Done } else if now { Now } else { Later };
        let cards_out = self.cards_out_in();
        let mut stalls = Vec::new();
        let mut gaps = Vec::new();
        for (seat, w) in &self.waits {
            if w.returning() {
                continue;
            }
            let name = seated
                .roster
                .iter()
                .find(|(n, _, _)| n == seat)
                .map(|(_, name, _)| name.clone())
                .unwrap_or_else(|| format!("Seat {seat}"));
            let off = self.link_off(*seat);
            let quiet = self.links.get(seat).and_then(|(_, _, q, _)| *q);
            let first = if off { format!("{name} is off the line") } else { format!("{name} stopped answering") };
            match stands_on(*seat, w, self.turn_seat, off, &self.stands.1, &self.waiting_for) {
                Some(Stands::Turn) => {
                    let on_clock = self.turn_since.map(|t| t.elapsed().as_secs());
                    let steps = vec![
                        (Done, first),
                        (
                            state(w.votes.is_some(), true),
                            match on_clock {
                                Some(secs) if w.votes.is_none() => format!("The hand waits on its clock – {secs} s"),
                                _ => "The hand waits on its clock".to_string(),
                            },
                        ),
                        (
                            state(false, w.votes.is_some()),
                            match w.votes {
                                Some((held, need)) => format!("The other seats agree to act for it – {held} of {need}"),
                                None => "The other seats agree to act for it".to_string(),
                            },
                        ),
                        (Later, "Certified out: the table checks or folds for it".to_string()),
                    ];
                    let detail = if w.votes.is_some() {
                        "Its clock ran out; when every other seat agrees, the table checks or folds for it and the hand goes on".to_string()
                    } else {
                        format!(
                            "{name} has not answered{}; when its clock runs out the other seats certify it out and play on. Nothing here is stuck",
                            quiet.map(|q| format!(" for {q} s")).unwrap_or_default()
                        )
                    };
                    stalls.push(WaitView {
                        title: format!("Waiting for {name}"),
                        panel: RejoinView { for_s: w.since.elapsed().as_secs(), steps, detail: Some(detail), back: false },
                    });
                }
                // `S1-FM`: the table's first hand, still opening. Before it a seat
                // that is not in the table's group yet is not a seat that went
                // quiet -- it is joining, and the table gives it a minute more
                // than any other stage (the owner, 2026-09-15).
                Some(Stands::Stage) if self.first_hand_opening() => {
                    let in_group = self.links.get(seat).is_some_and(|(_, group, _, _)| *group);
                    let steps = vec![
                        (
                            state(in_group, true),
                            if in_group {
                                format!("{name} is in the table's group")
                            } else {
                                format!("{name} joins the table's group – {} s", w.since.elapsed().as_secs())
                            },
                        ),
                        (state(false, in_group && w.votes.is_none()), format!("{name} signs the first hand's opening")),
                        (
                            state(false, w.votes.is_some()),
                            match w.votes {
                                Some((held, need)) => format!("The other seats agree to deal without it – {held} of {need}"),
                                None => "The other seats agree to deal without it".to_string(),
                            },
                        ),
                        (Later, "The first hand is dealt without it".to_string()),
                    ];
                    let detail = if w.votes.is_some() {
                        format!("{name} did not arrive in time; when every other seat agrees, the first hand is dealt without it")
                    } else if in_group {
                        format!("{name} is in the table's group; the first hand opens as soon as its part arrives. Nothing here is stuck")
                    } else {
                        format!(
                            "{name} is not in the table's group yet; before the first hand the table gives it a minute more than usual to join, then deals without it. Nothing here is stuck"
                        )
                    };
                    stalls.push(WaitView {
                        title: format!("Waiting for {name}"),
                        panel: RejoinView { for_s: w.since.elapsed().as_secs(), steps, detail: Some(detail), back: false },
                    });
                }
                Some(Stands::Stage) => {
                    let steps = vec![
                        (Done, first),
                        (state(w.votes.is_some(), true), "The cards cannot move on without it".to_string()),
                        (
                            state(false, w.votes.is_some()),
                            match w.votes {
                                Some((held, need)) => format!("The other seats agree to end the hand – {held} of {need}"),
                                None => "The other seats agree to end the hand".to_string(),
                            },
                        ),
                        (Later, "The next hand is dealt without it".to_string()),
                    ];
                    // `D-059`: and how much the table waits for it at a step now.
                    let patience = seated
                        .patience
                        .get(seat)
                        .map(|(waits, step_ms)| {
                            format!(
                                ". It has made the table wait {}: {} s at a step from now on",
                                match waits {
                                    1 => "once".to_string(),
                                    2 => "twice".to_string(),
                                    n => format!("{n} times"),
                                },
                                crate::table::hand::patience_ms(*step_ms, *waits) / 1_000
                            )
                        })
                        .unwrap_or_default();
                    let detail = if w.votes.is_some() {
                        format!("Its time ran out; when every other seat agrees, this hand ends and the next is dealt without it{patience}")
                    } else {
                        format!(
                            "The cards need {name}'s part{}; when its time runs out the other seats end this hand and deal the next without it{patience}. Nothing here is stuck",
                            quiet.map(|q| format!(", silent {q} s")).unwrap_or_default()
                        )
                    };
                    stalls.push(WaitView {
                        title: format!("Waiting for {name}"),
                        panel: RejoinView { for_s: w.since.elapsed().as_secs(), steps, detail: Some(detail), back: false },
                    });
                }
                None => {
                    if let Some(k) = w.certified_in.filter(|k| !w.played_on && cards_out.is_none_or(|id| id <= *k)) {
                        let over = self.hand.as_ref().is_some_and(|h| h.hand_id > k || h.over)
                            || self.last_ended.is_some_and(|e| e >= k);
                        gaps.push(WaitView {
                            title: format!("The table goes on without {name}"),
                            panel: RejoinView {
                                for_s: w.since.elapsed().as_secs(),
                                steps: vec![
                                    (Done, first),
                                    (Done, format!("Certified out of hand #{k}")),
                                    (state(over, true), format!("Hand #{k} finishes without it")),
                                    (state(false, over), "The next hand is dealt without it".to_string()),
                                ],
                                detail: Some(format!(
                                    "The table goes on without {name}; it can come back at a later hand. Nothing here is stuck"
                                )),
                                back: false,
                            },
                        });
                    }
                }
            }
        }
        stalls.extend(gaps);
        stalls
    }

    /// `D-057`: this client's own way back, as the felt shows it: every step,
    /// the one under way, and what it waits on.
    pub fn rejoin_view(&self) -> Option<crate::gui::table::RejoinView> {
        use crate::gui::table::StepState::{Done, Later, Now};
        let r = self.rejoin.as_ref()?;
        let back = r.back_at.is_some();
        // `D-057`, the owner's word: *Back in the game* is not said over a hand
        // this client holds its cards in.
        if back && self.holds_cards() {
            return None;
        }
        let state = |done: bool, now: bool| if done { Done } else if now { Now } else { Later };
        let mut steps = Vec::new();
        let first_done = back || r.reached || (r.line_lost && r.network_back) || r.restarted;
        steps.push((
            state(first_done, true),
            if r.restarted {
                "Coming back after a restart".to_string()
            } else if r.line_lost {
                "Connection to the network lost".to_string()
            } else {
                "Nobody at the table answers".to_string()
            },
        ));
        if r.line_lost {
            steps.push((state(back || r.network_back || r.reached, false), "Network back".to_string()));
        }
        steps.push((state(back || r.reached, first_done), "Back in the table's group".to_string()));
        if r.certified_out || r.following.is_some() || r.asked.is_some() {
            let following = match r.following.or(r.asked) {
                Some(h) => format!("Following hand #{h}, played on without this seat"),
                None => "Following the hand played on without this seat".to_string(),
            };
            steps.push((state(back || r.asked.is_some(), r.reached), following));
            let asked = match r.asked {
                Some(h) => format!("Asked to sit in after hand #{h}"),
                None => "Asking to sit in when that hand ends".to_string(),
            };
            steps.push((state(back, r.asked.is_some()), asked));
        }
        steps.push((state(back, false), "Back in the game".to_string()));

        let detail = if back {
            Some("Dealt in again".to_string())
        } else if r.line_lost && !r.network_back {
            self.line_message().or_else(|| Some("No connection to the Tox network: the table's hands ride it".to_string()))
        } else if !r.reached {
            Some(
                "Waiting for the table's group to take this seat back: the other seats offer it every few seconds, and this client takes the offer as soon as it can"
                    .to_string(),
            )
        } else if r.asked.is_some() {
            Some("The other seats agree to the return at the end of this hand; the next hand deals this seat in".to_string())
        } else if r.certified_out || r.following.is_some() {
            Some("The table played on while this seat was away; this client catches up with the running hand and asks to sit in at its end".to_string())
        } else {
            None
        };
        Some(crate::gui::table::RejoinView { for_s: r.since.elapsed().as_secs(), steps, detail, back })
    }

    /// `S1-FG`: the join this slot's word is about -- the join to the table the
    /// slot held, by the node's last marker, or any join when the slot held no
    /// table. The end of one table is not the end of a join to another.
    fn joining_of_this_slot(&mut self) -> Option<&mut Joining> {
        let key = self.slot_keys.get(&self.current_slot).copied().flatten();
        self.joining.as_mut().filter(|j| key.is_none_or(|k| k == j.key))
    }

    /// Whether this client holds its cards in a hand still being played.
    fn holds_cards(&self) -> bool {
        self.hand.as_ref().is_some_and(|h| !h.over && h.cards.is_some())
    }

    /// `D-057`: the way back starts -- this client's line went, its table stopped
    /// answering at a table of three or more, or its seat was certified out.
    fn rejoin_begins(&mut self) -> Option<&mut Rejoin> {
        let playing = self.seated.as_ref().is_some_and(|s| s.session.is_some())
            && self.ever_on_line
            && self.finished.is_none()
            && self.out_for_good.is_none()
            && !self.opponent_left;
        if !playing {
            return None;
        }
        Some(self.rejoin.get_or_insert_with(Rejoin::begin))
    }

    /// `S1-EI`: the other seats off the line during a running hand, with what
    /// happens about each -- so a table that waits does not look frozen.
    pub fn absent_seats(&self) -> Vec<crate::gui::table::AbsentSeat> {
        let Some(s) = self.seated.as_ref() else {
            return Vec::new();
        };
        let Some(h) = self.hand.as_ref() else {
            return Vec::new();
        };
        if s.session.is_none() || !self.ever_on_line || h.over {
            return Vec::new();
        }
        let Some(me) = s.seat else {
            return Vec::new();
        };
        s.roster
            .iter()
            .filter(|(n, _, _)| *n != me && h.dealt_in.contains(n))
            .filter_map(|(n, name, _)| {
                let (_, group, quiet_s, _) = self.links.get(n)?;
                if *group {
                    return None;
                }
                Some(crate::gui::table::AbsentSeat {
                    seat: *n,
                    name: name.clone(),
                    certified: self.certified.contains(n),
                    waited: self.waiting_for.contains(n) || self.stands.1.contains(n),
                    on_clock_s: if self.turn_seat == Some(*n) {
                        self.turn_since.map(|t| t.elapsed().as_secs())
                    } else {
                        None
                    },
                    quiet_s: *quiet_s,
                })
            })
            .collect()
    }

    pub fn tick_opponent(&mut self) {
        // `S1-EI`: alone at a bigger table -- everybody else off the line at
        // once -- is heads-up's question with more chairs: nobody can certify
        // anybody alone (D-036), so the table waits, and the player is asked
        // the one question that has an answer. No returns are counted here:
        // at three seats or more a return is the table's certificate (D-032).
        if self.heads_up_opponent().is_none() && self.seated.as_ref().is_some_and(|s| s.roster.len() >= 3) {
            let alone = self.alone_at_table();
            match (alone, self.opponent_gone.as_ref()) {
                (true, None) => {
                    self.opponent_gone = Some(OpponentGone {
                        since: std::time::Instant::now(),
                        said: false,
                        dismissed_at: None,
                        slow: false,
                        alone: true,
                    });
                }
                (false, Some(g)) if g.alone => {
                    self.opponent_gone = None;
                    self.note("a seat of the table can be reached again: the question is withdrawn".into());
                }
                _ => {}
            }
        }
        // `D-034`: a present opponent long past their time to decide is asked
        // about too -- heads-up nobody can fold for them (D-007), and the
        // question is the same. The episode ends when they act.
        if let (Some(opponent), Some(since), Some(allowance)) = (
            self.heads_up_opponent(),
            self.turn_since,
            self.seated.as_ref().map(|s| s.action_ms),
        ) {
            let past = since.elapsed().as_millis() as u64;
            if self.turn_seat == Some(opponent)
                && allowance > 0
                && past > allowance + OPPONENT_GONE_MS
                && self.opponent_gone.is_none()
                && !self.opponent_out
            {
                self.opponent_gone = Some(OpponentGone {
                    since: std::time::Instant::now()
                        .checked_sub(std::time::Duration::from_millis(OPPONENT_GONE_MS))
                        .unwrap_or_else(std::time::Instant::now),
                    said: false,
                    dismissed_at: None,
                    slow: true,
                    alone: false,
                });
            }
        }
        // A link reading gone stale is an opponent this client cannot reach,
        // whether or not the connection was ever seen to close.
        if let Some(opponent) = self.heads_up_opponent() {
            let stale = self
                .links
                .get(&opponent)
                .is_some_and(|(_, _, _, at)| at.elapsed().as_millis() as u64 > app_link_stale_ms());
            if stale {
                self.opponent_reachable(false);
            }
        }
        // `D-046`: a player who chose to wait is asked again after
        // `OPPONENT_ASK_AGAIN_MS` if the opponent is still out of reach, the
        // log line with it -- a wait is not for ever.
        if let Some(g) = self.opponent_gone.as_mut() {
            if g
                .dismissed_at
                .is_some_and(|d| d.elapsed().as_millis() as u64 >= OPPONENT_ASK_AGAIN_MS)
            {
                g.dismissed_at = None;
                g.said = false;
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
            } else if self.opponent_gone.as_ref().is_some_and(|g| g.slow) {
                let past = self.turn_since.map(|t| t.elapsed().as_secs()).unwrap_or(secs);
                let allowance = self.seated.as_ref().map(|s| s.action_ms / 1_000).unwrap_or(0);
                self.note(format!(
                    "your opponent has been on the clock for {past} s, past their {allowance} s to decide; heads-up, nobody can fold a hand for them (D-007): wait for them, or leave the table"
                ));
            } else if self.opponent_gone.as_ref().is_some_and(|g| g.alone) {
                self.note(format!(
                    "nobody at the table has been reachable for {secs} s; alone, nobody can certify anybody (D-036): wait for them, or leave the table"
                ));
            } else {
                self.note(format!(
                    "your opponent has been unreachable for {secs} s; heads-up, nobody can fold a hand for them (D-007): wait for them, or leave the table"
                ));
            }
        }
    }

    /// `S1-EL`: this client's own line to the Tox network is gone by the
    /// library's verdict, once a seat was on the line here.
    pub fn tox_line_gone(&self) -> bool {
        self.ever_on_line && self.status.tox == Some("offline")
    }

    /// `S1-EH`: what to say over the felt about this client's own line, if
    /// anything: the Tox network gone (the library's own verdict), with the
    /// lobby network's state beside it; or, before the library says so, a
    /// table where nobody can be reached any more. Gone the moment the line
    /// is back and a seat is heard.
    pub fn line_message(&self) -> Option<String> {
        let seated = self.seated.as_ref()?;
        // Once a seat has been on the line here: before that the library's
        // *offline* is the DHT still answering nobody, and the group's silence
        // is a group still forming.
        if !self.ever_on_line {
            return None;
        }
        if self.tox_line_gone() {
            let lobby = if self.status.peers == 0 {
                "The lobby network is unreachable too: the internet is down."
            } else {
                "The lobby network is still connected."
            };
            return Some(format!(
                "No connection to the Tox network. The table's hands ride it: nothing said here reaches the others until it is back. {lobby}"
            ));
        }
        if seated.session.is_none() || !self.ever_on_line || self.opponent_left {
            return None;
        }
        // `S1-FA`: and never once the game is over. The table's group is left
        // by **this client itself** ten seconds after the last hand (`D-042`),
        // so of course nobody is reachable in it -- saying *the line may be
        // down* about a line this client hung up is telling the player their
        // network failed when their tournament finished.
        if self.finished.is_some() {
            return None;
        }
        let me = seated.seat?;
        let others: Vec<u8> = seated.roster.iter().map(|(n, _, _)| *n).filter(|n| *n != me).collect();
        if others.is_empty() {
            return None;
        }
        let nobody = others
            .iter()
            .all(|n| self.links.get(n).is_some_and(|(_, group, _, _)| !*group));
        nobody.then(|| "Nobody at the table can be reached: the line may be down.".to_string())
    }

    /// The player chose to wait: the question rests for `OPPONENT_ASK_AGAIN_MS`
    /// and comes back if the opponent is still out of reach (D-046). A new
    /// episode -- the opponent back, then gone again -- asks anew.
    pub fn dismiss_opponent_gone(&mut self) {
        if let Some(g) = self.opponent_gone.as_mut() {
            g.dismissed_at = Some(std::time::Instant::now());
        }
    }

    /// How long the opponent has been unreachable, once it is worth asking
    /// about and until the player has answered.
    pub fn opponent_gone_for_s(&self) -> Option<u64> {
        self.opponent_gone
            .as_ref()
            .filter(|g| g.dismissed_at.is_none() && g.since.elapsed().as_millis() as u64 >= OPPONENT_GONE_MS)
            .map(|g| g.since.elapsed().as_secs())
    }

    /// `S1-CS`: the window asked to sit down; the small window says so until
    /// a seat comes, a refusal comes, or the wait runs out.
    pub fn begin_join(&mut self, key: [u8; 32], name: String, buyin: u64, password: Option<Vec<u8>>) {
        self.already_at = None;
        self.joining = Some(Joining {
            key,
            name,
            buyin,
            password,
            since: std::time::Instant::now(),
            failed: None,
            rejoin: false,
            gone: false,
        });
    }

    /// `S1-FG`: where this client is at table `key` -- a seat in one of its
    /// slots, the active one or another, or a join to it under way. Read from
    /// the state that draws the table windows, so it cannot outlast a leave:
    /// every road off a table clears that state (`LeftTable`, `JoinRefused`),
    /// and a join that failed is under way no more.
    pub fn at_table(&self, key: &[u8; 32]) -> Option<Here> {
        if self.seated.as_ref().is_some_and(|s| s.key == *key) {
            return Some(Here::Seated(self.active_slot));
        }
        if let Some((slot, _)) = self
            .background
            .iter()
            .find(|(_, t)| t.seated.as_ref().is_some_and(|s| s.key == *key))
        {
            return Some(Here::Seated(*slot));
        }
        self.joining
            .as_ref()
            .filter(|j| j.key == *key && j.failed.is_none())
            .map(|_| Here::Joining)
    }

    /// `S1-FG`: sit down at table `key` -- the command for the node, or none,
    /// and the lobby says this client is at that table already (the owner:
    /// joining a table one sits at must be refused, with the reason).
    pub fn sit_down(&mut self, key: [u8; 32], name: String, buyin: u64, password: Option<Vec<u8>>) -> Option<NodeCommand> {
        if self.at_table(&key).is_some() {
            self.already_at = Some(key);
            return None;
        }
        self.begin_join(key, name, buyin, password.clone());
        Some(NodeCommand::JoinTable { key, buyin, seat: None, password })
    }

    /// `S1-FG`: the join under way given up -- that join, and nothing else.
    /// The lobby used to send `LeaveTable` whenever this client sat anywhere,
    /// and the node leaves the active table: the one being played, its window
    /// closed, while the join went on (the owner, 2026-09-15).
    pub fn cancel_join(&mut self) -> Option<NodeCommand> {
        let j = self.joining.take()?;
        Some(NodeCommand::CancelJoin { key: j.key, forget: !j.rejoin })
    }

    /// `S1-DE`: the player answered *rejoin*. The question is taken down at
    /// the answer -- the node's own word on it comes at the deal, minutes
    /// later, or never -- and the join it starts is marked as the rejoin, so
    /// the connecting window says which game it is and, if it fails, offers
    /// to forget the record. What the node must be told comes back; a second
    /// answer finds no question.
    pub fn rejoin_unfinished(&mut self) -> Option<Unfinished> {
        let u = self.unfinished.take()?;
        self.begin_join(u.key, u.table_name.clone(), u.stack, None);
        if let Some(j) = self.joining.as_mut() {
            j.rejoin = true;
        }
        Some(u)
    }

    /// `S1-DE`: the player answered *forget it* -- at the question, or at a
    /// rejoin that failed. The question and the rejoin's window go at once;
    /// the node forgets the record and says so in the log.
    pub fn forget_unfinished(&mut self) {
        self.unfinished = None;
        if self.joining.as_ref().is_some_and(|j| j.rejoin) {
            self.joining = None;
        }
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
    /// `D-034`: the clock starts when the turn was GIVEN, on the giver's
    /// clock -- `elapsed_ms` in -- and not when this window heard of it, so
    /// the countdown and the seat's own fold end together. A later word
    /// about the same seat only ever moves the start earlier.
    fn clock_for(&mut self, seat: Option<u8>, elapsed_ms: u64) {
        let anchored = seat.map(|_| {
            std::time::Instant::now()
                .checked_sub(std::time::Duration::from_millis(elapsed_ms))
                .unwrap_or_else(std::time::Instant::now)
        });
        if seat != self.turn_seat {
            self.turn_seat = seat;
            self.turn_since = anchored;
            // `D-034`: the slow opponent acted; the question is withdrawn.
            if self.opponent_gone.as_ref().is_some_and(|g| g.slow) {
                self.opponent_gone = None;
            }
        } else if let (Some(was), Some(now)) = (self.turn_since, anchored) {
            if now < was {
                self.turn_since = Some(now);
            }
        }
        self.turn_moved();
    }

    /// `D-058`: the turn, or the stage, moved. The turn at a seat on the line is
    /// a table being played: a seat certified out of this hand is not said over
    /// it any more. A seat the hand no longer stands on has its votes spent, and
    /// a turn the certificate acted on is spent until the turn leaves the seat.
    fn turn_moved(&mut self) {
        let me = self.seated.as_ref().and_then(|s| s.seat);
        let hand = self.hand.as_ref().filter(|h| !h.over).map(|h| h.hand_id);
        let turn = self.turn_seat;
        let live = turn.is_some_and(|t| Some(t) == me || self.links.get(&t).is_some_and(|(_, group, _, _)| *group));
        let waiting = &self.waiting_for;
        for (seat, w) in self.waits.iter_mut() {
            if turn == Some(*seat) {
                continue;
            }
            w.turn_spent = false;
            if live && hand.is_some() && w.certified_in == hand {
                w.played_on = true;
            }
            if !waiting.contains(seat) {
                w.votes = None;
            }
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
        // `S1-FG`: the tables this client is at, and the word about one asked for
        // again -- while it is still so, and never after a leave.
        v.here = self
            .seated
            .iter()
            .chain(self.background.values().filter_map(|t| t.seated.as_ref()))
            .map(|s| s.key)
            .chain(self.joining.iter().filter(|j| j.failed.is_none()).map(|j| j.key))
            .collect();
        v.already_at = self.already_at.and_then(|key| {
            let here = self.at_table(&key)?;
            let name = self
                .lobby
                .tables()
                .find(|l| *l.key == key)
                .map(|l| l.held.ad.table_name.clone())
                .or_else(|| self.joining.as_ref().filter(|j| j.key == key).map(|j| j.name.clone()))
                .unwrap_or_else(|| short(&key));
            Some(crate::gui::lobby::AlreadyAtView {
                name,
                slot: match here {
                    Here::Seated(slot) => Some(slot),
                    Here::Joining => None,
                },
            })
        });
        v.joining = self.joining.as_ref().map(|j| crate::gui::lobby::JoiningView {
            name: j.name.clone(),
            elapsed_s: j.since.elapsed().as_secs(),
            failed: j.failed.clone(),
            rejoin: j.rejoin,
            gone: j.gone,
        });
        v.chat = self.chat.iter().cloned().collect();
        // Name and key together, because a name is decoration. Sorted by name
        // so the pane does not reshuffle every time somebody says they are
        // still here.
        // `D-043`: every table this client sits at, the turn marked.
        v.my_tables = self.slots();
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

    /// `D-053`: the client's log keeps what matters and drops the lobby's own
    /// bookkeeping -- and a line that matters is kept even where it holds a
    /// word the drop list names.
    #[test]
    fn the_client_log_keeps_what_matters_and_drops_the_lobby() {
        for line in [
            "seat 3 flooded the table's group (16 points of traffic no client of this build sends within 60 s)",
            "the table has certified seats [3, 4]'s timeout, unanimously among [0, 1, 2]",
            "seat 3 is out of the table for good for flooding the table's group (D-051)",
            "group member c37070e1 is no seat of this table and is removed from the group here",
            "this table is not safe: your opponent flooded the table's connection (D-051)",
            "a kick without the table's word is ignored",
            "hand #12 is waiting for seat 3",
            "seat 2 sits out by the table's group",
            // This client saying it cannot be played with is the first thing a
            // reader wants, however much it reads like the lobby's chatter.
            "could not join the public lobby: no listener",
            "no other poker client has been reached yet",
        ] {
            assert!(worth_logging(line), "kept: {line}");
        }
        for line in [
            "12D3KooWCibp… joined the lobby mesh",
            "lobby: answered 12D3KooWG8WM… with 0 table(s) offered",
            "found 12D3KooWJxyo… in the public lobby",
            "10 player(s) in the public lobby",
            "dial failed: Failed to negotiate transport protocol(s)",
            "relay said no: Failed to get Reservation.",
            "707 private address(es) from the DHT refused (2106 this run)",
            "12D3KooWDffC… is now a direct connection",
        ] {
            assert!(!worth_logging(line), "dropped: {line}");
        }
        // The keep list wins over the drop list: a security line about a member
        // found in the lobby is still a security line.
        assert!(worth_logging(
            "a stranger found in the public lobby was cut off here for good (D-051)"
        ));
    }

    /// `D-053`: the file is held to its bound by keeping the NEWEST half, so
    /// the lines just before a reader looked are the ones that survive.
    #[test]
    fn the_log_file_is_trimmed_to_its_newest_half() {
        let dir = std::env::temp_dir().join(format!("p2p-poker-log-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("client.log");
        let line = "x".repeat(99);
        let mut body = String::new();
        while body.len() < (LOG_FILE_MAX as usize) + 4_096 {
            body.push_str(&format!("{} {line}\n", body.len()));
        }
        let last = body.lines().next_back().expect("a last line").to_owned();
        std::fs::write(&path, &body).expect("the file is written");
        let len = std::fs::metadata(&path).expect("it is there").len();
        assert!(len > LOG_FILE_MAX);

        trim_log(&path, len);

        let after = std::fs::read_to_string(&path).expect("still readable");
        assert!(
            (after.len() as u64) <= LOG_FILE_KEEP,
            "trimmed to the keep bound, {} bytes",
            after.len()
        );
        assert!(after.ends_with(&format!("{last}\n")), "the newest line survived");
        assert!(
            after.lines().next().is_some_and(|l| body.lines().any(|b| b == l)),
            "and every line kept is a whole line"
        );
        let _ = std::fs::remove_dir_all(&dir);
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
        // `D-047`: code 9 is the seat out for good.
        for code in 1..=9u16 {
            assert_ne!(refusal(code), "no reason this client understands");
        }
        assert_eq!(refusal(0), "no reason this client understands");
        assert_eq!(refusal(10), "no reason this client understands");
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

    /// `D-040`: a table whose founder no longer offers it leaves the screen the
    /// moment the node says so, long before its advert would have expired.
    #[test]
    fn a_table_its_founder_withdrew_leaves_the_screen_at_once() {
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
        s.apply(NodeEvent::TableGone { key, why: "its founder no longer offers it".into() });
        assert_eq!(s.view().tables.len(), 0, "and it is gone at once");
        assert!(s.log.iter().any(|l| l.contains("gone: its founder no longer offers it")), "{:?}", s.log);
        // Said once: a second word about a table already gone is nothing.
        let before = s.log.len();
        s.apply(NodeEvent::TableGone { key, why: "again".into() });
        assert_eq!(s.log.len(), before);
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
        s.apply(NodeEvent::HandBegan { hand_id: 4, button: 0, dealt_in: vec![0, 1], small_blind: 10, big_blind: 20 });
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
        s.apply(NodeEvent::HandBegan { hand_id: 5, button: 0, dealt_in: vec![0, 1], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::Seated { key: [8u8; 32], seat: 3 });
        assert!(s.hand.is_none(), "a hand from another table is not this table's");

        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 3], small_blind: 10, big_blind: 20 });
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
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        assert_eq!(s.turn_seat, None);
        s.apply(NodeEvent::TableState {
            hand_id: 1, street: 0, pot: 30, to_act: Some(2), stacks: vec![1_000; 3], bets: vec![0, 10, 20], folded: vec![false; 3],
        });
        assert_eq!(s.turn_seat, Some(2));
        let started = s.turn_since.expect("a clock is running");
        s.apply(NodeEvent::NotYourTurn { hand_id: 1, seat: Some(2), elapsed_ms: 0 });
        assert_eq!(s.turn_since, Some(started), "the same seat keeps its start");
        s.apply(NodeEvent::NotYourTurn { hand_id: 1, seat: Some(1), elapsed_ms: 0 });
        assert_eq!(s.turn_seat, Some(1));
        assert!(s.turn_since.is_some());
        s.apply(NodeEvent::YourTurn {
            hand_id: 1, street: 0, to_call: 20, pot: 50, can_check: false, can_call: true, can_bet: false, can_raise: true, min_raise_to: 40, max_raise_to: 1_000, elapsed_ms: 0,
        });
        assert_eq!(s.turn_seat, Some(0), "our own turn is our own clock");
        s.apply(NodeEvent::HandEnded { hand_id: 1, stacks: vec![1_000; 3], shown: vec![None; 3], pots: Vec::new(), gained: Vec::new() });
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
        // `D-054`: the name is the name; the seat is the window's own, drawn
        // from `seat` so no name can claim to be another seat.
        assert_eq!(last.who, "Carol");
        assert_eq!(last.said, format!("line {}", MAX_CHAT_LINES + 4));
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: Some(120), group: false, quiet_s: None, away: false });
        assert_eq!(s.links.get(&2).map(|(r, _, _, _)| *r), Some(Some(120)));
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

    /// `S1-DE`: the question about an unfinished game is answered once.
    /// *Rejoin* takes it down and starts a join the window can tell from any
    /// other; *forget it* takes it down with nothing started; a rejoin that
    /// failed keeps its window, with the reason, and can be forgotten from
    /// there; and one the node gives up on ends in that window with the
    /// reason and nothing to try again.
    #[test]
    fn the_unfinished_question_is_answered_once_and_a_rejoin_says_so() {
        let on_record = || NodeEvent::UnfinishedSession {
            key: [7u8; 32],
            table_name: "Riverside".into(),
            seat: 1,
            stack: 10_000,
            hand_id: 0,
        };
        let mut s = AppState::new();
        s.apply(on_record());
        assert!(s.view().unfinished.is_some(), "the window asks");

        // Rejoin: the question goes, and the join is the rejoin.
        let u = s.rejoin_unfinished().expect("the record to rejoin");
        assert_eq!((u.key, u.stack), ([7u8; 32], 10_000));
        assert!(s.unfinished.is_none(), "the question is answered");
        let j = s.view().joining.expect("a join in progress");
        assert!(j.rejoin && !j.gone && j.failed.is_none() && j.name == "Riverside", "{j:?}");
        assert!(s.rejoin_unfinished().is_none(), "answered once");
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 1 });
        assert!(s.joining.is_none(), "a seat ends the rejoin like any join");

        // Forget it, at the question: nothing started, and the node's word
        // on it changes nothing.
        s.apply(on_record());
        s.forget_unfinished();
        assert!(s.unfinished.is_none() && s.joining.is_none());
        s.apply(NodeEvent::SessionGaveUp { why: "forgotten at the player's word".into() });
        assert!(s.unfinished.is_none() && s.joining.is_none());

        // A rejoin the founder does not answer keeps its window, with the
        // reason, and is forgotten from there.
        s.apply(on_record());
        s.rejoin_unfinished();
        s.apply(NodeEvent::LeftTable { why: "the founder did not answer: timeout".into() });
        let j = s.view().joining.expect("the failed rejoin is still shown");
        assert!(j.rejoin && !j.gone && j.failed.as_deref().is_some_and(|w| w.contains("did not answer")), "{j:?}");
        s.forget_unfinished();
        assert!(s.joining.is_none(), "forgotten from the failed rejoin");

        // A rejoin the node gives up on ends with the reason and nothing to
        // try again.
        s.apply(on_record());
        s.rejoin_unfinished();
        s.apply(NodeEvent::JoinRefused { reason: 5 });
        s.apply(NodeEvent::SessionGaveUp { why: "the founder refused the rejoin (reason 5)".into() });
        let j = s.view().joining.expect("said where the player is looking");
        assert!(j.rejoin && j.gone && j.failed.as_deref().is_some_and(|w| w.contains("refused")), "{j:?}");
        assert!(s.unfinished.is_none());

        // An ordinary join is no rejoin, and the node's give-up does not touch it.
        s.joining = None;
        s.begin_join([8u8; 32], "Elsewhere".into(), 1_000, None);
        s.apply(NodeEvent::SessionGaveUp { why: "no advertisement and no peer of the session for ten minutes".into() });
        let j = s.view().joining.expect("the ordinary join goes on");
        assert!(!j.rejoin && !j.gone && j.failed.is_none(), "{j:?}");
    }

    /// `S1-EL`: *Line down* by the library's verdict hides the question about the
    /// others; the question, when it stands, hides the group's softer overlay.
    #[test]
    fn the_line_down_word_and_the_question_are_not_shown_together() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        s.apply(NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "me".into(), 1_000), (1, "them".into(), 1_000)] });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: false, quiet_s: Some(21), away: false });
        s.opponent_gone.as_mut().unwrap().since = std::time::Instant::now() - std::time::Duration::from_millis(OPPONENT_GONE_MS + 1_000);
        let v = s.table_view();
        assert!(v.opponent_gone_s.is_some(), "the question stands");
        assert!(v.line.is_none(), "and the softer overlay yields to it: {:?}", v.line);
        s.apply(NodeEvent::ToxLine { how: "offline" });
        let v = s.table_view();
        assert!(v.line.as_deref().is_some_and(|l| l.contains("Tox network")), "the line is down by the library's verdict");
        assert!(v.opponent_gone_s.is_none(), "and the question is not asked beside it");
        s.apply(NodeEvent::ToxLine { how: "udp" });
        assert!(s.table_view().opponent_gone_s.is_some(), "the line back, the others still gone: the question again");
    }

    /// `D-057`: the far founder's way back in `fe181646-3`, as the felt shows it --
    /// nobody answers, the group again, the running hand followed, the sit-in, and
    /// dealt in -- and a short outage that put nobody out ends at *Back in the game*.
    #[test]
    fn the_way_back_is_shown_step_by_step_until_dealt_in_again() {
        use crate::gui::table::StepState::{Done, Later, Now};
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        s.apply(NodeEvent::Roster {
            key: [7u8; 32],
            seats: vec![(0, "me".into(), 1_000), (1, "a".into(), 1_000), (2, "b".into(), 1_000)],
        });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        assert!(s.table_view().rejoin.is_none(), "nothing wrong, nothing shown");

        s.apply(NodeEvent::TableReach { reachable: false });
        let v = s.table_view().rejoin.expect("nobody at three seats answers: the way back begins");
        assert_eq!(v.steps[0], (Now, "Nobody at the table answers".to_string()));
        assert!(v.steps.iter().skip(1).all(|(st, _)| *st == Later), "nothing else yet: {:?}", v.steps);

        s.apply(NodeEvent::TableReach { reachable: true });
        let v = s.table_view().rejoin.expect("still under way");
        assert_eq!(v.steps[1], (Done, "Back in the table's group".to_string()));
        assert!(!v.back, "reached is not yet back in the game");

        s.apply(NodeEvent::SessionResumed { hand_id: 20, member: false });
        let v = s.table_view().rejoin.expect("following");
        assert!(v.steps.iter().any(|(st, t)| *st == Now && t.contains("hand #20")), "the running hand, under way: {:?}", v.steps);

        s.apply(NodeEvent::SitInAsked { seat: 0, hand_id: 20 });
        let v = s.table_view().rejoin.expect("asked");
        assert!(v.steps.iter().any(|(st, t)| *st == Now && t.contains("sit in")), "{:?}", v.steps);

        s.apply(NodeEvent::HandBegan { hand_id: 21, button: 1, dealt_in: vec![0, 1, 2], small_blind: 100, big_blind: 200 });
        let v = s.table_view().rejoin.expect("the moment it is back is shown");
        assert!(v.back && v.steps.iter().all(|(st, _)| *st == Done), "{:?}", v.steps);
        // The owner: gone the moment this client holds its cards again.
        s.apply(NodeEvent::HoleCards { hand_id: 21, cards: [12, 13] });
        assert!(s.table_view().rejoin.is_none(), "gone with the hole cards");

        // A short outage while holding cards: the hand stands for this client while
        // it reaches nobody, and being back needs no word over the hand it plays.
        s.apply(NodeEvent::TableReach { reachable: false });
        assert!(s.table_view().rejoin.is_some(), "nobody answers: said");
        s.apply(NodeEvent::TableReach { reachable: true });
        s.rejoin.as_mut().unwrap().reached_at = Some(std::time::Instant::now() - REJOIN_SETTLES);
        s.apply(NodeEvent::Swept { now_ms: 1 });
        assert!(s.table_view().rejoin.is_none() && s.rejoin.is_none(), "back with its cards: nothing over the hand");

        // Between hands the same outage is said back for a moment, and then it goes.
        s.apply(NodeEvent::HandEnded { hand_id: 21, stacks: vec![], shown: vec![], pots: vec![], gained: vec![] });
        s.apply(NodeEvent::TableReach { reachable: false });
        s.apply(NodeEvent::TableReach { reachable: true });
        s.rejoin.as_mut().unwrap().reached_at = Some(std::time::Instant::now() - REJOIN_SETTLES);
        s.apply(NodeEvent::Swept { now_ms: 2 });
        assert!(s.table_view().rejoin.is_some_and(|v| v.back), "back in the game once nothing more follows");
        s.rejoin.as_mut().unwrap().back_at = Some(std::time::Instant::now() - REJOIN_BACK_SHOWN);
        s.apply(NodeEvent::Swept { now_ms: 3 });
        assert!(s.table_view().rejoin.is_none(), "and then it goes");
    }

    /// `D-058`: the table standing on a seat, from the members' side of `fe181646-3`
    /// -- said over the felt while the hand cannot go on without it (its turn, a
    /// stage it owes) and, once it is certified out, until the table is played
    /// again; never over a hand the others are playing (the owner, 2026-09-14,
    /// four times).
    #[test]
    fn the_felt_says_the_wait_on_a_seat_only_while_nothing_is_played() {
        use crate::gui::table::StepState::Now;
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 1 });
        s.apply(NodeEvent::Roster {
            key: [7u8; 32],
            seats: vec![(0, "Founder".into(), 1_000), (1, "me".into(), 1_000), (2, "b".into(), 1_000)],
        });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::SeatLink { seat: 0, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::HandBegan { hand_id: 12, button: 0, dealt_in: vec![0, 1, 2], small_blind: 100, big_blind: 200 });
        s.apply(NodeEvent::Swept { now_ms: 1 });
        assert!(s.table_view().waits.is_empty(), "nobody away, no panel");

        // Off the line while another seat is to act: nothing over the felt.
        s.apply(NodeEvent::SeatLink { seat: 0, rtt_ms: None, group: false, quiet_s: Some(22), away: false });
        s.apply(NodeEvent::NotYourTurn { hand_id: 12, seat: Some(2), elapsed_ms: 0 });
        s.apply(NodeEvent::Swept { now_ms: 2 });
        let v = s.table_view();
        assert!(v.waits.is_empty(), "the others are playing: {:?}", v.waits);
        assert!(v.absent.iter().all(|a| !a.stalls()), "nor the older sentences: {:?}", v.absent);

        // Its turn: the hand stands on it.
        s.apply(NodeEvent::NotYourTurn { hand_id: 12, seat: Some(0), elapsed_ms: 0 });
        let v = s.table_view_of(0);
        assert_eq!(v.waits.len(), 1);
        assert_eq!(v.waits[0].title, "Waiting for Founder");
        assert_eq!(v.waits[0].panel.steps[1].0, Now, "on its clock: {:?}", v.waits[0].panel.steps);
        s.apply(NodeEvent::TimeoutVotes { seat: 0, held: 1, need: 2 });
        let v = s.table_view().waits;
        assert!(v[0].panel.steps.iter().any(|(st, t)| *st == Now && t.contains("act for it – 1 of 2")), "{:?}", v[0].panel.steps);

        // Certified out: said until the table is played again -- here the turn at a
        // seat on the line.
        s.apply(NodeEvent::SeatCertified { seat: 0, hand_id: 12 });
        let v = s.table_view().waits;
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].title, "The table goes on without Founder", "not gone before anything moves");
        assert!(
            s.table_log.iter().any(|l| l.text == "Founder is certified out of this hand; the table plays on without it"),
            "{:?}",
            s.table_log
        );
        s.apply(NodeEvent::NotYourTurn { hand_id: 12, seat: Some(2), elapsed_ms: 0 });
        assert!(s.table_view().waits.is_empty(), "the hand is played again: nothing over it");

        // The cards need its part: the hand stands on it again, and the other seats
        // end the hand.
        s.apply(NodeEvent::NotYourTurn { hand_id: 12, seat: None, elapsed_ms: 0 });
        s.apply(NodeEvent::StageStands { hand_id: 12, seats: vec![0] });
        let v = s.table_view().waits;
        assert_eq!(v[0].title, "Waiting for Founder");
        assert_eq!(v[0].panel.steps[1], (Now, "The cards cannot move on without it".to_string()));
        s.apply(NodeEvent::TimeoutVotes { seat: 0, held: 1, need: 2 });
        let v = s.table_view().waits;
        assert!(v[0].panel.steps.iter().any(|(st, t)| *st == Now && t.contains("end the hand – 1 of 2")), "{:?}", v[0].panel.steps);

        // The hand called off: said until the next hand's cards are out, whether
        // the node unsays the stage or not.
        s.apply(NodeEvent::HandEnded { hand_id: 12, stacks: vec![], shown: vec![], pots: vec![], gained: vec![] });
        let v = s.table_view_of(0);
        assert_eq!(v.waits.len(), 1, "{:?}", v.waits);
        assert_eq!(v.waits[0].title, "The table goes on without Founder");
        assert_eq!(v.waits[0].panel.steps[3], (Now, "The next hand is dealt without it".to_string()));
        s.apply(NodeEvent::StageStands { hand_id: 12, seats: vec![] });
        s.apply(NodeEvent::SeatLeft { seat: 0, quit: false, removed: false });
        s.apply(NodeEvent::HandBegan { hand_id: 13, button: 2, dealt_in: vec![1, 2], small_blind: 100, big_blind: 200 });
        s.apply(NodeEvent::Swept { now_ms: 4 });
        assert_eq!(s.table_view().waits.len(), 1, "dealt, but no card is out yet");
        s.apply(NodeEvent::CardsDealt { hand_id: 13, seats: vec![1, 2] });
        s.apply(NodeEvent::HoleCards { hand_id: 13, cards: [12, 13] });
        assert!(s.table_view().waits.is_empty(), "the next hand is being played");
        s.apply(NodeEvent::Swept { now_ms: 5 });
        assert!(s.waits.is_empty(), "and nothing kept: {:?}", s.waits);

        // Its way back: at its seat and in the log, never over the felt.
        s.apply(NodeEvent::SeatLink { seat: 0, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::SitInAsked { seat: 0, hand_id: 20 });
        s.apply(NodeEvent::ReturnVotes { seat: 0, held: 1, need: 2 });
        s.apply(NodeEvent::Swept { now_ms: 6 });
        let v = s.table_view();
        assert!(v.waits.is_empty(), "not over the felt: {:?}", v.waits);
        assert!(v.seats.iter().any(|x| x.seat == 0 && x.coming_back), "said at its seat");
        assert!(s.table_log.iter().any(|l| l.text == "Founder asks to sit in after hand #20"), "{:?}", s.table_log);
        s.apply(NodeEvent::SitInAsked { seat: 0, hand_id: 20 });
        assert_eq!(s.table_log.iter().filter(|l| l.text.contains("asks to sit in")).count(), 1, "said once");

        s.apply(NodeEvent::HandBegan { hand_id: 21, button: 1, dealt_in: vec![0, 1, 2], small_blind: 100, big_blind: 200 });
        assert!(s.table_view().seats.iter().all(|x| !x.coming_back), "dealt in again");
        let log: Vec<&str> = s.table_log.iter().map(|l| l.text.as_str()).collect();
        let header = log.iter().rposition(|l| l.contains("Hand: 21")).expect("the hand's header");
        assert!(log[header..].contains(&"Founder is back in the game"), "under the header: {log:?}");
    }

    /// `D-058`, the owner's fourth word: the panel came *not reliably*, and went
    /// before the next hand was dealt. A stall is said the moment it is known --
    /// the felt asks before every frame, not the sweep thirty seconds apart -- for
    /// a hand this window never began (a seat that died between hands holds the
    /// next hand's opening), for a seat the group still hears by the node's word,
    /// and a certificate heard after its hand ended still holds the felt until
    /// the next hand's cards are out.
    #[test]
    fn a_stall_is_said_the_moment_it_is_known_and_held_until_the_next_deal() {
        use crate::gui::table::StepState::Now;
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 1 });
        s.apply(NodeEvent::Roster {
            key: [7u8; 32],
            seats: vec![(0, "a".into(), 1_000), (1, "me".into(), 1_000), (2, "b".into(), 1_000)],
        });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::SeatLink { seat: 0, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: None, group: true, quiet_s: Some(0), away: false });

        // A later hand's opening waits on a seat off the line -- a client back from
        // a restart holds no hand yet (`S1-FM` says the first hand's otherwise):
        // said at once, with no hand begun at this window and no sweep.
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: None, group: false, quiet_s: Some(30), away: false });
        s.apply(NodeEvent::HandWaiting { hand_id: 5, seats: vec![2] });
        let v = s.table_view_of(0);
        assert_eq!(v.waits.len(), 1, "{:?}", v.waits);
        assert_eq!(v.waits[0].title, "Waiting for b");
        assert_eq!(v.waits[0].panel.steps[0].1, "b is off the line");
        assert_eq!(v.waits[0].panel.steps[1], (Now, "The cards cannot move on without it".to_string()));

        // A seat the group still hears holds the stage: the node's word, after
        // its moment, is enough.
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        let v = s.table_view_of(0);
        assert!(v.waits.is_empty(), "heard, and not yet a stall: {:?}", v.waits);
        s.apply(NodeEvent::StageStands { hand_id: 5, seats: vec![2] });
        let v = s.table_view_of(0);
        assert_eq!(v.waits[0].title, "Waiting for b");
        assert_eq!(v.waits[0].panel.steps[0].1, "b stopped answering");
        assert!(v.waits[0].panel.for_s >= 4, "the clock counts from the stall: {}", v.waits[0].panel.for_s);

        // Certified out at that stage, the hand called off -- the two words in
        // either order, for a hand this window never held -- and the felt says
        // the table goes on until the next hand's cards are out.
        s.apply(NodeEvent::TimeoutVotes { seat: 2, held: 1, need: 2 });
        s.apply(NodeEvent::HandEnded { hand_id: 5, stacks: vec![], shown: vec![], pots: vec![], gained: vec![] });
        s.apply(NodeEvent::SeatCertified { seat: 2, hand_id: 5 });
        let v = s.table_view_of(0);
        assert_eq!(v.waits.len(), 1, "{:?}", v.waits);
        assert_eq!(v.waits[0].title, "The table goes on without b");
        assert_eq!(v.waits[0].panel.steps[3], (Now, "The next hand is dealt without it".to_string()));
        s.apply(NodeEvent::HandBegan { hand_id: 6, button: 0, dealt_in: vec![0, 1], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::NotYourTurn { hand_id: 6, seat: Some(0), elapsed_ms: 0 });
        let v = s.table_view_of(0);
        assert_eq!(v.waits.len(), 1, "dealt, no card out yet: {:?}", v.waits);
        s.apply(NodeEvent::HoleCards { hand_id: 6, cards: [1, 2] });
        let v = s.table_view_of(0);
        assert!(v.waits.is_empty(), "the next hand is being played: {:?}", v.waits);
    }

    /// `D-059`: a seat that made the table wait is said in the table's log with
    /// the time a cryptographic step gives it from now on, and the wait panel
    /// over a step it holds says the same.
    #[test]
    fn a_seat_that_made_the_table_wait_is_said_with_its_time_at_a_step() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 1 });
        s.apply(NodeEvent::Roster {
            key: [7u8; 32],
            seats: vec![(0, "Founder".into(), 1_000), (1, "me".into(), 1_000), (2, "Carol".into(), 1_000)],
        });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::SeatLink { seat: 0, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::HandBegan { hand_id: 12, button: 0, dealt_in: vec![0, 1, 2], small_blind: 100, big_blind: 200 });
        s.apply(NodeEvent::SeatWaited { seat: 2, waits: 1, step_ms: 30_000 });
        assert!(
            s.table_log.iter().any(|l| l.text == "Carol made the table wait (1 of 3): 20 s at a step from now on"),
            "{:?}",
            s.table_log
        );
        s.apply(NodeEvent::NotYourTurn { hand_id: 12, seat: None, elapsed_ms: 0 });
        s.apply(NodeEvent::StageStands { hand_id: 12, seats: vec![2] });
        let v = s.table_view_of(0).waits;
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].title, "Waiting for Carol");
        assert!(
            v[0].panel.detail.as_deref().is_some_and(|d| d.contains("It has made the table wait once: 20 s at a step from now on")),
            "{:?}",
            v[0].panel.detail
        );
        s.apply(NodeEvent::SeatWaited { seat: 2, waits: 3, step_ms: 30_000 });
        assert!(s.table_log.iter().any(|l| l.text == "Carol made the table wait (3 of 3): 10 s at a step from now on"));
        s.apply(NodeEvent::SeatWaited { seat: 2, waits: 4, step_ms: 30_000 });
        assert!(s.table_log.iter().any(|l| l.text == "Carol made the table wait again: 10 s at a step"));
    }

    /// `S1-FO`, the owner's game (2026-09-15): three seats, one all in, and the
    /// table removed the third for its fourth absence -- the window said *your
    /// opponent left the table, the game is over* while the all-in seat played
    /// on. An all-in seat is in the game, and the table's word removing a seat is
    /// no opponent leaving.
    #[test]
    fn an_all_in_seat_is_in_the_game_and_a_removal_is_no_opponent_leaving() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 1 });
        s.apply(NodeEvent::Roster {
            key: [7u8; 32],
            seats: vec![(0, "far".into(), 10_000), (1, "me".into(), 10_000), (2, "MIR".into(), 10_000)],
        });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        for seat in [0u8, 2] {
            s.apply(NodeEvent::SeatLink { seat, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        }
        s.apply(NodeEvent::HandEnded { hand_id: 36, stacks: vec![2_000, 2_800, 21_850], shown: vec![], pots: vec![], gained: vec![] });
        s.apply(NodeEvent::HandBegan { hand_id: 37, button: 2, dealt_in: vec![0, 1, 2], small_blind: 400, big_blind: 800 });
        s.apply(NodeEvent::TableState {
            hand_id: 37,
            street: 1,
            pot: 23_050,
            to_act: Some(0),
            stacks: vec![1_600, 2_400, 0],
            bets: vec![400, 400, 21_850],
            folded: vec![false, false, false],
        });
        assert_eq!(s.heads_up_opponent(), None, "three in the game, one of them all in");
        s.apply(NodeEvent::SeatLeft { seat: 0, quit: true, removed: true });
        assert!(!s.opponent_left && !s.opponent_out, "no opponent left");
        assert!(s.log.back().unwrap().contains("removed by the table's word"), "{}", s.log.back().unwrap());
        assert_eq!(s.heads_up_opponent(), Some(2), "heads-up against the all-in seat, which plays on");
        // And a leave by a seat's own client while another seat is in the game
        // is no end of it either.
        let mut t = AppState::new();
        t.apply(NodeEvent::Seated { key: [7u8; 32], seat: 1 });
        t.apply(NodeEvent::Roster {
            key: [7u8; 32],
            seats: vec![(0, "far".into(), 10_000), (1, "me".into(), 10_000), (2, "MIR".into(), 10_000)],
        });
        t.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        t.apply(NodeEvent::HandBegan { hand_id: 1, button: 2, dealt_in: vec![0, 1, 2], small_blind: 50, big_blind: 100 });
        t.apply(NodeEvent::TableState { hand_id: 1, street: 1, pot: 20_100, to_act: Some(0), stacks: vec![9_950, 9_900, 0], bets: vec![50, 100, 10_000], folded: vec![false, false, false] });
        t.apply(NodeEvent::SeatLeft { seat: 0, quit: true, removed: false });
        assert!(!t.opponent_left && !t.opponent_out, "MIR is all in and in the game");
    }

    /// `S1-FR`, the owner's test (2026-09-15): a client killed and started again
    /// is shown its way back from the seat it takes up again -- not the words for
    /// a table before its first hand -- until it is dealt in.
    #[test]
    fn a_client_back_after_a_restart_is_shown_its_way_back() {
        use crate::gui::table::StepState::{Done, Now};
        let mut s = AppState::new();
        s.apply(NodeEvent::UnfinishedSession { key: [7u8; 32], table_name: "New table".into(), seat: 1, stack: 10_000, hand_id: 6 });
        assert!(s.rejoin_unfinished().is_some());
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 1 });
        s.apply(NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "founder".into(), 10_000), (1, "me".into(), 10_000)] });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        let v = s.table_view().rejoin.expect("the way back, from the seat on");
        assert_eq!(v.steps[0], (Done, "Coming back after a restart".to_string()));
        assert_eq!(v.steps[1], (Now, "Back in the table's group".to_string()));
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 1 });
        assert!(s.table_view().rejoin.is_some(), "another roster's seat does not start it again");
        s.apply(NodeEvent::SeatLink { seat: 0, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::SessionResumed { hand_id: 6, member: false });
        let v = s.table_view().rejoin.expect("still on its way");
        assert!(v.steps.iter().any(|(st, t)| *st == Done && t == "Back in the table's group"), "{:?}", v.steps);
        s.apply(NodeEvent::HandBegan { hand_id: 7, button: 0, dealt_in: vec![0, 1], small_blind: 50, big_blind: 100 });
        s.apply(NodeEvent::HoleCards { hand_id: 7, cards: [1, 2] });
        assert!(s.table_view().rejoin.is_none(), "dealt in: nothing over the hand");

        // A founder back from its record, the same.
        let mut f = AppState::new();
        f.apply(NodeEvent::UnfinishedSession { key: [8u8; 32], table_name: "Mine".into(), seat: 0, stack: 10_000, hand_id: 3 });
        assert!(f.rejoin_unfinished().is_some());
        f.apply(NodeEvent::Hosting { key: [8u8; 32] });
        assert_eq!(f.table_view().rejoin.map(|v| v.steps[0].1.clone()).as_deref(), Some("Coming back after a restart"));
        // And a table founded fresh is no way back.
        let mut g = AppState::new();
        g.apply(NodeEvent::Hosting { key: [9u8; 32] });
        assert!(g.table_view().rejoin.is_none());
    }

    /// `S1-FQ`, the owner's test (2026-09-15): a heads-up opponent's client
    /// killed and started again leaves the table's group *on purpose* by the
    /// carrier's word -- a rejoin says the same goodbye -- and the window said the
    /// game was over. The group's word is never a player leaving, however long the
    /// seat stays away; the seat's own signed word is, at once.
    #[test]
    fn only_the_seats_own_word_is_a_player_leaving() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        s.apply(NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "me".into(), 1_000), (1, "them".into(), 1_000)] });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        for _ in 0..3 {
            s.apply(NodeEvent::SeatLeft { seat: 1, quit: true, removed: false });
            s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: false, quiet_s: None, away: false });
            s.tick_opponent();
            let v = s.table_view_of(0);
            assert!(!s.opponent_left && !v.opponent_left, "the group's goodbye is no player leaving");
            assert!(!s.left_for_good.contains(&1), "and not for good");
            s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
            assert!(!s.gone.contains(&1), "back in the group");
        }
        assert_eq!(s.heads_up_opponent(), Some(1));
        s.apply(NodeEvent::SeatLeftTable { seat: 1 });
        assert!(s.opponent_left && s.opponent_out, "its own word: the game is over");
        assert!(s.table_view().opponent_left, "the window is told at once");
        assert!(s.table_log.iter().any(|l| l.text == "them left the table"), "{:?}", s.table_log);
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        assert!(s.opponent_out && s.opponent_left, "nothing on the line undoes the word");
        assert_eq!(s.opponent_returns, 0, "a leave is no return");
    }

    /// `S1-FG`, the owner's word (2026-09-15): an action in the lobby never closes
    /// a table window. *Cancel* at a join to a second table gives that join up
    /// -- never `LeaveTable`, which the node applies to the table being played --
    /// and a join to a table this client sits at is refused with the reason, for
    /// as long as it sits there and not a moment after.
    #[test]
    fn the_lobby_never_leaves_the_table_being_played() {
        let (a, b) = ([1u8; 32], [2u8; 32]);
        let mut s = AppState::new();
        s.apply(NodeEvent::AtTable { slot: 0, key: Some(a) });
        s.apply(NodeEvent::Seated { key: a, seat: 1 });
        assert_eq!(s.at_table(&a), Some(Here::Seated(0)));

        // A join to a second table, given up.
        let cmd = s.sit_down(b, "second".into(), 1_000, None);
        assert!(matches!(cmd, Some(NodeCommand::JoinTable { key, .. }) if key == b), "{cmd:?}");
        assert_eq!(s.at_table(&b), Some(Here::Joining));
        let cmd = s.cancel_join();
        assert!(matches!(cmd, Some(NodeCommand::CancelJoin { key, forget: true }) if key == b), "{cmd:?}");
        assert!(s.seated.as_ref().is_some_and(|x| x.key == a), "the table being played is untouched");
        assert!(s.at_table(&b).is_none());

        // The end of the second join at its own slot is not the first table's.
        assert!(s.sit_down(b, "second".into(), 1_000, None).is_some());
        s.apply(NodeEvent::AtTable { slot: 1, key: Some(b) });
        s.apply(NodeEvent::Seated { key: b, seat: 2 });
        assert_eq!(s.at_table(&b), Some(Here::Seated(1)));
        assert_eq!(s.slots().len(), 2, "two windows");
        s.apply(NodeEvent::LeftTable { why: "the join was given up".into() });
        assert!(s.at_table(&b).is_none(), "that slot's table went");
        assert_eq!(s.slots().len(), 1, "one window, the first table's");
        assert!(s.seated.as_ref().is_some_and(|x| x.key == a));

        // A join to the table this client sits at: refused, with the reason and
        // the way to its window.
        assert!(s.sit_down(a, "first".into(), 1_000, None).is_none());
        let v = s.view();
        assert!(v.here.contains(&a));
        assert_eq!(v.already_at.as_ref().map(|x| x.slot), Some(Some(0)), "{:?}", v.already_at);

        // Left: the refusal goes with the table, and joining works again.
        s.apply(NodeEvent::AtTable { slot: 0, key: Some(a) });
        s.apply(NodeEvent::LeftTable { why: "left the table".into() });
        let v = s.view();
        assert!(v.already_at.is_none(), "not stuck after leaving: {:?}", v.already_at);
        assert!(!v.here.contains(&a));
        assert!(matches!(s.sit_down(a, "first".into(), 1_000, None), Some(NodeCommand::JoinTable { .. })));
    }

    /// `S1-FG`: the node's word that it did not start a join ends the join
    /// window's wait -- at a table this client sits at, as the lobby's refusal;
    /// otherwise with the reason -- and the end of another table fails no join.
    #[test]
    fn a_join_the_node_did_not_start_does_not_wait_for_ever() {
        let (a, b) = ([1u8; 32], [2u8; 32]);
        let mut s = AppState::new();
        assert!(s.sit_down(b, "gone".into(), 1_000, None).is_some());
        s.apply(NodeEvent::JoinNotStarted { key: b, why: "that table is no longer advertised".into(), already_here: false });
        assert_eq!(s.joining.as_ref().and_then(|j| j.failed.clone()).as_deref(), Some("that table is no longer advertised"));
        assert!(s.at_table(&b).is_none(), "a failed join is under way no more");

        s.apply(NodeEvent::AtTable { slot: 0, key: Some(a) });
        s.apply(NodeEvent::Seated { key: a, seat: 0 });
        s.joining = None;
        s.begin_join(a, "here".into(), 1_000, None);
        s.apply(NodeEvent::JoinNotStarted { key: a, why: "this client already sits at or joins that table".into(), already_here: true });
        assert!(s.joining.is_none());
        assert!(s.view().already_at.is_some(), "the lobby says so");

        // A join to b under way; table a ends at its slot: b's join goes on.
        assert!(s.sit_down(b, "b".into(), 1_000, None).is_some());
        s.apply(NodeEvent::AtTable { slot: 0, key: Some(a) });
        s.apply(NodeEvent::LeftTable { why: "the tournament is over".into() });
        assert!(s.joining.as_ref().is_some_and(|j| j.failed.is_none()), "{:?}", s.joining);
    }

    /// `S1-FM`: before the first hand, a seat not yet in the table's group is
    /// said to be joining it -- not to have stopped answering -- and the step
    /// turns when it is in.
    #[test]
    fn before_the_first_hand_a_seat_is_joining_the_group_not_gone() {
        use crate::gui::table::StepState::{Done, Now};
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        s.apply(NodeEvent::Roster {
            key: [7u8; 32],
            seats: vec![(0, "me".into(), 1_000), (1, "a".into(), 1_000), (2, "far".into(), 1_000)],
        });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: None, group: false, quiet_s: None, away: false });
        s.apply(NodeEvent::HandWaiting { hand_id: 1, seats: vec![2] });
        let v = s.table_view_of(0);
        assert_eq!(v.waits.len(), 1, "{:?}", v.waits);
        assert_eq!(v.waits[0].title, "Waiting for far");
        assert_eq!(v.waits[0].panel.steps[0].0, Now);
        assert!(v.waits[0].panel.steps[0].1.starts_with("far joins the table's group"), "{:?}", v.waits[0].panel.steps);
        assert!(!v.waits[0].panel.steps.iter().any(|(_, t)| t.contains("stopped answering") || t.contains("off the line")));

        // In the group, the node's word keeps the stage standing on it: the step turns.
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::StageStands { hand_id: 1, seats: vec![2] });
        let v = s.table_view_of(0);
        assert_eq!(v.waits[0].panel.steps[0], (Done, "far is in the table's group".to_string()));
        assert_eq!(v.waits[0].panel.steps[1], (Now, "far signs the first hand's opening".to_string()));

        // A later hand's stall is the usual one.
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::HandEnded { hand_id: 1, stacks: vec![], shown: vec![], pots: vec![], gained: vec![] });
        s.apply(NodeEvent::HandBegan { hand_id: 2, button: 1, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: None, group: false, quiet_s: Some(25), away: false });
        s.apply(NodeEvent::StageStands { hand_id: 2, seats: vec![2] });
        let v = s.table_view_of(0);
        assert_eq!(v.waits[0].panel.steps[1], (Now, "The cards cannot move on without it".to_string()));
    }

    /// `D-058`, the owner's rule: nothing over a hand that is being played -- the
    /// votes about a seat still heard are said only while its turn stands, and a
    /// return that goes quiet is not said at its seat past the hand after the one
    /// it was last heard at.
    #[test]
    fn no_wait_is_said_over_a_table_that_plays_on() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 1 });
        s.apply(NodeEvent::Roster {
            key: [7u8; 32],
            seats: vec![(0, "a".into(), 1_000), (1, "me".into(), 1_000), (2, "b".into(), 1_000)],
        });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::SeatLink { seat: 0, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::HandBegan { hand_id: 5, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });

        s.apply(NodeEvent::NotYourTurn { hand_id: 5, seat: Some(2), elapsed_ms: 0 });
        s.apply(NodeEvent::TimeoutVotes { seat: 2, held: 1, need: 2 });
        s.apply(NodeEvent::Swept { now_ms: 1 });
        let v = s.table_view().waits;
        assert_eq!(v.len(), 1, "its turn stands on the votes");
        assert_eq!(v[0].title, "Waiting for b");
        s.apply(NodeEvent::NotYourTurn { hand_id: 5, seat: Some(0), elapsed_ms: 0 });
        assert!(s.table_view().waits.is_empty(), "it acted after all: the votes are spent");
        s.apply(NodeEvent::Swept { now_ms: 2 });
        assert!(s.waits.is_empty(), "{:?}", s.waits);

        let coming_back = |s: &AppState| s.table_view().seats.iter().any(|x| x.seat == 0 && x.coming_back);
        s.apply(NodeEvent::HandBegan { hand_id: 6, button: 1, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::SeatLink { seat: 0, rtt_ms: None, group: false, quiet_s: Some(40), away: false });
        s.apply(NodeEvent::SeatCertified { seat: 0, hand_id: 6 });
        s.apply(NodeEvent::SitInAsked { seat: 0, hand_id: 6 });
        s.apply(NodeEvent::Swept { now_ms: 3 });
        assert!(s.table_view().waits.is_empty(), "{:?}", s.table_view().waits);
        assert!(coming_back(&s));
        s.apply(NodeEvent::HandBegan { hand_id: 7, button: 2, dealt_in: vec![1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::Swept { now_ms: 4 });
        assert!(coming_back(&s), "its votes may still come in the hand after");
        s.apply(NodeEvent::HandBegan { hand_id: 8, button: 1, dealt_in: vec![1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::Swept { now_ms: 5 });
        assert!(!coming_back(&s), "a return gone quiet is not said");
    }

    /// `D-057`: at two seats nobody answering is the question about the opponent
    /// (`D-046`), not this client's way back -- unless its own line went.
    #[test]
    fn heads_up_the_way_back_waits_for_this_clients_own_line() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        s.apply(NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "me".into(), 1_000), (1, "them".into(), 1_000)] });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::ToxLine { how: "udp" });
        s.apply(NodeEvent::TableReach { reachable: false });
        assert!(s.table_view().rejoin.is_none(), "the opponent may be the one gone");
        s.apply(NodeEvent::ToxLine { how: "offline" });
        let v = s.table_view().rejoin.expect("this client's own line went");
        assert_eq!(v.steps[0].1, "Connection to the network lost");
        s.apply(NodeEvent::ToxLine { how: "udp" });
        let v = s.table_view().rejoin.expect("still under way");
        assert_eq!(v.steps[1], (crate::gui::table::StepState::Done, "Network back".to_string()));
    }

    /// `S1-EJ`: a table founded for three is heads-up once one seat has left for good,
    /// and the question about the other -- and *Opponent left* -- comes as at a table
    /// founded for two.
    #[test]
    fn a_table_founded_for_three_is_heads_up_once_one_left_for_good() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        s.apply(NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "me".into(), 1_000), (1, "a".into(), 1_000), (2, "b".into(), 1_000)] });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        assert!(s.heads_up_opponent().is_none(), "three in the game: no heads-up");
        s.apply(NodeEvent::SeatLeftTable { seat: 2 });
        assert_eq!(s.heads_up_opponent(), Some(1), "one left for good: heads-up against the other");
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: false, quiet_s: Some(21), away: false });
        assert!(s.opponent_gone.is_some(), "the opponent out of reach opens the question");
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::SeatLeftTable { seat: 1 });
        assert!(s.opponent_left && s.opponent_out, "the opponent left: the game is over, said as at two seats");
    }

    /// `S1-EI`: at a bigger table the seats off the line are listed with what happens
    /// about each, and everybody off the line at once is heads-up's question.
    #[test]
    fn seats_off_the_line_are_said_and_everybody_off_is_the_question() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        s.apply(NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "me".into(), 1_000), (1, "a".into(), 1_000), (2, "b".into(), 1_000)] });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::HandBegan { hand_id: 3, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        assert!(s.absent_seats().is_empty(), "everybody on the line");
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: None, group: false, quiet_s: Some(25), away: false });
        s.apply(NodeEvent::HandWaiting { hand_id: 3, seats: vec![2] });
        let absent = s.absent_seats();
        assert_eq!(absent.len(), 1);
        assert!(absent[0].seat == 2 && absent[0].waited && !absent[0].certified, "{absent:?}");
        s.apply(NodeEvent::SeatCertified { seat: 2, hand_id: 3 });
        assert!(s.absent_seats()[0].certified, "certified out, said so");
        s.tick_opponent();
        assert!(s.opponent_gone.is_none(), "one seat off is not the question");
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: false, quiet_s: Some(21), away: false });
        s.tick_opponent();
        assert!(s.opponent_gone.as_ref().is_some_and(|g| g.alone), "everybody off: the question, about everybody");
        assert_eq!(s.opponent_returns, 0, "no returns counted at three seats");
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(1), away: false });
        s.tick_opponent();
        assert!(s.opponent_gone.is_none(), "a seat back: withdrawn");
        s.apply(NodeEvent::HandBegan { hand_id: 4, button: 1, dealt_in: vec![0, 1], small_blind: 10, big_blind: 20 });
        assert!(s.certified.is_empty(), "the next hand starts clean");
    }

    /// `S1-EE`: a seat-left about the player's own seat marks nothing -- the felt
    /// drew *left the table* behind the player's own cards after a reconnection,
    /// and no reading about one's own seat ever clears it.
    #[test]
    fn a_seat_left_about_the_players_own_seat_marks_nothing() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 2 });
        s.apply(NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "a".into(), 1_000), (2, "me".into(), 1_000), (8, "b".into(), 1_000)] });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::SeatLeft { seat: 2, quit: false, removed: false });
        assert!(!s.gone.contains(&2), "the player is here");
        assert!(!s.table_view().seats.iter().any(|v| v.seat == 2 && v.left), "and is not drawn as left");
        s.apply(NodeEvent::SeatLeft { seat: 8, quit: false, removed: false });
        assert!(s.gone.contains(&8), "another seat still is");
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
        // `S1-EC`: before the opponent was ever on the line, *not on the line* is a
        // seat still joining the group, not an absence -- run160251-2 spent a return
        // (D-032) and asked the question at 19 s while the joiner was handshaking.
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: false, quiet_s: None, away: false });
        assert!(s.opponent_gone.is_none(), "no episode before the opponent was ever reached");
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        assert!(s.opponent_gone.is_none() && s.opponent_returns == 0, "the first contact is not a return");
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: false, quiet_s: None, away: false });
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
        s.tick_opponent();
        assert_eq!(s.opponent_gone_for_s(), None, "and is not asked again at once");
        // `D-046`: thirty seconds later, still out of reach, the question comes back.
        s.opponent_gone.as_mut().unwrap().dismissed_at =
            Some(std::time::Instant::now() - std::time::Duration::from_millis(OPPONENT_ASK_AGAIN_MS + 1_000));
        s.tick_opponent();
        assert!(s.opponent_gone_for_s().is_some(), "asked again after the wait (D-046)");
        assert_eq!(s.log.iter().filter(|l| l.contains("unreachable")).count(), 2, "and said again");
        s.dismiss_opponent_gone();
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(40), group: false, quiet_s: None, away: false });
        assert!(s.opponent_gone.is_none(), "a ping ends the episode");
        s.apply(NodeEvent::Carrier { seen: 0, want: 1 });
        assert!(s.opponent_gone.is_none(), "an empty group is the other seat still joining it, not an opponent out of reach");
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: false, quiet_s: None, away: false });
        assert!(s.opponent_gone.as_ref().is_some_and(|g| g.dismissed_at.is_none()), "a closed connection is a new episode, asked anew");
        s.apply(NodeEvent::Carrier { seen: 1, want: 1 });
        assert!(s.opponent_gone.is_some(), "a seat the group merely sees is not one this client can reach");
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(30), group: false, quiet_s: None, away: false });
        assert!(s.opponent_gone.is_none(), "a ping is");
        // And a reading gone stale -- no ping for longer than the window doubts --
        // starts an episode by itself.
        s.links.insert(1, (Some(30), false, None, std::time::Instant::now() - std::time::Duration::from_millis(app_link_stale_ms() + 1_000)));
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
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: false, quiet_s: None, away: false });
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
        // `S1-EC`: the opponent has been on the line once; before that nothing is an absence.
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(40), group: false, quiet_s: None, away: false });
        assert_eq!(s.opponent_returns, 0, "the first contact is no return");
        for n in 1..=MAX_RETURNS {
            s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: false, quiet_s: None, away: false });
            s.opponent_gone.as_mut().unwrap().since = std::time::Instant::now() - std::time::Duration::from_millis(OPPONENT_GONE_MS + 1_000);
            s.tick_opponent();
            assert!(!s.opponent_out, "absence {n} is not the last");
            s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(40), group: false, quiet_s: None, away: false });
            assert_eq!(s.opponent_returns, n, "return {n} counted");
            assert!(s.log.back().unwrap().contains("D-032"), "and said");
        }
        // A blip shorter than the question does not count.
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: false, quiet_s: None, away: false });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(40), group: false, quiet_s: None, away: false });
        assert_eq!(s.opponent_returns, MAX_RETURNS, "a blip is no return");
        // The fourth absence is final.
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: false, quiet_s: None, away: false });
        s.opponent_gone.as_mut().unwrap().since = std::time::Instant::now() - std::time::Duration::from_millis(OPPONENT_GONE_MS + 1_000);
        s.tick_opponent();
        assert!(s.opponent_out, "the fourth absence ends the game");
        let line = s.log.back().unwrap().clone();
        assert!(line.contains("D-032") && line.contains("limit"), "{line}");
        assert!(s.table_view().opponent_out, "the window is told");
        assert!(s.table_view().opponent_gone_s.is_some(), "and still shows how long");
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(40), group: false, quiet_s: None, away: false });
        assert!(s.opponent_out, "and nothing reachable undoes it");
        assert_eq!(s.opponent_returns, MAX_RETURNS);
    }

    /// `D-034`: a present heads-up opponent long past their time to decide is
    /// asked about with the same question, said as what it is; the episode ends
    /// when they act, and it is no absence and no return.
    #[test]
    fn a_slow_heads_up_opponent_is_asked_about_and_the_question_ends_when_they_act() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        s.apply(NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "me".into(), 1_000), (1, "them".into(), 1_000)] });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::TableParams {
            key: [7u8; 32],
            name: "t".into(),
            seats: 2,
            needed: 2,
            small_blind: 50,
            big_blind: 100,
            action_ms: 30_000,
        });
        s.clock_for(Some(1), 0);
        s.tick_opponent();
        assert!(s.opponent_gone.is_none(), "inside their time, nothing is asked");
        s.turn_since = Some(std::time::Instant::now() - std::time::Duration::from_millis(30_000 + OPPONENT_GONE_MS + 1_000));
        s.tick_opponent();
        assert!(s.opponent_gone.as_ref().is_some_and(|g| g.slow), "past their time and the question's own wait: asked");
        assert!(s.opponent_gone_for_s().is_some(), "and asked at once");
        let line = s.log.back().unwrap().clone();
        assert!(line.contains("on the clock") && line.contains("D-007"), "{line}");
        assert!(s.table_view().opponent_gone_s.is_some());
        // They act: the question is withdrawn, and nothing was counted.
        s.clock_for(Some(0), 0);
        assert!(s.opponent_gone.is_none(), "the slow opponent acted");
        assert_eq!(s.opponent_returns, 0, "no absence, no return");
        // A ping while the slow question stands changes nothing either.
        s.clock_for(Some(1), 0);
        s.turn_since = Some(std::time::Instant::now() - std::time::Duration::from_millis(30_000 + OPPONENT_GONE_MS + 1_000));
        s.tick_opponent();
        assert!(s.opponent_gone.as_ref().is_some_and(|g| g.slow));
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(30), group: false, quiet_s: None, away: false });
        assert!(s.opponent_gone.is_none(), "a reading that reaches them ends any episode");
        assert_eq!(s.opponent_returns, 0, "but a slow one was no absence");
    }

    /// `D-035`: a seat whose client left the table's group is shown gone, and
    /// back when it is heard again; `S1-FQ`: heads-up only the seat's own word
    /// that it left ends the game.
    #[test]
    fn a_seat_that_left_the_group_is_gone_and_heads_up_only_its_own_word_ends_the_game() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        s.apply(NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "me".into(), 1_000), (1, "them".into(), 1_000)] });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(20), group: false, quiet_s: None, away: false });
        // A timeout: gone from the felt, not the end of the game.
        s.apply(NodeEvent::SeatLeft { seat: 1, quit: false, removed: false });
        assert!(s.gone.contains(&1));
        assert!(s.table_view().seats.iter().any(|v| v.seat == 1 && v.left), "drawn as left");
        assert!(!s.opponent_out && !s.opponent_left);
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(25), group: false, quiet_s: None, away: false });
        assert!(!s.gone.contains(&1), "seen in the group again: back");
        assert!(!s.table_view().seats.iter().any(|v| v.seat == 1 && v.left));
        // The carrier's *on purpose*: gone from the felt, and still no end.
        s.apply(NodeEvent::SeatLeft { seat: 1, quit: true, removed: false });
        assert!(s.gone.contains(&1) && !s.opponent_out && !s.opponent_left);
        assert!(s.log.back().unwrap().contains("left the table's group"), "{}", s.log.back().unwrap());
        // Its own word ends a game of two, at once.
        s.apply(NodeEvent::SeatLeftTable { seat: 1 });
        assert!(s.opponent_out && s.opponent_left, "the game is over");
        let v = s.table_view();
        assert!(v.opponent_left && v.opponent_gone_s.is_some(), "the window is told at once");
        assert_eq!(s.opponent_returns, 0, "a leave is no return");
    }

}
