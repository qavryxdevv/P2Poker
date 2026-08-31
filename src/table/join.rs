//! The join messages: `PROTOCOL.md` §4.3's `0x0201` to `0x0205`.
//!
//! The bodies and their validation rules. The transport that carries them is
//! [`crate::net`]'s; what is here is what each side must check before believing
//! anything, and it is separated from the transport for the reason §1.4 gives in
//! so many words: *a receiver that skips §4.0 step 9 because the transport
//! already authenticated the sender has accepted a forged action.*
//!
//! # The founder is not trusted, and every message says where that shows
//!
//! The founder allocates seats, echoes rosters and signs the list — and is a
//! peer like any other. So:
//!
//! * `JOIN_ACCEPT` repeats the **complete signed advert** rather than naming it,
//!   so the joiner re-verifies the table key's own signature instead of trusting
//!   a gossip copy or the founder's word.
//! * The joiner recomputes `table_params_hash` from that advert and compares it
//!   against its own. Trivially equal while the founder echoes honestly — and
//!   required anyway, so that an implementation which ever relaxes the echo still
//!   carries the parameter check.
//! * A `PLAYER_LIST` is a **proposal**. It becomes fact only when every listed
//!   seat signs `TABLE_READY` over it.
//! * A `JOIN_REJECT` reason is **advisory**. The founder may lie, so a rejection
//!   is never proof of anything and the interface shows it as a claim.
//!
//! # What each side retains, and why it must
//!
//! The joiner keeps **its own copy of the advert it joined under**, from the
//! moment it sends `JOIN_REQUEST`. It does not read the lobby store for the three
//! §4.3 comparisons, because [`LobbyStore`](crate::net::lobby::LobbyStore)
//! overwrites the held advert on every accepted re-broadcast — so *"the
//! advertisement it joined under"* is a moving target unless somebody pins it.
//! That is `U11` (D-018), still open in the corpus and answered here by
//! retaining, which is the only reading under which the comparisons are
//! computable at all.

use crate::net::advert::table_params_hash;
use crate::net::lobby::TableAd;
use crate::poker::state::Hash;
use crate::table::formation::{password_proof, Roster, RosterRejected, SeatEntry};

/// What a joiner asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinRequest {
    /// `event_hash` of the `LOBBY_TABLE_AD` being joined.
    pub advert_hash: Hash,
    /// Must equal the envelope's `sender_public_key`.
    pub app_public_key: [u8; 32],
    /// Must equal the connection's authenticated remote `PeerId`.
    pub peer_id: Vec<u8>,
    pub display_name: String,
    /// A specific seat, or `None` for "any".
    pub requested_seat: Option<u8>,
    /// Present exactly when the advert requires a password.
    pub password_proof: Option<Hash>,
    pub buyin: u64,
    /// Fresh per join. The anti-replay for this RPC, since the envelope's
    /// `sequence` is a sentinel.
    pub join_nonce: Hash,
    /// The table being joined — must equal the advert's signing key.
    pub table_id: Hash,
}

/// Why a founder will not seat this request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinRefused {
    /// The advert named is not one this founder signed, or has expired.
    UnknownAdvert,
    /// `n(8) table_id` is not the advert's signing key.
    WrongTable,
    /// `app_public_key` is not the envelope's sender.
    KeyIsNotTheSender,
    /// `peer_id` is not the connection's authenticated remote.
    PeerIdIsNotTheConnection,
    /// The advert wants a password and this request has none, or the other way
    /// about, or it does not recompute.
    BadPassword,
    /// The seat asked for is taken, or outside the table.
    SeatUnavailable { seat: u8 },
    /// Every seat is taken.
    TableFull,
    /// This key, or this node, already holds a seat here.
    AlreadySeated,
    /// The entry the request would produce is not admissible.
    Seat(RosterRejected),
}

/// `JOIN_REJECT`'s reason codes, §4.3.
///
/// **Advisory in every case.** The founder may lie, so a rejection is never
/// proof of anything and the interface must show it as a claim rather than a
/// fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    TableFull = 1,
    SeatTaken = 2,
    BadPassword = 3,
    BuyinOutOfRange = 4,
    AdvertExpired = 5,
    /// `U2`: no ban list, state, scope, authority or lifetime exists anywhere in
    /// the corpus. Kept because the code point is allocated; **never emitted by
    /// this client**, because emitting a reason with no mechanism behind it says
    /// something untrue about why the seat was refused.
    Banned = 6,
    /// `U3`: `JOIN_REQUEST` carries no capability field and §1.3 puts the check
    /// at `TABLE_READY`. Same disposition as `Banned` and for the same reason.
    CapabilityMismatch = 7,
    AlreadySeated = 8,
}

