# PHASE0_FIXPLAN.md — the single ruling for all 30 findings of `PHASE0_REVIEW.md`

**Purpose.** `PHASE0_REVIEW.md` raises 30 findings against the five specification
documents. Eleven of them are contradictions *between* documents and several of
the A findings touch three or four documents at once. If editors work in parallel
without one ruling, two of them will resolve the same contradiction in opposite
directions and the corpus will still be inconsistent.

This document decides, once, what the canonical answer is. It is not a summary of
the review and it is not evidence; it is an instruction sheet. An editor applying
a finding **must not re-derive the ruling** — apply the per-document instructions
literally, and if they appear wrong, raise it here rather than deviating in one
document.

**Precedence used throughout.**

1. `SPEC_CS.md` — binding, 36 sections.
2. `DECISIONS.md` — D-001 … D-007. D-007 corrects D-006 and wins over it.
3. Where 1 and 2 do not settle a question, **the option that makes the weaker,
   more defensible claim wins**, and the entry says so explicitly. `SPEC_CS.md`
   §18 and §36 and the closing paragraph forbid smoothing over a gap, inventing a
   construction to make a gap look closed, or claiming the system prevents all
   cheating.
4. Where a ruling would change chip movement or reverse a numbered decision, the
   ruling states the **safe default** and escalates the choice to the project
   owner as a numbered `OPEN QUESTION`. An editor never silently reverses a D-nnn.

**Counts.** 30 rulings. 12 leave an OPEN QUESTION. Section 0 carries four
cross-cutting rulings that several findings share; every finding entry that
depends on one points at it.

---

## 0. Cross-cutting rulings

These are referenced by finding entries below. They are stated once here so no
editor invents a second version.

### 0.1 The envelope gains two fields: `chain_scope` and `event_class`

Every `SignedEvent` envelope carries, in addition to the fields it carries today:

| Field | Type | Values |
|---|---|---|
| `chain_scope` | `u8` | `1` = the event occupies a stage slot in a hand chain or in the setup chain; `0` = unchained (handshake, join, lobby) |
| `event_class` | `u8` | `0` = ordinary chain event; `1` = `TIMEOUT_VOTE`; `2` = `TIMEOUT_CERT`. Meaningless and always `0` when `chain_scope == 0`. |

An **unchained** event (`chain_scope = 0`) must carry
`table_id = ZERO32`, `hand_id = 0xFFFF_FFFF_FFFF_FFFF`,
`previous_event_hash = ZERO32`, `sequence = 0`. The table it concerns, where it
concerns one, is named **in the payload**, never in the envelope.

Unchained message types, exhaustively: `HELLO`, `CAPABILITIES`, `JOIN_REQUEST`,
`JOIN_ACCEPT`, `JOIN_REJECT`, `PLAYER_LIST`, `LOBBY_TABLE_AD`,
`LOBBY_TABLE_REMOVE`, `LOBBY_PLAYER_PRESENCE`, `LOBBY_SNAPSHOT_REQUEST`,
`LOBBY_SNAPSHOT_RESPONSE`, `LOBBY_CHAT`. Everything else is chained.

This is the mechanism behind A-4 and A-5. It is not optional for either.

### 0.2 The stage-kind principle

> **A stage whose body is a pure function of the state before it is *collective*.
> A stage whose body carries a choice its emitter is entitled to make is
> *single-writer*.**

This principle is normative and settles C-5, C-10 and `STATE_MACHINE.md` Q2
together. Applying it:

| Stage | Kind after this plan | Why |
|---|---|---|
| `HAND_INIT`, `HAND_COMPLETE`, `HAND_ABORT` | **collective** (changed) | every field is derived; a single writer would be an authority over the hand-to-hand transition, which `SPEC_CS.md` §4 forbids |
| `TIMEOUT_CERT` | **collective** (changed) | derived from the votes; see C-10 |
| `SHUFFLE_STEP`, `SHUFFLE_PROOF`, every `ACTION_*` | single-writer (unchanged) | the permutation and the betting decision are genuine choices |
| `PLAYER_SIT_OUT` (voluntary), `PLAYER_SIT_IN`, `PLAYER_LEAVE` | single-writer (unchanged) | a genuine choice by that seat |
| `TABLE_READY`, `RNG_COMMIT`, `RNG_REVEAL`, `DECK_INIT`, `DECK_COMMIT`, `DEAL_PRIVATE`, `BOARD_REVEAL`, `SHOWDOWN_REVEAL`, `STATE_HASH`, `STATE_ACK` | collective (unchanged) | |

A collective derived stage costs `n` copies of a small message instead of one and
removes the writer's veto over progress. That is the trade, and it is taken.

### 0.3 Heads-up (`n = 2`) has no deadline agreement mechanism at all

D-007 is binding: at two seats, an action deadline is advisory and a fold-effect
timeout certificate is forbidden. This plan **extends the same reasoning to
`kind = 2`** (the cryptographic-step deadline), because the underlying result
D-007 states — *two peers with no trusted clock and no third party cannot agree
that a deadline passed* — does not depend on what the certificate does afterwards,
and a `kind = 2` certificate is strictly more valuable to an attacker than a
`kind = 1` one: it aborts the hand, attributes the victim, and forfeits the
victim's committed chips under D-005.

**Ruling.** At `n = 2`:

* no `kind = 1` certificate exists (D-007 point 1);
* a `kind = 2` certificate may still end the hand — liveness requires it, or a
  vanished opponent deadlocks the table forever — but it carries
  **`attributed = []`** and produces **no forfeiture**; stacks return to their
  start-of-hand values.

This reopens, at two seats only, the rage-quit escape D-005 closed. That is
stated as an unfixed limitation rather than hidden. The alternative — letting one
peer's unilateral assertion take the other's chips — is a positive-gain attack on
an honest player, and this plan prefers a liveness/fairness loss to a theft. The
choice between the two is escalated: **OQ-A (below)**, and it needs a numbered
owner decision (proposed `D-008`) before real money is ever considered.

### 0.4 The canonical shared-constant table

This is the C-1 ruling and it is reproduced here because five other findings
quote a value from it. See **C-1** for the per-document instructions.

---

## A findings — false or overstated security claims

### A-1 — Heads-up, the "unanimous" timeout certificate is one signature, so a modified opponent can fold any hand at will

**Ruling.** D-007 settles it and is applied verbatim; §0.3 extends it to
`kind = 2`. There is no construction to be found here, and `SPEC_CS.md` §36
forbids inventing one. The certificate stands unchanged at `n >= 3` and does not
exist as a state transition at `n = 2`.

The review's option (ii) — require the subject's own counter-signature — is
**rejected**: a subject who is absent cannot counter-sign, so it converts every
genuine timeout into a deadlock, and D-007 already recorded that no arrangement of
signatures gives both properties.

**`PROTOCOL.md`**

* §8.3: delete the sentence *"A timeout takes effect only through a **unanimous**
  certificate"* as a general claim and replace it with: the certificate is
  defined for `n >= 3`; at `n = 2` the required voter set has exactly one member,
  which is the opponent, so unanimity is vacuous. State that
  `V(subject)` is **"every other dealt-in seat"**, `|V| = n - 1`, and print the
  table `n = 2 -> |V| = 1`, `n = 3 -> |V| = 2`, `n = 10 -> |V| = 9`.
* §8.3: add a normative rule — *`kind = 1` certificates are invalid at `n = 2`
  and must be rejected by every receiver. A heads-up action deadline is a UI
  countdown and produces no signed state transition (D-007 point 1).*
* §8.3: add a normative rule — *at `n = 2` a `kind = 2` certificate is accepted
  only with `attributed = []`; a `kind = 2` certificate naming a subject at
  `n = 2` is invalid.* Cross-reference §0.3 of this plan and OQ-A.
* §4.10 `HAND_ABORT`: `attributed` may be empty for `cause = 4` **and for
  `cause = 1` when `n = 2`**. The `cert_hash` requirement for `cause = 1` stands.
* §6.3 step 4 case (b): remove *"via the §8 certificate machinery"*. Replace the
  whole case with: *the hand aborts with `cause = 1` and `attributed = []`. The
  transcript records which peers were asked for which events and did not serve
  them, which is publicly readable but is not adjudicable inside the protocol,
  because any adjudication that requires the accused peer's signature is circular
  at every table size (D-007 point 4).* Cite OQ-B.
* §12: Q-02 stays open; add OQ-A and OQ-B (below) to the table.

**`THREAT_MODEL.md`**

* §5.3 X10: retitle to *"A forged timeout certificate by all other dealt-in
  seats"*. Delete "a malicious majority" everywhere in the row. State the
  requirement as **all other dealt-in seats — one seat at `n = 2`, two at
  `n = 3`** and add: *at `n = 2` the mechanism gives no protection at all and is
  therefore not used; D-007 makes the heads-up deadline advisory.* Class stays
  **DNA** for `n >= 3`.
* §5.3: X11 is merged into X10 — see A-2.
* §9.1 limitation 3: rewrite. Remove *"Effect is bounded to an auto check/fold"*.
  New text: *At `n >= 3` a coalition of all the other dealt-in seats can steal
  one honest player's action through a certificate that is valid by construction;
  it is detectable and not adjudicable without a trusted clock. At `n = 2` no
  action deadline is enforceable at all: a stalling opponent cannot be punished
  and the only remedy is to leave the table (D-007). Neither case is solved.*
* §9.1: add a new limitation — *at `n = 2` a hand that stalls on a missing
  cryptographic contribution aborts without attribution and with stacks restored,
  so a heads-up player can escape a losing pot by going silent. Closing this
  requires a mechanism that does not exist (OQ-A).*
* §2 A13: the assumption's text asserts that time enters only as a certificate
  signed by every other still-active player. Add: *at `n = 2` this reduces to one
  signer and the assumption does not hold; see D-007.*
* §9.2: add OQ-A and OQ-B.

**`STATE_MACHINE.md`**

* §8.4 validity rule 4: keep `signers == deck.participants \ {subject}` and add
  rule 6 — *if `|deck.participants| == 2`, a certificate with
  `kind == ActionDeadline` is rejected, and a certificate with
  `kind == CryptoDeadline` is accepted only if it names no subject for
  attribution.* T34 must therefore not be reachable at two seats.
* §8.4 "Why unanimity, and what it buys": the paragraph is false at `n = 2`.
  Replace with the `|V| = n - 1` table and a pointer to D-007.
* §8.5: add — *at two seats there is no action-timeout certificate, so
  `consecutive_auto_actions` never increments from a timeout and a heads-up seat
  is never marked sitting out by the deadline path. It can still sit out
  voluntarily.*
* §9.5: add the heads-up consequence to the "§32 heads-up first" section: the
  first shipped mode is the one where the deadline machinery does not apply.
* §11: Q3 stays open; add a pointer to OQ-A.

**`CRYPTOGRAPHY.md`**

* §2.8 item 2 and §2.10: the sentences that say a timeout certificate supplies
  the auto check/fold must add *"at `n >= 3`; heads-up the deadline is advisory
  (D-007)"*. No other change.

**`NETWORK_STACK.md`**

* §8.4 last bullet ("Connection loss is a hint, not a verdict") says the absent
  decision is made by "the D-006 timeout certificate signed by the other
  still-active players". Add *"— which at two seats is a single player, so
  D-007 makes it advisory; the transport layer's behaviour is unchanged either
  way"*.
* §15 decision register: add a **D-007** row pointing at §8.4.

