//! Table formation: sitting down.
//!
//! `PROTOCOL.md` §4.3. A table is visible once the lobby carries its advert;
//! this is what turns a visible table into a seated one.
//!
//! # Roster uniqueness is on three keys, and the third was carried and never read
//!
//! `seat`, `app_public_key` **and `peer_id`**. Every `SeatEntry` carried the
//! third from the beginning and no rule ever validated it — which is a defect
//! class rather than an omission, because a field nothing checks is a field
//! every reader assumes somebody else checks. It closes `U17` (D-018).
//!
//! **What it buys, exactly.** One node cannot hold **two seats at one table**:
//! `peer_id` must equal the connection's authenticated remote `PeerId`, so a
//! second seat from the same node is refused at the founder and again at every
//! receiver of that table's `PLAYER_LIST`. That covers the accidental case — two
//! copies of the client on one machine, one person joining a table twice — and
//! costs a determined attacker one more process.
//!
//! **The check is per roster, so multi-tabling is unaffected and intended.** Each
//! table checks its own roster and no other; one client may sit at as many
//! different tables as it likes. Reading the rule as *one node, one table* would
//! forbid the ordinary way people play, so the distinction is drawn where it
//! belongs and is pinned by a test.
//!
//! **What it does not buy.** *One person per seat.* Somebody with two machines
//! presents two `PeerId`s and two application keys and is indistinguishable from
//! two people. That is Sybil without an identity layer, `SPEC_CS.md` §18 places
//! it outside the threat model, and no roster rule reaches it. *One key per seat*
//! and *one node per seat* are enforceable and enforced here; *one person per
//! seat* is neither claimed nor achievable.
//!
//! # The starting stack, and why it had no definition
//!
//! `roster_hash(0)` needs `stack_at_hand_start[s]`, and that was defined only as
//! the previous hand's final stacks — of which, for the first hand, there is no
//! previous hand. The value feeds `session_id` and therefore every later
//! `GENESIS(k)`, so two implementers guessing differently would have diverged on
//! every event of every hand with nothing to attribute it to.
//!
//! It is `SeatEntry.buyin`, and D-018 closed it in `PROTOCOL.md` §3.1. Which is
//! why [`SeatEntry::admissible`] exists: an unconstrained buy-in in the roster
//! is an unconstrained starting stack inside the genesis of the whole table.

use crate::net::lobby::{Mode, TableAd};
use crate::poker::state::Hash;
use crate::protocol::constants::MAX_SEATS;
use crate::protocol::serialization::h;
use crate::protocol::signatures::Domain;
use crate::protocol::transcript::{roster_hash, RosterSeat};

/// One seat of a roster, as `PROTOCOL.md` §4.3 defines it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatEntry {
    pub seat: u8,
    pub app_public_key: [u8; 32],
    /// The joiner's authenticated `PeerId`, at most 42 bytes.
    ///
    /// Checked for uniqueness across the roster, which is `U17`.
    pub peer_id: Vec<u8>,
    /// Display only, and untrusted forever. **Never an identifier.**
    pub display_name: String,
    /// The buy-in, which **is** this seat's `stack_at_hand_start` at hand 0.
    pub buyin: u64,
}

/// Why a seat entry is not admissible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatRejected {
    SeatOutOfRange { seat: u8 },
    /// Below the advert's `min_buyin` or above its `max_buyin`.
    BuyinOutOfRange { buyin: u64 },
    /// A tournament pays every entrant the same stack, so the buy-in **is** the
    /// stack and there is exactly one admissible value.
    TournamentBuyinIsTheStack { buyin: u64, start_stack: u64 },
    PeerIdTooLong { len: usize },
    DisplayNameTooLong { len: usize },
    DisplayNameHasControlCharacters,
}

