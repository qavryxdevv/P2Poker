# Phase 5 consistency gate — the D-015 sweep against the vendoring, and the code beneath both

**Date:** 2026-08-29
**Scope:** `docs/` (8 normative documents), `src/`, `tests/`, `vendor/ziffle/`,
`Cargo.toml`, `Cargo.lock`
**Method:** independent re-derivation. Nothing below is taken from
`DECISIONS.md`'s `D-015-1` or `ZR-2` rows; both were read *after* the sweep was
run, and where they disagree with the tree that is reported as a finding.

---

## 0. Verdict

| Question | Answer |
|---|---|
| Does any required path still emit, consume or wait for `TIMEOUT_VOTE`, `TIMEOUT_CERT` or `EquivocationProof`? | **No.** Zero live survivors. Three unmarked-but-inert documentation sites, listed in §1.3 |
| Do all 20 phases still terminate? | **Yes.** 21 rows, five distinct exits — T4, T46, T47, T57, T61 — none of which ever read a certificate |
| Six-handed, one player present but not deciding — does the hand proceed? | **Yes**, on that seat's **own** signed `ACTION_CHECK`/`ACTION_FOLD`, emitted by its own client on its own timer. One caveat at showdown, §3.3 |
| Silent client — does the table reach an end condition? | **Yes.** One or two `hand_deadline_ms`, then normal speed. §3.4 has the numbers |
| Is `vendor/ziffle/` byte-identical to the reviewed crate, digest verified? | **Yes**, re-verified here from the tarball and per-file. §4 |
| Does the tree build and test clean against it? | **Yes.** clippy `--all-targets` 0 warnings after a forced recompile; 189 unit + 5 + 4 integration tests, 0 failed |
| Did anything narrow the slot key? | **No.** All eight components and both subject forms intact, in the document and in the code. §5 |
| Conditions C-0 … C-15 | **5 done, 5 partial, 6 untouched.** §6 |
| New defects | **9**, one a live spec/code divergence and one a latent regression Q-01 would fire. §7 |
| Can `src/mental_poker/` be written? | **Partly.** Four of the five files are unblocked; `shuffle.rs`'s chain driver and every `ziffle::` call site remain blocked on ZR-1 (C-12). §8 |

The two parallel changes **agree** on the protocol. They disagree in one place
and it is a bookkeeping one: `DECISIONS.md`'s `ZR-2` row describes a
`src/mental_poker/` that is two commits out of date. That is finding **F-3**.

---

## 1. The D-015 sweep

### 1.1 Counts

Matching lines for
`TIMEOUT_VOTE|TIMEOUT_CERT|EquivocationProof|TimeoutVote|TimeoutCert|equivocation_proof|EQUIVOCATION`:

| File | Lines | Disposition |
|---|---:|---|
| `docs/PROTOCOL.md` | 93 | definitions kept, prohibitions kept, 3 unmarked-inert |
| `docs/STATE_MACHINE.md` | 67 | deletion records, §8.4's retained-spec block, 0 live |
| `docs/THREAT_MODEL.md` | 33 | reclassifications, 0 live |
| `docs/NETWORK_STACK.md` | 17 | **every one a prohibition** — 0 assumptions, as claimed |
| `docs/DECISIONS.md` | 9 | D-009 / D-015 text and the open list |
| `docs/CRYPTOGRAPHY.md` | 4 | §2.10's `\|V\|` branch, marked; §6.4's domain-separation aside |
| `docs/CONTRIBUTING.md` | 3 | the D-009 rule-1 checklist question |
| `docs/DEPENDENCIES.md` | 1 | the `libp2p-allow-block-list` prohibition |
| **normative total** | **227** | |
| `docs/research/*.md` | 152 | historical review records, out of scope |
| `src/` | 30 | see §1.2 |

### 1.2 Every site in `src/` — all 30 lines, classified

There are **four** files and no fifth. Nothing under `src/poker/`,
`src/security/`, `src/net/`, `src/gui/`, `src/storage/` or `src/mental_poker/`
names any of the three.

| File:line | What it is | Live? |
|---|---|---|
| `messages.rs:89-90` | `TimeoutVote = 0x0601`, `TimeoutCert = 0x0602` | definition — kept by D-015 point 3 |
| `messages.rs:158-159` | membership in `EventType::ALL` | definition |
| `messages.rs:218` | `stage_kind` — `TimeoutVote` is `Single` | classification |
| `messages.rs:224` | `stage_kind` — `TimeoutCert` is `Collective` | classification |
| `messages.rs:278-279` | `name()` → the two wire strings | display |
| `messages.rs:364` | doc on the `event_class` field | doc |
| `messages.rs:430-431` | `EventBody::expected_class` → 1, 2 | envelope cross-check |
| `messages.rs:612` | envelope test over a chained `TimeoutVote` body | test |
| `slot.rs:36,42` | `Subject::Seat` / `Subject::Digest` docs | definition |
| `slot.rs:128-129` | `subject_shape_for` → 1, 2 | definition |
| `slot.rs:264,275,291` | three key-separation tests | test |
| `signatures.rs:81,103,124` | `Domain::TimeoutCert` and its context string | definition |
| `antireplay.rs:36` | module doc stating the refusal | doc |
| `antireplay.rs:310-345` | three tests: refusal, subject separation, non-interference | test |

**Not one line emits, assembles, requires or waits for any of the three.**
The single enforcement site in the whole binary is:

```rust
// src/protocol/antireplay.rs, AntiReplay::observe
Subject::Seat(_) | Subject::Digest(_) => Err(StoreError::ClassNotProduced(slot.event_class)),
```

