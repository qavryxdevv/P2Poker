//! The `ziffle` implementation of the deck.
//!
//! **This is the only module in the crate that names a library type.** Every
//! other module speaks [`Ciphertext`] bytes and the traits of
//! [`protocol`](super::protocol), so replacing the library is a change here and
//! nowhere else. That indirection is the mitigation for an unaudited backing
//! crate (`docs/research/ZIFFLE_VERDICT.md`), not speculative generality.
//!
//! # Where the parameters live, and why (C-13)
//!
//! `Shuffle::<52>` is 8 832 bytes, it is `Copy`, and `default()` costs 5.9 ms —
//! all three measured, the last two being the reason it must never be
//! constructed per hand or passed by value. [`DeckParams`] holds it once per
//! process and is shared by `Arc`; [`HandDeck`] is the per-hand object and
//! borrows it.
//!
//! That split is also why the per-hand state is not in the same object as the
//! parameters. A single struct holding both would have to be rebuilt every hand,
//! which is exactly what C-13 forbids.
//!
//! # Why a `HandDeck` keeps a store of decks it has verified
//!
//! ziffle's `Verified<MaskedDeck>` has a private field and no constructor —
//! deliberately, and it is the library's best idea. The consequence is that a
//! verified deck **cannot be rebuilt from bytes**: only the peer that ran the
//! verification holds one.
//!
//! So the boundary stays byte-oriented, which is what keeps it replaceable, and
//! this implementation keeps the handles it earned, keyed by the hash of the
//! deck's canonical bytes. When the chain asks it to verify a link against a
//! `prev` it has never seen, the answer is
//! [`Unavailable::InputDeckUnknown`] — **not** a rejection. The distinction is
//! the whole of `CRYPTOGRAPHY.md` §8.1.5: a check that did not run is never
//! evidence against anybody, and a peer that is merely behind must not be
//! accused of forging.
//!
//! The store is bounded by the chain: at most one deck per seat per hand, so ten
//! entries of 3 432 bytes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ziffle::{
    AggregatePublicKey, AggregateRevealToken, MaskedDeck, OwnershipProof, PublicKey, RevealToken,
    RevealTokenProof, SecretKey, Shuffle, ShuffleProof, Verified as LibVerified,
};

use crate::poker::state::{Card, Hash};
use crate::protocol::serialization::h;
use crate::protocol::signatures::Domain;
use crate::security::rng::OsRng;

use super::deck::CardIndex;
use super::protocol::{
    Ciphertext, DeckCrypto, DeckCtx, DeckWire, DecodeError, Final, InvalidReason, Unavailable,
    Verified, VerifyOutcome,
};
use super::reveal::SoundnessFault;

/// The deck the game plays with.
pub const DECK: usize = 52;

/// One compressed secp256k1 point.
const POINT: usize = 33;

/// One ElGamal ciphertext: a pair of points.
const CIPHERTEXT: usize = 2 * POINT;

// ---------------------------------------------------------------------------
// The wire types
// ---------------------------------------------------------------------------

/// Implement [`DeckWire`] over a library type through `ark-serialize`.
///
/// `decode_raw` uses `deserialize_compressed`, which validates that the bytes
/// are a point on the curve and in the prime-order subgroup. The `_unchecked`
/// variants are never used anywhere in this crate: the review fed two million
/// random byte strings to the unvalidated path and it accepted every one of them
/// as a point (C-1).
///
/// The length gate and the re-encode comparison come from [`DeckWire::decode`]
/// and are not repeated here, which is the point of putting them in the trait.
macro_rules! wire {
    ($name:ident, $inner:ty, $len:expr, $doc:expr) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $name($inner);

        impl DeckWire for $name {
            const LEN: usize = $len;

            fn decode_raw(bytes: &[u8]) -> Result<Self, DecodeError> {
                <$inner>::deserialize_compressed(bytes)
                    .map($name)
                    .map_err(|_| DecodeError::Malformed)
            }

            fn encode(&self) -> Vec<u8> {
                let mut out = Vec::with_capacity(Self::LEN);
                self.0
                    .serialize_compressed(&mut out)
                    .expect("a Vec sink cannot fail");
                out
            }
        }
    };
}

