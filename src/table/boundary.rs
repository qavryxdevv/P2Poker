//! The boundary checkpoint of hand `k`: its two stages, and how long they live.
//!
//! `PROTOCOL.md` §4.9 places checkpoint **8** at every hand boundary, on every
//! hand and on **both** terminal paths, the moment `TERMINAL(k)` is fixed at
//! this peer. §6.2 row 8 carries it and §6.1 calls it *"the only way a silent
//! divergence is ever caught"*. `S1-O` found the second job it does: §4.9's
//! readmission set `A` is written by a checkpoint-8 `STATE_HASH` that agrees, so
//! this is also the door back for a seat that missed a hand.
//!
//! # Why it is not part of `Hand`
//!
//! It **outlives** the hand it is about. §4.9 admits a checkpoint-8 `STATE_HASH`
//! of hand `k` until `TERMINAL(k+1)` is fixed, and by then hand `k+1` is live;
//! §5.3 names the band as *"the one structure that outlives its hand"*. A
//! `Hand` that owned this would be dropped with the stage half open.
//!
//! # The two stages, and why their sets differ
//!
//! | | `STATE_HASH` | `STATE_ACK` |
//! |---|---|---|
//! | sequence | `8 192` | `8 193` |
//! | required | `P(k)` | `P(k)` |
//! | accepted | **every occupied roster seat** | `P(k)` |
//!
//! `STATE_HASH`'s required set is the **snapshot** of `P(k)` at the moment
//! `TERMINAL(k)` is fixed, and §4.9 is explicit that this is the one required
//! set in the document that is `P(k)` rather than `P(k-1)` or a subset: it is
//! the stage whose whole job is to publish what `P(k)` is. The snapshot is what
//! removes the circularity — a checkpoint-8 `STATE_HASH` is itself a chain-`k`
//! event and adds its sender to `P(k)`, so a set read after the stage opened
//! would grow with its own contributions and never complete.
//!
//! **The comparison is not scoped on that set** (§4.9, the disposition of `L3`):
//! a copy from any occupied roster seat is *accepted, compared and retained*. It
//! is not a stage violation and §4.0 step 12 must not reject it. It cannot
//! complete the stage and cannot block it — completion is `heard ⊇ P(k)` and
//! nothing else — and it occupies one slot per seat, which is what stops two
//! peers whose `P` has forked from never colliding.
//!
//! **`STATE_ACK` is not widened** (§4.9): an ack asserts that this peer saw the
//! complete required set and that every member agreed, and a seat outside the
//! set has not seen the set and has nothing to assert.
//!
//! # Retention is the store's, not a second rule beside it
//!
//! `CheckpointStore` already implements §4.9's lifetime in three slots: `open`
//! drops the `boundary` slot and fills `live`, and `cross_boundary` moves `live`
//! down at T47. This type holds the two `Collective`s the store does not — it
//! carries the comparison, they carry the stage — and **prunes itself against
//! the store** after every move, so there is one retention rule rather than two
//! that can come to disagree. A `Boundary` whose record the store has released
//! would accept events into a stage nothing compares.

use crate::protocol::checkpoint::{
    CheckpointState, CheckpointStore, StateAckEvent, StateAckOutcome, StateHashEvent,
    StateHashOutcome,
};
use crate::poker::state::{Hash, SeatIdx};
use crate::protocol::messages::EventType;
use crate::protocol::seats::SeatSet;
use crate::table::checkwire;
use crate::table::stage::{Collective, Heard};
use std::collections::BTreeMap;


/// One hand's boundary checkpoint.
#[derive(Debug, Clone)]
pub struct Boundary {
    hand_id: u64,
    /// The table, so the acknowledgement can be sealed after the `Hand` that
    /// produced the checkpoint is gone. It is: §4.9 keeps this stage open past
    /// `HAND_INIT(k+1)`, and by then hand `k` has been dropped.
    table_id: Hash,
    /// `TERMINAL(k)`, which is what the `STATE_HASH` event chains from.
    terminal: Hash,
    /// `P(k)`, snapshot at `TERMINAL(k)`. The required set of both stages, and
    /// the set §4.4 draws hand `k+1`'s `dealt_in` from.
    participants: Vec<SeatIdx>,
    /// This peer's own `state_hash`, repeated in its acknowledgement.
    own: Hash,
    hash_stage: Collective,
    ack_stage: Collective,
    /// Whether this peer has published its own `STATE_ACK` for this checkpoint.
    ///
    /// One copy per seat per stage, and the ack is emitted on a **condition**
    /// that stays true once it is reached — `hash_stage.complete()` and
    /// unanimous — so without this the second `STATE_HASH` to arrive after
    /// completion would produce a second ack and read as equivocation to
    /// everybody else.
    ack_sent: bool,
}

