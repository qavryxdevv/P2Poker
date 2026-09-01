//! Sealing and opening a **chained** event: one that occupies a stage slot.
//!
//! Every message from `TABLE_READY` onwards is one of these. They differ from
//! the join RPC's four in exactly one way, and it is the way that matters: an
//! unchained event carries §2.3's sentinels and enters nothing, while a chained
//! one names its place in a hash chain and is therefore evidence. Getting a
//! chained envelope wrong does not produce a rejected message — it produces a
//! message that verifies, is accepted, and belongs to a chain nobody else has.
//!
//! # The slot is checked, not trusted
//!
//! A receiver knows where it is in the chain. It knows the table, the hand, the
//! sequence it expects next, and the hash of what came before. So [`open`] takes
//! all four and compares, before the payload is decoded and before a single
//! field is believed.
//!
//! That last one is the whole of D-013's fork check at the transport: two peers
//! who joined under different parameters derive different `GENESIS(0)`, so their
//! first chained events name different parents. Without the comparison the
//! symptom is a collective stage that silently never completes, and the cause is
//! thirty minutes upstream in a table advertisement.
//!
//! # This exists because the first one was written for one message
//!
//! `TABLE_READY`'s sealer hard-coded `hand_id = 0` and `sequence = 0`, and its
//! reader hard-coded the same two. That is correct for `TABLE_READY` and useless
//! for the eight messages after it, every one of which is the same shape with
//! two different numbers in it.

use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use minicbor::{Decode, Encode};

use super::joinwire::WireError;
use crate::poker::state::Hash;
use crate::protocol::messages::{EventBody, EventType, SignedEvent, PROTOCOL_VERSION};
use crate::protocol::serialization::{from_canonical, to_canonical};
use crate::protocol::signatures::to_be_signed;
use crate::protocol::transcript::event_hash;

/// Where a chained event sits: which chain, and which link of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    /// The table. A chained event carries the real identity, not the sentinel.
    pub table_id: Hash,
    /// `0` is the setup chain; `1, 2, …` are the hands.
    pub hand_id: u64,
    /// Which stage of that chain.
    pub sequence: u64,
    /// `GENESIS(k)` for the first stage, the previous stage's hash after that.
    pub previous_event_hash: Hash,
}

impl Slot {
    /// Stage 0 of the setup chain, whose parent is `GENESIS(0)`.
    pub fn setup(table_id: Hash, genesis: Hash) -> Slot {
        Slot {
            table_id,
            hand_id: 0,
            sequence: 0,
            previous_event_hash: genesis,
        }
    }

    /// A **named** stage of the same chain, rather than the next one.
    ///
    /// `then` walks forward by one, which is every ordinary stage. The
    /// boundary checkpoint does not walk: §4.9 puts its `STATE_HASH` at
    /// `sequence = BOUNDARY_CHECKPOINT_BASE` and its `STATE_ACK` at `+ 1`,
    /// eight thousand stages above whatever the hand reached, with
    /// `previous_event_hash = TERMINAL(k)` — *"equivalently, §3.2's rule
    /// extended a second time: `stage_hash(BOUNDARY_CHECKPOINT_BASE - 1) :=
    /// TERMINAL(k)`"*. Reaching it by repeated `then` would be counting to
    /// 8 192.
    pub fn at(self, sequence: u64, parent: Hash) -> Slot {
        Slot {
            sequence,
            previous_event_hash: parent,
            ..self
        }
    }

    /// The next stage of the same chain, after an event whose hash is `parent`.
    pub fn then(self, parent: Hash) -> Slot {
        Slot {
            sequence: self.sequence + 1,
            previous_event_hash: parent,
            ..self
        }
    }
}

