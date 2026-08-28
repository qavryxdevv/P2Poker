//! Application signatures, and the closed register of domain strings.
//!
//! `SPEC_CS.md` section 12 requires every critical event to be signed over
//! precisely defined bytes. Section 20 requires the poker application identity
//! to be a **different** key from the libp2p `PeerId`, because a `PeerId` says
//! which socket you are talking to, not who is playing.
//!
//! # The register is closed, and it is `PROTOCOL.md` §2.8's
//!
//! Every hash in this protocol is bound to a purpose. If that purpose were a
//! `&str` chosen at the call site, two call sites could disagree by a character
//! and a value hashed for one purpose could be replayed as another — a replay
//! across constructions, not a typo.
//!
//! [`Domain`] is therefore a closed enum whose strings are copied from
//! `PROTOCOL.md` §2.8, which owns the register for the whole corpus (D-011). A
//! test asserts the strings are distinct; another walks this crate's own sources
//! for the three strings §2.8 marks **retired, never valid**, in the shape
//! [`crate::security::rng`] already uses for the generator rule.
//!
//! # The event signature prefix is not one of them
//!
//! `DOMAIN_EVENT` is a **literal 24-byte prefix**, not a `derive_key` domain,
//! and its separators are slashes rather than spaces for exactly that reason
//! (§2.4). It never goes through [`Domain`], and the string with spaces is one
//! of the retired three.

use ed25519_dalek::{Signature, SigningKey, VerifyingKey, SIGNATURE_LENGTH};

use crate::poker::state::{Hash, PlayerId};
use crate::protocol::serialization;

/// The 24-byte literal prefix of every signed event (`PROTOCOL.md` §2.4, §13).
///
/// `"p2p-poker/v1/event"` — eighteen ASCII bytes — NUL-padded to twenty-four.
/// The hex is normative and is pinned by a test below.
pub const DOMAIN_EVENT: [u8; 24] = [
    0x70, 0x32, 0x70, 0x2d, 0x70, 0x6f, 0x6b, 0x65, 0x72, 0x2f, 0x76, 0x31, 0x2f, 0x65, 0x76, 0x65,
    0x6e, 0x74, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// The domain strings of `protocol_version = 1`, from `PROTOCOL.md` §2.8.
///
/// Adding a variant is a minor protocol change; changing or removing one is a
/// major change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Domain {
    /// `event_hash` (§3.2).
    Transcript,
    /// `stage_hash` (§3.2).
    Stage,
    /// The genesis hash of each chain (§3.1).
    Genesis,
    /// `ABORT_TERMINAL(k)`, the terminal value of an aborted chain (§3.1).
    AbortTerminal,
    /// `STATE_HASH` (§6).
    State,
    /// `roster_hash` (§3.1).
    Roster,
    /// The `RNG_COMMIT` commitment (§4.4).
    RngCommit,
    /// The combined seed from `RNG_REVEAL` (§4.4).
    RngBeacon,
    /// The `DECK_COMMIT` digest (§4.5).
    DeckCommit,
    /// The `ctx` byte string handed to the deck library (§4.5).
    DeckCtx,
    /// `session_id` (§4.3).
    Session,
    /// `table_params_hash` (§3.1) — what D-013 put in the genesis in place of
    /// the per-receiver advertisement hash.
    TableParams,
    /// `connection_nonce` (§1.2).
    Connection,
    /// Reserved; `table_id` is currently the table public key itself (§4.1).
    TableId,
    /// Reserved; unused in version 1. Since D-013 the advertisement hash is a
    /// lobby-layer pointer only and enters no chained hash.
    Advert,
    /// The certificate subject digest (§8.3).
    TimeoutCert,
}

impl Domain {
    /// The wire-visible context string, verbatim from `PROTOCOL.md` §2.8.
    pub const fn context(self) -> &'static str {
        match self {
            Domain::Transcript => "p2p-poker v1 transcript",
            Domain::Stage => "p2p-poker v1 stage",
            Domain::Genesis => "p2p-poker v1 genesis",
            Domain::AbortTerminal => "p2p-poker v1 abort-terminal",
            Domain::State => "p2p-poker v1 state",
            Domain::Roster => "p2p-poker v1 roster",
            Domain::RngCommit => "p2p-poker v1 rng-commit",
            Domain::RngBeacon => "p2p-poker v1 rng-beacon",
            Domain::DeckCommit => "p2p-poker v1 deck-commit",
            Domain::DeckCtx => "p2p-poker v1 deck-ctx",
            Domain::Session => "p2p-poker v1 session",
            Domain::TableParams => "p2p-poker v1 table-params",
            Domain::Connection => "p2p-poker v1 connection",
            Domain::TableId => "p2p-poker v1 table-id",
            Domain::Advert => "p2p-poker v1 advert",
            Domain::TimeoutCert => "p2p-poker v1 timeout-cert",
        }
    }

    /// Every variant, so tests and audits can enumerate the register.
    pub const ALL: [Domain; 16] = [
        Domain::Transcript,
        Domain::Stage,
        Domain::Genesis,
        Domain::AbortTerminal,
        Domain::State,
        Domain::Roster,
        Domain::RngCommit,
        Domain::RngBeacon,
        Domain::DeckCommit,
        Domain::DeckCtx,
        Domain::Session,
        Domain::TableParams,
        Domain::Connection,
        Domain::TableId,
        Domain::Advert,
        Domain::TimeoutCert,
    ];
}

