# PHASE4_GATE3.md — the config gate

**Commission.** Fifteenth pass. The fourteenth (`PHASE4_GATE2.md`) cleared the
whole build list except two items and held them back on three defects it had
just found: `TableConfig` on `G7-S2`, the `table_params_hash` producer on
`G7-S3`, and both — with the constants module — on `G7-S1`. This pass decides
those two items, and only those two: everything else on the list is being
written now.

**Method, and the one rule that decided this pass.** `PHASE4_GATE2.md` closed
with a rule it had paid for: **a sweep is specified by pattern, never by section
list.** Both of this gate's two commissioned sweeps were run that way — a `grep`
over all nine documents and all of `src/`, with the section list used afterwards
only to *explain* the hits, never to *find* them. That is why Part 2 is a table
of every hit rather than of every section, and it is how three of this pass's
eight new defects were found: `G8-C1` is a survivor of a fix that was applied
where it was pointed, `G8-C4` is a duplicate the previous pass's own fix
introduced, and `G8-C5` is a count that is wrong in the direction that invites
the defect the sentence exists to prevent.

Nothing is graded off a summary row or off `DECISIONS.md`'s account of what
landed. Every "DONE" cell was checked against the file it points at — `G6-R2-m`
— and this pass had the sharpest tool the series has had for it: **the three
edited documents are uncommitted, so `git diff` gives the exact set of lines this
pass changed.** Part 5 is short and certain because of it: the diff does not
touch §5.3 steps 4 and 8, T62, T63, T67, §4.0 rows 10a/10b/12/12a or §4.9's close
box, at any line.

**Files as read, 2026-08-28.** `STATE_MACHINE.md` **22:24 — edited, +448/−191**,
`PROTOCOL.md` **22:20 — edited, +163 lines of hunks**, `DECISIONS.md` **22:13 —
edited, four rows added and one amended**, `src/protocol/constants.rs` **22:05 —
new**, `tests/random_hands.rs` 22:06, `src/poker/tournament.rs` **22:03 —
edited**, `src/protocol/mod.rs` 22:02, `src/protocol/slot.rs` 21:28,
`src/security/validation.rs` 21:23, `NETWORK_STACK.md` 21:00, `CRYPTOGRAPHY.md`
20:59, `THREAT_MODEL.md` **20:53 — unchanged, third consecutive pass**,
`DEPENDENCIES.md` 19:33 — **unchanged, fourth consecutive pass**,
`CONTRIBUTING.md` 19:32 — **unchanged, fourth consecutive pass**, `SPEC_CS.md`
09:29.

`cargo check --lib -j 19` clean; `cargo test --lib -j 19 -- --test-threads=19`
— **147 passed, 0 failed** (139 last pass); `cargo test --tests` — **4 passed**,
the random-hand harness at 2, 6 and 10 seats and the tiny-stack all-in walk.

> **The timestamp finding, eleventh iteration, and the amended rule held.** Last
> pass amended the rule to *the file edited last under a fix is clean; the file
> edited last under a sweep is not.* This pass's last-edited file is
> `STATE_MACHINE.md` at 22:24, and it was edited **under a sweep** — the pass
> swept it for every restated numeric table parameter and deleted them all,
> §9.1's preset block included. By the amended rule it should carry the debt, and
> it does: `G8-C2`, `G8-C4`, `G8-C5` and `G8-C8` are all in it, and `G8-C4` is
> *in the very box the sweep wrote*. The rule's other half also held:
> `THREAT_MODEL.md`, `DEPENDENCIES.md` and `CONTRIBUTING.md` are the three files
> not opened, and `THREAT_MODEL.md` is the named owner of both items that stayed
> PARTIAL for a reason no other document can fix.

---

## Verdict counts

| Verdict | Count | Items |
|---|---:|---|
| **RESOLVED** | 8 | Q6, G6-R6, G7-S1, G7-S2, G7-S3, G7-S4, G7-S5, G7-S6 |
| **PARTIAL** | 4 | G4-P3-e, G7-S7, G7-S8, G7-S9 |
| **UNRESOLVED** | 0 | — |
| **REGRESSED** | 0 | — |

**Both blockers are closed, and both closed by deletion or by the addition of a
field rather than by correcting a digit** — which is the form each was asked
for. `G7-S1`'s `hand_deadline_sec = 600` is gone from the corpus: §9.1's
twenty-one-value listing is deleted and replaced by a citation, and `grep` over
all nine documents finds no `600 000` that is a hand deadline. `G7-S2`'s
`crypto_step_timeout_ms` is in §2.3's `TableConfig` and every `DeadlineKind`
arming site now has a named source.

**Eight new defects, `G8-C1`–`G8-C8`. None blocks.** Four are in
`STATE_MACHINE.md`, two in `PROTOCOL.md`, two in `src/poker/tournament.rs`. Two
of them — `G8-C1` and `G8-C4` — are the same shape as the pair this pass closed,
and one of those two was *created* by this pass's own fix.

**Readiness verdict: BUILD both held-back items. `TableConfig` from
`STATE_MACHINE.md` §2.3, and the `table_params_hash` producer and advert builder
from `PROTOCOL.md` §3.1 and §13.** The rest of the build list is still clear.
Part 8 gives the per-item answer and the caveats each build carries.

---

# Part 1 — Per-item verdicts

## Q6 / `G5-Q6` — an advertisement at exactly the floor tolerates zero reopening raises — **RESOLVED**

**The owner's half closed last pass; the engine half, which was the whole of what
kept it PARTIAL, landed this pass.** `STATE_MACHINE.md` §9.4 l. 4471 now reads:

```
hand_deadline_ms >= HAND_DEADLINE_MIN(seats)           // §7.2 rule 2a, the same predicate over the
                                                       // same fields; PROTOCOL.md §8.2 owns the
                                                       // formula and no part of it is restated
                                                       // here (D-011 rule 2)
```

The consequence the last gate named — *"between `FLOOR(n)` and `MIN(n)` this
engine **starts** a table that every conforming joiner **refuses**"* — is gone,
and §9.4's prose states the history rather than hiding it: *"The form that
replaced it, `>= HAND_DEADLINE_FLOOR(seats)`, was right about the term and one
`REOPENING_COST` short of the bound the joiner applies … a founder whose own
client plays a table nobody may enter."*

**`DECISIONS.md`'s `G5-Q6` row named exactly this as owed and it is now paid.**
The one residual the row disclaims is `G4-P3-r`, which is unmoved and correctly
so — raising the bound from zero reopenings to one does not change the shape of a
hand that outruns the founder's headroom.

## G4-P3-e — the `600 000` sweep and the §9.4 validator line — **PARTIAL**

**Three of four parts closed; the fourth is untouched for the third consecutive
pass.**

| Part | State |
|---|---|
| the literal at §2.3 | **closed** last pass, and this pass went further: §2.3 now carries **no value at all**, only `n(k)` numbers, and says so — *"Seven of these lines carried the preset's value as a comment until this pass"* |
| the validator line at §9.4 | **closed.** `HAND_DEADLINE_MIN(seats)`, §7.2 rule 2a cited correctly — see `Q6` and `G7-S4` |
| the stale figure *"in roughly a dozen places"* | **closed, and this pass swept the whole file rather than the six sections the row named.** §12.1's residual bounds — the ones *"quoted to users"* — now read `HAND_DEADLINE_MIN(2)` and `hand_delay_ms` in place of `1 017 000`, `2 034 000` and `hand_delay_sec` |
| the `THREAT_MODEL.md` §5.2 attacker row — *a founder advertising a deadline every legal hand exceeds* | **not written.** `THREAT_MODEL.md` is unchanged at **20:53, a third consecutive pass**. `PROTOCOL.md` §11 carries the mitigation and now cites **this row by identifier** rather than saying *"filed in `DECISIONS.md`'s open list"*, which is `G6-R2-m` applied |

**The remaining part is a document nobody has opened in three passes, and that is
the finding.** `PROTOCOL.md` §11's cell is now sharp enough to transcribe — the
capability required is **none**, and there is **no second line of defence**
because once a seat is taken the value is inside `table_params_hash` — so what is
owed is a transcription, not a decision.

## G6-R6 — `HEADS_UP_PLAY_MONEY_V1` is a named preset no document defines — **RESOLVED**

**The document half closed last pass; the two lines owed to the code landed this
pass, and a third thing landed with them that the row did not ask for.**

`src/poker/tournament.rs`:

* the constant is renamed **`HEADS_UP_CUSTOM_2P`**;
* its `id` is **`"CUSTOM"`**;
* the doc comment *"named so that two clients can agree on it without
  negotiating"* — the exact claim the decision refuses — is **gone**, and what
  replaces it states the refusal: *"A third name is rejected on sight, because a
  name that asserts values nothing checks is how two clients ship different
  tables under one identity. That has already happened twice here."*
* and the test `G6-R6` asked for is written:
  `the_heads_up_configuration_advertises_itself_as_custom`, which asserts the
  `id` against `PresetId::Custom.as_str()` **and** round-trips both names through
  `PresetId::parse`.

The third thing is `src/protocol/constants.rs`'s **`PresetId`**, a closed
two-variant enum whose `parse` returns `None` for a third value, and
`preset_ids_round_trip_and_a_third_name_is_refused`, which feeds it
`"HEADS_UP_PLAY_MONEY_V1"` by name and requires the refusal. **The retired name
is now a negative test case**, which is the strongest disposition available for a
name that must never travel again.

`G6-R6` is closed. Its successor scope — the *complete advert* the name's removal
leaves the code owing — is `G7-S9`, below, and it is a larger item than the two
lines this row owed.

## G7-S1 — `600` survived, in a preset listing nobody had swept — **RESOLVED**