/// Sign a chained event into its slot.
pub fn seal<T: Encode<()>>(
    kind: EventType,
    slot: &Slot,
    payload: &T,
    key: &SigningKey,
    now_ms: u64,
    next_deadline_ms: u32,
    cap: usize,
) -> Result<Vec<u8>, WireError> {
    if kind.chain_scope() != 1 {
        return Err(WireError::Unencodable("that type is not chained"));
    }
    let payload_bytes = to_canonical(payload).map_err(|_| WireError::Unencodable("the payload"))?;
    if payload_bytes.len() > cap {
        return Err(WireError::TooLong("the payload is over its cap"));
    }

    let envelope = EventBody {
        protocol_version: PROTOCOL_VERSION,
        table_id: slot.table_id,
        hand_id: slot.hand_id,
        sequence: slot.sequence,
        sender_public_key: key.verifying_key().to_bytes(),
        event_type: kind.code(),
        payload: payload_bytes,
        previous_event_hash: slot.previous_event_hash,
        emitted_at_unix_ms: now_ms,
        next_deadline_ms,
        chain_scope: 1,
        // From the catalogue, not written here. `check_envelope` compares
        // against `expected_class` and a literal would have been a second
        // opinion that can differ — which is exactly what the unchained
        // builder's own doc comment records happening the first time it was
        // written by hand. A `TIMEOUT_VOTE` is class 1 and a `TIMEOUT_CERT`
        // class 2; sealing either as 0 produced an event every receiver
        // refuses, and there was no way to emit one correctly at all.
        event_class: EventBody::expected_class(kind),
    };

    let envelope_bytes =
        to_canonical(&envelope).map_err(|_| WireError::Unencodable("the envelope"))?;
    let signature = {
        use ed25519_dalek::Signer;
        key.sign(&to_be_signed(&envelope_bytes))
    };
    to_canonical(&SignedEvent {
        body: envelope_bytes,
        signature: signature.to_bytes(),
    })
    .map_err(|_| WireError::Unencodable("the signed event"))
}

/// What opening a chained event yields before its payload is read.
pub struct Opened {
    pub sender: [u8; 32],
    pub event_hash: Hash,
    pub envelope: EventBody,
}

/// Decode, check the envelope against the catalogue, check the **slot**, verify
/// the signature — in that order, and all four before any field is believed.
pub fn open(
    bytes: &[u8],
    cap: usize,
    expected: EventType,
    slot: &Slot,
) -> Result<Opened, WireError> {
    open_inner(
        bytes,
        cap,
        expected,
        &slot.table_id,
        slot.hand_id,
        Some(slot),
    )
}

/// Open an event by **which chain** it belongs to, not **where in it**.
///
/// `table_id`, `hand_id`, the catalogue envelope check and `verify_strict` are
/// not relaxed, in that order — only the two positional comparisons are, and
/// only for callers that have a clause saying they must be:
///
/// * `Hand::on_hand_abort`. §3.2's terminal is witness-independent: it has no
///   required emitter set and no chain position, and §4.10 states the exemption
///   in terms — a strict parent check would have two peers each reject the
///   other's artefact, which is the deadlock again by another route.
/// * `Hand::on_timeout_cert`, and the votes it carries. `event_class` 1 and 2
///   **reference** a stage rather than occupying it (§4.8); the position is in
///   the payload, and the receiver re-checks it there against every carried
///   vote's own signed envelope — a binding the voter itself signed, which is
///   stronger than anything the receiver's own cursor could offer.
///
/// * A **stale checkpoint-8 `STATE_HASH`** of a hand this receiver has already
///   completed (§4.9, §4.0 step 10b). It names a chain position this receiver
///   has left — that is the whole of what makes it stale — so the positional
///   check is the one thing it must not face, and it is not applied: step 10b
///   says such an event *"is not dropped for arriving late"*. What it does is
///   add its sender to the readmission set when its value **agrees**, and
///   nothing else: it is not applied, enters no `stage_hash` and completes no
///   stage. The comparison is against this receiver's own retained value, so a
///   forged one is refused by the comparison and a replayed one is idempotent.
///
/// No fourth caller may be added without a clause of its own.
pub fn open_in_hand(
    bytes: &[u8],
    cap: usize,
    expected: EventType,
    table_id: &Hash,
    hand_id: u64,
) -> Result<Opened, WireError> {
    open_inner(bytes, cap, expected, table_id, hand_id, None)
}