impl Boundary {
    pub fn hand_id(&self) -> u64 {
        self.hand_id
    }
    pub fn terminal(&self) -> Hash {
        self.terminal
    }
    /// `P(k)`.
    pub fn participants(&self) -> &[SeatIdx] {
        &self.participants
    }
    /// The `stage_hash` of the `STATE_HASH` stage, once it is complete. This is
    /// `STATE_ACK`'s `checkpoint_hash`.
    pub fn checkpoint_hash(&self) -> Option<Hash> {
        self.hash_stage.hash()
    }
    pub fn hash_stage_complete(&self) -> bool {
        self.hash_stage.complete()
    }
    pub fn ack_stage_complete(&self) -> bool {
        self.ack_stage.complete()
    }
    /// Which seats of `P(k)` have not published a `STATE_HASH` here yet.
    pub fn waiting_for(&self) -> Vec<SeatIdx> {
        self.hash_stage.waiting_for()
    }

    /// Which seats of `P(k)` have not acknowledged yet.
    ///
    /// The two stages fail differently and a single *"not complete"* cannot say
    /// which: nobody's hash arriving and everybody's hash arriving with nobody's
    /// acknowledgement are different faults with different causes.
    pub fn acks_waiting_for(&self) -> Vec<SeatIdx> {
        self.ack_stage.waiting_for()
    }

    /// This peer's own `STATE_ACK`, once the `STATE_HASH` stage has completed.
    ///
    /// `None` before that, because `checkpoint_hash` is the `stage_hash` of a
    /// stage that has not closed and there is nothing to acknowledge.
    ///
    /// **It chains from the stage it acknowledges**, not from `TERMINAL(k)`: the
    /// `STATE_HASH` stage is the previous stage of this chain and its
    /// `stage_hash` is exactly what §5.2.1 makes a parent. So `checkpoint_hash`
    /// appears twice, once as the parent and once in the body — §4.9 asks for
    /// it in the body so that an acknowledgement says **what** it acknowledges
    /// rather than only that it does, and a bare ack of a stage is an ack of
    /// whatever the reader thinks that stage said.
    ///
    /// **It arms no deadline**, for the reason the `STATE_HASH` does not: its
    /// window runs to `TERMINAL(k+1)`, which belongs to the hand after and is a
    /// promise about somebody else's clock.
    pub fn state_ack_event(
        &self,
        key: &ed25519_dalek::SigningKey,
        now_ms: u64,
    ) -> Option<Vec<u8>> {
        let checkpoint_hash = self.checkpoint_hash()?;
        let body = checkwire::StateAck {
            checkpoint: checkwire::BOUNDARY_CHECKPOINT,
            agreed_state_hash: self.own,
            checkpoint_hash,
        };
        let slot = crate::net::chained::Slot {
            table_id: self.table_id,
            hand_id: self.hand_id,
            sequence: checkwire::ack_sequence(0)?,
            previous_event_hash: checkpoint_hash,
        };
        crate::net::chained::seal(
            EventType::StateAck,
            &slot,
            &body,
            key,
            now_ms,
            0,
            STATE_ACK_CAP,
        )
        .ok()
    }
}

/// How much of a `STATE_ACK` body this client will decode. §9.3's cap for the
/// checkpoint bodies, which are three small fixed fields.
pub const STATE_ACK_CAP: usize = 512;