impl SeatEntry {
    /// Whether this entry may enter a roster formed under `ad`.
    ///
    /// The buy-in check is not a courtesy to the player: this value is the
    /// seat's starting stack in `roster_hash(0)`, and therefore in the genesis
    /// of every hand the table will ever play.
    pub fn admissible(&self, ad: &TableAd) -> Result<(), SeatRejected> {
        if self.seat >= ad.max_players {
            return Err(SeatRejected::SeatOutOfRange { seat: self.seat });
        }
        if self.peer_id.len() > 42 {
            return Err(SeatRejected::PeerIdTooLong {
                len: self.peer_id.len(),
            });
        }
        if self.display_name.len() > 32 {
            return Err(SeatRejected::DisplayNameTooLong {
                len: self.display_name.len(),
            });
        }
        if self.display_name.chars().any(|c| c.is_control()) {
            return Err(SeatRejected::DisplayNameHasControlCharacters);
        }

        match Mode::parse(ad.mode) {
            Some(m) if m.is_tournament() => {
                if self.buyin != ad.start_stack {
                    return Err(SeatRejected::TournamentBuyinIsTheStack {
                        buyin: self.buyin,
                        start_stack: ad.start_stack,
                    });
                }
            }
            _ => {
                if self.buyin < ad.min_buyin || self.buyin > ad.max_buyin {
                    return Err(SeatRejected::BuyinOutOfRange { buyin: self.buyin });
                }
            }
        }
        Ok(())
    }

    fn as_roster_seat(&self) -> RosterSeat {
        RosterSeat {
            seat: self.seat,
            app_public_key: self.app_public_key,
            stack_at_hand_start: self.buyin,
        }
    }
}

/// Why a roster is not well formed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RosterRejected {
    /// A seat entry is not admissible on its own terms.
    Seat(SeatRejected),
    /// Not in ascending seat order.
    ///
    /// A hard check and not a `debug_assert`: a roster arrives from the network,
    /// and `roster_hash` is order-dependent, so an unsorted one is two peers
    /// hashing two different values from the same members.
    NotSorted,
    /// Two entries share a seat.
    DuplicateSeat { seat: u8 },
    /// Two entries share an application key.
    ///
    /// One player holding two seats already holds both decryption shares, which
    /// quietly makes the threshold `n - 1`.
    DuplicateKey { first: usize, second: usize },
    /// Two entries share a `PeerId` — `U17`.
    DuplicatePeer { first: usize, second: usize },
    /// More entries than the table has seats.
    TooManySeats { n: usize },
    /// Fewer than the table needs to start.
    TooFewSeats { n: usize, need: u8 },
}

/// A roster, checked.
///
/// The only way to one is [`Roster::form`], so a value of this type has been
/// through every rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roster {
    seats: Vec<SeatEntry>,
}

impl Roster {
    /// Check a roster against the table it is being formed for.
    ///
    /// `require_minimum` is false while formation is still open — a partial
    /// roster in a `JOIN_ACCEPT`'s `roster_so_far` is not yet a table — and true
    /// at `TABLE_READY`, where it is.
    pub fn form(
        seats: Vec<SeatEntry>,
        ad: &TableAd,
        require_minimum: bool,
    ) -> Result<Self, RosterRejected> {
        if seats.len() > ad.max_players as usize || seats.len() > MAX_SEATS as usize {
            return Err(RosterRejected::TooManySeats { n: seats.len() });
        }
        if require_minimum && seats.len() < ad.min_players_to_start as usize {
            return Err(RosterRejected::TooFewSeats {
                n: seats.len(),
                need: ad.min_players_to_start,
            });
        }

        for e in &seats {
            e.admissible(ad).map_err(RosterRejected::Seat)?;
        }

        for w in seats.windows(2) {
            if w[0].seat == w[1].seat {
                return Err(RosterRejected::DuplicateSeat { seat: w[0].seat });
            }
            if w[0].seat > w[1].seat {
                return Err(RosterRejected::NotSorted);
            }
        }

        // The other two uniqueness keys. Quadratic over at most ten entries.
        for i in 0..seats.len() {
            for j in (i + 1)..seats.len() {
                if seats[i].app_public_key == seats[j].app_public_key {
                    return Err(RosterRejected::DuplicateKey {
                        first: i,
                        second: j,
                    });
                }
                if seats[i].peer_id == seats[j].peer_id {
                    return Err(RosterRejected::DuplicatePeer {
                        first: i,
                        second: j,
                    });
                }
            }
        }

        Ok(Roster { seats })
    }

