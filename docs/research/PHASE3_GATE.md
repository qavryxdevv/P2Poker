# PHASE3_GATE.md — the Phase 3 readiness gate on the wire specification

**Commission.** Eight passes have run. The seventh (`PHASE2_GATE3.md`) was the first
to close a prior pass completely — RESOLVED 10, PARTIAL 0, UNRESOLVED 0,
REGRESSED 0 — and declared `STATE_MACHINE.md` implementable without guessing while
blocking on two defects in `PROTOCOL.md`. **D-013** ruled on both. The engine is
written and under test. What is gated now is the wire, because `src/protocol/` is
written against `PROTOCOL.md` next.

**Method.** Every verdict is quoted from the document as it now stands. No
specification document was edited. As for three passes, the three sweeps were run
**against the constructions themselves** — every `h("…")` site enumerated first and
each input traced backwards to its source, every collective stage's `R` enumerated
from `§4.11`'s *Emitter* column and each `R` traced to what computes it, every
`PublicTableState` field traced — and the corpus's own sweep tables read afterwards
to compare rather than to source the verdict. That inversion produced this pass's
blockers again, and this time it produced something new: **the first blocker is not
a survivor of a rule, it is a defect inside the rule D-013 introduced**, and it is
visible only by asking what happens on the *second* iteration of a case the corpus
analyses once.

**Files as read, 2026-08-28.** `PROTOCOL.md` 17:14, `DECISIONS.md` 17:13,
`THREAT_MODEL.md` 17:10, `STATE_MACHINE.md` 17:09, `NETWORK_STACK.md` 16:37,
`CRYPTOGRAPHY.md` 16:10, `CONTRIBUTING.md` 16:03, `DEPENDENCIES.md` 16:03,
`SPEC_CS.md` 09:29. Source tree read at the same time.

> **The timestamp finding, fourth iteration, and it has inverted back.** Last pass
> the trailing file was `PROTOCOL.md` and both blockers were in it. This pass
> `PROTOCOL.md` is the *newest* file and carries **23 occurrences of `D-013`**
> against 49 in `STATE_MACHINE.md`, 1 in `THREAT_MODEL.md`, and **0 in
> `NETWORK_STACK.md`, `CRYPTOGRAPHY.md`, `CONTRIBUTING.md` and `DEPENDENCIES.md`**.
> D-012's own process rule — *"every decision sweep covers every document"* — has
> now been broken in the pass immediately after the pass that wrote it. Two of this
> pass's carried items and two of its new defects are in the four documents with a
> zero.

---

## Verdict counts

| Verdict | Count | Items |
|---|---:|---|
| **RESOLVED** | 8 | J1, J2, J3, J4, J5, J6, J7, J-3 |
| **PARTIAL** | 0 | — |
| **UNRESOLVED** | 2 | J-4, J-5 |
| **REGRESSED** | 0 | — |

**Every one of J1–J7 is closed.** That is the second clean sweep of a prior pass's
seven findings in eight attempts, and it includes both blockers. The two
UNRESOLVED items are not J1–J7: they are the **cross-owner blockers D-013's own
process rule put in `DECISIONS.md`'s open list**, assigned to `NETWORK_STACK.md`
and `CRYPTOGRAPHY.md`, and neither owner acted. D-013 created the open list
precisely so that a finding assigned across an owner boundary would not be
invisible; the list worked as a record and failed as a prompt.

**Seven new defects, K1–K7. Three block.** Plus three code/specification
divergences, C1–C3, which are not specification defects but are blocking for the
build order, because they are in the modules Part 9 was told already exist.

* **K1** — **`P(k)` is a per-receiver quantity on exactly the path where it does
  work, and its payoff is a silent permanent fork.** §3.2's justifying inference
  inverts the property P3 built. This is a D-012 violation inside the rule D-013
  introduced to fix a D-012 violation.
* **K2** — **`Q-09` is a wire question, not a placement question**, and it sits
  under the one message D-013 made the sole route back into `P`.
* **K3** — **the drain hand carries no checkpoint**, so §5.2.4 and §6.3 — the
  corpus's only cross-peer detection route — are inert in the regime D-013 made a
  steady state. This is what makes K1 silent rather than loud.

**Readiness verdict: NOT-READY.** Reasons in Part 9. The gap is one rule about how
`P(k)` is ratified, one paragraph placing three message types in the chain, and one
row added to `§6.2`. It is narrower than J1+J2 was, and unlike that gap none of it
needs a decision the owner has not already made in principle.

---

# Part 1 — Per-item verdicts

## J1 — `advert_hash` in `GENESIS(0)`, `session_id` and `ctx` — **RESOLVED**

Removed from all three, replaced by a normative construction, and — the part that
makes this a fix rather than a substitution — **checked at three sites where the old
value was carried and read by nothing.**

> `PROTOCOL.md` §3.1 (l. 866): "**`table_params_hash` — normative, and this is the
> disposition of J1 (D-013).** `advert_hash` stood in `GENESIS(0)`, in `session_id`,
> and transitively in `ctx`. It is **removed from all three** and this box replaces
> it."

The box is complete rather than gestural. **Twenty-five parts in a stated order**,
each mapped to its `LOBBY_TABLE_AD` field index, with `BlindSchedule` expanded in
place at its own field's position, `preset_id` and `deck_suite` as raw payload bytes
and everything else fixed-width big-endian. The arithmetic checks: `LOBBY_TABLE_AD`
has 29 fields (`n(0)`–`n(28)`), `BlindSchedule` expands one into four, so 32 parts
exist; 25 are included and **7 are excluded by name with a reason each** — `n(3)`
and `n(10)` display and advisory, `n(23)` an admission gate spent at
`JOIN_REQUEST`, `n(25)`/`n(26)` identity and routing whose privilege ends at
`TABLE_READY`, and `n(27)`/`n(28)` named as *"the two fields that made `advert_hash`
per-receiver in the first place"*. 25 + 7 = 32. Nothing is unaccounted for.

**J1(b) is closed at three checked places, not one**, which is the half the previous
pass could not have graded because the fix did not exist:

1. §7.2 receiver rule 7 — a re-broadcast whose `table_params_hash` differs from the
   held one is discarded **and the table is marked unjoinable**;
2. §4.3 `PLAYER_LIST` `n(1)` — was `advert_hash`, is now `table_params_hash`, and
   the receiver rule reads *"equals this client's own recomputation … a mismatch
   means the founder is forming a table under parameters other than the ones this
   client agreed to, and the client leaves rather than sitting down"*;
3. §4.3 `TABLE_READY` `n(2)` — same replacement, and the check is
   double-sided: equal to this receiver's own recomputation **and** equal to the
   `PLAYER_LIST` being ratified.

§4.3 states the point the gate needed stated: *"the field was carried into a signed
body and read by nothing … The replacement is a value every honest joiner computes
identically, and it is now **checked**."*

`ctx` needed no edit and §4.5 says why in the right terms — it never named
`advert_hash`, it names `session_id` — and then records what the fix bought:
*"every one of `ctx`'s seven parts is a protocol constant or a value fixed by
accepted chained content."* That is the condition `CRYPTOGRAPHY.md`'s discipline
item 1 states, and §4.5 correctly notes it *"previously failed transitively without
either document knowing the path existed"*.

**Checked independently.** `grep` for `advert_hash` over all nine documents returns
hits in exactly two live roles: `JOIN_REQUEST` `n(0)` and `JOIN_ACCEPT`'s
`request_hash`/`advert_event` validation — both **unchained** RPC (`chain_scope = 0`,
`table_id = ZERO32`), so neither reaches a chained hash — and §2.8's register row,
which is now explicit: *"Since **D-013** it is a **lobby-layer pointer only**: it
names which advertisement a joiner is answering, and it enters no chained hash — not
`GENESIS(0)`, not `session_id`, not `ctx`, not `roster_hash`, not `state_hash`."*
The remaining hits are in `NETWORK_STACK.md`, and they are J-4.

## J2 — `HAND_INIT`'s required emitter set was the liveness gate — **RESOLVED**

D-013's rule is instantiated where it belongs, generalised where it needed to be,
and — the part that decides the verdict — **the class was enumerated rather than the
one instance patched.**

> `PROTOCOL.md` §3.2: "**`R` is derived from demonstrated participation in the agreed
> chain, never from a seat's status.** … **A seat is required to emit in hand `k+1`
> only if it signed at least one chained event during hand `k`.**"

Three properties make this a fix and not a substitution:

**(a) It is stated once, at the stage level, and every message type instances it.**
§3.2 carries the rule; §4.4 instantiates `R(HAND_INIT, k) = P(k-1)`; §4.9
instantiates `STATE_HASH`/`STATE_ACK`; §4.10 instantiates `HAND_COMPLETE`; §4.11's
*Emitter* column is the index. The alternative — each type carrying its own habit —
is what let four sets be status-defined for seven passes.

