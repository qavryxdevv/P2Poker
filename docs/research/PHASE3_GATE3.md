# PHASE3_GATE3.md — the receiver gate

**Commission.** Ten passes have run. The ninth (`PHASE3_GATE2.md`) closed K2 and
K4–K7 and both stale cross-owner carries, left K1 and K3 partial, and opened
L1–L9 with four blockers. Its most useful sentence was that **four surgical edits
unblock the receiver half**. This pass grades those four edits and the L-list,
and it asks the one question `src/protocol/messages.rs` needs answered: **can a
correct receiver be built from `PROTOCOL.md` alone?**

**Method, and it is the inversion that produced the last four passes' blockers.**
Nothing here is read off a summary table. The K1 interleaving was re-run from the
transition rows and the receiver pipeline rather than from either document's
account of it; the healthy table was walked stage by stage before §12.1's bound
was opened; the D-014 sweep was run against the decision text before the
documents' own coverage rows. That inversion produced this pass's blocker, and it
produced it where the pattern predicts: **inside the exit that the previous
pass's fix installed to release the freeze it added.**

**Files as read, 2026-08-28.** `STATE_MACHINE.md` 18:54, `DECISIONS.md` 18:52,
`THREAT_MODEL.md` 18:51, `PROTOCOL.md` 18:47, `CRYPTOGRAPHY.md` 17:57,
`NETWORK_STACK.md` 17:52, `CONTRIBUTING.md` 16:03, `DEPENDENCIES.md` 16:03,
`SPEC_CS.md` 09:29. `src/protocol/messages.rs` read at 18:27, at build-order
item 2.

> **The timestamp finding, sixth iteration.** `STATE_MACHINE.md` is newest again
> and `PROTOCOL.md` is seven minutes behind it — the same ordering as last pass,
> and this time it is *not* where the blocker is. The blocker is in the one place
> neither document's timestamp can show: **a transition in `STATE_MACHINE.md`
> whose guard is satisfied by a stage in `PROTOCOL.md` that the same pass made
> reachable for the first time.** Both halves are new, both are correct read
> alone, and their composition undoes the fix they were written for.

---

## Verdict counts

| Verdict | Count | Items |
|---|---:|---|
| **RESOLVED** | 9 | K3, L1, L2, L3, L4, L5, L6, L7, L9 |
| **PARTIAL** | 1 | K1 |
| **UNRESOLVED** | 1 | L8 |
| **REGRESSED** | 0 | — |

**The four edits all landed, and they landed well.** §4.9 carries a checkpoint-8
box in exactly the shape §4.10's boundary-window box takes; §6.1 carries
`signed_this_hand`; §6.2 carries row 8 and L9's three words; §4.0 carries step
10b; §5.3 carries the retained record and both band exemptions; §13 carries
`BOUNDARY_CHECKPOINT_BASE` and `MAX_RETAINED_HAND_RECORDS`.
`STATE_MACHINE.md` carries the engine half of all of it — T62–T66, I33, I34,
`solitary_since`, `solitary_contradicted`, §9.3 condition 0.6.

**K1 is PARTIAL for a new reason, not the old one.** The freeze now fires. It
fires twice, and the first firing lands *before* the solitary peer deals a single
drain hand — earlier than the fix was designed to achieve. It does not hold.

**Nine new defects, N1–N9. Two block.**

* **N1 — the solitary peer releases its own freeze.** §6.3 step 3's reconciliation
  stage has the required emitter set of the checkpoint it re-derives, which at a
  solitary peer is `P(k) = {self}`. The peer completes that stage in the step that
  emits it, carrying one value that agrees with itself, so `STATE_MACHINE.md`
  **T53** fires unconditionally, clears `solitary_contradicted`, and returns the
  peer to the phase it held. The exit K-9 added to release the freeze releases it
  against the one peer it was added to hold. K1's payoff returns.
* **N2 — §4.0 step 10a is undefined for the events step 10b exists to handle.**
  Step 10a runs first and looks a chained event up in a store §5.3 keeps *"one map
  per table per hand"*; §5.3 also says a finished hand keeps only `TERMINAL(k)`,
  the terminal body and the retained record. There is no store to look a
  stale-hand event up in, and the two available readings are "allocate one" — an
  attacker-driven unbounded allocation in the section written to prevent exactly
  that — and "treat absence as no entry", which is correct and unstated.
* **N3 — §4.0's normative anti-eviction box contradicts D-014 and §4.9's own
  `kind = 3` box.**
* **N5 — K-3b's readmission route does not survive step 10b.**

**Readiness verdict: NOT-READY for the receiver, READY for the message layer.**
Build-order items 1 to 8 are all implementable today with two named exceptions,
and everything currently deferred except §4.0 steps 12/12a is unblocked. Part 8
gives the per-item answer.

---

# Part 1 — Per-item verdicts

## K1 — `P(k)` is per-receiver on the one path where it narrows — **PARTIAL**

Everything the previous pass asked for was delivered, on both sides of the owner
boundary, and the trigger now fires. What was not checked is what happens to the
peer **after** it freezes.

**What is fixed, and it is the whole of what was filed.** §3.2's rule is restated
in the past tense and anchored on a record:

> `PROTOCOL.md` §3.2: "A peer **was in the solitary regime for hand `k`** when the
> required emitter set of a collective stage of hand `k` was `{self}` … **The test
> is a property of the hand the event names, not of the phase the receiver is in
> when the event arrives**, and it is answered from that hand's retained record
> (§5.3)."

§4.0 step 10b is the step that carries it, it sits before step 11, it runs *"on
every incoming chained event, in every phase, at every table"*, and the paragraph
under the table states in terms that **a document that makes a phase absorbing
against it has reopened K1**. `STATE_MACHINE.md` matches it exactly: T62's guard
is `solitary_since == Some(j) ∧ j <= e.hand_id <= hand_id`, T63 gives
`TableClosed` a self-edge for this one event class and retracts
`settlement.tournament_winner`, §9.3 condition 0.6 closes the table on the latch,
and I33(c) is the anti-fixed-point assertion. `DECISIONS.md` L4 states the
acceptance criterion in the direction that makes the hand-off checkable — *"if the
staleness step lands after step 12, or stays scoped on the receiver's current
`hand_id`, T62 is unreachable"* — and the wire side satisfies it.

**And a second, better trigger was added with it.** §4.9's checkpoint-8 box admits
an out-of-set `STATE_HASH` for comparison, §6.1 hashes `signed_this_hand`, and
§6.3 step 1 states both entry conditions. Part 2 shows the second trigger is the
one that actually fires first in K1's own trace, one network delay after the abort
of the hand that forked — before any drain hand exists.

**What blocks.** Both triggers reach a freeze; neither freeze holds.

> `PROTOCOL.md` §4.9: a reconciliation `STATE_HASH` is a new collective stage
> whose "**required emitter set is the emitter set of that checkpoint**".
>
> `STATE_MACHINE.md` T53: `Diverged` + `StateHash`, guard "`round >= 1` ∧ the
> reconciliation-round stage for the disputed checkpoint is complete over its
> required signer set ∧ every value in it agrees" → resume, and **T53 clears
> `solitary_contradicted`; it is the only thing that clears it** (I33(c)).