impl RejectReason {
    /// Whether this client will ever send this reason.
    ///
    /// Two of the eight have no mechanism behind them anywhere in the corpus.
    /// Sending one would be asserting something this client cannot know, and a
    /// reason that is never emitted is better than one that is emitted falsely.
    pub const fn emittable(self) -> bool {
        !matches!(self, RejectReason::Banned | RejectReason::CapabilityMismatch)
    }

    pub const fn code(self) -> u16 {
        self as u16
    }
}

/// What the joiner retains from the moment it asks.
///
/// `U11`: *"the advertisement it joined under"* is never pinned to a stored
/// object in the corpus, and the lobby store overwrites its copy on every
/// accepted re-broadcast. Retaining it here is the only reading under which
/// §4.3's three comparisons are computable.
#[derive(Debug, Clone)]
pub struct JoinedUnder {
    /// The advert, as this client verified it.
    pub ad: TableAd,
    /// Its `event_hash`, which is what `JOIN_REQUEST n(0)` carries.
    pub advert_hash: Hash,
    /// The table's signing key, which is the table's identity.
    pub table_id: Hash,
    /// Recomputed once, here, and compared everywhere else.
    pub params: Hash,
}

impl JoinedUnder {
    pub fn pin(ad: TableAd, advert_hash: Hash, table_id: Hash) -> Self {
        let params = table_params_hash(&ad);
        JoinedUnder {
            ad,
            advert_hash,
            table_id,
            params,
        }
    }
}

/// The founder's admission check for a `JOIN_REQUEST`.
///
/// `connection_peer_id` is what the transport authenticated, and `roster` is what
/// the founder has seated so far.
pub fn admit_join(
    req: &JoinRequest,
    sender_public_key: &[u8; 32],
    connection_peer_id: &[u8],
    under: &JoinedUnder,
    roster: &Roster,
    password: Option<&[u8]>,
) -> Result<SeatEntry, JoinRefused> {
    if req.advert_hash != under.advert_hash {
        return Err(JoinRefused::UnknownAdvert);
    }
    if req.table_id != under.table_id {
        return Err(JoinRefused::WrongTable);
    }
    if &req.app_public_key != sender_public_key {
        return Err(JoinRefused::KeyIsNotTheSender);
    }
    if req.peer_id != connection_peer_id {
        return Err(JoinRefused::PeerIdIsNotTheConnection);
    }

    // The password gate, both ways round: a proof where none is wanted is as
    // wrong as none where one is.
    match (under.ad.password_required, req.password_proof, password) {
        (false, None, _) => {}
        (true, Some(offered), Some(secret)) => {
            let expected = password_proof(secret, &under.table_id, &req.join_nonce);
            if offered != expected {
                return Err(JoinRefused::BadPassword);
            }
        }
        _ => return Err(JoinRefused::BadPassword),
    }

    // Already seated, on either key. `peer_id` is `U17`: one node, one seat at
    // this table — and this table only, so multi-tabling is untouched.
    if roster
        .seats()
        .iter()
        .any(|e| e.app_public_key == req.app_public_key || e.peer_id == req.peer_id)
    {
        return Err(JoinRefused::AlreadySeated);
    }

    let seat = match req.requested_seat {
        Some(s) => {
            if s >= under.ad.max_players || roster.seats().iter().any(|e| e.seat == s) {
                return Err(JoinRefused::SeatUnavailable { seat: s });
            }
            s
        }
        // `U7` leaves the founder's rule undefined. Lowest free is deterministic,
        // which is the only kind a joiner could ever check.
        None => roster
            .lowest_free_seat(under.ad.max_players)
            .ok_or(JoinRefused::TableFull)?,
    };

    let entry = SeatEntry {
        seat,
        app_public_key: req.app_public_key,
        peer_id: req.peer_id.clone(),
        display_name: req.display_name.clone(),
        buyin: req.buyin,
    };

    // The whole roster is re-formed with the new entry rather than the entry
    // being checked alone, so the uniqueness rules run over the result and not
    // over an intention.
    let mut with = roster.seats().to_vec();
    with.push(entry.clone());
    with.sort_by_key(|e| e.seat);
    Roster::form(with, &under.ad, false).map_err(JoinRefused::Seat)?;

    Ok(entry)
}

/// What the founder sends back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinAccept {
    pub request_hash: Hash,
    pub seat: u8,
    /// The complete `SignedEvent` of the advert, repeated verbatim.
    ///
    /// Repeated rather than named so the joiner re-verifies the table key's own
    /// signature instead of trusting a gossip copy or the founder's word.
    pub advert_event: Vec<u8>,
    pub roster_so_far: Vec<SeatEntry>,
}