`EquivocationProof` appears **nowhere in `src/` at all** — no type, no field, no
constructor. `src/security/validation.rs` (D-014's adjudicator) contains no
reference to it, which is the property D-014 needed and now has by absence.

### 1.3 Survivors — three, all inert, all documentation

These are the only sites that read as a requirement without carrying a D-015
mark. Each is inert, and the reason each cannot become a wait is given.

**S-1. `PROTOCOL.md:820` — the required-emitter-set table.**
Row `| TIMEOUT_CERT | V(subject), inductive over completed certificates |
unchanged — clean … |`. This is the corpus's table of *what a stage waits for*,
which is precisely the shape the gate asks about, and every other row carries its
D-013 disposition. **Inert because the stage can never open:** a collective stage
begins when a required emitter emits, nothing emits a `TIMEOUT_CERT` (header box
point 1), and a received one is dropped before it reaches any stage machinery. A
row describing the required set of a stage that is never opened holds nothing
open. *Severity: cosmetic — but it is the one row a reader auditing waits would
stop at.*

**S-2. `PROTOCOL.md:1204` and `:1222` — the stage-kind classification.**
`TIMEOUT_CERT` is listed among the collective stages twice, unmarked. Same
argument as S-1, and `src/protocol/messages.rs:224` mirrors it faithfully.
*Severity: cosmetic.*

**S-3. `PROTOCOL.md:3874` — the terminal `HAND_ABORT` legality list.**
*"`Legal:` only on one of the terminal triggers enumerated below … a `kind = 2`
`TIMEOUT_CERT` that closed a stage, the expiry of `hand_deadline_ms`, …"* — the
certificate trigger carries no D-015 mark, while the acceptance-gate table
**eight lines below it** marks that exact path *"unreachable in version 1
(D-015) … in this version its disposition is reject"*. Two adjacent statements
about one trigger, one marked and one not. **Inert**: the acceptance gate is the
normative half and it rejects. *Severity: low, and it is exactly the adjacency
`CONTRIBUTING.md` §2.5's re-grading rule exists to catch.*

### 1.4 What the sweep got right, checked rather than assumed

* `STATE_MACHINE.md`: `Event::TimeoutCertificate` and `Event::EquivocationProof`
  are gone from §4.1; **T16, T22, T27, T34, T41, T44** deleted (as were T8, T12,
  T55, T56 in earlier passes); `certified_subjects` gone from `TableState`;
  **I29** retired. Verified by grep: `V(subject)` and `certified_subjects` appear
  in **no guard** — every remaining occurrence is inside §8.4's explicitly-boxed
  retained-specification block (from line 3784) or in a deletion record.
* `NETWORK_STACK.md`: all 17 occurrences are prohibitions. Nothing there *assumes*
  the machinery exists, so the claim of "zero assumptions" holds.
* `THREAT_MODEL.md`: X10, X30, X31 reclassified **CP** (structurally excluded);
  X7 collapsed to **DNA** uniformly; §9.1.0 item 5 carries the three costs.
* `CRYPTOGRAPHY.md`: exactly one substantive site, §2.10's `|V|` branch, edited,
  and its retained half boxed.

---

## 2. Termination, re-derived for all 20 phases

Independently re-derived from `STATE_MACHINE.md` §5.2's transition rows, not
copied from §12.1.1. Twenty phases, twenty-one rows — phase 16 splits by entry,
phase 20 by regime.

**The exits and their triggers, verified individually:**

| Exit | Trigger | Reads a certificate, `V(subject)` or `certified_subjects`? |
|---|---|---|
| **T4** | local lobby/join timer; guard `hand_id == 0 ∧ ledger_in == 0` | **no** — a local timer, reads nothing from the wire |
| **T46** | derived, immediate, `HandAborted → HandComplete` | **no** — needs no external input |
| **T47** | derived, immediate; on the settled path gated on `checkpoints.live.heard ⊇ required`, where `required` is `P(k)` from `signed_this_hand` (D-013) | **no** |
| **T57** | `HandDeadlineAbort`; guard `hand_id` matches the live hand ∧ `stalled_sequence` is a stage index of it | **no** |
| **T61** | `HandDeadlineAbort` for hand `k+1`; guard `hand_id == state.hand_id + 1` | **no** |

| # | Phase | Exit under total silence | Certificate row deleted here | Exit changed? |
|---:|---|---|---|---|
| 1 | `Seating` | **T4** → `TableClosed` | — | no |
| 2 | `AwaitingSeatRngCommit` | **T4** → `TableClosed` | T8 (earlier, `P4`) | no |
| 3 | `AwaitingSeatRngReveal` | **T4** → `TableClosed` | T12 (earlier, `P4`) | no |
| 4 | `AwaitingKeySetup` (incl. a `HAND_INIT` that never completes) | **T57** → `HandAborted` | **T16** | no — slower only |
| 5 | `AwaitingShuffle` | **T57** | **T22** | no — slower only |
| 6 | `AwaitingDeal` | **T57** | **T27** | no — slower only |
| 7 | `BettingPreFlop` | **T57** | **T34** | no — and T34's *function* is replaced, not lost (§3) |
| 8 | `AwaitingFlopReveal` | **T57** | **T41** | no — slower only |
| 9 | `BettingFlop` | **T57** | **T34** | no — as row 7 |
| 10 | `AwaitingTurnReveal` | **T57** | **T41** | no — slower only |
| 11 | `BettingTurn` | **T57** | **T34** | no — as row 7 |
| 12 | `AwaitingRiverReveal` | **T57** | **T41** | no — slower only |
| 13 | `BettingRiver` | **T57** | **T34** | no — as row 7 |
| 14 | `AwaitingShowdownReveal` | **T57** | **T44** | no — slower only. **See F-1** |
| 15 | `Settling` | **T57**, no award applied | — | no |
| 16a | `HandComplete`, entered from T46 | **T47**, derived, immediate | — | no |
| 16b | `HandComplete`, entered from T45 | **T61** → `HandAborted` → T46 → row 16a | — | no |
| 17 | `HandAborted` | **T46**, derived, immediate | — | no — this was the *destination* of five deleted rows; a destination with fewer inbound edges keeps every outbound one |
| 18 | `Paused` | idles; owes nobody anything; left by T59 on an event | — | no |
| 19 | `TableClosed` | terminal; no exit owed | — | no |
| 20a | `Diverged`, hand live | **T57** | — | no |
| 20b | `Diverged`, no hand live, `hand_id > 0` | **T61** | — | no |
| 20c | `Diverged`, no hand started | **T4** | — | no |

**The one-line argument, and why it is sound.** A `TimeoutCertificate` required
`|V(subject)|` peers to sign votes and then certificates. This column's subject is
what happens when *no event ever arrives again*. An event-triggered transition can
never be an entry in it — so deleting every certificate row cannot delete an exit.

**That argument is necessary and not sufficient, and the second half is what
actually had to be checked.** A row that is not an exit can still be a
*destination*. Five of the six deleted rows (T16, T22, T27, T41, T44) led to
`HandAborted` — row 17, which keeps every outbound edge it had; the sixth (T34)
stayed inside its own betting phase. **No path is orphaned.** What five phases
lose is *speed*: a stall that used to end in one `crypto_step_timeout_ms` (30 s)
now ends in one `hand_deadline_ms`. That cost is real, it is stated in the table
rather than absorbed, and it is the cost `PROTOCOL.md` §8.3 already charged every
`|V| < 2` table — which, heads-up being the shipped mode, was already every table
this project runs.

**Orphan check on the removed quantities.** `consecutive_auto_actions` and
`auto_action_limit` were written by T34 alone; with T34 gone their consumer — the
`SittingOut` marking at the hand boundary — is unreachable. `STATE_MACHINE.md`
§8.5 names that as the one behavioural loss. The *code* did not follow: **F-6**.

---

## 3. The away-from-keyboard case, walked end to end

### 3.1 Setup

Six-handed `CUSTOM` table, seats 0–5. Seat 3's human has walked away. Seat 3's
client is running, connected, and cooperating throughout. Everyone else plays
normally.

### 3.2 The walk, stage by stage

| Stage | What seat 3 owes | Who produces it | Human needed? |
|---|---|---|---|
| `HAND_INIT` (collective, `R = P(k-1)`) | its own derived copy | seat 3's client, automatically | no |
| `DECK_INIT` (collective, `R = dealt_in`) | its hand public key + ownership proof | seat 3's client | no |
| `SHUFFLE_STEP` / `SHUFFLE_PROOF` (single-writer) | its permutation and proof | seat 3's client | no |
| `DECK_COMMIT` (collective) | its derived copy | seat 3's client | no |
| `DEAL_PRIVATE` (collective, `R = dealt_in`) | decryption shares for the other five seats' hole cards | seat 3's client | no |
| **`BettingPreFlop`** and every later betting phase | an action when `player_to_act == 3` | **seat 3's own client**, on **its own** `action_timeout_ms` timer | **no** |
| `BOARD_REVEAL` (collective, `R = dealt_in`) | its share for each street | seat 3's client | no |
| `AwaitingShowdownReveal` | `SHOWDOWN_REVEAL` if in the required-to-show set | seat 3's client, because `showdown_policy = MANDATORY_REVEAL` makes it a rule and not a choice | no — **but see F-1** |
| `HAND_COMPLETE` (collective) | its derived copy | seat 3's client | no |
| checkpoint 8 `STATE_HASH` / `STATE_ACK` | its copies | seat 3's client | no |

### 3.3 Does the hand proceed, and on what event, from whom?

**Yes.** The only stage in the whole hand that was ever a human decision is the
betting action, and it is resolved by **an ordinary `ACTION_CHECK` or
`ACTION_FOLD` signed by seat 3's own key, emitted by seat 3's own client when
seat 3's own `action_timeout_ms` expires** (`STATE_MACHINE.md` §8.5: `Check` if
`to_call == 0`, else `Fold` — never fold a hand that could check for free,
D-006 §1). The other five peers consume it through **T29–T33**, exactly as they
consume a human's action, and it is a real entry in `history`.

