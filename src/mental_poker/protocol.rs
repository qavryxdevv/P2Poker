//! The boundary between the poker protocol and whatever library carries the
//! mental-poker construction.
//!
//! No module outside `src/mental_poker/` names a library type, so replacing the
//! library is a change to this directory and nowhere else. Reviewed against
//! `ziffle` 0.1.0 in `docs/research/ZIFFLE_VERDICT.md`, whose conditions C-1 to
//! C-15 are discharged **by this trait's contract** rather than by the
//! discipline of its callers.
//!
//! # What the review found, and why this shape
//!
//! Twenty-two attack families were run against the Bayer–Groth argument and
//! none forged a proof. Fourteen defects were found **around** it, and the
//! pattern in all of them is the same: the library proves exactly what it
//! claims, and the claim is not what a poker game needs.
//!
//! The sharpest examples, each of which this boundary closes:
//!
//! - A "shuffle" may leave chosen slots in plaintext and the proof verifies.
//!   Bayer–Groth constrains the re-masking scalars not at all, so the identity
//!   permutation and `next == prev` are both valid shuffles.
//! - One shared re-masking scalar preserves ciphertext differences, and an
//!   observer recovers the whole permutation from public data.
//! - A reveal token binds one ciphertext coordinate and drops the other, so a
//!   token issued for one card decrypts every card sharing that coordinate —
//!   and a malicious shuffler can arrange the collision.
//!
//! [`structural_check`] closes the exploit path of all three, costs 0.019 ms
//! against the argument's ~36 ms, and is the highest-value function in the
//! review. It is therefore **not** something a caller remembers to run:
//! [`DeckCrypto::verify_shuffle`] is a provided method that runs it first, and
//! an implementor writes [`DeckCrypto::verify_argument`] instead.
//!
//! # A valid shuffle proof is evidence of *a* permutation, never a *random* one
//!
//! Unpredictability comes only from the honest shufflers in the chain, never
//! from the proof. Every seated player shuffles, in a fixed committed order,
//! and cards are dealt only from the last deck of the chain.

use std::collections::HashSet;

use crate::poker::state::Hash;
use crate::protocol::serialization::h;
use crate::protocol::signatures::Domain;

/// The 32-byte context binding every proof is made under.
///
/// Opaque and fixed-width by construction, so no caller can hand the library a
/// concatenation — which was **measured** to let one player rebroadcast
/// another's proof as their own. It must carry the protocol version, table,
/// hand, shuffle round and the shuffler's identity.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DeckCtx([u8; 32]);

impl DeckCtx {
    /// Build a context from an already length-prefixed, domain-separated hash.
    ///
    /// Takes a [`Hash`] rather than bytes so the only way to make one is to
    /// have hashed something properly. Simplifying the construction to a
    /// concatenation reintroduces a measured proof-transfer attack.
    pub const fn from_hash(h: Hash) -> Self {
        DeckCtx(h)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Build the context of `PROTOCOL.md` §4.5.
    ///
    /// **This is the construction site the review named (C-5), and the shape is
    /// not a stylistic choice.** Simplifying it to a concatenation reintroduces
    /// a *measured* proof-transfer attack: without the length prefixes, a
    /// context whose fields differ can produce the same byte string, and a proof
    /// made in one session then verifies in another. It is hashed, not
    /// concatenated, because that makes the input fixed-length and reuses the
    /// one length-prefixed, domain-separated hasher the corpus has.
    ///
    /// Field order is part of the protocol. The parameters are a struct rather
    /// than seven positional arguments because `table_id`, `session_id` and
    /// `sender_public_key` are all `[u8; 32]`: a transposition would compile,
    /// would produce a perfectly well-formed context, and would be found only by
    /// two clients failing to agree.
    pub fn build(f: &CtxFields) -> Self {
        let version = f.protocol_version.to_be_bytes();
        let hand = f.hand_id.to_be_bytes();
        let sequence = f.sequence.to_be_bytes();
        let round = [f.position.round_byte()];

        DeckCtx(h(
            Domain::DeckCtx.context(),
            &[
                &version,
                &f.table_id,
                &f.session_id,
                &hand,
                &sequence,
                &round,
                &f.sender_public_key,
            ],
        ))
    }
}

/// Where in the protocol a proof sits, which is what stops a shuffle proof and
/// a reveal proof from ever sharing a context.
///
/// `PROTOCOL.md` §4.5 spends one byte on this: the 0-based position in the
/// shuffle chain, or `0xFF` for everything that is not a shuffle step —
/// `DECK_INIT` ownership proofs and reveal-token DLEQ proofs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofPosition {
    /// The `round`-th shuffler in the chain.
    ShuffleStep(u8),
    /// Not a step of the chain, and so must not share a context with one.
    NotAShuffleStep,
}

impl ProofPosition {
    /// The `round` of a shuffle step, refusing `0xFF`.
    ///
    /// `0xFF` is the sentinel meaning "not a shuffle step", so a step claiming
    /// it would produce a context indistinguishable from a reveal proof of the
    /// same hand — which is exactly the separation §4.5 spends the byte to buy.
    /// Unreachable at ten seats, and refused rather than assumed away, because
    /// the round is an index into a chain whose length arrives from the network.
    pub const fn shuffle_step(round: u8) -> Option<Self> {
        if round == NOT_A_SHUFFLE_STEP {
            None
        } else {
            Some(ProofPosition::ShuffleStep(round))
        }
    }

    const fn round_byte(self) -> u8 {
        match self {
            ProofPosition::ShuffleStep(r) => r,
            ProofPosition::NotAShuffleStep => NOT_A_SHUFFLE_STEP,
        }
    }
}

/// §4.5's sentinel for a context that is not a shuffle-chain step.
const NOT_A_SHUFFLE_STEP: u8 = 0xFF;