    pub fn seats(&self) -> &[SeatEntry] {
        &self.seats
    }

    pub fn len(&self) -> usize {
        self.seats.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seats.is_empty()
    }

    pub fn seat_of(&self, key: &[u8; 32]) -> Option<u8> {
        self.seats
            .iter()
            .find(|e| &e.app_public_key == key)
            .map(|e| e.seat)
    }

    /// `roster_hash(0)` over this roster.
    ///
    /// The starting stack of each seat is its buy-in, which is D-018's closure of
    /// `U1` and is what makes this value computable at all for the first hand.
    pub fn hash_at_zero(&self) -> Hash {
        let seats: Vec<RosterSeat> = self.seats.iter().map(SeatEntry::as_roster_seat).collect();
        roster_hash(&seats)
    }

    /// The lowest free seat, for a joiner that asked for "any".
    ///
    /// Lowest rather than random, because `U7` leaves the founder's rule
    /// undefined and a deterministic one is the only kind a joiner can check.
    /// **Recorded as a choice**: nothing in the corpus requires it, and a founder
    /// using another rule is not violating anything written down.
    pub fn lowest_free_seat(&self, max_players: u8) -> Option<u8> {
        (0..max_players).find(|s| !self.seats.iter().any(|e| e.seat == *s))
    }
}

