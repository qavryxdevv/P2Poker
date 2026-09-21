//! The lobby pane: available tables, their state, and joining one.
//!
//! `SPEC_CS.md` §22 asks for PokerTH's shape — a table list with columns for the
//! game, the blinds, the occupancy and the state, a player list, chat, a join
//! button and a create button. All of it is still here. `D-067` (2026-09-18)
//! changed how it is *said*: the lobby is a card room and not a network console
//! — the one action a player wants first is the largest thing on the screen,
//! a table's seats are drawn rather than counted, an empty list is an
//! invitation and never a zero, and everything technical about the connection
//! is one click away rather than on the screen. The words are here, as
//! functions with tests, because a word on a screen is a claim and every claim
//! this pane makes must be true: nothing is ever invented — not a player, not
//! a number, not a wait.
//!
//! # Why the logic is here and the drawing is a thin layer over it
//!
//! Every derivation a row needs — what the state text says, whether the join
//! button may be pressed, what a warning means — is a plain function with a
//! test. The `egui` code below reads those functions and draws.
//!
//! That is not a stylistic preference. A rule that lives inside a paint callback
//! is a rule nothing can check, and this pane carries two that matter: a table
//! whose parameters changed under a live advert **must not be joinable**, and a
//! password-protected table **must say what its password does not protect**.
//!
//! # What the pane must never do
//!
//! Show a card. That is the table window's, and `SPEC_CS.md` §22 closes with the
//! rule this whole project is arranged around: *never display a
//! cryptographically unverified card as valid.* The lobby holds adverts, and an
//! advert has no cards in it.

use crate::net::lobby::{Held, LobbyStore, Mode};
use crate::net::matchmaker::Format;
use crate::table::formation::Roster;

/// One row of the table list.
///
/// The columns are the Python client's — Table, Players, Type, Stack, Blinds,
/// Timing, Host — because that layout has been used and this one had not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    /// The table's key, which is its identity. Shown truncated; used whole.
    pub key: [u8; 32],
    pub name: String,
    /// "NLHE cash" or "NLHE Sit & Go".
    pub game: String,
    /// "10 / 20".
    pub blinds: String,
    /// "2 / 6".
    pub occupancy: String,
    /// How many are seated, as a number rather than as text.
    pub seated: u8,
    /// How many the table needs before it can start.
    pub needed: u8,
    /// What the sit-down dialog should offer: the maximum a cash table allows,
    /// or the one stack a tournament pays. Offered rather than imposed — the
    /// founder checks it either way — but a dialog that opened on a number the
    /// table would refuse is a dialog that wastes a round trip.
    pub default_buyin: u64,
    /// The starting stack for a tournament, or the buy-in range for cash.
    pub stack: String,
    /// "20 s + 5 s", the clock a player gets to act.
    pub timing: String,
    /// The founder, as eight characters of its key.
    pub host: String,
    pub state: TableState,
    pub password_required: bool,
    /// How many seats the table has, as a number: the seats are drawn.
    pub seats: u8,
    /// A Sit & Go, as against a cash game.
    pub sit_and_go: bool,
    /// The clock a player gets to act, in seconds.
    pub action_s: u32,
}

/// What the state column says, and what the join button does about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableState {
    /// Seats free and nothing wrong with it.
    Open,
    /// Every seat taken.
    Full,
    /// The founder re-signed the advert with different parameters under a live
    /// one, so two joiners would have been handed two rule sets.
    ///
    /// **Not joinable, and it does not become joinable.** A founder who wants to
    /// change the game forms a new table under a new key, which is what changing
    /// the game means.
    ParametersChanged,
}

impl TableState {
    /// Whether the join button is live.
    pub const fn joinable(&self) -> bool {
        matches!(self, TableState::Open)
    }

    /// The text in the state column.
    pub const fn label(&self) -> &'static str {
        match self {
            TableState::Open => "open",
            TableState::Full => "full",
            // One word, because it shares a row with seven other columns.
            // What it means is on the hover and in the information pane, and
            // "parameters changed" in a table cell is a column nothing else
            // fits beside.
            TableState::ParametersChanged => "changed",
        }
    }

    /// What a player is told when they ask why they cannot sit down.
    ///
    /// A refusal with no reason is indistinguishable from a broken client, and a
    /// player who is not told will conclude the software is at fault.
    pub const fn why_not(&self) -> Option<&'static str> {
        match self {
            TableState::Open => None,
            TableState::Full => Some("Every seat is taken."),
            TableState::ParametersChanged => Some(
                "The founder changed this table's rules while it was advertised. \
                 Two players joining now could be given two different rule sets, \
                 so this client will not sit down. A table whose game changes is \
                 a new table.",
            ),
        }
    }
}

/// Derive a row from a held advert.
pub fn row(key: [u8; 32], held: &Held) -> TableRow {
    let ad = &held.ad;
    let game = match Mode::parse(ad.mode) {
        Some(Mode::CashPlayMoney) => "NLHE cash".to_string(),
        Some(Mode::TournamentSngPlayMoney) => "NLHE Sit & Go".to_string(),
        // An advert with an unknown mode never reaches the store — §7.2 rule 2
        // refuses it — so this arm is unreachable through the wire. It says what
        // it is rather than pretending, because a row that lied about the game
        // would be worse than one that admitted it did not know.
        None => "unknown".to_string(),
    };

    let state = if held.unjoinable {
        TableState::ParametersChanged
    } else if ad.players >= ad.max_players {
        TableState::Full
    } else {
        TableState::Open
    };

    let stack = if Mode::parse(ad.mode).is_some_and(|m| m.is_tournament()) {
        format!("{}", ad.start_stack)
    } else {
        format!("{} - {}", ad.min_buyin, ad.max_buyin)
    };

    TableRow {
        key,
        name: ad.table_name.clone(),
        game,
        blinds: format!("{} / {}", ad.small_blind, ad.big_blind),
        occupancy: format!("{} / {}", ad.players, ad.max_players),
        seated: ad.players,
        needed: ad.min_players_to_start,
        default_buyin: if Mode::parse(ad.mode).is_some_and(|m| m.is_tournament()) {
            ad.start_stack
        } else {
            ad.max_buyin
        },
        stack,
        timing: format!(
            "{} s + {} s",
            ad.action_timeout_ms / 1_000,
            ad.action_grace_ms / 1_000
        ),
        host: short_key(&ad.founder_app_key),
        state,
        password_required: ad.password_required,
        seats: ad.max_players,
        sit_and_go: Mode::parse(ad.mode).is_some_and(|m| m.is_tournament()),
        action_s: ad.action_timeout_ms / 1_000,
    }
}

/// A table's shape in a player's words: *heads-up*, *6-max*, *full ring*.
pub fn shape_words(seats: u8) -> String {
    match seats {
        2 => "heads-up".to_string(),
        10 => "full ring".to_string(),
        n => format!("{n}-max"),
    }
}

/// The badge a row wears: the game, and what the game means for somebody who
/// has not played it -- the hover text, so the lobby teaches a newcomer without
/// lecturing a regular.
pub fn format_badge(row: &TableRow) -> (&'static str, &'static str) {
    if row.sit_and_go {
        (
            "Sit & Go",
            "Sit & Go: everybody starts with the same stack, the game begins when the \
             seats are in, and it ends when one player holds every chip.",
        )
    } else {
        (
            "Cash",
            "Cash game: sit down with a buy-in of your choice, play as long as you \
             like, and leave between hands with what you have.",
        )
    }
}

/// What the table is doing, in a sentence a player acts on.
///
/// *Waiting for 2 more* is the goal-gradient effect put to honest use: a table
/// two seats short is a table somebody can start by sitting down, and the
/// sentence says how far it is rather than only that it is open. A full table
/// and one whose rules changed say so; the reason for the latter stays on the
/// hover and in the information pane, as before.
pub fn status_words(row: &TableRow) -> String {
    match row.state {
        TableState::ParametersChanged => "rules changed".to_string(),
        TableState::Full => "full".to_string(),
        TableState::Open => {
            let short = row.needed.saturating_sub(row.seated);
            if short > 0 {
                format!("waiting for {short} more")
            } else {
                let left = row.seats.saturating_sub(row.seated);
                if left == 1 {
                    "1 seat left".to_string()
                } else {
                    format!("{left} seats left")
                }
            }
        }
    }
}

/// How the list is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// The tables about to start first: open before full, the fullest first,
    /// then by name. What a player looking for a game now wants at the top.
    #[default]
    ClosestToStart,
    Name,
    /// The biggest blinds first.
    Blinds,
}

impl Sort {
    pub const ALL: [Sort; 3] = [Sort::ClosestToStart, Sort::Name, Sort::Blinds];

    pub const fn label(self) -> &'static str {
        match self {
            Sort::ClosestToStart => "closest to starting",
            Sort::Name => "by name",
            Sort::Blinds => "by blinds",
        }
    }
}

/// The rows in the order asked for. Every order ends on the name and then the
/// key, so two tables alike in every counted way keep one order between
/// frames: `rows` already promises the list does not reshuffle under the
/// cursor, and a sort that broke ties by arrival would break that promise.
pub fn sorted<'a>(mut rows: Vec<&'a TableRow>, sort: Sort) -> Vec<&'a TableRow> {
    let rank = |r: &TableRow| match r.state {
        TableState::Open => 0u8,
        TableState::Full => 1,
        TableState::ParametersChanged => 2,
    };
    let fill = |r: &TableRow| {
        if r.seats == 0 {
            0u32
        } else {
            u32::from(r.seated) * 1_000 / u32::from(r.seats)
        }
    };
    let blinds = |r: &TableRow| {
        r.blinds
            .split('/')
            .next_back()
            .and_then(|b| b.trim().parse::<u64>().ok())
            .unwrap_or(0)
    };
    match sort {
        Sort::ClosestToStart => rows.sort_by(|a, b| {
            rank(a)
                .cmp(&rank(b))
                .then(fill(b).cmp(&fill(a)))
                .then(b.seated.cmp(&a.seated))
                .then(a.name.cmp(&b.name))
                .then(a.key.cmp(&b.key))
        }),
        Sort::Name => rows.sort_by(|a, b| a.name.cmp(&b.name).then(a.key.cmp(&b.key))),
        Sort::Blinds => rows.sort_by(|a, b| {
            blinds(b)
                .cmp(&blinds(a))
                .then(a.name.cmp(&b.name))
                .then(a.key.cmp(&b.key))
        }),
    }
    rows
}

/// The colour a word is said in, decided here so the drawing does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Dim,
    Ok,
    Accent,
    Warn,
    Danger,
}