wire!(
    WireDeck,
    MaskedDeck<DECK>,
    DECK * CIPHERTEXT,
    "A masked deck, as it crosses the network."
);
wire!(
    WireShuffleProof,
    ShuffleProof<DECK>,
    5_547,
    "A Bayer-Groth shuffle argument, as it crosses the network."
);
wire!(
    WireKey,
    PublicKey,
    POINT,
    "A player's per-hand deck key, as it crosses the network."
);
wire!(
    WireKeyProof,
    OwnershipProof,
    65,
    "A proof of knowing the secret behind a [`WireKey`]."
);
wire!(
    WireToken,
    RevealToken,
    POINT,
    "One player's decryption share for one card."
);
wire!(
    WireTokenProof,
    RevealTokenProof,
    98,
    "A DLEQ proof that a [`WireToken`] was computed with the claimed key."
);

/// A key whose owner has proved it holds the matching secret.
///
/// No public constructor: the only way to one is
/// [`HandDeck::verify_key`]. It wraps the library's own verified typestate, so
/// the guarantee survives even though our layer never sees the key material.
#[derive(Debug, Clone, Copy)]
pub struct VerifiedKey {
    /// The library's typestate, for the calls that demand it.
    verified: LibVerified<PublicKey>,
    /// The same key, for the calls that take a bare one.
    ///
    /// Both are kept because `Verified<T>` has a private field and no accessor -
    /// which is the right design and means the raw key has to be carried
    /// alongside rather than recovered. They are minted together in
    /// [`HandDeck::verify_key`] and cannot disagree.
    raw: PublicKey,
}

/// A decryption share this peer has checked against its own final deck.
///
/// No public constructor; see [`HandDeck::verify_token`].
#[derive(Debug, Clone, Copy)]
pub struct VerifiedToken(LibVerified<RevealToken>);

// ---------------------------------------------------------------------------
// The parameters, once per process
// ---------------------------------------------------------------------------

/// The library's parameters: the open deck and the Pedersen commitment key.
///
/// Built once. `tests/deck_constants.rs` pins both against an independent
/// recomputation, because they are the wire meaning of a card and a bump in
/// `rand` or arkworks would otherwise redefine it silently (C-7).
pub struct DeckParams {
    shuffle: Shuffle<DECK>,
}

impl DeckParams {
    /// Build the parameters. **Once per process** — this costs 5.9 ms.
    pub fn new() -> Arc<Self> {
        Arc::new(DeckParams {
            shuffle: Shuffle::default(),
        })
    }

    /// A fresh per-hand key and the proof that its owner holds the secret.
    pub fn keygen(&self, ctx: &DeckCtx) -> (SecretKey, WireKey, WireKeyProof) {
        let (sk, pk, proof) = self.shuffle.keygen(&mut OsRng, ctx.as_bytes());
        (sk, WireKey(pk), WireKeyProof(proof))
    }
}

// ---------------------------------------------------------------------------
// One hand
// ---------------------------------------------------------------------------

/// The deck of one hand at one table.
pub struct HandDeck {
    params: Arc<DeckParams>,
    apk: AggregatePublicKey,
    /// Decks this peer verified for itself, by the hash of their canonical
    /// bytes. See the module documentation for why this exists.
    verified: Mutex<HashMap<Hash, LibVerified<MaskedDeck<DECK>>>>,
}

impl HandDeck {
    /// Open a hand over the seats whose keys have been verified.
    ///
    /// `n`-of-`n`, and there is deliberately no `t`-of-`n`: `SPEC_CS.md` §35
    /// forbids one for hole cards, and a library that offered it would still not
    /// be allowed to be called that way here.
    pub fn new(params: Arc<DeckParams>, seats: &[VerifiedKey]) -> Self {
        let keys: Vec<LibVerified<PublicKey>> = seats.iter().map(|k| k.verified).collect();
        HandDeck {
            params,
            apk: AggregatePublicKey::new(&keys),
            verified: Mutex::new(HashMap::new()),
        }
    }

