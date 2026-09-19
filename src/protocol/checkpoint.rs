//! Checkpoints: the three-slot store, T49/T50/T51, and the solitary floor.
//!
//! `STATE_MACHINE.md` §2.6 and `PROTOCOL.md` §6.2 and §4.9.
//!
//! # Why three named slots and not a container
//!
//! Since `P1`, hand `k`'s checkpoint-8 record **outlives hand `k`**: §4.9 accepts
//! a checkpoint-8 `STATE_HASH` of hand `k` until `TERMINAL(k+1)` is fixed here,
//! and by then hand `k+1` is live and has a checkpoint of its own. One
//! `Option<CheckpointState>` could not hold both, so the phrase *"that
//! checkpoint"* in T49, T50 and T51 had no single referent.
//!
//! The answer is **three named slots** — `live`, `agreed`, `boundary` — and not a
//! map. A map would be keyed on something, and the only candidates are quantities
//! a sender chooses; `SPEC_CS.md` §27 forbids a container fed from the network,
//! and this is exactly the shape it forbids. Three slots are three rôles, each
//! written by named transitions, and [`CheckpointStore::named`] matches at most
//! one of them.
//!
//! # Why `own` is written once and never re-derived
//!
//! The guards of T49 and T50 used to read *"the value equals this peer's own
//! derivation at that checkpoint"*. On the live slot an implementation can
//! satisfy that by re-deriving from current state; on the other two it **cannot**,
//! because the state that produced the value has been overwritten by the next
//! hand.
//!
//! `I32(d)` names re-derivation as the implementation that passes every
//! single-hand trace and fails every boundary one — the worst kind, because the
//! tests that would catch it are the ones nobody writes first. So [`own`] is a
//! field, set by [`CheckpointState::open`] and by nothing else: there is no
//! setter, and the struct cannot be built any other way.
//!
//! [`own`]: CheckpointState::own
//!
//! # Why `values: u8` is gone and must not come back
//!
//! T50's guard was *"two distinct `state_hash` values now exist"*, carried by a
//! count. A count cannot be maintained without the values: deciding whether an
//! arriving hash increments it means comparing it against what was already seen,
//! and the field held nothing to compare against. The guard was undefined on
//! every slot, the live one included.
//!
//! `e.state_hash != c.own` is the same predicate. This peer's own value is always
//! one of the values in the stage, because it publishes it, so the number of
//! distinct values is exactly `1 + dissent.is_some()`. A third distinct value
//! adds a signature to a divergence already detected and changes nothing.

use std::collections::VecDeque;

use crate::poker::state::Hash;
use crate::protocol::constants::MAX_RETAINED_HAND_RECORDS;
use crate::protocol::seats::SeatSet;

/// One checkpoint record.
///
/// Canonical state — a pure function of the events this peer accepted, with no
/// clock and no arrival fact in it — and deliberately **not** a field of
/// `PublicTableState`: hashing the record of a comparison into the value being
/// compared is circular.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointState {
    /// The chain this checkpoint belongs to: hand `k` for checkpoint 8 of hand
    /// `k`, and 0 for checkpoint 1.
    hand_id: u64,
    /// §6.2's checkpoint number, 1 to 8.
    number: u16,
    /// The stage index of its `STATE_HASH` stage, which is what `round` is
    /// derived from.
    sequence: u64,
    /// The emitter set §6.2 gives this checkpoint.
    required: SeatSet,
    /// Seats whose `STATE_HASH` for it this peer has accepted.
    heard: SeatSet,
    /// **This** peer's own `state_hash`, fixed when the record was opened.
    own: Hash,
    /// The first accepted `state_hash` differing from [`own`](Self::own).
    ///
    /// `own` and `dissent` are T50's retained evidence, and `dissent.is_none()`
    /// is T51's unanimity test.
    dissent: Option<Hash>,
    /// Seats whose `STATE_ACK` for it this peer has accepted.
    acked: SeatSet,
    /// The `checkpoint_hash`, set by T51 when the **`STATE_ACK` stage**
    /// completes — `acked ⊇ required`, and not on the first ack.
    agreed: Option<Hash>,
}

impl CheckpointState {
    /// Open a checkpoint. The only constructor, and the only write of `own`.
    pub fn open(
        hand_id: u64,
        number: u16,
        sequence: u64,
        required: SeatSet,
        own: Hash,
    ) -> Self {
        CheckpointState {
            hand_id,
            number,
            sequence,
            required,
            heard: SeatSet::EMPTY,
            own,
            dissent: None,
            acked: SeatSet::EMPTY,
            agreed: None,
        }
    }

