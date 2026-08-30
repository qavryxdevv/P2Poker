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
    // Exactly the two certificates about this one subject — this client's own
    // and its neighbour's — and nothing else has reached the set.
    for s in 0..2usize {
        assert_eq!(
            t.hands[s].verified_certificates(),
            2,
            "seat {s} did not record both certificates it verified"
        );
    }
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


/// **An abort may name a player only against a certificate this client
/// verified itself.**
///
/// `certs` is the whole of that check, and it is what stops a one-message hand
/// void: one peer ending a hand and naming somebody, on its own word, with a
/// thirty-two byte string it invented. The set must therefore contain nothing
/// but `TIMEOUT_CERT` hashes this client opened, checked vote by vote against
/// the voter set, and found unanimous.
///
/// This exists because a bad edit put the `certs.insert` into `on_hand_init`
/// instead of `on_timeout_cert`, which filled the set with **every `HAND_INIT`
/// hash of the hand** — a value every peer at the table holds and can quote —
/// and left the certificate path inserting nothing at all. The gate was open to
/// anyone and shut to the mechanism it was written for, and the end-to-end
/// tests did not notice: both survivors abort on their own completed
/// certificate, so neither ever needs to accept the other's abort.
#[test]
fn an_abort_naming_a_player_is_refused_without_a_certificate_this_client_verified() {
    use p2p_poker::net::chained;
    use p2p_poker::protocol::messages::EventType;
    use p2p_poker::table::hand::HAND_ABORT_CAP;
    use p2p_poker::table::handwire::HandAbort;

    let (mut t, opening) = Table::open_with_seat_two_silent();
    t.settle(opening, NOW);
    assert!(t.hands[0].aborted().is_none());

    // **The invariant, held directly.** Nothing has been certified, so the set
    // an abort is checked against must be empty. The bad edit made this two:
    // one `HAND_INIT` hash per surviving seat.
    for s in 0..2usize {
        assert_eq!(
            t.hands[s].verified_certificates(),
            0,
            "seat {s} would accept a named abort quoting something it never verified"
        );
    }

    // Seat 1 names seat 2 and quotes a hash seat 0 has never verified as a
    // certificate. Every field but `cert_hash` is what an honest abort carries.
    let mut body = HandAbort::on_deadline(t.hands[1].stacks());
    body.attributed = vec![key(12).verifying_key().to_bytes()];

    for (what, forged) in [
        ("a hash out of thin air", [0x5a; 32]),
        // The shape the bad edit actually admitted: a hash every peer at the
        // table holds and can quote, rather than one only a verifier has.
        ("a hash from the hand's own chain", t.hands[1].slot().previous_event_hash),
    ] {
        body.cert_hash = Some(forged);
        let bytes = chained::seal(
            EventType::HandAbort,
            &t.hands[1].slot(),
            &body,
            &t.keys[1],
            LATE,
            // `hand_delay_ms + crypto_step_timeout_ms`, PROTOCOL.md section 8.2.
            7_000 + 30_000,
            HAND_ABORT_CAP,
        )
        .expect("the abort seals");

        let outcome = t.hands[0].on_event(&bytes, &t.keys[0], LATE);
        assert!(
            t.hands[0].aborted().is_none(),
            "seat 0 ended the hand on {what}: {outcome:?}"
        );
    }
}