    /// Check that a seat holds the secret behind the key it presented.
    ///
    /// The identity key and a key already presented by another seat are refused
    /// **before** the proof is checked (C-4), because a proof of knowing the
    /// secret behind the identity is not the question — the question is whether
    /// a distinct, non-degenerate key was presented at all.
    pub fn verify_key(
        key: WireKey,
        proof: &WireKeyProof,
        already_seated: &[VerifiedKey],
        ctx: &DeckCtx,
    ) -> Result<VerifiedKey, VerifyOutcome> {
        let mut all: Vec<[u8; POINT]> = already_seated
            .iter()
            .map(|k| encoded_key(&WireKey(k.raw)))
            .collect();
        all.push(encoded_key(&key));
        super::protocol::check_key_set(&all).map_err(VerifyOutcome::Invalid)?;

        proof
            .0
            .verify(key.0, ctx.as_bytes())
            .map(|verified| VerifiedKey {
                verified,
                raw: key.0,
            })
            .ok_or(VerifyOutcome::Invalid(InvalidReason::ArgumentFailed))
    }

    /// Shuffle and re-mask, producing the deck and its argument.
    ///
    /// `prev` is `None` for the first link of the chain, whose input is the open
    /// deck, and otherwise the chain's own last verified deck — which is why it
    /// is a [`Verified`] and **not** a [`Final`]: a final deck is one nobody
    /// shuffles again. Randomness is the operating system's and nothing else
    /// (`SPEC_CS.md` §7).
    pub fn shuffle(
        &self,
        prev: Option<&Verified<Vec<Ciphertext>>>,
        ctx: &DeckCtx,
    ) -> Result<(Vec<Ciphertext>, Vec<u8>), VerifyOutcome> {
        let (deck, proof) = match prev {
            None => self
                .params
                .shuffle
                .shuffle_initial_deck(&mut OsRng, self.apk, ctx.as_bytes()),
            Some(p) => {
                let handle = self.lookup(p.as_ref())?;
                self.params
                    .shuffle
                    .shuffle_deck(&mut OsRng, self.apk, &handle, ctx.as_bytes())
            }
        };
        Ok((to_ciphertexts(&WireDeck(deck)), WireShuffleProof(proof).encode()))
    }

    /// This peer's decryption share for one card of the final deck.
    ///
    /// Takes an index and never a card, so a share cannot be produced for a
    /// ciphertext a sender supplied: the card is looked up in **this peer's own**
    /// verified deck or there is no share.
    pub fn token(
        &self,
        sk: &SecretKey,
        key: &VerifiedKey,
        deck: &Final<Verified<Vec<Ciphertext>>>,
        index: CardIndex,
        ctx: &DeckCtx,
    ) -> Result<(WireToken, WireTokenProof), VerifyOutcome> {
        let handle = self.lookup(deck.as_ref().as_ref())?;
        let card = handle
            .get(index.get() as usize)
            .ok_or(VerifyOutcome::Invalid(InvalidReason::ArgumentFailed))?;
        let (token, proof) =
            card.reveal_token(&mut OsRng, sk, key.raw, ctx.as_bytes());
        Ok((WireToken(token), WireTokenProof(proof)))
    }

    /// Check somebody else's share against **this peer's own** final deck.
    pub fn verify_token(
        &self,
        key: &VerifiedKey,
        deck: &Final<Verified<Vec<Ciphertext>>>,
        index: CardIndex,
        token: WireToken,
        proof: &WireTokenProof,
        ctx: &DeckCtx,
    ) -> Result<VerifiedToken, VerifyOutcome> {
        let handle = self.lookup(deck.as_ref().as_ref())?;
        let card = handle
            .get(index.get() as usize)
            .ok_or(VerifyOutcome::Invalid(InvalidReason::ArgumentFailed))?;
        proof
            .0
            .verify(key.verified, token.0, card, ctx.as_bytes())
            .map(VerifiedToken)
            .ok_or(VerifyOutcome::Invalid(InvalidReason::ArgumentFailed))
    }