    pub fn hand_id(&self) -> u64 {
        self.hand_id
    }
    pub fn number(&self) -> u16 {
        self.number
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn required(&self) -> SeatSet {
        self.required
    }
    pub fn heard(&self) -> SeatSet {
        self.heard
    }
    pub fn own(&self) -> Hash {
        self.own
    }
    pub fn dissent(&self) -> Option<Hash> {
        self.dissent
    }
    pub fn acked(&self) -> SeatSet {
        self.acked
    }
    pub fn agreed(&self) -> Option<Hash> {
        self.agreed
    }

    /// Whether the `STATE_HASH` stage of this checkpoint is complete.
    ///
    /// T47's gate on the boundary checkpoint reads exactly this.
    pub fn hash_stage_complete(&self) -> bool {
        self.heard.is_superset_of(self.required)
    }

    /// Whether the `STATE_ACK` stage is complete.
    pub fn ack_stage_complete(&self) -> bool {
        self.acked.is_superset_of(self.required)
    }
}

/// The three-slot store of `STATE_MACHINE.md` §2.6.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckpointStore {
    /// The checkpoint of the current chain that is open.
    live: Option<CheckpointState>,
    /// The most recent checkpoint of the current chain whose `STATE_ACK` stage
    /// completed here. D-014's tier-2 precondition reads it.
    agreed: Option<CheckpointState>,
    /// Hand `k-1`'s checkpoint 8, retained across the boundary because §4.9
    /// accepts events into it until `TERMINAL(k)` is fixed here.
    boundary: Option<CheckpointState>,
}

/// Which slot a record was found in.
///
/// Named because two rows care: T47 gates on the boundary slot, and D-014's
/// tier 2 reads the agreed one. Nothing else may look into the store by any
/// other route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Live,
    Agreed,
    Boundary,
}

impl CheckpointStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// The record with this `hand_id` and `number`, or `None`.
    ///
    /// At most one slot can match, by the lifetimes below. T49, T50 and T51 read
    /// it, and an event naming no record is a rejection — **never** an
    /// insertion, which is what bounds the store at three.
    pub fn named(&self, hand_id: u64, number: u16) -> Option<(Slot, &CheckpointState)> {
        for (slot, held) in [
            (Slot::Live, &self.live),
            (Slot::Agreed, &self.agreed),
            (Slot::Boundary, &self.boundary),
        ] {
            if let Some(c) = held {
                if c.hand_id == hand_id && c.number == number {
                    return Some((slot, c));
                }
            }
        }
        None
    }

    fn named_mut(&mut self, hand_id: u64, number: u16) -> Option<&mut CheckpointState> {
        for held in [&mut self.live, &mut self.agreed, &mut self.boundary] {
            if let Some(c) = held {
                if c.hand_id == hand_id && c.number == number {
                    return held.as_mut();
                }
            }
        }
        None
    }

    /// The record of hand `h` with `agreed` set and the greatest `number`.
    ///
    /// T64 and T65 read it and nothing else does.
    pub fn agreed_checkpoint(&self, hand_id: u64) -> Option<&CheckpointState> {
        [&self.live, &self.agreed, &self.boundary]
            .into_iter()
            .flatten()
            .filter(|c| c.hand_id == hand_id && c.agreed.is_some())
            .max_by_key(|c| c.number)
    }

    pub fn live(&self) -> Option<&CheckpointState> {
        self.live.as_ref()
    }
    pub fn boundary(&self) -> Option<&CheckpointState> {
        self.boundary.as_ref()
    }

    /// Open a checkpoint, at T45 or T46.
    ///
    /// `boundary := None` **first**, which is what releases hand `k-1`'s record
    /// at the exact moment `TERMINAL(k)` is fixed, and then `live := Some(new)`.
    /// The order is the whole of it: doing it the other way round would drop the
    /// record one moment too early or hold two of them at once, and either way
    /// [`named`](Self::named) would stop having one answer.
    pub fn open(&mut self, new: CheckpointState) {
        self.boundary = None;
        self.live = Some(new);
    }

    /// Move the live checkpoint down to the boundary slot, at T47 or T61.
    ///
    /// Hand `k`'s boundary checkpoint is **moved down, not dropped**, and it is
    /// dropped at the next [`open`](Self::open). The clause this replaces was
    /// `checkpoint := None`, and that was `P1`.
    pub fn cross_boundary(&mut self) {
        self.boundary = self.live.take();
        self.agreed = None;
    }
}

/// What arrived, for T49 and T50.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateHashEvent {
    pub hand_id: u64,
    pub checkpoint: u16,
    pub seat: u8,
    pub state_hash: Hash,
    /// The reconciliation round. T49 and T50 are scoped on `round == 0`.
    pub round: u16,
}

/// What arrived, for T51.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateAckEvent {
    pub hand_id: u64,
    pub checkpoint: u16,
    pub seat: u8,
    pub checkpoint_hash: Hash,
}

