//! A seat that goes away mid-hand, and the certificate that ends the hand.
//!
//! This covers the whole appeal, end to end and in one process: two survivors
//! whose own timers expire, the votes they exchange, the certificate unanimity
//! produces, and — the part that was broken and that no unit test reached — the
//! `HAND_ABORT` that certificate produces being **accepted by the other peer**.
//!
//! The defect it exists to catch was not in any of the machinery it was written
//! for. Votes tallied, digests matched, unanimity was reached and both peers
//! emitted a certificate; then each refused the other's abort, because the
//! abort named a subject and carried no `cert_hash` and §4.10 forbids exactly
//! that shape. Everything looked right up to the last message, and the table
//! stopped dead. So this test asserts the ending, not the machinery.

use ed25519_dalek::SigningKey;
use p2p_poker::table::hand::{Failed, Hand, Opening, Send, GRACE_HANDS};

const NOW: u64 = 1_700_000_000_000;
/// Past every deadline in the opening below, so both survivors' own clocks
/// have certainly expired. §8.2 measures on each peer's own monotonic clock,
/// and here both peers share one.
const LATE: u64 = NOW + 600_000;

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn opening3(my_seat: u8) -> Opening {
    Opening {
        table_id: [1; 32],
        hand_id: 1,
        session_id: [2; 32],
        roster_hash: [3; 32],
        genesis: [4; 32],
        required: vec![0, 1, 2],
        seats: vec![
            (0, key(10).verifying_key().to_bytes(), 10_000),
            (1, key(11).verifying_key().to_bytes(), 10_000),
            (2, key(12).verifying_key().to_bytes(), 10_000),
        ],
        max_players: 3,
        small_blind: 50,
        big_blind: 100,
        level: 1,
        my_seat,
        crypto_step_timeout_ms: 30_000,
        action_timeout_ms: 20_000,
        action_grace_ms: 5_000,
        hand_delay_ms: 7_000,
        hand_deadline_ms: 600_000,
        grace: vec![GRACE_HANDS; 3],
        present_run: vec![0; 3],
        button: None,
    }
}

/// The two survivors, and everything seat 2 ever said (which is nothing).
struct Table {
    hands: Vec<Hand>,
    keys: Vec<SigningKey>,
    /// Every refusal, with the seat that refused it. A test that only checked
    /// the ending would pass on a hand that aborted for the wrong reason.
    refusals: Vec<(usize, String)>,
}

impl Table {
    /// Seat 2 is present in the roster and silent from the first message: it
    /// receives nothing and says nothing. That is a peer whose process died
    /// before it wrote its first event, and it is the one kill point that
    /// needs no magic number — the hand stops at the first stage seat 2 owes,
    /// and both survivors are at that stage by construction.
    fn open_with_seat_two_silent() -> (Self, Vec<Vec<u8>>) {
        let keys: Vec<SigningKey> = (0..3u8).map(|s| key(10 + s)).collect();
        let mut hands = Vec::new();
        let mut queue = Vec::new();
        for s in 0..2u8 {
            let (h, out) =
                Hand::open(opening3(s), &keys[usize::from(s)], NOW, 30_000).expect("the hand opens");
            hands.push(h);
            for Send::Broadcast(b) in out {
                queue.push(b);
            }
        }
        (
            Table {
                hands,
                keys,
                refusals: Vec::new(),
            },
            queue,
        )
    }

    /// Deliver until nothing is left to deliver. Both survivors hear
    /// everything either of them says, which is what a mesh that forwards
    /// does — and what this client did not do until it reported a validation
    /// verdict on every path.
    fn settle(&mut self, mut queue: Vec<Vec<u8>>, now_ms: u64) {
        for _ in 0..400 {
            if queue.is_empty() {
                return;
            }
            let batch = std::mem::take(&mut queue);
            for bytes in batch {
                for to in 0..2usize {
                    match self.hands[to].on_event(&bytes, &self.keys[to], now_ms) {
                        Ok(out) => {
                            for Send::Broadcast(b) in out {
                                queue.push(b);
                            }
                        }
                        // Its own copy coming back, or a stage it has left.
                        Err(Failed::NotYet) => self.hands[to].hold(bytes.clone()),
                        Err(e) => self.refusals.push((to, e.to_string())),
                    }
                }
            }
        }
        panic!("the delivery loop never went quiet");
    }
}

#[test]
fn a_silent_seat_is_certified_and_the_hand_ends_on_both_survivors() {
    let (mut t, opening) = Table::open_with_seat_two_silent();
    t.settle(opening, NOW);

    // The hand has stopped where seat 2 owes something, and both survivors
    // have stopped at the same place. Two peers a stage apart would name
    // different subjects and could never tally together, so this is a
    // precondition of the appeal and not decoration.
    assert_eq!(
        t.hands[0].slot().sequence,
        t.hands[1].slot().sequence,
        "the survivors stopped at different stages, so no subject they name can agree"
    );
    for s in 0..2usize {
        assert!(
            t.hands[s].waiting_for().contains(&2),
            "seat {s} is waiting for {:?}, not for the seat that went away",
            t.hands[s].waiting_for()
        );
        assert!(t.hands[s].aborted().is_none(), "seat {s} gave up too early");
    }

    // Both clocks run out. A vote is not an accusation and does nothing alone.
    let mut queue = Vec::new();
    for s in 0..2usize {
        let out = t.hands[s]
            .vote_on_timeouts(&t.keys[s], LATE)
            .expect("a vote is sealed");
        assert_eq!(out.len(), 1, "seat {s} should vote about seat 2, once");
        for Send::Broadcast(b) in out {
            queue.push(b);
        }
    }

    // The exchange, and everything it sets off: the certificate each peer
    // emits once the set is complete, and the abort the certificate produces.
    t.settle(queue, LATE);

    assert!(
        t.refusals.is_empty(),
        "the appeal was refused somewhere: {:?}",
        t.refusals
    );
    for s in 0..2usize {
        assert!(
            t.hands[s].aborted().is_some(),
            "seat {s} never ended the hand, so the certificate it helped make did nothing"
        );
    }
}

/// The certificate is what makes naming a seat legitimate, so an abort that
/// names one without it must be refused — by the same rule, from the other
/// side. Without this the fix above could be "stop checking".
#[test]
fn a_named_subject_without_its_certificate_is_refused() {
    use p2p_poker::table::handwire::HandAbort;

    let stacks = vec![10_000u64; 3];
    let mut body = HandAbort::on_deadline(stacks.clone());
    body.attributed = vec![key(12).verifying_key().to_bytes()];
    assert!(
        body.consistent(&stacks).is_err(),
        "a named subject with no certificate is one peer's word and nothing more"
    );

    body.cert_hash = Some([7; 32]);
    assert!(
        body.consistent(&stacks).is_ok(),
        "a named subject with the certificate that named it is the whole point"
    );

    // And the mirror: a certificate that names nobody says nothing.
    let mut orphan = HandAbort::on_deadline(stacks.clone());
    orphan.cert_hash = Some([7; 32]);
    assert!(orphan.consistent(&stacks).is_err());
}