/// **A hand is decided or aborted, never both** (`PROTOCOL.md` section 4.10).
///
/// A receiver holding a complete `HAND_COMPLETE` stage must discard every
/// `HAND_ABORT` for that hand. Without the guard the abort was *applied*:
/// `HandAbort::consistent` compares `final_stacks` against the hand's
/// **start**-of-hand stacks, which do not move at settlement, so a late abort
/// passed every check and reverted a hand that had already paid out — on that
/// one receiver, forking it from the table for every hand after.
#[test]
fn a_settled_hand_discards_a_late_abort() {
    use p2p_poker::net::chained;
    use p2p_poker::poker::actions::Action;
    use p2p_poker::protocol::messages::EventType;
    use p2p_poker::table::hand::HAND_ABORT_CAP;
    use p2p_poker::table::handwire::HandAbort;

    let (mut t, opening) = Live::open(0);
    t.settle(opening);

    // Everybody folds to one player, which settles the hand.
    for _ in 0..12 {
        let owed = t.hands[0].waiting_for();
        let Some(&seat) = owed.first() else { break };
        let s = usize::from(seat);
        let Ok(out) = t.hands[s].act(Action::Fold, &t.keys[s], NOW) else {
            break;
        };
        t.settle(out.into_iter().map(|Send::Broadcast(b)| b).collect());
        if t.hands[0].betting_over() {
            break;
        }
    }
    assert!(
        t.hands[0].betting_over(),
        "the hand did not settle, so this test proves nothing"
    );
    let settled: Vec<Vec<u64>> = (0..3).map(|s| t.hands[s].stacks()).collect();

    // A late abort, correct in every field, from a seat that is at the table.
    // `on_deadline` is cause 1 with nobody named — the hand-deadline abort,
    // the one shape a receiver accepts on its own expired clock.
    //
    // **The hand's START stacks, not the settled ones.** `HandAbort::consistent`
    // compares `final_stacks` against `self.mine.stacks`, which is where the
    // hand began and does not move at settlement, so those are the numbers an
    // abort has to carry to be accepted at all — and carrying them is exactly
    // what reverts the payout. Built with the settled stacks the first time,
    // this test passed with the guard removed and proved nothing.
    let body = HandAbort::on_deadline(vec![10_000; 3]);
    let bytes = chained::seal(
        EventType::HandAbort,
        &t.hands[1].slot(),
        &body,
        &t.keys[1],
        NOW + 600_000,
        7_000 + 30_000,
        HAND_ABORT_CAP,
    )
    .expect("the abort seals");

    let _ = t.hands[0].on_event(&bytes, &t.keys[0], NOW + 600_000);
    assert!(
        t.hands[0].aborted().is_none(),
        "a settled hand was reopened by an abort that arrived after it"
    );
    assert_eq!(
        t.hands[0].stacks(),
        settled[0],
        "the settlement was reverted to the hand's starting stacks"
    );
}


/// Three seats, and from the `cut`th delivery onward **seat 2 is heard by
/// nobody**. It keeps emitting and keeps advancing on its own; the other two
/// stop where they last needed it. That is a peer whose last broadcast reached
/// the network and no listener, which is what the measured run actually was.
///
/// `None` when that cut does not produce the divergence — the point at which a
/// seat owes something is a property of the deal, not a number worth asserting.
fn diverged(cut: usize) -> Option<(Vec<Hand>, Vec<SigningKey>)> {
    let keys: Vec<SigningKey> = (0..3u8).map(|s| key(10 + s)).collect();
    let mut hands: Vec<Hand> = Vec::new();
    let mut queue: Vec<(usize, Vec<u8>)> = Vec::new();
    for s in 0..3u8 {
        let (h, out) =
            Hand::open(opening3(s), &keys[usize::from(s)], NOW, 30_000).expect("the hand opens");
        hands.push(h);
        for Send::Broadcast(b) in out {
            queue.push((usize::from(s), b));
        }
    }

    let mut delivered = 0usize;
    for _ in 0..400 {
        if queue.is_empty() {
            break;
        }
        for (from, bytes) in std::mem::take(&mut queue) {
            for to in 0..3usize {
                if to == from {
                    continue;
                }
                if from == 2 && delivered > cut {
                    continue;
                }
                delivered += 1;
                match hands[to].on_event(&bytes, &keys[to], NOW) {
                    Ok(out) => {
                        for Send::Broadcast(b) in out {
                            queue.push((to, b));
                        }
                    }
                    Err(Failed::NotYet) => hands[to].hold(bytes.clone()),
                    Err(_) => {}
                }
            }
        }
    }

    let ahead = hands[2].slot().sequence > hands[0].slot().sequence;
    let owed = hands[0].waiting_for().contains(&2) && hands[1].waiting_for().contains(&2);
    let together = hands[0].slot().sequence == hands[1].slot().sequence;
    (ahead && owed && together).then_some((hands, keys))
}

