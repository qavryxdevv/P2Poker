# PHASE2_GATE.md — the Phase 2 readiness gate

**Commission.** Four adversarial passes ran, each finding a worse defect than the
last, and **D-010** was adopted to dissolve the class rather than patch it a fifth
time. This pass answers one question: did that work.

**Method.** Every verdict is quoted from the document as it now stands, with line
numbers. No specification document was edited. Crate claims were not re-derived;
where a verdict depends on one it is marked as carried from
`PHASE1_VERIFY3.md`.

**Files as read.** `PROTOCOL.md` 14:25, `STATE_MACHINE.md` 14:32,
`CRYPTOGRAPHY.md` 14:21, `THREAT_MODEL.md` 14:24, **`NETWORK_STACK.md` 12:41**,
`DECISIONS.md` 14:07, `CONTRIBUTING.md` 12:55, `research/CRYPTO_LIBS.md` 12:42.

> The `NETWORK_STACK.md` timestamp is the first finding of this pass and it is
> not incidental. Every other specification document was rewritten for D-010
> between 14:07 and 14:32. `NETWORK_STACK.md` was not touched, and it contains
> **zero occurrences of the string `D-010`** against 59 in `PROTOCOL.md`, 71 in
> `STATE_MACHINE.md`, 67 in `THREAT_MODEL.md` and 33 in `CRYPTOGRAPHY.md`. The
> D-010 sweep found its survivors exactly there.

## Verdict counts

| Verdict | Count | Items |
|---|---:|---|
| **RESOLVED** | 9 | P1, P2, P4, P5, P6, P7, P8, N8, M3 |
| **PARTIAL** | 3 | P3, R-1, R-2 |
| **UNRESOLVED** | 2 | R-3, R-4 |
| **REGRESSED** | 0 | — |

**Five new defects, G1–G5.** Three are consensus-critical. **G1** is D-009
rule 1's **fifth** recurrence and it was created, exactly as `PHASE1_VERIFY3.md`
predicted in its closing sentence, by the path the P3 fix newly made load-bearing:
the witness-independent terminal `HAND_ABORT` is written at the *stalled stage's
own `sequence`*, where every honest peer that contributed to that stage has
already signed a different body into the same capacity-one slot. **G2** reopens
P2 — `STATE_MACHINE.md` T55 still consumes an `EquivocationProof` and ends a hand
with it, which `PROTOCOL.md` §5.2 now forbids in a box that names
`STATE_MACHINE.md` as the document that reproduces it. **G3** is the D-010 sweep's
survivor: `NETWORK_STACK.md` block-lists a peer on a proof, in two places.

**Readiness verdict: NOT-READY.** Reasons in Part 7.

---

# Part 1 — Per-item verdicts

## P1 — the reconciliation `STATE_HASH`'s envelope — **RESOLVED**

The fix taken is `PHASE1_VERIFY3.md`'s own first-ranked correction: a new stage,
not a second copy and not a key extension.

> `PROTOCOL.md` §4.9 (l. 1988–2007), normatively: "**A reconciliation round is its
> own stage, never a second copy of the checkpoint's.** … A reconciliation
> `STATE_HASH` is a **new collective stage** with a **new `sequence`**, chained
> from the disputed checkpoint's `stage_hash`. … Successive rounds take successive
> `sequence` values, so a peer that reconciles twice occupies two slots and never
> two bodies in one."

The second half of P1 — the artefact that did not exist — is closed with it.
§6.3 step 3 (l. 2844–2856) is retitled "**reconcile transcripts, then re-derive in
a new stage**" and the sentence P1 named is deleted, with the deletion recorded
in place (l. 2855–2863). The alternatives are recorded as considered and rejected
with reasons, which is the right conduct.

§4.11's per-group check table is corrected rather than left to certify the old
shape (l. 2386): "**Not clean by the stage rule, and the verdict that claimed they
were is withdrawn.** … They are clean **only because §4.9 gives a reconciliation
round its own `sequence`** … an editor who deletes the rule reopens the defect."
The arithmetic is restated as `13 + 11 + 2 + 10 + 1 + 2 = 39`, with the note that
"a correct count is not a check". That is the best-executed repair in the corpus.

Two residuals, both small and both carried as **G5**: `STATE_MACHINE.md` still
records the `PROTOCOL.md` edit as outstanding when it has landed, and its
`StateHash::round` field has no counterpart in §4.9's payload.

## P2 — the position of an unchained event in the total order — **RESOLVED**

Disposition two was taken — delete the effect rather than specify the order —
and it is the smaller specification, which `SPEC_CS.md` §36 prefers.

> `PROTOCOL.md` §5.2 (l. 2553–2562), normative and canonical: "**No
> `chain_scope = 0` event may terminate a stage, end a hand, move a chip, or
> change any value that enters a `stage_hash`, a `STATE_HASH`, or a
> `HAND_COMPLETE` / `HAND_ABORT` body.** An unchained event is *evidence and
> transport* … the engine's state is bit-identical whether it arrived, arrived
> late, or never arrived at all."

The four-seat certificate-versus-proof race P2 described is quoted in full
(l. 2574–2589) and closed by deleting `HAND_ABORT cause = 5` (l. 2591–2604).
§4.10's enumeration carries the deletion in the field table (l. 2165): "**Value
`5` (equivocation proven) is deleted** … Every surviving value is a function of
chained content that the ordering buffer can place, which is what makes two
honest peers derive one body." What happens instead is stated rather than left
to be inferred (l. 2605–2612): the two conflicting events share a slot, §4.0 step
10a rejects the second, the next checkpoint shows two hashes, §6.3 runs, and the
hand ends through the chained path.

The finding is closed in `PROTOCOL.md`. **`STATE_MACHINE.md` did not follow, and
that is G2.**

## P3 — the required emitter set of an unattributed `HAND_ABORT` — **PARTIAL**

`STATE_MACHINE.md` produced a genuinely good answer. `PROTOCOL.md` declined to
adopt it and says so in terms, so the corpus's *binding* answer is still the one
that deadlocks.

**What `STATE_MACHINE.md` did.** §4.1 (l. 636–652) replaces the ordinary
collective stage with a **witness-independent terminal stage** and gives it its
own `stage_hash`:

```
stage_hash(s) = h("p2p-poker v1 stage",
                  [ u64_be(s), u16_be(stage_type), u8(0xFF), event_hash(body) ])
```

closing "on the first verifying copy from any peer", with `owed` deliberately
kept out of the hashed body because it is "exactly the quantity peers disagree
about". The reasoning is correct and it disposes of both horns
`PHASE1_VERIFY3.md` named — the deadlock and the fork. It is the right shape.

**What `PROTOCOL.md` did.** Nothing, and it says so:

> §4.10 (l. 2146–2158): "**Unresolved, and named rather than papered over: the
> emitter set of an unattributed abort (P3).** … under the cell above that silent
> seat is in the required emitter set, so the stage cannot complete and
> `TERMINAL(k)` is never defined. … **This document does not settle it in this
> revision** … Whatever rule is adopted must make the set a function of state
> every peer already agrees on, and **it must be adopted here, in this cell,
> before the engine is written**."

The cell above it is unchanged (l. 2124–2126): "the required emitter set is the
same set that emitted `HAND_INIT`, **minus every seat named in this abort's own
`n(1) attributed`**", and §4.11's row (l. 2362) says the same. §3.2 is unchanged
(l. 719): "Two shapes", with `HAND_ABORT` listed as collective and no third form
defined.

