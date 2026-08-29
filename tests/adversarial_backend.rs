//! The review's successful attacks, run against the **real** backend.
//!
//! `ZIFFLE_VERDICT.md` condition **C-15**. `tests/adversarial.rs` runs the same
//! family of attacks against a mock whose argument accepts everything, which is
//! the right way to prove that the *structural* rules bite on their own. It
//! proves nothing about the shipped code, because a mock has no cryptography to
//! get wrong.
//!
//! This file is the other half: the same attacks against
//! [`HandDeck`](p2p_poker::mental_poker::backend::HandDeck) and the vendored,
//! forked `ziffle`. Every case here is one the review executed against the real
//! crate — `docs/research/ZIFFLE_ATTACKS.md` calls them B1 to B6 — so what is
//! asserted is that our wrapper closes them, not that they were imaginary.
//!
//! # What is not here, and why
//!
//! **B3, the shared re-masking scalar**, is not a wrapper-level test. The attack
//! is a shuffler choosing one scalar for every card, which preserves ciphertext
//! differences and lets an observer recover the whole permutation from public
//! data; the review recovered all 52 positions. Reproducing it needs a *patched
//! shuffler*, because the honest one draws a fresh scalar per card and there is
//! no API to make it do otherwise.
//!
//! It is stated rather than silently omitted, because the reason it is missing
//! matters: our defence against B3 is
//! [`structural_check`](p2p_poker::mental_poker::protocol::structural_check)'s
//! pairwise-distinctness rule, which **is** asserted — in `adversarial.rs`
//! against a hand-built colliding deck, where the collision can actually be
//! constructed. The mock is the only place that attack can be written down.

use std::sync::Arc;

use p2p_poker::mental_poker::backend::{
    DeckParams, HandDeck, VerifiedKey, WireKey, DECK,
};
use p2p_poker::mental_poker::protocol::{
    Ciphertext, CtxFields, DeckCrypto, DeckCtx, DeckWire, DecodeError, InvalidReason,
    ProofPosition, Unavailable, VerifyOutcome,
};
use p2p_poker::mental_poker::shuffle::{ChainParams, ShuffleChain, StepError};

fn ctx_for(round: Option<u8>, sender: u8, hand: u64) -> DeckCtx {
    DeckCtx::build(&CtxFields {
        protocol_version: 1,
        table_id: [1u8; 32],
        session_id: [2u8; 32],
        hand_id: hand,
        sequence: 0,
        position: match round {
            Some(r) => ProofPosition::shuffle_step(r).unwrap(),
            None => ProofPosition::NotAShuffleStep,
        },
        sender_public_key: [sender; 32],
    })
}

fn setup(seats: usize) -> (Arc<DeckParams>, Vec<VerifiedKey>, HandDeck) {
    let params = DeckParams::new();
    let key_ctx = ctx_for(None, 0, 1);
    let mut seated: Vec<VerifiedKey> = Vec::new();
    for _ in 0..seats {
        let (_sk, pk, proof) = params.keygen(&key_ctx);
        seated.push(
            HandDeck::verify_key(pk, &proof, &seated, &key_ctx).expect("an honest key verifies"),
        );
    }
    let hand = HandDeck::new(Arc::clone(&params), &seated);
    (params, seated, hand)
}

fn chain(len: usize) -> ShuffleChain {
    ShuffleChain::open(
        ChainParams {
            protocol_version: 1,
            table_id: [1u8; 32],
            session_id: [2u8; 32],
            hand_id: 1,
        },
        (0..len as u8).collect(),
        (0..len as u8).map(|s| [s; 32]).collect(),
    )
    .unwrap()
}

/// The identity, as `tests/deck_constants.rs` measures arkworks to encode it.
const IDENTITY: [u8; 33] = {
    let mut b = [0u8; 33];
    b[32] = 0x40;
    b
};

// ---------------------------------------------------------------------------
// B5 — the identity public key
// ---------------------------------------------------------------------------

