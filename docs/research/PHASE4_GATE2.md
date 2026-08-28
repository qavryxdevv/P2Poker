# PHASE4_GATE2.md — the constants and state gate

**Commission.** Fourteenth pass. The thirteenth (`PHASE4_GATE.md`) cleared every
item on the build list except the constants module, which it blocked on `G6-R1`
— `src/poker/tournament.rs` shipping `hand_deadline_sec: 2_700` against §13's
normative `3 300 000`. This pass decides whether `src/protocol/constants.rs`,
`PublicTableState` and `state_hash` can now be written, and re-runs the freeze
path behind them.

**Method, unchanged, and it is what produced this pass's blockers.** Nothing is
graded off a summary row, off a disposition column, or off `DECISIONS.md`'s own
account of what landed. `G6-R2` cost the last pass a finding against its own
method — a filing claim accepted because of where it appeared — so every
"DONE" cell below was checked against the file it points at. Two things are new
in the method here, and both found something:

1. **The last pass compared one document against the code in the place the code
   had moved last.** This pass ran that comparison in the other direction, and
   over **every** document rather than the owner: `RATED_SNG_POKERTH_V1`'s
   values were re-read at every site that lists them, not only at §13. That is
   `G7-S1`.
2. **A field named in a formula was grepped for as a field, not as a phrase.**
   `HAND_DEADLINE_FLOOR(n)`'s dominant term is `crypto_step_timeout_ms`;
   `STATE_MACHINE.md` §9.4 enforces that formula; the identifier
   `crypto_step_timeout` occurs in `STATE_MACHINE.md` **zero times**. That is
   `G7-S2`.

**Files as read, 2026-08-28.** `DECISIONS.md` **21:39**, `PROTOCOL.md`
**21:39**, `STATE_MACHINE.md` **21:34**, `src/security/validation.rs` **21:23 —
edited after the last gate, and it closed `Q2`**, `src/poker/tournament.rs`
**21:20**, `NETWORK_STACK.md` 21:00, `CRYPTOGRAPHY.md` 20:59, `THREAT_MODEL.md`
**20:53 — unchanged, second consecutive pass**, `DEPENDENCIES.md` 19:33 —
**unchanged, third consecutive pass**, `CONTRIBUTING.md` 19:32 — **unchanged,
third consecutive pass**, `SPEC_CS.md` 09:29.
`cargo check --lib -j 19` clean; `cargo test --lib -j 19 -- --test-threads=19`
— **139 passed, 0 failed** (125 last pass); `cargo test --tests` — **4 passed**,
the random-hand harness at 2, 6 and 10 seats and the all-in walk.

> **The timestamp finding, tenth iteration, and it inverted a second time.** The
> rule's prediction last pass was *the two files not opened are `CONTRIBUTING.md`
> and `DEPENDENCIES.md` — and `src/security/validation.rs`, the named owner of an
> open item for two passes*. `validation.rs` was opened at 21:23 and `Q2` closed
> exactly as specified. **The rule's other half — *the file edited last is where
> the unpaid debt is* — points at `DECISIONS.md` and `PROTOCOL.md`, both 21:39,
> and it is wrong for the first time in the series: both are clean.** The debt
> this pass found is in `STATE_MACHINE.md`, edited at 21:34, five minutes
> earlier, in **the two sections no gate in this series has ever had a reason to
> open** — §9.1, a preset listing, and §2.3, a config struct. The rule should now
> be read as *the file edited last under a fix is clean; the file edited last
> under a sweep is not*, and §9.1 and §2.3 were neither.

---

## Verdict counts

| Verdict | Count | Items |
|---|---:|---|
| **RESOLVED** | 7 | P7, Q2, G6-R1, G6-R2, G6-R3, G6-R4, G6-R5 |
| **PARTIAL** | 3 | Q6, G4-P3-e, G6-R6 |
| **UNRESOLVED** | 0 | — |
| **REGRESSED** | 0 | — |

**This is the cleanest carried set in the series and the first pass with no
UNRESOLVED item.** `G6-R1` closed in both directions — the code moved to
`3 300` and gained a test that pins it to a literal rather than to an inequality,
and `2 700 000` is gone from the corpus entirely. `Q2` closed in the type
system rather than in a comment. `P7` closed by **deletion**, which is the
better of the two available fixes.

**Nine new defects, `G7-S1`–`G7-S9`. Two block.** Both are in
`STATE_MACHINE.md` and both are the same shape as `G6-R1`: a second listing of
a quantity whose owner is another document.

* **`G7-S1` — `600` survived, in a preset listing nobody has swept.**
  `STATE_MACHINE.md` §9.1 l. 4176 carries `hand_deadline_sec = 600` inside a
  full `RATED_SNG_POKERTH_V1` block. That is **600 000 ms**, below
  `HAND_DEADLINE_FLOOR(10) = 2 297 000` and below the new admitted minimum
  `HAND_DEADLINE_MIN(10) = 2 522 000`. **`P3` is reopened by one survivor**, and
  `G6-R1` is repeated in a file the sweep for it did not cover.
* **`G7-S2` — the engine's own config struct cannot express the engine's own
  bound.** `crypto_step_timeout_ms` appears **nowhere** in `STATE_MACHINE.md`:
  not in §2.3's `TableConfig`, not under any other name. `Deadline.duration_ms`
  says *"from config"*; fifteen transitions arm `ArmDeadline{Crypto}`; and §9.4's
  validator enforces `hand_deadline_ms >= HAND_DEADLINE_FLOOR(seats)`, whose
  largest term is `(2n + 23) * crypto_step_timeout_ms` — **1 290 000 of the
  2 297 000 at ten seats**.

**Readiness verdict: BUILD everything on the list, including the constants
module, sourced from `PROTOCOL.md` §13 and §8.2 alone. Do not build
`TableConfig` from `STATE_MACHINE.md` §2.3, and do not build a
`table_params_hash` producer at all.** Part 8 gives the per-item answer.

---

# Part 1 — Per-item verdicts

## P7 — `PublicTableState`'s field order is given twice, differently — **RESOLVED**

**Closed by deletion, which is the fix `P7-w` asked for and the better of the two
available.** The alternative — aligning §2.9's order to §6.1's — would have left
two listings and required them to be kept in step for ever; this leaves one.

`PROTOCOL.md` §2.9 enumeration 3, l. 790, now reads:

> **3. `PublicTableState`.** Every field of **§6.1's table**, which is this
> enumeration's index and is the **one normative statement of the struct's field
> order** in this document; the list that stood here is deleted rather than
> corrected (`P7-w`).

and it states the class rather than the instance: *"A sweep is a list of verdicts
and has no business being a second wire definition … a fourth listing anywhere is
a defect on sight (D-011 rule 2)."*

**§6.1 gained a normative box above its table**, l. 4964:

> **Normative, and this is `P7-w`.** The table below is the **field order**, read
> top to bottom, left to right within a cell. `#[cbor(array)]` means the order
> **is** the encoding, so this table is not a summary of the struct — it is the
> struct. It is the only field-order statement this document makes … An
> implementer transcribing `PublicTableState` into `src/protocol/messages.rs`
> reads this table and nothing else.

`STATE_MACHINE.md` §3.3's box is unchanged and now points at a single list rather
than adjudicating between two. `DECISIONS.md` `P7-w` is marked **closed**, and it
records what it corrected on the way — a false filing claim in §3.3 — which is
`G6-R2`'s discipline applied by the row that found it.

**Three listings became one. The class cannot recur by drift; it can only recur
by somebody adding a fourth listing, which §2.9 now calls a defect on sight.**

## Q2 — `AgreedCheckpoint` witnesses the hand and the emitters and not the position — **RESOLVED**

`src/security/validation.rs` was opened at **21:23**, and the fix is in the type
rather than in a comment:

```rust
pub struct AgreedCheckpoint {
    state_hash: Hash,
    sequence: u64,
    /// Which checkpoint of the hand this is, `1..=8`.
    number: u8,
    hand_id: u64,
    emitters: Vec<PlayerId>,
}
```

`covering` gained a range check — `CheckpointOutOfRange` outside `1..=8` — and
`Tier2Finding::new` gained the position half as a **refusal**:

```rust
if against.number() < fixed_at_checkpoint {
    return Err(WitnessError::CheckpointTooEarly { have: ..., need: ... });
}
```