**Why this is PARTIAL and not RESOLVED.** This is the **M1 shape returned**: two
documents give different accept predicates for a consensus-critical stage, and
`stage_hash` of the terminal stage — hence `TERMINAL(k)`, hence `GENESIS(k+1)` —
differs byte-for-byte between them. D-009 rule 2 settled the M1 precedent by
ruling that `PROTOCOL.md`'s reading wins. Applied here, the binding answer is the
one `PROTOCOL.md` itself calls a deadlock. `STATE_MACHINE.md` §13 (l. 2996–3007)
carries the edit as an outstanding request, honestly and in the exact words the
edit needs, which is the right conduct and does not make it less blocking.

`PHASE1_VERIFY3.md` ranked P3 second of four blockers. It is now first, and it is
compounded by **G1**, which breaks the witness-independent stage as well.

## P4 — T8 and T12 in the setup chain — **RESOLVED**

The smaller of the two corrections was taken.

> `STATE_MACHINE.md` §5.2 (l. 916–920): "**There are no pre-hand certificate rows
> any more (P4).** T8 and T12 carried a `TimeoutCertificate{Crypto}` in the
> seat-order beacon, which is in the setup chain (`hand_id = 0`), and
> `PROTOCOL.md` §8.4 rules that a vote or a certificate is *never emitted* there.
> Both rows are deleted rather than defended".

The behavioural change P4 said nobody had chosen is now chosen out loud
(l. 991–1002): a seat that commits and never reveals "can no longer be unseated,
at any table size", `Fault{NoRngCommit}` / `Fault{NoRngReveal}` are "now
**unreachable**", one seat can stall any forming table until `join_deadline_ms`,
and that is "a griefing cost, not an integrity cost" because `ledger_in == 0`. T4
is widened to cover the beacon with a named default (l. 971–979), and the counts
move to 57 live rows over the numbering T1–T59 — verified by count: 57 `| T`
rows, highest number 59, two deleted.

## P5 — `Diverged` has no liveness exit — **RESOLVED as written**

> `STATE_MACHINE.md` §5.2 (l. 1165–1184): "**`Diverged` is now in scope, and the
> exclusion that stood here is deleted (P5).** … The hand deadline is therefore
> **not disarmed on entry to `Diverged`**, and on expiry T57 ends the hand there
> with the same body, the same restoration and the same absence of attribution as
> anywhere else."

Three reasons are given for why that is not a new race, and all three hold: T54
and T57 settle chips identically under D-010, so the timer decides nothing the
phase had not; `Fault{StateDivergence}` already exists from T50; and the abort
names nobody. `Diverged` entered before any hand is covered by T4 instead
(l. 960–968, guarded on `hand_id == 0 ∧ ledger_in == 0`). Between the two, every
reachable `Diverged` has an exit on paper.

**Void in practice under G1.** T57's guard in `Diverged` is that "the last complete
stage is the one before the disputed checkpoint" (l. 1101), so the abort is written
at the *disputed checkpoint's own `sequence`* — the one every peer has already
occupied with its checkpoint `STATE_HASH`. The exit is specified correctly and
cannot be taken. The verdict on P5 as filed is RESOLVED; the phase is still frozen.

## P6 — §6.3 case (b)'s abort shape — **RESOLVED**

Dissolved rather than repaired, which is the disposition D-010 makes available.

> `PROTOCOL.md` §6.3 (l. 2887–2892): "**The hand aborts neutrally**: `cause = 1`,
> `attributed = []`, `cert_hash = None`, every stack restored to its
> start-of-hand value, and the table is **not** faulted. §4.10 carries this as the
> third `attributed = []` path and its `cert_hash` rule now permits it."

§4.10 carries it (l. 2240 and l. 2255–2268), the "exactly two such cases"
enumeration is gone, and `n(2) cert_hash` now reads "`None` on every
`attributed = []` path". Folding it into `cause = 4` was considered and rejected
with a reason worth keeping — case (b) is a different fact about the world from
case (c), and recording which is "the `cause` field's only remaining job".
**OQ-B is explicitly untouched.**

## P7 — an absent seat eligible for the pot it posted into — **RESOLVED**

The one-line fix plus the assertion that would have caught it.

> `STATE_MACHINE.md` §7.5 (l. 1691, 1701): `fn build_pots(committed: &[Chips],
> folded: &[bool], dealt_in: &[bool])` … `.filter(|&s| dealt_in[s] && !folded[s])`

**I7** (l. 2615) gains `eligible ∩ ¬dealt_in == ∅` and **I8** (l. 2616) gains
`dealt_in[s]`. The consequence P7 did not name is picked up rather than left to be
discovered (l. 1731–1742): a level with two or more contributors and an empty
`eligible` set is now legally reachable, is refunded to its contributors, and the
engine must still assert `eligible ≠ ∅` on every constructed `Pot`. That is the
correct handling and it keeps I6 and I7 both true.

## P8 — cash-mode seat entry — **RESOLVED**

Closed by deleting the description, which was the defensible option P8 offered.

> `STATE_MACHINE.md` §9.4 (l. 2477–2489): "**A seat may not be added after
> `Seating`, in either mode, in this version (P8).** … The half-state — a
> capability described in prose, absent from the alphabet — is the defect; it is
> closed by **removing the description**, not by inventing a message type, a stage
> kind and a transition for a mode the MVP does not ship."

§5.3 step 0's unreachable clause is deleted (l. 1429–1434), the `PLAYER_SEAT`
citation is gone, and the consequence is stated: "I1's right-hand side is
therefore **non-increasing** after hand 1 in both modes … A player who wants to
join a running cash table opens a new one." **Q6** carries the question.

## N8 — `DECISIONS.md`'s open list — **RESOLVED**

All three sub-points are closed. The open list now carries **OQ-A** (a named,
versioned reference engine, with the interim answer "withdraw the claim"),
**OQ-D** (a dispute path not requiring the accused's signature, with D-010's
interim answer) and **OQ-F** (whether the vote/certificate/proof machinery should
still be *produced* at all now that D-010 gives it no effect). The stale N4 row is
gone and its removal is recorded rather than silent: "**Settled and removed from
this list:** the `DISPUTE` self-equivocation question (review N4) is answered by
D-009 rule 1". Three passes recorded this; the fourth edit landed.

**One defect survives inside the fix and it is not N8's:** the letters collide
across documents. `DECISIONS.md`'s `OQ-A` is the reference-engine question and
`THREAT_MODEL.md`'s `OQ-A` is the heads-up unfinishable-hand question;
`DECISIONS.md`'s `OQ-D` is `PROTOCOL.md`'s `OQ-B`. `THREAT_MODEL.md` §9.2
(l. 1862–1873) documents the collision rather than fixing it — see R-2.

## M3 — the `SmallRng` absence claim — **RESOLVED**

Both targets that had not acted have acted.

> `PROTOCOL.md` §4.4 (l. 1356–1362): the claim is quoted and withdrawn — "It
> read: *'`SmallRng` is absent because rand 0.8's `small_rng` feature is not
> enabled'*" — and replaced with "**`SmallRng` *is* compiled into the** binary",
> naming `igd-next` (through `upnp`) and `yamux` as the paths, and pointing at
> the `src/` scan that "**fails the build**" (l. 1379).