Three properties are why this needs no replacement for the certificate:

1. **Single-writer by the seat that owed the action.** No vote, no voter set, no
   unanimity, no shared clock — and **no other peer can produce one**, because it
   is signed by seat 3.
2. **Every peer derives the same next state**, because the event is chained
   content like any other action.
3. **Nothing else in the hand was ever waiting on the human.** Every other
   obligation seat 3 has is cryptographic, and the client discharges it by itself.

The hand advances at `action_timeout_ms` (20 s under the rated timings), not at
`hand_deadline_ms`, and **T57 is never reached**. T34's *function* is replaced;
T34's *row* is what was deleted.

**What is lost, and it is exactly what §8.5 says:** no peer can set `was_auto`
for another seat, so `consecutive_auto_actions` never increments and seat 3 is
**never automatically marked `SittingOut`**. Its client keeps folding it and its
stack drains on the blinds instead — the same end state by a slower route, and
D-005's absent-seat behaviour unchanged.

### 3.4 The silent-client case

Seat 3's client crashes, loses its network, or deliberately emits nothing.

**Does the table reach an end condition? Yes.** The mechanism is
`hand_deadline_ms` → **T57** (or **T61** at a boundary), then D-013's
`signed_this_hand` predicate. Three sub-cases, and no trace pays for two of them:

| When seat 3 went silent | What stalls | Cost |
|---|---|---|
| **From a hand boundary** — already gone when hand `k` begins | hand `k` stalls at `HAND_INIT`; T57 aborts it; hand `k+1` skips seat 3 (it signed nothing in `k`) | **one** `hand_deadline_ms` |
| **Mid-hand** — signed `HAND_INIT(k)`, then stopped | hand `k` stalls wherever it reached and aborts; hand `k+1` still requires seat 3, because it *did* sign a chained event during `k`, and stalls at `HAND_INIT`; hand `k+2` skips it | **two** `hand_deadline_ms` |
| **After the hand completed** — signed `HAND_COMPLETE(k)`, then stopped | the *boundary* stalls, because `P(k)` contains seat 3; **T61** fires and runs hand init for `k+1`, so the next boundary is ungated; hand `k+2` skips it | **one** `hand_deadline_ms` |