/// What accepting one checkpoint-8 event did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Took {
    /// Counted, and this peer's own `STATE_ACK` is now due: the `STATE_HASH`
    /// stage completed on this event and every value in it agreed.
    ///
    /// Returned **once**. See [`Boundary::ack_sent`].
    AckIsDue,
    /// Counted, and nothing further is due.
    Counted,
    /// The `STATE_ACK` stage completed: every member of `P(k)` acknowledged the
    /// same `checkpoint_hash`, and [`Boundaries::agreed`] now answers.
    ///
    /// **Distinguished from `Counted` so that a caller can say it happened.**
    /// A checkpoint that agrees produces no divergence warning, and a checkpoint
    /// stage that never ran produces no divergence warning either — the two are
    /// identical in a log that only reports faults, and one of them is the
    /// mechanism §6.1 calls the only way a silent divergence is ever caught.
    Agreed,
    /// From a seat outside `P(k)` and inside the roster: compared and retained,
    /// not counted. §4.9's `L3`, and the copy that writes §4.9's readmission
    /// set when it agrees.
    Bystander,
    /// The same seat saying the same thing again. A mesh redelivers as a matter
    /// of course.
    Again,
    /// The same seat, a different event, in one stage.
    Equivocation { first: Hash, second: Hash },
    /// A value differing from this peer's own. §6.3's freeze is not built, so
    /// this is reported and nothing else happens.
    Diverged { solitary_contradicted: bool },
    /// Not a seat this stage will hear, or no such checkpoint.
    Uninvited,
}

/// Every boundary checkpoint this peer still admits events for.
#[derive(Debug)]
pub struct Boundaries {
    store: CheckpointStore,
    open: BTreeMap<u64, Boundary>,
}

impl Default for Boundaries {
    fn default() -> Self {
        Self::new()
    }
}

impl Boundaries {
    pub fn new() -> Self {
        Boundaries {
            store: CheckpointStore::new(),
            open: BTreeMap::new(),
        }
    }

    /// Open hand `k`'s boundary checkpoint, at the moment `TERMINAL(k)` is fixed.
    ///
    /// `roster` is every **occupied** seat of the table, which is the accepted
    /// set of the `STATE_HASH` stage and nothing else; `participants` is `P(k)`
    /// and is the required set of both.
    ///
    /// Returns `None` if the sets contradict each other — `P(k)` must be a
    /// subset of the roster, and a caller for whom it is not has a bug this
    /// should not paper over.
    pub fn open(
        &mut self,
        hand_id: u64,
        table_id: Hash,
        terminal: Hash,
        own_state_hash: Hash,
        participants: &[SeatIdx],
        roster: &[SeatIdx],
    ) -> Option<&Boundary> {
        let hash_stage = Collective::new(
            checkwire::hash_sequence(0)?,
            EventType::StateHash.code(),
            participants,
            roster,
        )?;
        // Not widened: §4.9. `closed` is `new(required, required)`.
        let ack_stage = Collective::closed(
            checkwire::ack_sequence(0)?,
            EventType::StateAck.code(),
            participants,
        )?;
        let required = SeatSet::from_seats(participants).ok()?;
        // Drops the store's `boundary` slot — hand `k-1`'s, whose window §4.9
        // closed when `TERMINAL(k)` was fixed, which is now.
        self.store.open(CheckpointState::open(
            hand_id,
            checkwire::BOUNDARY_CHECKPOINT,
            checkwire::hash_sequence(0)?,
            required,
            own_state_hash,
        ));
        self.open.insert(
            hand_id,
            Boundary {
                hand_id,
                table_id,
                terminal,
                participants: participants.to_vec(),
                own: own_state_hash,
                hash_stage,
                ack_stage,
                ack_sent: false,
            },
        );
        self.prune();
        self.open.get(&hand_id)
    }

    /// T47: hand `k+1`'s `HAND_INIT` stage completed, so hand `k`'s checkpoint
    /// moves down to the boundary slot.
    ///
    /// **Moved, not dropped.** §4.9 keeps admitting `STATE_ACK` copies into it —
    /// `N6`, *"the checkpoint-8 `STATE_ACK` stage is not closed by that
    /// window"* — and what the window bounds is the admission of a `STATE_HASH`,
    /// because that is the only checkpoint-8 event that can grow `P(k)`.
    pub fn cross_boundary(&mut self) {
        self.store.cross_boundary();
        self.prune();
    }

    /// Keep exactly the boundaries the store still names, and nothing else.
    fn prune(&mut self) {
        let gone: Vec<u64> = self
            .open
            .keys()
            .copied()
            .filter(|h| {
                self.store
                    .named(*h, checkwire::BOUNDARY_CHECKPOINT)
                    .is_none()
            })
            .collect();
        for h in gone {
            self.open.remove(&h);
        }
    }

    pub fn get(&self, hand_id: u64) -> Option<&Boundary> {
        self.open.get(&hand_id)
    }