`research/CRYPTO_LIBS.md` §1.2.1 and §7.3 gained the 0.9.5 line and the
three-major table (l. 306: "`SmallRng` | **not** in the build | **in the build**
(`small_rng`, a default) | **in the build**, ungated"), and l. 1339 lists the old
claim as "**FALSE**, and this is the third repetition of the pattern". D-009
rule 3 is now satisfied in every document that carried the claim.

## R-1 — no timer covers a stalled `HAND_INIT` — **PARTIAL**

`STATE_MACHINE.md` fixed it and `PROTOCOL.md` did not, so the corpus disagrees
about when the only whole-hand timer starts.

> `STATE_MACHINE.md` §5.2 (l. 1160–1163): "A stalled `HAND_INIT` **is** covered,
> and that is R-1's fix: the hand deadline runs from `TERMINAL(k−1)` rather than
> from `HAND_INIT` (§8.2), and the phase at that moment is `AwaitingKeySetup`,
> which is in T57's scope".
>
> `PROTOCOL.md` §8.2 (l. 3319), unchanged: "The whole-hand limit
> `hand_deadline_ms` runs on the same relative basis **from `HAND_INIT`**."
> §8.4 (l. 3594–3596), unchanged: "`hand_deadline_ms` runs from `HAND_INIT`, so it
> says nothing about a stall in the setup chain."

Under `PROTOCOL.md`'s reading a `HAND_INIT` collective stage that never completes
starts no timer and is covered by none — §4.3's abandonment timer stops at
`TABLE_READY` and T4's guard is `hand_id == 0 ∧ ledger_in == 0`, which is false
from hand 2 onward. `STATE_MACHINE.md` §13 (l. 3016–3019) carries the edit as an
outstanding request. It is one sentence in one document.

## R-2 — `THREAT_MODEL.md` OQ-D's owner column — **PARTIAL**

The missing qualifier is now moot because `DECISIONS.md`'s list exists (N8), but
the finding's family produced something larger that is documented and unfixed:

> `THREAT_MODEL.md` §9.2 (l. 1862–1873): "**A defect in the letter series itself,
> recorded rather than fixed unilaterally.** … **two of those letters name
> different questions there than they do here** … so a reader following `OQ-A`
> from one document to the other lands on the wrong question. This is
> bookkeeping, but it is the bookkeeping that schedules the decisions Phase 4 is
> blocked on".

Recording a collision is better than reworderding one document's letters
unilaterally, and the refusal is reasoned. It still needs one editor.

## R-3 — `CONTRIBUTING.md`'s self-inserted authority order — **UNRESOLVED**

Unchanged, and now additionally stale on the decision range.

> `CONTRIBUTING.md` l. 10–15: "**Authority order**, and it is not negotiable by
> anything in this file: 1. `docs/SPEC_CS.md` … 2. `docs/DECISIONS.md` — **D-001
> … D-008.** … 4. `docs/DEPENDENCIES.md` for the §28 register, **this file** for
> §31."

Two passes have now raised the self-insertion; D-009 and D-010 were each an
opportunity to move the ranking into `DECISIONS.md` and neither took it. The
range is also two decisions out of date, which is the smaller half but the more
visible one.

## R-4 — `PROTOCOL.md` §5.3's first bullet — **UNRESOLVED**

Unchanged at l. 2687: "one `Vec<Option<event_hash>>` indexed by `(stage, seat)`",
followed by "The `event_class` axis is what lets a seat hold both its own
contribution at stage `s` and a `TIMEOUT_VOTE` about stage `s`". The class axis is
expressed by separate structures, not by an index component. True in effect,
loose in wording. Cosmetic, and it is the only item in this report that is.

---

# Part 2 — The D-010 sweep

Every place in all five documents where an abort, proof, certificate or
attribution could move a chip, apply a penalty, evict, block-list or unseat.
Patterns swept: `forfeit`, `forfeiture`, `block_list`, `block-list`, `blocklist`,
`allow_block`, `evict`, `unseat`, `banned`, `penalis`, `penaliz`, `penalty`, plus
every `AbortRecord`, `attributed` and `EquivocationProof` consumer.

## Chips: clean, and the receiver enforces it

The chip half of D-010 landed completely and it landed in the strongest possible
place — a **receiver check**, not an emitter obligation:

> `PROTOCOL.md` §4.10 (l. 2169–2170, 2176–2180): "`n(4) deltas` … **all zeroes,
> always (D-010)** … `n(5) final_stacks` … **the start-of-hand stacks, always
> (D-010)**" and, in the validation list, "the check that makes D-010 enforceable
> at the receiver rather than trusted at the emitter — **`n(4) deltas` is all
> zeroes and `n(5) final_stacks` equals this receiver's own `stack_at_hand_start`
> vector**. A copy that moves a chip is rejected under §4.0 like any other
> mismatch, whoever signed it and whatever it names."
>
> `STATE_MACHINE.md` §8.6 (l. 2246–2252): "`for s in all seats: stack[s] +=
> committed_hand[s]` … That is the whole of it. … It does not branch on
> `AbortKind`, on `attributed`, on `|V|`, on the seat count or on who published
> what. **T46 applies it unconditionally.**"

The forfeiture formula, its three branches, the `culprits == {}` special case, the
`AbortKind` split and the forfeiture precondition are each listed as deleted with
the deletion checkable (l. 2255–2276). **I27** is restated without its old
`attributed == []` scope and is asserted on all eleven paths that reach T46.
The 110 surviving occurrences of `forfeit*` across the five documents were read
individually: every one is a withdrawal notice, a historical quotation, a reason
for the trade, or `TdaMuckWithForfeiture` — the voluntary showdown muck, which is
a player's own choice at showdown and is not a protocol-applied penalty. **No
survivor on chips.**

## Eviction: three survivors, and one document was never edited

| # | Site | What it does | Verdict |
|---|---|---|---|
| **G3a** | `NETWORK_STACK.md` §6.6, l. 975–978 | "Proven protocol violations (an invalid signature on a message the peer originated, or an `EquivocationProof` in the `PROTOCOL.md` §5.2 sense …) go further and **land the peer in `allow_block_list::Behaviour<BlockedPeers>` via `block_peer`**" | **survivor** |
| **G3b** | `NETWORK_STACK.md` §11.5, l. 1879–1886 | "`libp2p::allow_block_list::Behaviour<BlockedPeers>` with `block_peer(PeerId)` **for peers with a proven protocol violation**: an invalid signature …, or **an `EquivocationProof`** as `PROTOCOL.md` §5.2 defines it" | **survivor** |
| **G4** | `STATE_MACHINE.md` T11, l. 979 | "`AwaitingSeatRngReveal` \| `RngReveal` \| commitment mismatch \| `Seating` \| **subject unseated**; `Fault{RngCommitmentMismatch}` — this is an equivocation-class fault with a self-contained proof" | **survivor** |
| — | `STATE_MACHINE.md` T55/T56, l. 1247–1248, 1327 | "the accused is **blocked at the protocol layer** (`PROTOCOL.md` §5.2)" ×2, and "`PROTOCOL.md` §5.2 **still blocks that key** at the transport layer" | **stale citation of a deleted rule** — the consequence is G3's, the claim is G2's |

Everywhere else the sweep is clean, and unusually well executed:

* `PROTOCOL.md` §4.0 (l. 1036–1046) quotes the block-list clause and deletes it:
  "That is an eviction applied by the protocol on the strength of an attribution,
  which D-010 point 3 forbids, so it goes. What survives is the resource bound …
  keyed on **volume**, not on fault, and identical for a buggy peer and a hostile
  one. It is not a sanction for cheating and it never keys on `attributed`."
* `PROTOCOL.md` §5.2's normative box (l. 2668–2670): "**It changes no state.** It
  ends no hand, moves no chip, unseats nobody, block-lists nobody, and refuses
  nobody a seat. It is not consumed by any transition."
* `STATE_MACHINE.md` §8.7 (l. 2284–2299) states the three negatives and then does
  the harder thing — names the one state change attribution still causes and
  argues it is not a penalty: `status := Absent` at T46, where "the seat **keeps
  its stack**, **keeps its position** …, is dealt no cards … and returns to
  `Active` at any hand boundary on its own `PlayerSitsIn`".
* `THREAT_MODEL.md` §8 limitation 4 (l. 1678–1705) discloses the cost in the terms
  `SPEC_CS.md` §18 requires, and X8 (l. 965) is rewritten as "**The rage-quit
  escape, reopened by D-010**" with the payoff column reading "**the attacker
  keeps its committed chips**". The regression is disclosed, not hidden.

**Verdict on the sweep: D-010 landed on chips and did not land on eviction.**
Three survivors, and the commission's own standard — one survivor means it did not
land — is met three times over. The mitigating fact is that all three are outside
the chip machinery and two of them are in the one document nobody edited; the
aggravating fact is G1, which makes an honest peer *manufacture* the proof that
G3 then acts on.

---

# Part 3 — The equivocation property, message by message

D-009 rule 1, as `PROTOCOL.md` §5.2 (l. 2262–2270) now states it: *no sequence of
emissions this protocol requires or permits of an honest peer may produce two
`SignedEvent`s in one slot.*

The slot key, quoted verbatim (l. 2434–2446):

```
slot(E) = (protocol_version, table_id, hand_id, sequence, event_class,
           sender_public_key == K)
        ++ subject(E)

subject(E) = ()                           when event_class == 0
           = (payload n(1) subject_seat)   when event_class == 1   (TIMEOUT_VOTE)
           = (payload n(0) subject_digest) when event_class == 2   (TIMEOUT_CERT)
```

**`event_type` is not in the key.** That is the fact this part turns on, and it
is not an oversight — it is what makes `SHOWDOWN_REVEAL` against `SHOWDOWN_MUCK`
a real check rather than a formality. It also means any two *different* message
types from one sender at one `sequence` and one `event_class` collide.

| Type(s) | Required honest emissions | Verdict |
|---|---|---|
| The 13 `chain_scope = 0` types — `HELLO`, `CAPABILITIES`, `JOIN_REQUEST`, `JOIN_ACCEPT`, `JOIN_REJECT`, `PLAYER_LIST`, the six `LOBBY_*`, `DISPUTE` | any number, any time | **Clean.** Outside the predicate by §5.2; per-type anti-replay named there |
| `TABLE_READY` | one per seat | **Clean** |
| `RNG_COMMIT`, `RNG_REVEAL` | one each, two consecutive stages | **Clean** |
| `HAND_INIT` | one derived copy per present seat | **Clean.** R-1's stall hazard is liveness, not equivocation |
| `DECK_INIT`, `DECK_COMMIT` | one per dealt-in seat per stage | **Clean** |
| `SHUFFLE_STEP`, `SHUFFLE_PROOF` | one each, single-writer | **Clean** |
| `DEAL_PRIVATE` | one per dealt-in seat, exactly `2(m−1)` entries | **Clean.** The "exactly" forbids incremental publication |
| `BOARD_REVEAL` | one per dealt-in seat per street | **Clean.** Each street its own stage |
| `SHOWDOWN_REVEAL` / `SHOWDOWN_MUCK` | exactly one of the two | **Clean**, and the exclusivity is normative rather than conventional (§4.6 l. 1697–1706) |
| The five `ACTION_*` | one per turn, single-writer | **Clean** |
| `TIMEOUT_VOTE` | one per subject per stage | **Clean since M2.** Subject in the key |
| `TIMEOUT_CERT` | one per `subject_digest` per stage | **Clean since M2.** Digest in the key; the one variable field `n(1) votes` is pinned by the receiver check |
| `PLAYER_SIT_OUT`, `PLAYER_SIT_IN`, `PLAYER_LEAVE` | one per seat at a hand boundary, single-writer, only there since M4 | **Clean** |
| `STATE_HASH`, `STATE_ACK` | one per checkpoint, **plus a reconciliation round at `sequence = s_ckpt + r`** | **Clean since P1.** This was the known survivor and the fix holds: the rounds occupy distinct `sequence` values, so the two bodies occupy two slots. Checked directly against §4.9's normative box, not taken from §4.11's table |
| `HAND_COMPLETE` | one derived copy per present seat at the terminal stage | **Clean.** The last stage before it completed, so the terminal stage takes a fresh `sequence` |
| **`HAND_ABORT`** | one derived copy per required emitter, **at the stalled stage's own `sequence`** | **FAILS. This is G1.** |

**§4.11's per-group table (l. 2385) re-certifies the failing row by the exact
sentence P1 proved unsound.** The collective-ordinary group — which still contains
`HAND_ABORT` — reads: "The emitter set is one contribution per seat per stage, and
each street's stage has its own `sequence`. **Capacity one by the stage rule**."
The editors correctly broke `STATE_HASH` and `STATE_ACK` out of that group and
wrote, in the replacement row, that the stage rule "is true of every other member
of that group and **false of these two**". It is false of a third: `HAND_ABORT` is
the one message type that is *written into a stage another message type already
occupies*. Detail in G1.

---

# Part 4 — Chip conservation

Every path that moves a chip, in both modes. **Conservation holds on every path.**
This part is materially shorter than the last pass's because D-010 deleted the
arithmetic that needed checking.

**Abort — one branch, all causes.** `stack[s] += committed_hand[s]` for every
seat (§8.6 l. 2246). `Σ` before `== Σ` after by inspection: the sum moved out of
`committed_hand` equals the sum moved into `stack`. **I2** (step-local), **I5**
(`stack[s] + committed_hand[s] == start_stack_this_hand[s]`) and **I27** (no abort
moves chips between seats) all hold, and I27 is now implied by the rule rather
than carving an exception out of it. The receiver check on `deltas` and
`final_stacks` (§4.10) makes a non-conserving abort unacceptable on the wire.

**Settlement — `build_pots`.** For levels `l₀ < l₁ < …` over positive
commitments, `Σ (lᵢ − lᵢ₋₁) × |contributors_i| = Σ_s committed[s]` exactly, and a
single-contributor band becomes a refund rather than a pot. **I6**
(`Σ pot.size + Σ refunds == Σ committed_hand`) is unaffected by P7's fix, because
a non-eligible contributor's chips stay in the pot and the level with an empty
`eligible` set is refunded to its contributors (§7.5 l. 1731–1739). Checked: the
new empty-`eligible` branch returns each contributor exactly its own `step`, so it
neither creates nor destroys.

**Odd chips.** `share = pot.size / |W|`, `remainder = pot.size % |W|`,
`0 ≤ remainder < |W|`, one chip each clockwise from the first seat left of
`button_pos` (§7.6). Exact; the tie-break is a pure function of public state. The
forfeiture remainder loop that also used this rule is gone, and §8.6 records that
"§7.6's A8 rule is now used only at settlement" — the deletion did not orphan it.

**Blinds off an absent seat.** §5.3 steps 6 and 7 have `Absent` and `SittingOut`
seats post antes and blinds while step 4 leaves `dealt_in = false`. They enter
`committed_hand`, so they are inside every sum above. Since P7 they are excluded
from `eligible`, so the deviation `STATE_MACHINE.md` §12 and `THREAT_MODEL.md`
§9.1.1 both claim — "an absent seat cannot win the blind it posts" — is now
implemented rather than merely asserted. The drain terminates: step 2 busts a zero
stack, §9.3 condition 1 ends the tournament.

**The ledger identity.** **I1** is `Σ stack + Σ committed_hand == ledger_in −
ledger_out`, with both counters moving only in hand-init step 0 (**I28**), at T10
and T47 and never inside a hand and never at T58. Tournament mode: the right-hand
side is constant after the first hand. **Cash mode: since P8 the right-hand side
is non-increasing** — `ledger_in` is fixed at the first hand init, `ledger_out`
grows as seats leave — and §9.4 says exactly that instead of describing an entry
mechanism the alphabet does not have. Conservation holds in both modes and cash
mode's variant is now reachable, which it was not before.

**Cash-mode entry** is out of scope by decision, not by omission (Q6).

**One observation, not a defect.** The only remaining asymmetry between the four
`cause` values is that `cause = 4` faults the table (§4.10 l. 2270). That is a
liveness consequence and it moves no chips, and it is correctly labelled as such.

---

# Part 5 — Termination

For every phase, under every adversarial behaviour including a peer that simply
stops.

| Phase | Exit under total silence | Status |
|---|---|---|
| 1 `Seating` | T4 `FormationAbandoned`, local timer, `ledger_in == 0` | ✔ |
| 2 `AwaitingSeatRngCommit` | T4 — widened to the whole setup chain by P4's named default | ✔ |
| 3 `AwaitingSeatRngReveal` | T4, same | ✔ |
| 4–14 `AwaitingKeySetup` … `AwaitingShowdownReveal` | T57 on `hand_deadline_ms` | **✘ under G1** |
| 15 `Settling` | T45, derived, unconditional guard, no external input | ✔ |
| 16 `HandComplete` | T47, derived | ✔ |
| 17 `Paused` | T59 on a `PlayerSitsIn`; otherwise idles with no hand live and no chips committed | ✔ (idle, not frozen) |
| 18 `HandAborted` | T46, derived `AbortSettle` | ✔ |
| 19 `TableClosed` | absorbing | ✔ |
| 20 `Diverged` | T53/T54 if signers reconcile; T57 on the hand deadline (P5); T4 before any hand | **✘ under G1** |

**P5's frozen phase is fixed and a strictly larger freeze replaced it.** Phases
4–14 and 20 — that is, every phase in which chips are committed — depend on T57,
and T57's artefact cannot be accepted wherever the emitter contributed to the
stalled stage. On the MVP's shipped regime (heads-up, `|V| = 1`, every certificate
inert) that is the ordinary case, not a corner. Detail in G1.

Two termination properties that do hold and are worth recording:

* **No timeout ends the tournament or the cash game.** §4.10 (l. 2270–2278) and
  D-006 point 5 are carried consistently; the next hand begins automatically.
* **Liveness is explicitly not owed, and the bound is stated.** `STATE_MACHINE.md`
  §12: "a stall costs `hand_deadline_ms` per hand and can be repeated every hand".
  That is an honest statement of a real limit, and it presumes the hand *ends* —
  which is what G1 takes away.

---

# Part 6 — New defects

Ordered by severity. Continuing the letter series: **G**.

## G1 — the terminal `HAND_ABORT` is written into the stalled stage's own slot, so every honest peer that contributed to that stage signs a second body into an occupied slot; the abort is rejected under §4.0 step 10a and is a verifying `EquivocationProof` against its own emitter

**Severity: highest. Consensus-critical, and it is D-009 rule 1's fifth
recurrence** — after A-4 (lobby/join), N4 (`DISPUTE`), M2 (`TIMEOUT_VOTE`) and P1
(`STATE_HASH`). It was created by the P3 fix, which is precisely the mechanism
`PHASE1_VERIFY3.md` named in its last sentence: *the defect is never in the rule;
it is in the path the rule newly makes load-bearing.*

**Where the abort is written.** `STATE_MACHINE.md` §4.1 (l. 626–630) and T57
(l. 1101):

> "whose `(stalled_sequence, parent_hash)` is this peer's chain head for that hand
> — **the successor of the last complete stage, and its `stage_hash`** (T57)"
> "`parent_hash == stage_hash` of the last **complete** stage of this hand and
> `stalled_sequence` its successor"

`PROTOCOL.md` §3.2 (l. 760): "Every event of stage `s` carries
`previous_event_hash = stage_hash(s-1)`." An event chaining from the last complete
stage `s−1` **is** an event of stage `s`, and `s` is the stalled stage's index.
So `sequence(HAND_ABORT) == stalled_sequence == the stalled stage's own index`.
There is no third option: chaining from `stage_hash(s)` is impossible because
stage `s` never completed, which is what "stalled" means.

**The collision.** A stage stalls when *some* required emitters were heard and at
least one was not. Take the MVP's shipped regime — heads-up, `|V| = 1`, every
certificate inert (§8.3), so `hand_deadline_ms` is the only terminus:

1. Seats `A` and `B` reach the `DECK_INIT` collective stage, `sequence = s`.
2. `A` emits its `DECK_INIT`: slot `(pv, table_id, hand_id, s, 0, A)`.
3. `B` goes silent. The stage never completes.
4. `hand_deadline_ms` expires. `A` emits its own copy of
   `HAND_ABORT{cause = 1, attributed = [], cert_hash = None}` at `sequence = s`,
   `event_class = 0`, sender `A`: slot `(pv, table_id, hand_id, s, 0, A)`.

`event_type` is not in the slot key (§5.2 l. 2434–2446); `subject(E) = ()` for
`event_class == 0`; `HAND_ABORT` is `chain_scope = 1`, `event_class = 0` (§4.11
l. 2362, §2.3 l. 392). **Same slot, different body.** Both halves of the defect
follow mechanically:

**(a) Liveness — the abort cannot be accepted, so P3's deadlock returns by a new
route.** `PROTOCOL.md` §4.0 step 10a (l. 1012): "Anti-replay: `table_id`,
`hand_id`, `sequence`, `event_class`, `previous_event_hash` for a chained event"
→ "**violation** or duplicate-drop". A differing body at an occupied slot is a
violation at *every* receiver, including at `A`'s own local view. The only peers
whose slot at `s` is free are those that emitted nothing at the stalled stage —
which is exactly the set of silent peers the abort exists to dispose of. Heads-up
there is no such peer that is also willing to emit. **T57 can never fire**, and
`STATE_MACHINE.md`'s witness-independent stage — the whole of the P3 fix — is
unreachable. `TERMINAL(k)` is never defined, `GENESIS(k+1)` never exists, and the
table cannot start another hand.

**(b) Equivocation — the honest peer manufactures a proof against itself.**
§5.2's predicate is satisfied exactly: two `SignedEvent`s, both canonical, both
verifying under `A`'s key, both `chain_scope == 1`, equal slot keys, different
`event_hash`. This is behaviour the protocol **requires** of `A` — §4.10 makes
every required emitter derive and emit its own copy — so it is D-009 rule 1's
predicate met by mandatory honest conduct, for the fifth time.

**What it costs, and why D-010 does not dissolve this one.** D-010 dissolved the
chip half: the proof moves nothing. Two consequences survive it.

* The liveness half (a) is not a chip defect at all, and D-010's own note says so
  of P3: "it is a liveness defect, not a chip defect" (`PROTOCOL.md` §4.10
  l. 2150).
* The eviction half survives through **G3**: `NETWORK_STACK.md` §6.6 and §11.5
  still `block_peer` on an `EquivocationProof`, and `STATE_MACHINE.md` T55/T56
  still cite that consequence as live. So the composite is the four-pass pattern
  reconstituted with the chips removed: **an honest peer follows the protocol,
  manufactures a proof against itself, and has its key block-listed at the
  transport layer.** It keeps its stack and loses the network.

**Which paths collide, checked one by one.** Not every abort does, and the
distinction matters for the fix:

| Abort path | Last complete stage | Collides? |
|---|---|---|
| `cause = 1`, `hand_deadline_ms`, at a **collective** stalled stage | `s−1` | **Yes**, for every emitter that contributed at `s` |
| `cause = 1`, `hand_deadline_ms`, at a **single-writer** stalled stage | `s−1` | **No** — only the writer would have occupied `s`, and it is silent |
| `cause = 1`, certified-subject path | `s`, closed by the certificate (§3.2 l. 765) | **No** — the abort takes `s+1` |
| `cause = 2`, invalid `SHUFFLE_PROOF` (single-writer) | `s−1` | **No** — the shuffler is `attributed` and excluded |
| `cause = 3`, invalid DLEQ at `DEAL_PRIVATE` / `BOARD_REVEAL` (collective) | `s−1` | **Yes**, for every honest revealer that already emitted at `s` |
| `cause = 4` from T54 | the completed reconciliation round | **No** — the abort takes the next `sequence` |
| `cause = 1` from T57 **in `Diverged`** | "the one before the disputed checkpoint" (T57) | **Yes, always** — every peer has already signed its checkpoint `STATE_HASH` at that `sequence`. This is what voids P5 |
| §6.3 case (b) | depends on where the divergence was noticed | **Yes** wherever the stalled stage was collective |

**Why the last pass could not see it.** P3 was filed as a defect in the *emitter
set*, and the fix correctly changed the emitter set. The `sequence` the terminal
stage occupies was never in question, because until P3 the abort had a set that
could never complete and the question of what slot it landed in never arose. The
check D-009 rule 1 mandates for every new chained emission was not run on it,
for the same reason it was not run on P1: nobody thought a new emission had been
added — the message type is old, only its stage shape changed.

**Correction.** The terminal `HAND_ABORT` needs a `sequence` that no other event
occupies. Two candidates, ranked:

1. **Abandon the stalled stage and give the abort `s+1`, chaining from a defined
   sentinel rather than from `stage_hash(s)`.** The natural sentinel is the
   partial stage's `parent_hash` carried explicitly in the body — which
   `STATE_MACHINE.md` already puts there as `parent_hash`. The abort then reads
   "the stage at `s` is abandoned; here is what it chained from", occupies a
   virgin slot at `s+1`, and stays placeable by the ordering buffer. This costs
   one sentence in §3.2 defining `stage_hash(s)` for an abandoned stage, and it is
   the smallest edit that closes both halves.
2. **Give the terminal abort its own `event_class = 3`.** Mechanically sufficient
   — the class is in the slot key — and it needs no new `sequence` semantics. But
   it extends an envelope field whose three values `PROTOCOL.md` §2.3 calls
   exhaustive, it needs a `subject(E)` clause, and §3.2's "which events enter a
   `stage_hash`" rule would need a third arm. Larger than it looks.

Whichever is taken, **§4.11's per-group check table (l. 2385) must lose
`HAND_ABORT` from the group whose reason is "capacity one by the stage rule"** —
that is the sentence that certified this row as clean, and it is the second time
that same sentence has certified a defect.

## G2 — `STATE_MACHINE.md` T55 consumes an `EquivocationProof` and ends a hand with it, which `PROTOCOL.md` §5.2 forbids in a normative box that names `STATE_MACHINE.md` as the document reproducing it; the `AbortKind` it produces has no `cause` value left to carry it

**Severity: high, consensus-critical. This is P2 reopened**, and it is the M1
shape for the third time — two documents, opposite answers, on an accept
predicate a modified client can trigger at will.

`PROTOCOL.md` §5.2's box (l. 2660–2670), which states in its own header that it
"is canonical for the corpus; **`STATE_MACHINE.md` reproduces it and does not
restate it in its own words**":

> "**It changes no state.** It ends no hand, moves no chip, unseats nobody,
> block-lists nobody, and refuses nobody a seat. **It is not consumed by any
> transition.**"

And the general rule it rests on (l. 2555–2558): "**No `chain_scope = 0` event may
terminate a stage, end a hand, move a chip**…". An `EquivocationProof` reaches a
peer inside a `DISPUTE`, which is `chain_scope = 0` (§4.11 l. 2358).

`STATE_MACHINE.md`, two rows:

> T55 (l. 1247): "| **any phase except `TableClosed`**, when a hand is live —
> 2–15 and 20 | `EquivocationProof` | the proof verifies … | **`HandAborted`** |
> `Fault{Equivocation}` — **evidence only** (D-010 point 2);
> `AbortRecord{kind: Equivocation, attributed: [accused]}`; **restoration** …"
> T56 (l. 1248): "… `Fault{Equivocation}` naming `accused`; no abort, no chip
> movement …; **the accused is blocked at the protocol layer (`PROTOCOL.md`
> §5.2)** and the proof is retained"

Three separate contradictions, and each is independently blocking:

1. **T55 ends a hand on an unchained event.** Forbidden by §5.2's general rule.
   The failure P2 documented is exactly reachable again: two honest peers, one
   applying a certificate first and one applying a proof first, derive two
   different terminal bodies, the terminal stage never completes, and the session
   cannot start another hand. The attacker supplies only the delivery order.
2. **`AbortKind::Equivocation` has no wire representation.** §4.10's `cause`
   enumeration is `1`–`4` and states "**Value `5` (equivocation proven) is
   deleted**". T55 produces an `AbortRecord{kind: Equivocation}` that no
   `HAND_ABORT` can carry — the C-6 defect class (an engine alphabet containing a
   variant no legal message produces) in its sixth appearance, after
   `RevealRejected`, `StateAck`, `EquivocationProof`, `TimeoutCertificate{Join}`
   and the beacon rows.
3. **Both rows assert a transport consequence `PROTOCOL.md` deleted.** "the
   accused is blocked at the protocol layer (`PROTOCOL.md` §5.2)" — §5.2 says
   "block-lists nobody" and §4.0 (l. 1039–1041) quotes the block-list clause and
   deletes it as "an eviction applied by the protocol on the strength of an
   attribution, which D-010 point 3 forbids". `STATE_MACHINE.md` l. 1327 repeats
   it in prose: "`PROTOCOL.md` §5.2 **still blocks that key** at the transport
   layer."

**Why this was missed.** `STATE_MACHINE.md` §8.7 and §12 both state, correctly and
at length, that no proof moves a chip or removes a player, and §8.6's deletion
list is scrupulous about the *chip* consequences of T55. The edit pass treated
D-010 as a chip question and P2 as `PROTOCOL.md`'s question, and T55's **phase
transition** — the thing P2 was about — was left as it stood. The row's own note
(l. 1316–1320) shows the reasoning: "Under **D-010 it costs a hand and nothing
else**", which reads as a justification for keeping the abort, at the moment
`PROTOCOL.md` was deleting the ability to abort.

**Correction.** T55 loses its `HandAborted` destination and its `AbortRecord`, and
becomes T56's shape in every phase: retain, record `Fault{Equivocation}` as
evidence, change no other state. `AbortKind::Equivocation` is deleted from the
type. Both rows drop the block-listing sentence. If instead `PROTOCOL.md` is to be
overruled, that needs a numbered decision and it reopens P2 — but §5.2's box is
right and the engine should follow it.

## G3 — `NETWORK_STACK.md` block-lists a peer on an `EquivocationProof`, in two places, in the one document never edited for D-010

**Severity: high. This is the D-010 sweep's survivor** and it is what gives G1 a
victim.

> §6.6 (l. 975–978): "Proven protocol violations (an invalid signature on a
> message the peer originated, or an `EquivocationProof` in the `PROTOCOL.md` §5.2
> sense — two conflicting **chained** events, `SPEC_CS.md` §14) go further and
> land the peer in `allow_block_list::Behaviour<BlockedPeers>` via `block_peer`."
> §11.5 (l. 1879–1886): "`libp2p::allow_block_list::Behaviour<BlockedPeers>` with
> `block_peer(PeerId)` for peers with a **proven** protocol violation: an invalid
> signature on a message they originated, or an `EquivocationProof` as
> `PROTOCOL.md` §5.2 defines it".