A gated boundary can only follow a hand that completed, and a hand that completed
cannot also have stalled — so the worst case over all three is **two
`hand_deadline_ms`**, then the table plays at whatever speed the remaining five
play, with seat 3's stack draining on the blinds until §9.3's end condition fires.

**How long, in numbers, six-handed:**

| Configuration | `hand_deadline_ms` | Worst case (2 deadlines) |
|---|---:|---:|
| `CUSTOM` at the six-seat floor, `HAND_DEADLINE_MIN(6)` | 1 782 000 ms (29 min 42 s) | **59 min 24 s** |
| `RATED_SNG_POKERTH_V1` | 3 300 000 ms (55 min) | **1 h 50 min** |
| The protocol cap, `HAND_DEADLINE_CAP_MS` | 3 600 000 ms (60 min) | **2 h** |

`HAND_DEADLINE_MIN(6)` recomputed here from
`src/protocol/constants.rs::hand_deadline_min_ms` with the rated timings
(`hand_delay_ms = 7 000`, `crypto_step_timeout_ms = 30 000`,
`action_timeout_ms + action_grace_ms = 25 000`):
`7 000 + 35·30 000 + 24·25 000 = 1 657 000`, plus one reopening
`5·25 000 = 125 000` → **1 782 000 ms**, which agrees with `PROTOCOL.md` lines
6155 and 6214.

**These are long, and that is the deliberate trade.** `SPEC_CS.md` §19 ranks
security above finishing a hand conveniently, and the deadline has to be long
enough that an honest slow table never trips it. What D-015 changed is not the
bound — T57 was always the exit under total silence — but which *cases* reach it:
a stalled cryptographic stage now waits the whole hand deadline instead of one
crypto-step timeout.

---

## 4. The vendoring

Every check below was **re-run here**, not read out of `PROVENANCE.md`.