/// One decoder and one signature check, so the two entry points cannot drift.
fn open_inner(
    bytes: &[u8],
    cap: usize,
    expected: EventType,
    table_id: &Hash,
    hand_id: u64,
    position: Option<&Slot>,
) -> Result<Opened, WireError> {
    let signed: SignedEvent = from_canonical(bytes, cap)
        .map_err(|_| WireError::Malformed("not a canonical signed event"))?;
    let envelope: EventBody = from_canonical(&signed.body, cap)
        .map_err(|_| WireError::Malformed("not a canonical envelope"))?;

    let kind = envelope
        .check_envelope()
        .map_err(|_| WireError::Envelope("the envelope is not what the catalogue says"))?;
    if kind != expected {
        return Err(WireError::WrongType);
    }

    // The slot, field by field, with a distinct message for each: "a message
    // from another table", "from another hand" and "from another game" are three
    // different situations and only the last of them is a parameter fork.
    if envelope.table_id != *table_id {
        return Err(WireError::Envelope("an event of another table"));
    }
    if envelope.hand_id != hand_id {
        return Err(WireError::Envelope("an event of another hand"));
    }
    if let Some(slot) = position {
        if envelope.sequence != slot.sequence {
            return Err(WireError::Envelope("an event at another stage"));
        }
        if envelope.previous_event_hash != slot.previous_event_hash {
            return Err(WireError::Envelope(
                "a different parent: the sender is on another chain",
            ));
        }
    }

    let key = VerifyingKey::from_bytes(&envelope.sender_public_key)
        .map_err(|_| WireError::BadSignature)?;
    let sig = Signature::from_bytes(&signed.signature);
    key.verify_strict(&to_be_signed(&signed.body), &sig)
        .map_err(|_| WireError::BadSignature)?;

    Ok(Opened {
        sender: envelope.sender_public_key,
        event_hash: event_hash(&signed.body),
        envelope,
    })
}

/// What an event says it is, before anything about it is believed.
///
/// `open` demands the expected type up front, which is right where the caller
/// knows what it is waiting for and wrong where one channel carries several —
/// a table's mesh carries the formation's messages and the hand's, and telling
/// a duplicate of a stage already left from a message of another kind entirely
/// needs the type and the sequence *first*.
///
/// **Nothing here is trusted.** No signature is checked, so the answer is a
/// claim: use it to route, never to decide.
pub fn peek(bytes: &[u8], cap: usize) -> Result<(EventType, u64, u64), WireError> {
    let signed: SignedEvent =
        from_canonical(bytes, cap).map_err(|_| WireError::Malformed("not a signed event"))?;
    let envelope: EventBody =
        from_canonical(&signed.body, cap).map_err(|_| WireError::Malformed("not an envelope"))?;
    let kind = envelope
        .check_envelope()
        .map_err(|_| WireError::Envelope("the envelope is not what the catalogue says"))?;
    Ok((kind, envelope.hand_id, envelope.sequence))
}