/// **The certificate its own subject can apply.**
///
/// This is the divergence the whole position-free path exists for, and it was
/// measured over the network before it was written down: seat 0 emitted the
/// event the others were waiting on, they never received it, they voted,
/// certified and ended the hand — and seat 0, which had moved past that stage
/// *because* it did the thing they never saw, could not rebuild the subject
/// from its own cursor and held their certificate for ever. The two then ran
/// hands two to six with different participation sets under identical genesis
/// hashes, and nothing was refused anywhere.
///
/// The peer least able to be standing at the stage a certificate is about is
/// the subject of it. That is not a corner: it is the ordinary shape of the
/// event.
#[test]
fn the_subject_applies_a_certificate_about_a_stage_it_has_left() {
    const LATE_ENOUGH: u64 = NOW + 600_000;

    let Some((mut hands, keys)) = (4..40).find_map(diverged) else {
        panic!("no cut point left seat 2 ahead of a table still waiting for it");
    };

    // The two that can still hear each other vote, agree and certify — and
    // everything they say is offered to the subject as well.
    let mut appeal: Vec<Vec<u8>> = Vec::new();
    for s in 0..2usize {
        for Send::Broadcast(b) in hands[s]
            .vote_on_timeouts(&keys[s], LATE_ENOUGH)
            .expect("a vote is sealed")
        {
            appeal.push(b);
        }
    }
    for _ in 0..10 {
        if appeal.is_empty() {
            break;
        }
        for bytes in std::mem::take(&mut appeal) {
            let _ = hands[2].on_event(&bytes, &keys[2], LATE_ENOUGH);
            for to in 0..2usize {
                if let Ok(out) = hands[to].on_event(&bytes, &keys[to], LATE_ENOUGH) {
                    for Send::Broadcast(b) in out {
                        appeal.push(b);
                    }
                }
            }
        }
    }

    for s in 0..2usize {
        assert!(
            hands[s].aborted().is_some(),
            "seat {s} never ended the hand it certified"
        );
    }

    // **The point.** The subject was never at the stage it was certified for,
    // and it still applied the certificate: it recorded it, it ended the hand,
    // and it agrees with the others about who plays the next one.
    assert!(
        hands[2].verified_certificates() >= 1,
        "the subject verified no certificate about itself"
    );
    assert!(
        hands[2].aborted().is_some(),
        "the subject is still playing a hand the table has ended"
    );

    let next: Vec<_> = (0..3usize)
        .map(|s| hands[s].next_hand().expect("a next hand"))
        .collect();
    for s in 1..3usize {
        assert_eq!(
            next[0].required, next[s].required,
            "seat {s} derives a different required set from seat 0"
        );
        assert_eq!(
            next[0].genesis, next[s].genesis,
            "seat {s} derives a different genesis from seat 0"
        );
    }
    assert!(
        !next[0].required.contains(&2),
        "the certified seat is still required: {:?}",
        next[0].required
    );
}