Checkpoint 8's required emitter set is `P(k)`. At a solitary peer `P(k) = {self}`.
So the reconciliation stage is complete the moment that peer emits its own copy,
and the one value in it agrees with itself. **T53's guard is satisfied by a
solitary peer talking to itself, in the emitting step, with no wall clock and no
other seat involved.** The freeze is released, the latch is cleared, the peer
resumes at the phase it held, and it goes on draining to §9.3 condition 1.

That is N1, and it is graded here rather than as a K1 downgrade to UNRESOLVED
because the detection genuinely works and the fault records are genuinely
written: `Fault{StateDivergence}` from T50 and `Fault{SolitaryDivergence}` from
T62 both reach `Effect::Fault` and `SPEC_CS.md` §22's status panel. K1's original
sentence was *"a silent permanent fork"*. The fork is no longer silent. It is
still permanent.

Two residuals carried unchanged and not re-graded: `Q-10` stays open and is
correctly labelled unclosable by ratification; the bidirectional partition is
conceded in §3.2 in terms.

## K3 — a drain hand places no checkpoint — **RESOLVED**

Closed on both sides, and closed with the property the previous pass could not
grade: **the wire half states what depends on it, and it names a dependant in the
other owner's document.**

* **L1.** §4.9's checkpoint-8 box gives all five quantities — chain `k`,
  `hand_id = k`, `previous_event_hash = TERMINAL(k)` with §3.2's rule extended a
  second time (`stage_hash(BOUNDARY_CHECKPOINT_BASE − 1) := TERMINAL(k)`),
  `sequence = BOUNDARY_CHECKPOINT_BASE` with the reconciliation band
  `8 192 … 8 207` and `1 <= r <= 7`, and total order by the collective stage rule
  — plus the required emitter set, the close condition, and the argument for why
  the base sits **above** the boundary window (*"a reconciliation round extends
  upwards"*). §12's `Q-09` row records that the position between `TERMINAL(k)` and
  `HAND_INIT(k+1)` **needs a rule per message and not one rule**, which is the
  reusable half.
* **L2.** §6.2 row 8 exists, under a paragraph that states what its absence cost
  (*"T61 fires at `hand_deadline_ms` after every settled hand"*). §6.1 carries
  `signed_this_hand`, appended last for §2.2 rule 5, with the argument for why it
  is the one boundary quantity two honest peers can hold differently and never
  notice, and with the trap-avoidance paragraph that stops it being read as a
  fifth status vector.
* **L3.** §4.9's box widens the *accepted* set at that one stage and says why the
  two sets differ there and nowhere else — *"its body is a claim about who the
  participants are, and the seat whose participation is in question is exactly the
  seat whose copy the strict rule throws away"*. §4.0 step 12 is amended so it
  cannot reject it. §4.11 row `0x0701` carries it; §4.11's honest-interleaving rows
  33 and 34 carry it and state that `STATE_ACK` is **not** widened.
* **K-9-a.** The stale justification is withdrawn in §6.3 in terms — *"§6.2 row 8
  now places the boundary checkpoint on every hand including a drain hand and an
  aborted one, so the checkpoint route exists"* — and §6.3 states why both triggers
  are kept.

K-3b's readmission claim is undermined by the same pass's step 10b; that is filed
as **N5** against `PROTOCOL.md` §4.0 and `STATE_MACHINE.md` §5.2, not as a K3
downgrade, because what K3 asked for is delivered.

## L1 — **RESOLVED.** See K3.
## L2 — **RESOLVED.** See K3.
## L3 — **RESOLVED.** See K3.

## L4 — the solitary-stage freeze has no trigger that can fire — **RESOLVED**

The wire half is §4.0 step 10b plus §5.3's `RetainedHand`; the engine half is
`solitary_since`, T62 and T63. Step 10b routes to step 12a for a hand already
finished, and step 12a's cell records the second route explicitly — *"the second
route is what makes the rule able to fire at all (`L4`)"*. §5.3 bounds the record
at `MAX_RETAINED_HAND_RECORDS = 4 096` and states the cost of eviction. The
second-order check the previous pass asked for is run in §5.3 and comes out clean
on all three candidates.

Two residual mismatches between the two halves are filed as **N4** (two different
memories for one past-tense test) and **N7** (`P(k)` where the rule says
`P(k-1)`). Neither prevents the trigger from firing.

## L5 — the boundary window vs `MAX_STAGES_PER_HAND` — **RESOLVED**

Both bands are exempted normatively in §5.3 and in §13, in the same words, with
the failure named: *"an implementer who range-checks `sequence <
MAX_STAGES_PER_HAND` rejects **every** boundary event and every boundary
checkpoint, which is `Q-09`'s closure and `K-3`'s closure both undone by a
comparison."* §5.3's occupancy bound is recomputed — `MAX_STAGES_PER_HAND ×
MAX_SEATS × 2` plus `MAX_SEATS` for the window plus `16 × MAX_SEATS` for the band,
170 further entries per hand against 40 960 — and the old bound is recorded as
having been short by exactly those entries.

## L6 — `PROTOCOL.md` §4.4's stale claim about `CRYPTOGRAPHY.md` §7.3 — **RESOLVED**

Rewritten in the past tense, which is the disposition the item asked for:
§4.4 now reads *"`CRYPTOGRAPHY.md` §7.3 **used to** reproduce both … **The owner
has since made that edit.**"* Third instance of *the finding document was not told
when the owner acted*, and the first to be closed in the pass that opened it.

## L7 — `STATE_MACHINE.md` §9.3 condition 2 — **RESOLVED**

Condition 2 reads `|dealt_in| == 0`, and conditions 2, 3 and 4 are now §5.3 step
9's three branches in step 9's order on step 9's quantity. The §9.3 header is
amended to say the conditions are evaluated after hand init has run, which is what
makes reading `dealt_in` there well-defined. K-7 fixed the exit a pass earlier;
this is the entry, and the two now read one predicate.

## L8 — sweep records that stop at D-012 — **UNRESOLVED**

`CONTRIBUTING.md` l. 8 still reads *"**Swept against D-009 to D-012 on
2026-08-28**"*; `DEPENDENCIES.md` l. 1022 still reads *"**Sweep record,
2026-08-28 — D-009 to D-012 against this document.**"* Both carry **zero**
occurrences of D-013 and of D-014. `DECISIONS.md`'s open list records the item as
*"Not acted on here; recorded so it is owned"*, which is D-013's process rule
working; the item is nonetheless not discharged, and two more documents have since
joined it — see **N9**. `THREAT_MODEL.md` §9.2 shows the discipline that closes
it: report the counts rather than assert a sweep.

## L9 — the `BOARD_REVEAL` gate on the all-in run-out — **RESOLVED**

§6.2 now reads *"only after the preceding street's `STATE_ACK` stage completes,
**where one was placed**"*, followed by a paragraph that states why the three
words are not a softening: the property protected is *no card opens while peers
disagree about the state*, and a street that placed no checkpoint produced no
disagreement to be waiting on. The deadlock is named on the right hand shape
(T38's path) and the rows that place no checkpoint on it are named (`4`–`6`).

---

# Part 2 — The K1 interleaving, run again

Three seats, `A` (0), `B` (1), `C` (2). Hand `k-1` completes normally;
`P(k-1) = {A, B, C}`. The adversary's whole budget is dropped frames.

### 2.1 The walk

**1. Hand `k-1`'s boundary.** `C` emits `PLAYER_LEAVE` at
`sequence = BOUNDARY_SEQUENCE_BASE + 2`, parent `TERMINAL(k-1)`. **`A` receives
it. `B` does not.**

**2. Checkpoint 8 of hand `k-1` agrees, and this is new and it does not help.**
T45 opens the checkpoint *"the moment `TERMINAL(k-1)` is fixed at this peer"*, so
every peer's body is the state at `TERMINAL(k-1)` — before the boundary window
runs. `A`'s copy and `B`'s copy are byte-identical, the stage completes over
`P(k-1) = {A,B,C}`, and T47's gate discharges at both. **The boundary checkpoint
cannot see a boundary-window disagreement, by construction, because it is placed
before the window opens.** Both peers enter hand `k`.

**3. Hand `k` stalls at stage 0.** `R(HAND_INIT, k) = P(k-1)`. `A`'s body reflects
`C`'s departure; `B`'s does not. Each rejects the other's copy on body mismatch.
`C` has left and emits nothing. Stage 0 stalls at both, which is exactly where
§3.1's frozen roster and §4.10's per-receiver window close route this — loudly, as
designed.

**4. Hand `k` aborts.** T57 at `hand_deadline_ms` → `HandAborted` → **T46**, which
restores every stack and **opens checkpoint 8 and publishes this peer's own
`STATE_HASH` for it**. `ABORT_TERMINAL(k)` is a function of `GENESIS(k)` alone, so
both hold the same terminal and the same `GENESIS(k+1)`.

**5. `P(k) = {A}` at `A` and `{B}` at `B`.** `A`'s own `HAND_INIT(k)` copy counts
(own emission counts, K1's answered sub-question), as does its own checkpoint-8
`STATE_HASH`; `B`'s copy was rejected; `C`'s `PLAYER_LEAVE` counts into no `P`
(§5.3 step 4(ii)); the terminal `HAND_ABORT` counts into none (I31(c)). §5.3 step
8 records `solitary_since := Some(k+1)` at hand `k+1`'s init.

**6. The checkpoint-8 bodies for hand `k` differ, and they differ in the field
§6.1 was amended to carry.** `A`'s `signed_this_hand` is `[true,false,false]`;
`B`'s is `[false,true,false]`. That difference alone is sufficient and it is
K-8-independent; `roster` and `ledger_out` differ as well under the reading in
which hand `k`'s init already subtracts `C`'s stack, which is `K-8` and is open.

**7. `A`'s checkpoint-8 window closes almost instantly.** T47's gate does not
apply on the aborted path (*"hand `k` reached `HandComplete` through **T46**, in
which case there is no gate"*), so `A` runs hand init for `k+1` at once, and
`HAND_INIT(k+1)`'s required set is `P(k) = {A}` — self-completing. §4.9's close
condition is *"this receiver's acceptance of a complete `HAND_INIT(k+1)` stage"*,
so the window closes microseconds after it opened.

**8. First freeze — through checkpoint 8, and it is earlier than the rule the fix
was written for.** `B`'s checkpoint-8 `STATE_HASH` for hand `k` arrives naming a
hand `A` has finished → **§4.0 step 10b, second branch**: *"if it is a checkpoint-8
`STATE_HASH` (§4.9) it is compared against the retained checkpoint-8 `state_hash`,
and a mismatch routes to §6.3 step 1"*. Mismatch. §6.3 step 1 → **T50** →
`Diverged`, `Fault{StateDivergence}`. **This lands one network delay after the
abort of hand `k`, before `A` has dealt a single drain hand.**

**9. Second freeze — through the rule itself, and it is the one that latches.**
`B`'s `HAND_INIT(k+1)` copy follows on the same stream. Hand `k+1` is a hand `A`
dealt with `|P(k)| == 1`; `B` is outside `P`; the type is neither exempt case →
**§4.0 step 10b, first branch → step 12a → `SolitaryDivergence` → T62**, whose
scope is *"any phase except `TableClosed`"* and therefore includes `Diverged`.
`solitary_contradicted := true`. §9.3 condition 0.6 would close the table at the
next T47.

### 2.2 Where it is lost

**Steps 8 and 9 are the fix working, and step 10 undoes it.**

**10. §6.3 step 3 runs, and `A` completes it alone.** The freeze puts `A` into
§6.3. Step 2 broadcasts a `DISPUTE`; step 3 has every peer *"publish a fresh
`STATE_HASH` for the disputed checkpoint at a new `sequence`"* in §4.9's
reconciliation stage. That stage's **required emitter set is the emitter set of
the checkpoint it re-derives** — checkpoint 8's, which is `P(k) = {A}`. `A` emits
its own reconciliation `STATE_HASH` at `BOUNDARY_CHECKPOINT_BASE + 2`. The stage
is complete over its required signer set in the step that emits it, and *"every
value in it agrees"* — there is one value and it is `A`'s.

**T53 fires.** Guard satisfied. `Diverged` → the phase held. `resume; nothing was
opened and no chips moved while frozen`. And **T53 clears
`solitary_contradicted`** — I33(c) is explicit that it is the only thing that
does.

**11. `A` resumes and finishes the tournament alone.** It plays out §12.1.2's ~40
drain hands, `B`'s and `C`'s stacks reach zero, §9.3 condition 1 fires, `A` enters
`TableClosed` naming itself. `B` does the same. Both hold a private tournament
win. The only difference from the pre-fix trace is that each transcript now
carries two `Fault` records and one aborted hand.

**Does a later contradiction re-freeze it?** Only by luck. After T53 resumes `A`,
the next contradicting event to arrive re-freezes it — and `B` is still emitting,
so one will. But each freeze is released by the same self-completing reconciliation
stage, at local-computation speed, and each release is followed by more drain
hands. The race is between `B`'s events arriving and `A`'s ~40 hands completing,
and nothing in the corpus biases it: §12.1.2 puts the forty hands at *"about five
minutes"* only because it budgets `hand_delay_sec` per hand as a GUI figure, and
§8.3 and `STATE_MACHINE.md` §2 both say the protocol never waits for it.

**And the non-latching variant is worse.** If `B`'s `HAND_INIT(k+1)` copy is among
the dropped frames — a lossy link is K1's premise — step 9 never happens and only
T50 fires. **T50 sets no latch**: `solitary_contradicted` is set by T62 and T63
only (`STATE_MACHINE.md` l. 341, §9.3's note, I33(c)). So §9.3 condition 0.6 is
never reached, and:

* T50 → `Diverged`, hand `k+1` frozen mid-flight, no chips moved;
* §6.3 step 3's reconciliation stage self-completes at `A` → **T53** → resume;
  or, if no reconciliation runs at all, T57/T61 at `hand_deadline_ms` → aborted →
  T46 → `HandComplete` → T47 with no gate → hand `k+2`;
* hand `k+2` is solitary, places checkpoint 8, `B`'s copy for `k+1` arrives,
  mismatches, T50 again.

Either the table loops with no progress and no chip movement forever, or it
resumes and drains. **In both branches `I33(c)`'s assertion passes throughout** —
*"no trace contains two `HandComplete` entries with the latch set and a hand dealt
between them"* — because the latch is never set on this route. That is J2's fixed
point reached through the mechanism K-3 added, and the invariant written to
exclude it does not see it.

### 2.3 The exact sentence to change

Two, and neither is a decision the owners have not already made in principle:

1. **`PROTOCOL.md` §4.9 — the reconciliation stage needs a cardinality floor, or
   an explicit statement that it has none and why.** A reconciliation round whose
   required emitter set has one member re-derives nothing: it is one peer
   confirming its own value, which is precisely the quantity P3 refused. The
   natural rule is the one §6.3 already implies — *a reconciliation stage whose
   required emitter set is `{self}` alone resolves nothing; it neither completes
   nor releases a freeze* — with the disposition for the frozen peer being the same
   timer-borne exit T61 already provides.
2. **`STATE_MACHINE.md` T50 and T53 — the latch must cover both freeze routes.**
   Either T50 sets `solitary_contradicted` when the diverging checkpoint is
   checkpoint 8 and this peer's `solitary_since` covers the hand it names, or T53
   acquires the conjunct T62's guard already has. As it stands the two freeze
   routes have two different memories and only one of them is remembered.

---

# Part 3 — The load-bearing check

For each fix made in this pass, the rule it newly makes load-bearing, and whether
that rule holds. This has caught the worst defect in five consecutive passes; it
catches it again, and it catches it in the other owner's document for the second
time running.

| # | Fix | Rule it newly makes load-bearing | Holds? |
|---|---|---|---|
| 1 | **§6.2 row 8 + §4.9's box** — checkpoint 8 exists on the wire on every hand | **§6.3 step 3's reconciliation stage**, which now has a stage to chain from at every boundary of every hand shape, including a solitary one. `STATE_MACHINE.md` §5.2 says so in terms: *"this exit exists **because of K-3**"* | **NO — N1.** Its required emitter set is the checkpoint's, so at a solitary peer it is `{self}` and self-completes. T53 releases the freeze the same pass installed |
| 2 | **§6.1's `signed_this_hand`** — checkpoint 8 becomes a comparison of the participation set | **T50's non-latching freeze.** Before this pass a checkpoint mismatch was a state disagreement about cards and chips; it is now also the primary detector for a forked `P`, on a path where the correct disposition is the *latched* freeze | **NO — N1, second half.** T50 is reached and T62 is not, and only T62 and T63 set the latch |
| 3 | **§4.0 step 10b** — a stale-hand chained event is evaluated | **§4.0 step 10a's behaviour for a finished hand.** Step 10a runs first and indexes a per-hand store | **NO — N2.** §5.3 drops that store at hand end. Undefined, and one of the two readings is an unbounded allocation |
| 4 | **§4.9's close condition** — the checkpoint-8 window closes at `HAND_INIT(k+1)` | **K-3b's readmission route** (`STATE_MACHINE.md` §5.2: *"a seat returning from silence needs no `PLAYER_SIT_IN` at all"*) | **NO — N5.** Outside the window step 10b forbids the event from counting into any `P`, and the window is one round trip wide at a healthy table and zero at a solitary one |
| 5 | **§4.10's D-014 clause on `PLAYER_SIT_IN`; §4.9's `kind = 3` box** | **§4.0's normative anti-eviction box**, which binds every document and every layer | **NO — N3.** It forbids unseating on *"a protocol proof … or a failed verification"*, which is D-014 tier 1 verbatim. §11 records the exception; §4.0 does not |
| 6 | **`solitary_since`** replacing the retained per-hand map | **`PROTOCOL.md` §5.3's `RetainedHand.was_solitary`**, which answers the same past-tense question with a different structure | **NO — N4.** `solitary_since` is cleared to `None` when the regime is left; the per-hand record is not. After a re-entry the two halves give opposite answers for the same event |
| 7 | **§5.3's band exemptions (L5)** | **§5.3's occupancy bound**, which now has to count two bands above `MAX_STAGES_PER_HAND` | **YES.** Recomputed in the same paragraph, with the old bound recorded as short by exactly those entries |
| 8 | **§4.9's `BOUNDARY_CHECKPOINT_BASE = 8 192`** | **§5.2.1's key disjointness** — the band must not collide with the boundary window when a reconciliation round extends it | **YES.** `8 192 … 8 207` vs `4 096 … 4 105` vs stage indices `< 2 048`, with `r <= 7` bounding the extension and the reason for choosing above rather than below written out |
| 9 | **§6.1's `signed_this_hand` appended last** | **§2.2 rule 5's append-only field order** | **YES.** Stated at the field and again in the wire-change paragraph, on the same footing as the `absent` deletion |
| 10 | **§6.2's "where one was placed" (L9)** | **The property the gate protects** — *no card opens while peers disagree* — must survive a street that placed no checkpoint | **YES.** Argued rather than asserted: a street that placed no checkpoint produced no disagreement to wait on |

**Six of ten fail, and five of the six sit across an owner boundary.** The
previous pass's finding was that *"the path a fix newly makes load-bearing is most
often in the other owner's document, because that is the path the fixing author
did not have open."* This pass both confirms it and refines it: **rows 1, 2 and 6
are not a fix reaching into the other document — they are two documents each
correctly implementing one half of one rule, in structures that are not the same
structure.** §4.9 gives the reconciliation stage a set; T53 reads a set; neither
asks whether the set can have one member. §5.3 keeps a per-hand record; §2.6 keeps
an interval; neither states which is canonical. That is a new shape of the same
failure and it is worth naming: **a rule split across two owners needs one of them
declared canonical for the *representation*, not only for the rule** — which is
D-011 rule 2's discipline (the slot key as one literal tuple) applied to a
quantity that has not yet been given it.

---

# Part 4 — The healthy-table check

Four players, twenty hands, nothing goes wrong, every peer answers in one round
trip. This is the check L2 showed nobody had run.

### 4.1 Does any deadline fire?

**No deadline that ends a hand or a stage fires, on any of the twenty hands.**
Walked against §8.2's `next_deadline_ms` table and the deadline rows of §5.2:

| Deadline | Value | Armed for | Fires? |
|---|---:|---|---|
| betting action | `action_timeout_ms + action_grace_ms` = 25 000 | each `ACTION_*` | no — the human acts, or the table is not "healthy" |
| cryptographic step | `crypto_step_timeout_ms` = 30 000 | `DECK_INIT`, `SHUFFLE_*`, `DECK_COMMIT`, `DEAL_PRIVATE`, `BOARD_REVEAL`, `SHOWDOWN_*` | no — one round trip plus ≈42 ms of Bayer–Groth proving per shuffler |
| `STATE_HASH` / `STATE_ACK` | `crypto_step_timeout_ms` = 30 000 | every checkpoint, 8 included | no |
| hand boundary | `hand_delay_ms + crypto_step_timeout_ms` = 37 000 | `HAND_INIT` | no |
| whole hand | `hand_deadline_ms` = 600 000 from `TERMINAL(k-1)` | T57, T61 | no |

**The gate L2 was about discharges in one round trip.** T47's settled-path gate is
`checkpoint.heard ⊇ checkpoint.required` with `required = signed_this_hand` read at
T45 — all four seats, each of which has just signed `HAND_COMPLETE` and is
therefore alive. `STATE_MACHINE.md` §8.3 states the consequence correctly and
states that it must not be implemented as a delay: *"the wait is on a collective
stage of chain content, it ends the moment the last required copy arrives — which
on a healthy table is one round trip, far inside the seven seconds a GUI is
animating anyway."* Twenty hands cost twenty extra boundary round trips and zero
deadline expiries. **L2's 600 000 ms per two hands is gone.**

### 4.2 Does every hand place a checkpoint?

**Yes, and on every hand shape.** Per shape, from §6.2's eight rows and
`STATE_MACHINE.md` §5.2:

| Hand shape | Checkpoints placed | Wire-carriable |
|---|---|---|
| Full hand to showdown | 2, 3, 4, 5, 6, 7, **8** | all |
| Fold-out (T30) | 2, 3, **8** | all |
| All-in run-out (T38) | 2, 3, 7, **8** — rows 4–6 do not fire, and **L9's amendment is what lets the turn and the river open at all** | all |
| Drain hand (one dealt-in seat) | **8** only — §5.3 step 9 goes straight to `Settling` | yes |
| Aborted hand (stalled `HAND_INIT`) | **8** only, at T46 | yes |

Over twenty healthy four-handed hands, the mix is dominated by fold-outs and
full hands; every one of them places at least three checkpoints and every one of
them places checkpoint 8. `STATE_MACHINE.md` §10's I32 asserts it, and §12.1's
harness note asserts it over the forty-hand drain as well.

### 4.3 How long does a hand take?

Counting stages that need a round trip, at four seats, one betting round per
street with no re-raises:

| Segment | Collective/single stages | Round trips |
|---|---:|---:|
| boundary: checkpoint 8 `STATE_HASH` + `STATE_ACK`, then `HAND_INIT(k+1)` | 3 | 3 |
| key setup and deck: `DECK_INIT`, 4 × (`SHUFFLE_STEP` + `SHUFFLE_PROOF`), `DECK_COMMIT` | 10 | 10 |
| deal and checkpoint 2 | 3 | 3 |
| pre-flop betting (4 actions) + checkpoint 3 | 6 | 6 |
| flop / turn / river: 3 × (`BOARD_REVEAL` + ~3 actions + checkpoint pair) | 18 | 18 |
| showdown: `SHOWDOWN_REVEAL`, checkpoint 7 pair, `HAND_COMPLETE` | 4 | 4 |
| **total** | **≈44** | **≈44** |

At a 50 ms round trip that is ≈2.2 s of protocol, plus 4 × ≈42 ms of shuffle
proving ≈0.17 s [MENTAL §5.1], plus one Ed25519 verification per event on the
receive path, for which **no figure has been measured** (§4.0 says so; it is one
of `SPEC_CS.md` §33's seven Phase 4 profiling targets). Against that, one human
betting decision is budgeted at 20 s. **The protocol is not the cost of a hand and
checkpoint 8 has not made it one**: it adds three of the forty-four round trips,
inside a `hand_delay_ms` of 7 000 that the GUI is spending on animation anyway.

### 4.4 What the healthy table does surface

**The checkpoint-8 `STATE_ACK` stage has no gate and a window that can close under
it — N6.** T47 is gated on the `STATE_HASH` stage only. Each peer emits its
checkpoint-8 `STATE_ACK` and its `HAND_INIT(k+1)` copy on the same trigger, and
§4.9's window closes *"at this receiver's acceptance of a complete
`HAND_INIT(k+1)` stage"*. The ACK stage completes before the window closes only
because per-peer FIFO ordering on the table mesh delivers each peer's ACK before
its `HAND_INIT` copy, and **nothing in the corpus requires that ordering**. Where
it does not hold, §4.0 step 10b drops the late `STATE_ACK` — it names only the
`STATE_HASH` as compared — and the *"we all agreed here"* point §6.2 says a later
dispute can name is silently never placed. Nothing on the healthy path notices,
which is what makes it worth filing: the one consumer that would notice is
D-014's tier-2 precondition, which requires *"a completed `STATE_ACK` stage"*.

---

# Part 5 — The D-014 sweep

D-014 is one pass old. `DECISIONS.md` D-014-1 records the integration as *"not
started"*; it is now substantially started and the row is stale.

### 5.1 Which documents carry it

| Document | D-014 occurrences | Carried where | Verdict |
|---|---:|---|---|
| `STATE_MACHINE.md` | **54** | §2.4 `SeatStatus::Removed`, T64/T65/T66, I34, §5.2's D-014 box, §12.1's rows 1, 3, 4, 19 | **complete**, and it is the fullest integration in the corpus |
| `THREAT_MODEL.md` | **42** | §5.1's D&A definition split by tier, §5.2's re-classification table, **X37**, §5.5's tier column and the mirror gate, §9.2's coverage rows | **complete**, and X37 is the best row in the pass |
| `PROTOCOL.md` | **32** | §4.9's `kind = 3 CHEAT_EVIDENCE` box and the `kind` register, §4.10's `cause = 6` and the `PLAYER_SIT_IN` bar, §5.2's `cause = 5` note, §11's row and its exception paragraph | **complete except §4.0 — N3** |
| `DECISIONS.md` | 12 | D-014 itself, D-014-1/2/3 | — |
| `NETWORK_STACK.md` | **0** | — | **defensible, unrecorded — N9** |
| `CRYPTOGRAPHY.md` | **0** | — | **not defensible — N9** |
| `CONTRIBUTING.md` | **0** | — | **L8** |
| `DEPENDENCIES.md` | **0** | — | **L8** |
| `SPEC_CS.md` | **0** | — | correct; it is the source, not a consumer |

### 5.2 Is anything inconsistent with the two tiers?

**The tier boundary is held everywhere it is stated.** Checked at five sites:

* `PROTOCOL.md` §4.9's box: tier 2 *"removes its subject only when this receiver
  holds a **completed `STATE_ACK` stage** for a checkpoint of the same chain whose
  `sequence` is at or before the offending event's, and whose emitter set contained
  **both the accused and this receiver**"*; before that, *"voids the hand and
  removes nobody"*. The `STATE_ACK`-not-`STATE_HASH` choice is argued from K1's own
  divergence and is the right argument.
* `STATE_MACHINE.md` T64's guard carries the precondition as a conjunct —
  `tier == SelfContained ∨ (tier == StateDependent ∧ judged_at_checkpoint ==
  Some(n) ∧ checkpoint n reached agreed.is_some())` — rather than as prose, which
  is what makes it testable. `THREAT_MODEL.md` X37 says so and says why.
* `THREAT_MODEL.md` §5.1 gives the classification rule an editor needs: *"can two
  honest receivers of this evidence disagree about the verdict? If yes, there is no
  removal, whatever the attack costs."*
* Equivocation is excluded by name at three sites (`PROTOCOL.md` §5.2's
  `cause = 5` note, §5.1, X37's row 16) with the reason each time.
* `PROTOCOL.md` §4.10 `cause = 6`: *"the offender, and **still evidence only** —
  the removal is derived from the signature on the evidence, never from this
  field"*.

**The "must name a message the accused signed" rule holds, and it is checked in
the strongest place.** §4.9 states that the removal's inputs are exactly two — *"a
signature that verifies under the accused's key"* and, for tier 2, *"a checkpoint
the accused signed"* — and that **`n(1) attributed` is not read**, so D-010 point 2
survives D-014 unamended. `DISPUTE { kind = 3 }` carries **exactly one**
`SignedEvent`, with the reason given: *"a finding that needs a second event to be
decidable is not self-authenticating and is not a D-014 finding."* No new message
type; no removal message, and *"none may be added"*.

### 5.3 What is inconsistent

* **N3 — `PROTOCOL.md` §4.0.** The step table's own footnote still reads
  *"**Nothing further follows automatically (D-010).**"*, and the normative box
  three paragraphs below still reads *"**No document in this corpus may block,
  evict, unseat, refuse or allow-list a peer on the strength of a protocol proof,
  an attribution, a fault record or a failed verification.**"* D-014 tier 1 is
  precisely a removal on a failed verification (step 9) and on a failed proof
  (step 14). §11 records the exception — *"the one row that ends in a seat being
  taken is D-014's"* — and §4.0, which is where a receiver implementer reads the
  rule, does not.
* **N9 — `CRYPTOGRAPHY.md`.** Tier 1 names *"a failed shuffle proof, a failed
  decryption-share proof, a failed key-ownership proof"* — three verifiers this
  document owns. X37 names the risk in the same words: *"a proof verifier stricter
  than the prover"* turns an honest message into tier-1 evidence, and tier 1 has no
  checkpoint to wait for. `CRYPTOGRAPHY.md` also carries the corpus's one standing
  soundness caveat (Phase 0 verified thirteen attacks, which is not soundness).
  Zero occurrences of D-014 is a substantive gap, not an editorial one.
* **A narrowing made by an owner rather than by the decision.**
  `STATE_MACHINE.md` T66 refuses every removal in the setup chain — *"no seat is
  removed"*, because `roster_hash(k)`'s seat vector is frozen at `TABLE_READY` and
  removing a seat there forks the genesis. The reasoning is right and the row is
  right. But D-014 says a player who sends a provably illegal message is removed,
  with no phase exception, and a seat-RNG commitment mismatch is tier 1 by D-014's
  own list. **The exception belongs in D-014**, one line, rather than only in the
  transition that implements it.
* **A dangling reference in the decision itself.** D-014 says the information
  window is *"`SPEC_CS.md` §22's"*. `SPEC_CS.md` §22 defines no information window;
  the nearest hook is the table window's *protocol/security status* element. The
  content is carried correctly by `STATE_MACHINE.md` T64 (*"`Effect::Fault` … the
  record carries the subject, the tier and the evidence hash, which is exactly the
  three things D-014 says the window must name"*, plus the void statement asserted
  by I27), so nothing is lost — but the citation points at a section that does not
  contain the thing cited. **N8.**
* **D-014-3 is open and correctly recorded as blocking.** `PublicTableState` has
  no `removed` vector and hashes none, so `SeatStatus::Removed` is canonical
  per-seat state that no vector hashes. `PROTOCOL.md` §4.9 answers half of it —
  the removal reaches canonical state at `HAND_INIT(k+1)`'s byte-identical body,
  and a disagreement stalls stage 0 — which is a real answer and is the same
  disposition §4.10 and §4.9 take elsewhere. The other half, the wire
  representation, is open, is `PROTOCOL.md`'s, and is on the list. That is the
  process rule working and it is not filed again here.

---

# Part 6 — Coverage table

Occurrences per document, `grep -o … | wc -l`, 2026-08-28.

| Document | D-009 | D-010 | D-011 | D-012 | D-013 | D-014 |
|---|---:|---:|---:|---:|---:|---:|
| `PROTOCOL.md` | 22 | 65 | 49 | 15 | 37 | 32 |
| `STATE_MACHINE.md` | 28 | 85 | 35 | 34 | 80 | 54 |
| `THREAT_MODEL.md` | 37 | 127 | 104 | 39 | 33 | 42 |
| `NETWORK_STACK.md` | 5 | 25 | 35 | 25 | 21 | **0** |
| `CRYPTOGRAPHY.md` | 18 | 38 | 29 | 19 | 14 | **0** |
| `CONTRIBUTING.md` | 12 | 11 | 11 | 8 | **0** | **0** |
| `DEPENDENCIES.md` | 11 | 4 | 8 | 10 | **0** | **0** |
| (`SPEC_CS.md`) | 0 | 0 | 0 | 0 | 0 | 0 |
| (`DECISIONS.md`) | 6 | 21 | 11 | 15 | 19 | 12 |

**Six zeros across the seven specification documents, in two classes.**

* **D-013's two zeros are unchanged from last pass** (`CONTRIBUTING.md`,
  `DEPENDENCIES.md`) and are **L8**, still open. Both documents carry a dated sweep
  record that stops at D-012 and both cite the process rule that says it must not.
* **D-014's four zeros split.** `CONTRIBUTING.md` and `DEPENDENCIES.md` are the
  same L8 row one decision later — D-014 has no crate consequence and no
  review-process consequence, and one row each saying so discharges it.
  `NETWORK_STACK.md` has a defensible answer it does not give: D-014's removal is
  from the **table**, never from the transport, so §0.1's table row *"A protocol
  proof, verdict or attribution … **forbidden.** No transport consequence of any
  kind"* is unaffected and D-011 rule 3 is untouched. One row. **`CRYPTOGRAPHY.md`
  does not have a defensible answer** — see N9.
* **`SPEC_CS.md`'s row of zeros is correct and is not a gap.** It is the owner's
  binding requirement document and the source the decisions answer to; a decision
  reference inside it would invert the authority order.

D-012 rose in `PROTOCOL.md` 15 → 15, `STATE_MACHINE.md` 28 → 34,
`THREAT_MODEL.md` 33 → 39 and `DECISIONS.md` 11 → 15, which is D-014-3 being
argued as a D-012 question rather than as a packaging question. That is the
process rule working in the direction it was written for.

---

# Part 7 — New defects

Concentrated on what the four edits made load-bearing. Numbered `N` to avoid
collision with `DECISIONS.md`'s open `M-1`.

| # | Severity | Defect |
|---|---|---|
| **N1** | **BLOCKER** | **A solitary peer completes its own reconciliation stage and releases its own freeze.** `PROTOCOL.md` §4.9: a reconciliation `STATE_HASH` stage's *"required emitter set is the emitter set of that checkpoint"*; checkpoint 8's is `P(k)`, which in the solitary regime is `{self}`. `STATE_MACHINE.md` **T53** fires when *"the reconciliation-round stage … is complete over its required signer set ∧ every value in it agrees"* — satisfied in the step the peer emits its own copy — and **T53 clears `solitary_contradicted`, which I33(c) says is the only thing that does**. The freeze K-9 installed is released by the exit K-9 named, without any other seat speaking, and §9.3 condition 0.6 is never reached. Compounding it: the checkpoint-8 mismatch route reaches **T50**, which **sets no latch at all**, so on the interleaving where `B`'s `HAND_INIT` copy is also dropped the table freezes and thaws indefinitely with I33(c) passing throughout. Owner: `PROTOCOL.md` §4.9 (a cardinality floor on the reconciliation stage, or a statement that there is none and why) **and** `STATE_MACHINE.md` T50/T53 (one latch, both routes). Part 2.3 gives both edits. |
| **N2** | **BLOCKER for the receiver** | **§4.0 step 10a is undefined for a stale-hand chained event, and step 10a runs before step 10b.** §5.3's `event_class == 0` store is *"one map per table per hand"*, and *"finished hands keep only `TERMINAL(k)`, the terminal body … and the retained hand record"*. A chained event naming a finished hand therefore reaches step 10a with no store to be looked up in. Reading (a), allocate a map for the named hand, is an **attacker-driven unbounded allocation** keyed on a `u64` the sender chooses, in the section whose first sentence is *"anti-replay must not become a memory-exhaustion vector"*. Reading (b), treat an absent store as no entry and fall through, is correct and unstated — and it also means a stale-hand event is never de-duplicated, so one contradicting event replayed re-enters §6.3 step 1 on every copy. One sentence on step 10a or in §5.3: *a chained event naming a finished hand occupies no slot; step 10a is skipped for it and step 10b is its whole anti-replay rule, bounded by the retained record's own idempotence.* Owner: `PROTOCOL.md` §4.0, §5.3. |
| **N3** | medium | **`PROTOCOL.md` §4.0 contradicts D-014 and its own §4.9.** The footnote *"**Nothing further follows automatically (D-010).**"* and the normative box *"No document in this corpus may block, evict, **unseat**, refuse or allow-list a peer on the strength of a protocol proof, an attribution, a fault record or a **failed verification**"* both predate D-014 and both are now false for tier 1 — which is a removal on a failed signature verification (step 9) and on a failed proof (step 14). §11 carries the exception in full and says it is *"stated here rather than hidden"*; §4.0 is where a receiver implementer reads the rule, and it says the opposite. The box should keep its force and name the one exception by reference, exactly as §11 does. Owner: `PROTOCOL.md` §4.0. |
| **N4** | medium | **The past-tense regime test has two representations and they are not equivalent.** `PROTOCOL.md` §5.3 keeps `RetainedHand { hand_id, was_solitary, p, checkpoint8_state_hash }` per finished hand, LRU-capped at 4 096. `STATE_MACHINE.md` §5.3 step 8 keeps `solitary_since: Option<u64>` and sets it to **`None`** whenever a hand init reads `\|signed_this_hand\| > 1`. So after a re-entry — an accepted `PLAYER_SIT_IN`, or an accepted out-of-set checkpoint-8 `STATE_HASH` — a late chained event of an *earlier* solitary hand makes step 10b route to step 12a and emit `SolitaryDivergence`, while T62's guard `solitary_since == Some(j)` fails and no row consumes the event. The wire says freeze; the engine says reject. `DECISIONS.md` K-9 argues the interval property makes one `u64` sufficient — *"inside it, `P` can only grow through an accepted `0x0804 PLAYER_SIT_IN`, so the solitary hands are an interval"* — which is true of the hands **inside** the regime and says nothing about a record of a regime that has been **left**. Owner: `STATE_MACHINE.md` §2.6 and `PROTOCOL.md` §5.3 — declare one representation canonical, as D-011 rule 2 does for the slot key. |
| **N5** | medium | **K-3b's readmission route does not survive step 10b.** `STATE_MACHINE.md` §5.2: *"A seat returning from silence needs no `PLAYER_SIT_IN` at all … its own checkpoint-8 `STATE_HASH` is a chained event that T49 accepts — from any seat, in-set or not — so accepting it adds the seat to `signed_this_hand`."* `PROTOCOL.md` §4.0 step 10b: a stale-hand event *"is **never applied**, never enters a `stage_hash`, **never counts into any `P`**"*. The two are consistent only inside §4.9's checkpoint-8 window, which closes at acceptance of a complete `HAND_INIT(k+1)` — **one round trip at a healthy table and zero at a solitary peer**, whose `HAND_INIT(k+1)` is required of `{self}` and self-completes. `PROTOCOL.md` §4.10's *"a seat rejoins by signing a chained event … that is the entire test and there is no other route"* has the same problem, because the boundary window closes on the same event. So D-013's readmission promise holds only for a seat whose message beats the next hand's stage 0, and it fails hardest in the regime D-013 made a steady state. Owner: `PROTOCOL.md` §4.0 step 10b and §4.9's close condition — a stale-hand checkpoint-8 `STATE_HASH` that **agrees** could safely count into `P` without weakening anything, since agreement there is agreement about the participation set (§6.1). |
| **N6** | medium | **The checkpoint-8 `STATE_ACK` stage is gated by nothing and its window closes on an event emitted concurrently with it.** T47 reads only the `STATE_HASH` stage. Each peer emits its checkpoint-8 `STATE_ACK` and its `HAND_INIT(k+1)` copy on the same trigger, and §4.9's window closes at acceptance of a **complete** `HAND_INIT(k+1)`. The ACK stage completes first only by per-peer FIFO delivery, which no document requires; where it does not, step 10b drops the late ACK — it names only the `STATE_HASH` as compared — and §6.2's *"definite, chained 'we all agreed here' point that a later dispute can name"* is never placed at the boundary. Its one consumer is D-014's tier-2 precondition, which requires *"a completed `STATE_ACK` stage"*. Owner: `PROTOCOL.md` §4.9 — either state the emission order, or close the window on the ACK stage rather than on `HAND_INIT(k+1)`. |
| **N7** | low | **Step 10b tests membership of the wrong set by its own text.** §3.2's rule and §4.0 step 12 both say *"a chained event of hand `k` from a seat outside **`P(k-1)`**"*. Step 10b says *"`sender_seat` is outside the recorded `P`"*, and §5.3's record holds `p: SeatSet, // P(hand_id) as this receiver derived it` — that is `P(k)`, not `P(k-1)`. The two coincide in the pure solitary case and diverge once an exempt event has grown `P(k)`. An implementer writing step 10b from §4.0 alone tests the later set. One word in §5.3's struct comment or in step 10b. Owner: `PROTOCOL.md` §4.0, §5.3. |
| **N8** | low | **D-014 cites an information window `SPEC_CS.md` §22 does not contain.** §22 is the GUI section; its nearest element is the table window's *protocol/security status*. The content is carried correctly by `STATE_MACHINE.md` T64 through `Effect::Fault` and I27, so nothing is lost — but a binding decision points at a section that does not hold the thing it points at, and the next editor will look for it. Owner: `DECISIONS.md` D-014 — cite §22's status element, or state that the window is new and specified by T64's record. |
| **N9** | low | **`CRYPTOGRAPHY.md` carries zero D-014 while owning three of tier 1's six clauses.** Tier 1 names a failed shuffle proof, a failed decryption-share proof and a failed key-ownership proof; §8's verification rules and §6.5's target table are where those verifiers live, and `THREAT_MODEL.md` X37 names *"a proof verifier stricter than the prover"* as the mechanism that turns an honest message into tier-1 evidence with no checkpoint to wait for. `NETWORK_STACK.md`'s zero is defensible and unrecorded (D-014 has no transport consequence; §0.1's forbidden row is unaffected). This is L8's class with two more documents in it, and one of the two is substantive. Owner: `CRYPTOGRAPHY.md` §8 and §0; `NETWORK_STACK.md` §0.1 — one row. |

**The pattern check, run on this pass's own findings.** Six of the nine are on a
path a fix in this pass newly made load-bearing (N1, N2, N3, N4, N5, N6), and two
of the six — N1 and N4 — are the new shape Part 3 names: **not a fix reaching
across an owner boundary, but two owners implementing one rule in two structures
neither declares canonical.** N1 is the more serious of the two because the
structures are a *set* and a *guard over that set*, and the guard was never asked
whether the set can have one member — which is the same question §3.2 spent this
pass answering about `P` itself, asked one level out.

---

# Part 8 — The verdict

## Is `PROTOCOL.md` implementable, per build-order item?

| # | Item | Implementable today? | Gap |
|---:|---|---|---|
| 1 | `EventBody` and `SignedEvent`, §2.3's twelve and two fields | **Yes.** Reference struct compiles; three nesting levels with field indices; nothing open | — |
| 2 | `EventType` as a closed `#[repr(u16)]` enum, 39 codes, with `chain_scope()` / `event_class()` / `channel()` / `stage_kind()` | **Yes.** §4.11 is a complete index, all 39 rows present, `0xF000..=0xFFFF` refused. Already written to this point in `src/protocol/messages.rs` | — |
| 3 | Envelope sentinel checks for `chain_scope == 0` | **Yes, off §2.3's exhaustive thirteen-type list.** **Do not build it off §4.11's prose**: §4.11 l. 3808 still says *"`DISPUTE` is the one table-mesh message with `chain_scope = 0`"* while its own table lists four — `HELLO`, `CAPABILITIES`, `PLAYER_LIST`, `DISPUTE`. That is `M-1` in `DECISIONS.md`'s open list, it is still open, and it is the exact sentence this item would be written from | `M-1` — §4.11 prose |
| 4 | The 39 payload structs with §9.3 caps and §9.4 bounds as associated `const`s | **Yes.** Every type has a cap and every `Vec` and string a hard maximum. `DISPUTE`'s `n(5)`/`n(6)` appended per §2.2 rule 5. `DISPUTE { kind = 3 }` needs **exactly one** entry in `n(3) evidence` (§4.9) — a bound, not a cap, and it is stated | — |
| 5 | `slot(E)`, one function, §5.2.1's 8-tuple with `subject(E)` | **Yes.** One site, literal tuple with envelope field indices, four components declared deliberately absent with a reason each, and a normative sentence that no document may add one | — |
| 6 | §4.0 steps 1–11 as a `validate_prefix` pipeline | **Yes for 1–10 and 11. Step 10b is now in this range and it is not fully specified — N2.** Steps 1–10a and 11 are pinned. Step 10b's *reading* of the retained record is pinned; what is not pinned is what step **10a** does when the named hand's store no longer exists, and 10a runs first. Also N7: step 10b's set is `P(hand_id)` where §3.2 says `P(k-1)` | **N2**, **N7** |
| 7 | A `constants` module from §13 | **Yes.** `BOUNDARY_SEQUENCE_BASE`, `BOUNDARY_CHECKPOINT_BASE`, `MAX_RETAINED_HAND_RECORDS` all present with their bands and their exemption; `DOMAIN_EVENT`'s 24 bytes printed twice | — |
| 8 | Round-trip and canonicality tests over every payload type, plus §2.7's determinism cases | **Yes.** §2.5's gate and §2.7's cases are complete and independent of everything above | — |

**Named gaps by section:** §4.11's `chain_scope` prose (`M-1`); §4.0 step 10a and
§5.3's store lifetime (**N2**); §4.0 step 10b's set and §5.3's `RetainedHand.p`
(**N7**); §4.0's anti-eviction box (**N3**, editorial for `messages.rs`,
normative for anything that acts on a validator failure).

**Everything else `messages.rs` needs is pinned and was pinned last pass:** the
three nesting levels; `TO_BE_SIGNED` with its hex printed twice; the canonicality
gate; `h` with the exact `derive_key` call and the per-part 8-byte big-endian
length prefix; the closed 17-row domain register; all 39 codes with their four
axes; a per-type payload cap for every type; a hard maximum for every collection;
the slot key as one tuple; and every two-sided constant in one place.

## What in the deferred list has become unblocked

| Deferred last pass | Now |
|---|---|
| **`PublicTableState` and `state_hash`** — blocked on L2 | **UNBLOCKED.** §6.1's field list is settled and `signed_this_hand` is in it, appended last. Write the struct and the hash. The one thing that would change it again is D-014-3's `removed` vector, which is **open and named as blocking for D-014**, so write `PublicTableState` now and expect one append-only field if D-014-3 lands that way |
| **The `checkpoint` field's range** — leave `u16` unrange-checked | **UNBLOCKED, and now range-checkable to `1..=8`.** §6.2's table is eight rows under *"at these points and no others"*. The reconciliation stage carries the number of the checkpoint it re-derives, so it needs no ninth value |
| **The anti-replay stores of §5.3** — blocked on L4 and L5 | **PARTIALLY UNBLOCKED.** L5 is closed: both bands are exempt from the stage-count abort and the occupancy bound counts them. The three stores and their indices are implementable now. **The store's lifetime is not** — N2 — so write the stores and leave the finished-hand path behind one named `TODO` referencing N2 rather than guessing |
| **The retained hand record** — did not exist | **UNBLOCKED.** `RetainedHand`'s four fields, `MAX_RETAINED_HAND_RECORDS = 4 096`, LRU, ≈56 B each. Write it with item 6; it is read by step 10b and nothing else |
| **§4.0 steps 12, 12a** | **STILL DEFERRED**, and now for a better reason. Step 12's stage-legality half is fully specified; step 12a's disposition is specified; what is not settled is N1 — whether a freeze that step 12a produces survives the next reconciliation stage. Build the `PrefixOutcome` enum with a `SolitaryDivergence` variant and wire no release path |
| **§4.0 steps 13, 14, 15** | **STILL DEFERRED.** They need the engine, as before |
| **`TIMEOUT_VOTE`, `TIMEOUT_CERT`, `EquivocationProof`** | **UNCHANGED.** `OQ-F` may delete them; write the payload structs, wire no behaviour |
| **`SHOWDOWN_MUCK` policy** | **UNCHANGED.** `Q-01`; the payload and cap are defined, the policy gate is not this module's |

## What must land before the receiver half is written

Two edits, both of them corrections to fixes made in this pass rather than new
decisions:

1. **`PROTOCOL.md` §4.9 and `STATE_MACHINE.md` T50/T53 — N1.** A reconciliation
   stage whose required emitter set has one member resolves nothing, and both
   freeze routes must reach the same latch. Without this the receiver can be built
   and K1's payoff returns.
2. **`PROTOCOL.md` §4.0 and §5.3 — N2.** One sentence saying a chained event
   naming a finished hand occupies no slot and skips step 10a. Without it an
   implementer picks between an unbounded allocation and an unstated fall-through.

Three more are one line each and should land with them: **N3** (§4.0's box names
D-014's exception), **N7** (step 10b's set), **M-1** (§4.11's prose). **N4**,
**N5** and **N6** are the next pass's, and **N5** is the one most likely to be
this list's successor: it is a promise D-013 makes that step 10b quietly took
away, which is the same shape as L4 and in the opposite direction.