| Check | Method | Result |
|---|---|---|
| Tarball digest | `sha256sum ~/.cargo/registry/cache/index.crates.io-1949cf8c6b5b557f/ziffle-0.1.0.crate` | `ba79285194a16b02512566a9a64d885567646045b144bb0efeef662001cd83a5` — **matches** `PROVENANCE.md` §1 and `ZIFFLE_VERDICT.md` C-0 |
| Per-file digests | `sha256sum` over all 9 files | **all 9 match** §2.2's table, byte for byte |
| File count and size | `find … -printf '%s'` | **9 files, 120 171 bytes** — matches §2.2, and confirms §2.2's own correction to C-0's *"141 KB"* |
| Byte-identity to the reviewed crate | `diff -r --exclude=.cargo-ok <registry checkout> vendor/ziffle` | **`Only in vendor/ziffle: PROVENANCE.md`** — nothing else differs. `src/lib.rs` is at `41c7bcbb…`, the digest the review is a review *of* |
| Digest verified or copied? | **Verified.** Computed from the `.crate` on this machine; the three-way agreement (local digest, pre-vendoring `Cargo.lock` checksum, C-0's value) is recorded with the commands that produced it | pass |
| Digest still in `Cargo.lock`? | `grep ba79285 Cargo.lock` → **0 hits** | correct, and §2.1 predicts it: `[patch.crates-io]` rewrites the entry as a path dependency and drops `source`/`checksum`. **The digest's only home is `PROVENANCE.md`**, which is why §2.2's per-file table matters |
| Patch active | `Cargo.toml`'s `[patch.crates-io] ziffle = { path = "vendor/ziffle" }`; the lockfile's `ziffle` entry carries no `source` | pass |
| Build | `cargo clippy --all-targets --jobs 19` after touching `src/lib.rs` and both integration tests — a forced recompile, not a cache replay | **0 warnings, exit 0** |
| Test | `cargo test --release --jobs 19 -- --test-threads=19` | **189 unit + 5 `deck_constants` + 4 `random_hands`, 0 failed** |

**Licence.** `PROVENANCE.md` §4's finding is real and correctly handled: the
shipped `LICENSE-MIT` opens `Copyright (c) 2015 The cargo-readme Developers`, so
the MIT branch of the dual licence purports to grant under a party with no
standing in this work. Relying on the Apache-2.0 branch alone is the right call,
the text is vendored, and the instruction *not* to "fix" the MIT file is correct —
editing a third party's licence file is worse than leaving the branch we do not
rely on broken.

**One gap in the record, disclosed rather than hidden.** Nobody has checked out
`bcb8e61651cd6d4140c895a414d7ed8bc06d32bd` and compared it to these files. The
git sha1 is the publisher's own claim, written by cargo into the archive the
digest covers. `PROVENANCE.md` §1 says exactly this. No action needed; noted so
the gate is not read as attesting more than it does.

---

## 5. The slot key — nothing narrowed it

This is the property five review passes got wrong, so it is checked in both
places that define it and in the tests that guard it.

**`PROTOCOL.md` §5.2.1** — all eight components present, `subject(E)` still has
all three arms:

```
subject(E) = ()                              when event_class == 0
           = ( payload n(1) subject_seat )   when event_class == 1  (0x0601)
           = ( payload n(0) subject_digest ) when event_class == 2  (0x0602)
```

and the D-015 header box adds, in normative text: *"**Narrowing any of those
definitions is a defect**, and §5.2.1's key in particular keeps both subject
axes — five passes went wrong narrowing it, and a key that is right costs nothing
while nothing populates it."*

**`src/protocol/slot.rs`** — the code matches component for component:

| §5.2.1 component | `Slot` field | present |
|---|---|---|
| `protocol_version` | `protocol_version: u16` | yes |
| `table_id` | `table_id: [u8; 32]` | yes |
| `hand_id` | `hand_id: u64` | yes |
| `sequence` | `sequence: u64` | yes |
| `sender_public_key` | `sender_public_key: [u8; 32]` | yes |
| `event_class` | `event_class: u8` | yes |
| `event_type` | `event_type: u16` | yes |
| `subject(E)` | `subject: Subject` — `None` \| `Seat(u8)` \| `Digest([u8;32])` | **all three arms** |

`subject_shape_for` still maps `TimeoutVote → 1` and `TimeoutCert → 2`; `slot()`
still refuses a class/subject mismatch in **both** directions (a seat subject
offered to an ordinary event, *and* `Subject::None` offered to a vote); the
deliberately-absent fields (`previous_event_hash`, `payload`,
`emitted_at_unix_ms`, `next_deadline_ms`, `chain_scope`) are still absent.

**Four tests guard axes that nothing populates**, which is the correct posture:

* `slot.rs::two_timeout_subjects_at_one_sequence_are_two_slots`
* `slot.rs::two_certificate_digests_at_one_sequence_are_two_slots`
* `slot.rs::a_subject_of_the_wrong_class_is_refused`
* `antireplay.rs::the_slot_key_still_separates_two_timeout_subjects` — asserted
  **inside the store's own test module**, precisely so that removing the store's
  two classes could not quietly take the axis with them

**Confirmed: nothing narrowed the key while removing the classes.** The store
narrowed; the key did not, and `antireplay.rs`'s module doc says so in as many
words. One residual on how the *type* enforces the pairing: **F-4**.

---

## 6. `ZIFFLE_VERDICT.md` C-0 … C-15 — status

Checked against the tree, not against `ZR-2`. C-3 and C-4 were being written by
hand in parallel with this workflow; what is reported is what is in
`src/mental_poker/protocol.rs` at commit `62e9e76`.

| # | Condition | Status | Evidence |
|---|---|---|---|
| **C-0** | Vendor `ziffle` 0.1.0 with a `[patch.crates-io]` and a `PROVENANCE.md` | **DONE** | §4 — every check re-verified here |
| **C-1** | Compressed encoding only, `Validate::Yes`, ban `*_unchecked` in CI | **UNTOUCHED** | `grep -rn unchecked src/ tests/` → 0 hits, but there is **no CI grep and no test asserting it**. Vacuously satisfied today because nothing calls a deserialiser yet — which is the state C-1 exists to protect against. The pattern to copy already exists: `signatures.rs::no_retired_domain_string_appears_in_our_sources` is a grep-as-test |
| **C-2** | Exact expected byte length before parsing; re-serialise and compare | **UNTOUCHED for deck types** | `src/protocol/serialization.rs` implements exactly this discipline for *our* CBOR envelope; nothing does it for an arkworks point or a ziffle proof, and no such parser exists yet |
| **C-3** | The structural deck check, **inside** `verify_shuffle` | **DONE** | `mental_poker/protocol.rs::structural_check` — all four conditions (identity, pairwise-distinct `c1`, no carry-over from `prev`, `next != prev`) — and it is a **provided method**, so an implementor writes `verify_argument` and cannot skip it. `the_structural_check_cannot_be_skipped_by_an_implementor` proves the ordering with an always-accepting argument. Caveat **F-2** |
| **C-4** | Reject identity `hand_public_key`; reject a non-distinct key set | **DONE** | `check_key_set`, plus two tests. Caveat **F-5** |
| **C-5** | `ctx` a 32-byte length-prefixed domain-separated hash, never a concatenation, with the comment | **PARTIAL** | The *type* is done and is the strong half: `DeckCtx::from_hash(Hash)` is opaque, fixed-width, has no byte constructor, and carries the required comment about the measured proof-transfer attack. `Domain::DeckCtx = "p2p-poker v1 deck-ctx"` is in the register. **The construction site does not exist** — nothing builds a `DeckCtx` from `shuffle_round` and `sender_public_key`; every `DeckCtx` in the tree is a test literal |
| **C-6** | Chain discipline normative: fixed committed order; deal only from the last deck; a disconnect aborts | **PARTIAL** | Stated in `mental_poker/protocol.rs`'s module doc and enforced *typewise* by `Final<T>` — reveal tokens are issued and checked against `Final` decks alone. `Final::new` is `#[allow(dead_code)]`: the chain driver that would mint one does not exist, and `shuffle.rs` is a one-line stub |
| **C-7** | Freeze `open_deck` and the Pedersen key as test vectors | **DONE** | `tests/deck_constants.rs`, 5 tests, all pass. Recomputes both BLAKE3 digests from arkworks and `rand` **directly, not through ziffle**; `open_deck_is_the_deck_ziffle_deals` closes the second-copy gap by running a real one-seat shuffle through ziffle's public API and comparing all 52 recovered plaintexts. The commitment key has no such binding and the file says so rather than implying one |
| **C-8** | Establish every input before any `verify_*`; encode invalid-vs-unavailable as a type | **PARTIAL** | The type is **done, and is the load-bearing half**: `VerifyOutcome::{Invalid(InvalidReason), CouldNotVerify(Unavailable)}`, with `Unavailable::{InputDeckUnknown, AggregateKeyUnknown, ContextUnknown}` and the test `invalid_and_could_not_verify_are_different_things`. The *discipline* — establishing inputs before the call — has no call sites to apply to yet |
| **C-9** | DoS ordering and limits: length + canonicality → structural → rate limit → verify | **PARTIAL, and the missing half is a defect** | Structural-before-argument is done. **Length is not checked at all** (**F-2**); there is no canonicality gate on deck bytes, no rate limit, and no one-proof-per-peer-per-position rule |
| **C-10** | A verified token set that fails to open a card is a soundness fault, not an abort | **UNTOUCHED** | No such variant anywhere. `InvalidReason` and `Unavailable` are both the wrong home by construction — it is neither attributable nor a missing input — so it needs a third thing |
| **C-11** | Write down that a valid shuffle proof is evidence of *a* permutation, never a *random* one | **PARTIAL** | Written, in substance verbatim, in `mental_poker/protocol.rs`'s module header. C-11 asks for it *"where the shuffle chain is implemented"*; `shuffle.rs` is a stub and carries nothing |
| **C-12** | The fork decision, before the first proof is persisted or exchanged | **UNTOUCHED — open as `DECISIONS.md` ZR-1**, and it is the owner's call. Nothing in the tree pre-empts it in either direction, which is correct | |
| **C-13** | Construct `Shuffle::<52>` once per process, hold by reference | **UNTOUCHED** | `tests/deck_constants.rs` builds one per test, which is fine for a test and is not the shipped path. No production site exists |
| **C-14** | Register the fallback with its rev | **DONE** | `DEPENDENCIES.md` §11 — `paritytech/mental-poker` @ `e05744b4cc431088ec2fda769a73b067b4664893`, with the note that cargo rejects the abbreviated rev and the full sha is the only durable name |
| **C-15** | Port the adversarial probes into `tests/adversarial/` | **UNTOUCHED** | The directory does not exist. `THREAT_MODEL.md:1508` and `:1511` already cite `tests/adversarial/equivocation.rs` and `tests/adversarial/disconnect.rs` by name, so the register is ahead of the tree |

**Tally — done 5** (C-0, C-3, C-4, C-7, C-14); **partial 5** (C-5, C-6, C-8, C-9,
C-11); **untouched 6** (C-1, C-2, C-10, C-12, C-13, C-15).

**C-3 and C-4 landed as written, and in a stronger form than written.** C-3 asked
for the check *"inside `verify_shuffle` and not beside it"*. The trait makes that
structural rather than advisory by giving `verify_shuffle` a body and demanding
`verify_argument` from the implementor — and the test proves it with an
implementation whose argument always succeeds, so a structurally broken deck is
still refused. That is the strongest available form of the condition.

---

## 7. New defects, concentrated on what the removal made load-bearing

Ordered by severity. Each names its owner; per D-013, a defect belonging to
another owner goes into `DECISIONS.md`'s open list in the pass that finds it.

### F-1 — `MANDATORY_REVEAL` is now the only thing keeping a human out of phase 14 *(medium; owner: `DECISIONS.md`, against Q-01)*

D-015 deleted **T44**, the certificate row that ended a stalled
`AwaitingShowdownReveal`. §8.5's replacement — the seat's own auto-emission — is
defined **only for the action deadline**: *"When a seat's own action deadline
expires its client signs `Check` if `to_call == 0`, else `Fold`."* There is no
auto-emission rule for any other stage.

Today that is inert, and the reason is a *different* open question:
`showdown_policy = MANDATORY_REVEAL` is version 1's default and only enabled
value (`PROTOCOL.md` §4.6: under it `SHOWDOWN_MUCK` *"is never legal and is a
protocol violation"*), so `SHOWDOWN_REVEAL` is a rule, not a choice, and the
client publishes it like a decryption share.

**If Q-01 ever answers `TDA_MUCK`,** the showdown stage acquires a genuine human
decision — show or forfeit — with no auto-emission rule and, since D-015, no
certificate. Phase 14's only exit becomes T57: one whole `hand_deadline_ms` (up
to an hour, §3.4) every time an away-from-keyboard player reaches showdown, in
the one place §3's walk currently holds.

This is exactly the shape the gate looks for: **the removal made a previously
redundant answer to Q-01 load-bearing.** Q-01 must now be decided knowing that
`TDA_MUCK` requires an auto-muck (or auto-reveal) rule in §8.5 as a
*precondition*, not as a follow-up.

*Recommendation:* one row in `DECISIONS.md`'s open list against Q-01 stating the
new precondition. No code change; nothing is broken today.

### F-2 — `structural_check` has no length gate *(medium; owner: `src/mental_poker/protocol.rs`, condition C-9)*

`DeckCrypto::DECK_LEN = 52` is declared and **never read**. Neither
`verify_shuffle` nor `structural_check` checks `prev.len()`, `next.len()`, or
that the two agree:

```rust
// prev.len() == 52, next.len() == 3 — no error is produced
structural_check(&prev, &next)   // -> Ok(())
```

Three consequences:

1. **A deck that gained or lost cards passes the boundary**, leaving the check
   entirely to the library's argument — the component this whole review exists to
   distrust. `SelfContained::DeckNotAPermutation` exists in
   `src/security/validation.rs` as a tier-1 violation and **has no producer**.
2. **The loops are O(n²) in an attacker-controlled length.** With
   `TABLE_FRAME_MAX = 262 144` a frame carries roughly 3 970 ciphertexts, giving
   ~7.9 M 33-byte comparisons — before any rate limit, which C-9 also does not
   have yet. That inverts §4.0's cheap-before-expensive ordering inside the one
   function whose entire selling point is that it is cheap (0.019 ms against
   ~36 ms).
3. `Verified::new(next.to_vec())` allocates an attacker-sized vector on success,
   which `SPEC_CS.md` §17 and §27 forbid from network input.

`InvalidReason` has no variant for it either. *Fix:* one guard at the top of
`verify_shuffle` — `prev.len() == Self::DECK_LEN && next.len() == Self::DECK_LEN`
— plus an `InvalidReason::WrongDeckLength { .. }`. About six lines, and it makes
`DECK_LEN` mean something.

### F-3 — `DECISIONS.md` `ZR-2` describes a `src/mental_poker/` two commits out of date *(medium; owner: `DECISIONS.md` open list)*

**This is the inconsistency between the two parallel changes.** `ZR-2` — written
with the vendoring, still uncommitted in the working tree — says:

> *"**C-1 to C-6, C-8 to C-13 and C-15 are not started**, and `src/mental_poker/`
> is still five stub files; C-3's three-line structural deck check is the
> highest-value item in that set"*

Commit `62e9e76`, **already in HEAD**, lands 517 lines of
`src/mental_poker/protocol.rs` containing C-3 in full, C-4 in full, C-8's type,
C-11's statement and C-5's `DeckCtx` type. The tree is four stub files plus one
substantial module, not five stubs.

The failure mode is the ordinary one: `ZR-2` was drafted against the tree as it
stood when the vendoring began, and the Phase 5 commit landed underneath it. It
is worth fixing rather than ignoring, because `ZR-2`'s stated purpose is *"the
count is written down rather than left to be inferred"* — and a count that is
wrong is worse than no count. Note that `ZR-2`'s C-0/C-7/C-14 half is correct;
only the C-3/C-4/C-8/C-11 half is stale.

*Fix:* restate `ZR-2` from §6 of this document.

### F-4 — `Slot`'s class↔subject invariant is a function's postcondition, not a type invariant *(low; owner: `src/protocol/slot.rs`)*

`slot()` refuses `SubjectClassMismatch` in both directions, and that check is what
D-015's enforcement leans on: `AntiReplay::observe` dispatches on
`slot.subject`, so an event whose `event_type` is `0x0601` but whose `subject` is
`Subject::None` would be **recorded in the ordinary map** under key
`(sequence, seat, 0x0601)` rather than refused.

`slot()` makes that unconstructible — but every field of `Slot` is `pub`, so any
module in the crate can build one directly, as `antireplay.rs`'s own test helper
does. Not reachable today (one production path, no other constructor). It becomes
reachable the moment a second call site is written, which is the next thing Phase
4 will do.

*Fix:* private fields plus accessors, or have `observe` dispatch on
`subject_shape_for(slot.typed())` and cross-check `slot.subject` against it,
rather than trusting the pairing it was handed.

### F-5 — the "library-independent" structural check decodes arkworks' flag byte *(low; owner: `src/mental_poker/protocol.rs`)*

The module doc claims the boundary *"compares and hashes them and never
interprets them, so the structural check is independent of which library is
underneath."* It is not:

```rust
c1.iter().all(|&b| b == 0) || (c1[32] & 0x40) != 0     // is_identity_c1
key.iter().all(|&b| b == 0) || (key[32] & 0x40) != 0   // check_key_set
```

`0x40` is `ark_serialize`'s `SWFlags` infinity bit in the trailing flag byte of a
compressed short-Weierstrass point, and `Ciphertext = [u8; 66]` is 33 + 33 —
secp256k1 compressed, ziffle's encoding. The designated fallback (C-14,
`paritytech/mental-poker`) does not use that encoding, so a swap silently turns
both identity checks into no-ops — and they are C-3's and C-4's whole substance.