**(b) The containment that would have moved the stall is normative.** §4.4:
*"**`dealt_in` is a subset of `P(k-1)`, and that containment is normative.** Without
it the narrowing of `HAND_INIT`'s emitter set buys nothing … the table would stall
at `DECK_INIT` instead of at `HAND_INIT` with nothing else changed."* The engine
half states the mirror image at §5.3 step 4: *"Narrowing only the emitter set would
not have worked … Narrowing only `dealt_in` would not have worked either."* Both
halves reason about the other's failure mode, which is what D-011 rule 1 is supposed
to produce and rarely does.

**(c) The exclusion nobody would have thought of is stated and is not optional.**
§3.2: *"a terminal `HAND_ABORT` never counts as participation … two honest peers
routinely accept copies signed by **different seats** — so counting its signer would
derive `P(k)` from which copy this receiver happened to accept first."* `I31(c)`
asserts it from the engine side with a directed test: *"accept a terminal
`HAND_ABORT` and require `signed_this_hand` unchanged."*

**The sweep is in Part 2 and finds no survivor.** No `R` anywhere in the corpus is
defined on a seat's status. J2 as filed is closed.

**What is not closed is the rule that replaced it**, and that is K1, filed as a new
defect rather than as a J2 regression because it is a different fact: J2 was *"a set
defined on a status can never lose a member"*; K1 is *"the set that replaced it is
per-receiver on the one path where it loses a member"*. Grading K1 as a J2
regression would hide that the emitter-set class is now correctly enumerated and
that the residual is a ratification question, not a class question.

## J3 — `I30(c)` asserted an implication that does not hold — **RESOLVED**

Deleted and replaced with the claim that is actually establishable, and the
replacement carries a *checkable* test instruction rather than the unfalsifiable one
that hid J2.

> `STATE_MACHINE.md` I30(c): "**The clause that used to follow — 'hence a `HAND_INIT`
> collective stage that completes' — is deleted as false (J3).** Agreement on the
> body makes the stage *completable*; completing it additionally requires every
> member of the required emitter set to emit, which is a liveness property and is
> outside this invariant's scope."

The self-criticism is the valuable half and is worth keeping: *"it was the sentence
that hid J2 for two passes, and because it carried a test instruction, a harness
asserting it passed on every trace where the stage completed and was never run
against the trace where it did not. **An invariant that is unfalsifiable exactly
where the defect lives is worse than no invariant.**"* The replacement instruction —
*"Assert (c) as byte-identity of the two peers' derived `HAND_INIT` bodies, not as
stage completion"* — is strictly stronger evidence and is generatable.

## J4 — the escape route Q7 and §12 named did not exist — **RESOLVED**

Dissolved rather than answered, which was the correct disposition, and the
dissolution is recorded as one.

> `STATE_MACHINE.md` Q7: "**Closed by D-013, and closed by dissolving it rather than
> answering it (J2).** Marking a seat `Absent` would never have helped: `PROTOCOL.md`
> §4.4's required emitter set names absent and sitting-out seats **in its inclusion
> clause**, so no status ever removed a seat from it."

A seat now needs no escape route: it is skipped by having stopped signing, with no
transition, no vote and no action by any other seat. §4.4 and §5.3 step 4 both say
so and neither restates the other.

The *return* route — `PLAYER_SIT_IN` with a widened legality condition — is named in
§4.10 and is the whole of re-entry. It is not implementable as written and it has no
phase in the engine that can accept it. Those are **K2** and **K3b**, filed as new
defects rather than as a J4 downgrade, because the finding J4 filed is genuinely
gone and the new one is about a mechanism that did not exist when J4 was written.

## J5 — `SeatStatus::Absent` unreachable while five live mechanisms read it — **RESOLVED**

Resolved on both sides of the owner boundary, and this is also the disposition of
**J-3** in `DECISIONS.md`'s open list.

**Wire side.** §6.1 deletes the `absent` vector from `PublicTableState` and
therefore from `state_hash`, and gives the right reason: *"A vector that is always
false forks nothing, so this is not a correctness fix — it is the removal of a
**trap** … An implementer who finds a status the engine never sets, sees it hashed
into every checkpoint of every hand, and wires it to the obvious local signal … has
forked the chain at every checkpoint, silently."* §4.4 deletes `bb_seat`'s
`non-absent` qualifier on the same ground: *"nothing sets that status, so the
qualifier constrained nothing."* The flag vector is four, not five, and §2.9's
enumeration 3 records the change.

**Engine side.** `STATE_MACHINE.md` §2.4 takes the harder of the two options the
open list offered — *"Retire the variant with its name recorded … or state precisely
what still sets it"* — and states the third: keep it, label it, and enumerate every
reader.

> §2.4: "**The variant stays, and it is a trap that must be labelled rather than
> tidied away (J5).** `SeatStatus::Absent` is **unreachable in this version,
> deliberately**, and five live mechanisms still read it: `PROTOCOL.md` §6.1's
> `absent` vector inside every `state_hash`, §4.4's `bb_seat` constraint, §5.3 step
> 4's `dealt_in` filter, §5.3 steps 6–7's ante and blind posting, and T59's
> sit-back-in guard. All five are correct with a permanently-false vector."

Two of those five have since been deleted by §6.1 and §4.4, so the list is now
stale by two entries in the safe direction — it over-warns. That is not worth a
finding; it is worth one editorial line at the next touch.

## J6 — §4.5 asserted a reproduction H7's fix deleted — **RESOLVED**

Corrected in the exact words the previous gate proposed, and the class is named
where D-011 does not name it.

> §4.5: "`CRYPTOGRAPHY.md` §6.4 gives the reasoning about **what** `ctx` must bind
> and points here for the construction; it reproduces no part of this block. … The
> class is worth naming because D-011 does not: **when a copy is deleted, the
> owner's sentence describing the copy is the second edit.**"

## J7 — §5.2 cited a key `STATE_MACHINE.md` no longer contains — **RESOLVED**

The ordering-buffer key is now stated as `PROTOCOL.md`'s own, with the exact
disambiguating line the other document asked for.

> §5.2: "keyed on `(table_id, hand_id, sequence, previous_event_hash)` — four
> envelope fields §2.3 defines, so **this document owns that key and states it here**
> (D-011 rule 1). … **This is not §5.2.1's slot key**, and the two must never be
> conflated: the slot key … deliberately **excludes** `previous_event_hash`, while
> this key deliberately **includes** it, because the buffer's whole job is to place
> an event relative to its parent."

## J-4 — `NETWORK_STACK.md` §0.5.6 still reports J1 as an open survivor — **UNRESOLVED**

`DECISIONS.md`'s open list filed this on 2026-08-28 in the same pass that adopted
D-013, under the process rule D-013 created. The owner did not act. The file's
mtime is 16:37, before every other edited document.

> `NETWORK_STACK.md` §0.5.6 (l. 300): "**The one site where a local view still
> reaches canonical state.** Reported rather than fixed, because the fix is not this
> document's to make."
>
> (l. 336): "**The other half is `PROTOCOL.md`'s and is a blocker.**"

It is not stale by disposition only, it is **stale by content**: §0.5.6 offers two
candidate fixes — *"the founder's `PLAYER_LIST` value made normative and
`TABLE_READY` required to equal it, or `advert_hash` dropped from `GENESIS(0)` in
favour of `table_public_key`"* — and **neither is the fix that was adopted.** A
reader sent there for the state of J1 gets a live blocker notice, a wrong problem
statement, and two wrong remedies.

Five further sites in the same document carry the same claim and must move with it:
§0.5.2's merged-lobby-view row (*"One consequence of this row is not yet safe:
§0.5.6"*), l. 1496, l. 1529, l. 2220, and the **D-012 coverage row at l. 2668**,
which is the document's own compliance record and currently reads *"One site is
reported and not fixed"*.

**Severity: medium, and it is process rather than wire.** Nothing in
`NETWORK_STACK.md` is read by an implementer building `src/protocol/`. What it costs
is the corpus's ability to tell a closed blocker from an open one, which is the
exact failure that made J1 survive seven passes.

## J-5 — `CRYPTOGRAPHY.md` should record that `ctx` is now clean — **UNRESOLVED**

Filed as low in the open list, and it is low. Discipline item 1 reads:

> `CRYPTOGRAPHY.md` (l. 1046): "**This is also the D-012 obligation on `ctx`:** every
> one of `PROTOCOL.md` §4.5's fields is either a protocol constant or a value fixed
> by a chained event every participant accepted, and none of them is a local view."

The sentence is **now true**. It was false when written, and false transitively
through `session_id` in a way neither document could see. Nothing is owed on the
wire; what is owed is the record, so that a future editor does not read a claim that
was once wrong as evidence that the check was once run.

`CRYPTOGRAPHY.md` has **zero occurrences of `D-013`**, and it was not opened in this
pass. That is the same omission that produced the D-012 process rule, in the same
document, one decision later.

---

# Part 2 — The emitter-set sweep

**Method.** Enumerated from `§4.11`'s summary table, which lists all 39 message
types with a *Stage kind* and an *Emitter* cell, and cross-checked against §3.2's
three shapes and against `STATE_MACHINE.md`'s guards. Every set was traced to what
computes it and classified: **participation-derived** (a function of the accepted
chain under D-013), **content-derived** (a function of chained content that is not
participation), or **status-derived** (a survivor, which reopens J2).

