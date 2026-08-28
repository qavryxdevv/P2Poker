# PHASE3_GATE4.md — the receiver gate, third attempt

**Commission.** Eleven passes. The tenth (`PHASE3_GATE3.md`) landed four
unblocking edits, produced the first healthy-table numbers that were not a
disaster, and left two blockers — `N1` and `N2`. This pass grades `K1`, `N1`–`N9`
and `L8` against the documents as they now stand, re-runs the K1 interleaving,
widens the healthy-table check to two and six seats, and asks the one question
`src/protocol/` needs answered next: **can `PublicTableState`, `state_hash`,
`slot(E)`, the §13 constants and §4.0 steps 1–11 be written today?**

**Method, unchanged, because it is what has produced every blocker since Phase 2.**
Nothing is graded off a summary row or off `DECISIONS.md`'s disposition column.
Every verdict below was re-derived from the normative text: §3.2's disjunction and
§4.9's floor were read before `DECISIONS.md`'s `N1` row; `solitary_at` was
evaluated by hand against §5.3 step 8's assignment rather than against §2.6's
lemma; the D-014 sweep was run against D-014's own tier-1 list and against
`src/security/validation.rs` before either document's coverage table. That
inversion produced this pass's blockers, and it produced them where the pattern
predicts.

**Files as read, 2026-08-28.** `DECISIONS.md` 19:39, `PROTOCOL.md` 19:38,
`STATE_MACHINE.md` 19:38, `THREAT_MODEL.md` 19:34, `DEPENDENCIES.md` 19:33,
`CONTRIBUTING.md` 19:32, `CRYPTOGRAPHY.md` 19:31, `NETWORK_STACK.md` **17:52**,
`SPEC_CS.md` 09:29. `src/security/validation.rs` read in full at 303 lines;
`src/protocol/` at `8fe740b`.

> **The timestamp finding, seventh iteration, and this time it names the gap
> before the gap is opened.** Eight documents were edited in this pass.
> `NETWORK_STACK.md` was not, and it is the one document that still carries
> **zero** D-014 and a normative prohibition list whose word *unseat* now
> contradicts D-014 tier 1 — the identical defect `N3` just fixed in
> `PROTOCOL.md` §4.0, in the one file the pass did not open. The rule holds: **the
> worst defect is on the path the previous fix newly made load-bearing**, and the
> cheapest place to find one is the document a corpus-wide fix did not touch.

---

## Verdict counts

| Verdict | Count | Items |
|---|---:|---|
| **RESOLVED** | 6 | N2, N3, N4, N7, N9 *(`CRYPTOGRAPHY.md` half)*, L8 |
| **PARTIAL** | 5 | K1, N1, N5, N6, N8 |
| **UNRESOLVED** | 0 | — |
| **REGRESSED** | 0 | — |

**Every item moved, none went backwards, and the wire half of every one of them
landed.** This is the best-executed pass in the series: `PROTOCOL.md` §3.2's
disjunction, §4.9's reconciliation floor `R(c) ∪ W` with `|R| >= 2`, §4.0 step
10a's skip, §4.9's readmission set `A`, the checkpoint-8 `STATE_ACK` retention,
§4.0's D-014 exception, and `CRYPTOGRAPHY.md`'s new §8.1 are all present,
normative, and argued rather than asserted.

**Five items are PARTIAL for one reason, and it is one shape.** In each, the wire
half is complete and the **engine half cannot consume it**:

* **N1 / K1** — `STATE_MACHINE.md` §2.6's floor is `j <= k`; the hand the fix
  exists for has `j = k + 1`. `T62`'s guard rejects the exact event §4.0 step 10b
  freezes on. Recorded as **`N-1e`, blocking**, and the repair is one character.
* **N6** — §4.9 now requires a checkpoint-8 `STATE_ACK` of hand `k` to be accepted
  until `TERMINAL(k+1)`. `TableState.checkpoint` is `Option<CheckpointState>`,
  *"at most one is open at a time"*. There is nowhere to put it, so D-014's
  tier-2 precondition is exactly as unsatisfiable as N6 said it was.
* **N5** — the readmission set `A` is defined on the wire and has no engine half
  (`N-5e`, open), and its interaction with N2's skip is a replay-driven liveness
  attack (**P2**).
* **N8** — the disposition is recorded in two places; the sentence in `D-014`
  itself is unchanged.

**Eight new defects, P1–P8. Two block the receiver.**

* **P1 — checkpoint state is single-slot and does not outlive its hand**, while
  §4.9 now requires the boundary checkpoint of hand `k` to accept events after
  hand `k+1` has started. It disables N6's own fix, T50's new N1 conjunct on the
  stale route, and the wire's *"enters §6.3 at step 1"* for every non-solitary
  receiver.
* **P2 — a replayed, agreeing checkpoint-8 `STATE_HASH` re-enlarges the next hand
  init's required emitter set once per hand, for ever.** N2's skip removed
  duplicate suppression on the argument that step 10b's four dispositions are
  idempotent; N5's disposition writes to a set that is **cleared at every hand
  init**, so it is idempotent within a hand and not across hands.
* **P4 — D-014 tier 1 contains one clause that is not self-contained.** *"A
  message whose chain parent does not exist"* is decidable only against the
  receiver's own store — the quantity §6.3 step 3 exists to reconcile and §4.x's
  ordering buffer exists to tolerate. `CRYPTOGRAPHY.md` §8.1 states the principle
  it violates, in this pass, in the section written for D-014.

**Readiness verdict: READY for the message layer and for four of the five newly
unblocked items; NOT-READY for the receiver's freeze path.** Part 8 gives the
per-item answer.

---

# Part 1 — Per-item verdicts

## K1 — `P(k)` per-receiver on the path where it narrows — **PARTIAL**

**What is fixed, and it is more than was asked for.** §3.2's regime test is now
the **disjunction** `P(k-1) == {self} ∨ P(k) == {self}`, written out with the
reason neither disjunct may be dropped and with the observation that decides it:

> `PROTOCOL.md` §3.2: *"K1's trace lands on the second one first: the hand `P`
> narrows in is a hand whose stage 0 was required of **everybody** … Its `P(k-1)`
> has three seats and its `P(k)` has one."*

That sentence is correct, it is the sharpest thing in the pass, and it is the
sentence the engine half does not implement.

**Where it is lost.** `STATE_MACHINE.md` §2.6:

> `solitary_at(k)` := `solitary_since == Some(j) ∧ j <= k <= hand_id`

