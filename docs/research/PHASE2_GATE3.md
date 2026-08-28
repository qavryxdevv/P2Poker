# PHASE2_GATE3.md — the final Phase 2 readiness gate

**Commission.** Seven passes have run. The sixth (`PHASE2_GATE2.md`) found no
disagreement between any two documents for the first time, and left eight new
defects H1–H8 plus two carried items R-2 and R-3. **D-012** ruled on H1 and H2 and
added a process half — *every sweep covers every document*. The editors have since
applied that ruling. This pass answers whether the rest of the deterministic engine
can now be written from `STATE_MACHINE.md` without guessing.

**Method.** Every verdict is quoted from the document as it now stands. No
specification document was edited. The D-012 sweep, the termination sweep and the
slot-key sweep were run **against the constructions themselves** — every `h("…")`
site in the corpus enumerated first, each one's inputs traced back to their source,
and the corpus's own sweep tables read afterwards to compare rather than to source
the verdict. That inversion is now standing practice for three passes and it is what
produced this pass's two blockers: **both were found by tracing an input backwards,
and one of them the corpus reports about itself in a document the previous gate
graded clean.**

**Files as read, 2026-08-28.** `STATE_MACHINE.md` 16:13, `NETWORK_STACK.md` 16:37,
`CRYPTOGRAPHY.md` 16:10, `THREAT_MODEL.md` 16:09, `CONTRIBUTING.md` 16:03,
`DEPENDENCIES.md` 16:03, `PROTOCOL.md` 16:00, `DECISIONS.md` 15:53,
`SPEC_CS.md` 09:29.

> **The timestamp finding, third iteration, and it has inverted.** For two passes
> the trailing timestamp marked the un-swept document and the survivors were inside
> it. This pass `PROTOCOL.md` is the second-oldest file in the corpus and carries
> **three occurrences of `D-012` against 27 in `NETWORK_STACK.md`, 22 in
> `STATE_MACHINE.md`, 18 in `CRYPTOGRAPHY.md` and 10 in `THREAT_MODEL.md`**. The
> owner of the wire, the chain, the roster and the genesis — the four things D-012
> is *about* — has the lowest coverage of the rule in the corpus outside
> `SPEC_CS.md`. Both of this pass's blockers are in it, and one of them is written
> down, in full, with its fix, inside `NETWORK_STACK.md` §0.5.6, where it has been
> waiting since that document was swept.

## Verdict counts

| Verdict | Count | Items |
|---|---:|---|
| **RESOLVED** | 10 | H1, H2, H3, H4, H5, H6, H7, H8, R-2, R-3 |
| **PARTIAL** | 0 | — |
| **UNRESOLVED** | 0 | — |
| **REGRESSED** | 0 | — |

**Every item from the sixth pass is closed.** That is the first clean sweep of a
prior pass's findings in seven attempts, and it includes both blockers, both
low-severity carried items, and the `56`/`57` bookkeeping error.

**Seven new defects, J1–J7. Two block.**

* **J1** is a **D-012 survivor**: `advert_hash` enters `GENESIS(0)` and `session_id`
  out of the per-receiver lobby view. It is not a new discovery — `NETWORK_STACK.md`
  §0.5.6 states it in full, names its two candidate fixes, and marks it *"a
  blocker"* assigned to `PROTOCOL.md`. `PROTOCOL.md` has not acted on it. One
  survivor blocks.
* **J2** is that **`HAND_INIT`'s required emitter set is the liveness gate, and no
  seat status removes a seat from it.** One permanently silent seat holding chips
  stalls the table *forever*, at every seat count, with `hand_id` and the blind
  level advancing and no end condition in §9.3 ever able to fire — because T46
  restores stacks on every abort, so no seat ever busts. This is the honest answer
  to the termination question this gate was asked, and it is **not** the "slow
  table" the corpus says it accepted.

**Readiness verdict: NOT-READY.** Reasons in Part 8. The gap is narrower than at any
previous gate and is one paragraph in `PROTOCOL.md` §3.1 plus one rule about
collective-stage emitter sets — but the second of those is a rule nobody has yet
written, and Part 9 states what to build while it is written.

---

# Part 1 — Per-item verdicts

## H1 — `status := Absent` derived from the accepted abort copy's `attributed` — **RESOLVED**

Deleted, which was the ranked-first correction, and the invariant whose absence
*was* the defect is added.

> `STATE_MACHINE.md` T46 (l. 1241): "**No seat's `status` changes here** — the line
> that set `Absent` from `abort.attributed` is deleted (D-012, H1); see the note
> under the seat table below and I30"

The edit is complete rather than local. `grep` for `abort.attributed` over
`STATE_MACHINE.md` returns **five hits and no live consumer**: T46's row saying it
is deleted, §5.2's note (l. 1391), §8.6's deletion list (l. 2566, 2593) and §13's
change record (l. 3376). `PROTOCOL.md` §4.10's prohibition — *"no receiver may
derive a seat's state from it"* — is now true corpus-wide with no exception, which
is exactly the property H1 said was missing.

**I30 is the part that makes this checkable rather than asserted**, and it is the
best-constructed invariant in the document. Three parts: **(a)** an *exhaustive*
list of the transitions that may assign `status` (T1/T2, T3, T58, T59, T34's
`auto_action_limit` marking, and hand-init steps 0 and 2), with the closing sentence
"**That list is exhaustive; a status assignment anywhere else is a defect**";
**(b)** the negative form — no `status` from `abort.attributed`, `abort.owed`,
`observed_by`, a `Fault` record, `deck.tokens` "or any other value two honest
receivers can hold differently"; **(c)** cross-peer agreement at a hand boundary.
Each part carries its own test instruction, and (c)'s is correct about the hard
part: "generate it directly rather than by random play — the interleaving needs one
peer to complete the terminal stage on a certificate-borne abort while another
completes it on T57's, which legal play produces only when a message is dropped."

Q7 is widened rather than left describing the narrow case: "*(Widened by H1. It
formerly read 'when the abort names nobody'; since T46 no longer marks anything, the
general question is the one that is open.)*" That is the right conduct — the
question got bigger and the document says so.

**One defect inside the fix.** I30(c)'s final clause asserts an implication that
does not hold. It is J3.

## H2 — `seat_flags` undefined — **RESOLVED**

Deleted rather than defined, in the safe direction D-012 named, and the reasoning is
generalised so it binds future additions rather than closing one field.

> `PROTOCOL.md` §3.1 (l. 721–730):
> ```
> roster_hash(k) = h("p2p-poker v1 roster",
>                    [ for each seat s in ascending seat index:
>                        u8(s) || app_public_key[s] || u64_be(stack_at_hand_start[s]) ])
> ```
> "**The roster is the ordered list of seated identities and their start-of-hand
> stacks, and nothing else. This is D-012 and it is normative.** A
> `u8(seat_flags[s])` component stood in this hash and is **deleted**."

`grep -n "seat_flags"` over all nine documents now returns **three lines, none of
them a live construction**: the deletion notice above, `DECISIONS.md` D-012's
narration of the finding, and `CRYPTOGRAPHY.md` l. 2508 recording that the field
never appeared in that document at all — "checked, not assumed", which is the D-012
process rule producing the report it is supposed to produce even when the answer is
nothing.

The section then does the harder and more valuable thing: it states the **general
rule** rather than the deletion.

> §3.1 (l. 741–753): "Anything mutable about a seat is one of exactly two things,
> and neither belongs here: … It is **established by a chained event every
> participant accepted** … Then it is already in the chain … It is a **local view**
> … Then it can differ between honest receivers, and hashing it forks the genesis.
> There is no third case. **No per-seat status, flag, presence bit or liveness
> estimate may enter `roster_hash`**, and a document or an implementation that adds
> one has a bug."

That paragraph is the reason J1 is a defect the corpus can be held to rather than an
oversight: §3.1 states the two-case test three lines above the construction that
fails it.

Two further things are done well and should not be lost. The failure mode is stated
in the terms an implementer will meet it in — "it costs them every event of the
hand, because neither will verify a single one of the other's. That failure is
total, and it is silent until the first event arrives." And `stack_at_hand_start` is
retained **with its redundancy admitted**: "By the bullets above the stack is
redundant — it *could* be dropped on the same argument — and it is retained
deliberately, as a check rather than as a binding." A retained redundancy that says
it is redundant cannot drift into being load-bearing by accident.

## H3 — §5.3's `event_type_slot` a lossy projection — **RESOLVED**

Fixed in the direction the gate ranked *second*, and the choice is argued rather
than assumed.

The gate offered two corrections: state that the slot is a projection, or index on
the full `event_type` and pay the memory. The editors took the second and then
showed the memory is not paid:

> §5.3 (l. 3097–3104): "One map per table per hand, keyed by `(stage, seat,
> event_type)` — the **whole `u16`**, exactly as §5.2.1 carries it, not a positional
> slot standing in for it"
>
> (l. 3106–3122): "**Capacity 2 is a bound on occupancy, not a component of the
> index, and the distinction is the whole of H3.** … Keying by the full `event_type`
> removes the contradiction without costing the bound, because the bound never came
> from the index in the first place: **§4.0 step 12 admits at most two `class = 0`
> events per `(stage, seat)`** … So occupancy stays at `MAX_STAGES_PER_HAND ×
> MAX_SEATS × 2` entries."

The store now agrees with the key it claims to derive from, and the section states
the failure it prevents in the terms H3 raised it — a client following the old §5.3
"would … refuse an `EquivocationProof` a conforming client built from the same pair
— the store contradicting the key it claims to index." `grep` for `event_type_slot`
over the whole corpus returns **one line**, and it is inside that paragraph,
describing what was deleted. Sparseness is preserved and its reason given ("a dense
`u16` axis would be 65 536 cells per `(stage, seat)` for a pair").

## H4 — the two indistinguishable `cause = 1` gate rows — **RESOLVED**

Collapsed, which was the ranked-first correction and the one that loses nothing.

> §4.10 (l. 2354): "| `1`, uncertified path (`attributed = []`, `cert_hash = None`)
> — the hand-deadline path and §6.3 case (b) are **one row**, see below | its
> **own** `hand_deadline_ms` has expired (§8.2), **or** it has itself reached §6.3
> case (b) | **buffer, do not reject** |"

The union is taken, as the gate's sane default said it should be, and §4.10 states
why the distinction was never real: "Both bodies carry `cause = 1`,
`attributed = []`, `cert_hash = None`" — so a receiver could not have applied two
rows to one wire body whatever the table said. The consequence is carried into
`n(1) attributed`'s and `n(2) cert_hash`'s own field rules (l. 2425–2426) rather
than left in the gate table alone, which is where an implementer reads it.

## H5 — §5.2.4's stale G2 prose — **RESOLVED**

The paragraph is gone. §5.2.4 as it now stands contains no reference to T55 or T56
and no prescription of a consuming transition; the normative box is the section's
only statement of disposition, and it reads "**Nothing consumes it.** There is no
transition, in any document, that takes an `EquivocationProof` as its input event"
(l. 3067–3070). The stricter reading `STATE_MACHINE.md` had already adopted is now
the only reading either document offers.

## H6 / R-2 — `THREAT_MODEL.md`'s `OQ-*` letters — **RESOLVED**

The last document on the old scheme has moved onto `DECISIONS.md`'s letters, and it
did the migration in the form that leaves a reader able to follow an old
cross-reference.

> §9.2 (l. 2156–2158): "**OQ-A**, **OQ-D**, **OQ-F** — and `PROTOCOL.md` §12 and
> `STATE_MACHINE.md` §11 have already adopted them. The parallel scheme that stood
> here … "

A four-row mapping table (l. 2174–2178) records where each old letter went:
`OQ-A`→closed by D-010 with the letter reassigned, `OQ-B`→`OQ-D`,
`OQ-C`→`PROTOCOL.md` `Q-08`, `OQ-D`→split between `Q-07` and `OQ-A`, `OQ-E`
unchanged and still this document's own. Both false claims the gate named are
deleted and their deletion is recorded in place (l. 2188–2190) rather than silently
performed. `OQ-E` is correctly kept as `THREAT_MODEL.md`'s own — it is referenced by
`PROTOCOL.md` Q-02 and `STATE_MACHINE.md` Q3 and defined nowhere else, which is
D-011 rule 1 working in the direction that is easy to get backwards.

## H7 — `CRYPTOGRAPHY.md` un-swept for D-011 — **RESOLVED**

The document went from **zero** occurrences of `D-011` to **24**, and the ownership
table's discipline is inverted in terms:

> l. 2276: "| `PROTOCOL.md` | **owns, and this document references by section number
> and does not reproduce (D-011 rule 1):** the `ctx` construction — `PROTOCOL.md`
> §4.5 …; the deck-index map and the no-burn rule …; the hash constructor `h` and
> the domain-string register …; the `DOMAIN_EVENT` signature prefix bytes … **This
> is a change of discipline, not of content.** Until this pass the row read *"owns,
> and this document reproduces"* … Every copy is deleted; the byte-identity claims
> that policed them are deleted with the copies, because there is nothing left to
> compare."

All four copies are gone — `grep` for a `ctx` construction in `CRYPTOGRAPHY.md`
returns nothing. What survives is what should survive: the *argument* for each
construction, explicitly separated from the construction, with the tie-break stated
— "If a construction and this document's reasoning about it ever disagree,
`PROTOCOL.md` has the construction."

The document also went from zero to **18** occurrences of `D-012` and ran its own
sweep (l. 2483–2510), finding three sites and clearing each. **The first of the
three is a real catch that no gate asked for**: `apk = Σ pk_i` is appended to
ziffle's Fiat–Shamir transcript under the label `"apk"`, so *who is in the key set*
is a hashed quantity; §2.10's line "from the next hand the absent seat is simply not
in `apk`" was safe only while every peer agreed which seat that was. Both sites are
now pinned to the chained `HAND_INIT`'s `dealt_in`, with the failure named — "every
shuffle proof in the next hand failing to verify for one honest peer." That is
D-012 applied by the document to itself, on a path outside the one the decision was
written about. It is the clearest evidence in this corpus that the process half of
D-012 pays for itself.

The sweep also reports what it found *nothing* about — `seat_flags` "never appeared
in this document at any point — checked, not assumed" — and closes with a paragraph
asserting that nothing was strengthened to pay for the edits: "not one
`Verification:` line in §2 to §10 changed."

## H8 — two surviving restatements in `STATE_MACHINE.md` — **RESOLVED**

Both, plus four more the same sweep turned up.

**(a)** §8 no longer prints `commitment_i` and `seed`. l. 2100: "`commitment_i` and
`seed` are defined in **`PROTOCOL.md` §4.4**, over the hash constructor and the
…". §13 item (l. 3560) records the replacement and why the previous *"we print a
corrected copy because a wrong copy caused C-2"* reasoning was itself the error.

**(b)** §3.2 no longer prints the ordering-buffer key. The document goes further and
files the residual against the other side (l. 3625–3639): "**§5.2 — the ordering
buffer is attributed to this document, and it is not this document's** (new with
H8)." That residual is real and is J7.

`STATE_MACHINE.md`'s change list names six sites in total for H8 — §3.2, §5.2's T9
and T10, §7.8, §7.9, §8.2 — so the sweep found four beyond the two the gate had.

**The `56`/`57` bookkeeping error is fixed.** Counted directly against the current
file: **56** `| T` rows, **30** `| **I**` rows, **20** phases, and §9.5 now reads
"20 phases, 56 live transitions, 30 …" with the four retired numbers (T8, T12, T55,
T56) listed. `BadKeyProof`'s `cause` remains a flagged named default (`cause = 2`
with the offending event in `n(3) evidence`, §8.6), correctly identified as interim
and referred to `PROTOCOL.md` — unchanged and still not a guess.

## R-3 — `CONTRIBUTING.md`'s self-inserted authority order — **RESOLVED**

Fixed in the strongest available form: the list is not corrected, it is **deleted and
replaced by a pointer**, and the file says why in the terms of the rule it broke.

> `CONTRIBUTING.md` l. 15–21: "**Authority.** `docs/DECISIONS.md` outranks this file
> and every specification document, and **this file does not restate its contents**
> — not the decisions, not their numbers, not their count. That is D-011's
> one-normative-owner rule applied to a process document, and it is applied here
> because this document failed it: the authority order that stood in this place until
> now still read *"D-001 … D-008"* four decisions after D-012 was accepted. **A list
> of D-numbers copied into a second file is precisely the copy that drifts.**"

§3's checklist is rewritten against the decisions as they now stand — it now asks
"**Can any of it differ between two honest receivers?** (D-012.)" at l. 387 — and
§2.5 carries D-012's process rule as a named project rule with its three instances.
`DEPENDENCIES.md` received the same treatment (l. 13, l. 961), including the honest
entry that its own authority line had the same defect.

**The process half of D-012 is the item that landed hardest across the corpus**, and
the evidence is that the two documents that had never been swept in seven passes
were swept, and one of them (`DEPENDENCIES.md` l. 1035) reports **nothing** for
D-012 with the reason stated: "Nothing in this document enters a state hash, a
roster hash, a chained event body or a genesis; crate versions, licences and
advisory IDs are the same for every receiver by construction." A sweep that reports
a clean result is the cheap check the rule was adopted to buy.

---

# Part 2 — The D-012 sweep

**Method.** Every hash construction in the corpus was enumerated first
(`grep -n 'h("p2p-poker' PROTOCOL.md`, 16 sites, plus `state_hash` and the domain
register's 16 rows as a cross-check that no construction exists outside §2.8's
list). Each site's inputs were then traced backwards to a source that is one of:
a protocol constant, a value fixed by a chained event every participant accepted,
or a local view. The third answer is a survivor. The transport layer was swept on
the four shapes the commission named — derived from who is connected, from delivery
order, from a timer, from a snapshot merge — against `NETWORK_STACK.md`'s own
catalogue and independently against its mechanism sections.

## 2.1 The constructions, and where each input comes from

| # | Construction | Inputs | Verdict |
|---:|---|---|---|
| 1 | `GENESIS(0)` §3.1 | `protocol_version`, `table_id`, `0`, `table_public_key`, **`advert_hash`**, `ZERO32` | **SURVIVOR — J1** |
| 2 | `GENESIS(k)` §3.1 | `protocol_version`, `table_id`, `k`, **`session_id`**, `roster_hash(k)`, `TERMINAL(k-1)` | **SURVIVOR — J1**, transitively; `session_id` contains `advert_hash` |
| 3 | `roster_hash(k)` §3.1 | seat index, `app_public_key[s]`, `stack_at_hand_start[s]` | **Clean.** `seat_flags` deleted (H2). Seat index and key ratified unanimously at `TABLE_READY`; the stack is a function of `TERMINAL(k-1)` and `HAND_INIT`'s `n(11) ledger_delta`, both chained |
| 4 | `ABORT_TERMINAL(k)` §3.1 | `protocol_version`, `table_id`, `k`, `GENESIS(k)` | **Clean in itself** — this is P3's fix and it works. Inherits J1 through `GENESIS(k)` |
| 5 | `stage_hash(s)`, single-writer §3.2 | `s`, `stage_type`, `writer_seat`, `event_hash` | **Clean** |
| 6 | `stage_hash(s)`, collective §3.2 | `s`, `stage_type`, and for each seat in **`R`**: seat, `event_hash` | **Clean.** `R` is per-type and each instance is chain-derived — see 6a below |
| 7 | terminal stage | **no `stage_hash` at all** | **Clean, and this is the strongest single edit in the corpus.** A stage that closes on the first accepted copy has no hash, so "which copy did I accept" reaches nothing |
| 8 | `event_hash` §3.2 | `body_bytes` only | **Clean.** Excludes the signature, deliberately, against Ed25519 nonce malleability |
| 9 | `session_id` §4.3 | `table_id`, **`advert_hash`**, `roster_hash(0)`, each seat's `TABLE_READY` `event_hash` | **SURVIVOR — J1** |
| 10 | `commitment_i` §4.4 | `table_id`, `session_id`, seat key, `r`, `salt` | **Clean** in its own inputs; inherits J1 |
| 11 | `seed` §4.4 | `r_1 … r_n` ascending by seat | **Clean.** `CRYPTOGRAPHY.md` §7.3's D-012 note is correct: the combine runs over the **chained committer set**, never over the reveals a peer happened to receive, and a missing reveal stalls setup rather than shortening the list |
| 12 | `ctx` §4.5 | `protocol_version`, `table_id`, `session_id`, `hand_id`, `sequence`, `shuffle_round`, `sender_public_key` | **Clean** in its own inputs — every field a constant or a chained value; inherits J1 through `session_id` |
| 13 | `index_map_hash` §4.5 | `m`, and per index the role and owner seat, from `dealt_in` and `button_position` | **Clean.** Both are `HAND_INIT` fields, chained |
| 14 | `input_deck_hash` / `final_deck_hash` §4.5 | deck bytes | **Clean** |
| 15 | `subject_digest` §4.8 | `subject_sequence`, `subject_seat`, `subject_event_type`, `parent_event_hash`, `deadline_ms`, `kind` | **Clean.** `parent_event_hash` is the stalled stage's `stage_hash`; `deadline_ms` is the parent's `next_deadline_ms`, a chained field, not a local clock read |
| 16 | `state_hash` §6.1 | `canonical_cbor(PublicTableState)` | **Clean** — 20 fields checked individually, see 2.2 |
| 17 | `password_proof` §4.3 | password, `table_id`, `join_nonce` | **Clean**, and it enters no chain |

**6a — the collective emitter sets `R`, checked one by one**, because `R` is the one
place a "who was heard" quantity could hide inside a hash:

* `TABLE_READY` — `R` = the `PLAYER_LIST` roster, ratified unanimously. Clean.
* `RNG_COMMIT` / `RNG_REVEAL` — `R` = every seated participant. Clean.
* `HAND_INIT` / `HAND_COMPLETE` — `R` = dealt-in seats plus occupied absent/sitting-out
  seats; `HAND_COMPLETE`'s is "the same set that emitted `HAND_INIT`". Both functions
  of the previous hand boundary. **Clean for D-012 — and this set is J2.**
* `DECK_INIT` / `DECK_COMMIT` / `DEAL_PRIVATE` / `BOARD_REVEAL` — `R` = `dealt_in`,
  a chained `HAND_INIT` field. Clean.
* `TIMEOUT_CERT` — `R` = `V(subject)`, and §8.3's definition is **inductive over
  completed certificates in the transcript**, with §8.5 making that checkable
  offline: "every seat missing from `V` was removed by a completed, valid certificate
  that the same transcript contains". A `V` shrunk by bare votes fails the check.
  Clean, and it is clean because D-008 already fixed the version that was not.
* `STATE_HASH` / `STATE_ACK` — `R` = the checkpoint's emitter set; a reconciliation
  round explicitly reuses "the emitter set of that checkpoint" (§4.9). Clean.
* `HAND_ABORT` — **no `R`**. That is item 7 above.

**One candidate chased and cleared, recorded so the next pass does not re-file it.**
`TIMEOUT_CERT`'s `n(1) votes` embeds **complete `SignedEvent`s, signatures included**,
and §3.2 concedes that a malicious signer can produce a second valid signature over
one body. So two honest certificate emitters could embed different signature bytes
for one vote and produce different `event_hash`es. That does **not** fork the stage:
the certificate stage is collective, `stage_hash` is taken over the *whole* set of
certificates rather than one chosen copy, each voter emits its own, and §4.8's
receiver check accepts any embedded vote that independently passes §4.0 — so both
peers hold both certificates and compute the identical `stage_hash`. The collective
form is what saves it, and it is worth recording that this is the second property
the collective-stage rule buys that §3.2 does not claim for it.

## 2.2 `PublicTableState` — every field, because it is the widest hashed struct

Twenty entries. `protocol_version`, `table_id`, `hand_id`, `checkpoint`, `roster`,
`button_position`, `sb_position`, `bb_seat`, `level`, `small_blind`, `big_blind`,
`ante`, `street`, `board`, `committed_this_round`, `committed_this_hand`,
`current_bet`, `last_full_raise`, `player_to_act`, `pots`, `deck_commitment`,
`ledger_in`, `ledger_out`, `transcript_head`, and the five `Vec<bool>` flags.

All clean, and three deserve naming:

* **`level` / `small_blind` / `big_blind`** are derived, not incremented:
  `STATE_MACHINE.md` §9.2 gives `level(h) = 1 + (h-1)/11` as a closed form over
  `hand_id`, asserted at every hand init against invariant I25, with the reason
  stated — "an incremented counter can drift between peers after a rejected or
  replayed event and a derived value cannot." **No wall clock touches the blind
  level.** This is the single most common place a poker protocol admits a
  per-receiver quantity and this corpus closed it before the rule existed.
* **The five `Vec<bool>` flags** — `folded`, `all_in`, `acted_this_round`,
  `sitting_out`, `absent` — carry §6.1's D-012 paragraph, which states the rule from
  the receiver's side and names the one that could go wrong: "A seat's presence must
  not be derived from how long that seat has been quiet at this receiver, from a
  dropped connection, from a relay status, or from any field of an accepted
  `HAND_ABORT`." §6.1 then defers *which* chained events set them to
  `STATE_MACHINE.md` and I30(a) is that list. The two halves fit.
* **`transcript_head`** is the `stage_hash` of the last completed stage, and every
  checkpoint is itself a collective stage, so every peer emitting at checkpoint `c`
  has completed the same prefix. Clean.

The exclusion list is correct and complete: wall-clock times, evaluator scores, hole
cards, PeerIds/multiaddrs/connection state/relay status ("network facts, not game
facts, and they legitimately differ per peer"), display names.

## 2.3 The transport layer

`NETWORK_STACK.md` §0.5 is a full D-012 pass and it is the strongest section on this
rule in the corpus. §0.5.2 catalogues **ten** per-receiver quantities the layer
produces and gives each a permitted destination, with every row answering "may it
enter a hash, a roster, a chained body or a genesis" with **no**. Each of the four
shapes the commission named is covered and correctly:

* **who is connected** — §0.5.2 row 1 and §0.5.5. A degraded circuit produces "a
  disconnect and **remains seated**", and §0.5.5 draws the line an implementer would
  otherwise blur: relay capacity may gate *this client's own decision to ask for a
  seat*, never anyone else's seat, "since the alternative is a roster that differs
  by who is behind which relay."
* **delivery order** — §0.5.2 row 2, permitted destinations "**nothing**", and
  `PROTOCOL.md` §5.2's own statement of the mechanism: an unchained event's position
  in the order is one "the attacker chooses, by choosing a delivery order."
* **a timer** — §0.5.4, and it is the sharpest paragraph in the section: clock tests
  are confined to unchained lobby traffic, and "**No clock-derived test may be
  applied to table-stream traffic** … If the transport ever refused a chained event
  because it looked early or late by the local clock, two honest receivers would hold
  different chains — the H1 failure in a new place, and total rather than partial."
* **a snapshot merge** — §0.5.2 row 6 (responder counts: "counting is never
  evidence") and row 7, **the merged lobby view**, whose permitted destinations are
  "display, and choosing a table to try to join", followed by the sentence that is
  this pass's blocker: "**One consequence of this row is not yet safe: §0.5.6**".

§0.5.3 handles the one transport value that legitimately sits in a chained body —
`JOIN_REQUEST`'s `peer_id` — and the argument is right: it is self-declared in a
signed event rather than measured by the receiver, the receiver's check is a local
admission test whose outcome enters nothing, and decisively it is not a component of
`roster_hash(k)` or `GENESIS(k)`. The section then generalises to a standing
prohibition on ever proposing one.

**And §0.5.6 reports the survivor.** It is quoted in J1.

## 2.4 Survivors

**One. J1 — `advert_hash` reaches `GENESIS(0)` and `session_id` out of a
per-receiver lobby view.** Full treatment in Part 7.

Everything else traces to a constant, to a chained event, or to a construction that
deliberately holds no per-receiver input. **One survivor blocks.**

---

# Part 3 — The coverage table

Nine documents, because D-012's process half says every sweep covers every document
and the count of documents is not a judgement call. The seven the authority order
governs as specification-or-process are marked ●; `SPEC_CS.md` and the research
directory are the two that are not.

| Document | D-009 | D-010 | D-011 | D-012 | Reading |
|---|---:|---:|---:|---:|---|
| ● `PROTOCOL.md` | 22 | 59 | 35 | **3** | **The finding of this table.** The owner of the wire, the chain, the roster and the genesis has the corpus's lowest live coverage of the decision that is *about* hashed inputs. Its three hits are §3.1's `seat_flags` deletion and §6.1's two on `PublicTableState`. **Both of this pass's blockers are in this document**, and J1 is a hashed input this document owns |
| ● `STATE_MACHINE.md` | 27 | 76 | 28 | 22 | Swept. I30 is the invariant D-012 produced and it is well built |
| ● `THREAT_MODEL.md` | 34 | 112 | 102 | 10 | Swept. X33 is D-012's catalogue row; §9.2's letters reconciled |
| ● `NETWORK_STACK.md` | 5 | 24 | 34 | **27** | The **highest** D-012 coverage in the corpus, correctly — it produces almost every per-receiver quantity. §0.5 is a model of what a sweep section should look like, and it is the document that found J1 |
| ● `CRYPTOGRAPHY.md` | 17 | 38 | 24 | 18 | Went from 0 → 24 on D-011 and 0 → 18 on D-012. Its own D-012 sweep found the `apk` site, which no gate had asked about |
| ● `DECISIONS.md` | 5 | 13 | 7 | **1** | **Correct and inapplicable.** D-012's single hit is its own heading. A decision record does not cite itself repeatedly; the count is the number of *other* decisions that reference it, and D-012 is the newest, so nothing downstream of it exists yet. Not a signature of the failure |
| ● `CONTRIBUTING.md` | 12 | 11 | 11 | 8 | Went from **0 on all four** to swept. R-3's fix. §2.5 now carries the process rule with its three instances |
| ● `DEPENDENCIES.md` | 11 | 4 | 8 | 10 | Went from an authority line reading "D-001 … D-008" to swept, and **reports nothing for D-012 with the reason given** (l. 1035) — the cheap check producing the null result it was adopted to buy |
| `SPEC_CS.md` | **0** | **0** | **0** | **0** | **Genuinely inapplicable, and it is the one document where that is true by construction.** It is the binding user requirement in Czech, it predates every decision, it is never edited by the project, and it outranks `DECISIONS.md`. Its §19 and §35 are what the decisions are *derived from*. A D-number appearing here would be the defect |
| `docs/research/*` | — | — | — | — | Evidence, never authority (`CONTRIBUTING.md` §2.2). Each report is a statement of the corpus as it stood on its date and must not be swept, or the record of what was fixed would be destroyed |

**No zero in this table is the signature of the failure that has happened three
times.** The two documents that carried that signature last pass —
`CRYPTOGRAPHY.md`'s zero on D-011 and `CONTRIBUTING.md`'s zero on all four — are
both closed, and `DEPENDENCIES.md`, which had never been opened in seven passes, is
swept and reports honestly.

**But the table has a new shape and it should be read.** The failure is no longer a
zero; it is `PROTOCOL.md`'s **3**. Three occurrences is not an un-swept document —
somebody made two edits — but it is not a sweep either. A document with 35
occurrences of D-011 and 3 of D-012 has been *patched* at the two sites the decision
named and not *swept* for the class the decision describes, and §3.1 of that same
document contains the two-case test that J1 fails. **The pattern has moved from
"which document was skipped" to "which decision was applied as a patch rather than
as a rule", and it now costs the same.**

---

# Part 4 — Termination, re-derived

Every phase, under total silence, derived from the transitions rather than read off
§12.1 and compared afterwards. Silence is the whole obligation: an event that
arrives is either accepted, advancing the phase, or rejected under I21 leaving the
state bit-identical, in which case the silence case applies again.

| # | Phase | Exit under total silence | Fires on | Independently confirmed |
|---:|---|---|---|---|
| 1 | `Seating` | **T4** `FormationAbandoned`, local timer, nobody named | `join_deadline_ms` from first `JOIN_ACCEPT` | ✔ no chips exist; `ledger_in == 0` |
| 2 | `AwaitingSeatRngCommit` | **T4**, guard `hand_id == 0 ∧ ledger_in == 0` | `join_deadline_ms` from `TABLE_READY` | ✔ named default, §5.2 under T4 |
| 3 | `AwaitingSeatRngReveal` | **T4** | as above | ✔ G4's row; a commitment mismatch is T11 `Rejection`, not an unseating |
| 4 | `AwaitingKeySetup` | **T57** `HandDeadlineAbort` | `hand_deadline_ms` from `TERMINAL(k−1)` | ✔ **including a `HAND_INIT` that never completes** — R-1's anchor is what puts this in scope from hand 2 |
| 5–14 | `AwaitingShuffle` … `AwaitingShowdownReveal` | **T57** | as above | ✔ |
| 15 | `Settling` | **T57** | as above | ✔ no award applied before the collective `HAND_COMPLETE` completes, so restoration is coherent |
| 16 | `HandComplete` | **T47**, derived, no external input | immediately | ✔ |
| 17 | `HandAborted` | **T46**, derived `AbortSettle`, guard `—` | immediately | ✔ |
| 18 | `Paused` | **idles**, correctly | — | ✔ no hand live, `Σ committed_hand == 0`, nothing owed |
| 19 | `TableClosed` | absorbing | — | ✔ |
| 20 | `Diverged`, hand live | **T57** — the deadline is not disarmed at T50 and phase 20 is in T57's scope | `hand_deadline_ms`, still running | ✔ |
| 20 | `Diverged`, no hand started | **T4**, guard `hand_id == 0 ∧ ledger_in == 0` | `join_deadline_ms` | ✔ reachable: checkpoint 1 sits after `TABLE_READY`, before any `HAND_INIT` |

**Every phase has a reachable exit. The result of the sixth pass holds and nothing
regressed.** The four load-bearing properties re-checked directly:

* **T57's artefact is acceptable to its own emitter** — the terminal `HAND_ABORT`
  differs from the emitter's own contribution at the stalled stage by `event_type`,
  which is in §5.2.1's key. Checked against the tuple, not against §4.11's row.
* **T57 does not wait for the silent peer** — the stage has no required emitter set,
  so the emitter's own copy closes it. Heads-up, where `|V| = 1` and every
  certificate is inert, this is the only terminus and one live peer reaches it.
* **No exit reads a certificate, a voter set or `|V|`** — T4, T45, T46, T47, T57 are
  the whole column.
* **The acceptance gate buffers rather than rejects**, and §8.2's anchor at
  `TERMINAL(k−1)` is what bounds the hold. H4's collapse of the two rows removed the
  one seam.

## The question this gate was asked: a silent seat is now dealt in every hand

**Each hand ends. The table makes no progress, ever.** Not slowly — never. The
derivation:

1. Nothing marks a seat `Absent` (I30(a)'s exhaustive list; Q7's named default). A
   seat that goes silent stays `Active`.
2. Hand init step 4: `dealt_in[s] := (status == Active)` for every seat with
   `stack > 0`. The silent seat is dealt in.
3. `HAND_INIT` is a **collective** stage. `PROTOCOL.md` §4.4: "The required emitter
   set is every seat that will be `dealt_in`, plus every occupied seat that is absent
   or sitting out and therefore posts dead money." The silent seat is required and
   does not emit. The stage never completes.
4. `hand_deadline_ms` expires — **600 000 ms, ten minutes under the preset**. T57
   fires, `cause = 1`, `attributed = []`.
5. T46: `for s in all seats: stack[s] += committed_hand[s]`. **Every stack is
   restored, including the blinds.** No chip has moved.
6. T47 runs hand init again. Step 0: nothing to apply. Step 1: `hand_id += 1`,
   blind level recomputed from the closed form. Step 2: no seat has `stack == 0`,
   so nobody busts. Step 4: `dealt_in` is identical. Step 9: `|dealt_in| ≥ 2` →
   `AwaitingKeySetup`.
7. §9.3's end conditions are evaluated at T47 and **none can ever fire**: condition 0
   needs every seat to have left; 0.5 needs `table_faulted`, which only T54 sets; 1
   needs exactly one seat with `stack > 0`, and stacks never change; 2 needs no
   `Active` seat with chips, and the silent seat is `Active`. Condition 4 is taken.
   Go to 3.

**The loop is unbounded on the only counters that can end it.** `hand_id` rises and
the blind level rises with it — `level(h) = 1 + (h-1)/11`, doubling every eleven
hands to the 50 000 cap — but the blinds are restored on every abort, so the rising
level never removes a chip from anybody. The table burns ten minutes per iteration
forever.

**And nobody can stop it from inside the protocol.** `PLAYER_SIT_OUT` and
`PLAYER_LEAVE` are **single-writer, by the seat itself** (`PROTOCOL.md` §4.10;
§4.11 rows 37 and 39: "single | the seat"). T59 consumes `PlayerSitsOut` for seat
`s`, and only `s` can produce it. So the escape both `STATE_MACHINE.md` Q7 and §12
name — "until a human sits it out (T59) or leaves (T58)" — has the **silent seat**
as its grammatical subject, and the silent seat is the one that cannot act. The only
terminating routes are the silent peer returning, or every remaining player leaving
until §9.3 condition 0 closes an empty table. That is abandonment, not recovery.

T34's `auto_action_limit` marking does not rescue it either: T34's input is a
`TimeoutCertificate{Action}` requiring `|V| ≥ 2`, and reaching a betting round
requires `HAND_INIT` to have completed — which requires the silent seat. A seat
silent from the hand boundary never lets the counter start.

**How long to make progress: no finite bound exists.** This is J2. §12's disclaimer
— "There is no bound on how long a table can be made to make no progress: a stall
costs `hand_deadline_ms` per hand and can be repeated every hand" — is *literally*
true and reads as a bound on grief. It is not: the repetition is not adversarial
choice, it is the fixed point of the machine.

---

# Part 5 — The slot key

**One site. Clean across all 39 message types.**

§5.2.1 (l. 2727–2751) is the definition, and its normative box names the four
documents that reference it and reproduce none of it. Verified independently:

| Site | Verdict |
|---|---|
| `PROTOCOL.md` §5.2.1 l. 2736–2749 | **the definition**, the 8-tuple with `subject(E)` per `event_class` |
| `PROTOCOL.md` §4.0 step 10a l. 1169 | pointer — "look up **`slot(E)` exactly as §5.2.1 defines it** — this step reproduces no part of that tuple and reads it whole" |
| `STATE_MACHINE.md` | **zero** reproductions. Three hits on `subject_seat` (l. 1013, 2365, 3574) are prose about *why* the subject is in the key, naming no other component |
| `CRYPTOGRAPHY.md`, `NETWORK_STACK.md`, `DECISIONS.md`, `CONTRIBUTING.md`, `DEPENDENCIES.md`, `SPEC_CS.md` | **zero** occurrences of `subject_seat` or `subject_digest`; nothing tuple-shaped |
| `THREAT_MODEL.md` | three hits, all classification prose (X31, §5.5 row 3) naming `subject_seat` as the fix and printing no tuple; the five- and six-field versions it used to carry are recorded as deleted |

**The one four-field tuple that is not the slot key** — the ordering buffer's
`(table_id, hand_id, sequence, previous_event_hash)` — now appears **once**, at
`PROTOCOL.md` l. 2956. `STATE_MACHINE.md` l. 3627 quotes it only inside the change
note explaining that its own copy is deleted. H8(b) landed. The residual is that
`PROTOCOL.md` cites `STATE_MACHINE.md` §3.2 as the owner of a key `STATE_MACHINE.md`
no longer contains — J7.

**The 39-type check.** Built independently and compared afterwards; the two agree on
all 39. `event_type` in the key is doing the work at rows **17** (`HAND_INIT` versus
the abort that abandons it), **18 / 21 / 23** (`DECK_INIT`, `DECK_COMMIT`,
`BOARD_REVEAL` versus the abort at the same stage), **35** (`HAND_COMPLETE` versus
T57 firing from `Settling`) and **36** (the terminal `HAND_ABORT` versus its own
emitter's contribution). Rows 25 and 26–30 are clean by rejection rather than by the
proof label, and §5.2.1 argues the trade rather than hiding it. Rows 33–34 are clean
**only** by §4.9's reconciliation-stage rule, and the table says so in the row, which
is the right place to protect a rule from an editor who does not know what it
carries.

The corpus's table now derives per type rather than per stage-kind group, and it
numbers 1–39 so a message type added without a row is visible as a gap in the
numbering. Both were the sixth pass's requests.

---

# Part 6 — Chip conservation

**Holds on every path, in both modes. D-012 touched no chip arithmetic**, and that
is checkable rather than assumed: T46's edit removed one `status` assignment and
nothing else from the row.

* **Abort — one branch, every cause, every `AbortKind`.**
  `for s in all seats: stack[s] += committed_hand[s]` (§8.6 l. 2545). "It does not
  branch on `AbortKind`, on `attributed`, on `|V|`, on the seat count or on who
  published what. **T46 applies it unconditionally.**" I27 names all eleven paths
  that reach it — T15, T16, T21, T22, T27, T41, T44, T48, T54, T57, T60 — and
  requires the assertion on each. §8.6 lists the five things the forfeiture formula
  took with it, so the deletion is checkable rather than trusted.
* **Enforced at the receiver.** §4.10 requires `n(4) deltas` all zeroes and
  `n(5) final_stacks` equal to *this receiver's own* `stack_at_hand_start` vector.
  A copy that moves a chip is rejected under §4.0 whoever signed it.
* **Settlement.** `HAND_COMPLETE`'s receiver check recomputes every field from the
  receiver's own engine — pot layering, eligible sets, winners, the clockwise
  odd-chip distribution — and requires `sum(deltas) == 0` and
  `sum(final_stacks) + sum(committed_hand) == ledger_in − ledger_out`. A mismatch is
  a stage rejection, not a §6 divergence, which is the point of the collective form.
  I6 holds exactly over the pot bands.
* **The ledger identity.** I1: `Σ stack + Σ committed_hand == ledger_in − ledger_out`.
  I28: both counters move **only** in hand-init step 0, through `HAND_INIT`'s
  `n(11) ledger_delta`, never inside a hand and never at T58. §5.3 step 0 states it
  is "the **only** place either ledger counter changes". Tournament mode: the
  right-hand side is constant after hand 1. Cash mode: non-increasing, since
  `ledger_in` is fixed at the first hand init and `ledger_out` grows as seats leave.
* **A no-evaluator-score rule guards it from the other side.** §4.10 and §6.1 both
  forbid a third-party evaluator's raw rank value from entering a hashed body,
  because "a table regeneration or a version bump would change the hash on some peers
  and manufacture false disputes." That is D-012 in a place D-012 does not mention,
  and it was there first.

**Nothing found.** Under J2's loop, conservation is trivially maintained because no
chip ever moves — which is the arithmetic working correctly and the game not.

---

# Part 7 — New defects

Ordered by severity. Continuing the letter series: **J**.

## J1 — `advert_hash` enters `GENESIS(0)` and `session_id` out of the per-receiver lobby view; the corpus reports this about itself and assigns the fix to `PROTOCOL.md`, which has not made it

**Severity: highest. Blocking. It is a D-012 survivor, and the sweep's rule is that
one survivor blocks.**

**The construction.** `PROTOCOL.md` §3.1 l. 713–719:

```
GENESIS(0) = h("p2p-poker v1 genesis",
               [ u16_be(protocol_version), table_id, u64_be(0),
                 table_public_key, advert_hash, ZERO32 ])
```

and §4.3 l. 1446–1449:

```
session_id = h("p2p-poker v1 session",
               [ table_id, advert_hash, roster_hash(0),
                 for each seat s ascending: event_hash(TABLE_READY from s) ])
```

`advert_hash` is defined at §3.1 l. 766 as "the `event_hash` of the
`LOBBY_TABLE_AD` the participants joined under", and each joiner names it for itself
in `JOIN_REQUEST` `n(0)` — **out of its own merged lobby view.**

**Why the view is per-receiver, in the strong sense.** `NETWORK_STACK.md` §0.5.6,
which found this:

> "the founder re-broadcasts every `AD_REBROADCAST_MS`, and `PROTOCOL.md` §7.2's own
> rule 6 requires each re-broadcast to carry a strictly greater `timestamp_unix_ms`
> or be discarded. So each re-broadcast is a **different signed event with a
> different `event_hash`**, and which one a joiner holds is decided by nothing but
> when it happened to be listening. Two honest players joining a minute apart
> therefore name two different `advert_hash` values, both validly signed by the
> founder, both unexpired, and both accepted."

Both halves are confirmed against `PROTOCOL.md`: §7.2 rule 6 at l. 3595–3597 is
exactly as quoted, and `AD_REBROADCAST_MS = 30_000` at l. 4701. So the divergence is
not a partition case — it is what happens on a healthy network to two players who
join thirty seconds apart. **This is the only defect in seven passes that fires on
the happy path with no adversary and no dropped message.**

**`JOIN_ACCEPT` does not converge it.** §4.3's `n(2) advert_event` echoes the
complete `SignedEvent` back, and the receiver check is that its `event_hash` "equals
the `advert_hash` **the joiner sent**". The founder echoes the joiner's own copy,
which makes the joiner's local view canonical rather than replacing it.

**`TABLE_READY` does not converge it either, and this is the gap that could be
closed in one line.** `TABLE_READY` carries `n(2) advert_hash`, and §4.4's *Receiver
must validate* list is: "the sender is in the roster at `my_seat`; `roster_hash`
recomputes; every seat's `capability_set` contains …". **It never checks that the
sender's `advert_hash` equals the receiver's own**, or that the `n` copies agree with
each other. The field is carried into a signed body and read by nothing.

**What breaks.** `TABLE_READY`'s envelope is `previous_event_hash = GENESIS(0)`, and
`GENESIS(0)` contains each peer's own `advert_hash`. Two peers holding different
copies therefore compute different `GENESIS(0)`, so §4.0's chain check rejects every
copy of the other's `TABLE_READY`, the collective stage never completes, and — since
§4.3 abandons formation at `join_deadline_ms` — the table is abandoned with no
participant able to see why. §3.1's own words for this failure mode apply verbatim:
"it costs them every event of the hand, because neither will verify a single one of
the other's. **That failure is total, and it is silent until the first event
arrives.**"

**And it does not stop at hand 0.** `session_id` is a component of **every**
`GENESIS(k)` (§4.3: "Every subsequent hand's `GENESIS(k)` contains `session_id`") and
of **`ctx`** (§4.5), the Fiat–Shamir binding handed to the deck library. So the same
per-receiver value reaches every hash in the corpus except `event_hash` itself.
`CRYPTOGRAPHY.md`'s discipline item 1 states the consequence for `ctx` without
knowing it is reachable: "Adding a field that could differ between honest receivers
would … make honest verifiers derive different challenges and reject each other's
proofs."

**J1(b) — a second, independent route through the same view, which §0.5.6 does not
name.** Nothing requires two successive `LOBBY_TABLE_AD`s for one `table_id` to carry
the *same table parameters*. §7.2's rules validate each ad's internal consistency
(`min_players_to_start ≤ max_players`, preset values matching a named preset,
`expires_at > timestamp`) but compare it against nothing but the held ad's
`timestamp`. A founder who re-signs with a different `small_blind` or `max_players`
gives two joiners two different **rule sets**, and those parameters feed
`HAND_INIT`'s `n(4) level`, `n(5) small_blind` and `n(6) big_blind` — a collective
stage whose bodies must be byte-identical. So the same per-receiver view can fork the
game rules as well as the genesis, and this route survives even if `advert_hash` is
dropped from `GENESIS(0)`.

**Why nobody saw it for seven passes.** Three reasons, and the third is the one to
carry:

1. Every previous sweep started at the hand chain. `GENESIS(0)` is the setup chain's
   genesis, before any seat, any chip and any card, and the phase-1 termination row
   for `Seating` is about a *timer*, not about a hash.
2. `NETWORK_STACK.md` **did** see it, wrote it up in full, named both candidate fixes,
   and marked it a blocker — and then correctly declined to fix it, because under
   D-011 rule 1 `GENESIS(0)` is `PROTOCOL.md`'s: "this document must not pre-empt it
   by inventing a lobby-side convergence rule that would only move the per-receiver
   quantity somewhere less visible." **The discipline worked and the hand-off did
   not.** A finding assigned across an owner boundary has no mechanism in this corpus
   that makes the receiving document answer it.
3. `PROTOCOL.md` has three occurrences of `D-012`. It was patched at the two sites the
   decision named and not swept for the class. Part 3's table is where that is
   visible.

**Correction, ranked.** §0.5.6 names both and they are both `PROTOCOL.md` §3.1's:

1. **Drop `advert_hash` from `GENESIS(0)` and from `session_id`, in favour of
   `table_public_key`, which `GENESIS(0)` already contains.** Smallest, removes the
   quantity rather than trying to make it agree, and is the move D-012 made for
   `seat_flags` — the same decision, the same document, the same section. What is
   lost is §3.1's stated property that "every participant provably joined the same
   advertised game"; that property was never true, because the ad each participant
   joined under is exactly what differs. **It should be replaced rather than mourned:
   binding the *parameters* — a hash of the agreed `(blinds, max_players, preset_id,
   deck_suite, buy-in range)` tuple, which every peer holds identically or must not
   sit down — gives the real property J1(b) needs, and it is a function of chained
   content.**
2. **Pin one copy by a chained event.** Make `PLAYER_LIST`'s `n(1) advert_hash`
   normative and add to §4.4's `TABLE_READY` receiver check that `n(2) advert_hash`
   equals it. This keeps the binding and is two sentences. It is larger than (1) only
   because it makes the founder's choice canonical, which is a small authority the
   corpus otherwise ends at `TABLE_READY`.

Either closes J1. Only (1) plus a parameter hash closes J1(b).

## J2 — `HAND_INIT`'s required emitter set is the liveness gate, no seat status removes a seat from it, and no document asks what would

**Severity: highest. Blocking — not as a fork, but because the specification's
stated cost is not the cost it has.**

**The construction.** `PROTOCOL.md` §4.4 l. 1600–1603:

> "The required emitter set is **every seat that will be `dealt_in`, plus every
> occupied seat that is absent or sitting out and therefore posts dead money.**"

**The consequence.** `dealt_in = false` does **not** remove a seat from this set.
`SittingOut` does not. `Absent` — the status this corpus spent two passes arguing
about — does not; it is named in the *inclusion* clause. The only statuses outside
the set are `Empty` (hand-init step 0, after an accepted `PLAYER_LEAVE`) and, by
implication, `Busted`, which posts no dead money because `min(BB, 0) = 0`.

**So the seat status a peer holds was never what decided whether the next hand could
start.** Three things follow, and each is worth stating separately.

**(a) H1's stated failure mode was over-attributed, and the record should say so.**
H1's mechanism was that peer `A` marks seat `C` absent and `B` does not, so "`A`
deals 2 and `B` deals 3", their `HAND_INIT` bodies differ, and the stage never
completes. The bodies do differ — that half is right, and D-012 correctly forbids
it. But the stage could not have completed at **either** peer regardless, because
both emitter sets contain `C` and `C` is silent. H1 was a genuine consensus defect —
two honest peers holding different canonical state is a defect whatever else is
true — and D-012 is the right rule. What is not right is the *cost model* the corpus
built on top of it, in `STATE_MACHINE.md` §12: "It was already the behaviour
heads-up, which is the MVP's regime; what changed is that three seats and six now
behave the same way." **Nothing changed at three seats and six either.** They already
behaved this way, because marking a seat absent never removed it from the emitter
set. The liveness the corpus believes D-012 spent was already gone.

**(b) The stall is total and permanent, not slow.** Derived in Part 4: no end
condition in §9.3 can fire, because T46 restores every stack on every abort so no
seat ever busts; `hand_id` and the blind level advance and neither is read by any
terminating condition. Ten minutes per iteration, forever.

**(c) Nobody at the table has a remedy.** `PLAYER_SIT_OUT` and `PLAYER_LEAVE` are
single-writer by the seat itself, so the escape Q7 and §12 both name is available
only to the peer that is not there. This is J4 and it is the sentence that must
change first, because it is the one an implementer would build against.

**Why nobody saw it.** Every previous pass asked "what marks a seat `Absent`?" —
which is Q7, and it is the wrong question, or rather it is the second half of the
right one. The right question is **"what removes a seat from a collective stage's
required emitter set?"**, and no document in this corpus asks it. §3.2 defines the
collective stage kind and its `R`; §4.4 instantiates `R` for `HAND_INIT`; and the
gap between "this seat takes no cards" and "this seat need not sign" is never
examined, in seven passes, by anyone.

**Correction.** This is a design decision the project owner must take, and it is not
a documentation fix. The three candidates, with what each costs:

1. **Narrow the set to `dealt_in`, and derive the dead-money seats' postings from
   the accepted copies.** `HAND_INIT`'s body already contains `n(9) stacks` over
   every occupied seat and `n(11) ledger_delta`; every field is a pure function of
   `TERMINAL(k-1)`, so a non-dealt-in seat's copy adds no information. This is the
   smallest change that restores liveness and it costs the ratification property —
   a seat that posts dead money no longer signs the hand it pays into. Given that
   §4.4's own argument for the collective form is *"a body with no choices in it must
   not give one seat a veto"*, extending the veto to seats that take no cards looks
   like the accident rather than the design.
2. **Give the corpus an agreed basis for marking a seat out**, which is Q7's open
   question, and then narrow `dealt_in` *and* the emitter set on it. This is the
   complete answer and it is the one Q7 says "needs a basis every peer already agrees
   on, which does not exist yet." A candidate does exist and the corpus already trusts
   it: **`consecutive_auto_actions` is derived from chained `TimeoutCertificate`s and
   I30(a) already admits it as a status source.** Extending the same construction to
   *stage* non-participation — a seat that is a required emitter of `N` consecutive
   collective stages that stalled, certified the way T34's is — would be D-012-clean
   by exactly the argument that admits T34. It needs `|V| ≥ 2`, so it does nothing
   heads-up, which is the MVP's regime; that limit should be stated rather than
   discovered.
3. **Accept it and say so precisely.** If the answer is that a two-player table where
   one player vanishes is simply over, that is defensible — `SPEC_CS.md` §19 ranks
   security above finishing a hand — but §12 must then say *"the table never plays
   another hand and no participant can end it"*, not *"a stall costs
   `hand_deadline_ms` per hand and can be repeated"*, and the GUI must offer the
   remaining player the one action that exists: leave.

**This is blocking not because an implementer would guess wrong about a byte, but
because they would build (3) while reading a document that describes (1).**

## J3 — I30(c) asserts an implication that does not hold, and it is the implication that hides J2

**Severity: medium. It is inside the invariant added to close H1.**

> `STATE_MACHINE.md` I30 (l. 2958), part (c): "Two peers that have accepted the same
> event prefix hold the **identical `status` vector** in `HandComplete`, hence
> identical `dealt_in` and `bb_seat` from §5.3 for hand `k+1`, **hence a `HAND_INIT`
> collective stage that completes**."

The first implication is sound. The second is not. Agreement on the body makes the
stage **completable**; completion additionally requires every member of the required
emitter set to emit, and §4.4 puts every occupied seat with chips in that set.
Identical `dealt_in` at every peer is perfectly consistent with a stage that never
completes, and under J2 that is the normal case rather than an edge one.

The same overclaim appears once more, in softer form, at §12.1's note 5: "Row 4's
exit is unchanged; what changed is that the phase it leads back to **can now be left
by something other than the deadline**." That is true of a table where every seat is
live and false of the case §12.1 exists to reason about.

**This matters beyond wording** because I30(c) carries a test instruction, and a
harness that asserts (c) as written will pass on every trace in which the stage
completes and will never be run against the trace in which it does not — so the
invariant is unfalsifiable exactly where J2 lives.

**Correction.** One clause. I30(c) should end at "identical `dealt_in` and `bb_seat`
from §5.3 for hand `k+1`", and state separately that whether the stage *completes* is
a liveness property outside this invariant's scope, pointing at §12.1. The
assertion the harness should make is that the two peers' derived `HAND_INIT` bodies
are byte-identical — which is checkable, is what (c) actually establishes, and is
strictly stronger evidence about H1 than a claim about completion.

## J4 — the escape route Q7 and §12 name does not exist for the seat that needs it

**Severity: medium. Two sentences, in the two places an implementer reads.**

> `STATE_MACHINE.md` Q7 (l. 3032): "A seat that goes silent is therefore dealt in
> again every hand and stalls each one for `hand_deadline_ms` **until a human sits it
> out (T59) or leaves (T58)**."
>
> §12 (l. 3109–3113): "The next hand deals the silent seat in and stalls again for
> `hand_deadline_ms`."

T59 consumes `PlayerSitsOut` and T58 consumes `PlayerLeft`; both arrive as
`PLAYER_SIT_OUT` / `PLAYER_LEAVE`, which §4.10 and §4.11 rows 37 and 39 make
**single-writer, emitted by the seat**. The human who can sit out the silent seat is
the silent one. No participant, and no quorum of participants, has any action that
changes that seat's status.

Q7's phrasing invites the reading that a remedy exists and is merely manual. It does
not, and the difference matters for the GUI: what the remaining player can actually
do is leave, and the client should say so rather than presenting a wait.

**Correction.** Q7 and §12 state that the only routes out are the silent peer
returning or the remaining participants leaving (§9.3 condition 0), and that a
protocol-level remedy is what Q7 is asking for. That is one clause each and it makes
Q7 an accurate statement of an open question rather than an inaccurate statement of a
closed one.

## J5 — `SeatStatus::Absent` is unreachable and five live mechanisms still read it

**Severity: low, and it is what D-012's deletion made unreachable.**

I30(a)'s exhaustive list does not contain `Absent` and Q7 confirms it: "No transition
in this document enters `SeatStatus::Absent`." The variant is dead. Still reading it:

1. `PROTOCOL.md` §6.1 — `absent: Vec<bool>` is a field of `PublicTableState` and
   therefore inside **every `state_hash`**, permanently all-false.
2. `PROTOCOL.md` §4.4 — `n(3) bb_seat` "must be an occupied, **non-absent** seat".
3. `PROTOCOL.md` §4.4 — the `HAND_INIT` emitter set's "every occupied seat that is
   **absent** or sitting out" clause.
4. `STATE_MACHINE.md` §5.3 step 4 — "`SittingOut`, `Absent` and `Busted` seats get
   `dealt_in = false`"; and steps 6–7's ante and blind posting for `Absent` seats.
5. `STATE_MACHINE.md` T59 — guard `status ∈ {Active, SittingOut, Absent}`, "a seat may
   sit back in from `Absent`", which is unreachable because nothing enters `Absent`.

None of this is currently wrong: a permanently-false vector inside a hash is
consistent across peers, so no fork follows. It is a **trap**, and §6.1 knows it — its
D-012 paragraph singles out `absent` as "the one that could be got wrong". An
implementer who finds a status the engine never sets, sees five mechanisms reading
it, and wires it to the obvious local signal has forked the chain at every
checkpoint.

**Correction.** Either retire the variant with its number recorded, as the corpus
does for T8/T12/T55/T56 — the discipline exists and is used well — or keep it and add
one sentence at its declaration saying it is currently unreachable, that this is
deliberate, and that Q7 is the question whose answer would make it reachable. The
second is smaller and preserves the field layout. Retiring it would remove
`PublicTableState.absent` from `state_hash`, which is a wire change and should not be
made for tidiness.

## J6 — `PROTOCOL.md` §4.5 asserts a reproduction that H7's fix deleted

**Severity: low.**

> `PROTOCOL.md` §4.5 (l. 1787): "`CRYPTOGRAPHY.md` §6.4 **reproduces this block** for
> the reader and does not own it."

`CRYPTOGRAPHY.md` §6.4 reproduces nothing: H7's sweep deleted both `ctx` blocks, and
the ownership table records it — "Every copy is deleted; the byte-identity claims that
policed them are deleted with the copies, because there is nothing left to compare.
… Since this pass there is also no `ctx` block here for such a change to reach."
A reader sent to §6.4 for the construction finds the argument and no construction.

This is the D-011 sweep's own wake: the deleting document updated itself and its
ownership table, and the *owner* was not told a pointer to it had gone stale. Worth
naming as a class — **when a copy is deleted, the owner's sentence describing the copy
is the second edit**, and nothing in D-011 says so.

**Correction.** One clause: "`CRYPTOGRAPHY.md` §6.4 gives the reasoning about what
`ctx` must bind and points here for the construction."

## J7 — `PROTOCOL.md` §5.2 cites `STATE_MACHINE.md` §3.2 for a key `STATE_MACHINE.md` no longer contains

**Severity: low. `STATE_MACHINE.md` filed this against `PROTOCOL.md` itself.**

> `PROTOCOL.md` §5.2 (l. 2954–2957): "`STATE_MACHINE.md` §3.2 removes network arrival
> order with an ordering buffer keyed on `(table_id, hand_id, sequence,
> previous_event_hash)`"

`STATE_MACHINE.md` §13 (l. 3625–3639) records the analysis and the hand-off: "the
corpus had the key written twice and owned nowhere, which is the shape D-011 rule 1
exists to remove. Under that rule it is the wire owner's … **The smallest fix on the
other side is for §5.2 to state the key as its own rather than cite this document for
it.**" That is exactly right and the other side has not made it.

The immediate risk is small — the key is printed once, so nothing can drift — but the
sentence sends a reader to a section that does not contain what they were sent for,
one column from a description of the replay filter, which is the M2 confusion the
corpus has already paid for once.

**Correction.** §5.2 states the ordering-buffer key as `PROTOCOL.md`'s own (it runs
in `protocol`, over four envelope fields §2.3 defines, before `step` is called) and
adds the one line `STATE_MACHINE.md` §3.2 asked for: **this is not §5.2.1's slot key.**

## Checked and found not to be defects

Recorded so the next pass does not re-file them.

* **`TIMEOUT_CERT`'s embedded vote signatures.** Chased as a chain-fork candidate;
  cleared by the collective stage form. Reasoning in Part 2.1.
* **The blind level and the tournament clock.** Derived from `hand_id` by a closed
  form asserted at every hand init (I25). No wall clock. Clean before D-012 existed.
* **`SPEC_CS.md`'s zero coverage.** Correct by construction — it predates every
  decision, outranks them, and is never edited by the project.
* **`DECISIONS.md`'s single D-012 occurrence.** It is the heading. D-012 is the newest
  decision, so nothing downstream of it exists to cite it.
* **`SPEC_CS.md` §19's "reputation penalty".** D-010 deleted every automatic
  consequence, so three of §19's four required items are defined and the fourth is
  not. This is already a **disclosed deviation** — `THREAT_MODEL.md` §9.1.1 entry 6
  names it, and OQ9 asks whether a visible abort record is a meaningful sanction when
  identity is free. Disclosed, not a new finding.
* **`BadKeyProof`'s `cause`.** Still a named default (`cause = 2` with the offending
  event in `n(3) evidence`), still flagged as interim, still referred to
  `PROTOCOL.md`. Unchanged and correctly labelled.
* **§4.3's formation timer stopping one stage early.** `STATE_MACHINE.md` §13 names
  it with a named default and shows it is safe either way, because hand 1's own
  `hand_deadline_ms` spans the interval and is five times longer. Flagged interim.

---

# Part 8 — The readiness verdict

## **NOT-READY**

**And the character of the blockage has changed again, for the better and in a way
worth recording.** Five passes were blocked by *disagreements* — two documents giving
different answers to one question. The sixth was blocked by *silences* — two
questions nothing answered. **This pass is blocked by neither.** Both blockers are
questions the corpus has already answered somewhere:

* **J1 is written down, in full, with both candidate fixes, in `NETWORK_STACK.md`
  §0.5.6, correctly assigned to `PROTOCOL.md`, and marked a blocker.** The finding
  exists. The hand-off failed.
* **J2 is derivable in four lines from two sentences in `PROTOCOL.md` §4.4 and
  `STATE_MACHINE.md` §5.3 step 4**, both of which have stood unchanged for several
  passes and neither of which is wrong. Nobody put them side by side, because for
  seven passes the question everyone asked was *what marks a seat absent* rather than
  *what excuses a seat from signing*.

### Blocking

1. **J1 — `advert_hash` reaches `GENESIS(0)` and `session_id` from a per-receiver
   view.** Name the construction: `PROTOCOL.md` §3.1 l. 713–719 and §4.3
   l. 1446–1449. Name the invariant: none — §3.1's two-case rule is normative prose,
   not an invariant, and there is no assertion anywhere that two peers derive the same
   `GENESIS(0)`.
   **Could an implementer pick a sane default?** No, and this is the sharper case of
   the two. The two candidate fixes are wire-visible and incompatible: dropping
   `advert_hash` changes `GENESIS(0)`'s input list, pinning it adds a receiver check
   to `TABLE_READY`. Two implementers choosing differently share no verifying event —
   the same total, silent failure H2 had, one link earlier in the chain. And unlike
   H2, this one fires between two conforming clients of the **same** implementation,
   because it is a property of the specification and not of the guess.

2. **J2 — no seat status removes a seat from a collective stage's required emitter
   set.** Name the transition: T47 → row 4 → T57 → T46 → T47, unbounded. Name the
   invariant: **I30(c)**, which asserts the loop cannot happen and is wrong in its
   last clause (J3).
   **Could an implementer pick a sane default?** They would not know they were
   picking one. The specification is internally consistent and buildable exactly as
   written; the engine an implementer produces from it would be *correct* and the
   table would never play a second hand once anybody's connection dropped for ten
   minutes. That is worse than a gap an implementer must fill, because nothing
   prompts them to fill it. It is blocking because the project owner has to decide
   between the three options in J7's — J2's — correction list, and that decision
   changes `PROTOCOL.md` §4.4's normative emitter set.

### An implementer would additionally have to decide these

3. **J3 — I30(c)'s final clause.** Sane default available: assert byte-identical
   `HAND_INIT` bodies, not stage completion.
4. **J4 — Q7's and §12's escape sentence.** Not a guess; it misdescribes what the
   remaining player can do, which is a GUI-visible consequence.
5. **J5 — `SeatStatus::Absent` unreachable with five live readers.** Sane default
   available: leave it unreachable, document it at the declaration.
6. **J6, J7 — two stale cross-references.** No guess required; both are one clause.
7. **`BadKeyProof`'s `cause`, §4.3's formation timer, and `Q-05` / `OQ-E` / `OQ-F`** —
   all flagged in place with named defaults or as open decisions, all correctly
   labelled interim.

### Is `STATE_MACHINE.md` implementable without guessing?

**Yes — and that is now a meaningful answer, because for the first time neither
blocker is in it.**

Both J1 and J2's normative half live in `PROTOCOL.md`. `STATE_MACHINE.md`'s 20
phases, 56 live transitions over the numbering T1–T60 with four retired, and 30
invariants were re-counted directly against the file and reconcile with §5.1, §9.5
and §10. Every declared `Event` variant has a consuming row or an explicit statement
that it deliberately has none. Every `AbortKind` maps to a `cause`. §5.3's hand-init
procedure is nine numbered steps with no ambiguity in any of them. §6's legal
actions, §7's blinds, button, minimum raise, incomplete all-in, side pots, odd chips
and showdown are specified to the level an implementer codes against, with TDA
citations attached. §12.1's termination walk is a table whose every row was
independently re-derived here and holds. I30 is the best-constructed invariant in the
document and its (a) and (b) are directly assertable after every transition.

What `STATE_MACHINE.md` cannot do is tell an implementer whether the table they build
will ever play a second hand, because that turns on `PROTOCOL.md` §4.4's emitter set,
which it correctly does not restate.

### The pattern, seventh iteration

D-011's closing rule — *if a sixth pass finds a defect of either named shape, cut the
mechanism rather than adding a rule* — was not triggered at the sixth pass and is not
triggered here. **No copy has drifted**: the restatement sweep's survivors are two
stale *pointers* (J6, J7), which is the opposite failure and a much cheaper one. **No
consequence makes an attack worth mounting**: the eviction sweep is still clean at
every layer, chips move on no abort path, and the worst an adversary buys anywhere in
this corpus is a wasted hand — or, under J2, a wasted table, which costs the attacker
their own seat too.

D-012's third shape — *the defect is on the path the previous fix newly made
load-bearing* — appears once, in J3: the invariant written to close H1 overclaims on
exactly the clause that H1's closure made load-bearing.

**But the dominant pattern of this pass is a fourth shape, and it should be named,
because the corpus now has the machinery to act on it and no rule that says to:**

> **A finding correctly assigned across an owner boundary is a finding nobody owns.**

`NETWORK_STACK.md` did everything D-011 asks: it found a defect in its own outputs,
traced it to a construction it does not own, declined to fix it locally with an
explicit statement of why a local fix would be worse, named both candidate remedies,
and marked it blocking. `PROTOCOL.md` did not answer. The same shape produced J6 and
J7 in miniature — a document deletes a copy and updates itself, and the owner's
sentence describing that copy goes stale, because D-011 rule 1 says who owns a
definition and says nothing about who maintains the sentences that point at it.

**The remedy is small and it is a process rule, not a protocol one.** D-011 and D-012
each produced one; this is the third and it is cheaper than either. A finding one
document files against another is not closed when it is written; it is closed when
the owner's document contains the edit or an explicit refusal. The corpus already has
the vehicle — `DECISIONS.md` has an open-decisions table — and needs only the
discipline that a cross-document finding goes into it rather than staying in the
finder's own §0.

The other thing to record: **D-012's process half is the single highest-return rule
in this project.** It swept two documents that had never been opened, one of them
found J1, and one of them (`DEPENDENCIES.md`) reported nothing and cost one agent one
read. Three of the last four blocking defects were in documents a previous sweep had
skipped by judgement. That rule should not be revisited.

---

# Part 9 — What to build next

**The recommendation is to start writing the deterministic engine's non-poker half
now, in a specific order, and to close J1 and J2 in parallel rather than in series.**
The reasoning is that both blockers are confined to two constructions and one
normative sentence, neither touches the layers listed below, and the layers below are
where six passes of specification have already converged.

## Close, in this order

1. **J2's decision** — the project owner picks between narrowing `HAND_INIT`'s emitter
   set to `dealt_in`, building the certified stage-non-participation marking Q7 asks
   for, or accepting the stall and saying so precisely. It is the only item in this
   report that is a design decision rather than an edit, so it should start first and
   it does not block writing code.
2. **J1** — `PROTOCOL.md` §3.1 and §4.3, one paragraph. Recommendation: **drop
   `advert_hash`, add a parameter hash.** It is the D-012-shaped answer, it is the
   same move §3.1 already made for `seat_flags`, and it closes J1(b), which pinning
   alone does not.
3. **J3, J4, J6, J7** — four clauses, in whatever pass touches those sections.
4. **J5** — one sentence at the `SeatStatus` declaration.

## Build, starting now

The poker layer is done and tested — card encoding, evaluator, pots and side pots,
legal actions and the reopening rule, blinds, action order, round completion, the
tournament preset, 58 unit tests, ~120 000 random hands with chip conservation
asserted after every action, two real engine defects already found by it. **The next
layer up is the one specified in the most detail, blocked by nothing in this report,
and required by everything else: the protocol / signed-event-log layer, `src/protocol/`.**

Concretely, in dependency order:

1. **`serialization.rs` — canonical CBOR and the canonicality gate.** §2.1's
   arrays-only rule, §2.2's encoding rules, and §2.5's **decode, re-encode, require
   byte equality — before any signature check and at every one of the three nesting
   levels**. This is the foundation everything hashes on top of, it is fully
   specified, and §2.7 already prescribes how to test it. Build it first and build the
   Phase 0 probe's assertions in as permanent tests: `h(D, ["AB","C"]) != h(D,
   ["A","BC"])`, and two domains over identical parts differ.

2. **The hash constructor and the domain register.** §2.8's `h(domain, parts)` — one
   function, `blake3::derive_key(domain, b"p2p-poker/v1")`, 8-byte big-endian length
   prefix per part — and the 16-string register as a closed enum, with the three
   retired strings as a test that asserts they are *not* constructible. That test is
   cheap and it is the one that catches a future `CRYPTOGRAPHY.md`-style divergence
   mechanically instead of by sweep.

3. **`signatures.rs`.** §2.4's `TO_BE_SIGNED`, `verify_strict` only, `hazmat` never
   enabled. Two adversarial tests the corpus asks for by name: a second valid
   signature over one body must produce **one** event (`event_hash` excludes the
   signature), and a small-order key must be refused.

4. **`transcript.rs` — the chain.** `event_hash`, both `stage_hash` forms, `GENESIS`,
   `roster_hash`, `ABORT_TERMINAL`, `TERMINAL`. **Write this against §3.1 as it stands
   and leave `advert_hash` behind a single named constructor**, so J1's fix is one
   function signature rather than a search. The stage-kind trichotomy — single-writer,
   collective, witness-independent terminal — should be a type, not a convention: the
   terminal kind having no `stage_hash` is the corpus's best single edit and it should
   be impossible to compute one for it.

5. **The slot key and `§5.3`'s three anti-replay structures.** `slot(E)` as one
   function returning the 8-tuple, called from exactly one place, mirroring D-011
   rule 2 in the code. Then §5.3's three structures — the `(stage, seat, event_type)`
   map with occupancy capacity 2 enforced by §4.0 step 12 rather than by the index
   (H3's fix), and the two per-stage subject-keyed maps. **`PROTOCOL.md` §4.11's
   39-row table is a test table**: one case per message type asserting that no honest
   emission sequence produces two events with equal keys and differing `event_hash`.
   Write all 39. Rows 17, 18, 21, 23, 35 and 36 are the ones that failed under the
   seven-tuple and are the regression tests for G1.

6. **`§4.0`'s twelve-step receiver validation**, as an ordered pipeline with the step
   numbers in the code. It is the single most referenced construction in the corpus —
   the acceptance gate, the anti-replay lookup, the stage rule and the chain check all
   name a step of it — and it is fully specified.

Then the state machine's plumbing — the ordering buffer (which J7 says is
`PROTOCOL.md`'s and should be built in `protocol`, before `step` is called), the phase
enum, and `step` as a pure function with I1–I30 assertable after every call. **I30(a)
and (b) are directly implementable today and should be `debug_assert`s from the first
transition**, because (a) is an exhaustive list and an exhaustive list is a match
statement. I30(c) needs the multi-peer harness and J3's correction first.

**What to defer.** Do not build `TIMEOUT_VOTE` / `TIMEOUT_CERT` / `EquivocationProof`
yet — `OQ-F` asks whether they should exist in the MVP at all now that D-010 gives
them no effect, and it is unanswered. Every one of them is inert heads-up, which is
the first shipped mode. Building them before `OQ-F` is answered is four passes' worth
of surface for no MVP behaviour.

**Where the next defect will be.** Not in the poker layer, and — on this pass's
evidence — not in a document that was skipped, because that class is now closed by
D-012's process half. It will be in a construction that one document reports about
another and the owner has not answered, or in a property an invariant asserts one
clause too strongly. J1 and J3 are one of each, and both were found by reading an
input backwards to its source rather than by reading a table forwards. **That method
should be the standing form of the sweep.**