Nine collective stages exist. `HAND_ABORT` is the one witness-independent terminal
and has no set by construction. `DISPUTE` is `chain_scope = 0` and occupies no slot.
Everything else is single-writer and has a writer rather than a set.

| # | Stage | `R` as it now reads | Traced to | Verdict |
|---:|---|---|---|---|
| 1 | `TABLE_READY` §4.3 | every seat of the `PLAYER_LIST` roster; **its signers *are* `P(0)`** | the roster the founder proposed, ratified unanimously; the base case | **content-derived, and it defines the base case** |
| 2 | `RNG_COMMIT` §4.4 | `P(0)` | §3.2 | **participation** — was *"every seated participant"* |
| 3 | `RNG_REVEAL` §4.4 | `P(0)` | §3.2 | **participation** — same phrase deleted |
| 4 | `HAND_INIT` §4.4 | `P(k-1)` | §3.2 | **participation** — this was J2 |
| 5 | `DECK_INIT`, `DECK_COMMIT` §4.4 | `dealt_in` | `dealt_in ⊆ P(k-1)`, normative in §4.4; full derivation `STATE_MACHINE.md` §5.3 step 4 | **participation, via the containment** |
| 6 | `DEAL_PRIVATE`, `BOARD_REVEAL` §4.6 | `dealt_in` | same | **participation, via the containment** |
| 7 | `SHOWDOWN_REVEAL` / `SHOWDOWN_MUCK` §4.6 | the required-to-show set | a function of the betting sequence and `showdown_policy`; a subset of `dealt_in` | **content-derived** |
| 8 | `TIMEOUT_CERT` §4.8 | `V(subject) = deck.participants \ ({subject} ∪ certified_subjects)` | `deck.participants` = `dealt_in`; `certified_subjects` is canonical state emptied at hand init | **content-derived, and inert under D-010** |
| 9 | `STATE_HASH` / `STATE_ACK` §4.9 | `P(k-1)`; `P(0)` at checkpoint 1; a reconciliation round takes the set of the checkpoint it re-derives | §3.2, §4.9 | **participation** — was *"all present seats"* |
| — | `HAND_COMPLETE` §4.10 | the set that emitted `HAND_INIT`, `P(k-1)` | §3.2 | **participation** |
| — | `HAND_ABORT` §4.10 | **none** | witness-independent terminal | **no set by construction** |