/// The seven fields of §4.5, named.
///
/// Three of them are `[u8; 32]` and would transpose silently as positional
/// arguments.
#[derive(Debug, Clone, Copy)]
pub struct CtxFields {
    pub protocol_version: u16,
    /// The table's Ed25519 public key.
    pub table_id: [u8; 32],
    /// `session_id` — the session nonce of `SPEC_CS.md` §14/§20.
    pub session_id: [u8; 32],
    pub hand_id: u64,
    /// The chain stage index of the event carrying the proof.
    pub sequence: u64,
    pub position: ProofPosition,
    /// The emitter's application Ed25519 key.
    pub sender_public_key: [u8; 32],
}

/// Our verified typestate, not the library's.
///
/// Private field, no public constructor and no deserialisation: the only way to
/// hold one is to have run the verification in this module. That is what keeps
/// "never display an unverified card" a compile error, and it survives a swap
/// to a library that has no such type of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified<T>(T);

impl<T> Verified<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
    /// Only this module may mint one.
    pub(crate) fn new(value: T) -> Self {
        Verified(value)
    }
}

impl<T> AsRef<T> for Verified<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

/// A deck that is the **last** link of a completed shuffle chain.
///
/// Every step of a chain verifies, so an intermediate deck's cards are exactly
/// as usable as the final deck's — which is why the distinction is a type. The
/// chain driver, and only it, mints this, and reveal tokens are issued and
/// checked against `Final` decks alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Final<T>(T);

impl<T> Final<T> {
    /// Minted by the shuffle-chain driver alone, once the last link verifies.
    ///
    /// Unused until that driver exists; it is here now because the type is what
    /// stops a reveal token being issued against an intermediate deck, and
    /// adding the type later would mean auditing every call site again.
    #[allow(dead_code)]
    pub(crate) fn new(value: T) -> Self {
        Final(value)
    }
}

impl<T> AsRef<T> for Final<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

/// Why a verification did not produce a `Verified`.
///
/// The distinction is load-bearing and is the whole of condition C-8: under
/// D-014 a tier-1 finding **removes a player**, so a rejection that means "I
/// could not check this" must never be mistaken for one that means "this is
/// invalid". The library returns a bare `Option` for both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// The proof is invalid. Evidence against its signer.
    Invalid(InvalidReason),
    /// Verification could not be attempted. **Never** evidence against anybody.
    CouldNotVerify(Unavailable),
}

/// Why something is invalid — always attributable to whoever signed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidReason {
    /// The zero-knowledge argument did not verify.
    ArgumentFailed,
    /// A deck position is the group identity, so its card is not hidden at all.
    IdentityCiphertext { position: usize },
    /// Two deck positions share a ciphertext coordinate.
    ///
    /// A shuffler can arrange this deliberately, and it is what makes a reveal
    /// token issued for one card open another.
    DuplicateCiphertext { first: usize, second: usize },
    /// A card of the output deck appears unchanged in the input deck, so it was
    /// not re-masked and is trackable through the shuffle.
    CardNotRemasked { position: usize },
    /// The output deck equals the input deck. A valid proof, and not a shuffle.
    DeckUnchanged,
    /// The bytes were not a well-formed, canonical, correctly sized encoding.
    ///
    /// Always attributable: a decode either ran or it did not, and neither
    /// answer depends on state this receiver might be missing. So unlike a
    /// verification, a decode failure is evidence against whoever sent it.
    Decode(DecodeError),
    /// A deck is not the length the game plays with.
    ///
    /// Checked first and separately, because everything after it is a claim
    /// about a permutation of a fixed set — and because the length arrives from
    /// the network.
    WrongDeckLength { expected: usize, got: usize },
    /// A public key is the group identity, for which an ownership proof anyone
    /// can write verifies.
    IdentityPublicKey { seat: usize },
    /// Two seats offered the same public key.
    DuplicatePublicKey { first: usize, second: usize },
}

/// Why verification could not be attempted. Not attributable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// The input deck this proof is against is not held.
    InputDeckUnknown,
    /// The aggregate key of the hand is not yet established.
    AggregateKeyUnknown,
    /// The context could not be derived from chained state.
    ContextUnknown,
}

/// A deck position's ciphertext, as this boundary sees it.
///
/// Opaque bytes: the compressed, validated encoding of the pair. The boundary
/// compares and hashes them and never interprets them, so the structural check
/// is independent of which library is underneath.
pub type Ciphertext = [u8; 66];

/// The first coordinate of a ciphertext, which is what a reveal token binds.
pub fn c1_of(ct: &Ciphertext) -> &[u8] {
    &ct[..33]
}

/// The identity element in compressed form: the encoding whose leading byte
/// marks the point at infinity.
///
/// A deck position holding it is not hidden at all.
fn is_identity_c1(ct: &Ciphertext) -> bool {
    // arkworks marks infinity with a flag bit in the final byte of the
    // coordinate; an all-zero coordinate is the other degenerate form. Both are
    // refused, because a boundary that guesses wrong here refuses too little.
    let c1 = c1_of(ct);
    c1.iter().all(|&b| b == 0) || (c1[32] & 0x40) != 0
}