    /// Whether hand `k`'s boundary is one this peer still admits events for.
    pub fn holds(&self, hand_id: u64) -> bool {
        self.open.contains_key(&hand_id)
    }

    /// The newest boundary held, for saying where the checkpoint has got to.
    ///
    /// *"Waiting for seat 4"* is a sentence somebody can act on; a stage that
    /// silently never closes is not, and a checkpoint that agrees and a
    /// checkpoint that never ran are the same silence.
    pub fn newest(&self) -> Option<&Boundary> {
        self.open.values().next_back()
    }

    /// The `checkpoint_hash` every member of `P(k)` acknowledged, once the
    /// `STATE_ACK` stage has completed. `None` before that, and it is the whole
    /// point of the ack stage that it is not set on the first ack.
    pub fn agreed(&self, hand_id: u64) -> Option<Hash> {
        self.store
            .agreed_checkpoint(hand_id)
            .and_then(|c| c.agreed())
    }

    /// One checkpoint-8 `STATE_HASH`.
    ///
    /// `event_hash` addresses the copy in the stage; `state_hash` is the value
    /// being compared. They are different things and a caller that passed one
    /// for the other would produce a stage hash over the wrong quantity, so
    /// they are separate arguments rather than one struct.
    pub fn on_state_hash(
        &mut self,
        hand_id: u64,
        seat: SeatIdx,
        event_hash: Hash,
        state_hash: Hash,
    ) -> Took {
        let Some(b) = self.open.get_mut(&hand_id) else {
            return Took::Uninvited;
        };
        let heard = b.hash_stage.hear(seat, event_hash);
        match heard {
            Heard::Uninvited => return Took::Uninvited,
            Heard::Again => return Took::Again,
            Heard::Equivocation { first, second } => return Took::Equivocation { first, second },
            Heard::Counted | Heard::Bystander => {}
        }

        // The store carries T49/T50 — the comparison and the retained evidence
        // — and is fed for **every** accepted copy, bystanders included: §4.9
        // scopes the comparison on the roster and not on `P(k)`, and a
        // bystander whose value differs is exactly the fork this is for.
        let outcome = self.store.on_state_hash(&StateHashEvent {
            hand_id,
            checkpoint: checkwire::BOUNDARY_CHECKPOINT,
            seat,
            state_hash,
            round: 0,
        });
        if let StateHashOutcome::Diverged {
            solitary_contradicted,
        } = outcome
        {
            return Took::Diverged {
                solitary_contradicted,
            };
        }
        if matches!(heard, Heard::Bystander) {
            return Took::Bystander;
        }
        // Due once, and only on a stage that completed **and** agreed. The
        // store's own `on_state_ack` refuses an ack for a checkpoint whose hash
        // stage is incomplete or carries a dissent, so emitting one here would
        // be emitting a message every receiver drops.
        let Some(b) = self.open.get_mut(&hand_id) else {
            return Took::Counted;
        };
        // `named`, not `agreed_checkpoint`: the latter is filtered on `agreed`
        // being set, which is what the **ack** stage completing writes, so
        // asking it here would be asking whether the acknowledgement has already
        // happened before deciding to acknowledge.
        if !b.ack_sent
            && b.hash_stage.complete()
            && self
                .store
                .named(hand_id, checkwire::BOUNDARY_CHECKPOINT)
                .map(|(_, c)| c.dissent().is_none())
                .unwrap_or(false)
        {
            b.ack_sent = true;
            return Took::AckIsDue;
        }
        Took::Counted
    }