/// The header's counts, in a person's words and **only when they are not
/// zero**.
///
/// *0 tables, 0 open, 0 in lobby, 0 searching* was the first thing the old
/// header said, and four zeroes read as an empty restaurant. What is happening
/// is said; what is not happening is not announced. Nothing is rounded up and
/// nobody is counted twice: the DHT's several hundred peers are not players
/// and are not here (they are in the network details).
pub fn headline_counts(tables: usize, open: usize, online: usize, searching: usize) -> Vec<(String, Tone)> {
    let plural = |n: usize, one: &str, many: &str| if n == 1 { one.to_string() } else { many.to_string() };
    let mut out = Vec::new();
    if tables > 0 {
        out.push((format!("{tables} {}", plural(tables, "table", "tables")), Tone::Dim));
    }
    if open > 0 {
        out.push((format!("{open} open"), Tone::Ok));
    }
    if online > 0 {
        out.push((format!("{online} {} online", plural(online, "player", "players")), Tone::Accent));
    }
    if searching > 0 {
        out.push((format!("{searching} searching"), Tone::Warn));
    }
    out
}

/// A session's length, said the way a person says it.
pub fn session_words(secs: u64) -> String {
    let mins = secs / 60;
    if mins < 60 {
        format!("{mins} min")
    } else {
        format!("{} h {:02} min", mins / 60, mins % 60)
    }
}

/// The lobby's word on one finished game, for the card about the player.
///
/// A win is congratulated; a place is stated. **Neither asks for anything
/// back**: *win it back* is the sentence a casino writes, and this lobby does
/// not write it.
pub fn result_words(e: &crate::storage::results::Entry) -> String {
    let place = crate::gui::table::ordinal(usize::from(e.place));
    if e.won() {
        format!("You won at {}", e.table)
    } else if e.tied {
        format!("Tied for {place} of {} at {}", e.seats, e.table)
    } else {
        format!("{} of {} at {}", capitalised(&place), e.seats, e.table)
    }
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// A word for a good result, and none for the rest: a win is congratulated,
/// a top place at a table of six or more is praised, and a place is a place.
pub fn result_praise(e: &crate::storage::results::Entry) -> Option<&'static str> {
    if e.won() {
        Some("Congratulations!")
    } else if e.place <= 3 && e.seats >= 6 {
        Some("Well played.")
    } else {
        None
    }
}

/// The format's name on a chip: short, because four of them share one row
/// with the word beside them.
pub const fn format_chip(f: Format) -> &'static str {
    match f {
        Format::HeadsUp => "Heads-up",
        Format::SixMax => "6-max",
        Format::FullRing => "Full ring",
        Format::Any => "Any format",
    }
}

/// The line under the big button: what a search takes, when that is known
/// from this profile's own searches, and who else is looking right now, when
/// anybody is. Never a number the client has not measured: before the first
/// search it says what the wait depends on.
pub fn hero_note(typical_s: Option<u32>, format: Format, searching: u32) -> String {
    let mut note = match typical_s {
        Some(t) => format!(
            "Searches for {} usually take about {} here.",
            format_chip(format).to_lowercase(),
            clock(u64::from(t))
        ),
        None => "How long it takes depends on who else is looking; the search shows an estimate as it learns.".to_string(),
    };
    match searching {
        0 => {}
        1 => note.push_str(" 1 player is searching right now."),
        n => note.push_str(&format!(" {n} players are searching right now.")),
    }
    note
}

/// Every table the client holds, in a stable order.
///
/// Sorted by name and then by key, so the list does not reshuffle under the
/// user's cursor every time an advert is re-broadcast. Arrival order would.
pub fn rows(store: &LobbyStore) -> Vec<TableRow> {
    let mut out: Vec<TableRow> = store.tables().map(|l| row(*l.key, l.held)).collect();
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.key.cmp(&b.key)));
    out
}

/// The list filters, as the Python client offers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    Open,
    SitAndGo,
    Cash,
    WaitingForPlayers,
}

impl Filter {
    /// `D-072`: `Cash` is not offered. A cash advert is refused before it is
    /// filed, so the choice could only ever show an empty list -- and a
    /// filter that is always empty reads as a lobby with nobody in it. The
    /// variant itself stays: it is what a row's own game text is matched
    /// against, and `sit_and_go` on a `TableRow` is still a question about a
    /// table somebody else is offering.
    pub const ALL: [Filter; 4] =
        [Filter::All, Filter::Open, Filter::SitAndGo, Filter::WaitingForPlayers];

    pub const fn label(self) -> &'static str {
        match self {
            Filter::All => "all tables",
            Filter::Open => "open tables",
            Filter::SitAndGo => "sit & go",
            Filter::Cash => "cash game",
            Filter::WaitingForPlayers => "waiting for players",
        }
    }

    fn keeps(self, row: &TableRow) -> bool {
        match self {
            Filter::All => true,
            Filter::Open => row.state.joinable(),
            Filter::SitAndGo => row.game.contains("Sit & Go"),
            Filter::Cash => row.game.contains("cash"),
            // A table nobody can start yet, which is the one a player can
            // usefully do something about by sitting down.
            Filter::WaitingForPlayers => row.state.joinable() && row.seated < row.needed,
        }
    }
}

/// Which rows survive the search box and the filter.
///
/// The search matches the table's **name or its host**, which is what the
/// Python client's placeholder promises, and it is case-insensitive because a
/// search that is not is a search that finds nothing.
pub fn visible<'a>(rows: &'a [TableRow], search: &str, filter: Filter) -> Vec<&'a TableRow> {
    let needle = search.trim().to_lowercase();
    rows.iter()
        .filter(|r| filter.keeps(r))
        .filter(|r| {
            needle.is_empty()
                || r.name.to_lowercase().contains(&needle)
                || r.host.to_lowercase().contains(&needle)
        })
        .collect()
}

/// What an empty list says: a title and a line under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyState {
    pub title: &'static str,
    pub body: &'static str,
    /// Still connecting: the drawing shows it is working rather than waiting.
    pub connecting: bool,
}

/// What to say when the list is empty, which is three different things.
///
/// *"This lobby is quiet"*, *"your filter hides everything"* and *"not on the
/// network yet"* look identical on screen and need three reactions, so the
/// message says which. And each one is an **invitation, not an apology**: the
/// old text explained the network (*nobody is advertising a table*) where a
/// player wants to know what to do next -- start the search, and they are
/// seated the moment somebody else does.
pub fn empty_state(total: usize, shown: usize, peers: usize) -> Option<EmptyState> {
    if shown > 0 {
        return None;
    }
    Some(if total > 0 {
        EmptyState {
            title: "Every table is hidden",
            body: "Clear the search or the filter to see the rest.",
            connecting: false,
        }
    } else if peers == 0 {
        EmptyState {
            title: "Connecting\u{2026}",
            body: "Looking for other players on the network. Tables appear here as soon as they are found.",
            connecting: true,
        }
    } else {
        EmptyState {
            title: "Nobody is playing right now",
            body: "Start a search and you are seated the moment an opponent arrives \u{2014} \
                   or create a table and invite your friends.",
            connecting: false,
        }
    })
}

/// The three steps, for a card a newcomer reads once.
pub const HOW_IT_WORKS: [(&str, &str); 3] = [
    ("Find a game", "or create a table for your friends."),
    ("The table starts", "when its seats are in."),
    ("Every card is verified", "on your own machine, shuffled by all the players together."),
];

/// What *provably fair* means here, in four sentences, the fourth being what it
/// does not mean. The claim is `CRYPTOGRAPHY.md`'s: Barnett–Smart mental poker
/// with Bayer–Groth shuffle proofs and no server -- and its honesty clause.
pub const FAIR_PLAY: &str =
    "Every player shuffles the deck in turn and proves the shuffle was honest. A card is opened only \
     when every player releases their part of it, so nobody \u{2014} not even the table's founder \u{2014} \
     can see or stack a card. There is no server, no house and no rake. What no cryptography stops is \
     two players talking to each other outside the game; that is the same at every table.";

/// What a password-protected table's dialog must say.
///
/// §4.3 requires this in so many words, and it is here rather than in a comment
/// because a warning that is not rendered is not a warning.
pub const PASSWORD_WARNING: &str =
    "A table password proves you know it; it does not keep the table private. \
     Anyone who sees one join can guess a weak password offline at their leisure.";

/// The identity of a table, short enough for a column.
///
/// The **key**, and never the name: two tables may carry one name, and a name is
/// display data that a founder chooses. Eight hex characters of a 32-byte key is
/// not a collision-resistant identifier and is not used as one — it is a label a
/// person can read back, and every comparison in the client uses the whole key.
pub fn short_key(key: &[u8; 32]) -> String {
    key[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// The state of this client's own connection, for the network-status pane.
#[derive(Debug, Clone, Default)]
pub struct NetworkStatus {
    /// Which protocol opened a port, if one did. `"PCP"`, `"NAT-PMP"`, or
    /// `None` — and `None` is the ordinary case rather than a fault.
    pub port_mapped: Option<&'static str>,
    /// Everything this node is connected to, poker client or not — several
    /// hundred, because the lobby rides the public libp2p DHT. It is a true
    /// statement about the network and it is **not** a count of players, which
    /// is why the next field exists beside it rather than replacing it.
    pub peers: usize,
    /// Other poker clients: peers whose `identify` answered with this client's
    /// own protocol version. What a player means when they ask who is about.
    pub lobby_peers: usize,
    pub listening: Vec<String>,
    pub dht_announced: bool,
    pub relay: Option<RelayStatus>,
    /// `D-002` point 3 (`S1-FK`): this client's own relay -- the poker clients
    /// holding a reservation on it, and the circuits crossing it now.
    pub relaying: (usize, usize),
    pub public: Option<bool>,
    /// `S1-EH`: this client's line to the Tox network -- `udp`, `tcp` or
    /// `offline`; `None` before any table rode it.
    pub tox: Option<&'static str>,
    /// How many dials have failed.
    ///
    /// Counted rather than logged line by line: most dials fail on an open DHT,
    /// so a log full of them buries what matters — but with no count at all,
    /// *most dials fail* and *this client is broken* look identical.
    pub failed_dials: usize,
}

/// What a relay reservation bought, in the terms a player cares about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayStatus {
    pub peer: String,
    pub adequate: bool,
}

impl NetworkStatus {
    /// One line, honest about the worst thing that is true.
    ///
    /// The order matters: a client that cannot be reached and has no relay
    /// cannot play, and saying "connected to 4 peers" while that is true would
    /// be the most misleading thing on the screen.
    ///
    /// **What is deliberately not here:** whether the relay's budget can carry a
    /// hand. It used to lead — *"Relay found, but it cannot carry a hand"* — and
    /// it was the wrong thing in the wrong place. Every public relay reports the
    /// library defaults, so it became the permanent headline the moment this
    /// client learned to find one; it named a table's problem while the player
    /// was still reading a lobby; and it ended with *"see the note"*, a note
    /// nothing on the screen offered. [`RelayStatus::adequate`] still carries
    /// the finding, and `net::relay` still decides on it — at the point it
    /// belongs, which is somebody sitting down.
    pub fn summary(&self) -> String {
        match (self.public, self.relay.is_some(), self.peers) {
            (Some(false), false, _) => {
                "Behind NAT and no relay found — tables may not be joinable".into()
            }
            (_, _, 0) => "Looking for peers".into(),
            (_, true, n) => format!(
                "Connected to {n} peer{} through a relay",
                if n == 1 { "" } else { "s" }
            ),
            (_, _, n) => format!("Connected to {n} peer{}", if n == 1 { "" } else { "s" }),
        }
    }

    /// The one light and its word, for the strip a player reads: *Online*,
    /// and nothing technical about how. Red only for the one thing that is a
    /// real fault of the connection -- unreachable and relayless -- which is
    /// [`summary`](Self::summary)'s first arm, in the same order. The rest of
    /// what the connection is doing is one click away, in the details.
    pub fn headline(&self) -> (&'static str, Tone) {
        match (self.public, self.relay.is_some(), self.peers) {
            (Some(false), false, _) => (
                "No way in from the internet \u{2014} tables may not be joinable",
                Tone::Danger,
            ),
            (_, _, 0) => ("Connecting\u{2026}", Tone::Dim),
            (_, true, _) => ("Online through a relay", Tone::Ok),
            _ => ("Online", Tone::Ok),
        }
    }
}

/// `S1-CS`: what the connecting window says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JoiningView {
    pub name: String,
    pub elapsed_s: u64,
    pub failed: Option<String>,
    /// `S1-DE`: the rejoin of the game on record -- said as such, and a
    /// failed one offers to forget the record.
    pub rejoin: bool,
    /// `S1-DE`: the node gave the rejoin up; the reason is in `failed`, and
    /// the one button left closes the window.
    pub gone: bool,
}

