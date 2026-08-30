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
    opening3_with_bank(my_seat, 0)
}

fn opening3_with_bank(my_seat: u8, time_bank_ms: u32) -> Opening {
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
        time_bank_ms,
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


/// A table of three that all play, so the hand reaches a betting stage, and
/// only then loses a seat.
struct Live {
    hands: Vec<Hand>,
    keys: Vec<SigningKey>,
}

impl Live {
    fn open(time_bank_ms: u32) -> (Self, Vec<Vec<u8>>) {
        let keys: Vec<SigningKey> = (0..3u8).map(|s| key(10 + s)).collect();
        let mut hands = Vec::new();
        let mut queue = Vec::new();
        for s in 0..3u8 {
            let (h, out) = Hand::open(
                opening3_with_bank(s, time_bank_ms),
                &keys[usize::from(s)],
                NOW,
                30_000,
            )
            .expect("the hand opens");
            hands.push(h);
            for Send::Broadcast(b) in out {
                queue.push(b);
            }
        }
        (Live { hands, keys }, queue)
    }

    fn settle(&mut self, mut queue: Vec<Vec<u8>>) {
        for _ in 0..600 {
            if queue.is_empty() {
                return;
            }
            for bytes in std::mem::take(&mut queue) {
                for to in 0..3usize {
                    match self.hands[to].on_event(&bytes, &self.keys[to], NOW) {
                        Ok(out) => {
                            for Send::Broadcast(b) in out {
                                queue.push(b);
                            }
                        }
                        Err(Failed::NotYet) => self.hands[to].hold(bytes.clone()),
                        Err(_) => {}
                    }
                }
            }
        }
        panic!("the delivery loop never went quiet");
    }
}

/// **The reserve is what a peer must wait out before it accuses anybody.**
///
/// Every peer budgets the whole reserve for every other seat, because what a
/// seat has left of its own is known only to that seat. Budget less and the
/// table certifies a player who is still legitimately thinking and folds a hand
/// out from under them — which is the one outcome D-023's machinery exists to
/// make impossible.
#[test]
fn nobody_votes_a_seat_late_until_its_reserve_has_run_out() {
    const BANK: u64 = 30_000;
    let (mut t, opening) = Live::open(BANK as u32);
    t.settle(opening);

    // The hand has dealt and somebody is on the clock.
    let owed = t.hands[0].waiting_for();
    assert_eq!(
        owed.len(),
        1,
        "one seat should be on the clock at a betting stage, not {owed:?}"
    );
    let subject = owed[0];
    let survivors: Vec<usize> = (0..3usize).filter(|s| *s as u8 != subject).collect();
    assert_eq!(survivors.len(), 2, "two seats must be left to vote");

    // Past the plain action deadline, and not one voter says a word: the
    // subject is inside the reserve its table advertised.
    let plain = NOW + 20_000 + 5_000 + 1;
    for &s in &survivors {
        let out = t.hands[s]
            .vote_on_timeouts(&t.keys[s], plain)
            .expect("voting is not an error");
        assert!(
            out.is_empty(),
            "seat {s} accused seat {subject} while its reserve was still running"
        );
    }

    // Past the reserve as well, and now the appeal opens.
    let spent = NOW + 20_000 + 5_000 + BANK + 1;
    for &s in &survivors {
        let out = t.hands[s]
            .vote_on_timeouts(&t.keys[s], spent)
            .expect("voting is not an error");
        assert_eq!(
            out.len(),
            1,
            "seat {s} should vote about seat {subject} once the reserve is gone"
        );
    }
}

/// A table with no reserve must not wait for one that does not exist.
#[test]
fn a_table_without_a_reserve_votes_at_the_plain_deadline() {
    let (mut t, opening) = Live::open(0);
    t.settle(opening);
    let subject = t.hands[0].waiting_for()[0];
    let voter = (0..3usize).find(|s| *s as u8 != subject).expect("a voter");
    let out = t.hands[voter]
        .vote_on_timeouts(&t.keys[voter], NOW + 20_000 + 5_000 + 1)
        .expect("voting is not an error");
    assert_eq!(out.len(), 1, "with no reserve the plain deadline is the whole deadline");
}


/// **The certificate arrives before the vote that would have justified it.**
///
/// The mesh does not order two messages, so this ordering is ordinary weather
/// rather than an exception, and it is the one that deadlocked: a peer's
/// certificate opens the collective stage here, and the guard that stops a
/// second certificate being *started* also stopped this client contributing its
/// own copy to the stage that was already open. Both survivors then held a
/// stage neither could close, and no message was refused anywhere — one
/// reported `one is already in progress` while the other waited for it.
#[test]
fn a_peers_certificate_arriving_first_does_not_silence_this_client() {
    let (mut t, opening) = Table::open_with_seat_two_silent();
    t.settle(opening, NOW);

    // Both clocks expire, and each peer holds only its own vote.
    let mut votes = Vec::new();
    for s in 0..2usize {
        let out = t.hands[s]
            .vote_on_timeouts(&t.keys[s], LATE)
            .expect("a vote is sealed");
        assert_eq!(out.len(), 1);
        for Send::Broadcast(b) in out {
            votes.push((s, b));
        }
    }

    // Seat 0 alone completes its set, and emits a certificate.
    let (_, ref one_vote) = votes[1];
    let certs = t.hands[0]
        .on_event(one_vote, &t.keys[0], LATE)
        .expect("seat 1's vote is accepted");
    let certs: Vec<Vec<u8>> = certs.into_iter().map(|Send::Broadcast(b)| b).collect();
    assert_eq!(certs.len(), 1, "seat 0 should certify once it has both votes");

    // That certificate reaches seat 1 **before** seat 0's own vote does. Seat 1
    // cannot sign yet — it holds one vote — and must not be counted as having.
    let out = t.hands[1]
        .on_event(&certs[0], &t.keys[1], LATE)
        .expect("a certificate from a voter is accepted");
    assert!(
        out.is_empty(),
        "seat 1 signed a certificate before it held a complete set of votes"
    );
    assert!(
        t.hands[1].aborted().is_none(),
        "seat 1 ended the hand on one peer's word"
    );

    // Now the vote arrives, and seat 1 owes its copy to a stage already open.
    let (_, ref zero_vote) = votes[0];
    let mut queue: Vec<Vec<u8>> = t.hands[1]
        .on_event(zero_vote, &t.keys[1], LATE)
        .expect("seat 0's vote is accepted")
        .into_iter()
        .map(|Send::Broadcast(b)| b)
        .collect();
    assert!(
        !queue.is_empty(),
        "seat 1 stayed silent, so the stage seat 0 opened can never close"
    );

    t.settle(std::mem::take(&mut queue), LATE);
    assert!(t.refusals.is_empty(), "something was refused: {:?}", t.refusals);
    for s in 0..2usize {
        assert!(
            t.hands[s].aborted().is_some(),
            "seat {s} never ended the hand"
        );
    }
}


/// **Two survivors must open the next hand at the same table.**
///
/// `R(k+1)` and the reconnection allowance both go into hand `k+1`'s genesis,
/// so a peer that derives them differently derives a different hand and refuses
/// its neighbour's. Deriving them from `signed` — this client's own record of
/// whose events it accepted — cannot be right: a hand ended by a
/// witness-independent terminal closes at whatever each peer had reached, and
/// the tail of a hand is exactly where two honest records differ. Measured over
/// the network as hand four opening with `[0, 2]` on one survivor and
/// `[0, 1, 2]` on the other, each refusing the other with *"dealt_in differs
/// from what I derived"*.
#[test]
fn both_survivors_derive_the_same_next_hand() {
    let (mut t, opening) = Table::open_with_seat_two_silent();
    t.settle(opening, NOW);

    let mut queue = Vec::new();
    for s in 0..2usize {
        for Send::Broadcast(b) in t.hands[s]
            .vote_on_timeouts(&t.keys[s], LATE)
            .expect("a vote is sealed")
        {
            queue.push(b);
        }
    }
    t.settle(queue, LATE);
    for s in 0..2usize {
        assert!(t.hands[s].aborted().is_some(), "seat {s} never ended the hand");
    }

    // The two peers deliberately hold different records of who was heard from:
    // that is the state this is about, and it must not reach the genesis.
    let zero = t.hands[0].next_hand().expect("seat 0 opens the next hand");
    let one = t.hands[1].next_hand().expect("seat 1 opens the next hand");

    assert_eq!(zero.required, one.required, "the required sets differ");
    assert_eq!(zero.grace, one.grace, "the reconnection allowances differ");
    assert_eq!(zero.genesis, one.genesis, "the genesis differs");
    assert_eq!(zero.roster_hash, one.roster_hash, "the roster hashes differ");
    assert_eq!(zero.button, one.button, "the button differs");
    assert!(
        !zero.required.contains(&2),
        "the certified seat is still required: {:?}",
        zero.required
    );
}
