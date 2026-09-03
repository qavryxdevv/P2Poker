//! **A certificate at ten seats has to fit in its own cap.**
//!
//! `TIMEOUT_CERT_CAP`'s doc says it bounds *"a digest and up to `MAX_SEATS - 1`
//! embedded signed votes"*, and `TimeoutCert::votes` says those are *"the whole
//! signed events and not just their signatures"*. Nothing checked that the two
//! statements are compatible. `tests/timeout_certificate.rs` is a thousand
//! lines long and every case in it opens three seats, where the voter set is
//! two — so the largest certificate ever built in this repo carried **two**
//! votes against a documented maximum of nine.
//!
//! It does not fit. Measured on a real ten-seat, two-machine run,
//! `split163641-10`: hand #4 stalled with nine of ten seats at *"your turn"*
//! and the tenth, seat 2, still at *"the deck is shuffled and sealed"* — it had
//! never learned it was on the clock. The other nine each ran their clock out
//! on seat 2 and the tally climbed 1/9, 2/9 … to **9/9 agree at 376.0 s**. The
//! table was unanimous. And from the moment the ninth vote landed, every seat
//! logged
//!
//! ```text
//! hand: TooLong("the payload is over its cap")
//! ```
//!
//! once per attempt for the remaining four minutes of the run. The hand never
//! ended. A full table cannot certify a timeout at all, so one seat missing one
//! message freezes the table permanently — which is the worst failure this
//! protocol has, since it is indistinguishable to every honest seat from the
//! table simply being over.
//!
//! This file pins the arithmetic at the documented maximum so the cap can never
//! drift under it again.

use ed25519_dalek::SigningKey;
use minicbor::Encode;
use p2p_poker::net::chained::{self, Slot};
use p2p_poker::protocol::constants::MAX_SEATS;
use p2p_poker::protocol::messages::EventType;
use p2p_poker::table::hand::{
    DEAL_PRIVATE_CAP, HAND_COMPLETE_CAP, HAND_INIT_CAP, TIMEOUT_CERT_CAP, TIMEOUT_VOTE_CAP,
};
use p2p_poker::table::handwire::{
    DealPrivate, HandComplete, HandInit, PotAward, Refund, RevealEntry, TimeoutCert, TimeoutVote,
};

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

/// Every field at its widest, because a cap is about the worst case.
fn slot() -> Slot {
    Slot {
        table_id: [0xAB; 32],
        hand_id: u64::MAX,
        sequence: u64::MAX,
        previous_event_hash: [0xCD; 32],
    }
}

fn a_vote() -> TimeoutVote {
    TimeoutVote {
        subject_sequence: u64::MAX,
        subject_seat: MAX_SEATS - 1,
        subject_event_type: u16::MAX,
        parent_event_hash: [0xEF; 32],
        deadline_ms: u32::MAX,
        kind: 2,
    }
}

fn canonical<T: Encode<()>>(v: &T) -> Vec<u8> {
    let mut out = Vec::new();
    minicbor::encode(v, &mut out).expect("the value encodes");
    out
}

/// One sealed vote, the way `cast_votes` produces it: a whole signed event.
fn sealed_vote(seed: u8) -> Vec<u8> {
    chained::seal(
        EventType::TimeoutVote,
        &slot(),
        &a_vote(),
        &key(seed),
        u64::MAX,
        u32::MAX,
        TIMEOUT_VOTE_CAP,
    )
    .expect("a vote is inside its own cap")
}

#[test]
fn a_single_vote_fits_the_vote_cap() {
    let one = sealed_vote(1);
    // Not the cap itself: the cap bounds the *payload*, and this is the sealed
    // event around it. The number is here so the failure message below can say
    // what a vote actually costs.
    assert!(
        one.len() > 0,
        "a sealed vote must encode to something"
    );
    assert!(
        canonical(&a_vote()).len() <= TIMEOUT_VOTE_CAP,
        "a vote body of {} B is over TIMEOUT_VOTE_CAP = {} B",
        canonical(&a_vote()).len(),
        TIMEOUT_VOTE_CAP,
    );
}

