//! A collective stage: a fixed set of seats, each emitting once.
//!
//! `PROTOCOL.md` §3.2 gives three shapes of stage and this is the one used by
//! nearly every hand event that is not a betting action — `HAND_INIT`,
//! `DECK_INIT`, `DEAL_PRIVATE`, `BOARD_REVEAL`, `STATE_HASH`, `HAND_COMPLETE`.
//! The rule is one sentence:
//!
//! > A fixed set `R` of seats each emit exactly one event, and the stage is
//! > complete only when every seat in `R` has been heard.
//!
//! and the hash that becomes the next stage's parent is taken over `R` in
//! ascending seat order:
//!
//! ```text
//! stage_hash(s) = h("p2p-poker v1 stage",
//!                   [ u64_be(s), u16_be(type), u8(seat), event_hash, … ])
//! ```
//!
//! # Why `R` and the set of seats allowed to speak are two different things
//!
//! `R` is the **liveness gate**: the stage does not advance until every one of
//! those seats is heard, so a member who cannot be heard stops the table. It is
//! derived from demonstrated participation, never from a seat's status
//! (§3.2, the disposition of J2) — for hand 1 it is the signers of
//! `TABLE_READY`.
//!
//! The *accepted* set can be wider. A seat outside `R` that emits a well-formed
//! event has not misbehaved and its event is not a reason to distrust it; it
//! simply does not count towards completion, and — this is the part that must
//! not be got wrong — **it does not enter the hash**. If it did, two peers who
//! happened to hear different bystanders would compute different parents and the
//! chain would fork without anybody lying.
//!
//! # Equivocation is a finding, not an overwrite
//!
//! One seat, two different events, one stage. A map keyed on the seat would
//! quietly keep the last one and the two peers who saw them in different orders
//! would disagree for ever afterwards. So it is reported, and the caller decides
//! (D-014 is where that leads).

use std::collections::{BTreeMap, BTreeSet};

use crate::poker::state::{Hash, SeatIdx};
use crate::protocol::transcript::{stage_hash_collective, StageEmitter};

/// What hearing an event did to the stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    /// The first event from a seat that is in `R`. Counts towards completion.
    Counted,
    /// The first event from a seat that may speak but is not required. Kept out
    /// of the hash, and out of the completion test.
    Bystander,
    /// The same seat, the same event, again. GossipSub delivers a message more
    /// than once as a matter of course and a duplicate is not a fault.
    Again,
    /// The same seat, a **different** event, in the same stage.
    ///
    /// Two peers who saw the two copies in different orders would disagree for
    /// ever if this were an overwrite, so it is a finding. Both hashes are
    /// carried because the evidence is the pair, not the fact.
    Equivocation { first: Hash, second: Hash },
    /// A seat that is not allowed to speak in this stage at all.
    Uninvited,
}

/// One collective stage in progress.
#[derive(Debug, Clone)]
pub struct Collective {
    sequence: u64,
    stage_type: u16,
    /// Ascending and without repeats, which `stage_hash_collective` asserts.
    required: Vec<SeatIdx>,
    accepted: BTreeSet<SeatIdx>,
    heard: BTreeMap<SeatIdx, Hash>,
}

impl Collective {
    /// A stage, or `None` if the sets do not make sense together.
    ///
    /// Refused rather than repaired: a required set that is not a subset of the
    /// accepted one is a caller that has contradicted itself, and quietly
    /// widening `accepted` would hide it until the two peers disagreed.
    pub fn new(
        sequence: u64,
        stage_type: u16,
        required: &[SeatIdx],
        accepted: &[SeatIdx],
    ) -> Option<Self> {
        let mut r: Vec<SeatIdx> = required.to_vec();
        r.sort_unstable();
        r.dedup();
        if r.len() != required.len() {
            return None;
        }
        let a: BTreeSet<SeatIdx> = accepted.iter().copied().collect();
        if !r.iter().all(|s| a.contains(s)) {
            return None;
        }
        Some(Collective {
            sequence,
            stage_type,
            required: r,
            accepted: a,
            heard: BTreeMap::new(),
        })
    }

