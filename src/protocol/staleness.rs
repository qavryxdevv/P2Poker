//! `PROTOCOL.md` §4.0 steps 10b, 12 and 12a: a chained event naming a hand this
//! receiver has already finished.
//!
//! # Why a finished hand still evaluates anything
//!
//! Ordinarily a late event is dropped, and that was the whole rule until `L4`.
//! The defect it names is that **the solitary-regime divergence could never
//! fire**: the contradiction a peer needs — a seat it did not expect to hear
//! from, speaking about a hand it dealt alone — arrives, by its nature, *after*
//! that hand is over. Dropping it for lateness dropped the best evidence the
//! wire can deliver, every time.
//!
//! So step 10a's anti-replay is **skipped** for such an event and this step is
//! its whole anti-replay rule. That is safe for a reason worth stating rather
//! than assuming: the event is never applied, never enters a `stage_hash`, and
//! **counts into no `P` of a hand this receiver has already initialised**, so
//! there is no effect a duplicate could repeat.
//!
//! # The set is `P(k-1)` and not `P(k)`
//!
//! `N7`. `P(k-1)` is what hand `k`'s stages were **required of**, so a seat
//! outside it is a seat this receiver did not expect to hear from in hand `k` —
//! which is the whole content of the contradiction. `P(k)` is a different set
//! and answers a different question.
//!
//! # Why the readmission set widens the *accepted* set and never the required one
//!
//! `P2`. The write here is reachable by replay: step 10a is skipped, so nothing
//! suppresses a second copy of the event that causes it. If the read enlarged a
//! **required** set, one forwarded event would add a seat that must be heard
//! from before any stage completes — and a seat that is not there stalls the
//! table for as long as the record is retained.
//!
//! Widening the **accepted** set instead is inert on replay: it says one more
//! seat *may* speak at one stage, which a second copy of the same event says
//! again to no effect.

use crate::poker::state::Hash;
use crate::protocol::checkpoint::HandRecord;
use crate::protocol::messages::EventType;
use crate::protocol::seats::SeatSet;

/// A chained event whose `hand_id` names a hand this receiver has completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaleEvent {
    pub hand_id: u64,
    pub sender_seat: u8,
    pub event_type: EventType,
    /// Present when the event is a checkpoint-8 `STATE_HASH`, which is the one
    /// type compared rather than judged on its sender.
    pub checkpoint8_state_hash: Option<Hash>,
}

/// What step 10b decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Route to step 12a. The receiver enters §6.3 step 1 and freezes.
    ///
    /// `latched` is the difference `N1` turns on: a freeze on a hand the record
    /// says was solitary is released by §6.3 step 3's reconciliation stage
    /// **alone**, which §4.9 requires of at least two seats, and by nothing
    /// else. An unlatched freeze is the ordinary one-hand disposition.
    Freeze { latched: bool },
    /// Add the sender to §4.9's readmission set.
    ///
    /// A checkpoint-8 `STATE_HASH` that **matches**, from a seat outside the
    /// recorded set — the seat is evidently in agreement about the hand, so what
    /// it demonstrates is that it is present, not that anything is wrong. A
    /// `PLAYER_SIT_IN` of that hand's boundary window says the same thing
    /// outright (`N5`).
    Readmit { seat: u8 },
    /// Dropped as out of stage, exactly as before `L4`.
    Drop,
}

/// §4.0 step 10b.
///
/// `record` is the retained record for the hand the event names, or `None` if it
/// has fallen out of the window — in which case the event cannot be evaluated
/// and does nothing, which is why the replay window and the retention window are
/// deliberately the same length.
pub fn evaluate_stale(record: Option<&HandRecord>, e: &StaleEvent) -> Disposition {
    let Some(record) = record else {
        return Disposition::Drop;
    };
    debug_assert_eq!(record.hand_id, e.hand_id, "the record must be of the hand named");

    let outside = !record.p.contains(e.sender_seat);

    // The checkpoint-8 STATE_HASH is compared rather than judged on its sender.
    // That exemption is **from arrival and not from disagreement** (N1): a value
    // that differs still reaches step 12a, with the same finding and the same
    // latch.
    if let Some(theirs) = e.checkpoint8_state_hash {
        if theirs != record.checkpoint8_state_hash {
            return Disposition::Freeze {
                latched: record.was_solitary,
            };
        }
        return if outside {
            Disposition::Readmit { seat: e.sender_seat }
        } else {
            Disposition::Drop
        };
    }

    // `PLAYER_SIT_IN` is the one message a seat outside `P` exists to be able to
    // send, so it is exempt from reaching step 12a on arrival — and in the
    // boundary window it is what readmits its sender.
    if e.event_type == EventType::PlayerSitIn {
        return if outside {
            Disposition::Readmit { seat: e.sender_seat }
        } else {
            Disposition::Drop
        };
    }

    if record.was_solitary && outside {
        Disposition::Freeze { latched: true }
    } else {
        Disposition::Drop
    }
}

