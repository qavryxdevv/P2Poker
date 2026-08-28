//! The anti-replay slot key (`PROTOCOL.md` §5.2.1).
//!
//! One site, for the whole project. Four documents reference §5.2.1 by number
//! and reproduce none of it, and this module is the code's single
//! corresponding place — §4.0's validation, §5.3's stored state and §4.11's
//! per-type check all read this and nothing else.
//!
//! # Why the absent components matter as much as the present ones
//!
//! The key must have capacity **one**, so it may contain only fields that
//! legitimately vary. Three things are deliberately outside it:
//!
//! - **`previous_event_hash`.** A peer chaining one body to two parents at one
//!   sequence is forking the chain, and that has to stay an equivocation.
//!   Putting the parent in the key would excuse it.
//! - **`payload`, `emitted_at_unix_ms`, `next_deadline_ms`.** A key containing
//!   the body is a key no two events ever share, and a predicate over it is
//!   vacuous — it would prove nothing about anybody.
//! - **`chain_scope`.** Not a component but the precondition: an unchained
//!   event occupies no slot at all, ever, and can never appear in an
//!   equivocation proof.
//!
//! `event_class` is implied by `event_type` and is carried anyway, because a
//! coarser key is the unsafe direction: dropping a component merges slots and
//! manufactures false positives against honest peers — the failure D-009 rule 1
//! exists to prevent, and which this project hit five times before the key was
//! written down once.

use crate::protocol::messages::{EventBody, EventType};

/// The eighth component, which exists only for the two timeout classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Subject {
    /// Class 0: an ordinary chain event has no subject.
    None,
    /// Class 1, `TIMEOUT_VOTE`: the seat being voted against.
    ///
    /// In the **key**, not only in the body. The protocol admits two
    /// simultaneous subjects, so a voter that had to put both in one slot would
    /// manufacture an equivocation proof against itself.
    Seat(u8),
    /// Class 2, `TIMEOUT_CERT`: the certificate's subject digest.
    Digest([u8; 32]),
}

impl Subject {
    /// The event class this subject shape belongs to.
    pub const fn class(self) -> u8 {
        match self {
            Subject::None => 0,
            Subject::Seat(_) => 1,
            Subject::Digest(_) => 2,
        }
    }
}

/// The anti-replay slot: the 8-tuple of `PROTOCOL.md` §5.2.1.
///
/// Two accepted events sharing a slot and differing in any other field are an
/// equivocation by their common signer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Slot {
    pub protocol_version: u16,
    pub table_id: [u8; 32],
    pub hand_id: u64,
    pub sequence: u64,
    pub sender_public_key: [u8; 32],
    pub event_class: u8,
    pub event_type: u16,
    pub subject: Subject,
}

/// Why a slot could not be formed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotError {
    /// The event is unchained, so it occupies no slot. Not an error condition
    /// in itself — the caller must not have asked.
    Unchained,
    /// The subject's shape does not match the event's class.
    SubjectClassMismatch { subject_class: u8, event_class: u8 },
}

impl core::fmt::Display for SlotError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SlotError::Unchained => write!(f, "an unchained event occupies no slot"),
            SlotError::SubjectClassMismatch { subject_class, event_class } => write!(
                f,
                "a class {subject_class} subject was offered for a class {event_class} event"
            ),
        }
    }
}

impl std::error::Error for SlotError {}

/// The slot an event occupies.
///
/// `subject` is supplied by the caller because it comes from the payload — the
/// third nesting level, which this function deliberately does not parse. The
/// class is cross-checked, so a caller cannot hand a seat subject to an
/// ordinary event and widen its slot.
pub fn slot(body: &EventBody, subject: Subject) -> Result<Slot, SlotError> {
    if body.chain_scope != 1 {
        return Err(SlotError::Unchained);
    }
    if subject.class() != body.event_class {
        return Err(SlotError::SubjectClassMismatch {
            subject_class: subject.class(),
            event_class: body.event_class,
        });
    }
    Ok(Slot {
        protocol_version: body.protocol_version,
        table_id: body.table_id,
        hand_id: body.hand_id,
        sequence: body.sequence,
        sender_public_key: body.sender_public_key,
        event_class: body.event_class,
        event_type: body.event_type,
        subject,
    })
}

