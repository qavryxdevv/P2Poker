# PHASE1_VERIFY.md — adversarial verification of the fixes applied to the five specification documents

**Scope.** Every one of `PHASE0_REVIEW.md`'s 30 findings, checked against the
current text of `docs/THREAT_MODEL.md`, `docs/PROTOCOL.md`, `docs/CRYPTOGRAPHY.md`,
`docs/NETWORK_STACK.md`, `docs/STATE_MACHINE.md`. Plus five independent sweeps the
editors were not asked to run. Nothing here is a style objection.

**Method.** Every claim about a crate that a verdict turns on was re-read in the
unpacked source under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`. Where a
document and the crate disagree, the crate wins and the line numbers are given.

## Verdict counts

| Verdict | Count | Findings |
|---|---:|---|
| **RESOLVED** | 24 | A-1, A-2, A-3, A-4, A-5, A-6, A-7, B-1, B-4, C-1, C-2, C-3, C-4, C-5, C-7, C-8, C-9, C-10, C-11, D-1, D-3, D-4, D-5, D-7 |
| **PARTIAL** | 5 | B-2, B-3, C-6, D-2, D-6 |
| **UNRESOLVED** | 0 | — |
| **REGRESSED** | 1 | A-8 |

Nine defects that no finding named are recorded as **N1 … N9** below. Two of them
(**N1**, **N2**) are as severe as the A-class findings the review raised; **N3** is
worse than the A-1 it descends from, because it is an unfixed instance of A-1 at
`n >= 3`.

---

# Part 1 — Defects found by this verification

Ordered by severity, not by class.

## N3 — `PROTOCOL.md` §8.4's exclusion rule collapses `|V|` to one signer at any table size, reinstating A-1 at `n >= 3`

**Severity: highest.** This is the A-1 attack, unfixed, outside the `n = 2` scope
that D-007 and every edit above address.

`PROTOCOL.md` §8.3 defines the required voter set:

> "The set is the same for `kind = 1` … and `kind = 2` …, minus any seat already
> attributed by an earlier certificate in this hand, and **minus any seat excluded
> by §8.4**."

and §8.4 supplies the exclusion:

> "Rule: a seat already named as the subject of an outstanding, older unmet
> deadline is excluded from `V`."

Nothing bounds how many seats may be excluded, and nothing requires that the
"outstanding unmet deadline" ever produced a certificate. A seat becomes "named as
the subject" of a deadline by somebody emitting a `TIMEOUT_VOTE` against it —
`PROTOCOL.md` §4.8 — and §8.3 concedes in its own words that a voter which lies
about what it saw is unprovable:

> "**There is no artefact that settles the race when a voter lies about what it
> saw.**"

**Concrete attack, six-seat table, one modified client.** Mallory holds seat 1;
seats 2–6 are honest.

1. At some collective crypto stage `s`, Mallory emits `TIMEOUT_VOTE` against seats
   2, 3, 4 and 5. Each vote is individually legal on its face; none reaches
   unanimity, so each is an *outstanding, older unmet deadline*.
2. At stage `s+1` Mallory declares seat 6 the subject. By §8.3,
   `V(6) = {1,2,3,4,5} \ {2,3,4,5} = {1}`.
3. Mallory votes, assembles a complete `kind = 2` certificate from her own single
   signature, and the certificate stage completes because the required emitter set
   is `V(6) = {Mallory}`.
4. Per §8.3, `kind = 2` at `n >= 3` → `HAND_ABORT cause = 1`, seat 6 attributed,
   seat 6's committed chips forfeited under D-005 and distributed to the rest.

This is exactly what D-007 forbids — *"any adjudication that requires one peer's
unilateral assertion"* — and §8.3's normative rejection rule is scoped only to
`n = 2`:

> "**Normative — no `kind = 1` certificate at two seats.**"
> "**Normative — a `kind = 2` certificate at two seats attributes nobody.**"

`STATE_MACHINE.md` §8.4 validity rule 6 has the same `|deck.participants| == 2`
scope, so the engine accepts the certificate too. The `|V| = n - 1` table printed
in three documents is therefore **not** the operative rule: `|V|` is
`n - 1 - |excluded|`, and the attacker controls `|excluded|`.

**Correction.** Either (i) delete the §8.4 exclusion rule and accept that
simultaneous failures deadlock into the `hand_deadline_ms` abort with
`attributed = []` — which §8.4 already specifies as the interim behaviour anyway,
so the exclusion buys nothing that the safe default does not already provide — or
(ii) make the normative rules of §8.3 depend on `|V|` rather than on `n`: *a
certificate whose required voter set has fewer than two members is invalid for
`kind = 1` and carries `attributed = []` for `kind = 2`.* Option (ii) is a
two-line change and closes the whole class. `STATE_MACHINE.md` §8.4 rule 6 must be
rephrased on `|V|` in the same edit.

---

## N1 (= A-8 REGRESSED) — "`max_circuit_bytes` is per circuit **and per direction**" is false; the fix pass propagated it into four documents

The original A-8 defect (table-wide traffic compared against a per-circuit cap) is
corrected everywhere. In its place the fix plan's normative table introduced a new
claim that the crate source falsifies.

`libp2p-relay 0.21.1` relays a circuit with one `CopyFuture` holding **one**
counter, incremented by **both** directions before the comparison:

```rust
// libp2p-relay-0.21.1/src/copy_future.rs:41-48, 73-77, 88-104
pub(crate) struct CopyFuture<S, D> { src, dst, max_circuit_duration: Delay,
                                     max_circuit_bytes: u64, bytes_sent: u64 }