/// Why a joiner will not act on an acceptance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptRefused {
    /// Not signed by the table this client asked to join.
    NotTheTableKey,
    /// It answers a different request.
    WrongRequest,
    /// The echoed advert is not the one this client joined under.
    AdvertMismatch,
    /// The parameters recomputed from the echoed advert are not this client's.
    ///
    /// Trivially satisfied while the founder echoes honestly, and required
    /// anyway: an implementation that ever relaxed the echo would still carry
    /// the parameter check.
    ParametersMismatch,
    /// The seat given is not in the roster, or is somebody else's.
    SeatNotOurs { seat: u8 },
    /// The roster seats this client with a different buy-in from the one it
    /// asked for.
    ///
    /// Not a rounding difference to shrug at: D-018 made `SeatEntry.buyin` this
    /// seat's `stack_at_hand_start` in `roster_hash(0)`, which feeds
    /// `session_id` and every hand's `GENESIS(k)`. A founder that seats you with
    /// a number you did not choose has changed your stack and the identity of
    /// every hand the table will play.
    BuyinNotOurs { asked: u64, given: u64 },
    /// The roster is not well formed.
    Roster(RosterRejected),
    /// Emitted too long ago, or too far ahead of this client's own clock.
    ///
    /// A list carries no time of its own; this is its **envelope's**, which the
    /// founder signed. The check matters most where there is nothing else to
    /// check against: until the first list arrives a client holds no serial, so
    /// any serial is admissible and a genuine list from an hour ago is a valid
    /// one.
    Stale,
}

/// The joiner's check on a `JOIN_ACCEPT`.
///
/// `advert_hash_of_echo` and `ad_of_echo` are what §4.0 produced from
/// `advert_event` — decoded, signature-checked and admitted. Passing them in
/// rather than re-deriving them here keeps this function about §4.3's rules and
/// leaves §4.0's pipeline in one place.
#[allow(clippy::too_many_arguments)]
pub fn admit_accept(
    accept: &JoinAccept,
    sender_public_key: &[u8; 32],
    our_request_hash: &Hash,
    advert_hash_of_echo: &Hash,
    ad_of_echo: &TableAd,
    under: &JoinedUnder,
    our_key: &[u8; 32],
    our_buyin: u64,
) -> Result<Roster, AcceptRefused> {
    if sender_public_key != &under.table_id {
        return Err(AcceptRefused::NotTheTableKey);
    }
    if &accept.request_hash != our_request_hash {
        return Err(AcceptRefused::WrongRequest);
    }
    if advert_hash_of_echo != &under.advert_hash {
        return Err(AcceptRefused::AdvertMismatch);
    }
    if table_params_hash(ad_of_echo) != under.params {
        return Err(AcceptRefused::ParametersMismatch);
    }

    let roster =
        Roster::form(accept.roster_so_far.clone(), &under.ad, false).map_err(AcceptRefused::Roster)?;

    let ours = roster
        .seats()
        .iter()
        .find(|e| &e.app_public_key == our_key)
        .ok_or(AcceptRefused::SeatNotOurs { seat: accept.seat })?;

    if ours.seat != accept.seat {
        return Err(AcceptRefused::SeatNotOurs { seat: accept.seat });
    }
    // The buy-in is this seat's starting stack in `roster_hash(0)`, so a founder
    // that seated us with a different one changed our stack and the identity of
    // every hand the table will play.
    if ours.buyin != our_buyin {
        return Err(AcceptRefused::BuyinNotOurs {
            asked: our_buyin,
            given: ours.buyin,
        });
    }
    Ok(roster)
}

/// The founder's proposal of who is at the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerList {
    pub roster: Vec<SeatEntry>,
    pub table_params_hash: Hash,
    /// Strictly increasing per table. The anti-replay for a message that is not
    /// yet in a hash chain.
    pub list_serial: u64,
}

/// Why a player list is not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListRefused {
    NotTheTableKey,
    /// Not strictly greater than the last accepted serial.
    NotNewer { got: u64, held: u64 },
    /// The founder is forming a table under parameters other than the ones this
    /// client agreed to.
    ///
    /// The client **leaves** rather than sitting down. There is no version of
    /// this that is a warning.
    ParametersMismatch,
    Roster(RosterRejected),
    /// Emitted too long ago, or too far ahead of this client's own clock.
    ///
    /// A list carries no time of its own; this is its **envelope's**, which the
    /// founder signed. It matters most where there is nothing else to check
    /// against: until the first list arrives a client holds no serial, so any
    /// serial is admissible and a genuine list from an hour ago is a valid one.
    Stale,
}