/// `S1-FG`: the table asked for is one this client is at already: its name,
/// and the slot whose window it is -- `None` for a join still under way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlreadyAtView {
    pub name: String,
    pub slot: Option<u8>,
}

/// Everything the pane draws, prepared away from the paint loop.
///
/// `SPEC_CS.md` §33: cryptography never blocks the GUI event loop, and this is
/// the shape that keeps it true — the pane reads a snapshot and never a lock.
#[derive(Debug, Clone, Default)]
pub struct LobbyView {
    /// This player's own name, as they chose it.
    pub me: String,
    pub tables: Vec<TableRow>,
    pub status: NetworkStatus,
    pub selected: Option<[u8; 32]>,
    pub chat: Vec<ChatLine>,
    pub seated: Vec<String>,
    /// `D-043`: every table this client sits at, the active one first.
    pub my_tables: Vec<crate::app::SlotView>,
    /// What has happened, newest last. A local view and never canonical state.
    pub log: Vec<String>,
    /// `S1-CR`: an unfinished game on record, for the window to ask about.
    pub unfinished: Option<crate::app::Unfinished>,
    /// `S1-CS`: a join in progress, for the small window that says so.
    pub joining: Option<JoiningView>,
    /// `S1-FG`: the tables this client sits at or joins; joining one of them
    /// again is refused, with the reason.
    pub here: Vec<[u8; 32]>,
    /// `S1-FG`: the word that the table asked for is one this client is at.
    pub already_at: Option<AlreadyAtView>,
    /// `D-064`: the automatic search under way, for its modal window.
    pub search: Option<SearchView>,
    /// `D-064`: other clients searching for a game, as the queue topic says.
    pub searching: u32,
    /// `D-067`: how long this client has been open, for the card about the
    /// player -- said, never nagged about.
    pub session_s: u64,
    /// `D-067`: this player's own record, from the profile; `None` while the
    /// window has not read it.
    pub record: Option<crate::storage::results::Summary>,
    /// `D-067`: the search just found a game, for the word about it.
    pub found: Option<FoundView>,
    /// `D-068`: the rewards on the card about the player; `None` with the
    /// switch in the settings off, and nothing of them is drawn.
    pub rewards: Option<super::rewards::YouRewards>,
    /// `D-068`: one neutral sentence about the rewards file, when a load had
    /// one to say.
    pub rewards_notice: Option<&'static str>,
    /// `D-068`: a card or a level to show for a moment -- only while no hand
    /// is being played at any of this client's tables.
    pub reveal: Option<RevealView>,
    /// `D-068`: this profile is running on another device as well. No new game
    /// is started from here until it is not; a game under way is never touched.
    pub profile_elsewhere: bool,
}

/// `D-068`: what the lobby says while the profile runs in two places.
pub const PROFILE_ELSEWHERE: &str = "This profile is also running on another device. One profile is one player: \
     close p2p-poker on one of the two. Until then no new game is started from here; a game you are \
     playing goes on.";

/// `D-068`: what the lobby shows for a moment when something was earned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealView {
    pub title: String,
    pub text: String,
    /// The card earned, where it is one -- the rarest of them, where they are several.
    pub card: Option<&'static crate::app::rewards::catalog::Card>,
    pub age_ms: u64,
    /// How long this one stays: longer for several cards than for one.
    pub stays_ms: u64,
}

/// `D-068`: how long a reveal stays.
pub const REVEAL_MS: u64 = 6_000;

/// What each further card of one reveal adds to [`REVEAL_MS`], and the most a
/// reveal stays: four names take longer to read than one, and not for ever.
pub const REVEAL_MORE_MS: u64 = 1_500;
pub const REVEAL_MAX_MS: u64 = 12_000;

/// How long a reveal has to have been up, when a hand begins, to count as seen.
/// Under the five seconds a table pauses between two hands, so that a reveal
/// that comes up as a hand ends is over when the next one begins.
pub const REVEAL_SEEN_MS: u64 = 2_500;

/// How many cards one reveal names; the rest are counted.
const REVEAL_NAMES: usize = 4;

/// How many reveals wait at most. Bounded, as everything fed from the network is.
const REVEALS_KEPT: usize = 8;

/// One thing waiting to be revealed.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Waiting {
    /// Cards earned, which wait together and are revealed as one.
    Cards(Vec<&'static crate::app::rewards::catalog::Card>),
    Words { title: String, text: String },
}

/// What [`Reveals::now`] gives the window: the reveal, and whether this is the
/// moment its sound is played.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub view: RevealView,
    pub sound: bool,
}

/// `D-068`: what waits to be revealed at the top of the lobby, and the rule that
/// **each thing is revealed once**.
///
/// **It used to be revealed after every hand, for as long as the game lasted.**
/// Nothing is shown while a hand is played, so a reveal waits for the pause
/// between two hands -- five seconds -- and it had to be up for [`REVEAL_MS`],
/// six, to leave the queue. The next hand took it down a second short, reset its
/// clock, and the pause after that hand brought the same card up again as new,
/// with its sound: a card that had been in the album for half an hour was
/// announced forty times over in a game of forty hands (the owner, 2026-09-20).
///
/// So a reveal that was up for [`REVEAL_SEEN_MS`] when a hand begins has been
/// seen and is gone; one cut shorter than that comes back once more, in full,
/// and says nothing the second time. And the cards that wait together are one
/// reveal -- *3 new cards*, the rarest of them pictured -- where they were a
/// run of eight reveals of six seconds each after a good game, beside a summary
/// that names every one of them anyway.
///
/// The clock is the caller's, in milliseconds that only go forward.
#[derive(Debug, Default)]
pub struct Reveals {
    waiting: std::collections::VecDeque<Waiting>,
    /// Since when the first of them is up, if it is.
    up_since_ms: Option<u64>,
    /// Whether the first of them has made its sound.
    sounded: bool,
}

impl Reveals {
    /// A card was earned. It joins the cards that wait behind the reveal that is
    /// up, so what is being read never changes under the reader.
    pub fn card(&mut self, card: &'static crate::app::rewards::catalog::Card) {
        let already = self.waiting.iter().any(|w| matches!(w, Waiting::Cards(c) if c.iter().any(|x| x.id == card.id)));
        if already {
            return;
        }
        let last_is_up = self.up_since_ms.is_some() && self.waiting.len() == 1;
        match self.waiting.back_mut() {
            Some(Waiting::Cards(cards)) if !last_is_up => cards.push(card),
            _ => self.waiting.push_back(Waiting::Cards(vec![card])),
        }
        self.bound();
    }

    /// Something said in words: a level, a break.
    pub fn words(&mut self, title: impl Into<String>, text: impl Into<String>) {
        self.waiting.push_back(Waiting::Words { title: title.into(), text: text.into() });
        self.bound();
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }

    /// The oldest reveal that is not up goes first.
    fn bound(&mut self) {
        while self.waiting.len() > REVEALS_KEPT {
            if self.up_since_ms.is_some() {
                self.waiting.remove(1);
            } else {
                self.waiting.pop_front();
            }
        }
    }

    /// The reveal to show at `now_ms`, if any. `may_show` is false while a hand
    /// is being played at any of this client's tables, and nothing is shown then.
    pub fn now(&mut self, now_ms: u64, may_show: bool) -> Option<Shown> {
        if !may_show {
            if let Some(since) = self.up_since_ms.take() {
                if now_ms.saturating_sub(since) >= REVEAL_SEEN_MS {
                    self.waiting.pop_front();
                    self.sounded = false;
                }
            }
            return None;
        }
        loop {
            let first = self.waiting.front()?;
            let stays_ms = match first {
                Waiting::Cards(cards) => {
                    (REVEAL_MS + REVEAL_MORE_MS * (cards.len() as u64).saturating_sub(1)).min(REVEAL_MAX_MS)
                }
                Waiting::Words { .. } => REVEAL_MS,
            };
            let since = *self.up_since_ms.get_or_insert(now_ms);
            let age_ms = now_ms.saturating_sub(since);
            if age_ms >= stays_ms {
                self.waiting.pop_front();
                self.up_since_ms = None;
                self.sounded = false;
                continue;
            }
            let sound = !self.sounded && matches!(first, Waiting::Cards(_));
            let view = match first {
                Waiting::Cards(cards) => {
                    let names: Vec<String> =
                        cards.iter().take(REVEAL_NAMES).map(|c| format!("{} {}", c.label(), c.title)).collect();
                    let more = cards.len().saturating_sub(REVEAL_NAMES);
                    // The first of the rarest: `max_by_key` alone would take the last.
                    let rarest = cards.iter().rev().max_by_key(|c| c.rarity()).copied();
                    // Broken between two names and never inside one: left to the
                    // label, the fifth card's *and 1 more* ended a line on *1* and put
                    // *more* alone under it (photographed, 2026-09-20).
                    let mut lines: Vec<String> =
                        names.chunks(names.len().div_ceil(2).max(2)).map(|l| l.join(" \u{00b7} ")).collect();
                    if more > 0 {
                        if let Some(last) = lines.last_mut() {
                            last.push_str(&format!(" and {more} more"));
                        }
                    }
                    RevealView {
                        title: if cards.len() == 1 { "New card".into() } else { format!("{} new cards", cards.len()) },
                        text: lines.join("\n"),
                        card: rarest,
                        age_ms,
                        stays_ms,
                    }
                }
                Waiting::Words { title, text } => {
                    RevealView { title: title.clone(), text: text.clone(), card: None, age_ms, stays_ms }
                }
            };
            self.sounded = true;
            return Some(Shown { view, sound });
        }
    }
}