**Closed by deletion, as required, and the deletion is the whole section.**
`STATE_MACHINE.md` §9.1 is now a citation:

> **The preset's values are `PROTOCOL.md` §13's, and they are not written down
> here — `G7-S1` and `G7-S5`.** §13 is the single normative home for every
> two-sided constant in the corpus …

and it names what it deleted rather than quietly replacing it: *"What stood here
was a second listing of twenty-one values, and it had drifted. It was written in
seconds where §13 is in milliseconds, it omitted `crypto_step_timeout_ms`
entirely … and it carried `hand_deadline_sec = 600`."*

**The lesson is recorded in the form the next editor needs, and it is this
gate's own observation quoted back:**

> A second copy of a preset value cannot be tested: the healthy-table walk passes
> at `600 000` too, because what that figure cannot pay for is the *human* action
> term and not the protocol's, so no timing check in this corpus would ever have
> named it. A corrected digit buys a copy that drifts again at the next change; a
> citation buys one place to change.

**And the section states the scope of the sweep it ran, which is what closes the
class rather than the instance:** *"The same treatment is applied to every other
numeric restatement of a table parameter in this document, found by grep over the
whole file rather than by a section list — a section list is what let this one
survive the `G4-P3-e` sweep."* Part 2 confirms that claim independently; it is
true.

`hand_deadline_sec` now occurs twice in the corpus, both times inside a sentence
narrating this defect (`STATE_MACHINE.md` l. 3676, l. 4251).

## G7-S2 — the engine's own config struct cannot express the engine's own bound — **RESOLVED**

**Closed in three places, and the third is the one that makes it stay closed.**

1. **The field.** §2.3 l. 158: `pub crypto_step_timeout_ms: u32, // n(16) — every
   DeadlineKind::Crypto duration`.
2. **A rename that came with it.** `action_timeout_grace_ms` → **`action_grace_ms`**,
   *"the name `PROTOCOL.md` §7.2, §8.2 and §13 all use — it was the only site in
   the corpus carrying the other name, and §13 requires that no value have two
   names."* True of the documents; see `G8-C6` for the code.
3. **A source for every arming site**, which is the half the item asked for
   beyond the field. §2.6 gains a box mapping each stage kind to the
   `TableConfig` fields the scheduler is handed, and then enumerates the sites:
   *"§5.2 arms `Action` at four sites — T26, T29, T31, T37 — and `Crypto` at
   eleven — T2, T7, T10, T14, T18, T19, T32, T33, T38, T39 and §5.3's
   `|dealt_in| >= 2` branch."* **I verified that enumeration by grep over the
   §5.2 rows and it is exact**: fourteen transitions arm a deadline, four
   `Action` and ten `Crypto`, plus the one §5.3 branch.

**And the struct now answers the second question the item asked** — *state
whether `TableConfig` is meant to be the whole advert or a projection of it, and
say which*:

> **`TableConfig` is a projection of the advertisement, not the advertisement.**
> … so **`table_params_hash` is computed over the advertisement and never over
> this struct** … A peer that reconstructs the hash from `TableConfig` derives a
> different value from every conforming peer, and since the hash is in
> `GENESIS(0)` and in `PLAYER_LIST` it can then join no table at all.

That paragraph is what unblocks both of this gate's build items at once. Its
arithmetic is off by two — see `G8-C5` — but its conclusion is right and is the
load-bearing part.

## G7-S3 — three parts of `table_params_hash` have no pinned value anywhere — **RESOLVED**

**Closed, and closed for the class rather than for the three.** §13's rated block
is re-indexed to `n(…)` and now carries **all twenty-five parts of
`table_params_hash`, in §3.1's box order**, with `n(1) mode = 2`,
`n(7) min_buyin = 10 000` and `n(8) max_buyin = 10 000` pinned. I diffed the
block against §3.1's box part by part: **25 for 25, same order, no omission and
no extra.** The heads-up reference configuration was re-indexed the same way and
is also 25 for 25.

**The remedy for the class is the indexing, and §13 says so:** *"a named
configuration is audited by diffing its block against §3.1's twenty-five parts,
and a part with no line is visible at a glance. **Every part carries a line**,
including the ones §7.2 admits a single legal value for … because a part left out
to save a line is indistinguishable from a part nobody thought about — which is
the whole of this finding."*

**Two of the three values were made unfree rather than merely written down**,
which is the better fix and is why this is RESOLVED and not merely DONE. §7.2
rule 2 now enforces, at the receiver:

* **`min_buyin == max_buyin == start_stack` in a tournament mode** — *"a
  Sit-and-Go's buy-in **is** its starting stack"* — so the two buy-in fields stop
  being free parts a named configuration has to pin one at a time;
* **`small_blind == blind_schedule.first_small_blind`**, which also closed a
  fourth hole nobody had noticed: `n(4) small_blind` is a distinct hash part from
  `n(13.2) first_small_blind`, so a configuration that pinned only the schedule
  pinned only one of the two.

`mode` is argued to be forced rather than chosen: `n(9) start_stack` is admitted
in tournament modes only, and version 1 has one, so `mode = 2` is the only value
consistent with the rest of the block.

## G7-S4 — §9.4 claims its validator and the joiner's are the same predicate — **RESOLVED**

**Both false halves are fixed, and the sameness claim is now a derivation rather
than an assertion**, which is what the item asked for:

> **The sameness claim is now true, and it is a derivation rather than an
> assertion (`G7-S4`).** … They are the same predicate now because this line does
> not **state** the bound: it names `HAND_DEADLINE_MIN`, which `PROTOCOL.md` §8.2
> defines and §7.2 rule 2a applies … **There is nothing here left to drift from**,
> which is the only form of *the same predicate* that survives a pass nobody
> sweeps.

The citation half is fixed too — *"`PROTOCOL.md` §9.4 is *Collection bounds* and
contains no numbered rule at all — the joiner's rules are §7.2's, and citing §9.4
for them is `G6-R7`"* — and §7.2 now carries a box declaring its own numbering:
*"These numbered rules are §7.2's and are cited as *"§7.2 rule N"*. … this
document's six are corrected, the rest are filed (`G6-R7`)."*

**`G6-R7` therefore narrowed rather than closed**: the six inside `PROTOCOL.md`
are done, the citations outside it are still owed and still filed.

## G7-S5 — two documents each claim to "fully specify" the preset — **RESOLVED**

There is now one. `STATE_MACHINE.md`'s *"`RATED_SNG_POKERTH_V1` is fully
specified in §9.1"* is gone with the block it pointed at, and `PROTOCOL.md`
l. 7087's *"`RATED_SNG_POKERTH_V1` is fully specified and is not playable by the
MVP"* stands alone. §9.1 now lists what the engine document says about the preset
and states that **no number is in it** — five bullets, every one a pointer or a
property.

**This is the root-cause fix the item predicted would close both blockers, and it
did.** `G7-S1` and `G7-S2`'s §9.1 half both fell out of it.

## G7-S6 — `n = 8` is absent from every deadline table — **RESOLVED**

Both `PROTOCOL.md` §8.2 tables carry the row — floor **1 977 000**,
`REOPENING_COST` **175 000**, admitted minimum **2 152 000** — and the code
carries it twice: `the_floor_matches_the_published_derivation` and
`the_admitted_minimum_is_the_floor_plus_one_reopening` in
`src/protocol/constants.rs` both iterate `[2, 4, 6, 8, 10]`, and
`the_deadline_floor_matches_the_published_derivation` in `tournament.rs` gained
`(8, 1_977_000)`. Verified against the recomputed values in Part 2: all correct.

## G7-S7 — the hand deadline is described as *"ten minutes"* — **PARTIAL**

**`PROTOCOL.md`'s three sites are fixed; `THREAT_MODEL.md`'s remain, and they are
filed.** The fixes are worth recording because two of the three were wrong in a
*second* way the item did not name:

* §3.2 said *"38 minutes at ten seats"* — that is `HAND_DEADLINE_FLOOR(10)`, the
  floor, and not the preset's deadline. Now `3 300 000 ms` and **fifty-five
  minutes**;
* §8.3's *"waiting ten minutes for the same outcome"* now names `hand_deadline_ms`
  and **no number at all**, which is the better fix wherever the magnitude is not
  the point;
* §14 note 4's *"ten minutes rather than thirty seconds"* is corrected.

**Owed:** `THREAT_MODEL.md` l. 1248 (X34), 1819, 1830, 1851, 1852, 1905 and 2255
— **seven sites by grep, where `DECISIONS.md`'s row says six.** All price an
escape or a stall in *"ten minutes"*; l. 2255 states it as *"beyond the ten
minutes of `hand_deadline_ms`"*, a claim about a named constant rather than an
aside; X8's and X34's are quoted to users. **None is a correctness claim and all
are wrong, in the conservative direction** — the true figure makes X8's
*visibility and cost* argument stronger, not weaker.

## G7-S8 — `AgreedCheckpoint.sequence` is read by nothing and §4.10 states the precondition in it — **PARTIAL**

**The named site is fixed, in the direction the item preferred and with a better
argument than the item offered. A second site was not fixed, and it is the one
labelled normative.**

`PROTOCOL.md` §4.10 l. 3813 now requires a checkpoint *"whose §6.2 checkpoint
number is at or after the number of the checkpoint the illegality was fixed at"*,
and the box below it makes the case: *"the two units are not equivalent: **at or
before the offending event** is satisfied by checkpoint 1 of the chain, which
fixes almost no state and decides almost no illegality"*, while `n' >= n` is
agreement over a prefix containing checkpoint `n`'s stage because
`transcript_head` is in `PublicTableState` and chains through every earlier
stage. The field keeps the purpose it has — §4.1's `round` derivation — *"and
stops being asked to carry a precondition as well."*

