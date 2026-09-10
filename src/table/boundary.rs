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
//!
//! # §4.10's hand boundary window lives beside the checkpoint, not inside it
//!
//! The window carrying `PLAYER_SIT_OUT`, `PLAYER_SIT_IN` and `PLAYER_LEAVE`
//! opens at the same instant this checkpoint does — `TERMINAL(k)` fixed — and
//! for the same reason it cannot live in `Hand`: it belongs to the hand that has
//! just ended and is read while the next one is live. `S1-BZ` is what it closes:
//! three chained types were declared on the wire and answered with *wrong type*
//! by the only handler that saw them.
//!
//! **It was folded into [`Boundary`] and that was wrong, on the one path that
//! matters.** The two open at the same instant but not on the same paths.
//! `Boundaries::open` is reached only through `Hand::checkpoint8`, which is
//! `None` after an abort — §6.2 row 8's aborted checkpoint is `S1-R` and is not
//! built — so a window inside the checkpoint existed on the **settled path
//! alone**. §4.10 places the window on both, and gives the abort path's parent
//! by name: `ABORT_TERMINAL(k)`, *"a function of `GENESIS(k)` alone"*, which
//! needs no stage and no agreement about the middle of the hand. **And the
//! abort path is the one a seat goes quiet on**, so the half that was missing
//! was the half the window's whole population lives in. It has its own store
//! now, opened from [`Hand::terminal`], which answers §3.1's question on both
//! paths.
//!
//! **It closes at T47 and the checkpoint does not**, and the two dates are not
//! interchangeable. §4.10: the window *"closes at this receiver's acceptance of a
//! complete `HAND_INIT(k+1)`"*. §4.9: the checkpoint-8 `STATE_ACK` stage *"is not
//! closed by that window"* (`N6`) and the `Boundary` is retained past it. So
//! [`Boundaries::cross_boundary`] shuts every window it holds and prunes
//! neither.
//!
//! **What the window does, and the short list is the point.** A `PLAYER_SIT_IN`
//! makes its sender an **accepted** emitter of the next `HAND_INIT` this
//! receiver runs — §4.9's readmission set `A`, written by the caller because `A`
//! outlives this store — *"and does nothing else at all"*. A `PLAYER_SIT_OUT`
//! and a `PLAYER_LEAVE` are recorded and read by nobody yet: a leave *"removes
//! no seat from `roster_hash` and counts into no `P`"* (§3.1, §3.2), and the
//! seat's chips leave through `HAND_INIT`'s `n(11) ledger_delta` inside a
//! collective body, which is still not built: `S1-BM`'s return certificate
//! (D-028) moves the roster and not the ledger.

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
    /// This peer's own `STATE_HASH` **event**, kept whole.
    ///
    /// §6.3 step 2's dispute carries it as evidence — the complete signed bytes,
    /// not the value — because a receiver whose own copies all agreed must be
    /// able to **verify** a second value at that checkpoint rather than take
    /// this peer's word for it. Without the bytes there is nothing to carry and
    /// the dispute degenerates into an accusation.
    own_event: Option<Vec<u8>>,
    hash_stage: Collective,
    ack_stage: Collective,
    /// `W`, section 4.9's **contradiction set**: the seats whose checkpoint
    /// value differed from this peer's own here.
    ///
    /// Per-receiver by construction, and §4.9 says so — `R(c) ∪ W` is *"the one
    /// required emitter set in this document with a per-receiver component"*.
    /// Two peers on opposite sides of a fork therefore open the same round with
    /// different required sets, which is deliberate: each must hear from the
    /// seat that contradicted **it**.
    contradicted: Vec<SeatIdx>,
    /// The reconciliation rounds of §6.3 step 3, by round number.
    rounds: BTreeMap<u16, Round>,
    /// Whether this peer has published its own `STATE_ACK` for this checkpoint.
    ///
    /// One copy per seat per stage, and the ack is emitted on a **condition**
    /// that stays true once it is reached — `hash_stage.complete()` and
    /// unanimous — so without this the second `STATE_HASH` to arrive after
    /// completion would produce a second ack and read as equivocation to
    /// everybody else.
    ack_sent: bool,
}


/// §4.10's **hand boundary window** for one hand, and nothing else.
///
/// Its own type and its own store, because it opens on **both** terminal paths
/// while §4.9's checkpoint is built on one — see the module doc. It holds no
/// stage: §4.10 says the window *"has no `stage_hash`"*, nothing chains from it,
/// and it is *"a fan and not a chain"*.
#[derive(Debug, Clone)]
pub struct Window {
    hand_id: u64,
    /// `TERMINAL(k)`, the one parent every event of this window carries.
    terminal: Hash,
    /// `P(k)` at `TERMINAL(k)`. Only `PLAYER_SIT_IN` reads it, to refuse a copy
    /// from a seat that is already inside.
    participants: Vec<SeatIdx>,
    /// Every **occupied** seat of the table — §4.10's *"any occupied seat"*.
    roster: Vec<SeatIdx>,
    /// What each seat said here.
    ///
    /// **One entry per seat and the type is what it says.** §4.10: *"A seat may
    /// emit at most one boundary event for hand `k`; a second, of any type, is a
    /// stage violation under §4.0 step 12."* Keyed by seat rather than by
    /// `(seat, type)` for exactly that reason — a map keyed by both would admit
    /// a `PLAYER_LEAVE` and a `PLAYER_SIT_IN` from one seat at one boundary,
    /// which is a seat saying it is going and staying.
    ///
    /// A `BTreeMap`, so reading it back is §4.10's total order — *"ascending
    /// seat index, which the `sequence` rule already fixes"* — without a
    /// tie-break anywhere.
    said: BTreeMap<SeatIdx, EventType>,
    /// Whether it still admits events. Closed at T47.
    open: bool,
}