That is T64's `c.number >= n` enforced by construction. `Tier2Finding::new` is
the only constructor, so a caller holding a checkpoint from earlier in the hand
**cannot build the finding at all** — the same technique that made the tier-2
precondition unforgeable in `P6`, applied to the half `P6` left open. Two new
tests land with it, `a_later_checkpoint_still_witnesses_an_earlier_requirement`
and its negative, and both pass.

**The pre-flop-action-judged-against-river-state attack is now unrepresentable
rather than unreachable**, which is the difference the item asked for and the
reason it was worth landing before `D-014-3` gives T64 a producer.

**One residual, filed below as `G7-S8`:** `sequence` is now read by nothing.

## Q6 — an advertisement at exactly the floor tolerates zero reopening raises — **PARTIAL**

**Renamed `G5-Q6` on arrival, DECIDED, and the owner's half is fully closed.**
`PROTOCOL.md` §8.2 adds the mechanism by naming a denominator rather than by
adding a quantity:

```
REOPENING_COST(n)    = (n − 1) * (action_timeout_ms + action_grace_ms)
HAND_DEADLINE_MIN(n) = HAND_DEADLINE_FLOOR(n) + REOPENING_COST(n)
```

with the argument stated in the form that makes it non-negotiable — *"The floor
above budgets a **no-re-raise** hand … That is a legal hand and it is not
poker"* — and `n(17) >= HAND_DEADLINE_MIN(n)` observed to be **exactly**
`REOPENINGS >= 1`, which is why no second formula was written. §7.2 rule 2a
compares against `HAND_DEADLINE_MIN`, §7.2's `n(17)` row states the new lower
bound, and §13 reproduces the two formulae as names. The non-emptiness check
that has to pass before a lower bound may be raised was run and stated:
`2 522 000 < 3 600 000` at ten seats.

**What keeps this PARTIAL is the engine half, and `DECISIONS.md` names it as
owed in the same row that closes the other one:** *"**Owed:** `STATE_MACHINE.md`
§9.4's config validator must compare against `HAND_DEADLINE_MIN`, not the
floor."* §9.4 l. 4373 reads:

```
hand_deadline_ms  >= HAND_DEADLINE_FLOOR(seats)        // PROTOCOL.md §8.2 owns the formula
```

**The consequence is not cosmetic and it is new.** Between `FLOOR(n)` and
`MIN(n)` this engine **starts** a table that every conforming joiner **refuses**
under §7.2 rule 2a — a founder whose own client plays a table nobody may enter.
And §9.4's prose now asserts the opposite in terms: *"The engine's check and the
joiner's advert check (§9.4 rule 2a) are now the same predicate over the same
fields."* That sentence is false in both halves — the predicates differ, and
§9.4 is *Collection bounds* and has no numbered rules. Filed as `G7-S4`.

## G4-P3-e — the `600 000` sweep and the §9.4 validator line — **PARTIAL**

Four parts; **two closed, one closed at the wrong right-hand side, one
untouched.**

| Part | State |
|---|---|
| the literal at §2.3 l. 153 — `pub hand_deadline_ms: u32, // 600_000` | **closed.** Now `// >= HAND_DEADLINE_FLOOR(seats), PROTOCOL.md §8.2 — not a constant; 3_300_000 at the §13 preset` |
| the validator line at §9.4 l. 4373 | **moved, to `HAND_DEADLINE_FLOOR` rather than to `HAND_DEADLINE_MIN`** — see `Q6` above and `G7-S4` |
| the stale figure in *"roughly a dozen places (§2.6, §5.2 T61, §8.6, §9.3, §9.4, §12.1)"* | **closed in all six.** `grep "600 000\|600_000\|600000"` over `STATE_MACHINE.md` returns **nothing** |
| the `THREAT_MODEL.md` §5.2 attacker row — *a founder advertising a deadline every legal hand exceeds* | **not written.** `THREAT_MODEL.md` is unchanged at 20:53 and carries no such row; `PROTOCOL.md` l. 6756 carries the mitigation and says the catalogue entry *"is filed for `THREAT_MODEL.md` in `DECISIONS.md`'s open list"* |

**And the sweep the row commissioned did not cover the file the survivor is in.**
The row names §2.6, §5.2, §8.6, §9.3, §9.4 and §12.1. **§9.1 is not on that
list, and §9.1 is where `600` survived** — see `G7-S1`. A sweep specified by
section list rather than by pattern misses exactly the section nobody thought of,
which is what a sweep is for.

## G6-R1 — the shipped preset and the normative preset are different tables — **RESOLVED**

**Closed in the direction the last gate said it should close — the code moved to
the document — and closed a second time by a test that can hold it.**

`src/poker/tournament.rs` l. 83 now reads `hand_deadline_sec: 3_300`, with the
comment naming its owner: *"Normative in `PROTOCOL.md` §13."* `2 700 000` is
**gone from the corpus and from the code** — `grep` over all nine documents and
all of `src/` returns nothing.

**The test is the part worth crediting, because the old ones were the defect.**
`the_rated_preset_matches_the_normative_value_exactly` asserts a literal in both
directions:

```rust
assert_eq!(RATED_SNG_POKERTH_V1.hand_deadline_sec, 3_300,
    "PROTOCOL.md section 13 is normative for a named preset");
assert_eq!(RATED_SNG_POKERTH_V1.reopenings(), 4,
    "section 8.2 derives four reopening raises at this value");
```

The second assertion is the one that closes the class: §8.2's sentence *"At the
§13 preset's new value this is four at ten seats"* is now a **falsifiable
assertion in the code** rather than a claim in prose, and it is what the old
`reopenings() >= 1` inequality could not be. The doc comment on the test states
the lesson in the form the next editor needs: *"An earlier version shipped
2 700 s. It cleared the floor and passed a `reopenings() >= 1` check, and was
still wrong: it made this client a different table from every conforming one."*

**An inequality cannot pin a constant, and this is now the only place in the tree
where a constant is pinned by equality against a document.** That is the right
number of places for it to exist — and `G7-S1` is what happens where it does not.

## G6-R2 — §2.6's residual says it is filed and it is not — **RESOLVED**