    /// A stage where the only seats allowed to speak are the required ones.
    pub fn closed(sequence: u64, stage_type: u16, required: &[SeatIdx]) -> Option<Self> {
        Self::new(sequence, stage_type, required, required)
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Record one event.
    pub fn hear(&mut self, seat: SeatIdx, event_hash: Hash) -> Heard {
        if !self.accepted.contains(&seat) {
            return Heard::Uninvited;
        }
        match self.heard.get(&seat) {
            Some(first) if *first == event_hash => Heard::Again,
            Some(first) => Heard::Equivocation {
                first: *first,
                second: event_hash,
            },
            None => {
                self.heard.insert(seat, event_hash);
                if self.required.contains(&seat) {
                    Heard::Counted
                } else {
                    Heard::Bystander
                }
            }
        }
    }

    /// Whether every required seat has been heard.
    /// What this seat has already been heard saying, if anything.
    ///
    /// Non-mutating, and it exists so a caller can tell an **exact repeat**
    /// from a first hearing *before* it spends anything on the message. That
    /// distinction is not cosmetic: a mesh redelivers as a matter of course, and
    /// a handler that verifies first and hears second either
    ///
    /// * charges the duplicate through a check that is not idempotent — a deck
    ///   key already in the set, a token set that takes one share per seat — and
    ///   reports an honest peer as at fault, or
    /// * hears first and then finds the message does not verify, having already
    ///   counted a seat towards a stage it never validly contributed to.
    ///
    /// With this the order is: exact repeat → done; otherwise verify, then
    /// hear, which still catches a **different** body from the same seat as the
    /// equivocation it is.
    pub fn heard(&self, seat: SeatIdx) -> Option<Hash> {
        self.heard.get(&seat).copied()
    }

    pub fn complete(&self) -> bool {
        self.required.iter().all(|s| self.heard.contains_key(s))
    }

    /// Which required seats have not been heard yet.
    ///
    /// For saying *why* a table is not moving. "Waiting for seat 4" is a
    /// sentence a player can act on; a spinner is not.
    pub fn waiting_for(&self) -> Vec<SeatIdx> {
        self.required
            .iter()
            .copied()
            .filter(|s| !self.heard.contains_key(s))
            .collect()
    }

    /// The stage's hash, once it is complete, over `R` and nobody else.
    pub fn hash(&self) -> Option<Hash> {
        if !self.complete() {
            return None;
        }
        let emitters: Vec<StageEmitter> = self
            .required
            .iter()
            .map(|seat| StageEmitter {
                seat: *seat,
                event_hash: self.heard[seat],
            })
            .collect();
        Some(stage_hash_collective(
            self.sequence,
            self.stage_type,
            &emitters,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIND: u16 = 0x0303;

    fn h(n: u8) -> Hash {
        [n; 32]
    }

    #[test]
    fn a_stage_completes_when_every_required_seat_is_heard() {
        let mut s = Collective::closed(0, KIND, &[0, 1, 2]).unwrap();
        assert!(!s.complete());
        assert_eq!(s.hear(0, h(10)), Heard::Counted);
        assert_eq!(s.hear(2, h(12)), Heard::Counted);
        assert_eq!(s.waiting_for(), vec![1]);
        assert!(s.hash().is_none(), "an incomplete stage has no hash");
        assert_eq!(s.hear(1, h(11)), Heard::Counted);
        assert!(s.complete());
        assert!(s.hash().is_some());
    }

    /// The order events arrive in must not change the parent the next stage
    /// hangs off, or two peers on one table build two different chains.
    #[test]
    fn the_hash_does_not_depend_on_the_order_they_arrived() {
        let mut a = Collective::closed(7, KIND, &[0, 1, 2]).unwrap();
        let mut b = Collective::closed(7, KIND, &[0, 1, 2]).unwrap();
        for (seat, hash) in [(0u8, h(10)), (1, h(11)), (2, h(12))] {
            a.hear(seat, hash);
        }
        for (seat, hash) in [(2u8, h(12)), (0, h(10)), (1, h(11))] {
            b.hear(seat, hash);
        }
        assert_eq!(a.hash(), b.hash());
    }

    /// A seat that may speak but is not required is heard, and is kept out of
    /// both the completion test and the hash. If it entered the hash, two peers
    /// who heard different bystanders would fork the chain with nobody lying.
    #[test]
    fn a_bystander_does_not_change_the_stage() {
        let mut with = Collective::new(0, KIND, &[0, 1], &[0, 1, 5]).unwrap();
        let mut without = Collective::new(0, KIND, &[0, 1], &[0, 1, 5]).unwrap();

        assert_eq!(with.hear(5, h(55)), Heard::Bystander);
        assert!(!with.complete(), "a bystander cannot complete a stage");

        for s in [&mut with, &mut without] {
            s.hear(0, h(10));
            s.hear(1, h(11));
        }
        assert!(with.complete() && without.complete());
        assert_eq!(with.hash(), without.hash());
    }

    /// GossipSub delivers the same message more than once as a matter of
    /// course, so a repeat is not a fault and must not read as one.
    #[test]
    fn the_same_event_twice_is_not_a_fault() {
        let mut s = Collective::closed(0, KIND, &[0, 1]).unwrap();
        assert_eq!(s.hear(0, h(10)), Heard::Counted);
        assert_eq!(s.hear(0, h(10)), Heard::Again);
        assert_eq!(s.waiting_for(), vec![1]);
    }

    /// One seat, two different events, one stage. Reported rather than
    /// overwritten: the last-one-wins version leaves two peers who saw them in
    /// different orders disagreeing for ever, and neither of them knowing why.
    #[test]
    fn one_seat_with_two_stories_is_a_finding() {
        let mut s = Collective::closed(0, KIND, &[0, 1]).unwrap();
        s.hear(0, h(10));
        assert_eq!(
            s.hear(0, h(99)),
            Heard::Equivocation {
                first: h(10),
                second: h(99)
            }
        );
        // And the first one still stands, so the honest peers agree.
        s.hear(1, h(11));
        let mut clean = Collective::closed(0, KIND, &[0, 1]).unwrap();
        clean.hear(0, h(10));
        clean.hear(1, h(11));
        assert_eq!(s.hash(), clean.hash());
    }

    #[test]
    fn a_seat_that_may_not_speak_is_refused() {
        let mut s = Collective::closed(0, KIND, &[0, 1]).unwrap();
        assert_eq!(s.hear(4, h(44)), Heard::Uninvited);
        assert!(!s.complete());
    }

    /// A caller that contradicts itself is refused rather than repaired.
    #[test]
    fn the_sets_have_to_make_sense_together() {
        assert!(
            Collective::new(0, KIND, &[0, 1], &[0]).is_none(),
            "required must be a subset of accepted"
        );
        assert!(
            Collective::closed(0, KIND, &[1, 1]).is_none(),
            "a repeated seat is a caller mistake, not a set"
        );
        assert!(Collective::closed(0, KIND, &[2, 0, 1]).is_some());
    }

    /// The sequence and the type are in the hash, so the same emitters at a
    /// different stage produce a different parent.
    #[test]
    fn the_same_events_at_a_different_stage_hash_differently() {
        let mut a = Collective::closed(0, KIND, &[0]).unwrap();
        let mut b = Collective::closed(1, KIND, &[0]).unwrap();
        let mut c = Collective::closed(0, 0x0304, &[0]).unwrap();
        a.hear(0, h(10));
        b.hear(0, h(10));
        c.hear(0, h(10));
        assert_ne!(a.hash(), b.hash());
        assert_ne!(a.hash(), c.hash());
    }
}