/// The subject shape an event type requires.
pub const fn subject_shape_for(t: EventType) -> u8 {
    match t {
        EventType::TimeoutVote => 1,
        EventType::TimeoutCert => 2,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::messages::{PROTOCOL_VERSION, UNCHAINED_HAND_ID, ZERO32};
    use std::collections::BTreeSet;

    fn body(t: EventType) -> EventBody {
        let chained = t.chain_scope() == 1;
        EventBody {
            protocol_version: PROTOCOL_VERSION,
            table_id: if chained { [3u8; 32] } else { ZERO32 },
            hand_id: if chained { 7 } else { UNCHAINED_HAND_ID },
            sequence: if chained { 12 } else { 0 },
            sender_public_key: [4u8; 32],
            event_type: t.code(),
            payload: vec![1, 2, 3],
            previous_event_hash: if chained { [5u8; 32] } else { ZERO32 },
            emitted_at_unix_ms: 1_700_000_000_000,
            next_deadline_ms: 20_000,
            chain_scope: t.chain_scope(),
            event_class: EventBody::expected_class(t),
        }
    }

    fn subject_for(t: EventType) -> Subject {
        match subject_shape_for(t) {
            1 => Subject::Seat(3),
            2 => Subject::Digest([9u8; 32]),
            _ => Subject::None,
        }
    }

    #[test]
    fn an_unchained_event_occupies_no_slot() {
        for t in EventType::ALL.into_iter().filter(|t| t.chain_scope() == 0) {
            assert_eq!(
                slot(&body(t), Subject::None),
                Err(SlotError::Unchained),
                "{t} is unchained"
            );
        }
    }

    #[test]
    fn every_chained_event_occupies_one() {
        for t in EventType::ALL.into_iter().filter(|t| t.chain_scope() == 1) {
            assert!(slot(&body(t), subject_for(t)).is_ok(), "{t}");
        }
    }

    /// The property that makes the equivocation predicate mean anything.
    ///
    /// Two events that differ only in their body share a slot — that is what
    /// equivocation *is*. If the payload were in the key, no two events would
    /// ever share one and the predicate would be vacuous.
    #[test]
    fn the_body_is_not_in_the_key() {
        let t = EventType::ActionBet;
        let base = slot(&body(t), Subject::None).unwrap();

        for mutate in [
            (|b: &mut EventBody| b.payload = vec![9, 9, 9, 9]) as fn(&mut EventBody),
            |b: &mut EventBody| b.previous_event_hash = [0xAA; 32],
            |b: &mut EventBody| b.emitted_at_unix_ms = 0,
            |b: &mut EventBody| b.next_deadline_ms = 1,
        ] {
            let mut other = body(t);
            mutate(&mut other);
            assert_eq!(
                slot(&other, Subject::None).unwrap(),
                base,
                "this field must stay outside the key"
            );
        }
    }

    /// A peer chaining one body to two parents at one sequence is forking, and
    /// that has to remain an equivocation - so the parent is outside the key
    /// and the two events collide. This is the same fact as above, stated the
    /// way the specification argues it.
    #[test]
    fn two_parents_at_one_sequence_collide() {
        let t = EventType::HandInit;
        let mut a = body(t);
        let mut b = body(t);
        a.previous_event_hash = [1u8; 32];
        b.previous_event_hash = [2u8; 32];
        assert_eq!(slot(&a, Subject::None).unwrap(), slot(&b, Subject::None).unwrap());
    }

    /// Each of the eight components must separate slots, or a coarser key
    /// merges them and manufactures a false positive against an honest peer.
    #[test]
    fn every_component_of_the_key_separates_slots() {
        let t = EventType::ActionCall;
        let base = slot(&body(t), Subject::None).unwrap();

        for mutate in [
            (|b: &mut EventBody| b.protocol_version = 9) as fn(&mut EventBody),
            |b: &mut EventBody| b.table_id = [0xEE; 32],
            |b: &mut EventBody| b.hand_id = 999,
            |b: &mut EventBody| b.sequence = 999,
            |b: &mut EventBody| b.sender_public_key = [0xEE; 32],
        ] {
            let mut other = body(t);
            mutate(&mut other);
            assert_ne!(slot(&other, Subject::None).unwrap(), base);
        }

        // event_type, with class held constant.
        let other = body(EventType::ActionFold);
        assert_ne!(slot(&other, Subject::None).unwrap(), base);
    }

    /// Two actions at one turn are different slots because `event_type` is in
    /// the key. Without it a player who checked and then folded at the same
    /// sequence would look like one who signed two bodies into one slot.
    #[test]
    fn two_action_types_at_one_sequence_are_two_slots() {
        let check = slot(&body(EventType::ActionCheck), Subject::None).unwrap();
        let fold = slot(&body(EventType::ActionFold), Subject::None).unwrap();
        assert_ne!(check, fold);
    }

    /// The defect that cost five review passes: the protocol admits two
    /// simultaneous timeout subjects, so a voter voting against both must land
    /// in two slots. With the subject only in the body, it landed in one and
    /// proved equivocation against itself.
    #[test]
    fn two_timeout_subjects_at_one_sequence_are_two_slots() {
        let b = body(EventType::TimeoutVote);
        let against_two = slot(&b, Subject::Seat(2)).unwrap();
        let against_five = slot(&b, Subject::Seat(5)).unwrap();
        assert_ne!(
            against_two, against_five,
            "an honest voter must not incriminate itself by voting twice"
        );
    }

    #[test]
    fn two_certificate_digests_at_one_sequence_are_two_slots() {
        let b = body(EventType::TimeoutCert);
        let a = slot(&b, Subject::Digest([1u8; 32])).unwrap();
        let c = slot(&b, Subject::Digest([2u8; 32])).unwrap();
        assert_ne!(a, c);
    }

    #[test]
    fn a_subject_of_the_wrong_class_is_refused() {
        // An ordinary event may not be handed a seat subject, which would widen
        // its slot and let one signer occupy several.
        assert_eq!(
            slot(&body(EventType::ActionBet), Subject::Seat(1)),
            Err(SlotError::SubjectClassMismatch { subject_class: 1, event_class: 0 })
        );
        // And a vote may not be handed none, which would narrow it.
        assert_eq!(
            slot(&body(EventType::TimeoutVote), Subject::None),
            Err(SlotError::SubjectClassMismatch { subject_class: 0, event_class: 1 })
        );
    }

    /// Sweeping all 39 types at one sequence: every chained type lands in a
    /// distinct slot, which is what lets a receiver index them together.
    #[test]
    fn all_thirty_nine_types_are_distinguishable_at_one_sequence() {
        let mut slots = BTreeSet::new();
        let mut chained = 0;
        for t in EventType::ALL {
            if let Ok(s) = slot(&body(t), subject_for(t)) {
                chained += 1;
                assert!(slots.insert(s), "{t} collided with another type");
            }
        }
        assert_eq!(chained, 26, "39 types less the 13 with no chain scope");
        assert_eq!(slots.len(), chained);
    }
}