/// The structural check that closes three of the review's four demonstrated
/// exploits, before the argument is even attempted.
///
/// Bayer–Groth proves that the output deck is a permutation of the input under
/// *some* re-masking. It does not prove the re-masking hides anything: the
/// scalars are unconstrained, so leaving chosen slots in plaintext, reusing one
/// scalar for every card, and shuffling a deck to itself are all valid.
///
/// Five conditions, checked cheapest first:
///
/// 0. **Both decks are exactly `expected` cards.** First, because everything
///    after it is a claim about a permutation of a fixed set, and because the
///    length arrives from the network — without this gate the loops below run
///    on an attacker's number.
/// 1. **No position is the identity.** Such a card is in the clear.
/// 2. **The coordinates are pairwise distinct.** A collision is what lets a
///    reveal token issued for one card open another.
/// 3. **No output coordinate appears in the input.** An unchanged card is
///    trackable straight through the shuffle.
/// 4. **The deck changed.** The identity permutation verifies otherwise.
///
/// Linear in the deck size, and microseconds against the argument's tens of
/// milliseconds — so it also makes a bogus proof cheap to reject rather than
/// expensive, which is the ordering §4.0 requires and which an earlier version
/// of this function inverted by being quadratic on a length it never checked.
pub fn structural_check(
    prev: &[Ciphertext],
    next: &[Ciphertext],
    expected: usize,
) -> Result<(), InvalidReason> {
    for deck in [prev, next] {
        if deck.len() != expected {
            return Err(InvalidReason::WrongDeckLength { expected, got: deck.len() });
        }
    }

    if prev == next {
        return Err(InvalidReason::DeckUnchanged);
    }

    for (i, ct) in next.iter().enumerate() {
        if is_identity_c1(ct) {
            return Err(InvalidReason::IdentityCiphertext { position: i });
        }
    }

    // Sets rather than nested loops: the input length is checked above, but a
    // check whose whole purpose is to be cheap should not be quadratic even
    // when it is safe.
    let mut seen: HashSet<&[u8]> = HashSet::with_capacity(next.len());
    for (i, ct) in next.iter().enumerate() {
        if !seen.insert(c1_of(ct)) {
            let first = next
                .iter()
                .position(|o| c1_of(o) == c1_of(ct))
                .expect("the duplicate has an earlier occurrence");
            return Err(InvalidReason::DuplicateCiphertext { first, second: i });
        }
    }

    let previous: HashSet<&[u8]> = prev.iter().map(c1_of).collect();
    for (i, ct) in next.iter().enumerate() {
        if previous.contains(c1_of(ct)) {
            return Err(InvalidReason::CardNotRemasked { position: i });
        }
    }

    Ok(())
}

/// The structural check for the **first** link, whose input is the open deck.
///
/// Three of [`structural_check`]'s five conditions look only at `next`, and the
/// other two are **subsumed** when the input is the open deck — not skipped:
///
/// - *No output coordinate appears in the input.* Every input `c1` is the
///   identity, and the identity check has already refused an output `c1` that is
///   the identity. So no output coordinate can equal an input one.
/// - *The deck changed.* Same argument: the input is all identity, and an output
///   containing no identity cannot equal it.
///
/// Both hold **only because the identity check runs**, which is why it runs
/// first here and not as an afterthought.
///
/// The two functions therefore accept and reject exactly the same decks, which
/// is measured in
/// [`the_initial_check_accepts_and_rejects_the_same_decks`](tests::the_initial_check_accepts_and_rejects_the_same_decks)
/// rather than argued. They can name a **different reason** for one input: a
/// deck returned unchanged is `DeckUnchanged` to the full check and
/// `IdentityCiphertext` to this one. Both are refusals and both are evidence;
/// for the first link the second reason is the truer one, because an output
/// equal to the open deck is not merely unchanged, it is fifty-two cards left
/// face up.
///
/// The alternative was to hand the full check a fabricated all-identity deck,
/// which would have been bytes invented to satisfy a test — the kind of thing
/// that is true when written and quietly false a year later.
pub fn structural_check_initial(
    next: &[Ciphertext],
    expected: usize,
) -> Result<(), InvalidReason> {
    if next.len() != expected {
        return Err(InvalidReason::WrongDeckLength {
            expected,
            got: next.len(),
        });
    }

    for (i, ct) in next.iter().enumerate() {
        if is_identity_c1(ct) {
            return Err(InvalidReason::IdentityCiphertext { position: i });
        }
    }

    let mut seen: HashSet<&[u8]> = HashSet::with_capacity(next.len());
    for (i, ct) in next.iter().enumerate() {
        if !seen.insert(c1_of(ct)) {
            let first = next
                .iter()
                .position(|o| c1_of(o) == c1_of(ct))
                .expect("the duplicate has an earlier occurrence");
            return Err(InvalidReason::DuplicateCiphertext { first, second: i });
        }
    }

    Ok(())
}

/// The key-set check, run when the hand's keys arrive and before any ownership
/// proof is verified.
///
/// The identity public key passes the library's ownership proof with a proof
/// anyone can write and no secret at all. Duplicate keys aggregate silently.
pub fn check_key_set(keys: &[[u8; 33]]) -> Result<(), InvalidReason> {
    for (seat, key) in keys.iter().enumerate() {
        if key.iter().all(|&b| b == 0) || (key[32] & 0x40) != 0 {
            return Err(InvalidReason::IdentityPublicKey { seat });
        }
    }
    for i in 0..keys.len() {
        for j in (i + 1)..keys.len() {
            if keys[i] == keys[j] {
                return Err(InvalidReason::DuplicatePublicKey { first: i, second: j });
            }
        }
    }
    Ok(())
}


// ---------------------------------------------------------------------------
// The wire boundary (conditions C-1, C-2, C-6, C-9)
// ---------------------------------------------------------------------------

