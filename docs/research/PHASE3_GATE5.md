# PHASE3_GATE5.md — the freeze-path gate

**Commission.** Twelve passes. The eleventh (`PHASE3_GATE4.md`) graded `K1`, `N1`,
`N5`, `N6`, `N8` PARTIAL and opened `P1`–`P8`, two of them blocking. The receiver
is being written; everything except the freeze path is unblocked. This pass
decides whether the freeze path can now be written, and whether anything already
written must change.

**Method, unchanged, because it is what has produced every blocker since Phase 2.**
Nothing is graded off a summary row, off a disposition column, or off
`DECISIONS.md`'s own account of what landed. Every verdict below was re-derived
from the normative text: `STATE_MACHINE.md` §2.3's store was read before
`DECISIONS.md`'s `P1` row; §5.3 steps 4 and 8 were evaluated by hand against
`PROTOCOL.md` §4.9's *new* readmission rule rather than against the row that
describes them; `HAND_DEADLINE_FLOOR` was recomputed from §13's preset;
`src/security/validation.rs` was read in full and its tests run. That inversion
produced this pass's blockers, and it produced them where the pattern predicts.

**Files as read, 2026-08-28.** `STATE_MACHINE.md` **20:21**, `DECISIONS.md`
20:12, `PROTOCOL.md` 20:11, `src/security/validation.rs` 19:56, `THREAT_MODEL.md`
19:34, `DEPENDENCIES.md` 19:33, `CONTRIBUTING.md` 19:32, `CRYPTOGRAPHY.md` 19:31,
`NETWORK_STACK.md` **17:52 — unchanged, second consecutive pass**, `SPEC_CS.md`
09:29. `cargo check --lib -j 19` clean; `cargo test --lib -j 19` — **120 passed,
0 failed**.

> **The timestamp finding, eighth iteration.** `NETWORK_STACK.md` was again the
> one file not opened, and it again carries **zero** `D-014` and a prohibition
> list whose named trigger *"an invalid application signature"* is a tier-1
> trigger by name. Last pass predicted this file would be the cheapest place to
> find a defect and it was; this pass predicts it again and it is again. But the
> prediction is now worth less than the *other* half of the same rule, which is
> where both of this pass's blockers came from: **`STATE_MACHINE.md` was edited
> last, at 20:21, nine minutes after `DECISIONS.md` recorded the debt it owed —
> and the debt was recorded and not paid, in the file that was open at the time.**

---

## Verdict counts

| Verdict | Count | Items |
|---|---:|---|
| **RESOLVED** | 8 | K1, N1, N6, N8, P1, P3, P6, P8 |
| **PARTIAL** | 2 | N5, P2 |
| **UNRESOLVED** | 3 | P4, P5, P7 |
| **REGRESSED** | 0 | — |

**This is the best pass in the series by execution and the most dangerous by
composition.** Four items that had been PARTIAL for two passes closed completely:
the checkpoint store became three named slots with a proved bound (`P1`), the
solitary floor gained its one hand of slack (`N-1e`, and with it `K1` and `N1`),
the whole-hand deadline became a derived function of the seat count with a joiner
refusal (`P3`), and the tier-2 witness became a type that actually witnesses
(`P6`). **Both interleaving variants of the K1 walk now freeze on the first,
best-evidenced event and hold** — the hole `PHASE3_GATE4.md` found in variant 2 is
closed.

**And the two documents disagree about the one mechanism both of them changed.**
`PROTOCOL.md` §4.9 deleted the readmission set from the *required* emitter set and
made it widen the *accepted* one; `STATE_MACHINE.md` §5.3 step 4 still reads
`admitted = signed_this_hand ∪ readmit` into `dealt_in`, and §5.3 step 8 still
reads it into `solitary_since`. `DECISIONS.md` records the first half of that debt
as **`P2-e`** and does not name the second half at all. That is **`Q1`**, it is a
blocker, and it is exactly the shape the last gate named.

**Six new defects, `Q1`–`Q6`. Two block the freeze path.**

* **`Q1` — the engine implements the deleted readmission rule, and the guard that
  was holding it back has just been unlatched.** `STATE_MACHINE.md` §2.7's
  `Readmitted` box says *"an implementation must build the field and the two read
  sites and wire no producer for `Readmitted`"* **until `P2` lands**. `P2` landed
  in `PROTOCOL.md` in this pass. What is behind that guard was written for the
  rule `P2` deleted.
* **`Q4` — `CheckpointState` records no `state_hash`.** `P1`'s whole purpose is to
  make T49, T50 and T51 evaluable against a checkpoint of a *finished* hand.
  T49's guard is *"the value equals this peer's own derivation at that
  checkpoint"* and T50's is *"two distinct `state_hash` values now exist"*; the
  struct holds `values: u8`, a bare count, and no value at all. On the live
  checkpoint the derivation was recomputable from current state. On `boundary` and
  on `agreed` it is not. **The fix's own three consumers cannot compute their
  guards on the two slots the fix added.**

**Readiness verdict: READY for `PublicTableState`, `state_hash`, the three
anti-replay stores, `RetainedHand`, the constants module and §4.0 steps 1–12a
including the solitary freeze; NOT-READY for the `CheckpointState` record, for
T49/T50/T51, and for any producer of `Readmitted`.** Part 8 gives the per-item
answer.

---

# Part 1 — Per-item verdicts

## K1 — `P(k)` per-receiver on the path where it narrows — **RESOLVED**

**The one character landed and it is written as the stronger of the two forms.**
`STATE_MACHINE.md` §2.6:

> `solitary_at(k)` := `solitary_since == Some(j) ∧ j <= k + 1 ∧ k <= hand_id`

with the note that decides an implementation detail rather than leaving it:
*"written `j <= k + 1` rather than `j - 1 <= k` because `j` is a `u64` and the two
are the same inequality without the underflow"*. `DECISIONS.md`'s `N-1e` gave the
repair as `j - 1 <= k`; the document took the equivalent form that cannot
underflow, which is the correct call and is argued rather than silently
substituted.

**The lemma is restated over the disjunction and proved by case**, which is what
`N-4` asked for and did not get last pass:

> *"First disjunct, `P(k-1) == {self}` … step 8 executed `solitary_since :=
> solitary_since.or(Some(k))`. Hence `j <= k <= k + 1`. Second disjunct,
> `P(k) == {self}` … **hand `k+1`'s init** read a one-member set and wrote
> `solitary_since := solitary_since.or(Some(k+1))`. Hence `j <= k + 1`, which is
> the slack and the whole of it."*

**And the ordering the second case silently depends on is named rather than
assumed** — §9.3's end conditions are evaluated *after* hand init has run, so step
8 writes `solitary_since` even on the boundary at which the table closes, without
which **T63** could never fire. That is the sentence that makes the proof a proof
instead of a walk, and it is the single best thing in this pass.

**What the slack costs is stated and bounded.** `solitary_at(k)` is now true for
one hand *before* the first solitary episode as well as inside a gap between two
episodes; both cost nothing reachable because a `SolitaryDivergence` exists only
where step 10b's record says *solitary*, and for those hands the record says
otherwise. **The floor never manufactures a freeze; it can only fail to reject one
the wire has already decided on.** That is the right shape for a floor and the
document says so.

**The residual §2.6 names is real, is correctly scoped, and is correctly left.**
§5.3 step 8 reads `signed_this_hand` at hand `m`'s init and since K-3b that set
spans the boundary window, so a seat that signed *only in the window* makes the
set two-membered and the floor can be `None` for a hand whose record says
solitary. The argument that it costs no detection is sound — a second signer means
hand `m` is not solitary at this peer either, and stage 0 stalls loudly instead —
and the alternative (a second read of a second quantity for one question) is the
defect `K-7` and `L7` each cost a pass. Recorded rather than fixed, which is the
right call.

**The one thing that re-opens K1 is `Q1`, and it re-opens it completely.** See
Part 7. K1 is graded RESOLVED against the corpus's own walk, in which `readmit`
is empty.

## N1 — a solitary peer releases its own freeze — **RESOLVED**

The wire half was RESOLVED last pass. **All four parts of the engine half are now
present.** T53 carries the two-signer conjunct; I33(b) carries the three-message
exemption that makes it satisfiable; I33(c) is scoped on the freeze rather than on
the latch; and the fourth part — the floor — is `N-1e` above.

**T50's `|c.required| == 1` conjunct is now readable on the stale route**, which
is what `P1`'s store bought it, and the row states why the test is on the named
checkpoint's own `required` rather than on a remembered regime: *"`c.required`
**is** the regime for the stage being compared … fixed when the checkpoint opened
and therefore not grown by the very copy that contradicts it"*. That is the
correct reason and it is a better one than the item asked for.

**`N1-a` is still open and is still one sentence.** `DECISIONS.md` states both
answers are safe and that what must not happen is the third state — the exit
described and unreachable. Unchanged from last pass. Low, and it should land with
the freeze path rather than after it, because an implementer writing T53 has to
choose.

**Two `DECISIONS.md` rows now contradict the fix — `Q3`.** `N-1e` sits in the
**open** list, unmarked, reading *"the lemma is false for exactly the hand the fix
exists for"* and *"blocking for K-9"*, and `N-4` restates the **superseded**
predicate `solitary_at(k) := … ∧ j <= k <= hand_id` as its resolution.
`DECISIONS.md` outranks `STATE_MACHINE.md`. See Part 7.

## N2 — §4.0 step 10a undefined for a stale-hand event — **RESOLVED, and its argument is now true**

Carried forward. The third clause of the idempotence argument — *"a seat already
in the readmission set is already in it"* — was false for one pass and is now
true, because `P2`'s fix removes the effect that repeated. §4.9 says so in terms:
*"It also restores that argument's own sentence to truth … because entering `A` no
longer has an effect that repeats."* A fix that repairs the *justification* of an
earlier fix, in the earlier fix's own words, is the cleanest form this series has
produced.

## N5 — K-3b's readmission route does not survive step 10b — **PARTIAL**

**The wire half is RESOLVED and has been superseded by something better.** `P2`
replaced N5's union with an accepted-set widening; §4.9 states the new rule in a
box, states what it costs (*"a seat readmitted by this route is **not `dealt_in`**
for hand `m+1` … and becomes a required emitter, and dealable, at hand `m+2`, one
hand later"*), and states why the required set was the wrong place. D-013's
promise is kept with one hand of latency, which is the honest trade.

**The engine half does not exist and what stands in its place is the deleted
rule.** `STATE_MACHINE.md` §5.3 step 4:

> `admitted := signed_this_hand ∪ readmit`. Then
> `dealt_in[s] := (status == Active ∧ s ∈ admitted)`

and, two paragraphs down, in bold: *"**`admitted` is one quantity and it is
`PROTOCOL.md` §4.4's `P(m) ∪ A`** … §4.4 makes hand `m+1`'s **required emitter
set** `P(m) ∪ A`."* §4.4 says the opposite, in this pass, at l. 4224: *"§4.9's
readmission set `A` widens the **accepted** emitter set … and leaves the required
set alone (`P2`)."*

`N-5e` is marked SUPERSEDED and replaced by **`P2-e`**, which is the right
handling — but `P2-e` describes the current text as *"§5.3 step 4 keeps
`dealt_in ⊆ P(k-1)` unchanged and adds nothing"*, and step 4 does not keep it: it
computes `dealt_in` from a strict superset. **That is `Q1`.**

## N6 — the checkpoint-8 `STATE_ACK` stage — **RESOLVED**

**There is now somewhere for it to go and the row that consumes it is rewritten.**
`CheckpointStore.boundary` holds hand `k−1`'s checkpoint 8 across the boundary;
`CheckpointState` gains `acked: SeatSet`; and T51 is corrected in the two ways
that matter, both of which the row calls defects rather than refinements:

> *"(a) `agreed` is set when the **ACK stage** completes and not on the first ack
> … there was no `acked` set, so the completed stage was not a representable
> object at all. (b) The checkpoint is selected by name, so a checkpoint-8
> `STATE_ACK` of hand `k` arriving after hand `k+1` has started **can** set
> `agreed`."*

**D-014's tier-2 precondition is satisfiable at a hand boundary for the first
time**, which is the exact fault N6 was filed to remove. The retention is stated
as a superset of the wire's two windows, with the sentence an implementer needs:
*"An implementer who reverses the inequality re-creates `P1`."*

**The one thing standing between this and a working T51 is `Q4`** — the guard
*"every value in `c` agrees"* reads values the record does not hold. That is filed
as a new defect against `P1`'s fix rather than graded against N6, because the
structure N6 asked for is present and correct.

## N8 — D-014 cites a `SPEC_CS.md` §22 window that does not exist — **RESOLVED**

The sentence itself is edited, in `DECISIONS.md`, at l. 1379–1391:

> *"That window is a required addition to `SPEC_CS.md` §22 and is not an element
> §22 contains — `N8`, and the wording is corrected here because this decision is
> [what a later editor reads first] … as though pointing at something already
> specified."*

Three consumers agree (`STATE_MACHINE.md` T64, `THREAT_MODEL.md` §9.2, the open
list). The owner's edit to `SPEC_CS.md` §22 is still owed and is correctly not
made from here.

## P1 — the engine keeps one checkpoint and the wire needs two — **RESOLVED**

**The largest and best-argued edit in the series.** `Option<CheckpointState>`
becomes `CheckpointStore { live, agreed, boundary }` — three named `Option` slots,
*"no `Vec`, no map, and nothing keyed on a quantity a sender chooses"* — with:

* **the defect stated as three consequences and each one traced**, including the
  one reachable on a healthy table through §1.5's forwarding reorder;
* **a proved bound** — *"at most three `CheckpointState` values exist at any
  moment, for the whole life of the table"* — with the disjointness argument
  (*"hand `k`'s is released at the same transition that opens hand `k+1`'s,
  because both are keyed on `TERMINAL(k+1)` being fixed"*);
* **a lifetime per slot**, with `boundary := live.take()` at T47/T61 and
  `boundary := None` at the next T45/T46, and the T10 special case costed at one
  line;
* **two read accessors and a prohibition on any third route** — `named(h, n)` and
  `agreed_checkpoint(h)`, *"D-011 rule 2's discipline applied inside one
  document"*;
* **`Event::StateHash` and `Event::StateAck` carry `hand_id`, and the document
  proves it is not a wire change** — it is in the envelope and inside
  `TO_BE_SIGNED` already;
* **T64's guard changed from exact-`n` to `c.number >= n`**, with the argument
  from `transcript_head`'s chaining that makes the loosening sound rather than
  convenient — *"an agreed checkpoint at `n' >= n` of the same hand is agreement
  over a prefix that contains checkpoint `n`'s stage"*. Without that change the
  guard *"could not be discharged at all"*, which is a defect the pass found in
  its own fix.

**All three faces of `P1` are closed.** (i) T51 can set `agreed` for a boundary
checkpoint. (ii) T50's conjunct is readable on the stale route. (iii) The
non-solitary stale mismatch has a carrier, named in T50's own cell: *"since `P1`
this row is the carrier for the stale boundary mismatch at a receiver that was not
alone … the conjunct is false there because `|c.required| >= 2`, and the
disposition is the ordinary one — one hand, not the table."* §12.1.2 states the
wire dependency for it and I32(d) asserts both lifetimes and the pair of directed
cases.

**`Q4` is the defect this fix newly made load-bearing.** See Part 5 and Part 7.

## P2 — the readmission set is a replay amplifier — **PARTIAL**

**The wire half is RESOLVED and the fix is the right one: it closes the amplifier
at the signature rather than with a counter.**

> `PROTOCOL.md` §4.9: *"`R(HAND_INIT, m+1)` is `P(m)`, unchanged and unenlarged.
> `A` widens the *accepted emitter* set of that one stage to `P(m) ∪ A`."*
> … *"**What closes it is the signature, not a counter.** … the only thing that
> can enlarge a required emitter set is a seat's **own accepted copy of
> `HAND_INIT(m+1)`**, whose `hand_id`, `sequence` and `previous_event_hash` are
> inside `TO_BE_SIGNED` … **No replay of any chain-`k` event can produce one**,
> so the amplifier has no input."*

Three things make this better than the two repairs the last gate proposed. It
needs no new state (a per-hand anti-replay bit would have been a fourth store);
it makes the property structural rather than counted (a replay is inert because
of what it *is*, not because of how often it arrives); and it restores N2's own
idempotence sentence to truth instead of carving an exception around it. §4.0 row
10a, §4.0 row 10b, §4.4's `HAND_INIT` row, §5.3's store list and §11's attack
table all carry the same rule in the same words.

**The engine half is not applied and its specification is incomplete.** `P2-e`
names the `dealt_in` half and does not name the `solitary_since` half, and the
engine text as it stands implements the deleted rule in both. `Q1`.

## P3 — one unscaled deadline — **RESOLVED, and more than was asked**

`hand_deadline_ms` is no longer a constant. §8.2 derives a normative floor:

```
HAND_DEADLINE_FLOOR(n) =  hand_delay_ms
                        + (2n + 23) * crypto_step_timeout_ms
                        + 4n        * (action_timeout_ms + action_grace_ms)
```