/// `pk = identity` passes `OwnershipProof::verify` with a proof anyone can
/// write, because the identity is the point whose discrete log everybody knows.
/// A seat holding it contributes nothing to the aggregate key, and the remaining
/// seats can decrypt the deck between them.
///
/// The refusal has to come **before** the ownership proof is checked (C-4): the
/// question is not whether the sender knows the secret, it is whether a
/// non-degenerate key was presented at all.
#[test]
fn b5_the_identity_key_is_refused_before_its_proof_is_examined() {
    let params = DeckParams::new();
    let key_ctx = ctx_for(None, 0, 1);

    // A real proof from a real keypair, so the only thing wrong is the key.
    let (_sk, _pk, honest_proof) = params.keygen(&key_ctx);

    let identity = WireKey::decode(&IDENTITY).expect("the identity is a valid curve point");
    assert_eq!(
        HandDeck::verify_key(identity, &honest_proof, &[], &key_ctx).err(),
        Some(VerifyOutcome::Invalid(InvalidReason::IdentityPublicKey {
            seat: 0
        }))
    );
}

/// And two seats presenting one key is refused too — not because it needs two
/// secrets, but because it needs *one*: it is one player holding two seats, who
/// already has both shares and has quietly made the threshold `n - 1`.
#[test]
fn b5b_two_seats_cannot_present_one_key() {
    let params = DeckParams::new();
    let key_ctx = ctx_for(None, 0, 1);
    let (_sk, pk, proof) = params.keygen(&key_ctx);

    let first = HandDeck::verify_key(pk, &proof, &[], &key_ctx).expect("the first is fine");
    assert_eq!(
        HandDeck::verify_key(pk, &proof, &[first], &key_ctx).err(),
        Some(VerifyOutcome::Invalid(InvalidReason::DuplicatePublicKey {
            first: 0,
            second: 1
        }))
    );
}

// ---------------------------------------------------------------------------
// B2 — a shuffle that leaves cards face up
// ---------------------------------------------------------------------------

/// Bayer–Groth constrains the re-masking scalars not at all, so a shuffler may
/// leave chosen slots in the clear and the argument still verifies. The review
/// executed this.
///
/// **What this test proves, and what it does not.** The deck here is tampered
/// with *after* the proof was made, so the argument would have caught it too —
/// the real B2 needs a shuffler that chooses the scalar zero for a slot, and
/// there is no API to make the honest one do that, exactly as with B3 above.
///
/// So what is asserted here is the **ordering**: the deck is refused by the
/// structural check rather than by the argument, in microseconds instead of the
/// ~40 ms a verification costs on this machine. That is the property that
/// matters at scale, because a defence that only worked after the expensive step
/// would be a denial-of-service surface on every client at the table.
///
/// The other half — that the structural check catches a face-up card the
/// argument accepts — is in `adversarial.rs`, against a mock whose argument
/// accepts everything. That is the only place the attack can be written down,
/// and between the two files both halves are covered.
///
/// Verified to bite: with the identity check disabled this fails with
/// `ArgumentFailed` in place of `IdentityCiphertext`.
#[test]
fn b2_a_deck_with_a_card_left_face_up_is_refused_cheaply() {
    let (_params, _seats, hand) = setup(2);
    let ctx = ctx_for(Some(0), 0, 1);

    let (mut deck, proof) = hand.shuffle(None, &ctx).expect("an honest shuffle");
    assert!(hand.verify_initial_argument(&deck, &proof, &ctx).is_ok());

    // Position 7 is put back in the clear: c1 = identity, so c2 is the card.
    deck[7][..33].copy_from_slice(&IDENTITY);

    let started = std::time::Instant::now();
    let outcome = hand.verify_initial_shuffle(&deck, &proof, &ctx);
    let took = started.elapsed();

    assert_eq!(
        outcome,
        Err(VerifyOutcome::Invalid(InvalidReason::IdentityCiphertext {
            position: 7
        }))
    );
    assert!(
        took < std::time::Duration::from_millis(5),
        "the structural check must run before the argument, and it took {took:?}"
    );
}

/// The same for a deck shuffled to itself, which is the identity permutation and
/// which the argument accepts as a perfectly valid shuffle.
#[test]
fn b2b_a_deck_shuffled_to_itself_is_refused() {
    let (_params, _seats, hand) = setup(2);
    let mut chain = chain(2);

    let c0 = chain.next_ctx(0).unwrap();
    let (deck, proof) = hand.shuffle(None, &c0).unwrap();
    chain.accept_step(&hand, 0, deck.clone(), &proof, 0).unwrap();

    let c1 = chain.next_ctx(1).unwrap();
    let (_next, next_proof) = hand.shuffle(chain.last_verified(), &c1).unwrap();

    // Seat 1 returns what it was given, with a proof of something else.
    assert_eq!(
        chain.accept_step(&hand, 1, deck, &next_proof, 1),
        Err(StepError::Rejected(VerifyOutcome::Invalid(
            InvalidReason::DeckUnchanged
        )))
    );
}

