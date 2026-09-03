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

// ---------------------------------------------------------------------------
// The ending, at the seat count that broke it.
//
// The measurements above prove the bytes fit. They do not prove the table ends
// the hand, and that distinction is the whole of S1-AO: every part of the
// appeal worked in production — votes tallied, digests matched, unanimity was
// reached at 9/9 — and the table still stopped dead, because the last message
// could not be sent. So this half asserts the ending.
//
// It is also the coverage gap that let the defect through. Every test in this
// repo that opens a table opens it with three seats: `opening3` in
// `timeout_certificate.rs` and in `anti_replay_authority.rs`, and nothing else
// opens one at all. At three seats the voter set is two and a certificate
// carries two votes, so the documented maximum of nine had never been built by
// anything.
//
// **What this half does NOT do, said plainly.** It does not reproduce the
// production freeze. Its certificate is 3 454 B on the wire — see
// `the_widest_event_a_full_table_emits_is_reported`, which prints the number —
// and it therefore fits under the old 4 096 cap as well as the new one. The
// silent seat here stalls at the first cryptographic stage it owes, where the
// sequence numbers are small; in `split163641-10` it stalled at a betting stage
// after a full ten-way shuffle, and that certificate was over 4 096. Roughly
// 94 B per vote separates the two and this file does not account for it.
//
// So the regression guard is the byte measurement above, which is exact and
// worst-case: nine votes at their widest come to 4 699 B, and a cap that
// documents nine has to hold them. This half guards something different and
// worth having on its own — that a full table reaches unanimity, seals nine
// certificates, and ends the hand — which no test in the repo did before.
// ---------------------------------------------------------------------------

use p2p_poker::table::hand::{Failed, Hand, Opening, Send, GRACE_HANDS};

const NOW: u64 = 1_700_000_000_000;
/// Past the crypto-step and action deadlines, and far short of the hand
/// deadline. Both halves matter: the first makes every survivor's clock expire
/// on the silent seat, the second denies them the cause-1 abort that would end
/// the hand without a certificate at all.
const LATE: u64 = NOW + 120_000;

/// The silent one. Present in the roster, never heard from — a peer whose
/// process died before it wrote its first event.
const SILENT: u8 = 2;

fn opening_at_max_seats(my_seat: u8) -> Opening {
    let n = usize::from(MAX_SEATS);
    Opening {
        table_id: [1; 32],
        hand_id: 1,
        session_id: [2; 32],
        roster_hash: [3; 32],
        genesis: [4; 32],
        required: (0..MAX_SEATS).collect(),
        readmitted: Vec::new(),
        every_n_hands: 11,
        first_small_blind: 50,
        small_blind_cap: 50_000,
        seats: (0..MAX_SEATS)
            .map(|s| (s, key(10 + s).verifying_key().to_bytes(), 10_000))
            .collect(),
        max_players: MAX_SEATS,
        small_blind: 50,
        big_blind: 100,
        level: 1,
        my_seat,
        crypto_step_timeout_ms: 30_000,
        action_timeout_ms: 20_000,
        action_grace_ms: 5_000,
        hand_delay_ms: 7_000,
        time_bank_ms: 0,
        // **An hour, deliberately.** `LATE` is past the crypto-step and action
        // deadlines and nowhere near this one, so the plain hand-deadline abort
        // -- cause 1, which needs no certificate -- cannot end the hand. The
        // only road out is the certificate, which is the road that broke.
        hand_deadline_ms: 3_600_000,
        grace: vec![GRACE_HANDS; n],
        present_run: vec![0; n],
        button: None,
    }
}

/// The nine survivors. Seat `SILENT` has no `Hand` at all.
struct FullTable {
    hands: Vec<Hand>,
    keys: Vec<SigningKey>,
    seats: Vec<u8>,
    refusals: Vec<(u8, String)>,
    /// The largest event any seat broadcast. A certificate at a full table is
    /// the biggest thing this protocol emits, and knowing the number is what
    /// separates *"the cap holds"* from *"the cap happened to hold today"*.
    widest: usize,
}