The code is *correct for ziffle*; the doc claim is what is wrong, and an
overclaimed boundary is how a swap ships broken. *Fix:* one sentence naming these
two predicates as the single library-specific part of the module, to be
re-derived on any swap — plus, ideally, a named `const IDENTITY_FLAG: u8 = 0x40`.

Secondary, in the same function: only `c1` is tested for the identity; `c2`
(`ct[33..66]`) is never examined. That is defensible — the plaintext-leak attack
is `r = 0`, which shows in `c1` — but it is not stated, and the doc's *"No
position is the identity"* reads as covering both coordinates.

### F-6 — `MAX_CONSECUTIVE_AUTO_ACTIONS` is orphaned in the code with no note *(low; owner: `src/protocol/constants.rs`)*

`src/protocol/constants.rs:74` — `pub const MAX_CONSECUTIVE_AUTO_ACTIONS: u8 = 3;`
— has **zero consumers** in the crate and **no doc comment**, alone among its
neighbours. `STATE_MACHINE.md` §8.5 states that `auto_action_limit` *"is
therefore not reached in version 1"*, because no peer can set `was_auto` for
another seat. The document recorded the orphaning; the code did not follow.

§12.1.1 explicitly lists `consecutive_auto_actions` among the quantities whose
consumers had to be re-derived, so this is a miss inside the sweep's own third
check — see the process note in §8.3.