Against D-010 point 3 — "No peer is block-listed, unseated or penalised by the
protocol on the strength of a proof" — and against `PROTOCOL.md` §4.0, which
quotes this exact clause and deletes it. `NETWORK_STACK.md` contains no occurrence
of `D-010`.

**Both readings are wrong.** Read as a *protocol* action it is the eviction D-010
forbids. Read as a *local, user-visible* action it is still wrong as written,
because §11.5 says "Blocking is local, is persisted in the profile, and is never
applied on suspicion" — describing an automatic rule with a manual-sounding
justification. `PROTOCOL.md` §5.2 states the correct disposition: "whether to sit
with that key again is a **user** decision", and §4.0 keeps the *volume*-keyed
resource bound while deleting the *fault*-keyed one. The distinction
`NETWORK_STACK.md` is missing is exactly that one.

**Correction.** §6.6 keeps its rate-limit and disconnect ladder — which is keyed
on volume and is untouched by D-010 — and loses the `EquivocationProof` clause and
the invalid-signature clause from the `block_peer` sentence. §11.5 becomes a
description of a UI-driven block list: the behaviour stays compiled in, the trigger
becomes the user. The document also needs the D-010 row its four siblings have.

## G4 — T11 unseats a seat on the strength of a self-contained proof, three rows above the paragraph that says this document has no peer-removal transition