**What keeps this PARTIAL: `PROTOCOL.md` §4.9 l. 3641 still states the same
precondition in the retired unit**, inside a box headed *"Tier 2 is judged
against a checkpoint both peers signed — **normative**"*. Filed as **`G8-C1`**,
and it is the whole of what `G7-S8` has left.

## G7-S9 — the two lines `G6-R6` owes the code — **PARTIAL**

**The two lines landed** (see `G6-R6` above). **What replaced the row is larger
than the row, and `PROTOCOL.md` §13 says so in terms** — *"`G6-R6`'s two lines
have landed; this is larger."* Three obligations, none of them a value, all still
open:

1. **The advert type owns the twenty-five parts, not `Preset`.** `Preset` carries
   **fourteen**; the eleven it does not are `n(0)`, `n(1)`, `n(4)`, `n(5)`,
   `n(7)`, `n(8)`, `n(13.0)`, `n(20)`, `n(21)`, `n(22)` and `n(24)`, and every one
   is hashed. *"That is `G7-S3` in the code rather than in a document, and it is
   the same defect wearing the same disguise: a **missing** field reads as an
   omission, never as a contradiction."*
2. **`n(2)` is typed, not spelt.** `Preset::id` is `&'static str` at
   `tournament.rs` l. 23; `PresetId` exists in `constants.rs` and should be the
   type, *"so the rule the wire enforces is the rule the type enforces."*
3. **Units are part of the hash.** `Preset` stores `*_sec`; every timing part is
   hashed as `u32_be(…_ms)`. *"a client that hashes seconds sits at a different
   table from one that hashes milliseconds, with no message and no error to say
   so."*

**None of the three blocks the build; all three are constraints *on* it**, and
Part 8 carries them as the advert builder's caveats. The item is PARTIAL rather
than RESOLVED because the code does not yet hold them and because `Preset::id`
is still a `&'static str` a third name is representable in.

---

# Part 2 — The deadline sweep, by pattern

**Run as `grep` over all nine documents and all of `src/` and `tests/`, not as a
section list.** Three patterns, unioned: (a) every line matching
`hand_deadline[a-z_]*` within sixty characters of a digit run — which is what
catches a figure in a *second unit under a second name*, the shape `G7-S1` hid
in; (b) every occurrence of `600[ _]?000`; (c) every occurrence of each figure
the floor formula can produce.

Both formulae recomputed from `PROTOCOL.md` §8.2, not carried:
`FLOOR(n) = 7 000 + (2n + 23)·30 000 + 4n·25 000`;
`MIN(n) = FLOOR(n) + (n − 1)·25 000`.

| `n` | crypto term | action term | **`FLOOR(n)`** | `REOPENING_COST` | **`MIN(n)`** |
|---:|---:|---:|---:|---:|---:|
| 2 | 810 000 | 200 000 | **1 017 000** | 25 000 | **1 042 000** |
| 4 | 930 000 | 400 000 | **1 337 000** | 75 000 | **1 412 000** |
| 6 | 1 050 000 | 600 000 | **1 657 000** | 125 000 | **1 782 000** |
| 8 | 1 170 000 | 800 000 | **1 977 000** | 175 000 | **2 152 000** |
| 10 | 1 290 000 | 1 000 000 | **2 297 000** | 225 000 | **2 522 000** |

### Every numeric hand-deadline figure in the corpus and in the tree

| # | Site | Value | Applies at | Bound | Verdict |
|---:|---|---:|---:|---:|---|
| 1 | `PROTOCOL.md` §13 l. 7065 `n(17) hand_deadline_ms` | 3 300 000 | `n = 10` | `MIN(10) = 2 522 000` | **at/above — clears by 778 000; `REOPENINGS = 4`** |
| 2 | `PROTOCOL.md` §13 l. 7180, heads-up reference configuration | 1 200 000 | `n = 2` | `MIN(2) = 1 042 000` | **at/above — clears by 158 000; `REOPENINGS = 7`** |
| 3 | `PROTOCOL.md` §13 l. 7205 | 1 200 000 against 3 300 000 | 2 / 10 | — | **both at/above; a comparison, not a third listing** |
| 4 | `PROTOCOL.md` §7.2 `n(17)` row l. 5608 | *derived*, `MIN(n(11)) … 3_600_000` | any | — | **no literal lower bound — correct by construction** |
| 5 | `PROTOCOL.md` §7.2 rule 2a l. 5646 | *derived* | any | — | **no literal** |
| 6 | `PROTOCOL.md` §8.2 floor table l. 5975–5979 | 600 000, five rows | 2, 4, 6, 8, 10 | — | **historical; column header *"preset before `P3`"*, every cell labelled `— below`. Not a live figure.** `n = 8` added (`G7-S6`) |
| 7 | `PROTOCOL.md` §8.2 admitted-minimum table l. 6036–6041 | 1 042 000 … 2 522 000 | 2…10 | — | **the bound itself; `n = 8` added (`G7-S6`)** |
| 8 | `PROTOCOL.md` §8.2 l. 5925, 5929, 5931 | 600 000 | — | — | **narrative of the defect `P3` fixed — not a live figure** |
| 9 | `PROTOCOL.md` §6.4 l. 6187 | 3 300 000 | `n = 10` | `MIN(10)` | **at/above** |
| 10 | `PROTOCOL.md` §3.2 l. 1509 | 3 300 000 | `n = 10` | `MIN(10)` | **at/above — this said *"38 minutes"*, the floor, until `G7-S7`** |
| 11 | `PROTOCOL.md` §13 l. 7032 `REANNOUNCE_INTERVAL_MS` | 600 000 | — | — | **not a hand deadline.** The one `600 000` the constants module must write, and it is written |
| 12 | `PROTOCOL.md` l. 2219, 5609, 6042, 6076, 7070, 7128, 7131, 7211 | 3 600 000 | any | — | **`n(17)`'s cap, `n(18)`'s cap and `n(2) retry_after_ms` — an upper bound, plus the non-emptiness check `2 522 000 < 3 600 000`** |
| 13 | `PROTOCOL.md` l. 7217, 7357 | 600 000 | — | — | **narration of `G6-R1` and of the retired figure — not live** |
| 14 | `STATE_MACHINE.md` §2.3 l. 159 | **no value at all** | any | — | **at/above by construction — the comment names `HAND_DEADLINE_MIN(seats)` and no number** |
| 15 | `STATE_MACHINE.md` §9.4 l. 4471 validator | *derived*, `>= HAND_DEADLINE_MIN(seats)` | any | `MIN(seats)` | **at/above — `Q6` and `G7-S4` closed** |
| 16 | **`STATE_MACHINE.md` §9.1** | **no figure at all** | — | — | **the block is deleted (`G7-S1`)** |
| 17 | `STATE_MACHINE.md` l. 3676, 4251, 4258 | `600` / `600 000` | — | — | **three sentences narrating `G7-S1`; the only occurrences of `hand_deadline_sec` left in the corpus** |
| 18 | `STATE_MACHINE.md` §12.1 l. 5332, 5336, 5343 | *derived*, `HAND_DEADLINE_MIN(2)` and *"twice"* it | `n = 2` | `MIN(2)` | **at/above — `1 017 000` and `2 034 000` are gone; this pass replaced them with the name** |
| 19 | `STATE_MACHINE.md` l. 2977, 5907 | *derived* | any | — | **at/above — `3 300 000`, `1 017 000`, `2 297 000` and `4 096` all replaced by names this pass** |
| 20 | `src/poker/tournament.rs` l. 84 | 3 300 000 (`3_300` s) | `n = 10` | `MIN(10)` | **at/above, and pinned by equality to §13 by `the_rated_preset_matches_the_normative_value_exactly`** |
| 21 | `src/poker/tournament.rs` l. 121 | 1 200 000 (`1_200` s) | `n = 2` | `MIN(2)` | **at/above** |
| 22 | `src/poker/tournament.rs` l. 371 | 600 000 | 2…10 | — | **a regression assertion that 600 000 is *below* the floor at every seat count — correct** |
| 23 | `src/protocol/constants.rs` l. 107 `REANNOUNCE_INTERVAL_MS` | 600 000 | — | — | **the legitimate one, and its doc comment says so** |
| 24 | `src/protocol/constants.rs` l. 215 `RATED_HAND_DEADLINE_MS` | 3 300 000 | `n = 10` | `MIN(10)` | **at/above, and enforced at compile time: `const _: () = assert!(RATED_HAND_DEADLINE_MS >= hand_deadline_min_ms(MAX_SEATS, …))`** |
| 25 | `src/protocol/constants.rs` l. 125 `HAND_DEADLINE_CAP_MS` | 3 600 000 | — | — | **the cap** |
| 26 | `src/protocol/constants.rs` l. 295 | 600 000 | 2…10 | — | **the same regression assertion, in the module that now owns the formulae** |
| 27 | `THREAT_MODEL.md` l. 1248, 1819, 1830, 1851, 1852, 1905, 2255 — *"ten minutes"* | 600 000 implied | — | — | **stale prose, conservative in direction — `G7-S7`, still owed** |
| 28 | `NETWORK_STACK.md` l. 866, 1140; `THREAT_MODEL.md` l. 2041 — *"every 10 minutes"* | 600 000 | — | — | **`REANNOUNCE_INTERVAL_MS`, not a hand deadline. Correct** |

### Result

> **The sweep passes. `P3` stays closed, and it is now closed in a form no
> section list can reopen.**
>
> **Not one live hand-deadline figure anywhere in the corpus or the tree is below
> its bound.** Every `600 000` that survives is one of three things: the lobby
> re-announce interval, a labelled historical cell in `PROTOCOL.md` §8.2's
> *"preset before `P3`"* column, or a regression assertion that the figure is
> below the floor. The token `hand_deadline_sec` — the second unit `G7-S1` hid
> in — occurs twice in the corpus, both times inside a sentence about the defect.

