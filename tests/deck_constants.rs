//! What card index 17 *is*, frozen.
//!
//! `ZIFFLE_VERDICT.md` condition **C-7**, closing defect **D-10**.
//!
//! ziffle derives two constants at `Shuffle::<N>::default()`:
//!
//! * `open_deck` — the `N` plaintext card points, `s_i · G` for scalars drawn
//!   from `StdRng::from_seed(sha256(b"CARDS-V1"))`;
//! * the Pedersen commitment key — `h` from `sha256(b"PEDERSON-H-V1")` and `N`
//!   generators from `sha256(b"PEDERSON-VECTOR-G-V1")`.
//!
//! Both are `pub(crate)` state of `Shuffle<N>` with no accessor, and both are
//! functions of two implementation details: `rand 0.8`'s `StdRng` output stream,
//! which `rand` explicitly does not preserve across major versions, and
//! `ark-ec 0.5.0`'s rejection-sampling loop, whose byte consumption is nobody's
//! documented contract.
//!
//! Nothing pins them. Two clients whose `open_deck` differs disagree about what
//! card index 17 is, and deal each other different hands from the same verified
//! deck; two clients whose Pedersen generators differ reject every proof the
//! other produces. Vendoring plus `Cargo.lock` freezes this for *our* build and
//! not for a second implementation, and neither would catch an accidental bump.
//!
//! So the two digests below are the wire meaning of a card, written down. A
//! `rand` or arkworks bump that moves either one fails this test instead of
//! silently redefining the deck.
//!
//! # What each test proves, and what it does not
//!
//! [`open_deck_digest_is_frozen`] and [`commit_key_digest_is_frozen`] recompute
//! the derivations from arkworks and `rand` directly — not through ziffle — and
//! compare against frozen digests. That catches a dependency bump. On its own it
//! would *not* catch our recomputation having drifted from what ziffle actually
//! uses, because the recomputation is a second copy of the derivation.
//!
//! [`open_deck_is_the_deck_ziffle_deals`] closes that gap for `open_deck`: it
//! runs a real one-seat shuffle through ziffle's public API, decrypts each of
//! the 52 masked cards with the seat's own secret key, and checks the recovered
//! plaintext against *our* `open_deck` at the index `Shuffle::reveal_card`
//! reports. If the two derivations ever diverged, all 52 comparisons would fail.
//!
//! No equivalent binding exists for the commitment key: it never appears in a
//! ziffle output, only inside proofs that verify or do not. Its digest is a
//! freeze of an independent recomputation and is claimed as nothing more.

use ark_ec::{AffineRepr, CurveGroup, short_weierstrass::SWCurveConfig};
use ark_ff::UniformRand;
use ark_secp256k1::{Affine, Config, Fr, Projective};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::rand::{SeedableRng, rngs::StdRng};
use sha2::{Digest, Sha256};
use ziffle::{AggregatePublicKey, AggregateRevealToken, Shuffle};

/// The deck size the protocol uses. `PROTOCOL.md` §4.
const N: usize = 52;

/// A compressed secp256k1 point, and a compressed scalar.
const POINT: usize = 33;
const SCALAR: usize = 32;

/// The three seed strings, copied character for character from
/// `vendor/ziffle/src/lib.rs` — including the misspelling of "Pedersen", which
/// is part of the seed and therefore part of the constant.
const OPEN_CARD_PRNG_SEED: &[u8] = b"CARDS-V1";
const PEDERSON_H_PRNG_SEED: &[u8] = b"PEDERSON-H-V1";
const PEDERSON_VECTOR_G_PRNG_SEED: &[u8] = b"PEDERSON-VECTOR-G-V1";

/// `blake3` over the 52 compressed `open_deck` points, in deck order.
///
/// Independently recomputed and confirmed by the ziffle review, D-10.
const OPEN_DECK_DIGEST: &str =
    "4e931f9e24cf0cc9b511525c165ebd9bd1454c2aa0fe9eb74ef315a3dedfae7c";

/// `blake3` over the compressed `h`, then the 52 compressed generators.
const COMMIT_KEY_DIGEST: &str =
    "14c8e144b43285da721b4ec2fe8fd8576849c6befdc9f5f8a7457174018d1dd4";