and §5.3 step 8 (l. 2451): *"if `|signed_this_hand| == 1` then `solitary_since :=
solitary_since.or(Some(hand_id))`"*, where `signed_this_hand` at hand `k`'s init
**is** `P(k-1)`. On K1's own walk `P(k-1) = {A,B,C}`, so nothing is written at
hand `k`'s init; `P(k) = {A}`, so `solitary_since := Some(k+1)` at hand `k+1`'s
init. Evaluating `solitary_at(k)` gives `k + 1 <= k` — **false**.

§2.6's lemma asserts the opposite:

> *"If `PROTOCOL.md`'s retained record says hand `k` was solitary at this
> receiver, then `solitary_at(k)` holds. Proof. The record says so exactly when
> this peer's §5.3 step 4 read a one-member `signed_this_hand` at hand `k`'s
> init."*

The premise of that proof is the **first disjunct only**. §5.3's `RetainedHand`
is explicit that the bool is both: *"`was_solitary` is **not** `p == {self}`…
§3.2's regime test is the disjunction"*. So the lemma is false for exactly the
class of hand the disjunction was added to catch, and I33(a) asserts the false
lemma as an invariant a harness is told to check.

`DECISIONS.md` **`N-1e`** records this, marks it **blocking for K-9**, and gives
the exact repair — `solitary_at(k) := solitary_since == Some(j) ∧ j - 1 <= k <=
hand_id`, one hand of slack and no more, with the proof that a hand satisfying the
second disjunct has `P(k) == {self}` and therefore forces `j <= k + 1`. **The
repair is right and it has not been applied.** Grading K1 PARTIAL rather than
REGRESSED because the second freeze route (Part 2) does fire and does hold; what
is lost is the earliest, best-evidenced detection and the retraction of an
already-computed tournament win.

## N1 — a solitary peer releases its own freeze — **PARTIAL**

**The wire half is RESOLVED and it is the best single edit of the pass.** §4.9
gives the reconciliation round its own required emitter set, `R(c) ∪ W`, with `W`
the receiver's contradiction set, and states the floor as a general rule rather
than a special case:

> `PROTOCOL.md` §4.9: *"`W` is never empty, so `|R(c) ∪ W| >= 2`, and no peer
> completes a reconciliation stage alone… **a stage whose purpose is to detect a
> fork may not have a required emitter set that the forked peer can satisfy
> alone.**"*

Four things make it hold rather than merely say so. (i) `W` is proved non-empty
from each of §6.3 step 1's entry conditions. (ii) The D-012 question is asked and
answered in the right terms — `W` derives no canonical state, only *enlarges* a
required set, so it can delay a completion and never manufacture one — and §2.9
generalises it into a sweep rule (*"a per-receiver quantity that can only enlarge
a required emitter set is admissible; one that can shrink or replace one is
not"*), which is worth more than the fix. (iii) The healthy table is shown to pay
nothing: where `P` has not forked, `W ⊆ R(c)` and the union is the old set. (iv)
§6.3 step 3 states the release condition **exhaustively** — *"Nothing else
releases it. Not a timer; not the abort of the hand it froze in; not
`TERMINAL(k)`; … not the table closing"* — which is what makes the freeze
auditable.

**The engine half landed three of its four parts.** T53 gains *"the completed
stage carries values signed by at least two distinct seats"*, written on the
**signers** rather than on the set, deliberately, so it does not move when
`PROTOCOL.md` changes the set. T50 gains `solitary_contradicted := true` when
`|checkpoint.required| == 1`. I33(b) gains the exemption that makes T53
satisfiable at all — the frozen peer may publish its `DISPUTE`, its
reconciliation `STATE_HASH` and its `STATE_ACK` and nothing else — which is the
pass checking its own load-bearing rule and finding it broken **in the same
edit**. I33(c) is re-scoped off the latch and onto the freeze:

> *"between any two entries into `Diverged` with a hand dealt between them there
> is a completed reconciliation stage carrying values signed by at least two
> distinct seats"*

— falsifiable against both triggers and both exits, where the old form was
falsifiable against neither. That is the single best invariant edit in eleven
passes.

**The fourth part is the floor, and it is off by one — `N-1e`.** See K1 above.

**`N1-a` is closed in effect and unstated.** `DECISIONS.md` asks whether §4.9
admits an out-of-set copy into a reconciliation round. It does, by construction:
`R(c) ∪ W` makes the contradicting seat a **required** emitter of that stage, and
a required emitter's copy is accepted by definition. One sentence in §4.9 saying
so closes `N1-a` at zero cost; leaving it open risks an editor "fixing" it the
other way.

## N2 — §4.0 step 10a undefined for a stale-hand event — **RESOLVED**

Closed on both ends, in the two sections that own the two ends, and closed in the
direction that removes the allocation rather than bounding it.

* §4.0 row 10a, first sentence: *"For a chained event whose `hand_id` names a hand
  this receiver has completed, this step is **skipped** and step 10b is the whole
  of its anti-replay rule… **no `hand_id` a sender chooses causes an allocation of
  any kind, here or anywhere in the pipeline.**"*
* §5.3 states the same rule from the memory end and names the rejected reading.
* The **one exception is carved out and bounded**: the checkpoint-8 `STATE_ACK`
  band, whose slots §5.3 retains until `TERMINAL(k+1)`, at `8 × MAX_SEATS = 80`
  entries for one hand at a time.
* What the skip gives up is stated rather than glossed: duplicate suppression and
  §5.2's equivocation slot, with the argument that neither is needed.

**The argument for the first of those two is now false, and that is P2, not N2.**
The claim is *"each of its four dispositions is idempotent — … a seat already in
the readmission set is already in it"*. `A` is cleared at every hand init (§4.9),
so a replay after the clear is not a no-op. N2's own disposition is correct; what
it newly made load-bearing is not.

## N3 — §4.0's anti-eviction box contradicts D-014 — **RESOLVED**

The box keeps its full force and names the one exception by reference, exactly as
asked, and the exception paragraph is better than the item requested: it lists
tier 1's clauses, states that tier 2 *"removes nobody until this receiver holds
the completed `STATE_ACK` stage §4.9 requires"*, and closes with the line that
makes it checkable — *"the exception ends in a seat, never in a chip, and D-010
points 1 and 2 stand unamended"*. The trailing paragraph names the code as a
consumer, which is the right dependency direction.

**Two residues, filed separately.** The tier-1 list it now carries contains
`a parent that does not exist` (**P4**), and the same prohibition lives
unamended in `NETWORK_STACK.md` §1.2 prohibition 7 (**P5**).

## N4 — two memories for one past-tense test — **RESOLVED**

`RetainedHand.was_solitary` is declared the **sole authority**; `solitary_since`
becomes monotone, *"never cleared, at any hand init, for any reason"*, and is
demoted to a floor. The false contiguity argument is retracted by name and
`DECISIONS.md` keeps a `K-9-b` row so the retracted argument cannot be
re-derived from the row that carried it. The residual `N4-a` (the record's `p`
documented as `P(hand_id)`) is closed by §5.3's new comment `P(hand_id - 1)` and
the paragraph under it.

This is the pattern this class needed and it is worth stating as the reusable
form: **the wire's record is exact, the engine's field is a floor beneath it, and
the ordering between them is asserted as an invariant** — not two answers, and not
one answer duplicated. The only defect left in it is that the ordering asserted is
not the one that holds (`N-1e`).

## N5 — K-3b's readmission route does not survive step 10b — **PARTIAL**

The wire half is complete and well argued. §4.9 defines the readmission set `A`,
§3.2 lists it as the third of three enlargements of `R`, §4.0 step 10b writes it,
§5.3 sizes it (`|A| <= MAX_SEATS`, one seat set, read once, cleared), and §4.9
states why D-013's promise was void without it — *"every window at a solitary peer
is zero-width"* — and why the enlargement is safe. §4.9 also states, correctly and
in bold, that **`A` does not thaw a freeze**.

**Two things are missing.**

1. **`N-5e` — the engine half does not exist.** `STATE_MACHINE.md` contains no
   readmission set: §5.3 step 4 reads `signed_this_hand` and nothing widens it,
   so the engine's `HAND_INIT(m+1)` required set is `P(m)` where the wire's is
   `P(m) ∪ A`. Two required emitter sets for one stage in one peer. `DECISIONS.md`
   records it as *"two lines"*; it is two lines, and until they land the wire rule
   has no implementation.
2. **P2** — see Part 7. The mechanism is a replay amplifier.

## N6 — the checkpoint-8 `STATE_ACK` stage — **PARTIAL**

The wire half is exactly right, and it is right for a better reason than the item
gave: §4.9 does not merely state the emission order, it observes that **forwarding
(§1.5) can reorder the two whatever order they were written in**, so the failure
is ordinary and non-adversarial. The rule is the clean one — the window closes over
`STATE_HASH` copies only, and *"a checkpoint-8 `STATE_ACK` of chain `k` from a seat
in `P(k)` is accepted until `TERMINAL(k+1)` is fixed at this receiver"* — with the
cost argued (an out-of-set ACK is still rejected, so an accepted late one grows no
set and changes no body) and the storage bounded in §5.3.

**And there is nowhere for it to go.** `STATE_MACHINE.md` §2.3:

> `pub checkpoint: Option<CheckpointState>,   // the checkpoint currently open, if any`
> `pub struct CheckpointState { … }   // PROTOCOL.md §6.2; at most one is open at a time`