/// The outcome of a `STATE_HASH` at round 0.
///
/// T49 and T50 are **exhaustive and disjoint**, which is what makes the pair a
/// total function of the arriving value and removes the ordering dependence the
/// deleted counter had.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateHashOutcome {
    /// T49: the value equals this peer's own. The phase is unchanged.
    Agreed,
    /// T50: the value differs. The phase becomes `Diverged` and the table
    /// freezes.
    Diverged {
        /// N1's second half: set when the named checkpoint's own required set
        /// has exactly one member.
        ///
        /// Read off `c.required` and not off a remembered regime, because
        /// `c.required` **is** the regime for the stage being compared, fixed
        /// when the checkpoint opened and therefore not grown by the very copy
        /// that contradicts it.
        ///
        /// Without it, a solitary peer that diverges by hash rather than by
        /// emitter freezes, restores, deals another solitary hand, meets the
        /// same mismatch and freezes again — every `hand_deadline_ms`, forever.
        solitary_contradicted: bool,
    },
    /// The event names no record, or is not round 0. A rejection, and the state
    /// is bit-identical: **no slot is created for it**.
    NoSuchCheckpoint,
}

/// The outcome of a `STATE_ACK`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateAckOutcome {
    /// T51 fired. `agreed` is set only when the **stage** completed.
    Recorded { stage_complete: bool },
    /// The named checkpoint's `STATE_HASH` stage is not complete, or it is not
    /// unanimous, or there is no such record.
    NotApplicable,
}

impl CheckpointStore {
    /// T49 and T50.
    pub fn on_state_hash(&mut self, e: &StateHashEvent) -> StateHashOutcome {
        if e.round != 0 {
            return StateHashOutcome::NoSuchCheckpoint;
        }
        let Some(c) = self.named_mut(e.hand_id, e.checkpoint) else {
            return StateHashOutcome::NoSuchCheckpoint;
        };
        // An out-of-range seat cannot be recorded; the stage then simply does not
        // complete from this event, which is the same as never having heard it.
        let _ = c.heard.insert(e.seat);

        if e.state_hash == c.own {
            StateHashOutcome::Agreed
        } else {
            // Written at most once per record, so the retained evidence is the
            // pair `(own, dissent)` rather than an instruction with no field
            // behind it.
            c.dissent = c.dissent.or(Some(e.state_hash));
            StateHashOutcome::Diverged {
                solitary_contradicted: c.required.len() == 1,
            }
        }
    }

    /// T51.
    pub fn on_state_ack(&mut self, e: &StateAckEvent) -> StateAckOutcome {
        let Some(c) = self.named_mut(e.hand_id, e.checkpoint) else {
            return StateAckOutcome::NotApplicable;
        };
        if !c.hash_stage_complete() || c.dissent.is_some() {
            return StateAckOutcome::NotApplicable;
        }
        let _ = c.acked.insert(e.seat);

        let stage_complete = c.ack_stage_complete();
        if stage_complete {
            c.agreed = Some(e.checkpoint_hash);
        }
        StateAckOutcome::Recorded { stage_complete }
    }
}

// ---------------------------------------------------------------------------
// The solitary floor, and the record it is a floor over
// ---------------------------------------------------------------------------

/// What `PROTOCOL.md` §4.0 step 10b retains about a finished hand.
///
/// Four fields, about 56 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandRecord {
    pub hand_id: u64,
    /// §3.2's regime test **in the past tense**, and it is the **disjunction**
    /// `P(k-1) == {self} ∨ P(k) == {self}`.
    ///
    /// An independent bool, never `p == {self}` and never `|p| == 1` — that is
    /// `N4-a`, and the difference matters because the second disjunct is about a
    /// set this record does not hold.
    pub was_solitary: bool,
    /// `P(hand_id - 1)`: the required emitter set **of** the hand named.
    ///
    /// `P(k-1)` and not `P(k)`, which is `N7`: §3.2's rule and §4.0 step 12 both
    /// test *"a seat outside `P(k-1)`"*, because `P(k-1)` is what hand `k`'s
    /// stages were required of, and a seat outside it is one this receiver did
    /// not expect to hear from in hand `k`.
    pub p: SeatSet,
    /// This peer's own checkpoint-8 value for that hand.
    pub checkpoint8_state_hash: Hash,
}

/// The retained hand records, LRU-capped.
///
/// The replay window and the retention window are the same
/// [`MAX_RETAINED_HAND_RECORDS`] hands, and that is not a coincidence: a
/// retained record is what makes a stale event evaluable, so an event older than
/// the window cannot be evaluated and cannot do anything.
#[derive(Debug, Clone, Default)]
pub struct RetainedHands {
    order: VecDeque<u64>,
    records: Vec<HandRecord>,
}

impl RetainedHands {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a finished hand, evicting the oldest if the cap is reached.
    pub fn insert(&mut self, record: HandRecord) {
        if let Some(existing) = self.records.iter_mut().find(|r| r.hand_id == record.hand_id) {
            *existing = record;
            return;
        }
        if self.records.len() >= MAX_RETAINED_HAND_RECORDS {
            if let Some(oldest) = self.order.pop_front() {
                self.records.retain(|r| r.hand_id != oldest);
            }
        }
        self.order.push_back(record.hand_id);
        self.records.push(record);
    }