/// ziffle's seeding step: `StdRng::from_seed(Sha256::digest(seed))`.
///
/// SHA-256 is SHA-256, so using our own `sha2 0.11` here rather than ziffle's
/// transitive `sha2 0.10` cannot change the seed. `StdRng` is the part that can
/// move, and it is `rand 0.8`'s in both.
fn seeded(seed: &[u8]) -> StdRng {
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&Sha256::digest(seed));
    StdRng::from_seed(bytes)
}

/// The generator ziffle multiplies each card scalar by.
fn generator() -> Affine {
    <Config as SWCurveConfig>::GENERATOR
}

/// `vendor/ziffle/src/lib.rs`, `fn open_deck<N>()`, recomputed.
fn open_deck() -> [Affine; N] {
    let mut drng = seeded(OPEN_CARD_PRNG_SEED);
    core::array::from_fn(|_| (generator() * Fr::rand(&mut drng)).into_affine())
}

/// `vendor/ziffle/src/lib.rs`, `impl Default for PedersonCommitKey<N>`,
/// recomputed. Two independent streams, one per seed.
fn commit_key() -> (Projective, [Projective; N]) {
    let mut h_drng = seeded(PEDERSON_H_PRNG_SEED);
    let h = Projective::rand(&mut h_drng);

    let mut gs_drng = seeded(PEDERSON_VECTOR_G_PRNG_SEED);
    let gs = core::array::from_fn(|_| Projective::rand(&mut gs_drng));

    (h, gs)
}

fn push_compressed(out: &mut Vec<u8>, point: &impl CanonicalSerialize) {
    let before = out.len();
    point
        .serialize_compressed(&mut *out)
        .expect("writing to a Vec cannot fail");
    assert_eq!(
        out.len() - before,
        POINT,
        "a compressed secp256k1 point is 33 bytes; \
         a different width means ark-serialize changed its encoding, \
         which redefines every digest in this file"
    );
}

#[test]
fn open_deck_digest_is_frozen() {
    let deck = open_deck();

    let mut bytes = Vec::with_capacity(N * POINT);
    for card in &deck {
        push_compressed(&mut bytes, card);
    }
    assert_eq!(bytes.len(), N * POINT);

    assert_eq!(
        blake3::hash(&bytes).to_hex().as_str(),
        OPEN_DECK_DIGEST,
        "the 52 plaintext card points moved. Two clients built either side of \
         this change deal different cards from the same verified deck. \
         Do not update this constant to make the test pass - find the bump \
         (rand's StdRng stream, or ark-ec's rejection sampling) and decide \
         deliberately, because the decision is a wire-format break. \
         ZIFFLE_VERDICT.md D-10 / C-7."
    );
}

#[test]
fn commit_key_digest_is_frozen() {
    let (h, gs) = commit_key();

    let mut bytes = Vec::with_capacity((N + 1) * POINT);
    push_compressed(&mut bytes, &h.into_affine());
    for g in &gs {
        push_compressed(&mut bytes, &g.into_affine());
    }
    assert_eq!(bytes.len(), (N + 1) * POINT);

    assert_eq!(
        blake3::hash(&bytes).to_hex().as_str(),
        COMMIT_KEY_DIGEST,
        "the Pedersen commitment key moved. Two clients built either side of \
         this change reject every shuffle proof the other produces. \
         Same rule as above: this is a wire-format break, not a constant to \
         refresh. ZIFFLE_VERDICT.md D-10 / C-7."
    );
}

/// The generators must be distinct from each other and from `h`; a repeat would
/// make the commitment binding on fewer messages than the argument assumes.
/// Cheap, and it would catch a seeding change that the digest test alone would
/// only report as "moved".
#[test]
fn commit_key_generators_are_distinct() {
    let (h, gs) = commit_key();

    for (i, g) in gs.iter().enumerate() {
        assert_ne!(*g, h, "generator {i} equals h");
        assert!(!g.into_affine().is_zero(), "generator {i} is the identity");
        for (j, other) in gs.iter().enumerate().take(i) {
            assert_ne!(*g, *other, "generators {j} and {i} collide");
        }
    }
    assert!(!h.into_affine().is_zero(), "h is the identity");
}