**Severity: medium-high.** A D-010 point 3 survivor, and an internal contradiction
inside one document.

> `STATE_MACHINE.md` T11 (l. 979): "| `AwaitingSeatRngReveal` | `RngReveal` |
> commitment mismatch | `Seating` | **subject unseated**; `Fault{RngCommitmentMismatch}`
> — **this is an equivocation-class fault with a self-contained proof** (the
> commitment and the bad opening) |"

Against the same document's §8.7 (l. 2286–2288): "it produces **no block-listing
and no unseating** … **There is no peer-removal transition in this document and
D-010 point 3 forbids adding one** (§5.2)"; and §12 (l. 2741): "It does not
forfeit, does not unseat, does not block-list, and does not penalise."

The contradiction is sharpened by P4's own reasoning eight lines below T11
(l. 999–1001), which rejects the alternative fix for T8/T12 because it would "buy
an unseating that **D-010 point 3 forbids the protocol from performing anyway**" —
while T11, in the same table, performs one.

**Mitigating and aggravating.** Mitigating: no chips exist at the beacon
(`ledger_in == 0`, I1, I28), so nothing is taken, and a seat whose reveal does not
open its own commitment has produced an artefact no honest client can produce —
this is closer to a validity rejection than to an adjudication. Aggravating: it is
the only place in the corpus where a *proof* produces a *removal*, D-010 draws no
"but there were no chips" exception, and a reader cannot tell from the corpus
whether T11 is intended, overlooked, or carved out.