with `n = max_players` and the reason it must not be the live seated count
(*"a floor derived from a count that moves during the table's life is not a
constant every peer holds identically"*). The stage decomposition is exactly
`6n + 20` split as `4n − 3` betting actions and `2n + 23` crypto-timeout stages;
`4n` is used rather than `4n − 3` because *"the three spare allowances are cheaper
than an off-by-one an implementer has to re-derive"*. **The arithmetic checks:**
`(6n + 20) − (4n − 3) = 2n + 23`, and only one stage carries `hand_delay_ms`
(§8.3's boundary row), so the single `+ hand_delay_ms` is right.

**The finding is sharper than the last gate's.** The preset was below its own
floor **at every seat count**, including the two that three passes measured; two
and four survived only because the crypto term is margin a healthy table never
spends, and six is where the *action* term alone crosses. The document says so and
says why the narrow passes missed it.

**Three further things landed that the item did not ask for.**

1. **The rule is restated in the form that fixes the shape rather than the
   number** — *"a backstop that fires first is not a backstop; it is a shorter
   deadline that attributes nobody"* — which closes the incentive as well as the
   arithmetic: any seat could otherwise reach the non-attributing abort path by
   making a hand long, at no cost and without misbehaving.
2. **§9.4 rule 2a is a joiner-side refusal**, computed from the advert's own
   fields *before the advert is shown to a user or stored*, with the correct
   prohibition on the tempting alternative: *"A client must not join and
   substitute its own floor locally: the deadline is two-sided."*
3. **`REOPENINGS(config)` is derived and the residual is named** (`P3-r`), with
   both closures stated and neither taken, and the reason for not taking either.

The preset is now `3 300 000` at `seats = 10`, clearing `FLOOR(10) = 2 297 000` by
`1 003 000`, which buys four reopening raises and stays under `n(17)`'s
`3 600 000` cap. **`Q6`** refines the residual: at an advertisement of *exactly*
the floor, `REOPENINGS = 0`, and §9.4 rule 2a accepts such an advertisement.

## P4 — D-014 tier 1 contains a clause that is not self-contained — **UNRESOLVED**

**The code is fixed and the documents are not, which is a new inversion of the
authority order.** `src/security/validation.rs` deleted `ParentUnknown`, left a
comment explaining why, and added a tripwire:

```rust
/// `ParentUnknown` used to sit in tier 1. It is decidable only against the
/// receiver's own store, so one dropped frame would have removed an honest
/// player - a live counter-example to the rule this module states. It is
/// gone, and this test is the tripwire against it coming back.
#[test]
fn no_tier_one_violation_depends_on_the_receivers_store() { … }
```

`SelfContained` has nine variants and the test asserts nine. **The clause is still
tier 1 in four places, one of which is the decision itself:**

| Site | Text |
|---|---|
| `DECISIONS.md` l. 1330 — **D-014's own tier-1 list** | *"a message whose chain parent does not exist, or whose signer is not a party to this table"* |
| `PROTOCOL.md` l. 1987 — §4.0's D-014 exception box | *"a deck that is not a permutation, **a parent that does not exist**, a signer that is not a party to the table — removes its sender from the table"* |
| `PROTOCOL.md` l. 3777 — §4.10's `cause = 6` row | *"this receiver's own run of §4.0 over that event returns a tier-1 illegality — … **a parent that does not exist** \| accept at once"* |
| `THREAT_MODEL.md` l. 1005, l. 1082 | §5.1's D&A definition; **row 13, *replay of old actions*, tier **1**, *"loses its seat"*** |

**Two consequences, and the second is new.** First, `DECISIONS.md` outranks the
code, so the next implementer re-adds the variant and the tripwire test is the
only thing between the corpus and a removal on a dropped frame. Second — and this
is created by the code-side deletion — **`THREAT_MODEL.md` row 13 now asserts a
disposition the code cannot produce**: *replay of old actions* is classified tier
1 with *"loses its seat"*, and there is no longer a tier-1 variant it maps to. The
row is not merely stale; it names a consequence with no implementation.

**There is no open-list row for `P4`.** The pass fixed the artifact it was allowed
to edit and did not record the debt in the file that owns it, which is D-013
applied in the one direction that does not work.

## P5 — `NETWORK_STACK.md` §1.2 prohibition 7 — **UNRESOLVED, correctly filed**

`NETWORK_STACK.md` is unchanged at 17:52 and carries **zero** `D-014`. The three
sites are verbatim as the last gate quoted them:

* l. 97, §0.1 — *"No automated eviction, anywhere: no `block_peer`, no unseating,
  and no allow-list or block-list populated by the poker protocol"*, explicitly
  *"as D-011 rule 3 extends it to every layer"*;
* l. 541, §1.2 prohibition 7 — *"remove, block, **unseat**, refuse or penalise a
  peer on the strength of a protocol proof"*, whose named triggers include
  *"an invalid application signature"* — **`SelfContained::SignatureInvalid`, a
  tier-1 trigger by name**;
* l. 2536, §11.5.1 — *"no `block_peer` call, no disconnect, no dial refusal, no
  unseating"*.

**The D-013 handling is exemplary and is the reason this is not graded worse.**
`DECISIONS.md` l. 1473 carries a `P5` row with all three line numbers, the exact
edit for each (sites 1 and 2 re-scoped to the transport; site 3 kept as written
plus one sentence that a tier-1 removal is a *table* disposition producing no
transport action), the observation that site 3 needs the *opposite* edit, and a
withdrawal of `N-9`'s aside that graded the zero *"defensible"*. `PROTOCOL.md`
§4.0's box points at it and declines to edit it (D-011 rule 1). Everything a
future pass needs is written down; the file was simply not opened.

## P6 — `AgreedCheckpoint` cannot witness its precondition — **RESOLVED**

The type now witnesses the thing:

```rust
pub struct AgreedCheckpoint {
    state_hash: Hash,
    sequence: u64,
    hand_id: u64,          // "A checkpoint from another hand judges nothing here."
    emitters: Vec<PlayerId>,
}
```

with **one constructor**, `covering(…)`, which refuses with a typed
`WitnessError` — `AccusedNotAnEmitter`, `ReceiverNotAnEmitter`,
`WrongHand { checkpoint, message }` — and four tests, all passing. The doc comment
states the defect it repairs rather than the feature it adds: *"A state hash and a
sequence number do **not**: they say some checkpoint existed, not that the accused
was inside its emitter set, and not which hand it covers."*

**`Q2` is the half that is still a comment.** `sequence` is stored and `covering`
never reads it; T64's guard is on `c.number >= n` and the type has no `number`.

## P7 — `PublicTableState`'s field order is given twice, differently — **UNRESOLVED**

Unchanged. §6.1's table — the normative owner — places the four `Vec<bool>` flags
between `committed_this_hand` and `current_bet`, and `signed_this_hand` last.
§2.9 l. 795 still ends:

> *"`ledger_in`, `ledger_out`, `transcript_head`, `signed_this_hand`, and the
> `Vec<bool>` flag vectors."*

In a `#[cbor(array)]` struct the order is the encoding, so the two enumerations
produce different `state_hash` values. No open-list row. It is one clause and it
is in the struct `src/protocol/` is about to write.

## P8 — `DECISIONS.md` D-014-1 reads *"all — not started"* — **RESOLVED**

Rewritten per document with occurrence counts *"as the cheap check that a reader
can repeat"*, and it names its own error: *"The disposition `all — not started` was
wrong and is `P8`: four of the five had landed it, and one had landed nothing."*
The disposition column now reads *"four done, one not started"*. The row also
corrects `N-9`'s staleness in passing — **and does not edit the `N-9` row itself**,
which is `Q3`'s third instance.

---

# Part 2 — Both interleaving variants, walked to the end

Three seats, `A` (0), `B` (1), `C` (2). Hand `k−1` completes normally,
`P(k−1) = {A,B,C}`. Adversary budget: dropped frames only. Walked at `A`.

### 2.1 The common prefix

1. `C` emits `PLAYER_LEAVE` at `BOUNDARY_SEQUENCE_BASE + 2`, parent
   `TERMINAL(k−1)`. **`A` receives it; `B` does not.** It adds nothing to
   `signed_this_hand` (§5.3 step 4(ii); §3.2 exclusion 2).
2. Checkpoint 8 of hand `k−1` **agrees at both** — T45 opens it the moment
   `TERMINAL(k−1)` is fixed, before the boundary window runs.
3. Hand `k`'s init at `A`: `admitted = signed_this_hand ∪ readmit = {A,B,C} ∪ ∅`,
   `|admitted| = 3`, so step 8 writes nothing. **T47 then does
   `checkpoints.boundary := checkpoints.live.take()`** — hand `k−1`'s checkpoint 8
   moves to the boundary slot. *(New this pass; under the single slot it was
   discarded here.)*
4. Hand `k` stalls at stage 0: `R(HAND_INIT, k) = P(k−1) = {A,B,C}`, `A`'s
   `n(8) dealt_in` reflects `C`'s departure and `B`'s does not, each rejects the
   other on body mismatch, `C` emits nothing.
5. T57 at `hand_deadline_ms` → `HandAborted` → **T46**: stacks restored,
   `checkpoints.boundary := None` (hand `k−1`'s record released at `TERMINAL(k)`,
   which is this abort), `checkpoints.live := ckpt8(hand k)` with
   `required = signed_this_hand` read before hand init clears it.
6. **`P(k) = {A}` at `A` and `{B}` at `B`.** `A`'s own `HAND_INIT(k)` copy counts
   (§5.3 step 4(i)); `B`'s was rejected on body mismatch so it does not; the
   terminal `HAND_ABORT` counts into no `P` (§3.2 exclusion 1).
7. The checkpoint-8 bodies for hand `k` differ in `signed_this_hand`:
   `[true,false,false]` at `A`, `[false,true,false]` at `B`. K-8-independent.
8. `A`'s checkpoint-8 window closes at once — T47's gate does not apply on the
   T46 path, `HAND_INIT(k+1)` is required of `P(k) = {A}` and self-completes.
   **§5.3 step 8 writes `solitary_since := Some(k+1)`** by the second disjunct.
   T47 then moves hand `k`'s checkpoint 8 into `boundary`. `RetainedHand(k)` is
   written with **`was_solitary = true`**, `p = P(k−1) = {A,B,C}` and
   `checkpoint8_state_hash(k)` = `A`'s own value.

**State at `A` entering the contradiction:** `solitary_since = Some(k+1)`;
`checkpoints.boundary = ckpt8(hand k, required = {A})`; `readmit = ∅`.

### 2.2 Variant 1 — `B`'s `HAND_INIT(k+1)` copy arrives

**9. `B`'s checkpoint-8 `STATE_HASH` for hand `k` arrives first.** `hand_id = k`,
a hand `A` has completed → step 10a **skipped** (N2) → **step 10b**, checkpoint-8
branch → compared against the retained `checkpoint8_state_hash(k)` → **mismatch**
→ *"enters §6.3 at step 1 and, where the record says the hand was solitary, routes
to step 12a with it"*. The record says solitary. Step 12a emits
`SolitaryDivergence { hand_id: k }`.

**10. The engine accepts it.** T62's guard is `solitary_at(k)` =
`solitary_since == Some(k+1) ∧ (k+1) <= k+1 ∧ k <= hand_id` → **true**.
**T62 fires**: `Diverged`; `Fault{SolitaryDivergence}` recording `e.seat` and
`e.event_hash`; `solitary_contradicted := true`; the offending event **not applied
and not counted into `signed_this_hand`**; the hand deadline is **not** disarmed.

> **This is the change.** Last pass this event produced a `SolitaryDivergence` the
> engine dropped, and the freeze waited for `HAND_INIT(k+1)` one hand later. The
> freeze now fires on the **first, best-evidenced** contradiction — the
> checkpoint-8 comparison on the fork hand itself, which is what §3.2's second
> disjunct and §4.9's `N1` half were both written for.

**11. The freeze holds.** `A` broadcasts `DISPUTE { kind = 1 }` (§6.3 step 2,
mandatory) and publishes its reconciliation `STATE_HASH` for checkpoint 8 of hand
`k` at `BOUNDARY_CHECKPOINT_BASE + 2` — permitted by I33(b)'s three-message
exemption, and **only** that traffic is permitted. The stage is required of
`R(c) ∪ W = {A} ∪ {B} = {A,B}`, so `|R| = 2` and `A` cannot complete it alone;
T53's two-signer conjunct refuses a one-signer completion independently. Then
either:

* `B` publishes its own re-derived value → one value remains (T53: resume, latch
  cleared) or two remain (T54 → `table_faulted`, or T60 → `HandAborted` → T46); or
* `B` never speaks → `A` stays frozen, T57 fires at `hand_deadline_ms`, stacks
  restore, **the latch is still set**, and §9.3 condition 0.6 closes the table at
  the next boundary. No hand is dealt and no end condition names a winner.

**12. `B`'s `HAND_INIT(k+1)` copy arrives and changes nothing.** `A` is already in
`Diverged`; T62 does not re-enter, T49/T50 are excluded from `Diverged`, and
I33(c) is satisfied because no hand was dealt between the two entries.

**Verdict, variant 1: the freeze fires on event 9 and holds.**

### 2.3 Variant 2 — `B`'s `HAND_INIT(k+1)` copy is dropped; only the checkpoint-8 `STATE_HASH` of hand `k` ever contradicts

Steps 9, 10 and 11 run **identically**. The event that fires the freeze in variant
1 is the checkpoint-8 `STATE_HASH` of hand `k` itself, not the `HAND_INIT(k+1)`,
so dropping the `HAND_INIT` copy removes nothing.

* **The freeze fires on the same event.** T62's guard is true because
  `j = k + 1 <= k + 1`.
* **It holds by the same route.** `R(c) ∪ W = {A,B}`; `A` cannot complete a
  reconciliation stage alone; T53 refuses one signer.
* **`A` never reaches a solitary tournament win.** §9.3 condition 0.6 is evaluated
  before conditions 1 to 4 while `solitary_contradicted` is set, so the table
  closes on the divergence and not on the drain.
* **And if the copy arrives after `A` has already closed**, T63 has the same guard
  — now true — so `settlement.tournament_winner := None`. The result is retracted,
  in the one phase the document deliberately makes non-absorbing.

> **The hole `PHASE3_GATE4.md` found is closed, and closed by the one character.**
> Its statement was: *"If the **only** contradicting event that ever reaches `A`
> is `B`'s checkpoint-8 `STATE_HASH` **of hand `k` itself** … then `A` freezes on
> nothing, drains to §9.3 condition 1, and enters `TableClosed` naming itself …
> and `settlement.tournament_winner` is **not** retracted."* Evaluating the same
> walk against `j <= k + 1` gives `k + 1 <= k + 1` — **true** — at both T62 and
> T63. K1's payoff is gone on the walk K1 was filed for.

**Verdict, variant 2: the freeze fires on the checkpoint-8 `STATE_HASH` of hand
`k` and holds; the tournament win is retracted if the copy arrives after
closure.**

### 2.4 The one input that reverses both verdicts

Both walks assume `readmit = ∅` at hand `k+1`'s init — true here, because nothing
in the walk feeds it. **Feed it and step 8 writes nothing:** `|admitted| = 2`,
`solitary_since` stays `None`, `solitary_at(k)` is false, and T62 and T63 both
reject. That is `Q1`, it needs no key, and the feeder is Part 3's.

### 2.5 The non-solitary walk, which is the one that used to have no carrier

Same prefix but `A` was **not** alone — say `P(k) = {A,D}` because a fourth seat
`D` also signed. `B`'s stale checkpoint-8 `STATE_HASH` of hand `k` mismatches;
step 10b routes it to *"enters §6.3 at step 1"* without step 12a, because the
record does not say solitary; the engine receives it as `Event::StateHash` with
`hand_id = k`; `named(k, 8)` resolves to `checkpoints.boundary`; **T50 fires**,
`Diverged`, `Fault{StateDivergence}`, and `|c.required| == 2` so the latch is not
set and the table plays on after one hand. **`P1`'s second face is closed.**

**Except that T50 cannot compute its own guard on that record** — *"two distinct
`state_hash` values now exist for that checkpoint"* — because `CheckpointState`
stores `values: u8` and no values. That is `Q4`, and this walk is where it bites.

---

# Part 3 — The replay check

**Setup.** Ten consecutive hands. Once per hand, an observer replays one signed,
**agreeing** checkpoint-8 `STATE_HASH` from a seat `X` that is outside the target
receiver's retained `P(m−1)` for some finished hand `m`. No key is used; the
message is one `X` genuinely signed and §1.5 permits any peer to forward it. The
retained record keeps it evaluable for `MAX_RETAINED_HAND_RECORDS = 4 096` hands.

**Result on the wire — the amplifier is gone.**

| Question | Hand 1 | … | Hand 10 |
|---|---|---|---|
| Does step 10a suppress the duplicate? | no (skipped, N2) | … | no |
| Does `A` gain `X`? | yes | … | yes |
| Does `R(HAND_INIT, m+1)` grow? | **no** | … | **no** |
| Does stage 0 stall on the wire's completion test? | **no** | … | **no** |

`R(HAND_INIT, m+1)` is `P(m)` and completion is `heard ⊇ P(m)` and nothing else
(§4.9, §4.4 l. 4224, §4.0 row 10a l. 765). Replaying the message re-opens an
acceptance that costs a map lookup. **P2's stall is closed at its input.**

**Result in the engine — the stall comes back through a different field.**
`STATE_MACHINE.md` T67 writes `readmit ∪= {X}`; §5.3 step 4 computes
`admitted = signed_this_hand ∪ readmit` and derives
`dealt_in[s] := (status == Active ∧ s ∈ admitted)`. So at every one of the ten
hand inits:

* the target deals `X` in and **every other peer does not**;
* the `n(8) dealt_in` vectors differ, so the `HAND_INIT(m+1)` bodies differ;
* **stage 0 stalls**, at every peer, and the hand aborts at `hand_deadline_ms`
  with `cause = 1`, `attributed = []`, `cert_hash = None` — no progress, nobody at
  fault. Under the new preset that is up to **3 300 000 ms** per iteration rather
  than 600 000.

This contradicts a clause `PROTOCOL.md` calls normative twice — *"`dealt_in` is a
subset of `P(k−1)`, always"* (§4.4 l. 2532, l. 2557) and *"a seat readmitted by
this route is **not `dealt_in`** for hand `m+1` — `dealt_in ⊆ R(HAND_INIT, m+1) =
P(m)`"* (§4.9 l. 3397) — and it is asserted as an engine invariant, **I31(b)**:

> *"`dealt_in[s] ⇒ status[s] == Active ∧ stack[s] > 0 ∧ s ∈
> admitted-as-of-the-previous-hand`, where `admitted = signed_this_hand ∪ readmit`
> is the one quantity §5.3 step 4 computes (`N-5e`)"*

so a harness written to the engine document would **assert** the behaviour the
wire document forbids.

**The second payoff, and it is worse than the stall.** §5.3 step 8 writes
`solitary_since` only when `|admitted| == 1`, and reads `admitted` deliberately:
*"It reads `admitted` and not `signed_this_hand`, and that is `N-5e` reaching one
step further than it looks."* That argument was correct while `A ⊆ P`. It is
inverted now: under `P2` a seat in `A` is precisely a seat that did **not** sign
into chain `m`, so `P(m−1) = {self}` and the wire's `RetainedHand.was_solitary` is
`true` — while `|admitted| = 2` and the engine writes nothing.

**Ten hands, one replay each:**

| Hand | `readmit` at init | `admitted` | `solitary_since` | `solitary_at(k)` in the episode | T62 / T63 |
|---:|---|---|---|---|---|
| 1–10 | `{X}` | `{self, X}` | **`None`, every hand** | **false** | **never fire** |

**So the replay's payoff is no longer a stall — it is the silent disarming of the
solitary freeze**, at exactly one receiver, for as long as the replays continue,
with no key and no other peer seeing a reason. K1's payoff returns in full,
including the unretracted tournament win, on the walk Part 2 just showed closed.

**The cheapest feeder needs no fork at all.** The second route into `A` — *"a
`0x0804 PLAYER_SIT_IN` in chain `k`'s boundary window"* — carries **no matching
requirement**. One seat sitting out and back in once, legitimately, produces one
signed message that, re-sent once per hand, holds the target permanently
non-solitary for 4 096 hands.

**What stops this today, and why that is not reassuring.** `STATE_MACHINE.md`
§2.7's `Readmitted` box says: *"Until it lands, an implementation must build the
field and the two read sites and **wire no producer for `Readmitted`**"* — where
*it* is `P2`. With no producer, `readmit` is always `∅` and both payoffs are
inert. **`P2` landed in `PROTOCOL.md` in this pass.** The guard's stated condition
is satisfied, and what is behind the guard was written for the rule `P2` deleted.

---

# Part 4 — The healthy-table check at every seat count

Twenty hands, `n` seats, nothing goes wrong, every peer answers in one round trip.
Round trips per hand are `6n + 20`, verified against §8.2's own decomposition
(`4n − 3` betting actions + `2n + 23` crypto-timeout stages). 50 ms RTT; 42 ms of
Bayer–Groth proving per shuffler [`MENTAL_POKER.md` §5.1], sequential.

| Seats | Round trips | Protocol | Proving | **Per hand** | **20 hands** | `HAND_DEADLINE_FLOOR(n)` | Any deadline fires? |
|---:|---:|---:|---:|---:|---:|---:|---|
| 2 | 32 | 1.600 s | 0.084 s | **1.684 s** | **33.7 s** | 1 017 000 ms | **no** |
| 4 | 44 | 2.200 s | 0.168 s | **2.368 s** | **47.4 s** | 1 337 000 ms | **no** |
| 6 | 56 | 2.800 s | 0.252 s | **3.052 s** | **61.0 s** | 1 657 000 ms | **no** |
| 8 | 68 | 3.400 s | 0.336 s | **3.736 s** | **74.7 s** | 1 977 000 ms | **no** |
| 10 | 80 | 4.000 s | 0.420 s | **4.420 s** | **88.4 s** | 2 297 000 ms | **no** |

**Six seats and up now pass.** The last gate's failure was arithmetic, not timing:
`4n` actions at 25 000 ms reached the flat 600 000 ms deadline at `n = 6` and
exceeded it by 400 s at `n = 10`. With `hand_deadline_ms` derived, the preset at
`seats = 10` is 3 300 000 ms against a 2 297 000 ms floor, and every smaller table
advertises its own value with the joiner refusing anything below.

**Margins, per deadline kind:**

| Deadline | Value at the preset | Worst healthy consumption | Margin |
|---|---:|---:|---:|
| `crypto_step_timeout_ms` (per stage) | 30 000 ms | ≈ 92 ms (42 ms proof + 50 ms RTT) | **326×** |
| boundary (`hand_delay_ms + crypto_step_timeout_ms`) | 37 000 ms | ≈ 150 ms | **246×** |
| `action_timeout_ms + action_grace_ms` | 25 000 ms | human | n/a |
| `hand_deadline_ms` (whole hand) | 3 300 000 ms | 4.42 s protocol + human | **≥ 520×** on protocol |
| `join_deadline_ms` | 120 000 ms | — | T4 still fires long first |

**Does every hand place a checkpoint?** Yes, at all five sizes and all five hand
shapes, unchanged from the last gate and now with the store to hold it across the
boundary: §6.2 row 8 is unconditional, I32(a) asserts coverage on both terminal
paths, and I32(d) adds the lifetime assertions the three slots need.

**What the widening surfaces this time.**

1. **The deck segment grows twice over.** `2n + 2` of `6n + 20` round trips at
   `n = 10` is 22 of 80, and proving is 0.42 s of the 4.42 s. Still inside
   `crypto_step_timeout_ms` per stage by two orders of magnitude, and still owed a
   Phase 4 measurement alongside `verify_strict`.
2. **`REOPENINGS` at the preset**, from §8.2's formula against
   `hand_deadline_ms = 3 300 000`: `n = 2` → 91, `n = 4` → 26, `n = 6` → 13,
   `n = 8` → 7, `n = 10` → **4**. A hand with more reopening raises than that
   still aborts on a legal path.
3. **At an advertisement of exactly the floor, `REOPENINGS = 0`** — `Q6`. §9.4
   rule 2a accepts it, so a founder who reads §8.2 and sets the deadline to the
   derived floor ships a conforming table on which the **first re-raise of any
   hand** aborts it, with `cause = 1` and nobody at fault.
4. **The heads-up MVP's stalled-hand wait is now 17 minutes, not 10.** A two-seat
   table must advertise at least `FLOOR(2) = 1 017 000 ms`. Correct, and worth
   knowing before it is reported as a regression.

---

# Part 5 — The load-bearing check

Every fix this pass made, the rule it newly makes load-bearing, and whether it
holds. **Including this pass's fixes against each other**, which is the shape the
last gate named: *one fix deletes a guard while another adds a disposition behind
it, in the same pass.*

| # | Fix | Rule it newly makes load-bearing | Holds? |
|---|---|---|---|
| 1 | **`solitary_at(k)`'s `j <= k + 1` (`N-1e`)** | that §5.3 step 8 writes `solitary_since` at **every** init that reads a one-member set, including the one at which the table closes | **YES**, and the ordering inside T47 it depends on is named explicitly rather than assumed, with T63 given as the row that would silently die without it |
| 2 | **`solitary_at`'s slack** | that a `SolitaryDivergence` exists **only** where step 10b's record says solitary, so the slack cannot manufacture a freeze | **YES.** §2.6 states it as *"the engine's floor never manufactures a freeze; it can only fail to reject one the wire has already decided on"* |
| 3 | **`CheckpointStore`'s three slots (`P1`)** | that `named(h, n)` matches **at most one** slot | **YES**, proved from the lifetimes (two boundary checkpoints never coexist; `live` and `agreed` are disjoint by the supersession rule and cleared together) |
| 4 | **`CheckpointStore`'s `boundary` slot** | **that T49, T50 and T51 can evaluate their guards against a record of a *finished* hand** | **NO — `Q4`.** T49 reads *"this peer's own derivation at that checkpoint"* and T50 reads *"two distinct `state_hash` values"*; `CheckpointState` holds `values: u8` and stores no value. On the live checkpoint the derivation was recomputable; on `boundary` it is not |
| 5 | **T51's `acked` set and stage-completion rule** | that `agreed` is set from a *stage*, so `agreed_checkpoint(h)` is well-defined | **YES**, and the `if live.agreed.is_some()` guard on the supersession write is what keeps a superseded-but-agreed record from being lost |
| 6 | **T64's `c.number >= n`** | that `transcript_head` chains through every earlier stage of the hand | **YES.** The argument is stated and is sound; it is a loosening with a proof rather than a convenience |
| 7 | **§4.9's `A` widens the accepted set (`P2`)** | **that no consumer of `A` reads it into a required set, a hashed body, or a regime test** | **NO — `Q1`.** §5.3 step 4 reads it into `dealt_in`, which is `n(8)` of a hashed collective body, and §5.3 step 8 reads it into `solitary_since`. `P2-e` names the first and not the second |
| 8 | **§4.9's *"not `dealt_in` for hand `m+1`"*** | that the engine's `dealt_in` derivation is `P(m)` and not `admitted` | **NO — `Q1`.** I31(b) **asserts** the union as an invariant a harness must check |
| 9 | **`HAND_DEADLINE_FLOOR(n)` (`P3`)** | that every stage of a legal hand is counted exactly once and that only one stage carries `hand_delay_ms` | **YES.** `(6n + 20) − (4n − 3) = 2n + 23` checks, and §8.3's boundary row confirms `hand_delay_ms` belongs to one stage |
| 10 | **§9.4 rule 2a's joiner refusal** | that the floor is computable from the **advert's own fields** before a seat is taken | **YES**, and the tempting alternative (join and substitute a local floor) is forbidden by name with the two-sidedness argument |
| 11 | **the preset moving to 3 300 000** | that no consumer hardcodes 600 000 | **NO, but filed** — `P3-e` lists roughly a dozen sites in `STATE_MACHINE.md` and the §9.4 validator line `hand_deadline_ms >= 10 * action_timeout_ms`, which is *"a **twelfth** of the floor at ten seats"* and passes exactly the configurations `P3` shows are unplayable |
| 12 | **deleting `ParentUnknown` from the code (`P4`)** | that the documents' tier-1 lists agree with `SelfContained` | **NO — `P4`.** Four sites still carry the clause, one of them D-014 itself, and `THREAT_MODEL.md` row 13 now names a disposition with no implementation |
| 13 | **`AgreedCheckpoint`'s `emitters` and `hand_id` (`P6`)** | that the **whole** precondition is in the type, not part of it | **NO — `Q2`.** `sequence` is stored and never checked; T64's guard is on `number`, which the type lacks |
| 14 | **`D-014-1`'s row rewritten (`P8`)** | that the other stale rows in the same list were swept with it | **NO — `Q3`.** `N-1e` and `N-4` describe an applied fix as owed, and `N-9` is stale by the new row's own admission |

**Six of fourteen fail, and five of the six are the composition shape.** Rows 4,
7, 8, 13 and 14 are all *"a correct edit in one place whose consumer in another
place was written for the deleted rule"*, and in four of them the consumer was
open in the same pass. The discipline the last gate proposed — *list the
dispositions again after the additions land, not before* — was followed for the
one case it was written about (`P2`'s own `A`-writers were re-listed, and §4.0's
idempotence paragraph was corrected in place) **and was not extended to the
readers**.

> **The rule this pass adds, and it is the mirror of the last one.** A fix that
> **narrows** a quantity must re-derive every **reader** of that quantity, not
> only every writer. `P2` narrowed `A` from a required-set member to an
> accepted-set member; its writers were all re-derived and its two readers — one
> hashed body and one regime test, both in the other document — were not. The
> writers are the ones the fixing author is looking at; the readers are the ones
> that break.

**And the sharper instance is row 4, where both halves are in one document, sixty
lines apart, in the same edit.** `P1` added two slots whose whole purpose is to be
read by T49/T50/T51, and did not add the field those three rows read. Nothing
crossed an owner boundary; nothing was deferred; the pass simply did not re-read
the guards after moving what they select over.

---

# Part 6 — The D-014 sweep

### 6.1 Coverage, `grep -o … | wc -l`, after this pass's edits

| Document | D-009 | D-010 | D-011 | D-012 | D-013 | D-014 |
|---|---:|---:|---:|---:|---:|---:|
| `PROTOCOL.md` | 22 | 67 | 52 | 19 | 41 | **42** |
| `STATE_MACHINE.md` | 28 | 85 | 42 | 35 | 91 | **67** |
| `THREAT_MODEL.md` | 37 | 127 | 104 | 39 | 35 | 49 |
| `NETWORK_STACK.md` | 5 | 25 | 35 | 25 | 21 | **0** |
| `CRYPTOGRAPHY.md` | 18 | 39 | 30 | 21 | 15 | 23 |
| `CONTRIBUTING.md` | 12 | 12 | 11 | 8 | 9 | 12 |
| `DEPENDENCIES.md` | 11 | 6 | 10 | 10 | 5 | 14 |
| (`DECISIONS.md`) | 6 | 22 | 15 | 17 | 24 | 40 |
| (`SPEC_CS.md`) | 0 | 0 | 0 | 0 | 0 | 0 |

`SPEC_CS.md`'s row is correct and is not a gap: it is the source the decisions
answer to. **`NETWORK_STACK.md`'s zero is the corpus's only coverage defect and it
is a contradiction rather than an omission — `P5`, unchanged, second pass.**

### 6.2 The tier boundary, checked at every site that states it

Held, and now enforced by a type in the place that matters most.

* `PROTOCOL.md` §4.0's box — tier 2 *"removes nobody until this receiver holds the
  completed `STATE_ACK` stage §4.9 requires"*.
* §4.9's `kind = 3` box — exactly one `SignedEvent` in `n(3) evidence`, because
  *"a finding that needs a second event to be decidable is not
  self-authenticating"*.
* §4.10's `cause = 6` rows — tier 1 accepts *"at once"*; tier 2 rejects until the
  receiver holds the stage.
* `STATE_MACHINE.md` T64 — `agreed_checkpoint(hand_id)` is `Some(c)` with
  `c.number >= n`, **and it is dischargeable for the first time**, which it was
  not under the single slot.
* `src/security/validation.rs` — `Tier2Finding` has no constructor that does not
  take an `AgreedCheckpoint`; `AgreedCheckpoint::covering` is the only constructor
  and refuses on three typed errors; `Finding::Tier2Unconfirmed` maps to
  `Outcome::VoidHandOnly` with `None` for the removal order. Ten tests, all
  passing.
* Equivocation, timeouts, `attributed`, votes and quorums excluded by name at five
  sites.
* **`SeatStatus::Removed` is in §5.3 step 6's ante list** (`D-014-t`), so the dead
  seat is blinded off and I1 holds with no term moving at the removal.

### 6.3 Contradictions with `src/security/validation.rs`

**Three, and the direction has inverted since last pass.**

1. **`ParentUnknown` — the code is right and four document sites are wrong
   (`P4`).** `DECISIONS.md` l. 1330 (D-014's own list), `PROTOCOL.md` l. 1987 and
   l. 3777, `THREAT_MODEL.md` l. 1005 and l. 1082. `DECISIONS.md` outranks the
   code, so the next reader re-adds the variant; the module's tripwire test is the
   only thing standing between the corpus and a tier-1 removal decidable only
   against the receiver's own store.
2. **`THREAT_MODEL.md` row 13 names a disposition the code cannot produce.**
   *replay of old actions | **1** (chain parent does not exist at the replayed
   position) | … **loses its seat*** — with `ParentUnknown` gone there is no
   tier-1 variant it maps to. This is **created by** the code-side fix and is the
   most concrete face of `P4`.
3. **`AgreedCheckpoint` still cannot witness the position half (`Q2`).**
   `covering` checks `hand_id` and both emitters and never reads `sequence`; T64's
   guard is on `c.number >= n` and the struct has no `number`. A witness can be
   built from a checkpoint of the right hand and the right emitters at the
   **wrong position**, judging an early action against late state.

**And one stale claim in the other direction.** `STATE_MACHINE.md` T64 still says
*"`c.required` is what carries §4.9's *'the emitter set contained both the accused
and this receiver'*, **which the type in `src/security/validation.rs` cannot
witness today** (`P6`, filed against that file)"*. The type witnesses it, as of
19:56, twenty-five minutes before that file's own timestamp.

### 6.4 Places a receiver rule says a violation has no consequence

Nineteen sites swept for *"removes nobody"*, *"remove nobody"*, *"no removal"*,
*"nothing further follows"*. **Eighteen correct, one contradicts D-014**, and it is
the same one:

* `PROTOCOL.md` l. 1914, 1949–1957, 3456, 3487, 5828, 5884, 7193 — all carry the
  exception or concern a class D-014 does not touch.
* `THREAT_MODEL.md` l. 1376, 1406 — tier 2's precondition, correct.
* `STATE_MACHINE.md` T66, `hand_id == 0` — a narrowing made by an owner rather
  than by the decision; the reasoning is right (the roster's seat vector is frozen
  at `TABLE_READY`, so removing a seat there forks the genesis) and the exception
  still belongs in D-014 as one line. **Carried a third pass, still not filed.**
* **`NETWORK_STACK.md` l. 97, l. 541, l. 2536 — `P5`.**

`NETWORK_STACK.md` §0.1's transport row remains unaffected and correct, because
D-014's removal is from the **table** and never from the transport.

### 6.5 `NETWORK_STACK.md` §1.2 prohibition 7, as the commission asks

> *"7. **remove, block, unseat, refuse or penalise a peer on the strength of a
> protocol proof.** An `EquivocationProof` (`PROTOCOL.md` §5.2), a timeout
> certificate (`PROTOCOL.md` §8.3), a signed abort attribution or **an invalid
> application signature** are evidence for a human and produce **no transport
> action at all**."*

**The last trigger is `SelfContained::SignatureInvalid`**, the first variant of the
tier-1 enum, whose D-014 disposition is that the sender loses its seat. The
prohibition's *transport* half is correct and must stay; the words *unseat* and
*remove* are the corpus-wide, every-layer form D-014 narrowed for one layer. One
clause fixes it and `DECISIONS.md` l. 1473 already contains that clause.

---

# Part 7 — New defects

Numbered `Q` to avoid collision with `DECISIONS.md`'s `N-*` rows and with the
`P1`–`P8` of `PHASE3_GATE4.md` — which, as `Q5` records, already collide with an
older `P`-series still live in the corpus.

| # | Severity | Defect |
|---|---|---|
| **`Q1`** | **BLOCKER for the freeze path and for readmission** | **The engine reads the readmission set into two places `PROTOCOL.md` deleted it from, and the guard that was holding that back is now unlatched.** §4.9 in this pass makes `A` widen only the *accepted* emitter set — *"`R(HAND_INIT, m+1)` is `P(m)`, unchanged and unenlarged"*, and *"a seat readmitted by this route is **not `dealt_in`** for hand `m+1` — `dealt_in ⊆ R(HAND_INIT, m+1) = P(m)`"*, with `dealt_in ⊆ P(k-1)` normative at §4.4 l. 2557. `STATE_MACHINE.md` §5.3 step 4 still computes `admitted := signed_this_hand ∪ readmit` and `dealt_in[s] := (status == Active ∧ s ∈ admitted)`, still says in bold *"§4.4 makes hand `m+1`'s **required emitter set** `P(m) ∪ A`"*, and **I31(b) asserts the union as an invariant a harness must check**. §5.3 step 8 then writes `solitary_since` only when `\|admitted\| == 1`, reading `admitted` deliberately — an argument that was sound while `A ⊆ P` and is inverted now. **Two payoffs, one root, both keyless and both once-per-hand for 4 096 hands.** (a) `dealt_in` differs from every other peer's, the `HAND_INIT(m+1)` bodies differ, **stage 0 stalls** — `P2`'s stall relocated from `R` to `n(8)`. (b) With the union removed from step 4 only, as `P2-e` instructs, step 8 still reads `admitted`, `solitary_since` is never written while `A` is fed, and **T62 and T63 go dead** — K1's payoff and the unretracted tournament win return in full. **`DECISIONS.md`'s `P2-e` names (a) and not (b), and describes step 4 as already keeping `dealt_in ⊆ P(k-1)`, which it does not.** The mitigation in force — §2.7's *"wire no producer for `Readmitted`"* — is conditioned on `P2` landing, and `P2` landed in this pass. **Fix:** §5.3 step 4 derives `dealt_in` from `signed_this_hand` alone and uses `admitted` only for the acceptance test; §5.3 step 8 reads `signed_this_hand`, with the inverted argument restated; I31(b) restated; `P2-e` extended to name step 8. Owner: `STATE_MACHINE.md` §5.3 steps 4 and 8, I31(b), §2.7; `DECISIONS.md` `P2-e`. |
| **`Q4`** | **BLOCKER for T49, T50, T51 and therefore for the non-solitary freeze carrier** | **`CheckpointState` records no `state_hash`, and `P1` is what made that fatal.** The struct is `{ hand_id, number, sequence, required, heard, values: u8, acked, agreed: Option<Hash> }`, where `values` is *"count of DISTINCT state_hash values seen"* — a derived count with no inputs — and `agreed` is the `checkpoint_hash` from the ACK, not a `state_hash`. **T49's guard is *"the value equals this peer's own derivation at that checkpoint"*; T50's is *"two distinct `state_hash` values now exist for that checkpoint"*; T51's is *"every value in `c` agrees"*.** While there was one slot and it was always the live checkpoint, all three were computable by re-deriving `PublicTableState` from current state. **`boundary` and `agreed` hold checkpoints of a state the engine has left**, and hand `k`'s checkpoint-8 value cannot be re-derived once hand `k+1`'s init has rotated the button, advanced the level and cleared `signed_this_hand`. So the three rows `P1` exists to make reachable cannot compute their guards on the two slots `P1` added — including T50 as the **non-solitary stale-mismatch carrier**, which is `P1`'s own third face, and T51's completion test, which is `N6`'s. I32(d)'s directed case (*"a checkpoint-8 `STATE_HASH` of hand `k` that differs … must reach T50"*) is unrunnable as stated. **Fix:** two fields on `CheckpointState` — `own: Hash`, written when the checkpoint is opened and this peer publishes its own body (T45/T46 and §3.4 already produce it), and `other: Option<Hash>`, the first differing value seen; `values` becomes `1 + other.is_some()` and stops being independent state. Both are fixed-size, three records exist, and the §27 bound is unchanged. Owner: `STATE_MACHINE.md` §2.3, §5.2 (T49, T50, T51), I32(d). |
| **`Q2`** | medium | **`AgreedCheckpoint` witnesses the hand and the emitters and not the position.** `covering(state_hash, sequence, hand_id, emitters, accused, receiver, message_hand_id)` checks `hand_id == message_hand_id` and both emitters, and **never reads `sequence`**. T64's guard is `agreed_checkpoint(hand_id)` is `Some(c)` with **`c.number >= n`**, and the struct has no `number`. So a tier-2 finding can be built against a checkpoint of the right hand and the right emitters at the wrong position — judging a pre-flop action against state fixed at the river — which is the module's own stated failure mode (*"A finding built on that would convict a player who was never party to the state it is judged against"*) with the word *state* replaced by *position*. Not reachable today because `D-014-3` leaves T64 with no producer, and that is exactly why it should land now rather than after. **Fix:** carry `number: u16` beside `sequence`, take the offending message's `checkpoint` number and `sequence` in `covering`, and refuse unless `number >= n` and `sequence <= message_sequence`. Owner: `src/security/validation.rs`. |
| **`Q3`** | medium | **`DECISIONS.md`'s open list describes three applied fixes as owed, and one of them restates the superseded predicate.** (i) **`N-1e`** sits unmarked in the open list reading *"the lemma is false for exactly the hand the fix exists for"* and *"blocking for K-9"* — the fix is in `STATE_MACHINE.md` §2.6, which names `N-1e` in its own text. (ii) **`N-4`** gives the resolution as *"`solitary_at(k) := solitary_since == Some(j) ∧ j <= k <= hand_id` read by T62 and T63 only"* — **the predicate `N-1e` replaced.** An editor following the authority order reads `DECISIONS.md` first and re-derives the wrong inequality. (iii) **`N-9`** still reads *"`CRYPTOGRAPHY.md` carries zero D-014"*; `D-014-1`'s rewritten row says in this pass that *"the open list's `N-9` is stale in saying otherwise"* — **the pass noticed the staleness in one row and did not edit the other**. This is `P8`'s defect class, in the file that outranks everything, created in the pass that fixed `P8`. **Fix:** mark `N-1e` DONE with the form taken (`j <= k + 1`, not `j - 1 <= k`), correct `N-4`'s quoted predicate, close `N-9`. Owner: `DECISIONS.md`. |
| **`Q5`** | low | **The corpus's defect-identifier namespace collides with itself.** `PROTOCOL.md` l. 83 (*"P3 is settled here"*, the witness-independent terminal stage) and l. 5833 (*"this is `P3`"*, the deadline floor) are two different `P3`s in one file. `STATE_MACHINE.md` T50 carries *"(P5, T57)"* — an older `P5` — while `DECISIONS.md`'s `P5` row is `NETWORK_STACK.md`'s prohibition. `STATE_MACHINE.md` §2.3 carries *"(P8, §9.4, Q6)"* against `PHASE3_GATE4.md`'s `P8`. Three collisions found without looking for a fourth, and the failure mode is an editor "fixing" the wrong item — which is precisely how `N-4` came to quote a superseded predicate. **Fix:** the gate series prefixes its items with the gate that opened them (`G4-P3`, `G5-Q1`), and existing references stay untouched. Owner: `research/` convention, `CONTRIBUTING.md` §2.5. |
| **`Q6`** | low–medium | **An advertisement at exactly `HAND_DEADLINE_FLOOR(n)` tolerates zero reopening raises, and §9.4 rule 2a accepts it.** §8.2 derives the floor for the **no-re-raise** walk and derives `REOPENINGS` from the headroom above it; at zero headroom the value is zero, so the first reopening raise of any hand pushes past the deadline and the hand aborts with `cause = 1` and `attributed = []`. The joiner's check is `n(17) >= FLOOR(n(11))`, which passes such a table. A founder who reads §8.2 and sets the deadline to the derived floor — the obvious reading of the word *floor* — ships a conforming table on which every raised hand aborts. `P3-r` names the residual in general and states the bound as *"four reopenings at the preset"*; it does not state that the bound is **zero** at the boundary the validator enforces. **Fix:** one clause in §9.4 rule 2a requiring headroom for at least one reopening — `n(17) >= FLOOR(n(11)) + (n(11) − 1)(n(14) + n(15))` — or one sentence in §8.2 saying the floor is not a recommended value. Owner: `PROTOCOL.md` §8.2, §9.4. |

**The pattern check, run on this pass's own findings.** Five of the six (`Q1`,
`Q2`, `Q3`, `Q4`, `Q6`) are on a path this pass newly made load-bearing, and
**three — `Q1`, `Q3`, `Q4` — are the composition shape**. `Q1` and `Q3` are the
last gate's named shape exactly (a fix and its consumer in one pass, in two
files); `Q4` is a new and worse variant of it, **a fix and its consumer in one
pass, in one file, sixty lines apart**. `Q5` and `Q6` are what the method found
rather than what the fixes broke: a namespace read across two series, and a
boundary value tested rather than assumed.

---

# Part 8 — What the receiver can now build

## The eight build-order items

| # | Item | Build now? | Gap |
|---:|---|---|---|
| 1 | `EventBody` / `SignedEvent`, §2.3's twelve and two fields | **Yes.** Written and green | — |
| 2 | `EventType`, closed `#[repr(u16)]`, 39 codes, four axes | **Yes.** Written and tested | — |
| 3 | Envelope sentinel checks for `chain_scope == 0` | **Yes.** Written; `M-1` closed | — |
| 4 | The 39 payload structs with §9.3 caps and §9.4 bounds | **Yes.** `DISPUTE { kind = 3 }` bound at exactly one `n(3)` entry | — |
| 5 | `slot(E)`, one function, §5.2.1's 8-tuple | **Yes.** One site, literal tuple | — |
| 6 | §4.0 steps 1–11 as `validate_prefix` | **Yes, and 10a/10b are fully pinned.** 10a's skip (`N2`), 10b's set as `P(hand_id − 1)` (`N7`), 10b's readmission branch now specified **and replay-safe** (`P2`) | — |
| 7 | A `constants` module from §13 | **Yes, with one change of kind.** `hand_deadline_ms` is **no longer a constant**: write `HAND_DEADLINE_FLOOR(n)` as a `const fn` over the five config fields and the preset's `3 300 000` as a preset value, never `600_000` | — |
| 8 | Round-trip and canonicality tests over every payload type | **Yes.** §2.5's gate and §2.7's cases are complete | — |

**All eight are implementable, and item 6 no longer carries a `TODO`.** `P2`
closed the one qualification the last gate left on it — but see the readmission
row below for what item 6 must **not** be wired to.

## The freeze path and the newly unblocked items

| Item | Build now? | Gap |
|---|---|---|
| **`PublicTableState`** | **Yes.** Transcribe from **§6.1's table only**, never from §2.9's enumeration — the two orders differ and the struct is a CBOR array (**`P7`**). `signed_this_hand` is in it and **appended last**, per §2.2 rule 5. Expect one append-only field if `D-014-3` lands a `removed` vector | **`P7`**; `D-014-3` open |
| **`state_hash`** | **Yes.** `h("p2p-poker v1 state", [canonical_cbor(PublicTableState)])`, one part, domain string in §2.8's closed register | — |
| **the `checkpoint` field's range** | **Yes — range-check to `1..=8`.** §6.2's eight rows are *"these points and no others"* and the reconciliation stage carries the disputed checkpoint's own number | — |
| **§5.3's three anti-replay stores** | **Yes, all three, with the lifetimes.** `event_class == 0` keyed by `slot(E)`, dropped at hand end, **no store created for a finished hand**; classes 1 and 2 sparse, per stage, keyed `(seat, subject_seat)` and `(seat, subject_digest)`; the `DISPUTE` counter at 8 per sender per hand | — |
| **the checkpoint-8 `STATE_ACK` slice** | **Yes.** `8 × MAX_SEATS = 80` entries, one hand at a time, at `BOUNDARY_CHECKPOINT_BASE + 2r + 1`, `0 <= r <= 7`, retained to `TERMINAL(k+1)`. The one structure that outlives its hand and the one place step 10a is **not** skipped | — |
| **`RetainedHand`** | **Yes.** Four fields; **`was_solitary` is the disjunction and an independent bool, never `p == {self}` and never `\|p\| == 1`** (`N4-a`, now closed in §5.3's struct comment); `p = P(hand_id − 1)`; LRU at 4 096, ≈ 56 B | — |
| **the readmission set `A` (wire half)** | **Yes.** One `SeatSet`, `\|A\| <= MAX_SEATS`, written by step 10b, read once at the next hand init, cleared there, **widening the *accepted* emitter set only**. Accept an out-of-set `HAND_INIT(m+1)` copy from a seat in `A` and count it into `P(m+1)`; **completion stays `heard ⊇ P(m)`** | — |
| **`Readmitted` (engine producer)** | **NO — do not wire it.** §2.7's deferral stands even though its stated condition is met: the two engine readers are written for the deleted rule (**`Q1`**). Build `A` on the wire, build the `readmit` field, wire no producer, and add a test that asserts `readmit` is empty on every trace | **`Q1`** |
| **`CheckpointStore` (the three slots)** | **Yes for the shape** — `live`, `agreed`, `boundary`, `named(h, n)`, `agreed_checkpoint(h)`, the lifetimes at T45/T46/T47/T61, and the §27 bound of three. Write the accessors as the **only** route into the store | — |
| **`CheckpointState` (the record)** | **NO — two fields short.** T49, T50 and T51 cannot compute their guards against a non-live record. Add `own: Hash` and `other: Option<Hash>` first (**`Q4`**) | **`Q4`** |
| **T49 / T50 / T51** | **NO**, for `Q4`. T50 is the non-solitary stale-mismatch carrier and T51 is D-014 tier 2's precondition, so this is the gate on both | **`Q4`** |
| **`PrefixOutcome` with `SolitaryDivergence`** | **YES — build it, and wire the engine consumption.** `SolitaryDivergence { hand_id, seat, event_hash }`; T62's guard is `solitary_at(e.hand_id)` with `j <= k + 1`; T63 is the same guard in `TableClosed` with `settlement.tournament_winner := None`. Both walks in Part 2 fire and hold | — |
| **§4.0 steps 12, 12a** | **YES — no longer deferred.** Step 12's solitary-stage conjunct, step 12a's two arrival exemptions, and the second exemption's *"from arrival and not from disagreement"* clause are all pinned, and the engine guard that consumes them is correct | — |
| **the freeze's release** | **YES for the shape, with `N1-a` chosen explicitly.** §6.3 step 3's release is exhaustive, `R(c) ∪ W` with `\|R\| >= 2` is the required set, I33(b)'s three-message exemption is what the frozen peer may publish. **Choose `N1-a` before writing T53**: whether §4.9 admits an out-of-set copy into a reconciliation round. Both answers are safe; the unreachable-exit state is not | **`N1-a`**, one sentence |
| **§4.0 steps 13, 14, 15** | Still deferred; they need the engine | — |
| **`TIMEOUT_VOTE`, `TIMEOUT_CERT`, `EquivocationProof`** | Unchanged. `OQ-F` may delete them; write the payload structs, wire no behaviour | `OQ-F` |
| **`SHOWDOWN_MUCK` policy** | Unchanged. `Q-01` | `Q-01` |
| **T64 / T65 producers** | **NO.** `D-014-3` is unanswered: a removal is canonical per-seat state that no hashed vector carries, so a removal has no route to canonical state. The rows are specified and nothing fires them, which is the honest state | `D-014-3` |
| **`src/security/validation.rs`** | **One edit owed, and one edit owed *to* it.** Owed by the file: `Q2` — carry `number`, check position. Owed **to** the file: `P4` — the documents must be brought to the code's nine variants, not the code back to the documents' ten | **`Q2`**, **`P4`** |

**Named gaps by section.** `STATE_MACHINE.md` §5.3 steps 4 and 8, I31(b), §2.7
(**`Q1`**, blocking); `STATE_MACHINE.md` §2.3 and §5.2's T49/T50/T51 and I32(d)
(**`Q4`**, blocking); `DECISIONS.md` D-014's tier-1 list plus `PROTOCOL.md` §4.0
and §4.10 and `THREAT_MODEL.md` §5.1/§5.2 (**`P4`**); `NETWORK_STACK.md` §0.1,
§1.2 prohibition 7, §11.5.1 (**`P5`**); `PROTOCOL.md` §2.9's field enumeration
(**`P7`**); `src/security/validation.rs` (**`Q2`**); `DECISIONS.md`'s `N-1e`,
`N-4`, `N-9` rows (**`Q3`**); `PROTOCOL.md` §9.4 rule 2a (**`Q6`**);
`STATE_MACHINE.md` §9.4's validator line and a dozen `600 000` figures
(**`P3-e`**, filed).

## What must land before the freeze path is written

**Two, and both are corrections to this pass's own fixes.**

1. **`Q4` — `STATE_MACHINE.md` §2.3.** Two fields on `CheckpointState`. Without
   them T50 has no computable guard on the record `P1` added for it, so the
   non-solitary stale mismatch has a carrier that cannot decide, and T51 cannot
   complete a boundary `STATE_ACK` stage, which leaves D-014 tier 2 exactly as
   unsatisfiable at a boundary as `N6` said it was.
2. **`Q1` — `STATE_MACHINE.md` §5.3 steps 4 and 8.** Until they read the same rule
   `PROTOCOL.md` §4.9 now states, `Readmitted` must have no producer, and the
   deferral protecting that is written against a condition that has already been
   met.

**The solitary half of the freeze path — `SolitaryDivergence`, T62, T63, §4.0
steps 12 and 12a — can be written today**, and both interleaving variants confirm
it fires on the right event and holds.

**Three more are one line each and should land with them:** **`P4`** (delete the
clause from D-014's tier-1 list and its three consumers, and re-classify
`THREAT_MODEL.md` row 13), **`Q3`** (three open-list rows), **`P7`** (one clause in
§2.9). **`P5`** is `NETWORK_STACK.md`'s, is fully specified in `DECISIONS.md`
l. 1473, and needs the file to be opened. **`Q2`, `Q5`, `Q6`, `N1-a`, `P3-e`,
`P3-r`** are the next pass's, and **`Q1` is the one most likely to be its
blocker**: it is a fix's *readers* invalidated by the fix, in the other document,
in the same pass — which is the mirror of the shape the last gate named and the
one no single-document review can see.