/// Hash length-prefixed parts under a registered domain.
///
/// A thin, typed front door onto [`serialization::h`], so no caller can pass a
/// string the register does not contain.
pub fn hash(domain: Domain, parts: &[&[u8]]) -> Hash {
    serialization::h(domain.context(), parts)
}

/// The exact byte string that is signed (`PROTOCOL.md` §2.4).
///
/// `DOMAIN_EVENT ‖ u32_be(len(body_bytes)) ‖ body_bytes`. The length prefix is
/// redundant given a fixed-length tag, but four bytes removes any argument
/// about concatenation ambiguity.
pub fn to_be_signed(body_bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(DOMAIN_EVENT.len() + 4 + body_bytes.len());
    out.extend_from_slice(&DOMAIN_EVENT);
    out.extend_from_slice(&(body_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(body_bytes);
    out
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
    /// The signature does not verify over these bytes under this key.
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

/// Sign an event body.
///
/// `body_bytes` must already be canonical CBOR
/// ([`serialization::to_canonical`]); this function does not encode.
pub fn sign_event(key: &SigningKey, body_bytes: &[u8]) -> Sig {
    use ed25519_dalek::Signer;
    Sig(key.sign(&to_be_signed(body_bytes)).to_bytes())
}

/// Verify an event signature.
///
/// Uses `verify_strict`, never `verify`. Plain `verify` accepts signatures
/// under small-order and non-canonical public keys, which permits signature
/// malleability — and a malleable signature is an equivocation hole: two
/// distinct byte strings validating for one logical event.
pub fn verify_event(player: &PlayerId, body_bytes: &[u8], sig: &Sig) -> Result<(), VerifyError> {
    let key = VerifyingKey::from_bytes(player).map_err(|_| VerifyError::BadKey)?;
    let signature = Signature::from_bytes(&sig.0);
    key.verify_strict(&to_be_signed(body_bytes), &signature)
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

    /// The normative hex of `PROTOCOL.md` §2.4 and §13, pinned. These bytes are
    /// the format; a change here invalidates every signature ever made.
    #[test]
    fn the_event_prefix_is_the_normative_twenty_four_bytes() {
        assert_eq!(DOMAIN_EVENT.len(), 24);
        assert_eq!(&DOMAIN_EVENT[..18], b"p2p-poker/v1/event");
        assert_eq!(&DOMAIN_EVENT[18..], &[0u8; 6], "NUL-padded, not space-padded");
        assert_eq!(
            DOMAIN_EVENT,
            [
                0x70, 0x32, 0x70, 0x2d, 0x70, 0x6f, 0x6b, 0x65, 0x72, 0x2f, 0x76, 0x31, 0x2f, 0x65,
                0x76, 0x65, 0x6e, 0x74, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
            ]
        );
    }

    #[test]
    fn to_be_signed_is_prefix_then_length_then_body() {
        let body = b"body";
        let signed = to_be_signed(body);
        assert_eq!(&signed[..24], &DOMAIN_EVENT);
        assert_eq!(&signed[24..28], &4u32.to_be_bytes());
        assert_eq!(&signed[28..], body);
        assert_eq!(signed.len(), 24 + 4 + body.len());
    }

    /// Without the length prefix, a shorter body followed by attacker-chosen
    /// bytes would sign the same string as a longer one.
    #[test]
    fn the_length_prefix_separates_bodies_that_would_otherwise_concatenate() {
        assert_ne!(to_be_signed(b"AB"), to_be_signed(b"A"));
        assert_ne!(to_be_signed(b"ABC"), to_be_signed(b"AB"));
    }

    #[test]
    fn the_domain_register_has_no_duplicates() {
        // Two purposes sharing a context string would let a value hashed for
        // one be replayed as the other.
        let contexts: BTreeSet<&str> = Domain::ALL.iter().map(|d| d.context()).collect();
        assert_eq!(contexts.len(), Domain::ALL.len(), "a context string is reused");
    }

    #[test]
    fn every_domain_string_is_versioned_and_space_separated() {
        for domain in Domain::ALL {
            let context = domain.context();
            assert!(
                context.starts_with("p2p-poker v1 "),
                "{context} does not match the register's form"
            );
            assert!(
                !context.contains('/'),
                "{context} uses slashes, which belong to the literal event prefix alone"
            );
        }
    }

    #[test]
    fn hashing_under_two_domains_differs() {
        let parts: &[&[u8]] = &[b"identical parts"];
        assert_ne!(hash(Domain::State, parts), hash(Domain::Stage, parts));
        assert_eq!(hash(Domain::State, parts), hash(Domain::State, parts));
    }

    /// `PROTOCOL.md` §2.8 lists three strings as retired and never valid. A
    /// hash built under one of them is a non-conforming implementation, so the
    /// rule is enforced over this crate's own sources rather than written down
    /// and hoped for - the shape `security/rng.rs` uses for the generator rule.
    #[test]
    fn no_retired_domain_string_appears_in_our_sources() {
        use std::fs;
        use std::path::{Path, PathBuf};

        fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
            for entry in fs::read_dir(dir).expect("src/ must be readable") {
                let path = entry.expect("a readable directory entry").path();
                if path.is_dir() {
                    rust_sources(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }

        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_sources(&src, &mut files);
        assert!(files.len() > 10, "the scan found suspiciously few sources");

        // Split so this list does not itself match a naive grep of the tree.
        let retired = [
            concat!("p2p-poker v1 ", "rng-seed"),
            concat!("p2p-poker/", "seat-beacon/v1"),
            concat!("p2p-poker v1 ", "event"),
        ];

        let this_file = src.join("protocol").join("signatures.rs");
        let mut offences = Vec::new();
        for file in files {
            if file == this_file {
                continue;
            }
            let text = fs::read_to_string(&file).expect("a readable source file");
            for (lineno, line) in text.lines().enumerate() {
                for needle in retired {
                    if line.contains(needle) {
                        offences.push(format!(
                            "{}:{}: {needle}",
                            file.strip_prefix(&src).unwrap_or(&file).display(),
                            lineno + 1
                        ));
                    }
                }
            }
        }
        assert!(
            offences.is_empty(),
            "PROTOCOL.md section 2.8 retires these domain strings; a hash under \
             one is a non-conforming implementation:\n  {}",
            offences.join("\n  ")
        );
    }

    #[test]
    fn a_signature_verifies_over_the_body_it_was_made_for() {
        let key = a_key();
        let id = player_id(&key);
        let body = b"canonical CBOR of an event body";
        let sig = sign_event(&key, body);
        assert_eq!(verify_event(&id, body, &sig), Ok(()));
    }

    #[test]
    fn a_changed_body_does_not_verify() {
        let key = a_key();
        let id = player_id(&key);
        let sig = sign_event(&key, b"the original");
        assert_eq!(verify_event(&id, b"the originaL", &sig), Err(VerifyError::BadSignature));
    }

    #[test]
    fn another_players_key_does_not_verify() {
        let mine = a_key();
        let theirs = a_key();
        let sig = sign_event(&mine, b"mine");
        assert_eq!(
            verify_event(&player_id(&theirs), b"mine", &sig),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn a_corrupted_signature_is_rejected_and_does_not_panic() {
        let key = a_key();
        let id = player_id(&key);
        let good = sign_event(&key, b"payload");
        for bit in [0usize, 1, 31, 32, 63] {
            let mut bytes = good.to_bytes();
            bytes[bit] ^= 0x01;
            assert_eq!(
                verify_event(&id, b"payload", &Sig::from_bytes(bytes)),
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
        let sig = sign_event(&key, b"payload");
        let bad: PlayerId = [0u8; 32];
        assert!(matches!(
            verify_event(&bad, b"payload", &sig),
            Err(VerifyError::BadKey) | Err(VerifyError::BadSignature)
        ));
    }

    #[test]
    fn two_keys_have_two_identities() {
        assert_ne!(player_id(&a_key()), player_id(&a_key()));
    }

    #[test]
    fn signing_is_deterministic_for_the_same_key_and_body() {
        // Ed25519 is deterministic by construction, which matters here: a
        // re-transmitted event must be byte-identical, or it becomes a second
        // body in one anti-replay slot (D-009 rule 1).
        let key = a_key();
        assert_eq!(sign_event(&key, b"payload"), sign_event(&key, b"payload"));
    }
}