/// `D-064`: the search under way, as the modal window shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchView {
    pub id: u32,
    pub format: &'static str,
    /// How many games at once the player asked for.
    pub games: u8,
    pub elapsed_s: u64,
    /// The node's last word on the search, once it has said one.
    pub report: Option<crate::net::matchmaker::SearchReport>,
    /// What searches of this format took before, from the profile.
    pub typical_s: Option<u32>,
}

/// Seconds as `mm:ss`: the search clock's face.
pub fn clock(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// `D-067`: the search found a game -- said once, for a moment, in the
/// table's style, and then not again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundView {
    /// The table's name, as the node said it.
    pub table: String,
    /// How long ago the game started.
    pub age_ms: u64,
}

/// How long the word about a found game stays: long enough to read twice,
/// short enough that nobody waits for it to go.
pub const FOUND_TOAST_MS: u64 = 4_500;

/// The search window's estimate, in the order of what is known: the search's
/// own, this profile's history, or an honest *measuring…*.
pub fn eta_words(eta_s: Option<u64>, typical_s: Option<u32>) -> String {
    match (eta_s, typical_s) {
        (Some(e), _) => format!("about {}", clock(e)),
        (None, Some(t)) => format!("usually about {}", clock(u64::from(t))),
        (None, None) => "measuring\u{2026}".to_string(),
    }
}

/// Who else is looking, in a person's words. The queue's silence before it
/// could be heard is not a zero (`S1-HK`), so it is *finding out…*; a heard
/// zero is *nobody else right now*, which is true and is not an apology.
pub fn queue_words(report: Option<&crate::net::matchmaker::SearchReport>) -> String {
    match report {
        None => "finding out\u{2026}".to_string(),
        Some(r) if !r.queue_known => "finding out\u{2026}".to_string(),
        Some(r) if r.queue == 0 => "nobody else right now".to_string(),
        Some(r) => {
            let who = if r.queue == 1 { "1 other player".to_string() } else { format!("{} other players", r.queue) };
            match r.queue_wait_s {
                Some(w) => format!("{who} (waiting {} on average)", clock(w)),
                None => who,
            }
        }
    }
}

/// The seats held, as a heading: how many tables hold one for this player,
/// or -- when none does -- what the search does about that.
pub fn held_words(held: usize) -> String {
    match held {
        0 => "No table holds a seat for you yet. If nothing turns up, the search founds one.".to_string(),
        1 => "A seat is held for you at 1 table".to_string(),
        n => format!("A seat is held for you at {n} tables"),
    }
}

/// The table closest to starting among the seats held: the fullest against
/// what it needs, and the most players at a tie. `None` while no seat is held.
pub fn closest_table(
    report: Option<&crate::net::matchmaker::SearchReport>,
) -> Option<&crate::net::matchmaker::ReservationView> {
    report?
        .reservations
        .iter()
        .filter(|x| x.capacity > 0)
        .max_by_key(|x| (u32::from(x.players) * 1_000 / u32::from(x.capacity), x.players))
}

/// `S1-IE`: the seats a held table's drawing shows -- what its founder needs
/// **now** to start, never fewer than sit there and never more than the table
/// has -- so the rings go as the founder comes down. The window drew the
/// table's ten seats from the first second to the last while the node said
/// the falling number every second (the owner, 2026-09-18).
pub fn needed_now(x: &crate::net::matchmaker::ReservationView) -> u8 {
    x.capacity.max(x.players).min(x.seats.max(x.players)).max(2)
}

/// One held table's line: how many of the players needed now are there, the
/// table's size, whose it is. At another client's table the number needed is
/// this client's reading of that founder's clock, and is said as *about*.
pub fn reservation_words(x: &crate::net::matchmaker::ReservationView) -> String {
    let needed = needed_now(x);
    let mut s = if needed < x.seats {
        format!(
            "{} of {}{} needed to start \u{00b7} table of {}",
            x.players,
            if x.mine { "" } else { "about " },
            needed,
            x.seats
        )
    } else {
        format!("{} of {} seated", x.players, x.seats)
    };
    if x.mine {
        s.push_str(" \u{00b7} yours");
    }
    // *Starting* is a table with the players it needs now and this client's
    // consent; consent alone is not said -- a joiner gives it from two seats,
    // and *ready* beside *2 of about 9 needed* read as a promise.
    if x.armed && x.players >= needed {
        s.push_str(" \u{00b7} starting");
    }
    s
}

/// `S1-IE`: what a search table this client holds a seat at needs now, for
/// its row in the list -- from the search's own report. `None` for any other
/// table: an advert does not carry its founder's clock, and the row then says
/// the table's seats and nothing about when it starts.
pub fn row_needed(view: &LobbyView, key: &[u8; 32]) -> Option<u8> {
    view.search
        .as_ref()?
        .report
        .as_ref()?
        .reservations
        .iter()
        .find(|x| x.key.as_ref() == Some(key) && x.capacity < x.seats)
        .map(needed_now)
}

/// The row's sentence for a held search table: the distance to its start in
/// players, by what its founder needs now.
pub fn status_words_needed(seated: u8, needed: u8) -> String {
    match needed.saturating_sub(seated) {
        0 => "about to start".to_string(),
        n => format!("waiting for {n} more"),
    }
}

/// One line of lobby chat.
///
/// `who` is a display name and is **untrusted forever** — it is never an
/// identifier, and two players may choose one name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatLine {
    pub who: String,
    pub said: String,
}

impl LobbyView {
    pub fn from(store: &LobbyStore, status: NetworkStatus) -> Self {
        LobbyView {
            my_tables: Vec::new(),
            me: String::new(),
            tables: rows(store),
            status,
            selected: None,
            chat: Vec::new(),
            seated: Vec::new(),
            log: Vec::new(),
            unfinished: None,
            joining: None,
            here: Vec::new(),
            already_at: None,
            search: None,
            searching: 0,
            session_s: 0,
            record: None,
            found: None,
            rewards: None,
            rewards_notice: None,
            reveal: None,
            profile_elsewhere: false,
        }
    }

    /// The row the user has selected, if it is still in the list.
    ///
    /// Adverts expire, so a selection can name a table that is gone — and a pane
    /// that assumed otherwise would index into a shorter list.
    pub fn selected_row(&self) -> Option<&TableRow> {
        let key = self.selected?;
        self.tables.iter().find(|r| r.key == key)
    }

    /// Whether the join button is live for the current selection.
    pub fn can_join(&self) -> bool {
        self.selected_row().is_some_and(|r| r.state.joinable())
    }
}

/// What the create-table form must collect, and what it may not.
///
/// The buy-in bounds are the founder's to choose and the joiner's to check, and
/// the deadline is derived rather than typed: §7.2 rule 2a's minimum is a
/// function of the table's own shape, and a founder who typed a smaller number
/// would advertise a table where every hand aborts.
pub fn suggested_hand_deadline(
    seats: u8,
    action_timeout_ms: u32,
    action_grace_ms: u32,
    crypto_step_timeout_ms: u32,
    hand_delay_ms: u32,
    time_bank_ms: u32,
) -> u32 {
    crate::protocol::constants::hand_deadline_min_ms(
        seats,
        action_timeout_ms as u64,
        action_grace_ms as u64,
        crypto_step_timeout_ms as u64,
        hand_delay_ms as u64,
        time_bank_ms as u64,
    ) as u32
}

/// The seated players of a table, for the player list.
pub fn player_names(roster: &Roster) -> Vec<String> {
    roster
        .seats()
        .iter()
        .map(|e| {
            // Untrusted display data. A name that is empty or all spaces would
            // render as a blank row that a user cannot click, so it is replaced
            // rather than shown.
            let trimmed = e.display_name.trim();
            if trimmed.is_empty() {
                format!("seat {}", e.seat)
            } else {
                trimmed.to_string()
            }
        })
        .collect()
}

/// `D-078`: the system this client runs on, where the words for it differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Windows,
    /// Linux -- and whatever else is not Windows: the packages are Linux's.
    Linux,
}

impl System {
    pub const THIS: System = if cfg!(windows) { System::Windows } else { System::Linux };
}

/// `D-077`: whether the lobby is open to this version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    Open,
    /// The window has just opened and is asking GitHub: nobody sits down
    /// before the answer.
    Asking,
    /// A newer release is out, under `tag`, and this version does not sit down
    /// again: this beta's protocol still changes from one version to the next.
    Closed { latest: String, tag: String },
}

/// `D-077`: the gate, from what the version check has said.
///
/// **Only an answer that was read and understood closes the lobby.** Not
/// asked, asked by the button and not yet answered, the newest, no release at
/// all, and a check that could not be made are all open: a client that cannot
/// reach GitHub plays, and its log says why. The one wait is the question the
/// window asks as it opens, and that has a clock (`app::update`).
pub fn gate(update: &super::render::UpdateUi) -> Gate {
    use super::render::UpdateUi;
    use crate::app::update::Verdict;
    match update {
        UpdateUi::Opening => Gate::Asking,
        UpdateUi::Done(Ok(Verdict::Newer { latest, tag })) => Gate::Closed { latest: latest.clone(), tag: tag.clone() },
        UpdateUi::Idle | UpdateUi::Checking | UpdateUi::Done(_) => Gate::Open,
    }
}

/// `D-077`: what reaches the client from a frame of the lobby. With the gate
/// open, whatever the lobby produced; with it shut, **the gate's own window's
/// buttons and nothing else** -- not a click that reached a row before the
/// window covered it, and not a line typed into a box that kept the keyboard.
pub fn through_the_gate(
    gate: &Gate,
    lobby: super::render::LobbyAction,
    from_gate: Option<super::render::LobbyAction>,
) -> super::render::LobbyAction {
    match gate {
        Gate::Open => lobby,
        Gate::Asking | Gate::Closed { .. } => from_gate.unwrap_or(super::render::LobbyAction::None),
    }
}

/// `D-077`: the words of the window that closes the lobby.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedWords {
    pub heading: String,
    pub body: String,
    /// The gold button's label.
    pub download: String,
    /// What to do once the download is done.
    pub next: String,
    /// The folder `next` speaks of, in a well of its own -- a path inside a
    /// sentence buries the sentence (`D-073`'s first photograph).
    pub folder: Option<std::path::PathBuf>,
    /// This copy is portable: the player puts the new program in its place.
    pub by_hand: bool,
}

