//! The adversarial suite (`SPEC_CS.md` §25, ziffle verdict condition C-15).
//!
//! The review that cleared `ziffle` for the play-money MVP ran twenty-two
//! attack families against it and found fourteen defects **around** the
//! Bayer–Groth argument rather than in it. Every one is closed by a check in
//! `src/mental_poker/protocol.rs`, and this file is what keeps those checks
//! asserted rather than remembered — a condition of the verdict, not an extra.
//!
//! # What is covered here and what is not
//!
//! The boundary exists; the library implementation behind it does not yet. So
//! the families split in two, and the split is stated rather than left for a
//! reader to infer from what is missing:
//!
//! **Covered now** — every family whose attack is stopped by the wrapper, which
//! is where the review said the fixes belong: degenerate and non-permuted
//! decks, the ciphertext collision behind the reveal-token defect, the identity
//! public key, non-canonical encodings, trailing bytes, and proof flooding.
//!
//! **Awaiting the implementation** — the families that need a real prover to
//! attempt: forging the argument itself, grinding a challenge, and replaying a
//! proof across contexts. Each is named in [`awaiting_implementation`] with the
//! condition it belongs to, so the gap is visible in the test output instead of
//! being an absence nobody notices.

use p2p_poker::mental_poker::protocol::{
    check_key_set, structural_check, Ciphertext, DeckCrypto, DeckCtx, DeckWire, DecodeError,
    InvalidReason, NotAdmitted, ShuffleAdmission, VerifyOutcome,
};

const DECK: usize = 52;

/// A plausible compressed ciphertext pair: valid tags, distinct body, no
/// infinity flag.
fn ct(seed: u8) -> Ciphertext {
    let mut c = [0u8; 66];
    c[0] = 0x02;
    c[1] = seed;
    c[2] = seed.wrapping_mul(31);
    c[33] = 0x03;
    c[34] = seed ^ 0xFF;
    c
}

fn deck(from: u8) -> Vec<Ciphertext> {
    (0..DECK as u8).map(|i| ct(from.wrapping_add(i))).collect()
}

/// An implementation whose argument always verifies.
///
/// Every attack below therefore has to be stopped by the wrapper, which is the
/// point: the review's finding was that the argument holds and its surface does
/// not, so a suite that leaned on the argument would be testing the wrong half.
struct ArgumentAlwaysAccepts;

impl DeckCrypto for ArgumentAlwaysAccepts {
    fn verify_initial_argument(
        &self,
        _next: &[Ciphertext],
        _proof: &[u8],
        _ctx: &DeckCtx,
    ) -> Result<(), VerifyOutcome> {
        Ok(())
    }

    fn verify_argument(
        &self,
        _prev: &[Ciphertext],
        _next: &[Ciphertext],
        _proof: &[u8],
        _ctx: &DeckCtx,
    ) -> Result<(), VerifyOutcome> {
        Ok(())
    }
}

fn ctx() -> DeckCtx {
    DeckCtx::from_hash([0x5A; 32])
}

// ---------------------------------------------------------------------------
// A1, A3 — a deck that is not a permutation of its input
// ---------------------------------------------------------------------------

#[test]
fn a1_a_duplicated_card_is_refused() {
    let prev = deck(0);
    let mut next = deck(100);
    // Two positions holding the same ciphertext: the shape of a deck with the
    // ace of spades twice.
    next[40] = next[7];

    assert_eq!(
        ArgumentAlwaysAccepts.verify_shuffle(&prev, &next, b"", &ctx()),
        Err(VerifyOutcome::Invalid(InvalidReason::DuplicateCiphertext {
            first: 7,
            second: 40
        }))
    );
}

#[test]
fn a3_a_substituted_card_that_reuses_a_coordinate_is_refused() {
    let prev = deck(0);
    let mut next = deck(100);
    // A card swapped for one carrying an input coordinate — trackable straight
    // through the shuffle.
    next[12] = prev[30];

    assert!(matches!(
        ArgumentAlwaysAccepts.verify_shuffle(&prev, &next, b"", &ctx()),
        Err(VerifyOutcome::Invalid(InvalidReason::CardNotRemasked { .. }))
    ));
}

#[test]
fn a_deck_of_the_wrong_size_is_refused_before_anything_else() {
    let prev = deck(0);
    let short: Vec<Ciphertext> = deck(100).into_iter().take(51).collect();

    assert_eq!(
        ArgumentAlwaysAccepts.verify_shuffle(&prev, &short, b"", &ctx()),
        Err(VerifyOutcome::Invalid(InvalidReason::WrongDeckLength {
            expected: DECK,
            got: 51
        }))
    );
}