#[test]
fn a_certificate_carrying_max_seats_minus_one_votes_fits_its_cap() {
    let votes: Vec<Vec<u8>> = (1..MAX_SEATS).map(sealed_vote).collect();
    assert_eq!(
        votes.len(),
        usize::from(MAX_SEATS) - 1,
        "the documented maximum is MAX_SEATS - 1 votes",
    );

    let per_vote = votes[0].len();
    let cert = TimeoutCert {
        subject_digest: [0x11; 32],
        votes,
    };
    let encoded = canonical(&cert).len();

    assert!(
        encoded <= TIMEOUT_CERT_CAP,
        "a certificate at the documented maximum encodes to {encoded} B, over \
         TIMEOUT_CERT_CAP = {TIMEOUT_CERT_CAP} B. One sealed vote is {per_vote} B \
         and {} of them are carried whole. A table of {MAX_SEATS} seats therefore \
         cannot certify a timeout at all: the votes reach unanimity and the \
         certificate is refused with TooLong, so the hand never ends. Measured \
         in split163641-10, hand #4.",
        usize::from(MAX_SEATS) - 1,
    );
}

/// The same question asked of the other body whose cap is a round 4 096, and
/// whose doc likewise describes a maximum nobody ever built: *"at most
/// `MAX_SEATS` pots, each with three seat lists, plus four vectors of that
/// length"*. This one is fine, but the point of the file is that it is
/// **measured** rather than reasoned about — reasoning about it is exactly what
/// went wrong above.
#[test]
fn a_settlement_at_max_seats_fits_its_cap() {
    let seats: Vec<u8> = (0..MAX_SEATS).collect();
    let pots: Vec<PotAward> = (0..MAX_SEATS)
        .map(|_| PotAward {
            size: u64::MAX,
            eligible: seats.clone(),
            winners: seats.clone(),
            odd_chips: seats.clone(),
        })
        .collect();
    let body = HandComplete {
        pots,
        refunds: seats
            .iter()
            .map(|&seat| Refund {
                seat,
                amount: u64::MAX,
            })
            .collect(),
        deltas: vec![i64::MIN; usize::from(MAX_SEATS)],
        final_stacks: vec![u64::MAX; usize::from(MAX_SEATS)],
        busted: seats.clone(),
        state_hash: [0x22; 32],
    };

    let encoded = canonical(&body).len();
    assert!(
        encoded <= HAND_COMPLETE_CAP,
        "a settlement at {MAX_SEATS} seats encodes to {encoded} B, over \
         HAND_COMPLETE_CAP = {HAND_COMPLETE_CAP} B",
    );
}

/// `HAND_INIT` carries four per-seat vectors and its cap is `PROTOCOL.md`
/// §9.3's, so it is not free to change if it turns out to be tight. Measured at
/// `MAX_SEATS` with every number at its widest.
#[test]
fn a_hand_init_at_max_seats_fits_its_cap() {
    let body = HandInit {
        hand_id: u64::MAX,
        button_position: MAX_SEATS - 1,
        sb_position: MAX_SEATS - 1,
        bb_seat: MAX_SEATS - 1,
        level: u16::MAX,
        small_blind: u64::MAX,
        big_blind: u64::MAX,
        ante: u64::MAX,
        dealt_in: (0..MAX_SEATS).collect(),
        stacks: vec![u64::MAX; usize::from(MAX_SEATS)],
        roster_hash: [0x33; 32],
        ledger_delta: (0..MAX_SEATS).map(|s| (s, i64::MIN)).collect(),
    };
    let encoded = canonical(&body).len();
    assert!(
        encoded <= HAND_INIT_CAP,
        "a hand init at {MAX_SEATS} seats encodes to {encoded} B, over \
         HAND_INIT_CAP = {HAND_INIT_CAP} B — and that cap is PROTOCOL.md §9.3's, \
         so the fix would be a corpus change and not a constant",
    );
}

/// The reveal bodies are capped by an entry count rather than by the seat
/// count, and `DEAL_PRIVATE_CAP`'s doc says so in terms — *"stated as a number
/// here rather than derived from the seat count of whatever table happens to be
/// running"*. Measured at the entry count the doc names, since that is the
/// bound a peer can actually make this client hold.
#[test]
fn a_private_deal_at_its_documented_entry_count_fits_its_cap() {
    const ENTRIES: usize = 25;
    let body = DealPrivate {
        entries: (0..ENTRIES)
            .map(|i| RevealEntry {
                deck_index: i as u8,
                token: vec![0x44; 33],
                proof: vec![0x55; 98],
            })
            .collect(),
    };
    let encoded = canonical(&body).len();
    assert!(
        encoded <= DEAL_PRIVATE_CAP,
        "a private deal of {ENTRIES} entries encodes to {encoded} B, over \
         DEAL_PRIVATE_CAP = {DEAL_PRIVATE_CAP} B",
    );
}