/// `D-077`: the window's words for release `latest`, to a client of version
/// `this` that lives at `home`, on `system`.
///
/// **What to do next depends on where this copy lives, and getting it wrong
/// makes a new player.** An installed copy is updated by the new file's own
/// installer (`D-073`), which keeps the profile. A portable copy keeps its
/// profile beside the program, so the new program goes where the old one is:
/// started from the downloads folder, it would find no player beside it and
/// offer to install a new one. `D-078`: a Linux package keeps the profile in
/// the user's data folder, so the new package is installed as this one was --
/// and the button opens the release page, which has the three packages, where
/// on Windows it hands over the one program.
pub fn closed_words(
    latest: &str,
    this: &str,
    home: Option<&super::render::HomeView>,
    system: System,
) -> ClosedWords {
    let packaged = system == System::Linux && home.is_none_or(|h| !h.profile.starts_with(&h.folder));
    let (next, folder, by_hand) = match home {
        _ if packaged => (
            "Choose the package for your system on the release page -- the AppImage, the .deb or the .rpm -- then \
             close P2Poker and install it the way you installed this one. Your player profile stays as it is, in \
             your home folder."
                .to_owned(),
            None,
            false,
        ),
        Some(h) if system == System::Linux => (
            "When it has downloaded, close P2Poker, put the new program in place of this one, in the folder below, \
             and start it there: your player profile is kept beside it and stays as it is."
                .to_owned(),
            Some(h.folder.clone()),
            true,
        ),
        Some(h) if h.installed => (
            "When it has downloaded, close P2Poker and start the new p2p-poker.exe: it offers to update the \
             installed copy, in the folder below, and your player profile stays as it is."
                .to_owned(),
            Some(h.folder.clone()),
            false,
        ),
        Some(h) => (
            "When it has downloaded, close P2Poker, put the new p2p-poker.exe in place of this one, in the folder \
             below, and start it there: your player profile is kept beside it and stays as it is. Started from \
             anywhere else, it would be a new player."
                .to_owned(),
            Some(h.folder.clone()),
            true,
        ),
        None => (
            "When it has downloaded, close P2Poker and start the new p2p-poker.exe. Your player profile stays as it is."
                .to_owned(),
            None,
            false,
        ),
    };
    ClosedWords {
        heading: format!("P2Poker {latest} is out"),
        body: format!(
            "This is a beta, and its protocol still changes from one version to the next, so version {this} can no \
             longer play with the players who have {latest}. Download the new version to go on playing."
        ),
        download: format!("Download {latest}"),
        next,
        folder,
        by_hand,
    }
}