impl Window {
    pub fn hand_id(&self) -> u64 {
        self.hand_id
    }
    /// `TERMINAL(k)`: what every event of this window must chain from.
    pub fn terminal(&self) -> Hash {
        self.terminal
    }
    /// Whether §4.10's hand boundary window still admits events here.
    pub fn is_open(&self) -> bool {
        self.open
    }
    /// What each seat said at this boundary, in §4.10's total order.
    pub fn said(&self) -> Vec<(SeatIdx, EventType)> {
        self.said.iter().map(|(s, k)| (*s, *k)).collect()
    }
    /// One event of §4.10's hand boundary window, already opened and verified
    /// by the caller against this boundary's `terminal` and the seat's own slot.
    ///
    /// **What this decides and what it does not.** It decides the two rules that
    /// are this store's — the window is open, and one event per seat per
    /// boundary — and §4.10's one type-specific legality condition, that a
    /// `PLAYER_SIT_IN` from a seat already in `P(k)` *"decides nothing"*. The
    /// envelope rules are the caller's, because they are decided from the
    /// event's own bytes and this type never sees them.
    ///
    /// **Two clauses of §4.10 are deliberately not enforced anywhere, and
    /// saying so is better than a check that looks like one.**
    ///
    /// * *"with a non-zero stack"*. This store holds no stacks, and admitting a
    ///   busted seat costs nothing that can be measured: `A` widens the
    ///   **accepted** set of `HAND_INIT(k+1)` and never the required one, a seat
    ///   in it completes no stage, and `Hand::next_hand` filters `required` and
    ///   `dealt_in` through its own `alive` before either reaches a genesis. The
    ///   clause is a legality nicety here and not a safety property.
    /// * *"and has not been removed under D-014"*. **No such set exists in this
    ///   tree.** `security/validation.rs` adjudicates a tier-1 or tier-2 finding
    ///   and nothing retains the verdict against a seat, so there is nothing to
    ///   consult. Enforcing it needs the removal set first, which is D-014's own
    ///   work and not this row's.
    pub fn take_boundary_event(&mut self, seat: SeatIdx, kind: EventType) -> WindowTook {
        if !self.open {
            return WindowTook::Closed;
        }
        if !self.roster.contains(&seat) {
            return WindowTook::NotASeat;
        }
        // **Before the type test, because a second event is a violation
        // whichever type it is.** A seat that sat out and then asked to sit in
        // at one boundary has said two things about one hand, and §4.10 answers
        // that with a stage violation rather than with the later of the two.
        if self.said.contains_key(&seat) {
            return WindowTook::AlreadySpoke;
        }
        if kind == EventType::PlayerSitIn && self.participants.contains(&seat) {
            return WindowTook::DecidesNothing;
        }
        self.said.insert(seat, kind);
        WindowTook::Took(kind)
    }
}

/// What became of one event of §4.10's hand boundary window.
///
/// **Every refusal is named rather than folded into one.** A window that
/// answered *no* to a stale copy, to a second copy from one seat and to a
/// `PLAYER_SIT_IN` that decides nothing would be a window nobody could debug —
/// and the three have different dispositions on the mesh: a stale copy is
/// ordinary weather, a second copy is a stage violation, and a sit-in from
/// inside `P(k)` is an out-of-stage chained event under §4.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowTook {
    /// Recorded, and the type is carried back because the caller's disposition
    /// differs by type: a `PLAYER_SIT_IN` also writes §4.9's readmission set.
    Took(EventType),
    /// No window for that hand is open here — it has not opened yet, or T47
    /// closed it. §4.10: *"A boundary event for hand `k` arriving after that is
    /// rejected as out of stage; the seat re-emits at the next boundary."*
    Closed,
    /// This seat has already spoken at this boundary.
    AlreadySpoke,
    /// A `PLAYER_SIT_IN` from a seat already in `P(k)`. §4.10: *"A copy from a
    /// seat already in `P(k)` decides nothing and is rejected as an out-of-stage
    /// chained event under §4.0."*
    DecidesNothing,
    /// Not an occupied seat of this table.
    NotASeat,
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

    /// This peer's own re-derived `STATE_HASH` for reconciliation round `r`.
    ///
    /// §6.3 step 3: *"each peer then publishes a fresh `STATE_HASH` for the
    /// disputed checkpoint at a new `sequence`, chained from that checkpoint's
    /// `stage_hash`, in the reconciliation stage §4.9 defines"*.
    ///
    /// **The value is this peer's own, re-derived.** With no transcript exchange
    /// built (`S1-Q`) there is nothing new to derive it from, so it is the same
    /// value — which is honest: a round that carries two values says the
    /// divergence is a fork this peer cannot fill, and that is exactly what a
    /// peer with nothing to reconcile from should be saying.
    pub fn round_hash_event(
        &self,
        round: u16,
        key: &ed25519_dalek::SigningKey,
        now_ms: u64,
    ) -> Option<Vec<u8>> {
        let checkpoint_hash = self.checkpoint_hash()?;
        let body = checkwire::StateHash {
            checkpoint: checkwire::BOUNDARY_CHECKPOINT,
            state_hash: self.own,
            transcript_head: self.terminal,
        };
        let slot = crate::net::chained::Slot {
            table_id: self.table_id,
            hand_id: self.hand_id,
            sequence: checkwire::hash_sequence(round)?,
            previous_event_hash: checkpoint_hash,
        };
        crate::net::chained::seal(
            EventType::StateHash,
            &slot,
            &body,
            key,
            now_ms,
            0,
            STATE_ACK_CAP,
        )
        .ok()
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

/// One reconciliation round of §6.3 step 3.
#[derive(Debug, Clone)]
struct Round {
    stage: Collective,
    /// Every distinct `state_hash` heard in it. §6.3: the divergence is
    /// **resolved** when the stage completes carrying one value and
    /// **unresolved** when it completes carrying two, *"and the engine needs no
    /// other signal"*.
    values: Vec<Hash>,
}

/// Why a reconciliation round did not open.
///
/// **Three causes, and they are not interchangeable.** A single `None` here said
/// *"below the floor"* for a stage that had simply not closed yet, on every node
/// of a measured run — a diagnostic that names the wrong cause is worse than
/// none, because it sends the reader somewhere there is nothing to find.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoRound {
    /// Round 0 is the checkpoint itself, and above 7 the band has no room.
    NotARound,
    /// No such boundary is held.
    NoSuchBoundary,
    /// §6.3 chains the round from *"that checkpoint's `stage_hash`"*, and a
    /// stage that has not completed has none. **Not permanent**: the stage
    /// completes when the last member of `P(k)` is heard, which may be after
    /// the copy that contradicted.
    StageNotClosed,
    /// §4.9's floor: `|R(c) ∪ W| >= 2`. A stage whose purpose is to detect a
    /// fork may not have a set the forked peer can satisfy alone.
    BelowFloor,
}