/// The 52 card points must be distinct, or two indices name the same card and
/// `reveal_card`'s position lookup is ambiguous.
#[test]
fn open_deck_points_are_distinct() {
    let deck = open_deck();
    for (i, card) in deck.iter().enumerate() {
        assert!(!card.is_zero(), "card {i} is the identity");
        for (j, other) in deck.iter().enumerate().take(i) {
            assert_ne!(card, other, "cards {j} and {i} are the same point");
        }
    }
}

/// The binding test: our recomputed `open_deck` is the deck ziffle actually
/// deals from, not merely a second derivation that happens to hash to a frozen
/// value.
///
/// One seat, so the aggregate key is that seat's key and the seat can decrypt.
/// `shuffle_initial_deck` remasks card `perm[i]` as
/// `(r·G, open_deck[perm[i]] + r·apk)`, so with `apk = sk·G` the plaintext is
/// `c2 - sk·c1`. `reveal_card` independently reports `perm[i]` by looking the
/// plaintext up in ziffle's own `open_deck`. Comparing the two closes the loop.
#[test]
fn open_deck_is_the_deck_ziffle_deals() {
    let ours = open_deck();

    let shuffle = Shuffle::<N>::default();
    // Fixed seed: this test must not be able to pass or fail by luck.
    let mut rng = StdRng::from_seed([0x5a; 32]);
    let ctx = b"p2p-poker/tests/deck_constants/C-7";

    let (sk, pk, ownership) = shuffle.keygen(&mut rng, ctx);
    let vpk = ownership
        .verify(pk, ctx)
        .expect("ziffle rejected its own ownership proof");
    let apk = AggregatePublicKey::new(&[vpk]);

    let (deck, proof) = shuffle.shuffle_initial_deck(&mut rng, apk, ctx);

    // The masked deck's raw ciphertexts. Taken before verification, because
    // `Verified<MaskedDeck<N>>` does not serialise - only the unverified form
    // does, and the two are the same 52 pairs.
    let mut wire = Vec::new();
    deck.serialize_compressed(&mut wire)
        .expect("writing to a Vec cannot fail");
    assert_eq!(
        wire.len(),
        N * 2 * POINT,
        "MaskedDeck<52> is documented as 3432 bytes"
    );

    // The seat's own scalar, recovered through the only door ziffle leaves
    // open. `SecretKey` is opaque and `ZeroizeOnDrop`, but it serialises.
    let mut sk_wire = Vec::new();
    sk.serialize_compressed(&mut sk_wire)
        .expect("writing to a Vec cannot fail");
    assert_eq!(sk_wire.len(), SCALAR);
    let s = Fr::deserialize_compressed(&sk_wire[..]).expect("ziffle's own scalar encoding");

    let verified = shuffle
        .verify_initial_shuffle(apk, deck, proof, ctx)
        .expect("ziffle rejected its own shuffle proof");

    let mut seen = [false; N];

    for position in 0..N {
        let card = verified.get(position).expect("position is in range");

        let (token, token_proof) = card.reveal_token(&mut rng, &sk, pk, ctx);
        let verified_token = token_proof
            .verify(vpk, token, card, ctx)
            .expect("ziffle rejected its own reveal token proof");
        let index = shuffle
            .reveal_card(AggregateRevealToken::new(&[verified_token]), card)
            .expect("a single-seat reveal of a verified deck always opens");

        // Decrypt the same card ourselves, straight off the wire bytes.
        let at = position * 2 * POINT;
        let c1 = Affine::deserialize_compressed(&wire[at..at + POINT]).expect("c1");
        let c2 = Affine::deserialize_compressed(&wire[at + POINT..at + 2 * POINT]).expect("c2");
        let plaintext = (c2.into_group() - (c1 * s)).into_affine();

        assert_eq!(
            plaintext, ours[index],
            "position {position} opened as ziffle's card {index}, but that \
             index in our recomputed open_deck is a different point. The two \
             derivations have diverged, so the frozen digest no longer \
             describes the deck this build deals. ZIFFLE_VERDICT.md D-10 / C-7."
        );

        assert!(!seen[index], "index {index} was dealt twice");
        seen[index] = true;
    }

    assert!(
        seen.iter().all(|&b| b),
        "the shuffle did not deal all 52 distinct cards"
    );
}