/// Why a byte string was refused.
///
/// Always attributable: a decode either ran or it did not, and neither answer
/// depends on state this receiver might be missing. So unlike a verification,
/// a decode failure is always evidence against whoever sent it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// Not exactly the length this type has.
    ///
    /// Checked before parsing, because the upstream deserialisers **ignore
    /// trailing bytes** — so without this a proof and that proof with a
    /// megabyte appended are the same object to the library and two different
    /// objects to anything that hashes or deduplicates them.
    WrongLength { expected: usize, got: usize },
    /// The bytes are not a valid encoding of this type.
    Malformed,
    /// The value parsed, but its encoding is not the canonical one.
    ///
    /// The measured reason this exists: the upstream encoding accepts at least
    /// 2^624 distinct byte strings for one 52-card deck, because six bits per
    /// point are read and discarded. Anyone holding no secret at all can emit
    /// unbounded distinct byte strings that every peer accepts as the same
    /// value, which defeats deduplication, message identity, and any signature
    /// taken over the received bytes rather than over the canonical form.
    NonCanonical,
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::WrongLength { expected, got } => {
                write!(f, "expected exactly {expected} bytes, got {got}")
            }
            DecodeError::Malformed => write!(f, "not a valid encoding"),
            DecodeError::NonCanonical => write!(f, "not the canonical encoding"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl From<DecodeError> for InvalidReason {
    fn from(e: DecodeError) -> Self {
        InvalidReason::Decode(e)
    }
}

/// Every type that crosses the network into the cryptographic layer.
///
/// An implementor writes [`decode_raw`](DeckWire::decode_raw) and
/// [`encode`](DeckWire::encode); [`decode`](DeckWire::decode) is provided and
/// enforces the two conditions that must not be left to a caller's memory —
/// exact length before parsing, and re-encode-and-compare after it.
///
/// `encode` is the **only** form that is ever hashed, signed, deduplicated or
/// used as a message identifier. Received bytes are never any of those things.
pub trait DeckWire: Sized + PartialEq {
    /// The one length a valid encoding of this type has.
    const LEN: usize;

    /// Parse, having been given exactly `LEN` bytes.
    ///
    /// Must use the compressed, validated form and no unchecked path: the
    /// uncompressed path with validation off accepted two million of two
    /// million random byte strings as points not on the curve.
    fn decode_raw(bytes: &[u8]) -> Result<Self, DecodeError>;

    /// The canonical encoding.
    fn encode(&self) -> Vec<u8>;

    /// Parse bytes that arrived from outside.
    ///
    /// Length first, then the parse, then the round-trip comparison. The order
    /// is the cheap-before-expensive ordering the protocol requires, and the
    /// comparison is what makes one value one byte string.
    fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() != Self::LEN {
            return Err(DecodeError::WrongLength { expected: Self::LEN, got: bytes.len() });
        }
        let value = Self::decode_raw(bytes)?;
        if value.encode() != bytes {
            return Err(DecodeError::NonCanonical);
        }
        Ok(value)
    }
}

/// One shuffle proof per peer per position per hand (condition C-9).
///
/// A second is a protocol violation rather than something to verify, which
/// matters because rejecting one bogus proof costs 10 to 36 ms of verification
/// **at every seat**: a single peer on a slow link can otherwise burn most of a
/// core at every other player's client.
///
/// So the admission check comes before the expensive work, and it is a
/// structure with a bound rather than a counter, because the position and the
/// sender both arrive from the network.
#[derive(Debug, Default, Clone)]
pub struct ShuffleAdmission {
    seen: HashSet<(u8, u8)>,
}

/// Why a shuffle proof was not admitted for verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotAdmitted {
    /// This seat already submitted a proof for this position in this hand.
    AlreadySubmitted { seat: u8, position: u8 },
    /// A seat or position outside the table.
    OutOfRange { seat: u8, position: u8 },
}

impl ShuffleAdmission {
    /// Admit one proof for verification, or say why not.
    ///
    /// Called **before** `verify_shuffle`, never after: the point is to spend
    /// nothing on a proof that is not allowed to exist.
    /// **Two bounds, because they are two quantities.** A seat index and a
    /// position in the shuffle order are not the same number and do not have
    /// the same range: the order holds the seats that are dealt in, so at a
    /// table of three with one seat certified absent it has two entries while
    /// the remaining seats are still numbered 1 and 2.
    ///
    /// One parameter served both, and the caller passed the order's length, so
    /// **`seat >= seats` refused any seat whose index reached the number of
    /// shufflers** — on its first attempt, with nothing submitted. It could
    /// then never take its turn, the chain could never complete, and every hand
    /// stalled at the same point. Measured over four hands of one live run:
    /// `own shuffle refused at round 1: chain step 1, turn Some(2)`, which says
    /// the chain was waiting for exactly the seat it was refusing.
    pub fn admit(
        &mut self,
        seat: u8,
        position: u8,
        seats: u8,
        positions: u8,
    ) -> Result<(), NotAdmitted> {
        if seat >= seats || position >= positions {
            return Err(NotAdmitted::OutOfRange { seat, position });
        }
        if !self.seen.insert((seat, position)) {
            return Err(NotAdmitted::AlreadySubmitted { seat, position });
        }
        Ok(())
    }

    /// How many proofs have been admitted this hand.
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

/// The construction, behind one boundary.
///
/// An implementor writes [`verify_argument`](DeckCrypto::verify_argument) and
/// **not** `verify_shuffle`: the structural check has to run inside the
/// verification rather than beside it, and a provided method is how that stops
/// being something a caller remembers.
pub trait DeckCrypto {
    /// How many cards the deck holds.
    const DECK_LEN: usize = 52;

    /// Verify the zero-knowledge argument alone.
    ///
    /// Called only after the structural check has passed, so an implementor
    /// never has to repeat it.
    fn verify_argument(
        &self,
        prev: &[Ciphertext],
        next: &[Ciphertext],
        proof: &[u8],
        ctx: &DeckCtx,
    ) -> Result<(), VerifyOutcome>;

    /// Verify the argument for the **first** link, whose input is the open deck.
    ///
    /// Separate because the open deck is not a deck anyone shuffled and is not a
    /// deck this peer verified: it is the library's own starting point, and only
    /// the library can state it. A chain that had to name it would be a chain
    /// carrying a constant it cannot check.
    fn verify_initial_argument(
        &self,
        next: &[Ciphertext],
        proof: &[u8],
        ctx: &DeckCtx,
    ) -> Result<(), VerifyOutcome>;

    /// Verify the first link of a shuffle chain.
    ///
    /// [`structural_check_initial`] first, for the same reason and in the same
    /// order as below.
    fn verify_initial_shuffle(
        &self,
        next: &[Ciphertext],
        proof: &[u8],
        ctx: &DeckCtx,
    ) -> Result<Verified<Vec<Ciphertext>>, VerifyOutcome> {
        structural_check_initial(next, Self::DECK_LEN).map_err(VerifyOutcome::Invalid)?;
        self.verify_initial_argument(next, proof, ctx)?;
        Ok(Verified::new(next.to_vec()))
    }

