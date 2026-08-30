//! The lobby pane: available tables, their state, and joining one.
//!
//! `SPEC_CS.md` §22 asks for PokerTH's shape — a table list with columns for the
//! game, the blinds, the occupancy and the state, a player list, chat, a join
//! button and a create button.
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
    }
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
    pub const ALL: [Filter; 5] = [
        Filter::All,
        Filter::Open,
        Filter::SitAndGo,
        Filter::Cash,
        Filter::WaitingForPlayers,
    ];

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

/// What to say when the list is empty, which is two different things.
///
/// *"This lobby is quiet"* and *"your filter hides everything"* look identical
/// on screen and need opposite reactions, so the message says which. The Python
/// client makes the same distinction and for the same reason.
pub fn empty_explanation(total: usize, shown: usize, peers: usize) -> Option<&'static str> {
    if shown > 0 {
        return None;
    }
    Some(if total > 0 {
        "Every table is hidden by the search or the filter. Clear them to see the rest."
    } else if peers == 0 {
        "No peers yet. This client is still looking for others; nothing can be \
         advertised to it until it finds some."
    } else {
        "Connected, and nobody is advertising a table. Create one and it will \
         appear in the other clients' lobbies."
    })
}

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
    pub public: Option<bool>,
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
    /// What has happened, newest last. A local view and never canonical state.
    pub log: Vec<String>,
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
            me: String::new(),
            tables: rows(store),
            status,
            selected: None,
            chat: Vec::new(),
            seated: Vec::new(),
            log: Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::lobby::{BlindSchedule, TableAd, DECK_SUITE_V1};
    use crate::protocol::constants::hand_deadline_min_ms;

    const NOW: u64 = 1_700_000_000_000;

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

    /// An empty list means two opposite things and the message says which.
    #[test]
    fn an_empty_list_says_which_silence_it_is() {
        assert!(empty_explanation(3, 0, 5).unwrap().contains("hidden by the search"));
        assert!(empty_explanation(0, 0, 0).unwrap().contains("No peers yet"));
        assert!(empty_explanation(0, 0, 4).unwrap().contains("nobody is advertising"));
        assert!(empty_explanation(3, 3, 5).is_none(), "a list explains itself");
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
                },
                SeatEntry {
                    seat: 1,
                    app_public_key: [2u8; 32],
                    peer_id: vec![2u8; 8],
                    display_name: "Alice".into(),
                    buyin: 500,
                },
            ],
            &ad(2),
            false,
        )
        .unwrap();

        assert_eq!(player_names(&roster), vec!["seat 0", "Alice"]);
    }
}