*Fix:* a doc comment marking it not-produced-in-version-1 (D-015). **Not**
deletion, on the same reasoning that keeps the wire codes.

### F-7 — the code enforces D-015 four steps later than the specification promises *(low; owner: `src/`, Phase 4 work)*

`PROTOCOL.md`'s header box, point 2: a `SignedEvent` whose `event_type` is
`0x0601` or `0x0602` is *"**rejected at §4.0 step 6** … **before its signature is
verified, before its body is decoded, and before any store is touched**."*

In the tree, `EventBody::check_envelope` accepts such an event **cleanly** — it
validates `event_class == 1` as *correct* for a `TimeoutVote`, which by
definition it is. The only refusal anywhere in the binary is
`AntiReplay::observe`, which sits at **step 10a** — after step 9's signature
verification, the expensive step the header box promises to stay in front of.

This is a spec/code divergence and not a hole: nothing is accepted, nothing
allocates (`a_timeout_class_is_refused_and_allocates_nothing` proves the store
stays empty), and D-015's memory result is fully realised. What is missing is the
cheap early drop. Step 6 is unwritten code — `NEXT.md` item 3 lists §4.0's
validation steps as still outstanding — so this is **a note for whoever writes
step 6**, not a regression.

### F-8 — `PROTOCOL.md` §4.0's step-6 row does not itself name the two types *(low; owner: `PROTOCOL.md`)*

Step 6 reads *"`event_type` is known **and** legal on this channel | drop"*.
`0x0601` and `0x0602` **are** known — they are in §4.11's catalogue — and §4.11
lists them on the table mesh; those rows say only *"Required emitter: none in
version 1"*, which is a statement about emitters, not about legality on receipt.
The disposition that makes them illegal lives **only** in the header box.