    /// Verify one later link of a shuffle chain.
    ///
    /// Structure first, then the argument. The order is not an optimisation:
    /// the structural failures are the ones the argument does not catch.
    fn verify_shuffle(
        &self,
        prev: &[Ciphertext],
        next: &[Ciphertext],
        proof: &[u8],
        ctx: &DeckCtx,
    ) -> Result<Verified<Vec<Ciphertext>>, VerifyOutcome> {
        structural_check(prev, next, Self::DECK_LEN).map_err(VerifyOutcome::Invalid)?;
        self.verify_argument(prev, next, proof, ctx)?;
        Ok(Verified::new(next.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ct(seed: u8) -> Ciphertext {
        let mut c = [0u8; 66];
        // A plausible compressed point: leading tag, then distinct body, and no
        // infinity flag.
        c[0] = 0x02;
        c[1] = seed;
        c[33] = 0x03;
        c[34] = seed ^ 0xFF;
        c
    }

    fn deck(seeds: &[u8]) -> Vec<Ciphertext> {
        seeds.iter().map(|&s| ct(s)).collect()
    }

    /// The deck size the tests below use, so a length gate does not have to
    /// mean building 52 cards in every case.
    const N: usize = 4;

    #[test]
    fn an_honest_looking_shuffle_passes_the_structural_check() {
        let prev = deck(&[1, 2, 3, 4]);
        let next = deck(&[10, 11, 12, 13]);
        assert_eq!(structural_check(&prev, &next, N), Ok(()));
    }

    /// A valid proof, and not a shuffle. Bayer–Groth verifies the identity
    /// permutation quite happily.
    #[test]
    fn a_deck_shuffled_to_itself_is_refused() {
        let prev = deck(&[1, 2, 3, 4]);
        assert_eq!(
            structural_check(&prev, &prev.clone(), N),
            Err(InvalidReason::DeckUnchanged)
        );
    }

    /// The measured attack: a shuffler leaves chosen slots readable. The
    /// argument does not constrain the re-masking scalars at all.
    #[test]
    fn a_position_left_in_the_clear_is_refused() {
        let prev = deck(&[1, 2, 3, 4]);
        let mut next = deck(&[10, 11, 12, 13]);
        next[2] = [0u8; 66];
        assert_eq!(
            structural_check(&prev, &next, N),
            Err(InvalidReason::IdentityCiphertext { position: 2 })
        );

        // The other degenerate form: the infinity flag set.
        let mut flagged = deck(&[10, 11, 12, 13]);
        flagged[1][32] |= 0x40;
        assert_eq!(
            structural_check(&prev, &flagged, N),
            Err(InvalidReason::IdentityCiphertext { position: 1 })
        );
    }

    /// The collision a malicious shuffler arranges so that a reveal token
    /// issued for one card opens another. This is the exploit path of the
    /// review's most serious finding, and it is closed here rather than in the
    /// library.
    #[test]
    fn two_positions_sharing_a_coordinate_are_refused() {
        let prev = deck(&[1, 2, 3, 4]);
        let mut next = deck(&[10, 11, 12, 13]);
        // Same c1, different c2 — exactly the shape the token defect needs.
        let shared = c1_of(&next[0]).to_vec();
        next[3][..33].copy_from_slice(&shared);
        assert_eq!(
            structural_check(&prev, &next, N),
            Err(InvalidReason::DuplicateCiphertext { first: 0, second: 3 })
        );
    }

    /// An unchanged card is trackable straight through the shuffle, which is
    /// how a permutation becomes public.
    #[test]
    fn a_card_carried_through_unremasked_is_refused() {
        let prev = deck(&[1, 2, 3, 4]);
        let mut next = deck(&[10, 11, 12, 13]);
        next[1] = prev[2];
        assert_eq!(
            structural_check(&prev, &next, N),
            Err(InvalidReason::CardNotRemasked { position: 1 })
        );
    }


    /// The gate an earlier version of this function did not have. Without it a
    /// deck of three cards was accepted against a deck of fifty-two - so a
    /// non-permutation passed the boundary - and the loops ran on a length the
    /// attacker chose, in a function whose whole purpose is to be the cheap
    /// check that runs before the expensive one.
    #[test]
    fn a_deck_of_the_wrong_length_is_refused_before_anything_else() {
        let full = deck(&[1, 2, 3, 4]);
        let short = deck(&[10, 11, 12]);

        assert_eq!(
            structural_check(&full, &short, N),
            Err(InvalidReason::WrongDeckLength { expected: N, got: 3 })
        );
        assert_eq!(
            structural_check(&short, &full, N),
            Err(InvalidReason::WrongDeckLength { expected: N, got: 3 })
        );

        // An empty deck is the degenerate case, and it must not read as an
        // unchanged deck or as a clean permutation.
        assert_eq!(
            structural_check(&[], &[], N),
            Err(InvalidReason::WrongDeckLength { expected: N, got: 0 })
        );
    }

    /// The length is checked before the identity scan, so a long attacker deck
    /// full of degenerate cards costs one comparison rather than a walk.
    #[test]
    fn the_length_gate_runs_first() {
        let prev = deck(&[1, 2, 3, 4]);
        let mut huge = vec![[0u8; 66]; 10_000];
        huge[0][0] = 0x02;
        assert_eq!(
            structural_check(&prev, &huge, N),
            Err(InvalidReason::WrongDeckLength { expected: N, got: 10_000 }),
            "the length, not the first degenerate card"
        );
    }

    #[test]
    fn the_default_deck_length_is_the_one_the_game_plays_with() {
        struct Any;
        impl DeckCrypto for Any {
            fn verify_initial_argument(
                &self,
                _n: &[Ciphertext],
                _pr: &[u8],
                _c: &DeckCtx,
            ) -> Result<(), VerifyOutcome> {
                Ok(())
            }
            fn verify_argument(
                &self,
                _p: &[Ciphertext],
                _n: &[Ciphertext],
                _pr: &[u8],
                _c: &DeckCtx,
            ) -> Result<(), VerifyOutcome> {
                Ok(())
            }
        }
        assert_eq!(Any::DECK_LEN, 52);

        // And the provided method passes it, so a four-card deck is refused
        // even though it is otherwise sound.
        let ctx = DeckCtx::from_hash([1u8; 32]);
        assert_eq!(
            Any.verify_shuffle(&deck(&[1, 2, 3, 4]), &deck(&[9, 8, 7, 6]), b"", &ctx),
            Err(VerifyOutcome::Invalid(InvalidReason::WrongDeckLength {
                expected: 52,
                got: 4
            }))
        );
    }


    /// The identity in compressed form, as `tests/deck_constants.rs` measures it
    /// against arkworks: 32 zero bytes and the infinity flag.
    fn open_deck_card(seed: u8) -> Ciphertext {
        let mut c = [0u8; 66];
        c[32] = 0x40; // c1 = identity
        c[33] = 0x02; // c2 = the plaintext card point, whatever it is
        c[34] = seed;
        c
    }

    /// The two conditions dropped from the initial check are subsumed, not
    /// waived. Measured over the shapes an attacker gets to choose, so the
    /// argument in the doc comment is checked rather than believed.
    ///
    /// The assertion is on accept-versus-reject, because the two can name
    /// different reasons for one input and the doc comment says which.
    #[test]
    fn the_initial_check_accepts_and_rejects_the_same_decks() {
        let open: Vec<Ciphertext> = (0..4u8).map(open_deck_card).collect();

        let cases: Vec<(&str, Vec<Ciphertext>)> = vec![
            ("an honest shuffle", deck(&[10, 11, 12, 13])),
            ("a position left in the clear", {
                let mut d = deck(&[10, 11, 12, 13]);
                d[2] = open_deck_card(2);
                d
            }),
            ("two positions sharing a coordinate", {
                let mut d = deck(&[10, 11, 12, 13]);
                d[3] = d[1];
                d
            }),
            ("the open deck returned unchanged", open.clone()),
            ("a short deck", deck(&[10, 11])),
        ];

        for (name, next) in cases {
            let initial = structural_check_initial(&next, N);
            let full = structural_check(&open, &next, N);
            assert_eq!(
                initial.is_ok(),
                full.is_ok(),
                "the two checks disagreed on: {name} ({initial:?} vs {full:?})"
            );
        }
    }


    /// And the one input where the reasons differ, pinned so the doc comment
    /// cannot drift away from the code.
    #[test]
    fn an_untouched_open_deck_is_fifty_two_cards_face_up() {
        let open: Vec<Ciphertext> = (0..4u8).map(open_deck_card).collect();
        assert_eq!(
            structural_check_initial(&open, N),
            Err(InvalidReason::IdentityCiphertext { position: 0 })
        );
        assert_eq!(
            structural_check(&open, &open.clone(), N),
            Err(InvalidReason::DeckUnchanged)
        );
    }

    #[test]
    fn the_identity_public_key_is_refused() {
        let good = [[0x02u8; 33], [0x03u8; 33]];
        assert_eq!(check_key_set(&good), Ok(()));

        let mut zero = good;
        zero[1] = [0u8; 33];
        assert_eq!(
            check_key_set(&zero),
            Err(InvalidReason::IdentityPublicKey { seat: 1 })
        );

        let mut flagged = good;
        flagged[0][32] |= 0x40;
        assert_eq!(
            check_key_set(&flagged),
            Err(InvalidReason::IdentityPublicKey { seat: 0 })
        );
    }

    #[test]
    fn two_seats_offering_one_key_are_refused() {
        let same = [[0x02u8; 33], [0x03u8; 33], [0x02u8; 33]];
        assert_eq!(
            check_key_set(&same),
            Err(InvalidReason::DuplicatePublicKey { first: 0, second: 2 })
        );
    }

    /// The structural check must run **inside** the verification, so an
    /// implementor cannot forget it. This test proves the provided method's
    /// ordering by using an implementation whose argument check always
    /// succeeds: a structurally broken deck must still be refused.
    #[test]
    fn the_structural_check_cannot_be_skipped_by_an_implementor() {
        struct AlwaysAccepts;
        impl DeckCrypto for AlwaysAccepts {
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

        let ctx = DeckCtx::from_hash([7u8; 32]);
        let prev: Vec<Ciphertext> = (0..52u8).map(ct).collect();

        // A deck that would pass any argument, and must not pass this.
        assert_eq!(
            AlwaysAccepts.verify_shuffle(&prev, &prev.clone(), b"", &ctx),
            Err(VerifyOutcome::Invalid(InvalidReason::DeckUnchanged))
        );

        // And an honest one still gets through.
        let next: Vec<Ciphertext> = (100..152u8).map(ct).collect();
        assert!(AlwaysAccepts.verify_shuffle(&prev, &next, b"", &ctx).is_ok());
    }

    /// Under D-014 a tier-1 finding removes a player, so "invalid" and "could
    /// not verify" must never be one value. The library returns a bare `Option`
    /// for both, which is why this distinction lives here.
    #[test]
    fn invalid_and_could_not_verify_are_different_things() {
        struct Unavailable_;
        impl DeckCrypto for Unavailable_ {
            fn verify_initial_argument(
                &self,
                _next: &[Ciphertext],
                _proof: &[u8],
                _ctx: &DeckCtx,
            ) -> Result<(), VerifyOutcome> {
                Err(VerifyOutcome::CouldNotVerify(
                    super::Unavailable::AggregateKeyUnknown,
                ))
            }
            fn verify_argument(
                &self,
                _prev: &[Ciphertext],
                _next: &[Ciphertext],
                _proof: &[u8],
                _ctx: &DeckCtx,
            ) -> Result<(), VerifyOutcome> {
                Err(VerifyOutcome::CouldNotVerify(
                    super::Unavailable::AggregateKeyUnknown,
                ))
            }
        }

        let ctx = DeckCtx::from_hash([7u8; 32]);
        let prev: Vec<Ciphertext> = (0..52u8).map(ct).collect();
        let next: Vec<Ciphertext> = (100..152u8).map(ct).collect();

        let outcome = Unavailable_.verify_shuffle(&prev, &next, b"", &ctx);
        assert_eq!(
            outcome,
            Err(VerifyOutcome::CouldNotVerify(
                super::Unavailable::AggregateKeyUnknown
            )),
            "a missing input must never be attributed to the signer"
        );
        assert!(
            !matches!(outcome, Err(VerifyOutcome::Invalid(_))),
            "and must never be evidence"
        );
    }

    /// A `Verified` cannot be forged from outside this module: it has no public
    /// constructor and no deserialisation. That is what keeps "never display an
    /// unverified card" a compile error rather than a convention.
    #[test]
    fn a_verified_deck_can_only_come_from_verification() {
        struct Ok_;
        impl DeckCrypto for Ok_ {
            fn verify_initial_argument(
                &self,
                _n: &[Ciphertext],
                _pr: &[u8],
                _c: &DeckCtx,
            ) -> Result<(), VerifyOutcome> {
                Ok(())
            }
            fn verify_argument(
                &self,
                _p: &[Ciphertext],
                _n: &[Ciphertext],
                _pr: &[u8],
                _c: &DeckCtx,
            ) -> Result<(), VerifyOutcome> {
                Ok(())
            }
        }
        let ctx = DeckCtx::from_hash([1u8; 32]);
        let prev: Vec<Ciphertext> = (0..52u8).map(ct).collect();
        let next: Vec<Ciphertext> = (100..152u8).map(ct).collect();
        let verified = Ok_
            .verify_shuffle(&prev, &next, b"", &ctx)
            .expect("structurally sound and the argument accepted");
        assert_eq!(verified.as_ref().len(), 52);
    }


    /// A type whose canonical encoding has one spare bit, so a second byte
    /// string decodes to the same value. That is the shape of the real defect:
    /// upstream reads and discards six bits per point.
    #[derive(Debug, PartialEq)]
    struct Sloppy(u8);

    impl DeckWire for Sloppy {
        const LEN: usize = 2;
        fn decode_raw(bytes: &[u8]) -> Result<Self, DecodeError> {
            // The top bit of the second byte is read and ignored, exactly as
            // the real deserialiser ignores its flag bits.
            Ok(Sloppy(bytes[0]))
        }
        fn encode(&self) -> Vec<u8> {
            vec![self.0, 0x00]
        }
    }

    #[test]
    fn the_canonical_encoding_round_trips() {
        let bytes = Sloppy(7).encode();
        assert_eq!(Sloppy::decode(&bytes), Ok(Sloppy(7)));
    }

    /// The length gate runs before the parse, because the upstream
    /// deserialisers ignore trailing bytes - so a proof and that proof with a
    /// megabyte appended would be one object to the library and two to
    /// anything that hashes or deduplicates it.
    #[test]
    fn trailing_bytes_are_refused_before_parsing() {
        let mut bytes = Sloppy(7).encode();
        bytes.push(0xFF);
        assert_eq!(
            Sloppy::decode(&bytes),
            Err(DecodeError::WrongLength { expected: 2, got: 3 })
        );
        assert_eq!(
            Sloppy::decode(&[7]),
            Err(DecodeError::WrongLength { expected: 2, got: 1 })
        );
        assert_eq!(
            Sloppy::decode(&[]),
            Err(DecodeError::WrongLength { expected: 2, got: 0 })
        );
    }

    /// The round-trip comparison, which is what makes one value one byte
    /// string. Without it anyone holding no secret can emit unbounded distinct
    /// encodings that every peer accepts as the same thing.
    #[test]
    fn a_second_encoding_of_one_value_is_refused() {
        let canonical = Sloppy(7).encode();
        let variant = vec![7, 0x80]; // decodes to the same value

        assert_eq!(Sloppy::decode(&canonical), Ok(Sloppy(7)));
        assert_eq!(
            Sloppy::decode(&variant),
            Err(DecodeError::NonCanonical),
            "two byte strings for one value defeats dedup, message ids and \
             any signature over received bytes"
        );
    }

    /// **A seat index and a position in the shuffle order are two quantities.**
    ///
    /// The order holds the seats that are dealt in, so at a table of three with
    /// one seat certified absent it has two entries while the remaining seats
    /// are still numbered 1 and 2. One parameter bounded both and the caller
    /// passed the order's length, so seat 2 was refused at position 1 of 2 — on
    /// its first attempt, with nothing submitted. It could then never take its
    /// turn, the chain could never complete, and every hand stalled at the same
    /// point: `own shuffle refused at round 1: chain step 1, turn Some(2)`,
    /// which says the chain was waiting for exactly the seat it refused.
    ///
    /// **To make this fail:** bound `position` and `seat` by the same argument
    /// again.
    #[test]
    fn a_seat_may_shuffle_at_a_position_below_its_own_index() {
        let mut admission = ShuffleAdmission::default();
        // Two shufflers, seats 1 and 2, which is a three-seat table that has
        // certified one seat absent.
        assert_eq!(
            admission.admit(1, 0, 10, 2),
            Ok(()),
            "seat 1 takes position 0 of 2"
        );
        assert_eq!(
            admission.admit(2, 1, 10, 2),
            Ok(()),
            "and seat 2 takes position 1 of 2, which is the case that stalled"
        );

        // Both bounds still bite, in their own directions.
        assert_eq!(
            admission.admit(10, 0, 10, 2),
            Err(NotAdmitted::OutOfRange {
                seat: 10,
                position: 0
            }),
            "a seat outside the table"
        );
        assert_eq!(
            admission.admit(1, 2, 10, 2),
            Err(NotAdmitted::OutOfRange {
                seat: 1,
                position: 2
            }),
            "a position outside the order"
        );
    }

    #[test]
    fn one_shuffle_proof_per_seat_per_position() {
        let mut admission = ShuffleAdmission::default();
        assert_eq!(admission.admit(0, 0, 6, 6), Ok(()));
        assert_eq!(admission.admit(0, 1, 6, 6), Ok(()), "a different position is fine");
        assert_eq!(admission.admit(1, 0, 6, 6), Ok(()), "a different seat is fine");

        assert_eq!(
            admission.admit(0, 0, 6, 6),
            Err(NotAdmitted::AlreadySubmitted { seat: 0, position: 0 }),
            "a second proof is a protocol violation, not something to verify"
        );
        assert_eq!(admission.len(), 3);
    }

    /// Both indices arrive from the network, so both are bounded before they
    /// reach a structure.
    #[test]
    fn a_seat_or_position_outside_the_table_is_refused() {
        let mut admission = ShuffleAdmission::default();
        assert_eq!(
            admission.admit(6, 0, 6, 6),
            Err(NotAdmitted::OutOfRange { seat: 6, position: 0 })
        );
        assert_eq!(
            admission.admit(0, 200, 6, 6),
            Err(NotAdmitted::OutOfRange { seat: 0, position: 200 })
        );
        assert!(admission.is_empty(), "neither was recorded");
    }

    /// A whole table's worth of proofs fits, and nothing beyond it does.
    #[test]
    fn admission_is_bounded_by_the_table() {
        let mut admission = ShuffleAdmission::default();
        for seat in 0..10 {
            for position in 0..10 {
                assert_eq!(admission.admit(seat, position, 10, 10), Ok(()));
            }
        }
        assert_eq!(admission.len(), 100);
        assert!(admission.admit(0, 0, 10, 10).is_err());
    }


    fn fields() -> CtxFields {
        CtxFields {
            protocol_version: 1,
            table_id: [1u8; 32],
            session_id: [2u8; 32],
            hand_id: 7,
            sequence: 19,
            position: ProofPosition::NotAShuffleStep,
            sender_public_key: [3u8; 32],
        }
    }

    /// Every field is bound. A field that fell out of the hash would be a field
    /// a proof is not bound to, and the whole purpose of the context is that a
    /// proof made in one place does not verify in another.
    #[test]
    fn every_field_of_the_context_changes_it() {
        let base = DeckCtx::build(&fields());

        let mut mutations: Vec<(&str, CtxFields)> = Vec::new();
        let mut f = fields();
        f.protocol_version = 2;
        mutations.push(("protocol_version", f));
        let mut f = fields();
        f.table_id = [9u8; 32];
        mutations.push(("table_id", f));
        let mut f = fields();
        f.session_id = [9u8; 32];
        mutations.push(("session_id", f));
        let mut f = fields();
        f.hand_id = 8;
        mutations.push(("hand_id", f));
        let mut f = fields();
        f.sequence = 20;
        mutations.push(("sequence", f));
        let mut f = fields();
        f.position = ProofPosition::ShuffleStep(0);
        mutations.push(("position", f));
        let mut f = fields();
        f.sender_public_key = [9u8; 32];
        mutations.push(("sender_public_key", f));

        assert_eq!(mutations.len(), 7, "all seven fields of section 4.5");
        for (name, m) in mutations {
            assert_ne!(base, DeckCtx::build(&m), "{name} is not bound into ctx");
        }
    }

    /// The replay the protocol requires be impossible: seat 2 rebroadcasting
    /// its own valid proof as seat 3, and the same proof carried into the next
    /// hand.
    #[test]
    fn a_proof_context_does_not_transfer_between_seats_or_hands() {
        let seat_2 = DeckCtx::build(&CtxFields {
            position: ProofPosition::shuffle_step(2).unwrap(),
            ..fields()
        });
        let seat_3 = DeckCtx::build(&CtxFields {
            position: ProofPosition::shuffle_step(3).unwrap(),
            ..fields()
        });
        assert_ne!(seat_2, seat_3);

        let next_hand = DeckCtx::build(&CtxFields {
            position: ProofPosition::shuffle_step(2).unwrap(),
            hand_id: 8,
            ..fields()
        });
        assert_ne!(seat_2, next_hand);
    }

    /// A shuffle step and a reveal proof of the same hand must never share a
    /// context, which is the one byte section 4.5 spends on the distinction.
    #[test]
    fn a_shuffle_step_never_shares_a_context_with_a_reveal() {
        let reveal = DeckCtx::build(&CtxFields {
            position: ProofPosition::NotAShuffleStep,
            ..fields()
        });
        for round in 0..=254u8 {
            let step = DeckCtx::build(&CtxFields {
                position: ProofPosition::shuffle_step(round).unwrap(),
                ..fields()
            });
            assert_ne!(step, reveal, "round {round} collided with the sentinel");
        }
    }

    /// And the sentinel cannot be claimed as a round, which is how that
    /// separation is enforced rather than hoped for.
    #[test]
    fn the_sentinel_is_not_a_round() {
        assert_eq!(ProofPosition::shuffle_step(0xFF), None);
        assert_eq!(
            ProofPosition::shuffle_step(0),
            Some(ProofPosition::ShuffleStep(0))
        );
        assert_eq!(
            ProofPosition::shuffle_step(254),
            Some(ProofPosition::ShuffleStep(254))
        );
    }

    /// The construction is deterministic, or two honest peers disagree about
    /// every proof.
    #[test]
    fn the_context_is_a_function_of_its_fields() {
        assert_eq!(DeckCtx::build(&fields()), DeckCtx::build(&fields()));
    }

    #[test]
    fn the_context_is_fixed_width_and_opaque() {
        let ctx = DeckCtx::from_hash([0xAB; 32]);
        assert_eq!(ctx.as_bytes().len(), 32);
        assert_eq!(ctx, DeckCtx::from_hash([0xAB; 32]));
        assert_ne!(ctx, DeckCtx::from_hash([0xAC; 32]));
    }
}
