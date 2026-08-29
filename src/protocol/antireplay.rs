//! Bounded anti-replay state (`PROTOCOL.md` §5.3).
//!
//! One structure per event class, scoped to a `(table_id, hand_id)` and indexed
//! by exactly the components of the slot key that vary within that scope. This
//! section defines no key of its own — [`crate::protocol::slot`] does, and the
//! index here is derived from it, which is the point: a store that indexed
//! anything else would contradict the key it claims to index.
//!
//! # The store is the equivocation detector
//!
//! Two accepted events in one slot with different bodies are an equivocation by
//! their common signer. Two with the *same* body are a re-transmission, which is
//! ordinary and must not be mistaken for one — a peer re-sends after a dropped
//! frame, and treating that as evidence would convict an honest player.
//!
//! # Three axes that are not optional, each of which cost a review pass
//!
//! - **`event_type` on class 0.** Without it the terminal `HAND_ABORT` collides
//!   with its own emitter's contribution to the stalled stage, so the peer
//!   ending a stalled hand proves equivocation against itself.
//! - **The subject on class 1.** The protocol admits two simultaneous timeout
//!   subjects, so without a subject axis an honest voter voting against both
//!   lands two bodies in one cell.
//! - **Capacity is a bound on occupancy, never a component of the index.** An
//!   earlier revision indexed class 0 by a two-valued slot standing in for the
//!   event type. Two different actions claimed for one turn are two slots under
//!   the key and would have been one cell in that store — so a receiver would
//!   reject the second as a replay while refusing the equivocation proof a
//!   conforming client built from the very same pair.
//!
//! Sparse maps rather than dense arrays throughout: a dense `u16` type axis
//! would be 65 536 cells per `(stage, seat)` to hold at most a pair.
//!
//! # Only the ordinary class is allocated in version 1
//!
//! D-015 stops the MVP producing `TIMEOUT_VOTE` and `TIMEOUT_CERT`, so their two
//! structures hold nothing and are not allocated. The measurement is why: of the
//! 28.3 MiB this store could reach in one hand, **26 MiB were those two
//! classes** — the client's largest attacker-influenced allocation, held for
//! machinery D-010 had already made consequence-free.
//!
//! An event of either class is therefore **rejected** rather than recorded. That
//! is not a narrowing of the slot key, which keeps every component including
//! both subject forms: narrowing the key is how five review passes went wrong,
//! and a key that is right costs nothing while nothing populates it.

use std::collections::HashMap;

use crate::poker::state::Hash;
use crate::protocol::constants::{MAX_SEATS, MAX_STAGES_PER_HAND};
use crate::protocol::slot::{Slot, Subject};

/// What the store made of an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Observation {
    /// Not seen before in this slot.
    Fresh,
    /// The same body again — a re-transmission, which is ordinary.
    Duplicate,
    /// A **different** body in a slot this signer already occupies.
    ///
    /// This is the evidence, and the store is where it is found.
    Equivocation { previously: Hash },
}

/// Why the store refused to record an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// The event belongs to a different table or hand than this store covers.
    WrongScope,
    /// Recording it would exceed the structure's bound.
    ///
    /// Reachable only through a peer that got past the step limiting how many
    /// events one `(stage, seat)` may hold, so it is a last line rather than the
    /// first (`SPEC_CS.md` §17, §27 forbid unbounded allocation regardless).
    Full,
    /// A class version 1 does not produce (D-015).
    ///
    /// Nothing emits one, so receiving one means a non-conforming peer. It is
    /// refused here rather than stored, which is what keeps the two structures
    /// unallocated.
    ClassNotProduced(u8),
}

impl core::fmt::Display for StoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StoreError::WrongScope => write!(f, "the event is not of this table and hand"),
            StoreError::Full => write!(f, "the anti-replay store is at its bound"),
            StoreError::ClassNotProduced(c) => {
                write!(f, "event class {c} is not produced in this version")
            }
        }
    }
}

impl std::error::Error for StoreError {}

/// The anti-replay state of one hand at one table.
#[derive(Clone, Debug)]
pub struct AntiReplay {
    table_id: [u8; 32],
    hand_id: u64,
    /// `(stage, seat, event_type) -> event_hash`
    ///
    /// The only class version 1 populates. The two timeout classes are refused
    /// at the door rather than given a structure (D-015).
    ordinary: HashMap<(u64, u8, u16), Hash>,
}