/// `password_proof` of `PROTOCOL.md` §4.3.
///
/// A **possession proof, not a secret transfer**, and per-join so it does not
/// replay to another table.
///
/// It is **not** a password strength mechanism. A weak table password is
/// guessable offline by anyone who sees one proof, and §4.3 requires the user
/// interface to say so — which is a product obligation, not a cosmetic one.
///
/// # The undefined part, marked as undefined
///
/// `password_utf8` appears exactly once in the whole corpus, with no encoding,
/// length bound, normalisation form, or trim and case rule (`U5`, D-018). Two
/// clients that normalise differently produce different proofs, and the join
/// fails as *bad password* — the one diagnosis that sends a user to look at their
/// keyboard rather than at the protocol.
///
/// This function takes the bytes it is given and hashes them. **It does not
/// normalise**, because inventing a normalisation here would make this
/// implementation's choice the de facto standard without anybody deciding it.
pub fn password_proof(password_utf8: &[u8], table_id: &Hash, join_nonce: &Hash) -> Hash {
    h(
        Domain::Session.context(),
        &[password_utf8, table_id, join_nonce],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::lobby::{BlindSchedule, DECK_SUITE_V1};
    use crate::protocol::constants::hand_deadline_min_ms;

    fn cash_table() -> TableAd {
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
            players: 0,
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
            founder_peer_id: b"12D3KooWfounder".to_vec(),
            timestamp_unix_ms: 1_700_000_000_000,
            expires_at_unix_ms: 1_700_000_090_000,
        };
        a.hand_deadline_ms = hand_deadline_min_ms(
            a.max_players,
            a.action_timeout_ms as u64,
            a.action_grace_ms as u64,
            a.crypto_step_timeout_ms as u64,
            a.hand_delay_ms as u64,
        0,
        ) as u32;
        a
    }

    fn tournament_table() -> TableAd {
        let mut a = cash_table();
        a.mode = Mode::TournamentSngPlayMoney.code();
        a.start_stack = 10_000;
        a.min_buyin = 10_000;
        a.max_buyin = 10_000;
        a
    }

    fn entry(seat: u8, key: u8, peer: u8, buyin: u64) -> SeatEntry {
        SeatEntry {
            seat,
            app_public_key: [key; 32],
            peer_id: vec![peer; 12],
            display_name: format!("hrac {seat}"),
            buyin,
        }
    }

    #[test]
    fn an_ordinary_roster_forms() {
        let ad = cash_table();
        let r = Roster::form(
            vec![entry(0, 1, 1, 500), entry(2, 2, 2, 1_000)],
            &ad,
            true,
        )
        .expect("two distinct players at two distinct seats");
        assert_eq!(r.len(), 2);
        assert_eq!(r.seat_of(&[2u8; 32]), Some(2));
        assert_eq!(r.seat_of(&[9u8; 32]), None);
    }

    /// `U17`, and the owner's own remark. The field was carried from the
    /// beginning and no rule ever read it.
    #[test]
    fn one_node_cannot_hold_two_seats() {
        let ad = cash_table();
        let mut second = entry(1, 2, 1, 500); // different key, SAME peer
        second.display_name = "ja znovu".into();

        assert_eq!(
            Roster::form(vec![entry(0, 1, 1, 500), second], &ad, true),
            Err(RosterRejected::DuplicatePeer {
                first: 0,
                second: 1
            }),
            "two seats from one node is what the peer_id check exists to refuse"
        );
    }

    /// Multi-tabling: the check is per roster, so one node at two **different**
    /// tables is two legal rosters. Reading the rule as one node per client
    /// would forbid the ordinary way people play.
    #[test]
    fn one_node_may_sit_at_two_different_tables() {
        let ad = cash_table();
        let me = entry(0, 1, 1, 500);

        // Table A: this node and somebody else.
        assert!(Roster::form(vec![me.clone(), entry(1, 2, 2, 500)], &ad, true).is_ok());

        // Table B, a different table with its own roster: the same node again,
        // at whatever seat that table gave it.
        let mut elsewhere = me.clone();
        elsewhere.seat = 3;
        assert!(
            Roster::form(vec![entry(0, 5, 5, 500), elsewhere], &ad, true).is_ok(),
            "one client, two tables, and each roster is checked on its own"
        );
    }

    /// And what it does **not** stop, asserted so the claim cannot drift upward.
    /// Two machines are two PeerIds and two keys, and are indistinguishable from
    /// two people. That is Sybil, and SPEC_CS.md 18 puts it outside the model.
    #[test]
    fn two_machines_are_two_players_as_far_as_this_can_tell() {
        let ad = cash_table();
        let r = Roster::form(
            vec![entry(0, 1, 1, 500), entry(1, 2, 2, 500)],
            &ad,
            true,
        );
        assert!(
            r.is_ok(),
            "one person with two machines forms a legal roster, and no rule here \
             can see the difference"
        );
    }

    /// One player holding two seats already holds both decryption shares, which
    /// quietly makes the threshold `n - 1`.
    #[test]
    fn one_key_cannot_hold_two_seats() {
        let ad = cash_table();
        assert_eq!(
            Roster::form(
                vec![entry(0, 1, 1, 500), entry(1, 1, 2, 500)],
                &ad,
                true
            ),
            Err(RosterRejected::DuplicateKey {
                first: 0,
                second: 1
            })
        );
    }

    /// A hard check and not a `debug_assert`. A roster arrives from the network
    /// and `roster_hash` is order-dependent, so an unsorted one is two peers
    /// hashing two different values from the same members.
    #[test]
    fn an_unsorted_roster_is_refused_rather_than_hashed() {
        let ad = cash_table();
        assert_eq!(
            Roster::form(
                vec![entry(3, 1, 1, 500), entry(1, 2, 2, 500)],
                &ad,
                true
            ),
            Err(RosterRejected::NotSorted)
        );
    }

    #[test]
    fn two_entries_cannot_share_a_seat() {
        let ad = cash_table();
        assert_eq!(
            Roster::form(
                vec![entry(1, 1, 1, 500), entry(1, 2, 2, 500)],
                &ad,
                true
            ),
            Err(RosterRejected::DuplicateSeat { seat: 1 })
        );
    }

    /// The buy-in is the seat's starting stack in `roster_hash(0)` and therefore
    /// in the genesis of every hand this table will play. An unchecked one is an
    /// unchecked genesis.
    #[test]
    fn a_buyin_outside_the_adverts_range_is_refused() {
        let ad = cash_table();
        for bad in [199u64, 2_001] {
            assert_eq!(
                Roster::form(vec![entry(0, 1, 1, bad)], &ad, false),
                Err(RosterRejected::Seat(SeatRejected::BuyinOutOfRange {
                    buyin: bad
                }))
            );
        }
        assert!(Roster::form(vec![entry(0, 1, 1, 200)], &ad, false).is_ok());
        assert!(Roster::form(vec![entry(0, 1, 1, 2_000)], &ad, false).is_ok());
    }

    /// In a tournament every entrant receives the same stack, so there is exactly
    /// one admissible value and no room for a founder to vary it per seat.
    #[test]
    fn a_tournament_buyin_is_the_start_stack_and_nothing_else() {
        let ad = tournament_table();
        assert!(Roster::form(vec![entry(0, 1, 1, 10_000)], &ad, false).is_ok());
        assert_eq!(
            Roster::form(vec![entry(0, 1, 1, 9_999)], &ad, false),
            Err(RosterRejected::Seat(
                SeatRejected::TournamentBuyinIsTheStack {
                    buyin: 9_999,
                    start_stack: 10_000
                }
            ))
        );
    }

    #[test]
    fn a_seat_outside_the_table_is_refused() {
        let ad = cash_table();
        assert_eq!(
            Roster::form(vec![entry(6, 1, 1, 500)], &ad, false),
            Err(RosterRejected::Seat(SeatRejected::SeatOutOfRange {
                seat: 6
            }))
        );
    }

    #[test]
    fn the_display_name_is_bounded_and_printable() {
        let ad = cash_table();
        let mut long = entry(0, 1, 1, 500);
        long.display_name = "x".repeat(33);
        assert!(matches!(
            Roster::form(vec![long], &ad, false),
            Err(RosterRejected::Seat(SeatRejected::DisplayNameTooLong { .. }))
        ));

        let mut ctrl = entry(0, 1, 1, 500);
        ctrl.display_name = "a\u{0}b".into();
        assert_eq!(
            Roster::form(vec![ctrl], &ad, false),
            Err(RosterRejected::Seat(
                SeatRejected::DisplayNameHasControlCharacters
            ))
        );
    }

    #[test]
    fn a_peer_id_over_the_bound_is_refused() {
        let ad = cash_table();
        let mut long = entry(0, 1, 1, 500);
        long.peer_id = vec![0u8; 43];
        assert!(matches!(
            Roster::form(vec![long], &ad, false),
            Err(RosterRejected::Seat(SeatRejected::PeerIdTooLong { len: 43 }))
        ));
    }

    /// A partial roster in a `JOIN_ACCEPT` is not yet a table; a `TABLE_READY`
    /// one is. The same check with two callers and one flag, rather than two
    /// checks that could drift.
    #[test]
    fn the_minimum_applies_at_ratification_and_not_before() {
        let ad = cash_table();
        let one = vec![entry(0, 1, 1, 500)];
        assert!(Roster::form(one.clone(), &ad, false).is_ok());
        assert_eq!(
            Roster::form(one, &ad, true),
            Err(RosterRejected::TooFewSeats { n: 1, need: 2 })
        );
    }

    #[test]
    fn a_roster_wider_than_the_table_is_refused() {
        let ad = cash_table();
        let too_many: Vec<SeatEntry> = (0..7).map(|i| entry(i, i + 1, i + 1, 500)).collect();
        assert_eq!(
            Roster::form(too_many, &ad, false),
            Err(RosterRejected::TooManySeats { n: 7 })
        );
    }

    /// The starting stack in the genesis is the buy-in, which is D-018's closure
    /// of `U1`. Two rosters that differ only in a buy-in are two different
    /// tables, all the way down to every hand's `GENESIS(k)`.
    #[test]
    fn the_buyin_is_the_starting_stack_in_the_genesis() {
        let ad = cash_table();
        let a = Roster::form(vec![entry(0, 1, 1, 500), entry(1, 2, 2, 500)], &ad, true).unwrap();
        let b = Roster::form(vec![entry(0, 1, 1, 500), entry(1, 2, 2, 501)], &ad, true).unwrap();
        assert_ne!(
            a.hash_at_zero(),
            b.hash_at_zero(),
            "one chip of difference is a different table"
        );

        let same = Roster::form(vec![entry(0, 1, 1, 500), entry(1, 2, 2, 500)], &ad, true).unwrap();
        assert_eq!(a.hash_at_zero(), same.hash_at_zero());
    }

    /// The display name is not an identifier and must not reach the genesis: two
    /// clients that disagree about a name would otherwise disagree about every
    /// hand.
    #[test]
    fn the_display_name_does_not_reach_the_genesis() {
        let ad = cash_table();
        let mut renamed = entry(0, 1, 1, 500);
        renamed.display_name = "uplne jine jmeno".into();

        let a = Roster::form(vec![entry(0, 1, 1, 500), entry(1, 2, 2, 500)], &ad, true).unwrap();
        let b = Roster::form(vec![renamed, entry(1, 2, 2, 500)], &ad, true).unwrap();
        assert_eq!(a.hash_at_zero(), b.hash_at_zero());
    }

    #[test]
    fn the_lowest_free_seat_is_deterministic() {
        let ad = cash_table();
        let r = Roster::form(vec![entry(0, 1, 1, 500), entry(2, 2, 2, 500)], &ad, false).unwrap();
        assert_eq!(r.lowest_free_seat(6), Some(1));

        let full: Vec<SeatEntry> = (0..6).map(|i| entry(i, i + 1, i + 1, 500)).collect();
        let r = Roster::form(full, &ad, false).unwrap();
        assert_eq!(r.lowest_free_seat(6), None);
    }

    /// Per-join and per-table, so a proof seen at one table does not open
    /// another and a second join does not replay the first.
    #[test]
    fn a_password_proof_does_not_travel() {
        let pw = b"table password";
        let table_a = [1u8; 32];
        let table_b = [2u8; 32];
        let nonce_1 = [3u8; 32];
        let nonce_2 = [4u8; 32];

        let base = password_proof(pw, &table_a, &nonce_1);
        assert_eq!(base, password_proof(pw, &table_a, &nonce_1));
        assert_ne!(base, password_proof(pw, &table_b, &nonce_1), "another table");
        assert_ne!(base, password_proof(pw, &table_a, &nonce_2), "another join");
        assert_ne!(base, password_proof(b"jine heslo", &table_a, &nonce_1));
    }

    /// It hashes what it is given and normalises nothing. `password_utf8` has no
    /// defined normalisation anywhere in the corpus (`U5`), and inventing one
    /// here would make this implementation's choice the de facto standard without
    /// anybody deciding it. The test states the consequence rather than hiding
    /// it.
    #[test]
    fn two_spellings_of_one_password_are_two_passwords() {
        let table = [1u8; 32];
        let nonce = [2u8; 32];
        assert_ne!(
            password_proof(" heslo".as_bytes(), &table, &nonce),
            password_proof("heslo".as_bytes(), &table, &nonce),
            "leading space; nothing trims it and nothing says it should"
        );
        // Precomposed and decomposed forms of the same accented word.
        assert_ne!(
            password_proof("\u{00e1}".as_bytes(), &table, &nonce),
            password_proof("a\u{0301}".as_bytes(), &table, &nonce),
            "U5: two clients normalising differently fail as `bad password`"
        );
    }
}
