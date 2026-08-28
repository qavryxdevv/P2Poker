# PHASE3_GATE2.md — the wire-specification gate, second pass

**Commission.** Nine passes have run. The eighth (`PHASE3_GATE.md`) closed J1–J7
completely and left three blockers, K1–K3, plus K4–K7 and the two cross-owner
carries J-4 and J-5. `DECISIONS.md`'s open list now records K-1, K-2, K-5 and K-6
as done and opens K-8 and K-9. What is gated here is `PROTOCOL.md`, because
`src/protocol/messages.rs` — the envelope and all 39 payloads — is written against
it next, and it is today a two-line stub.

**Method, and it is the same inversion that produced the last three passes'
blockers.** Every verdict is quoted from the document as it now stands. No
specification document was edited. The three sweeps were run **against the
constructions**, not against the corpus's own sweep tables: the boundary-window box
was read before §4.11's rows, the checkpoint set was enumerated per *hand shape*
before §6.2's table was read, and the twenty phases were walked from
`STATE_MACHINE.md` §5.2's rows before §12.1.1's summary. That inversion produced
this pass's blocker, and it produced it in the place the pattern predicts:
**inside the fix K-3 landed, on the wire the fix assumed and did not check.**

**Files as read, 2026-08-28.** `STATE_MACHINE.md` 18:04, `PROTOCOL.md` 17:58,
`CRYPTOGRAPHY.md` 17:57, `DECISIONS.md` 17:54, `NETWORK_STACK.md` 17:52,
`THREAT_MODEL.md` 17:48, `CONTRIBUTING.md` 16:03, `DEPENDENCIES.md` 16:03,
`SPEC_CS.md` 09:29. Source tree read at the same time.

> **The timestamp finding, fifth iteration, and it has inverted again.** Last pass
> `PROTOCOL.md` was the newest file and both blockers were in it. This pass
> `STATE_MACHINE.md` is the newest by six minutes, it carries **76** occurrences of
> `D-013` against `PROTOCOL.md`'s 35, and **every blocker below is a thing
> `STATE_MACHINE.md` now requires of `PROTOCOL.md` that `PROTOCOL.md` has not been
> told about.** The corpus fixed the engine half of K-3 completely and did not open
> the wire half at all. That is not a new failure mode — it is J-4 and J-5's failure
> mode, *the finding document was not told when the owner acted*, running in the
> opposite direction for the first time.

---

## Verdict counts

| Verdict | Count | Items |
|---|---:|---|
| **RESOLVED** | 7 | K2, K4, K5, K6, K7, J-4, J-5 |
| **PARTIAL** | 2 | K1, K3 |
| **UNRESOLVED** | 0 | — |
| **REGRESSED** | 0 | — |

**Both surviving items are the two that blocked last pass, and neither survives as
filed.** K2 is closed outright and closed well. K1 and K3 each received a real,
normative, carefully-argued fix; each fix is **half a fix**, and in both cases the
missing half is on `PROTOCOL.md`'s side of an owner boundary that the fixing
document names correctly and then reasons past.

**Nine new defects, L1–L9. Four block.**

* **L1 — checkpoint 8 has no chain position.** `STATE_MACHINE.md` makes it the only
  comparison a drain hand, an aborted hand or a tournament result ever produces, and
  `PROTOCOL.md` gives its `STATE_HASH` stage no `hand_id`, no `sequence`, no parent
  and no total order. This is `Q-09` verbatim, on the message K-3's fix newly made
  load-bearing.
* **L2 — `PROTOCOL.md` §6.2 still says the checkpoints are 1 to 7 "and no others",
  and §6.1's `PublicTableState` still has no `signed_this_hand`.** A receiver built
  from `PROTOCOL.md` rejects checkpoint 8, and T47's gate then never discharges on
  the settled path, so **a normal table pays `hand_deadline_ms` at every second hand
  boundary**. K-3's fix, read against the wire as it stands, re-creates D-013's
  fixed point at ten minutes per two hands.
* **L3 — the out-of-set `STATE_HASH` that both of K-3's payoffs rest on is
  forbidden by the wire.**
* **L4 — the solitary-stage freeze has no trigger that can fire.** Nothing paces a
  solitary hand, `hand_delay_ms` is explicitly not an engine input, and §4.0 defines
  no handling for a chained event naming a hand the receiver has finished. K1's
  original trace runs to completion unchanged. Detail in Part 2.

**Readiness verdict: NOT-READY**, and the gap is narrower than last pass's: three
rows and one field in `PROTOCOL.md` §6, one paragraph in §4.9 doing for checkpoint 8
what §4.10 already did for the boundary window, and one sentence in §4.0. Part 10
gives the build order, and **most of `messages.rs` is unblocked** — the envelope,
the catalogue, the caps, the bounds and the slot key are all implementable today.

---

# Part 1 — Per-item verdicts

## K1 — `P(k)` is per-receiver on the one path where it narrows — **PARTIAL**

The analysis is right, the rule adopted is the right rule, and the rule cannot fire.

**What is genuinely fixed, and it is most of the item.** The inverted sentence is
gone and its replacement states the truth:

> `PROTOCOL.md` §3.2: "**`P(k)` is agreed exactly where it is inert, and
> per-receiver exactly where it acts.**"