**Correction, one of:** (a) make the mismatched `RngReveal` a `Rejection` — the
seat stays seated, the beacon stalls, and T4 closes the table with no attribution,
which is exactly the disposition P4 chose for the adjacent rows and is consistent;
or (b) state a carve-out explicitly, that a seat whose own signed opening
contradicts its own signed commitment may be removed before any hand exists, and
say why that is not a D-010 point 3 eviction. Option (a) is smaller, matches the
ruling already made two rows away, and needs no new principle. Either way
`Fault{RngCommitmentMismatch}` must join `NoRngCommit` / `NoRngReveal` in whatever
disposition is chosen, so the three beacon faults do not have two dispositions
between them.

## G5 — `StateHash::round` has no field in `PROTOCOL.md` §4.9's payload, and two of `STATE_MACHINE.md`'s three outstanding objections have already been satisfied

**Severity: low. An implementer can pick a sane default for both halves**, which
is why this is not in the blocking list — but both cost a reader time and one of
them will read as a wire-format disagreement.

**(a) The field.** `STATE_MACHINE.md` §4.1 (l. 583–602): "**`StateHash::round` is
new** … `round == 0` is the checkpoint emission … `round >= 1` is a
**reconciliation round**", and T49/T50/T51 read `round == 0` while T53/T54 read
`round >= 1`. `PROTOCOL.md` §4.9 (l. 1985–1986) declares the payload as exactly
`n(0) checkpoint: u16`, `n(1) state_hash: bytes[32]`, `n(2) transcript_head:
bytes[32]` — **no `round`**. `PROTOCOL.md` distinguishes rounds by `sequence`
alone.

