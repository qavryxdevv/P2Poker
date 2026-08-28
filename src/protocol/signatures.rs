//! Application signatures, and the closed register of domain strings.
//!
//! `SPEC_CS.md` section 12 requires every critical event to be signed over
//! precisely defined bytes. Section 20 requires the poker application identity
//! to be a **different** key from the libp2p `PeerId`, because a `PeerId` says
//! which socket you are talking to, not who is playing.
//!
//! # The domain register is a closed enum, not a string
//!
//! Every signature and every hash in this protocol is bound to a purpose. If
//! that purpose were a `&str` chosen at the call site, two call sites could
//! disagree by a character and a value signed for one purpose would verify for
//! another — which is a replay across constructions, not a typo.
//!
//! So [`Domain`] is a closed enum. Adding a purpose means adding a variant, and
//! a test asserts every variant maps to a distinct string. The strings
//! themselves are wire-visible and may never change once events carrying them
//! exist.

use ed25519_dalek::{Signature, SigningKey, VerifyingKey, SIGNATURE_LENGTH};

use crate::poker::state::{Hash, PlayerId};
use crate::protocol::serialization;

/// Every purpose a signature or a hash can be bound to.
///
/// Closed on purpose: see the module note.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Domain {
    /// A signed protocol event in a table's chain.
    Event,
    /// The hash chaining one stage of the chain to the previous one.
    StageHash,
    /// The public state hash exchanged at a checkpoint.
    StateHash,
    /// The table advertisement published in the lobby.
    TableAdvertisement,
    /// The agreed table parameters, which enter the genesis.
    ///
    /// Replaces the advertisement hash, which was per-receiver because each
    /// re-broadcast carries a later timestamp (D-013, blocker J1).
    TableParameters,
    /// A player's long-term application identity.
    PlayerIdentity,
    /// A mental-poker proof, bound to one hand at one table.
    DeckProof,
}

impl Domain {
    /// The wire-visible context string.
    ///
    /// Versioned, so a future protocol revision cannot collide with this one.
    /// These strings are part of the format: changing one invalidates every
    /// event that carries it.
    pub const fn context(self) -> &'static str {
        match self {
            Domain::Event => "p2p-poker v1 event",
            Domain::StageHash => "p2p-poker v1 stage-hash",
            Domain::StateHash => "p2p-poker v1 state-hash",
            Domain::TableAdvertisement => "p2p-poker v1 table-advertisement",
            Domain::TableParameters => "p2p-poker v1 table-parameters",
            Domain::PlayerIdentity => "p2p-poker v1 player-identity",
            Domain::DeckProof => "p2p-poker v1 deck-proof",
        }
    }

    /// Every variant, so tests and audits can enumerate the register.
    pub const ALL: [Domain; 7] = [
        Domain::Event,
        Domain::StageHash,
        Domain::StateHash,
        Domain::TableAdvertisement,
        Domain::TableParameters,
        Domain::PlayerIdentity,
        Domain::DeckProof,
    ];
}

/// A hash bound to a purpose.
pub fn hash(domain: Domain, bytes: &[u8]) -> Hash {
    serialization::hash_domain(domain.context(), bytes)
}

/// A detached Ed25519 signature.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Sig([u8; SIGNATURE_LENGTH]);

impl Sig {
    pub const fn to_bytes(self) -> [u8; SIGNATURE_LENGTH] {
        self.0
    }
    pub const fn from_bytes(bytes: [u8; SIGNATURE_LENGTH]) -> Self {
        Sig(bytes)
    }
}

impl core::fmt::Debug for Sig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Enough to tell two signatures apart in a failing assertion, without
        // pasting 64 bytes into every line of output.
        write!(f, "Sig({:02x}{:02x}..{:02x}{:02x})", self.0[0], self.0[1], self.0[62], self.0[63])
    }
}

/// Why a signature was not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyError {
    /// The 32 bytes offered are not a valid Ed25519 public key.
    BadKey,
    /// The signature does not verify over these bytes under this key and
    /// domain.
    BadSignature,
}

impl core::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VerifyError::BadKey => write!(f, "not a valid Ed25519 public key"),
            VerifyError::BadSignature => write!(f, "signature does not verify"),
        }
    }
}

impl std::error::Error for VerifyError {}

/// Sign canonical bytes under a domain.
///
/// The domain is bound by hashing it in, so the same bytes signed for two
/// purposes produce two unrelated signatures and neither verifies as the other.
pub fn sign(key: &SigningKey, domain: Domain, message: &[u8]) -> Sig {
    use ed25519_dalek::Signer;
    let bound = hash(domain, message);
    Sig(key.sign(&bound).to_bytes())
}