    pub fn get(&self, hand_id: u64) -> Option<&HandRecord> {
        self.records.iter().find(|r| r.hand_id == hand_id)
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// The monotone solitary floor.
///
/// The **first** `hand_id` this peer ever dealt with `|P(k-1)| == 1`. Written
/// once, never cleared, not even when `P` grows again — that is `N4`, and it is
/// what makes it a floor rather than an answer.
///
/// Which hands were solitary is §4.0 step 10b's retained record. This field only
/// guarantees it never rejects a hand that record admits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SolitaryFloor {
    since: Option<u64>,
}

impl SolitaryFloor {
    /// At a hand init, when `P(k-1)` was read: `since := since.or(Some(k))`.
    ///
    /// Monotone, so a later hand with a larger `P` does not clear it.
    pub fn observe(&mut self, hand_id: u64, p: SeatSet) {
        if p.len() == 1 {
            self.since = self.since.or(Some(hand_id));
        }
    }

    pub fn since(&self) -> Option<u64> {
        self.since
    }

    /// `solitary_at(k) := since == Some(j) ∧ j <= k + 1 ∧ k <= hand_id`.
    ///
    /// **`j <= k + 1` is exactly one hand of slack, neither more nor less.** The
    /// form that stood here was `j <= k`, and it was false for the one class of
    /// hand the rule exists for: §3.2's regime test is a *disjunction*, and the
    /// walk that motivates it lands on the second disjunct first — the hand `P`
    /// narrows in has three seats in `P(k-1)` and one in `P(k)`, so nothing is
    /// written at hand `k`'s init and the floor becomes `Some(k+1)` at hand
    /// `k+1`'s. Under `j <= k` the engine would drop the best-evidenced
    /// contradiction the wire can deliver.
    ///
    /// Written `j <= k + 1` rather than `j - 1 <= k` because `j` is a `u64` and
    /// the two are the same inequality without the underflow.
    ///
    /// **The test is in the past tense**: the event's `hand_id`, never the
    /// receiver's current phase. `current_hand` is the receiver's own hand only
    /// as the upper bound `k <= hand_id`.
    pub fn solitary_at(&self, hand_id: u64, current_hand: u64) -> bool {
        match self.since {
            Some(j) => j <= hand_id.saturating_add(1) && hand_id <= current_hand,
            None => false,
        }
    }
}

/// The evidence a `SolitaryDivergence` carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolitaryDivergence {
    pub hand_id: u64,
    pub seat: u8,
    pub event_hash: Hash,
}

/// What T62 and T63 do with one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixOutcome {
    /// T62: freeze, `Diverged`, `Fault{SolitaryDivergence}`,
    /// `solitary_contradicted := true`.
    ///
    /// The offending event itself is **not applied and not counted into**
    /// `signed_this_hand` (§4.0 step 12a), so `P` does not grow from it.
    Diverged,
    /// T63: the same, inside `TableClosed`, and additionally
    /// `tournament_winner := None`.
    ///
    /// `TableClosed` stops being absorbing for exactly this one event class. It
    /// has to: a solitary tournament win is precisely the thing a late
    /// contradiction must be able to reverse.
    ReopenedClosedTable,
    /// The floor does not admit the hand named. Nothing happens.
    NotSolitary,
}