/// The joiner's check on a `PLAYER_LIST`.
pub fn admit_list(
    list: &PlayerList,
    sender_public_key: &[u8; 32],
    last_serial: Option<u64>,
    under: &JoinedUnder,
) -> Result<Roster, ListRefused> {
    if sender_public_key != &under.table_id {
        return Err(ListRefused::NotTheTableKey);
    }
    if let Some(held) = last_serial {
        if list.list_serial <= held {
            return Err(ListRefused::NotNewer {
                got: list.list_serial,
                held,
            });
        }
    }
    if list.table_params_hash != under.params {
        return Err(ListRefused::ParametersMismatch);
    }
    Roster::form(list.roster.clone(), &under.ad, false).map_err(ListRefused::Roster)
}

/// One seat's ratification of a roster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableReady {
    pub roster_hash: Hash,
    pub list_serial: u64,
    pub table_params_hash: Hash,
    pub my_seat: u8,
    pub capability_set: Vec<Vec<u8>>,
}

/// Why a ratification is not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadyRefused {
    /// The sender is not in the roster, or not at the seat it claims.
    NotAtThatSeat { seat: u8 },
    /// The roster hash does not recompute.
    RosterMismatch,
    /// The parameters are not the ones this client joined under, or not the
    /// ones the list being ratified carried.
    ParametersMismatch,
    /// A different list than the one this client holds.
    WrongList { got: u64, held: u64 },
    /// The roster being ratified is smaller than the table needs to start.
    ///
    /// `Roster::form`'s `require_minimum` existed from the beginning and **no
    /// caller ever passed true**, so `min_players_to_start` was a field the
    /// advert carried, the admission rules range-checked, and nothing enforced.
    /// A founder could ratify a two-seat roster for a table advertised as
    /// needing ten.
    TooFewToStart { seated: usize, need: u8 },
}