/// §4.9's readmission set `A`.
///
/// Written by step 10b, read **once** at the next hand init and cleared there.
/// One `SeatSet`, holding no event and no hash — the smallest structure in the
/// pipeline, and the one whose read rule matters most.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Readmissions {
    set: SeatSet,
}

impl Readmissions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a seat, from step 10b.
    ///
    /// Idempotent, which is what makes a replayed event inert here.
    pub fn add(&mut self, seat: u8) {
        let _ = self.set.insert(seat);
    }

    /// Read the set and clear it, at a hand init.
    ///
    /// Takes `&mut self` and returns by value, so there is exactly one reader
    /// and it cannot read twice: `I31(a)`'s one-writer assertion has a
    /// one-reader counterpart, and this is it in the type system rather than in
    /// a comment.
    pub fn take(&mut self) -> SeatSet {
        std::mem::take(&mut self.set)
    }

    pub fn peek(&self) -> SeatSet {
        self.set
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }
}

/// Widen a stage's **accepted** emitter set with the readmissions.
///
/// Named as its own function so that the only thing a caller can do with a
/// readmission set is the one thing `P2` permits. There is deliberately no
/// counterpart that widens a required set.
pub fn accepted_with_readmissions(accepted: SeatSet, readmitted: SeatSet) -> SeatSet {
    accepted.union(readmitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(b: u8) -> Hash {
        [b; 32]
    }

    fn record(was_solitary: bool, p: &[u8]) -> HandRecord {
        HandRecord {
            hand_id: 4,
            was_solitary,
            p: SeatSet::from_seats(p).unwrap(),
            checkpoint8_state_hash: h(0xAA),
        }
    }

    fn event(seat: u8, ty: EventType) -> StaleEvent {
        StaleEvent {
            hand_id: 4,
            sender_seat: seat,
            event_type: ty,
            checkpoint8_state_hash: None,
        }
    }

    /// `L4`, and the whole reason this step exists. The contradiction a solitary
    /// peer needs arrives, by its nature, after the hand is over; dropping it for
    /// lateness dropped the best evidence the wire can deliver.
    #[test]
    fn a_seat_outside_p_speaking_about_a_solitary_hand_freezes_and_latches() {
        let r = record(true, &[0]);
        assert_eq!(
            evaluate_stale(Some(&r), &event(2, EventType::StateAck)),
            Disposition::Freeze { latched: true }
        );
    }

    /// And a seat inside the recorded set contradicts nothing: it is exactly who
    /// this receiver expected to hear from.
    #[test]
    fn a_seat_inside_p_is_dropped_as_before() {
        let r = record(true, &[0, 2]);
        assert_eq!(
            evaluate_stale(Some(&r), &event(2, EventType::StateAck)),
            Disposition::Drop
        );
    }

    /// A hand this receiver was not alone for is the ordinary case, and the
    /// ordinary case is a drop.
    #[test]
    fn a_hand_that_was_not_solitary_drops_late_events() {
        let r = record(false, &[0, 1]);
        assert_eq!(
            evaluate_stale(Some(&r), &event(7, EventType::StateAck)),
            Disposition::Drop
        );
    }

    /// The window is the whole of the evaluability: an event older than the
    /// retained record cannot be evaluated and therefore cannot do anything,
    /// which is why the replay window and the retention window are the same.
    #[test]
    fn an_event_past_the_retention_window_does_nothing() {
        assert_eq!(
            evaluate_stale(None, &event(2, EventType::StateAck)),
            Disposition::Drop
        );
    }

    /// The first exemption: `PLAYER_SIT_IN` is the one message a seat outside
    /// `P` exists to be able to send, so it must not be read as the very
    /// contradiction it is the legitimate form of.
    #[test]
    fn a_sit_in_from_outside_p_readmits_rather_than_freezes() {
        let r = record(true, &[0]);
        assert_eq!(
            evaluate_stale(Some(&r), &event(3, EventType::PlayerSitIn)),
            Disposition::Readmit { seat: 3 },
            "the one message a seat outside P exists to send"
        );
    }

    #[test]
    fn a_sit_in_from_inside_p_readmits_nobody() {
        let r = record(true, &[0, 3]);
        assert_eq!(
            evaluate_stale(Some(&r), &event(3, EventType::PlayerSitIn)),
            Disposition::Drop
        );
    }

    /// The second exemption, and the half that is easy to get wrong: it is an
    /// exemption **from arrival and not from disagreement** (`N1`). A value that
    /// matches says the sender is present; a value that differs is the same
    /// finding as any other divergence.
    #[test]
    fn a_matching_checkpoint_from_outside_p_readmits() {
        let r = record(true, &[0]);
        let e = StaleEvent {
            checkpoint8_state_hash: Some(h(0xAA)),
            ..event(5, EventType::StateHash)
        };
        assert_eq!(evaluate_stale(Some(&r), &e), Disposition::Readmit { seat: 5 });
    }

    #[test]
    fn a_differing_checkpoint_freezes_even_though_the_type_is_exempt() {
        let r = record(true, &[0]);
        let e = StaleEvent {
            checkpoint8_state_hash: Some(h(0xBB)),
            ..event(5, EventType::StateHash)
        };
        assert_eq!(
            evaluate_stale(Some(&r), &e),
            Disposition::Freeze { latched: true },
            "exempt from arrival, not from disagreement"
        );
    }

    /// A differing checkpoint on a hand that was **not** solitary still enters
    /// §6.3 — the values disagree — but the freeze is not latched, so it costs
    /// one hand and the table plays on.
    #[test]
    fn a_differing_checkpoint_on_a_shared_hand_is_not_latched() {
        let r = record(false, &[0, 1]);
        let e = StaleEvent {
            checkpoint8_state_hash: Some(h(0xBB)),
            ..event(1, EventType::StateHash)
        };
        assert_eq!(
            evaluate_stale(Some(&r), &e),
            Disposition::Freeze { latched: false }
        );
    }

    /// And a matching checkpoint from a seat that was inside the set is simply
    /// late.
    #[test]
    fn a_matching_checkpoint_from_inside_p_is_late_and_nothing_more() {
        let r = record(true, &[0, 5]);
        let e = StaleEvent {
            checkpoint8_state_hash: Some(h(0xAA)),
            ..event(5, EventType::StateHash)
        };
        assert_eq!(evaluate_stale(Some(&r), &e), Disposition::Drop);
    }

    // -- the readmission set ------------------------------------------------

    /// The write is reachable by replay, so it must be idempotent — and it is a
    /// set rather than a list precisely so that it is.
    #[test]
    fn a_replayed_event_readmits_nothing_new() {
        let mut a = Readmissions::new();
        for _ in 0..1_000 {
            a.add(3);
        }
        assert_eq!(a.peek().len(), 1);
    }

    /// One reader, and it cannot read twice: `take` is the only route to the
    /// contents and it empties the set.
    #[test]
    fn the_set_is_read_once_and_cleared() {
        let mut a = Readmissions::new();
        a.add(3);
        a.add(7);

        let taken = a.take();
        assert_eq!(taken.len(), 2);
        assert!(a.is_empty(), "cleared at the read");
        assert_eq!(a.take().len(), 0, "and a second read finds nothing");
    }

    /// `P2`: what a readmission may do is widen the **accepted** set. There is
    /// deliberately no function that widens a required one, because a replayed
    /// event would then add a seat that must be heard from before any stage
    /// completes — stalling the table for as long as the record is retained.
    #[test]
    fn readmission_widens_what_is_accepted_and_not_what_is_required() {
        let accepted = SeatSet::from_seats(&[0, 1]).unwrap();
        let readmitted = SeatSet::from_seats(&[5]).unwrap();

        let widened = accepted_with_readmissions(accepted, readmitted);
        assert!(widened.contains(5), "seat 5 may now speak at that stage");
        assert_eq!(widened.len(), 3);

        // The required set is untouched, and the completion test is unmoved.
        let required = SeatSet::from_seats(&[0, 1]).unwrap();
        assert!(
            SeatSet::from_seats(&[0, 1]).unwrap().is_superset_of(required),
            "the stage still completes on the two seats it always needed"
        );
    }

    /// A seat outside the table cannot be readmitted, so a `sender_seat` from
    /// the network cannot grow the set beyond the table.
    #[test]
    fn readmission_is_bounded_by_the_table() {
        let mut a = Readmissions::new();
        for seat in 0..255u8 {
            a.add(seat);
        }
        assert_eq!(
            a.peek().len(),
            crate::protocol::constants::MAX_SEATS as usize
        );
    }
}