...
if this.max_circuit_bytes > 0 && this.bytes_sent > this.max_circuit_bytes { … }
let src_status = match forward_data(&mut this.src, &mut this.dst, cx) { … this.bytes_sent += i … };
let dst_status = match forward_data(&mut this.dst, &mut this.src, cx) { … this.bytes_sent += i … };
```

The cap is per circuit and **bidirectional**. A-8's citations
(`src/behaviour.rs` `impl Default for Config`, `src/behaviour/handler.rs`)
establish the value and its delivery to each circuit; they do not reach the
accounting.

`NETWORK_STACK.md` §16.1 records exactly this objection and it is **well founded**
— but the corrected numbers were not applied anywhere, so the document now
contradicts itself, and three other documents carry the wrong figure with no
objection attached:

| Where | Text carried |
|---|---|
| `CRYPTOGRAPHY.md` §6.5 (l. 1027–1038) | "A relay's `max_circuit_bytes` is a **per-circuit, per-direction** budget" … "Hands per direction … **~14**" |
| `CRYPTOGRAPHY.md` §14 cross-reference table (l. 1849) | "8 979 B per hand per circuit per direction" |
| `PROTOCOL.md` §9.3 (l. 2834, 2841–2842) | "**per-direction** cap" … "one circuit, one direction … 8 979 B" … "~14" |
| `THREAT_MODEL.md` §3.5 (l. 132, 497–501) | "128 KiB per circuit per direction" … "roughly 14 hands" |
| `THREAT_MODEL.md` X20 (l. 758) | "per circuit and per direction … roughly 14 hands" |
| `THREAT_MODEL.md` OQ12 (l. 1307) | "over **one relayed circuit, per direction**" … "~10 KB per direction per hand" |
| `NETWORK_STACK.md` §9.5 (l. 1330–1331), OQ-7 (l. 1919) | same, and then contradicted by §16.1 |

The right figures are `2 × 8 979 = 17 958 B` per circuit per hand and **~7 hands**,
and OQ12 / OQ-7 must ask for a bidirectional measurement, or the person who runs it
will measure one direction and report twice the headroom. The *conclusion* — that
duration, not bytes, is the binding public-relay limit — survives at 7 hands as at
14, which is why this is a regressed claim rather than a broken decision. But
`SPEC_CS.md` §36 forbids carrying a claim stronger than its evidence, and this one
is now in four documents.

**Correction.** Apply `NETWORK_STACK.md` §16.1's numbers to all seven places above,
delete "per direction" from every one of them, and restate OQ12 / OQ-7 as a
bidirectional measurement. `8 979 B` per shuffler and `(n−1) × 8 979 B` as a peer's
total per-hand outbound are both correct and unaffected.

---

## N2 — "publicly / deterministically adjudicable offline" is asserted in five places and has no referent

`THREAT_MODEL.md`'s Objection 1 raises this and it is **well founded**. Recorded
here as a defect rather than only as an objection, because the claim now appears in
five places across three documents and softens two DNA classifications:

| Where | Text |
|---|---|
| `PROTOCOL.md` §6.3 case (c) | "is **publicly adjudicable offline** by anyone who runs the reference engine over it, forever" |
| `PROTOCOL.md` §6.4 | "the evidence is deterministically adjudicable offline by any third party running the reference engine over the unanimous transcript" |
| `PROTOCOL.md` §11.3 | same sentence again |
| `THREAT_MODEL.md` X29 | "**deterministically adjudicable offline** by any third party running the reference engine over it, forever" |
| `THREAT_MODEL.md` §9.1.2 limitation 5 | "the evidence is deterministically adjudicable offline" |

A-3 and A-6 both refuse live attribution on the ground that **no
observer-independent derivation exists** — every peer derives with its own engine,
so "attribute whoever disagrees" is a vote over the facts, which `SPEC_CS.md` §15
forbids. Moving the same derivation offline does not manufacture the missing
reference. It would if a canonical reference engine existed, were identified, were
versioned against `protocol_version`, and were agreed authoritative. No document in
the corpus owns one: `STATE_MACHINE.md` specifies *the* deterministic engine as a
thing every client implements, not as a normative artefact a third party runs, and
D-4 defers the source tree to `docs/ARCHITECTURE.md` in Phase 2, so there is not
even a named binary. `PROTOCOL.md` Q-06 already notices the dependency from the
other side ("It is the only thing that makes §11.3's 'publicly adjudicable offline'
real") without noticing that the engine, not the transcript, is the missing half.

**Correction**, one of:
1. name the reference engine, its versioning rule and who may run it, in whichever
   document takes ownership; or
2. downgrade all five to *"the evidence is preserved and is sufficient for a human
   or a future adjudicator to diagnose the divergence; no adjudication procedure is
   specified"*; or
3. record it as an open question beside OQ-D.

Until then the sentence is an intention, not a property, and must not be quoted as
one of X29's three bounds.

---

## N4 — `DISPUTE` is chained, may be emitted at any time, and has no defined `sequence`; two honest disputes are an equivocation proof

A-4 scoped the equivocation predicate to `chain_scope == 1` and moved every
unchained message to sentinel envelope values. `DISPUTE` is on neither side of that
line cleanly.

* `PROTOCOL.md` §2.3's exhaustive unchained list — `HELLO`, `CAPABILITIES`,
  `JOIN_*`, `PLAYER_LIST`, the six lobby types — **excludes** `DISPUTE`.
  "Everything else is chained."
* `PROTOCOL.md` §4.11 gives `0x0703 DISPUTE` `chain_scope = 1`, stage kind
  **`out-of-stage`**, emitter "any participant".
* `PROTOCOL.md` §4.9: "may be emitted by any participant **at any time** on the
  table mesh, and is the only message that is legal outside its stage." No
  `sequence` rule, no `previous_event_hash` rule, no `event_class` assignment.

So a peer that emits two `DISPUTE`s during one hand — which §6.3 step 2 *requires*
("**Every peer** broadcasts `DISPUTE { kind = 1 STATE_DIVERGENCE … }`") and §6.3
step 4(a) requires again for the equivocation case — produces two bodies at one
`(protocol_version, table_id, hand_id, sequence, event_class = 0, sender)` unless
some rule assigns them different sequences, and no such rule exists. That is
§5.2's predicate satisfied exactly, against an honest peer, by mandatory behaviour.
It is the same defect class A-4 closed for lobby and join traffic, left open for the
one message type whose whole purpose is to carry evidence.

**Correction.** Either give `DISPUTE` `chain_scope = 0` with the §2.3 sentinels and
its own anti-replay (it names its subject through `at_sequence` already), or give it
a distinct `event_class = 3` **and** a normative `sequence` rule. The §2.3
exhaustive list and the §4.11 table must then agree.

---

## N5 — `STATE_MACHINE.md` consumes `EquivocationProof` only from `Diverged`; `PROTOCOL.md` makes `cause = 5` unconditional

C-6's whole point was that the engine must be able to represent every terminal state
the protocol can reach. The fix added exactly one transition for equivocation:

> `STATE_MACHINE.md` T55: "| T55 | `Diverged` | `EquivocationProof` | the proof
> verifies at the protocol layer | `HandAborted` | …"

But `PROTOCOL.md` §5.2 states the consequence with no divergence precondition:

> "**Consequence.** The hand aborts with `cause = 5`, the accused is attributed,
> the peer is added to `libp2p::allow_block_list`…"

and §5.2 makes the point that verification is *self-contained* — "No table state, no
transcript, no knowledge of the game" — so a proof can be handed to a peer that has
observed no divergence at all (Q-05 contemplates gossiping it lobby-wide). By
`STATE_MACHINE.md`'s own rule — "A transition not listed does not exist; any event
arriving in a state with no matching row is a `Rejection`" — and invariant I13, that
proof is silently discarded in every phase except `Diverged`. This is the identical
defect the review raised against `Event::RevealRejected`, reintroduced one finding
later. Counted under **C-6 PARTIAL** below.

**Correction.** Widen T55 to `any phase except TableClosed | EquivocationProof →
HandAborted`, the way T48 was widened for `RevealRejected`.

---

## N6 — `PROTOCOL.md` §11.3's first bullet still carries X8's unqualified forfeiture claim, which the same section then contradicts

> §11.3, bullet 1: "**Going silent to force a hand abort.** … It cannot steal cards
> or chips — **quitting costs exactly what folding would have cost, by D-005's
> forfeiture rule** …"

> §11.3, bullet 3, twelve lines later: "**Ending a heads-up hand by going silent
> (§8.3).** At `n = 2` there is no enforceable deadline of either kind, so a
> `kind = 2` abort carries no attribution and restores stacks, and **a heads-up
> player can escape a losing pot by going quiet.**"

`THREAT_MODEL.md` X8 was correctly qualified ("The escape is closed on the abort
path … at `n >= 3`. It is **not** closed on the divergence path … and it is not
closed at `n = 2` at all"); `CRYPTOGRAPHY.md` §2.10 was correctly qualified. This
one sentence in `PROTOCOL.md` was not, and it sits in the section whose stated
purpose is that §18 "forbids pretending otherwise". Bullet 1 must gain "at
`n >= 3`, and not on the `cause = 4` divergence path" or point at bullets 2 and 3.

---

## N7 — "No checkpoint is placed after any hole card has been opened" is literally false as written

`PROTOCOL.md` §6.2 states it as a normative rule, and `THREAT_MODEL.md` X29 repeats
it as one of the three bounds on the attack:

> §6.2: "**No checkpoint is placed after any hole card has been opened.**"
> X29: "checkpoint 7 sits **before `SHOWDOWN_REVEAL`**, and no checkpoint is placed
> after any hole card has opened"

Every player opens its own two hole cards at `DEAL_PRIVATE`, which precedes
checkpoints 3 through 7. The intended claim is about *public* opening, and the
surrounding prose gets it right ("it forces the attacker to commit to burning the
table while it still knows only its own hand"). The rule as stated is falsifiable on
its face, and it is doing load-bearing work in a DNA justification.

**Correction.** "No checkpoint is placed after any hole card has been opened **to
anyone but its owner** — that is, after `SHOWDOWN_REVEAL`."

---

## N8 — OQ-A (proposed `D-008`) and OQ-D are not in `DECISIONS.md`'s open list

The fix plan's own register requires it: OQ-A → "`DECISIONS.md` open list (proposed
`D-008`)"; OQ-D → "`docs/DECISIONS.md`"; OQ-B → "`DECISIONS.md` open list".
`PROTOCOL.md` §12, `THREAT_MODEL.md` §9.2 and `STATE_MACHINE.md` §11 all state that
OQ-A is recorded there. `DECISIONS.md`'s open-decisions table carries four rows:
licence, relay admission, "A dispute path that does not require the accused peer's
signature" (= OQ-B), and the `dns`/`kad` question. OQ-A and OQ-D are absent.

This matters more than a bookkeeping slip: OQ-A is the one open question that
**reverses D-005 at `n = 2`**, and three documents currently point at a list that
does not contain it. `DECISIONS.md` is outside the five documents the editors were
assigned, which is presumably why; it still needs doing before Phase 2.

---

## N9 — Two smaller internal mismatches

**(a) `HAND_ABORT`'s emitter set.** `PROTOCOL.md` §4.10 says *"the required emitter
set is the same set that emitted `HAND_INIT`, **minus any seat whose failure is the
reason for the abort**"*; the §4.11 summary table gives `0x0802 HAND_ABORT` emitter
**"all present seats"**. Since a collective stage completes only when every required
emitter is heard, and the attributed seat is by construction the one that is silent,
§4.10 is right and §4.11 is stale. As written, an abort against a vanished seat can
never complete its own stage.

**(b) `STATE_MACHINE.md` T4 uses a deadline kind no other document defines.**
`| T4 | Seating | TimeoutCertificate{Join} | …`. §8.4 knows only `kind == Action`
and `kind == Crypto`, and `PROTOCOL.md` §4.8 defines `kind` as `1` = action
deadline, `2` = cryptographic-step deadline. A join-deadline certificate is a third
kind, unspecified, and — since the table is not yet formed — with no defined voter
set. `PROTOCOL.md` §4.3's new formation-abandonment rule ("the advertisement …
disappears from every lobby by `AD_TTL_MS` with no cooperation from anybody") does
not need a certificate at all, so T4's trigger should be a local timer expiry at the
lobby layer, not a `TimeoutCertificate`.

---

# Part 2 — Per-finding verdicts

## A-1 — heads-up timeout certificate is one signature — **RESOLVED**

D-007 applied verbatim in all five documents, plus §0.3's extension to `kind = 2`.

> `PROTOCOL.md` §8.3: "**The certificate is defined for `n >= 3`.** At `n = 2` the
> required voter set has exactly one member, which is the opponent, so unanimity is
> vacuous" … "**Normative — no `kind = 1` certificate at two seats.** `kind = 1`
> certificates are invalid at `n = 2` and must be rejected by every receiver."
> … "**Normative — a `kind = 2` certificate at two seats attributes nobody.**"

> `PROTOCOL.md` §6.3 case (b): "the hand aborts with `cause = 1` and
> `attributed = []`. … not adjudicable inside the protocol, because any
> adjudication that requires the accused peer's signature is circular at every
> table size (D-007 point 4)." — the "via the §8 certificate machinery" circularity
> is gone.

> `THREAT_MODEL.md` X10 retitled "A forged timeout certificate by **all other
> dealt-in seats**"; "a malicious majority" deleted; `|V| = n - 1` table printed;
> "**At `n = 2` the mechanism gives no protection at all and is therefore not
> used.**" §9.1.2 limitations 3 and 4 rewritten; A13 gains "**The assumption does
> not hold at `n = 2`.**"

> `STATE_MACHINE.md` §8.4 validity rule 6 added; §8.5 records that
> `consecutive_auto_actions` never increments from a timeout at two seats; §9.5:
> "**The first shipped mode is the one where the deadline machinery does not
> apply.**"

> `CRYPTOGRAPHY.md` §2.8 item 2 and §2.10 qualified; `NETWORK_STACK.md` §8.4 and
> §15's D-007 row added.

Caveat: the fix is correct for `n = 2` and **incomplete for `n >= 3`** — see **N3**,
which is the same attack reachable through §8.4's exclusion rule. That is a defect
the finding did not name, so A-1 stands as RESOLVED and N3 is carried separately.

## A-2 — the race-safety mechanism does not exist — **RESOLVED**

Deleted, not repaired, in all three documents.

> `PROTOCOL.md` §5.2: "**What it does not catch, and never did.** A `TIMEOUT_VOTE`
> and a real event for the same stage are **not** an equivocation. … The earlier
> claim that this pair made the D-006 timeout race safe was false under this
> document's own predicate and is deleted."

> `PROTOCOL.md` §8.3: "**There is no artefact that settles the race when a voter
> lies about what it saw.**"

> `STATE_MACHINE.md` §8.4: "The previous claim here — *'There is never a state in
> which both a valid action and a valid certificate exist for the same parent'* —
> is **false** and is withdrawn."

> `THREAT_MODEL.md`: X11 deleted and folded into X10; "**X11 is retired.** … The
> number is not reused". OQ-C recorded in `PROTOCOL.md` §12 and pointed at from
> §9.2. §5.4 recounted (CP 18 → 17) and §9.3 restated with the reason: "The count
> fell from 18 to 17 in the Phase 0 review".

Counts verified by hand: extended rows X1–X10 + X12–X29 = 28; CP 6 / D&A 6 / DNA 3 /
OOS 12 / split 1 = 28; combined 19 + 28 = 47 and 17 + 13 + 3 + 1 + 13 = 47. Correct.

## A-3 — `cause = 4` restores stacks on demand — **RESOLVED**

> `PROTOCOL.md` §6.4: "**Restoration here is not safe; it is merely the only
> disposition every peer can agree on when nobody can be named.** A peer that
> publishes a false `state_hash` at a checkpoint can fault any table at any time and
> recover its own commitment. That is an in-protocol exploit, it is **not closed**".
> The old "because reaching it requires a genuine engine disagreement among at least
> two peers and is not available on demand to a single player" sentence is gone.

> `PROTOCOL.md` §6.2: checkpoint 7 moved to "immediately before `SHOWDOWN_REVEAL`".

> `PROTOCOL.md` §4.10 `HAND_COMPLETE`: "**A mismatch is not accepted; the
> mismatching copy is rejected under §4.0 … It does not open a §6 divergence.**"

> `THREAT_MODEL.md` X8: "and economically closed" and "so the escape has no value"
> both deleted; ends "It is **not** closed on the divergence path (`cause = 4`,
> restoration — X29), and it is not closed at `n = 2` at all". §9.1.2 limitation 5
> added; OQ-D in §9.2; `STATE_MACHINE.md` §8.6 gives `StateDivergence` restoration
> and §10 I2 covers the restoration paths.

Residual: the checkpoint-placement rule is overstated — **N7**.

## A-4 — equivocation predicate collides with lobby and join traffic — **RESOLVED**

The `chain_scope` / `event_class` envelope fields exist at `n(10)` / `n(11)`
(`PROTOCOL.md` §2.3, with the `EventBody` Rust shape updated), the sentinel rule is
normative, the predicate is scoped, and `THREAT_MODEL.md` G7 quotes it verbatim
rather than paraphrasing.

> `PROTOCOL.md` §5.2: "both carry `chain_scope == 1`, their bodies agree on all six
> of `(protocol_version, table_id, hand_id, sequence, event_class,
> sender_public_key == K)`" … "Events with `chain_scope == 0` are outside this
> predicate entirely and can never produce an `EquivocationProof`."

> `PROTOCOL.md` §4.3: `JOIN_REQUEST` envelope zeroed, and the new payload field
> "`n(8) table_id` | `bytes[32]` | the table being joined; must equal the
> `table_public_key` of the advert named by `advert_hash`". The
> "`sequence` = per-connection counter" line is gone.

> `NETWORK_STACK.md` §7.4 rule 4 rewritten: "This is **not** an
> `EquivocationProof` in the `PROTOCOL.md` §5.2 sense — lobby messages are
> unchained … and it **must not be called one**"; §6.4 step 4 checks the sentinels;
> §11.5 restricts `block_peer` to chained proofs.

> `THREAT_MODEL.md` G7 "Second limit — the false-positive class" records the
> specification-level source explicitly, as the finding required, and §5.5's
> `CheaterEquivocation` row makes the false-positive half a mandatory assertion.

Residual: `DISPUTE` is outside the scoping — **N4**.

## A-5 — `TIMEOUT_VOTE` self-incrimination at collective stages — **RESOLVED**

> `PROTOCOL.md` §4.8: "`chain_scope = 1` and **`event_class = 1`**. The class puts
> the vote in its own slot, so a voter that has already emitted its own event at a
> collective stage `s` can vote about stage `s` without equivocating against itself.
> … **There is no mutual exclusion between a vote and an action, and none is
> claimed.**" `TIMEOUT_CERT` gets `event_class = 2`.

> `PROTOCOL.md` §3.2 / §5.1 / §5.3 all keyed on `event_class`, including the
> anti-replay bound `MAX_STAGES_PER_HAND × MAX_SEATS × 3`.

> `STATE_MACHINE.md` §8.4: "**A seat's own contribution and its vote are not
> mutually exclusive.** … Any reading of T27, T41, T44 or this section that treats
> the pair as an equivocation is wrong."

## A-6 — unanimity-minus-one eviction missing from the threat model — **RESOLVED**

The rule was removed rather than documented, as ruled, and the residual attack is
catalogued.

> `PROTOCOL.md` §6.3 case (c): "**This is the same disposition at every `n`.** An
> earlier draft … resolved case (c) by *unanimity minus one* … **That rule is
> deleted.** … it cost an attack in which `n-1` colluders falsely claim a divergent
> state hash, evict one honest player and take their committed chips under D-005's
> forfeiture, available at `n = 3` with two colluders."

> `THREAT_MODEL.md` **X29**, class **DNA**, present with the deletion history in the
> row; §9.1.2 limitation 5; §5.4 recounted. `STATE_MACHINE.md`: "**There is no
> peer-removal transition, and no unanimity-minus-one guard anywhere in this
> document.**"

Residual: X29's third bound is **N2**.

## A-7 — X7 classified D&A while attribution is unresolved — **RESOLVED**

> `THREAT_MODEL.md` X7 class cell is now `D&A / DNA` with the three-case table
> inline: "*one silent seat, `n >= 3`* — **D&A**; *two or more seats silent
> simultaneously* — **DNA**; *any silent seat at `n = 2`* — **DNA**." "so
> attribution is sound" is deleted.

> `PROTOCOL.md` §8.4: "and **every** non-voting seat is attributed" is gone,
> replaced by "the hand aborts with `cause = 1`, `attributed = []`,
> `cert_hash = None`, and stacks restored. **Nobody is named** … **This is a safe
> default, not a solution.**"

> `STATE_MACHINE.md` §8.6 carries the explicit `if culprits is empty:` branch and
> §10 carries **I27**; `THREAT_MODEL.md` §9.2 carries **OQ-E** marked **Blocking**.

## A-8 — relay byte budget — **REGRESSED**

Original error fixed; new false claim introduced in its place. See **N1**.

## B-1 — "the `rand` crate is deliberately absent" — **RESOLVED**

> `PROTOCOL.md` §4.4: "**`rand 0.8.8` *is* in the runtime tree**, via `ark-std
> 0.5.0` … `SmallRng` is absent because rand 0.8's `small_rng` feature is not
> enabled; `StdRng` is present and is used by `ziffle` for deterministic [public
> constants]".

> `THREAT_MODEL.md` A7: "That is false. `rand 0.8.8` **is** in the runtime tree via
> `ark-std 0.5.0`" … *If false* clause names the lint.

Re-verified in source: `ark-std-0.5.0/Cargo.toml:68-71` (`rand` 0.8,
`features = ["std_rng"]`, `default-features = false`); `rand-0.8.8/Cargo.toml`
(`default = ["std", "std_rng"]`, `small_rng = []` unenabled);
`ziffle-0.1.0/src/lib.rs:127` imports `rngs::StdRng`. All three claims correct.

## B-2 — `relay::RateLimiter` is publicly re-exported — **PARTIAL**

The false claim is gone from both named places and the named-type form is adopted.
Verified: `libp2p-relay-0.21.1/src/lib.rs:42` re-exports
`behaviour::rate_limiter::RateLimiter`; `src/behaviour/rate_limiter.rs:38` declares
`pub trait RateLimiter: Send`, `:56` the blanket impl. Every `relay::Config` field
name in §9.6's block exists (`src/behaviour.rs:55-66`).

> `NETWORK_STACK.md` §9.6: "the trait itself is **re-exported at the crate root** …
> so `libp2p::relay::RateLimiter` is a public, nameable, implementable trait" …
> "**a named type is used instead** … The earlier justification for the closure —
> that the trait could not be named from outside — was simply false".

**Why PARTIAL.** `NETWORK_STACK.md` §15's decision register still describes D-002
as "publicly reachable client volunteers as relay, off by default, **admission via
the rate-limiter closures**". That is the abandoned mechanism, in the same document,
in the table a reader consults to find where a decision is honoured. One word.

## B-3 — the stale RustSec `rand` row — **PARTIAL**

The authoritative side is done:

> `CRYPTOGRAPHY.md` §9 carries the canonical row verbatim — "**in the runtime tree
> at 0.8.8 via `ark-std 0.5.0`. `informational = "unsound"`, patched at `>= 0.8.6`
> …**" — plus `rand`, `rand_chacha` and `rand_core` rows in the six-column register.

**Why PARTIAL.** `research/CRYPTO_LIBS.md` was not touched, and the fix plan named
it explicitly:

> `CRYPTO_LIBS.md:924`: "| `rand` | RUSTSEC-2026-0097 | **Crate not in the tree at
> all** |"
> `CRYPTO_LIBS.md:44`: "| `rand` crate | **Dropped entirely.** …"
> `CRYPTO_LIBS.md:1040`, inside the copy-verbatim `Cargo.toml` block: "The `rand`
> crate is deliberately ABSENT"

§10's version list also still lacks `rand 0.8.8` / `rand_chacha 0.3.1` /
`rand_core 0.6.4`. This is precisely the row the finding said "survives into a
future audit unnoticed", and the block at line 1040 is labelled "Copy this verbatim
into the workspace `Cargo.toml`". Understandable — the editors were assigned the
five specification documents — but not done.

## B-4 — `DOMAIN_EVENT` literal bytes — **RESOLVED**

> `PROTOCOL.md` §2.8 first row: "`p2p-poker/v1/event` | the 24-byte `DOMAIN_EVENT`
> signature prefix (§2.4). **This is a literal prefix, not a `derive_key` domain**,
> which is why its separators differ from every other row in this table."

The 24 hex bytes appear identically in §2.4 and §13
(`70 32 70 2d 70 6f 6b 65 72 2f 76 31 2f 65 76 65 6e 74 00 00 00 00 00 00` — checked
byte for byte, and `70 32 70 2d …` decodes to `p2p-poker/v1/event`). `p2p-poker v1
event` (spaces) is in §2.8's new "Retired, never valid" table. `CRYPTOGRAPHY.md`
§6.4 points at `PROTOCOL.md` §13 and does not restate the bytes.

## C-1 — every shared size constant differed — **RESOLVED**

See the constant sweep (Part 3.1). One home (`PROTOCOL.md` §13), one name per value,
`NETWORK_STACK.md` §14 reduced to a pointer plus genuinely local tuning, §11.3
labelled "reproduced, not defined … Where the two ever differ, `PROTOCOL.md` §13 is
right". `GOSSIP_MAX_TRANSMIT = 65 536` verified against
`libp2p-gossipsub-0.49.5/src/config.rs:244-246` (`default_max_transmit_size() ->
65536`). The 2 560 / 1 024 conflict is resolved by the new `TABLE_AD_SIGNED_MAX =
1 536`, and `128 × 1 536 = 196 608 < 262 144` checks out.

## C-2 — two definitions of `ctx` — **RESOLVED**

`PROTOCOL.md` §4.5 owns it; `CRYPTOGRAPHY.md` §6.4 reproduces it byte for byte
("`PROTOCOL.md` §4.5 owns it; if the two ever differ, §4.5 wins") including the
`shuffle_round` field and the `0xFF` sentinel; `THREAT_MODEL.md` A11 and OQ3 name
the same seven fields; `session_nonce` is retired everywhere in favour of
`session_id`. The extension ruling landed too: `RNG_COMMIT`'s five-part binding is
identical in `PROTOCOL.md` §4.4, `CRYPTOGRAPHY.md` §7.3 and `STATE_MACHINE.md` §7.9,
and §2.8's retired-strings table names `p2p-poker v1 rng-seed` and
`p2p-poker/seat-beacon/v1`.

`PROTOCOL.md` §14 Objection 1 — that the canonical combine
`h("p2p-poker v1 rng-beacon", [r_1 … r_n])` drops the `session_id` this document
previously bound directly — is **well founded and correctly disposed of**: the
objection identifies a real weakening, states why the binding survives indirectly
(each `r_s` is bound by its own stage-1 commitment; the beacon events chain to
`GENESIS(0)`), and asks for it to be an explicit choice. Nothing was silently
deviated from.

## C-3 — burn cards — **RESOLVED**

`CRYPTOGRAPHY.md` §2.4's burn layout is replaced by the no-burn table "reproduced
from `PROTOCOL.md` §4.5, which owns it"; §2.8 item 3 now reads "**There are no burn
cards.** Indices `2m+5 … 51` are never opened and a reveal token for any of them is a
protocol violation". The three index tables (`PROTOCOL.md` §4.5,
`CRYPTOGRAPHY.md` §2.4, `STATE_MACHINE.md` §7.8) are identical, and `k` is renamed
to `m` corpus-wide. Recorded as entry 3 of the §9.1.1 deviation register.

## C-4 — `DEAL_PRIVATE` topology — **RESOLVED**

> `CRYPTOGRAPHY.md` §2.7: "the earlier form of this argument in this document —
> *'Bob receives no token for Alice's indices from anybody'* — was **false** under
> the shipped broadcast design and has been deleted; the counting argument above is
> the correct one and is the basis of `THREAT_MODEL.md` G1."

§2.9's budget line is rewritten for broadcast ("`2(n−1)` tokens per sender, one
message per sender, one collective stage"), and the consequence the finding said was
missing is now carried. The entitlement rule survives in its broadcast shape ("a
token from `P` for `P`'s own hole index before `SHOWDOWN_REVEAL` is the specific
violation to reject and attribute to `P`").

## C-5 — `HAND_INIT` / `HAND_COMPLETE` — **RESOLVED**

Both documents adopt the collective signed derived stage.

> `STATE_MACHINE.md` §3.4: "**They carry no signature, because there is no signer**"
> is gone. "the protocol layer wraps each one in that peer's own signed envelope and
> emits it as a **collective** stage … There is no writer and no authority; there is
> also no unsigned event." Q2 is closed and recorded in §11's closed table.

> `PROTOCOL.md` §4.4 / §4.10 / §4.11 all say collective; §3.2 carries the stage-kind
> principle normatively; §12 records Q2's answer.

## C-6 — no dispute path in `STATE_MACHINE.md` — **PARTIAL**

Nearly everything landed: `Phase::Diverged` as phase 20; `Event::StateHash`,
`StateAck`, `Dispute`, `EquivocationProof`; `AbortKind::StateDivergence` and
`Equivocation`; T48 (`RevealRejected` → `HandAborted` with
`Fault{InvalidRevealProof}`, closing the dangling event); T49–T55 for §6.3 steps 1–4;
recounted headline figures "20 phases, 55 numbered transitions, 28 invariants" with
the delta explained. `STATE_MACHINE.md` §13's two recorded completions (a carrier
event for T55, and T49/T51 so `StateAck` and an agreeing `StateHash` are not
`Rejection`s) are both **well founded** — without them the fix would have shipped two
declared events that no row consumes, which is the exact defect C-6 raises.

**Why PARTIAL.** T55 fires only from `Diverged`, while `PROTOCOL.md` §5.2 makes
`cause = 5` unconditional and stresses that the proof is verifiable with "No table
state, no transcript, no knowledge of the game". See **N5**.

## C-7 — the join flow — **RESOLVED**

`PROTOCOL.md` §1.1 lists six protocol strings including `/p2p-poker/join/1`; §1.4
has a five-row channel table plus an explicit message→channel table "so that adding a
channel or a message type cannot leave it stale"; §4.3's heading is "(channel: join
RPC, except `PLAYER_LIST` and `TABLE_READY`)"; §4.11's Channel column updated for
`0x0201`–`0x0203`; §13 carries `JOIN_PROTOCOL`, `JOIN_REQ_MAX`, `JOIN_RESP_MAX`.
`PROTOCOL.md` §14 Objection 2 (C-7 said "five" strings, C-8 makes it six) is **well
founded**: the two rulings are individually right and their arithmetic does not
compose. §1.1 says six and lists six.

## C-8 — lobby chat — **RESOLVED**

`PROTOCOL.md` §7.7 defines `0x0106 LOBBY_CHAT` with the three fields, the caps, the
§9.4 string rules restated at the message, and "Chat carries **no security claim of
any kind**." §7.6 carries the 1-per-2 s / burst 5 limit; §13 carries
`LOBBY_CHAT_TOPIC` and `LOBBY_CHAT_MAX`; §1.4 and §4.11 carry the channel;
`THREAT_MODEL.md` X15's scope now names chat. The §4.11 `0xF000`–`0xFFFF` gap is
closed by chat having a real code.

## C-9 — invariant I1 false for cash mode — **RESOLVED**

The ledger identity is identical in `STATE_MACHINE.md` §10 (with the tournament
corollary and the cash-mode note), `THREAT_MODEL.md` G9, and `PROTOCOL.md` §4.10's
`HAND_COMPLETE` check. `ledger_in` / `ledger_out` are canonical state
(`STATE_MACHINE.md` §2.6 and §2.8's justification table) and are inside
`state_hash` (`PROTOCOL.md` §6.1 `PublicTableState`). `HAND_INIT` gains
`n(11) ledger_delta` with the ≤ `MAX_SEATS` bound in §9.4. **I28** added.

## C-10 — `CERT_SETTLE_MS` — **RESOLVED**

> `PROTOCOL.md` §4.8: "**The certificate stage is collective.** … There is no timer,
> no tie-break and no assembler privilege." §4.11 row changed to `collective` /
> `required voters`; `CERT_SETTLE_MS` deleted from §13 and listed under "Retired
> constants"; **Q-04 closed** in §12 with the answer recorded.

> `PROTOCOL.md` §8.2: "Since the certificate stage became collective (§4.8) this is
> true without exception: `CERT_SETTLE_MS` was the one wall-clock read left inside
> the chain-building rule and it is gone." The residual (a stage can close two ways)
> is stated in §8.2, `STATE_MACHINE.md` §8.4 and `THREAT_MODEL.md` A13 rather than
> hidden.

## C-11 — the per-table beacon deviation — **RESOLVED**

One register, `THREAT_MODEL.md` §9.1.1, with five rows. "Recorded spec deviation"
headings in `PROTOCOL.md` §4.4, `CRYPTOGRAPHY.md` §7.3 and `STATE_MACHINE.md` §7.9,
each pointing at the register instead of restating.

## D-1 — `InMemoryTransport` — **RESOLVED**

`NETWORK_STACK.md` §1.3.1 specifies the upward `Transport` trait (`send`, `events`,
`connection_state`, the four-variant `TransportEvent`, the `ConnectionState` enum)
with "**Nothing libp2p-typed may appear above this trait**"; §1.3.2 specifies the
seeded deterministic scheduler (one `u64` seed, a logical clock, no `Instant::now`),
all eight injection modes, and how a conflicting message is injected — including the
sharp point that "the harness therefore needs the sender's application signing key,
because both copies must carry valid signatures; a test that injects an invalidly
signed second copy tests the signature check, not the equivocation predicate".
§12 records that it blocks Phases 3–6.

## D-2 — the §28 dependency register — **PARTIAL**

Both interim sides landed with all six columns and explicit incompleteness notices
(`CRYPTOGRAPHY.md` §9: "**This table is not complete and does not claim to be**";
`NETWORK_STACK.md` §5.1.1: "**Neither is complete on its own**"), and
`libp2p-stream 0.4.0-alpha` and `ziffle 0.1.0` are flagged in both. **`docs/DEPENDENCIES.md`
does not exist**, which the ruling designated as the owner. Interim as ruled;
the permanent home is still open.

## D-3 — the eleven §25 cheaters mapped to tests — **RESOLVED**

`THREAT_MODEL.md` §5.5 is a four-column table covering all eleven plus §25's two
mandatory standalone tests, with the honest caveats the ruling demanded:
"The test modules are **planned locations, not existing code**"; and
`CheaterPredictableRNG`'s row records "**this is a statistical test, not a proof**,
and it is the only row in this table whose assertion is not a hard accept/reject".
`CRYPTOGRAPHY.md` §12 item 8 and `STATE_MACHINE.md` §10 are made subordinate to it.

## D-4 — source architecture — **RESOLVED (as the interim ruled)**

All five documents carry the §1 module line and name `docs/ARCHITECTURE.md` as the
Phase 2 destination, including `THREAT_MODEL.md`'s "no module. It governs
`tests/adversarial/` and `tests/fuzz/`".

## D-5 — §33 profiling coverage — **RESOLVED**

`CRYPTOGRAPHY.md` §6.5 carries the seven-target table with the three missing metrics
marked and phased. Every quotation of the estimate is now labelled: `CRYPTOGRAPHY.md`
§6.5 ("**These two figures are estimates at an assumed 100 ms RTT**"),
`PROTOCOL.md` §3.2 ("an estimate, not a measurement") and §4.4 ("**Hand start-up
latency is an estimate, not a measurement.**"). §4.0 step 9: "Step 9 is one Ed25519
verification, and **no figure for it has been measured**". The C-5 revision
(collective `HAND_INIT` adds a round trip) is noted in both places.

## D-6 — §31 Git discipline — **PARTIAL**

Both one-line pointers landed (`CRYPTOGRAPHY.md` §12, `PROTOCOL.md` §9.6: "Every
change to anything enumerated in this section is a security-critical change under
`SPEC_CS.md` §31 and requires a reproducing regression test to land first. See
`docs/CONTRIBUTING.md`."). **`docs/CONTRIBUTING.md` does not exist**, so the
load-bearing clause has a pointer and no target. The ruling scheduled it for "the
close of Phase 1", so this is on schedule rather than skipped — but two documents now
reference a file that is not there.

## D-7 — founder departure and the ten-seat preset — **RESOLVED**

`PROTOCOL.md` §4.3's "**If formation never completes**" paragraph covers the
`JOIN_ACCEPT` holders, the ledger ("no chips have moved, because a buy-in enters the
ledger only at the first `HAND_INIT`"), the un-revocable advert and the TTL, and
connects to `STATE_MACHINE.md` T4. The preset statement appears identically in
`PROTOCOL.md` §13, `STATE_MACHINE.md` §9.5 and `NETWORK_STACK.md` §12.12, and the
latter names the acceptance configuration ("**The acceptance test runs on a `CUSTOM`
two-seat table**") and draws the two consequences the ruling did not ask for but
should have.

---

# Part 3 — Independent sweeps

## 3.1 Constant sweep (C-1, C-2)

**Numeric constants.** Every two-sided value now has exactly one definition
(`PROTOCOL.md` §13) and one name. `NETWORK_STACK.md` §11.3 reproduces and says so;
§14 is a pointer plus local tuning. Checked pairwise:

| Constant | `PROTOCOL.md` §13 | `NETWORK_STACK.md` | Agree |
|---|---:|---:|:--:|
| `GOSSIP_MAX_TRANSMIT` | 65 536 | 65 536 (§6.2, §11.3) | ✓ |
| `LOBBY_MSG_MAX` | 8 192 | 8 192 (§6.5, §11.3) | ✓ |
| `TABLE_AD_MAX` | 1 024 | 1 024 (§6.5, §7.3, §11.3) | ✓ |
| `TABLE_AD_SIGNED_MAX` | 1 536 | 1 536 (§6.5, §7.3, §8.4, §11.3) | ✓ |
| `LOBBY_CHAT_MAX` | 2 048 | 2 048 (§6.5, §11.3) | ✓ |
| `SNAPSHOT_REQ_MAX` | 1 024 | 1 024 (§7.1 code, §11.3) | ✓ |
| `SNAPSHOT_RESP_MAX` | 262 144 | `256 * 1024` (§7.1 code), 262 144 (§7.3, §11.3) | ✓ |
| `SNAPSHOT_MAX_ADS` | 128 | 128 (§7.3, §11.3) | ✓ |
| `JOIN_REQ_MAX` | 4 096 | 4 096 (§8.4, §11.3) | ✓ |
| `JOIN_RESP_MAX` | 16 384 | 16 384 (§8.4, §11.3) | ✓ |
| `TABLE_FRAME_MAX` | 262 144 | 262 144 (§8.4, §11.3) | ✓ |
| `MAX_EMBEDDED_EVENT` | 32 768 | 32 768 (§11.3) | ✓ |
| `MAX_SEATS` / `MAX_STAGES_PER_HAND` / `MAX_CBOR_NESTING_DEPTH` | 10 / 2 048 / 8 | not restated | ✓ |
| every TTL / interval / `SNAPSHOT_PEER_COUNT` | §13 | not restated | ✓ |
| `CERT_SETTLE_MS` | **deleted**, listed as retired | absent | ✓ |

Internal consistency also checks out where the review found it broken:
`JOIN_ACCEPT`'s `advert_event` cap is now `≤ TABLE_AD_SIGNED_MAX = 1 536 B`
(`PROTOCOL.md` §4.3), the 2 560 B figure is gone corpus-wide, and
`128 × 1 536 = 196 608 ≤ 262 144` is stated in three places. `TABLE_FRAME_MAX`'s
justification is now `DISPUTE` (140 000 B cap in §9.3) rather than the shuffle, and
140 000 > 131 072 confirms the choice. The retired names `LOBBY_MAX_MESSAGE`,
`TABLE_MAX_FRAME`, `SNAPSHOT_MAX_REQUEST`, `SNAPSHOT_MAX_RESPONSE` survive only in
§13's "Retired constants" note and in §7.1's explanation of the deletion.

The only numeric disagreement left in the corpus is the relay byte arithmetic —
**N1** — where `NETWORK_STACK.md` §9.5 and §16.1 give ~14 and ~7 hands respectively,
in the same document.

**Domain-separation strings.** Extracted every `p2p-poker …` string from all five
documents and diffed:

* `PROTOCOL.md` §2.8's register is the single register (15 rows), and every string
  used in `CRYPTOGRAPHY.md`, `STATE_MACHINE.md` and `NETWORK_STACK.md` is drawn from
  it. No document invents one.
* `DOMAIN_EVENT` is byte-identical in `PROTOCOL.md` §2.4 and §13, and quoted (never
  restated) in `CRYPTOGRAPHY.md` §6.4 and §14.
* The `ctx` block is byte-identical between `PROTOCOL.md` §4.5 and
  `CRYPTOGRAPHY.md` §6.4 — same field order, same comments, same `0xFF` sentinel,
  same "The separator is the length prefix; no other separator, delimiter or padding
  exists."
* The `h` constructor (`blake3::derive_key(domain, b"p2p-poker/v1")` + 8-byte BE
  length prefixes) is stated identically in `PROTOCOL.md` §2.8/§4.5,
  `CRYPTOGRAPHY.md` §6.4 and `STATE_MACHINE.md` §7.9.
* `commitment_i` and `seed` are byte-identical in `PROTOCOL.md` §4.4,
  `CRYPTOGRAPHY.md` §7.3 and `STATE_MACHINE.md` §7.9.
* Retired strings (`p2p-poker v1 rng-seed`, `p2p-poker/seat-beacon/v1`,
  `p2p-poker v1 event` with spaces) appear only inside the "Retired, never valid"
  table and inside the two sections that record their own retirement. No live use.
* Three `p2p-poker/…/v1` strings are **not** protocol domains and are correctly not
  in the register: `CRYPTOGRAPHY.md`'s `b"p2p-poker/identity/v1"` (Windows DPAPI
  application entropy), `THREAT_MODEL.md` §8.5's hypothetical epoch infohash
  `H("p2p-poker/lobby/v1" ‖ floor(day))`, and the two Mainline derivation strings,
  which are in §13.

**Verdict: constants and domain strings agree byte for byte, with the single
exception of N1.**

## 3.2 Claim sweep — "prevented", "impossible", "guaranteed", "cannot"

Every occurrence in `THREAT_MODEL.md` and `CRYPTOGRAPHY.md` was read in context.

* No unjustified "prevented". `THREAT_MODEL.md` §5.1's CP definition is scoped
  ("Under assumptions A1–A7 (and the specific ones named in the row)"), and §9.3
  qualification 4 states it outright: *"'Prevented' always means under the stated
  assumptions, never impossible."*
* Every "impossible" is either quoting `SPEC_CS.md`, denying the claim
  (`THREAT_MODEL.md` l. 16, 849, 1343, 1351; `CRYPTOGRAPHY.md` l. 33), or is a true
  statement about the `n`-of-`n` liveness property ("any player who stops publishing
  tokens makes it impossible for anyone to open any further card"), which is
  correct and is the reason the abort path exists.
* Every downgrade the review demanded is present in the class column, not only in
  prose: X7 `D&A / DNA`, X10 `DNA (at n >= 3)`, X29 `DNA`, X11 retired, CP count
  moved 18 → 17 with the reason recorded.
* `SPEC_CS.md`'s closing instruction is quoted verbatim in `THREAT_MODEL.md` §9.3,
  `CRYPTOGRAPHY.md` §11, `PROTOCOL.md` §11 and `NETWORK_STACK.md` §12.

**Three claims still stronger than their mechanism**, all recorded above:
**N1** ("per direction"), **N2** ("deterministically adjudicable offline"),
**N6** (`PROTOCOL.md` §11.3's "quitting costs exactly what folding would have
cost"), plus the literal falsity of **N7**.

## 3.3 Heads-up sweep — D-007 compliance

Grepped every occurrence of "unanimous", "unanimity", "still-active" and every
`n = 2` / heads-up passage in all five documents.

**Compliant, with the heads-up case stated explicitly:** `PROTOCOL.md` §4.10
(three-case `attributed = []` table), §6.3(b), §8.2, §8.3, §8.4, §8.5, §11.3, §12
OQ-A; `THREAT_MODEL.md` A13, X7, X8, X10, §5.5 `CheaterDisconnect`, §7.3, §9.1.1
row 5, §9.1.2 limitations 3 and 4, OQ-A, OQ8; `STATE_MACHINE.md` §8.4 rules 4 and 6
and its `|V|` table, §8.5, §8.6, §9.5, §11 OQ-A; `CRYPTOGRAPHY.md` §2.8 item 2,
§2.10, §11; `NETWORK_STACK.md` §8.4, §12.12, §15's D-007 row.

**One place still assumes the certificate works with `|V| = 1`:** the §8.4
exclusion rule — **N3** — which is not a heads-up bug but the same bug at every
table size, because the rules that reject it are written on `n` rather than on
`|V|`. That is the one substantive heads-up-sweep finding.

Two cosmetic residues, harmless but worth a pass: `PROTOCOL.md` §2.6 ("the timeout
certificate of §8, which use relative durations and unanimity rather than absolute
time" — true at `n >= 3`, and it points at §8, so it does not mislead), and
`STATE_MACHINE.md` T4's `TimeoutCertificate{Join}` — **N9(b)**.

## 3.4 New-contradiction sweep — the five most load-bearing two-document claims

| # | Claim | Documents | Diff |
|---|---|---|---|
| 1 | the equivocation predicate | `PROTOCOL.md` §5.2 ↔ `THREAT_MODEL.md` G7 | **identical**, quoted as a block; `NETWORK_STACK.md` §7.4 and §11.5 defer to it and say what their own rule is *not* |
| 2 | the `ctx` construction | `PROTOCOL.md` §4.5 ↔ `CRYPTOGRAPHY.md` §6.4 | **identical**, with an explicit ownership sentence and a tie-break rule |
| 3 | the deck-index map | `PROTOCOL.md` §4.5 ↔ `CRYPTOGRAPHY.md` §2.4 ↔ `STATE_MACHINE.md` §7.8 | **identical**, same symbol `m`, same `2m+5 … 51` unused range |
| 4 | the timeout-certificate rules | `PROTOCOL.md` §8.3/§8.4 ↔ `STATE_MACHINE.md` §8.4 ↔ `THREAT_MODEL.md` X10/A13 | agree on `V`, on the `n = 2` rules, on the collective stage and on the race statement. **Both carry N3's `\|V\|` hole identically**, which is at least consistent |
| 5 | the relay byte budget | `CRYPTOGRAPHY.md` §6.5 ↔ `PROTOCOL.md` §9.3 ↔ `THREAT_MODEL.md` §3.5/X20/OQ12 ↔ `NETWORK_STACK.md` §9.5 **↔ `NETWORK_STACK.md` §16.1** | **contradiction — N1.** Four documents say ~14 hands per direction; one section of the fifth says ~7 hands bidirectional and is right |

Three further pairs checked and clean: the ledger identity (`STATE_MACHINE.md` §10 /
`THREAT_MODEL.md` G9 / `PROTOCOL.md` §4.10, identical); the collective hand-boundary
stages (`PROTOCOL.md` §3.2/§4.4/§4.10/§4.11 / `STATE_MACHINE.md` §3.4, identical);
the `rand` facts (`PROTOCOL.md` §4.4 / `THREAT_MODEL.md` A7 / `CRYPTOGRAPHY.md` §7.2,
identical and all three verified against the crates).

One intra-document mismatch: **N9(a)**, `PROTOCOL.md` §4.10 vs §4.11 on
`HAND_ABORT`'s emitter set.

## 3.5 Assessment of the editors' recorded objections

Five objections across four documents. **All five are well founded.** None was used
as cover for deviating from a ruling; in every case the ruling was applied as
written and the disagreement recorded, which is what the editing rule asked for.

| Objection | Verdict | Note |
|---|---|---|
| `NETWORK_STACK.md` §16.1 — `max_circuit_bytes` is not per direction | **Well founded, and materially important.** Confirmed in `libp2p-relay-0.21.1/src/copy_future.rs:41-48, 78, 88-104`: one `bytes_sent` counter, incremented by both `forward_data` calls. | The editor was right and the ruling is wrong. This is **N1**; the corrected numbers must now be applied to four documents, and OQ12 / OQ-7 restated as bidirectional. The editor's own conclusion — that the ruling's *decision* survives because duration binds either way — is also correct. |
| `THREAT_MODEL.md` Objection 1 — "publicly adjudicable offline" has no referent | **Well founded.** The claim needs a canonical, versioned, agreed reference engine, and no document owns one; A-3 and A-6 spent their reasoning establishing that no observer-independent derivation exists at run time, and moving it offline does not create one. | This is **N2**. Of the editor's three proposed dispositions, (2) — downgrade to "the evidence is preserved and is sufficient for a human or a future adjudicator to diagnose the divergence; no adjudication procedure is specified" — is the one `SPEC_CS.md` §36 points at, and it costs nothing: the sentence is doing no work that the preserved transcript does not already do. |
| `CRYPTOGRAPHY.md` §15 note 1 — A-1's "no other change" was too narrow | **Well founded.** §2.10's D-005 disposition and §11's "detected and attributed" bullet were both unconditional and both false at `n = 2` under the plan's own §0.3; leaving them would have had this document assert exactly what A-3's edit to X8 deletes. | The editor went beyond instructions in the only direction the plan permits — "No claim was strengthened; two were weakened." Correct call. The same defect survives *unfixed* in `PROTOCOL.md` §11.3 — **N6** — which is evidence that this objection should have been generalised across the corpus rather than applied in one file. |
| `CRYPTOGRAPHY.md` §15 note 2 — A-8's instruction and A-8's own table disagree | **Well founded.** The instruction said to keep `n × 8 979` relabelled as one peer's outbound; the normative table gives `(n−1) × 8 979`. Those differ by 8 979 B, and `n × 8 979` is the table-wide aggregate whose comparison against a per-circuit cap *was the original defect*. Relabelling it would have reintroduced A-8 under a new name. | Following the table was right. |
| `PROTOCOL.md` §14 notes 1–3 — C-2 drops a direct session binding; C-7's "five strings" vs six; A-1's `cert_hash` vs A-7's `cert_hash = None` | **All three well founded.** Note 1 identifies a real weakening (direct → indirect binding) and shows why it is still sound. Note 2 is stale arithmetic in the plan, not an error in the text. Note 3 resolves a genuine collision between two rulings by scoping A-1's sentence to the certificate path, which is the only reading under which A-7's interim behaviour is representable. | Note 3's resolution is visible in the text: §4.10's `cert_hash` row carries "with the single exception of the §8.4 `hand_deadline_ms` path". Sound. |
| `STATE_MACHINE.md` §13 — "None", plus two completions | **Well founded.** C-6 mandated a transition for `EquivocationProof` without declaring an event to carry it, and declared `Event::StateAck` with no consuming transition — the identical defect C-6 raises against `RevealRejected`. T49, T51 and T55 fix both, and the transition count includes them. | The editor's completion is right as far as it goes; **N5** is what it did not reach — T55's `Diverged`-only guard leaves `PROTOCOL.md`'s unconditional `cause = 5` unrepresentable. |

---

# Part 4 — What must happen before Phase 2

Ordered by what unblocks or endangers the most.

1. **N3 — rewrite `PROTOCOL.md` §8.3's two normative rules on `|V|` rather than on
   `n`, and `STATE_MACHINE.md` §8.4 rule 6 with them**, or delete the §8.4 exclusion
   rule. As written, one modified client can forfeit an honest player's committed
   chips at any table size. This is the A-1 attack, still open.
2. **N1 — apply `NETWORK_STACK.md` §16.1's arithmetic to the four documents that
   carry "per direction"**, and restate OQ12 / OQ-7 as bidirectional. A single
   document currently contradicts itself on a number three other documents quote.
3. **N2 — decide the reference-engine question** (name it, downgrade the sentence,
   or open it as a question). Five occurrences, two of them softening DNA rows.
4. **N4 — give `DISPUTE` a defined envelope.** Until then §6.3's mandatory
   declarations are, by §5.2's own predicate, equivocation proofs against every
   honest peer that files one.
5. **N5, N6, N7, N9 — four small edits**, each a one-line or one-cell correction.
6. **B-3 — update `research/CRYPTO_LIBS.md`** §7.3, §10 and the `Cargo.toml` comment
   block. The block is labelled "Copy this verbatim into the workspace
   `Cargo.toml`" and currently instructs a future implementer to do the impossible.
7. **B-2 — one word in `NETWORK_STACK.md` §15's D-002 row** ("closures" → the named
   type).
8. **N8 — add OQ-A (proposed `D-008`) and OQ-D to `DECISIONS.md`'s open list**,
   which three documents already claim carries them.
9. **D-2 / D-6 — `docs/DEPENDENCIES.md` and `docs/CONTRIBUTING.md`.** Both are
   scheduled rather than skipped, but two documents already point at
   `CONTRIBUTING.md` and it does not exist.

Nothing in this verification touches the core cryptographic construction, and
nothing found here reverses a ruling of the fix plan except A-8's "per direction",
which one editor had already caught. The A-class rewrites are, with the single
exception of N3, applied faithfully and in the direction `SPEC_CS.md` §36 requires:
weaker claims, open questions left open, and no invented constructions.
