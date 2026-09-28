//! `0x0703 DISPUTE` on the wire (`PROTOCOL.md` §9, §6.3 step 2).
//!
//! The message a frozen peer broadcasts to say *what* it observed, carrying the
//! evidence rather than the claim: §6.3's `kind = 1 STATE_DIVERGENCE` embeds the
//! emitter's own `STATE_HASH` event for the disputed checkpoint, complete and
//! signed, so that a receiver verifies it instead of believing anybody.
//!
//! # Why the embedded event matters, and it is load-bearing
//!
//! §6.3: *"a recipient whose own copies all agreed did not observe the
//! divergence and would otherwise take no part in the procedure; the dispute is
//! what puts a second, independently verifying value at that checkpoint in front
//! of it"*. Such a recipient enters the procedure at step 1 and adds the
//! evidence's signer to its contradiction set `W` (§4.9) — which is what lets
//! the reconciliation stage complete at a table where only two seats saw the
//! mismatch directly.
//!
//! **The embedded event is never applied.** It occupies no slot, enters no
//! `stage_hash` and completes no stage: a `DISPUTE` is unchained (§2.3) and
//! *"nothing carried inside one ever becomes a chained event by being carried"*.
//!
//! # Two disputes in one hand are not an equivocation
//!
//! §6.3 step 2 obliges every frozen peer to emit one and step 4(a) obliges the
//! same peer to emit a second. `DISPUTE` carries `chain_scope = 0`, so §5.2's
//! predicate cannot reach it and no number of disputes from one honest key
//! produces an `EquivocationProof` against that key. §9: *"a slot that cannot be
//! occupied cannot be occupied twice"*. Emitting the second is **mandatory, not
//! optional-and-risky**, and the corpus says an implementation that withholds it
//! to stay safe is wrong.
//!
//! # `table_id` and `hand_id` are in the payload
//!
//! `n(5)` and `n(6)`, appended rather than placed first because §2.2 rule 5
//! makes field indices append-only. They are in the body at all because the
//! **envelope** of an unchained event carries §2.3's sentinels rather than an
//! identity, so a dispute that did not say which table and hand it was about
//! would not say it anywhere.

use minicbor::{Decode, Encode};

use crate::poker::state::Hash;
use crate::protocol::constants::MAX_EMBEDDED_EVENT;
use crate::protocol::messages::{EventBody, EventType, SignedEvent};
use crate::protocol::serialization::{from_canonical, to_canonical};
use crate::protocol::signatures::to_be_signed;

/// §6.3 step 2: this peer's own `STATE_HASH` for the disputed checkpoint.
pub const KIND_STATE_DIVERGENCE: u16 = 1;
/// §5.2's proof object. **Not produced in version 1** (D-015), and a `DISPUTE`
/// carrying it is dropped at §4.0 step 11 with its payload never decoded.
pub const KIND_EQUIVOCATION: u16 = 2;
/// D-014: exactly one `SignedEvent`, signed by the seat named in `accused`,
/// which the emitter holds to be provably illegal.
pub const KIND_CHEAT_EVIDENCE: u16 = 3;

/// At most four embedded events (§9).
pub const MAX_EVIDENCE: usize = 4;
/// At most 256 bytes of human text, never parsed (§9).
pub const MAX_NOTE: usize = 256;
/// How much of a dispute body this client will decode: four embedded events at
/// their own cap, plus the small fixed fields and the note.
pub const DISPUTE_CAP: usize = MAX_EVIDENCE * MAX_EMBEDDED_EVENT + 1024;

/// §9's seven fields, in its numbering.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct Dispute {
    #[n(0)]
    pub kind: u16,
    /// App public key, when the kind names one.
    #[cbor(n(1), with = "minicbor::bytes")]
    pub accused: Option<[u8; 32]>,
    /// The stage the dispute is **about**. §9: *"it does not place the dispute
    /// anywhere"* — a `DISPUTE` is out-of-stage and this is a reference, not a
    /// position.
    #[n(2)]
    pub at_sequence: u64,
    /// Each entry a complete `SignedEvent`. Plain, not `minicbor::bytes`: the
    /// helper is for one byte string and this is a list of them, exactly as
    /// `HAND_ABORT`'s own `evidence` is written.
    #[n(3)]
    pub evidence: Vec<Vec<u8>>,
    /// Human text, never parsed.
    #[cbor(n(4), with = "minicbor::bytes")]
    pub note: Vec<u8>,
    #[cbor(n(5), with = "minicbor::bytes")]
    pub table_id: [u8; 32],
    #[n(6)]
    pub hand_id: u64,
}