/// Verify a signature over canonical bytes under a domain.
///
/// Uses `verify_strict`, which rejects signatures that verify only because a
/// small-order or non-canonically-encoded public key was used. Plain `verify`
/// admits those, and one of them is a signature that verifies under two
/// different keys — which in this protocol would mean one event attributable to
/// two players.
pub fn verify(player: &PlayerId, domain: Domain, message: &[u8], sig: &Sig) -> Result<(), VerifyError> {
    let key = VerifyingKey::from_bytes(player).map_err(|_| VerifyError::BadKey)?;
    let bound = hash(domain, message);
    let signature = Signature::from_bytes(&sig.0);
    key.verify_strict(&bound, &signature)
        .map_err(|_| VerifyError::BadSignature)
}

/// The public identity a signing key presents.
pub fn player_id(key: &SigningKey) -> PlayerId {
    key.verifying_key().to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::rng;
    use std::collections::BTreeSet;

    fn a_key() -> SigningKey {
        // The OS CSPRNG, per spec section 7, through the one sanctioned source.
        SigningKey::from_bytes(&rng::secret_32().expect("the OS CSPRNG must be available"))
    }

    #[test]
    fn the_domain_register_has_no_duplicates() {
        // Two purposes sharing a context string would let a value signed for
        // one verify as the other, which is a replay across constructions.
        let contexts: BTreeSet<&str> = Domain::ALL.iter().map(|d| d.context()).collect();
        assert_eq!(contexts.len(), Domain::ALL.len(), "a context string is reused");
    }

    #[test]
    fn every_domain_string_is_versioned() {
        for domain in Domain::ALL {
            let context = domain.context();
            assert!(
                context.starts_with("p2p-poker v1 "),
                "{context} is not versioned, so a future revision could collide"
            );
        }
    }

    #[test]
    fn a_signature_verifies_over_the_bytes_it_was_made_for() {
        let key = a_key();
        let id = player_id(&key);
        let message = b"canonical bytes of some event";

        let sig = sign(&key, Domain::Event, message);
        assert_eq!(verify(&id, Domain::Event, message, &sig), Ok(()));
    }

    /// The property the whole register exists for.
    #[test]
    fn a_signature_does_not_verify_under_another_domain() {
        let key = a_key();
        let id = player_id(&key);
        let message = b"the very same bytes";

        let as_event = sign(&key, Domain::Event, message);
        for domain in Domain::ALL {
            let expected = if domain == Domain::Event { Ok(()) } else { Err(VerifyError::BadSignature) };
            assert_eq!(
                verify(&id, domain, message, &as_event),
                expected,
                "an event signature must not verify as {domain:?}"
            );
        }
    }

    #[test]
    fn a_changed_message_does_not_verify() {
        let key = a_key();
        let id = player_id(&key);
        let sig = sign(&key, Domain::Event, b"the original");
        assert_eq!(
            verify(&id, Domain::Event, b"the originaL", &sig),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn another_players_key_does_not_verify() {
        let mine = a_key();
        let theirs = a_key();
        let sig = sign(&mine, Domain::Event, b"mine");
        assert_eq!(
            verify(&player_id(&theirs), Domain::Event, b"mine", &sig),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn a_corrupted_signature_is_rejected_and_does_not_panic() {
        let key = a_key();
        let id = player_id(&key);
        let good = sign(&key, Domain::Event, b"payload");

        for bit in [0usize, 1, 31, 32, 63] {
            let mut bytes = good.to_bytes();
            bytes[bit] ^= 0x01;
            assert_eq!(
                verify(&id, Domain::Event, b"payload", &Sig::from_bytes(bytes)),
                Err(VerifyError::BadSignature),
                "flipping a bit of byte {bit} must not verify"
            );
        }
    }

    /// Section 17 assumes a peer that emits arbitrary bytes, so a malformed key
    /// is a network condition and must be an error rather than a panic.
    #[test]
    fn a_malformed_public_key_is_an_error_not_a_panic() {
        let key = a_key();
        let sig = sign(&key, Domain::Event, b"payload");
        // All-zero is not a valid compressed Edwards point.
        let bad: PlayerId = [0u8; 32];
        assert!(matches!(
            verify(&bad, Domain::Event, b"payload", &sig),
            Err(VerifyError::BadKey) | Err(VerifyError::BadSignature)
        ));
    }

    #[test]
    fn two_keys_have_two_identities() {
        assert_ne!(player_id(&a_key()), player_id(&a_key()));
    }

    #[test]
    fn signing_is_deterministic_for_the_same_key_and_message() {
        // Ed25519 is deterministic by construction, which matters here: a
        // re-transmitted event must be byte-identical, or it becomes a second
        // body in one anti-replay slot (D-009 rule 1).
        let key = a_key();
        let a = sign(&key, Domain::Event, b"payload");
        let b = sign(&key, Domain::Event, b"payload");
        assert_eq!(a, b, "re-signing must reproduce the same bytes");
    }
}