    /// Open a card from a complete set of verified shares.
    ///
    /// `tokens` must be one per key in the aggregate, counted by the caller: the
    /// library sums whatever it is given and never learns how many players
    /// exist. [`TokenSet`](super::reveal::TokenSet) is what does the counting.
    ///
    /// A failure here is a [`SoundnessFault`] and never a rejection (C-10). If
    /// every share verified against this peer's own deck and the product is
    /// still not a card, then the argument's soundness has failed — nobody did
    /// anything, and there is nothing to retry.
    pub fn open(
        &self,
        deck: &Final<Verified<Vec<Ciphertext>>>,
        index: CardIndex,
        tokens: &[VerifiedToken],
    ) -> Result<Card, SoundnessFault> {
        let handle = self.lookup(deck.as_ref().as_ref()).map_err(|_| SoundnessFault {
            index: index.get(),
            what: "the final deck is not one this peer verified",
        })?;
        let card = handle
            .get(index.get() as usize)
            .ok_or_else(|| SoundnessFault {
                index: index.get(),
                what: "the index is past the end of the deck",
            })?;

        let shares: Vec<LibVerified<RevealToken>> = tokens.iter().map(|t| t.0).collect();
        let aggregate = AggregateRevealToken::new(&shares);

        let position = self
            .params
            .shuffle
            .reveal_card(aggregate, card)
            .ok_or_else(|| SoundnessFault {
                index: index.get(),
                what: "a complete verified token set did not decrypt to a card of the open deck",
            })?;

        // The library answers with a position in its own open deck, which is an
        // arbitrary list of curve points. That position **is** the card's
        // identity, by definition and by nothing else — which is why
        // `tests/deck_constants.rs` freezes the list. Two clients whose open
        // decks differ deal each other different hands from the same verified
        // deck and never find out.
        Card::from_index(position as u8).map_err(|_| SoundnessFault {
            index: index.get(),
            what: "the open deck yielded a position that is not a card",
        })
    }

    /// The verified handle for a deck this peer checked, or the reason it cannot
    /// answer.
    fn lookup(
        &self,
        deck: &[Ciphertext],
    ) -> Result<LibVerified<MaskedDeck<DECK>>, VerifyOutcome> {
        let key = deck_hash(deck);
        self.verified
            .lock()
            .expect("the deck store is not held across a panic")
            .get(&key)
            .copied()
            .ok_or(VerifyOutcome::CouldNotVerify(Unavailable::InputDeckUnknown))
    }

    fn remember(&self, deck: &[Ciphertext], handle: LibVerified<MaskedDeck<DECK>>) {
        self.verified
            .lock()
            .expect("the deck store is not held across a panic")
            .insert(deck_hash(deck), handle);
    }

    fn decode_step(
        next: &[Ciphertext],
        proof: &[u8],
    ) -> Result<(MaskedDeck<DECK>, ShuffleProof<DECK>), VerifyOutcome> {
        let deck = WireDeck::decode(&flatten(next))
            .map_err(|e| VerifyOutcome::Invalid(InvalidReason::from(e)))?;
        let proof = WireShuffleProof::decode(proof)
            .map_err(|e| VerifyOutcome::Invalid(InvalidReason::from(e)))?;
        Ok((deck.0, proof.0))
    }
}

impl DeckCrypto for HandDeck {
    const DECK_LEN: usize = DECK;

    fn verify_initial_argument(
        &self,
        next: &[Ciphertext],
        proof: &[u8],
        ctx: &DeckCtx,
    ) -> Result<(), VerifyOutcome> {
        let (deck, proof) = Self::decode_step(next, proof)?;
        let handle = self
            .params
            .shuffle
            .verify_initial_shuffle(self.apk, deck, proof, ctx.as_bytes())
            .ok_or(VerifyOutcome::Invalid(InvalidReason::ArgumentFailed))?;
        self.remember(next, handle);
        Ok(())
    }