// ---------------------------------------------------------------------------
// B4 — a proof with more than one encoding
// ---------------------------------------------------------------------------

/// The deserialiser ignores trailing bytes, so without a length gate a proof and
/// that proof with garbage appended are one object to the library and two to
/// anything that hashes, deduplicates or identifies messages. The review counted
/// 2^66 accepted encodings of one proof.
#[test]
fn b4_a_proof_has_exactly_one_accepted_encoding() {
    let (_params, _seats, hand) = setup(2);
    let ctx = ctx_for(Some(0), 0, 1);
    let (deck, proof) = hand.shuffle(None, &ctx).unwrap();

    assert!(hand.verify_initial_argument(&deck, &proof, &ctx).is_ok());

    let mut padded = proof.clone();
    padded.push(0x00);
    assert!(
        matches!(
            hand.verify_initial_argument(&deck, &padded, &ctx),
            Err(VerifyOutcome::Invalid(InvalidReason::Decode(
                DecodeError::WrongLength { .. }
            )))
        ),
        "one extra byte is a different message and must not be the same proof"
    );

    let mut truncated = proof.clone();
    truncated.pop();
    assert!(matches!(
        hand.verify_initial_argument(&deck, &truncated, &ctx),
        Err(VerifyOutcome::Invalid(InvalidReason::Decode(
            DecodeError::WrongLength { .. }
        )))
    ));
}

/// And a deck that is not 52 cards is refused on its length, before any loop
/// runs over a number the sender chose.
#[test]
fn b4b_a_deck_of_the_wrong_length_is_refused_first() {
    let (_params, _seats, hand) = setup(2);
    let ctx = ctx_for(Some(0), 0, 1);
    let (deck, proof) = hand.shuffle(None, &ctx).unwrap();

    let short: Vec<Ciphertext> = deck[..10].to_vec();
    assert_eq!(
        hand.verify_initial_shuffle(&short, &proof, &ctx),
        Err(VerifyOutcome::Invalid(InvalidReason::WrongDeckLength {
            expected: DECK,
            got: 10
        }))
    );

    let mut long = deck.clone();
    long.extend_from_slice(&deck[..5]);
    assert_eq!(
        hand.verify_initial_shuffle(&long, &proof, &ctx),
        Err(VerifyOutcome::Invalid(InvalidReason::WrongDeckLength {
            expected: DECK,
            got: 57
        }))
    );
}

// ---------------------------------------------------------------------------
// B6 — replaying somebody else's proof
// ---------------------------------------------------------------------------

/// A valid proof carried into another position, another sender, or another hand.
/// The context of `PROTOCOL.md` §4.5 binds all three, and this is the test that
/// section asks for by name.
#[test]
fn b6_a_valid_proof_does_not_transfer() {
    let (_params, _seats, hand) = setup(2);
    let mine = ctx_for(Some(0), 0, 1);
    let (deck, proof) = hand.shuffle(None, &mine).unwrap();
    assert!(hand.verify_initial_argument(&deck, &proof, &mine).is_ok());

    for (what, other) in [
        ("another position in the chain", ctx_for(Some(1), 0, 1)),
        ("another sender", ctx_for(Some(0), 9, 1)),
        ("another hand", ctx_for(Some(0), 0, 2)),
        ("a reveal rather than a shuffle", ctx_for(None, 0, 1)),
    ] {
        assert_eq!(
            hand.verify_initial_argument(&deck, &proof, &other),
            Err(VerifyOutcome::Invalid(InvalidReason::ArgumentFailed)),
            "the proof transferred to {what}"
        );
    }
}

/// The chain refuses the replay a step earlier still: seat 1 cannot present
/// seat 0's step, because step `k` is only accepted from the seat that owns
/// position `k`. Two independent defences, which is deliberate — the context
/// binding is cryptographic and this one is free.
#[test]
fn b6b_the_chain_refuses_a_replay_before_any_cryptography_runs() {
    let (_params, _seats, hand) = setup(3);
    let mut c = chain(3);

    let (deck, proof) = hand.shuffle(None, &ctx_for(Some(0), 0, 1)).unwrap();
    let started = std::time::Instant::now();
    let refused = c.accept_step(&hand, 2, deck, &proof, 0);
    let took = started.elapsed();

    assert_eq!(
        refused,
        Err(StepError::NotYourTurn {
            expected: 0,
            got: 2
        })
    );
    assert!(
        took < std::time::Duration::from_millis(5),
        "out of turn is answered before the argument, and it took {took:?}"
    );
}