T51's guard reads *"every required `StateHash` for this checkpoint is present and
agrees"* — the open one. Once hand `k+1` opens checkpoint 2, hand `k`'s
checkpoint-8 `CheckpointState` is gone, `agreed` was never set, and the late ACK
completes nothing. **D-014's tier-2 precondition — *"a completed `STATE_ACK`
stage"* — is therefore still unsatisfiable at the boundary, which is the exact
consequence N6 was filed to remove.** That is **P1**, and N6 is PARTIAL because
its wire half is a precondition for the fix rather than the fix.

## N7 — step 10b tests the wrong set — **RESOLVED**

`RetainedHand.p` is `P(hand_id - 1)` at the field, in the struct comment, in a
paragraph under it, in §3.2 and in §4.0 step 10b's own text, with the reason
(*"`P(k-1)` is what hand `k`'s stages were **required of**"*) and the failure it
avoids (*"testing against `P(k)` would silence the rule for exactly the seats
those exemptions admitted"*). One set, one name, one test site — D-011 rule 2
applied to a record.

## N8 — D-014 cites a `SPEC_CS.md` §22 window that does not exist — **PARTIAL**

The **disposition** is correct and is recorded twice: `DECISIONS.md`'s open list
grades it *"DONE as a record, not as an edit, because the target is the owner's
document"*, and `STATE_MACHINE.md` T64 now says the window is *"a **required
addition** to `SPEC_CS.md` §22, not an element §22 contains"*. That is the right
handling of a defect in the owner's binding document.

**The sentence itself is unchanged.** `DECISIONS.md` l. 1371 still reads *"`SPEC_CS.md`
§22's information window"*, and `DECISIONS.md` outranks both documents that
correct it. The next editor reads the decision first. One clause — *"§22 carries
no such window today; this is a required addition to it, specified by
`STATE_MACHINE.md` T64's `Fault` record"* — discharges it.

## N9 — `CRYPTOGRAPHY.md` carries zero D-014 — **RESOLVED for `CRYPTOGRAPHY.md`; the other half is P5**

`CRYPTOGRAPHY.md` went from 0 to **23** occurrences and the new §8.1 is the best
new section in this pass. It states, per proof, *what a failure proves* and *what
it does not*, and it states the general condition in the one form that makes tier
1 sound:

> *"The public inputs must be the **agreed** ones… A verifier that substitutes a
> locally reconstructed input for a chained one has stopped computing the shared
> function, and its *invalid* verdict then proves nothing about the signer — it
> proves the two peers disagree about the input, which is a divergence… and never
> a removal."*

That is the correct general rule, it is normative, and **P4 is a clause of D-014's
own tier-1 list that violates it**.

`NETWORK_STACK.md` is still at **zero** D-014 and was not opened. Gate 3 graded
its zero *"defensible, unrecorded"*. That grading was wrong and this pass
withdraws it: see **P5**.

## L8 — sweep records that stop at D-012 — **RESOLVED**

Both documents carry a second dated record. `CONTRIBUTING.md` l. 15: *"Swept again
against D-013 and D-014 on 2026-08-28, and this second record exists because the
first one stopped at D-012 while two further decisions landed"*, reporting the
counts before (zero, zero) and naming what changed. `DEPENDENCIES.md` l. 1057
carries the same in the same form. Counts confirm it: `CONTRIBUTING.md` D-013 = 9,
D-014 = 12; `DEPENDENCIES.md` D-013 = 5, D-014 = 14.

**And the record does the thing this series has been asking for since Phase 1**: it
reports the miss as a numbered failure of the file's own §2.5 rule and states the
lesson in a form a future reader can act on — *"a deferred documentation defect
does not decay gracefully — it is read as current by everyone who arrives after
it"*. Three passes late, and the delay is written down as the finding.

---

# Part 2 — The K1 interleaving, run again

Three seats, `A` (0), `B` (1), `C` (2). Hand `k-1` completes normally,
`P(k-1) = {A,B,C}`. Adversary budget: dropped frames only. Walked at `A`.

### 2.1 The common prefix (steps 1–7 unchanged from Gate 3, re-verified)

1. `C` emits `PLAYER_LEAVE` at `BOUNDARY_SEQUENCE_BASE + 2`, parent
   `TERMINAL(k-1)`. **`A` receives it; `B` does not.**
2. Checkpoint 8 of hand `k-1` **agrees at both** — T45 opens it *"the moment
   `TERMINAL(k-1)` is fixed"*, before the boundary window runs, so no
   boundary-window disagreement is visible to it. Both enter hand `k`.
3. Hand `k` stalls at stage 0: `R(HAND_INIT, k) = P(k-1) = {A,B,C}`, `A`'s body
   reflects `C`'s departure and `B`'s does not, each rejects the other on body
   mismatch, `C` emits nothing.
4. T57 at `hand_deadline_ms` → `HandAborted` → **T46**, which restores stacks and
   opens checkpoint 8 for hand `k` with `required = P(k)`.
5. **`P(k) = {A}` at `A` and `{B}` at `B`.** `PLAYER_LEAVE` counts into no `P`
   (§3.2 exclusion 2); the terminal `HAND_ABORT` counts into none (exclusion 1).
6. The checkpoint-8 bodies for hand `k` differ in `signed_this_hand`:
   `[true,false,false]` at `A`, `[false,true,false]` at `B`. K-8-independent.
7. `A`'s checkpoint-8 window closes at once — T47's gate does not apply on the
   T46 path, `HAND_INIT(k+1)` is required of `P(k) = {A}` and self-completes.
   §5.3 step 8 writes **`solitary_since := Some(k+1)`**. `RetainedHand(k)` is
   written with **`was_solitary = true`** (second disjunct) and `p = P(k-1) = {A,B,C}`.

**Note the asymmetry the fix created and did not reconcile: the wire's record for
hand `k` says solitary; the engine's floor starts at `k+1`.**

### 2.2 Variant 1 — `B`'s `HAND_INIT(k+1)` copy arrives

**8. `B`'s checkpoint-8 `STATE_HASH` for hand `k` arrives.** Stale hand → step 10a
skipped (N2) → **step 10b**, checkpoint-8 branch → compared against
`checkpoint8_state_hash(k)` → **mismatch** → *"enters §6.3 at step 1 and, where
the record says the hand was solitary, routes to step 12a with it"*. The record
says solitary. Step 12a emits `SolitaryDivergence { hand_id: k }`.

**9. The engine drops it.** `T62`'s guard is `solitary_at(k)` =
`solitary_since == Some(k+1) ∧ k+1 <= k` → **false**. `T63` has the same guard.
No other row consumes `SolitaryDivergence` (§2.6: *"read by T62 and T63 … and by
nothing else"*). **No freeze, no latch, no `Fault` record.** T50 cannot substitute:
it triggers on `Event::StateHash`, step 10b says the event *"never reaches step
13"*, and the checkpoint it would read is closed and its `CheckpointState`
replaced (**P1**).

**10. `B`'s `HAND_INIT(k+1)` copy arrives, and this one is caught.** It names
hand `k+1`, which `A` finished in microseconds. Step 10b: `RetainedHand(k+1)` has
`p = P(k) = {A}`, `B ∉ p`, `was_solitary = true`, and `HAND_INIT` is neither
exempt type → step 12a → `SolitaryDivergence { hand_id: k+1 }`. `solitary_at(k+1)`
= `Some(k+1) ∧ k+1 <= k+1 <= hand_id` → **true**. **T62 fires.** `Diverged`,
`Fault{SolitaryDivergence}`, `solitary_contradicted := true`, the offending event
not applied and not counted into `signed_this_hand`.

**11. The freeze holds.** `A` broadcasts `DISPUTE { kind = 1 }` (§6.3 step 2,
mandatory), publishes its reconciliation `STATE_HASH` for checkpoint 8 of hand
`k+1` at `BOUNDARY_CHECKPOINT_BASE + 2` — permitted by I33(b)'s new exemption, and
**only** that traffic is permitted. The stage is required of
`R(c) ∪ W = {A} ∪ {B} = {A,B}`, `|R| = 2`. `A` cannot complete it alone; T53's
two-signer conjunct independently refuses a one-signer completion. Then either

* `B` publishes its own re-derived value → one value (resume, latch cleared) or
  two values remaining → T54 / T60 → `HandAborted` → T46; or
* `B` never speaks → `A` stays frozen, T57 fires at `hand_deadline_ms`, stacks
  restore, **the latch is still set**, and §9.3 condition 0.6 closes the table at
  the next boundary. No hand is dealt, no end condition names a winner.

**Verdict for variant 1: the freeze holds.** It holds one hand later than it
should, and the first, best-evidenced detection — the checkpoint-8 comparison on
the fork hand, which is the route §3.2's second disjunct and §4.9's `N1` second
half were both written for — is silently discarded by the engine.

### 2.3 Variant 2 — `B`'s `HAND_INIT(k+1)` copy is dropped

Step 8 and step 9 run as above: `SolitaryDivergence { hand_id: k }` is produced by
the wire and **rejected by the engine's floor**. Step 10 does not happen.

* **No freeze at all.** Not T62 (guard false). Not T63. Not T50 — the checkpoint
  is closed, `CheckpointState` has been replaced by hand `k+1`'s, and the event
  never becomes an `Event::StateHash` (**P1**).
* `A` continues draining. `B`, stalled in its own hand `k`, aborts, deals its own
  solitary hand `k+1`, places its checkpoint 8 for it, and emits.
* **That copy is caught.** It names hand `k+1`, `RetainedHand(k+1).was_solitary`
  is true, `solitary_at(k+1)` is true, step 10b's checkpoint-8 branch routes to
  step 12a, **T62 fires**, latch set, §9.3 condition 0.6 closes the table.

**So variant 2 also holds — provided at least one contradicting event naming a
hand `k' >= k+1` arrives.** It is lost on exactly one interleaving, and it is a
legal one under K1's own adversary budget:

> **Where it is lost, exactly.** If the **only** contradicting event that ever
> reaches `A` is `B`'s checkpoint-8 `STATE_HASH` **of hand `k` itself** — the fork
> hand — then `A` freezes on nothing, drains to §9.3 condition 1, and enters
> `TableClosed` naming itself. And if that copy arrives *after* `A` has closed,
> **T63 does not fire either** (same guard), so
> `settlement.tournament_winner` is **not retracted**. K1's payoff — a private
> tournament win held against a seat that was signing — survives, on the one hand
> for which `PROTOCOL.md` §3.2 says the evidence is strongest.
>
> **The single line:** `STATE_MACHINE.md` §2.6,
> `solitary_at(k) := solitary_since == Some(j) ∧ j <= k <= hand_id`,
> evaluated at T62 with `e.hand_id = k` and `solitary_since == Some(k+1)`.
> `DECISIONS.md` `N-1e` gives the repair: `j - 1 <= k`.

**One further observation the walk produced, and it belongs to `PROTOCOL.md`.**
On the non-solitary version of step 8 — a stale checkpoint-8 mismatch at a peer
that was *not* alone — §4.9 and step 10b say the receiver *"enters §6.3 at step
1"*, and **no engine row consumes that either**. `SolitaryDivergence` is the only
carrier the wire has for a step-10b finding, and it is guarded on the regime. That
is **P1**'s second face and it is reachable on a healthy table.

---

# Part 3 — The load-bearing check

Eleven fixes, the rule each newly makes load-bearing, and whether it holds. Six
consecutive passes have found their worst defect here; this is the sixth.

| # | Fix | Rule it newly makes load-bearing | Holds? |
|---|---|---|---|
| 1 | **§4.9's floor `R(c) ∪ W`, `\|R\| >= 2`** | **§6.3 step 2's `DISPUTE` is now a liveness precondition**, not only a disclosure: a peer whose own copies agreed never observed the divergence and must be brought into the stage by the dispute | **YES.** §4.9 says so explicitly, §6.3 step 2 already made the dispute mandatory, and §4.9 adds the sentence that a withholder cannot be waited out |
| 2 | **§4.9's `W` as a per-receiver component of an `R`** | **D-012's rule** that no canonical state derives from a per-receiver quantity | **YES**, and better than required: §2.9 generalises it into a sweep rule (only-enlarge is admissible, shrink-or-replace is not) and applies the same test to `A` |
| 3 | **T53's two-signer conjunct** | **I33(b)'s complete-freeze clause**, which forbade the frozen peer every `Effect::Publish` — including the one thing T53's exit consumes | **YES**, and it was caught *in this pass*, by this method, and fixed in the same edit. The exemption is exactly three message types and the argument for it is that a repair completes a stage *about* the hand, not *of* it |
| 4 | **T50's latch conjunct `\|checkpoint.required\| == 1`** | **that `checkpoint.required` is readable at the moment the contradiction lands** | **NO — P1.** `TableState.checkpoint` is `Option<CheckpointState>`, one open at a time. On the stale route the checkpoint is gone, so the conjunct is unreadable and T50 is unreachable |
| 5 | **§3.2's disjunction `P(k-1) == {self} ∨ P(k) == {self}`** | **§2.6's lemma**, which orders the engine's floor beneath the wire's record | **NO — `N-1e`.** The lemma's proof assumes the first disjunct; the fix exists for the second. Off by one, on K1's own hand |
| 6 | **§4.0 step 10a's skip (N2)** | **that all four of step 10b's dispositions are idempotent**, which is the entire justification for dropping duplicate suppression | **NO — P2.** The readmission disposition writes to a set §4.9 **clears at every hand init**; a replay after the clear is not a no-op |
| 7 | **§4.9's readmission set `A` enlarging `R(HAND_INIT(m+1))`** | **that a seat placed in `A` will sign `HAND_INIT(m+1)`** | **NO — P2's payoff.** It need not: `A`'s member emitted one stale message, possibly a replay of one. Stage 0 then stalls at this receiver alone, to `hand_deadline_ms` |
| 8 | **§4.9's `STATE_ACK` retention to `TERMINAL(k+1)` (N6)** | **that the engine can complete a checkpoint stage of a finished hand** | **NO — P1.** Single-slot `CheckpointState`; T51 reads the open checkpoint. D-014's tier-2 precondition stays unsatisfiable at the boundary |
| 9 | **§4.0's D-014 exception (N3)** | **that every clause of tier 1's list is decidable from the offending message's own bytes** | **NO — P4.** *"A parent that does not exist"* is decidable only against the receiver's own store. `CRYPTOGRAPHY.md` §8.1, written in this pass, states the principle it breaks |
| 10 | **§5.3's `p: P(hand_id - 1)` (N7)** | **that `P(k-1)` is retrievable for a finished hand** | **YES.** It is a field of the retained record, sized into the 56-byte figure |
| 11 | **`CRYPTOGRAPHY.md` §8.1 (N9)** | **that each proof's public inputs are agreed chain content** | **YES**, and it is stated per proof with the failure mode named (a locally reconstructed input turns a verdict into a divergence) |

**Five of eleven fail, and the shape has changed.** Gate 3's finding was that
failures cluster across the owner boundary. That is still true of rows 5, 8 and 9,
but three of this pass's five (rows 4, 6, 7) are a **new** shape and it is worth
naming:

> **A fix that removes a check must re-derive the argument for every consumer of
> that check, including consumers added in the same pass.** N2 removed duplicate
> suppression on an idempotence argument that was true when written; N5 added a
> non-idempotent disposition to the same step, in the same edit, in a different
> section. Neither author was wrong about their own half. The composition is the
> defect, and the ordering of the two rows in the pass is what hid it.

The discipline that catches it is one line in the method: **when a pass both
deletes a guard and adds a disposition behind it, list the dispositions again
after the additions land, not before.**

---

# Part 4 — The healthy-table check, repeated and widened

Four seats then two then six, twenty hands each, nothing goes wrong, every peer
answers in one round trip.

### 4.1 Round trips per hand

Counting stages that need a network round trip, `n` seats, one betting round per
street with no re-raises, all seats to showdown:

| Segment | Stages | `n = 2` | `n = 4` | `n = 6` |
|---|---|---:|---:|---:|
| boundary: ckpt-8 `STATE_HASH`, ckpt-8 `STATE_ACK`, `HAND_INIT` | 3 | 3 | 3 | 3 |
| deck: `DECK_INIT`, `n ×` (`SHUFFLE_STEP` + `SHUFFLE_PROOF`), `DECK_COMMIT` | `2n + 2` | 6 | 10 | 14 |
| deal + checkpoint 2 pair | 3 | 3 | 3 | 3 |
| pre-flop: `n` actions + checkpoint 3 pair | `n + 2` | 4 | 6 | 8 |
| flop / turn / river: `3 × (BOARD_REVEAL + (n−1) actions + pair)` | `3(n + 2)` | 12 | 18 | 24 |
| showdown: `SHOWDOWN_REVEAL`, ckpt-7 pair, `HAND_COMPLETE` | 4 | 4 | 4 | 4 |
| **total = `6n + 20`** | | **32** | **44** | **56** |

`n = 4` reproduces Gate 3's 44 exactly, which is the cross-check that the formula
is the same walk.

### 4.2 Wall clock per hand, protocol only

At 50 ms RTT and 42 ms of Bayer–Groth proving per shuffler [MENTAL §5.1],
sequential:

| Seats | Round trips | Protocol | Proving | **Per hand** | **20 hands** |
|---:|---:|---:|---:|---:|---:|
| 2 | 32 | 1.60 s | 0.084 s | **1.68 s** | **33.7 s** |
| 4 | 44 | 2.20 s | 0.168 s | **2.37 s** | **47.4 s** |
| 6 | 56 | 2.80 s | 0.252 s | **3.05 s** | **61.1 s** |

Plus one Ed25519 `verify_strict` per event on the receive path, still unmeasured
(`SPEC_CS.md` §33 Phase 4 target). Against one human decision at 20 000 ms, the
protocol is not the cost of a hand at any of the three sizes.

### 4.3 Does any deadline fire?

**At two and four seats: no.** Every stage deadline has three orders of magnitude
of margin — `crypto_step_timeout_ms = 30 000` against a worst single stage of
≈ 92 ms; the boundary's `hand_delay_ms + crypto_step_timeout_ms = 37 000` against
150 ms. The whole-hand deadline `hand_deadline_ms = 600 000` from `TERMINAL(k-1)`
(§8.2, R-1) covers protocol plus human time: at most `4n` actions in a
no-re-raise hand, at `action_timeout_ms + action_grace_ms = 25 000` each —
200 s at two seats, 400 s at four. Both fit.

**At six seats it fires, on legal play, with nobody at fault — P3.**
`4n = 24` actions × 25 000 ms = **600 000 ms**, which is `hand_deadline_ms`
exactly, before the boundary and the deck and before a single legal re-raise
(each of which adds up to `n` more actions). The abort is `cause = 1`,
`attributed = []`, `cert_hash = None`: stacks restored, nobody blamed, no
progress. Repeat and the table never completes a hand. At `MAX_SEATS = 10` the
figure is 1 000 s against a 600 s deadline and the margin is negative by 400 s.

**Nothing in the corpus reconciles the two constants.** `hand_deadline_ms` is a
single figure in §13's preset with a field cap of `3 600 000` (§9.4, `n(17)`), and
no document scales it by seat count or notes the interaction. This is the finding
the widening was for: three passes have checked a four-handed table, and four is
the largest size at which the preset is safe.

### 4.4 Does every hand place a checkpoint?

**Yes, at all three sizes and on all five hand shapes.** §6.2 row 8 is normative
and unconditional — *"the boundary checkpoint … on **every** hand and on both
terminal paths"* — and `STATE_MACHINE.md` I32(a) asserts coverage for every
`hand_id` the engine reaches `HandComplete` for.

| Hand shape | Checkpoints | `n = 2` | `n = 6` |
|---|---|---|---|
| full hand to showdown | 2,3,4,5,6,7,**8** | yes | yes |
| fold-out (T30) | 2,3,**8** | yes | yes |
| all-in run-out (T38) | 2,3,7,**8** (rows 4–6 do not fire; L9's *"where one was placed"* is what opens the turn and river) | yes | yes |
| drain hand (`\|dealt_in\| == 1`) | **8** only | yes | yes |
| aborted at `HAND_INIT` | **8** only, at T46 | yes | yes |

**What the widened check surfaces beyond P3.** At two seats every checkpoint's
required set has two members, so N1's floor and T50's new conjunct are inert and
the heads-up MVP pays nothing for either — which is the right answer for the size
`SPEC_CS.md` ships first. At six seats the deck segment is `2n + 2 = 14` of the 56
round trips, so shuffle cost grows linearly in seats twice over (round trips and
proving); at `MAX_SEATS = 10` that is 22 of 80 round trips and 0.42 s of proving,
still inside `crypto_step_timeout_ms` per stage but worth a Phase 4 measurement
alongside the `verify_strict` figure.

---

# Part 5 — The D-014 sweep

Every document, and every place a receiver rule says a violation has no
consequence. Tier 1 removes the sender; tier 2 removes only against a checkpoint
both peers signed; everything else removes nobody.

### 5.1 Which documents carry it

| Document | D-014 | Carried where | Verdict |
|---|---:|---|---|
| `STATE_MACHINE.md` | **56** | §2.4 `SeatStatus::Removed`, T64/T65/T66, I34, §5.2's box, §12.1 rows | **complete** |
| `THREAT_MODEL.md` | **49** | §5.1's tier split, §5.2, **X37**, §5.5's mirror gate, §9.2 | **complete** |
| `PROTOCOL.md` | **40** | §4.0's box (N3), §4.9's `kind = 3` box, §4.10 `cause = 6`, §5.2, §11 | **complete except P4** |
| `CRYPTOGRAPHY.md` | **23** | new **§8.1**, three proofs, each with what a failure proves and does not | **complete** — N9 closed |
| `DEPENDENCIES.md` | **14** | second sweep record, l. 1057 | **complete** — L8 |
| `CONTRIBUTING.md` | **12** | second sweep record, §2.5's fourth failure, §3 checklist | **complete** — L8 |
| `DECISIONS.md` | 26 | D-014, D-014-1/2/3, D-014-t | — (N8's citation) |
| `NETWORK_STACK.md` | **0** | — | **contradiction — P5** |
| `SPEC_CS.md` | 0 | — | correct; it is the source |

### 5.2 The tier boundary, checked at every site that states it

**Held, and enforced by a type rather than by prose in the two places that
matter.**

* `PROTOCOL.md` §4.0's box: tier 2 *"removes nobody until this receiver holds the
  completed `STATE_ACK` stage §4.9 requires; before one exists it voids the hand
  and removes nobody"*.
* §4.9's `kind = 3` box: the `STATE_ACK`-not-`STATE_HASH` choice, argued from K1's
  own divergence; **exactly one** `SignedEvent` in `n(3) evidence`, because *"a
  finding that needs a second event to be decidable is not self-authenticating"*.
* §4.10's `cause = 6` rows: tier 1 accepts *"at once"*, tier 2 **rejects** until
  the receiver holds the stage and the event is illegal *against the
  `PublicTableState` that checkpoint fixed*.
* `STATE_MACHINE.md` T64's guard carries the precondition as a conjunct.
* `src/security/validation.rs`: `Tier2Finding` has no constructor that does not
  take an `AgreedCheckpoint`, so a tier-2 removal without one does not compile,
  and `Finding::Tier2Unconfirmed` maps to `Outcome::VoidHandOnly` with `None` for
  the removal order.
* Equivocation, timeouts, `attributed`, votes and quorums are excluded by name at
  five sites.

**Contradictions with `src/security/validation.rs`: one, and one omission.**

1. **`SelfContained::ParentUnknown`** is tier 1 in the code, in D-014's list, in
   §4.0's box and in §4.10's `cause = 6` row. It is not self-contained. **P4.**
2. **`AgreedCheckpoint { state_hash, sequence }` carries no emitter set.** §4.9
   requires the checkpoint's *"emitter set contained **both the accused and this
   receiver**"* and T64 requires `agreed.is_some()` for checkpoint `n` *of this
   hand*. The type proves *a checkpoint was named*; it does not prove *whose*.
   The doc comment claims *"a checkpoint whose state every participant signed"*,
   which the struct cannot witness. **P6** — one field, and the module's own
   thesis is that this precondition lives in the type system.

Everything else in the code matches: the ten tier-1 variants are §4.0's list plus
`Malformed`, which D-014's own list carries; `explain()` gives the information
window one sentence per variant; `adjudicate` produces a `RemovalOrder` only for
`Tier1` and `Tier2`.

### 5.3 Places a receiver rule says a violation has no consequence

Swept for *"removes nobody"*, *"remove nobody"*, *"no removal"*, *"nothing further
follows"*. Nineteen sites. **Eighteen are correct and one contradicts D-014:**

* `PROTOCOL.md` l. 1914 (*"Nothing further follows automatically (D-010), with…"*),
  l. 1949–1957, l. 3456, l. 3487, l. 5828, l. 5884, l. 7193 — all now carry the
  exception or are about a class D-014 does not touch (sitting out, votes below
  `|V| = 2`, attribution).
* `THREAT_MODEL.md` l. 1376, l. 1406 — tier 2's precondition, correct.
* `STATE_MACHINE.md` l. 1215 (T66, `hand_id == 0`, removes nobody) — a **narrowing
  made by an owner rather than by the decision**, carried over from Gate 3 and
  still not in D-014. The reasoning is right (the roster's seat vector is frozen
  at `TABLE_READY`, so removing a seat there forks the genesis); the exception
  belongs in D-014 as one line. Carried, not re-filed.
* **`NETWORK_STACK.md` l. 97, l. 541 (§1.2 prohibition 7), l. 2536** — *"no
  eviction, anywhere: no `block_peer`, no **unseating**…"* and *"remove, block,
  **unseat**, refuse or penalise a peer on the strength of a protocol proof"*.
  D-014 tier 1 is a removal on a failed proof. **P5.**

`NETWORK_STACK.md` §0.1's transport row — *"A protocol proof, verdict or
attribution … **forbidden.** No transport consequence of any kind"* — is
**unaffected and correct**, because D-014's removal is from the table and never
from the transport. That is the defensible half Gate 3 identified. The
indefensible half is the word *unseat* in a prohibition list that binds a
document D-014 amended, in the one file with zero D-014 occurrences.

---

# Part 6 — Coverage table

`grep -o … | wc -l`, 2026-08-28, after this pass's edits.

| Document | D-009 | D-010 | D-011 | D-012 | D-013 | D-014 |
|---|---:|---:|---:|---:|---:|---:|
| `PROTOCOL.md` | 22 | 67 | 51 | 19 | 40 | 40 |
| `STATE_MACHINE.md` | 28 | 85 | 36 | 34 | 81 | 56 |
| `THREAT_MODEL.md` | 37 | 127 | 104 | 39 | 35 | 49 |
| `NETWORK_STACK.md` | 5 | 25 | 35 | 25 | 21 | **0** |
| `CRYPTOGRAPHY.md` | 18 | 39 | 30 | 21 | 15 | **23** |
| `CONTRIBUTING.md` | 12 | 12 | 11 | 8 | **9** | **12** |
| `DEPENDENCIES.md` | 11 | 6 | 10 | 10 | **5** | **14** |
| (`SPEC_CS.md`) | 0 | 0 | 0 | 0 | 0 | 0 |
| (`DECISIONS.md`) | 6 | 21 | 12 | 16 | 24 | 26 |

**Six zeros became one.** `CONTRIBUTING.md` and `DEPENDENCIES.md` closed both of
theirs (L8); `CRYPTOGRAPHY.md` closed its D-014 zero with a substantive section
(N9). `SPEC_CS.md`'s row is correct and is not a gap — it is the source the
decisions answer to, and a decision reference inside it would invert the authority
order.

**`NETWORK_STACK.md`'s single remaining zero is now the corpus's only coverage
defect, and it is a contradiction rather than an omission (P5).** Also visible:
`CRYPTOGRAPHY.md` D-013 = 15, up from 14 but still the lowest of the five
specification documents, and `DECISIONS.md`'s `J-5` records that this document
*"carries zero occurrences of D-013 and was not opened"* — a row now stale by
count and still right by substance.

---

# Part 7 — New defects

Numbered `P` to avoid collision with `DECISIONS.md`'s open `N-*` rows.

| # | Severity | Defect |
|---|---|---|
| **P1** | **BLOCKER for the receiver** | **The engine keeps one checkpoint and the wire now needs two.** `STATE_MACHINE.md` §2.3: `pub checkpoint: Option<CheckpointState>` — *"the checkpoint currently open, if any"*, *"at most one is open at a time"*. §4.9 in this pass makes the boundary checkpoint of hand `k` outlive hand `k`: a checkpoint-8 `STATE_ACK` of chain `k` is accepted *"until `TERMINAL(k+1)` is fixed"* (N6), and a checkpoint-8 `STATE_HASH` of chain `k` is compared, and on mismatch *"enters §6.3 at step 1"*, after `HAND_INIT(k+1)` closed the window. **Three consequences.** (i) T51 can never set `agreed` for checkpoint 8 of hand `k` once hand `k+1` has opened one, so **D-014's tier-2 precondition remains unsatisfiable at the boundary** — the exact fault N6 was filed for. (ii) T50's new `\|checkpoint.required\| == 1` conjunct is unreadable on the stale route, so N1's second half never fires there. (iii) For a **non-solitary** receiver a stale checkpoint-8 mismatch has **no engine carrier at all**: `SolitaryDivergence` is the only event the wire produces from step 10b, `Event::StateHash` is excluded by *"never reaches step 13"*, and this is reachable on a healthy table because forwarding (§1.5) reorders `STATE_ACK` and `HAND_INIT(k+1)` — the reordering §4.9 itself now relies on. **Fix:** `STATE_MACHINE.md` §2.3 keeps the *previous hand's* boundary `CheckpointState` alongside the open one, with the same lifetime §5.3 gives its slots (`TERMINAL(k+1)`), and `PROTOCOL.md` §6.3 step 1 gets a carrier for the non-solitary stale mismatch. Owner: `STATE_MACHINE.md` §2.3, §5.2 (T50, T51); `PROTOCOL.md` §4.0 step 12a. |
| **P2** | **BLOCKER for the receiver** | **A replayed, *agreeing* checkpoint-8 `STATE_HASH` stalls one hand, once per hand, for ever.** §4.0 step 10a is **skipped** for stale-hand events (N2), on the stated argument that *"each of its four dispositions is idempotent — … a seat already in the readmission set is already in it"*. §4.9's readmission set `A` is *"read at exactly one place — the next hand init this receiver runs … and is **cleared** there"*. So the disposition is idempotent **within** a hand and not across hands: replay the same signed message after each hand init and `A` is re-populated each time. At a receiver that has narrowed the sender out — a draining or forked peer, which is the regime N5 exists for — the next hand init is required of `P(m) ∪ A`, the readmitted seat does not sign, and **stage 0 stalls to `hand_deadline_ms = 600 000`**. The attacker needs no key: it replays a message it observed, once per ten minutes, and the retained record keeps it valid for `MAX_RETAINED_HAND_RECORDS = 4 096` hands. That is D-013's own fixed point, restored by the fix written to keep D-013's promise, and it is *per-receiver* so no other peer sees a reason. **Fix:** either keep one bit of anti-replay for the readmission branch alone (a seat, once, per finished hand — bounded by the record that already exists), or make `A` sticky rather than cleared and let the ordinary `P` machinery drop it. Owner: `PROTOCOL.md` §4.0 step 10a/10b and §4.9. |
| **P3** | high | **`hand_deadline_ms` does not scale with seat count and is exceeded by legal play from six seats up.** §13's preset is one figure, `600 000`, running from `TERMINAL(k-1)` (§8.2, R-1) and covering the whole hand. A no-re-raise hand has up to `4n` actions and `action_timeout_ms + action_grace_ms = 25 000` each: 200 s at `n = 2`, 400 s at `n = 4`, **600 s at `n = 6`** before the deck and the boundary, and 1 000 s at `MAX_SEATS = 10`. The abort is `cause = 1`, `attributed = []` — stacks restored, nobody at fault, no progress — so a table of six unhurried honest players deals hands and completes none. Three passes checked four seats, which is the largest size at which the preset is safe. **Fix:** state the preset's validity bound, or derive `hand_deadline_ms` from `MAX_SEATS × (action_timeout_ms + action_grace_ms)` with a margin. Owner: `PROTOCOL.md` §13 and §8.2. |
| **P4** | high | **D-014 tier 1 contains a clause that is not self-contained: *"a message whose chain parent does not exist"*.** It appears in D-014's list, in `PROTOCOL.md` §4.0's new box, in §4.10's `cause = 6` row (*"this receiver's own run of §4.0 over that event returns a tier-1 illegality — … a parent that does not exist"*) and as `SelfContained::ParentUnknown` in `src/security/validation.rs`. **Whether a parent exists is a property of the receiver's store, not of the message's bytes.** §5.1's own table already says an event whose predecessor has not arrived is *"buffered, never applied, until stage `s+1` completes"*, and §4.11's ordering buffer exists for exactly that; §6.3 case (b) is *"a peer is missing events it cannot obtain"*; §6.3 step 3's whole purpose is *"peers exchange the events they are missing"*. So one dropped frame at one peer makes that peer remove an honest player while every other peer sees nothing wrong — and the removal then forks `HAND_INIT(k+1)`'s body and stalls stage 0. This is the class D-010 spent four passes deleting, re-entering through D-014's list, and `CRYPTOGRAPHY.md` §8.1 states the governing principle **in this pass**: *"a verifier that substitutes a locally reconstructed input for a chained one … proves nothing about the signer — it proves the two peers disagree about the input, which is a divergence … and never a removal."* It is also a live counter-example to **D-014-2**'s mirror test, which is the gate on shipping the feature. **Fix:** delete the clause from tier 1 (an unknown parent is a buffer, then a §6.3 case (b) abort), and delete `ParentUnknown` from `SelfContained`. Owner: `DECISIONS.md` D-014, `PROTOCOL.md` §4.0 and §4.10, `src/security/validation.rs`. |
| **P5** | medium | **`NETWORK_STACK.md` §1.2 prohibition 7 forbids what D-014 tier 1 requires, in the one document with zero D-014.** l. 541: *"remove, block, **unseat**, refuse or penalise a peer on the strength of a protocol proof"*; l. 97: *"no eviction, anywhere: no `block_peer`, no **unseating**"*; l. 2536 repeats it. §0.1's transport row is untouched by D-014 and correct — the removal is from the table, never from the transport — so the fix is to **scope prohibition 7 to the transport** and add the one row Gate 3 asked for. As it stands, a document that binds the corpus contradicts the decision, which is `N3` in a file this pass did not open. Owner: `NETWORK_STACK.md` §0.1, §1.2. |
| **P6** | medium | **`AgreedCheckpoint` cannot witness the precondition the module says it enforces.** `src/security/validation.rs` carries `AgreedCheckpoint { state_hash: Hash, sequence: u64 }` and documents it as *"a checkpoint whose state every participant signed"*, but nothing in the type records **who signed**. §4.9 requires the checkpoint's emitter set to contain *"both the accused and this receiver"*, and T64 requires `agreed.is_some()` for a checkpoint **of this hand** at or before the offending `sequence`. The module's thesis is *"that precondition is not a comment"* — and half of it currently is. **Fix:** add the emitter set (or the two seat indices the rule names) to `AgreedCheckpoint`, and the `hand_id`. Owner: `src/security/validation.rs`. |
| **P7** | low | **`PublicTableState`'s field order is given twice, differently, and it is a `#[cbor(array)]` struct.** §6.1's table — the normative owner — ends `… transcript_head, signed_this_hand`, with the four `Vec<bool>` flag vectors between `committed_this_hand` and `current_bet`. §2.9's enumeration ends *"`transcript_head`, `signed_this_hand`, and the `Vec<bool>` flag vectors"*, which puts the flags **after** `signed_this_hand`. In an array encoding the order is the encoding, so the two produce different `state_hash` values. §2.9 is a sweep listing rather than a field order and D-011 gives §6.1 the ownership, so this is one clause — but it is in the section that says *"every field, because it is the widest hashed struct in the corpus"*, and it is the struct `src/protocol/` is about to write. Owner: `PROTOCOL.md` §2.9. |
| **P8** | low | **`DECISIONS.md` D-014-1 still reads *"all — not started"*.** The integration is now complete in five documents and substantially in two more, and this pass's own coverage table is the evidence. Gate 3 flagged the row as stale; it is one pass staler. A blocking row that says *not started* about work that is done is the same defect class as `L8`'s sweep records, in the file that outranks them. Owner: `DECISIONS.md`. |

**The pattern check, run on this pass's own findings.** Five of the eight (P1, P2,
P4, P6, P7) are on a path this pass newly made load-bearing, and **two of them —
P1 and P2 — are the composition shape Part 3 names**: two correct edits in one
pass, in two sections, whose interaction neither author had open. P4 is the older
shape, a fix reaching across an owner boundary into a list nobody re-derived. P3
and P5 are the shape the *method* found rather than the fixes: a check widened
(two, four, six seats) and a document not opened.

---

# Part 8 — Per-item implementability

## The eight build-order items

| # | Item | Implementable today? | Gap |
|---:|---|---|---|
| 1 | `EventBody` / `SignedEvent`, §2.3's twelve and two fields | **Yes.** Three nesting levels, field indices, sentinels for `chain_scope == 0`. Already written | — |
| 2 | `EventType`, closed `#[repr(u16)]`, 39 codes, four axes | **Yes.** Written and tested | — |
| 3 | Envelope sentinel checks for `chain_scope == 0` | **Yes, and the caveat is gone.** `M-1` is closed: §4.11's prose now says *"`DISPUTE` is the only message of groups 3–8 with `chain_scope = 0`"*, which agrees with its own table's four rows (`HELLO`, `CAPABILITIES`, `PLAYER_LIST`, `DISPUTE`) | — |
| 4 | The 39 payload structs with §9.3 caps and §9.4 bounds | **Yes.** `DISPUTE { kind = 3 }` needs exactly one `n(3)` entry, stated as a bound | — |
| 5 | `slot(E)`, one function, §5.2.1's 8-tuple | **Yes.** One site, literal tuple, four components declared absent with a reason each | — |
| 6 | §4.0 steps 1–11 as `validate_prefix` | **Yes, and steps 10a/10b are now fully pinned.** N2 gives 10a's skip and §5.3 gives the store lifetime and the one retained band; N7 gives 10b's set as `P(hand_id - 1)`. **Write step 10b's readmission branch behind a `TODO(P2)`** — the branch is specified, its replay behaviour is not | **P2** on one branch |
| 7 | A `constants` module from §13 | **Yes.** `BOUNDARY_SEQUENCE_BASE`, `BOUNDARY_CHECKPOINT_BASE`, `MAX_RETAINED_HAND_RECORDS`, both band exemptions. **Do not encode `hand_deadline_ms` as validated for `MAX_SEATS`** — P3 | note P3 |
| 8 | Round-trip and canonicality tests over every payload type | **Yes.** §2.5's gate and §2.7's cases are complete and independent | — |

**All eight are implementable.** The one qualification is item 6's readmission
branch.

## The newly unblocked items

| Item | Implementable today? | Gap |
|---|---|---|
| **`PublicTableState`** | **Yes.** §6.1's table is the normative field list, `signed_this_hand` is in it, appended last for §2.2 rule 5, and the paragraph under it forbids reading it as a fifth status vector. **Transcribe from §6.1's table, never from §2.9's enumeration** — the two orders differ and the struct is a CBOR array (**P7**). Expect one append-only field if D-014-3 lands a `removed` vector | **P7**; D-014-3 open |
| **`state_hash`** | **Yes.** `h("p2p-poker v1 state", [canonical_cbor(PublicTableState)])`, one part, domain string in §2.8's closed register | — |
| **the `checkpoint` field's range** | **Yes — range-check to `1..=8`.** §6.2's table is eight rows under *"at these points and no others"*, and the reconciliation stage *"carries its `checkpoint` number rather than a number of its own"*, so there is no ninth value | — |
| **§5.3's three anti-replay stores** | **Yes, including the lifetime, which was N2's gap.** `event_class == 0` keyed by `slot(E)` and dropped at hand end; classes 1 and 2 sparse, per stage, keyed `(seat, subject_seat)` and `(seat, subject_digest)`; the `DISPUTE` counter at `MAX_DISPUTES_PER_SENDER_PER_HAND = 8`. Both reserved bands exempt from the `MAX_STAGES_PER_HAND` abort, occupancy recomputed (`+ MAX_SEATS + 16 × MAX_SEATS`). **One addition to write with it:** the checkpoint-8 `STATE_ACK` slice of the previous hand, `8 × MAX_SEATS = 80` entries, retained to `TERMINAL(k+1)`, which is the one structure that outlives its hand and the one place step 10a is **not** skipped | — |
| **`RetainedHand`** | **Yes.** Four fields — `hand_id`, `was_solitary` (the **disjunction**, an independent bool, not `p == {self}`), `p: SeatSet` (= `P(hand_id - 1)`), `checkpoint8_state_hash` — LRU at `MAX_RETAINED_HAND_RECORDS = 4 096`, ≈ 56 B each, read by step 10b and nothing else | — |
| **`PrefixOutcome` / the freeze path** | **No — do not wire a release.** Build the enum with a `SolitaryDivergence` variant carrying `hand_id`, `seat`, `event_hash`, and wire **no** release path and **no** engine consumption: the guard that consumes it is off by one (`N-1e`), the non-solitary carrier does not exist (**P1**), and the checkpoint it names is not retained (**P1**) | **`N-1e`, P1** |
| **§4.0 steps 12, 12a** | **Still deferred**, for `N-1e` and P1 rather than for N1 | — |
| **§4.0 steps 13, 14, 15** | Still deferred; they need the engine | — |
| **`TIMEOUT_VOTE`, `TIMEOUT_CERT`, `EquivocationProof`** | Unchanged. `OQ-F` may delete them; write the payload structs, wire no behaviour | — |
| **`SHOWDOWN_MUCK` policy** | Unchanged. `Q-01` | — |
| **`src/security/validation.rs`** | **Two edits owed**, and both are small: delete `ParentUnknown` from `SelfContained` (**P4**) and give `AgreedCheckpoint` the emitter set and `hand_id` the rule names (**P6**) | **P4, P6** |

**Named gaps by section.** `STATE_MACHINE.md` §2.6's floor (`N-1e`, blocking);
`STATE_MACHINE.md` §2.3 and §5.2's T50/T51 checkpoint retention (**P1**);
`PROTOCOL.md` §4.0 step 12a's carrier for the non-solitary stale mismatch
(**P1**); `PROTOCOL.md` §4.0 step 10a/10b and §4.9's readmission replay (**P2**);
`PROTOCOL.md` §13 and §8.2's deadline scaling (**P3**); `DECISIONS.md` D-014's
tier-1 list, `PROTOCOL.md` §4.0 and §4.10 (**P4**); `NETWORK_STACK.md` §1.2
prohibition 7 (**P5**); `PROTOCOL.md` §2.9's field enumeration (**P7**);
`STATE_MACHINE.md` (`N-5e`, the readmission set's engine half).

## What must land before the receiver's freeze path is written

Two, and both are corrections to fixes made in this pass rather than new
decisions:

1. **`STATE_MACHINE.md` §2.6 — `N-1e`.** `solitary_at(k) := solitary_since ==
   Some(j) ∧ j - 1 <= k <= hand_id`, with §2.6's lemma restated over the
   disjunction and I33(a) restated with it. One character, already derived in
   `DECISIONS.md`, and without it the checkpoint-8 comparison route is inert on the
   one hand it was written for.
2. **`STATE_MACHINE.md` §2.3 and `PROTOCOL.md` §4.0 step 12a — P1.** The previous
   hand's boundary checkpoint must be retained as long as §4.9 accepts events into
   it, and a stale checkpoint-8 mismatch at a non-solitary receiver needs a
   carrier. Without it N6's fix has no consumer and D-014 tier 2 has no
   precondition at a hand boundary.

**Three more are one line each and should land with them:** **P2** (one bit of
anti-replay on the readmission branch, or a sticky `A`), **P4** (delete
`ParentUnknown` from tier 1, in three documents and one file), **P5**
(`NETWORK_STACK.md` §1.2 prohibition 7 scoped to the transport, plus the one row).
**P3** is `PROTOCOL.md` §13's and is the one item on this list that a two-seat MVP
can ship without.

**`N-5e`, P6, P7 and P8 are the next pass's**, and **P2 is the one most likely to
be its blocker**: it is a fix's justification invalidated by another fix in the
same pass, which is the shape Part 3 names and the shape no single-document review
can see.