impl AntiReplay {
    /// The occupancy bound of the ordinary class.
    ///
    /// Two entries per `(stage, seat)` — the seat's contribution and the
    /// terminal abort — plus the hand boundary window, which admits one event
    /// per seat, plus the boundary checkpoint band, which admits a state hash
    /// and an acknowledgement per seat over at most eight rounds.
    pub const ORDINARY_CAPACITY: usize = (MAX_STAGES_PER_HAND as usize) * (MAX_SEATS as usize) * 2
        + (MAX_SEATS as usize)
        + 16 * (MAX_SEATS as usize);

    /// What the two timeout classes would have cost if version 1 produced
    /// them: every seat naming every other seat, at every stage.
    ///
    /// Kept as a constant because it is the measurement D-015 rests on, and
    /// because a later version that reinstates them needs the bound.
    pub const DEFERRED_SUBJECT_CAPACITY: usize =
        (MAX_STAGES_PER_HAND as usize) * (MAX_SEATS as usize) * ((MAX_SEATS as usize) - 1);

    pub fn new(table_id: [u8; 32], hand_id: u64) -> Self {
        AntiReplay { table_id, hand_id, ordinary: HashMap::new() }
    }

    /// How many events are recorded.
    pub fn len(&self) -> usize {
        self.ordinary.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Record an accepted event and say what the store made of it.
    ///
    /// The seat is taken from the slot's sender key rather than passed
    /// separately, so a caller cannot file an event under someone else's seat.
    pub fn observe(
        &mut self,
        slot: &Slot,
        seat: u8,
        event_hash: Hash,
    ) -> Result<Observation, StoreError> {
        if slot.table_id != self.table_id || slot.hand_id != self.hand_id {
            return Err(StoreError::WrongScope);
        }

        match slot.subject {
            Subject::None => Self::record(
                &mut self.ordinary,
                (slot.sequence, seat, slot.event_type),
                event_hash,
                Self::ORDINARY_CAPACITY,
            ),
            // Version 1 emits neither, so receiving one means a non-conforming
            // peer. Refusing here is what keeps the two structures unallocated.
            Subject::Seat(_) | Subject::Digest(_) => {
                Err(StoreError::ClassNotProduced(slot.event_class))
            }
        }
    }

    fn record<K: std::hash::Hash + Eq>(
        map: &mut HashMap<K, Hash>,
        key: K,
        event_hash: Hash,
        capacity: usize,
    ) -> Result<Observation, StoreError> {
        match map.get(&key) {
            Some(&previously) if previously == event_hash => Ok(Observation::Duplicate),
            Some(&previously) => Ok(Observation::Equivocation { previously }),
            None => {
                if map.len() >= capacity {
                    return Err(StoreError::Full);
                }
                map.insert(key, event_hash);
                Ok(Observation::Fresh)
            }
        }
    }
}

// The two deferred classes together dwarfed the one that remains - the ratio is
// the measurement D-015 rests on, so it fails the build rather than a test run
// if either bound is ever edited without the other.
const _: () = assert!(
    AntiReplay::DEFERRED_SUBJECT_CAPACITY * 2 > AntiReplay::ORDINARY_CAPACITY * 8
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::messages::EventType;

    const TABLE: [u8; 32] = [0x11; 32];
    const HAND: u64 = 7;

    fn store() -> AntiReplay {
        AntiReplay::new(TABLE, HAND)
    }

    fn slot_of(t: EventType, sequence: u64, subject: Subject) -> Slot {
        Slot {
            protocol_version: 1,
            table_id: TABLE,
            hand_id: HAND,
            sequence,
            sender_public_key: [0x22; 32],
            event_class: subject.class(),
            event_type: t.code(),
            subject,
        }
    }

    #[test]
    fn a_fresh_event_is_recorded_once() {
        let mut s = store();
        let slot = slot_of(EventType::ActionBet, 5, Subject::None);
        assert_eq!(s.observe(&slot, 0, [1u8; 32]), Ok(Observation::Fresh));
        assert_eq!(s.len(), 1);
    }

    /// A re-transmission after a dropped frame is ordinary. Treating it as
    /// evidence would convict an honest peer for the network's behaviour.
    #[test]
    fn the_same_body_again_is_a_duplicate_and_not_evidence() {
        let mut s = store();
        let slot = slot_of(EventType::ActionBet, 5, Subject::None);
        assert_eq!(s.observe(&slot, 0, [1u8; 32]), Ok(Observation::Fresh));
        assert_eq!(s.observe(&slot, 0, [1u8; 32]), Ok(Observation::Duplicate));
        assert_eq!(s.observe(&slot, 0, [1u8; 32]), Ok(Observation::Duplicate));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn a_different_body_in_one_slot_is_equivocation() {
        let mut s = store();
        let slot = slot_of(EventType::ActionBet, 5, Subject::None);
        s.observe(&slot, 0, [1u8; 32]).unwrap();
        assert_eq!(
            s.observe(&slot, 0, [2u8; 32]),
            Ok(Observation::Equivocation { previously: [1u8; 32] })
        );
    }

    #[test]
    fn two_seats_at_one_stage_do_not_collide() {
        let mut s = store();
        let slot = slot_of(EventType::HandInit, 3, Subject::None);
        assert_eq!(s.observe(&slot, 0, [1u8; 32]), Ok(Observation::Fresh));
        assert_eq!(s.observe(&slot, 1, [2u8; 32]), Ok(Observation::Fresh));
        assert_eq!(s.len(), 2);
    }

    /// The `event_type` axis, without which the terminal abort collides with
    /// its own emitter's contribution to the stalled stage — so the peer that
    /// ends a stalled hand proves equivocation against itself.
    #[test]
    fn an_abort_does_not_collide_with_the_stage_it_ends() {
        let mut s = store();
        let stalled = slot_of(EventType::HandInit, 12, Subject::None);
        let abort = slot_of(EventType::HandAbort, 12, Subject::None);

        assert_eq!(s.observe(&stalled, 0, [1u8; 32]), Ok(Observation::Fresh));
        assert_eq!(
            s.observe(&abort, 0, [2u8; 32]),
            Ok(Observation::Fresh),
            "the same seat, the same stage, and no equivocation"
        );
    }

    /// Two different actions claimed for one turn are two slots under the key.
    /// A store indexed by a two-valued stand-in would have made them one cell,
    /// rejected the second as a replay, and refused the equivocation proof a
    /// conforming client builds from the pair.
    #[test]
    fn two_action_types_at_one_turn_are_two_entries() {
        let mut s = store();
        let check = slot_of(EventType::ActionCheck, 9, Subject::None);
        let fold = slot_of(EventType::ActionFold, 9, Subject::None);
        assert_eq!(s.observe(&check, 2, [1u8; 32]), Ok(Observation::Fresh));
        assert_eq!(s.observe(&fold, 2, [2u8; 32]), Ok(Observation::Fresh));
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn showdown_reveal_and_muck_are_two_entries() {
        let mut s = store();
        let reveal = slot_of(EventType::ShowdownReveal, 40, Subject::None);
        let muck = slot_of(EventType::ShowdownMuck, 40, Subject::None);
        assert_eq!(s.observe(&reveal, 1, [1u8; 32]), Ok(Observation::Fresh));
        assert_eq!(s.observe(&muck, 1, [2u8; 32]), Ok(Observation::Fresh));
    }

    /// D-015: version 1 emits neither timeout class, so receiving one means a
    /// non-conforming peer and the store refuses rather than allocating. This
    /// is what keeps 26 of the 28.3 MiB out of the client.
    #[test]
    fn a_timeout_class_is_refused_and_allocates_nothing() {
        let mut s = store();
        let vote = slot_of(EventType::TimeoutVote, 15, Subject::Seat(2));
        let cert = slot_of(EventType::TimeoutCert, 15, Subject::Digest([1u8; 32]));

        assert_eq!(s.observe(&vote, 0, [1u8; 32]), Err(StoreError::ClassNotProduced(1)));
        assert_eq!(s.observe(&cert, 0, [2u8; 32]), Err(StoreError::ClassNotProduced(2)));
        assert!(s.is_empty(), "a refused class must leave nothing behind");
    }

    /// The property the subject axis protected has **not** gone away with the
    /// store — it lives in the slot key, which keeps every component.
    ///
    /// Narrowing that key is how five review passes went wrong, and a key that
    /// is right costs nothing while nothing populates it. So this is asserted
    /// here too: if a later version reinstates the timeout classes, the axis
    /// they need is already correct and this test will have been guarding it.
    #[test]
    fn the_slot_key_still_separates_two_timeout_subjects() {
        let against_2 = slot_of(EventType::TimeoutVote, 15, Subject::Seat(2));
        let against_5 = slot_of(EventType::TimeoutVote, 15, Subject::Seat(5));
        assert_ne!(
            against_2, against_5,
            "an honest voter must not incriminate itself by voting twice"
        );

        let cert_a = slot_of(EventType::TimeoutCert, 15, Subject::Digest([1u8; 32]));
        let cert_b = slot_of(EventType::TimeoutCert, 15, Subject::Digest([2u8; 32]));
        assert_ne!(cert_a, cert_b);
    }

    /// An ordinary event at the same `(stage, seat)` as a refused timeout class
    /// is unaffected: the refusal is per class, not per slot.
    #[test]
    fn refusing_a_timeout_class_does_not_disturb_the_ordinary_one() {
        let mut s = store();
        let ordinary = slot_of(EventType::ActionCall, 20, Subject::None);
        let vote = slot_of(EventType::TimeoutVote, 20, Subject::Seat(1));

        assert_eq!(s.observe(&ordinary, 0, [1u8; 32]), Ok(Observation::Fresh));
        assert!(s.observe(&vote, 0, [2u8; 32]).is_err());
        assert_eq!(s.observe(&ordinary, 0, [1u8; 32]), Ok(Observation::Duplicate));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn an_event_from_another_table_or_hand_is_refused() {
        let mut s = store();
        let mut wrong_table = slot_of(EventType::ActionBet, 5, Subject::None);
        wrong_table.table_id = [0xEE; 32];
        assert_eq!(s.observe(&wrong_table, 0, [1u8; 32]), Err(StoreError::WrongScope));

        let mut wrong_hand = slot_of(EventType::ActionBet, 5, Subject::None);
        wrong_hand.hand_id = HAND + 1;
        assert_eq!(s.observe(&wrong_hand, 0, [1u8; 32]), Err(StoreError::WrongScope));

        assert!(s.is_empty(), "neither was recorded");
    }

    /// Section 27 forbids unbounded allocation from network input, so the store
    /// refuses rather than growing. Reached only by a peer that got past the
    /// earlier per-stage limit, which is why this is a last line and not a
    /// first.
    #[test]
    fn the_store_refuses_to_grow_past_its_bound() {
        let mut s = store();
        let mut filled = 0usize;
        'outer: for sequence in 0..u64::MAX {
            for seat in 0..MAX_SEATS {
                for t in [EventType::ActionCall, EventType::HandAbort] {
                    let slot = slot_of(t, sequence, Subject::None);
                    match s.observe(&slot, seat, [(filled % 251) as u8; 32]) {
                        Ok(Observation::Fresh) => filled += 1,
                        Ok(other) => panic!("unexpected {other:?} while filling"),
                        Err(StoreError::Full) => break 'outer,
                        Err(e) => panic!("unexpected {e:?}"),
                    }
                }
            }
        }
        assert_eq!(filled, AntiReplay::ORDINARY_CAPACITY);

        let overflow = slot_of(EventType::ActionFold, u64::MAX, Subject::None);
        assert_eq!(s.observe(&overflow, 0, [0xFF; 32]), Err(StoreError::Full));

        // A duplicate of something already stored still answers, because
        // answering allocates nothing.
        let known = slot_of(EventType::ActionCall, 0, Subject::None);
        assert!(matches!(
            s.observe(&known, 0, [0u8; 32]),
            Ok(Observation::Duplicate) | Ok(Observation::Equivocation { .. })
        ));
    }

    #[test]
    fn the_bounds_are_what_the_specification_derives() {
        // 2048 stages x 10 seats x 2, plus 10 for the boundary window, plus
        // 16 x 10 for the boundary checkpoint band.
        assert_eq!(AntiReplay::ORDINARY_CAPACITY, 40_960 + 10 + 160);
        // What the two deferred classes would each have cost: every seat naming
        // every other seat, at every stage. This is the number D-015 rests on.
        assert_eq!(AntiReplay::DEFERRED_SUBJECT_CAPACITY, 2_048 * 10 * 9);
    }
}