**Result: no survivor. Zero required emitter sets in the corpus are defined on a
seat's status.** Four sets changed in the previous pass and the words they carried
are named in §2.9 — *absent*, *sitting out*, *seated*, *present* — and §4.11 now
states the rule as a rule: *"**A new row whose emitter cell names a status is a
defect** (J2, D-013, §2.9's enumeration 2)."* J2 does not reopen.

**Checked outside the two owners.** `grep` for `emitter set` / `required emitter`
across the corpus: `PROTOCOL.md` 33, `STATE_MACHINE.md` 34, `DECISIONS.md` 2,
`THREAT_MODEL.md` 5, and **zero elsewhere**. All five `THREAT_MODEL.md` hits are
references, and one of them says explicitly *"a threat model that proposed an
emitter set would be writing"* someone else's document. No third document defines a
set. `CONTRIBUTING.md` and `DEPENDENCIES.md`: checked, nothing owed.

**The one status word left anywhere on a progression path, and it is not an `R`.**
`STATE_MACHINE.md` T59's exit from `Paused` is guarded on *"at least two seats are
again **willing** and have chips"*, where *willing* is `status == Active ∧
stack > 0`. It is not an emitter set, it is deterministic, and it is not
per-receiver, so it does not reopen J2. But it is a **status-defined gate on whether
the table restarts**, and it disagrees in kind with §5.3 step 4 one screen below it,
which decides the same question on participation. Filed as **K7**, low.

---

# Part 3 — The genesis sweep

**Method.** Every input to `GENESIS(0)`, `GENESIS(k)`, `session_id` and `ctx`
enumerated from §3.1, §4.3 and §4.5, then traced backwards until it reaches one of:
a protocol constant, a value fixed by chained content every participant accepted, or
a local view. The third is a survivor and reopens J1.

## `GENESIS(0)`

```
GENESIS(0) = h("p2p-poker v1 genesis",
               [ u16_be(protocol_version), table_id, u64_be(0),
                 table_public_key, table_params_hash, ZERO32 ])
```

| Part | Traced to | Verdict |
|---|---|---|
| `protocol_version` | pinned per session at §1.2, checked at §4.0 step 5 | **constant** |
| `table_id` | §4.1: *"`table_id` is 32 bytes and **is** the table's Ed25519 public key"*; identical in every re-broadcast because every re-broadcast is signed by that key | **constant per table** |
| `u64_be(0)` | literal | **constant** |
| `table_public_key` | §4.1 makes this the **same 32 bytes** as `table_id` | **constant per table — and duplicated; K5** |
| `table_params_hash` | §3.1's box, 25 parts, every one a field of one signed `LOBBY_TABLE_AD` with the two varying fields excluded by name; equality enforced at §7.2 rule 7, `PLAYER_LIST` `n(1)`, `TABLE_READY` `n(2)` | **agreed content** |
| `ZERO32` | literal | **constant** |

## `session_id`

```
session_id = h("p2p-poker v1 session",
               [ table_id, table_params_hash, roster_hash(0),
                 for each seat s ascending: event_hash(TABLE_READY from s) ])
```

| Part | Traced to | Verdict |
|---|---|---|
| `table_id` | as above | **constant** |
| `table_params_hash` | as above | **agreed content** |
| `roster_hash(0)` | the `PLAYER_LIST` roster, ratified by `TABLE_READY` `n(0)` at every seat | **agreed content** |
| `event_hash(TABLE_READY from s)` for each `s` | the completed collective stage 0; the set is `R` and the stage does not close until every member is heard | **agreed content by construction** |

## `GENESIS(k)`, `k ≥ 1`

```
GENESIS(k) = h("p2p-poker v1 genesis",
               [ u16_be(protocol_version), table_id, u64_be(k),
                 session_id, roster_hash(k), TERMINAL(k-1) ])

roster_hash(k) = h("p2p-poker v1 roster",
                   [ for each seat s ascending: u8(s) || app_public_key[s]
                                                     || u64_be(stack_at_hand_start[s]) ])
```

| Part | Traced to | Verdict |
|---|---|---|
| `session_id` | above | **agreed content** |
| `roster_hash(k)` — seat index, `app_public_key` | ratified at `TABLE_READY`; changes only at a hand boundary through an accepted `PLAYER_LEAVE` or seat entry | **agreed content, with a placement caveat — K2** |
| `roster_hash(k)` — `stack_at_hand_start` | a function of `TERMINAL(k-1)` and `HAND_INIT`'s `n(11) ledger_delta`, both chained; equal to `roster_hash(k-1)`'s on every abort path, since §4.10 restores | **agreed content** |
| `TERMINAL(k-1)` | `stage_hash` of `HAND_COMPLETE`, **or** `ABORT_TERMINAL(k-1) = h(…, GENESIS(k-1))` | **agreed content, and witness-independent on the abort branch** |
| `seat_flags` | **deleted by D-012**; three corpus hits, none a live construction | **gone** |

## `ctx`

Seven parts: `protocol_version`, `table_id`, `session_id`, `hand_id`, `sequence`,
`shuffle_round` (`0xFF` sentinel for non-shuffle contexts), `sender_public_key`.
Every one is a constant or an envelope field of the event carrying the proof, and
`session_id` is the only composite. **Clean, and clean transitively for the first
time.**

## The parameter-hash field list, compared everywhere it appears

`table_params_hash` appears in **`PROTOCOL.md` only** — §2.8's register, §2.9's
sweep table, §3.1's box, §3.1's prose, §4.3 three times, §7.2 rule 7, §14 — plus two
narrative references in `DECISIONS.md`. **No other document reproduces the field
list, in whole or in part.** There is therefore nothing to compare and nothing that
can drift, which is D-011 rule 2 working as designed. §3.1 says so itself: *"Other
documents reference this box by number and reproduce no part of it."*

**Result of the genesis sweep: no survivor. J1 does not reopen.** One minor finding,
`GENESIS(0)`'s duplicated part, filed as **K5**.

**One thing the sweep looked for and did not find, recorded so it is not re-filed.**
`password_required` (`n(23)`) is excluded from `table_params_hash`, so a founder can
advertise it `true` to one joiner and `false` to another and both derive the same
hash. This is not a defect: the mismatch is resolved at `JOIN_REQUEST` against the
founder's own advert, an admission gate is spent before a seat exists, and a founder
who wants to refuse someone can already refuse them. §3.1 names the exclusion and
this reason.

---

# Part 4 — Termination under D-013

## The twenty phases

`STATE_MACHINE.md` §12.1.1 re-derives all twenty with the exit for each and states
the load-bearing property first: **D-013 adds no transition, deletes none and
changes no guard**, so the exits are the same objects the previous gate cleared. The
re-derivation was checked row by row against §5.2's transition table rather than
read.

| # | Phase | Exit | Fires on |
|---:|---|---|---|
| 1 | `Seating` | T4 → `TableClosed` | local lobby timer |
| 2 | `AwaitingSeatRngCommit` | T4 → `TableClosed` | guard `hand_id == 0 ∧ ledger_in == 0` |
| 3 | `AwaitingSeatRngReveal` | T4 → `TableClosed` | same |
| 4 | `AwaitingKeySetup` (**includes a stalled `HAND_INIT`**) | T57 → `HandAborted` | `hand_deadline_ms` |
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
| 15 | `Settling` | T57 → `HandAborted`, no award applied | `hand_deadline_ms` |
| 16 | `HandComplete` | T47, derived, immediate → `AwaitingKeySetup` \| `Paused` \| `TableClosed` | no external input |
| 17 | `HandAborted` | T46, derived, immediate → `HandComplete` | no external input |
| 18 | `Paused` | T59 on a `PlayerSitsIn` | an external chained event |
| 19 | `TableClosed` | absorbing | — |
| 20 | `Diverged`, hand live | T57 → `HandAborted` | `hand_deadline_ms`, not disarmed at T50 |
| 20 | `Diverged`, no hand started | T4 → `TableClosed` | guard `hand_id == 0 ∧ ledger_in == 0` |

**Every phase has a reachable exit, and none of the twenty exits reads
`signed_this_hand`, `dealt_in`, a status, a certificate or `|V|`.** That was checked
directly against the *Fires after* column and holds. Phases 4–15 and 20 depend on
T57, T57 depends on `hand_deadline_ms`, and §8.2 anchors that timer to
`TERMINAL(k-1)` rather than to `HAND_INIT` — which is R-1's disposition and is what
makes a stalled `HAND_INIT` covered at all. Phase 18's one entry is new under D-013
and is correct: if *every* seat stops signing, `|dealt_in| == 0` and the table idles
rather than burning ten minutes a hand forever.

## Does a table with one silent seat *progress*?

**This is the question the last gate answered wrongly, and the corpus now answers it
correctly for the case it analyses.** §12.1.2 does not stop at "each hand ends":

* **Silent from a hand boundary** — the seat signed nothing in hand `k`; hand `k`
  stalls at `HAND_INIT` and aborts. Hand `k+1` skips it. **One stalled hand.**
* **Silent mid-hand** — it signed `HAND_INIT(k)`, so it is still in `P(k)`; hand
  `k+1` stalls too. Hand `k+2` skips it. **Two stalled hands.** D-013's own summary
  says "exactly one hand"; §12.1.2 corrects its own decision rather than rounding,
  which is the conduct the corpus is supposed to have.
* **Then the table plays.** The skipped seat keeps its seat, its stack and its
  `roster_hash(k+1)` entry, is dealt out, posts blinds and antes as dead money, and
  **genuinely busts**, which is what makes §9.3 condition 1 reachable at all.
* **The arithmetic is done rather than asserted.** Heads-up under
  `RATED_SNG_POKERTH_V1`: `3 × small_blind(h)` every two hands, `small_blind`
  doubling every eleven hands, 10 000 starting stack, **busts at about hand 40**;
  each of those is a `|dealt_in| == 1` drain hand costing `hand_delay_sec` plus a
  round trip, so **about five minutes**. Total cost of an opponent vanishing:
  10 or 20 minutes of stall, five minutes of drain, then the tournament ends.
* **What is not claimed is stated.** *"It is not a bound against an adversary that
  keeps sending: a peer that stalls to the deadline, returns to sign one event, and
  stalls again costs a hand each time."* That is Q3 and it is honest.

**So: yes, a table with one silent seat now progresses — for a seat that is silent
to everybody.** The fixed point D-012 left behind is gone and the corpus's own
correction of its cost model is correct.

**And no, not for a seat that is silent to somebody.** §12.1.2's derivation assumes
the silent seat's silence is symmetric, so every honest peer computes the same
`P(k)`. When it is not — a dropped message, a partition, one relay hop failing — the
peers compute different `P(k)`, and the *second* iteration of that case is not a
stalled hand. It is **K1**, and it is the reason this gate is NOT-READY.

---

# Part 5 — The D-012 class sweep of `PROTOCOL.md`

The previous gate found this document patched at two named sites and never swept.
§2.9 is now the sweep, and it is the right shape: three enumerations, each
*exhaustive by construction* rather than by judgement, with the index named for each
so a missing entry is visible as a gap.

## 1. Hash constructions — 18 enumerated, 0 survivors

Checked against §2.8's register (17 live domain strings plus `DOMAIN_EVENT`) and
against every `h("…")` site in the file. The register and the sweep table agree, and
every construction in the file appears in the register. Three were survivors — all
of them J1 — and all three are fixed. Two entries are worth quoting because they
document properties that are easy to lose:

> `seed` §4.4: "clean — the combine runs over the **chained committer set**, never
> over the reveals a peer happened to receive."

> `subject_digest` §4.8: "clean — `parent_event_hash` is a `stage_hash` and
> `deadline_ms` is the parent's chained `next_deadline_ms`, **not a clock read**."

The candidate the sweep chased and cleared is the one worth keeping on the record:
`TIMEOUT_CERT`'s `n(1) votes` embeds complete `SignedEvent`s *with* signatures, and
§3.2 concedes a malicious signer can produce a second valid signature over one body
— so two honest certificate emitters could embed different bytes for one vote. It
does not fork, because the certificate stage is collective and `stage_hash` is taken
over the **whole** set of certificates rather than over one chosen copy. That is a
real property the collective form buys and §3.2 does not otherwise claim.

## 2. Collective emitter sets — 9 enumerated, 0 survivors

Part 2 above. §2.9's own table and this gate's independent trace agree row for row.

## 3. `PublicTableState` — 24 fields plus 4 flag vectors, 0 survivors

Traced from §6.1 rather than from the sweep table. Three deserve naming:

* `level`, `small_blind`, `big_blind` are **derived from `hand_id` in closed form**
  rather than incremented. That is the single most common place a poker protocol
  admits a per-receiver quantity, and this document closed it before D-012 existed.
* `transcript_head` is the `stage_hash` of the last completed stage, and every
  checkpoint is itself a collective stage, so every emitter has completed the same
  prefix.
* The flag vector **was five and is four**: `folded`, `all_in`, `acted_this_round`,
  `sitting_out`. `absent` is deleted (J5).

§6.1 states the rule from the other side and it is the right statement: *"Every
per-seat flag in this struct must be a deterministic function of accepted chained
events … A seat's state must not be derived from how long that seat has been quiet
at this receiver, from a dropped connection, from a relay status, or from any field
of an accepted `HAND_ABORT`."*

## What the sweep did not look at, and should have

§2.9's three enumerations cover **hashes, emitter sets and hashed state**. They do
not cover **derived sets that are neither hashed nor an emitter set** — and `P(k)`
is exactly that. `P(k)` is never hashed (it is deliberately not a `PublicTableState`
field, and `STATE_MACHINE.md` I31 says so), and it is not itself an emitter set — it
is *what every emitter set is computed from*. It therefore falls through all three
enumerations, and §2.9's table lists it only as a qualifier on someone else's row:
*"`stage_hash`, collective §3.2 — clean **conditionally on `R`**, and `R` is
enumeration 2."*

**A sweep whose index is "the things that are hashed" cannot see a quantity that
decides what gets hashed.** That is the structural reason K1 survived a sweep this
gate otherwise grades as correct, and it is worth one line in §2.9 as a fourth
enumeration: *every derived set that any `R` is computed from*.

---

# Part 6 — The slot key

**One site, clean across all 39 message types.**

The key is an 8-tuple at §5.2.1, with field indices, in a box whose first sentence
is the ownership claim: *"**NORMATIVE. This box is the definition of the anti-replay
slot for the whole corpus.** `STATE_MACHINE.md`, `CRYPTOGRAPHY.md`,
`NETWORK_STACK.md` and `THREAT_MODEL.md` reference this section by number and
reproduce none of it — not the tuple, not a subset of it, not a paraphrase of it."*

`grep` confirms it: no other document reproduces the tuple or any component list.
`STATE_MACHINE.md`'s binding-inputs row for D-011 says *"the key is one literal
tuple in `PROTOCOL.md` §5.2.1 and this document reproduces no part of it"*, and
§5.2's ordering-buffer key — the one thing that could be confused with it — is now
stated separately with an explicit *"This is not §5.2.1's slot key"* (J7).

Three properties make the box better than a tuple:

1. **Absences are justified, individually, and declared closed.** `previous_event_hash`
   (*"a peer that chains one body to two parents at one `sequence` is forking the
   chain, and that must remain an equivocation"*), `payload` / `emitted_at_unix_ms` /
   `next_deadline_ms` (*"a key containing the body is a key no two events ever share,
   and a predicate over it is vacuous"*), `chain_scope` (*"not a key component but
   the predicate's precondition"*).
2. **The redundant component is kept deliberately and the direction is argued.**
   `event_class` is implied by `event_type` and is carried anyway, because *"dropping
   a component merges slots and manufactures false positives against honest peers,
   which is the failure D-009 rule 1 exists to prevent."*
3. **The price of `event_type` is stated rather than hidden**: two different
   `ACTION_*` types at one turn, and `SHOWDOWN_REVEAL` against `SHOWDOWN_MUCK`, stop
   being equivocations and become validity rejections. Both remain rejected (§4.0
   step 12), both remain detected (§5.2.4), both remain evidence.

**The 39-row check.** §4.11's honest-interleaving table is derived **per type from
the key**, not from stage-kind groups, and the numbering 1–39 replaces the group
arithmetic that *"was correct on both occasions it covered a wrong verdict"*. Rows
33, 34 and 36 name the rule their verdict rests on, so an editor cannot delete the
rule without seeing what it carries. Row 36 records that it changed direction from
FAILS to clean and why.

**Verified independently.** All 39 codes in §4.11's summary table appear exactly once
in the interleaving table, the numbering has no gaps, and every `chain_scope = 1`
row's *Varies* column names a component that is in the key. `DISPUTE`,
`PLAYER_LEAVE` and the six `LOBBY_*` types are consistent between §2.3's exhaustive
unchained list, §4.11's `chain_scope` column and their own envelope paragraphs.

**One caveat, and it is K2's.** Rows 37–39 (`PLAYER_SIT_OUT`, `PLAYER_SIT_IN`,
`PLAYER_LEAVE`) are graded *"Clean since M4"* on the reasoning *"one per seat, hand
boundary only, single-writer"*. Two of the eight key components — `hand_id` and
`sequence` — are **undefined for those three types** (`Q-09`). The verdict is
almost certainly right under either placement, but it is a verdict about a key whose
value cannot yet be computed, and that should be recorded on the rows rather than
inferred.

---

# Part 7 — Chip conservation

**Conserved on every path, in both modes, at every peer.** The corpus states the
identity once, at the right level of generality, and every terminal path is checked
against it at the receiver rather than trusted at the emitter.

**The identity.** `C-9`: `Σ stack + Σ committed_hand == ledger_in − ledger_out`.
`ledger_in` and `ledger_out` are inside `state_hash` (§6.1), and both move **only**
at a hand boundary and **only** through `HAND_INIT`'s `n(11) ledger_delta`, which
every receiver recomputes from the accepted join and `PLAYER_LEAVE` events. In
tournament mode it reduces to `sum(final_stacks) == total_chips_in_play`.

| Path | Rule | Checked by |
|---|---|---|
| `HAND_COMPLETE` | `sum(deltas) == 0`; `n(3) final_stacks` recomputed | §4.10 receiver validation, every field, including pot layering, eligible sets, winners and the odd-chip loop |
| `HAND_ABORT`, every `cause` | `n(4) deltas` **all zeroes**; `n(5) final_stacks` **the start-of-hand stacks** | §4.10: *"the check that makes D-010 enforceable at the receiver rather than trusted at the emitter"* — a copy that moves a chip is rejected whoever signed it |
| Abort → next genesis | `roster_hash(k+1)`'s stack vector equals `roster_hash(k)`'s | §3.1, which is why no peer needs the aborted hand's content to build `GENESIS(k+1)` |
| Buy-in / departure | only through `ledger_delta` at a hand boundary | §4.4; a buy-in enters the ledger at the first `HAND_INIT` and nowhere earlier, which is why abandoned formation moves no chips |
| Drain hand, `\|dealt_in\| == 1` | the one dealt-in seat takes every posted blind and ante; `I7` keeps non-dealt-in blind posters out of `eligible` | `STATE_MACHINE.md` §5.3 step 9, T45, I7 |
| Formation abandoned | no chained event but `TABLE_READY`, which has not happened | §4.3 |
| Table faulted (`cause = 4`) | an abort, so neutral; §9.3 condition 0.5 closes the table | §6.4, `STATE_MACHINE.md` §9.3 |

The engine asserts it continuously: `I1`, `I2`, `I27`, plus the no-network harness
playing ~120 000 random hands with a chip-conservation assertion after **every
action**, not every hand.

**The honest qualification, and it is K1's.** Every one of these is a **single-peer**
invariant. Chip conservation says the chips at *this* peer sum correctly; it says
nothing about whether two peers agree on the sum. Cross-peer agreement is carried
entirely by `state_hash` comparison at a checkpoint (§6.2, §6.3). **K1 breaks
cross-peer agreement while leaving every single-peer invariant true at both peers**,
which is exactly why 120 000 hands of harness and 31 invariants would not report it.

---

# Part 8 — New defects

## K1 — `P(k)` is per-receiver on the one path where it narrows, and its payoff is a silent permanent fork — **BLOCKER**

**Severity: blocker. This is a D-012 violation inside the rule D-013 introduced to
fix a D-012 violation, and its failure mode is worse than J2's, because J2 froze
loudly and this diverges silently.**

### The inference that is inverted

§3.2 justifies the whole of D-013 with one sentence:

> §3.2 (l. 1120): "**`P(k)` is not a per-receiver quantity, and that is why this rule
> is available where a status field was not.** 'Did seat `s` sign anything in chain
> `k`' is a function of the accepted chain; **the terminal stage of a chain is
> witness-independent, so two honest peers that reach `TERMINAL(k)` have accepted the
> same chain-`k` prefix and derive the same `P(k)`.**"

The clause after the semicolon does not follow — it asserts the converse of the
property P3 built. `ABORT_TERMINAL(k) = h("p2p-poker v1 abort-terminal", [ …,
GENESIS(k) ])` is **defined** to be a function of the genesis and of nothing else,
and §3.1 says why in terms:

> §3.1: "An abort is, by definition, the outcome in which the peers **could not
> agree about the middle of the hand** — which stage stalled, which contributions
> arrived, which seat was silent. Deriving `TERMINAL(k)` from any of those
> quantities derives it from the one thing that is in dispute."

So on the abort path, reaching the same `TERMINAL(k)` is **evidence of nothing about
the prefix**. That is the entire point of it: two peers holding *different* accepted
prefixes reach the identical terminal, which is what unfroze eleven phases. §3.2
then uses the same property to argue the opposite conclusion.

### Why it matters: the abort path is the only path on which `P` narrows

§3.2 proves this itself, two paragraphs earlier, and does not notice what it has
proved:

> §3.2 (l. 1073): "**if `HAND_INIT(k)` completed, then `P(k)` contains the whole of
> `P(k-1)` at every peer and can narrow nowhere** … **The set can therefore narrow
> at one stage only, stage 0**, which is exactly the case it exists for."

A hand whose `HAND_INIT` completed cannot narrow `P`. A hand whose `HAND_INIT`
stalled is a hand that aborts. So **every narrowing of `P` — the only thing D-013's
rule ever does — happens on a hand that aborted, which is the path on which reaching
the terminal proves nothing about the prefix.** `P(k)` is agreed exactly where it is
inert and per-receiver exactly where it acts.

### The corpus knows the input is per-receiver and mis-sizes the consequence

`Q-10` names it and adopts a default:

> §3.2: "The stage that stalled has no `stage_hash`, so 'seat `s` contributed there'
> is strictly *who was heard*. … **Named default, adopted: a contribution to the
> stalled stage counts.** The residual is bounded and **loud rather than silent** —
> two peers that disagree derive different `n(8) dealt_in`, each rejects the other's
> `HAND_INIT` copy, the stage does not complete, and the hand aborts at the deadline;
> that is a hand lost and recoverable."

That analysis is correct **for one iteration** and is where it stops. Run it twice.

### The trace, heads-up, from one dropped message

Seats `A` and `B`. `P(k-1) = {A, B}`. Hand `k` begins; `B` emits its `HAND_INIT(k)`
copy; the copy to `A` is lost — one dropped frame, a relay reset, a partition. No
peer is malicious and no peer is even silent.

1. **Hand `k`.** `A` has heard only itself; the stage does not complete at `A`. `B`
   has heard both; it completes stage 0 and waits at `DECK_INIT` for `A`, which is
   still at stage 0. Both abort at `hand_deadline_ms`.
   `signed_this_hand` at `A` = `{A}`; at `B` = `{A, B}`.
   *(`I31(c)`: the terminal `HAND_ABORT` adds nobody.)*
2. **Hand `k+1`.** `R(HAND_INIT, k+1) = P(k)`, which is `{A}` at `A` and `{A, B}` at
   `B`. `A` derives `n(8) dealt_in = [A]`; `B` derives `[A, B]`. **`A`'s required
   set has one member and `A` has heard it.** `A` completes stage 0 alone, and
   §5.3 step 9 routes `|dealt_in| == 1` **straight to `Settling`** — *"no key setup,
   no shuffle, no reveal, nothing that could wait on the seat that is not there"*.
   `A` awards itself `B`'s posted blind and emits `HAND_COMPLETE`, whose required
   set is also `{A}`. Hand `k+1` **completes at `A`.** At `B`, the stage stalls and
   the hand aborts. `B`'s `signed_this_hand` for `k+1` is `{B}` — it rejected `A`'s
   `HAND_INIT` copy under §4.0, and *"a `Rejection` adds nothing"* (`I31(a)`).
3. **Hand `k+2` onward.** `P(k+1) = {A}` at `A` and `{B}` at `B`. Each peer's every
   collective stage now has itself as its sole member. **Both play on alone**, each
   draining the other's stack one blind at a time, until §9.3 condition 1 fires —
   *"exactly one seat has `stack > 0` … set `settlement.tournament_winner`"* — at
   both peers, **each naming itself the winner.**

Nothing in this trace is rejected as invalid, no deadline fires after step 2, no
invariant is violated at either peer, and chip conservation holds at both. `A` and
`B` are two tables that were one, and neither can tell.

### Why nothing catches it

* **The deadline machinery cannot.** `A` never waits for anything after step 2.
* **`§5.2.4`'s detection route cannot**, because it terminates in *"the next
  checkpoint (§6.2) shows two `state_hash` values"* and a drain hand places **no
  checkpoint** — see K3.
* **`§6.3` cannot**, because it is entered only from a checkpoint mismatch.
* **The equivocation predicate cannot.** Nobody equivocated. Two honest peers signed
  two different bodies at one `(hand_id, sequence)` — but they are *different
  senders*, and `sender_public_key` is in the slot key.
* **The chain cannot.** `GENESIS(k+2)` is a function of `roster_hash(k+2)` and
  `ABORT_TERMINAL(k+1)`, and `ABORT_TERMINAL` *"reads no participation at all"*
  (§3.2's own words, offered as reassurance). The two peers' genesis values agree at
  step 2 and diverge only once `roster_hash`'s stack vector diverges, at which point
  they have already stopped exchanging anything either would accept.
* **The harness cannot.** 120 000 hands with a chip-conservation assertion after
  every action are single-peer. `I31(d)` is the one invariant that would catch it and
  its own text scopes it away: *"Where the stage completed this holds by
  construction; the residual case … is **Q8** and is `PROTOCOL.md`'s to close."*

### The second, smaller ambiguity in the same rule, which decides which failure you get

Neither document says **whether a peer's own emission counts into its own
`signed_this_hand`.** §5.3's accumulation rule reads *"`step` adds `sender_seat` to
it on **every** event it accepts as content of the current hand"*, and a peer's own
emission is not obviously "accepted". It must count, because §5.3 step 4's base case
— *"On the first hand (T10) the set is the signers of `TABLE_READY`"* — requires a
peer to be in its own hand-1 set. But it is nowhere stated, and the two readings
diverge exactly at step 2 above: **counts ⇒ `P = {self}` ⇒ the silent solo fork;
does not count ⇒ `P = ∅` ⇒ `|dealt_in| == 0` ⇒ `Paused`**, an idle table. Two
conforming implementations, one forks and one freezes.

### Ranked corrections

1. **Ratify the stalled stage, which is what `Q-10` and `Q8` both ask for and both
   defer.** The cheapest ratification that is not "who was heard": require the
   `HAND_ABORT` body to carry the emitter's own contributor set for the stalled
   stage, and define `P(k)` as the **union** over the abort copies a receiver holds,
   or as a function of the *accepted* abort copy. Both are per-receiver in the same
   way `attributed` was, so neither is admissible under D-012 — which is why this is
   a decision and not an edit.
2. **Refuse to let `P` narrow below a floor.** `|R| == 1` is the state in which a
   collective stage stops being collective. A rule that a table with `|P(k)| < 2`
   enters `Paused` rather than dealing costs the drain path — which is the whole of
   §12.1.2's five-minute ending — and buys the property that no peer ever completes a
   hand alone. That is a real trade and it is the owner's.
3. **Make the divergence loud instead of preventing it** — K3's correction. A
   checkpoint on every hand, including a drain hand, converts K1 from a silent fork
   into a `cause = 4` faulted table. This is the smallest edit of the three and it
   does not close K1, it exposes it.
4. **State the own-emission rule** either way, in one sentence, in `STATE_MACHINE.md`
   §5.3. This is free and must happen regardless of which of 1–3 is taken.

### What must be corrected regardless

§3.2's justifying sentence is false as written and must not survive, whatever
replaces the rule. The honest form is: *`P(k)` is agreed wherever the stage that
determined it completed, and is `Q-10`'s residual wherever it did not — which is
every hand in which `P` narrows.*

## K2 — `Q-09` is a wire question, not a placement question, and it sits under the sole re-entry path — **BLOCKER**

**Severity: blocker for implementability. `PROTOCOL.md` §12 grades it as not
blocking, and that grade is wrong on its own terms.**

> §12, `Q-09`: "**Which chain does a hand-boundary single-writer event belong to, and
> at what `sequence`?** `PLAYER_SIT_OUT`, `PLAYER_SIT_IN` and `PLAYER_LEAVE` are
> chained (`chain_scope = 1`) and legal **only** at a hand boundary (§4.10), but
> `HAND_COMPLETE` is the terminal stage of chain `k` and `HAND_INIT` is stage 0 of
> chain `k+1`, so this document names no stage index for them in either chain.
> Nothing depended on it before this pass. … **answering it is a placement decision,
> not a wire change.**"

**It is a wire change, in four places, and each of them is a receiver rule this
document already states normatively.**

1. `hand_id` and `sequence` are envelope fields inside `TO_BE_SIGNED` (§2.4). Two
   implementations that place the event differently produce **different signed
   bytes** for the same intent.
2. They are two of the eight components of the slot key (§5.2.1), which §4.0 step 10a
   *"reads whole"*. A receiver cannot look the event up.
3. `previous_event_hash` must equal `stage_hash(s-1)` (§3.2), so the placement fixes
   the parent too, and §4.0 step 10a's chain-position rule has exactly **one** stated
   exemption in the whole document — the terminal `HAND_ABORT` — and it is not this.
4. §4.0 step 12 asks *"is this `event_type` from this seat expected at this
   `sequence`?"* There is no answer to give it.

**And there is a fifth question the open one does not even reach: what happens when
two seats emit boundary events at one boundary.** Each is a *single-writer* stage
and therefore needs its own `sequence`. Nothing assigns them. `PLAYER_SIT_OUT` from
seat 3 and `PLAYER_LEAVE` from seat 5 at the same boundary must be totally ordered
by a rule every peer computes identically, and none exists.

**Why this is now blocking rather than academic.** `Q-09` says *"nothing depended on
it before this pass"*, which is true, and then treats the new dependency as
bookkeeping. It is not. Under D-013, a seat outside `P(k)` may legally emit
**exactly one** chained event — `PLAYER_SIT_IN` — and every other chained event is
barred to it: it is not in any `R`, not in `dealt_in`, not in `deck.participants`,
not in `V(subject)`, and not in the `HAND_INIT` set that may emit a `HAND_ABORT`.
§4.10 makes this the entire mechanism:

> §4.10: "**A seat rejoins by signing a chained event, which requires it to be
> alive.** That is the entire test and there is no other route: no certificate, no
> vote, no quorum, no proof, no attribution, and no action by any other seat."

So **the one route back into the table runs through the one message whose position
in the chain the document declines to specify**, and `roster_hash(k)` — which feeds
`GENESIS(k)` — reads the outcome, because an accepted `PLAYER_LEAVE` removes a seat
from it. Two peers that place the boundary window differently do not merely disagree
about `dealt_in`; they compute **different `GENESIS(k)`**, after which no event of
hand `k` verifies at either. That is J1's failure mode, total and silent, reached
from a second direction.

**Correction.** §4.10 states the placement for all three types in one paragraph:
which chain (`k`, after `TERMINAL(k)`, is the natural answer and is the one §3.2's
`P(k)` window already assumes), what `sequence` (the successor of the terminal
stage's index, incrementing per accepted boundary event), and the total order when
several arrive (ascending seat index, with the window closing at the emission of
`HAND_INIT(k+1)`). Then §4.11 rows 37–39 cite it, and `Q-09` closes.

## K3 — the drain hand carries no checkpoint, so the corpus's only cross-peer detection route is inert in the regime D-013 made a steady state — **BLOCKER**

**Severity: blocker. This is the mechanism that makes K1 silent, and it is a defect
independently of K1.**

§6.2 lists the checkpoints *"at these points and no others"*:

| `checkpoint` | When |
|---|---|
| `1` | after `TABLE_READY` completes |
| `2` | after `DECK_COMMIT` completes |
| `3`–`6` | after each betting round closes |
| `7` | immediately before `SHOWDOWN_REVEAL` |

Checkpoint 1 is once per table. Checkpoints 2–7 all require a hand that reached
`DECK_COMMIT` or a betting round. `STATE_MACHINE.md` §5.3 step 9 routes
`|dealt_in| == 1` **straight to `Settling`** — *"no key setup, no shuffle, no reveal"*
— so a drain hand reaches none of them. **A drain hand places zero checkpoints.**

That was harmless when the drain path was an edge case. D-013 makes it the steady
state:

> `STATE_MACHINE.md` §5.3 step 9: "**since D-013 it is the path a heads-up table
> takes on every hand after its opponent has gone silent**"
>
> §12.1.2: "**It busts at about hand 40** … Each of those hands is a
> `\|dealt_in\| == 1` drain hand."

So the corpus's disclosed detection route for **every** unlabelled conflict —

> §5.2.4: "peers that accepted different first copies now hold different state; **the
> next checkpoint (§6.2) shows two `state_hash` values**; §6.3 runs; and the hand ends
> through that **chained** path, neutrally, like every other abort. **The outcome is
> identical either way**, which is what makes the labelling difference §5.2.1 accepts
> affordable."

— has no next checkpoint for forty consecutive hands at the end of every table that
ends the way D-013 says tables now end. §5.2.4's *"the outcome is identical either
way"* is the sentence that buys §5.2.1's `event_type` trade, and it is false on the
drain path.

**And there is a second consequence, which is about the ending rather than the
divergence.** §9.3 condition 1 declares a tournament winner and closes the table.
The last state any peer publishes before that is the last checkpoint of the last
contested hand — which may be forty hands and one blind level earlier. **The
tournament result is never checkpointed.** Nothing in the corpus obliges two peers to
agree on who won.

**Correction, and it is small.** Add checkpoint `8`, *after `HAND_COMPLETE`
completes on any hand that placed no other checkpoint*. It opens nothing (§6.2's
no-checkpoint-after-a-public-opening rule is untouched — a drain hand opens no card
at all), it costs one collective round trip on a hand that has none, and it converts
K1's silent fork and every drain-path divergence into a `cause = 4` abort and a
faulted table, which is the outcome §5.2.4 already promises. It is one row in a table
and one sentence.

**K3b, the same class at the engine boundary, and the second half of the re-entry
problem.** `STATE_MACHINE.md` T59 (`PlayerSitsIn`) has from-states
`HandComplete | Paused`. §12.1.1 row 16 gives `HandComplete`'s exit as *"T47,
derived, immediate"*, and §5.2's table row for it reads *"derived `NextHand`. **No
external input** | immediately"*. §8.3 removes the only candidate delay: *"`hand_delay_ms`
is deliberately **not** an engine input … the protocol does not wait for it: T47 fires
as soon as the terminal event of hand `h` is in the chain."*

**So `HandComplete` is a zero-width phase, and it is one of only two phases in which
a `PLAYER_SIT_IN` can be accepted.** The other is `Paused`, which requires
`|dealt_in| == 0` — every seat silent. A seat that goes quiet for one hand at a
table that keeps playing therefore has **no phase in which its re-entry message can
be applied**, and D-013's *"It rejoins by signing a chained event … That is the
entire test"* has no window in which the test can be taken.

The fix is the same edit K2 needs: define the boundary window explicitly — it opens
at `TERMINAL(k)` and closes at this peer's emission of `HAND_INIT(k+1)` — and make
`HandComplete` a phase the engine sits in for that window rather than a zero-width
derived hop.

## K4 — `THREAT_MODEL.md` still carries D-012's superseded cost model, which D-013 exists to correct — medium

D-013 opens with a section headed *"First, a correction"* whose entire subject is
that D-012's stated cost was wrong:

> D-013: "D-012 said the cost of deleting T46's status derivation was that 'a silent
> seat is dealt in every hand and stalls each one until the hand deadline'. **That
> understated it, and the gate found out why.** … **The table makes no progress,
> ever.** D-012's cost model was wrong and is corrected here rather than left
> standing."

`THREAT_MODEL.md` §5.4 carries the superseded model verbatim, in the document's own
voice, as its assessment of what D-012 bought:

> `THREAT_MODEL.md` (l. 2301): "The cost is stated in X33 and in `STATE_MACHINE.md`
> Q7: **nothing marks a seat absent automatically any more**, so a silent seat is
> dealt in every hand and stalls each one to the hand deadline until a human acts."

Three things are wrong with that paragraph now: the cost is not a stall per hand, it
is one or two stalls and then normal play; no human acts, and none can; and it cites
`Q7`, which `STATE_MACHINE.md` closed by dissolving it. `THREAT_MODEL.md` was edited
in this pass (17:10) and carries **one** occurrence of `D-013`, at X33.

A second, smaller one in the same document, §7.3(b): *"On reconnect they rejoin the
key-holder set at the next hand boundary."* Under D-013 nothing rejoins
automatically; the seat must sign a `PLAYER_SIT_IN`, which is K2's message.

**Severity: medium.** It is the deviation and limitation register, and a limitation
register that describes a machine the corpus no longer specifies is worse than a
short one.

## K5 — `GENESIS(0)` hashes the same 32 bytes as two separate parts — low

```
GENESIS(0) = h("p2p-poker v1 genesis",
               [ u16_be(protocol_version), table_id, u64_be(0),
                 table_public_key, table_params_hash, ZERO32 ])
```

§4.1 is normative that *"`table_id` is 32 bytes and **is** the table's Ed25519 public
key, `table_public_key`"*, and `NETWORK_STACK.md` §0.5.6 independently relies on the
same identity. So parts 2 and 4 are the same value.

Harmless cryptographically. Worth a line for two reasons: an implementer will pass
two variables and eventually pass two *different* variables; and §3.1's stated reason
for excluding `protocol_version` and `table_id` from `table_params_hash` —
*"`GENESIS(0)` already carries both as separate parts"* — is an argument that gets
weaker, not stronger, when one of those parts is a duplicate.

**Correction.** Delete `table_public_key` from the part list, or state in one clause
that it is deliberately redundant. Either is a pre-release wire revision on exactly
the footing §4.10 and §6.1 already state for `cause = 5` and the `absent` vector.

## K6 — two documents each claim to be the canonical source of `commitment_i` and `seed` — low

`PROTOCOL.md` §4.4:

> "This is the canonical form for the whole corpus (C-2). The earlier two-part
> `h(..., [r_i, salt_i])` is deleted."

`CRYPTOGRAPHY.md` §7.3, reproducing both constructions in full:

> "**This five-part binding is the canonical one**; the two-part form
> `h(domain, [r_i, salt_i])` that `PROTOCOL.md` §4.4 previously carried does not bind
> the table, the session or the committer and **was corrected to match this
> section**."

The two copies are byte-identical today, so nothing can be wrong *yet*. That is
precisely the condition D-011 rule 1 was written for: *"one normative owner per
concept … reference, never restate"*, adopted after five passes of copies drifting.
`CRYPTOGRAPHY.md` §6.4's `ctx` blocks were deleted under that rule in the H7 sweep;
§7.3's were not, and the ownership sentence points in the opposite direction from
`PROTOCOL.md`'s.

**Correction.** One document keeps the construction and the other keeps the
argument, as §6.4 and §4.5 already do for `ctx`. Given `commitment_i` and `seed`
carry domain strings from §2.8's register and are validated by §4.4's receiver
rules, `PROTOCOL.md` is the owner and `CRYPTOGRAPHY.md` §7.3 should keep points 1–5
and lose the two code blocks.

## K7 — the one status-defined gate on table progression left, and it is a guard rather than an `R` — low

`STATE_MACHINE.md` T59's exit from `Paused`:

> "unchanged, **except** `Paused` + `PlayerSitsIn` when **at least two seats are again
> willing and have chips** → `AwaitingKeySetup`"

*Willing* is `status == Active ∧ stack > 0`. It is deterministic and not
per-receiver, so it does not reopen J2. But it decides whether the table leaves
`Paused` on a **status count**, while §5.3 step 4 — which runs immediately
afterwards, in the same transition's side effects — decides who is actually dealt in
on **participation**. The two can disagree: one seat signs `PLAYER_SIT_IN`, three
other seats are `Active` with chips and silent, the guard passes on four, and step 4
produces `|dealt_in| == 1` and a drain hand. That may be the intended behaviour; it
is not stated anywhere as intended, and it is the one place a status word still
gates progress after D-013's sweep.

**Correction.** Either restate the guard on `|{s : s ∈ signed_this_hand ∧
stack[s] > 0}| ≥ 2`, or add one sentence saying the disagreement is deliberate and
that the drain hand is the intended outcome.

## Checked and found not to be defects

Recorded so the next pass does not re-file them.

* **`JOIN_REQUEST` `n(0) advert_hash` and `JOIN_ACCEPT`'s `advert_event`.** Chased as
  J1 survivors. Both are `chain_scope = 0` with `table_id = ZERO32`; neither reaches
  a chained hash; §2.8's register row makes the lobby-pointer role explicit. Clean.
* **`JOIN_ACCEPT` echoing the joiner's own advert copy.** Two joiners get two
  different `advert_event` byte strings and derive the *same* `table_params_hash`,
  which is the property the fix exists for. §4.3 requires the check anyway, *"so that
  an implementation which ever relaxes the echo rule still carries the parameter
  check"* — which is the right instinct.
* **`password_required` excluded from `table_params_hash`.** Part 3.
* **`next_deadline_ms`'s hand-boundary row reading "`HAND_INIT` after
  `hand_delay_ms`"** against `STATE_MACHINE.md` §8.3's *"the protocol does not wait
  for it"*. The row states a **budget**, not an obligation, and the budget covers the
  immediate emission. Wording drift, not a disagreement; worth one clause at the next
  touch.
* **`TIMEOUT_CERT`'s embedded vote signatures.** Cleared last pass by the collective
  stage form; re-checked, still clean.
* **The blind level closed form.** Derived from `hand_id`, asserted at every hand
  init (I25). No wall clock. Clean.
* **`SPEC_CS.md`'s zero D-013 coverage.** Correct by construction; it predates every
  decision and is never edited.

---

# Part 9 — The verdict, and what to build next

## Is `PROTOCOL.md` implementable without guessing?

**No — but the gap is three items, not three mechanisms, and none of them is in the
39 message shapes.** Everything an implementer needs to encode, sign, hash, validate
and chain a message is present, specified once, and cross-checked. What is missing
is *where two events go* and *what ratifies a stage that did not close*.

**Named by section:**

| Gap | Section | What an implementer must guess today |
|---|---|---|
| **K1** — what ratifies a contribution to the stalled stage, and therefore what `P(k)` is when it narrows | §3.2 (`Q-10`), §4.4; `STATE_MACHINE.md` §5.3 step 4, `Q8` | whether `P` may narrow to a singleton; whether a peer's own emission counts into its own set |
| **K2** — the chain, `hand_id`, `sequence`, parent and total order of the three hand-boundary single-writer types | §4.10 (`0x0803`, `0x0804`, `0x0805`), §12 `Q-09`; §4.11 rows 37–39 | four envelope fields, on the message that is the sole re-entry path |
| **K3** — a checkpoint on a hand that places none | §6.2; `STATE_MACHINE.md` §5.3 step 9 | whether the drain path is meant to run uncheckpointed |
| K3b — the width of the hand-boundary window in the engine | `STATE_MACHINE.md` §5.2 T47/T59, §8.3 | whether `HandComplete` is a phase or a hop |

Everything else is implementable as written. The list of what is *not* a gap is
longer than it has been at any previous gate: canonical bytes and the canonicality
gate (§2.1–2.5), the domain register (§2.8), the three chain shapes (§3.2), all 39
message shapes with per-field limits and receiver rules (§4), universal validation
in normative order (§4.0), the slot key and the 39-row interleaving check (§5.2.1,
§4.11), `state_hash` and the divergence procedure (§6), the lobby (§7), deadlines and
the certificate (§8), size limits (§9), and the constants table (§13).

## Build order for `src/protocol/`

**First, three code/specification divergences that must be settled before anything
is written on top of them.** The commission's premise — *"canonical CBOR with a
canonicality gate and domain-separated hashing already exist and are tested"* — is
half true. The CBOR half is right. The hashing and signing half does not implement
`PROTOCOL.md`.

* **C1 — `signatures::Domain`'s register does not match §2.8's, in every string.**
  The code has seven variants; §2.8 has seventeen live domain strings.
  `Domain::StageHash → "p2p-poker v1 stage-hash"` where §2.8 says
  `p2p-poker v1 stage`; `StateHash → "…state-hash"` where §2.8 says
  `p2p-poker v1 state`; `TableParameters → "…table-parameters"` where §3.1's box and
  §2.8 say `p2p-poker v1 table-params`; `DeckProof → "…deck-proof"` where §2.8 and
  §4.5 say `p2p-poker v1 deck-ctx`. `TableAdvertisement` and `PlayerIdentity` are not
  in the register at all. **And `Domain::Event → "p2p-poker v1 event"` is a string
  §2.8 lists in its *Retired, never valid* table**, replaced by the literal prefix
  `p2p-poker/v1/event`. Twelve register entries have no variant: `transcript`,
  `genesis`, `abort-terminal`, `roster`, `rng-commit`, `rng-beacon`, `deck-commit`,
  `session`, `table-params`, `connection`, `timeout-cert`, `table-id`.
  Note that `TableParameters` was *added in this pass*, naming D-013 and J1 in its
  doc comment, and still carries the wrong string — so the file was opened and the
  register was not compared.
* **C2 — `serialization::hash_domain` is not §2.8's `h`, in two ways.** It calls
  `blake3::Hasher::new_derive_key(context)`, which computes
  `derive_key(context, material)`; §2.8 specifies
  `new_keyed(derive_key(domain, b"p2p-poker/v1"))`. Different functions, different
  outputs. And it takes **one** byte string with **one** length prefix, while every
  multi-part construction in the corpus — `GENESIS`, `roster_hash`,
  `table_params_hash` (25 parts), `ctx` (7 parts), `session_id`, `commitment_i`,
  `seed`, `index_map_hash` — needs `h(domain, parts: &[&[u8]])` with a prefix **per
  part**, which is the property the Phase 0 probe asserted and which the current
  helper cannot express.
* **C3 — `signatures::sign`/`verify` do not produce §2.4's `TO_BE_SIGNED`.** They
  sign `hash(domain, message)`, a 32-byte digest under a `derive_key` domain. §2.4 is
  normative: `TO_BE_SIGNED = DOMAIN_EVENT || u32_be(len(body_bytes)) || body_bytes`,
  where `DOMAIN_EVENT` is the 24 hex bytes printed in §2.4 and §13 — *"the separators
  are slashes, **not** the spaces used by the `derive_key` domain strings of §2.8, and
  the difference is deliberate."* `verify_strict` is correctly used and the
  `hazmat`-avoidance discipline is correctly documented; the bytes are wrong.

None of these is a specification defect. All three are in modules the next phase
builds on, and each would be discovered as a cross-implementation failure rather than
as a test failure, because the current tests assert self-consistency.

**Then, in this order.**

1. **`serialization.rs` — generalise `h`.** `pub fn h(domain: Domain, parts: &[&[u8]])`,
   `new_keyed(derive_key(domain.context(), b"p2p-poker/v1"))`, 8-byte big-endian
   length prefix per part. Port the Phase 0 probe assertions as tests:
   `h(D, ["AB","C"]) != h(D, ["A","BC"])`, and two domains over identical parts
   differ. Keep `to_canonical` / `from_canonical` exactly as they are — the
   decode-re-encode-compare gate is correct and is the one piece that needs no work.
2. **`signatures.rs` — replace the register and the signed bytes.** `Domain` becomes
   the 17 live strings of §2.8 verbatim, plus a test asserting each variant's string
   equals the register and that all are distinct. Add `DOMAIN_EVENT` as a `[u8; 24]`
   constant with the hex from §13 asserted byte for byte, and `to_be_signed(body:
   &[u8]) -> Vec<u8>`. Add a test that the retired strings — `p2p-poker v1 rng-seed`,
   `p2p-poker/seat-beacon/v1`, and `p2p-poker v1 event` with spaces — appear nowhere
   in `src/`, in the shape of `security/rng.rs`'s existing source-walking gate, which
   is the pattern this crate already trusts.
3. **`messages.rs` — the envelope and all 39 payloads.** `#[cbor(array)]`
   throughout, field indices exactly as §4 numbers them, one `u16` code constant per
   type from §4.11, and the per-field and per-collection caps of §9.3 and §9.4 as
   `const`s referenced by the decoders rather than as literals. This is mechanical and
   it is the largest single piece; it is also entirely unblocked, because none of
   K1–K3 touches a message shape.
4. **`validation.rs` (new) — §4.0 steps 1–11.** Steps 12–15 are the engine's and the
   crypto layer's. The order is normative and the module should assert it: a test
   that the canonicality gate runs before the signature check, because §2.5 states
   *"gate, then verify, then interpret"* and that ordering is the whole defence
   against two encodings of one signed event.
5. **`transcript.rs` — the hash chain, minus the two undefined placements.**
   `event_hash`, `stage_hash` in both forms, `GENESIS(0)`, `GENESIS(k)`,
   `roster_hash`, `table_params_hash`, `ABORT_TERMINAL`, `session_id`, `ctx`,
   `index_map_hash`. Every one of these has a complete, checked definition today.
   Write `table_params_hash` against §3.1's box with the 25 parts in the stated order
   and a test that the seven excluded fields change nothing.
6. **`slot.rs` (new) — §5.2.1's 8-tuple and §5.3's bounded anti-replay state**, plus
   the ordering buffer keyed on §5.2's four fields, with a test asserting the two keys
   are different objects. The 39-row interleaving table is a test fixture: one case per
   row, asserting that the honest emissions a row permits do not collide.
7. **Stage machinery — blocked on K1 and K2.** `P(k)`, the required emitter set per
   stage, and the placement of the three hand-boundary types. Do not write these
   speculatively; a wrong `P(k)` is not a bug that surfaces in a test, it is a bug
   that surfaces as two peers playing two games.
8. **Two-peer harness — the thing that would have caught K1.** The existing harness
   is single-peer and 120 000 hands of it prove nothing about K1, because every
   invariant it asserts is true at both forked peers. What is needed is a two-peer
   driver with a **drop-one-message** fault injector and one assertion:
   *after every hand boundary, both peers hold the identical `P(k)`, the identical
   `dealt_in`, and byte-identical derived `HAND_INIT` bodies.* That is `I30(c)` and
   `I31(d)`'s instruction, and D-013 is the reason it is now worth its cost.

## Recommendation

Take **K3**'s correction first — one row in §6.2 — because it is a two-line edit that
converts K1 from silent to loud and is right regardless of how K1 is decided. Then
**K2**, which is a paragraph and unblocks step 7. **K1** needs the owner: it is a
choice between a ratification rule for the stalled stage and a floor on `|P(k)|`, and
each costs something the corpus currently claims.

Steps 1 through 6 are unblocked, are most of the work, and can start now.
