# PHASE4_GATE.md — the receiver build gate

**Commission.** Thirteen passes. The twelfth (`PHASE3_GATE5.md`) closed `K1` and
`P3` and left two blockers with one root: `P2`'s wire fix had landed and its
engine half had not, so the engine still read the readmission set `A` into
`dealt_in` and into `solitary_since`, and `K1`'s payoff came back through the
engine. This pass decides whether the receiver can now be written, item by item.

**Method, unchanged, and it is what produced this pass's findings.** Nothing is
graded off a summary row, off a disposition column, or off `DECISIONS.md`'s own
account of what landed. Every verdict below was re-derived from the normative
text: `STATE_MACHINE.md` §5.3 steps 4 and 8 were read before `DECISIONS.md`'s
`P2-e` row; `CheckpointState`'s fields were read before `Q4-e`'s; `NETWORK_STACK.md`
§0.1, §1.2 and §11.5.1 were opened rather than inferred from the row that
specifies them; the deadline floor was recomputed at five seat counts from the
preset's own fields **and then compared against the preset the code ships**.
That last comparison is the one that produced this pass's blocker, and it is a
direction the series has not swept before: **every previous pass compared
document against document, and this one compared document against code in the
place where the code had moved last.**

**Files as read, 2026-08-28.** `DECISIONS.md` **21:03**, `STATE_MACHINE.md`
**21:03**, `NETWORK_STACK.md` **21:00 — opened for the first time in three
passes**, `PROTOCOL.md` 21:00, `CRYPTOGRAPHY.md` 20:59, `THREAT_MODEL.md` 20:53,
`DEPENDENCIES.md` 19:33 — **unchanged, second consecutive pass**, `CONTRIBUTING.md`
19:32 — **unchanged, second consecutive pass**, `SPEC_CS.md` 09:29.
`src/poker/tournament.rs` **20:47 — edited after the last gate was written**,
`src/security/validation.rs` 19:56 — **unchanged, second consecutive pass**.
`cargo check --lib -j 19` clean; `cargo test --lib -j 19 -- --test-threads=19` —
**125 passed, 0 failed** (120 last pass; the five new ones are
`tournament.rs`'s deadline-floor tests).

> **The timestamp finding, ninth iteration, and it has finally inverted.**
> `NETWORK_STACK.md` was the one file not opened for three passes and carried
> **zero** `D-014`; it was opened in this pass and now carries **34**, and `P5`
> closed exactly as `DECISIONS.md` l. 1473 had specified it four passes earlier.
> The prediction the rule makes now points somewhere else: **the two files not
> opened this pass are `CONTRIBUTING.md`, `DEPENDENCIES.md` — and
> `src/security/validation.rs`, which has been the named owner of an open item
> (`Q2`) for two passes and has not been touched in either.** The rule's other
> half — *the file edited last is where the unpaid debt is* — points at
> `src/poker/tournament.rs`, edited at 20:47, three minutes after the last gate
> was written and thirteen minutes before the document that owns its numbers.
> **That is where this pass's blocker is.**

---

## Verdict counts

| Verdict | Count | Items |
|---|---:|---|
| **RESOLVED** | 8 | N5, P2, P4, P5, Q1, Q3, Q4, Q5 |
| **PARTIAL** | 1 | P7 |
| **UNRESOLVED** | 2 | Q2, Q6 |
| **REGRESSED** | 0 | — |

**This is the strongest pass in the series and the first whose failures are not
the composition shape.** Both blockers closed. The engine now reads
`signed_this_hand` alone in both places `P2` narrowed, `readmit` reaches no guard
in the document at all, and `CheckpointState` carries `own` and `dissent` so
T49, T50 and T51 can compute their guards on the two slots `P1` added for them.
Four items that had been open for two to four passes closed with them: the
parent clause is out of tier 1 in every document and `THREAT_MODEL.md` gained a
new §5.1.1 stating the admissibility test that would have caught it; the
transport document carries D-014; the three stale open-list rows are corrected;
and the identifier namespace has a convention.

**And the corpus's numbers and the code's numbers have parted.**
`src/poker/tournament.rs` ships `hand_deadline_sec: 2_700` — 2 700 000 ms — where
`PROTOCOL.md` §13 is normative that `hand_deadline_ms = 3 300 000`, and §9.4
rule 3 makes the preset **name** imply the preset's exact values. That is
**`G6-R1`**, it is a blocker for the constants module and for `PublicTableState`'s
`table_params_hash`, and it is the `P4` inversion repeated with the roles
reversed: last pass the code moved first and was right, this pass the code moved
first and is wrong.

**Six new defects, `G6-R1`–`G6-R6`. One blocks.**

* **`G6-R1` — the shipped preset and the normative preset are different tables.**
  Two clients, one built from §13 and one from `tournament.rs`, advertise the
  same `preset_id` with different `hand_deadline_ms`, compute different
  `table_params_hash`, and §9.4 rule 7 marks each other's table unjoinable.
* **`G6-R2` — §2.6's residual says it is filed and it is not.** The `N-1e`
  boundary-window residual ends *"It is recorded on `DECISIONS.md`'s open list
  rather than fixed here"*; there is no such row. **The last gate graded that
  residual *"Recorded rather than fixed, which is the right call"* — on the
  strength of the sentence, without checking the file the sentence points at.**
  That is this gate's own method failing in the one direction it is built to
  catch, and it is recorded here as a finding against the method as much as
  against the corpus.

**Readiness verdict: READY for every item on the build list except the
constants module, which must not be written until `G6-R1` picks a number.**
Part 8 gives the per-item answer.

---

# Part 1 — Per-item verdicts

## N5 — K-3b's readmission route does not survive step 10b — **RESOLVED**

**Both halves now state one rule and neither restates the other.** The wire half
was superseded by `P2` last pass; the engine half is present in this one.
`STATE_MACHINE.md` §5.3 step 4, l. 2860:

> 4. **Derive `dealt_in` from the required emitter set, and from that set alone.**
>    `dealt_in[s] := (status == Active ∧ s ∈ signed_this_hand)` … **`readmit` is
>    not read into this**, and the same step hands it over

with a normative box that names its own scope — *"`dealt_in ⊆ P(m)` is exact, and
the readmission set is not in it"* — and, better, keeps `PROTOCOL.md` §4.9's
letters `m` and `m+1` rather than translating them, *"so the two halves can be
read against each other word for word"*. That is D-011 rule 1 applied to
**notation**, which the series has not seen before and which is the cheapest
possible defence against the exact drift `Q1` was.

**What replaces the union is a hand-off and it is typed as one.** `readmit` is
read once, at step 4, and handed to the protocol layer as the accepted-emitter
widening for the `HAND_INIT(m+1)` stage; step 8 clears it. The document states
the consequence in the form an implementer needs: *"no guard in this document
reads `readmit` at all"* — which is checkable by grep and which I ran (below).

**The cost is stated and it is one hand**, and the mirror argument the deleted
union made is answered rather than dropped: *"widening only the emitter set makes
a readmitted seat a required emitter of a hand it takes no cards in … That is
true of a **required** set and it is exactly why `P2` does not widen one."*

## P2 — the readmission set is a replay amplifier — **RESOLVED**

**The engine half landed, in both places, and the second place is the one that
mattered.** §5.3 step 8, l. 3007:

> if `|signed_this_hand| == 1` then `solitary_since := solitary_since.or(Some(hand_id))`.
> **It reads `signed_this_hand` and never `signed_this_hand ∪ readmit`, and that
> is `P2-e` and the half of `K1` this document owed.**

with the inverted argument restated correctly: *"since `P2` the required set is
`P(k-1)` exactly — `A` widens the **accepted** set and never the required one —
so a peer holding `P(k-1) == {self}` and a non-empty `readmit` is **still**
comparing its state against a set of one."* §2.6 carries the same sentence at the
predicate, I31(b) carries it as an invariant, I33(a) carries the directed test,
and T67's cell carries it as the reason the row writes a set that reaches nothing.

**`DECISIONS.md`'s `P2-e` row is marked DONE and its cell names both halves**, which
is the half the last gate found missing. It also states, correctly, the one thing
that keeps T67 alive with no consumer: *"its `status ∉ {Removed, Empty}` conjunct
is what keeps §4.9's `A` from being the re-entry route D-014's one-way exit
forbids (I34(a))"* — the wire has no `status`, so that filter is the engine's
whole contribution to a mechanism it otherwise derives nothing from. Keeping a
transition for one conjunct, and saying so, is the right call and is argued
rather than assumed.

**`I31(b)` now asserts the deletion rather than the union, and the reason given
is the right one**: *"An implementation that kept the union passes every ordinary
test — `readmit` is empty on every trace with no returning seat — and fails
exactly that one."* That is a falsifiable direction where the previous form was a
tautology on every trace a random harness generates.

## P4 — D-014 tier 1 contains a clause that is not self-contained — **RESOLVED**

**All four document sites are corrected, the code was already right, and the
correction produced a new normative section rather than four edits.**

| Site | State |
|---|---|
| `DECISIONS.md` D-014's own tier-1 list, l. 1336 | **deleted**, with the reason, and *"may not return"* |
| `PROTOCOL.md` §4.0's box, l. 2002 | **deleted**, with the tripwire test named |
| `PROTOCOL.md` §4.10's `cause = 6` row, l. 3797 | **deleted**, and the list is now *"closed and every member is decidable from the offending event's own bytes"* |
| `THREAT_MODEL.md` §5.1's D&A cell, l. 1069 | **deleted** |
| `THREAT_MODEL.md` §5.2 row 13, l. 1138 | **re-classified to `none`**, not merely edited |

Row 13 is the part worth naming. *Replay of old actions* was tier 1 **because of**
the parent check, so deleting the clause left it with no tier; the pass gave it
**none** and named the mechanism that actually answers it — CP, the signed body
binding `hand_id`, `sequence` and `previous_event_hash`, so a replayed event is
valid at exactly one position and is rejected and named. **A row that loses its
disposition and is given a different mechanism rather than a weaker tier is the
correct handling**, and it is the one the last gate said was owed.

**And the pass wrote the rule instead of the fix.** `THREAT_MODEL.md` **§5.1.1** is
new and is the admissibility test a proposed tier-1 addition must pass — three
questions, all of which must be answered *no*: does deciding it read anything the
receiver stores; can the answer change with what the network did; does it need a
second message, a count, a vote or a certificate. It is placed in the document
that owns the classification scheme, every other site points at it, and it names
the confusion the parent check arrived in — *local* and *receiver-independent*
being different properties. **That is the difference between closing a defect and
closing its class**, and it is the best single edit in this pass.

`src/security/validation.rs`'s nine variants and the
`no_tier_one_violation_depends_on_the_receivers_store` tripwire are unchanged and
now agree with every document.

## P5 — `NETWORK_STACK.md` §1.2 prohibition 7 — **RESOLVED**

**The file was opened and the edit is the re-scoping `DECISIONS.md` l. 1473
specified, not a weakening. No transport behaviour changed.** The document went
from **zero** `D-014` to **34**.

* **§0.1, l. 115** — *"No automated eviction **at this layer**, ever"*, with the
  exception named and not restated: *"The one exception is not this layer's, and
  it takes nothing away from this box."*
* **§1.2 prohibition 7, l. 683** — *"remove, block, unseat, refuse or penalise a
  peer **at this layer** on the strength of a protocol proof"*, and then the
  sentence that makes the scope operative rather than decorative: *"This
  prohibition is about the *reason*, not the action."* The invalid-application-
  signature trigger stays in the list, correctly, with *"removes its sender from
  the **table** … and never from a socket, a dial queue, a relay reservation or a
  list of names here."*
* **§11.5.1, l. 2686** — the box is **unchanged**, which is the opposite edit and
  is the right one, plus the paragraph the last gate said was owed: *"**Arriving
  at T64 is not licence to call `block_peer`**, and there is no path from a
  tier-1 verdict into this section."*

**The sweep row at §0.4 and the D-014 row at l. 2973 both state the disposition as
*"no change to any behaviour, and a change to two scope words"***, which is the
honest summary and is what makes this a documentation fix rather than a silent
policy change. `block_peer` still has exactly one caller: the user (§11.5.3).

## P7 — `PublicTableState`'s field order is given twice, differently — **PARTIAL**

**The hazard is closed for the implementer and the defect is still in the file
that owns it.** `STATE_MACHINE.md` §3.3 adds a normative box:

> **Normative for the engine: `PROTOCOL.md` §6.1's table, read top to bottom, is
> the field order the engine builds `PublicTableState` in.** No other listing
> anywhere is a field order and none may be read as one … `src/protocol/messages.rs`
> reads §6.1's table and nothing else.

That settles what an implementer transcribes, adds no fourth listing, and is
explicitly *"not a ruling on the disagreement itself"* — D-011 rule 1 observed
under pressure, which is where it is usually dropped.

**`PROTOCOL.md` §2.9 l. 795 is verbatim as the last three gates quoted it**, ending
*"`transcript_head`, `signed_this_hand`, and the `Vec<bool>` flag vectors"*, against
§6.1's table placing the four flag vectors between `committed_this_hand` and
`current_bet` and `signed_this_hand` last. Two orders, one `#[cbor(array)]`
struct, two `state_hash` values.

**What lifts this from UNRESOLVED to PARTIAL is that the filing is now real.**
`P7-w` is on the open list, carries both line references, names the exact remedy
(*"`PROTOCOL.md` **say which of its own two lists is normative in one place and
delete the other***", with §2.9's identified as the one to go), and — the part
worth crediting — **corrects a false filing claim it found in the process**:
§3.3 had said the defect *"is recorded on `DECISIONS.md`'s open list in this
pass"* and it was not. A row that records the defect **and** the fact that an
earlier claim of recording it was untrue is the discipline `G6-R2` shows is
otherwise missing.

## Q1 — the engine implements the deleted readmission rule — **RESOLVED**

The blocker. Both readers are re-derived, the guard is lifted with an argument
rather than by omission, and the invariant is restated.

* **§5.3 step 4** derives `dealt_in` from `signed_this_hand` alone (above).
* **§5.3 step 8** reads `|signed_this_hand| == 1` (above).
* **I31(b)** deletes the union and states the falsifiable direction (above).
* **§2.7's `Readmitted` box, l. 1129**, lifts the deferral in terms:
  *"**`Readmitted` may therefore be wired**, which the previous pass forbade."*
  The condition it was written against — `P2` landing — is now met **in both
  documents**, which is what it always should have said and what it did not.
* **T67, l. 2447** states the consequence at the row: *"**Since `P2` the set it
  writes reaches no guard at all** — not `dealt_in`, not `solitary_since`, not
  §9.3."*

**I ran the check the document invites.** `grep -n admitted` over all seven
documents returns no live reader: every occurrence in `STATE_MACHINE.md` is
past-tense narration of the deleted union (l. 640, 1633, 2469, 3015, 5070, 5077,
5079, 5699, 5707), and every occurrence elsewhere is unrelated (`NETWORK_STACK.md`'s
admitted-peer set, `PROTOCOL.md`'s exemption prose). **The union is gone from the
corpus, not merely from the two sites the item named.**

## Q2 — `AgreedCheckpoint` witnesses the hand and the emitters and not the position — **UNRESOLVED**

`src/security/validation.rs` is **unchanged at 19:56**, second consecutive pass.
Read in full again:

```rust
pub struct AgreedCheckpoint {
    state_hash: Hash,
    sequence: u64,
    hand_id: u64,
    emitters: Vec<PlayerId>,
}
```

`covering(state_hash, sequence, hand_id, emitters, accused, receiver,
message_hand_id)` checks `hand_id == message_hand_id`, `emitters.contains(accused)`
and `emitters.contains(receiver)`, and **never reads `sequence`**. T64's guard is
`agreed_checkpoint(hand_id)` is `Some(c)` with **`c.number >= n`**, and the struct
has no `number`. A tier-2 finding can still be built against a checkpoint of the
right hand and the right emitters at the **wrong position** — a pre-flop action
judged against state fixed at the river.

**Not reachable today, because `D-014-3` leaves T64 with no producer**, and that
is still the reason it should land now rather than after: the fix is two fields
and two comparisons in a file with ten tests, and it is much cheaper before a
producer exists than after.

**And the fix is now mis-signposted from the other side — see `G6-R3`.**

## Q3 — three open-list rows describe applied fixes as owed — **RESOLVED**

All three, and one of them names its own error.

* **`N-1e`** is marked **DONE**, states the form the repair took (`j <= k + 1`),
  states why it is not the form the row proposed (*"the slack is written on `j`,
  not on `k` … an editor re-deriving it from a bound on `k` re-derives it
  wrongly"*), and instructs the reader which text is normative — *"Anyone reading
  this row for the predicate must take the `j <= k + 1` form; the paragraph below
  is kept for the derivation only."* Keeping the superseded text **and** telling
  the reader it is superseded is the right handling of a row that is also a trail.
* **`N-4`**'s quoted predicate is corrected in place, with the correction called
  out: *"`j <= k` is the superseded predicate — it is precisely the conjunct
  `N-1e` proved false … and an editor following the authority order reads
  `DECISIONS.md` before `STATE_MACHINE.md` and would have re-derived it."*
* **`N-9`** is **CLOSED**, both halves, and the row records the meta-defect:
  *"this row was stale in the pass that said so about its own twin."*

## Q4 — `CheckpointState` records no `state_hash` — **RESOLVED**

The second blocker, and the fix is smaller and better than the item asked for.

```
pub own:     Hash,            // THIS peer's own state_hash, fixed when the record
                              // is opened and never rewritten (Q4)
pub dissent: Option<Hash>,    // the FIRST accepted value that differs from `own`
```

`values: u8` is **deleted**. The three guards are re-derived over the same
triggers and targets:

| Row | Guard now |
|---|---|
| **T49** | `e.state_hash == c.own` |
| **T50** | `e.state_hash != c.own`, with `c.dissent := c.dissent.or(Some(e.state_hash))` |
| **T51** | `c.heard ⊇ c.required` ∧ `c.dissent.is_none()` |

**Three things make this better than the two-field patch the last gate
proposed.** First, the document states that `values: u8` was not merely missing
its inputs but was **undefined on every slot including `live`** — *"to decide
whether an arriving hash increments it you must compare it against what was
already seen"* — so the fix repairs a guard that never worked rather than one
`P1` broke. Second, **T49 and T50 are now exhaustive and disjoint on
`round == 0`**, which the document names: *"what makes the pair a total function
of the arriving value and removes the ordering dependence the counter had."*
Third, **two values are argued sufficient rather than assumed**: this peer's own
copy is always in the stage (I32(a)), so the number of distinct values is exactly
`1 + dissent.is_some()`, and a third distinct value *"adds a third signature to a
divergence already detected"* on a record whose row has already fired and whose
phase excludes re-entry.

**`own`'s write sites are stated at every opening, not only in the field
comment.** T45 and T46 carry `own: <the state_hash of the body just published>`
with *"**`own` written here and never again (`Q4-e`)**"*; §2.6's `live` bullet
carries the checkpoints 1–7 case — *"the engine writes `live` when it publishes
its own `STATE_HASH` body for that point (§3.4), and `own` is the hash of that
body — one write, at the opening, for every checkpoint number."*

**I32(d) is runnable as stated**, which it was not last pass, and it now asserts
the two fields directly with the failure mode named: *"an implementation that
re-derived it from the current `TableState` instead would pass every single-hand
trace and fail every boundary one."*

## Q5 — the defect-identifier namespace collides with itself — **RESOLVED**

Handled as a convention plus a rename, and the choice of **which** series to
rename is argued rather than taken by convenience: the fourth gate's `P1…P8`
become `G4-P1…G4-P8` because *"the original series is load-bearing inside
`STATE_MACHINE.md`'s normative text and inside archived gate reports that are
records and are not edited; renaming those would falsify the trail."* Six rows
carry the prefixed form with the old label kept in the cell *"so a search for the
remembered string still lands."*

**It also found a second form of the same collision the item did not name** —
`N1`/`N-1e`, `N4`/`N-4`, `N8`/`N-8`, `N9`/`N-9`, `P4`/`P-4`, five pairs of
identifiers for one defect each — and **`G5-Q5-x` files the citation sweep that
is owed rather than performing it from the wrong owner** (D-011 rule 1), with
the interim rule an editor can actually use: *"Until the sweep runs, the
disambiguator is the subject, not the label."* `G5-Q5-x` is carried open and is
correct to be.

## Q6 — an advertisement at exactly the floor tolerates zero reopening raises — **UNRESOLVED**

`PROTOCOL.md` §9.4 rule 2a, l. 5597, is verbatim as the last gate quoted it:
`n(17) hand_deadline_ms >= HAND_DEADLINE_FLOOR(n(11))`, with no headroom clause.
§8.2's `REOPENINGS` box, l. 5921, is unchanged and still says *"At the §13
preset's new value this is four at ten seats"* without stating that the value is
**zero** at the boundary the validator enforces. `G4-P3-r` states the residual in
general and does not state the boundary case.

**And the code has taken a position the documents have not.**
`src/poker/tournament.rs` carries two tests that are exactly this item —
`the_shipped_presets_can_afford_at_least_one_reopening_raise` and
`a_deadline_at_exactly_the_floor_affords_no_reopening` — so the **implementation**
enforces a rule for its own presets that **no document requires of a joiner**.
That is the right instinct in the wrong layer: a shipped preset passing a private
test does nothing about a founder's advert, which is the whole of what `Q6` is
about. One clause in rule 2a closes it.

---

# Part 2 — The replay check, run again

**Setup, as commissioned.** Ten consecutive hands. Once per hand, an observer
replays (a) one signed, **agreeing** checkpoint-8 `STATE_HASH` of a finished hand
`m` from a seat `X` outside the target receiver's retained `P(m−1)`, and
separately (b) one stale `0x0804 PLAYER_SIT_IN` from `X` in chain `m`'s boundary
window. No key is used in either; both are messages `X` genuinely signed and
§1.5 permits any peer to forward. The retained record keeps both evaluable for
`MAX_RETAINED_HAND_RECORDS = 4 096` hands.

**The wire is unchanged from the last pass and is still closed at its input.**
§4.0 row 10b, l. 1837, routes both: step 10a is skipped (`N2`), the agreeing
checkpoint-8 copy and the boundary-window `PLAYER_SIT_IN` each add `X` to §4.9's
readmission set `A`, and neither is applied, chains anything, or enters a
`stage_hash`. `R(HAND_INIT, m+1)` is `P(m)` and completion is `heard ⊇ P(m)`.

**The engine is where it failed last time, and all three answers have flipped.**

| Question | Route (a), agreeing `STATE_HASH` | Route (b), stale `PLAYER_SIT_IN` |
|---|---|---|
| `PrefixOutcome` at the wire | `A ∪= {X}` | `A ∪= {X}` |
| Engine event | `Readmitted{X}` → **T67**: `readmit ∪= {X}` and nothing else | identical |
| §5.3 step 4 reads `readmit`? | **no** — `dealt_in[s] := status == Active ∧ s ∈ signed_this_hand` | **no** |
| §5.3 step 8 reads `readmit`? | **no** — `|signed_this_hand| == 1` | **no** |

**Hand by hand, both routes, ten hands:**

| Hand | `readmit` at init | `dealt_in` vs every other peer | `n(8)` bodies | Stage 0 | `solitary_since` when `P(m) == {self}` |
|---:|---|---|---|---|---|
| 1 | `{X}` | **identical** | **byte-identical** | **completes** | **`Some(m)` — written** |
| 2 | `{X}` | identical | byte-identical | completes | written |
| … | `{X}` | identical | byte-identical | completes | written |
| 10 | `{X}` | **identical** | **byte-identical** | **completes** | **written** |

**The three commissioned questions, answered.**

1. **Does `dealt_in` diverge?** **No, on either route, at any of the ten hands.**
   Step 4's only input is `signed_this_hand`, which is a pure function of the
   accepted chain, so two peers with the same accepted prefix compute the same
   vector whether or not one of them holds `X` in `readmit`. `dealt_in ⊆ P(m)` is
   exact, and §4.4's *"`dealt_in` is a subset of `P(k−1)`, always"* (l. 2552,
   l. 2577) now holds *"with no union to check it against"*.
2. **Does stage 0 stall?** **No.** The required set is `P(m)` at the wire and the
   engine derives nothing that could enlarge it. The `HAND_INIT(m+1)` bodies are
   byte-identical, so no peer rejects another's copy on body mismatch, and the
   completion test `heard ⊇ P(m)` is reached in the ordinary round trip. The
   3 300 000 ms (or 2 700 000 ms — `G6-R1`) per-hand stall of the last pass is
   gone, ten times out of ten.
3. **Is `solitary_since` written when `K1` needs it?** **Yes.** Step 8 compares
   `|signed_this_hand| == 1`. A peer whose `P(m−1)` has narrowed to `{self}` and
   whose `readmit` holds `X` still reads a one-member set and writes
   `solitary_since := solitary_since.or(Some(m))`. `solitary_at(k)` is therefore
   true across the whole episode, and **T62 and T63 are live**, which is the
   condition Part 3's freeze depends on.

**What the replay still buys the attacker, stated because a bounded residual left
unnamed is read next pass as a closed one.** It re-opens, once per hand, an
*acceptance* for `X`'s own `HAND_INIT(m+1)` copy — a `SeatSet` bit and a map
lookup. If `X` is genuinely there, `X` is accepted into `P(m+1)` and is dealable
at `m+2`; if `X` is not there, no copy arrives and nothing happens at all. **The
amplifier has no input in either direction**, which is what §4.9 claims — *"What
closes it is the signature, not a counter"* — and which the walk now confirms in
the engine as well as on the wire.

**One thing the walk exercises that the document should say and does not.**
Route (b) needs no matching requirement — the stale `PLAYER_SIT_IN` route accepts
any boundary-window copy — and it is therefore still the cheaper feeder. It costs
nothing now, but it is the input that would re-open both payoffs the instant any
future reader of `readmit` is added. **The defence is that `readmit` has one
writer, one reader and no guard, and I31(a) asserts exactly that**; an
implementer adding a second reader has to pass through the invariant. That is
adequate, and it is the reason no further mechanism is asked for here.

---

# Part 3 — Both K1 interleaving variants, confirmed after this pass's edits

Three seats, `A` (0), `B` (1), `C` (2). Hand `k−1` completes normally,
`P(k−1) = {A,B,C}`. Adversary budget: dropped frames only. Walked at `A`. The
prefix is `PHASE3_GATE5.md` §2.1's, re-derived against the edited text; every
step below was re-read at its current line rather than carried.

**Common prefix, re-checked at the two steps this pass touched.** `C`'s
`PLAYER_LEAVE` reaches `A` and not `B`, and adds nothing to `signed_this_hand`
(§5.3 step 4(ii), l. 2952). Hand `k`'s init at `A` reads
`|signed_this_hand| == 3` — **not `|admitted| == 3`; the quantity changed and the
value did not** — so step 8 writes nothing. T47 does
`checkpoints.boundary := checkpoints.live.take()`. Hand `k` stalls at stage 0 and
aborts at `hand_deadline_ms` through T57 → T46, which sets
`checkpoints.boundary := None` and opens hand `k`'s checkpoint 8 in `live` with
`required = signed_this_hand` **and `own = <the state_hash of the body just
published>`** — the field `Q4` added, written here. `P(k) = {A}` at `A` and
`{B}` at `B`. `A`'s checkpoint-8 window self-completes, **§5.3 step 8 writes
`solitary_since := Some(k+1)`** by the second disjunct, T47 moves hand `k`'s
record to `boundary`, and `RetainedHand(k)` is written with `was_solitary = true`
and `checkpoint8_state_hash(k)` = `A`'s own value.

**State at `A` entering the contradiction:** `solitary_since = Some(k+1)`;
`checkpoints.boundary = ckpt8(hand k, required = {A}, own = A's value,
dissent = None)`; `readmit = ∅`.

## 3.1 Variant 1 — `B`'s `HAND_INIT(k+1)` copy arrives

`B`'s checkpoint-8 `STATE_HASH` for hand `k` arrives first. `hand_id = k`, a hand
`A` has completed → step 10a skipped (`N2`) → step 10b, checkpoint-8 branch →
compared against the retained `checkpoint8_state_hash(k)` → **mismatch** → enters
§6.3 step 1 and, because the record says solitary, routes to step 12a with it.
Step 12a emits `SolitaryDivergence { hand_id: k }`.

**T62's guard is `solitary_at(k)` = `solitary_since == Some(k+1) ∧ (k+1) <= k+1 ∧
k <= hand_id` → true.** T62 fires: `Diverged`; `Fault{SolitaryDivergence}`
recording `e.seat` and `e.event_hash`; `solitary_contradicted := true`; the
offending event is not applied and not counted into `signed_this_hand`; the hand
deadline is not disarmed.

The freeze holds by I33(b) and I33(c): `A` may publish only its `DISPUTE`, its
reconciliation `STATE_HASH` and the matching `STATE_ACK`; the reconciliation
stage is required of `R(c) ∪ W = {A,B}`, so `|R| = 2` and `A` cannot complete it
alone; **T53's two-signer conjunct refuses a one-signer completion
independently.** `B`'s `HAND_INIT(k+1)` copy then changes nothing — `A` is
already in `Diverged`, T62 does not re-enter, T49/T50 are excluded from the
phase, and I33(c) is satisfied because no hand was dealt between the two entries.

**Verdict, variant 1: the freeze fires on the checkpoint-8 `STATE_HASH` and
holds. Unchanged, and confirmed against the edited step 4 and step 8.**

## 3.2 Variant 2 — only the checkpoint-8 `STATE_HASH` of hand `k` ever contradicts

Every step above runs identically, because **the event that fires the freeze in
variant 1 is the checkpoint-8 `STATE_HASH` itself**, not the `HAND_INIT(k+1)`.
Dropping the `HAND_INIT` copy removes nothing.

* The freeze fires on the same event; `j = k + 1 <= k + 1`.
* It holds by the same route; `R(c) ∪ W = {A,B}`.
* `A` never reaches a solitary tournament win: §9.3 condition 0.6 is evaluated
  before conditions 1 to 4 while `solitary_contradicted` is set.
* If the copy arrives after `A` has closed, **T63** has the same guard, now true,
  and `settlement.tournament_winner := None`.

**Verdict, variant 2: the freeze fires and holds; the tournament win is retracted
if the copy arrives after closure. Unchanged.**

## 3.3 The input that reversed both verdicts last pass, re-run

`PHASE3_GATE5.md` §2.4 recorded the one input that reversed both walks: *"Feed
`readmit` and step 8 writes nothing: `|admitted| = 2`, `solitary_since` stays
`None`, `solitary_at(k)` is false, and T62 and T63 both reject."*

**Re-run against the current text: feed `readmit` at hand `k+1`'s init and
nothing changes.** Step 8 reads `|signed_this_hand| == 1`, which is still one at
`A`, so `solitary_since := Some(k+1)` is written regardless of `readmit`'s
contents; `solitary_at(k)` is true; T62 and T63 both fire. **The reversal is
gone, and both variants hold against the feed that used to break them.**

I33(a) now asserts exactly this as a directed harness case — *"deliver a
`Readmitted` into a peer whose `P` has narrowed to itself and require
`solitary_since` to be written at the next hand init anyway"* — with the reason
it must be generated directly rather than by random play. **That is the test that
would have caught `Q1` in the pass that created it**, and it is now written down.

## 3.4 The non-solitary walk, which is where `Q4` bit

Same prefix, `A` not alone — `P(k) = {A,D}`. `B`'s stale checkpoint-8
`STATE_HASH` mismatches; step 10b routes it to §6.3 step 1 without step 12a
because the record does not say solitary; the engine receives it as
`Event::StateHash` with `hand_id = k`; `named(k, 8)` resolves to
`checkpoints.boundary`.

**T50's guard is now computable on that record**: `e.state_hash != c.own`, where
`c.own` was written at T46 and never rewritten. T50 fires, `Diverged`,
`Fault{StateDivergence}`, `c.dissent := Some(e.state_hash)`, and `|c.required| == 2`
so the latch is not set and the table plays on after one hand. **`P1`'s third
face is closed and `Q4` no longer blocks it.**

---

# Part 4 — The healthy-table check at every seat count

Twenty hands, `n` seats, nothing goes wrong, every peer answers in one round
trip. Round trips per hand are `6n + 20`, verified against §8.2's own
decomposition (`4n − 3` betting actions + `2n + 23` crypto-timeout stages;
`(6n + 20) − (4n − 3) = 2n + 23` checks). 50 ms RTT; 42 ms of Bayer–Groth proving
per shuffler [`MENTAL_POKER.md` §5.1], sequential.

| Seats | Round trips | Protocol | Proving | **Per hand** | **20 hands** | `FLOOR(n)` | Advertised (code) | Any deadline fires? |
|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 2 | 32 | 1.600 s | 0.084 s | **1.684 s** | **33.7 s** | 1 017 000 ms | 1 200 000 | **no** |
| 4 | 44 | 2.200 s | 0.168 s | **2.368 s** | **47.4 s** | 1 337 000 ms | 2 700 000 | **no** |
| 6 | 56 | 2.800 s | 0.252 s | **3.052 s** | **61.0 s** | 1 657 000 ms | 2 700 000 | **no** |
| 8 | 68 | 3.400 s | 0.336 s | **3.736 s** | **74.7 s** | 1 977 000 ms | 2 700 000 | **no** |
| 10 | 80 | 4.000 s | 0.420 s | **4.420 s** | **88.4 s** | 2 297 000 ms | 2 700 000 | **no** |

`FLOOR(n)` recomputed from the preset's own fields at each count:
`7 000 + (2n + 23)·30 000 + 4n·25 000`. All five agree with §8.2's table and with
`tournament.rs`'s `the_deadline_floor_matches_the_published_derivation` — which
tests 2, 4, 6 and 10 and **omits 8**, the same omission §8.2's table has.

**No deadline fires at any seat count.** Margins per deadline kind:

| Deadline | Value | Worst healthy consumption | Margin |
|---|---:|---:|---:|
| `crypto_step_timeout_ms` (per stage) | 30 000 ms | ≈ 92 ms (42 ms proof + 50 ms RTT) | **326×** |
| boundary (`hand_delay_ms + crypto_step_timeout_ms`) | 37 000 ms | ≈ 150 ms | **246×** |
| `action_timeout_ms + action_grace_ms` | 25 000 ms | human | n/a |
| `hand_deadline_ms` (whole hand, ten seats) | 2 700 000 ms | 4.42 s protocol + human | **611×** on protocol |
| `join_deadline_ms` | 120 000 ms | — | T4 still fires long first |

**Does every hand place a checkpoint?** Yes, at all five sizes and all five hand
shapes. §6.2 row 8 is unconditional, I32(a) asserts coverage on both terminal
paths, I32(d) asserts the three lifetimes, and **`own` is now written at every
opening**, so the record a later hand compares against exists and is readable.

**What the widening surfaces this time, and it is not a timing finding.**

1. **`REOPENINGS` differs between the corpus and the code, at every seat count.**

   | Seats | at §13's 3 300 000 | at the code's 2 700 000 |
   |---:|---:|---:|
   | 2 | 91 | 67 |
   | 4 | 26 | 18 |
   | 6 | 13 | 8 |
   | 8 | 7 | 4 |
   | 10 | **4** | **1** |

   §8.2 states *"At the §13 preset's new value this is four at ten seats"*. The
   shipped preset affords **one**. That is `G6-R1`, and the code's own test
   asserts only `reopenings() >= 1`, so it passes at exactly the value that makes
   the document's sentence false.
2. **The deck segment is unchanged** — `2n + 2` of `6n + 20` round trips at
   `n = 10` is 22 of 80, proving is 0.42 s of the 4.42 s, still two orders of
   magnitude inside `crypto_step_timeout_ms` per stage, and still owed a Phase 4
   measurement alongside `verify_strict`.
3. **The heads-up MVP's stalled-hand wait is 20 minutes**, not the 17 the last
   gate computed from `FLOOR(2)`: the shipped `HEADS_UP_PLAY_MONEY_V1` advertises
   1 200 000 ms against a 1 017 000 ms floor. Correct, and worth knowing before it
   is reported as a regression.

---

# Part 5 — The load-bearing check, including this pass's fixes against each other

| # | Fix | Rule it newly makes load-bearing | Holds? |
|---|---|---|---|
| 1 | **§5.3 step 4 drops the union (`P2-e`)** | that **no** guard anywhere reads `readmit` | **YES.** T67 writes it, step 4 hands it off, step 8 clears it; I31(a) asserts one writer and one clearing site; the corpus-wide `admitted` grep returns only past-tense narration |
| 2 | **§5.3 step 8 reads `signed_this_hand` (`P2-e`)** | that the wire's regime test is on the **required** set, which since `P2` is `P(k−1)` exactly | **YES.** §4.9 and §4.4 l. 4244 both state it; §2.6 and I33(a) restate the inversion the old argument suffered |
| 3 | **T67 kept with no consumer** | that its `status ∉ {Removed, Empty}` filter is the only thing stopping `A` being a re-entry route for a removed seat | **YES**, and it is argued rather than assumed: *"the wire has no `status`"*, I34(a) |
| 4 | **`own: Hash` written at the opening (`Q4-e`)** | that every checkpoint record is opened at a moment this peer publishes its own body | **YES.** §6.2's points and §3.4 for 1–7, T45/T46 for 8, I32(a) asserts coverage on both terminal paths |
| 5 | **`dissent` as the whole of the divergence evidence** | that this peer's own value is always one of the stage's values | **YES.** I32(a); and the third-value case is disposed of by T50 having already fired and `Diverged` excluding the row |
| 6 | **T51's unanimity as `c.dissent.is_none()`** | that no `STATE_ACK` can complete a stage a differing `STATE_HASH` has entered | **YES.** T49/T50 are exhaustive and disjoint on `round == 0`, T50 moves to `Diverged`, and T51's scope excludes `Diverged` |
| 7 | **the three-slot `named(h, n)`** | that at most one slot matches | **YES**, proved from the lifetimes and re-checked: `agreed` never holds `number == 8`, `boundary` and `live` never hold two `number == 8` at once (I32(d)) |
| 8 | **`agreed_checkpoint(h)` scanning three slots** | that a boundary record whose `STATE_ACK` stage completed late is findable by T64 | **YES** — this is `N6`'s payoff and it is reachable for the first time |
| 9 | **P4's document-side deletion** | that `THREAT_MODEL.md` row 13 has a mechanism after losing its tier | **YES.** Re-classified to `none` with CP named as the answer, not merely downgraded |
| 10 | **`THREAT_MODEL.md` §5.1.1 (new)** | that every existing tier-1 member passes its own three questions | **YES.** All nine `SelfContained` variants are decidable from the offending event's bytes; I re-checked each against question 1 |
| 11 | **P5's *"at this layer"*** | that no D-014 removal ever needs a transport action | **YES.** §11.5.1's new paragraph states it and `block_peer` keeps its single caller |
| 12 | **§3.3's *"§6.1's table is normative for the engine"*** | that §6.1's table is itself complete and unambiguous | **YES** for the engine; **`P7` stands** for `PROTOCOL.md`'s own two listings |
| 13 | **`G5-Q5`'s rename** | that every citation now reads under the convention | **NO, but filed** — `G5-Q5-x`, with the interim disambiguation rule stated |
| 14 | **`tournament.rs`'s `hand_deadline_sec: 2_700`** | that no document states a different value for this preset | **NO — `G6-R1`.** §13 states 3 300 000 and §9.4 rule 3 makes the name imply the values |
| 15 | **`tournament.rs`'s `reopenings() >= 1` test** | that one reopening is the requirement | **NO — `G6-R1`.** §8.2's own sentence claims four at ten seats; the test passes at one |

**Thirteen of fifteen hold, and the two that fail are not the composition shape.**
Rows 14 and 15 are one defect and it is **code-against-document**, not
document-against-document: the code moved last, in a file no gate in this series
has had reason to open, and the document that owns the number was edited thirteen
minutes later without reconciling it.

> **The rule this pass adds, and it is the third in the series.** The last two
> were *re-derive every reader of a quantity you narrow* and *re-list the
> dispositions after the additions land*. This one is: **when a fix lands in both
> a document and the code, the pass that lands the second half must re-read the
> first half's number, not its argument.** §13's *argument* — a derived floor, a
> preset above it, a cap above that — survived intact into `tournament.rs`. Only
> the digit changed, and no rule in the corpus compares digits across the
> document/code boundary. `P4` was the same boundary crossed in the safe
> direction and was caught; this is the same boundary crossed in the unsafe one.

**And the sharper instance is `G6-R2`, which is a failure of this gate rather
than of the corpus.** `STATE_MACHINE.md` §2.6 l. 704 ends its residual with *"It
is recorded on `DECISIONS.md`'s open list rather than fixed here."* No such row
exists — `grep` for the residual's own terms over `DECISIONS.md` returns nothing.
**The previous gate read that sentence and graded the residual *"Recorded rather
than fixed, which is the right call."*** A filing claim is exactly the class of
statement this method exists to distrust, and it was accepted because it appeared
inside the fix rather than inside a disposition column.

---

# Part 6 — The D-014 sweep

### 6.1 Coverage, `grep -o … | wc -l`, after this pass's edits

| Document | D-009 | D-010 | D-011 | D-012 | D-013 | D-014 | Δ D-014 |
|---|---:|---:|---:|---:|---:|---:|---:|
| `PROTOCOL.md` | 22 | 67 | 52 | 19 | 41 | **44** | +2 |
| `STATE_MACHINE.md` | 28 | 85 | 43 | 35 | 86 | **69** | +2 |
| `THREAT_MODEL.md` | 37 | 127 | 105 | 39 | 35 | **52** | +3 |
| `NETWORK_STACK.md` | 5 | 30 | 40 | 26 | 22 | **34** | **+34** |
| `CRYPTOGRAPHY.md` | 18 | 39 | 30 | 21 | 15 | **29** | +6 |
| `CONTRIBUTING.md` | 12 | 12 | 11 | 8 | 9 | 12 | 0 |
| `DEPENDENCIES.md` | 11 | 6 | 10 | 10 | 5 | 14 | 0 |
| (`DECISIONS.md`) | 7 | 23 | 20 | 18 | 28 | **59** | +19 |
| (`SPEC_CS.md`) | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

**Every specification document now carries D-014, and the corpus has no coverage
defect for the first time in five passes.** `SPEC_CS.md`'s row is correct and is
not a gap: it is the source the decisions answer to. `CONTRIBUTING.md` and
`DEPENDENCIES.md` are unchanged at 12 and 14, which was already adequate and is
what the two-pass staleness prediction should now be pointed at instead.

### 6.2 The tier boundary, checked at every site that states it

Held, at nine sites, and the lists are now identical member for member.

| Site | Tier-1 members | Agrees with `SelfContained`? |
|---|---:|---|
| `DECISIONS.md` D-014's list, l. ~1320–1331 | 9 | **yes** |
| `PROTOCOL.md` §4.0's box, l. 1987–2020 | 9 | **yes** |
| `PROTOCOL.md` §4.10's `cause = 6` row, l. 3797 | 9, and *"the list is closed"* | **yes** |
| `THREAT_MODEL.md` §5.1's D&A cell, l. 1005 | 9 | **yes** |
| `THREAT_MODEL.md` §5.1.1 (new) | the **test**, not a list | n/a — and it is the better artifact |
| `NETWORK_STACK.md` §0.1, §1.2 p. 7, §11.5.1 | names one trigger by example, scoped to the table | **yes** |
| `src/security/validation.rs` | 9, asserted by the tripwire test | — |

Tier 2's precondition holds at every site: `PROTOCOL.md` §4.0's box (*"removes
nobody until this receiver holds the completed `STATE_ACK` stage"*), §4.9's
`kind = 3` box (exactly one `SignedEvent`, because *"a finding that needs a second
event to be decidable is not self-authenticating"*), §4.10's `cause = 6` rows,
`STATE_MACHINE.md` T64 — **and T64's guard is dischargeable, for the second pass
running and now with the record that can compute it.**
`src/security/validation.rs` enforces it in the type system: `Tier2Finding` has no
constructor that does not take an `AgreedCheckpoint`, `covering` is the only
constructor, and `Finding::Tier2Unconfirmed` maps to `Outcome::VoidHandOnly` with
`None` for the removal order. Equivocation, timeouts, `attributed`, votes and
quorums are excluded by name at five sites. `SeatStatus::Removed` is in §5.3
step 6's ante list, so the dead seat is blinded off and I1 holds with no term
moving at the removal.

### 6.3 Contradictions with `src/security/validation.rs`

**One, down from three, and it has changed direction again.**

1. **`ParentUnknown` — closed.** All five document sites deleted or
   re-classified; `THREAT_MODEL.md` row 13 given a mechanism rather than a tier;
   §5.1.1 written so the class cannot reopen. The code and the documents agree at
   nine variants.
2. **`AgreedCheckpoint` still cannot witness the position half — `Q2`,
   unchanged.** `covering` checks `hand_id` and both emitters and never reads
   `sequence`; T64's guard is on `c.number >= n` and the struct has no `number`.
3. **And the signpost pointing at it now points the wrong way — `G6-R3`.**
   `STATE_MACHINE.md` T64, l. 2444, still reads: *"`c.required` is what carries
   §4.9's 'the emitter set contained both the accused and this receiver', **which
   the type in `src/security/validation.rs` cannot witness today** (`P6`, filed
   against that file)"*. **`P6` landed two passes ago**: `AgreedCheckpoint` carries
   `emitters` and `covering` refuses on `AccusedNotAnEmitter` and
   `ReceiverNotAnEmitter`. The clause the type genuinely cannot witness is the
   **position**, which this sentence does not mention. An implementer reading T64
   for what the type owes reads the wrong half. Second consecutive pass; the last
   gate named it in §6.3 and gave it no number, which is why it survived.

### 6.4 Places a receiver rule says a violation has no consequence

Nineteen sites swept for *"removes nobody"*, *"remove nobody"*, *"no removal"*,
*"nothing further follows"*. **Eighteen correct, one unfiled narrowing.**

* `PROTOCOL.md` l. 1995–1999, 3598, 3629, 4747, 6098, 6154, 7492, 7499 — all
  carry the exception or concern a class D-014 does not touch.
* `THREAT_MODEL.md` l. 1437, 1467 — tier 2's precondition, correct.
* `NETWORK_STACK.md` l. 557, 570–575, 1615, 2686 — **all now scoped, `P5`
  closed.** The five-row table at l. 570 is the clearest statement in the corpus
  of what D-014 does *not* reach: disconnection, equivocation, `attributed`, a
  timeout certificate, and a count of any of them.
* `CRYPTOGRAPHY.md` l. 1681 — the setup beacon's non-revealer. Correct: failing
  to reveal is a **liveness** judgement, which D-014 excludes by name.
* **`STATE_MACHINE.md` T66, `hand_id == 0` — a narrowing made by an owner rather
  than by the decision, and D-014's own text still contains no setup-chain
  exception.** The reasoning is right — the roster's seat vector is frozen at
  `TABLE_READY` (`PROTOCOL.md` §3.1), so removing a seat there forks the genesis —
  and it belongs in D-014 as one line. **Carried a fourth pass. `G6-R4`, and it is
  numbered this time.**

### 6.5 `NETWORK_STACK.md` §1.2 prohibition 7, as the commission asks

> *"7. **remove, block, unseat, refuse or penalise a peer *at this layer* on the
> strength of a protocol proof.** … This prohibition is about the *reason*, not
> the action: the same disconnect is permitted when its reason is a resource,
> rate, size or admission fact (§0.2), and forbidden when its reason is a verdict
> about whether somebody cheated. **D-014 does not amend this prohibition and
> cannot reach it**, and the words *at this layer* are what this pass added: a
> tier-1 self-authenticating finding — an invalid application signature among its
> triggers by name — removes its sender from the **table** … and never from a
> socket, a dial queue, a relay reservation or a list of names here."*

**Closed, and closed better than the specification asked for.** `DECISIONS.md`
l. 1473 asked for two scope words; the file added the scope words **and** the
sentence that makes them operative — *the prohibition is about the reason, not
the action* — which is the distinction an implementer arriving from T64 actually
needs and which no other site states. §11.5.1 correctly took the **opposite**
edit, keeping its box verbatim and adding the paragraph that refuses the
inference. Four passes' delay, and the row that specified the fix was accurate
enough that the fix landed exactly as written.

---

# Part 7 — New defects

Prefixed `G6-` under `G5-Q5`'s convention, from the start.

| # | Severity | Defect |
|---|---|---|
| **`G6-R1`** | **BLOCKER for the constants module and for `table_params_hash`** | **The shipped preset and the normative preset are different tables.** `src/poker/tournament.rs` l. 83 ships `RATED_SNG_POKERTH_V1` with `hand_deadline_sec: 2_700` — **2 700 000 ms**. `PROTOCOL.md` §13 l. 6936 is normative: `hand_deadline_ms = 3 300 000`, with the derivation spelled out and the claim *"which buys REOPENINGS = 1 003 000 / (9 * 25 000) = 4 reopening raises per hand"*. §8.2 l. 5921 repeats it: *"At the §13 preset's new value this is four at ten seats."* At 2 700 000 the value is **one**. Both clear `FLOOR(10) = 2 297 000` and both are under `n(17)`'s 3 600 000 cap, so **this is not a timing failure — it is an identity failure.** §9.4 rule 3 is normative that *"`preset_id == \"RATED_SNG_POKERTH_V1\"` implies the preset's exact values, or the advert is rejected — a preset name that does not carry the preset's values is a lie about what game is being offered"*, and `hand_deadline_ms` is signed into `table_params_hash` (§3.1). So two conforming clients, one built from §13 and one from `tournament.rs`, **cannot join the same table**: rule 3 rejects the other's advert, and rule 7 marks the table unjoinable if a second advert arrives with a different `table_params_hash`. The code's own guard does not catch it — `the_shipped_presets_can_afford_at_least_one_reopening_raise` asserts `reopenings() >= 1` and passes at exactly the value that falsifies §8.2's sentence, and `the_deadline_floor_matches_the_published_derivation` tests the **floor**, which both values clear. **Fix:** one of the two moves, and §13 is the owner, so the code moves unless a reason is recorded for lowering the preset — in which case §13 and §8.2's sentence move together. Add a test asserting `hand_deadline_sec * 1_000` equals §13's figure literally, since the existing tests are all inequalities and an inequality cannot pin a constant. Owner: `src/poker/tournament.rs`, or `PROTOCOL.md` §13 and §8.2. |
| **`G6-R2`** | medium, **and it is a finding against this gate's method** | **`STATE_MACHINE.md` §2.6's residual claims to be filed and is not.** The `N-1e` boundary-window residual — a seat that signs only in the boundary window makes `signed_this_hand` two-membered, so step 8 writes nothing and the floor can be `None` for a hand whose retained record says solitary by the second disjunct — ends at l. 704: *"It is recorded on `DECISIONS.md`'s open list rather than fixed here."* **There is no such row.** `DECISIONS.md` has no entry for it under any of the terms the residual uses. The residual itself is correctly reasoned and correctly *not* fixed — widening step 8 to read the boundary checkpoint's `required` snapshot is a second read of a second quantity for one question, which is the defect `K-7` and `L7` each cost a pass — so the disposition is right and only the filing is missing, which is precisely the case D-013's process rule exists for. **The second half of the finding is this gate's.** `PHASE3_GATE5.md` read that sentence and graded the residual *"Recorded rather than fixed, which is the right call"*, without opening the file it points at. A filing claim inside a fix is exactly the class of statement the method distrusts inside a disposition column, and it was accepted because of where it appeared. **Fix:** one open-list row, and one line in this series' method — *a claim that something is filed is checked against the file, wherever the claim appears*. Owner: `DECISIONS.md`; and `research/` convention. |
| **`G6-R3`** | medium | **T64's signpost to `src/security/validation.rs` points at the half that landed and not at the half that did not.** `STATE_MACHINE.md` T64 l. 2444 still reads *"`c.required` is what carries §4.9's 'the emitter set contained both the accused and this receiver', **which the type in `src/security/validation.rs` cannot witness today** (`P6`, filed against that file)"*. `P6` landed two passes ago: `AgreedCheckpoint` carries `emitters`, `covering` is the only constructor, and it refuses with `AccusedNotAnEmitter` and `ReceiverNotAnEmitter`. **The clause the type genuinely cannot witness is T64's own `c.number >= n`** — the struct has no `number` and `covering` never reads `sequence` — which is `Q2` and which this sentence does not mention. An implementer reading T64 to find out what the module owes reads that the emitter set is the gap, re-implements something that exists, and leaves the position check unwritten. **Fix:** replace the clause with the position one and cite `Q2`. Owner: `STATE_MACHINE.md` T64. |
| **`G6-R4`** | low–medium | **D-014 still has no setup-chain exception, and T66 is a narrowing of a binding decision made by an owner.** T66 fires on `CheatProven` with `hand_id == 0` and removes **nobody** — *"`Fault{ProvenCheat}` and nothing else"* — against D-014's unqualified *a tier-1 finding removes its sender from the table*. The reasoning is correct and is stated at l. 2629 and l. 5016: the roster's seat vector is frozen at `TABLE_READY` (`PROTOCOL.md` §3.1), so a removal in the setup chain forks the genesis, and the beacon stalls to T4 exactly as before. **But D-014's own text contains no such exception** — a grep of D-014's whole body for *setup chain*, *`hand_id == 0`*, *genesis* and *`TABLE_READY`* returns nothing — so a reader following the authority order finds a decision that says *removes its sender* and a transition that does not, with no recorded reconciliation. **Carried a fourth consecutive pass, named in three gate reports and never numbered.** **Fix:** one line in D-014. Owner: `DECISIONS.md` D-014. |
| **`G6-R5`** | low | **`PROTOCOL.md` §4.9's restatement of step 10b claims exhaustiveness and enumerates one disposition fewer than its owner.** §4.9 l. 3340: *"§4.0 step 10b disposes of it in three ways **and no others**"*, then gives the mismatch route, the mismatch-plus-solitary route and the agreeing-out-of-set route. §4.0 row 10b — the normative owner — has **four**, and its outcome column says so: *"freeze, compare, readmit, or drop"*, with *"otherwise it is dropped as out of stage, exactly as before"*. The missing case is an **agreeing** checkpoint-8 `STATE_HASH` from a seat **inside** the recorded set, arriving after the window closed — which is not adversarial at all: it is the forwarding reorder of §1.5 that §4.9's own `N6` argument is built on. Under the owner it is dropped; under §4.9's box it has no disposition and the box says there are no others. Costs nothing reachable — the copy's only consumer, T47's gate, runs before hand init on the settled path and is absent on the abort path — but it is a restatement that contradicts its owner while claiming to be complete, which is the D-011 rule 1 failure mode in its cheapest form. **Fix:** add *"and anything else is dropped as out of stage"* to §4.9's box, or delete the words *and no others*. Owner: `PROTOCOL.md` §4.9. |
| **`G6-R6`** | low | **`HEADS_UP_PLAY_MONEY_V1` is a named preset that no document defines.** `src/poker/tournament.rs` l. 96 ships it *"named so that two clients can agree on it without negotiating"*, and no document in the corpus names it: `PROTOCOL.md` §13 defines `RATED_SNG_POKERTH_V1` only, and §9.4 rule 3's *a preset name that does not carry the preset's values is a lie* pins **only** that one name. So the mechanism the code's own comment relies on does not exist for this preset: a joiner receiving an advert with `preset_id = "HEADS_UP_PLAY_MONEY_V1"` has nothing to check it against and falls back to the ordinary custom-table range checks. That is not unsafe — rule 2a's floor still applies and 1 200 000 clears `FLOOR(2) = 1 017 000` — but it means the **first supported mode** (`SPEC_CS.md` §32, heads-up first) ships a preset name with no normative content, which is the same class of gap `G6-R1` is the acute form of. **Fix:** either add the preset to §13 with its values, or delete the `preset_id` claim and let it advertise as a custom table. Owner: `PROTOCOL.md` §13, or `src/poker/tournament.rs`. |

**The pattern check, run on this pass's own findings.** **`G6-R1` and `G6-R6` are
one shape and it is new to the series: a value or a name that lives in the code
and in a document, where no rule compares the two.** Every previous pass's
findings were document-against-document or code-against-document *within one
concept the documents own*; these are the reverse — the concept is the
**configuration**, the document owns it, and the code shipped a different
instance of it. `G6-R3` and `G6-R4` are staleness: a claim that outlived its
subject, and a narrowing that outlived four chances to be filed. `G6-R5` is a
restatement that drifted from its owner. **`G6-R2` is the only one that is a
failure of this method rather than of the corpus, and it is the one worth
carrying forward as a rule.**

---

# Part 8 — The build list, item by item

## `PublicTableState` — **BUILD**

Transcribe from **`PROTOCOL.md` §6.1's table only**, top to bottom, per
`STATE_MACHINE.md` §3.3's normative box. Never from §2.9's enumeration — the two
orders differ, the struct is `#[cbor(array)]`, and the order is the encoding
(**`P7`**, filed as `P7-w`). `signed_this_hand` is in it and is **appended last**
(§2.2 rule 5). `absent` is deleted and may not be re-added. Expect one
append-only field if `D-014-3` lands a `removed` vector. Gap: `P7` in
`PROTOCOL.md`, closed for the implementer by §3.3.

## `state_hash` — **BUILD**

`h("p2p-poker v1 state", [canonical_cbor(PublicTableState)])`, one part, domain
string in §2.8's closed register. `h` and the register are written and green
(`src/protocol/serialization.rs`, `src/protocol/signatures.rs`). No gap.

## the `checkpoint` field's range — **BUILD**

Range-check to `1..=8`. §6.2's eight rows are *"these points and no others"*, and
the reconciliation stage carries the disputed checkpoint's own number rather than
a ninth value. No gap.

## the three anti-replay stores — **BUILD, all three, with the lifetimes**

`event_class == 0` keyed by `slot(E)`, dropped at hand end, **no store created
for a finished hand** — that last clause is `N2` and is what makes §4.0 row 10a's
skip implementable rather than a coin flip. Classes 1 and 2 sparse, per stage,
keyed `(seat, subject_seat)` and `(seat, subject_digest)`. The `DISPUTE` counter
at 8 per sender per hand. No gap.

## the checkpoint-8 `STATE_ACK` slice — **BUILD**

`8 × MAX_SEATS = 80` entries, one hand at a time, at
`BOUNDARY_CHECKPOINT_BASE + 2r + 1`, `0 <= r <= 7`, retained to `TERMINAL(k+1)`.
The one structure that outlives its hand and the one place step 10a is **not**
skipped. No gap.

## `RetainedHand` — **BUILD**

Four fields; `p = P(hand_id − 1)`; **`was_solitary` is the disjunction and an
independent bool, never `p == {self}` and never `|p| == 1`** (`N4-a`, closed in
§5.3's struct comment); LRU at `MAX_RETAINED_HAND_RECORDS = 4 096`, ≈ 56 B each,
under 256 KB. No gap.

## the readmission set — **BUILD, both halves, and this is the change**

**Wire half:** one `SeatSet`, `|A| <= MAX_SEATS`, written by §4.0 step 10b, read
once at the next hand init, cleared there, **widening the *accepted* emitter set
only**. Accept an out-of-set `HAND_INIT(m+1)` copy from a seat in `A` and count
it into `P(m+1)`; completion stays `heard ⊇ P(m)`.

**Engine half — `Readmitted` may now be wired**, which every previous gate
forbade. §2.7's deferral is lifted in terms, and the condition it was written
against is met in both documents. Wire T67 with its full guard —
`hand_id > 0 ∧ seat occupied ∧ status ∉ {Removed, Empty}` — because that filter
is the whole of D-014's one-way exit on this route (I34(a)) and the wire cannot
supply it. **`readmit` must reach no guard**: build it as a field read once at
§5.3 step 4, handed to the protocol layer, and cleared at step 8, and add the
I31(a) assertion that it has one writer and one clearing site. The directed test
is I33(a)'s: deliver a `Readmitted` into a peer whose `P` has narrowed to itself
and require `solitary_since` to be written anyway. No gap.

## `CheckpointStore` — **BUILD, shape and record both**

Three named `Option` slots — `live`, `agreed`, `boundary` — no `Vec`, no map,
nothing keyed on a sender-chosen quantity. `named(h, n)` and
`agreed_checkpoint(h)` as the **only** routes into the store. Lifetimes at
T45/T46 (`boundary := None`, then `live := Some(new)` with `own` written),
T47/T61 (`boundary := live.take()`, `agreed := None`), and the T10 special case.
`Event::StateHash` and `Event::StateAck` carry `hand_id`, read off an envelope
already validated — not a wire change.

**`CheckpointState` — now buildable, which it was not last pass.** Fields:
`hand_id`, `number`, `sequence`, `required`, `heard`, **`own: Hash`**,
**`dissent: Option<Hash>`**, `acked`, `agreed: Option<Hash>`. `values: u8` is
deleted and must not be re-added. Write `own` **once**, at the opening, and never
re-derive it from current state — I32(d) names that as the implementation that
passes every single-hand trace and fails every boundary one. No gap.

## `PrefixOutcome` / `SolitaryDivergence` with T62 and T63 — **BUILD**

`SolitaryDivergence { hand_id, seat, event_hash }`. T62's guard is
`solitary_at(e.hand_id)` with **`j <= k + 1`**, in the past tense, on the event's
`hand_id` and never the receiver's current phase. T63 is the same guard inside
`TableClosed`, with `settlement.tournament_winner := None` — the one event class
for which that phase is not absorbing. Both interleaving variants in Part 3 fire
on the first, best-evidenced event and hold, **including under the readmission
feed that reversed them last pass**. Freeze semantics from I33(b): the only
permitted publications while frozen are the `DISPUTE`, the reconciliation
`STATE_HASH` and the matching `STATE_ACK`. No gap.

**T49 / T50 / T51 — also BUILD**, which they were not last pass. All three guards
are computable on all three slots. T50 is the non-solitary stale-mismatch carrier
and T51 is D-014 tier 2's precondition, so this unblocks both. No gap.

## §4.0 steps 12 and 12a — **BUILD**

Step 12's solitary-stage conjunct, step 12a's two arrival exemptions
(`PLAYER_SIT_IN`, and the checkpoint-8 `STATE_HASH` which is compared instead),
and the second exemption's *"from arrival and not from disagreement"* clause are
all pinned, and the engine guard that consumes them is correct. Step 12's
*"an out-of-set checkpoint-8 `STATE_HASH` is not an out-of-stage event"* clause
must be implemented or `P2`'s accepted-set widening is unreachable. No gap.

## the constants with `HAND_DEADLINE_FLOOR(n)` — **DO NOT BUILD YET**

**The formula is settled and the number is not.** Write
`HAND_DEADLINE_FLOOR(n) = hand_delay_ms + (2n + 23)·crypto_step_timeout_ms
+ 4n·(action_timeout_ms + action_grace_ms)` as a `const fn` over the five config
fields — `src/poker/tournament.rs` already has it and it is correct at all five
seat counts. **Do not write `hand_deadline_ms` as any literal until `G6-R1` is
settled**: `PROTOCOL.md` §13 says 3 300 000, `src/poker/tournament.rs` ships
2 700 000, and §9.4 rule 3 makes the two mutually unjoinable under one preset
name. Never write `600_000` under any circumstance.

**And the rest of the constants module is blocked on `G4-P3-e`, which is filed
and open.** `STATE_MACHINE.md` still carries eight `600 000` figures, l. 153
included — `pub hand_deadline_ms: u32, // 600_000`, which is the one an
implementer transcribing §2.3 into a constants module would take — plus §9.4's
validator line `hand_deadline_ms >= 10 * action_timeout_ms` (l. 4358), which at
the preset is 200 000 and **passes exactly the configurations `P3` shows are
unplayable**. Named section: `STATE_MACHINE.md` §2.3 l. 153 and §9.4 l. 4358.

## What is still deferred, and on what

| Item | Deferred on |
|---|---|
| §4.0 steps 13, 14, 15 | the engine — semantic re-validation and proof verification |
| `TIMEOUT_VOTE`, `TIMEOUT_CERT`, `EquivocationProof` behaviour | **`OQ-F`** — write the payload structs, wire no behaviour; `OQ-F` may delete them |
| `SHOWDOWN_MUCK` policy | **`Q-01`** |
| T64 / T65 producers | **`D-014-3`** — a removal is canonical per-seat state that no hashed vector carries, so it has no route to canonical state. The rows are specified and nothing fires them, which is the honest state |
| `src/security/validation.rs`'s position check | **`Q2`**, and its signpost is **`G6-R3`** |
| the reconciliation round's out-of-set admission | **`N1-a`**, one sentence, and it must be chosen **before** T53 is written: both answers are safe, the unreachable-exit state is not |
| `SeatStatus::Absent` and its readers | **`J-3`**, closed as *unreachable, deliberately* |
| the `n(17)` headroom clause | **`Q6`** |
| the `600 000` sweep and the §9.4 validator line | **`G4-P3-e`** |
| the reopening residual above the floor | **`G4-P3-r`** |
| the citation sweep for the renamed series | **`G5-Q5-x`** |
| `PROTOCOL.md` §2.9's second field listing | **`P7-w`** |
| the boundary-window solitary residual | **unfiled — `G6-R2`** |
| D-014's setup-chain exception | **unfiled — `G6-R4`** |

**Nothing on the freeze path is deferred any longer.** `SolitaryDivergence`, T62,
T63, §4.0 steps 12 and 12a, T49, T50, T51, `CheckpointStore`, `CheckpointState`
and the readmission set's engine producer can all be written today. The one item
that must not be written is the `hand_deadline_ms` literal, and it is blocked on
a two-digit disagreement between a document and a file that were edited thirteen
minutes apart.