/// Why a dispute was not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Not a `DISPUTE` at all, or it does not decode.
    Malformed,
    /// The signature does not verify under the key in the envelope.
    BadSignature,
    /// `n(5)` is not the table this receiver is at. §9 requires it to equal the
    /// `table_public_key` of the session it arrives on.
    AnotherTable,
    /// A field is outside its limit: §4.0 step 11.
    OutOfRange,
    /// `kind = 2`. **A drop, never a violation** (D-015): the sender may be a
    /// later version, so no `Fault` is recorded, nothing is attributed, and no
    /// dispute is raised about it.
    NotImplementedHere,
}

/// Broadcast §6.3 step 2's `kind = 1`, carrying this peer's own `STATE_HASH`.
///
/// `state_hash_event` is the complete `SignedEvent` this peer published at the
/// disputed checkpoint — the bytes, not the value. A receiver verifies it
/// itself, which is the whole reason the dispute carries it.
pub fn publish_state_divergence(
    table_id: Hash,
    hand_id: u64,
    at_sequence: u64,
    state_hash_event: Vec<u8>,
    key: &ed25519_dalek::SigningKey,
    now_ms: u64,
) -> Option<Vec<u8>> {
    if state_hash_event.len() > MAX_EMBEDDED_EVENT {
        return None;
    }
    let body = Dispute {
        kind: KIND_STATE_DIVERGENCE,
        accused: None,
        at_sequence,
        evidence: vec![state_hash_event],
        note: Vec::new(),
        table_id,
        hand_id,
    };
    let body_bytes = to_canonical(&body).ok()?;
    if body_bytes.len() > DISPUTE_CAP {
        return None;
    }
    // The one constructor that knows §2.3's sentinels, so an emitter cannot
    // disagree with a checker about them.
    let envelope = EventBody::unchained(
        EventType::Dispute,
        key.verifying_key().to_bytes(),
        body_bytes,
        now_ms,
    )?;
    let envelope_bytes = to_canonical(&envelope).ok()?;
    let signature = {
        use ed25519_dalek::Signer;
        key.sign(&to_be_signed(&envelope_bytes))
    };
    to_canonical(&SignedEvent {
        body: envelope_bytes,
        signature: signature.to_bytes(),
    })
    .ok()
}