// ---------------------------------------------------------------------------
// A16, A18 — the degenerate "shuffles" Bayer–Groth verifies quite happily
// ---------------------------------------------------------------------------

#[test]
fn a16_chosen_slots_left_in_plaintext_are_refused() {
    // The measured attack: a shuffler masks fifty cards and leaves two he wants
    // readable. The argument does not constrain the re-masking scalars at all.
    let prev = deck(0);
    for exposed in [0usize, 25, 51] {
        let mut next = deck(100);
        next[exposed] = [0u8; 66];
        assert_eq!(
            ArgumentAlwaysAccepts.verify_shuffle(&prev, &next, b"", &ctx()),
            Err(VerifyOutcome::Invalid(InvalidReason::IdentityCiphertext {
                position: exposed
            })),
            "position {exposed} was left in the clear"
        );
    }
}

#[test]
fn a16b_the_infinity_flag_is_the_other_degenerate_form() {
    let prev = deck(0);
    let mut next = deck(100);
    next[9][32] |= 0x40;
    assert_eq!(
        ArgumentAlwaysAccepts.verify_shuffle(&prev, &next, b"", &ctx()),
        Err(VerifyOutcome::Invalid(InvalidReason::IdentityCiphertext { position: 9 }))
    );
}

#[test]
fn a18_the_identity_permutation_is_refused() {
    // `next == prev` is a valid shuffle proof and not a shuffle. So is a deck
    // re-masked with one shared scalar, whose ciphertext differences survive
    // and hand an observer the whole permutation.
    let prev = deck(0);
    assert_eq!(
        ArgumentAlwaysAccepts.verify_shuffle(&prev, &prev.clone(), b"", &ctx()),
        Err(VerifyOutcome::Invalid(InvalidReason::DeckUnchanged))
    );
}

// ---------------------------------------------------------------------------
// A12 — the reveal-token collision, which is the review's most serious finding
// ---------------------------------------------------------------------------

#[test]
fn a12_the_collision_a_reveal_token_defect_needs_is_refused() {
    // The token binds one ciphertext coordinate and drops the other, so a token
    // issued for one card opens every card sharing that coordinate. A malicious
    // shuffler arranges the collision by reusing a re-masking scalar. The
    // wrapper refuses the deck, so the token defect has no deck to exploit.
    let prev = deck(0);
    let mut next = deck(100);
    let victim = 3usize;
    let opened_legitimately = 44usize;

    let shared: Vec<u8> = next[opened_legitimately][..33].to_vec();
    next[victim][..33].copy_from_slice(&shared);

    assert_eq!(
        ArgumentAlwaysAccepts.verify_shuffle(&prev, &next, b"", &ctx()),
        Err(VerifyOutcome::Invalid(InvalidReason::DuplicateCiphertext {
            first: victim,
            second: opened_legitimately
        })),
        "a hole card must not share a coordinate with a card the board opens"
    );
}

// ---------------------------------------------------------------------------
// B5 — the identity public key, for which anyone can write an ownership proof
// ---------------------------------------------------------------------------

#[test]
fn b5_the_identity_public_key_is_refused() {
    let mut keys: Vec<[u8; 33]> = (0..6u8).map(|s| [s + 2; 33]).collect();
    assert_eq!(check_key_set(&keys), Ok(()));

    keys[2] = [0u8; 33];
    assert_eq!(
        check_key_set(&keys),
        Err(InvalidReason::IdentityPublicKey { seat: 2 }),
        "an ownership proof for the identity needs no secret at all"
    );
}

#[test]
fn b5b_two_seats_offering_one_key_are_refused() {
    let mut keys: Vec<[u8; 33]> = (0..6u8).map(|s| [s + 2; 33]).collect();
    keys[5] = keys[1];
    assert_eq!(
        check_key_set(&keys),
        Err(InvalidReason::DuplicatePublicKey { first: 1, second: 5 }),
        "duplicate keys aggregate silently"
    );
}

// ---------------------------------------------------------------------------
// B4 — malleability: many byte strings, one value
// ---------------------------------------------------------------------------

/// A type with one bit its parser reads and discards, which is the real
/// encoding defect in miniature: upstream discards six bits per point, giving
/// at least 2^624 encodings of one 52-card deck.
#[derive(Debug, PartialEq)]
struct Malleable(u16);

impl DeckWire for Malleable {
    const LEN: usize = 3;
    fn decode_raw(bytes: &[u8]) -> Result<Self, DecodeError> {
        Ok(Malleable(u16::from_be_bytes([bytes[0], bytes[1]])))
    }
    fn encode(&self) -> Vec<u8> {
        let [a, b] = self.0.to_be_bytes();
        vec![a, b, 0x00]
    }
}