/// `D-077`: what the window says once the gold button was pressed: whether the
/// system took the address -- on Windows the program itself, on Linux the
/// release page with the packages (`D-078`).
pub fn handed_words(handed: bool, system: System) -> &'static str {
    match (handed, system) {
        (true, System::Windows) => "The download was handed to your browser.",
        (true, System::Linux) => "The release page was opened in your browser.",
        (false, _) => "Your browser could not be started. Copy the address below into it.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::lobby::{BlindSchedule, TableAd, DECK_SUITE_V1};
    use crate::protocol::constants::hand_deadline_min_ms;

    const NOW: u64 = 1_700_000_000_000;

    fn the_card(id: &str) -> &'static crate::app::rewards::catalog::Card {
        crate::app::rewards::catalog::card_by_id(id).expect("a card of the catalogue")
    }

    /// A table as it is played: a hand of `HAND_MS`, then the five seconds it pauses
    /// before the next. Returns every reveal that came up (title, whether it
    /// sounded) -- one entry for each time something NEW was on the screen.
    fn played(reveals: &mut Reveals, from_ms: u64, hands: u32) -> (Vec<(String, bool)>, u64) {
        const HAND_MS: u64 = 10_000;
        const PAUSE_MS: u64 = 5_000;
        let mut seen: Vec<(String, bool)> = Vec::new();
        let mut up: Option<String> = None;
        let mut now = from_ms;
        for _ in 0..hands {
            for phase in [(HAND_MS, false), (PAUSE_MS, true)] {
                let end = now + phase.0;
                while now < end {
                    match reveals.now(now, phase.1) {
                        Some(shown) => {
                            let words = format!("{}: {}", shown.view.title, shown.view.text);
                            if up.as_deref() != Some(words.as_str()) {
                                seen.push((words.clone(), shown.sound));
                                up = Some(words);
                            } else {
                                assert!(!shown.sound, "a reveal sounds as it comes up and not while it stays");
                            }
                        }
                        None => up = None,
                    }
                    now += 100;
                }
            }
        }
        (seen, now)
    }

    /// `D-068`, the owner's tables, 2026-09-20: **a card is revealed once, however
    /// many hands are played after it.** It was revealed after every hand: the
    /// pause between two hands is five seconds, a reveal had to be up for six to
    /// leave the queue, and the hand that took it down reset its clock -- the same
    /// card came up as new, with its sound, forty times in a game of forty hands.
    #[test]
    fn a_card_is_revealed_once_however_many_hands_are_played() {
        let mut reveals = Reveals::default();
        reveals.card(the_card("D2"));
        let (seen, _) = played(&mut reveals, 0, 40);
        assert_eq!(seen, vec![("New card: \u{2666}2 Showdown".to_string(), true)], "once, with its sound once");
        assert!(reveals.is_empty(), "and it is gone from the queue, not waiting for a quiet minute");
    }

    /// A reveal that a hand took down before anybody can have read it comes back
    /// once more, in full -- and says nothing the second time.
    #[test]
    fn a_reveal_cut_short_comes_back_silent_and_one_that_was_seen_does_not() {
        let mut reveals = Reveals::default();
        reveals.card(the_card("C4"));
        let first = reveals.now(1_000, true).expect("up");
        assert!(first.sound);
        assert_eq!(first.view.age_ms, 0);
        assert!(reveals.now(1_000 + REVEAL_SEEN_MS - 1, true).is_some());
        assert!(reveals.now(1_000 + REVEAL_SEEN_MS - 1, false).is_none(), "a hand began: nothing over a hand");
        let again = reveals.now(20_000, true).expect("it had not been up long enough to be seen");
        assert!(!again.sound, "the second time it is silent");
        assert_eq!(again.view.age_ms, 0, "and shown in full, from its beginning");
        assert!(reveals.now(20_000 + REVEAL_SEEN_MS, false).is_none());
        assert!(reveals.now(40_000, true).is_none(), "up for long enough when the hand began: seen, and gone");
        assert!(reveals.is_empty());
    }

    /// **The cards that wait together are one reveal.** A good game left a run of
    /// eight, six seconds each, beside a summary that names every card anyway.
    #[test]
    fn cards_that_wait_together_are_one_reveal() {
        let mut reveals = Reveals::default();
        for id in ["D4", "D6", "DJ", "C4", "C6"] {
            reveals.card(the_card(id));
        }
        reveals.card(the_card("D6"));
        let shown = reveals.now(0, true).expect("up");
        assert_eq!(shown.view.title, "5 new cards", "the card told twice is one card");
        assert_eq!(
            shown.view.text,
            "\u{2666}4 Two Pair \u{00b7} \u{2666}6 Straight\n\u{2666}J The Ladder \u{00b7} \u{2663}4 Five Down and 1 more",
            "two lines, broken between two names"
        );
        let mut three = Reveals::default();
        for id in ["S2", "S3", "C2"] {
            three.card(the_card(id));
        }
        assert_eq!(
            three.now(0, true).expect("up").view.text,
            "\u{2660}2 Top Half \u{00b7} \u{2660}3 First Win\n\u{2663}2 First Game"
        );
        let mut two = Reveals::default();
        two.card(the_card("S2"));
        two.card(the_card("S3"));
        assert_eq!(two.now(0, true).expect("up").view.text, "\u{2660}2 Top Half \u{00b7} \u{2660}3 First Win", "two names are one line");
        assert_eq!(shown.view.card.map(|c| c.id), Some("DJ"), "the rarest of them is the one pictured");
        assert_eq!(shown.view.stays_ms, REVEAL_MS + 4 * REVEAL_MORE_MS, "and it stays longer than one card does");
        assert!(shown.sound);
        assert!(reveals.now(shown.view.stays_ms - 1, true).is_some());
        assert!(reveals.now(shown.view.stays_ms, true).is_none(), "one reveal, and then nothing");

        // Never for ever, however many.
        let mut many = Reveals::default();
        for c in crate::app::rewards::catalog::CARDS.iter().take(30) {
            many.card(c);
        }
        let shown = many.now(0, true).expect("up");
        assert_eq!(shown.view.title, "30 new cards");
        assert!(shown.view.text.ends_with("and 26 more"));
        assert_eq!(shown.view.stays_ms, REVEAL_MAX_MS);
    }

    /// What is being read does not change under the reader: a card earned while
    /// a reveal is up waits behind it -- with whatever else is earned meanwhile.
    #[test]
    fn a_card_earned_while_a_reveal_is_up_waits_behind_it() {
        let mut reveals = Reveals::default();
        reveals.card(the_card("C2"));
        assert_eq!(reveals.now(0, true).expect("up").view.title, "New card");
        reveals.card(the_card("S2"));
        reveals.card(the_card("S3"));
        let still = reveals.now(1_000, true).expect("still up");
        assert_eq!(still.view.text, "\u{2663}2 First Game", "the reveal that is up is not rewritten");
        assert!(!still.sound);
        let next = reveals.now(REVEAL_MS, true).expect("the two that waited, as one");
        assert_eq!(next.view.title, "2 new cards");
        assert!(next.sound, "a new reveal, so its sound");

        // Words are never folded into cards, and keep their order among them.
        let mut mixed = Reveals::default();
        mixed.words("Level 5", "A White chip.");
        mixed.card(the_card("H2"));
        mixed.words("A short break?", "Two hours at the tables.");
        mixed.card(the_card("C4"));
        let came_up: Vec<(String, bool)> = (0..4)
            .map(|i| mixed.now(i * REVEAL_MS, true).expect("four reveals"))
            .map(|s| (s.view.title, s.sound))
            .collect();
        let expected = [("Level 5", false), ("New card", true), ("A short break?", false), ("New card", true)];
        assert_eq!(came_up.len(), expected.len());
        for ((title, sound), (want_title, want_sound)) in came_up.iter().zip(expected) {
            assert_eq!((title.as_str(), *sound), (want_title, want_sound), "a card sounds; a level and a break do not");
        }
    }

    /// Bounded, and the reveal that is up is never the one dropped.
    #[test]
    fn the_reveals_that_wait_are_bounded() {
        let mut reveals = Reveals::default();
        reveals.words("Level 2", "A White chip.");
        assert!(reveals.now(0, true).is_some());
        for level in 3..40 {
            reveals.words(format!("Level {level}"), "A chip.");
        }
        assert_eq!(reveals.now(1, true).expect("still up").view.title, "Level 2");
        let mut titles = Vec::new();
        let mut now = 1;
        while let Some(shown) = reveals.now(now, true) {
            if titles.last() != Some(&shown.view.title) {
                titles.push(shown.view.title);
            }
            now += 500;
        }
        assert_eq!(titles.len(), REVEALS_KEPT);
        assert_eq!(titles.last().map(String::as_str), Some("Level 39"), "the newest are the ones kept");
    }

    fn ad(players: u8) -> TableAd {
        let mut a = TableAd {
            game: 1,
            mode: Mode::CashPlayMoney.code(),
            preset_id: "CUSTOM".into(),
            table_name: "Riverside".into(),
            small_blind: 10,
            big_blind: 20,
            ante: 0,
            min_buyin: 200,
            max_buyin: 2_000,
            start_stack: 0,
            players,
            max_players: 6,
            min_players_to_start: 2,
            blind_schedule: BlindSchedule {
                mode: 1,
                every_n_hands: 20,
                first_small_blind: 10,
                small_blind_cap: 1_000,
            },
            action_timeout_ms: 20_000,
            action_grace_ms: 5_000,
            crypto_step_timeout_ms: 30_000,
            hand_deadline_ms: 0,
            join_deadline_ms: 120_000,
            hand_delay_ms: 7_000,
            time_bank_ms: 0,
            button_rule: 1,
            odd_chip_rule: 1,
            showdown_policy: 1,
            password_required: false,
            deck_suite: DECK_SUITE_V1.into(),
            founder_app_key: [7u8; 32],
            founder_peer_id: Vec::new(),
            timestamp_unix_ms: NOW,
            expires_at_unix_ms: NOW + 90_000,
            founder_tox_key: None,
            tox_chat_id: None,
        };
        a.hand_deadline_ms = hand_deadline_min_ms(6, 20_000, 5_000, 30_000, 7_000, 0) as u32;
        a
    }

    fn held(players: u8, unjoinable: bool) -> Held {
        Held {
            ad: ad(players),
            params_hash: [0u8; 32],
            advert_hash: [0u8; 32],
            received_at_ms: NOW,
            first_seen_ms: NOW,
            unjoinable,
        }
    }


    /// The search matches the name or the host, case-insensitively — a search
    /// that is case-sensitive is a search that finds nothing.
    #[test]
    fn the_search_matches_the_name_or_the_host() {
        let mut a = row([1u8; 32], &held(2, false));
        a.name = "Riverside".into();
        a.host = "deadbeef".into();
        let mut b = row([2u8; 32], &held(2, false));
        b.name = "The Kitchen".into();
        b.host = "cafebabe".into();
        let rows = vec![a, b];

        assert_eq!(visible(&rows, "", Filter::All).len(), 2);
        assert_eq!(visible(&rows, "river", Filter::All).len(), 1, "lower case");
        assert_eq!(visible(&rows, "  RIVER  ", Filter::All).len(), 1, "trimmed");
        assert_eq!(visible(&rows, "cafe", Filter::All).len(), 1, "by host");
        assert_eq!(visible(&rows, "nothing", Filter::All).len(), 0);
    }

    #[test]
    fn each_filter_keeps_what_it_says() {
        let open = row([1u8; 32], &held(2, false));
        let full = row([2u8; 32], &held(6, false));
        let broken = row([3u8; 32], &held(2, true));
        let rows = vec![open, full, broken];

        assert_eq!(visible(&rows, "", Filter::All).len(), 3);
        assert_eq!(visible(&rows, "", Filter::Open).len(), 1);
        assert_eq!(visible(&rows, "", Filter::Cash).len(), 3, "all three are cash");
        assert_eq!(visible(&rows, "", Filter::SitAndGo).len(), 0);
    }

    /// `D-072`: the row a player clicks has no *cash game* in it. That filter
    /// could now only ever answer with an empty list, and a filter that is
    /// always empty reads as a lobby with nobody in it rather than as a game
    /// this client does not deal. The variant itself stays -- it is how a row's
    /// own words are matched, and a table reached by a resume still has them.
    #[test]
    fn the_filter_row_does_not_offer_a_game_this_client_will_not_deal() {
        assert_eq!(Filter::ALL.len(), 4);
        assert!(!Filter::ALL.contains(&Filter::Cash), "{:?}", Filter::ALL);
        assert!(Filter::ALL.contains(&Filter::SitAndGo));
        // And the variant still says what it always said, to whoever asks it.
        assert_eq!(Filter::Cash.label(), "cash game");
    }

    /// A table that cannot start yet is the one a player can do something about.
    #[test]
    fn waiting_for_players_means_short_of_the_minimum() {
        let mut short = row([1u8; 32], &held(1, false));
        short.needed = 2;
        short.seated = 1;
        let mut ready = row([2u8; 32], &held(3, false));
        ready.needed = 2;
        ready.seated = 3;

        let rows = vec![short, ready];
        let waiting = visible(&rows, "", Filter::WaitingForPlayers);
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].key, [1u8; 32]);
    }

    /// An empty list means three different things and the message says which
    /// -- and each is an invitation, never an apology or a zero.
    #[test]
    fn an_empty_list_says_which_silence_it_is() {
        let hidden = empty_state(3, 0, 5).unwrap();
        assert!(hidden.title.contains("hidden"));
        assert!(hidden.body.contains("Clear the search"));
        let alone = empty_state(0, 0, 0).unwrap();
        assert!(alone.connecting, "not on the network yet: the drawing shows it working");
        assert!(alone.title.starts_with("Connecting"));
        let quiet = empty_state(0, 0, 4).unwrap();
        assert!(quiet.title.contains("Nobody is playing"));
        assert!(quiet.body.contains("Start a search"), "the next thing to do, not the network's state");
        assert!(!quiet.connecting);
        for s in [hidden, alone, quiet] {
            assert!(!s.title.contains('0') && !s.body.contains('0'), "no zero is announced: {s:?}");
            assert!(!s.body.to_lowercase().contains("advertis"), "no protocol word: {s:?}");
        }
        assert!(empty_state(3, 3, 5).is_none(), "a list explains itself");
    }

    /// `D-067`: a table's state is a sentence a player acts on, and the
    /// distance to a start is said in seats.
    #[test]
    fn the_status_says_how_far_a_table_is_from_starting() {
        let mut r = row([1u8; 32], &held(1, false));
        r.needed = 3;
        r.seated = 1;
        assert_eq!(status_words(&r), "waiting for 2 more");
        r.seated = 3;
        assert_eq!(status_words(&r), "3 seats left", "past the minimum, the seats free are what is said");
        r.seated = 5;
        assert_eq!(status_words(&r), "1 seat left");
        assert_eq!(status_words(&row([2u8; 32], &held(6, false))), "full");
        assert_eq!(status_words(&row([3u8; 32], &held(2, true))), "rules changed");
    }

    /// `D-067`: the shape and the badge are a player's words, and the badge
    /// carries what the game means for somebody who has not played it.
    #[test]
    fn the_shape_and_the_badge_are_a_players_words() {
        assert_eq!(shape_words(2), "heads-up");
        assert_eq!(shape_words(6), "6-max");
        assert_eq!(shape_words(10), "full ring");
        assert_eq!(shape_words(4), "4-max");
        let cash = row([1u8; 32], &held(2, false));
        assert!(!cash.sit_and_go);
        assert_eq!(format_badge(&cash).0, "Cash");
        assert!(format_badge(&cash).1.contains("buy-in"));
        let mut h = held(2, false);
        h.ad.mode = Mode::TournamentSngPlayMoney.code();
        let sng = row([2u8; 32], &h);
        assert!(sng.sit_and_go);
        assert_eq!(format_badge(&sng).0, "Sit & Go");
        assert!(format_badge(&sng).1.contains("same stack"));
        assert_eq!(sng.seats, 6);
        assert_eq!(sng.action_s, 20);
    }

    /// `D-067`: closest to starting means open before full, the fullest first,
    /// and every order ends on the name so nothing reshuffles between frames.
    #[test]
    fn the_default_order_puts_the_table_about_to_start_first() {
        let mut nearly = row([1u8; 32], &held(5, false));
        nearly.name = "Zeta".into();
        let mut empty = row([2u8; 32], &held(1, false));
        empty.name = "Alpha".into();
        let full = row([3u8; 32], &held(6, false));
        let broken = row([4u8; 32], &held(5, true));
        let rows = vec![&broken, &empty, &full, &nearly];

        let closest: Vec<&str> = sorted(rows.clone(), Sort::ClosestToStart).iter().map(|r| r.name.as_str()).collect();
        assert_eq!(closest, vec!["Zeta", "Alpha", "Riverside", "Riverside"]);
        let by_state: Vec<TableState> = sorted(rows.clone(), Sort::ClosestToStart).iter().map(|r| r.state.clone()).collect();
        assert_eq!(by_state[2], TableState::Full);
        assert_eq!(by_state[3], TableState::ParametersChanged, "a refused table is last");

        let by_name: Vec<&str> = sorted(rows.clone(), Sort::Name).iter().map(|r| r.name.as_str()).collect();
        assert_eq!(by_name, vec!["Alpha", "Riverside", "Riverside", "Zeta"]);

        let mut big = row([5u8; 32], &held(1, false));
        big.blinds = "50 / 100".into();
        big.name = "Big".into();
        let by_blinds: Vec<&str> = sorted(vec![&empty, &big], Sort::Blinds).iter().map(|r| r.name.as_str()).collect();
        assert_eq!(by_blinds, vec!["Big", "Alpha"], "the biggest blinds first");
    }

    /// `D-067`: the header says what is happening and never announces a zero;
    /// the DHT's peers are not among what it says.
    #[test]
    fn the_header_counts_only_what_is_there() {
        assert!(headline_counts(0, 0, 0, 0).is_empty(), "an empty room announces nothing");
        let some = headline_counts(1, 1, 12, 3);
        let words: Vec<&str> = some.iter().map(|(w, _)| w.as_str()).collect();
        assert_eq!(words, vec!["1 table", "1 open", "12 players online", "3 searching"]);
        let one = headline_counts(2, 0, 1, 0);
        let words: Vec<&str> = one.iter().map(|(w, _)| w.as_str()).collect();
        assert_eq!(words, vec!["2 tables", "1 player online"]);
        assert!(some.iter().all(|(w, _)| !w.starts_with('0')));
    }

    /// `D-067`: the strip's one light. Red for the one real fault, in the same
    /// order as the summary; *Online* otherwise, with nothing technical in it.
    #[test]
    fn the_headline_is_one_friendly_light() {
        let stuck = NetworkStatus { peers: 4, public: Some(false), relay: None, ..Default::default() };
        assert_eq!(stuck.headline().1, Tone::Danger);
        assert!(stuck.summary().contains("no relay"), "the two agree on the worst true thing");
        let alone = NetworkStatus::default();
        assert_eq!(alone.headline(), ("Connecting\u{2026}", Tone::Dim));
        let relayed = NetworkStatus {
            peers: 4,
            public: Some(false),
            relay: Some(RelayStatus { peer: "12D3KooW".into(), adequate: true }),
            ..Default::default()
        };
        assert_eq!(relayed.headline(), ("Online through a relay", Tone::Ok));
        let fine = NetworkStatus { peers: 40, public: Some(true), ..Default::default() };
        assert_eq!(fine.headline(), ("Online", Tone::Ok));
        for s in [&stuck, &alone, &relayed, &fine] {
            let w = s.headline().0.to_lowercase();
            assert!(!w.contains("peer") && !w.contains("dht") && !w.contains("dial"), "{w}");
        }
    }

    /// `D-067`: the card's words. A session is said, not nagged about; a win is
    /// congratulated, a place is stated, and nothing asks for the chips back.
    #[test]
    fn the_cards_words_are_a_persons_and_ask_for_nothing_back() {
        assert_eq!(session_words(59), "0 min");
        assert_eq!(session_words(25 * 60), "25 min");
        assert_eq!(session_words(65 * 60 + 30), "1 h 05 min");
        let won = crate::storage::results::Entry { when_unix_ms: 0, table: "Riverside".into(), seats: 6, place: 1, tied: false };
        assert_eq!(result_words(&won), "You won at Riverside");
        let third = crate::storage::results::Entry { place: 3, ..won.clone() };
        assert_eq!(result_words(&third), "3rd of 6 at Riverside");
        let tied = crate::storage::results::Entry { place: 4, tied: true, ..won.clone() };
        assert_eq!(result_words(&tied), "Tied for 4th of 6 at Riverside");
        for e in [&won, &third, &tied] {
            let w = result_words(e).to_lowercase();
            assert!(!w.contains("back") && !w.contains("again") && !w.contains("lost"), "{w}");
        }
    }

    /// `D-067`: the chips are short and distinct, the praise is for a good
    /// result only, and the note under the button invents no number.
    #[test]
    fn the_chips_the_praise_and_the_note_say_only_what_is_true() {
        let chips: Vec<&str> = Format::ALL.iter().map(|f| format_chip(*f)).collect();
        let mut distinct = chips.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), 4);
        assert!(chips.iter().all(|c| c.len() <= 10));

        let won = crate::storage::results::Entry { when_unix_ms: 0, table: "R".into(), seats: 6, place: 1, tied: false };
        assert_eq!(result_praise(&won), Some("Congratulations!"));
        assert_eq!(result_praise(&crate::storage::results::Entry { place: 3, ..won.clone() }), Some("Well played."));
        assert_eq!(result_praise(&crate::storage::results::Entry { place: 2, seats: 2, ..won.clone() }), None, "second of two is last");
        assert_eq!(result_praise(&crate::storage::results::Entry { place: 5, ..won.clone() }), None);

        let unknown = hero_note(None, Format::Any, 0);
        assert!(unknown.contains("depends on who else is looking"));
        assert!(!unknown.chars().any(|c| c.is_ascii_digit()), "no number before one was measured: {unknown}");
        let known = hero_note(Some(75), Format::SixMax, 1);
        assert!(known.contains("6-max usually take about 01:15"));
        assert!(known.ends_with("1 player is searching right now."));
        assert!(hero_note(Some(75), Format::HeadsUp, 3).ends_with("3 players are searching right now."));
    }

    /// `D-067`: the search window's words say what is known and no more --
    /// the queue's silence is not a zero, a heard zero is not an apology,
    /// and no table held is what the search does about it.
    #[test]
    fn the_search_windows_words_say_only_what_is_known() {
        use crate::net::matchmaker::{ReservationView, SearchReport};
        assert_eq!(eta_words(Some(80), Some(200)), "about 01:20", "the search's own estimate first");
        assert_eq!(eta_words(None, Some(168)), "usually about 02:48");
        assert_eq!(eta_words(None, None), "measuring\u{2026}");

        let report = |queue: u32, known: bool, wait: Option<u64>| SearchReport {
            id: 1,
            format: Format::Any,
            elapsed_s: 5,
            eta_s: None,
            queue,
            queue_known: known,
            queue_wait_s: wait,
            reservations: Vec::new(),
            looking_at: 4,
            running: 0,
            limit: 1,
            warning: None,
            phase: String::new(),
        };
        assert_eq!(queue_words(None), "finding out\u{2026}");
        assert_eq!(queue_words(Some(&report(0, false, None))), "finding out\u{2026}", "silence is not a zero");
        assert_eq!(queue_words(Some(&report(0, true, None))), "nobody else right now");
        assert_eq!(queue_words(Some(&report(1, true, None))), "1 other player");
        assert_eq!(queue_words(Some(&report(3, true, Some(70)))), "3 other players (waiting 01:10 on average)");

        assert!(held_words(0).contains("founds one"));
        assert_eq!(held_words(1), "A seat is held for you at 1 table");
        assert_eq!(held_words(2), "A seat is held for you at 2 tables");

        let table = |name: &str, players: u8, capacity: u8, seats: u8, mine: bool, armed: bool| ReservationView {
            slot: None,
            key: None,
            name: name.into(),
            players,
            capacity,
            seats,
            armed,
            mine,
            note: None,
        };
        let mut r = report(0, true, None);
        assert!(closest_table(Some(&r)).is_none(), "no seat held, no closest table");
        r.reservations = vec![table("Far", 1, 6, 6, false, false), table("Near", 3, 4, 6, true, true)];
        assert_eq!(closest_table(Some(&r)).map(|x| x.name.as_str()), Some("Near"));
        // `S1-IE`: the number said and drawn is what the founder needs now.
        assert_eq!(
            reservation_words(&r.reservations[1]),
            "3 of 4 needed to start \u{00b7} table of 6 \u{00b7} yours"
        );
        assert_eq!(needed_now(&r.reservations[1]), 4);
        assert_eq!(reservation_words(&r.reservations[0]), "1 of 6 seated");
        assert_eq!(needed_now(&r.reservations[0]), 6);
        let theirs = table("Theirs", 2, 5, 10, false, true);
        assert_eq!(
            reservation_words(&theirs),
            "2 of about 5 needed to start \u{00b7} table of 10",
            "another founder's clock is this client's reading of it; consent alone is not said"
        );
        let over = table("Over", 4, 2, 10, true, true);
        assert_eq!(needed_now(&over), 4, "never fewer rings than players");
        assert!(reservation_words(&over).ends_with("yours \u{00b7} starting"), "{}", reservation_words(&over));
        assert_eq!(status_words_needed(2, 5), "waiting for 3 more");
        assert_eq!(status_words_needed(3, 3), "about to start");
        // The row of a held table reads the search's report; any other row nothing.
        let mut view = LobbyView::default();
        view.search = Some(SearchView { id: 1, format: "Automatic", games: 1, elapsed_s: 9, report: Some(r.clone()), typical_s: None });
        assert_eq!(row_needed(&view, &[0u8; 32]), None);
        let mut held = r.clone();
        held.reservations[1].key = Some([7u8; 32]);
        view.search = Some(SearchView { id: 1, format: "Automatic", games: 1, elapsed_s: 9, report: Some(held), typical_s: None });
        assert_eq!(row_needed(&view, &[7u8; 32]), Some(4));
        assert_eq!(row_needed(&view, &[8u8; 32]), None, "an advert does not carry its founder's clock");
        assert!(FOUND_TOAST_MS >= 3_000 && FOUND_TOAST_MS <= 6_000, "long enough to read, short enough to not wait for");
    }

    /// `D-067`: the words about fairness claim what the cryptography gives and
    /// say what it does not; the three steps say what to do first.
    #[test]
    fn the_fair_play_note_is_honest_about_its_limit() {
        assert!(FAIR_PLAY.contains("proves the shuffle"));
        assert!(FAIR_PLAY.contains("no server, no house and no rake"));
        assert!(FAIR_PLAY.contains("What no cryptography stops"), "the honesty clause is on the screen too");
        assert!(!FAIR_PLAY.to_lowercase().contains("impossible"), "nothing claims cheating is impossible");
        assert_eq!(HOW_IT_WORKS.len(), 3);
        assert!(HOW_IT_WORKS[0].0.contains("Find a game"));
    }

    #[test]
    fn a_row_carries_the_columns_the_spec_asks_for() {
        let r = row([1u8; 32], &held(2, false));
        assert_eq!(r.name, "Riverside");
        assert_eq!(r.game, "NLHE cash");
        assert_eq!(r.blinds, "10 / 20");
        assert_eq!(r.occupancy, "2 / 6");
        assert_eq!(r.stack, "200 - 2000", "a cash table shows the buy-in range");
        assert_eq!(r.timing, "20 s + 5 s");
        assert_eq!(r.host, "07070707");
        assert_eq!(r.state, TableState::Open);
        assert_eq!((r.seats, r.sit_and_go, r.action_s), (6, false, 20), "and the numbers the drawing draws");
    }

    /// A tournament pays every entrant the same stack, so the column shows one
    /// number rather than a range that cannot vary.
    #[test]
    fn a_tournament_row_shows_the_stack_and_not_a_range() {
        let mut h = held(2, false);
        h.ad.mode = Mode::TournamentSngPlayMoney.code();
        h.ad.start_stack = 10_000;
        let r = row([1u8; 32], &h);
        assert_eq!(r.game, "NLHE Sit & Go");
        assert_eq!(r.stack, "10000");
    }

    /// The rule the whole lobby exists to enforce at the last moment: a table
    /// whose parameters changed under a live advert is not joinable, and the
    /// button is what stops it.
    #[test]
    fn a_table_whose_rules_changed_cannot_be_joined() {
        let r = row([1u8; 32], &held(2, true));
        assert_eq!(r.state, TableState::ParametersChanged);
        assert!(!r.state.joinable());
        assert!(
            r.state.why_not().unwrap().contains("two different rule sets"),
            "and the player is told why, or they conclude the client is broken"
        );
    }

    #[test]
    fn a_full_table_is_shown_and_not_joinable() {
        let r = row([1u8; 32], &held(6, false));
        assert_eq!(r.state, TableState::Full);
        assert!(!r.state.joinable());
        assert!(r.state.why_not().is_some());
    }

    /// Every refusal says why. A join button that is simply grey is
    /// indistinguishable from a broken client.
    #[test]
    fn every_refusal_has_a_reason() {
        for s in [TableState::Full, TableState::ParametersChanged] {
            assert!(s.why_not().is_some(), "{s:?} refuses without saying why");
        }
        assert!(TableState::Open.why_not().is_none());
    }

    /// The list does not reshuffle under the cursor when an advert is
    /// re-broadcast, which happens every thirty seconds per table.
    #[test]
    fn the_list_order_does_not_depend_on_arrival() {
        let mut a = LobbyStore::new();
        let mut b = LobbyStore::new();

        let mut zebra = ad(1);
        zebra.table_name = "Zebra".into();
        let mut alpha = ad(1);
        alpha.table_name = "Alpha".into();

        a.offer([1u8; 32], zebra.clone(), [0u8; 32], [0u8; 32], NOW).unwrap();
        a.offer([2u8; 32], alpha.clone(), [0u8; 32], [0u8; 32], NOW).unwrap();
        b.offer([2u8; 32], alpha, [0u8; 32], [0u8; 32], NOW).unwrap();
        b.offer([1u8; 32], zebra, [0u8; 32], [0u8; 32], NOW).unwrap();

        let ra = rows(&a);
        let rb = rows(&b);
        assert_eq!(ra, rb);
        assert_eq!(ra[0].name, "Alpha");
    }

    /// Adverts expire, so a selection can name a table that has gone. A pane
    /// that assumed otherwise would index into a shorter list.
    #[test]
    fn a_selection_that_expired_is_simply_gone() {
        let mut store = LobbyStore::new();
        store.offer([1u8; 32], ad(1), [0u8; 32], [0u8; 32], NOW).unwrap();
        let mut view = LobbyView::from(&store, NetworkStatus::default());

        view.selected = Some([1u8; 32]);
        assert!(view.selected_row().is_some());
        assert!(view.can_join());

        view.selected = Some([9u8; 32]);
        assert!(view.selected_row().is_none());
        assert!(!view.can_join(), "and the button is not live for a ghost");
    }

    /// The identity is the key. Two tables may carry one name, and a name is
    /// display data the founder chooses.
    #[test]
    fn the_label_is_the_key_and_not_the_name() {
        let a = short_key(&[0xDE, 0xAD, 0xBE, 0xEF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(a, "deadbeef");
        assert_ne!(a, short_key(&[1u8; 32]));
    }

    /// The status line says the worst true thing first. "Connected to 4 peers"
    /// while unreachable and relayless would be the most misleading text on the
    /// screen.
    #[test]
    fn the_status_line_leads_with_the_worst_true_thing() {
        let stuck = NetworkStatus {
            peers: 4,
            public: Some(false),
            relay: None,
            ..Default::default()
        };
        assert!(stuck.summary().contains("no relay"));

        let bad_relay = NetworkStatus {
            peers: 4,
            public: Some(false),
            relay: Some(RelayStatus {
                peer: "12D3KooW".into(),
                adequate: false,
            }),
            ..Default::default()
        };
        // The summary reports connectivity, not a table's budget: a relay
        // with the library's limits is still a relay, and a player reading the
        // lobby is not yet in a hand it could fail to carry.
        assert!(!bad_relay.summary().contains("cannot carry a hand"));
        assert!(bad_relay.summary().contains("through a relay"));

        let fine = NetworkStatus {
            peers: 4,
            public: Some(true),
            relay: None,
            ..Default::default()
        };
        assert_eq!(fine.summary(), "Connected to 4 peers");

        let alone = NetworkStatus::default();
        assert_eq!(alone.summary(), "Looking for peers");
    }

    /// §4.3 requires the interface to say what a table password does not
    /// protect. A warning that is not rendered is not a warning, so it is a
    /// constant with a test rather than a comment.
    #[test]
    fn the_password_warning_says_what_it_must() {
        assert!(PASSWORD_WARNING.contains("does not keep the table private"));
        assert!(PASSWORD_WARNING.contains("offline"));
    }

    /// The founder does not type this number. Rule 2a's minimum is a function of
    /// the table's own shape, and a smaller one advertises a table where every
    /// hand aborts with nobody named.
    #[test]
    fn the_suggested_deadline_is_derived_and_admissible() {
        for seats in [2u8, 6, 10] {
            let d = suggested_hand_deadline(seats, 20_000, 5_000, 30_000, 7_000, 0);
            assert_eq!(
                d as u64,
                hand_deadline_min_ms(seats, 20_000, 5_000, 30_000, 7_000, 0)
            );
        }
        assert!(
            suggested_hand_deadline(10, 20_000, 5_000, 30_000, 7_000, 0)
                > suggested_hand_deadline(2, 20_000, 5_000, 30_000, 7_000, 0),
            "more seats need more time, which is why it is derived"
        );
    }

    /// A display name is untrusted. An empty one would render as a blank row
    /// nobody can click.
    #[test]
    fn a_blank_display_name_becomes_something_clickable() {
        use crate::table::formation::SeatEntry;
        let roster = Roster::form(
            vec![
                SeatEntry {
                    seat: 0,
                    app_public_key: [1u8; 32],
                    peer_id: vec![1u8; 8],
                    display_name: "   ".into(),
                    buyin: 500,
                    tox_key: None,
                },
                SeatEntry {
                    seat: 1,
                    app_public_key: [2u8; 32],
                    peer_id: vec![2u8; 8],
                    display_name: "Alice".into(),
                    buyin: 500,
                    tox_key: None,
                },
            ],
            &ad(2),
            false,
        )
        .unwrap();

        assert_eq!(player_names(&roster), vec!["seat 0", "Alice"]);
    }

    /// **`D-077`: only an answer that was read and understood closes the
    /// lobby.** The breaks this must catch: a failed check read as *a newer one
    /// is out* -- every player locked out whenever GitHub has a bad hour -- and
    /// the button's own question, or no question at all, shutting the lobby.
    #[test]
    fn only_a_newer_release_closes_the_lobby_and_only_the_opening_question_holds_it() {
        use super::super::render::UpdateUi;
        use crate::app::update::Verdict;
        assert_eq!(gate(&UpdateUi::Opening), Gate::Asking, "the question the window asks as it opens is waited for");
        assert_eq!(
            gate(&UpdateUi::Done(Ok(Verdict::Newer { latest: "0.2.0".into(), tag: "v0.2.0".into() }))),
            Gate::Closed { latest: "0.2.0".into(), tag: "v0.2.0".into() }
        );
        for open in [
            UpdateUi::Idle,
            UpdateUi::Checking,
            UpdateUi::Done(Ok(Verdict::Newest { latest: "0.1.1".into() })),
            UpdateUi::Done(Ok(Verdict::NoRelease)),
            UpdateUi::Done(Err("GitHub could not be reached: timed out".into())),
            UpdateUi::Done(Err("GitHub is not answering this address right now".into())),
        ] {
            assert_eq!(gate(&open), Gate::Open, "{open:?}");
        }
    }

    /// **`D-077`: with the gate shut, the window's own buttons and nothing
    /// else.** A row clicked, a table joined, a search started or a line said
    /// in the frame the window covered the lobby does not reach the client.
    #[test]
    fn a_shut_lobby_passes_nothing_but_the_gates_own_buttons() {
        use super::super::render::LobbyAction;
        let shut = Gate::Closed { latest: "0.2.0".into(), tag: "v0.2.0".into() };
        for tried in [
            LobbyAction::Sit { key: [1u8; 32], buyin: 1_000, password: None },
            LobbyAction::Resume,
            LobbyAction::Say("hello".into()),
            LobbyAction::Select([1u8; 32]),
            LobbyAction::CheckForUpdate,
        ] {
            for gate in [&shut, &Gate::Asking] {
                assert_eq!(through_the_gate(gate, tried.clone(), None), LobbyAction::None, "{gate:?} let {tried:?} through");
            }
            assert_eq!(through_the_gate(&Gate::Open, tried.clone(), None), tried, "an open lobby passes it");
        }
        // The gate's own button beats whatever else the frame held.
        assert_eq!(
            through_the_gate(&shut, LobbyAction::Resume, Some(LobbyAction::DownloadUpdate)),
            LobbyAction::DownloadUpdate
        );
    }

    /// **`D-077`: the next step is said for where this copy lives**, because the
    /// wrong one makes a new player: an installed copy is updated by the new
    /// file's installer, a portable one by putting the new program in the old
    /// one's place. The folder is a field, never inside the sentence.
    #[test]
    fn the_next_step_is_said_for_where_this_copy_lives() {
        use super::super::render::HomeView;
        use std::path::PathBuf;
        let folder = PathBuf::from(r"C:\Users\someone\AppData\Local\Programs\P2Poker");
        let home = |installed| HomeView {
            folder: folder.clone(),
            profile: folder.join("profile"),
            installed,
            can_install: true,
            busy: false,
        };
        let installed = closed_words("0.2.0", "0.1.1", Some(&home(true)), System::Windows);
        assert_eq!(installed.heading, "P2Poker 0.2.0 is out");
        assert!(installed.body.contains("0.1.1") && installed.body.contains("0.2.0"), "{}", installed.body);
        assert!(installed.body.contains("protocol"), "why it is required: {}", installed.body);
        assert_eq!(installed.download, "Download 0.2.0");
        assert!(installed.next.contains("update the installed copy"), "{}", installed.next);
        assert!(installed.next.contains("profile stays"), "an update must not read as a new player");
        assert!(!installed.by_hand);

        let portable = closed_words("0.2.0", "0.1.1", Some(&home(false)), System::Windows);
        assert!(portable.next.contains("in place of this one"), "{}", portable.next);
        assert!(portable.next.contains("new player"), "and what the other way costs: {}", portable.next);
        assert!(portable.by_hand);

        for w in [&installed, &portable] {
            assert_eq!(w.folder.as_deref(), Some(home(true).folder.as_path()));
            assert!(![&w.heading, &w.body, &w.next].iter().any(|s| s.contains(r"C:\")), "a path inside a sentence");
        }
        let nowhere = closed_words("0.2.0", "0.1.1", None, System::Windows);
        assert_eq!(nowhere.folder, None);
        assert!(nowhere.next.contains("profile stays"));
        assert_ne!(handed_words(true, System::Windows), handed_words(false, System::Windows));
    }

    /// **`D-078`: on Linux the next step is the package, or the portable
    /// folder.** A package keeps the profile in the user's data folder, so the
    /// new version is installed as this one was and no folder is shown; a copy
    /// whose profile stands beside it is replaced in place, as on Windows. The
    /// words never name the Windows program, and the button opens a page.
    #[test]
    fn on_linux_the_next_step_is_the_package_or_the_portable_folder() {
        use super::super::render::HomeView;
        use std::path::PathBuf;
        let packaged = HomeView {
            folder: PathBuf::from("/usr/lib/p2poker"),
            profile: PathBuf::from("/home/someone/.local/share/p2poker/profile"),
            installed: false,
            can_install: false,
            busy: false,
        };
        let words = closed_words("0.2.0", "0.1.1", Some(&packaged), System::Linux);
        assert!(words.next.contains("AppImage") && words.next.contains(".deb") && words.next.contains(".rpm"), "{}", words.next);
        assert!(words.next.contains("profile stays"), "{}", words.next);
        assert_eq!(words.folder, None, "nothing to replace by hand");
        assert!(!words.by_hand);
        assert_eq!(closed_words("0.2.0", "0.1.1", None, System::Linux).next, words.next, "no home known: the package");

        let portable = HomeView { folder: PathBuf::from("/home/someone/p2poker"), profile: PathBuf::from("/home/someone/p2poker/profile"), ..packaged.clone() };
        let words = closed_words("0.2.0", "0.1.1", Some(&portable), System::Linux);
        assert!(words.next.contains("in place of this one"), "{}", words.next);
        assert_eq!(words.folder.as_deref(), Some(portable.folder.as_path()));
        assert!(words.by_hand);

        for w in [closed_words("0.2.0", "0.1.1", Some(&packaged), System::Linux), words] {
            assert!(![&w.heading, &w.body, &w.next].iter().any(|s| s.contains(".exe")), "no Windows program on Linux");
        }
        assert!(handed_words(true, System::Linux).contains("release page"));
        assert!(!handed_words(true, System::Windows).contains("release page"));
        let about = super::super::render::home_words(&packaged, System::Linux);
        assert!(about.contains("/usr/lib/p2poker") && about.contains(".local/share/p2poker/profile"), "{about}");
        let beside = super::super::render::home_words(&portable, System::Linux);
        assert!(beside.contains("portable"), "{beside}");
    }
}