**Two things about this sweep are worth carrying forward.**

1. **The number of live figures fell rather than the number of correct ones
   rising.** `STATE_MACHINE.md` carried a live numeric hand deadline at roughly a
   dozen sites two passes ago and carries **zero** now. §12.1's user-quoted
   residual bounds, §2.6's amplifier arithmetic and §2.3's seven comments were all
   converted from figures to names in this pass. **A figure that does not exist
   cannot drift, and cannot be missed by a sweep either.**
2. **The one class the sweep would still miss is a figure in a *second unit*
   under a *second name*,** which is exactly what `G7-S1` was — `hand_deadline_sec
   = 600` matches neither `600[ _]000` nor a `hand_deadline_ms` line. Pattern (a)
   catches it because it keys on `hand_deadline[a-z_]*` rather than on
   `hand_deadline_ms`, and `STATE_MACHINE.md` l. 3676 now names the class in the
   document: *"a second name in a second unit, which is the shape
   `hand_deadline_sec = 600` survived in."*

---

# Part 3 — The `table_params_hash` sweep

**Every part, and for each named configuration whether it is pinned to a single
value in exactly one place.** The test is not *do the sites agree* but *is there
one site, and does it carry every part* — a missing part reads as an omission and
never as a contradiction, which is why this class has produced two defects
(`G6-R1`, `G7-S3`) and could have produced a third silently.

### The producer's inputs

`PROTOCOL.md` §3.1's box, l. 996–1021: **twenty-five parts, in exactly that
order**, under §2.8's constructor, each length-prefixed, `preset_id` and
`deck_suite` as raw payload bytes and every other part fixed-width big-endian.
The order is §7.2's ascending `LOBBY_TABLE_AD` field order with `BlindSchedule`
expanded in place at its own field's position. The domain string is
`"p2p-poker v1 table-params"`, and it is **already written and green** in
`src/protocol/signatures.rs` as `Domain::TableParams`, inside the closed
sixteen-variant register with `Domain::ALL` and its round-trip tests.

The exclusions are named individually, each with its reason: `n(3) table_name`
and `n(10) players` (display and advisory), `n(23) password_required` (spent at
`JOIN_REQUEST`), `n(25)`/`n(26)` (identity and routing), `n(27)`/`n(28)` (the two
fields that made `advert_hash` per-receiver), `protocol_version` and `table_id`
(already separate parts of `GENESIS(0)`).

### Part by part, both named configurations

I diffed §13's two blocks against §3.1's box in order. **`RATED_SNG_POKERTH_V1`:
25 parts, 25 lines, same order, no omission, no extra. The heads-up reference
configuration: 25 parts, 25 lines, same order, no omission, no extra.**

| Part | Rated | Heads-up ref. | Pinned in exactly one place? |
|---|---:|---:|---|
| `n(0) game` | 1 | 1 | **yes** — sole legal value, §7.2; §13 marks it rather than omitting it |
| `n(1) mode` | 2 | 2 | **yes — new this pass (`G7-S3`)**, and argued to be forced by `n(9)` |
| `n(2) preset_id` | `"RATED_SNG_POKERTH_V1"` | `"CUSTOM"` | **yes** — §7.2 rule 3's closed two-value enum, mirrored by `PresetId` in code |
| `n(4) small_blind` | 50 | 50 | **yes** — and §7.2 rule 2 now *forces* `== n(13.2)`, so it cannot be filled independently |
| `n(5) big_blind` | 100 | 100 | **yes** — `== 2 * n(4)` by rule 2 |
| `n(6) ante` | 0 | 0 | **yes** — sole legal value in version 1 |
| `n(7) min_buyin` | 10 000 | 10 000 | **yes — new this pass**, and forced `== n(9)` in tournament modes |
| `n(8) max_buyin` | 10 000 | 10 000 | **yes — new this pass**, same rule |
| `n(9) start_stack` | 10 000 | 10 000 | **yes** |
| `n(11) max_players` | 10 | 2 | **yes** |
| `n(12) min_players_to_start` | 10 | 2 | **yes** |
| `n(13.0) blind_schedule.mode` | 1 | 1 | **yes** — sole legal value |
| `n(13.1) every_n_hands` | 11 | 11 | **yes for the value; see `G8-C3` for the missing range** |
| `n(13.2) first_small_blind` | 50 | 50 | **yes** |
| `n(13.3) small_blind_cap` | 50 000 | 10 000 | **yes** — and both are `n(11)·n(9)/2`, the rated preset's own **rule** rather than two numbers |
| `n(14) action_timeout_ms` | 20 000 | 20 000 | **yes** |
| `n(15) action_grace_ms` | 5 000 | 5 000 | **yes** — and the name is now single (`G7-S2`); see `G8-C6` for the code |
| `n(16) crypto_step_timeout_ms` | 30 000 | 30 000 | **yes** |
| `n(17) hand_deadline_ms` | 3 300 000 | 1 200 000 | **yes** — §9.1's second copy deleted (`G7-S1`) |
| `n(18) join_deadline_ms` | 120 000 | 120 000 | **yes** |
| `n(19) hand_delay_ms` | 7 000 | 7 000 | **yes** |
| `n(20) button_rule` | 1 | 1 | **yes** — sole legal value |
| `n(21) odd_chip_rule` | 1 | 1 | **yes** — sole legal value |
| `n(22) showdown_policy` | 1 | 1 | **yes**, pending `Q-01` — and §9.4 refuses a table whose policy this build does not implement, so an unimplemented value is a closed table and never a divergence |
| `n(24) deck_suite` | `"bs-bg12-secp256k1/1"` | same | **yes** — sole legal value |

### Result

> **The `table_params_hash` sweep passes for the first time in the series. Two
> conforming clients compute the same hash for both named configurations.**
>
> Twenty-five parts, twenty-five pinned values, one place each, both
> configurations, and every part indexed to the box that hashes it. The three
> holes `G7-S3` found are filled, and two of the three were filled by making the
> value **unfree** — `n(7)` and `n(8)` are now forced equal to `n(9)` by a
> receiver rule, so they cannot be filled differently by two clients even if a
> future configuration forgets to state them.

**What still cannot compute the hash, and it is a code fact rather than a
specification gap.** `src/poker/tournament.rs`'s `Preset` carries **fourteen** of
the twenty-five. That was `PHASE4_GATE2.md`'s reason for refusing the build, and
it is no longer a reason: the eleven missing parts now all have pinned values to
supply, so what remains is transcription. §13 states the shape the transcription
must take — *"The advert type owns the twenty-five parts, not `Preset`"* — and
Part 8 carries it as the build's first caveat.

**One residual observation the sweep produced, worth keeping as a rule.** §13's
blocks pin `n(23) password_required = false` on a line explicitly marked *"not a
part of `table_params_hash`, and stated only so its absence is not read as an
omission."* That is the correct treatment of an excluded field: **inside a
configuration block, an excluded part gets a line saying it is excluded.** It is
the same discipline as the *sole legal value* markers, and it is what makes the
diff-against-the-box audit total rather than approximate.

---

# Part 4 — The duplicate-value sweep

**Any table parameter whose value appears in more than one document.** Run as a
name-and-number grep for all twenty-four parameter names over the seven documents
that are **not** `PROTOCOL.md` (the owner) and not `DECISIONS.md` (the ledger,
whose rows narrate values by necessity), then every hit read in context.

### Result

> **No table parameter's value is specified in more than one document. The class
> that produced `G6-R1`, `G7-S1` and `G7-S5` has no live instance left.**

**What the sweep found instead — every hit, and why each is not a second
specification:**

| Site | Text | Why it is not a duplicate |
|---|---|---|
| `STATE_MACHINE.md` §9.2 l. 4285–4286 | `small_blind(h) = min(config.first_small_blind * 2^(level(h) − 1), config.small_blind_cap)` | **field names, no literals.** §9.2 says so: *"Three config fields and no literals (D-011 rule 2). The form that stood here wrote the preset's `11`, `50` and `50_000` into the formula, which made it a fourth copy of three values"* — **deleted this pass** |
| `STATE_MACHINE.md` §9.4 l. 4476–4480 | five inequalities over config fields | **the engine's own five conditions**, stated by §9.4 to be *"in no other document"*. The four literal ranges that stood beside them — `2 <= seats <= 10`, `2 <= min_players_to_start <= seats`, `first_small_blind >= 1`, `action_timeout_ms >= 5_000` — are **deleted this pass** as copies of §7.2's field table |
| `STATE_MACHINE.md` §2.6 l. 788–789 | `config.crypto_step_timeout_ms`, `config.hand_delay_ms + config.crypto_step_timeout_ms` | **field names, no literals** — but it is a second listing of a *mapping* §8.3 owns. See **`G8-C4`** |
| `STATE_MACHINE.md` §2.3 l. 138–195 | twenty-three fields, each with `n(k)` | **no values at all**, and the section says so: *"No value is written down in this struct, only field numbers (D-011 rule 2)"* |

**Four prose asides carry a preset value in a second document. All four agree
with §13, none is a specification, and each does work the pointer alone would not
do — but they are the class, so they are recorded:**