**OPEN QUESTION — OQ-A** (exact wording, to be carried in
`PROTOCOL.md` §12, `THREAT_MODEL.md` §9.2 and `DECISIONS.md`'s open list):

> At two seats a hand that cannot complete must end, and there is no way for the
> two peers to agree which of them failed. Ending it with restoration lets a
> player escape a losing pot by going silent; ending it with forfeiture lets a
> player take an honest opponent's committed chips by asserting a deadline that
> did not pass. This plan takes restoration, because a liveness/fairness loss is
> preferable to a theft, but the choice reverses D-005 at `n = 2` and needs a
> numbered owner decision.

**OPEN QUESTION — OQ-B**:

> A dispute path that does not require the accused peer's signature. Already
> recorded in `DECISIONS.md` (D-007 point 4); it is repeated here because
> `PROTOCOL.md` §6.3 case (b) currently depends on exactly that circularity and
> the replacement is not designed.

---

### A-2 — The mechanism said to make the timeout race safe does not exist

**Ruling.** The claim is false under the document's own equivocation predicate
(different signers) and there is no signed artefact recording that a voter
accepted an action. The claim is **deleted, not repaired**. The `ACTION_SEEN`
construction the review sketches is a design change; `SPEC_CS.md` §36 forbids
adopting it on intuition, so it is recorded as an open question and nothing is
built on it.

Note that A-5's fix makes the old claim structurally impossible as well: after
§0.1 a vote no longer occupies the stage slot it is about, so a vote and an
action can never be two bodies in one slot even for one sender.

**`PROTOCOL.md`**

* §5.2: delete the third bullet ("a `TIMEOUT_VOTE` and a real event for the same
  stage …"). Replace with: *A `TIMEOUT_VOTE` is signed by the voter and the
  action it is about is signed by the subject. They are two events by two
  different keys and constitute no proof against anyone. `event_class` (§0.1
  of the fix plan) additionally puts them in different slots, so the pair is not
  even syntactically comparable.*
* §8.3: delete *"and if a peer produces both, it has equivocated in the sense of
  §5.2 and that is provable from the two bodies alone."*
* §8.3: the paragraph "Unanimity settles the race cleanly" must be weakened to:
  *If at least one required voter is honest and saw the action, no certificate
  forms and the action stands. If every required voter is dishonest, or none saw
  the action, a certificate forms. There is no artefact that settles the race
  when a voter lies about what it saw.*
* §12: add OQ-C.

**`THREAT_MODEL.md`**

* §5.3: **delete row X11** and fold its content into X10. X10's justification
  gains: *the race between a late action and a certificate is settled only by the
  presence of an honest required voter; there is no cryptographic artefact that
  settles it.* Update §5.4 totals: extended catalogue CP drops by one
  (7 -> 6), total extended rows drop by one (28 -> 27) before A-6's new row is
  added; recount after applying A-6 and D-3 rather than editing the numbers
  twice.
* §9.3: the standing-caution paragraph quotes "18 of 47". Recount after all
  catalogue edits; do not carry the old numbers.

**`STATE_MACHINE.md`**

* §8.4: the sentence *"There is never a state in which both a valid action and a
  valid certificate exist for the same parent"* is false. Replace with the
  honest-voter-dependent form above.

**OPEN QUESTION — OQ-C**:

> Should a required voter be obliged to publish a signed `ACTION_SEEN
> { sequence, event_hash }` before it may vote, so that vote-and-seen are two
> events by one key in one slot and a lying voter becomes provable? This is a
> design change with a cost in messages and latency and it is not adopted here.

---

### A-3 — `HAND_ABORT cause = 4` restores stacks and is reachable on demand, reopening the rage-quit escape

**Ruling, in three parts.**

1. **`PROTOCOL.md` §6.4's justification is false and is deleted.** Publishing a
   wrong `state_hash` is a one-field lie in a message every peer is required to
   emit; it needs no invalid signature and no divergent transcript, and "at least
   two peers disagreeing" is satisfied by one liar plus the honest victim.

2. **The *informed* version of the attack is closed by moving the checkpoint.**
   Checkpoint 7 moves to **immediately before `SHOWDOWN_REVEAL`**. There is no
   checkpoint after any hole card has opened. `HAND_COMPLETE` is now a collective
   derived stage (§0.2) whose every field is recomputed by every receiver, so a
   mismatching copy is a **rejection** under §4.0, not a divergence freeze. A
   loser therefore cannot pull the abort after seeing that they lost; they must
   commit to burning the table before the showdown, when they know only their own
   hand.

3. **`cause = 4` continues to restore start-of-hand stacks, and this is recorded
   as an unfixed in-protocol exploit rather than described as closed.** The
   review's option (i) — attribute the peer whose `state_hash` contradicts the
   unanimous transcript — is **rejected**, and the reason must be written down
   because it is not obvious: attribution by "differs from the derivation" needs
   an observer-independent reference derivation, and at run time there is none.
   Every peer derives with its own engine, so a peer in the minority believes the
   majority is wrong; the rule is only implementable as *"attribute everyone who
   disagrees with me"*, which is a vote, and a vote over the facts is exactly what
   `SPEC_CS.md` §15 forbids. Restoration is chip-conserving and agreed by
   everybody; forfeiture in this case would punish a party nobody can name.
   Choosing between them is escalated as **OQ-D**.

**`PROTOCOL.md`**

* §6.2: move checkpoint 7 from *"immediately before `HAND_COMPLETE`"* to
  *"immediately before `SHOWDOWN_REVEAL`"*. Add the normative sentence: *No
  checkpoint is placed after any hole card has been opened. A checkpoint after
  the cards are known is an abort trigger the loser can pull with full
  information.*
* §4.10 `HAND_COMPLETE`: it is now collective (§0.2). Restate *"A mismatch is not
  accepted and goes to §6"* as *"A mismatch is not accepted; the mismatching copy
  is rejected under §4.0 and the stage simply does not complete for that emitter.
  It does not open a §6 divergence."*
* §6.4: delete the final paragraph beginning *"This is the one place where
  restoration is correct rather than exploitable …"* entirely. Replace with:
  *Restoration here is not safe, it is merely the only disposition every peer can
  agree on when nobody can be named. A peer that publishes a false `state_hash`
  at a checkpoint can fault any table at any time and recover its own
  commitment. That is an in-protocol exploit, it is not closed, and
  `THREAT_MODEL.md` X29 carries it.*
* §4.10 `cause = 4`: keep restoration, and replace the justification sentence
  with a pointer to §6.4 as rewritten and to OQ-D.
* §12: add OQ-D.

**`THREAT_MODEL.md`**

* §5.3 X8: delete *"and economically closed"* from the class column and delete
  *"so the escape has no value"* from the justification. New justification ends:
  *The escape is closed on the abort path (`cause = 1`, forfeiture) at `n >= 3`.
  It is **not** closed on the divergence path (`cause = 4`, restoration — X29),
  and it is not closed at `n = 2` at all (§0.3 of the fix plan, OQ-A).*
* §7.4: the table and the D-005 reasoning stay; add a third row — *Fault the
  table with restoration, when nobody can be attributed (`cause = 4`): the same
  free escape, now reachable by a single peer publishing a wrong `state_hash`.*
* §9.1: add as a permanent limitation.
* §9.2: add OQ-D.

**`STATE_MACHINE.md`**

* §8.6 `AbortKind`: add `StateDivergence` (see C-6) and record that its chip
  settlement is **restoration to `start_stack_this_hand`**, not the §8.6
  forfeiture formula. The forfeiture formula applies to every other `AbortKind`.
* §10: I2 (step-local conservation) must hold on the restoration path too;
  say so.
* §11: add OQ-D.

**OPEN QUESTION — OQ-D**:

> On `HAND_ABORT cause = 4` (unresolvable state divergence) the chips are
> restored to their start-of-hand values because no peer can be attributed. This
> gives any single peer a free escape from a losing pot at the price of the
> table. The alternatives — forfeiting an unnamed party's commitment, or settling
> from the last `STATE_ACK`-agreed checkpoint — each need a numbered decision and
> neither is adopted here.

---

### A-4 — The equivocation predicate's slot namespace collides with lobby and join traffic

**Ruling.** The predicate is scoped to chained events only, by the `chain_scope`
and `event_class` envelope fields of **§0.1**, and unchained messages are moved
out of the chain namespace by sentinel values. Both halves are required: the
scope discriminator alone still leaves `JOIN_REQUEST` carrying a real `table_id`
with `hand_id = 0`, which is a collision waiting for a future chained message
type.

**Canonical predicate, to be carried verbatim by `PROTOCOL.md` §5.2 and quoted by
`THREAT_MODEL.md` G7:**

> Peer `K` equivocates when two `SignedEvent`s `E1 != E2` exist such that both
> pass the canonicality gate, both verify under `K`'s public key with
> `verify_strict`, both carry `chain_scope == 1`, their bodies agree on all six of
>
> ```
> (protocol_version, table_id, hand_id, sequence, event_class, sender_public_key == K)
> ```
>
> and `event_hash(E1) != event_hash(E2)`.
>
> Events with `chain_scope == 0` are outside this predicate entirely and can
> never produce an `EquivocationProof`. Their anti-replay is per-type:
> `timestamp_unix_ms` strict monotonicity per `table_id` for lobby adverts,
> `list_serial` for `PLAYER_LIST`, `join_nonce` for the join RPC, and
> `connection_nonce` plus a per-connection counter for the handshake.

**`PROTOCOL.md`**

* §2.3 / §2.4: add `chain_scope` and `event_class` to the envelope field list,
  in a new `#[n(..)]` index each, appended after the existing fields (indices are
  append-only forever, per X4).
* §5.2: replace the definition with the canonical predicate above. Delete the
  paragraph *"This definition works because of the stage rule…"* and replace with
  the same argument restricted to chained events. Keep the self-contained
  verification paragraph but add `chain_scope` and `event_class` to the list of
  fields the checker compares.
* §4.3 `JOIN_REQUEST`: envelope becomes `table_id = ZERO32`,
  `hand_id = 0xFFFF_FFFF_FFFF_FFFF`, `sequence = 0`, `chain_scope = 0`. Add
  payload field `n(8) table_id: bytes[32]` — the table being joined — and require
  it to equal the `table_public_key` of the advert named by `advert_hash`.
  Delete *"`sequence` = per-connection counter"*.
* §4.2 `HELLO` / `CAPABILITIES`, §4.3 `JOIN_ACCEPT` / `JOIN_REJECT` /
  `PLAYER_LIST`: same envelope rule.
* §7.2 / §7.3 / §7.4 / §7.5 and the new §7.7 `LOBBY_CHAT` (C-8): same envelope
  rule, stated once at the head of §7 rather than per message. The advert's table
  identity is already `sender_public_key == table_public_key`, so nothing is lost
  by zeroing the envelope `table_id`.
* §4.11 summary table: add a `chain_scope` column so the reader can see at a
  glance which messages are chained.
* §9.4: no change to bounds.

**`THREAT_MODEL.md`**

* G7: restate as *"If a player signs two conflicting **chained** events for the
  same `(table_id, hand_id, sequence, event_class)` …"*.
* G7 "Limit" paragraph: add the false-positive class — *the predicate must never
  be applied to unchained traffic; before the `chain_scope` discriminator existed,
  two successive honest lobby adverts and a joiner's `JOIN_REQUEST` alongside its
  own `RNG_REVEAL` both satisfied the old predicate. A specification-level
  false-positive source is worse than a user-level one, and it is recorded here so
  that any future message type is checked against it.*
* §5.2 row 16: add `event_class` to the four fields it names.

**`NETWORK_STACK.md`**

* §7.4 rule 4 currently says *"the same `(table_id, sequence/timestamp)`"*, a
  third incompatible predicate. Replace with: *Two different validly signed
  adverts from the same `table_public_key` with the **same
  `timestamp_unix_ms`** and different bodies are a founder contradiction. The
  table is marked `EQUIVOCATION`, both are retained as evidence and it is never
  joinable. This is **not** an `EquivocationProof` in the `PROTOCOL.md` §5.2
  sense — lobby messages are unchained — and it must not be called one; it is a
  local lobby-hygiene rule with no chip or blocklist consequence beyond refusing
  the table.*
* §6.4: add a validation step — *the envelope's `chain_scope` is `0` and its
  `table_id`, `hand_id`, `sequence` carry the unchained sentinels; anything else
  on a lobby topic is rejected.*

**OPEN QUESTION.** None. This finding is fully closed by the ruling.

---

### A-5 — `TIMEOUT_VOTE` occupies the subject's stage slot, so at a collective stage every honest voter equivocates against itself

**Ruling.** The `event_class` discriminator of **§0.1** is adopted (the review's
first shape). The separate vote-chain shape is rejected as a larger change with
no additional benefit. A vote no longer occupies the stage slot it is about; it
*references* it through `subject_sequence` and `parent_event_hash`, which is
already how it is validated.

**`PROTOCOL.md`**

* §4.8 `TIMEOUT_VOTE`: delete the paragraph *"The envelope's own `sequence` for a
  `TIMEOUT_VOTE` is `subject_sequence` … a peer physically cannot sign both
  without equivocating."* Replace with: *The envelope carries
  `sequence = subject_sequence`, `previous_event_hash = parent_event_hash`,
  `chain_scope = 1` and **`event_class = 1`**. The class puts the vote in its own
  slot, so a voter that has already emitted its own event at a collective stage
  `s` can vote about stage `s` without equivocating against itself. There is no
  mutual exclusion between a vote and an action, and none is claimed — see A-2.*
* §4.8 `TIMEOUT_CERT`: `event_class = 2`.
* §3.2: add — *`stage_hash(s)` is computed over the events of `event_class = 0`
  when the stage closes normally, and over the certificates of `event_class = 2`
  when the stage closes by certificate. Votes (`event_class = 1`) never enter a
  `stage_hash`; they are carried inside the certificate.*
* §5.1 table, row "duplicate message": qualify by `event_class`.

**`STATE_MACHINE.md`**

* T27, T41, T44 and §8.4: no semantic change, but the note that a vote and the
  voter's own contribution are mutually exclusive must not appear. Add: *a seat
  may hold both its own contribution at stage `s` and a vote about stage `s`;
  they are different event classes and neither implicates the other.*

**`THREAT_MODEL.md`**

* No catalogue row changes, but any text describing the vote as occupying the
  subject's slot must be corrected.

**OPEN QUESTION.** None.

---

### A-6 — The unanimity-minus-one eviction attack is specified in `PROTOCOL.md` and missing from `THREAT_MODEL.md`

**Ruling.** The rule is **removed**, not documented. Unanimity-minus-one exists
only to name a culprit in case (c), and per A-3 there is no observer-independent
derivation that can name one at run time, so the rule buys nothing real and costs
an attack in which `n-1` colluders take an honest player's committed chips
(available at `n = 3` with two colluders).

**Case (c) therefore always faults the table**, at every `n`, with `cause = 4`,
`attributed = []`, and restoration. One liar can fault any table. That is a
liveness and griefing cost, and this plan accepts it in preference to an
integrity cost, per the weaker-claim rule: losing a table is recoverable, having
your chips taken by a coalition is not.

A-3 and A-6 are therefore resolved by the same edit, as the review asked — just
in the opposite direction from the review's suggestion, and the reason is written
into the documents so nobody re-opens it.

**`PROTOCOL.md`**

* §6.3 step 4 case (c): delete the entire "Unanimity minus one" block, the
  "This is **not** 'majority rules over the facts'" paragraph, and the "The attack
  this rule enables" paragraph. Replace with:

  > **(c) The transcripts are byte-identical after reconciliation, and derived
  > states still differ.** No signature distinguishes an implementation bug from
  > a client lying about its own derived state, and there is no
  > observer-independent derivation at run time — every peer derives with its own
  > engine, so "attribute whoever disagrees with the derivation" is a vote over
  > the facts, which `SPEC_CS.md` §15 forbids. The hand therefore aborts with
  > `cause = 4`, `attributed = []`, stacks restored, and the table is faulted
  > (§6.4). No peer is removed and no peer is named. The evidence — the unanimous
  > transcript plus every peer's signed `STATE_HASH` — is written to the profile
  > directory and is **publicly adjudicable offline** by anyone who runs the
  > reference engine over it, forever. Live adjudication is not available and is
  > not claimed.

* §6.4: `cause = 4` faults the table at every `n`. Delete the `n >= 3` /
  `n = 2` split.
* §13: no constant changes here.

**`THREAT_MODEL.md`**

* §5.3: add row **X29 — "Fault any table on demand by publishing a false
  `state_hash` at a checkpoint"**, class **DNA**. Justification: *A `state_hash`
  is a one-field value in a message every peer must emit. A single peer that
  publishes a value it did not derive forces the §6.3 case (c) path: the hand
  aborts, stacks are restored and the table closes, with no attribution live.
  Bounded by three things — it costs the attacker the table and, in a tournament,
  their own equity in it; checkpoint 7 sits before `SHOWDOWN_REVEAL`, so the
  attacker must commit before learning whether they won; and the evidence is
  deterministically adjudicable offline by any third party running the reference
  engine over the unanimous transcript. Not solved. Related: X8, OQ-D.*
* The eviction attack itself is **not** added, because after this ruling it no
  longer exists. Add one sentence to X29 recording that an earlier draft resolved
  case (c) by removing the odd peer out, and that the rule was deleted because it
  let `n-1` colluders take an honest player's chips at `n >= 3`.
* §9.1: add X29 as a permanent limitation.
* §9.2: covered by OQ-D.
* §5.4 and §9.3: recount.

**`STATE_MACHINE.md`**

* §5.2 / §8.6: the divergence transitions added under C-6 must implement the
  no-removal form. There is no `PeerRemoved` transition and no
  `unanimity-minus-one` guard anywhere.

**OPEN QUESTION.** Covered by OQ-D; no new one.

---

### A-7 — X7 is classified "detected and attributed" while two documents record attribution as unresolved for simultaneous failures

**Ruling.** X7 is downgraded to the accurate three-way classification, and
`PROTOCOL.md` §8.4's unratified fallback ("every non-voting seat is attributed")
is **replaced by a safe default that attributes nobody**, because attributing a
seat that is merely behind a partition and then forfeiting its chips under D-005
is a positive-gain attack on an honest player. Q-02 stays open with a defined
default rather than an unratified construction.

**Canonical classification of X7:**

| Case | Class |
|---|---|
| one silent seat, `n >= 3` | **D&A** |
| two or more seats silent simultaneously | **DNA** |
| any silent seat at `n = 2` | **DNA** (§0.3) |

**`THREAT_MODEL.md`**

* §5.3 X7: replace the class cell with `D&A / DNA` and put the table above in the
  justification. Delete *"so attribution is sound"* and replace with *"attribution
  is sound for a single silent seat at `n >= 3`; it is unavailable when two or
  more seats stop at once, because each required voter set contains the other
  subject (`PROTOCOL.md` Q-02, `STATE_MACHINE.md` Q3), and unavailable at `n = 2`
  for the reason in D-007."*
* §7.3 case (c)2: add the same qualification.
* §9.2: add **OQ-E** (below) as a **blocking** open question — it currently
  appears only in the two downstream documents, so the threat model's own risk
  register does not show it.
* §5.4 / §9.3: recount D&A and DNA.

**`PROTOCOL.md`**

* §8.4: keep the rule that a seat already named as the subject of an outstanding
  older unmet deadline is excluded from `V`. **Delete** *"and **every**
  non-voting seat is attributed"*. Replace with: *If unanimity still cannot be
  reached before `hand_deadline_ms` expires, the hand aborts with `cause = 1`,
  `attributed = []`, `cert_hash = None`, and stacks restored. Nobody is named,
  because a seat that did not vote may be silent, partitioned or simply slow, and
  D-005 forfeiture against a partitioned honest seat is a positive-gain attack.
  This is a safe default, not a solution: it lets two colluding seats void a hand
  by going silent together. Q-02 remains open.*
* §4.10: `attributed` may also be empty for `cause = 1` in this case — fold into
  the A-1 edit that already relaxes the rule.
* §12 Q-02: restate the question as *"how is a multi-subject deadline
  certificate constructed, and who does it attribute?"*, and record the safe
  default above as the interim behaviour.

**`STATE_MACHINE.md`**

* §8.4 Q3: keep open; add the interim rule — a `HandDeadline` abort with an empty
  `attributed` set runs the §8.6 formula with `culprits = {}`, which by that
  formula's own `others is empty` branch is not reached; specify explicitly that
  `culprits = {}` means every seat gets its `committed_hand` back. Add this as an
  explicit branch of the §8.6 pseudocode so it is not left to inference.
* §10: add invariant **I27** — *an abort with `attributed == []` returns exactly
  `committed_hand[s]` to every seat `s`, and moves no chips between seats.*

**OPEN QUESTION — OQ-E**:

> How is a timeout certificate constructed when two or more seats are
> simultaneously unresponsive, and whom does it attribute? `signers ==
> participants \ {subject}` is unachievable, and attributing every non-voting
> seat punishes honest seats behind a partition. The interim behaviour is to
> abort with no attribution and no chip movement, which lets two colluding seats
> void a hand for free. Blocking for Phase 4.

---

### A-8 — The relay byte budget compares table-wide traffic against a per-circuit cap; and the D-002 relay config cannot serve a full table

**Ruling.** The arithmetic is corrected to a per-circuit, per-direction basis.
`max_circuit_bytes` is per circuit and per direction, verified in
`libp2p-relay-0.21.1/src/behaviour.rs` (`impl Default for Config`,
`max_circuit_bytes: 1 << 17`) and enforced per connection in
`src/behaviour/handler.rs`. A table is a full mesh, so one circuit connects
exactly one pair.

**The numbers every document must now carry:**

| Quantity | Value | Basis |
|---|---:|---|
| `ShuffleProof<52>` + `MaskedDeck<52>`, one shuffler | 8 979 B | 5 547 + 3 432, measured [MENTAL §5.1] |
| Shuffle traffic over **one circuit**, **one direction**, per hand | **8 979 B** | that peer's own step and proof; **independent of `n`** |
| Hands per direction against a 131 072 B public-relay budget, shuffle only | **~14** | 131 072 / 8 979 |
| The same including the signed event stream | **~10 hands** | order-of-magnitude; measure under OQ-12 |
| A relayed peer's **total** per-hand outbound at an `n`-seat table | `(n-1) x 8 979 B` | 44 895 B at six seats — a bandwidth figure, spread over `n-1` separate budgets, never a single cap |
| The binding public-relay limit | **`max_circuit_duration = 120 s`**, not the byte cap | a session lasts far longer than two minutes |

**Normative D-002 relay config** — every field, so no document restates a subset:

```
max_circuit_duration      = 3600 s
max_circuit_bytes         = 1 << 30            (1 GiB)
max_reservations          = 64
max_reservations_per_peer = 4                  (crate default)
reservation_duration      = 3600 s             (crate default)
max_circuits              = 128
max_circuits_per_peer     = 9                  (= MAX_SEATS - 1)
```

`max_circuits_per_peer = 9` is forced: a relayed peer at a ten-seat table needs
one circuit per table-mate. The crate default of 4 refuses the fifth, so the
D-002 relay as previously written could not carry the case it exists for.
`max_circuits = 128` lets one volunteer serve roughly 14 relayed peers at full
tables.

**Per-tier tightening is not a `Config` field.** `max_circuits_per_peer` is
global. The Tier A / Tier B distinction of `NETWORK_STACK.md` §9.6 must therefore
be enforced inside the named `RateLimiter` implementation (B-2), which sees the
`PeerId` and can count that peer's live circuits: Tier A is refused beyond 2 live
circuits, Tier B beyond 9. Say so explicitly; a reader must not think the
`Config` field does it.

**`CRYPTOGRAPHY.md`**

* §6.5 consequence 2: delete *"so a public relay carries on the order of two
  six-handed hands before it resets"*. Replace with the table above. Keep the
  `n x (5547 + 3432)` figure but label it *total per-hand outbound for one peer,
  spread over `n-1` circuits*, never a per-circuit figure. State plainly that
  duration, not bytes, is the binding public-relay limit.

**`PROTOCOL.md`**

* §9.3 closing paragraph: delete *"which is the number that makes the 128 KiB
  default relay budget of D-001 a real constraint"*. Replace with the per-circuit
  figure and the duration statement.
* §4.4 and §3.2: see D-5 — the ~340 ms / ~1.1 s hand-startup figures must be
  marked as estimates wherever they appear.

**`THREAT_MODEL.md`**

* §3.5, last bullet: keep the "engineered abort attack against ourselves"
  framing, which is correct, but attribute it to the **2-minute duration limit**,
  not to the byte cap.
* §5.3 X20: same correction.
* §9.2 OQ12: restate — *what is the real per-hand byte count over one relayed
  circuit, per direction, measured against the `Limit` a real relay returns?
  Estimated ~10 KB per direction per hand against 128 KiB, which is comfortable;
  the binding limit is the 120 s duration.*

**`NETWORK_STACK.md`**

* §9.5 role table, second row: correct the arithmetic identically. The claim
  *"heads-up shuffle traffic alone is ~18 KB/hand"* is a table-wide figure being
  compared to a per-circuit cap; per circuit per direction it is 8 979 B.
* §9.6: replace the `relay::Config` block with the normative config above,
  verbatim, including `max_circuits` and `max_circuits_per_peer`.
* §9.6 Tier A / Tier B: add the sentence that the per-tier circuit ceiling is
  enforced by the `RateLimiter` implementation and not by the `Config` field.
* §9.6 consent disclosure: the bandwidth implication of `max_circuits = 128`
  and 1 GiB per circuit must be in the first-run disclosure text, in concrete
  terms (up to 128 concurrent circuits, up to 1 GiB each).

**OPEN QUESTION.** None new; OQ12 is restated, not opened.

---

## B findings — wrong or unverified facts

### B-4 — `PROTOCOL.md` §2.8 contradicts §2.4 on the literal `DOMAIN_EVENT` bytes

*(Placed first among the B findings because every signature in the protocol
depends on it.)*

**Ruling.** §2.4 is right; §2.8's table entry is a rendering of the domain-string
register's own spacing convention applied to a value that is not a
`derive_key` domain at all. The canonical value, with the hex printed so it can
never be misread again:

```
DOMAIN_EVENT = "p2p-poker/v1/event" NUL-padded to 24 bytes

hex: 70 32 70 2d 70 6f 6b 65 72 2f 76 31 2f 65 76 65 6e 74 00 00 00 00 00 00
     |<---------- 18 ASCII bytes ---------->|<---- 6 NUL ---->|

TO_BE_SIGNED = DOMAIN_EVENT || u32_be(len(body_bytes)) || body_bytes
```

The 24 hex bytes above are what `PROTOCOL.md` §13 must carry. Every other domain
string in the register **is** a `blake3::derive_key` domain and keeps its
space-separated form (`p2p-poker v1 transcript`, and so on). The two conventions
are different on purpose and the documents must say so.

**`PROTOCOL.md`**

* §2.8 domain table, first row: change the left cell from `p2p-poker v1 event`
  to `p2p-poker/v1/event` and change the right cell to *"the 24-byte
  `DOMAIN_EVENT` signature prefix (§2.4). **This is a literal prefix, not a
  `derive_key` domain**, which is why its separators differ from every other row
  in this table."*
* §13: add the 24-byte hex line above, verbatim.
* §2.4: unchanged, but add the hex line there too so the two places are
  byte-identical.

**`CRYPTOGRAPHY.md`, `NETWORK_STACK.md`, `STATE_MACHINE.md`, `THREAT_MODEL.md`**

* Any place that names the signature prefix must quote `p2p-poker/v1/event` and
  point at `PROTOCOL.md` §13. None may restate the byte string independently.

**OPEN QUESTION.** None.

---

### B-1 — "The `rand` crate is deliberately absent from the dependency tree" is false

**Ruling.** `CRYPTOGRAPHY.md` §7.2 is correct and the other two documents adopt
its text. The facts, verified: `ark-std 0.5.0` depends on `rand 0.8` with feature
`std_rng` (`ark-std-0.5.0/Cargo.toml`), so `rand 0.8.8`, `rand_chacha 0.3.1` and
`rand_core 0.6.4` are in the **runtime** tree; `StdRng` is compiled in and is used
by `ziffle` for nothing-up-my-sleeve public constants
(`ziffle-0.1.0/src/lib.rs:127`); `SmallRng` is **not** compiled in, because
rand 0.8's `small_rng` feature is not enabled (`rand-0.8.8/Cargo.toml`).

Therefore `SPEC_CS.md` §7 is enforced by **the CI lint of `CRYPTOGRAPHY.md` §12
item 6**, not by the dependency graph. No document may claim a structural
guarantee.

**`PROTOCOL.md`**

* §4.4, `RNG_COMMIT`: delete *"and the `rand` crate is deliberately absent from
  the dependency tree so that `SmallRng` and `StdRng` are structurally
  unavailable — which is `SPEC_CS.md` §7's requirement enforced by the dependency
  graph rather than by reviewer discipline"*. Replace with: *`rand 0.8.8` is in
  the runtime tree via `ark-std 0.5.0`; `SmallRng` is absent because the
  `small_rng` feature is not enabled, `StdRng` is present and used by `ziffle`
  for deterministic public constants only, and `SPEC_CS.md` §7 is enforced by the
  CI lint of `CRYPTOGRAPHY.md` §12 item 6 (see `CRYPTOGRAPHY.md` §7.2 and OQ-8).*

**`THREAT_MODEL.md`**

* §2 assumption A7: same replacement. The *"If false"* clause must name the lint
  as the mitigation: *If false, or if the lint is removed, §7's prohibition is
  unenforced and a future contributor can reach `StdRng` from our own crates.*

**`CRYPTOGRAPHY.md`**

* §7.2 and OQ-8: unchanged; they are the source. Add a one-line note that
  `PROTOCOL.md` §4.4 and `THREAT_MODEL.md` A7 were corrected to match, so the
  correction is not lost.

**OPEN QUESTION.** None new; `CRYPTOGRAPHY.md` OQ-8 already carries it.

---

### B-2 — `relay::RateLimiter` is publicly re-exported

**Ruling.** The trait is nameable:
`libp2p-relay-0.21.1/src/lib.rs:42` re-exports
`behaviour::rate_limiter::RateLimiter` at the crate root, so
`libp2p::relay::RateLimiter` is a public trait. The **named-type implementation
is adopted**, not the closure — D-002's own 2026-08-28 correction says so, and
admission control needs shared mutable state that a closure would have to capture
awkwardly. The blanket impl is real
(`src/behaviour/rate_limiter.rs:38,56`) and the closure form compiles, but the
stated *reason* for using it is false and the reason is what the documents were
carrying.

**`NETWORK_STACK.md`**

* §9.6: delete *"Their element type lives in a `pub(crate)` module and cannot be
  named from outside, but the crate carries a blanket implementation … so a
  closure coerces into the vector without naming the trait."* Replace with: *The
  module `behaviour::rate_limiter` is `pub(crate)`, but the trait is re-exported
  at the crate root (`src/lib.rs:42`), so `libp2p::relay::RateLimiter` is
  nameable and implementable by a downstream type. A named type is used rather
  than the crate's blanket `impl<T: FnMut(..)> RateLimiter for T`, because
  admission control holds shared mutable state.*
* §9.6: replace the closure code block with D-002's named-type form:

  ```rust
  struct PokerPeersOnly { known: Arc<Mutex<HashSet<PeerId>>>, /* tier + live-circuit counts */ }

  impl libp2p::relay::RateLimiter for PokerPeersOnly {
      fn try_next(&mut self, peer: PeerId, _addr: &Multiaddr, _now: web_time::Instant) -> bool { … }
  }
  ```

  Keep the `web-time = "1"` dependency note and the requirement that **both**
  `reservation_rate_limiters` and `circuit_src_rate_limiters` are gated. This type
  is also where A-8's per-tier circuit ceiling is enforced.

**`THREAT_MODEL.md`**

* §5.3 X21: delete *"without naming the crate-private trait"*. Replace with
  *"through a public trait re-exported at the crate root, implemented by a named
  type that holds the admitted-peer set"*.

**OPEN QUESTION.** None.

---

### B-3 — The RustSec `rand` row is stale and the advisory was never evaluated against the version actually pulled in

**Ruling.** The row is corrected, and — because a research document is evidence
rather than authority — the correction is additionally carried in
`CRYPTOGRAPHY.md` §9, which is the authoritative supply-chain section. The
advisory is benign here, verified in the local advisory database
(`~/.cargo/advisory-db/crates/rand/RUSTSEC-2026-0097.md`: `informational =
"unsound"`, `patched = [">= 0.10.1", "< 0.10.0, >= 0.9.3", "< 0.9.0, >= 0.8.6"]`),
and `0.8.8 >= 0.8.6`.

**Canonical row text, to be used verbatim in both places:**

> `rand` — RUSTSEC-2026-0097 — **in the runtime tree at 0.8.8 via `ark-std 0.5.0`.
> `informational = "unsound"`, patched at `>= 0.8.6`, so the pinned version is
> patched. The unsound path requires `rand::thread_rng` inside a custom `log`
> implementation, which does not occur here.**

**`research/CRYPTO_LIBS.md`**

* §7.3: replace the *"Crate not in the tree at all"* cell with the text above.
  Add `rand 0.8.8`, `rand_chacha 0.3.1` and `rand_core 0.6.4` to §10's version
  list.

**`CRYPTOGRAPHY.md`**

* §9: the "Supply-chain finding that changes `CRYPTO_LIBS.md` §4.4" block
  currently corrects only the `paste` / RUSTSEC-2024-0436 row. Add a second
  finding with the canonical row text, so a reader of the authoritative document
  sees it without opening the research note. Feeds D-2.

**`THREAT_MODEL.md`**

* A7 already changes under B-1; nothing further.

**OPEN QUESTION.** None.

---

## C findings — contradictions between documents

### C-1 — Every shared size constant differs between `PROTOCOL.md` and `NETWORK_STACK.md`

**Ruling.** One table, one home, one name per value.

* **Home:** `PROTOCOL.md` §13 is the single normative table for every two-sided
  constant. `NETWORK_STACK.md` §14 is replaced by a pointer to it plus only the
  values that are genuinely local tuning (GossipSub mesh parameters, dial
  budgets, LRU sizes).
* **Names:** `NETWORK_STACK.md` §14's names win, because that section already
  calls itself "the shared constants module" and its names are the ones a Rust
  `constants` module would carry. `PROTOCOL.md` renames
  `LOBBY_MAX_MESSAGE`, `TABLE_MAX_FRAME`, `SNAPSHOT_MAX_REQUEST` and
  `SNAPSHOT_MAX_RESPONSE` accordingly. **No value has two names anywhere in the
  corpus.**
* **Values:** as ruled below. Where the review notes a merit argument
  (`max_transmit_size` at the crate default), it is followed; otherwise the
  smaller, more conservative value wins unless a message the protocol actually
  defines does not fit inside it, in which case the larger wins and the reason is
  recorded.

#### The canonical table — both documents must carry these values, and only these

| Constant | Value | Ruling and reason |
|---|---:|---|
| `PROTOCOL_VERSION` | `1` | agreed |
| `IDENTIFY_PROTOCOL` | `/p2p-poker/1` | agreed |
| `LOBBY_TOPIC` | `/p2p-poker/lobby/1` | agreed |
| `LOBBY_CHAT_TOPIC` | `/p2p-poker/lobby-chat/1` | kept; see C-8 |
| `SNAPSHOT_PROTOCOL` | `/p2p-poker/lobby-snapshot/1` | agreed |
| `JOIN_PROTOCOL` | `/p2p-poker/join/1` | new to `PROTOCOL.md`; see C-7 |
| `TABLE_PROTOCOL` | `/p2p-poker/table/1` | agreed |
| `LOBBY_DERIVATION_STRING` | `p2p-poker/mainline-lobby/v1` | only in `NETWORK_STACK.md` today; moves to the shared table |
| `LOBBY_INFOHASH` | `fd7c0d69433e32e425db3ca2b7d7718928739f01` | as above |
| `RELAY_DERIVATION_STRING` | `p2p-poker/mainline-relay/v1` | as above |
| `RELAY_INFOHASH` | `9c18d8c80f69de3aa079b2ef519bc4bbb67e1cc1` | as above |
| `GOSSIP_MAX_TRANSMIT` | **65 536** | `NETWORK_STACK.md` wins on the merits: it is `libp2p-gossipsub 0.49.5`'s own default (`src/config.rs:244-246`), so pinning there maximises interoperability. `PROTOCOL.md`'s `LOBBY_MAX_MESSAGE = 16 384` conflated the transport ceiling with the application ceiling and is deleted. |
| `LOBBY_MSG_MAX` | **8 192** | application ceiling for any lobby message of any type. The transport limit is a backstop, never the operative limit. |
| `TABLE_AD_MAX` | **1 024** | payload cap for `LOBBY_TABLE_AD`. The 29 fields of §7.2 total under 500 B at worst case, so 1 024 is over 2x headroom; the smaller value wins. `PROTOCOL.md` §9.3's 2 048 is deleted. |
| `TABLE_AD_SIGNED_MAX` | **1 536** | **new constant, and it is the resolution of the 2 560 / 1 024 conflict** — the two documents were capping different objects. This is the complete `SignedEvent` of an advert: payload (<= 1 024) + envelope + 64 B signature. Every place that embeds or forwards a whole advert uses this: `JOIN_ACCEPT` `n(2) advert_event`, the snapshot elements, `LOBBY_SNAPSHOT_RESPONSE`. The figure `2 560 B` is deleted everywhere. |
| `LOBBY_CHAT_MAX` | **2 048** | payload cap; `NETWORK_STACK.md`'s value, kept. See C-8. |
| `SNAPSHOT_REQ_MAX` | **1 024** | `PROTOCOL.md` wins: the request payload is ~45 B and capped at 128 B, so 4 096 is pure slack and slack is DoS surface. |
| `SNAPSHOT_RESP_MAX` | **262 144** | `NETWORK_STACK.md` wins: the smaller value, and it fits the content — `128 x 1 536 = 196 608` plus array and envelope overhead. `PROTOCOL.md`'s 524 288 is deleted. |
| `SNAPSHOT_MAX_ADS` | **128** | `PROTOCOL.md` wins: the smaller count, and it is what makes `SNAPSHOT_RESP_MAX` fit. |
| `JOIN_REQ_MAX` | **4 096** | codec-level cap on `/p2p-poker/join/1` (C-7). `JOIN_REQUEST`'s payload cap stays 512 B, so this is envelope headroom only; `NETWORK_STACK.md`'s 16 384 is unnecessary slack. |
| `JOIN_RESP_MAX` | **16 384** | `NETWORK_STACK.md` wins: `JOIN_ACCEPT`'s payload cap is 8 192 and it embeds a whole advert (<= 1 536) plus a ten-entry roster, so the response needs the room. |
| `TABLE_FRAME_MAX` | **262 144** | `PROTOCOL.md` wins, and the reason must be recorded because `NETWORK_STACK.md` §8.4's justification looks convincing and is wrong: it sizes the frame against `ShuffleProof<52>` only and overlooks `DISPUTE`, whose own cap is 140 000 B (4 evidence entries x 32 768) and `HAND_ABORT`'s 80 000 B. A 131 072 B frame cannot carry the protocol's own evidence-bearing message. |
| `MAX_EMBEDDED_EVENT` | **32 768** | new name for the per-element evidence cap already used by `DISPUTE` and `HAND_ABORT`; unchanged in value |
| `MAX_SEATS` | `10` | agreed |
| `MAX_STAGES_PER_HAND` | `2 048` | agreed |
| `MAX_CBOR_NESTING_DEPTH` | `8` | agreed |
| `AD_TTL_MS` | `90 000` | agreed (`TABLE_AD_TTL` renamed to the `_MS` form) |
| `AD_REBROADCAST_MS` | `30 000` | agreed |
| `MAX_AD_LIFETIME_MS` | `300 000` | agreed (`MAX_FUTURE_EXPIRES_AT`) |
| `MAX_CLOCK_SKEW_MS` | `120 000` | only in `PROTOCOL.md` today; moves to the shared table |
| `PRESENCE_TTL_MS` / `PRESENCE_HEARTBEAT_MS` | `120 000` / `40 000` | agreed |
| `REANNOUNCE_INTERVAL_MS` | `600 000` | only in `NETWORK_STACK.md` today |
| `IDLE_CONNECTION_TIMEOUT_MS` | `60 000` | only in `NETWORK_STACK.md` today |
| `SNAPSHOT_PEER_COUNT` | `4` | one name for `SNAPSHOT_PEERS` and `SNAPSHOT_PEER_COUNT` |
| `MDNS_QUERY_INTERVAL_MS` | `15 000` | only in `NETWORK_STACK.md` today |
| `HANDSHAKE_DEADLINE_MS` | `15 000` | agreed |
| `MAX_CONSECUTIVE_AUTO_ACTIONS` | `3` | agreed |
| `CERT_SETTLE_MS` | **deleted** | see C-10 |

Local (not two-sided) constants stay where they are and must be **labelled
local** in the table: `MAX_TRACKED_TABLES`, `MAX_TRACKED_PRESENCE`, the
`MAX_ADS_*` / `MAX_PRESENCE_*` rate budgets, the GossipSub mesh parameters, the
`connection_limits` values.

**`PROTOCOL.md`**

* §13: rebuild the block from the table above, using the `NETWORK_STACK.md`
  names, and add the `DOMAIN_EVENT` hex (B-4) and the relay config (A-8, by
  reference to `NETWORK_STACK.md` §9.6 — the relay config is transport-local, not
  two-sided).
* §7.1: keep the "two-sided protocol constant, not a tuning knob" sentence but
  attach it to `GOSSIP_MAX_TRANSMIT = 65 536`.
* §9.2: rebuild the table with the new names and values.
* §9.3: `LOBBY_TABLE_AD` 2 048 -> **1 024**; `LOBBY_SNAPSHOT_RESPONSE`
  524 288 -> **262 144** with the note *"<= 128 x 1 536 = 196 608"*.
* §9.4: adverts-in-a-snapshot row becomes *"128, each a complete `SignedEvent`
  <= `TABLE_AD_SIGNED_MAX` = 1 536 B; 128 x 1 536 = 196 608 B, inside
  `SNAPSHOT_RESP_MAX`"*.
* §4.3 `JOIN_ACCEPT` `n(2) advert_event`: `<= 2 560 B` -> `<= 1 536 B`.
* §7.5: the same two corrections.

**`NETWORK_STACK.md`**

* §14: replace the table with *"Every two-sided constant is defined in
  `PROTOCOL.md` §13 and is not restated here. Changing any of them is a
  protocol-version change."* plus a short table of the genuinely local values.
* §6.2: `max_transmit_size(65_536)` unchanged; the justification stays.
* §6.5: `LOBBY_TABLE_AD` 1 024 unchanged; add `TABLE_AD_SIGNED_MAX = 1 536` as
  the cap applied when a **whole signed advert** is forwarded or embedded.
* §7.1: `set_request_size_maximum(1024)` and `set_response_size_maximum(256 * 1024)`.
* §7.3: *"at most 256 ads"* -> **128**; *"each ad subject to the same 1 024 B cap"*
  -> *"each ad's payload <= 1 024 B and each complete signed ad <= 1 536 B"*.
* §8.4: *"Max frame 131 072 bytes (128 KiB)"* -> **262 144**, and replace the
  justification with: *sized by `DISPUTE`, whose four evidence entries of
  32 768 B each plus envelope exceed 128 KiB; the shuffle objects are an order of
  magnitude smaller and are not the binding case.*
* §8.4: join caps 16 384/16 384 -> **4 096 / 16 384**.
* §11.3: rebuild from the canonical table.

**OPEN QUESTION.** None. This is mechanical once the table is fixed.

---

### C-2 — Two incompatible normative definitions of `ctx`

**Ruling.** `PROTOCOL.md` §4.5's **hashed** form wins: it is fixed-length, it
reuses the one audited length-prefixed domain-separated hasher of §2.8, and
`ziffle` hashes whatever it is given anyway (`SHA-256(ctx)`), so a 32-byte input
loses nothing. `CRYPTOGRAPHY.md` §6.4's raw concatenation is deleted.

Two fields from `CRYPTOGRAPHY.md`'s form are **kept**, because `THREAT_MODEL.md`
A11 names them literally and an editor should not have to argue that `sequence`
implies `shuffle_round`: `shuffle_round` becomes an explicit field, and
`session_id` is confirmed as the same object `SPEC_CS.md` §14 and §20 call the
*session nonce*. The name `session_nonce` is retired; there is one name,
`session_id`, defined by `PROTOCOL.md` §4.3.

#### The canonical `ctx`, field by field, in order, with separators

```
ctx = h("p2p-poker v1 deck-ctx", [
          u16_be(protocol_version),   //  2 B
          table_id,                   // 32 B  the table's Ed25519 public key
          session_id,                 // 32 B  = SPEC_CS.md §14/§20 "session nonce"
          u64_be(hand_id),            //  8 B
          u64_be(sequence),           //  8 B  the chain stage index of the event carrying the proof
          [ u8(shuffle_round) ],      //  1 B  0-based position of this shuffler in the chain;
                                      //       0xFF for every ctx that is not a shuffle step
          sender_public_key           // 32 B  the emitter's application Ed25519 key
      ])
```

where `h` is `PROTOCOL.md` §2.8's constructor:
`blake3::derive_key(domain, b"p2p-poker/v1")` as the key, then for each part an
8-byte big-endian length prefix followed by the part. The result is 32 bytes and
is what is handed to the deck library. **The separator is the length prefix; no
other separator, delimiter or padding exists.** Field order is exactly as listed
and is part of the protocol.

`shuffle_round = 0xFF` applies to the `ctx` used for `DECK_INIT` ownership proofs
and for reveal-token DLEQ proofs; those are not shuffle-chain steps and must not
share a `ctx` with one.

#### Extension: the same ruling applies to three more shared strings

The review did not name these, but they are the same class of defect — a
load-bearing byte string with two or three definitions — and an editor fixing
`ctx` will be looking straight at them. They are ruled here so nobody fixes one
and leaves the others.

| Object | Canonical definition | What is deleted |
|---|---|---|
| RNG beacon commitment | `commitment_i = h("p2p-poker v1 rng-commit", [ table_id, session_id, committer_app_public_key, r_i, salt_i ])` | `PROTOCOL.md` §4.4's two-part `h(..., [r_i, salt_i])`, which does not bind the table, the session or the committer, so a commitment could be lifted between tables. `CRYPTOGRAPHY.md` §7.3's binding is the correct one and wins. |
| RNG beacon combine | `seed = h("p2p-poker v1 rng-beacon", [ r_1, …, r_n ])`, ascending by seat index | `CRYPTOGRAPHY.md` §7.3's `p2p-poker v1 rng-seed` (a domain string not in the register) and `STATE_MACHINE.md` §7.9's `BLAKE3("p2p-poker/seat-beacon/v1" ‖ …)` (a third construction, unprefixed and not domain-separated through `derive_key`). `PROTOCOL.md` §2.8's register wins on the domain name; `CRYPTOGRAPHY.md`'s length-prefixed construction wins on the form. |
| `index_map_hash` | `h("p2p-poker v1 deck-commit", [ u8(m), for i in 0..2m+5: u8(i) ‖ u8(role_code(i)) ‖ u8(owner_seat_or_0xFF(i)) ])` with the **no-burn** map of C-3 | any other layout |

`PROTOCOL.md` §2.8's domain-string register is the **single register**. A document
that needs a domain string takes it from there; a document that invents one has a
bug. Add `p2p-poker v1 rng-seed` and `p2p-poker/seat-beacon/v1` to a short
"retired, never valid" list under the register so the mistake cannot recur
silently.

**`PROTOCOL.md`**

* §4.5: adopt the `ctx` block above verbatim, including the `shuffle_round`
  field and the `0xFF` sentinel. Add a sentence recording that this supersedes
  `CRYPTOGRAPHY.md` §6.4's raw-concatenation form.
* §4.3: state explicitly that `session_id` is the object `SPEC_CS.md` §14 and §20
  call the session nonce, and that `session_nonce` is not a separate field.
* §4.4 `RNG_COMMIT`: adopt the five-part commitment above.
* §2.8: add the retired-strings list.

**`CRYPTOGRAPHY.md`**

* §6.4: replace "The binding rule, normative" with *"The binding rule is
  normative in `PROTOCOL.md` §4.5 and is reproduced here for the reader"*, then
  reproduce the block above byte-for-byte. Delete the raw-concatenation form and
  delete `session_nonce` as a field name.
* §6.4: keep the "What ziffle does NOT bind" analysis unchanged — it is correct
  and it is the reason `ctx` matters.
* §7.3: change `p2p-poker v1 rng-seed` to `p2p-poker v1 rng-beacon`.
* §14 cross-reference table: the `PROTOCOL.md` row currently says it must carry
  "the exact `ctx` construction (§6.4)". Reverse the direction: *`PROTOCOL.md`
  §4.5 owns `ctx`; this document reproduces it.*
* OQ-3: stays open (it asks whether the binding is *sufficient*, which this
  ruling does not answer), but its text must now name the canonical form.

**`THREAT_MODEL.md`**

* A11: restate the assumed binding as the canonical field list above, not as a
  minimum. *"We assume the proof context `ctx` is exactly `PROTOCOL.md` §4.5's
  construction over `(protocol_version, table_id, session_id, hand_id, sequence,
  shuffle_round, sender_public_key)`."*
* OQ3: unchanged in substance; point at `PROTOCOL.md` §4.5.

**`STATE_MACHINE.md`**

* §7.9: replace the beacon construction with the canonical commit and combine
  above. The engine does not compute them, but the document currently prints a
  third version of both and that is what makes them diverge.

**OPEN QUESTION.** None from this ruling. `CRYPTOGRAPHY.md` OQ-3 /
`THREAT_MODEL.md` OQ3 remain open on their own terms (is the binding
*sufficient*), and the mandatory replay regression tests they name are unchanged.

---

### C-3 — Burn cards exist in `CRYPTOGRAPHY.md` and are explicitly absent from `PROTOCOL.md` and `STATE_MACHINE.md`

**Ruling.** **No burn cards.** Two documents against one, both giving the same
index map; `CRYPTOGRAPHY.md`'s own parenthetical concedes that the layout is
`PROTOCOL.md`'s to state; and the substantive argument is sound — a burn defeats
physical marked-card and edge-sorting attacks, there are no physical cards, and a
burn that is never opened is indistinguishable from an unused index.

The consequence the ruling turns on, and which must be stated in
`CRYPTOGRAPHY.md`: a burn **costs a deck position and changes the deal map**, and
the deal map is hashed into `index_map_hash` inside `DECK_COMMIT`. Two conforming
clients with different maps produce a guaranteed `DECK_COMMIT` mismatch every
hand — a manufactured §15 dispute at every hand, which after A-3/A-6 faults the
table. This is not a documentation nicety.

#### The canonical deck-index map

Let `D = [d_0, …, d_{m-1}]` be the `dealt_in` seats in clockwise order starting
from the first dealt-in seat strictly clockwise of `button_position` (normal deal
order, small blind first), `m = |dealt_in|`.

| Deck index | Role |
|---|---|
| `0 … m-1` | first hole card of `d_0 … d_{m-1}` |
| `m … 2m-1` | second hole card of `d_0 … d_{m-1}` |
| `2m`, `2m+1`, `2m+2` | flop |
| `2m+3` | turn |
| `2m+4` | river |
| `2m+5 … 51` | unused; no reveal token for these indices is ever legal |

At most 25 of 52 indices are used. The symbol for the dealt-in count is **`m`**
in every document; `STATE_MACHINE.md`'s `k` is renamed.

**`CRYPTOGRAPHY.md`**

* §2.4: delete the burn layout block entirely and replace it with the table
  above, marked *"reproduced from `PROTOCOL.md` §4.5, which owns it"*. Keep the
  surrounding argument — that the map must be fixed before the shuffle chain —
  unchanged; it is correct and is `THREAT_MODEL.md` A10.
* §2.8 item 3: delete *"Burn cards are simply never opened…"* and replace with:
  *There are no burn cards. Indices `2m+5 … 51` are never opened and a reveal
  token for any of them is a protocol violation, attributed to its sender.*
* §2.4: add the recorded departure — *A burn exists to defeat physical
  marked-card and edge-sorting attacks and has no analogue here. This is a
  deliberate departure from live procedure; it costs a deck position and would
  change `index_map_hash`, so it is not a free choice per document.*

**`PROTOCOL.md`**

* §4.5: unchanged. It is the source. Add a line that `CRYPTOGRAPHY.md` §2.4 was
  corrected to match, so the divergence is not reintroduced.

**`STATE_MACHINE.md`**

* §7.8: unchanged in substance; rename `k` to `m` for one symbol across the
  corpus.

**`THREAT_MODEL.md`**

* Add the no-burn choice to the deviation register of §9.1 (C-11).

**OPEN QUESTION.** None.

---

### C-4 — `DEAL_PRIVATE`: point-to-point in `CRYPTOGRAPHY.md`, broadcast in `PROTOCOL.md`

**Ruling.** **Broadcast wins.** `PROTOCOL.md` §3.4/§4.6 and `STATE_MACHINE.md`
§7.8 both specify broadcast; `CRYPTOGRAPHY.md` §2.7 is the outlier. More
importantly, the *reason* `CRYPTOGRAPHY.md` gives — "Bob receives no token" — is
false under the shipped design, and `THREAT_MODEL.md` G1 rests on the counting
argument, not on that reason. A security document must not carry a false argument
for a true conclusion.

#### The canonical argument, to be carried verbatim

> Opening deck index `i` requires a reveal token from **every one of the `n`
> dealt-in seats**. For a hole index owned by seat `P`, every seat except `P`
> broadcasts its token, so at most `n-1` tokens for that index ever exist
> publicly. Any other seat `Q` holds only its own token, which is already among
> the `n-1`; the one missing is `P`'s, and producing it is equivalent to
> computing `sk_P` from `pk_P`. `P` completes the set with its own share and
> reads its card. At showdown `P` publishes its own token, completing the set for
> everyone — the only moment a seat ever publishes a token for its own card, and
> any earlier such token is a protocol violation.

**`CRYPTOGRAPHY.md`**

* §2.7: rewrite for broadcast. Delete *"privately, over the direct authenticated
  libp2p stream to Alice"* and delete the bullet *"Bob receives no token for
  Alice's indices from anybody"*. Insert the counting argument above.
* §2.7: keep the entitlement rule ("a receiver rejects any token for an index it
  is not entitled to") — under broadcast it changes shape: the rule is now that a
  token is legal **only for the indices due at the current stage**, and a token
  from `P` for `P`'s own hole index before showdown is the specific violation to
  reject and attribute.
* §2.9: the `DEAL_PRIVATE` line currently reads *"`n-1` tokens per hole index,
  point-to-point (131 B per token)"*. Replace with *"each dealt-in seat
  broadcasts one message carrying its token for every **other** dealt-in seat's
  two hole indices — `2(n-1)` tokens per sender, one message per sender, one
  collective stage"*, and correct the per-hand budget accordingly.
* §2.9: add the consequence `CRYPTOGRAPHY.md` does not currently carry —
  *broadcast is what makes a showdown one message per revealing seat and what
  makes mucking a policy question rather than a cryptographic one.*

**`PROTOCOL.md`**

* §3.4 and §4.6: unchanged. They are the source. Add a note that
  `CRYPTOGRAPHY.md` §2.7/§2.9 were corrected to match.

**`THREAT_MODEL.md`**

* G1 and §5.2 row 6: confirm the counting argument is the stated basis; remove
  any wording implying point-to-point delivery.

**`STATE_MACHINE.md`**

* §7.8 "Why publishing hole-card reveal tokens publicly is safe": unchanged. It
  already carries the correct argument and is the model for the other documents.

**OPEN QUESTION.** None.

---

### C-5 — `HAND_INIT` / `HAND_COMPLETE`: signed single-writer in `PROTOCOL.md`, unsigned derived in `STATE_MACHINE.md`

**Ruling.** **Neither form survives.** Both documents are half right and each is
wrong where the other is right:

* `PROTOCOL.md` is right that the event must be **signed**: `SPEC_CS.md` §12
  requires `sender_public_key` and `signature` on every critical event, and
  §4.0 step 9 would drop an unsigned one.
* `STATE_MACHINE.md` is right that there must be **no single writer**:
  `SPEC_CS.md` §4 requires the hand-to-hand transition to be deterministic and
  verifiable for every participant and *not driven by whoever acts first*, and a
  single writer holds a veto over the transition, which is an authority over
  exactly the transition §4 protects.

**Canonical form: a collective signed derived stage.** Per §0.2,
`HAND_INIT`, `HAND_COMPLETE` and `HAND_ABORT` become **collective** stages. Every
present seat computes the byte-identical body from the state before the stage,
signs its own copy under its own key, and emits it. The stage completes when
every required emitter has been heard, and `stage_hash` uses the collective form
of `PROTOCOL.md` §3.2. A copy whose body differs from the receiver's own
computation is rejected under §4.0 and the stage does not complete for that
emitter.

This closes `STATE_MACHINE.md` Q2 permanently: `sender_public_key` is the
emitting peer's own application key, there is no unsigned envelope to design, and
the "counter-signature" the question asked about *is* the mechanism — every peer
signs, so the transcript carries attributable agreement by construction.

**`PROTOCOL.md`**

* §4.4 `HAND_INIT`: *Direction* becomes *"collective stage 0 of chain
  `hand_id = k`; every seat that will be `dealt_in`, plus every occupied seat that
  is absent or sitting out and therefore posts dead money"*. Delete *"The writer
  is the seat holding the button position"*. Keep the whole "announces nothing
  and decides nothing" paragraph — it is the justification and it becomes exactly
  true.
* §4.10 `HAND_COMPLETE` and `HAND_ABORT`: same change, *Direction:* collective.
* §3.2: add `HAND_INIT`, `HAND_COMPLETE`, `HAND_ABORT` to the collective-stage
  examples and remove them from the single-writer examples. Add the stage-kind
  principle of §0.2 as a normative rule so future message types are classified
  without argument.
* §4.11 summary table: change the Stage kind and Emitter columns for `0x0303`,
  `0x0801`, `0x0802` to `collective` / `all present seats`.
* §9.3: `HAND_INIT` and `HAND_COMPLETE` payload caps unchanged; the message count
  per stage rises from 1 to `n`, which must be reflected in the hand-startup
  estimate of D-5.
* §12: close nothing here, but record that `STATE_MACHINE.md` Q2 is answered by
  this ruling.

**`STATE_MACHINE.md`**

* §3.4: rewrite. Delete *"They carry no signature, because there is no signer."*
  Replace the section with: *Some transitions have no author.
  `HAND_COMPLETE`, `HAND_INIT` for the next hand, and the settlement step are
  computed, not decided (`SPEC_CS.md` §4). The engine emits them in
  `Step::derived`; the protocol layer wraps each one in that peer's own signed
  envelope and emits it as a **collective** stage, so every peer signs a
  byte-identical body and a peer that derives something different is rejected at
  the stage rather than detected later at a checkpoint. There is no writer and no
  authority; there is also no unsigned event.*
* §3.4: delete the OPEN QUESTION block. **Q2 is closed** by this ruling.
* §11: remove Q2 from the table and add a line recording that it was closed by
  the fix plan, with the answer, so the history is not lost.
* §4.2 `Effect::Publish(DerivedEvent)`: add that the protocol layer signs it
  under the local peer's key.

**`THREAT_MODEL.md`**

* §5.2 row 9 ("fake stack") and row 14: unaffected, but any wording that implies
  a single writer serialises the hand boundary must be corrected.

**`CRYPTOGRAPHY.md`**

* §2.9: the per-hand sequence lists `HAND_INIT` as one signed event. Note that it
  is a collective stage of `n` copies.

**OPEN QUESTION.** None. Q2 is closed.

---

### C-6 — `STATE_MACHINE.md` has no representation of `SPEC_CS.md` §15's dispute path, and two abort causes have no counterpart

**Ruling.** `STATE_MACHINE.md` grows to match `PROTOCOL.md` §6 as amended by A-3
and A-6. The engine must be able to represent every terminal state the protocol
can reach, or the protocol has states the engine cannot express and the two
diverge by construction.

**`STATE_MACHINE.md`**

* §4.1 `Event`: add
  `StateHash { seat, checkpoint: u16, state_hash: Hash }`,
  `StateAck { seat, checkpoint: u16, agreed: Hash, checkpoint_hash: Hash }`,
  `Dispute { seat, kind: DisputeKind, at_sequence: u64 }`.
* §5.1: add a **20th phase**, `Diverged`, entered from any phase on observing two
  distinct `state_hash` values at one checkpoint. It is a freeze: no card opens,
  no action applies, no chips move (`PROTOCOL.md` §6.3 step 1).
* §5.2: add transitions for §6.3 steps 1–4 —
  `* | StateHash(conflicting) -> Diverged`;
  `Diverged | Dispute -> Diverged` (declare);
  `Diverged | transcript reconciliation completes, states agree -> the phase held at the checkpoint` (resume);
  `Diverged | reconciliation completes, states still differ -> HandAborted` with
  `Fault{StateDivergence}` and **restoration**, no attribution, table faulted
  (A-3, A-6);
  `Diverged | EquivocationProof -> HandAborted` with `Fault{Equivocation}` and
  forfeiture against the accused.
* §8.6 `AbortKind`: add `StateDivergence` and `Equivocation`. Record in §8.6 that
  `StateDivergence` alone uses **restoration** rather than the forfeiture formula,
  and that `attributed` is always empty for it.
* §4.1 / §5.2: `Event::RevealRejected { seat, index, reason }` is currently
  consumed by no transition, so a failed Chaum–Pedersen DLEQ verification is
  silently discarded and the hand stalls to a timeout instead of aborting. Add
  `AwaitingDeal | AwaitingBoardReveal | AwaitingShowdownReveal + RevealRejected
  -> HandAborted` with `Fault{InvalidRevealProof}`, matching T21's treatment of
  `ShuffleRejected`, `PROTOCOL.md` §4.10 `cause = 3`, and `CRYPTOGRAPHY.md` §8
  rule 1.
* §5.1 / §5.2 / §10 headline counts: **recount and restate**. Do not carry
  "19 phases, 26 invariants, 47 transitions" forward; the phases become 20, the
  invariants gain I27 (A-7) and I28 (C-9), and the transition count changes by
  the additions above. State the new numbers once, in §5.1, and reference them
  elsewhere rather than repeating them.

**`PROTOCOL.md`**

* §6.3 and §6.4: already being edited by A-3 and A-6. Add a pointer to
  `STATE_MACHINE.md`'s `Diverged` phase so the two are visibly the same machine.
* §4.10 `cause = 3`: confirm it is reachable from the engine's
  `Fault{InvalidRevealProof}`.

**OPEN QUESTION.** None.

---

### C-7 — The join flow uses a different transport and protocol string in each document

**Ruling.** **`request-response` on `/p2p-poker/join/1` wins**
(`NETWORK_STACK.md`). Join is a one-shot RPC with a natural timeout and free size
caps; it avoids opening a table stream to a peer that has not been admitted,
which `NETWORK_STACK.md` §8.4's membership gate would otherwise have to
special-case; and after A-4 the join messages are unchained anyway, so they do
not belong on the chained table stream.

**`PROTOCOL.md`**

* §1.1: add `/p2p-poker/join/1` to the list of protocol strings for
  `PROTOCOL_MAJOR = 1`. There are now **five**, not four.
* §1.4: add a fourth channel row —
  *Join RPC | `request_response::cbor::Behaviour` on `/p2p-poker/join/1` |
  codec's own framing; `JOIN_REQ_MAX` / `JOIN_RESP_MAX` per §13; 20 s timeout.*
* §4.3: change the channel heading from *"(channel: table stream)"* to
  *"(channel: join RPC, except `PLAYER_LIST` and `TABLE_READY`)"*.
  `JOIN_REQUEST`, `JOIN_ACCEPT`, `JOIN_REJECT` move to the join RPC.
  `PLAYER_LIST` stays on the table mesh (it is a broadcast to admitted peers) and
  `TABLE_READY` stays a collective chained stage.
* §4.11: update the Channel column for `0x0201`, `0x0202`, `0x0203`.
* §13: add `JOIN_PROTOCOL`, `JOIN_REQ_MAX`, `JOIN_RESP_MAX`.
* §1.4: the "wrong channel is dropped" rule now has a fourth channel to be
  correct about; restate it as a table so it cannot go stale.

**`NETWORK_STACK.md`**

* §5.5, §8.4, §14: unchanged in substance; sizes change per C-1
  (4 096 / 16 384).

**OPEN QUESTION.** None.

---

### C-8 — Lobby chat exists in the transport document and in nothing else

**Ruling.** **Chat is kept.** `SPEC_CS.md` §22 requires a chat pane in the lobby
and is binding; deleting the topic would put the transport document in conflict
with the spec instead of with `PROTOCOL.md`. `PROTOCOL.md` gains the message
type.

Chat is the one message class that is pure attacker-controlled UTF-8 reaching a
UI, so the §9.4 string rules are applied to it explicitly rather than by
inheritance.

**`PROTOCOL.md`**

* §4 / §7: add **`0x0106 LOBBY_CHAT`**, channel *lobby broadcast* on
  `LOBBY_CHAT_TOPIC = /p2p-poker/lobby-chat/1`, unchained
  (`chain_scope = 0` per §0.1), signed by the sender's application key.

  | Field | Type | Limit / rule |
  |---|---|---|
  | `n(0) display_name` | `bytes` | <= 32 B UTF-8, §9.4 string rules |
  | `n(1) text` | `bytes` | <= 512 B UTF-8, §9.4 string rules |
  | `n(2) timestamp_unix_ms` | `u64` | advisory; strictly greater than this sender's previous, for anti-replay |

  Payload cap `LOBBY_CHAT_MAX = 2 048 B`. Both string fields are display-only,
  never identifiers, never parsed, never inputs to a state transition, and must
  reject control characters `U+0000`–`U+001F`, `U+007F` and the bidi overrides
  `U+202A`–`U+202E`, `U+2066`–`U+2069` — the §9.4 rules, restated at the message
  rather than inherited, because this is the field most likely to be attacked.
* §7.6: add the rate limit — 1 message per 2 s, burst 5, per remote `PeerId`,
  matching `NETWORK_STACK.md` §6.6.
* §4.11: add the row. Note that chat is on the chat topic, not the lobby topic,
  so the "wrong channel" rule covers it.
* §13: add `LOBBY_CHAT_TOPIC` and `LOBBY_CHAT_MAX`.
* §4.11's note about `0xF000`–`0xFFFF`: unchanged; chat now has a real code, so
  the gap the review identified is closed.

**`NETWORK_STACK.md`**

* §6.1, §6.5, §6.6: unchanged in substance. §6.5's 2 048 B cap is the payload
  cap and must be labelled as `LOBBY_CHAT_MAX`.
* §6.4: chat goes through the same validation order; add it to the per-type cap
  step.

**`THREAT_MODEL.md`**

* §5.3: chat is attacker-controlled text reaching a UI. Add it to X15's scope
  (lobby spam) and add one sentence to §6 recording that chat carries no security
  claim of any kind: it is not authenticated as coming from any particular
  *player* beyond the application key, and impersonation by display name is
  trivial and expected.

**OPEN QUESTION.** None.

---

### C-9 — Invariant I1 (chip conservation) is false for cash mode

**Ruling.** I1 becomes a **ledger identity**, and the buy-ins and cash-outs that
make it true become signed chained events so the ledger is itself verifiable. The
old I1 survives as a *corollary* in tournament mode, not as the definition.

#### The canonical invariant

```
I1  (ledger identity)
    Σ_s stack[s] + Σ_s committed_hand[s] == ledger_in − ledger_out

    ledger_in   = Σ over every accepted seat-entry of its buy-in
    ledger_out  = Σ over every accepted seat-exit of the stack it removed

    Corollary, tournament mode: no entry or exit occurs after the first hand,
    so the right-hand side is constant and equals players_at_start × start_stack
    — which is the old I1, now derived rather than assumed.

    Cash mode: the right-hand side changes only at a hand boundary (T47).
```

`I2` (step-local conservation) is unchanged and already carries the per-event
half. New **I28**: *`ledger_in` is monotonically non-decreasing, `ledger_out` is
monotonically non-decreasing, and both change only at a hand boundary.*

**`STATE_MACHINE.md`**

* §10 I1: replace with the block above. Delete
  `total_chips = players_at_start × start_stack, constant for the table's whole
  life`.
* §10: add I28 as stated. (I27 comes from A-7.)
* §2.6 `TableState`: add `ledger_in: Chips` and `ledger_out: Chips` as canonical
  state fields — they must be inside `STATE_HASH`, or two peers can disagree
  about the ledger and never detect it.
* §9.4: unchanged in substance; add that a `PlayerSeated` applied at T47
  increments `ledger_in` by its buy-in and a `PlayerLeft` applied at T47
  increments `ledger_out` by that seat's stack, both recorded in the derived
  `HAND_INIT` of the next hand.
* §10 I5 (`stack[s] + committed_hand[s] == start_stack_this_hand[s]`): unchanged,
  and it is what makes I1 checkable within a hand.

**`PROTOCOL.md`**

* §4.4 `HAND_INIT`: add `n(11) ledger_delta: Vec<(u8, i64)>` — the per-seat
  ledger change applied at this hand boundary, ascending by seat, positive for a
  buy-in and negative for a departing stack, recomputed by every receiver from
  the accepted `JOIN`/`PLAYER_LEAVE` events and rejected on mismatch. This keeps
  the ledger inside an existing message and needs no new message type.
* §6.1 `PublicTableState`: add `ledger_in` and `ledger_out` so they are inside
  `state_hash`.
* §4.10 `HAND_COMPLETE`: the check `sum(final_stacks) == total_chips_in_play`
  becomes `sum(final_stacks) + sum(committed_hand) == ledger_in − ledger_out`.

**`THREAT_MODEL.md`**

* G9: replace *"`sum(stacks) + sum(committed_this_hand)` is constant for the
  whole tournament"* with the ledger identity above. Keep *"This must hold on
  **every** path, including a D-005 abort with forfeiture"*, and add *"and on the
  restoration paths of `cause = 4` and the no-attribution aborts of A-1/A-7"*.

**OPEN QUESTION.** None.

---

### C-10 — `CERT_SETTLE_MS` puts a wall-clock read back into the chain-building rule

**Ruling.** Q-04's own second option is adopted: **the certificate stage is
collective**. This follows directly from §0.2 — a certificate's body is a pure
function of the votes, so it carries no choice — and it removes the timer, the
tie-break and the fork in one edit. `CERT_SETTLE_MS` is **deleted**.

The required emitter set for the certificate stage is `V(subject)`, the same set
that had to vote. Each voter emits its own `TIMEOUT_CERT` carrying the same votes
in the same order; the bodies differ only in `sender_public_key` and
`emitted_at_unix_ms`, which no longer matters because `stage_hash` is the
collective form over the whole set.

**One residual must be stated rather than hidden.** A stage can now close two
ways — by the subject's own event, or by certificates — and which one happened is
chain content. Two peers agree on it only if at least one required voter is
honest and refuses to vote after accepting the action (A-2). At `n >= 3` that is
the honest-voter assumption already in force. At `n = 2` there is no such voter
and D-007's advisory rule applies, so the ambiguity cannot arise: no certificate
takes effect.

**`PROTOCOL.md`**

* §4.8 `TIMEOUT_CERT`: delete the whole "Several peers may each assemble a
  certificate…" paragraph including *"the certificate that closes the stage is
  the one from the lowest seat index that emitted a valid one within
  `CERT_SETTLE_MS`, and every peer waits that long before chaining"*. Replace
  with: *The certificate stage is **collective**. The required emitter set is
  `V(subject)`; each voter emits its own certificate; `stage_hash` is the
  collective form of §3.2 over `event_class = 2`. There is no timer, no
  tie-break and no assembler privilege.*
* §4.11: `0x0602 TIMEOUT_CERT` Stage kind `single` -> `collective`, Emitter
  `lowest-seat assembler` -> `required voters`.
* §13: delete `CERT_SETTLE_MS = 2 000`.
* §8.2: add the residual paragraph above, so the honest-voter dependency is
  visible where the deadline machinery is described.
* §12: **close Q-04**, recording the answer.

**`STATE_MACHINE.md`**

* §8.4: record that the certificate arrives as a collective stage; the engine's
  `TimeoutCertificate` event is unchanged, but `signers` is now derivable from
  the stage's own emitter set as well as from the embedded votes, and both must
  agree.
* §3.1: the "no clock inside the engine" rule is now true without exception;
  say that `CERT_SETTLE_MS` was the one violation and is gone.

**`THREAT_MODEL.md`**

* A13: add that with the certificate stage collective there is no local timer
  anywhere in the chain-building rule.

**OPEN QUESTION.** None. Q-04 is closed.

---

### C-11 — `RNG_COMMIT`/`RNG_REVEAL` move from per hand to once per table, and the deviation is not recorded

**Ruling.** The deviation is **correct on the merits and stands** — the shuffle
chain is the per-hand distributed randomness, `research/MENTAL_POKER.md` §6
establishes it, and a per-hand beacon would add two stages of latency and buy
nothing. What is wrong is that `SPEC_CS.md` §36 and the Phase 0 brief require a
deviation from a binding spec section to be *recorded*, and this one is absorbed
rather than recorded.

**One deviation register, in `THREAT_MODEL.md` §9.1**, listing every recorded
deviation in one place, in the form already used for the `OsRng` rename:

| # | Spec section | Deviation | Recorded in |
|---|---|---|---|
| 1 | §7 | `OsRng` no longer exists; the OS CSPRNG is `getrandom::SysRng`. A rename, not a weakening. | `CRYPTOGRAPHY.md` §7.2 |
| 2 | §16 | `RNG_COMMIT` / `RNG_REVEAL` run **once per table** (setup chain), not per hand. The shuffle chain is the per-hand randomness; the beacon covers seating and the initial button only. | `PROTOCOL.md` §4.4, `CRYPTOGRAPHY.md` §7.3 |
| 3 | live procedure | **No burn cards** (C-3). Not a spec deviation — `SPEC_CS.md` does not mention burns — but a departure from live procedure, recorded because it changes `index_map_hash`. | `PROTOCOL.md` §4.5 |
| 4 | §19 / TDA | An absent seat posts its blind as dead money, takes no cards, and cannot win the hand it pays for (D-005). | `STATE_MACHINE.md` §12, `THREAT_MODEL.md` §7.3 |
| 5 | §4 / D-006 | At two seats an action deadline is advisory and produces no signed transition (D-007), so the hand-to-hand progress guarantee of §4 depends on both players' clients cooperating. | `PROTOCOL.md` §8.3, this plan §0.3 |

**`PROTOCOL.md`**

* §4.4: the "Where `RNG_COMMIT` / `RNG_REVEAL` sit, and why" block is already the
  full argument. Add the heading **"Recorded spec deviation"** to it, in the same
  form `CRYPTOGRAPHY.md` §7.2 uses for `OsRng`, and a pointer to
  `THREAT_MODEL.md` §9.1's register.

**`CRYPTOGRAPHY.md`**

* §7.3: add the same "Recorded spec deviation" note at the head of the section.

**`THREAT_MODEL.md`**

* §9.1: add the deviation register above as a new subsection §9.1.1, before the
  permanent-limitations list.

**`STATE_MACHINE.md`**

* §7.9 and §12: point at the register rather than restating each deviation.

**OPEN QUESTION.** None.

---

## D findings — uncovered `SPEC_CS.md` sections

Each is assigned to exactly one document, or explicitly deferred with the phase
and the destination named. A section with no owner is how a requirement gets
lost, so "later" is only acceptable when the destination is written down.

### D-1 — `SPEC_CS.md` §24 (`InMemoryTransport`) is named once and specified nowhere

**Ruling.** **Owner: `NETWORK_STACK.md`, §1.3, expanded into a full
specification.** It belongs there because §1.3 is where the narrow upward trait is
already asserted, and because that trait is what makes the substitution possible.
It is not deferred: `SPEC_CS.md` §24 is the only vehicle for §25's adversarial
suite, and every A-class finding in the review is the kind of defect a
conflicting-message and timeout-injection harness would have caught at
specification time.

**`NETWORK_STACK.md`**

* §1.3: specify the upward trait exactly — at minimum
  `send(PeerId, Bytes) -> Result<()>`,
  `events() -> Stream<TransportEvent>`,
  `connection_state(PeerId) -> ConnectionState { Direct | Relayed | Down }`,
  and the `TransportEvent` alphabet (`Connected`, `Disconnected`, `Message`,
  `ListenerClosed`). Nothing libp2p-typed may appear above this trait.
* §1.3: specify `InMemoryTransport` with
  (i) a **seeded, deterministic scheduler** — determinism is a requirement, not
  a nicety, because `STATE_MACHINE.md` I22 asserts replay determinism and an
  under-determined harness produces flaky adversarial tests that get muted;
  (ii) the eight injection modes §24 names — delay, duplicate, packet loss,
  reordered events, disconnect, reconnect, malicious packets, conflicting
  messages;
  (iii) how a **conflicting message** is injected, since that is the one mode
  with a protocol meaning: the harness must be able to hand two different
  `SignedEvent`s for one `(sender, table_id, hand_id, sequence, event_class)` to
  two different receivers, which is exactly the `EquivocationProof` input of
  A-4's canonical predicate;
  (iv) the acceptance bar of §24 — thousands of hands automatically, no DHT, no
  libp2p.
* §12: add a line that the in-memory path is the only one exercised before
  Phase 7, so an unspecified harness blocks Phases 3–6.

**OPEN QUESTION.** None.

---

### D-2 — `SPEC_CS.md` §28's dependency register exists for exactly one crate

**Ruling.** **Owner: a new document, `docs/DEPENDENCIES.md`**, with the six §28
columns — name, version, purpose, repository, licence, security status — for
every crate the client links, generated from `cargo metadata` and checked in CI so
it cannot drift. A register that is hand-maintained will be wrong within a month,
and B-3 is the proof.

Until that document exists, the two authoritative documents carry their own side
with all six columns, and neither may claim to be complete:

* `CRYPTOGRAPHY.md` §9 — the cryptographic side: `ziffle 0.1.0`, `ark-*`,
  `rand` / `rand_chacha` / `rand_core` (B-1, B-3), `blake3`, `ed25519-dalek`,
  `sha2`, `getrandom`, `minicbor`, `zeroize`.
* `NETWORK_STACK.md` §5.1 — the transport side: `libp2p 0.56.0` and every
  sub-crate actually enabled, `libp2p-stream 0.4.0-alpha`, `mainline 8.0.0`,
  `web-time`.

`libp2p-stream 0.4.0-alpha` and `ziffle 0.1.0` must be flagged **explicitly** in
both places as the two unaudited, semver-unstable entries.

**OPEN QUESTION.** None.

---

### D-3 — `SPEC_CS.md` §25's eleven named malicious peers are never mapped to tests

**Ruling.** **Owner: `THREAT_MODEL.md`, new §5.5.** It belongs there because §25
requires each cheater to prove *"cryptographically impossible **or**
unambiguously detected and the hand stopped, **per the threat model**"*, and the
threat model is the only document that holds the CP/D&A classification the
assertion is measured against. Putting the map anywhere else separates the claim
from its evidence.

**`THREAT_MODEL.md`**

* New §5.5, a four-column table: **cheater name -> catalogue row -> test module ->
  asserted outcome (CP or D&A)**. All eleven of `CheaterDuplicateAce`,
  `CheaterReplaceCard`, `CheaterInvalidShuffle`, `CheaterPredictableRNG`,
  `CheaterReplayAction`, `CheaterIllegalRaise`, `CheaterFakeStack`,
  `CheaterEquivocation`, `CheaterReadOpponentCard`, `CheaterFutureBoard`,
  `CheaterDisconnect`, plus §25's two mandatory standalone tests (a modified
  client cannot obtain plaintext opponent hole cards from data it legitimately
  received; one malicious player cannot choose the future board by manipulating
  the last RNG/shuffle step).
* `CheaterPredictableRNG` has no test described anywhere in the corpus. Its row
  must name a real assertion: the shuffle chain is uniform if **one** shuffler is
  honest, so the test seeds every cheater's RNG from a fixed value, leaves one
  honest shuffler, and asserts the final deck order is not predictable from the
  cheaters' seeds. Record that this is a statistical test, not a proof.
* §4: the falsifier lines for G1–G7 currently name nine cheaters informally;
  make them point at §5.5's rows rather than repeating the names.

**`CRYPTOGRAPHY.md`** §12 item 8 and **`STATE_MACHINE.md`** §10's three
adversarial notes stay where they are and gain a pointer to §5.5, so the map has
one home and the two library-level lists are visibly subordinate to it.

**OPEN QUESTION.** None.

---

### D-4 — `SPEC_CS.md` §23 (source architecture) is covered by no Phase 0/1 document

**Ruling.** **Deferred to Phase 2, destination `docs/ARCHITECTURE.md`.** The full
tree is Phase 2 work and writing it now would be speculation. But §23's binding
part — *preserve the separation of transport, poker engine and cryptographic
deck* — must have an owner today, because the five specification documents are
what that separation will be built from.

**Interim, applied now:** each of the five documents gains one line in its §1
naming the module it governs, so the mapping exists before the tree does:

* `NETWORK_STACK.md` -> `src/net/` (`dht.rs`, `swarm.rs`, `lobby.rs`, `streams.rs`)
  and the `InMemoryTransport` behind the same trait (D-1);
* `PROTOCOL.md` -> `src/protocol/` (`messages.rs`, `serialization.rs`,
  `signatures.rs`, `transcript.rs`);
* `STATE_MACHINE.md` -> `src/poker/` (`state.rs`, `engine.rs`, `actions.rs`,
  `pots.rs`, `tournament.rs`, `evaluator.rs`);
* `CRYPTOGRAPHY.md` -> `src/mental_poker/` (`protocol.rs`, `deck.rs`,
  `shuffle.rs`, `proofs.rs`, `reveal.rs`) and `src/security/`;
* `THREAT_MODEL.md` -> no module; it governs `tests/adversarial/` and
  `tests/fuzz/`.

`docs/ARCHITECTURE.md` at the start of Phase 2 carries the whole §23 tree
including `gui/`, `storage/` and `assets/`.

**OPEN QUESTION.** None.

---

### D-5 — `SPEC_CS.md` §33's profiling list is only half covered

**Ruling.** **Owner: `CRYPTOGRAPHY.md` §6.5**, which already holds the four
measured metrics; it gains a measurement plan for the three that are missing.
Separately, every quotation of the *estimated* hand-startup latency must be
marked as an estimate wherever it appears — an estimate repeated as a bare number
in three documents becomes a measurement by attrition.

**`CRYPTOGRAPHY.md`**

* §6.5: add a table of §33's seven targets with, for each, either the measured
  figure or the phase in which it will be measured:

  | §33 target | Status |
  |---|---|
  | shuffle generation | measured, 94–111 ms |
  | shuffle verification | measured, 37–43 ms |
  | private deal | measured, within the 1.39–4.34 ms per-card reveal |
  | board reveal | measured, 1.39–4.34 ms per card |
  | **signature verification** | **not measured** — Phase 4, one `verify_strict` per event, and the per-hand event count from `PROTOCOL.md` §3 |
  | **network latency** | **not measured** — Phase 8, two-network test; `NETWORK_STACK.md` §12.12 confirms none has been run |
  | **hand startup latency** | **estimated only** — ~340 ms heads-up, ~1.1 s six-handed at an assumed 100 ms RTT. Phase 8 measures it. |

* §6.5: the hand-startup estimate must additionally be revised for C-5 — making
  `HAND_INIT` collective replaces one message with `n`, which adds one collective
  round trip to the estimate.

**`PROTOCOL.md`**

* §3.2 and §4.4: mark the ~340 ms / ~1.1 s figures as **estimates at an assumed
  100 ms RTT**, every time they appear, and point at `CRYPTOGRAPHY.md` §6.5.
* §4.0 step 9: *"one Ed25519 verification"* — add that no figure has been
  measured and name Phase 4.

**`NETWORK_STACK.md`**

* §12.12: unchanged; it already states honestly that no two-network test has
  been run. Add a pointer from `CRYPTOGRAPHY.md` §6.5's table.

**OPEN QUESTION.** None. Three measurement obligations, not open questions.

---

### D-6 — `SPEC_CS.md` §31 (Git discipline) is covered by no document

**Ruling.** **Owner: a new `docs/CONTRIBUTING.md`**, created at the close of
Phase 1. §31 is a process requirement, not a protocol one, and putting process
rules inside a wire specification is how they get skipped. But its load-bearing
clause is security-critical and must be visible from the two documents that
enumerate security-critical tests.

**`docs/CONTRIBUTING.md`** carries §31 in full: small logical commits; no
deletion or rewrite of large parts of a working implementation without written
justification; and — the load-bearing part — **a regression test that reproduces
the problem must land before any security-critical change**.

**`CRYPTOGRAPHY.md`** §12 and **`PROTOCOL.md`** §9.6 each gain one line: *Every
change to anything enumerated in this section is a security-critical change under
`SPEC_CS.md` §31 and requires a reproducing regression test to land first. See
`docs/CONTRIBUTING.md`.*

**OPEN QUESTION.** None.

---

### D-7 — The founder leaving before `TABLE_READY`, and `min_players_to_start = 10` versus heads-up first

Two gaps, one entry, two owners.

**(i) The founder disappears during formation.**

**Ruling. Owner: `PROTOCOL.md` §4.3**, with the state-machine side already
present at `STATE_MACHINE.md` T4 and only needing to be connected. The correct
outcome is the one T4 already reaches — the table never starts — and the ruling's
job is to say what happens to the lobby entry and to the `JOIN_ACCEPT` holders.

The behaviour, which is safe because nothing has been chained yet: **no chips
move, because a buy-in enters the ledger only at the first `HAND_INIT`** (C-9),
and formation produces no chained events except `TABLE_READY`, which by
definition has not happened.

* `PROTOCOL.md` §4.3: add — *If `TABLE_READY` has not completed within
  `join_deadline_ms` of the first `JOIN_ACCEPT`, formation is abandoned. Every
  holder of a `JOIN_ACCEPT` discards it and its seat reservation; no chips have
  moved, because a buy-in enters the ledger only at the first `HAND_INIT`. The
  advertisement is not revoked by anyone — `LOBBY_TABLE_REMOVE` requires the
  table key, which is exactly what is missing — and it disappears from every
  lobby by `AD_TTL_MS` with no cooperation from anybody. A client that holds a
  `JOIN_ACCEPT` for a table whose advert has expired must not display the table
  as joinable.*
* `STATE_MACHINE.md` T4: add the cross-reference to the above, so the engine's
  `TableClosed` and the lobby's TTL expiry are visibly the same event.
* `NETWORK_STACK.md` §10.3: confirm that TTL expiry is the only cleanup path and
  that no peer may revoke another's advert.

**(ii) `RATED_SNG_POKERTH_V1` pins `seats = 10`, but `SPEC_CS.md` §32 requires
heads-up first.**

**Ruling. Owner: `PROTOCOL.md` §13 and `STATE_MACHINE.md` §9.5.** Both must state
it explicitly rather than leave it to inference; it is a spec-mandated preset that
the spec-mandated first mode cannot play, and a reader who does not notice will
build the wrong acceptance test.

* `PROTOCOL.md` §13, under the preset block: *`RATED_SNG_POKERTH_V1` is fully
  specified and is **not playable by the MVP**: it pins `seats = 10` and
  `min_players_to_start = 10`, while `SPEC_CS.md` §32 requires two-player heads-up
  as the first supported mode and §1.3 scopes the MVP at `nlhe/2-6`. The MVP
  ships `CUSTOM` tables; `RATED_SNG_POKERTH_V1` becomes playable when
  `nlhe/7-10` lands.*
* `STATE_MACHINE.md` §9.5: the same statement, next to the heads-up reachability
  analysis it already carries.
* `NETWORK_STACK.md` §12.12 and the D-003/D-004 acceptance criterion: state that
  the Phase 8 acceptance test uses a **`CUSTOM` two-seat table**, so the test the
  decisions are judged by is the configuration the MVP actually ships.

**OPEN QUESTION.** None.

---

## Summary registers

### Open questions this plan opens or restates

| ID | Where it must be recorded | One-line question |
|---|---|---|
| **OQ-A** | `PROTOCOL.md` §12, `THREAT_MODEL.md` §9.2, `DECISIONS.md` open list (proposed `D-008`) | At `n = 2`, does an unfinishable hand restore stacks (rage-quit escape) or forfeit (chip theft by a false deadline)? Restoration is the interim default. |
| **OQ-B** | `PROTOCOL.md` §12, `DECISIONS.md` open list | A dispute path that does not require the accused peer's signature (D-007 point 4). |
| **OQ-C** | `PROTOCOL.md` §12 | Should a voter publish a signed `ACTION_SEEN` before it may vote, so a lying voter becomes provable? Not adopted. |
| **OQ-D** | `PROTOCOL.md` §12, `THREAT_MODEL.md` §9.2, `STATE_MACHINE.md` §11 | On `cause = 4`, restore, forfeit an unnamed party, or settle from the last agreed checkpoint? Restoration is the interim default. |
| **OQ-E** | `THREAT_MODEL.md` §9.2 (blocking), `PROTOCOL.md` Q-02, `STATE_MACHINE.md` Q3 | How is a multi-subject deadline certificate built and whom does it attribute? Interim: abort with no attribution and no chip movement. |

Restated, not opened: `THREAT_MODEL.md` OQ12 (A-8), `CRYPTOGRAPHY.md` OQ-3 /
`THREAT_MODEL.md` OQ3 (C-2), `CRYPTOGRAPHY.md` OQ-8 (B-1).

Closed by this plan: `PROTOCOL.md` **Q-04** (C-10), `STATE_MACHINE.md` **Q2**
(C-5). `PROTOCOL.md` Q-02 and `STATE_MACHINE.md` Q3 stay open but now have a
written safe default (A-7).

### Documents that must be recounted after editing

`THREAT_MODEL.md` §5.4 and §9.3 (X11 removed, X29 added, X7 reclassified, D-3's
new §5.5). `STATE_MACHINE.md` §5.1 and §10 ("19 phases, 26 invariants, 47
transitions" all change). Do the recount once, at the end, not per finding.

### The one thing an editor must not do

Do not repair a claim this plan says to delete. Six of the eight A findings are
claims that were stronger than the evidence, and in five of them the honest
outcome is a weaker statement plus an open question, not a cleverer construction.
`SPEC_CS.md` §36 applies to the agreement layer — deadlines, divergence,
equivocation — as much as to the cryptography, and that layer is the one part of
this design with no published construction behind it.