**Both halves.** `DECISIONS.md` l. 1549 carries the row, and it states the
residual in full — the boundary window makes `signed_this_hand` two-membered, so
step 8 writes nothing while the retained record says solitary by the second
disjunct — together with the disposition and its **reason for being a
disposition**: widening step 8 to read the boundary checkpoint's `required`
snapshot is *"a **second read of a second quantity for one question**, the defect
`K-7` and `L7` each cost a pass"*. It states what is lost (a freeze on an older
hand), what replaces it (a loud stall on the current one), and the condition
under which to revisit (*"only if a measured trace shows the stall path failing to
fire"*).

**The method half is filed separately as `G6-R2-m`**, as a rule for `research/`
rather than as a defect in a document, which is the correct owner: the failure
was a gate's, not a corpus's. **I ran it against this pass.** Every "DONE" and
every "filed" claim graded above was checked against the file it names, and it
is how `G7-S9` was found — `G6-R6`'s row says two lines are owed to the code and
neither is written.

## G6-R3 — T64's signpost points at the half that landed — **RESOLVED**

`STATE_MACHINE.md` T64 l. 2452 now reads:

> `c.required` is what carries §4.9's *"the emitter set contained both the accused
> and this receiver"*, and **`src/security/validation.rs` witnesses that clause and
> this row's `c.number >= n` clause as well** … So the whole tier-2 precondition is
> **enforced by construction rather than described**.

and it names its own former error rather than quietly replacing it: *"The sentence
that stood here said the type 'cannot witness' the emitter clause; it was false
when `P6` landed and is false twice over now that the position half has landed as
well."* An implementer reading T64 now finds that the module owes **nothing**,
which is true, where before they would have re-implemented `emitters` and left
`number` unwritten.

## G6-R4 — D-014 has no setup-chain exception — **RESOLVED**

`DECISIONS.md` D-014 gained a named subsection after *What removal does*, l. 1392:
**"The one exception: the setup chain removes nobody (`G6-R4`)"** — *"In the setup
chain — `hand_id == 0`, before the first hand exists — a tier-1 finding removes
nobody, and the table simply does not start."* It records that the decision was
taken by T66 and by `NETWORK_STACK.md` and is written here because *"a reader
following the authority order"* found a decision and a transition that disagreed.

**Carried five passes, named in four gate reports, closed in one line.** T66's
row is unchanged, which is correct: the exception belonged in the decision, not
in the transition that already implemented it.

## G6-R5 — §4.9's restatement claims exhaustiveness with one disposition missing — **RESOLVED**

Fixed by **replacing the enumeration with the pointer**, not by adding the fourth
item, which is the D-011 rule 1 form of the fix. §4.9 l. 3348:

> **§4.0 row 10b is the sole authority on what becomes of it** — its outcome
> column, *"freeze, compare, readmit, or drop"*, is the enumeration, and this box
> reproduces no part of it (D-011 rule 1).

and it states the consequence the box actually needs — *"whichever of those four
dispositions applies, the copy is never applied and never enters a `stage_hash`"*
— which is the whole of what the close bounds and is true of all four. The
missing case is named on the way past: *"the ordinary **drop** of an agreeing copy
from a seat **inside** the recorded set — the §1.5 forwarding reorder this box's
own `N6` argument is built on, and not an adversarial case at all."*

**A restatement that becomes a pointer cannot drift from its owner again.** That
is a stronger fix than the one the item asked for.

## G6-R6 — `HEADS_UP_PLAY_MONEY_V1` is a named preset no document defines — **PARTIAL**

**The document half is closed, and the decision is better than either option the
item offered.** The item proposed *add the preset to §13, or delete the
`preset_id` claim*. `DECISIONS.md` took the second and then closed the **class**:

* **§7.2 `n(2)` is now a closed two-value enum** — `"RATED_SNG_POKERTH_V1"` or
  `"CUSTOM"` — and rule 3 rejects any third value **on sight, whether or not this
  client knows the name**. That closes the hazard for every future invented name,
  not only this one.
* **§13 carries the twelve values as a *heads-up reference configuration*,
  explicitly not a `preset_id`**, advertised as `CUSTOM`, with every value
  justified and the two that are genuinely this configuration's separated from
  the ten copied from the rated preset.
* **Pinning was rejected on provenance, and the reasoning is the strongest single
  paragraph in this pass:** *"A `preset_id` is a claim about values carried by a
  name … That claim is only worth its cost when an outside authority fixes the
  values — PokerTH's own rated-game settings check does, and there is **no
  analogous authority for a two-seat rated game** … Pinning them under a name
  would manufacture the appearance of provenance for a configuration this corpus
  invented."*

**What keeps this PARTIAL is that the row itself says two lines are owed to the
code, and neither is written.** `src/poker/tournament.rs` l. 99 still carries
`id: "HEADS_UP_PLAY_MONEY_V1"` — the field that would populate `n(2)` — and its
doc comment still carries *"named so that two clients can agree on it without
negotiating"*, which is the exact claim the decision refuses. Filed as `G7-S9`.

---

# Part 2 — The deadline sweep

**Every numeric hand-deadline figure in every document, with its value and
whether it is at or above the bound for the seat count it applies to.** Run as a
pattern sweep over all nine documents and all of `src/`, not as a section list —
which is what `G4-P3-e` did and is why it missed one.

`FLOOR(n) = 7 000 + (2n + 23)·30 000 + 4n·25 000`;
`MIN(n) = FLOOR(n) + (n − 1)·25 000`. Both recomputed here, not carried:

| `n` | crypto term | action term | **`FLOOR(n)`** | `REOPENING_COST` | **`MIN(n)`** |
|---:|---:|---:|---:|---:|---:|
| 2 | 810 000 | 200 000 | **1 017 000** | 25 000 | **1 042 000** |
| 4 | 930 000 | 400 000 | **1 337 000** | 75 000 | **1 412 000** |
| 6 | 1 050 000 | 600 000 | **1 657 000** | 125 000 | **1 782 000** |
| 8 | 1 170 000 | 800 000 | **1 977 000** | 175 000 | **2 152 000** |
| 10 | 1 290 000 | 1 000 000 | **2 297 000** | 225 000 | **2 522 000** |

### Every figure

| # | Site | Value | Applies at | Bound | Verdict |
|---:|---|---:|---:|---:|---|
| 1 | `PROTOCOL.md` §13 l. 7037 `hand_deadline_ms = 3 300 000` | 3 300 000 | `n = 10` | `MIN(10) = 2 522 000` | **at/above — clears by 778 000; `REOPENINGS = 4`** |
| 2 | `PROTOCOL.md` §13 l. 7125 heads-up reference configuration | 1 200 000 | `n = 2` | `MIN(2) = 1 042 000` | **at/above — clears by 158 000; `REOPENINGS = 7`** |
| 3 | `PROTOCOL.md` §7.2 `n(17)` row l. 5590 | *derived*, `MIN(n(11)) … 3 600 000` | any | — | **no literal — correct by construction** |
| 4 | `PROTOCOL.md` §7.2 rule 2a l. 5628 | *derived* | any | — | **no literal** |
| 5 | `PROTOCOL.md` §8.2 floor table l. 5954–5957 | 600 000, four rows | 2, 4, 6, 10 | — | **historical, and labelled `— below` in its own cell**; the column header is *"preset before `P3`"* — **not a live figure** |
| 6 | `PROTOCOL.md` §8.2 l. 5904, 5908, 5910 | 600 000 | — | — | **narrative of the defect `P3` fixed — not a live figure** |
| 7 | `PROTOCOL.md` §6.4 l. 6164 | 3 300 000 | `n = 10` | `MIN(10)` | **at/above** |
| 8 | `PROTOCOL.md` §13 l. 7009 `REANNOUNCE_INTERVAL_MS = 600 000` | 600 000 | — | — | **not a hand deadline.** A lobby re-announce interval that happens to share the digits. **It is the one `600 000` the constants module must write** |
| 9 | `STATE_MACHINE.md` §2.3 l. 153–154 | *derived*; `3_300_000` named as the preset's | any | — | **at/above — the `G4-P3-e` literal is gone** |
| 10 | `STATE_MACHINE.md` §8.x l. 2167, 3726, 4129, 4724 | 3 300 000 | `n = 10` | `MIN(10)` | **at/above, four sites** |
| 11 | `STATE_MACHINE.md` §2.6 l. 2906 | 2 297 000 / 3 300 000 | 10 | — | **at/above; quotes the floor as a floor** |
| 12 | `STATE_MACHINE.md` §12.1 l. 5176, 5183 | 2 034 000, 1 017 000 | `n = 2` | `FLOOR(2) = 1 017 000` | **at/above — these are *costs of a stall* expressed in floors, not advertised values** |
| 13 | `STATE_MACHINE.md` §9.4 l. 4373 validator | *derived*, `>= FLOOR(seats)` | any | `MIN(seats)` | **BELOW THE ADMITTED MINIMUM by one `REOPENING_COST` — `G7-S4`** |
| 14 | **`STATE_MACHINE.md` §9.1 l. 4176 `hand_deadline_sec = 600`** | **600 000** | **`n = 10`** | **`FLOOR(10) = 2 297 000`** | **BELOW THE FLOOR by 1 697 000 ms — `G7-S1`** |
| 15 | `src/poker/tournament.rs` l. 83 | 3 300 000 | `n = 10` | `MIN(10)` | **at/above** |
| 16 | `src/poker/tournament.rs` l. 110 | 1 200 000 | `n = 2` | `MIN(2)` | **at/above** |
| 17 | `src/poker/tournament.rs` l. 342 `600_000` | 600 000 | 2…10 | — | **a regression assertion that 600 000 is below the floor at every seat count — the one correct bare `600_000` in the tree** |
| 18 | `THREAT_MODEL.md` l. 1819, 1830, 1851, 1852, 1905, 2255 — *"ten minutes"* | 600 000 implied | — | — | **stale prose, conservative in direction — `G7-S7`** |
| 19 | `PROTOCOL.md` l. 6185, 7262 — *"ten minutes"* | 600 000 implied | — | — | **stale prose, and l. 7262 is inside §13, the owner — `G7-S7`, unfiled** |

### Result

> **The sweep fails on one figure. `P3` is reopened.**
>
> **`STATE_MACHINE.md` §9.1 l. 4176 carries `hand_deadline_sec = 600` inside a
> full `RATED_SNG_POKERTH_V1` listing.** It is 600 000 ms at `n = 10`, against a
> floor of 2 297 000 and an admitted minimum of 2 522 000 — **below its own floor
> by a factor of 3.8**, and below the action term alone (1 000 000 ms of pure
> human time at ten seats). A client built from it aborts **every hand** under
> §8.4 with `cause = 1`, `attributed = []` and `cert_hash = None`: no progress,
> nobody at fault, repeatable for ever, on a table where every peer is honest and
> every message is on time. **That is `P3`'s payoff verbatim, and `P3`'s own
> §8.2 text says so at l. 5910.** Filed as `G7-S1`, and it is a blocker.

Three things make this worse than a stale number, and each is why it must be
fixed before the constants module and not after:

1. **It is inside a `preset_id`.** §7.2 rule 3 is normative that
   `"RATED_SNG_POKERTH_V1"` implies §13's exact values; `hand_deadline_ms` is a
   part of `table_params_hash` (§3.1). A client built from §9.1 and a client
   built from §13 compute different `table_params_hash` for the same name and
   **cannot join each other's table** — rule 3 rejects the advert, rule 7 marks
   it unjoinable. That is `G6-R1` exactly, moved from a source file to a
   document, and it survived the pass that closed `G6-R1`.
2. **`STATE_MACHINE.md` contradicts itself, not just `PROTOCOL.md`.** l. 2167,
   3726, 4129 and 4724 all state 3 300 000 ms as the preset's value. §9.1 states
   600. Four sites against one, in one file — and l. 4457 calls §9.1 *"fully
   specified"*, so the one is the site an implementer is sent to.
3. **No test can catch it.** `every_shipped_preset_meets_its_own_floor` tests the
   presets in `tournament.rs`, which are right. Nothing in the tree reads
   `STATE_MACHINE.md` §9.1, and nothing can.

**The fix is a deletion, not an edit.** §9.1 must not carry the number at all:
`PROTOCOL.md` §13 is the owner under D-011 rule 1, §9.1's whole block is a second
listing, and correcting `600` to `3 300` would leave the same drift hazard
pointing at the next number to move. Delete the block and cite §13. That also
disposes of `G7-S2`'s §9.1 half and of `G7-S5`.

---

# Part 3 — The preset identity sweep

**For every named preset in any document or in `src/poker/tournament.rs`, is
every value it implies pinned in exactly one place?** The test is not *do the
sites agree* but *is there one site* — two agreeing sites are a defect that has
not fired yet, which is what `G6-R1` was for a day.

The consequence is fixed by §3.1: **twenty-five parts enter `table_params_hash`**,
including `preset_id`, `mode`, `min_buyin`, `max_buyin`, `hand_deadline_ms`,
`crypto_step_timeout_ms` and every blind-schedule field. Two clients that differ
on any one of them compute different hashes and cannot join each other.

### The named presets, after `G6-R6`

§7.2 `n(2)` is a **closed two-value enum**: `"RATED_SNG_POKERTH_V1"` and
`"CUSTOM"`. `"CUSTOM"` implies nothing and carries every value in its own advert
fields, so it has no identity to sweep. **There is exactly one named preset in
version 1**, and `HEADS_UP_PLAY_MONEY_V1` is correctly no longer one of them.

### `RATED_SNG_POKERTH_V1` — the value sources

| Site | What it lists | Agrees with §13? |
|---|---|---|
| **`PROTOCOL.md` §13 l. 7022–7045** | eighteen values | **owner** |
| `PROTOCOL.md` §7.2 l. 5605–5610 | the `BlindSchedule` four, plus `seats`, `start_stack`, `ante` | **yes** — but it is a second listing (see below) |
| **`STATE_MACHINE.md` §9.1 l. 4156–4180** | twenty-one values, and l. 4457 calls it *"fully specified"* | **NO — `hand_deadline_sec = 600` (`G7-S1`)**, and it omits `crypto_step_timeout` entirely (`G7-S2`) |
| `src/poker/tournament.rs` l. 68–84 | fourteen values | **yes**, and `hand_deadline_sec` is pinned by equality to §13 |

### Result

> **The preset identity sweep fails, in two independent ways.**

**1. Two sites disagree — `G7-S1`, above.** `hand_deadline_ms` is 3 300 000 at
§13 and in the code, and 600 000 at `STATE_MACHINE.md` §9.1.

**2. Three values the preset implies are pinned in *no* place at all — `G7-S3`,
and this one is new.** `table_params_hash` takes `n(1) mode`, `n(7) min_buyin`
and `n(8) max_buyin`. **§13's `RATED_SNG_POKERTH_V1` block states none of the
three.** §7.2 gives only ranges — `min_buyin >= big_blind`,
`max_buyin >= min_buyin` — and `n(1) mode` admits **two** values,
`1 = CASH_PLAY_MONEY` and `2 = TOURNAMENT_SNG_PLAY_MONEY`.

So §7.2 rule 3 says the name implies §13's exact values, and for three parts of
the hash §13 has no value to imply. **Two conforming clients, both correctly
implementing the rated preset from its owner, pick different `min_buyin` and
compute different `table_params_hash`.** That is `G6-R1`'s payoff with no digit
disagreement anywhere to notice — the two clients are not even wrong, and neither
can be shown to be.

**The evidence that this is an oversight and not a deliberate degree of freedom
is in the same section, written in the same pass.** The heads-up reference
configuration at l. 7110 pins **`mode = 2`** and
**`start_stack = 10 000 (= min_buyin = max_buyin)`** explicitly. The newer block
knew to pin them; the older one predates the rule that made it matter.

`game` and `deck_suite` are the two remaining unpinned parts and they are **safe**:
version 1 admits exactly one value of each (`n(0) = 1`, `deck_suite` must be
`"bs-bg12-secp256k1/1"`), so there is nothing for two clients to disagree about.
`protocol_version` and `table_id` are excluded from the hash by name.

**Two further findings from the sweep, both about *how many* places rather than
*which value*:**

* **`PROTOCOL.md` §7.2 l. 5605 is a second listing of six preset values inside the
  owner document.** It agrees today. It is the shape that produced `G6-R1`, and
  under D-011 rule 2 it should cite §13 rather than restate it. Low, and it is
  the only one of the four sites that has an argument for existing — it is
  explaining `BlindSchedule`'s encoding, not specifying the preset.
* **`src/poker/tournament.rs`'s `Preset` carries fourteen of the twenty-five
  hash parts.** It has no `mode`, `min_buyin`, `max_buyin`, `button_rule`,
  `odd_chip_rule`, `showdown_policy`, `game` or `deck_suite`. **A
  `table_params_hash` cannot be computed from a `Preset`**, which is a
  build-ordering fact rather than a defect — and it is the reason Part 8 refuses
  to build the hash producer: three of the eight missing fields have no pinned
  value to add.

---

# Part 4 — The field-order sweep

**Every place `PublicTableState`'s fields are listed, and whether they agree.**

| Site | Is it a field order? | State |
|---|---|---|
| **`PROTOCOL.md` §6.1 l. 4964–4990** | **YES — the one normative site** | The table, under a normative box that says *"this table is not a summary of the struct — it is the struct"* |
| `PROTOCOL.md` §2.9 enumeration 3, l. 790 | **no, and it says so** | The second listing is **deleted**. Reads *"Every field of §6.1's table"* and states that *"this paragraph enumerates verdicts and never order"* |
| `STATE_MACHINE.md` §3.3 l. 921–930 | **no, and it says so** | *"`PROTOCOL.md` §6.1's table, read top to bottom, is the field order the engine builds `PublicTableState` in. No other listing anywhere is a field order and none may be read as one"* |
| `STATE_MACHINE.md` §2.3, §2.8, l. 424, 601, 789–802 | **no** | Membership statements about individual fields — *"it is **not** a field of `PublicTableState`"* for `certified_subjects`, `readmit`, `checkpoints`, `table_faulted`. Sets, not order |
| `src/protocol/messages.rs` | **not yet written** | The struct does not exist. Correct — §6.1 l. 5022 forbids writing it before `signed_this_hand` is in it, and it now is |

### The one normative site

**`PROTOCOL.md` §6.1's table, read top to bottom, left to right within a cell.**
Twenty-nine fields:

```
 1 protocol_version        11 big_blind               21 current_bet
 2 table_id                12 ante                    22 last_full_raise
 3 hand_id                 13 street                  23 player_to_act
 4 checkpoint              14 board                   24 pots
 5 roster                  15 committed_this_round    25 deck_commitment
 6 button_position         16 committed_this_hand     26 ledger_in
 7 sb_position             17 folded                  27 ledger_out
 8 bb_seat                 18 all_in                  28 transcript_head
 9 level                   19 acted_this_round        29 signed_this_hand
10 small_blind             20 sitting_out
```

**They agree, because there is only one of them.** The four flag vectors sit at
17–20, between `committed_this_hand` and `current_bet`; `signed_this_hand` is
**appended last** under §2.2 rule 5, so the order stays append-only and a future
`removed` vector (`D-014-3`) appends at 30 without moving anything. `absent` is
deleted and may not be re-added.

**The hazard `P7` named is closed at its root rather than at its symptom.** It
was never that the two lists disagreed — it was that a `#[cbor(array)]` struct
had two lists at all, so any future edit to one was a silent `state_hash` fork.
One list cannot drift from itself.

---

# Part 5 — Both K1 interleaving variants and the replay check

Re-run against the current text at every line, not carried. Three of the last
four passes reopened something by fixing something else; the six items this pass
graded all touch §13, §7.2, §9.1 and `validation.rs`, and **none of them touches
§5.3, §4.0, §4.9 or T62/T63**, which is the first thing to check and is why this
part is short.

### 5.1 The load-bearing sites, re-read

| Site | Current text | Moved this pass? |
|---|---|---|
| §5.3 step 4, l. 2874 | `dealt_in[s] := (status == Active ∧ s ∈ signed_this_hand)`; *"**`readmit` is not read into this**"* | **no** |
| §5.3 step 8, l. 3018 | `if \|signed_this_hand\| == 1 then solitary_since := solitary_since.or(Some(hand_id))` … *"It reads `signed_this_hand` and never `signed_this_hand ∪ readmit`"* | **no** |
| §5.3 step 8, l. 2999 | `signed_this_hand := ∅` and `readmit := ∅` | **no** |
| T67, l. 2455 | `readmit ∪= {e.seat}` **and nothing else**, guard `hand_id > 0 ∧ occupied ∧ status ∉ {Removed, Empty}` | **no** |
| T62, l. 2450 | guard `solitary_at(e.hand_id)`, `j <= k + 1` (§2.6 l. 672) | **no** |
| T63, l. 2451 | same guard inside `TableClosed`; `settlement.tournament_winner := None` | **no** |
| §4.0 row 10b, l. 1844 | *"freeze, compare, readmit, or drop"* | **no** |
| §4.9's close box, l. 3348 | **edited by `G6-R5`** — enumeration replaced by a pointer to row 10b | **yes, and it is the only one** |

**The one edit is a strict weakening of a restatement**, not of a rule: §4.9 now
claims less than it did and points at row 10b for the rest. Row 10b is unchanged.
The property both walks depend on — *whichever disposition applies, the copy is
never applied and never enters a `stage_hash`* — is now stated in §4.9
explicitly, where before it was implied by an enumeration that was missing a case.

### 5.2 Variant 1 — `B`'s `HAND_INIT(k+1)` copy arrives

Prefix as `PHASE4_GATE.md` §3: three seats, `C`'s `PLAYER_LEAVE` reaches `A` and
not `B`, hand `k` stalls and aborts through T57 → T46, `P(k) = {A}` at `A`.

At hand `k`'s init `A` reads `|signed_this_hand| == 3` and step 8 writes nothing.
T47 moves `checkpoints.live` to `boundary`. T46 opens hand `k`'s checkpoint 8 in
`live` with `required = signed_this_hand` and **`own = <the state_hash of the body
just published>`**. `A`'s checkpoint-8 window self-completes; step 8 writes
**`solitary_since := Some(k+1)`** by the second disjunct; `RetainedHand(k)` is
written with `was_solitary = true`.

`B`'s checkpoint-8 `STATE_HASH` for hand `k` arrives: step 10a skipped (`N2`),
step 10b's checkpoint-8 branch, compared against `checkpoint8_state_hash(k)` →
mismatch → §6.3 step 1 → step 12a with the record saying solitary →
`SolitaryDivergence { hand_id: k }`.

**T62's guard: `solitary_at(k)` = `solitary_since == Some(k+1)` ∧ `k+1 <= k+1` ∧
`k <= hand_id` → true.** T62 fires: `Diverged`, `Fault{SolitaryDivergence}`,
`solitary_contradicted := true`, the offending event is not applied and not
counted into `signed_this_hand`, the hand deadline is not disarmed.

The freeze holds by I33(b) and I33(c): the only permitted publications are the
`DISPUTE`, the reconciliation `STATE_HASH` and the matching `STATE_ACK`; the
reconciliation stage is required of `R(c) ∪ W = {A,B}`, so `|R| = 2`; and
**T53's two-signer conjunct refuses a one-signer completion independently**.
`B`'s `HAND_INIT(k+1)` then changes nothing — `A` is in `Diverged`, T62 does not
re-enter, T49/T50 are excluded from the phase.

**Verdict, variant 1: fires on the checkpoint-8 `STATE_HASH` and holds. CLOSED.**

### 5.3 Variant 2 — only the checkpoint-8 `STATE_HASH` ever contradicts

Every step above is identical, because **the event that fires the freeze in
variant 1 is the checkpoint-8 `STATE_HASH` itself**. Dropping the `HAND_INIT`
copy removes nothing. `j = k + 1 <= k + 1`; `R(c) ∪ W = {A,B}`; §9.3 condition
0.6 is evaluated before conditions 1–4 while the latch is set, so `A` never
reaches a solitary tournament win; and if the copy arrives after `A` closed,
**T63** has the same guard and sets `settlement.tournament_winner := None`.

**Verdict, variant 2: fires and holds; the win is retracted after closure.
CLOSED.**

### 5.4 The replay check

Ten hands. Once per hand, an observer replays (a) an agreeing checkpoint-8
`STATE_HASH` of finished hand `m` from a seat `X` outside the receiver's retained
`P(m−1)`, and (b) a stale `0x0804 PLAYER_SIT_IN` from `X` in chain `m`'s boundary
window. No key is used; both are messages `X` genuinely signed and §1.5 permits
any peer to forward.

| Question | Route (a) | Route (b) |
|---|---|---|
| Wire | `A ∪= {X}` | `A ∪= {X}` |
| Engine | `Readmitted{X}` → T67: `readmit ∪= {X}` and nothing else | identical |
| Step 4 reads `readmit`? | **no** | **no** |
| Step 8 reads `readmit`? | **no** | **no** |
| `dealt_in` diverges? | **no, at all ten hands** | **no** |
| Stage 0 stalls? | **no** — `R(HAND_INIT, m+1) = P(m)`, bodies byte-identical | **no** |
| `solitary_since` written when `K1` needs it? | **yes** — `\|signed_this_hand\| == 1` is unaffected by `readmit` | **yes** |

**And the input that reversed both walks two passes ago, re-run:** feed `readmit`
at hand `k+1`'s init. Step 8 reads `|signed_this_hand| == 1`, which is still one
at `A`, so `solitary_since := Some(k+1)` is written **regardless of `readmit`'s
contents**; `solitary_at(k)` is true; T62 and T63 both fire. **The reversal is
still gone.**

**`grep -n "admitted"` over all nine documents returns no live reader** — every
occurrence in `STATE_MACHINE.md` is past-tense narration of the deleted union
(l. 640, 5014, 5736, 5741, 5744, 5747), and every occurrence elsewhere is
unrelated. **The union is still gone from the corpus.**

> **All three closed items are still closed. Nothing this pass fixed reached
> them, and the one edit that touched their neighbourhood (`G6-R5`) weakened a
> restatement rather than a rule.**

---

# Part 6 — The healthy-table check

Twenty hands, `n` seats, nothing goes wrong, every peer answers in one round
trip. `6n + 20` round trips per hand [§8.2, and `4n − 3` betting + `2n + 23`
crypto reproduces it]. 50 ms RTT; 42 ms of Bayer–Groth proving per shuffler
[`MENTAL_POKER.md` §5.1], sequential.

| Seats | Trips | Protocol | Proving | **Per hand** | **20 hands** | `FLOOR(n)` | `MIN(n)` | Deadline in force | Fires? |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 2 | 32 | 1.600 s | 0.084 s | **1.684 s** | **33.7 s** | 1 017 000 | 1 042 000 | 1 200 000 (§13 heads-up, `CUSTOM`) | **no** |
| 4 | 44 | 2.200 s | 0.168 s | **2.368 s** | **47.4 s** | 1 337 000 | 1 412 000 | 1 412 000 (minimum admissible) | **no** |
| 6 | 56 | 2.800 s | 0.252 s | **3.052 s** | **61.0 s** | 1 657 000 | 1 782 000 | 1 782 000 (minimum admissible) | **no** |
| 8 | 68 | 3.400 s | 0.336 s | **3.736 s** | **74.7 s** | 1 977 000 | 2 152 000 | 2 152 000 (minimum admissible) | **no** |
| 10 | 80 | 4.000 s | 0.420 s | **4.420 s** | **88.4 s** | 2 297 000 | 2 522 000 | 3 300 000 (§13 rated) | **no** |

No preset is shipped at 4, 6 or 8 seats, so the deadline in force is the smallest
a conforming founder may advertise — the worst legal case, which is the right one
to test.

**Margins per deadline kind:**

| Deadline | Value | Worst healthy consumption | Margin |
|---|---:|---:|---:|
| `crypto_step_timeout_ms`, per stage | 30 000 ms | ≈ 92 ms (42 ms proof + 50 ms RTT) | **326×** |
| boundary (`hand_delay_ms + crypto_step_timeout_ms`) | 37 000 ms | ≈ 150 ms | **246×** |
| `action_timeout_ms + action_grace_ms` | 25 000 ms | human | n/a |
| `hand_deadline_ms`, whole hand, `n = 10` | 3 300 000 ms | 4.420 s protocol + human | **746×** on protocol |
| `hand_deadline_ms`, whole hand, `n = 2` | 1 200 000 ms | 1.684 s protocol + human | **713×** on protocol |
| `join_deadline_ms` | 120 000 ms | — | T4 fires long first |

**No deadline of any kind fires at any seat count.** Every hand places a
checkpoint: §6.2 row 8 is unconditional, I32(a) asserts coverage on both terminal
paths, and `own` is written at every opening (T45, T46, §3.4), so the record a
later hand compares against exists and is readable.

**The measured half.** `cargo test --tests` runs the random-hand harness at 2, 6
and 10 seats plus a tiny-stack all-in walk at 2, 3 and 5 — **4 passed** — and
`cargo test --lib` is **139 passed, 0 failed**. Chip conservation holds at every
seat count tested.

> **What this check cannot see, and it is the reason `G7-S1` is a blocker rather
> than a stale number.** A healthy table at ten seats spends **4.42 s** of
> protocol time per hand. `G7-S1`'s 600 000 ms is 136 000× that, so **this walk
> passes at 600 000 ms too.** What 600 000 cannot pay for is the *action* term —
> `40 × 25 000 = 1 000 000 ms` of human thinking time at ten seats — which no
> timing harness generates and which §8.2 states in terms. **The defect is
> invisible to every check in this part, and it was invisible to the sweep that
> was supposed to find it.** It is visible only to the comparison Part 3 runs.

---

# Part 7 — New-defect hunt

| ID | Severity | Defect |
|---|---|---|
| **`G7-S1`** | **BLOCKER for the constants module, for `TableConfig` and for `table_params_hash`** | **`600` survived the `G4-P3-e` sweep, in a preset listing.** `STATE_MACHINE.md` §9.1 l. 4176 carries `hand_deadline_sec = 600` inside a full `RATED_SNG_POKERTH_V1` block, and l. 4457 calls that block *"fully specified"*. 600 000 ms is below `HAND_DEADLINE_FLOOR(10) = 2 297 000` **and** below the action term alone (1 000 000 ms), so a client built from it aborts every hand under §8.4 with `cause = 1` and `attributed = []` — **`P3`'s exact payoff, reopened**. It is also `G6-R1` a second time: `hand_deadline_ms` is a part of `table_params_hash` (§3.1) and §7.2 rule 3 makes the preset name imply exact values, so a §9.1 client and a §13 client cannot join each other's table. `STATE_MACHINE.md` contradicts itself four sites to one (l. 2167, 3726, 4129, 4724 all say 3 300 000). **Why the sweep missed it:** `G4-P3-e` specified its sweep by section list — *"§2.6, §5.2 T61, §8.6, §9.3, §9.4, §12.1"* — and §9.1 is not on it. A sweep scoped by section list misses the section nobody thought of. **Fix: delete §9.1's block and cite `PROTOCOL.md` §13.** Not *correct the digit* — correcting it leaves the second listing, which is the hazard. Owner: `STATE_MACHINE.md` §9.1. |
| **`G7-S2`** | **BLOCKER for `TableConfig` and for the engine's own validator** | **`crypto_step_timeout_ms` does not exist in `STATE_MACHINE.md`.** `grep -c "crypto_step_timeout"` over the file returns **0**. Yet: §2.3's `TableConfig` claims *"Every field comes from the signed table advertisement"* and `n(16) crypto_step_timeout_ms` is such a field and is a part of `table_params_hash`; `Deadline.duration_ms` (l. 385) is documented *"from config"* and `DeadlineKind::Crypto` is armed by fifteen transitions (T2, T7, T19, …); and §9.4 l. 4373 enforces `hand_deadline_ms >= HAND_DEADLINE_FLOOR(seats)`, whose largest term is `(2n + 23) · crypto_step_timeout_ms` — **1 290 000 of the 2 297 000 at ten seats**. So the engine document arms a timer whose duration has no source, and enforces a bound it cannot compute. The value appears once in the whole file, at l. 3726, as the bare phrase *"the crypto step's 30 000 ms"* — a literal, unnamed, in prose. §9.1's *"fully specified"* preset block omits the field too. **Fix: add `pub crypto_step_timeout_ms: u32` to §2.3's `TableConfig`** — and, since three other hash parts are also missing (`min_buyin`, `max_buyin`, `deck_suite`) plus `password_required`, state whether `TableConfig` is meant to be the whole advert or a projection of it, and say which. Owner: `STATE_MACHINE.md` §2.3. |
| **`G7-S3`** | **medium–high; blocker for any `table_params_hash` producer** | **Three parts of `table_params_hash` have no pinned value for `RATED_SNG_POKERTH_V1` anywhere in the corpus.** §3.1's box takes twenty-five parts; §7.2 rule 3 is normative that the name *"implies §13's exact values"*; and §13's rated block states no value for **`n(1) mode`**, **`n(7) min_buyin`** or **`n(8) max_buyin`**. §7.2 gives only ranges (`min_buyin >= big_blind`, `max_buyin >= min_buyin`) and `n(1)` admits two values. **Two conforming clients, both correctly implementing the preset from its owner, compute different `table_params_hash` and cannot join each other** — `G6-R1`'s payoff with no digit disagreement to notice, and neither client is wrong. `game` and `deck_suite` are safe: version 1 admits one value of each. **The evidence this is an oversight is in the same section:** the heads-up reference configuration, written in the pass that closed `G6-R6`, pins `mode = 2` and `start_stack = 10 000 (= min_buyin = max_buyin)` explicitly. **Fix: three lines in §13's rated block.** Owner: `PROTOCOL.md` §13. |
| **`G7-S4`** | medium | **`STATE_MACHINE.md` §9.4 claims its validator and the joiner's are the same predicate, and since `G5-Q6` they are not.** l. 4373 compares against `HAND_DEADLINE_FLOOR(seats)`; §7.2 rule 2a compares against `HAND_DEADLINE_MIN(n(11))`, which is one `REOPENING_COST` higher. §9.4's own prose then asserts *"The engine's check and the joiner's advert check (§9.4 rule 2a) are now the same predicate over the same fields, which is what makes a table this engine agrees to start one a conforming joiner agrees to enter."* **Both halves of that sentence are false.** The predicates differ, so between `FLOOR(n)` and `MIN(n)` this engine starts a table every conforming joiner refuses — a founder whose own client plays a table nobody may enter. And §9.4 is *Collection bounds* and contains no numbered rule; the citation should be §7.2 rule 2a (`G6-R7`). `DECISIONS.md` `G5-Q6` files the predicate half as owed; **the false sameness claim is not filed**, and it is the half that would stop an implementer noticing. **Fix: the right-hand side becomes `HAND_DEADLINE_MIN(seats)` and the citation becomes §7.2 rule 2a.** Owner: `STATE_MACHINE.md` §9.4. |
| **`G7-S5`** | low–medium | **`RATED_SNG_POKERTH_V1` is claimed to be *"fully specified"* by two documents, in two places, with different contents.** `PROTOCOL.md` l. 7087: *"`RATED_SNG_POKERTH_V1` is fully specified and is not playable by the MVP"*, of §13. `STATE_MACHINE.md` l. 4457: *"`RATED_SNG_POKERTH_V1` is fully specified in §9.1"*. §9.1 has twenty-one values, §13 has eighteen, they overlap in seventeen, they disagree on one (`G7-S1`) and §9.1 omits the field §9.4 needs (`G7-S2`). This is the D-011 rule 1 failure in its plainest form — **two owners for one concept** — and it is the root cause of both blockers, so fixing it fixes them. **Fix: `STATE_MACHINE.md` §9.1 becomes a citation.** Owner: `STATE_MACHINE.md` §9.1. |
| **`G7-S6`** | low, **carried** | **`n = 8` is absent from every deadline table in the corpus and from the test that reproduces them.** §8.2's floor table gives 2, 4, 6, 10; §8.2's admitted-minimum table gives 2, 4, 6, 10; `the_deadline_floor_matches_the_published_derivation` tests 2, 4, 6, 10. **`FLOOR(8) = 1 977 000` and `MIN(8) = 2 152 000` appear nowhere.** Eight seats is a legal `max_players` and is one of the five counts this gate series is commissioned to check. Named by `PHASE4_GATE.md` and unmoved. **Fix: one row in each table and one tuple in the test.** Owner: `PROTOCOL.md` §8.2 and `src/poker/tournament.rs`. |
| **`G7-S7`** | low | **The hand deadline is described as *"ten minutes"* at eight sites, and it has not been ten minutes since `P3`.** At §13's rated value it is **55 minutes**; at the heads-up reference configuration, **20**. `THREAT_MODEL.md` l. 1819, 1830, 1851, 1852, 1905, 2255 — where the figure is load-bearing for X8's *visibility and cost* argument, which the correct number makes **stronger**, so the error is conservative. `PROTOCOL.md` l. 6185 and **l. 7262, which is inside §13, the owner**. `G4-P3-e` names `STATE_MACHINE.md` for the stale figure and `THREAT_MODEL.md` only for a missing attacker row, so **`PROTOCOL.md`'s two are unfiled**. **Fix: replace the phrase with `hand_deadline_ms` and let the number live in one place.** Owner: `PROTOCOL.md` §8.3/§13; `THREAT_MODEL.md` X8/X30. |
| **`G7-S8`** | low | **`AgreedCheckpoint.sequence` is now read by nothing, and §4.10 still states the precondition in terms of it.** After `Q2`, `covering` does not compare `sequence`, `Tier2Finding::new` compares `number`, and T64's guard is `c.number >= n`. But `PROTOCOL.md` §4.10's `cause = 6` row (l. 3811) still says the receiver must hold a completed `STATE_ACK` stage *"for a checkpoint of the same chain **at or before the offending event's `sequence`**"* — the clause the field exists for and which no code path and no transition checks. Either the wire rule moves to the checkpoint number, matching T64 and `validation.rs`, or the field earns its keep with a comparison. Two statements of one precondition in two units is D-011 rule 1's shape. **Fix: one clause in §4.10, or one comparison in `covering`.** Owner: `PROTOCOL.md` §4.10. |
| **`G7-S9`** | low, **and it is `G6-R6`'s own owed half** | **The two lines `G6-R6` owes the code are not written.** `DECISIONS.md` `G6-R6` is explicit: *"its `id` may not be `\"HEADS_UP_PLAY_MONEY_V1\"` on the wire — it advertises `\"CUSTOM\"` — and its doc comment 'named so that two clients can agree on it without negotiating' … must go."* `src/poker/tournament.rs` l. 99 still reads `id: "HEADS_UP_PLAY_MONEY_V1"`, and the doc comment is verbatim at l. 93. `Preset::id` is the field that would populate `n(2) preset_id`, and §7.2 rule 3 now rejects any third value **on sight**, so this client's own advert would be refused by every conforming joiner **including itself**. Latent only because no advert builder exists — **which is exactly why it must land before one does**, and it is one of the two items this build gate is about to unblock. Also owed: the test `G6-R6` asks for, asserting every emitted advert's `preset_id` is inside the two-value enum. **Fix: two lines and one test.** Owner: `src/poker/tournament.rs`. |

## The pattern check, run on this pass's own findings

**Six of the nine are one pattern: a quantity listed in more than one place, or
in no place.** `G7-S1` (two places, disagreeing), `G7-S3` (no place), `G7-S5`
(two owners), `G7-S6` (four tables, all missing a row), `G7-S7` (eight places,
all stale), `G7-S8` (two units for one precondition). `G7-S2` is the sharpest
form — **a quantity a formula reads and a struct does not have** — and it is the
first instance in the series of a *field* going missing rather than a *value*
drifting.

**The direction is new and it is worth naming for the next pass.** Every previous
gate compared **values**: document against document, then document against code.
This pass compared **fields** — grepping for `crypto_step_timeout` as an
identifier rather than reading §2.3's list for plausibility — and that single
change found `G7-S2` and `G7-S3`, the two defects nobody could have found by
reading. `G7-S3` in particular is invisible to every value comparison in the
corpus, because there is no second value to compare against.

> **The rule this pass adds, and it is the fourth in the series.** The previous
> three were *re-derive every reader of a quantity you narrow*, *re-list the
> dispositions after the additions land*, and *when a fix lands in a document and
> the code, re-read the first half's number rather than its argument.* This one
> is: **a sweep is specified by pattern, never by section list.** `G4-P3-e`
> enumerated six sections and swept them correctly; the survivor was in the
> seventh. A `grep` for the digits would have taken one command and found it, and
> the gate that graded the sweep accepted the section list as the scope because
> the sweep itself proposed it — which is `G6-R2`'s failure mode with a list in
> place of a filing claim.

---

# Part 8 — The build list, item by item

## `src/protocol/constants.rs` — **BUILD**

**Unblocked. `G6-R1` is closed, §13 and the code agree at 3 300 000, and the
`600_000` that blocked it under `G4-P3-e` is out of §2.3.**

Source **`PROTOCOL.md` §13 and §8.2, and nothing else.** Never
`STATE_MACHINE.md` §9.1 (`G7-S1`) and never §2.3's comments.

Write as `const fn` over the five config fields, all three, because writing one
without the others is how `G5-Q6` had to be retro-fitted:

```
HAND_DEADLINE_FLOOR(n) = hand_delay_ms
                       + (2n + 23) * crypto_step_timeout_ms
                       + 4n        * (action_timeout_ms + action_grace_ms)
REOPENING_COST(n)      = (n − 1) * (action_timeout_ms + action_grace_ms)
HAND_DEADLINE_MIN(n)   = HAND_DEADLINE_FLOOR(n) + REOPENING_COST(n)
```

`n` is `max_players` — `n(11)` — **never the live seated count**. The three
already exist correctly in `src/poker/tournament.rs`; the constants module is
where they belong and `tournament.rs` should call them rather than own them.

**`hand_deadline_ms` may now be written as a literal, and only in one place:**
`3_300_000` for `RATED_SNG_POKERTH_V1`, pinned by equality against §13, which
`the_rated_preset_matches_the_normative_value_exactly` already does. The
heads-up figure `1_200_000` is a **reference configuration and not a preset**
(`G6-R6`) — write it as a fixture, not as a constant with a `preset_id`.

**Never write `600_000` as a hand deadline.** The one legitimate bare `600_000`
in the tree is **`REANNOUNCE_INTERVAL_MS = 600 000`** (§13 l. 7009), a lobby
re-announce interval that shares the digits and must be written; and the one in
`tournament.rs` l. 342 is a regression assertion that 600 000 is *below* the
floor. Both are correct. Do not let the rule delete either.

**Add the eight-seat row** while the file is open (`G7-S6`).

Gap: **none for this module.** `G7-S1` is a gap in `STATE_MACHINE.md` §9.1, and
the constants module must simply not read it.

## `PublicTableState` — **BUILD**

Transcribe from **`PROTOCOL.md` §6.1's table only**, twenty-nine fields, top to
bottom, in the order enumerated in Part 4. `#[cbor(array)]`, so the order **is**
the encoding. `signed_this_hand` is in it and is **appended last** (§2.2 rule 5).
`absent` is deleted and may not be re-added. Expect one append-only field at
position 30 if `D-014-3` lands a `removed` vector. **No gap — `P7` is closed.**

## `state_hash` — **BUILD**

`h("p2p-poker v1 state", [canonical_cbor(PublicTableState)])`, one part, domain
string in §2.8's closed register. `h` and the register are written and green
(`src/protocol/serialization.rs`, `src/protocol/signatures.rs`). **No gap.**

## the three anti-replay stores — **BUILD, all three**

`event_class == 0` keyed by `slot(E)`, dropped at hand end, **no store created
for a finished hand** (`N2` — what makes §4.0 row 10a's skip implementable rather
than a coin flip). Classes 1 and 2 sparse, per stage, keyed `(seat, subject_seat)`
and `(seat, subject_digest)`. The `DISPUTE` counter at
`MAX_DISPUTES_PER_SENDER_PER_HAND = 8` per sender per hand. **No gap.**

## the checkpoint-8 `STATE_ACK` slice — **BUILD**

`8 × MAX_SEATS = 80` entries, one hand at a time, at
`BOUNDARY_CHECKPOINT_BASE + 2r + 1`, `0 <= r <= 7`, retained to `TERMINAL(k+1)`.
The band is `8 192 .. 8 207` and sits **above** `BOUNDARY_SEQUENCE_BASE`'s
`4 096 .. 4 105` because a reconciliation round extends upwards. Both bands are
exempt from the `MAX_STAGES_PER_HAND` abort — §13 states it and names the
failure: *"An implementer who range-checks `sequence < MAX_STAGES_PER_HAND`
rejects every boundary event and every boundary checkpoint."* **No gap.**

## `RetainedHand` — **BUILD**

Four fields — `hand_id`, `was_solitary`, `p`, `checkpoint8_state_hash`;
`p = P(hand_id − 1)`; **`was_solitary` is the disjunction
`P(k−1) == {self} ∨ P(k) == {self}` and an independent bool, never `p == {self}`
and never `|p| == 1`** (`N4-a`). LRU at `MAX_RETAINED_HAND_RECORDS = 4 096`,
≈ 56 B each, under 256 KB. **No gap.**

## `CheckpointStore` and `CheckpointState` — **BUILD, shape and record both**

Three named `Option` slots — `live`, `agreed`, `boundary` — no `Vec`, no map,
nothing keyed on a sender-chosen quantity. `named(h, n)` and
`agreed_checkpoint(h)` as the **only** routes in. Lifetimes at T45/T46
(`boundary := None`, then `live := Some(new)` with `own` written), T47/T61
(`boundary := live.take()`, `agreed := None`), and the T10 special case.

`CheckpointState`: `hand_id`, `number`, `sequence`, `required`, `heard`,
**`own: Hash`**, **`dissent: Option<Hash>`**, `acked`, `agreed: Option<Hash>`.
**`values: u8` is deleted and must not be re-added.** Write `own` **once**, at
the opening, and never re-derive it from current state — I32(d) names that as the
implementation that passes every single-hand trace and fails every boundary one.
**No gap.**

## T49 / T50 / T51 — **BUILD**

`e.state_hash == c.own`; `e.state_hash != c.own` with
`c.dissent := c.dissent.or(Some(e.state_hash))`; `c.heard ⊇ c.required ∧
c.dissent.is_none()`. T49 and T50 are exhaustive and disjoint on `round == 0`.
T50 also sets `solitary_contradicted` when `|c.required| == 1` (`N1`).
**No gap.**

## `PrefixOutcome` / `SolitaryDivergence` with T62 and T63 — **BUILD**

`SolitaryDivergence { hand_id, seat, event_hash }`. T62's guard is
`solitary_at(e.hand_id)` with **`j <= k + 1`**, in the past tense, on the event's
`hand_id` and never the receiver's current phase. T63 is the same guard inside
`TableClosed`, with `settlement.tournament_winner := None` — the one event class
for which that phase is not absorbing. Freeze semantics from I33(b): while frozen
the **only** permitted publications are the `DISPUTE`, the reconciliation
`STATE_HASH` and the matching `STATE_ACK`. Both variants confirmed in Part 5.
**No gap.**

## §4.0 steps 12 and 12a — **BUILD**

Step 12's solitary-stage conjunct; step 12a's two arrival exemptions
(`PLAYER_SIT_IN`, and the checkpoint-8 `STATE_HASH` which is compared instead)
and the second's *"from arrival and not from disagreement"* clause. Step 12's
*"an out-of-set checkpoint-8 `STATE_HASH` is not an out-of-stage event"* clause
must be implemented or `P2`'s accepted-set widening is unreachable. §4.9's box is
now a pointer to row 10b rather than a second enumeration (`G6-R5`), so **row 10b
is the only thing to transcribe.** **No gap.**

## the readmission set — **BUILD, both halves**

Wire: one `SeatSet`, `|A| <= MAX_SEATS`, written by §4.0 step 10b, read once at
the next hand init, cleared there, **widening the *accepted* emitter set only**.
Engine: wire T67 with its full guard — `hand_id > 0 ∧ occupied ∧
status ∉ {Removed, Empty}` — because that filter is the whole of D-014's one-way
exit on this route (I34(a)) and the wire has no `status` to supply it.
**`readmit` must reach no guard**: read once at §5.3 step 4, handed to the
protocol layer, cleared at step 8, with I31(a)'s one-writer assertion. Directed
test: I33(a)'s. **No gap.**

## `TableConfig` — **DO NOT BUILD from `STATE_MACHINE.md` §2.3**

**`G7-S2`.** The struct has no `crypto_step_timeout_ms`, which
`Deadline.duration_ms` needs for `DeadlineKind::Crypto` and which §9.4's own
validator needs to compute `HAND_DEADLINE_FLOOR(seats)`. It also lacks
`min_buyin`, `max_buyin`, `deck_suite` and `password_required`, all of which §2.3
claims for it (*"Every field comes from the signed table advertisement"*) and
three of which are parts of `table_params_hash`. **Named section:
`STATE_MACHINE.md` §2.3 l. 132–158.** Build it from §7.2's advert field list
instead, or wait one edit.

## `table_params_hash` and the advert builder — **DO NOT BUILD**

**`G7-S3`.** §3.1's box is complete and correct and can be transcribed as a
function; what cannot be built is a **`RATED_SNG_POKERTH_V1` advert**, because
three of the twenty-five parts — `n(1) mode`, `n(7) min_buyin`, `n(8) max_buyin`
— have no pinned value in §13 and only ranges in §7.2. Two conforming clients
would pick differently and be mutually unjoinable, which is the whole failure
`table_params_hash` exists to prevent. **Named section: `PROTOCOL.md` §13's
`RATED_SNG_POKERTH_V1` block.** The heads-up reference configuration **can** be
built — it pins all three.

## What is still deferred, and on what

| Item | Deferred on |
|---|---|
| §4.0 steps 13, 14, 15 | the engine — semantic re-validation and proof verification |
| `TIMEOUT_VOTE`, `TIMEOUT_CERT`, `EquivocationProof` behaviour | **`OQ-F`** — write the payload structs, wire no behaviour |
| `SHOWDOWN_MUCK` policy | **`Q-01`** |
| T64 / T65 producers | **`D-014-3`** — a removal is canonical per-seat state that no hashed vector carries |
| the reconciliation round's out-of-set admission | **`N1-a`**, one sentence, and it must be chosen **before** T53 is written |
| `SeatStatus::Absent` and its readers | **`J-3`**, closed as *unreachable, deliberately*; the reader list over-warns by two |
| the reopening residual above the founder's headroom | **`G4-P3-r`** — unmoved by `G5-Q6`, which raised the bound from zero to one and did not change the shape |
| the `§9.4 rule N` citations outside `PROTOCOL.md` | **`G6-R7`** |
| the citation sweep for the renamed series | **`G5-Q5-x`** |
| the `THREAT_MODEL.md` below-floor-founder row | **`G4-P3-e`**, third part |
| §9.1's preset block | **`G7-S1`, `G7-S5`** |
| §2.3's `TableConfig` | **`G7-S2`** |
| §13's three unpinned hash parts | **`G7-S3`** |
| §9.4's validator right-hand side and its sameness claim | **`Q6` / `G5-Q6`, `G7-S4`** |
| `HEADS_UP_PLAY_MONEY_V1`'s wire name | **`G7-S9`** |

**Everything on the freeze path and everything on the state path can be written
today, the constants module included.** The two items that must wait are
`TableConfig` and the advert builder, and neither is on the freeze path. The
three blocking defects are all in sections no gate in this series had a reason to
open before, and all three are one edit each.