/// **`HAND_COMPLETE` wins over an abort, and the peer that gave up follows.**
///
/// `PROTOCOL.md` section 4.10: *a hand is decided or aborted, never both; a
/// receiver that applied an abort and later accepts a complete `HAND_COMPLETE`
/// stage for the same hand replaces its terminal with that stage's
/// `stage_hash`*. `HAND_COMPLETE` wins because it is collective — a completing
/// one proves no seat was silent, which is the premise every abort rests on.
///
/// It is not a nicety. `GENESIS(k+1)` depends on `TERMINAL(k)`, so a peer
/// keeping `abort_terminal(k)` while the others keep the settlement's hash
/// could never speak to them again. The race is narrow and entirely
/// legitimate: one peer's own deadline expiring between its own
/// `HAND_COMPLETE` emission and the arrival of the last other copy — which is
/// why the stage that peer had open has to be carried across the abort, or the
/// rule is unreachable in the one case it is written for.
#[test]
fn a_settlement_that_arrives_after_an_abort_replaces_its_terminal() {
    use p2p_poker::poker::actions::Action;
    use p2p_poker::protocol::messages::EventType;
    use p2p_poker::table::hand::Abort;

    const LATE: u64 = NOW + 600_000;

    fn is_settlement(bytes: &[u8]) -> bool {
        matches!(
            p2p_poker::net::chained::peek(bytes, 16_384),
            Ok((EventType::HandComplete, _, _))
        )
    }

    let (mut t, opening) = Live::open(0);

    // **One rule for the whole hand: seat 2 never hears anybody's settlement.**
    // Everything else reaches everyone, so seat 2 plays the hand out and
    // publishes a settlement of its own — and its stage stays one short, which
    // is the state section 4.10's race leaves a peer in.
    let mut held: Vec<Vec<u8>> = Vec::new();
    let mut queue = opening;
    for _ in 0..600 {
        if queue.is_empty() {
            // Nothing left to deliver: somebody has to act.
            let owed = t.hands[0].waiting_for();
            let Some(&seat) = owed.first() else { break };
            let s = usize::from(seat);
            let Ok(out) = t.hands[s].act(Action::Fold, &t.keys[s], NOW) else {
                break;
            };
            queue = out.into_iter().map(|Send::Broadcast(b)| b).collect();
            if queue.is_empty() {
                break;
            }
        }
        for bytes in std::mem::take(&mut queue) {
            if is_settlement(&bytes) {
                held.push(bytes.clone());
            }
            for to in 0..3usize {
                if to == 2 && is_settlement(&bytes) {
                    continue;
                }
                match t.hands[to].on_event(&bytes, &t.keys[to], NOW) {
                    Ok(out) => {
                        for Send::Broadcast(b) in out {
                            queue.push(b);
                        }
                    }
                    Err(Failed::NotYet) => t.hands[to].hold(bytes.clone()),
                    Err(_) => {}
                }
            }
        }
        if t.hands[0].betting_over() && t.hands[1].betting_over() {
            break;
        }
    }

    assert!(
        t.hands[0].betting_over() && t.hands[1].betting_over(),
        "seats 0 and 1 were meant to settle"
    );
    assert!(
        !t.hands[2].betting_over(),
        "seat 2 was meant to be one settlement short, not settled"
    );
    assert!(
        held.len() >= 2,
        "the settlements the other seats published were not captured"
    );

    // Seat 2's own deadline expires in that gap.
    let own: Vec<Vec<u8>> = t.hands[2]
        .abort_now(Abort::Deadline, &t.keys[2], LATE)
        .expect("the abort is sealed")
        .into_iter()
        .map(|Send::Broadcast(b)| b)
        .collect();
    assert!(t.hands[2].aborted().is_some(), "seat 2 did not give the hand up");

    // Seats 0 and 1 have settled, so they discard it. That is the first half of
    // the rule and has its own test; asserted here so this one cannot pass by
    // the whole table aborting together.
    for to in 0..2usize {
        for bytes in &own {
            let _ = t.hands[to].on_event(bytes, &t.keys[to], LATE);
        }
        assert!(
            t.hands[to].aborted().is_none(),
            "seat {to} reopened a hand it had settled"
        );
    }

    // And now the settlements it never heard reach it.
    for bytes in &held {
        let _ = t.hands[2].on_event(bytes, &t.keys[2], LATE);
    }

    // **The point.** All three open the next hand at one genesis and one
    // roster hash — so seat 2 took the settlement's terminal and the settled
    // stacks, rather than the abort's.
    let next: Vec<_> = (0..3usize)
        .map(|s| t.hands[s].next_hand().expect("a next hand"))
        .collect();
    for s in 1..3usize {
        assert_eq!(
            next[0].genesis, next[s].genesis,
            "seat {s} derives a different genesis: it kept the abort's terminal"
        );
        assert_eq!(
            next[0].roster_hash, next[s].roster_hash,
            "seat {s} derives a different roster hash: it kept the abort's stacks"
        );
    }
}