/// The payload of an opened event, decoded under its own cap.
pub fn payload<'a, T: Decode<'a, ()> + Encode<()>>(
    o: &'a Opened,
    cap: usize,
) -> Result<T, WireError> {
    from_canonical(&o.envelope.payload, cap)
        .map_err(|_| WireError::Malformed("the payload is not canonical"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::constants::JOIN_RESP_MAX;

    #[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
    #[cbor(array)]
    struct Probe {
        #[cbor(n(0), with = "minicbor::bytes")]
        value: [u8; 32],
    }

    const NOW: u64 = 1_700_000_000_000;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn slot() -> Slot {
        Slot::setup([6u8; 32], [9u8; 32])
    }

    fn probe() -> Probe {
        Probe { value: [3u8; 32] }
    }

    /// The round trip, and the sender it reports is the key that signed.
    #[test]
    fn a_chained_event_survives_its_own_slot() {
        let k = key(3);
        let wire = seal(
            EventType::RngCommit,
            &slot(),
            &probe(),
            &k,
            NOW,
            30_000,
            JOIN_RESP_MAX,
        )
        .unwrap();
        let o = open(&wire, JOIN_RESP_MAX, EventType::RngCommit, &slot()).unwrap();
        assert_eq!(o.sender, k.verifying_key().to_bytes());
        assert_eq!(payload::<Probe>(&o, JOIN_RESP_MAX).unwrap(), probe());
    }

    /// **Every field of the slot is checked**, and each has its own answer.
    ///
    /// A chained event that verified into the wrong slot would not be rejected —
    /// it would be accepted into a chain nobody else has, which is the failure
    /// mode this whole module exists to make impossible.
    #[test]
    fn an_event_of_another_slot_is_refused() {
        let wire = seal(
            EventType::RngCommit,
            &slot(),
            &probe(),
            &key(3),
            NOW,
            0,
            JOIN_RESP_MAX,
        )
        .unwrap();

        let elsewhere: [(&str, Slot); 4] = [
            (
                "another table",
                Slot {
                    table_id: [7u8; 32],
                    ..slot()
                },
            ),
            (
                "another hand",
                Slot {
                    hand_id: 1,
                    ..slot()
                },
            ),
            (
                "another stage",
                Slot {
                    sequence: 1,
                    ..slot()
                },
            ),
            (
                "another chain",
                Slot {
                    previous_event_hash: [10u8; 32],
                    ..slot()
                },
            ),
        ];
        for (what, wrong) in elsewhere {
            assert!(
                open(&wire, JOIN_RESP_MAX, EventType::RngCommit, &wrong).is_err(),
                "an event from {what} was taken"
            );
        }
    }

    /// A message of one type does not open as another, and the type is inside
    /// the signature.
    #[test]
    fn a_type_is_not_interchangeable() {
        let wire = seal(
            EventType::RngCommit,
            &slot(),
            &probe(),
            &key(3),
            NOW,
            0,
            JOIN_RESP_MAX,
        )
        .unwrap();
        assert_eq!(
            open(&wire, JOIN_RESP_MAX, EventType::RngReveal, &slot()).err(),
            Some(WireError::WrongType)
        );
    }

    /// One flipped bit anywhere and nothing is accepted.
    #[test]
    fn a_single_altered_byte_is_refused() {
        let wire = seal(
            EventType::RngCommit,
            &slot(),
            &probe(),
            &key(3),
            NOW,
            0,
            JOIN_RESP_MAX,
        )
        .unwrap();
        let mut refused = 0;
        for i in 0..wire.len() {
            let mut bad = wire.clone();
            bad[i] ^= 0x01;
            if open(&bad, JOIN_RESP_MAX, EventType::RngCommit, &slot()).is_err() {
                refused += 1;
            }
        }
        assert_eq!(refused, wire.len(), "a byte could be changed and accepted");
    }

    /// An unchained type cannot be sealed here. The four join messages are
    /// unchained precisely so they enter nothing, and a sealer that would put
    /// one in a stage slot would undo that.
    #[test]
    fn an_unchained_type_cannot_be_chained() {
        for kind in [
            EventType::JoinRequest,
            EventType::JoinAccept,
            EventType::LobbyTableAd,
        ] {
            assert!(
                seal(kind, &slot(), &probe(), &key(3), NOW, 0, JOIN_RESP_MAX).is_err(),
                "{kind:?} was sealed into a stage slot"
            );
        }
    }

    /// `then` walks the chain: the next stage, naming what came before.
    #[test]
    fn the_next_slot_names_the_previous_event() {
        let first = slot();
        let next = first.then([42u8; 32]);
        assert_eq!(next.table_id, first.table_id);
        assert_eq!(next.hand_id, first.hand_id);
        assert_eq!(next.sequence, first.sequence + 1);
        assert_eq!(next.previous_event_hash, [42u8; 32]);
    }

    /// Garbage is refused rather than panicking. Every one of these arrives from
    /// a peer that chose the bytes.
    #[test]
    fn no_byte_string_can_panic_the_opener() {
        for bytes in [
            vec![],
            vec![0x00],
            vec![0xff; 64],
            vec![0x82, 0x40, 0x40],
            (0..255u8).collect::<Vec<u8>>(),
        ] {
            let _ = open(&bytes, JOIN_RESP_MAX, EventType::RngCommit, &slot());
        }
    }
}