An implementer reading §4.0's table alone — and D-011 makes §4.0 the owner of the
pipeline — finds nothing that drops them. Three other sites (`:3067`, `:4985`,
`:6828`) already cite "step 6" as though the row said so. *Fix:* one clause in the
step-6 row, or a `Legal on receipt in v1` column in §4.11's table. Same shape as
`G4-P5`'s finding: a rule stated in one place and relied on from four.

### F-9 — `Verified`/`Final` are `pub(crate)`-mintable while the doc says "only this module" *(low; owner: `src/mental_poker/protocol.rs`)*

> *"Private field, no public constructor and no deserialisation: **the only way to
> hold one is to have run the verification in this module.**"* … *"Only this
> module may mint one."*

`Verified::new` and `Final::new` are `pub(crate)`. Any module anywhere in the
crate can mint a `Verified<Vec<Ciphertext>>` without verifying anything — the GUI
could. The private tuple field stops construction from *outside the crate*, which
is a much weaker statement than the one the doc makes, and it is C-8's typestate
that rests on it.

`pub(crate)` is probably deliberate, so the sibling `shuffle.rs` and `reveal.rs`
can mint after their own checks. If so, the doc should say *"this crate"* and name
which modules are permitted; if not, the constructors should be private and the
sibling paths should route through this module. Either way the code and the claim
disagree today, and `a_verified_deck_can_only_come_from_verification` does not
test the claim — it cannot, without a compile-fail test.

---

## 8. The build verdict for `src/mental_poker/`

### 8.1 What can be written now

| File | Verdict | Why |
|---|---|---|
| `protocol.rs` | **written, and it is the load-bearing one** | C-3, C-4, C-8's type, C-11's statement and C-5's type are in. Two edits owed: **F-2** (the length gate) and **F-5**/**F-9** (narrow the doc claims to what the code enforces) |
| `deck.rs` — the masked deck and the deal map fixed before the shuffle | **unblocked** | The deal map is `PROTOCOL.md` §4.5's, it names no library type, and `tests/deck_constants.rs` has already frozen what card index 17 *is* (C-7). Writing it persists and exchanges no proof, so it does not pre-empt C-12 |
| `proofs.rs` — what binds a proof to this hand at this table | **unblocked for the binding half** | `DeckCtx` exists and `Domain::DeckCtx` is registered. C-5's construction site — `h("p2p-poker v1 deck-ctx", [version, table_id, hand_id, shuffle_round, sender_public_key])` — can be written now and closes C-5. The *verification* half is blocked with `shuffle.rs` |
| `reveal.rs` — decryption shares and street gating | **partly unblocked** | The street-gating and entitlement rules are `CRYPTOGRAPHY.md` §2.7/§2.8's and need no library. The token type and its verification are blocked: **C-12(b) changes exactly this wire object** — whether `RevealTokenProof` absorbs the whole ciphertext or only `c1` is the fork's second item, and it is defect **B1**, the review's most serious |
| `shuffle.rs` — the chain driver | **blocked** | It is the module that mints `Final<T>`, drives `verify_shuffle`, and is therefore the first thing that persists or exchanges a proof |

### 8.2 What is still blocked, and on which condition

| Blocked | Condition | Why it cannot be pre-empted |
|---|---|---|
| **Every `ziffle::` call site outside `tests/`** | **C-12** (`DECISIONS.md` **ZR-1**) | *"The wire format is frozen by the first proof that is persisted or exchanged."* C-12(a) changes `Transcript`'s length framing — `usize::to_be_bytes()`, so a 32-bit or wasm client derives different challenges from the same proof and the table splits into two mutually unverifiable halves. C-12(b) changes `RevealTokenProof`'s challenge input. Both are wire breaks. Writing the call sites first is choosing *no fork* by default |
| `shuffle.rs`'s chain driver — `Final::new`'s only caller | **C-12**, then **C-6** | It persists and exchanges proofs by definition |
| The reveal-token type and its verification | **C-12(b)** | The fork's second item *is* this object |
| Any deck deserialiser | **C-1**, **C-2** | Neither the `*_unchecked` ban nor the re-serialise-and-compare gate exists. C-2 is cheapest written **with** the first parser and expensive to retrofit |
| Rate limiting, one-proof-per-peer-per-position | **C-9** | Needs the receiver pipeline (§4.0 steps 12–14), which is Phase 4 work |
| The soundness-fault path | **C-10** | Needs a third outcome kind that is neither `Invalid` nor `CouldNotVerify` |
| `tests/adversarial/` | **C-15** | Needs the wrapper it tests |

### 8.3 The recommended order

1. **F-2** — the length gate in `verify_shuffle`. Six lines; closes the one
   demonstrated DoS in the code as it stands, and makes `DECK_LEN` mean something.
2. **F-5, F-9, F-6, F-3** — four doc/comment corrections, no behaviour change.
   F-3 is the one that keeps the corpus honest about its own state.
3. **C-1's CI grep**, copying
   `signatures.rs::no_retired_domain_string_appears_in_our_sources`. Ten lines,
   and worth strictly more *before* the first deserialiser exists than after.
4. **C-5's construction site** in `proofs.rs`, and `deck.rs`'s deal map. Both are
   ours, neither touches ziffle, neither freezes a wire format.
5. **ZR-1 / C-12 — the owner's decision.** Everything after step 4 waits on it.
6. Then `shuffle.rs`, `reveal.rs`'s token half, C-9's ordering, C-10, C-15.

**One process note.** The single thing this gate found that the sweep's own checks
should have caught is **F-6**. §12.1.1's third obligation is *"list the consumers
of every check this pass removed, including consumers the pass itself added"*;
`consecutive_auto_actions` is named in that very list; and the constant survived
in the code anyway. The obligation was discharged against the *document* and not
against `src/`. Worth extending the rule's wording to name both trees — from this
pass onward there is always code to sweep, and a rule written when there was not
will keep missing it.