// ---------------------------------------------------------------------------
// B1 — a reveal token that opens a card it was not issued for
// ---------------------------------------------------------------------------

/// The token bound `c1` and dropped `c2`, so a token issued for one card was
/// valid for every card sharing that first coordinate — and a malicious shuffler
/// arranges the collision by reusing a re-masking scalar. The review executed it
/// end to end at 52 cards.
///
/// The fix is `FORK(b)` in the vendored library, and its own regression test
/// lives there, built on a hand-made collision. What is asserted here is the
/// property a player cares about: a token for one card does not verify against
/// another. Two cards of a real shuffled deck differ in **both** coordinates, so
/// this does not reproduce the original defect — the fork's test is what does —
/// but it is the check the game actually performs, and it must hold.
#[test]
fn b1_a_token_for_one_card_does_not_verify_against_another() {
    let params = DeckParams::new();
    let key_ctx = ctx_for(None, 0, 1);
    let (sk, pk, kp) = params.keygen(&key_ctx);
    let seated = vec![HandDeck::verify_key(pk, &kp, &[], &key_ctx).unwrap()];
    let hand = HandDeck::new(Arc::clone(&params), &seated);

    let mut c = ShuffleChain::open(
        ChainParams {
            protocol_version: 1,
            table_id: [1u8; 32],
            session_id: [2u8; 32],
            hand_id: 1,
        },
        vec![0, 1],
        vec![[0u8; 32], [1u8; 32]],
    )
    .unwrap();

    let c0 = c.next_ctx(0).unwrap();
    let (d0, p0) = hand.shuffle(None, &c0).unwrap();
    c.accept_step(&hand, 0, d0, &p0, 0).unwrap();
    let c1 = c.next_ctx(1).unwrap();
    let (d1, p1) = hand.shuffle(c.last_verified(), &c1).unwrap();
    c.accept_step(&hand, 1, d1, &p1, 1).unwrap();
    let deck = c.finish().unwrap();

    let reveal = ctx_for(None, 5, 1);
    let victim = p2p_poker::mental_poker::deck::DeckIndexMap::build(&[true, true], 0, DECK)
        .unwrap()
        .hole_cards(1)
        .unwrap()[0];
    let mine = p2p_poker::mental_poker::deck::DeckIndexMap::build(&[true, true], 0, DECK)
        .unwrap()
        .hole_cards(0)
        .unwrap()[0];

    let (token, proof) = hand.token(&sk, &seated[0], &deck, mine, &reveal).unwrap();

    // Against the card it was issued for: fine.
    assert!(hand
        .verify_token(&seated[0], &deck, mine, token, &proof, &reveal)
        .is_ok());

    // Against anybody else's: not fine.
    assert_eq!(
        hand.verify_token(&seated[0], &deck, victim, token, &proof, &reveal)
            .err(),
        Some(VerifyOutcome::Invalid(InvalidReason::ArgumentFailed)),
        "a share of one card must not open another"
    );

    // And not under a different context either.
    assert_eq!(
        hand.verify_token(&seated[0], &deck, mine, token, &proof, &ctx_for(None, 6, 1))
            .err(),
        Some(VerifyOutcome::Invalid(InvalidReason::ArgumentFailed))
    );
}

// ---------------------------------------------------------------------------
// The distinction the whole anti-cheat rests on
// ---------------------------------------------------------------------------

/// A peer that is merely behind is not a peer that cheated. Every refusal above
/// is `Invalid` and is admissible under D-014; this one is `CouldNotVerify` and
/// is admissible against nobody, and the difference is a type rather than a
/// convention.
#[test]
fn a_check_that_did_not_run_is_not_evidence() {
    let (_params, _seats, hand) = setup(2);
    let (a, proof) = hand.shuffle(None, &ctx_for(Some(0), 0, 1)).unwrap();
    let (b, _) = hand.shuffle(None, &ctx_for(Some(1), 1, 1)).unwrap();

    // `a` was produced but never verified here, so it is not an input this peer
    // established, and no answer about it is available.
    match hand.verify_shuffle(&a, &b, &proof, &ctx_for(Some(1), 1, 1)) {
        Err(VerifyOutcome::CouldNotVerify(Unavailable::InputDeckUnknown)) => {}
        other => panic!("expected an unanswerable check, got {other:?}"),
    }
}