    fn verify_argument(
        &self,
        prev: &[Ciphertext],
        next: &[Ciphertext],
        proof: &[u8],
        ctx: &DeckCtx,
    ) -> Result<(), VerifyOutcome> {
        // Before anything expensive: is `prev` a deck this peer established for
        // itself? If not, the honest answer is that the check did not run.
        let previous = self.lookup(prev)?;
        let (deck, proof) = Self::decode_step(next, proof)?;
        let handle = self
            .params
            .shuffle
            .verify_shuffle(self.apk, &previous, deck, proof, ctx.as_bytes())
            .ok_or(VerifyOutcome::Invalid(InvalidReason::ArgumentFailed))?;
        self.remember(next, handle);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Plumbing
// ---------------------------------------------------------------------------

fn encoded_key(key: &WireKey) -> [u8; POINT] {
    let mut a = [0u8; POINT];
    a.copy_from_slice(&key.encode());
    a
}

fn flatten(deck: &[Ciphertext]) -> Vec<u8> {
    let mut out = Vec::with_capacity(deck.len() * CIPHERTEXT);
    for ct in deck {
        out.extend_from_slice(ct);
    }
    out
}

fn to_ciphertexts(deck: &WireDeck) -> Vec<Ciphertext> {
    deck.encode()
        .chunks_exact(CIPHERTEXT)
        .map(|c| {
            let mut ct = [0u8; CIPHERTEXT];
            ct.copy_from_slice(c);
            ct
        })
        .collect()
}

/// The store key: a domain-separated hash of the deck's canonical bytes.
fn deck_hash(deck: &[Ciphertext]) -> Hash {
    let flat = flatten(deck);
    h(Domain::DeckCommit.context(), &[b"deck-store", &flat])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mental_poker::protocol::{CtxFields, ProofPosition};
    use crate::mental_poker::shuffle::{ChainParams, ShuffleChain};
    use std::collections::BTreeSet;

    fn ctx(round: u8) -> DeckCtx {
        DeckCtx::build(&CtxFields {
            protocol_version: 1,
            table_id: [1u8; 32],
            session_id: [2u8; 32],
            hand_id: 9,
            sequence: round as u64,
            position: ProofPosition::shuffle_step(round).unwrap(),
            sender_public_key: [round; 32],
        })
    }

    fn reveal_ctx() -> DeckCtx {
        DeckCtx::build(&CtxFields {
            protocol_version: 1,
            table_id: [1u8; 32],
            session_id: [2u8; 32],
            hand_id: 9,
            sequence: 100,
            position: ProofPosition::NotAShuffleStep,
            sender_public_key: [77u8; 32],
        })
    }

    /// Three players, real keys, a real two-link chain, and a card opened from a
    /// full set of real shares. If the boundary could not host the library this
    /// is where it would show.
    #[test]
    fn a_whole_deck_end_to_end() {
        let params = DeckParams::new();
        let key_ctx = reveal_ctx();

        let mut secrets = Vec::new();
        let mut seated: Vec<VerifiedKey> = Vec::new();
        for _ in 0..3 {
            let (sk, pk, proof) = params.keygen(&key_ctx);
            let v = HandDeck::verify_key(pk, &proof, &seated, &key_ctx)
                .expect("an honest key with an honest proof");
            secrets.push(sk);
            seated.push(v);
        }

        let hand = HandDeck::new(Arc::clone(&params), &seated);

        // The chain: three seats, each shuffling once.
        let mut chain = ShuffleChain::open(
            ChainParams {
                protocol_version: 1,
                table_id: [1u8; 32],
                session_id: [2u8; 32],
                hand_id: 9,
            },
            vec![0, 1, 2],
            (0..3u8).map(|s| [s; 32]).collect(),
        )
        .unwrap();

        for round in 0..3u8 {
            let t = std::time::Instant::now();
            let step_ctx = chain.next_ctx(round as u64).expect("the chain is open");
            let (deck, proof) = hand
                .shuffle(chain.last_verified(), &step_ctx)
                .expect("this peer can shuffle a deck it verified");
            let proved = t.elapsed();
            let t = std::time::Instant::now();
            chain
                .accept_step(&hand, round, deck.clone(), &proof, round as u64)
                .expect("an honest link verifies");
            let verified = t.elapsed();
            println!("round {round}: prove {proved:?}, verify {verified:?}");

            // A generous floor, forty times under what this machine measures
            // (~95 ms to prove, ~40 ms to verify a 52-card link). It is not a
            // performance assertion — it is the guard against this whole test
            // passing against a backend that quietly stopped doing the work,
            // which is the failure mode an end-to-end test is worst at showing.
            assert!(
                verified > std::time::Duration::from_millis(1),
                "verification took {verified:?}; a real Bayer-Groth check cannot be that cheap"
            );
        }

        let final_deck = chain.finish().expect("every seat shuffled");

        // Open one card with all three shares.
        let index = CardIndex::position(0, DECK).unwrap();
        let mut tokens = Vec::new();
        for (i, sk) in secrets.iter().enumerate() {
            let (token, proof) = hand
                .token(sk, &seated[i], &final_deck, index, &reveal_ctx())
                .expect("a share of a card of this deck");
            let verified = hand
                .verify_token(&seated[i], &final_deck, index, token, &proof, &reveal_ctx())
                .expect("an honest share verifies");
            tokens.push(verified);
        }

        let card = hand
            .open(&final_deck, index, &tokens)
            .expect("three of three shares open the card");

        // And the deck is a deck: 52 distinct cards, opened one at a time.
        let mut seen = BTreeSet::new();
        seen.insert(card);
        for i in 1..DECK as u8 {
            let index = CardIndex::position(i, DECK).unwrap();
            let mut tokens = Vec::new();
            for (j, sk) in secrets.iter().enumerate() {
                let (token, proof) = hand
                    .token(sk, &seated[j], &final_deck, index, &reveal_ctx())
                    .unwrap();
                tokens.push(
                    hand.verify_token(&seated[j], &final_deck, index, token, &proof, &reveal_ctx())
                        .unwrap(),
                );
            }
            seen.insert(hand.open(&final_deck, index, &tokens).unwrap());
        }
        assert_eq!(seen.len(), DECK, "a shuffled deck holds every card once");
    }

    /// Fewer than every share opens nothing. `n`-of-`n` is the property the
    /// whole game rests on: no proper subset of the table may see a card.
    #[test]
    fn a_short_token_set_does_not_open_a_card() {
        let params = DeckParams::new();
        let key_ctx = reveal_ctx();

        let mut secrets = Vec::new();
        let mut seated: Vec<VerifiedKey> = Vec::new();
        for _ in 0..3 {
            let (sk, pk, proof) = params.keygen(&key_ctx);
            seated.push(HandDeck::verify_key(pk, &proof, &seated, &key_ctx).unwrap());
            secrets.push(sk);
        }
        let hand = HandDeck::new(Arc::clone(&params), &seated);

        let (deck, _proof) = hand.shuffle(None, &ctx(0)).unwrap();
        // Establish it as verified for this peer by running the real check.
        let mut chain = ShuffleChain::open(
            ChainParams {
                protocol_version: 1,
                table_id: [1u8; 32],
                session_id: [2u8; 32],
                hand_id: 9,
            },
            vec![0, 1],
            vec![[0u8; 32], [1u8; 32]],
        )
        .unwrap();
        let c0 = chain.next_ctx(0).unwrap();
        let (d0, p0) = hand.shuffle(None, &c0).unwrap();
        chain.accept_step(&hand, 0, d0, &p0, 0).unwrap();
        let c1x = chain.next_ctx(1).unwrap();
        let (d1, p1) = hand.shuffle(chain.last_verified(), &c1x).unwrap();
        chain.accept_step(&hand, 1, d1, &p1, 1).unwrap();
        let final_deck = chain.finish().unwrap();
        let _ = deck;

        let index = CardIndex::position(3, DECK).unwrap();
        let mut tokens = Vec::new();
        for (j, sk) in secrets.iter().enumerate().take(2) {
            let (t, pr) = hand
                .token(sk, &seated[j], &final_deck, index, &reveal_ctx())
                .unwrap();
            tokens.push(
                hand.verify_token(&seated[j], &final_deck, index, t, &pr, &reveal_ctx())
                    .unwrap(),
            );
        }

        assert!(
            hand.open(&final_deck, index, &tokens).is_err(),
            "two of three shares must not open a card"
        );
    }

    /// A deck this peer never verified is not something it can answer about, and
    /// the answer is `CouldNotVerify` rather than a rejection. A peer that is
    /// merely behind must never be accused of forging.
    #[test]
    fn a_deck_this_peer_never_verified_is_unanswerable() {
        let params = DeckParams::new();
        let key_ctx = reveal_ctx();
        let mut seated: Vec<VerifiedKey> = Vec::new();
        for _ in 0..2 {
            let (_sk, pk, proof) = params.keygen(&key_ctx);
            seated.push(HandDeck::verify_key(pk, &proof, &seated, &key_ctx).unwrap());
        }
        let hand = HandDeck::new(Arc::clone(&params), &seated);

        let (stranger, proof) = hand.shuffle(None, &ctx(0)).unwrap();
        let (other, _) = hand.shuffle(None, &ctx(1)).unwrap();

        assert_eq!(
            hand.verify_argument(&stranger, &other, &proof, &ctx(1)),
            Err(VerifyOutcome::CouldNotVerify(
                Unavailable::InputDeckUnknown
            )),
            "the input deck was never established here"
        );
    }

    /// The wire lengths this module declares are the library's own, measured
    /// rather than assumed. A curve or serialisation change fails here instead of
    /// becoming a decode error at run time.
    #[test]
    fn the_declared_lengths_are_the_librarys() {
        use ark_serialize::Compress;

        let params = DeckParams::new();
        let c = reveal_ctx();
        let (sk, pk, kp) = params.keygen(&c);
        let seated = vec![HandDeck::verify_key(pk, &kp, &[], &c).unwrap()];
        let hand = HandDeck::new(Arc::clone(&params), &seated);
        let (deck, proof) = hand.shuffle(None, &c).unwrap();

        assert_eq!(WireKey::LEN, pk.0.serialized_size(Compress::Yes));
        assert_eq!(WireKeyProof::LEN, kp.0.serialized_size(Compress::Yes));
        assert_eq!(WireDeck::LEN, flatten(&deck).len());
        assert_eq!(WireShuffleProof::LEN, proof.len());

        // A one-link chain, so the final deck comes from the only thing that
        // can honestly mint one.
        let mut c1 = ShuffleChain::open(
            ChainParams {
                protocol_version: 1,
                table_id: [1u8; 32],
                session_id: [2u8; 32],
                hand_id: 9,
            },
            vec![0, 1],
            vec![[0u8; 32], [1u8; 32]],
        )
        .unwrap();
        let first = c1.next_ctx(0).unwrap();
        let (d0, p0) = hand.shuffle(None, &first).unwrap();
        c1.accept_step(&hand, 0, d0, &p0, 0).unwrap();
        let second = c1.next_ctx(1).unwrap();
        let (d1, p1) = hand.shuffle(c1.last_verified(), &second).unwrap();
        c1.accept_step(&hand, 1, d1, &p1, 1).unwrap();
        let after = c1.finish().unwrap();
        let (token, tproof) = hand
            .token(&sk, &seated[0], &after, CardIndex::position(0, DECK).unwrap(), &c)
            .unwrap();
        assert_eq!(WireToken::LEN, token.encode().len());
        assert_eq!(WireTokenProof::LEN, tproof.encode().len());
    }

    /// C-2: trailing bytes. The library ignores them, so a proof and that proof
    /// with a megabyte appended are one object to it and two to anything that
    /// hashes or deduplicates them.
    #[test]
    fn trailing_bytes_do_not_reach_the_library() {
        let params = DeckParams::new();
        let c = reveal_ctx();
        let (_sk, pk, kp) = params.keygen(&c);
        let seated = vec![HandDeck::verify_key(pk, &kp, &[], &c).unwrap()];
        let hand = HandDeck::new(Arc::clone(&params), &seated);
        let (deck, mut proof) = hand.shuffle(None, &c).unwrap();

        assert!(hand.verify_initial_argument(&deck, &proof, &c).is_ok());

        proof.extend_from_slice(&[0u8; 4096]);
        assert_eq!(
            hand.verify_initial_argument(&deck, &proof, &c),
            Err(VerifyOutcome::Invalid(InvalidReason::Decode(
                DecodeError::WrongLength {
                    expected: WireShuffleProof::LEN,
                    got: WireShuffleProof::LEN + 4096
                }
            )))
        );
    }

    /// C-13, measured: the parameters are built once and shared, not rebuilt per
    /// hand. `Shuffle::<52>` is 8 832 bytes and `default()` costs 5.9 ms.
    #[test]
    fn the_parameters_are_shared_and_not_copied() {
        let params = DeckParams::new();
        let a = HandDeck::new(Arc::clone(&params), &[]);
        let b = HandDeck::new(Arc::clone(&params), &[]);
        assert_eq!(Arc::strong_count(&params), 3);
        assert!(Arc::ptr_eq(&a.params, &b.params));
        assert!(
            std::mem::size_of::<Shuffle<DECK>>() >= 8_000,
            "if this ever became small the reason for the Arc would be gone"
        );
    }
}