/// The check on somebody else's `TABLE_READY`.
pub fn admit_ready(
    ready: &TableReady,
    sender_public_key: &[u8; 32],
    roster: &Roster,
    list_serial: u64,
    under: &JoinedUnder,
) -> Result<(), ReadyRefused> {
    // `TABLE_READY` is where a proposal becomes a fact, and it is therefore the
    // one place `min_players_to_start` can be enforced. Nothing enforced it.
    if roster.len() < under.ad.min_players_to_start as usize {
        return Err(ReadyRefused::TooFewToStart {
            seated: roster.len(),
            need: under.ad.min_players_to_start,
        });
    }
    match roster.seat_of(sender_public_key) {
        Some(s) if s == ready.my_seat => {}
        _ => {
            return Err(ReadyRefused::NotAtThatSeat {
                seat: ready.my_seat,
            })
        }
    }
    if ready.list_serial != list_serial {
        return Err(ReadyRefused::WrongList {
            got: ready.list_serial,
            held: list_serial,
        });
    }
    if ready.table_params_hash != under.params {
        return Err(ReadyRefused::ParametersMismatch);
    }
    if ready.roster_hash != roster.hash_at_zero() {
        return Err(ReadyRefused::RosterMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::lobby::{BlindSchedule, Mode, DECK_SUITE_V1};
    use crate::protocol::constants::hand_deadline_min_ms;

    fn ad() -> TableAd {
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
            founder_peer_id: b"founder".to_vec(),
            timestamp_unix_ms: 1_700_000_000_000,
            expires_at_unix_ms: 1_700_000_090_000,
        };
        a.hand_deadline_ms = hand_deadline_min_ms(6, 20_000, 5_000, 30_000, 7_000, 0) as u32;
        a
    }

    fn under() -> JoinedUnder {
        JoinedUnder::pin(ad(), [0xAA; 32], [0xBB; 32])
    }

    fn request(key: u8, peer: u8, seat: Option<u8>) -> JoinRequest {
        JoinRequest {
            advert_hash: [0xAA; 32],
            app_public_key: [key; 32],
            peer_id: vec![peer; 12],
            display_name: format!("hrac {key}"),
            requested_seat: seat,
            password_proof: None,
            buyin: 500,
            join_nonce: [key; 32],
            table_id: [0xBB; 32],
        }
    }

    fn empty_roster() -> Roster {
        Roster::form(Vec::new(), &ad(), false).unwrap()
    }

    fn seated(entries: Vec<SeatEntry>) -> Roster {
        Roster::form(entries, &ad(), false).unwrap()
    }

    #[test]
    fn an_ordinary_request_is_seated() {
        let u = under();
        let req = request(1, 1, Some(2));
        let e = admit_join(&req, &[1u8; 32], &[1u8; 12], &u, &empty_roster(), None)
            .expect("a legal request");
        assert_eq!(e.seat, 2);
        assert_eq!(e.buyin, 500);
    }

    /// `U7` has no founder rule, so this one is deterministic — the only kind a
    /// joiner could check.
    #[test]
    fn any_seat_means_the_lowest_free_one() {
        let u = under();
        let roster = seated(vec![SeatEntry {
            seat: 0,
            app_public_key: [9u8; 32],
            peer_id: vec![9u8; 12],
            display_name: "prvni".into(),
            buyin: 500,
        }]);
        let e = admit_join(
            &request(1, 1, None),
            &[1u8; 32],
            &[1u8; 12],
            &u,
            &roster,
            None,
        )
        .unwrap();
        assert_eq!(e.seat, 1);
    }

    /// The two identity checks the transport cannot make for us. §1.4 is explicit
    /// that a receiver skipping them because the transport authenticated the
    /// sender has accepted a forged action.
    #[test]
    fn the_key_and_the_peer_must_be_the_ones_that_connected() {
        let u = under();
        let req = request(1, 1, Some(0));

        assert_eq!(
            admit_join(&req, &[2u8; 32], &[1u8; 12], &u, &empty_roster(), None),
            Err(JoinRefused::KeyIsNotTheSender)
        );
        assert_eq!(
            admit_join(&req, &[1u8; 32], &[2u8; 12], &u, &empty_roster(), None),
            Err(JoinRefused::PeerIdIsNotTheConnection)
        );
    }

    /// `U17` at the founder: one node, one seat at this table.
    #[test]
    fn a_second_seat_from_one_node_is_refused_at_the_founder() {
        let u = under();
        let roster = seated(vec![SeatEntry {
            seat: 0,
            app_public_key: [9u8; 32],
            peer_id: vec![1u8; 12], // the same node as the request below
            display_name: "ja".into(),
            buyin: 500,
        }]);
        assert_eq!(
            admit_join(
                &request(1, 1, Some(1)),
                &[1u8; 32],
                &[1u8; 12],
                &u,
                &roster,
                None
            ),
            Err(JoinRefused::AlreadySeated)
        );
    }

    /// And the same key twice, which is the older half of the rule.
    #[test]
    fn a_second_seat_from_one_key_is_refused() {
        let u = under();
        let roster = seated(vec![SeatEntry {
            seat: 0,
            app_public_key: [1u8; 32],
            peer_id: vec![9u8; 12],
            display_name: "ja".into(),
            buyin: 500,
        }]);
        assert_eq!(
            admit_join(
                &request(1, 1, Some(1)),
                &[1u8; 32],
                &[1u8; 12],
                &u,
                &roster,
                None
            ),
            Err(JoinRefused::AlreadySeated)
        );
    }

    #[test]
    fn a_taken_or_impossible_seat_is_refused() {
        let u = under();
        let roster = seated(vec![SeatEntry {
            seat: 3,
            app_public_key: [9u8; 32],
            peer_id: vec![9u8; 12],
            display_name: "tam".into(),
            buyin: 500,
        }]);
        assert_eq!(
            admit_join(
                &request(1, 1, Some(3)),
                &[1u8; 32],
                &[1u8; 12],
                &u,
                &roster,
                None
            ),
            Err(JoinRefused::SeatUnavailable { seat: 3 })
        );
        assert_eq!(
            admit_join(
                &request(1, 1, Some(9)),
                &[1u8; 32],
                &[1u8; 12],
                &u,
                &roster,
                None
            ),
            Err(JoinRefused::SeatUnavailable { seat: 9 })
        );
    }

    #[test]
    fn a_full_table_seats_nobody() {
        let u = under();
        let full: Vec<SeatEntry> = (0..6)
            .map(|i| SeatEntry {
                seat: i,
                app_public_key: [i + 10; 32],
                peer_id: vec![i + 10; 12],
                display_name: format!("s{i}"),
                buyin: 500,
            })
            .collect();
        assert_eq!(
            admit_join(
                &request(1, 1, None),
                &[1u8; 32],
                &[1u8; 12],
                &u,
                &seated(full),
                None
            ),
            Err(JoinRefused::TableFull)
        );
    }

    /// A buy-in outside the advert's range is refused at the founder, because it
    /// would be this seat's starting stack in every hand's genesis.
    #[test]
    fn a_bad_buyin_never_reaches_the_roster() {
        let u = under();
        let mut req = request(1, 1, Some(0));
        req.buyin = 10;
        assert!(matches!(
            admit_join(&req, &[1u8; 32], &[1u8; 12], &u, &empty_roster(), None),
            Err(JoinRefused::Seat(_))
        ));
    }

    /// The password gate, both ways round: a proof where none is wanted is as
    /// wrong as none where one is.
    #[test]
    fn the_password_gate_is_symmetric() {
        let mut protected = ad();
        protected.password_required = true;
        let u = JoinedUnder::pin(protected, [0xAA; 32], [0xBB; 32]);
        let secret = b"table password";

        let mut req = request(1, 1, Some(0));
        req.password_proof = Some(password_proof(secret, &[0xBB; 32], &req.join_nonce));
        assert!(admit_join(
            &req,
            &[1u8; 32],
            &[1u8; 12],
            &u,
            &empty_roster(),
            Some(secret)
        )
        .is_ok());

        // Wrong password.
        let mut wrong = request(1, 1, Some(0));
        wrong.password_proof = Some(password_proof(b"jine", &[0xBB; 32], &wrong.join_nonce));
        assert_eq!(
            admit_join(
                &wrong,
                &[1u8; 32],
                &[1u8; 12],
                &u,
                &empty_roster(),
                Some(secret)
            ),
            Err(JoinRefused::BadPassword)
        );

        // None offered where one is wanted.
        assert_eq!(
            admit_join(
                &request(1, 1, Some(0)),
                &[1u8; 32],
                &[1u8; 12],
                &u,
                &empty_roster(),
                Some(secret)
            ),
            Err(JoinRefused::BadPassword)
        );

        // And one offered where none is wanted.
        let open = under();
        let mut unwanted = request(1, 1, Some(0));
        unwanted.password_proof = Some([0u8; 32]);
        assert_eq!(
            admit_join(
                &unwanted,
                &[1u8; 32],
                &[1u8; 12],
                &open,
                &empty_roster(),
                None
            ),
            Err(JoinRefused::BadPassword)
        );
    }

    /// A proof for one table does not open another, which is what the per-table
    /// binding is for.
    #[test]
    fn a_proof_from_another_table_does_not_open_this_one() {
        let mut protected = ad();
        protected.password_required = true;
        let u = JoinedUnder::pin(protected, [0xAA; 32], [0xBB; 32]);
        let secret = b"heslo";

        let mut req = request(1, 1, Some(0));
        req.password_proof = Some(password_proof(secret, &[0xCC; 32], &req.join_nonce));
        assert_eq!(
            admit_join(
                &req,
                &[1u8; 32],
                &[1u8; 12],
                &u,
                &empty_roster(),
                Some(secret)
            ),
            Err(JoinRefused::BadPassword)
        );
    }

    #[test]
    fn a_request_for_another_table_is_refused() {
        let u = under();
        let mut req = request(1, 1, Some(0));
        req.table_id = [0xCC; 32];
        assert_eq!(
            admit_join(&req, &[1u8; 32], &[1u8; 12], &u, &empty_roster(), None),
            Err(JoinRefused::WrongTable)
        );

        let mut other = request(1, 1, Some(0));
        other.advert_hash = [0xCC; 32];
        assert_eq!(
            admit_join(&other, &[1u8; 32], &[1u8; 12], &u, &empty_roster(), None),
            Err(JoinRefused::UnknownAdvert)
        );
    }

    // -- the joiner's side --------------------------------------------------

    fn accept_for(seat: u8, roster: Vec<SeatEntry>) -> JoinAccept {
        JoinAccept {
            request_hash: [0x11; 32],
            seat,
            advert_event: Vec::new(),
            roster_so_far: roster,
        }
    }

    fn me() -> SeatEntry {
        SeatEntry {
            seat: 1,
            app_public_key: [1u8; 32],
            peer_id: vec![1u8; 12],
            display_name: "ja".into(),
            buyin: 500,
        }
    }

    #[test]
    fn an_honest_acceptance_is_taken() {
        let u = under();
        let accept = accept_for(1, vec![me()]);
        let r = admit_accept(
            &accept,
            &[0xBB; 32],
            &[0x11; 32],
            &[0xAA; 32],
            &ad(),
            &u,
            &[1u8; 32],
            500,
        )
        .expect("the founder answered honestly");
        assert_eq!(r.seat_of(&[1u8; 32]), Some(1));
    }

    /// The founder is a peer like any other, so the joiner recomputes the
    /// parameters from the echoed advert rather than trusting the echo.
    /// Trivially equal while the founder is honest, and the check is what makes
    /// the honesty checkable.
    #[test]
    fn a_founder_forming_a_different_table_is_caught() {
        let u = under();
        let mut different = ad();
        different.small_blind = 25;
        different.big_blind = 50;
        different.blind_schedule.first_small_blind = 25;

        assert_eq!(
            admit_accept(
                &accept_for(1, vec![me()]),
                &[0xBB; 32],
                &[0x11; 32],
                &[0xAA; 32],
                &different,
                &u,
                &[1u8; 32],
                500
            ),
            Err(AcceptRefused::ParametersMismatch)
        );
    }

    #[test]
    fn an_acceptance_from_the_wrong_key_or_for_the_wrong_request_is_refused() {
        let u = under();
        assert_eq!(
            admit_accept(
                &accept_for(1, vec![me()]),
                &[0xCC; 32],
                &[0x11; 32],
                &[0xAA; 32],
                &ad(),
                &u,
                &[1u8; 32],
                500
            ),
            Err(AcceptRefused::NotTheTableKey)
        );
        assert_eq!(
            admit_accept(
                &accept_for(1, vec![me()]),
                &[0xBB; 32],
                &[0x22; 32],
                &[0xAA; 32],
                &ad(),
                &u,
                &[1u8; 32],
                500
            ),
            Err(AcceptRefused::WrongRequest)
        );
        assert_eq!(
            admit_accept(
                &accept_for(1, vec![me()]),
                &[0xBB; 32],
                &[0x11; 32],
                &[0xCC; 32],
                &ad(),
                &u,
                &[1u8; 32],
                500
            ),
            Err(AcceptRefused::AdvertMismatch)
        );
    }

    /// A seat this client is not in the roster at is not this client's seat,
    /// whatever the acceptance says.
    #[test]
    fn a_seat_the_roster_does_not_give_us_is_refused() {
        let u = under();
        assert_eq!(
            admit_accept(
                &accept_for(4, vec![me()]),
                &[0xBB; 32],
                &[0x11; 32],
                &[0xAA; 32],
                &ad(),
                &u,
                &[1u8; 32],
                500
            ),
            Err(AcceptRefused::SeatNotOurs { seat: 4 })
        );

        // And a roster that does not contain us at all.
        let stranger = SeatEntry {
            seat: 0,
            app_public_key: [9u8; 32],
            peer_id: vec![9u8; 12],
            display_name: "nekdo".into(),
            buyin: 500,
        };
        assert_eq!(
            admit_accept(
                &accept_for(0, vec![stranger]),
                &[0xBB; 32],
                &[0x11; 32],
                &[0xAA; 32],
                &ad(),
                &u,
                &[1u8; 32],
                500
            ),
            Err(AcceptRefused::SeatNotOurs { seat: 0 })
        );
    }

    // -- the list and the ratification --------------------------------------

    fn two() -> Vec<SeatEntry> {
        vec![
            SeatEntry {
                seat: 0,
                app_public_key: [9u8; 32],
                peer_id: vec![9u8; 12],
                display_name: "prvni".into(),
                buyin: 500,
            },
            me(),
        ]
    }

    #[test]
    fn a_list_is_taken_once_and_then_only_if_newer() {
        let u = under();
        let list = PlayerList {
            roster: two(),
            table_params_hash: u.params,
            list_serial: 5,
        };
        assert!(admit_list(&list, &[0xBB; 32], None, &u).is_ok());
        assert!(admit_list(&list, &[0xBB; 32], Some(4), &u).is_ok());
        assert_eq!(
            admit_list(&list, &[0xBB; 32], Some(5), &u),
            Err(ListRefused::NotNewer { got: 5, held: 5 })
        );
    }

    /// A founder forming under other parameters is not a warning. The client
    /// leaves rather than sitting down.
    #[test]
    fn a_list_under_other_parameters_is_refused() {
        let u = under();
        let list = PlayerList {
            roster: two(),
            table_params_hash: [0xEE; 32],
            list_serial: 1,
        };
        assert_eq!(
            admit_list(&list, &[0xBB; 32], None, &u),
            Err(ListRefused::ParametersMismatch)
        );
    }

    #[test]
    fn a_list_from_anybody_but_the_table_is_refused() {
        let u = under();
        let list = PlayerList {
            roster: two(),
            table_params_hash: u.params,
            list_serial: 1,
        };
        assert_eq!(
            admit_list(&list, &[0xCC; 32], None, &u),
            Err(ListRefused::NotTheTableKey)
        );
    }

    #[test]
    fn a_ratification_of_the_roster_this_client_holds_is_accepted() {
        let u = under();
        let roster = seated(two());
        let ready = TableReady {
            roster_hash: roster.hash_at_zero(),
            list_serial: 7,
            table_params_hash: u.params,
            my_seat: 1,
            capability_set: vec![b"nlhe/2-6".to_vec()],
        };
        assert_eq!(admit_ready(&ready, &[1u8; 32], &roster, 7, &u), Ok(()));
    }


    /// The buy-in is this seat's starting stack in `roster_hash(0)`, so a
    /// founder that seats you with a number you did not choose has changed your
    /// stack and the identity of every hand the table will play. Nothing
    /// checked it.
    #[test]
    fn a_seat_with_the_wrong_buyin_is_refused() {
        let u = under();
        let mut cheated = me();
        cheated.buyin = 250;

        assert_eq!(
            admit_accept(
                &accept_for(1, vec![cheated]),
                &[0xBB; 32],
                &[0x11; 32],
                &[0xAA; 32],
                &ad(),
                &u,
                &[1u8; 32],
                500
            ),
            Err(AcceptRefused::BuyinNotOurs {
                asked: 500,
                given: 250
            })
        );
    }

    /// `min_players_to_start` was a field the advert carried, the admission
    /// rules range-checked, and **nothing enforced** — `Roster::form`'s
    /// `require_minimum` existed and no caller ever passed true. A founder could
    /// ratify a two-seat roster for a table advertised as needing more.
    #[test]
    fn a_roster_below_the_minimum_cannot_be_ratified() {
        let mut small = ad();
        small.min_players_to_start = 4;
        let u = JoinedUnder::pin(small, [0xAA; 32], [0xBB; 32]);

        let roster = seated(two());
        let ready = TableReady {
            roster_hash: roster.hash_at_zero(),
            list_serial: 7,
            table_params_hash: u.params,
            my_seat: 1,
            capability_set: Vec::new(),
        };
        assert_eq!(
            admit_ready(&ready, &[1u8; 32], &roster, 7, &u),
            Err(ReadyRefused::TooFewToStart { seated: 2, need: 4 })
        );
    }

    /// Every way a ratification can fail to be about the same table, one at a
    /// time. A `TABLE_READY` is what turns a proposal into a fact, so a wrong one
    /// accepted is a fact nobody agreed to.
    #[test]
    fn a_ratification_of_something_else_is_refused() {
        let u = under();
        let roster = seated(two());
        let good = TableReady {
            roster_hash: roster.hash_at_zero(),
            list_serial: 7,
            table_params_hash: u.params,
            my_seat: 1,
            capability_set: Vec::new(),
        };

        // Not at that seat.
        let mut wrong_seat = good.clone();
        wrong_seat.my_seat = 0;
        assert_eq!(
            admit_ready(&wrong_seat, &[1u8; 32], &roster, 7, &u),
            Err(ReadyRefused::NotAtThatSeat { seat: 0 })
        );

        // Not in the roster at all.
        assert_eq!(
            admit_ready(&good, &[7u8; 32], &roster, 7, &u),
            Err(ReadyRefused::NotAtThatSeat { seat: 1 })
        );

        // A different list.
        assert_eq!(
            admit_ready(&good, &[1u8; 32], &roster, 8, &u),
            Err(ReadyRefused::WrongList { got: 7, held: 8 })
        );

        // Different parameters.
        let mut wrong_params = good.clone();
        wrong_params.table_params_hash = [0xEE; 32];
        assert_eq!(
            admit_ready(&wrong_params, &[1u8; 32], &roster, 7, &u),
            Err(ReadyRefused::ParametersMismatch)
        );

        // A roster hash that does not recompute.
        let mut wrong_roster = good;
        wrong_roster.roster_hash = [0xEE; 32];
        assert_eq!(
            admit_ready(&wrong_roster, &[1u8; 32], &roster, 7, &u),
            Err(ReadyRefused::RosterMismatch)
        );
    }

    /// Two of the eight reason codes have no mechanism behind them anywhere in
    /// the corpus. Emitting one would assert something this client cannot know,
    /// and a reason never sent is better than one sent falsely.
    #[test]
    fn the_reasons_with_no_mechanism_are_never_emitted() {
        assert!(!RejectReason::Banned.emittable(), "U2: no ban list exists");
        assert!(
            !RejectReason::CapabilityMismatch.emittable(),
            "U3: JOIN_REQUEST carries no capability field"
        );
        for r in [
            RejectReason::TableFull,
            RejectReason::SeatTaken,
            RejectReason::BadPassword,
            RejectReason::BuyinOutOfRange,
            RejectReason::AdvertExpired,
            RejectReason::AlreadySeated,
        ] {
            assert!(r.emittable());
        }
        assert_eq!(RejectReason::TableFull.code(), 1);
        assert_eq!(RejectReason::AlreadySeated.code(), 8);
    }
}