The sane default is available and `STATE_MACHINE.md` states the arithmetic itself
(l. 597–598): "round `r` is stage `s_ckpt + r` of the hand chain". So
`round = sequence − s_ckpt`, and `s_ckpt` is known in `Diverged` because the phase
was entered from that checkpoint. An implementer derives it and nothing breaks.
It should still be settled: either §4.9 gains the field, or §4.1 says `round` is
derived and not carried. `STATE_MACHINE.md` §13's request (l. 3009–3010) asks for
"`sequence = s_ckpt + r`, `round = r`", which reads as asking for the field.

The same applies to `HandDeadlineAbort`'s `stalled_sequence` and `parent_hash`,
which have no counterpart in §4.10's field table — but these are cleanly derivable
from the envelope's own `sequence` and `previous_event_hash`, and no reader will
be misled.

**(b) Stale objections.** `STATE_MACHINE.md` §5.2 (l. 1281–1295) and §13
(l. 3008–3014) both still say "**an event `PROTOCOL.md` does not yet define**" and
quote §6.3 step 3 as ending "Play resumes from the checkpoint with no further
action". That sentence is gone: §6.3 step 3 was rewritten (l. 2844–2856) and §4.9
carries the normative box. Likewise §4.11's check table has already lost the claim
the objection asks it to lose. One of the three objections — the third stage shape
(P3) — is live and correct; the second is satisfied; the third (§8.2, R-1) is
live. Leaving a satisfied objection standing next to two live ones makes it harder
to see which is which, and this document's own accuracy about what other documents
say is otherwise its strongest quality.

---

# Part 7 — The readiness verdict

## **NOT-READY**

`STATE_MACHINE.md` is not implementable without guessing, and the reason has moved
again. The last pass was blocked by four paths D-010's predecessors created and
left uncompleted. Three of those four are now closed properly — P1, P2 and P4 are
model repairs, and P6, P7 and P8 with them. What blocks now is narrower and
sharper: **the corpus does not agree with itself about how a hand ends.**

Three of the four blockers below are the same disagreement seen from three angles:
`PROTOCOL.md` and `STATE_MACHINE.md` give different answers about the terminal
stage — its shape (P3), its `sequence` (G1), and what may trigger it (G2). All
three determine `TERMINAL(k)`, hence `GENESIS(k+1)`, hence whether two honest
clients can play a second hand.

### Blocking — must be settled before the engine is written

1. **G1 — the `sequence` of the terminal `HAND_ABORT`.** Name the transition:
   **T57**, and every collective-stage path of T27/T41/T44 and the `cause = 3`
   aborts. Name the invariant: **I21** (a rejected event leaves state
   bit-identical) is what fires instead of the abort. As written the abort is
   unacceptable under §4.0 step 10a wherever its emitter contributed to the stalled
   stage, so every phase in which chips are committed has no exit, and the same
   event is a verifying `EquivocationProof` against its honest emitter.
   **An implementer could not pick a sane default:** both candidate fixes change
   `PROTOCOL.md` §3.2's normative stage rules, and choosing wrong forks the chain
   silently rather than failing loudly.
2. **P3 — the shape of that stage.** Name the transition: **T57**. Name the
   invariant: **I27** is asserted after T46 on a path that cannot be reached.
   `STATE_MACHINE.md` §4.1 specifies a witness-independent stage with its own
   `stage_hash` formula; `PROTOCOL.md` §3.2 says "Two shapes" and §4.10 says in
   terms that it "does not settle it in this revision" and that the rule "must be
   adopted here, in this cell, before the engine is written". Under D-009 rule 2's
   own precedent `PROTOCOL.md` wins, and `PROTOCOL.md`'s answer deadlocks.
   **An implementer could not pick a sane default:** the two documents give
   different bytes for `TERMINAL(k)`, and either choice is unilateral.
