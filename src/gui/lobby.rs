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
            TableState::ParametersChanged => "parameters changed",
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

    TableRow {
        key,
        name: ad.table_name.clone(),
        game,
        blinds: format!("{} / {}", ad.small_blind, ad.big_blind),
        occupancy: format!("{} / {}", ad.players, ad.max_players),
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
    pub peers: usize,
    pub listening: Vec<String>,
    pub dht_announced: bool,
    pub relay: Option<RelayStatus>,
    pub public: Option<bool>,
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
    pub fn summary(&self) -> String {
        if let Some(r) = &self.relay {
            if !r.adequate {
                return "Relay found, but it cannot carry a hand — see the note".into();
            }
        }
        match (self.public, self.relay.is_some(), self.peers) {
            (Some(false), false, _) => {
                "Behind NAT and no relay found — tables may not be joinable".into()
            }
            (_, _, 0) => "Looking for peers".into(),
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
    pub tables: Vec<TableRow>,
    pub status: NetworkStatus,
    pub selected: Option<[u8; 32]>,
    pub chat: Vec<ChatLine>,
    pub seated: Vec<String>,
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
            tables: rows(store),
            status,
            selected: None,
            chat: Vec::new(),
            seated: Vec::new(),
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
) -> u32 {
    crate::protocol::constants::hand_deadline_min_ms(
        seats,
        action_timeout_ms as u64,
        action_grace_ms as u64,
        crypto_step_timeout_ms as u64,
        hand_delay_ms as u64,
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
        a.hand_deadline_ms = hand_deadline_min_ms(6, 20_000, 5_000, 30_000, 7_000) as u32;
        a
    }

    fn held(players: u8, unjoinable: bool) -> Held {
        Held {
            ad: ad(players),
            params_hash: [0u8; 32],
            received_at_ms: NOW,
            unjoinable,
        }
    }

    #[test]
    fn a_row_carries_the_columns_the_spec_asks_for() {
        let r = row([1u8; 32], &held(2, false));
        assert_eq!(r.name, "Riverside");
        assert_eq!(r.game, "NLHE cash");
        assert_eq!(r.blinds, "10 / 20");
        assert_eq!(r.occupancy, "2 / 6");
        assert_eq!(r.state, TableState::Open);
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

        a.offer([1u8; 32], zebra.clone(), [0u8; 32], NOW).unwrap();
        a.offer([2u8; 32], alpha.clone(), [0u8; 32], NOW).unwrap();
        b.offer([2u8; 32], alpha, [0u8; 32], NOW).unwrap();
        b.offer([1u8; 32], zebra, [0u8; 32], NOW).unwrap();

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
        store.offer([1u8; 32], ad(1), [0u8; 32], NOW).unwrap();
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
        assert!(bad_relay.summary().contains("cannot carry a hand"));

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
            let d = suggested_hand_deadline(seats, 20_000, 5_000, 30_000, 7_000);
            assert_eq!(
                d as u64,
                hand_deadline_min_ms(seats, 20_000, 5_000, 30_000, 7_000)
            );
        }
        assert!(
            suggested_hand_deadline(10, 20_000, 5_000, 30_000, 7_000)
                > suggested_hand_deadline(2, 20_000, 5_000, 30_000, 7_000),
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