| Site | The value | The work it does |
|---|---|---|
| `STATE_MACHINE.md` l. 3046, 3053 | *"`config.ante`, 0 in the preset"* / *"In the preset the ante is 0"* | explains why a real defect in hand-init step 6 — `Removed` omitted from the ante-posting status list — is **inert at the shipped configuration**, which is why it needed finding by reading rather than by playing |
| `STATE_MACHINE.md` l. 3485 | *"pins `showdown_policy = MANDATORY_REVEAL` in `RATED_SNG_POKERTH_V1`"* | explains why two wire branches exist while one is live in the MVP |
| `NETWORK_STACK.md` l. 2865 | *"pins `seats = 10` and `min_players_to_start = 10`"* | the reason the Phase 8 acceptance test runs on a `CUSTOM` two-seat table |
| `STATE_MACHINE.md` l. 5357–5362 | the drain arithmetic — 825, 1 650, 3 300, 5 775, 4 225, hand 40 | **guarded this pass**: *"Worked against `PROTOCOL.md` §13's starting stack and blind schedule — and holding only against those, since every figure in this paragraph is **derived** from them rather than being a parameter of its own"* |

**The fourth is the model.** A *derived* figure in a second document is not a
second specification of a parameter, and the guard sentence is what makes the
difference legible to the next sweep. The first three would each be improved by
the same treatment — *"0 at §13's configuration"* rather than *"0 in the
preset"* — but none is a defect and none is filed.

**And one number that used to be a duplicate is now a name.**
`MAX_RETAINED_HAND_RECORDS` was written as `4 096` at `STATE_MACHINE.md` l. 723,
l. 2977 and l. 5907; all three now read the identifier with *"(`PROTOCOL.md` §13
owns the figure)"*. That is a local constant rather than a table parameter, and
it was swept anyway, which is the correct scope for a pattern sweep.

---

# Part 5 — Both K1 interleaving variants and the replay check

**This part is short and certain, and the reason is new to the series: the three
edited documents are uncommitted, so `git diff` gives the exact line set this
pass changed.** Every load-bearing site below was checked for membership in that
set, and then re-read at its current line.

### 5.1 The load-bearing sites, against the diff

| Site | Current line | In this pass's diff? |
|---|---:|---|
| §5.3 step 4 — `dealt_in[s] := (status == Active ∧ s ∈ signed_this_hand)` | 2941 | **no** |
| §5.3 step 8 — `if \|signed_this_hand\| == 1 then solitary_since := solitary_since.or(Some(hand_id))` | 3090 | **no** |
| §5.3 step 8 — *"It reads `signed_this_hand` and never `signed_this_hand ∪ readmit`"* | 3091 | **no** |
| §5.3 step 8 — `signed_this_hand := ∅` and `readmit := ∅` | 3071 | **no** |
| T67 — `readmit ∪= {e.seat}` **and nothing else**, guard `hand_id > 0 ∧ occupied ∧ status ∉ {Removed, Empty}` | 2527 | **no** |
| T62 — guard `solitary_at(e.hand_id)`, `j <= k + 1` | 2522 | **no** |
| T63 — same guard inside `TableClosed`, `settlement.tournament_winner := None` | 2523 | **no** |
| §2.6's `solitary_at` derivation, `j <= k + 1` | 705–709 | **no** |
| I31, I33, I34 | 4650–4653 | **no** |
| `PROTOCOL.md` §4.0 rows 10a, 10b, 12, 12a | 1845–1849 | **no** |
| `PROTOCOL.md` §4.9's close box — the *"freeze, compare, readmit, or drop"* pointer | 3351 | **no** |
| §5.2.1's anti-replay key | 72 | **no** |

**Not one of them moved.** `STATE_MACHINE.md`'s twenty-eight hunks are §2.3,
§2.6, §9.1, §9.2, §9.4, §12.1 and a scatter of single-line replacements of
figures by names; `PROTOCOL.md`'s are §3.2, §4.10, §7.2, §8.2, §11 and §13. The
three changes nearest the freeze path are l. 2977 (§2.6's amplifier arithmetic,
figures → names), l. 3813 (§4.10's tier-2 unit, `G7-S8`) and l. 5904–5907 (§12's
narration of `P2-e`, figures → names). **None of the three changes a rule.**

### 5.2 Variant 1 — `B`'s `HAND_INIT(k+1)` copy arrives

Prefix as `PHASE4_GATE.md` §3: three seats, `C`'s `PLAYER_LEAVE` reaches `A` and
not `B`, hand `k` stalls and aborts through T57 → T46, `P(k) = {A}` at `A`.

At hand `k`'s init `A` reads `|signed_this_hand| == 3` and step 8 writes nothing.
T47 moves `checkpoints.live` to `boundary`. T46 opens hand `k`'s checkpoint 8 in
`live` with `required = signed_this_hand` and `own` written at the opening. `A`'s
checkpoint-8 window self-completes; step 8 writes **`solitary_since := Some(k+1)`**
by the **second** disjunct; `RetainedHand(k)` is written with `was_solitary = true`.

`B`'s checkpoint-8 `STATE_HASH` for hand `k` arrives: step 10a skipped (`N2`, no
store for a finished hand), step 10b's checkpoint-8 branch, compared against
`checkpoint8_state_hash(k)` → mismatch → §6.3 step 1 → step 12a with the retained
record saying solitary → `SolitaryDivergence { hand_id: k }`.

**T62's guard:** `solitary_at(k)` = `solitary_since == Some(k+1)` ∧ `k + 1 <= k + 1`
∧ `k <= hand_id` → **true**. T62 fires: `Diverged`, `Fault{SolitaryDivergence}`,
`solitary_contradicted := true`, the offending event is not applied and not
counted into `signed_this_hand`, the hand deadline is not disarmed.

The freeze holds by I33(b) and I33(c): while frozen the only permitted
publications are the `DISPUTE`, the reconciliation `STATE_HASH` and the matching
`STATE_ACK`; the reconciliation stage is required of `R(c) ∪ W = {A, B}`, so
`|R| = 2`; and **T53's two-signer conjunct refuses a one-signer completion
independently of the set's definition.** `B`'s `HAND_INIT(k+1)` then changes
nothing — `A` is in `Diverged`, T62 does not re-enter, T49/T50 are excluded from
the phase.

**Verdict, variant 1: fires on the checkpoint-8 `STATE_HASH` and holds. CLOSED.**

### 5.3 Variant 2 — only the checkpoint-8 `STATE_HASH` ever contradicts

Every step is identical, because **the event that fires the freeze in variant 1
is the checkpoint-8 `STATE_HASH` itself**; dropping the `HAND_INIT` copy removes
nothing. `j = k + 1 <= k + 1`; `R(c) ∪ W = {A, B}`; §9.3 condition 0.6 is
evaluated before conditions 1–4 while the latch is set, so `A` never reaches a
solitary tournament win; and if the copy arrives after `A` closed, **T63** has the
same guard and sets `settlement.tournament_winner := None`.

**Verdict, variant 2: fires and holds; the win is retracted after closure. CLOSED.**

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
| `solitary_since` written when K1 needs it? | **yes** — `\|signed_this_hand\| == 1` is unaffected by `readmit` | **yes** |

**The input that reversed both walks three passes ago, re-run:** feed `readmit`
at hand `k+1`'s init. Step 8 reads `|signed_this_hand| == 1`, which is still one
at `A`, so `solitary_since := Some(k+1)` is written **regardless of `readmit`'s
contents**; `solitary_at(k)` is true; T62 and T63 both fire. **The reversal is
still gone.**