#[test]
fn b4_a_second_encoding_of_one_value_is_refused() {
    let canonical = Malleable(0xBEEF).encode();
    assert_eq!(Malleable::decode(&canonical), Ok(Malleable(0xBEEF)));

    // Every value of the discarded byte decodes to the same value upstream.
    for spare in 1u8..=255 {
        let variant = vec![0xBE, 0xEF, spare];
        assert_eq!(
            Malleable::decode(&variant),
            Err(DecodeError::NonCanonical),
            "spare byte {spare:#04x} produced a second accepted encoding"
        );
    }
}

#[test]
fn b4b_trailing_bytes_are_refused() {
    // Upstream ignores them, so without this a proof and that proof with a
    // megabyte appended are one object to the library and two to anything that
    // hashes or deduplicates it.
    let mut bytes = Malleable(1).encode();
    bytes.extend_from_slice(&[0u8; 1024]);
    assert_eq!(
        Malleable::decode(&bytes),
        Err(DecodeError::WrongLength { expected: 3, got: 1027 })
    );
}

#[test]
fn b4c_a_truncated_encoding_is_refused() {
    assert_eq!(
        Malleable::decode(&[0xBE, 0xEF]),
        Err(DecodeError::WrongLength { expected: 3, got: 2 })
    );
    assert_eq!(
        Malleable::decode(&[]),
        Err(DecodeError::WrongLength { expected: 3, got: 0 })
    );
}

// ---------------------------------------------------------------------------
// D-9 — flooding a table with proofs that are expensive to reject
// ---------------------------------------------------------------------------

#[test]
fn proof_flooding_is_refused_before_the_expensive_work() {
    // Rejecting one bogus proof costs 10 to 36 ms at every seat, so one peer on
    // a slow link could otherwise burn most of a core at every other client.
    let mut admission = ShuffleAdmission::default();
    assert_eq!(admission.admit(2, 2, 6), Ok(()));

    for _ in 0..1000 {
        assert_eq!(
            admission.admit(2, 2, 6),
            Err(NotAdmitted::AlreadySubmitted { seat: 2, position: 2 })
        );
    }
    assert_eq!(admission.len(), 1, "a flood costs one entry, not a thousand");
}

#[test]
fn an_out_of_range_seat_or_position_never_reaches_a_structure() {
    let mut admission = ShuffleAdmission::default();
    for (seat, position) in [(6u8, 0u8), (0, 6), (255, 255)] {
        assert!(admission.admit(seat, position, 6).is_err());
    }
    assert!(admission.is_empty());
}

// ---------------------------------------------------------------------------
// Honest play must survive all of it
// ---------------------------------------------------------------------------

/// The mirror of the whole suite, and the one that would catch an
/// over-eager check: a sound shuffle must pass.
///
/// D-014 makes a tier-1 finding remove a player, so a wrapper that refuses too
/// much ejects honest players. Every check above is worthless if this fails.
#[test]
fn an_honest_shuffle_passes_every_check() {
    for round in 0..8u8 {
        let prev = deck(round.wrapping_mul(53));
        let next = deck(round.wrapping_mul(53).wrapping_add(53));
        assert_eq!(structural_check(&prev, &next, DECK), Ok(()), "round {round}");
        assert!(ArgumentAlwaysAccepts
            .verify_shuffle(&prev, &next, b"", &ctx())
            .is_ok());
    }

    let keys: Vec<[u8; 33]> = (0..10u8).map(|s| [s + 2; 33]).collect();
    assert_eq!(check_key_set(&keys), Ok(()));

    let bytes = Malleable(0x1234).encode();
    assert_eq!(Malleable::decode(&bytes), Ok(Malleable(0x1234)));
}

// ---------------------------------------------------------------------------
// The gap, stated rather than left as an absence
// ---------------------------------------------------------------------------

/// The families that need a real prover, and the condition each belongs to.
///
/// This test passes; it exists so the gap appears in the output of every run
/// instead of being an absence nobody notices. A suite that quietly omitted
/// them would read as coverage it does not have.
#[test]
fn awaiting_implementation() {
    let pending = [
        ("A2/A15  forging the argument itself", "the prover"),
        ("A5/A13  grinding a Fiat-Shamir challenge", "the prover"),
        ("A19     the same battery against the chained path", "the chain driver"),
        ("B6      replaying a proof across contexts", "C-5, the ctx construction site"),
        ("D-7     invalid-curve points through reveal_token", "the library impl"),
        ("D-10    the deck constants moving under a dependency bump", "C-7, already a test"),
    ];
    for (family, blocked_on) in pending {
        println!("PENDING  {family}  -- blocked on {blocked_on}");
    }
    assert_eq!(pending.len(), 6, "six families still need the implementation");
}