    /// One checkpoint-8 `STATE_ACK`.
    pub fn on_state_ack(
        &mut self,
        hand_id: u64,
        seat: SeatIdx,
        event_hash: Hash,
        checkpoint_hash: Hash,
    ) -> Took {
        let Some(b) = self.open.get_mut(&hand_id) else {
            return Took::Uninvited;
        };
        match b.ack_stage.hear(seat, event_hash) {
            Heard::Uninvited => return Took::Uninvited,
            Heard::Again => return Took::Again,
            Heard::Equivocation { first, second } => return Took::Equivocation { first, second },
            // `ack_stage` is closed, so there are no bystanders in it.
            Heard::Counted | Heard::Bystander => {}
        }
        match self.store.on_state_ack(&StateAckEvent {
            hand_id,
            checkpoint: checkwire::BOUNDARY_CHECKPOINT,
            seat,
            checkpoint_hash,
        }) {
            StateAckOutcome::Recorded {
                stage_complete: true,
            } => Took::Agreed,
            StateAckOutcome::Recorded { .. } | StateAckOutcome::NotApplicable => Took::Counted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: Hash = [3u8; 32];
    const TERMINAL: Hash = [7u8; 32];
    const STATE: Hash = [9u8; 32];

    fn ev(seat: u8) -> Hash {
        [seat; 32]
    }

    /// The shape §4.9 specifies, asserted against the specification's own
    /// numbers: two stages, adjacent, at the base of the band.
    #[test]
    fn the_two_stages_sit_where_the_specification_puts_them() {
        let mut b = Boundaries::new();
        let c = b
            .open(4, TABLE, TERMINAL, STATE, &[0, 1, 2], &[0, 1, 2, 3])
            .expect("P(k) is inside the roster");
        assert_eq!(c.hand_id(), 4);
        assert_eq!(c.terminal(), TERMINAL);
        assert_eq!(c.participants(), &[0, 1, 2]);
        assert_eq!(checkwire::hash_sequence(0), Some(8192));
        assert_eq!(checkwire::ack_sequence(0), Some(8193));
    }

    /// §4.9's `L3`: a seat outside `P(k)` is **compared, not rejected** — and it
    /// neither completes the stage nor holds it open.
    ///
    /// This is the readmission door of `S1-O`: the copy that agrees is the one
    /// that writes `A`.
    #[test]
    fn a_seat_outside_the_required_set_is_compared_and_does_not_count() {
        let mut b = Boundaries::new();
        b.open(4, TABLE, TERMINAL, STATE, &[0, 1], &[0, 1, 2, 3])
            .expect("opens");

        assert_eq!(
            b.on_state_hash(4, 3, ev(3), STATE),
            Took::Bystander,
            "seat 3 is in the roster and outside P(k): compared, not counted"
        );
        assert!(
            !b.get(4).expect("held").hash_stage_complete(),
            "and it did not complete a stage it is not required in"
        );
        assert_eq!(
            b.get(4).expect("held").waiting_for(),
            vec![0, 1],
            "the stage is still waiting for exactly P(k)"
        );
    }

    /// The ack is due **once**, when the hash stage completes and agrees.
    ///
    /// One copy per seat per stage: a second ack from this peer would read as
    /// equivocation to everybody else, and the condition that triggers it stays
    /// true for ever once reached.
    #[test]
    fn the_acknowledgement_is_due_once_and_only_on_a_complete_unanimous_stage() {
        let mut b = Boundaries::new();
        b.open(4, TABLE, TERMINAL, STATE, &[0, 1], &[0, 1, 2])
            .expect("opens");

        assert_eq!(b.on_state_hash(4, 0, ev(0), STATE), Took::Counted);
        assert_eq!(
            b.on_state_hash(4, 1, ev(1), STATE),
            Took::AckIsDue,
            "the stage completed on this event and every value agreed"
        );
        assert!(b.get(4).expect("held").checkpoint_hash().is_some());

        // A late bystander cannot make it due a second time.
        assert_eq!(b.on_state_hash(4, 2, ev(2), STATE), Took::Bystander);
    }

    /// A value that differs is a divergence, and it is reported rather than
    /// counted towards an acknowledgement nobody could act on.
    #[test]
    fn a_differing_value_is_a_divergence_and_no_acknowledgement_follows() {
        let mut b = Boundaries::new();
        b.open(4, TABLE, TERMINAL, STATE, &[0, 1], &[0, 1]).expect("opens");

        assert_eq!(b.on_state_hash(4, 0, ev(0), STATE), Took::Counted);
        assert!(matches!(
            b.on_state_hash(4, 1, ev(1), [1u8; 32]),
            Took::Diverged { .. }
        ));
        assert!(
            b.agreed(4).is_none(),
            "nothing is agreed at a checkpoint that carries a dissent"
        );
    }

    /// The acknowledgement stage sets `agreed` when it **completes**, and not on
    /// the first ack.
    #[test]
    fn the_agreed_value_appears_when_the_acknowledgement_stage_completes() {
        let mut b = Boundaries::new();
        b.open(4, TABLE, TERMINAL, STATE, &[0, 1], &[0, 1]).expect("opens");
        b.on_state_hash(4, 0, ev(0), STATE);
        b.on_state_hash(4, 1, ev(1), STATE);
        let cp = b.get(4).expect("held").checkpoint_hash().expect("complete");

        assert_eq!(b.on_state_ack(4, 0, ev(10), cp), Took::Counted);
        assert!(b.agreed(4).is_none(), "one ack is not the stage");
        assert_eq!(
            b.on_state_ack(4, 1, ev(11), cp),
            Took::Agreed,
            "the stage completed, and the caller is told so it can say it happened"
        );
        assert_eq!(
            b.agreed(4),
            Some(cp),
            "and the value is the stage hash the acks named"
        );
    }

    /// The acknowledgement is at the specification's address and chains from
    /// the stage it acknowledges.
    ///
    /// Both halves matter and neither is implied by the other: an ack at the
    /// wrong `sequence` is a stage violation at §4.0 step 12, and one chaining
    /// from `TERMINAL(k)` rather than from the `STATE_HASH` stage would be a
    /// second event in that stage's slot rather than the stage after it.
    #[test]
    fn the_acknowledgement_is_at_the_next_sequence_and_chains_from_the_hash_stage() {
        let mut b = Boundaries::new();
        b.open(4, TABLE, TERMINAL, STATE, &[0, 1], &[0, 1])
            .expect("opens");
        let key = ed25519_dalek::SigningKey::from_bytes(&[5u8; 32]);
        assert!(
            b.get(4).expect("held").state_ack_event(&key, 1).is_none(),
            "nothing to acknowledge before the stage it acknowledges has closed"
        );

        b.on_state_hash(4, 0, ev(0), STATE);
        b.on_state_hash(4, 1, ev(1), STATE);
        let held = b.get(4).expect("held");
        let cp = held.checkpoint_hash().expect("the stage closed");
        let bytes = held.state_ack_event(&key, 1).expect("and now there is one");

        let opened = crate::net::chained::open_in_hand(
            &bytes,
            512,
            EventType::StateAck,
            &TABLE,
            4,
        )
        .expect("it is a well-formed chained STATE_ACK of hand 4");
        assert_eq!(opened.envelope.sequence, 8193);
        assert_eq!(
            opened.envelope.previous_event_hash, cp,
            "the parent is the stage_hash of the STATE_HASH stage"
        );
        let body: checkwire::StateAck =
            crate::net::chained::payload(&opened, 512).expect("the body decodes");
        assert_eq!(body.checkpoint, checkwire::BOUNDARY_CHECKPOINT);
        assert_eq!(
            body.checkpoint_hash, cp,
            "and it says what it acknowledges rather than only that it does"
        );
        assert_eq!(body.agreed_state_hash, STATE);
    }

    /// §4.9's lifetime, driven the way the store implements it: hand `k`'s
    /// checkpoint is **moved down** when hand `k+1`'s `HAND_INIT` completes and
    /// released when `TERMINAL(k+1)` opens the next one.
    ///
    /// The assertion that matters is the **lockstep**: a `Boundary` the store no
    /// longer names would accept events into a stage nothing compares, which is
    /// worse than not holding it at all.
    #[test]
    fn a_boundary_lives_exactly_as_long_as_the_record_it_compares_against() {
        let mut b = Boundaries::new();
        b.open(1, TABLE, TERMINAL, STATE, &[0], &[0]).expect("opens");
        assert!(b.holds(1));

        // Hand 2's HAND_INIT completes: hand 1's moves down, still admitting
        // acknowledgements.
        b.cross_boundary();
        assert!(b.holds(1), "moved, not dropped");

        // TERMINAL(2): hand 2's checkpoint opens and hand 1's window is over.
        b.open(2, TABLE, TERMINAL, STATE, &[0], &[0]).expect("opens");
        assert!(
            !b.holds(1) && b.holds(2),
            "one retention rule, and it is the store's"
        );
    }

    /// The lockstep again, from the other side: opening twice without crossing
    /// releases the first, because the store's `open` drops its boundary slot.
    ///
    /// Without the prune this type would keep a stage whose comparison had gone,
    /// and every value arriving into it would read as agreement.
    #[test]
    fn a_boundary_the_store_has_released_is_not_held_here_either() {
        let mut b = Boundaries::new();
        b.open(1, TABLE, TERMINAL, STATE, &[0], &[0]).expect("opens");
        b.open(2, TABLE, TERMINAL, STATE, &[0], &[0]).expect("opens");
        assert!(!b.holds(1) && b.holds(2));
    }
}