/// T62 and T63.
///
/// `table_closed` selects between them, and it is the only difference: the guard
/// is one predicate, read by these two rows and by nothing else.
///
/// **Specified, tested, and called by nothing -- by decision** (`S1-CH`, the
/// project owner, 2026-09-19; `PROTOCOL.md` `Q-10`). The client runs the
/// checkpoint-8 route of the solitary-stage rule (`StateHashOutcome::Diverged`
/// into §6.3's freeze) and no path raises a `SolitaryDivergence`: T63 is the one
/// transition that opens a closed table again, the two wins a bidirectional
/// partition leaves behind are play money, and what a client keeps about a
/// finished game has no road back from a win. This, `SolitaryFloor`,
/// `RetainedHands` and `HandRecord` stay as the trigger's tested half, to be
/// wired when a tournament win first means something outside its table.
pub fn on_solitary_divergence(
    floor: &SolitaryFloor,
    current_hand: u64,
    table_closed: bool,
    e: &SolitaryDivergence,
) -> PrefixOutcome {
    if !floor.solitary_at(e.hand_id, current_hand) {
        return PrefixOutcome::NotSolitary;
    }
    if table_closed {
        PrefixOutcome::ReopenedClosedTable
    } else {
        PrefixOutcome::Diverged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(b: u8) -> Hash {
        [b; 32]
    }

    fn open_at(hand: u64, number: u16, required: &[u8], own: u8) -> CheckpointState {
        CheckpointState::open(
            hand,
            number,
            100,
            SeatSet::from_seats(required).unwrap(),
            h(own),
        )
    }

    fn hash_event(hand: u64, cp: u16, seat: u8, value: u8) -> StateHashEvent {
        StateHashEvent {
            hand_id: hand,
            checkpoint: cp,
            seat,
            state_hash: h(value),
            round: 0,
        }
    }

    // -- the store ---------------------------------------------------------

    /// The defect `P1` fixed: a checkpoint-8 `STATE_HASH` of hand `k` arriving
    /// while hand `k+1` is live. With one slot the phrase "that checkpoint" had
    /// no single referent, and the arriving event named nothing.
    #[test]
    fn a_checkpoint_of_the_previous_hand_is_still_addressable() {
        let mut store = CheckpointStore::new();
        store.open(open_at(5, 8, &[0, 1], 0xAA));
        store.cross_boundary(); // hand 6 begins
        store.open(open_at(6, 8, &[0, 1], 0xBB));

        // Wrong: `open` clears boundary first, so hand 5's record is gone.
        assert_eq!(store.named(5, 8), None);
        assert!(store.named(6, 8).is_some());
    }

    /// The lifetime that keeps hand `k`'s record reachable is `cross_boundary`
    /// followed by nothing: it is dropped at the **next** opening, not at the
    /// boundary.
    #[test]
    fn the_boundary_record_survives_until_the_next_checkpoint_opens() {
        let mut store = CheckpointStore::new();
        store.open(open_at(5, 8, &[0, 1], 0xAA));
        store.cross_boundary();

        let (slot, c) = store.named(5, 8).expect("moved down, not dropped");
        assert_eq!(slot, Slot::Boundary);
        assert_eq!(c.own(), h(0xAA));

        store.open(open_at(6, 8, &[0, 1], 0xBB));
        assert_eq!(store.named(5, 8), None, "released when hand 6 opens");
    }

    /// An event naming no record is a rejection and never an insertion, which is
    /// what bounds the store at three slots against a network that can name any
    /// hand it likes.
    #[test]
    fn an_event_naming_no_record_creates_nothing() {
        let mut store = CheckpointStore::new();
        store.open(open_at(1, 8, &[0, 1], 0xAA));
        let before = store.clone();

        assert_eq!(
            store.on_state_hash(&hash_event(999, 8, 0, 0xAA)),
            StateHashOutcome::NoSuchCheckpoint
        );
        assert_eq!(
            store.on_state_hash(&hash_event(1, 3, 0, 0xAA)),
            StateHashOutcome::NoSuchCheckpoint
        );
        assert_eq!(store, before, "the state is bit-identical");
    }

    /// At most one slot matches a name, so `named` has one answer.
    #[test]
    fn at_most_one_slot_answers_a_name() {
        let mut store = CheckpointStore::new();
        store.open(open_at(7, 8, &[0], 0x11));
        store.cross_boundary();
        store.open(open_at(8, 8, &[0], 0x22));
        // boundary was cleared by `open`, so only hand 8 is addressable.
        assert!(store.named(8, 8).is_some());
        assert!(store.named(7, 8).is_none());
    }

    // -- T49 and T50 -------------------------------------------------------

    /// T49 and T50 are exhaustive and disjoint on `round == 0`, which is what
    /// makes the pair a total function of the arriving value.
    #[test]
    fn every_round_zero_value_takes_exactly_one_of_the_two_rows() {
        for value in [0xAAu8, 0xBB] {
            let mut store = CheckpointStore::new();
            store.open(open_at(1, 8, &[0, 1], 0xAA));
            let outcome = store.on_state_hash(&hash_event(1, 8, 1, value));
            match (value, outcome) {
                (0xAA, StateHashOutcome::Agreed) => {}
                (0xBB, StateHashOutcome::Diverged { .. }) => {}
                (v, o) => panic!("value {v:#x} took neither row: {o:?}"),
            }
            // Either way the seat was heard: the stage progresses even when the
            // value disagrees, because the emitter set is about who spoke.
            assert!(store.named(1, 8).unwrap().1.heard().contains(1));
        }
    }

    /// The evidence T50 retains is the **pair**, and the second value is written
    /// at most once — a third distinct value adds a signature to a divergence
    /// already detected and changes nothing.
    #[test]
    fn the_first_dissenting_value_is_the_one_kept() {
        let mut store = CheckpointStore::new();
        store.open(open_at(1, 8, &[0, 1, 2], 0xAA));

        store.on_state_hash(&hash_event(1, 8, 1, 0xBB));
        assert_eq!(store.named(1, 8).unwrap().1.dissent(), Some(h(0xBB)));

        store.on_state_hash(&hash_event(1, 8, 2, 0xCC));
        assert_eq!(
            store.named(1, 8).unwrap().1.dissent(),
            Some(h(0xBB)),
            "written at most once"
        );
        assert_eq!(store.named(1, 8).unwrap().1.own(), h(0xAA), "and own never");
    }

    /// N1: a one-member required set with two distinct values in it is a peer
    /// comparing its state against a set of one and being contradicted from
    /// outside it — T62's situation reported by a different message. Two
    /// distinct values cannot exist for one checkpoint unless a seat other than
    /// this one signed one of them.
    ///
    /// Without the conjunct, a solitary peer that diverges by hash rather than
    /// by emitter freezes, restores, deals another solitary hand, meets the same
    /// mismatch and freezes again, forever.
    #[test]
    fn a_mismatch_against_a_required_set_of_one_latches() {
        let mut store = CheckpointStore::new();
        store.open(open_at(4, 8, &[2], 0xAA));
        assert_eq!(
            store.on_state_hash(&hash_event(4, 8, 3, 0xBB)),
            StateHashOutcome::Diverged {
                solitary_contradicted: true
            }
        );
    }

    /// And the conjunct is what keeps it off healthy tables: where two or more
    /// seats were required, a mismatch still costs one hand and the table plays
    /// on. This is also the stale-boundary case at a receiver that was not alone.
    #[test]
    fn a_mismatch_against_a_larger_set_costs_one_hand_and_not_the_table() {
        let mut store = CheckpointStore::new();
        store.open(open_at(4, 8, &[0, 1], 0xAA));
        assert_eq!(
            store.on_state_hash(&hash_event(4, 8, 1, 0xBB)),
            StateHashOutcome::Diverged {
                solitary_contradicted: false
            }
        );
    }

    /// The regime is read off the named checkpoint's own required set, fixed
    /// when it opened — so it cannot be grown by the very copy that contradicts
    /// it.
    #[test]
    fn the_regime_is_the_checkpoints_own_and_not_the_current_one() {
        // Hand 4 was solitary. It is now in the boundary slot, and the table has
        // since grown - but the regime that is read is the one the checkpoint
        // opened with.
        let mut store = CheckpointStore::new();
        store.open(open_at(4, 8, &[2], 0xAA));
        store.cross_boundary();
        assert_eq!(
            store.on_state_hash(&hash_event(4, 8, 3, 0xBB)),
            StateHashOutcome::Diverged {
                solitary_contradicted: true
            },
            "hand 4 was solitary, whatever the table looks like now"
        );
    }

    /// A reconciliation round is not round 0 and takes neither row.
    #[test]
    fn a_reconciliation_round_is_out_of_scope_for_this_pair() {
        let mut store = CheckpointStore::new();
        store.open(open_at(1, 8, &[0, 1], 0xAA));
        let mut e = hash_event(1, 8, 1, 0xBB);
        e.round = 1;
        assert_eq!(store.on_state_hash(&e), StateHashOutcome::NoSuchCheckpoint);
        assert_eq!(store.named(1, 8).unwrap().1.dissent(), None);
    }

    // -- T51 ---------------------------------------------------------------

    /// `agreed` is set when the **ack stage** completes, not on the first ack.
    /// D-014's tier-2 precondition asks for a completed `STATE_ACK` stage, and
    /// before there was an `acked` set that was not a representable object.
    #[test]
    fn agreed_is_set_by_the_stage_and_not_by_the_first_ack() {
        let mut store = CheckpointStore::new();
        store.open(open_at(1, 8, &[0, 1], 0xAA));
        store.on_state_hash(&hash_event(1, 8, 0, 0xAA));
        store.on_state_hash(&hash_event(1, 8, 1, 0xAA));

        let ack = |seat| StateAckEvent {
            hand_id: 1,
            checkpoint: 8,
            seat,
            checkpoint_hash: h(0x77),
        };

        assert_eq!(
            store.on_state_ack(&ack(0)),
            StateAckOutcome::Recorded {
                stage_complete: false
            }
        );
        assert_eq!(store.named(1, 8).unwrap().1.agreed(), None);

        assert_eq!(
            store.on_state_ack(&ack(1)),
            StateAckOutcome::Recorded {
                stage_complete: true
            }
        );
        assert_eq!(store.named(1, 8).unwrap().1.agreed(), Some(h(0x77)));
    }

    /// T51 needs the hash stage complete **and** unanimous. A checkpoint that
    /// diverged never becomes agreed, however many acks arrive.
    #[test]
    fn a_diverged_checkpoint_is_never_agreed() {
        let mut store = CheckpointStore::new();
        store.open(open_at(1, 8, &[0, 1], 0xAA));
        store.on_state_hash(&hash_event(1, 8, 0, 0xAA));
        store.on_state_hash(&hash_event(1, 8, 1, 0xBB));

        for seat in [0u8, 1] {
            assert_eq!(
                store.on_state_ack(&StateAckEvent {
                    hand_id: 1,
                    checkpoint: 8,
                    seat,
                    checkpoint_hash: h(0x77),
                }),
                StateAckOutcome::NotApplicable
            );
        }
        assert_eq!(store.named(1, 8).unwrap().1.agreed(), None);
    }

    /// And an incomplete hash stage is not ackable either.
    #[test]
    fn an_incomplete_hash_stage_is_not_ackable() {
        let mut store = CheckpointStore::new();
        store.open(open_at(1, 8, &[0, 1], 0xAA));
        store.on_state_hash(&hash_event(1, 8, 0, 0xAA));
        assert_eq!(
            store.on_state_ack(&StateAckEvent {
                hand_id: 1,
                checkpoint: 8,
                seat: 0,
                checkpoint_hash: h(0x77),
            }),
            StateAckOutcome::NotApplicable
        );
    }

    /// A checkpoint-8 `STATE_ACK` of hand `k` arriving after hand `k+1` started
    /// **can** set `agreed`, which is what §4.9 accepts and what the single slot
    /// made impossible.
    #[test]
    fn a_late_ack_can_still_agree_a_boundary_checkpoint() {
        let mut store = CheckpointStore::new();
        store.open(open_at(5, 8, &[0, 1], 0xAA));
        store.on_state_hash(&hash_event(5, 8, 0, 0xAA));
        store.on_state_hash(&hash_event(5, 8, 1, 0xAA));
        store.cross_boundary();

        let ack = |seat| StateAckEvent {
            hand_id: 5,
            checkpoint: 8,
            seat,
            checkpoint_hash: h(0x99),
        };
        store.on_state_ack(&ack(0));
        assert_eq!(
            store.on_state_ack(&ack(1)),
            StateAckOutcome::Recorded {
                stage_complete: true
            }
        );
        assert_eq!(
            store.agreed_checkpoint(5).map(|c| c.agreed()),
            Some(Some(h(0x99)))
        );
    }

    /// `agreed_checkpoint` takes the greatest number, because a later checkpoint
    /// of the same hand supersedes an earlier one as evidence.
    #[test]
    fn the_agreed_checkpoint_is_the_latest_one() {
        let mut store = CheckpointStore::new();
        assert!(store.agreed_checkpoint(1).is_none());

        store.open(open_at(1, 3, &[0], 0xAA));
        store.on_state_hash(&hash_event(1, 3, 0, 0xAA));
        store.on_state_ack(&StateAckEvent {
            hand_id: 1,
            checkpoint: 3,
            seat: 0,
            checkpoint_hash: h(0x33),
        });
        assert_eq!(
            store.agreed_checkpoint(1).map(|c| c.number()),
            Some(3),
            "one agreed record, and it is that one"
        );
    }

    // -- the solitary floor ------------------------------------------------

    /// Monotone: written once, never cleared, not even when `P` grows again.
    #[test]
    fn the_floor_is_written_once_and_never_cleared() {
        let mut floor = SolitaryFloor::default();
        assert_eq!(floor.since(), None);

        floor.observe(3, SeatSet::from_seats(&[0, 1]).unwrap());
        assert_eq!(floor.since(), None, "two seats is not solitary");

        floor.observe(4, SeatSet::from_seats(&[0]).unwrap());
        assert_eq!(floor.since(), Some(4));

        floor.observe(5, SeatSet::from_seats(&[1]).unwrap());
        assert_eq!(floor.since(), Some(4), "the FIRST such hand");

        floor.observe(6, SeatSet::from_seats(&[0, 1, 2]).unwrap());
        assert_eq!(floor.since(), Some(4), "and P growing does not clear it");
    }

    /// One hand of slack, and it is the class of hand the rule exists for.
    ///
    /// The hand `P` narrows in has three seats in `P(k-1)` and one in `P(k)`, so
    /// nothing is written at hand `k`'s init and the floor becomes `Some(k+1)`
    /// at hand `k+1`'s. Under `j <= k` the engine drops the best-evidenced
    /// contradiction the wire can deliver.
    #[test]
    fn the_slack_is_exactly_one_hand() {
        let mut floor = SolitaryFloor::default();
        floor.observe(9, SeatSet::from_seats(&[0]).unwrap()); // j = 9

        assert!(
            floor.solitary_at(8, 20),
            "hand 8 is admitted: j <= k + 1 is 9 <= 9"
        );
        assert!(floor.solitary_at(9, 20));
        assert!(floor.solitary_at(15, 20));
        assert!(
            !floor.solitary_at(7, 20),
            "and one more hand back is not: 9 <= 8 is false"
        );
    }

    /// The upper bound is the receiver's own hand, so a hand it has not reached
    /// cannot be claimed to have been solitary.
    #[test]
    fn a_hand_the_receiver_has_not_reached_is_not_admitted() {
        let mut floor = SolitaryFloor::default();
        floor.observe(2, SeatSet::from_seats(&[0]).unwrap());
        assert!(floor.solitary_at(5, 5));
        assert!(!floor.solitary_at(6, 5));
        assert!(!floor.solitary_at(u64::MAX, 5), "and no overflow either");
    }

    /// A peer that was never alone admits nothing.
    #[test]
    fn a_floor_that_was_never_written_admits_nothing() {
        let floor = SolitaryFloor::default();
        for k in 0..100 {
            assert!(!floor.solitary_at(k, 100));
        }
    }

    // -- T62 and T63 -------------------------------------------------------

    /// The test is in the past tense: the event's `hand_id`, never the
    /// receiver's current phase. If it were scoped on the current hand, T62
    /// would be unreachable — the event that names a hand other than the current
    /// one is the only event of its kind.
    #[test]
    fn t62_freezes_on_a_hand_that_was_solitary() {
        let mut floor = SolitaryFloor::default();
        floor.observe(3, SeatSet::from_seats(&[0]).unwrap());

        let e = SolitaryDivergence {
            hand_id: 4,
            seat: 2,
            event_hash: h(0x5A),
        };
        assert_eq!(
            on_solitary_divergence(&floor, 40, false, &e),
            PrefixOutcome::Diverged,
            "hand 4 is long past, and that is the point"
        );
    }

    /// T63: `TableClosed` stops being absorbing for exactly this one event
    /// class. It has to — a solitary tournament win is precisely the thing a
    /// late contradiction must be able to reverse.
    #[test]
    fn t63_reopens_a_closed_table_and_nothing_else_does() {
        let mut floor = SolitaryFloor::default();
        floor.observe(3, SeatSet::from_seats(&[0]).unwrap());
        let e = SolitaryDivergence {
            hand_id: 4,
            seat: 2,
            event_hash: h(0x5A),
        };
        assert_eq!(
            on_solitary_divergence(&floor, 40, true, &e),
            PrefixOutcome::ReopenedClosedTable
        );
    }

    /// And the guard still has to hold: a table that was never solitary is not
    /// reopened by an event claiming it was.
    #[test]
    fn a_claim_the_floor_does_not_admit_does_nothing() {
        let floor = SolitaryFloor::default();
        let e = SolitaryDivergence {
            hand_id: 4,
            seat: 2,
            event_hash: h(0x5A),
        };
        assert_eq!(
            on_solitary_divergence(&floor, 40, true, &e),
            PrefixOutcome::NotSolitary
        );
        assert_eq!(
            on_solitary_divergence(&floor, 40, false, &e),
            PrefixOutcome::NotSolitary
        );
    }

    // -- the retained records ----------------------------------------------

    /// `was_solitary` is the disjunction and an independent bool, never
    /// `|p| == 1`: the second disjunct is about `P(k)`, a set this record does
    /// not hold, so deriving it from `p` would be wrong on exactly the hands
    /// that matter.
    #[test]
    fn was_solitary_is_not_derivable_from_p() {
        let record = HandRecord {
            hand_id: 4,
            was_solitary: true,
            p: SeatSet::from_seats(&[0, 1, 2]).unwrap(),
            checkpoint8_state_hash: h(1),
        };
        assert!(record.was_solitary);
        assert_eq!(record.p.len(), 3, "P(k-1) had three seats, and it was still solitary");
    }

    #[test]
    fn records_are_kept_and_found_by_hand() {
        let mut r = RetainedHands::new();
        for k in 1..=5u64 {
            r.insert(HandRecord {
                hand_id: k,
                was_solitary: k % 2 == 0,
                p: SeatSet::from_seats(&[0]).unwrap(),
                checkpoint8_state_hash: h(k as u8),
            });
        }
        assert_eq!(r.len(), 5);
        assert!(r.get(4).unwrap().was_solitary);
        assert!(!r.get(3).unwrap().was_solitary);
        assert!(r.get(9).is_none());
    }

    /// The replay window and the retention window are the same, which is not a
    /// coincidence: a retained record is what makes a stale event evaluable, so
    /// an event past the window cannot do anything.
    #[test]
    fn the_store_is_capped_and_evicts_the_oldest() {
        let mut r = RetainedHands::new();
        for k in 1..=(MAX_RETAINED_HAND_RECORDS as u64 + 10) {
            r.insert(HandRecord {
                hand_id: k,
                was_solitary: false,
                p: SeatSet::EMPTY,
                checkpoint8_state_hash: h(0),
            });
        }
        assert_eq!(r.len(), MAX_RETAINED_HAND_RECORDS);
        assert!(r.get(1).is_none(), "the oldest went first");
        assert!(r.get(MAX_RETAINED_HAND_RECORDS as u64 + 10).is_some());
    }

    /// Re-recording one hand replaces it rather than growing the store, so a
    /// forwarded duplicate cannot push the window.
    #[test]
    fn a_repeated_hand_does_not_consume_a_second_slot() {
        let mut r = RetainedHands::new();
        for _ in 0..1_000 {
            r.insert(HandRecord {
                hand_id: 7,
                was_solitary: true,
                p: SeatSet::EMPTY,
                checkpoint8_state_hash: h(0),
            });
        }
        assert_eq!(r.len(), 1);
    }
}