/// What a reconciliation round did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundTook {
    /// Heard, and the stage has not completed.
    Counted,
    /// The stage completed carrying **one** value: the divergence was a gap this
    /// peer could fill, and §6.3 releases the freeze here and nowhere else.
    ///
    /// Completing it required at least one seat other than this receiver to have
    /// signed the same re-derived value — that is what the floor `|R| >= 2` is
    /// for, and it is what separates *a gap this peer could fill* from *a fork
    /// it cannot*.
    Resolved,
    /// The stage completed carrying **two**: §6.3 case (c), `cause = 4`, the
    /// table is faulted.
    Unresolved,
    /// Not a seat this round will hear, or no such round.
    Uninvited,
    /// The same seat saying the same thing again.
    Again,
    /// The same seat, a different event, in one stage.
    Equivocation { first: Hash, second: Hash },
}

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
    /// A value differing from this peer's own: §6.3 step 1, and the caller
    /// latches the freeze on it, declares with a dispute, and opens a
    /// reconciliation round. `solitary_contradicted` is `N1`'s second half.
    Diverged { solitary_contradicted: bool },
    /// Not a seat this stage will hear, or no such checkpoint.
    Uninvited,
}

/// Every boundary checkpoint this peer still admits events for.
#[derive(Debug)]
pub struct Boundaries {
    store: CheckpointStore,
    open: BTreeMap<u64, Boundary>,
    /// §4.10's windows, by hand. **Separate from `open`**, because a window is
    /// opened on the abort path where no checkpoint is (see the module doc).
    windows: BTreeMap<u64, Window>,
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
            windows: BTreeMap::new(),
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
                own_event: None,
                contradicted: Vec::new(),
                rounds: BTreeMap::new(),
                hash_stage,
                ack_stage,
                ack_sent: false,
            },
        );
        // §4.10's window opens at the same instant, because both open at
        // `TERMINAL(k)` — through the same call here, and through
        // [`Boundaries::open_window`] on the abort path, where this one is
        // never reached.
        self.open_window(hand_id, terminal, participants, roster);
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
    /// Open §4.10's hand boundary window for hand `k`, with no checkpoint.
    ///
    /// **The abort path's entry point, and the reason this store is separate.**
    /// §6.2 row 8's aborted-path checkpoint is `S1-R` and is not built, so
    /// [`Boundaries::open`] is unreachable after an abort — and §4.10 places the
    /// window on **both** terminal paths, with `ABORT_TERMINAL(k)` as the
    /// parent. Idempotent: a second call for a hand already held changes
    /// nothing, so the settled path calling both is safe.
    pub fn open_window(
        &mut self,
        hand_id: u64,
        terminal: Hash,
        participants: &[SeatIdx],
        roster: &[SeatIdx],
    ) {
        if self.windows.contains_key(&hand_id) {
            return;
        }
        self.windows.insert(
            hand_id,
            Window {
                hand_id,
                terminal,
                participants: participants.to_vec(),
                roster: roster.to_vec(),
                said: BTreeMap::new(),
                open: true,
            },
        );
        // Two hands' worth, which is one more than §4.10 can have open at once:
        // a window closes at `HAND_INIT(k+1)` and the next opens at
        // `TERMINAL(k+1)`, so the extra slot is for the one just closed and
        // still worth reading.
        while self.windows.len() > 2 {
            let oldest = *self.windows.keys().next().expect("non-empty");
            self.windows.remove(&oldest);
        }
    }

    /// The window for hand `k`, open or closed, while this peer still holds one.
    pub fn window(&self, hand_id: u64) -> Option<&Window> {
        self.windows.get(&hand_id)
    }

    /// Whether §4.10's window for hand `k` is open here.
    pub fn window_is_open(&self, hand_id: u64) -> bool {
        self.windows.get(&hand_id).is_some_and(|w| w.is_open())
    }

    pub fn cross_boundary(&mut self) {
        self.store.cross_boundary();
        // **§4.10's window closes here and the checkpoint does not.** T47 *is*
        // the acceptance of a complete `HAND_INIT(k+1)`, which is the window's
        // stated close; the `STATE_ACK` stage above it stays open, which is
        // `N6`. Every window held is shut rather than the newest one, because a
        // window for a hand older than the one crossing was already closed by
        // the crossing before it and shutting it twice is free.
        for w in self.windows.values_mut() {
            w.open = false;
        }
        self.prune();
    }

    /// One event of §4.10's hand boundary window, for the boundary it names.
    ///
    /// [`WindowTook::Closed`] for a hand this peer holds no open window for,
    /// which is the same answer for one that has not opened and one T47 has
    /// shut: §4.10 disposes of both identically — *"rejected as out of stage;
    /// the seat re-emits at the next boundary"* — and a receiver that told them
    /// apart would be reading its own clock into a rule about the sender's.
    pub fn on_boundary_event(
        &mut self,
        hand_id: u64,
        seat: SeatIdx,
        kind: EventType,
    ) -> WindowTook {
        match self.windows.get_mut(&hand_id) {
            Some(w) => w.take_boundary_event(seat, kind),
            None => WindowTook::Closed,
        }
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
            // `W`. Recorded here because here is where the contradiction is
            // seen, and §6.3 step 3's stage cannot be opened without it.
            if let Some(b) = self.open.get_mut(&hand_id) {
                if !b.contradicted.contains(&seat) {
                    b.contradicted.push(seat);
                    b.contradicted.sort_unstable();
                }
            }
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

    /// Keep this peer's own `STATE_HASH` event, for §6.3 step 2's evidence.
    pub fn remember_own_event(&mut self, hand_id: u64, bytes: &[u8]) {
        if let Some(b) = self.open.get_mut(&hand_id) {
            b.own_event = Some(bytes.to_vec());
        }
    }

    /// This peer's own `STATE_HASH` event for hand `k`, if it published one.
    pub fn own_event(&self, hand_id: u64) -> Option<&[u8]> {
        self.open.get(&hand_id).and_then(|b| b.own_event.as_deref())
    }

    /// Record a seat into `W` without a `STATE_HASH` of its own having arrived.
    ///
    /// §6.3 step 2: a peer whose own copies all agreed *"did not observe the
    /// divergence and would otherwise take no part in the procedure"*. A dispute
    /// puts a second, independently verified value in front of it, and the
    /// evidence's signer joins `W` exactly as a directly-observed contradiction
    /// would. That is what makes the reconciliation stage's floor of two
    /// reachable at a table where only two seats saw the mismatch.
    pub fn contradiction_from_a_dispute(&mut self, hand_id: u64, seat: SeatIdx) -> bool {
        let Some(b) = self.open.get_mut(&hand_id) else {
            return false;
        };
        if b.contradicted.contains(&seat) {
            return false;
        }
        b.contradicted.push(seat);
        b.contradicted.sort_unstable();
        true
    }

    /// This peer's own end-of-hand value for hand `k`, for comparing against a
    /// value that arrives inside something else.
    ///
    /// §6.3 step 2's dispute carries another peer's `STATE_HASH` as evidence,
    /// and that copy must be **compared without being applied**: it occupies no
    /// slot, enters no `stage_hash` and completes no stage, because *"a
    /// `DISPUTE` is unchained and nothing carried inside one ever becomes a
    /// chained event by being carried"*. So it cannot go through
    /// [`on_state_hash`](Self::on_state_hash), which would enter it, and the
    /// caller needs the value to compare against instead.
    pub fn own_value(&self, hand_id: u64) -> Option<Hash> {
        self.open.get(&hand_id).map(|b| b.own)
    }

    /// The first value seen at this checkpoint that differed from this peer's
    /// own, if any. §6.3's retained evidence is the pair `(own, dissent)`.
    pub fn dissent(&self, hand_id: u64) -> Option<Hash> {
        self.store
            .named(hand_id, checkwire::BOUNDARY_CHECKPOINT)
            .and_then(|(_, c)| c.dissent())
    }

    /// Every distinct value heard in reconciliation round `r`.
    ///
    /// §6.3 reads the outcome off this: one value resolves, two fault. The
    /// report §6.4 requires needs the values themselves and not only the count.
    pub fn round_values(&self, hand_id: u64, round: u16) -> Vec<Hash> {
        self.open
            .get(&hand_id)
            .and_then(|b| b.rounds.get(&round))
            .map(|r| r.values.clone())
            .unwrap_or_default()
    }

    /// Which seats were heard in the checkpoint's own stage, with the event
    /// each was heard saying.
    pub fn heard_at_checkpoint(&self, hand_id: u64) -> Vec<(SeatIdx, Hash)> {
        let Some(b) = self.open.get(&hand_id) else {
            return Vec::new();
        };
        b.participants
            .iter()
            .filter_map(|s| b.hash_stage.heard(*s).map(|h| (*s, h)))
            .collect()
    }

    /// `W`, this peer's contradiction set for hand `k`.
    pub fn contradicted(&self, hand_id: u64) -> Vec<SeatIdx> {
        self.open
            .get(&hand_id)
            .map(|b| b.contradicted.clone())
            .unwrap_or_default()
    }

    /// Open reconciliation round `r` of §6.3 step 3.
    ///
    /// **The required set is `R(c) ∪ W`** — the re-derived checkpoint's own set
    /// together with this receiver's contradiction set — and it is the one
    /// required emitter set in the corpus with a per-receiver component. §4.9
    /// admits it, and the reason is the **floor**: `|R(c) ∪ W| >= 2`. *"A stage
    /// whose purpose is to detect a fork may not have a set the forked peer can
    /// satisfy alone."* Without the floor a solitary peer would complete its own
    /// reconciliation, agree with itself, and release the freeze on no evidence
    /// at all.
    ///
    /// `None` when the floor is not met, when the round is outside `1 ..= 7`, or
    /// when the checkpoint's own stage has not completed — §6.3 chains the round
    /// from *"that checkpoint's `stage_hash`"*, and a stage that has not closed
    /// has none.
    pub fn open_round(
        &mut self,
        hand_id: u64,
        round: u16,
        roster: &[SeatIdx],
    ) -> Result<(), NoRound> {
        if round == 0 {
            return Err(NoRound::NotARound);
        }
        let sequence = checkwire::hash_sequence(round).ok_or(NoRound::NotARound)?;
        let b = self.open.get_mut(&hand_id).ok_or(NoRound::NoSuchBoundary)?;
        b.hash_stage.hash().ok_or(NoRound::StageNotClosed)?;
        let mut required = b.participants.clone();
        for seat in &b.contradicted {
            if !required.contains(seat) {
                required.push(*seat);
            }
        }
        required.sort_unstable();
        if required.len() < 2 {
            return Err(NoRound::BelowFloor);
        }
        let stage = Collective::new(sequence, EventType::StateHash.code(), &required, roster)
            .ok_or(NoRound::NoSuchBoundary)?;
        b.rounds.insert(
            round,
            Round {
                stage,
                values: Vec::new(),
            },
        );
        Ok(())
    }

    /// Whether round `r` of hand `k` has been opened here.
    pub fn has_round(&self, hand_id: u64, round: u16) -> bool {
        self.open
            .get(&hand_id)
            .map(|b| b.rounds.contains_key(&round))
            .unwrap_or(false)
    }

    /// Which seats a reconciliation round is still waiting for.
    pub fn round_waiting_for(&self, hand_id: u64, round: u16) -> Vec<SeatIdx> {
        self.open
            .get(&hand_id)
            .and_then(|b| b.rounds.get(&round))
            .map(|r| r.stage.waiting_for())
            .unwrap_or_default()
    }

    /// One `STATE_HASH` of a reconciliation round.
    pub fn on_round_hash(
        &mut self,
        hand_id: u64,
        round: u16,
        seat: SeatIdx,
        event_hash: Hash,
        state_hash: Hash,
    ) -> RoundTook {
        let Some(b) = self.open.get_mut(&hand_id) else {
            return RoundTook::Uninvited;
        };
        let Some(r) = b.rounds.get_mut(&round) else {
            return RoundTook::Uninvited;
        };
        match r.stage.hear(seat, event_hash) {
            Heard::Uninvited => return RoundTook::Uninvited,
            Heard::Again => return RoundTook::Again,
            Heard::Equivocation { first, second } => {
                return RoundTook::Equivocation { first, second }
            }
            // A round has no bystanders that matter: its accepted set is the
            // roster, so an out-of-set copy is compared like any other and only
            // the required ones complete it.
            Heard::Counted | Heard::Bystander => {}
        }
        if !r.values.contains(&state_hash) {
            r.values.push(state_hash);
        }
        if !r.stage.complete() {
            return RoundTook::Counted;
        }
        // §6.3: one value resolves, two do not, "and the engine needs no other
        // signal". The count is over the values heard in **this** stage and not
        // over the checkpoint's, because the whole point of re-deriving is that
        // a peer may now hold a different value from the one it published there.
        if r.values.len() == 1 {
            RoundTook::Resolved
        } else {
            RoundTook::Unresolved
        }
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

    /// The floor is the point of the round, and a solitary peer cannot pass it.
    ///
    /// §4.9 gives the reconciliation stage the only required emitter set in the
    /// corpus with a per-receiver component, `R(c) ∪ W`, and the only one with a
    /// floor: `|R| >= 2`. Without it a peer whose `P(k)` has narrowed to itself
    /// would complete its own reconciliation, agree with itself, and release the
    /// freeze on no evidence at all — which is exactly the fork the stage exists
    /// to detect.
    #[test]
    fn a_reconciliation_round_cannot_be_satisfied_by_one_seat_alone() {
        let mut b = Boundaries::new();
        b.open(4, TABLE, TERMINAL, STATE, &[0], &[0, 1]).expect("opens");
        b.on_state_hash(4, 0, ev(0), STATE);
        assert!(
            b.get(4).expect("held").hash_stage_complete(),
            "a solitary checkpoint completes alone, which is why it compares nothing"
        );
        assert_eq!(
            b.open_round(4, 1, &[0, 1]),
            Err(NoRound::BelowFloor),
            "P(k) is this seat alone and nobody has contradicted: the floor refuses the round"
        );

        // One contradicting copy, from a seat outside P(k), and the round opens.
        b.on_state_hash(4, 1, ev(1), [1u8; 32]);
        assert_eq!(b.contradicted(4), vec![1], "W is what the contradiction wrote");
        assert_eq!(
            b.open_round(4, 1, &[0, 1]),
            Ok(()),
            "R(c) union W is two seats now, and the floor is met"
        );
        assert_eq!(b.round_waiting_for(4, 1), vec![0, 1]);
    }

    /// One value resolves; two do not. §6.3 says the engine needs no other
    /// signal, and this is that signal.
    #[test]
    fn a_round_resolves_on_one_value_and_faults_on_two() {
        let mut b = Boundaries::new();
        b.open(4, TABLE, TERMINAL, STATE, &[0, 1], &[0, 1]).expect("opens");
        b.on_state_hash(4, 0, ev(0), STATE);
        b.on_state_hash(4, 1, ev(1), [1u8; 32]);
        b.open_round(4, 1, &[0, 1]).expect("the floor is met");

        assert_eq!(
            b.on_round_hash(4, 1, 0, ev(20), STATE),
            RoundTook::Counted,
            "one seat is not the stage"
        );
        assert_eq!(
            b.on_round_hash(4, 1, 1, ev(21), STATE),
            RoundTook::Resolved,
            "the re-derivation agrees, so the divergence was a gap this peer could fill"
        );

        // And the other way, in a second round.
        b.open_round(4, 2, &[0, 1]).expect("the floor is still met");
        assert_eq!(b.on_round_hash(4, 2, 0, ev(30), STATE), RoundTook::Counted);
        assert_eq!(
            b.on_round_hash(4, 2, 1, ev(31), [2u8; 32]),
            RoundTook::Unresolved,
            "two values, and section 6.3's case (c) faults the table"
        );
    }

    /// **A round driven by the real emitter cannot resolve, and that makes
    /// every §6.3 freeze terminal.**
    ///
    /// `a_round_resolves_on_one_value_and_faults_on_two` above proves the
    /// arithmetic of [`Boundaries::on_round_hash`] by hand-feeding values, and
    /// its resolving half passes `STATE` for seat 1 — a value seat 1 never
    /// held, since at the checkpoint it published `[1u8; 32]`. So that test
    /// says nothing about what a peer would actually publish, and its
    /// `Resolved` branch is reachable only because the test chose the number.
    ///
    /// Here both sides emit through [`Boundary::round_hash_event`] and the
    /// value is read back off the **sealed frame**, so the loop closes through
    /// the real emitter and the real wire format. The emitter republishes
    /// `self.own`, because with no transcript exchange built (`S1-Q`) there is
    /// nothing else to derive it from — so the round carries the same two
    /// values the checkpoint did and `RoundTook::Unresolved` is the only
    /// outcome available.
    ///
    /// **What that means beyond this file** (`S1-R`): §6.3's division of labour
    /// is *"checkpoint 8 is the route that heals a table where the two peers
    /// agree; the freeze is the route that stops one where they do not"* — and
    /// the healing route cannot run. A genuine divergence therefore reaches
    /// §6.4 and the table deals no further hand; a contradictor that stays
    /// silent never closes the stage at all. **Every freeze is terminal, so any
    /// NEW freeze trigger is a table-killer**, which is the ground the aborted
    /// path's checkpoint designs died on. The corpus has never exercised it:
    /// zero freeze lines in 108 run directories.
    ///
    /// **To make this fail**: give `round_hash_event` something to re-derive
    /// from — which is what `S1-Q`'s transcript exchange would be — and the
    /// values stop being equal to the published ones. That is the intended
    /// future, and this test is what will notice it arriving.
    #[test]
    fn a_round_driven_by_the_real_emitter_cannot_resolve() {
        use ed25519_dalek::SigningKey;

        const OTHER: Hash = [1u8; 32];
        let key_a = SigningKey::from_bytes(&[10u8; 32]);
        let key_b = SigningKey::from_bytes(&[11u8; 32]);

        // Two peers, each holding its OWN end-of-hand value, each hearing the
        // other's. This is the divergence §6.3 opens a round for.
        let mut a = Boundaries::new();
        let mut b = Boundaries::new();
        a.open(4, TABLE, TERMINAL, STATE, &[0, 1], &[0, 1]).expect("opens");
        b.open(4, TABLE, TERMINAL, OTHER, &[0, 1], &[0, 1]).expect("opens");
        a.on_state_hash(4, 0, ev(0), STATE);
        assert_eq!(
            a.on_state_hash(4, 1, ev(1), OTHER),
            Took::Diverged { solitary_contradicted: false },
            "A hears B and they differ"
        );
        b.on_state_hash(4, 1, ev(1), OTHER);
        assert_eq!(
            b.on_state_hash(4, 0, ev(0), STATE),
            Took::Diverged { solitary_contradicted: false },
            "and B hears A"
        );

        a.open_round(4, 1, &[0, 1]).expect("the floor is met");
        b.open_round(4, 1, &[0, 1]).expect("the floor is met");

        // What each side actually puts on the wire for the round.
        let said = |who: &Boundaries, key: &SigningKey| -> Hash {
            let bytes = who
                .get(4)
                .expect("held")
                .round_hash_event(1, key, 1)
                .expect("the checkpoint stage closed, so there is a round frame");
            let opened = crate::net::chained::open_in_hand(
                &bytes,
                512,
                EventType::StateHash,
                &TABLE,
                4,
            )
            .expect("a well-formed chained STATE_HASH of hand 4");
            assert_eq!(
                opened.envelope.sequence,
                checkwire::hash_sequence(1).expect("round 1 has a sequence"),
                "the round frame sits at the round's own sequence"
            );
            let body: checkwire::StateHash =
                crate::net::chained::payload(&opened, 512).expect("the body decodes");
            body.state_hash
        };

        let a_said = said(&a, &key_a);
        let b_said = said(&b, &key_b);

        // The crux, and it is a property of the emitter rather than of the round.
        assert_eq!(a_said, STATE, "A republishes its own value");
        assert_eq!(b_said, OTHER, "B republishes its own value");
        assert_ne!(
            a_said, b_said,
            "so the round carries the same two values the checkpoint carried"
        );

        // Now drive both rounds with exactly those, and nothing else.
        assert_eq!(
            a.on_round_hash(4, 1, 0, ev(20), a_said),
            RoundTook::Counted,
            "one seat is not the stage"
        );
        assert_eq!(
            a.on_round_hash(4, 1, 1, ev(21), b_said),
            RoundTook::Unresolved,
            "A cannot resolve: the re-derivation is what it already published"
        );
        assert_eq!(b.on_round_hash(4, 1, 1, ev(21), b_said), RoundTook::Counted);
        assert_eq!(
            b.on_round_hash(4, 1, 0, ev(20), a_said),
            RoundTook::Unresolved,
            "and neither can B, so the fault is symmetric and terminal"
        );
    }

    /// The three things §6.4's divergence report reads off a faulted boundary
    /// are all still there at the moment it is written.
    ///
    /// The report is written from inside the `RoundTook::Unresolved` arm, which
    /// is to say **at the instant the table faults and not later**, and every
    /// one of its inputs is a live accessor over state that `prune` will drop.
    /// A refactor that let any of them start answering "nothing retained" would
    /// leave a report that is still produced, still well-formed, and empty
    /// where the evidence should be — which is the failure §6.4 is trying to
    /// prevent, wearing the shape of a success.
    #[test]
    fn a_faulted_boundary_still_holds_what_the_divergence_report_prints() {
        let mut b = Boundaries::new();
        b.open(4, TABLE, TERMINAL, STATE, &[0, 1], &[0, 1]).expect("opens");
        b.on_state_hash(4, 0, ev(0), STATE);
        b.on_state_hash(4, 1, ev(1), [1u8; 32]);
        b.open_round(4, 1, &[0, 1]).expect("the floor is met");
        b.on_round_hash(4, 1, 0, ev(20), STATE);
        assert_eq!(
            b.on_round_hash(4, 1, 1, ev(21), [2u8; 32]),
            RoundTook::Unresolved,
            "two values: this is the moment the report is written"
        );

        assert_eq!(b.own_value(4), Some(STATE), "the peer's own derived value");
        assert_eq!(
            b.dissent(4),
            Some([1u8; 32]),
            "the first value that differed from it"
        );
        assert_eq!(b.contradicted(4), vec![1], "W");
        assert_eq!(
            b.get(4).map(|x| x.participants().to_vec()),
            Some(vec![0, 1]),
            "P(k)"
        );
        assert_eq!(
            b.heard_at_checkpoint(4),
            vec![(0, ev(0)), (1, ev(1))],
            "who was heard, and the event each was heard saying"
        );
        assert_eq!(
            b.round_values(4, 1),
            vec![STATE, [2u8; 32]],
            "both values of the round, which is what makes the fault legible"
        );

        // And a hand with no boundary answers empty rather than panicking: the
        // report is written on a fault, and a fault is not a good moment to
        // discover an unwrap.
        assert!(b.heard_at_checkpoint(9).is_empty());
        assert!(b.round_values(9, 1).is_empty());
        assert!(b.round_values(4, 7).is_empty());
    }

    /// A round sits in the band §4.9 reserves, and nothing outside `1 ..= 7`
    /// opens one.
    ///
    /// Round 0 is the checkpoint itself, not a re-derivation, and a round above
    /// `MAX_RECONCILIATION_ROUNDS` has no sequence — emitting one would be
    /// emitting the stage violation §4.0 step 12 exists to refuse.
    #[test]
    fn rounds_exist_only_where_the_band_has_room_for_them() {
        let mut b = Boundaries::new();
        b.open(4, TABLE, TERMINAL, STATE, &[0, 1], &[0, 1]).expect("opens");
        b.on_state_hash(4, 0, ev(0), STATE);
        b.on_state_hash(4, 1, ev(1), STATE);

        assert_eq!(
            b.open_round(4, 0, &[0, 1]),
            Err(NoRound::NotARound),
            "round 0 is the checkpoint"
        );
        assert_eq!(b.open_round(4, 7, &[0, 1]), Ok(()), "seven is the last");
        assert_eq!(
            b.open_round(4, 8, &[0, 1]),
            Err(NoRound::NotARound),
            "and eight is outside the band"
        );
        assert_eq!(checkwire::hash_sequence(7), Some(8206));
        assert_eq!(checkwire::ack_sequence(7), Some(8207));
    }

    /// A round cannot be opened before the checkpoint it re-derives has closed.
    ///
    /// §6.3 chains it from *"that checkpoint's `stage_hash`"*, and a stage that
    /// has not completed has none — so an implementation that opened one anyway
    /// would be chaining from a value it had invented.
    #[test]
    fn a_round_needs_the_stage_it_re_derives_to_have_closed() {
        let mut b = Boundaries::new();
        b.open(4, TABLE, TERMINAL, STATE, &[0, 1, 2], &[0, 1, 2]).expect("opens");
        b.on_state_hash(4, 0, ev(0), STATE);
        b.on_state_hash(4, 1, ev(1), [1u8; 32]);
        assert!(!b.get(4).expect("held").hash_stage_complete());
        assert_eq!(
            b.open_round(4, 1, &[0, 1, 2]),
            Err(NoRound::StageNotClosed),
            "seat 2 has not spoken, so the checkpoint has no stage_hash to chain from"
        );
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

/// §4.10's hand boundary window, against the rules the box states — `S1-BZ`.
#[cfg(test)]
mod the_boundary_window {
    use super::*;

    const TABLE: Hash = [3u8; 32];
    const TERMINAL: Hash = [7u8; 32];
    const STATE: Hash = [9u8; 32];

    /// `P(k) = [0, 1]`, roster `[0, 1, 2, 3]`. Seats 2 and 3 are the ones the
    /// window exists for: occupied, outside `P(k)`, and with nothing else that
    /// puts them back.
    fn open_one() -> Boundaries {
        let mut b = Boundaries::new();
        b.open(4, TABLE, TERMINAL, STATE, &[0, 1], &[0, 1, 2, 3])
            .expect("P(k) is inside the roster");
        b
    }

    #[test]
    fn the_window_opens_with_the_checkpoint_and_takes_a_sit_in_from_outside_p() {
        let mut b = open_one();
        assert!(b.window_is_open(4));
        assert_eq!(
            b.on_boundary_event(4, 2, EventType::PlayerSitIn),
            WindowTook::Took(EventType::PlayerSitIn)
        );
        assert_eq!(b.window(4).expect("held").said(), vec![(2, EventType::PlayerSitIn)]);
    }

    /// §4.10: *"A copy from a seat already in `P(k)` decides nothing and is
    /// rejected as an out-of-stage chained event under §4.0."*
    #[test]
    fn a_sit_in_from_inside_p_decides_nothing() {
        let mut b = open_one();
        assert_eq!(
            b.on_boundary_event(4, 1, EventType::PlayerSitIn),
            WindowTook::DecidesNothing
        );
        assert!(
            b.window(4).expect("held").said().is_empty(),
            "a refused event must not occupy the seat's slot, or one refusal \
             would cost the seat its whole boundary"
        );
    }

    /// The other two types are not narrowed to seats outside `P(k)`: a seat
    /// that is playing is exactly the seat that sits itself out or leaves.
    #[test]
    fn a_sit_out_and_a_leave_are_legal_from_a_seat_inside_p() {
        let mut b = open_one();
        assert_eq!(
            b.on_boundary_event(4, 0, EventType::PlayerSitOut),
            WindowTook::Took(EventType::PlayerSitOut)
        );
        assert_eq!(
            b.on_boundary_event(4, 1, EventType::PlayerLeave),
            WindowTook::Took(EventType::PlayerLeave)
        );
    }

    /// §4.10: *"A seat may emit at most one boundary event for hand `k`; a
    /// second, **of any type**, is a stage violation."* The second clause is
    /// the one worth a test — a map keyed by `(seat, type)` would pass the
    /// same-type case and fail this.
    #[test]
    fn one_seat_speaks_once_per_boundary_whatever_it_says() {
        let mut b = open_one();
        assert_eq!(
            b.on_boundary_event(4, 2, EventType::PlayerSitOut),
            WindowTook::Took(EventType::PlayerSitOut)
        );
        assert_eq!(
            b.on_boundary_event(4, 2, EventType::PlayerSitIn),
            WindowTook::AlreadySpoke,
            "a seat that sat out and then asked to sit in has said two things \
             about one hand"
        );
        assert_eq!(
            b.on_boundary_event(4, 2, EventType::PlayerSitOut),
            WindowTook::AlreadySpoke
        );
        assert_eq!(b.window(4).expect("held").said(), vec![(2, EventType::PlayerSitOut)]);
    }

    #[test]
    fn a_seat_that_is_not_on_the_roster_is_refused() {
        let mut b = open_one();
        assert_eq!(
            b.on_boundary_event(4, 7, EventType::PlayerSitIn),
            WindowTook::NotASeat
        );
    }

    /// §4.10: the window *"closes at this receiver's acceptance of a complete
    /// `HAND_INIT(k+1)`"*, which is T47.
    #[test]
    fn t47_closes_the_window_and_leaves_the_checkpoint_open() {
        let mut b = open_one();
        b.cross_boundary();
        assert!(!b.window_is_open(4), "the window must close at T47");
        assert!(
            b.window(4).is_some(),
            "and it is still readable: closed is not gone"
        );
        assert!(
            !b.get(4)
                .expect("the checkpoint is retained past T47")
                .ack_stage_complete(),
            "and the STATE_ACK stage must still be open (N6)"
        );
        assert_eq!(
            b.on_boundary_event(4, 2, EventType::PlayerSitIn),
            WindowTook::Closed
        );
    }

    /// A hand with no window at all — it has not opened, or the store has
    /// released it — answers the same way, which is §4.10's disposition for
    /// both.
    #[test]
    fn a_boundary_this_peer_does_not_hold_answers_closed() {
        let mut b = open_one();
        assert_eq!(
            b.on_boundary_event(9, 2, EventType::PlayerSitIn),
            WindowTook::Closed
        );
    }

    /// §4.10's total order: *"ascending seat index, which the `sequence` rule
    /// already fixes"*, and it must not depend on arrival order.
    #[test]
    fn the_window_reads_back_in_seat_order_whatever_the_arrival_order() {
        let mut b = open_one();
        for seat in [3u8, 0, 2, 1] {
            let kind = if seat >= 2 {
                EventType::PlayerSitIn
            } else {
                EventType::PlayerSitOut
            };
            assert!(matches!(
                b.on_boundary_event(4, seat, kind),
                WindowTook::Took(_)
            ));
        }
        let seats: Vec<SeatIdx> =
            b.window(4).expect("held").said().iter().map(|(s, _)| *s).collect();
        assert_eq!(seats, vec![0, 1, 2, 3]);
    }

    /// The window belongs to one boundary. Two boundaries open at once — which
    /// is the ordinary state while hand `k+1` is being opened — must not share
    /// one seat's slot.
    #[test]
    fn two_boundaries_hold_two_windows() {
        let mut b = Boundaries::new();
        b.open(4, TABLE, TERMINAL, STATE, &[0, 1], &[0, 1, 2])
            .expect("opens");
        assert!(matches!(
            b.on_boundary_event(4, 2, EventType::PlayerSitIn),
            WindowTook::Took(_)
        ));
        b.open(5, TABLE, TERMINAL, STATE, &[0, 1], &[0, 1, 2])
            .expect("opens");
        assert!(
            matches!(
                b.on_boundary_event(5, 2, EventType::PlayerSitIn),
                WindowTook::Took(_)
            ),
            "a seat that spoke at hand 4's boundary may speak again at hand 5's"
        );
    }

    /// **The hole this store exists to close.** §4.10 puts the window on both
    /// terminal paths; §6.2 row 8's aborted checkpoint is `S1-R` and is not
    /// built, so `Boundaries::open` is unreachable after an abort. A window
    /// folded into the checkpoint was therefore settled-path only — and the
    /// abort path is the one a seat goes quiet on, which is the entire
    /// population §4.10's window serves.
    ///
    /// The break that must make this fail: delete the `open_window` call on the
    /// abort path in `run.rs`, or make this test open through `open` instead.
    #[test]
    fn a_hand_that_aborted_still_opens_a_window() {
        let mut b = Boundaries::new();
        // No checkpoint anywhere: this is exactly what an aborted hand has.
        b.open_window(4, TERMINAL, &[0, 1], &[0, 1, 2, 3]);
        assert!(b.get(4).is_none(), "an abort builds no checkpoint (S1-R)");
        assert!(b.window_is_open(4), "and it must still build a window");
        assert_eq!(
            b.on_boundary_event(4, 2, EventType::PlayerSitIn),
            WindowTook::Took(EventType::PlayerSitIn),
            "a seat outside P(k) may ask to sit in at an ABORTED boundary, \
             which is the boundary it went quiet at"
        );
        assert_eq!(b.window(4).expect("held").terminal(), TERMINAL);
    }

    /// Opening the same window twice — which the settled path does, once
    /// through `open` and once directly — must not wipe what it holds.
    #[test]
    fn opening_a_window_twice_keeps_what_it_has() {
        let mut b = Boundaries::new();
        b.open_window(4, TERMINAL, &[0, 1], &[0, 1, 2, 3]);
        assert!(matches!(
            b.on_boundary_event(4, 2, EventType::PlayerLeave),
            WindowTook::Took(_)
        ));
        b.open_window(4, [1u8; 32], &[0], &[0, 1]);
        assert_eq!(
            b.window(4).expect("held").said(),
            vec![(2, EventType::PlayerLeave)],
            "the second call is idempotent, or the settled path would erase the \
             window it just filled"
        );
        assert_eq!(b.window(4).expect("held").terminal(), TERMINAL);
    }

    /// Two windows at once is the ordinary state; a third is not, and an
    /// unbounded map is a way to be attacked.
    #[test]
    fn the_window_store_keeps_two_and_no_more() {
        let mut b = Boundaries::new();
        for hand in 1..=4u64 {
            b.open_window(hand, TERMINAL, &[0, 1], &[0, 1]);
        }
        assert!(b.window(1).is_none() && b.window(2).is_none());
        assert!(b.window(3).is_some() && b.window(4).is_some());
    }
}