The section then does the thing the previous pass asked for and does it without
flinching — it enumerates the three constructions that could remove the residual
and rejects all three by name (a collective step at the point collectivity failed;
ratification in the abort body, which is D-012 by another name; refusing to narrow
`P`, which restores D-013's fixed point), and concludes *"what this document owes is
not a proof that it cannot happen but a rule that it cannot be **silent**"*. The
sub-question is answered by reductio and answered correctly: a peer's own emission
counts into its own `P`, because `P(0)` is the signers of `TABLE_READY` and under the
other reading no peer is ever a permitted emitter of the `HAND_INIT` it is required
to emit. `PLAYER_LEAVE` is excluded, with the right reason. §2.9's own D-012 sweep is
corrected against itself — *"a sweep that enumerates sets and not the transitions
between them will miss this class again"* — which is the most useful sentence in the
pass.

**What blocks.** The solitary-stage rule is a *receiver* rule whose trigger is
scoped to one hand:

> §3.2: "While in the solitary regime, a chained event of hand `k` from a seat
> outside `P(k-1)` that reaches §4.0 step 12 is not a rejection."
>
> §4.0 step 12a: "Reached only when step 12 says so."

Both clauses are hand-scoped, and nothing in the corpus keeps a solitary peer inside
the hand long enough for the other peer's copy to arrive. Part 2 runs the
interleaving. **The rule as written closes the trace it was written for only if the
contradicting event arrives inside the hand it names, and in the trace it was written
for it never does.**

Two smaller residuals, both already recorded and neither graded here: `Q-10` stays
open and is now correctly labelled unclosable by ratification; the bidirectional
partition is named honestly in §3.2 as beyond any wire evidence.

**K-9, the engine half, does not exist.** `grep -c solitary STATE_MACHINE.md` returns
**0**. §5.3 step 9 still routes `|dealt_in| == 1` straight to `Settling`, there is no
phase the freeze lands in and no transition that consumes it, and §2.6's `Phase` enum
has 20 variants with no frozen state other than `Diverged`, which only T50 enters and
only on two distinct `state_hash` values. This is on `DECISIONS.md`'s open list as
K-9 and is not re-filed; it is noted because it is the second reason the rule cannot
fire, independent of the first.

## K2 — `Q-09` is a wire question sitting under the sole re-entry path — **RESOLVED**

Closed completely, closed as a wire question, and closed with the one property the
previous pass could not have graded: **the box states what depends on it.**

> `PROTOCOL.md` §4.10, the hand-boundary-window box: "**Chain.** A boundary event
> belongs to **chain `k`**, the hand that has just ended, and carries `hand_id = k`.
> … **`sequence`.** `sequence = BOUNDARY_SEQUENCE_BASE + sender_seat` … **Parent.**
> `previous_event_hash = TERMINAL(k)` for **every** boundary event of the window …
> **Total order.** Ascending seat index."

All four quantities defined, plus capacity ("one per seat per boundary" — a stage
violation at step 12 otherwise) and the close condition ("at this receiver's
acceptance of a complete `HAND_INIT(k+1)`"). `BOUNDARY_SEQUENCE_BASE = 4 096` is in
§13 with the disjointness argument against `MAX_STAGES_PER_HAND = 2 048` written out.
§12's `Q-09` row reads **CLOSED (K2)** and — the part that matters — records that its
own former grade was wrong and why: *"`hand_id` and `sequence` are inside
`TO_BE_SIGNED` (§2.4) and in the slot key (§5.2.1), so two placements are two signed
byte strings for one intent."*

The genesis half is closed **by deletion**, which is the stronger disposition:
§3.1 now says normatively that the seat vector of `roster_hash(k)` is fixed at
`TABLE_READY` for the life of the table, so a `PLAYER_LEAVE` reaches no genesis and a
window disagreement can only stall stage 0. §4.11 rows 37–39 cite the box; the
honest-interleaving rows 37–39 cite it; the `Emitter` column carries the reserved
`sequence`. Independently checked: `PLAYER_LEAVE`'s "courtesy notice that is not
chained" clause is deleted (M4) and §2.3's exhaustive unchained list does not name
it.

**The one thing K2's disposition depends on is K1's**, and both documents say so in
terms — §4.10: *"a stalled stage 0 is the one place `P` narrows, so this disposition
depends on §3.2's solitary-stage rule and is not safe without it."* That dependency
is stated correctly and is why K2 is graded on its own terms rather than downgraded:
what K2 owed is delivered; what it leans on is K1, which is Part 2's subject.

## K3 — a drain hand places no checkpoint — **PARTIAL**

`STATE_MACHINE.md` did everything asked and more. `PROTOCOL.md` was not opened.

**The engine half is the best work in the pass.** §5.2 carries a normative
checkpoint-8 box; T45 and T46 both open it and publish; **T47 is gated on it on the
settled path only**, with the asymmetry argued from the right principle — *"on the
aborted path `P(k)` contains, by construction, the seat whose silence ended the hand,
and gating on it would be the exact defect §12.1 exists to prevent"*; **T61** is
added as the timer-borne exit so the gate can never be held open by a peer; `I32` is
added with three parts including the trace assertion *"no two consecutive
`HandComplete` entries are both gated"*; §12.1.1 is re-derived over twenty-one rows;
§12.1.2 re-derives the liveness bound and shows it unchanged. **K-3b is answered**:
`HandComplete` now has width on the settled path, which is where the `PLAYER_SIT_IN`
window was wanted.

**The wire half is absent, and the document says so about itself:**

> `STATE_MACHINE.md` §5.2: "**One of those quantities is not in §6.1 today and the
> checkpoint is worth little without it**: `signed_this_hand` must join
> `PublicTableState`, and that is the half of K-3 that belongs to `PROTOCOL.md`."

Checked against `PROTOCOL.md` as it stands:

* §6.2's table is `1`–`7`, under the words *"at these points and no others"*. There
  is no row for 8, no `grep` hit for "checkpoint 8" anywhere in the file.
* §6.1's `PublicTableState` field list has no `signed_this_hand`.
* §6.3 step 1's second entry condition — the solitary-stage trigger — still reads
  *"There is no checkpoint on that path to compare hashes at … (that is `K-3` in
  `DECISIONS.md`'s open list)"*, i.e. `PROTOCOL.md` still believes K-3 is open.
* §4.9 pins `STATE_HASH`'s required emitter set to `P(k-1)` at every checkpoint of
  hand `k`, which is not checkpoint 8's set.

That is L1, L2 and L3. Verdict **PARTIAL**, and the consequence is worse than
"incomplete": see L2, where the two halves as they now stand produce a slower table
than the one D-013 was written to fix.

## K4 — `THREAT_MODEL.md` §5.4's superseded cost model — **RESOLVED**

Both sites corrected, and corrected as corrections rather than overwritten.
`D-013` went from **1** occurrence to **29**. §5.4's preamble now quotes the false
sentence and forbids its reproduction: *"the sentence that stood here, `nothing marks
a seat absent automatically any more, so a silent seat is dealt in every hand and
stalls each one to the deadline until a human acts`, was false in all three of its
parts and is not to be reproduced."* §7.3(b) carries the second: *"**Second, and this
is the sentence that was false: they do *not* rejoin automatically.** This paragraph
read *'On reconnect they rejoin the key-holder set at the next hand boundary'* for
four passes."* X34 is a full row for the cost D-013 corrects, and §7.3(b) records the
K-3b residual rather than smoothing it. `Q7` is no longer cited as live.

## K5 — `GENESIS(0)` hashes the same 32 bytes twice — **RESOLVED**

§3.1's slot 4 is `ZERO32`. The disposition is the sentinel rather than the deletion,
and the reason given is the right one: `h` (§2.8) carries no arity, so two part-lists
of different lengths under one domain string are two preimages a future editor has to
reason about. §3.1's exclusion argument for `protocol_version` and `table_id` from
`table_params_hash` is unaffected because slots 1 and 2 carry them once each.

## K6 — two documents each canonical for `commitment_i` and `seed` — **RESOLVED**

`CRYPTOGRAPHY.md` §7.3 made the edit: both code blocks deleted, the *"was corrected
to match this section"* sentence deleted, points 1–5 kept, the `SPEC_CS.md` §16
deviation kept, and the ownership stated in the direction the corpus runs —
*"`PROTOCOL.md` §4.4 owns `commitment_i` and `seed`, and this section owns the
argument for why they are shaped that way."* §14's ownership row records it. The
reason given is the correct one and is worth keeping: *"they are recomputed by
receivers as a validation step, and a receiver reading a stale copy rejects honest
peers."*

One residual, filed as **L6**: `PROTOCOL.md` §4.4 still describes §7.3 as reproducing
both constructions and pointing the other way. Stale by one pass, in the same class
as J-4 and J-5, and in the opposite direction for the first time.

## K7 — the last status-defined gate on table progression — **RESOLVED**

T59's `Paused` exit no longer asks whether a seat is *willing*; it **is** §5.3
step 4, referenced and not restated:

> `STATE_MACHINE.md` T59: "the exit fires exactly when running step 4 on the
> post-transition state would deal two or more seats in, which after this transition
> means `|{s : stack[s] > 0 ∧ status[s] == Active ∧ s ∈ signed_this_hand}| ≥ 2`."

The disposition is the right shape and the text says why: *"The fix is not to replace
one word with another; it is to stop having two predicates."* The paragraph that
explains why `status` survives **inside** that predicate — `SittingOut` is set only by
that seat's own chained event, so it is a record of a choice and not a liveness
estimate — is the distinction D-013 needed and had not previously written down.
§5.3 step 8's note that `signed_this_hand` is cleared only at hand init, so it spans
the boundary window and a paused table, is what makes the new guard reachable.

One survivor of the same class is filed as **L7**, not as a K-7 downgrade: `§9.3`
condition 2 is the mirror position and still reads a status alone.

## J-4 — `NETWORK_STACK.md` §0.5.6 reporting a fixed defect as open — **RESOLVED**

`D-013` went from **0** occurrences to **21**. §0.5.6 is retitled *"The site where a
local view reached canonical state, and how it was closed"*, records the disposition,
and — the part the item actually asked for — records that **neither remedy this
document proposed was the one adopted**. §0.5.2's merged-lobby-view row now reads
*"This row carried the corpus's one live exception until the Phase 3 gate; it is
closed (§0.5.6), and the row now has the same unqualified answer as every other."*
The D-012 coverage row at the foot of the document is updated: *"**The one site
reported and not fixed is now fixed**."* All five follow-on sites move with it.

## J-5 — `CRYPTOGRAPHY.md`'s discipline item 1 — **RESOLVED**

`D-013` went from **0** occurrences to **14**, and the correction is written in the
tense the item asked for:

> `CRYPTOGRAPHY.md` §0: "The correction is at §6.4 item 1 and it is written in the
> past tense deliberately: **a claim that was false when made and is true now must
> not be left reading as evidence that the check was once run**, because the next
> editor will treat it as a discharged obligation and will not re-run it. That is
> `J-5`."

§6.4's `ctx` block is gone and replaced by a reference to `PROTOCOL.md` §4.5, with
the drift history kept as the argument for the deletion.

---

# Part 2 — The K1 re-check, done adversarially

## 2.1 The interleaving that used to fork

Three seats, `A` (0), `B` (1), `C` (2). Hand `k-1` completes normally; both `A` and
`B` hold `TERMINAL(k-1)` and `P(k-1) = {A, B, C}`.

1. In the boundary window of hand `k-1`, `C` emits `PLAYER_LEAVE` at
   `sequence = 4 096 + 2`, parent `TERMINAL(k-1)`. **`A` receives it. `B` does not** —
   one dropped frame, which is the whole of the adversary's budget.
2. Hand `k`. `R(HAND_INIT, k) = P(k-1)`. `A` derives a body whose `n(11)
   ledger_delta` removes `C`'s stack; `B` derives one that does not. Each rejects the
   other's copy on body mismatch. **Stage 0 stalls at both.** This is exactly the
   path §3.1's frozen roster and §4.10's per-receiver window close deliberately route
   the disagreement onto, and it is loud, as designed.
3. Hand `k` runs to `hand_deadline_ms` and aborts. `ABORT_TERMINAL(k)` is a function
   of `GENESIS(k)` alone, so both hold the same terminal. `roster_hash(k+1)`'s seat
   vector is frozen and its stack vector is hand `k`'s start-of-hand stacks, which
   are `TERMINAL(k-1)`'s `final_stacks` and are agreed; nothing moved, because hand
   `k`'s `HAND_INIT` never completed. **Both hold the same `GENESIS(k+1)`.**
4. `P(k)` at `A`: its own `HAND_INIT(k)` copy (own emission counts), and nothing
   else — `B`'s copy was rejected, `C`'s `PLAYER_LEAVE` is excluded by §3.2(2), the
   terminal `HAND_ABORT` is excluded by §3.2(1). **`P(k) = {A}`.** Symmetrically
   `P(k) = {B}` at `B`.
5. Both peers are in the solitary regime for hand `k+1`.

Under the specification as it stood at the Phase 3 gate, that is the fork: each
self-completes every collective stage, each drains the others one blind at a time,
each reaches §9.3 condition 1 naming itself.

## 2.2 What now happens — and it is the same thing

The solitary-stage rule fires when a chained event of hand `k+1` from a seat outside
`P(k)` **reaches §4.0 step 12**. Trace `B`'s `HAND_INIT(k+1)` copy into `A`:

* steps 1–7 pass;
* step 8 passes — `B` is a roster member, and the roster is frozen for the life of
  the table (§3.1);
* step 9 passes — the signature verifies;
* step 10 passes — `chain_scope = 1` matches §4.11;
* step 10a passes — `slot(E)` differs from `A`'s own entry in `sender_public_key`,
  and the parent is `GENESIS(k+1)`, which `A` holds identically;
* step 11 passes;
* step 12 routes to 12a; **`A` freezes.**

That is correct, and it is the disposition working. **It requires the event to arrive
while `A` is still evaluating hand `k+1`.** It does not.

**Nothing paces a solitary hand.** `A`'s hand `k+1` is `|dealt_in| == 1`, which §5.3
step 9 sends *"straight to `Settling`"* — no key setup, no shuffle, no reveal. Every
collective stage of it has `R = {self}` and completes in the step that emits it. T45
opens checkpoint 8, whose `required` is `P(k) = {A}`, and `A` hears itself, so T47's
gate discharges in the same step. And the one constant that looks like a brake is
explicitly not one:

> `STATE_MACHINE.md` §2: "`hand_delay_ms` is deliberately **not** an engine input. It
> is a presentation delay so a human can see the result."
>
> `PROTOCOL.md` §8.3: "the protocol does not wait for `hand_delay_ms`."

So `A` traverses hand `k+1` in the time it takes to compute it — microseconds — while
`B`'s copy is one network delay away. By the time it arrives, `A` is on hand `k+2`
or later. §12.1.2's *"the wall-clock cost is `hand_delay_sec` — 7 seconds — plus one
round trip"* is a **GUI** figure and it assumes a peer to round-trip with; on the
solitary path there is neither.

**And there is no rule for an event naming a hand the receiver has finished.** §4.0
has no `hand_id` step at all. The only staleness rule in the document is `DISPUTE`'s,
which is unchained: *"a `hand_id` that is neither the current hand nor a completed
hand of this session, is dropped"* (§4.9). §5.3 has meanwhile discarded the material
step 10a would look up — *"Finished hands keep only `TERMINAL(k)` and the terminal
body in memory"* — and §3.2's regime definition is itself hand-scoped, so whether `A`
is still "in the solitary regime for hand `k+1`" after hand `k+1` closed is not
stated. The natural implementation of step 12 for a hand that is over is *"not
expected at this `sequence`"* → **violation, drop**. Which is a rejection. Which is
precisely what §3.2 says this event must not be.

**Run it forward.** `A` plays ~40 drain hands (§12.1.2's own arithmetic) as fast as it
can compute them, `B`'s and `C`'s stacks reach zero, §9.3 condition 1 fires, and `A`
enters `TableClosed`, which is **absorbing — every event is a `Rejection`** (§12.1.1
row 19). `B` does the same. Neither ever evaluated a contradicting event at step 12.

> **Result: the interleaving that used to fork still forks.** No deadline fires, no
> invariant breaks, chip conservation holds at both, both reach a private
> "tournament won". §1.5's forwarding permission makes late arrival more likely, not
> less. The solitary-stage rule is correct and is never reached.

To be exact about what *is* bought: the rule closes the fork whenever the
contradicting event lands inside the hand it names. That is the common case at a
table where the two peers are still exchanging traffic and stay roughly in step —
and it is not the case the rule was written for, because K1's premise is a link that
drops frames, and its regime is one in which the only thing bounding a peer's rate is
its own CPU.

## 2.3 The rule the fix newly made load-bearing, and whether it holds

§3.2 names its own dependants and names them correctly:

> "**What now depends on this rule, and must be checked with it.** §3.1's frozen
> roster deliberately routes a boundary-window disagreement into a **stalled stage
> 0** … §4.10's boundary window closes per-receiver for the same reason. Both are
> safe **because** a narrowed `P` can no longer complete a hand in silence."

Both of those hold, conditionally on the freeze firing — and §2.2 shows it does not,
so **§3.1's frozen roster and §4.10's per-receiver window close are, today, unsafe by
their own stated condition.** That is the dependency the document declared; it fails.

But the dependency the document *did not* declare is the load-bearing one, and it is
the one the pattern predicts:

> **The solitary-stage rule made `§4.0`'s step ordering, and the receiver's treatment
> of a chained event naming a past hand, into consensus-critical rules for the first
> time.**

Before K1's fix, §4.0 was a validation pipeline: every step's failure mode was
"drop", and dropping an event that arrived too late cost nothing, because a late
event of a finished hand carries no information a finished hand needs. K1's fix
inverts that for exactly one class of event. A late chained event from a seat outside
`P` is now the **only** wire evidence distinguishing "I am alone" from "I am wrong",
and §4.0 will discard it before step 12 for the ordinary reason that it is late. The
fix is defended in §3.2 as *"unilateral, needs no reply, and fires on the **first**
solitary hand, because the peer that did not narrow is still emitting"* — which is
true about the emitter and says nothing about the receiver, and the receiver is the
half the rule is written on.

**The fix, and it is small.** Re-anchor the trigger on a record instead of on a live
phase. One sentence in §4.0 and one bullet in §5.3:

1. §4.0, a new step between 10a and 11, or a clause on step 8: **a chained event
   whose `hand_id` names a hand this receiver has completed is not dropped; it is
   evaluated against that hand's retained record.** Bound it the way §5.3 already
   bounds everything else — retain, per finished hand, `(hand_id, was_solitary,
   P(hand_id))`, LRU-capped, three `u64`-sized items per hand.
2. §3.2: state the regime test as a property of the *hand named by the event*, not of
   the receiver's current phase — *"a peer was in the solitary regime for hand `k`
   if…"*, past tense.

That converts §2.2's trace back into a freeze on the first contradicting event to
arrive at all, whenever it arrives, including after `TableClosed` — and `TableClosed`
must therefore lose its absorbing property for this one class, or the fortieth drain
hand is a race the rule loses. The alternative — pacing a solitary hand — is worse:
it puts a wall-clock read on a consensus path, which §2.6 forbids.

**Second-order check, done because the last three passes were caught here.** Does
making a stale-hand event evaluable reopen anything? Three candidates, all clean:
§5.2's equivocation predicate is unaffected because the event is not *applied* and
never enters a `stage_hash`; §5.3's occupancy bound is unaffected because the retained
record is not the class-0 map; §4.0's cheap-before-expensive ordering is unaffected
because the new step sits before step 11 and the event still never reaches step 14.
The one thing it does open is a memory bound, and §5.3 is the section that already
owns exactly that discipline.

---

# Part 3 — The boundary-event sweep

`PLAYER_SIT_OUT` is swept with the other two because §4.10 defines all three in one
box and rows 37–39 grade them together.

| | `0x0803 PLAYER_SIT_OUT` | `0x0804 PLAYER_SIT_IN` | `0x0805 PLAYER_LEAVE` |
|---|---|---|---|
| `hand_id` | **defined** — `k`, chain `k`, the hand that just ended (§4.10) | **defined** — same | **defined** — same |
| `sequence` | **defined** — `BOUNDARY_SEQUENCE_BASE + sender_seat` | **defined** — same | **defined** — same |
| parent | **defined** — `previous_event_hash = TERMINAL(k)`, i.e. `stage_hash(BOUNDARY_SEQUENCE_BASE − 1) := TERMINAL(k)` | **defined** — same | **defined** — same |
| total order | **defined** — ascending seat index, fixed by the `sequence` rule, no tie-break needed | **defined** — same | **defined** — same |
| slot key | **defined** — §5.2.1's 8-tuple; capacity one by the reserved `sequence`; a second boundary event of any type from that seat is a step-12 stage violation | **defined** — same | **defined** — same; shares the seat's one slot with 37 and 38, which is what forbids sitting out and leaving at one boundary |

**Five of five, three types of three. K2 does not reopen.** Independently checked
against §4.11's summary row, §4.11's honest-interleaving rows 37–39, §13's
`BOUNDARY_SEQUENCE_BASE` note, §3.2's `stage_hash` extension, and §12's `Q-09` row.
The window's close condition and its capacity-one property are both stated, and the
exemption count from step 10a's chain-position rule is kept exact ("the **second**
exemption … after the terminal `HAND_ABORT`'s").

**`PLAYER_SEATED` does not exist.** `grep` over all nine documents and the whole
source tree returns zero hits. The catalogue is closed at 39 types, §4.11 numbers its
honest-interleaving rows 1 to 39 precisely so *"a row added to the summary table
without a row here is visible as a gap in the numbering"*, and §9.3's cap table has
33 rows covering all 39. This is an answer and not an absence: **§3.1's frozen-roster
box forbids a seat-entry message in version 1 outright** — *"No seat is added to the
vector, removed from it, or reordered within it for the life of the table"* — and
§4.1 ends the table key's authority at `TABLE_READY`, so there is no key that could
sign an admission. An implementer looking for a mid-session seating message must find
nothing, and §3.1 is where the reason is written.

**One gap in the neighbourhood, filed as L5.** §13 reserves `4 096 … 4 105` for the
window and §5.3 says *"A hand exceeding `MAX_STAGES_PER_HAND` aborts with
`cause = 4`"* with `MAX_STAGES_PER_HAND = 2 048`. Nothing states that the window is
exempt from that abort, and §5.3's stated occupancy bound
`MAX_STAGES_PER_HAND × MAX_SEATS × 2` is computed over indices below 2 048 and is
therefore short by up to `MAX_SEATS` entries per hand. Both are one sentence.

---

# Part 4 — The checkpoint sweep

Per hand shape, from §6.2's table and `STATE_MACHINE.md` §5.2's rows, taken
independently and compared afterwards.

| Hand shape | Checkpoints from `PROTOCOL.md` §6.2 | Plus `STATE_MACHINE.md` §5.2 | Wire-carriable today |
|---|---|---|---|
| Full hand to showdown | 2, 3, 4, 5, 6, 7 | 8 (T45) | 2–7 only |
| Fold-out (all fold, T30) | 2, 3 | 8 (T45) | 2, 3 only |
| All-in run-out (T38 path) | 2, 3, 7 — **4, 5 and 6 do not fire**, because T38 skips the betting phase and §6.2's rows 4–6 all read *"after the … betting round closes"* | 8 (T45) | 2, 3, 7 only |
| Drain hand, one dealt-in seat | **none** — §5.3 step 9 goes straight to `Settling`, so no `DECK_COMMIT` and no betting round; §6.2 row 1 is setup-only | 8 (T45) | **none** |
| Aborted hand (stalled at `HAND_INIT`, T57 / T61) | **none** — same reason, one stage earlier | 8 (T46) | **none** |

**Two shapes place nothing on the wire. K3 does not close.** `STATE_MACHINE.md` fixes
both, `PROTOCOL.md` cannot carry the fix, and `PROTOCOL.md` §6.3 still describes K-3
as open in `DECISIONS.md`'s list. Verdict PARTIAL, filed as L1 and L2.

**A third shape is broken independently of K-3, and it is not a solitary-table
shape.** §6.2: *"A `BOARD_REVEAL` stage runs only after the preceding street's
`STATE_ACK` stage completes."* On the all-in run-out, T38 opens the turn with the
flop's betting phase skipped, so the flop placed no checkpoint and there is no
preceding `STATE_ACK` for the turn's `BOARD_REVEAL` to follow. The gate is
unsatisfiable from the turn onward. That is **L9**, it is on the second-most-common
hand shape in no-limit hold'em, and it is a two-word fix — *"the preceding street's
`STATE_ACK` stage, where one was placed"*.

---

# Part 5 — Termination

`STATE_MACHINE.md` §12.1.1 carries the walk, twenty phases over twenty-one rows
(phase 16 splits by entry, phase 20 by whether a hand is live). Checked
row-by-row against §5.2's transition table rather than read off the summary.

| # | Phase | Exit | Fires after |
|---:|---|---|---|
| 1 | `Seating` | T4 → `TableClosed` | local lobby timer |
| 2 | `AwaitingSeatRngCommit` | T4 → `TableClosed` | timer, guard `hand_id == 0 ∧ ledger_in == 0` |
| 3 | `AwaitingSeatRngReveal` | T4 → `TableClosed` | timer, same guard |
| 4 | `AwaitingKeySetup` (incl. a stalled `HAND_INIT`) | T57 → `HandAborted` | `hand_deadline_ms` |
| 5 | `AwaitingShuffle` | T57 → `HandAborted` | `hand_deadline_ms` |
| 6 | `AwaitingDeal` | T57 → `HandAborted` | `hand_deadline_ms` |
| 7 | `BettingPreFlop` | T57 → `HandAborted` | `hand_deadline_ms` |
| 8 | `AwaitingFlopReveal` | T57 → `HandAborted` | `hand_deadline_ms` |
| 9 | `BettingFlop` | T57 → `HandAborted` | `hand_deadline_ms` |
| 10 | `AwaitingTurnReveal` | T57 → `HandAborted` | `hand_deadline_ms` |
| 11 | `BettingTurn` | T57 → `HandAborted` | `hand_deadline_ms` |
| 12 | `AwaitingRiverReveal` | T57 → `HandAborted` | `hand_deadline_ms` |
| 13 | `BettingRiver` | T57 → `HandAborted` | `hand_deadline_ms` |
| 14 | `AwaitingShowdownReveal` | T57 → `HandAborted` | `hand_deadline_ms` |
| 15 | `Settling` | T57 → `HandAborted` | `hand_deadline_ms` |
| 16a | `HandComplete` via **T46** | T47, derived | immediately — ungated by design |
| 16b | `HandComplete` via **T45** | T47 on checkpoint 8, else **T61** | the checkpoint, else `hand_deadline_ms` |
| 17 | `HandAborted` | T46, derived | immediately |
| 18 | `Paused` | T59 | §5.3 step 4's own predicate on the post-transition state |
| 19 | `TableClosed` | — | absorbing, terminal |
| 20a | `Diverged`, hand live | T57 → `HandAborted` | `hand_deadline_ms`, not disarmed at T50 |
| 20b | `Diverged`, no hand live, `hand_id > 0` | T61 → `HandAborted` | `hand_deadline_ms` |
| 20c | `Diverged`, no hand started | T4 → `TableClosed` | timer |

**Every exit fires after a timer or the word *immediately*, and none of the
twenty-one reads `signed_this_hand`, `dealt_in`, a status, a certificate or `|V|`.**
The one apparent counter-example, T47's `checkpoint.required = signed_this_hand`, is
correctly not one, because on the path where T47 reads the set it is not the exit —
T61 is. The three intervals are contiguous with no gap: T4 covers setup, T57 covers a
live hand, **T61 covers the boundary**, which is the interval `HandComplete`'s new
width created and which did not need covering before this pass. That is D-012's third
shape stated in advance for once, and it is the first time in nine passes the corpus
has anticipated it rather than been caught by it.

**Does a table with one silent seat reach an end condition?** *In the engine, yes.*
§12.1.2: one or two `hand_deadline_ms` depending on when the seat went silent (three
cases, all re-derived, worst case two), then ~40 drain hands, §9.3 condition 1,
`TableClosed`. The result is now compared before `TableClosed` absorbs, which is what
K-3 bought.

**Against the wire as it stands, no.** Row 16b's gate is on a checkpoint
`PROTOCOL.md` does not define. A peer built from `PROTOCOL.md` emits nothing for
checkpoint 8, so `checkpoint.heard ⊇ checkpoint.required` never holds, T47 never
fires on the settled path, and **T61 fires at `hand_deadline_ms` after every settled
hand**. T61 leaves for `HandAborted`, so the next boundary is ungated and the hand
after it settles and stalls again: **600 000 ms every two hands, forever, on a table
where nothing is wrong.** That is D-013's fixed point at half the rate, produced by
the fix for the defect D-013's fixed point left behind. It is L2, it is the pass's
blocker, and it is why the readiness verdict is NOT-READY rather than "ship the
envelope".

---

# Part 6 — The status sweep

Every place a set, a guard or a progress path is computed from a seat's status
rather than from demonstrated participation, after D-013's sweep and K-7's fix.

| Site | Reads | Verdict |
|---|---|---|
| `PROTOCOL.md` §4.11 *Emitter* column, all 39 rows | `P`, `dealt_in`, the showdown set, `V(subject)` | **clean.** No status word survives; §2.9's enumeration 2 is the index and the four changed sets are named |
| `PROTOCOL.md` §4.4 `RNG_COMMIT` / `RNG_REVEAL` | `P(0)` | **clean.** "every seated participant" deleted in terms |
| `PROTOCOL.md` §4.9 `STATE_HASH` / `STATE_ACK` | `P(k-1)`; `P(0)` at checkpoint 1 | **clean as a set** — but see L3: it is the *wrong* set for checkpoint 8, and the wire's strictness here is what voids K-3's payoff |
| `PROTOCOL.md` §4.10 `PLAYER_SIT_IN` legality | *"any occupied seat … that is not a required emitter of the next hand"* | **clean.** Widened off `SittingOut` by D-013 |
| `STATE_MACHINE.md` §5.3 step 4 `dealt_in` | `status == Active ∧ s ∈ signed_this_hand ∧ stack > 0` | **clean, and deliberately mixed.** T59's note is the justification: `status` here records a choice the seat made with its own signature, not an estimate of whether it is there |
| `STATE_MACHINE.md` T59 `Paused` exit | §5.3 step 4's own predicate | **clean since K-7** |
| `STATE_MACHINE.md` T59 entry guard | `status ∈ {Active, SittingOut, Absent}` | **clean.** It gates the seat's own event about its own status; no other seat's progress reads it |
| `STATE_MACHINE.md` §5.3 steps 6–7, ante and blind posting | `Active`, `SittingOut`, `Absent` | **clean.** A chip rule, not a progress path; every input chained |
| `STATE_MACHINE.md` §5.3 step 3, `succ_active` | seats with chips | **clean** — stack, not status |
| `STATE_MACHINE.md` §9.3 condition 1 | `stack > 0` | **clean** |
| **`STATE_MACHINE.md` §9.3 condition 2** | **`\|{s : status == Active ∧ stack > 0}\| == 0` → `Paused`** | **survivor — L7.** A status-computed progress path at T47, three lines from §5.3 step 9's participation-computed `\|dealt_in\| == 0 → Paused`. Two predicates for one question, which is the exact shape K-7 has just deleted from T59, in the mirror position. Currently **inert**, because condition 2 strictly implies step 9's branch — but "inert because one predicate happens to imply the other" is what K-7's own text refuses as a defence |
| `PROTOCOL.md` §6.1 `sitting_out` vector | a `Vec<bool>` in `state_hash` | **clean.** Set only by chained events; §6.1's normative paragraph forbids any other source |
| `SeatStatus::Absent` | five readers, two since deleted | **clean by declaration (J-3).** Unreachable, deliberately, with every reader enumerated as a labelled trap. The over-warning by two entries is still one editorial line |

**One survivor, low, and it is the mirror of the item just closed.** The class J2
opened is enumerated and empty on the wire; §4.11's *Emitter* column is the index and
a new status word there is now defined as a defect.

---

# Part 7 — Coverage table

Occurrences of each decision, counted with `grep -o … | wc -l`, 2026-08-28.

| Document | D-009 | D-010 | D-011 | D-012 | D-013 | D-014 |
|---|---:|---:|---:|---:|---:|---:|
| `PROTOCOL.md` | 22 | 60 | 45 | 15 | 35 | **0** |
| `STATE_MACHINE.md` | 28 | 76 | 33 | 28 | 76 | **0** |
| `THREAT_MODEL.md` | 34 | 115 | 102 | 33 | 29 | **0** |
| `NETWORK_STACK.md` | 5 | 25 | 35 | 25 | 21 | **0** |
| `CRYPTOGRAPHY.md` | 18 | 38 | 29 | 19 | 14 | **0** |
| `CONTRIBUTING.md` | 12 | 11 | 11 | 8 | **0** | **0** |
| `DEPENDENCIES.md` | 11 | 4 | 8 | 10 | **0** | **0** |
| (`DECISIONS.md`) | 6 | 21 | 11 | 11 | 18 | 6 |

**D-013's zeros are down from four to two.** `NETWORK_STACK.md` went 0 → 21 and
`CRYPTOGRAPHY.md` 0 → 14, which is J-4 and J-5 discharged; `THREAT_MODEL.md` went
1 → 29, which is K-4.

**The two remaining zeros are not explained by the documents, and both documents
claim otherwise.** `CONTRIBUTING.md` §0 carries a dated sweep record — *"**Swept
against D-009 to D-012 on 2026-08-28**, the first sweep this document has"* — and
`DEPENDENCIES.md` carries a fuller one, *"**Sweep record, 2026-08-28 — D-009 to D-012
against this document.** Reported in full, including the decisions that changed
nothing, because D-012's process rule…"*. Both stop at D-012 and both cite the
process rule that says they must not. A defensible answer exists for each — D-013 is a
liveness rule with no crate consequence and no review-process consequence — but
neither document gives it, and "the sweep stopped one decision short" is the shape
that produced J-4, J-5 and K-4 in the first place. **One row each, recording the
decision and the words "no change", discharges it.**

**D-014's seven zeros are recorded rather than hidden**: `DECISIONS.md`'s open list
carries `D-014-1` as *"Integrate D-014 across the corpus … Touches all five
specification documents. **not started**"*. That is the process rule working. It is
listed here so the count is not read as a new omission, and it is **not** a blocker
for `messages.rs`: D-014 changes what a peer does with an invalid message, not what
the message is.

---

# Part 8 — The slot key

**One site, clean across all 39 message types.**

`PROTOCOL.md` §5.2.1 is the only place the tuple appears. It is stated as a literal
8-tuple with envelope field indices, with `subject(E)` broken out by `event_class`,
with four components listed as **deliberately absent** and a reason for each, and
with the normative sentence that no document may add one. Checked by `grep` across
the other six documents: `STATE_MACHINE.md` points at §5.2.1 at eleven sites and
reproduces nothing (§0's D-011 row, §3.1's ordering-buffer note — *"The ordering
buffer's key and the anti-replay slot key are two different tuples, and confusing
them…"* — §5.2 T-rows, §7, §10, §13); `CRYPTOGRAPHY.md` states at three sites that it
has never written one; `NETWORK_STACK.md` §1.3.2 lists it in its
not-owned-here table; `THREAT_MODEL.md` G7 quotes the predicate and not the key.

The per-type check is §4.11's honest-interleaving table, **derived per type from the
key rather than from stage-kind groups**, numbered 1 to 39 so a missing row is
visible as a gap in the numbering. All 39 verdicts are clean. Three rows name the
rule their verdict rests on so an editor cannot delete the rule without seeing what
it carries: rows 33 and 34 (`STATE_HASH` / `STATE_ACK`, clean only by §4.9's
reconciliation-round `sequence`), and row 36 (`HAND_ABORT`, clean only because
`event_type` is in the key). Two verdicts that changed direction are recorded as
changes rather than restated as if they had always read that way.

**One consequence of §5.2.1 that L1 will have to respect.** Checkpoint 8's
`STATE_HASH` needs a `sequence` that no other stage of the hand occupies and that a
reconciliation round can extend by `+r` without colliding with the boundary window at
`4 096 …`. That is a constraint on the fix, not a defect in the key.

---

# Part 9 — New defects

| # | Severity | Defect |
|---|---|---|
| **L1** | **BLOCKER** | **Checkpoint 8's `STATE_HASH` / `STATE_ACK` stage has no chain position.** `STATE_MACHINE.md` §5.2 makes it normative, emits it at T45 and T46, and gates T47 on it; `PROTOCOL.md` defines no `hand_id`, no `sequence`, no `previous_event_hash`, no total order and no stage kind for it. On the settled path it would have to sit after `HAND_COMPLETE`, whose `stage_hash` **is** `TERMINAL(k)` and from which §3.2 says nothing chains; on the aborted path the terminal `HAND_ABORT` *"has no `stage_hash`"* at all, so there is no parent value in existence other than `TERMINAL(k)` — which §4.10 has already assigned to the boundary window. This is `Q-09` verbatim, on the message K-3's fix newly made load-bearing, and it needs exactly what §4.10's box gave the window. Owner: `PROTOCOL.md` §4.9 and §6.2. |
| **L2** | **BLOCKER** | **`PROTOCOL.md` §6.2 lists checkpoints 1–7 under the words "at these points and no others", and §6.1's `PublicTableState` has no `signed_this_hand`.** A conforming receiver rejects a checkpoint-8 `STATE_HASH` at §4.0 step 11, T47's gate never discharges on the settled path, and **T61 fires at `hand_deadline_ms` after every settled hand** — 600 000 ms every two hands on a healthy table. And were checkpoint 8 accepted, it would compare a `PublicTableState` that omits the one quantity K-1 forks in, which `STATE_MACHINE.md` §5.2 says in terms: *"Until it is there, checkpoint 8 compares a hand boundary and not the participation set … and misses the input K-1 is actually about."* Two rows and one field. Owner: `PROTOCOL.md` §6.1, §6.2. |
| **L3** | **BLOCKER** | **Both of K-3's payoffs rest on a `STATE_HASH` from a seat outside the required emitter set, which the wire forbids.** `STATE_MACHINE.md` §5.2: *"**T49 and T50 are not scoped on that set**: a `STATE_HASH` for checkpoint 8 from *any* seat is compared, in-set or not. That is deliberate and it is the whole value of the checkpoint."* And K-3b point 1: a returning seat *"needs no `PLAYER_SIT_IN` at all"* because its own checkpoint-8 `STATE_HASH` is accepted and thereby adds it to `signed_this_hand`. Against `PROTOCOL.md`: §4.9 pins the set to `P(k-1)`; §4.0 step 12 rejects an out-of-set contribution as a stage violation; §3.2 and §4.10 make `0x0804 PLAYER_SIT_IN` **the one type** a seat outside `P(k)` may emit; §3.2's solitary exemption list names only `0x0804`. So an out-of-set checkpoint-8 `STATE_HASH` is a rejection at a normal peer and a **freeze** at a solitary one — never a comparison and never a re-entry. Owner: `PROTOCOL.md` §4.9 (widen the set for checkpoint 8 only, and say why) with a mirror line in `STATE_MACHINE.md` §5.2. |
| **L4** | **BLOCKER** | **The solitary-stage freeze has no trigger that can fire.** §4.0 defines no handling for a chained event naming a hand the receiver has finished; §5.3 has already dropped that hand's state; §3.2's regime test is scoped to the receiver's current hand; and nothing paces a solitary hand, because every stage of it self-completes and `hand_delay_ms` is *"deliberately not an engine input"*. Part 2 runs the trace: `A` reaches `TableClosed` — absorbing — before the contradicting event arrives. Fix in Part 2.3: evaluate a stale-hand chained event against a retained per-hand record, and state the regime test in the past tense. Owner: `PROTOCOL.md` §4.0, §5.3, §3.2. |
| **L5** | medium | **The boundary window is not exempted from `MAX_STAGES_PER_HAND`, and §5.3's occupancy bound does not count it.** §13 puts the window at `4 096 … 4 105`; §5.3 says *"A hand exceeding `MAX_STAGES_PER_HAND` aborts with `cause = 4`"* with the constant at 2 048, and states an occupancy bound of `MAX_STAGES_PER_HAND × MAX_SEATS × 2` computed over indices below it. An implementer who range-checks `sequence < MAX_STAGES_PER_HAND` rejects every boundary event; the stated bound is short by up to `MAX_SEATS` entries per hand. Owner: `PROTOCOL.md` §5.3, §13. |
| **L6** | low | **`PROTOCOL.md` §4.4 carries a stale claim about `CRYPTOGRAPHY.md` §7.3.** It still reads that §7.3 *"reproduces both constructions in full and its own ownership sentence points the other way"* and quotes the deleted *"was corrected to match this section"*. §7.3 deleted both blocks and that sentence in this pass. Third iteration of *the finding document was not told when the owner acted* (J-4, J-5, now this), and the first in which the direction is `PROTOCOL.md` ← `CRYPTOGRAPHY.md`. Owner: `PROTOCOL.md` §4.4 — one paragraph, rewritten in the past tense the way `CRYPTOGRAPHY.md` §0 rewrote J-5's. |
| **L7** | low | **`STATE_MACHINE.md` §9.3 condition 2 is a status-computed progress path.** `\|{s : status == Active ∧ stack > 0}\| == 0 → Paused` at T47, three lines from §5.3 step 9's `\|dealt_in\| == 0 → Paused`. K-7's own argument applies unchanged: *"The fix is not to replace one word with another; it is to stop having two predicates."* Inert today because condition 2 strictly implies step 9's branch. Owner: `STATE_MACHINE.md` §9.3. |
| **L8** | low | **`CONTRIBUTING.md` and `DEPENDENCIES.md` carry dated sweep records that stop at D-012**, while both cite D-012's process rule that every decision sweep covers every document. Zero occurrences of D-013 in either. One row each recording "no change, and here is why" discharges it; leaving the record reading *"swept against D-009 to D-012"* one and two decisions later is what J-5 is about. Owner: both documents. |
| **L9** | medium | **§6.2's `BOARD_REVEAL` gate is unsatisfiable on the all-in run-out.** *"A `BOARD_REVEAL` stage runs only after the preceding street's `STATE_ACK` stage completes."* On T38's path the betting phase for a street is skipped, so that street places no checkpoint and there is no `STATE_ACK` for the next street's `BOARD_REVEAL` to follow — the turn and the river cannot open. Independent of K-3 and on one of the two most common hand shapes in no-limit hold'em. Fix: *"…the preceding street's `STATE_ACK` stage, where one was placed."* Owner: `PROTOCOL.md` §6.2. |

**The pattern check, done on this pass's own findings.** Four of the nine are on the
path a fix in this pass newly made load-bearing — L1, L2 and L3 on K-3's checkpoint 8,
L4 on K-1's solitary-stage rule. The count is not the finding; the finding is that
**both fixes named their dependants and both named only the dependants inside their
own document.** K-1's §3.2 lists §3.1 and §4.10, both `PROTOCOL.md`. K-3's §5.2 lists
T45, T46, T47, T61 and I32, all `STATE_MACHINE.md`. Neither looked across the owner
boundary, which is where every one of the four sits. That is a sharper version of
D-012's third shape and it is worth writing down as such: **the path a fix newly makes
load-bearing is most often in the other owner's document, because that is the path the
fixing author did not have open.**

---

# Part 10 — The verdict

## Is `PROTOCOL.md` implementable without guessing?

**For `src/protocol/messages.rs` — the envelope and the 39 payloads — yes, with two
narrow exceptions.** For the receiver behaviour behind them, not yet.

| Gap | Section | Could an implementer pick a sane default? |
|---|---|---|
| **L1** checkpoint 8 has no chain position | §4.9, §6.2 | **No.** Four undefined envelope quantities, two of them inside `TO_BE_SIGNED`, on a stage two implementations must agree on byte-for-byte. This is `Q-09` and `Q-09` was closed by writing a box, not by defaulting |
| **L2** checkpoint set is 1–7 "and no others" | §6.1, §6.2 | **Partly.** An implementer can leave `checkpoint: u16` unrange-checked (see the build order) and be forward-compatible; it cannot invent `signed_this_hand`'s position in `PublicTableState`, because that changes `state_hash` for everyone |
| **L3** out-of-set `STATE_HASH` | §4.9 | **No.** Widening the set is a consensus change; narrowing it voids K-3. Two implementations that default differently reject each other's checkpoints |
| **L4** stale-hand chained event | §4.0 | **It will, and it will pick the wrong one.** "Drop what is out of stage" is the obvious reading and it reopens K1 silently. This is the one gap where a default is *available* and that is what makes it dangerous |
| **L5** window vs `MAX_STAGES_PER_HAND` | §5.3, §13 | **Yes** — §4.10 and §13 together make the intent clear; but the two texts disagree on their face and a strict reader rejects every boundary event |
| **L9** `BOARD_REVEAL` gate | §6.2 | **Yes** — "where one was placed" is the only reading that is not a deadlock |
| `Q-01` `showdown_policy` | §13, §4.6 | **Yes for the wire.** `SHOWDOWN_MUCK`'s payload and cap are defined; only whether the policy value is offered is open |
| `OQ-F` produce the deadline machinery? | §5.2.1, §5.3, §8.3 | **Yes for the wire.** The payloads and the key's subject axes are defined; deferring removes behaviour, not shapes |
| `Q-10` ratification of the stalled stage | §3.2 | **Not an implementation gap.** Named, adopted as a default, labelled unclosable |
| `K-8` when the departing stack is subtracted | §3.1, §4.10 | **No, and it is a `STATE_MACHINE.md` derivation.** It does not block `messages.rs`, but it blocks the receiver check on `HAND_INIT`'s `n(11)` and C-9's ledger identity |

**Everything else needed for `messages.rs` is pinned:** the three nesting levels with
field indices and a compiled reference struct (§2.3); `TO_BE_SIGNED` with its 24 hex
bytes printed twice (§2.4, §13); the canonicality gate (§2.5); `h` with the exact
`derive_key` call and the 8-byte big-endian per-part length prefix (§2.8); the closed
17-row domain register (§2.8); all 39 codes with `chain_scope`, stage kind, channel
and emitter (§4.11); a per-type payload cap for every type (§9.3); a hard maximum for
every `Vec` and every string (§9.4); the slot key as one literal tuple (§5.2.1); and
every two-sided constant in one place (§13).

## Build order for `src/protocol/messages.rs`

1. **`EventBody` and `SignedEvent`**, exactly §2.3's twelve and two fields, with
   `#[cbor(array)]` and `minicbor::bytes` on the fixed-width arrays. Copy the
   reference struct in §2.3; it compiled in the Phase 0 probe. Nothing here is open.
2. **`EventType` as a closed `#[repr(u16)]` enum**, all 39 codes from §4.11, with
   `chain_scope()`, `event_class()`, `channel()` and `stage_kind()` as `const fn`
   methods off it and a `TryFrom<u16>` that rejects everything else — including
   `0xF000..=0xFFFF`, which §4.11 makes never-acceptable inside a session. §4.11 is a
   complete index; do not derive these from grouping the codes.
3. **`Envelope` sentinel checks for `chain_scope == 0`** — `table_id = ZERO32`,
   `hand_id = 0xFFFF_FFFF_FFFF_FFFF`, `previous_event_hash = ZERO32`, `sequence = 0`
   — driven off §2.3's exhaustive thirteen-type unchained list, not off a predicate.
4. **The 39 payload structs**, in §4 order, each carrying its §9.3 cap and its §9.4
   collection bounds as associated `const`s so the cap lives beside the type rather
   than in a table the next editor forgets. Write `DISPUTE`'s `n(5)`/`n(6)` appended,
   per §2.2 rule 5.
5. **`slot(E)`**, one function, §5.2.1's 8-tuple with `subject(E)`. One site, no
   helper that reconstructs a subset of it anywhere.
6. **§4.0 steps 1–11 as a `validate_prefix` pipeline.** Every one of these is fully
   specified and none needs the engine. Stop at 11.
7. **A `constants` module from §13**, names verbatim, including
   `BOUNDARY_SEQUENCE_BASE`, and a test asserting `DOMAIN_EVENT`'s 24 bytes against
   the hex printed in §2.4 and §13.
8. **Round-trip and canonicality tests**: encode → decode → re-encode → byte-compare,
   over every payload type, plus §2.7's determinism cases.

## What to defer, and why

* **§4.0 steps 12, 12a, 13, 14, 15.** Step 12a is L4 and step 12's stale-hand
  behaviour is the defect; steps 13–15 need the engine. Build 1–11 now and leave a
  single `enum PrefixOutcome` the later steps hang off.
* **The `checkpoint` field's range.** Encode `checkpoint: u16` and **do not**
  range-check it to `1..=7`. Pinning §6.2's current text compiles L2 into the wire and
  makes the fix a breaking change; leaving it open costs nothing, because a value the
  engine does not know is rejected at step 12 anyway.
* **`PublicTableState` and `state_hash`.** Blocked on L2 — adding `signed_this_hand`
  later changes the hash for every peer, so write neither the struct nor the hash
  until the field list is settled.
* **The anti-replay stores of §5.3.** Blocked on L4 and L5. The *key* (item 5 above)
  is not blocked and should land now.
* **`TIMEOUT_VOTE`, `TIMEOUT_CERT`, `EquivocationProof`.** `OQ-F` may delete them.
  Write the payload structs — they are cheap, they are defined, and §5.2.1's key needs
  their subject axes — and wire no behaviour.
* **`SHOWDOWN_MUCK` policy.** `Q-01`. The payload and cap are defined; the policy
  gate is not this module's.

## What must land in `PROTOCOL.md` before the receiver half is written

Four edits, none of them a decision the owner has not already made in principle:

1. §4.9 or §6.2 — a checkpoint-8 box in the shape of §4.10's boundary-window box:
   chain, `hand_id`, `sequence`, parent, total order, required emitter set, and the
   sentence saying what depends on it (**L1**, **L3**).
2. §6.2 — row 8 in the table; §6.1 — `signed_this_hand` in `PublicTableState`
   (**L2**).
3. §4.0 — one step, and §5.3 — one bullet, making a chained event of a finished hand
   evaluable against a retained record, with §3.2's regime test restated in the past
   tense (**L4**).
4. §6.2 — three words on the `BOARD_REVEAL` gate (**L9**); §13 or §5.3 — one
   sentence exempting the boundary window from the stage-count abort (**L5**).

With those four, `PROTOCOL.md` is implementable without guessing, K1 closes on its own
trace, K3 closes, and the table that a silent seat leaves behind terminates in the
time §12.1.2 publishes rather than at ten minutes per two hands.