**`grep -n "admitted"` over all nine documents returns no live reader.** Every
occurrence in `STATE_MACHINE.md` is past-tense narration of the deleted union
(l. 677, l. 5900, l. 5904 and the rest of §12's `P2-e` account), every occurrence
in `DECISIONS.md` is `P2-e`'s row, and every occurrence elsewhere is unrelated.
**The union is still gone from the corpus.** The one line of that narration this
pass touched replaced `3 300 000 ms` with *"a whole `hand_deadline_ms`"* and
`4 096` with `MAX_RETAINED_HAND_RECORDS` — a figure-to-name substitution inside a
sentence about a deleted mechanism.

> **All three closed items are still closed. Nothing this pass changed is within
> a rule of them, and the diff proves it rather than the reading asserting it.**

---

# Part 6 — The healthy-table check, and the human-paced table

## 6.1 Protocol-paced — what the harness measures

Twenty hands, `n` seats, nothing goes wrong, every peer answers in one round
trip. `6n + 20` round trips per hand [§8.2]. 50 ms RTT; 42 ms of Bayer–Groth
proving per shuffler [`MENTAL_POKER.md` §5.1], sequential.

| Seats | Trips | Protocol | Proving | **Per hand** | **20 hands** | Deadline in force | Fires? |
|---:|---:|---:|---:|---:|---:|---:|---|
| 2 | 32 | 1.600 s | 0.084 s | **1.684 s** | **33.7 s** | 1 200 000 (§13 heads-up ref., `CUSTOM`) | **no** |
| 4 | 44 | 2.200 s | 0.168 s | **2.368 s** | **47.4 s** | 1 412 000 (`MIN(4)`, worst legal) | **no** |
| 6 | 56 | 2.800 s | 0.252 s | **3.052 s** | **61.0 s** | 1 782 000 (`MIN(6)`) | **no** |
| 8 | 68 | 3.400 s | 0.336 s | **3.736 s** | **74.7 s** | 2 152 000 (`MIN(8)`) | **no** |
| 10 | 80 | 4.000 s | 0.420 s | **4.420 s** | **88.4 s** | 3 300 000 (§13 rated) | **no** |

No configuration is shipped at 4, 6 or 8 seats, so the deadline in force is the
smallest a conforming founder may advertise — `HAND_DEADLINE_MIN(n)`, which since
`G5-Q6` is one `REOPENING_COST` above the floor.

**Margins per deadline kind, recomputed:**

| Deadline | Value | Worst healthy consumption | Margin |
|---|---:|---:|---:|
| `crypto_step_timeout_ms`, per stage | 30 000 ms | ≈ 92 ms (42 ms proof + 50 ms RTT) | **326×** |
| boundary (`hand_delay_ms + crypto_step_timeout_ms`) | 37 000 ms | ≈ 150 ms | **246×** |
| `join_deadline_ms` | 120 000 ms | — | T4 fires long first |
| `action_timeout_ms + action_grace_ms` | 25 000 ms | human — **§6.2** | n/a |
| `hand_deadline_ms`, whole hand, `n = 10` | 3 300 000 ms | 4.420 s protocol | **746× on protocol alone** |
| `hand_deadline_ms`, whole hand, `n = 2` | 1 200 000 ms | 1.684 s protocol | **713× on protocol alone** |

**No deadline of any kind fires at any seat count.** Every hand places a
checkpoint: §6.2 row 8 is unconditional, I32(a) asserts coverage on both terminal
paths, and `own` is written at every opening (T45, T46, §3.4), so the record a
later hand compares against exists and is readable.

**The measured half.** `cargo test --tests` — the random-hand harness at 2, 6 and
10 seats plus the tiny-stack all-in walk at 2, 3 and 5 — **4 passed**;
`cargo test --lib` — **147 passed, 0 failed**, up from 139. Chip conservation
holds at every seat count tested.

## 6.2 Human-paced — the term the protocol walk does not measure

**This is what the deadline actually has to pay for, and the previous gate named
it without pricing it.** A hand's betting actions are `4n − 3` — `n` pre-flop and
`n − 1` on each of three streets [§8.2] — and the deadline budgets each at
`action_timeout_ms + action_grace_ms = 25 000 ms`. A human-paced hand costs the
crypto walk at machine speed **plus** the betting actions at human speed:

```
HUMAN(n) = hand_delay_ms                  [7 000]
         + (2n + 23) · 50 ms              [the crypto trips, machine-paced]
         + 42n ms                         [Bayer–Groth proving, sequential]
         + (4n − 3) · t                   [the betting actions, at t per action]
```

**Worst legal case, `t = 25 000 ms` — every seat uses its whole timeout and its
whole grace, at every action, in every hand:**

| Seats | betting actions | betting time | crypto + proving + delay | **worst legal hand** | **+ the one reopening `MIN(n)` buys** |
|---:|---:|---:|---:|---:|---:|
| 2 | 5 | 125 000 | 8 434 | **133 434 ms** ≈ 2 min 13 s | **158 434 ms** |
| 4 | 13 | 325 000 | 8 718 | **333 718 ms** ≈ 5 min 34 s | **408 718 ms** |
| 6 | 21 | 525 000 | 9 002 | **534 002 ms** ≈ 8 min 54 s | **659 002 ms** |
| 8 | 29 | 725 000 | 9 286 | **734 286 ms** ≈ 12 min 14 s | **909 286 ms** |
| 10 | 37 | 925 000 | 9 570 | **934 570 ms** ≈ 15 min 35 s | **1 159 570 ms** |

**Against the deadline in force:**

| Seats | worst legal hand + 1 reopening | deadline in force | **margin** | at `MIN(n)` exactly | **margin** |
|---:|---:|---:|---:|---:|---:|
| 2 | 158 434 | 1 200 000 | **7.6×** | 1 042 000 | **6.6×** |
| 4 | 408 718 | 1 412 000 | **3.5×** | 1 412 000 | **3.5×** |
| 6 | 659 002 | 1 782 000 | **2.7×** | 1 782 000 | **2.7×** |
| 8 | 909 286 | 2 152 000 | **2.4×** | 2 152 000 | **2.4×** |
| 10 | 1 159 570 | 3 300 000 | **2.8×** | 2 522 000 | **2.2×** |

**A realistic table, `t = 8 000 ms` median decision:**

| Seats | 2 | 4 | 6 | 8 | 10 |
|---|---:|---:|---:|---:|---:|
| per hand | 48 434 ms | 112 718 ms | 176 002 ms | 240 286 ms | 305 570 ms |
| ≈ | 48 s | 1 min 53 s | 2 min 56 s | 4 min 0 s | 5 min 6 s |
| margin at the deadline in force | 24.8× | 12.5× | 10.1× | 9.0× | 10.8× |

### What the two halves say together

> **The smallest margin anywhere in this part is 2.2×, at ten seats on a table
> advertised at exactly `HAND_DEADLINE_MIN(10)`, against a hand in which every
> one of thirty-seven actions consumes its whole timeout and its whole grace and
> one reopening raise is made.** That is the worst legal case the bound admits,
> and it clears. Every other cell clears by more.

**Why the margin is smallest at eight and ten seats and not at two.** The floor's
crypto term — `(2n + 23) · 30 000`, which is 1 290 000 ms of the 2 297 000 at ten
seats — is **pure slack on a healthy table**: it is spent at 92 ms per step, a
326× overpayment. At two seats that slack is 810 000 ms against a 125 000 ms
human term, so the deadline there is dominated by money nobody spends. As `n`
grows the human term grows twice as fast as the crypto term (`4n` against `2n`),
and by ten seats it is 1 000 000 against 1 290 000. **The margin narrows towards
the seat counts the harness tests least.**

**And this is the sharpened form of `PHASE4_GATE2.md`'s observation, which is
what this gate was asked to produce.** At `G7-S1`'s retired `600 000 ms`:

| Seats | worst legal human hand | vs 600 000 | with one reopening | vs 600 000 |
|---:|---:|---|---:|---|
| 2 | 133 434 | 22 % — **passes** | 158 434 | 26 % — **passes** |
| 4 | 333 718 | 56 % — **passes** | 408 718 | 68 % — **passes** |
| 6 | 534 002 | **89 % — passes** | 659 002 | 110 % — **fails** |
| 8 | 734 286 | 122 % — **fails** | 909 286 | 152 % — **fails** |
| 10 | 934 570 | 156 % — **fails** | 1 159 570 | 193 % — **fails** |

> **Not even a human-paced walk would have caught `G7-S1` at two, four or six
> seats without a reopening raise in the hand** — and two, four and six were the
> seat counts this corpus tested until `G7-S6` added eight. The protocol walk
> passes at `600 000` by a factor of 136 000; the *human* walk passes at two and
> four by a factor of four, and at six seats it consumes **89 %** of the deadline
> and still passes. The defect becomes visible at six seats only if the hand
> contains one reopening raise, and unconditionally only at eight.
>
> **So the class of check that finds this defect is not a timing check at any
> pace. It is the comparison Part 3 runs — a named configuration diffed against
> the box that hashes it — and that is the whole argument for why the fix had to
> be a deletion and not a corrected digit.**

---

# Part 7 — New-defect hunt

| ID | Severity | Defect |
|---|---|---|
| **`G8-C1`** | **medium — and it is `G7-S8`'s remaining half** | **`PROTOCOL.md` §4.9 l. 3641 still states the tier-2 precondition in the unit §4.10 retired this pass, and the surviving site is the one labelled *normative*.** §4.10 l. 3813 now requires a checkpoint *"whose §6.2 checkpoint number is at or after the number of the checkpoint the illegality was fixed at"*, and its box gives the argument: *"the two units are not equivalent: **at or before the offending event** is satisfied by checkpoint 1 of the chain, which fixes almost no state and decides almost no illegality."* §4.9's box — headed *"Tier 2 is judged against a checkpoint both peers signed — **normative**"* — still reads *"a checkpoint of the same chain **whose `sequence` is at or before the offending event's**"*. **The box that named the defect fixed one of the two sites and left the other**, and the one left is the one an implementer reading §4.9's `CHEAT_EVIDENCE` definition meets first. Under the retired unit, checkpoint 1 of the chain satisfies the precondition, so a tier-2 removal is admissible against a state that fixes almost nothing — the exact hazard §4.10's box argues against. **Why the sweep missed it:** the fix was applied at the site the item named, and `G7-S8` named §4.10. A `grep` for *"at or before the offending event"* returns both, in one command. **Fix: §4.9's clause becomes a citation of §4.10's row** — the precondition should be stated once, and §4.10's row is where the disposition table lives. Owner: `PROTOCOL.md` §4.9. |
| **`G8-C2`** | **medium, latent** | **`STATE_MACHINE.md` §9.4's engine validator refuses every conforming cash table, in the section that specifies cash mode.** The validator carries `start_stack >= 2 * (2 * first_small_blind + ante)  // at least one full orbit`, unconditionally. `PROTOCOL.md` §7.2's `n(9) start_stack` row reads *"tournament modes only; **`0` for cash**"*, and §7.2's `n(1) mode` admits `1 = CASH_PLAY_MONEY`. So a well-formed cash advert has `start_stack = 0`, fails `0 >= 4` at even the weakest legal blind, and §9.4's own next sentence disposes of it: *"A config that fails goes to `TableClosed` and is never played. There is no repair path."* **§9.4 is titled *Cash mode and custom tables*, opens by specifying how cash mode differs in exactly three ways, and its validator then makes the mode unreachable.** The quantity the orbit check wants in cash mode is `min_buyin`, which §7.2 bounds at `>= big_blind` and which `TableConfig` deliberately does not carry (§2.3 excludes `n(7)`). Latent because no advert parser exists and the MVP is tournament-only — which is precisely why it should be fixed before one exists. **Fix: scope the line to tournament modes and give cash mode its counterpart over `min_buyin`; or state that cash mode is refused in version 1 and delete the three-way description.** Owner: `STATE_MACHINE.md` §9.4. |
| **`G8-C3`** | **medium; two halves, and the second is `G6-R2`'s exact shape** | **`n(13.1) every_n_hands` has no lower bound in `PROTOCOL.md` §7.2, so a conforming advert may carry `0` and divide by zero at every peer — and §9.4 claims the defect is filed when it is not.** §7.2's `BlindSchedule` paragraph gives `n(1) every_n_hands: u16` with no range, and §7.2 rule 2 is *"every numeric range above"*, so there is nothing to check. §9.2's `level(h) = 1 + (h − 1) / e` is then a division by zero **at every peer that computes it**, and `src/poker/tournament.rs`'s `level()` — `1 + (hand - 1) / self.raise_every_hands` — panics. §9.4 catches it at the engine (`blind_raise_every_hands >= 1`), says so, and then says: *"the missing range belongs to `PROTOCOL.md` §7.2; it is **reported for that owner's list under D-013** rather than fixed from here."* **`grep "every_n_hands" docs/DECISIONS.md` returns nothing. The row does not exist.** That is `G6-R2` verbatim — a filing claim accepted because of where it appeared — reappearing in the pass that applied `G6-R2-m`'s discipline to everything else. The two halves are independent and both must land: **one range line in §7.2's `BlindSchedule` paragraph, and one row in `DECISIONS.md`'s open list.** Owner: `PROTOCOL.md` §7.2 (the range); `DECISIONS.md` (the row). |
| **`G8-C4`** | **low–medium, and this pass's own fix created it** | **`STATE_MACHINE.md` §2.6's new `duration_ms` table is a second listing of `PROTOCOL.md` §8.3's normative `next_deadline_ms` mapping — with the wrong section cited and the wrong row count.** The box was written this pass to close `G7-S2`'s second half, and it opens: *"`PROTOCOL.md` **§8.2** owns `next_deadline_ms` and its **three** rows; no value of theirs is restated here."* Both facts are wrong. `next_deadline_ms` is owned by **§8.3** (l. 5880–5888) — §8.2 is the hand-deadline derivation — and §8.3's table has **four** rows, not three: §2.6 merged *any cryptographic contribution* with *`STATE_HASH` / `STATE_ACK`*. The disclaimer is true as far as it goes (no *value* is restated; both tables carry field names) but the thing duplicated is the **mapping**, which §8.3 declares *"normative, not the emitter's choice"*, and D-011 rule 2 is *reference, never restate*. **The merge also drops the content an implementer needs:** §8.3 enumerates the crypto event types — `DECK_INIT`, `SHUFFLE_*`, `DECK_COMMIT`, `DEAL_PRIVATE`, `BOARD_REVEAL`, `SHOWDOWN_*` — where §2.6 says only *"any cryptographic contribution"*. If §8.3 ever splits or adds a row, §2.6 drifts silently, and it is a mapping no test reads. **This is `G7-S1`'s shape produced by `G7-S1`'s own remedy.** **Fix: §2.6's table cites §8.3's rows and names only the `config.` fields the scheduler reads, or §8.3's table moves here and §8.3 cites it — one or the other, not both; and correct the citation to §8.3 and the count to four.** Owner: `STATE_MACHINE.md` §2.6. |
| **`G8-C5`** | **low–medium; the count is wrong in the direction that invites the defect the sentence exists to prevent** | **`STATE_MACHINE.md` §2.3 undercounts the `table_params_hash` parts absent from `TableConfig` by two.** The paragraph lists ten advert fields as absent — `n(3)`, `n(7)`, `n(8)`, `n(10)`, `n(23)`, `n(24)`, `n(25)`, `n(26)`, `n(27)`, `n(28)` — and concludes *"**Three** of those — `n(7)`, `n(8)` and `n(24)` — are nevertheless parts of `table_params_hash`, so **`table_params_hash` is computed over the advertisement and never over this struct**."* True of the ten it lists; **false of the struct.** `n(4) small_blind` and `n(5) big_blind` are also absent from `TableConfig` — the engine derives them per hand from `first_small_blind` via §9.2 — and both are parts of §3.1's box. **Five parts are missing, not three.** The conclusion is right and is the load-bearing half, but the count is wrong in the one direction that matters: a reader who believes three parts are missing may reasonably think *"then I add three fields and hash from `TableConfig`"*, which is the exact failure the sentence exists to forbid — and `n(4)` and `n(5)` are the two an implementer is likeliest to assume are derivable, since they **are** derivable for play and are **not** derivable for the hash. **Fix: name `n(4)` and `n(5)` in the absent list with their own reason (*derived per hand by §9.2, and still hashed*), and change *three* to *five*.** Owner: `STATE_MACHINE.md` §2.3. |
| **`G8-C6`** | **low** | **`src/poker/tournament.rs` carries the retired name `action_timeout_grace_sec` at five sites, against §13's *"No value has two names anywhere in the corpus"* and against the rename this pass made for exactly that reason.** §2.3 renamed `action_timeout_grace_ms` to `action_grace_ms` and justified it: *"it was the only site in the corpus carrying the other name, and §13 requires that no value have two names."* True of the documents, false of the tree: `Preset::action_timeout_grace_sec` at l. 34, its two initialisers at l. 78 and l. 117, and its two readers at l. 196 and l. 217. **§13's rule is explicitly about code names** — *"The names are the ones a Rust `constants` module would carry, which is why several differ from earlier drafts"* — so the tree is in scope, and this is the last site. It matters beyond tidiness for one reason: the field is what will populate `n(15) action_grace_ms` in the advert builder, and a hashed part whose only link to its `n(k)` is a name that does not match is the shape `G7-S9`'s third bullet warns about. **Fix: rename the field, and add the sweep test — `no_retired_domain_string_appears_in_our_sources` in `signatures.rs` is the precedent and the pattern.** Owner: `src/poker/tournament.rs`. |
| **`G8-C7`** | **low, latent** | **`every_shipped_preset_meets_its_own_floor` asserts against the retired bound.** The test compares each shipped configuration's deadline to `hand_deadline_floor_ms()`, but since `G5-Q6` both §7.2 rule 2a and §9.4 admit only `HAND_DEADLINE_MIN`. **A configuration between the floor and the admitted minimum passes this test and is refused by every conforming joiner** — the founder-nobody-can-join defect `G7-S4` closed in the document, still open in the test that would catch it. Latent: both shipped configurations clear `MIN` today (`REOPENINGS = 4` and `7`), and `constants.rs` pins the rated one at compile time against `hand_deadline_min_ms`. But the test's **name** claims it checks what a shipped configuration must meet, and it checks one `REOPENING_COST` less than that. `the_shipped_presets_can_afford_at_least_one_reopening_raise` covers the same ground by a different route, which is why this is latent rather than open — and that is itself the finding: **two tests express one bound, and the one whose name claims the bound has the weaker form.** **Fix: the assertion becomes `>= hand_deadline_min_ms(…)`, taken from the constants module rather than from `Preset`'s own copy.** Owner: `src/poker/tournament.rs`. |
| **`G8-C8`** | **trivial** | **`STATE_MACHINE.md` §2.3 l. 178 says `DeadlineKind::Crypto` *"is armed by eleven **transitions**"*; §2.6 l. 800 says *"`Crypto` at eleven — T2, T7, T10, T14, T18, T19, T32, T33, T38, T39 and §5.3's `\|dealt_in\| >= 2` branch"*.** §2.6 is right and §2.3 is off by one in kind: ten transitions and one §5.3 branch, eleven **sites**. I verified the enumeration by grep over §5.2's rows and it is exact — fourteen transitions arm a deadline, four `Action` (T26, T29, T31, T37) and ten `Crypto`. Recorded rather than filed as a defect, because §2.6 states it correctly two hundred lines away and the number is the same. **Fix: *sites* for *transitions* in §2.3.** Owner: `STATE_MACHINE.md` §2.3. |

## The pattern check, run on this pass's own findings

**Three of the eight are one pattern, and it is the pattern this gate series
exists to catch: a statement that exists in two places, where fixing one is
mistaken for fixing the class.** `G8-C1` (§4.10 fixed, §4.9 not), `G8-C4` (a
mapping duplicated by the fix for a missing mapping), `G8-C5` (a list that is
complete for itself and incomplete for its subject). **Two more are filing
failures rather than content failures:** `G8-C3`'s second half is a claim to have
filed something that was not filed, and `G8-C8` is one section disagreeing with
another about a count both took from the same table.

**`G8-C4` is the one worth the next pass's attention, because of how it was
made.** `G7-S2` was *"a quantity a formula reads and a struct does not have"* — a
**missing source**. The fix supplied the source by writing a table. The table
duplicates a normative mapping another document owns. **The remedy for a missing
reference is a reference, and a table is not one.** That is the fifth rule in the
series, and it is narrower and more useful than *prefer deleting to defending*:

> **When a document is found to be missing a source for a quantity, the fix is a
> citation of the owner. Writing the owner's content locally satisfies the
> finding and creates its opposite.**

The previous four were: *re-derive every reader of a quantity you narrow*;
*re-list the dispositions after the additions land*; *when a fix lands in a
document and the code, re-read the first half's number rather than its argument*;
and *a sweep is specified by pattern, never by section list.* **The fourth was
this pass's method and it worked** — Parts 2 and 4 both passed, and the two
defects in Part 7 that come from sweeps (`G8-C1`, `G8-C3`) come from fixes
applied at a named site rather than by grep.

## One bookkeeping observation, filed as an observation and not as a defect

**`G7-S1`, `G7-S2`, `G7-S4`, `G7-S5` and `G7-S8` appear nowhere in
`DECISIONS.md`.** Each was fixed by its own owner in the same pass it was raised,
and each is narrated at the site it was fixed — §9.1 names `G7-S1` and `G7-S5`,
§2.3 names `G7-S2`, §9.4 names `G7-S4`, §4.10 names `G7-S8` — so nothing is lost
to a reader following the authority order. D-013's rule is about a defect
belonging to **another** owner, and none of these did. **It is recorded because
the ledger's silence and a defect's absence look identical from outside**, and a
future pass reading `DECISIONS.md` alone would not know these five were ever
raised. The four that **are** filed (`G7-S3`, `G7-S6`, `G7-S7`, `G7-S9`) are the
four with an owed half, which is the correct discipline applied.

---

# Part 8 — The build list, item by item

## `TableConfig` — **BUILD**

**Unblocked. `G7-S2` is closed in the struct, in the arming sites and in the
statement of what the struct is for.** Source: `STATE_MACHINE.md` §2.3
l. 138–195 — twenty-three fields, twenty of them advertised and each carrying its
`n(k)`, three named as not advertised (`protocol_version` and `table_id` from the
envelope and from `GENESIS(0)`; `auto_action_limit` from §13's
`MAX_CONSECUTIVE_AUTO_ACTIONS`). I counted the struct: **twenty-three fields,
twenty with an `n(k)`.** The claim checks out.

**Four caveats, and the first is what makes the build safe:**

1. **Never compute `table_params_hash` from it.** §2.3 says so in bold and gives
   the consequence — a peer that does *"can then join no table at all"*. The
   hash's producer takes the advertisement. **And the parts absent from
   `TableConfig` are five, not the three §2.3 counts** — add `n(4) small_blind`
   and `n(5) big_blind` to the list you carry (`G8-C5`).
2. **Name the grace field `action_grace_ms`.** §2.3 renamed it this pass so that
   no value has two names; `tournament.rs` still carries the other one
   (`G8-C6`). The new struct must not inherit it.
3. **`Deadline.duration_ms` is filled from `PROTOCOL.md` §8.3's four rows**, not
   from §2.6's three (`G8-C4`). The two agree today. Cite the owner.
4. **`hand_delay_ms` is in the struct and is not an engine input.** It is
   display-only (§8.3) and is carried because §8.2 makes it a *term* in two
   durations — the hand-boundary deadline and `HAND_DEADLINE_FLOOR(n)`. §2.3
   states this; a reader who deletes the field on the strength of
   *"DISPLAY ONLY"* breaks the floor computation.

`preset_id` should be typed as `PresetId` — the enum is written and green in
`constants.rs` — rather than as a string, which is `G7-S9`'s second bullet
arriving one struct early.

**Gap: none.** `G8-C2`, `G8-C5` and `G8-C8` sit in §9.4 and §2.3 and none of them
changes a field of this struct.

## `table_params_hash` and the advert builder — **BUILD**

**Unblocked. `G7-S3` is closed and Part 3 verifies the closure part by part:
twenty-five parts, twenty-five pinned values, one place each, both named
configurations.** Two conforming clients now compute the same hash.

Source: **`PROTOCOL.md` §3.1's box for the hash, §13's two blocks for the values,
§7.2's field table and rules 2 / 2a / 3 / 7 for admission.** The crypto half is
already written and green: `Domain::TableParams` → `"p2p-poker v1 table-params"`
in `src/protocol/signatures.rs`, inside the closed register with its enumeration
test.

**Four caveats, all from `G7-S9` and §13, and none of them is a value:**

1. **The advert type owns the twenty-five parts, not `Preset`.** `Preset` carries
   fourteen; the eleven it does not are `n(0)`, `n(1)`, `n(4)`, `n(5)`, `n(7)`,
   `n(8)`, `n(13.0)`, `n(20)`, `n(21)`, `n(22)` and `n(24)`, and **every one is
   hashed**. `Preset` may stay as the convenience that fills its fourteen.
2. **Fill the single-legal-value parts explicitly.** §13 pins them with a *sole
   legal value* marker rather than omitting them, for the stated reason: *"a part
   left out to save a line is indistinguishable from a part nobody thought
   about."* The builder should mirror that — no defaults, no
   `..Default::default()`.
3. **Type `n(2)` as `PresetId`**, so a third name is unrepresentable at the
   emitter and not merely rejected at the receiver.
4. **Hash milliseconds.** `Preset` stores `*_sec`; §3.1 hashes `u32_be(…_ms)`.
   The ×1000 is part of the hash, and a client that hashes seconds *"sits at a
   different table … with no message and no error to say so."*

**Build the tournament path.** A cash advert is well formed under §7.2 with
`start_stack = 0`, and `STATE_MACHINE.md` §9.4's own validator then refuses it
(`G8-C2`); the MVP is tournament-only and both named configurations are
`mode = 2`.

**Test it the way `G6-R1` was finally tested — by equality against the document,
not by an inequality.** The right shape is a fixture holding §13's twenty-five
rated values and one assertion that the builder's `table_params_hash` equals a
recorded digest, plus the same for the heads-up reference configuration. An
inequality cannot pin a constant and a hash offers no inequality, which makes
this the easiest item in the tree to pin correctly.

**Gap: none for the two named configurations.** A third, future configuration is
audited by diffing its block against §3.1's twenty-five parts, which is the check
§13 now prescribes.

## The rest of the build list — **still clear**

Re-checked against this pass's diff and against the current text; none of the
eleven items below is touched by any hunk, and none acquires a new gap.

| Item | State | Evidence |
|---|---|---|
| `src/protocol/constants.rs` | **BUILT this pass** | 341 lines; the three formulae as `const fn`, `PresetId`, `RATED_HAND_DEADLINE_MS`, ten compile-time `const _: () = assert!` relationships including `RATED_HAND_DEADLINE_MS >= hand_deadline_min_ms(MAX_SEATS, …)`, and eight tests covering `n = 8` (`G7-S6`). `REANNOUNCE_INTERVAL_MS = 600_000` is written with a doc comment saying why it is legitimate |
| `PublicTableState` | **BUILD** | twenty-nine fields from `PROTOCOL.md` §6.1's table only, in order, `#[cbor(array)]`, `signed_this_hand` appended last, `absent` deleted. §6.1's normative box is unchanged. `P7` stays closed |
| `state_hash` | **BUILD** | `h("p2p-poker v1 state", [canonical_cbor(PublicTableState)])`; `Domain::State` written and green |
| the three anti-replay stores | **BUILD** | §5.2.1 is still the only definition of the key — *"§5.2.1 and only §5.2.1"* — and `src/protocol/slot.rs` implements it at one site. `N2`'s no-store-for-a-finished-hand rule is intact at §4.0 row 10a |
| the checkpoint-8 `STATE_ACK` slice | **BUILD** | bands `4 096 .. 4 105` and `8 192 .. 8 207`, both exempt from the `MAX_STAGES_PER_HAND` abort, stated at §13 l. 7005–7008 and at §4.9 l. 4091 with the failure named |
| `RetainedHand` | **BUILD** | four fields; `was_solitary` the independent disjunction, never `p == {self}`; LRU at `MAX_RETAINED_HAND_RECORDS`, now cited by name everywhere in `STATE_MACHINE.md` rather than as `4 096` |
| `CheckpointStore` / `CheckpointState` | **BUILD** | three named `Option` slots; `values: u8` stays deleted; `own` written once at the opening |
| T49 / T50 / T51 | **BUILD** | unchanged; T50's `solitary_contradicted` on `\|c.required\| == 1` (`N1`) intact |
| `PrefixOutcome` / `SolitaryDivergence`, T62, T63 | **BUILD** | both variants re-confirmed in Part 5; T62's `j <= k + 1` and T63's `tournament_winner := None` unmoved |
| §4.0 steps 12 and 12a | **BUILD** | rows 10a, 10b, 12, 12a all outside this pass's diff; §4.9's close box is still a pointer to row 10b (`G6-R5`) |
| the readmission set | **BUILD** | T67's full guard intact, `readmit` reaching no guard, cleared at step 8 |

## What is still deferred, and on what

| Item | Deferred on |
|---|---|
| §4.0 steps 13, 14, 15 | the engine — semantic re-validation and proof verification |
| `TIMEOUT_VOTE`, `TIMEOUT_CERT`, `EquivocationProof` behaviour | **`OQ-F`** — write the payload structs, wire no behaviour |
| `SHOWDOWN_MUCK` policy | **`Q-01`** |
| T64 / T65 producers | **`D-014-3`** |
| the reconciliation round's out-of-set admission | **`N1-a`**, and it must be chosen **before** T53 is written |
| `SeatStatus::Absent` and its readers | **`J-3`** |
| the reopening residual above the founder's headroom | **`G4-P3-r`** — unmoved, and correctly so |
| the `§9.4 rule N` citations outside `PROTOCOL.md` | **`G6-R7`** — narrowed this pass; `PROTOCOL.md`'s six are corrected |
| the citation sweep for the renamed series | **`G5-Q5-x`** |
| the `THREAT_MODEL.md` below-floor-founder row | **`G4-P3-e`**, fourth part — three passes unopened |
| `THREAT_MODEL.md`'s seven *"ten minutes"* sites | **`G7-S7`** |
| the complete advert, the typed `n(2)`, the units | **`G7-S9`** — constraints on the build, not blockers of it |
| §4.9's tier-2 `sequence` clause | **`G8-C1`** |
| §9.4's orbit check in cash mode | **`G8-C2`** |
| §7.2's `every_n_hands` range, and its missing ledger row | **`G8-C3`** |
| §2.6's duplicated `next_deadline_ms` mapping | **`G8-C4`** |
| §2.3's undercount and its arming-site wording | **`G8-C5`, `G8-C8`** |
| the retired grace-field name, and the floor-not-minimum test | **`G8-C6`, `G8-C7`** |

**Everything on the build list can now be written, the two held-back items
included. This is the first pass in the series with nothing blocked.** The eight
new defects are one edit each, none is on the freeze path, none is on the state
path, and **none of the eight changes a value that two clients must agree on** —
which, after `G6-R1`, `G7-S1` and `G7-S3`, is the property worth stating last.