/// Open a dispute, checked in full.
///
/// Returns the body and the key that signed it. **The signer is not trusted for
/// anything**: it is returned so a caller can say who spoke, while what the
/// dispute asserts is carried by the evidence and verified separately.
pub fn receive(bytes: &[u8], table_id: &Hash) -> Result<(Dispute, [u8; 32]), Refused> {
    let signed: SignedEvent =
        from_canonical(bytes, DISPUTE_CAP + 1024).map_err(|_| Refused::Malformed)?;
    let envelope: EventBody =
        from_canonical(&signed.body, DISPUTE_CAP + 1024).map_err(|_| Refused::Malformed)?;
    if envelope.check_envelope() != Ok(EventType::Dispute) {
        return Err(Refused::Malformed);
    }
    let key = ed25519_dalek::VerifyingKey::from_bytes(&envelope.sender_public_key)
        .map_err(|_| Refused::BadSignature)?;
    let sig = ed25519_dalek::Signature::from_bytes(&signed.signature);
    key.verify_strict(&to_be_signed(&signed.body), &sig)
        .map_err(|_| Refused::BadSignature)?;

    let body: Dispute =
        from_canonical(&envelope.payload, DISPUTE_CAP).map_err(|_| Refused::Malformed)?;

    // §4.0 step 11 is the first step at which the payload's `kind` exists, and
    // `kind = 2` is dropped there with its payload never decoded as a proof.
    if body.kind == KIND_EQUIVOCATION {
        return Err(Refused::NotImplementedHere);
    }
    if body.kind != KIND_STATE_DIVERGENCE && body.kind != KIND_CHEAT_EVIDENCE {
        // The register is closed: a `kind` outside it is dropped like any other
        // out-of-range field.
        return Err(Refused::OutOfRange);
    }
    if body.evidence.len() > MAX_EVIDENCE
        || body.evidence.iter().any(|e| e.len() > MAX_EMBEDDED_EVENT)
        || body.note.len() > MAX_NOTE
    {
        return Err(Refused::OutOfRange);
    }
    // §9: `kind = 3` carries **exactly one** event, signed by the accused.
    if body.kind == KIND_CHEAT_EVIDENCE && (body.evidence.len() != 1 || body.accused.is_none()) {
        return Err(Refused::OutOfRange);
    }
    if &body.table_id != table_id {
        return Err(Refused::AnotherTable);
    }
    Ok((body, envelope.sender_public_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: Hash = [3u8; 32];

    fn key(seed: u8) -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
    }

    /// A dispute round-trips, and it says which table and hand it is about.
    ///
    /// The envelope of an unchained event carries §2.3's sentinels rather than
    /// an identity, so if `n(5)` and `n(6)` did not carry them nothing would.
    #[test]
    fn a_state_divergence_dispute_says_which_table_and_hand_it_is_about() {
        let k = key(1);
        let bytes = publish_state_divergence(TABLE, 7, 8192, vec![0xAA; 64], &k, 1_000)
            .expect("it seals");
        let (body, sender) = receive(&bytes, &TABLE).expect("and opens");
        assert_eq!(body.kind, KIND_STATE_DIVERGENCE);
        assert_eq!(body.table_id, TABLE);
        assert_eq!(body.hand_id, 7);
        assert_eq!(body.at_sequence, 8192, "the checkpoint stage it is about");
        assert_eq!(body.evidence, vec![vec![0xAA; 64]], "carried verbatim");
        assert_eq!(sender, k.verifying_key().to_bytes());
    }

    /// A dispute about another table is refused, which §9 requires in terms.
    #[test]
    fn a_dispute_about_another_table_is_refused() {
        let bytes = publish_state_divergence(TABLE, 7, 8192, vec![0xAA; 8], &key(1), 1_000)
            .expect("it seals");
        assert_eq!(receive(&bytes, &[9u8; 32]), Err(Refused::AnotherTable));
    }

    /// `kind = 2` is a **drop, never a violation** (D-015).
    ///
    /// The distinction is the whole of the disposition: a receiver cannot tell a
    /// future version from a modified client, so nothing is attributed and no
    /// dispute is raised about it.
    #[test]
    fn an_equivocation_dispute_is_dropped_and_never_faulted() {
        let k = key(1);
        let body = Dispute {
            kind: KIND_EQUIVOCATION,
            accused: Some([4u8; 32]),
            at_sequence: 3,
            evidence: vec![vec![0u8; 8]],
            note: Vec::new(),
            table_id: TABLE,
            hand_id: 1,
        };
        let bytes = seal_for_test(&body, &k);
        assert_eq!(
            receive(&bytes, &TABLE),
            Err(Refused::NotImplementedHere),
            "dropped, and the caller is told which kind of no this is"
        );
    }

    /// Every limit §9 states, refused rather than truncated.
    #[test]
    fn a_dispute_over_any_of_its_limits_is_refused() {
        let k = key(1);
        let base = Dispute {
            kind: KIND_STATE_DIVERGENCE,
            accused: None,
            at_sequence: 8192,
            evidence: Vec::new(),
            note: Vec::new(),
            table_id: TABLE,
            hand_id: 1,
        };

        let mut many = base.clone();
        many.evidence = vec![vec![0u8; 4]; MAX_EVIDENCE + 1];
        assert_eq!(
            receive(&seal_for_test(&many, &k), &TABLE),
            Err(Refused::OutOfRange),
            "five pieces of evidence"
        );

        let mut wordy = base.clone();
        wordy.note = vec![b'x'; MAX_NOTE + 1];
        assert_eq!(
            receive(&seal_for_test(&wordy, &k), &TABLE),
            Err(Refused::OutOfRange),
            "a note over its cap"
        );

        let mut unknown = base.clone();
        unknown.kind = 4;
        assert_eq!(
            receive(&seal_for_test(&unknown, &k), &TABLE),
            Err(Refused::OutOfRange),
            "the kind register is closed"
        );

        // §9: `kind = 3` carries exactly one event, signed by the accused.
        let mut cheat = base;
        cheat.kind = KIND_CHEAT_EVIDENCE;
        assert_eq!(
            receive(&seal_for_test(&cheat, &k), &TABLE),
            Err(Refused::OutOfRange),
            "cheat evidence with nothing in it and nobody named"
        );
    }

    /// A forged dispute does not open. The signature is over the envelope, and
    /// the envelope names the key.
    #[test]
    fn a_dispute_whose_signature_does_not_verify_is_refused() {
        let mut bytes = publish_state_divergence(TABLE, 7, 8192, vec![0xAA; 8], &key(1), 1_000)
            .expect("it seals");
        let n = bytes.len();
        bytes[n - 1] ^= 1;
        assert_eq!(receive(&bytes, &TABLE), Err(Refused::BadSignature));
    }

    /// Seal an arbitrary body, for the refusal tests. The publisher above builds
    /// only well-formed `kind = 1` disputes, which is correct of it and useless
    /// for testing what a receiver must refuse.
    fn seal_for_test(body: &Dispute, k: &ed25519_dalek::SigningKey) -> Vec<u8> {
        let body_bytes = to_canonical(body).expect("encodes");
        let envelope = EventBody::unchained(
            EventType::Dispute,
            k.verifying_key().to_bytes(),
            body_bytes,
            1_000,
        )
        .expect("DISPUTE is unchained");
        let envelope_bytes = to_canonical(&envelope).expect("encodes");
        let signature = {
            use ed25519_dalek::Signer;
            k.sign(&to_be_signed(&envelope_bytes))
        };
        to_canonical(&SignedEvent {
            body: envelope_bytes,
            signature: signature.to_bytes(),
        })
        .expect("encodes")
    }
}