3. **G2 — whether T55 exists.** Name the transition: **T55**, and **T56**'s
   block-listing note. `PROTOCOL.md` §5.2's normative box says an
   `EquivocationProof` "is not consumed by any transition"; T55 consumes one, ends
   a hand with it, and produces an `AbortKind` whose `cause` value §4.10 deleted.
   **An implementer could not pick a sane default:** deleting T55 changes what
   ends a hand; keeping it reopens P2's two-honest-peers fork.
4. **G3 — whether a proof block-lists a key.** Name the section:
   `NETWORK_STACK.md` §6.6 and §11.5. This is the D-010 sweep's survivor and it is
   what turns G1 from a liveness bug into an attack on an honest peer.
   **An implementer could pick a sane default** — follow `PROTOCOL.md`, which is
   higher authority and unambiguous — but it is listed as blocking because a
   security property must not rest on an implementer noticing which of two
   documents to disbelieve.

### An implementer would additionally have to decide these

5. **G4 — whether T11 unseats.** Name the transition: **T11**. Sane default
   available: treat it as a `Rejection`, matching the P4 ruling two rows away.
6. **R-1 — where `hand_deadline_ms` starts.** Name the sections: `PROTOCOL.md`
   §8.2 against `STATE_MACHINE.md` §5.2. Sane default available: start at
   `TERMINAL(k−1)`, which is strictly safer and is the engine's own reading.
7. **G5 — whether `StateHash` carries `round`.** Sane default available: derive it
   from `sequence − s_ckpt`.
8. **R-2** — the `OQ-A` / `OQ-D` letter collision across three documents. Not a
   guess, but it misroutes a reader following a cross-reference.
9. **R-3** — `CONTRIBUTING.md`'s self-inserted authority order, still at "D-001 …
   D-008". Two passes, no edit.
10. **Q6, Q7, Q3 / Q-02 / OQ-E, Q-01 / Q1** all carry named, interim defaults an
    implementer can build against and knows are interim. **Not guesses**, and they
    are correctly flagged.

### What is genuinely ready, and it is more than last pass

The 20 phases, 57 live transitions over the numbering T1–T59, and 29 invariants
are internally consistent and were re-counted directly: 57 `| T` rows, highest
number 59, two deleted with the deletion recorded; 29 `| **I**` rows with no gaps.
§5.1 states the count once. Every declared `Event` variant has a consuming row or
an explicit statement that it deliberately has none, and the document now closes
each such case out loud rather than leaving it inferable — `Show`, `DealComplete`,
`FormationAbandoned` and the deleted beacon certificates are each argued in place.

The chip half of D-010 is complete, enforceable at the receiver, and conserves on
every path in both modes — and it is *smaller* than what it replaced, which is the
result D-010 was adopted for. §6's legal-action computation, §7's blinds, button,
minimum raise, incomplete all-in, side pots, odd chips and showdown are specified
to the level an implementer can code against, with TDA citations attached. The
equivocation slot key is stated once and pointed at from four documents. P1's
repair is the best-executed edit in the corpus: it fixed the rule, fixed the
document that consumed it, and then went back and corrected the check table that
had certified the defect as clean — including the observation that "a correct count
is not a check". The honesty is also intact: `PROTOCOL.md` §4.10 declines to settle
P3 in its own voice rather than papering it, `THREAT_MODEL.md` X8 discloses the
reopened rage-quit escape as a payoff to the attacker, and `THREAT_MODEL.md` §9.2
records the letter collision instead of resolving it unilaterally.

### The pattern, fifth iteration

D-010 was adopted to dissolve a class rather than patch it again. **On its own
terms it worked**: no defect in this pass ends in an honest peer's chips moving,
P1/P2/P4 cost nothing once a proof moves no chips, and the chip sweep is clean. The
class that survived is the *other* half of the same machinery — eviction — and it
survived in the one document nobody opened.

And the meta-pattern held exactly. `PHASE1_VERIFY3.md` closed by naming it: *the
defect is never in the rule; it is in the path the rule newly makes load-bearing.*
G1 is that sentence again. The P3 fix was correct about the emitter set, correct
about the `stage_hash`, correct about `owed`, and correct about why every prior
candidate failed — and it moved the terminal abort into a slot the equivocation
predicate had been sharpened four times to guard. Nobody re-ran the check, for the
same reason nobody re-ran it on P1: the message type was old, and only its stage
shape had changed.

The lesson to carry into whatever edit follows is narrower than "check
everything". It is: **when a fix changes where an event sits in the chain, the
D-009 rule 1 check must be re-run on it, even when no new message type was
added.** Both P1 and G1 were slot defects introduced by editors who were changing
something else, and both were certified clean by §4.11's per-group table using the
same sentence. That table should be rewritten to derive its verdicts from
`(sequence, event_class, subject)` per type rather than from stage-kind groups,
because the group is not what the predicate indexes.

---

# Part 8 — What must happen before Phase 2

Ordered by what endangers the most.

1. **G1 — give the terminal `HAND_ABORT` a `sequence` no other event occupies.**
   Preferred: abandon the stalled stage, define `stage_hash` for an abandoned
   stage, and write the abort at `s+1` chaining from the body's own `parent_hash`.
   Correct §4.11's per-group table in the same edit — remove `HAND_ABORT` from the
   "capacity one by the stage rule" group, which has now certified two defects.
2. **P3 — adopt the witness-independent terminal stage in `PROTOCOL.md` §3.2 and
   §4.10**, as `STATE_MACHINE.md` §13 asks, in the same edit as G1 since they touch
   the same two cells. §3.2 gains a third shape; §4.10's cell and its "which is
   what those two cases want" sentence are restated for `cause = 1` on the
   hand-deadline path, `cause = 4`, and §6.3 case (b) alike.
3. **G2 — delete T55's abort.** T55 becomes T56's shape: retain, record the fault
   as evidence, change nothing else. Delete `AbortKind::Equivocation`. Delete the
   "blocked at the protocol layer" sentence from T55, T56 and §5.2's prose at
   l. 1327.
4. **G3 — `NETWORK_STACK.md` §6.6 and §11.5 lose the fault-keyed `block_peer`.**
   Keep the volume-keyed ladder. Add the D-010 row the other four documents carry;
   this document has none, which is how it was missed.
5. **G4 — T11 becomes a `Rejection`**, or gains an explicit, argued carve-out.
   Whichever, the three beacon faults get one disposition between them.
6. **R-1 — `PROTOCOL.md` §8.2 starts `hand_deadline_ms` at `TERMINAL(k−1)`**, and
   at checkpoint 1's `STATE_ACK` for a table's first hand. One sentence.
7. **G5 — settle whether `STATE_HASH` carries `round`**, and delete
   `STATE_MACHINE.md`'s two satisfied objections in §5.2 and §13 so the one live
   objection stands alone.
8. **R-2 — one editor reconciles the `OQ-*` letters** across `DECISIONS.md`,
   `PROTOCOL.md`, `THREAT_MODEL.md` and `STATE_MACHINE.md`.
9. **R-3 — move the authority order into `DECISIONS.md`** as a numbered decision,
   and delete it from `CONTRIBUTING.md`; the range there is two decisions stale.
10. **R-4** — one word in `PROTOCOL.md` §5.3, in whatever edit touches §5.

Nothing in this verification reverses D-009, D-010 or any ruling of
`research/PHASE0_FIXPLAN.md`. D-010 is the right decision and its chip half is
executed well. Two things it did not do, and both should be recorded for the
decision that revisits it: it was applied as a *chip* decision, so the eviction
half of point 3 was only swept in the documents whose subject is chips; and it
does not, on its own, protect a peer that manufactures a proof against itself,
because the proof still exists and something downstream may still act on it. G1
plus G3 is that composite, and it is the fifth iteration of a pattern that has now
been named four times.
