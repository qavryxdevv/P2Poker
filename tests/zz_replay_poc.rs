//! Temporary proof-of-concept: within-hand replay attempts.
use ed25519_dalek::SigningKey;
use p2p_poker::protocol::messages::{EventBody, SignedEvent};
use p2p_poker::protocol::serialization::{from_canonical, to_canonical};
use p2p_poker::table::hand::{Failed, Hand, Opening, Send, GRACE_HANDS};

const NOW: u64 = 1_700_000_000_000;

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
        time_bank_ms: 0,
        hand_deadline_ms: 600_000,
        grace: vec![GRACE_HANDS; 3],
        present_run: vec![0; 3],
        button: None,
    }
}

fn open(seat: u8) -> (Hand, Vec<u8>) {
    let (h, out) = Hand::open(opening3(seat), &key(10 + seat), NOW, 30_000).unwrap();
    let Send::Broadcast(b) = out.into_iter().next().unwrap();
    (h, b)
}

/// Re-label an observed event with a new sequence. The signature is left as it
/// was and therefore no longer covers the body.
fn relabel_sequence(bytes: &[u8], sequence: u64) -> Vec<u8> {
    let signed: SignedEvent = from_canonical(bytes, 65536).unwrap();
    let mut body: EventBody = from_canonical(&signed.body, 65536).unwrap();
    body.sequence = sequence;
    let re = to_canonical(&body).unwrap();
    to_canonical(&SignedEvent {
        body: re,
        signature: signed.signature,
    })
    .unwrap()
}

#[test]
fn poc() {
    let (mut a, a0) = open(0);
    let (mut b, b0) = open(1);
    let (_c, c0) = open(2);

    // Drive `a` to the end of stage 0 so it emits a genuine stage-1 event.
    a.on_event(&b0, &key(10), NOW).unwrap();
    let out = a.on_event(&c0, &key(10), NOW).unwrap();
    assert_eq!(a.slot().sequence, 1, "a is at stage 1");
    let Send::Broadcast(genuine_early) = out.into_iter().next().unwrap();

    // ---- A2: the early queue takes unverified bytes -------------------
    assert_eq!(b.slot().sequence, 0);
    // a genuine stage-1 event, held because b is still at stage 0
    assert!(matches!(b.on_event(&genuine_early, &key(11), NOW), Err(Failed::NotYet)));
    b.hold(genuine_early.clone());
    assert_eq!(b.held(), 1);

    // 64 re-labelled copies of a's stage-0 event. The signature is broken.
    for i in 0..64u64 {
        let forged = relabel_sequence(&a0, 1_000 + i);
        let r = b.on_event(&forged, &key(11), NOW);
        assert!(
            matches!(r, Err(Failed::NotYet)),
            "forged copy {i} was answered {r:?}, not NotYet"
        );
        b.hold(forged);
    }
    assert_eq!(b.held(), 64, "the queue is full of forgeries");

    // b now completes stage 0 for real.
    b.on_event(&a0, &key(11), NOW).unwrap();
    b.on_event(&c0, &key(11), NOW).unwrap();
    assert_eq!(b.slot().sequence, 1, "b reached stage 1");
    let (sends, failures) = b.replay_early(&key(11), NOW);
    assert!(sends.is_empty(), "the genuine stage-1 event was evicted: {sends:?}");
    assert!(failures.is_empty(), "the forgeries are never even refused: {failures:?}");
    assert_eq!(b.held(), 64, "and they stay held for the rest of the hand");

    // ---- A1: a stale genuine event replayed at a later stage ----------
    let r = b.on_event(&a0, &key(11), NOW);
    assert!(matches!(r, Ok(v) if v.is_empty()), "stale event is dropped silently");
}