impl FullTable {
    fn open() -> (Self, Vec<Vec<u8>>) {
        let seats: Vec<u8> = (0..MAX_SEATS).filter(|&s| s != SILENT).collect();
        let keys: Vec<SigningKey> = seats.iter().map(|&s| key(10 + s)).collect();
        let mut hands = Vec::new();
        let mut queue = Vec::new();
        for (i, &s) in seats.iter().enumerate() {
            let (h, out) = Hand::open(opening_at_max_seats(s), &keys[i], NOW, 30_000)
                .expect("the hand opens");
            hands.push(h);
            for Send::Broadcast(b) in out {
                queue.push(b);
            }
        }
        (
            FullTable {
                hands,
                keys,
                seats,
                refusals: Vec::new(),
                widest: 0,
            },
            queue,
        )
    }

    /// Deliver until nothing is left. Nine seats hearing everything the other
    /// eight say is a great deal more traffic than the three-seat harness, so
    /// the bound is generous — it is a runaway guard, not a limit on the run.
    fn settle(&mut self, mut queue: Vec<Vec<u8>>, now_ms: u64) {
        for _ in 0..20_000 {
            if queue.is_empty() {
                return;
            }
            let batch = std::mem::take(&mut queue);
            for bytes in batch {
                for to in 0..self.hands.len() {
                    match self.hands[to].on_event(&bytes, &self.keys[to], now_ms) {
                        Ok(out) => {
                            for Send::Broadcast(b) in out {
                                self.widest = self.widest.max(b.len());
                                queue.push(b);
                            }
                        }
                        Err(Failed::NotYet) => {
                            let _ = self.hands[to].hold(bytes.clone());
                        }
                        Err(e) => self.refusals.push((self.seats[to], e.to_string())),
                    }
                }
            }
        }
        panic!("the delivery loop never went quiet");
    }

    /// Every survivor's clock runs out. A vote is not an accusation and does
    /// nothing alone; what it sets off is the certificate, and the abort the
    /// certificate produces.
    fn run_the_clocks_out(&mut self, now_ms: u64) {
        let mut queue = Vec::new();
        for i in 0..self.hands.len() {
            if let Ok(out) = self.hands[i].vote_on_timeouts(&self.keys[i], now_ms) {
                for Send::Broadcast(b) in out {
                    self.widest = self.widest.max(b.len());
                    queue.push(b);
                }
            }
        }
        self.settle(queue, now_ms);
    }
}

#[test]
fn a_silent_seat_at_a_full_table_is_certified_and_the_hand_ends() {
    let (mut table, queue) = FullTable::open();
    table.settle(queue, NOW);

    // Every survivor's clock expires on the silent seat. Repeated, because the
    // appeal is several stages: vote, unanimity, certificate, abort.
    for _ in 0..6 {
        table.run_the_clocks_out(LATE);
    }

    // The production failure, named exactly. Before S1-AO this refusal appeared
    // on every seat once per attempt, for the rest of the run.
    let over_cap: Vec<&(u8, String)> = table
        .refusals
        .iter()
        .filter(|(_, e)| e.contains("over its cap"))
        .collect();
    assert!(
        over_cap.is_empty(),
        "a certificate at {MAX_SEATS} seats was refused for its size: {over_cap:?}. \
         This is S1-AO: the votes reach unanimity, the certificate cannot be sent, \
         and the table freezes for ever.",
    );

    // Positive, because "nothing was refused" also describes a hand that never
    // reached the certificate at all. Nine survivors each emit one about the same
    // subject, so every one of them should verify nine.
    let want = table.hands.len();
    for (i, &seat) in table.seats.iter().enumerate() {
        let got = table.hands[i].verified_certificates();
        assert_eq!(
            got, want,
            "seat {seat} verified {got} certificates, not the {want} a full table \
             produces about one silent seat",
        );
    }

    for (i, &seat) in table.seats.iter().enumerate() {
        assert!(
            table.hands[i].aborted().is_some(),
            "seat {seat} never ended the hand, so the certificate it helped make did \
             nothing. That is S1-AO's shape exactly. Refusals: {:?}",
            table.refusals,
        );
    }
}

/// What a full table actually puts on the wire, printed rather than asserted
/// away. `split163641-10` froze because a certificate would not fit and nobody
/// knew how large one was; the number belongs somewhere a reader can find it.
#[test]
fn the_widest_event_a_full_table_emits_is_reported() {
    let (mut table, queue) = FullTable::open();
    table.settle(queue, NOW);
    for _ in 0..6 {
        table.run_the_clocks_out(LATE);
    }
    println!(
        "widest event broadcast at {MAX_SEATS} seats: {} B (TIMEOUT_CERT_CAP = {})",
        table.widest, TIMEOUT_CERT_CAP,
    );
    assert!(table.widest > 0, "a full table must broadcast something");
}
