# PHASE2_GATE2.md — the Phase 2 readiness gate, second attempt

**Commission.** Five passes ran. The fifth returned NOT-READY on four blockers,
three of which were one disagreement between `PROTOCOL.md` and `STATE_MACHINE.md`
about the terminal stage. **D-011** made `PROTOCOL.md` the owner, required the
other documents to reference rather than restate, and fixed the anti-replay slot
key as one literal tuple. This pass answers one question: did that work.

**Method.** Every verdict is quoted from the document as it now stands, with line
numbers. No specification document was edited. Crate claims were not re-derived.
The slot-key and termination sweeps were run **independently** of the corpus's own
tables — the check tables were read afterwards, to compare, not to source the
verdict. That inversion is deliberate: §4.11's grouped predecessor certified two
defects with one sentence, and a sweep that reads the corpus's own certificate
first inherits its blind spot.

**Files as read.** `STATE_MACHINE.md` 15:37, `PROTOCOL.md` 15:10,
`THREAT_MODEL.md` 15:10, `NETWORK_STACK.md` 15:02, `DECISIONS.md` 14:49,
**`CRYPTOGRAPHY.md` 14:21**, **`CONTRIBUTING.md` 12:55**.

> The two trailing timestamps are the first finding, and the pattern is the one
> the last gate opened with. `CRYPTOGRAPHY.md` contains **zero occurrences of
> `D-011`** against 35 in `PROTOCOL.md`, 74 in `THREAT_MODEL.md`, 26 in
> `NETWORK_STACK.md` and 20 in `STATE_MACHINE.md`. `CONTRIBUTING.md` contains
> zero occurrences of `D-009`, `D-010` **or** `D-011`. Last pass the unswept
> document was `NETWORK_STACK.md` and the D-010 sweep's three survivors were all
> inside it. This pass the unswept documents are `CRYPTOGRAPHY.md` and
> `CONTRIBUTING.md`, and the restatement sweep's survivors are inside them.

## Verdict counts

| Verdict | Count | Items |
|---|---:|---|
| **RESOLVED** | 8 | P3, G1, G2, G3, G4, G5, R-1, R-4 |
| **PARTIAL** | 1 | R-2 |
| **UNRESOLVED** | 1 | R-3 |
| **REGRESSED** | 0 | — |

**Eight new defects, H1–H8.** Two are blocking. **H1** is the D-011 pattern's
sixth iteration in its purest form: the fix that made `TERMINAL(k)` independent of
the aborted hand's content left `status := Absent` derived from the *accepted
abort copy's* `attributed`, and the witness-independent stage is precisely what
lets two peers accept different copies — so the fork moved from the terminal hash
to the next hand's `dealt_in`. **H2** is that `seat_flags` — a component of
`roster_hash(k)`, hence of `GENESIS(k)`, hence of every event in the corpus —
appears **exactly once in all five documents and is never defined anywhere**.

**Readiness verdict: NOT-READY.** Reasons in Part 8.

---

# Part 1 — Per-item verdicts

## P3 — the shape of the terminal stage — **RESOLVED**

`PROTOCOL.md` adopted `STATE_MACHINE.md`'s shape and then improved on it, which is
D-011 rule 1 working exactly as designed: the owner ruled, and it ruled in the
non-owner's favour on the merits.

> `PROTOCOL.md` §3.2 (l. 836–851), normative: "**Witness-independent terminal
> stage — normative, and the disposition of P3.** Used by `HAND_ABORT` and by
> nothing else. … A witness-independent terminal stage has **no required emitter
> set**. Any seat of the hand's `HAND_INIT` set may emit its copy … and the stage
> **closes at a receiver on the first copy that verifies and passes §4.10's
> acceptance gate**. Waiting for a second copy is never required and a silent seat
> never blocks it. … It is the **last** stage of its chain. Nothing chains from
> it, so it has **no `stage_hash`**."

The part that is better than what was asked for is §3.1 (l. 733–745):

```
TERMINAL(k) = stage_hash of the HAND_COMPLETE stage          the hand was decided
            = ABORT_TERMINAL(k)                              the hand was aborted

ABORT_TERMINAL(k) = h("p2p-poker v1 abort-terminal",
                      [ u16_be(protocol_version), table_id, u64_be(k),
                        GENESIS(k) ])
```

> "An abort is, by definition, the outcome in which the peers **could not agree
> about the middle of the hand** … Deriving `TERMINAL(k)` from any of those
> quantities derives it from the one thing that is in dispute. `GENESIS(k)` is not
> in dispute."

`STATE_MACHINE.md`'s `stage_hash` formula for the terminal stage is **not**
adopted and is deleted from that document, correctly: with `ABORT_TERMINAL(k)` a
function of `GENESIS(k)` alone, the stage needs no hash. §4.10 (l. 2262–2272)
carries the ruling in the cell that declined to give it last pass, and quotes the
cell it replaces. The cost — an aborted hand's events are not bound into
`GENESIS(k+1)` — is stated rather than hidden (l. 763–772) and is argued from the
fact that an aborted hand changes no later value.

This is the best-executed repair in the corpus and it supersedes P1's. **It also
created H1**, which is Part 7's first entry.

## G1 — the `sequence` of the terminal `HAND_ABORT` — **RESOLVED**

Closed by the definition rather than by a rule about one message type, which is
what D-011 rule 2 is for.

> `PROTOCOL.md` §5.2.1 (l. 2662–2691), the whole corpus's one slot key:
> `slot(E)` is the **8-tuple** `(protocol_version, table_id, hand_id, sequence,
> sender_public_key, event_class, event_type, subject(E))`.
>
> §5.2.1 (l. 2708–2717): "`event_type` is the component G1 needed. The terminal
> `HAND_ABORT` of §4.10 is written at the **stalled stage's own `sequence`** — it
> must be, because a stage that never completed has no `stage_hash` to chain a
> successor from … Without `event_type` those are one slot … With `event_type`
> they are two slots and both halves are gone."

The price is paid out loud in the same section (l. 2721–2727): two *different*
`ACTION_*` types claimed for one turn, and `SHOWDOWN_REVEAL` against
`SHOWDOWN_MUCK`, stop being equivocations and become validity rejections under
§4.0 step 12. Both remain rejected, both remain detected through the divergence
path, and under D-010 what is lost is the label, not an effect. That trade is
argued rather than asserted, and it is the right side of it.

`PROTOCOL.md` §3.2 (l. 878–884) carries the consequence in the stage rules — "at
an abandoned stage, a partial set of contributions and the abort that abandons it
may both exist for the same `sequence` … Those are two slots, not one, because
`event_type` is in §5.2.1's key" — and §5.3 (l. 3050–3059) carries the matching
storage axis. See **H3** for the one place the storage and the key do not agree.

## G2 — whether T55 exists — **RESOLVED**

Deleted rather than repaired, on the owner's ruling.

> `PROTOCOL.md` §5.2.4 (l. 2999–3014), normative box: "**Nothing consumes it.**
> There is no transition, in any document, that takes an `EquivocationProof` as
> its input event. It ends no hand, moves no chip, unseats nobody, block-lists
> nobody, refuses nobody a seat, and produces no `AbortRecord`. … **There is no
> wire representation for an equivocation-caused abort and none will be added.**"
>
> `STATE_MACHINE.md` §5.2 (l. 1432–1442): "**T55 and T56 are deleted, and nothing
> in this document consumes an `EquivocationProof` (G2).** … The two rows go,
> `Event::EquivocationProof` goes with them (§4.1), `AbortKind::Equivocation` goes
> with that (§8.6), **`Fault{Equivocation}` becomes unreachable and is removed
> from the fault alphabet**."

All three of the gate's sub-findings are closed: the row is gone, the `AbortKind`
with no `cause` is gone, and every "blocked at the protocol layer" sentence is
gone from T55, T56, §8.6, §8.7 and §12. The document then does the harder thing
and states what an equivocation costs *now* (l. 1467–1476) so that "nothing
consumes it" is not read as "nothing happens": the two copies share a stage cell,
the peers that accepted different first copies diverge, and the hand ends through
T50 → T53/T54/T60/T57. Retention is explicitly reassigned to the protocol layer
(l. 1481–1484), which is the loose end the deletion could have left.

One residual, **H5**: §5.2.4's own G2 paragraph still prescribes the superseded
fix — "T55 loses its `HandAborted` destination … and becomes T56's shape"
(l. 3023–3024) — one paragraph below a box that says nothing consumes it at all.

## G3 — whether a proof block-lists a key — **RESOLVED**

The sweep's survivor is gone, and the document that had never been opened has been
rewritten around the rule rather than patched at the two call sites.

> `NETWORK_STACK.md` §11.5.1 (l. 2118–2123): "No `block_peer` call, no disconnect,
> no dial refusal, no unseating, no persisted mark and no allow-list or block-list
> entry may be produced by any of: an `EquivocationProof` …, a timeout certificate
> …, a `HAND_ABORT` attribution, a `DISPUTE`, an invalid application signature, a
> `STATE_HASH` divergence, or a count of any of them."
>
> §11.5.3 (l. 2185–2188): "**`block_peer` has exactly one caller: an explicit
> action the user took in the GUI.**"
>
> §6.6 (l. 1142–1152): "**A proven protocol violation produces no transport action
> at all.** … An `EquivocationProof` is evidence for a human and reaches this
> layer not at all: the transport neither constructs one, consumes one, nor
> changes any behaviour on seeing one."

The distinction the last gate said the document was missing is now drawn once, in
§0.2, and §11.5.2 keeps the ten resource-keyed defences with three properties
asserted over all of them — nothing persisted, nothing a mark, nothing consults a
proof. The behaviour field is renamed `user_blocklist` with the single-caller rule
in a code comment (l. 909–912), which is the constraint expressed where an
implementer will actually read it. §0.4's ten-row sweep table records what each
edit replaced. The document now carries the D-010 and D-011 rows its siblings have
(l. 2384–2385).

**One residual, disclosed and not a survivor.** §6.4 (l. 1093) maps a `Reject` —
which an invalid signature triggers — onto GossipSub's topic P₄
`invalid_message_deliveries_weight`. §6.7 (l. 1156–1161) establishes from source
that `PeerScoreParams::default()` has `topics: HashMap::new()`, so P₄ contributes
**zero** until a `TopicScoreParams` is installed, and OQ-5 carries the decision to
Phase 8. It is inert in v1 and it is disclosed; it should be named in §11.5.1's
list before OQ-5 is answered, because that is the moment it stops being inert.

## G4 — whether T11 unseats — **RESOLVED**

Option (a), the smaller one, which is what the gate recommended and what P4 had
already ruled two rows away.

> `STATE_MACHINE.md` T11 (l. 1063): "| `AwaitingSeatRngReveal` | `RngReveal` |
> commitment mismatch | `AwaitingSeatRngReveal` | **`Rejection`**, state
> bit-identical (I21) … **No unseating** (G4, D-010 point 3) |"

The disposition is now uniform across the three beacon faults, as the gate
required: `RngCommitmentMismatch` joins `NoRngCommit` and `NoRngReveal` in
stalling the beacon to T4. §12.1 row 3 carries the consequence for termination
(l. 3047) and §12's "no peer-removal transition" claim (l. 1496) is true of the
document for the first time.

## G5 — `StateHash::round`, and the stale objections — **RESOLVED**

Both halves.

**(a)** The gate's own sane default is adopted and stated as this document's own
derivation rather than as a request for a wire field:
`STATE_MACHINE.md` §11's divergence rows are scoped on `round := sequence −
s_ckpt` (l. 1397–1398: "§4.1 gives the one thing that is this document's — the
derivation `round := sequence − s_ckpt` — and the rows are scoped on it"). No
field is asked of §4.9 and none is added. `PROTOCOL.md` still distinguishes rounds
by `sequence` alone, and the two documents now agree.

**(b)** §13's objection list reads "**Two objections addressed to `PROTOCOL.md`.**
The three that stood here are closed" (l. 3384–3387), naming §3.2/§4.10, §6.3
step 3/§4.9 and §8.2 as the three. The two that remain — `cause` for an invalid
key proof, and one wording question — each carry a named default the engine builds
against.

## R-1 — where `hand_deadline_ms` starts — **RESOLVED**

The owner adopted the engine's reading, and then found it was load-bearing for
something else.

> `PROTOCOL.md` §8.2 (l. 3705–3710): "**The whole-hand limit `hand_deadline_ms`
> runs on the same relative basis from `TERMINAL(k-1)` — normative, and this is
> R-1's disposition.** … It is **not** started by `HAND_INIT`."

Two reasons are given and the second is new: §4.10's acceptance gate has a
receiver *buffer* a terminal abort until its own deadline expires, and buffering
is bounded only if every peer's timer started from the same agreed event.
§14 note 19 (l. 4999–5006) records the upgrade in status: "The gate called this
'one sentence in one document' and it would have been, before note 16." That is
the right conduct — the fix is recorded as having become a precondition rather
than presented as always having been one.

## R-2 — the `OQ-*` letter collision — **PARTIAL**

Three of the four documents reconciled; the fourth did not, and it is now the odd
one out in a way that is sharper than the uniform collision it replaced.

`DECISIONS.md` is authority. `PROTOCOL.md` §12 adopted its letters and deleted its
own parallel scheme, keeping a one-time mapping table (l. 4462–4487):
`OQ-F`→`OQ-A`, `OQ-B`→`OQ-D`, old `OQ-D`→`Q-07`, old `OQ-C`→`Q-08`.
`STATE_MACHINE.md` §11 (l. 2918–2924) did the same and annotates its two former
labels in place rather than reusing them.

**`THREAT_MODEL.md` §9.2 did not**, and still runs its own `OQ-A` / `OQ-B` /
`OQ-D` / `OQ-E` series (l. 2099–2103), in which `OQ-A` is the heads-up
unfinishable-hand question and `OQ-D` is the `cause = 4` disposition. Its refusal
paragraph (l. 2143–2155) is unchanged and still reads "recorded rather than fixed
unilaterally … flagged here rather than patched in one place, which would deepen
the collision rather than close it". The other two documents have since patched,
so that reasoning has expired, and two of the paragraph's factual claims are now
false:

> l. 2119–2121: "The wording is left byte-identical with `PROTOCOL.md` §12 and
> `STATE_MACHINE.md` §11 as the lettered questions require" — **false**; both of
> those documents deleted those rows.
>
> l. 2157–2158: "**OQ-C** is recorded in `PROTOCOL.md` §12 only" — **false**;
> `PROTOCOL.md` renamed it `Q-08`.

PARTIAL rather than UNRESOLVED because the collision is now confined to one
document and the mapping to follow is written down twice. It is one editor's work
and it is carried as **H6**.

## R-3 — `CONTRIBUTING.md`'s self-inserted authority order — **UNRESOLVED**

Unchanged for a third pass, and now three decisions stale rather than two.

> `CONTRIBUTING.md` l. 8–15: "**Authority order**, and it is not negotiable by
> anything in this file: 1. `docs/SPEC_CS.md` … 2. `docs/DECISIONS.md` — **D-001
> … D-008.** … 4. `docs/DEPENDENCIES.md` for the §28 register, **this file** for
> §31."

The document contains **zero occurrences of `D-009`, `D-010` or `D-011`**. §5's
review checklist (l. 236–241) still instructs a reviewer to check the certificate
scoping under D-008 and says nothing about D-010's deletion of the consequences
that scoping protected, so a reviewer following this file checks the shape of a
mechanism the corpus has since made inert. That is a process defect, not a
protocol one, and it does not block Phase 2 — but it is the file that tells the
next editor which document wins, and it is wrong about the range.

## R-4 — `PROTOCOL.md` §5.3's first bullet — **RESOLVED**

> §5.3 (l. 3072–3078): "**Why the three structures are separate rather than one
> map (R-4).** The sentence that stood here said the `event_class` axis 'is what
> lets a seat hold both its own contribution at stage `s` and a `TIMEOUT_VOTE`
> about stage `s`', immediately after an index that did not contain
> `event_class`. That was true in effect and wrong in wording … The separation is
> the axis."

The section is rewritten to derive its indices from §5.2.1 rather than to state a
key of its own ("this section defines no key of its own", l. 3048). See **H3** for
where the derivation is lossy.

---

# Part 2 — The slot-key sweep

## 2.1 Is the key reproduced anywhere else?

**No.** The five documents were swept for every field ordering that could
constitute a partial reproduction. Results:

| Site | Text | Verdict |
|---|---|---|
| `PROTOCOL.md` §5.2.1 l. 2671–2685 | the 8-tuple | **the definition** |
| `PROTOCOL.md` §4.0 step 10a l. 1130 | "look up **`slot(E)` exactly as §5.2.1 defines it** — this step reproduces no part of that tuple and reads it whole" | pointer |
| `PROTOCOL.md` §5.1 l. 2646 | "the slot key is §5.2.1's — read whole, reproduced nowhere" | pointer |
| `PROTOCOL.md` §4.8 l. 2023, §4.11 l. 2581 | "the vote's anti-replay slot key includes its subject"; "derived per type from the slot key of §5.2.1" | pointer |
| `STATE_MACHINE.md` l. 20, 713, 960, 1255, 1429, 2262, 2524, 3073 | eight references, each naming §5.2 or §5.2.1 | pointer ×8 |
| `CRYPTOGRAPHY.md` §6.4, §7.1, l. 2251, 2403 | "**Owns and this document deliberately does not restate:** the equivocation predicate and its anti-replay slot key" | pointer |
| `NETWORK_STACK.md` §1.3.2, l. 355–357, 2383, 2385 | "**The slot key is that section's literal tuple and this document reproduces none of it**"; §1.3.2's former restatement recorded as deleted | pointer |
| `THREAT_MODEL.md` §5.2 row 16, X31, X32, §5.5, R4/R7/R10 | "The slot key is not reproduced here; it is one literal tuple in `PROTOCOL.md` §5.2 and this row points at it"; the five- and six-field versions this file used to print are recorded as deleted | pointer |
| `DECISIONS.md` l. 809 | `(stage, seat, event_class)` | **historical**, inside D-009's narration of defect M2; a decision record quoting the index that failed. Correct conduct |
| `research/*.md` | six older tuples | **evidence, not authority**; each is a report of the key as it stood on that date |

Two tuples exist that are *not* the slot key and must not be mistaken for partial
reproductions of it: the **ordering-buffer** key `(table_id, hand_id, sequence,
previous_event_hash)` at `PROTOCOL.md` l. 2891 and `STATE_MACHINE.md` l. 452, and
`GENESIS`/`roster_hash` at §3.1. The first is a genuine restatement across the
owner boundary and is **H8**; it is not a slot-key reproduction.

**Verdict: the key has one site. D-011 rule 2 landed.**

## 2.2 The 39 message types, checked independently

The question asked of each type: *can one honest peer, doing what the corpus
requires or permits of it, produce two `SignedEvent`s whose §5.2.1 keys are equal
and whose `event_hash`es differ?* The corpus's own answer is `PROTOCOL.md` §4.11's
39-row table (l. 2586–2624); the table below was built first and compared after.
The two agree on all 39.

| # | Type(s) | The two-emission candidates an honest peer can produce | Verdict |
|---:|---|---|---|
| 1–13 | `HELLO`, `CAPABILITIES`, `JOIN_REQUEST`, `JOIN_ACCEPT`, `JOIN_REJECT`, `PLAYER_LIST`, six `LOBBY_*`, `DISPUTE` | unbounded, by design | **Outside the predicate.** `chain_scope = 0` occupies no slot; §5.2.1's box names the per-type anti-replay for each. The `DISPUTE` case is the one that matters — §6.3 step 2 and step 4(a) both oblige the same peer to emit one about one hand — and it is why `DISPUTE` is unchained |
| 14 | `TABLE_READY` | one per seat | **Clean** |
| 15 | `RNG_COMMIT` | one per seat | **Clean** |
| 16 | `RNG_REVEAL` | commit then reveal | **Clean** — consecutive stages, `sequence` differs |
| 17 | `HAND_INIT` | one derived copy; plus a `HAND_ABORT` at `HAND_INIT`'s own `sequence` when it stalls (R-1's newly covered case) | **Clean by `event_type`.** This pair is *new* since R-1 made the stalled `HAND_INIT` reachable by the abort, and it is the second collision `event_type` closes |
| 18 | `DECK_INIT` | one per dealt-in seat; plus the abort at the same `sequence` | **Clean by `event_type`** — this is G1's own example |
| 19 | `SHUFFLE_STEP` | one, single-writer | **Clean** |
| 20 | `SHUFFLE_PROOF` | step then proof | **Clean** — two consecutive single-writer stages |
| 21 | `DECK_COMMIT` | one per dealt-in seat; plus the abort | **Clean by `event_type`** |
| 22 | `DEAL_PRIVATE` | one per dealt-in seat, **exactly** `2(m−1)` entries in one body | **Clean.** The "exactly" forbids incremental publication, which would be two bodies at one stage |
| 23 | `BOARD_REVEAL` | one per seat per street; plus a `cause = 3` abort at the same stage | **Clean by `event_type`.** Under the seven-tuple this row failed too — the last gate listed `cause = 3` at a collective reveal stage as a colliding path |
| 24 | `SHOWDOWN_REVEAL` | one per showdown seat | **Clean** |
| 25 | `SHOWDOWN_MUCK` | mutually exclusive with 24 by §4.6 | **Clean, with a changed reason.** Since `event_type` entered the key the pair is two slots, so a seat emitting both is a stage violation under §4.0 step 12, not an equivocation. §4.6's exclusivity is what rejects the second, and it is normative |
| 26–30 | the five `ACTION_*` | one per turn | **Clean, same changed reason.** Two *different* action types at one `sequence` are a stage violation; two `ACTION_RAISE` bodies with different amounts are still one slot and still an equivocation |
| 31 | `TIMEOUT_VOTE` | one per subject per stage; **two simultaneous subjects are normal** (§8.4) | **Clean since M2**, by `subject_seat` in the key |
| 32 | `TIMEOUT_CERT` | one per `subject_digest` per stage | **Clean since M2**, by `subject_digest`. `n(1) votes` is the one variable field and is pinned by §4.8's receiver check |
| 33 | `STATE_HASH` | checkpoint at `s_ckpt`, **plus one per reconciliation round** — the corpus's only required re-emission with changed content | **Clean since P1**, and by §4.9's rule alone: round `r` is stage `s_ckpt + r`. Not by the stage rule. Deleting §4.9's box reopens P1 |
| 34 | `STATE_ACK` | same | **Clean since P1**, same rule |
| 35 | `HAND_COMPLETE` | one derived copy at a fresh `sequence`; **plus a `HAND_ABORT` at that same `sequence`** when the stage never completes and T57 fires from `Settling` | **Clean by `event_type`.** This is a third path the seven-tuple broke, and it is reachable only because §12.1 row 15 gave `Settling` an exit — a fix and its collision in the same revision |
| 36 | `HAND_ABORT` | **one per emitter per hand**, at the stalled stage's own `sequence` | **Clean since G1, and only by `event_type`.** §4.10 l. 2296–2298 makes "one per emitter per hand" normative and closes the second-abort route: "It cannot vary the `sequence` to produce a second, because the first ends the hand at it" |
| 37–39 | `PLAYER_SIT_OUT`, `PLAYER_SIT_IN`, `PLAYER_LEAVE` | one per seat at a hand boundary, single-writer, only there | **Clean since M4** |

**Three observations the corpus's own table does not make.**

1. **`event_type` closes four collisions, not one.** Rows 17, 18/21/23, 35 and 36
   all collide under the seven-tuple. The last gate found one of them (36) and
   listed 23 in its by-path breakdown; 17 and 35 became reachable *in this
   revision*, by R-1's fix and by §12.1's `Settling` fix respectively. Had the key
   been extended per-message rather than by definition, both new paths would have
   shipped uncovered. **This is the strongest evidence in the corpus that D-011
   rule 2 was the right form of fix** — the definition covered two paths that did
   not exist when it was written.
2. **The predicate got weaker in one direction and the corpus says so.** Rows 25
   and 26–30 lost the *label* "equivocation". §5.2.1 l. 2729–2739 argues the trade
   and D-010 is what makes it cheap: a proof ends no hand and moves no chip, so
   the difference between "an `EquivocationProof` names this key" and "two signed
   events by this key contradict each other" is vocabulary. That reasoning is
   sound **only while D-010 stands**, and D-010's own "when this is revisited"
   clause does not name it. The revisiting decision must re-open this trade.
3. **The check table now derives per type rather than per stage-kind group**
   (§4.11 l. 2581–2585), which is the correction the last gate demanded, and it
   states why: "The grouped form that stood here certified two defects with one
   sentence … because the group is not what the predicate indexes." The two
   verdicts that changed direction are recorded as changes (l. 2626–2631) rather
   than restated as if they had always read that way.

**Verdict on the sweep: clean on all 39. The property holds, and it holds by a
definition rather than by 39 separate arguments.** The one qualification is **H3**
— §5.3's storage is a lossy projection of the key it claims to derive from.

---

# Part 3 — The termination sweep

For every phase, the reachable exit under every adversarial behaviour including a
peer that simply stops. Silence is the whole proof obligation: any event that
arrives is either accepted, in which case the phase advances, or rejected under
I21, in which case the state is bit-identical and the silence case still applies.

| # | Phase | Exit under total silence | Fires on | Independently checked |
|---:|---|---|---|---|
| 1 | `Seating` | **T4** `FormationAbandoned`, local timer, nobody named | `join_deadline_ms` from first `JOIN_ACCEPT` | ✔ `ledger_in == 0`, no chips exist |
| 2 | `AwaitingSeatRngCommit` | **T4**, guard `hand_id == 0 ∧ ledger_in == 0` | `join_deadline_ms` from `TABLE_READY` | ✔ |
| 3 | `AwaitingSeatRngReveal` | **T4** | as above | ✔ **and this row changed**: T11's unseating (G4) is now a `Rejection`, so a commitment mismatch stalls the beacon here instead of removing the seat |
| 4 | `AwaitingKeySetup` | **T57** | `hand_deadline_ms` from `TERMINAL(k−1)` | ✔ **including a `HAND_INIT` that never completes** — R-1's fix is what puts this row in scope from hand 2 onward |
| 5–14 | `AwaitingShuffle` … `AwaitingShowdownReveal` | **T57** | as above | ✔ |
| 15 | `Settling` | **T57** | as above | ✔ **new in this revision.** T45 no longer fires on the local derivation; the award is applied when the collective `HAND_COMPLETE` stage completes, and T57 covers the case where it never does. No award has been applied, so restoration is coherent |
| 16 | `HandComplete` | **T47**, derived, no external input | immediately | ✔ |
| 17 | `HandAborted` | **T46**, derived `AbortSettle` | immediately | ✔ |
| 18 | `Paused` | **idles.** No hand live, no chips committed, no deadline armed, `Σ committed_hand == 0`. Left by T59 | — | ✔ idle, not frozen. A `Paused` table every seat has left stays `Paused`; nothing is owed |
| 19 | `TableClosed` | absorbing | — | ✔ |
| 20 | `Diverged`, hand live | **T57** — the deadline is not disarmed on entry (T50) and phase 20 is in T57's scope | `hand_deadline_ms`, still running | ✔ |
| 20 | `Diverged`, no hand started | **T4**, guard `hand_id == 0 ∧ ledger_in == 0` | `join_deadline_ms` | ✔ reachable, because checkpoint 1 sits after `TABLE_READY` and before any `HAND_INIT` |

**Every phase has a reachable exit. Phases 4–14 and 20 — the whole regime in which
chips are committed, and the ones the last gate found frozen — are closed.**

The load-bearing checks, run against the artefact rather than against the claim:

* **T57's artefact can now be accepted by its own emitter.** That was G1's liveness
  half. `HAND_ABORT` at the stalled stage's `sequence` no longer collides with the
  emitter's own contribution there, so §4.0 step 10a passes. Verified against
  §5.2.1's tuple directly, not against §4.11's row.
* **T57 does not wait for the silent peer.** The stage has no required emitter set
  (§3.2), so the emitter's own copy closes it. Heads-up, where `|V| = 1` and every
  certificate is inert, this is the only terminus and it is reachable with one
  live peer.
* **The acceptance gate does not put the deadlock back.** §4.10's gate says
  **buffer, never reject** on all three `cause = 1` and `cause = 4` paths
  (l. 2313–2318). A peer whose clock runs behind holds the artefact instead of
  refusing it; §8.2's anchor at `TERMINAL(k−1)` is what bounds the hold. This is
  the coupling between R-1 and P3 that the corpus found and the last gate did not.
  The one seam is **H4**: the two `cause = 1, attributed = [], cert_hash = None`
  gate rows are not distinguishable from the wire body.
* **No exit reads a certificate, a voter set or `|V|`.** T4, T45, T46, T47 and T57
  are the whole column. That is what makes the table true at two seats and at
  nine, and it is what D-007 and D-008 require.
* **The walk terminates.** Rows 1–3 and the second row of 20 reach the absorbing
  `TableClosed`. Rows 4–15 and the first row of 20 reach `HandAborted` → T46 →
  `HandComplete` → T47 → row 4, `Paused` or `TableClosed`. `hand_id` strictly
  increases around that cycle and §9.3's end conditions are re-evaluated each
  time, so the cycle makes progress on the one counter that matters.

**Two limits, both stated by the corpus rather than found here.** The table is a
termination argument, not a liveness bound: `hand_deadline_ms` per hand with no
limit on repetition is a passing row and an unplayable table. And **H1 is a
termination defect at one remove** — every row above ends a *hand*, and H1 is
about what the next hand starts from.

---

# Part 4 — The eviction sweep

Every `block_peer`, unseating, allow/block-list entry and proof-driven penalty in
all five documents. Patterns swept: `block_peer`, `allow_block_list`,
`BlockedPeers`, `blocklist`, `block-list`, `block list`, `unseat`, `evict`,
`banned`, `penalis`, `penaliz`, `penalty`, plus every consumer of `attributed`,
`AbortRecord` and `EquivocationProof`.

## Survivors: none

| Document | `block_peer` | `unseat` | Disposition |
|---|---:|---:|---|
| `PROTOCOL.md` | 0 | 7 | all seven are prohibitions or deletion notices; §4.0 l. 1163–1173 carries the corpus-binding negative box |
| `STATE_MACHINE.md` | 1 | 19 | the single `block_peer` is in D-011's own summary row stating the prohibition; T11 and T55/T56 are deleted (G4, G2) |
| `CRYPTOGRAPHY.md` | 0 | 5 | all five are prohibitions |
| `NETWORK_STACK.md` | 10 | 6 | §11.5.1 forbidden, §11.5.2 kept-and-resource-keyed, §11.5.3 user-only; §0.4's sweep table records the ten sites it replaced |
| `THREAT_MODEL.md` | 4 | 17 | all are classifications or historical narration of what D-005 used to do |

**Legitimate and correctly preserved**, checked one by one against the standard
that a defence must key on a fact about *our own* process and never on a belief
about the peer: connection limits, memory limit, discovery budget, size and
canonicality, the lobby token bucket and its stop-dial→disconnect ladder,
GossipSub scoring at defaults, D-002 relay admission, relay capacity, roster
membership on a table stream, the Mainline deny-all `RequestFilter`, and §9.5's
volume-keyed malformed-frame ladder. `NETWORK_STACK.md` §11.5.2 asserts three
properties over all of them — nothing persisted, nothing a mark, nothing consults
a proof — and each holds on inspection.

**The user's own choice is preserved and is the only writer.** `user_blocklist`,
one caller, in the GUI, local, never gossiped, shown in full, every entry
removable. That is D-010's own mitigation and D-011 rule 3's closing sentence.

## The one state change attribution still causes

`STATE_MACHINE.md` T46 sets `status := Absent` for every seat in
`abort.attributed`. §8.7 (l. 2493–2502) argues it is not a penalty and the
argument is checkable rather than rhetorical — the seat keeps its stack, keeps its
position, keeps its `player`, pays blinds as dead money, and returns to `Active`
at any hand boundary on its own `PlayerSitsIn`. `PROTOCOL.md` §4.10 (l. 2468–2477)
cedes the rule to `STATE_MACHINE.md` under D-011 rule 1 and keeps only the wire
fact: "`n(1) attributed` is not read by any rule in this document, and no receiver
may derive a seat's state from it".

**Accepted as not an eviction. But the two documents are one sentence apart from
each other, and the marking is the vehicle for H1** — see Part 7.

**Verdict: D-010 point 3 has landed at every layer. Zero survivors.**

---

# Part 5 — The restatement sweep

D-011 rule 1: a document **references** another's definition by section number and
does not restate it. Where a restatement exists, it is deleted and replaced with a
pointer. Where two documents disagree, the owner wins.

## What was deleted, and it is a great deal

`PROTOCOL.md` §14 note 20 (l. 5012–5024) lists seven copies deleted from the
owner of the wire alone: §11's restatement of `THREAT_MODEL.md`'s classification,
§2.8's BLAKE3 rationale (`CRYPTOGRAPHY.md`'s), §1.4's libp2p behaviour column,
§1.5's connectivity floor and §9.5's `connection_limits` table (all
`NETWORK_STACK.md`'s), and §4.10's absent-seat paragraph, §8.4's join-timeout
paragraph and §6.3/§6.4's two `Diverged` sentences (all `STATE_MACHINE.md`'s).
`NETWORK_STACK.md` §0.4 lists ten. `STATE_MACHINE.md` §13 items 20–26 list its
own. `THREAT_MODEL.md` deleted the three slot-key tuples it used to print and
records why: "printing a superseded key beside a live one is how a reader
certifies the next X31 as clean".

The objection is recorded rather than suppressed (§14 note 20's last paragraph):
some copies were load-bearing for a reader, and §11 in particular was where a
reader of `PROTOCOL.md` alone learned the rage-quit escape is open. That reader
now has to open `THREAT_MODEL.md`. Accepted, with the reason measured.

## What survives — five sites, and two documents were never swept

| # | Site | What is restated | Owner | Severity |
|---|---|---|---|---|
| **1** | `CRYPTOGRAPHY.md` §6.4 (l. 932–974) | the `ctx` construction, printed **twice** — its own copy, then `PROTOCOL.md` §4.5 "quoted verbatim" beside it | `PROTOCOL.md` §4.5 | see H7 |
| **2** | `CRYPTOGRAPHY.md` §2.4 | the deck-index → role map | `PROTOCOL.md` §4.5, which says "This section owns the map; no other document may restate it independently" | see H7 |
| **3** | `CRYPTOGRAPHY.md` §2.8, §13 | the domain-string register and the `DOMAIN_EVENT` prefix bytes | `PROTOCOL.md` §2.8, §13 | see H7 |
| **4** | `STATE_MACHINE.md` §8 (l. 2005–2008) | `commitment_i` and `seed`, the RNG-beacon constructions | `PROTOCOL.md` §4.4, §2.8 | see H8 |
| **5** | `STATE_MACHINE.md` §3.2 (l. 452) | the ordering-buffer key `(table_id, hand_id, sequence, previous_event_hash)`, with no section pointer | `PROTOCOL.md` §4.0, §5.3 | see H8 |

Sites 1–3 are all in **`CRYPTOGRAPHY.md`, which has zero occurrences of `D-011`**,
and they are not accidental: its own ownership table (l. 2251) declares the
discipline it is operating under, and it is the *pre*-D-011 one —

> "| `PROTOCOL.md` | **owns, and this document reproduces:** the `ctx`
> construction …; the dealing map …; the domain-string register …; the
> `DOMAIN_EVENT` signature prefix bytes …"

"owns, and this document reproduces" is exactly the arrangement D-011 rule 1 was
adopted to end. The same table gets the *other* half right in the next sentence —
"**Owns and this document deliberately does not restate:** the equivocation
predicate and its anti-replay slot key" — which is what the corpus looks like after
a D-011 sweep. One document, two disciplines, because only the half that D-009
had already touched was ever revisited.

**The copies have not drifted yet**, and the document does the best a copy-keeper
can: §6.4 (l. 955–962, 977–981) records a mechanical byte-comparison of both `ctx`
blocks, dated, with the result, and notes the comparison was done by extraction
rather than by reading "because reading is how the last drift got missed". That is
good conduct and it is not what D-011 asks for. D-011's whole finding is that a
copy verified today is the copy that drifts tomorrow, and five passes are the
evidence.

Site 4 is sharper still, because `STATE_MACHINE.md` states the argument against
itself in place (l. 1997–1999): "the constructions are printed here because this
document previously printed a **third, incompatible version of both and that is
what made them diverge** (C-2)." The remedy chosen for a defect caused by a copy
was a corrected copy.

**Verdict: D-011 rule 1 landed in three documents of five and did not reach
`CRYPTOGRAPHY.md` or `CONTRIBUTING.md`.** Nothing here is currently wrong on the
merits; every listed site is the drift of the next pass, which is precisely what
this sweep was commissioned to find.

---

# Part 6 — Chip conservation

Every path that moves a chip, both modes. **Conservation holds on every path.**

**Abort — one branch, all causes, all `AbortKind`s.**
`for s in all seats: stack[s] += committed_hand[s]` (§8.6 l. 2445). `Σ` before
`== Σ` after by inspection: what leaves `committed_hand` equals what enters
`stack`. It does not branch on `AbortKind`, on `attributed`, on `|V|`, on the seat
count or on who published what; **T46 applies it unconditionally**, on all eleven
paths I27 now names (T15, T16, T21, T22, T27, T41, T44, T48, T54, T57, T60). The
new `AbortKind::UnobtainableEvents` (T60) takes the same branch, and the new
`AbortKind` → `cause` mapping table (l. 2402–2413) closes the C-6 class that G2
found: eleven kinds, eleven `cause` values, one of them a named default (see
Part 7's note on `BadKeyProof`).

**Enforced at the receiver, not at the emitter.** §4.10's validation list requires
`n(4) deltas` all zeroes **and** `n(5) final_stacks` equal to *this receiver's
own* `stack_at_hand_start` vector. A copy that moves a chip is rejected under
§4.0 whoever signed it. That is the strongest available placement and it is
unchanged from the last pass.

**Settlement — `build_pots`.** For levels `l₀ < l₁ < …` over positive
commitments, `Σ (lᵢ − lᵢ₋₁) × |contributors_i| = Σ_s committed[s]` exactly; a
single-contributor band is a refund, not a pot; a level with two or more
contributors and an empty `eligible` set is refunded to its contributors (P7's
consequence). **I6** holds. `dealt_in` gates `eligible` (P7), so an absent seat
cannot win the blind it posts, and I7/I8 assert it.

**One change worth checking, and it holds.** T45 no longer applies the award on
the local derivation; the settlement is computed and this peer's copy published on
entry to `Settling` (T30, T42) and **applied at T45 when the collective
`HAND_COMPLETE` stage completes**. Conservation is unaffected — the arithmetic is
the same — and the change closes a chip defect the last gate did not find: under
the old row a peer awarded pots and then, if `HAND_COMPLETE` never completed, an
abort accepted under §4.10's precedence rule would have **restored stacks the
engine had already paid out**. `STATE_MACHINE.md` §13 item 26 records it, and it
is one of the two places in this corpus where an editor found a chip defect the
gate had missed.

**Odd chips.** `share = pot.size / |W|`, `remainder = pot.size % |W|`, one chip
each clockwise from the first seat left of `button_pos`. Exact, a pure function of
public state, and now reached only from settlement — §8.6 records that the
forfeiture remainder loop that also used it is gone and that "§7.6's A8 rule is
now used only at settlement", so the deletion orphaned nothing.

**Blinds off an absent seat.** `Absent` and `SittingOut` seats post antes and
blinds at §5.3 steps 6–7 while step 4 leaves `dealt_in = false`; those chips enter
`committed_hand` and are inside every sum above. The drain terminates: step 2
busts a zero stack, §9.3 condition 1 ends the tournament.

**The ledger identity.** **I1** is `Σ stack + Σ committed_hand == ledger_in −
ledger_out`, both counters moving only at hand-init step 0 through
`HAND_INIT`'s `n(11) ledger_delta`, never inside a hand, never at T58 (**I28**).
Tournament mode: the right-hand side is constant after hand 1. Cash mode: it is
non-increasing, since `ledger_in` is fixed at the first hand init and `ledger_out`
grows as seats leave (P8, Q6). **Conservation holds in both modes.**

**Nothing found.** This part is the same length as last pass's and for the same
reason: D-010 deleted the arithmetic that needed checking, and nothing in this
revision put any of it back.

---

# Part 7 — New defects

Ordered by severity. Continuing the letter series: **H**.

## H1 — `status := Absent` is derived from the accepted abort copy's `attributed`, and the witness-independent terminal stage is exactly what lets two peers accept copies with different `attributed`; the next hand's `dealt_in`, `bb_seat` and `roster_hash(k+1)` therefore fork

**Severity: highest. Consensus-critical, and it is the D-011 pattern's sixth
iteration** — *the defect is never in the rule; it is in the path the rule newly
makes load-bearing.*

**What P3's fix bought, and what it did not.** §3.1's `ABORT_TERMINAL(k)` is a
function of `GENESIS(k)` alone, and the reasoning is exactly right:

> `PROTOCOL.md` §3.1 (l. 748–752): "An abort is, by definition, the outcome in
> which the peers **could not agree about the middle of the hand** … Deriving
> `TERMINAL(k)` from any of those quantities derives it from the one thing that is
> in dispute."

That closes the fork **in `TERMINAL(k)`**. It does not close the fork in every
*other* value derived from the abort, and there is one:

> `STATE_MACHINE.md` T46 (l. 1188): "`∀ s ∈ abort.attributed: status := Absent`
> (D-005 — a non-participation marking, not a penalty)"

`attributed` is a field of the accepted abort copy. Under §3.2 the terminal stage
"closes at a receiver on the **first** copy that verifies and passes §4.10's
acceptance gate", and §4.10 states no precedence rule between two `HAND_ABORT`
copies — only between `HAND_ABORT` and `HAND_COMPLETE`. So which copy a peer
accepts is a per-receiver fact, and `attributed` differs across the paths:

| Producing transition | `cause` | `attributed` |
|---|---|---|
| T16 / T22 / T27 / T41 / T44 — certified subject | `1` | `[subject]` |
| T21 / T15 / T48 — invalid proof | `2` / `2` / `3` | the seat |
| **T57 — hand deadline** | `1` | **`[]`** |
| T60 — §6.3 case (b) | `1` | `[]` |
| T54 — §6.3 case (c) | `4` | `[]` |

**The reachable interleaving, heads-up-independent, no partition required beyond
one dropped message.** Three seats, `C` silent at a collective crypto stage.

1. `A` and `B` are the voter set, both vote, a `kind = 2` `TIMEOUT_CERT` with
   `|V| = 2` completes. `A` takes T16 and emits
   `HAND_ABORT{cause = 1, attributed = [C], cert_hash = Some(h)}`.
2. `B`'s copy of the certificate is lost. `B`'s `hand_deadline_ms` expires. `B`
   takes T57 and emits `HAND_ABORT{cause = 1, attributed = [], cert_hash = None}`,
   which its own gate accepts at once — its own deadline has expired.
3. `A`'s copy reaches `B`. `B` has already closed the terminal stage.
4. `B`'s copy reaches `A`. §4.10's gate for the hand-deadline path says *buffer*
   until `A`'s own deadline expires; `A` has already closed the stage.

Both peers now hold `TERMINAL(k) = ABORT_TERMINAL(k)`, identical — P3's fix works.
Both hold identical stacks — D-010 works. **`A` has `status[C] = Absent` and `B`
does not.**

**What that breaks, in two independent places.**

**(a) `HAND_INIT` for hand `k+1` cannot complete.** It is a **collective** stage
and `PROTOCOL.md` §4.4 (l. 1590–1594) says "Every field is a pure function of
`TERMINAL(k-1)` and the table parameters, so every receiver recomputes all of them
and rejects a copy in which any field differs." Two of its fields are not:

* `n(8) dealt_in` — `STATE_MACHINE.md` §5.3 step 4 (l. 1583) gives `Absent` seats
  `dealt_in = false`, so `A` deals 2 and `B` deals 3;
* `n(3) bb_seat` — §4.4's own limit is "must be an occupied, **non-absent** seat".

Each peer rejects the other's `HAND_INIT` copy. The stage never completes, hand
`k+1` stalls to `hand_deadline_ms`, T57 aborts it — with `attributed = []`, which
changes nothing — and hand `k+2` starts from the same divergence. **The table
never completes another hand.** This is phase 4 with no exit again, arrived at
from a different direction, and §12.1's row 4 does not see it because the row is
about *this* hand ending, and this hand does end.

**(b) `GENESIS(k+1)` may fork outright**, depending on **H2**. `roster_hash(k)`
contains `u8(seat_flags[s])`. If `seat_flags` encodes seat status — the only
reading the name admits — then `A` and `B` compute different `roster_hash(k+1)`,
hence different `GENESIS(k+1)`, hence a silent byte-level chain fork rather than a
loud stage rejection. Whether (b) fires cannot be determined from the corpus,
because `seat_flags` is undefined. That is H2, and this is why H2 is blocking
rather than cosmetic.

**Why nobody saw it.** Three reasons, and each is instructive:

1. `PROTOCOL.md` §4.10 (l. 2475–2477) says "**`n(1) attributed` is not read by any
   rule in this document**, and no receiver may derive a seat's state from it".
   The first clause is true. The second is a corpus-wide prohibition, and T46
   violates it — but the sentence is scoped by its opening words to *this
   document*, so a reader of either document alone sees no conflict. This is
   D-011 rule 1 applied to the letter and missed in substance: the copy was
   deleted and the *disagreement it was hiding* went with it.
2. `STATE_MACHINE.md` Q7 (l. 2909) asks the adjacent question — "What marks a seat
   `Absent` when the abort names nobody?" — and answers it correctly, including
   the exactly-right warning that an implementer "must not derive `Absent` from
   the abort's `owed` list: that list is the quantity peers disagree about, and
   deriving canonical state from it forks `TERMINAL(k)`". **The identical argument
   applies to `attributed`** and is not made, because before P3 the terminal stage
   was collective and every peer that completed it had the same body.
   Witness-independence is what turned `attributed` into a disagreed quantity, and
   Q7 was written before it.
3. §4.1 kept `owed` out of the hashed body "because it is exactly the quantity
   peers disagree about". The same test was never applied to `attributed`, which
   is in the body *and* feeds canonical state.

**Correction, ranked.**

1. **Delete the marking.** T46 stops setting `status := Absent`, and
   `PROTOCOL.md` §4.10's prohibition becomes true corpus-wide with no exception.
   This is the smallest edit and it is already the MVP's behaviour on the shipped
   path — §8.7 notes T57's `attributed` is empty by construction, so heads-up
   nothing is marked today. It costs exactly what Q7 already accepts as the
   named default: a stall repeats every hand until a user sits the seat out. **A
   liveness cost the corpus has already accepted, in exchange for closing a
   fork.** D-005's requirement is not damaged: D-005 asks that the game continue
   and the absent seat be blinded off, and `PlayerSitsOut` (T58) still does that
   at the seat's own hand.
2. **Make the marking a function of agreed state.** Q7's own answer: it needs "a
   basis every peer already agrees on, which does not exist yet". It does not
   exist because §3.1 deliberately erased the abort's content from the agreed
   record — so this route requires re-introducing exactly what P3's fix removed.
3. **Add abort-vs-abort precedence** so every peer accepts the same copy. This
   needs a total order over copies from different senders at possibly different
   `sequence` values, in the one situation the corpus defines as "the peers cannot
   agree about the middle of the hand". It is a consensus protocol, which is what
   D-010 was adopted to stop building.

**Option 1.** It is smaller, it is what the MVP already does, and D-011's closing
rule — *if a sixth pass finds a defect of either shape, cut the mechanism out
rather than adding a seventh rule* — points at it directly.

## H2 — `seat_flags` is a component of `roster_hash(k)`, hence of `GENESIS(k)`, hence of every event in the corpus, and it appears exactly once in all five documents and is never defined

**Severity: highest. Blocking, and an implementer cannot pick any default.**

> `PROTOCOL.md` §3.1 (l. 721–725):
> ```
> roster_hash(k) = h("p2p-poker v1 roster",
>                    [ for each seat s in ascending seat index:
>                        u8(s) || app_public_key[s] || u64_be(stack_at_hand_start[s])
>                        || u8(seat_flags[s]) ])
> ```

`grep -n "seat_flags" *.md` over `PROTOCOL.md`, `STATE_MACHINE.md`,
`CRYPTOGRAPHY.md`, `NETWORK_STACK.md`, `THREAT_MODEL.md`, `DECISIONS.md`,
`SPEC_CS.md`, `CONTRIBUTING.md` and `DEPENDENCIES.md` returns **one line: the one
above.** A search for `flags` in any other sense returns only unrelated prose about
crate authors flagging a fork. There is no bit layout, no enumeration, no
statement of which of `Active` / `SittingOut` / `Absent` / `Busted` / `Leaving`
it encodes, and no statement of whether it encodes seat status at all.

**Why no default is available.** `roster_hash(k)` → `GENESIS(k)` → every event of
hand `k` by `previous_event_hash` → `TERMINAL(k)` → `GENESIS(k+1)`. Two
implementations that choose different bit layouts, or different sets of statuses,
produce different `GENESIS(k)` and share no event. They fail **silently and
totally**: not one message verifies, and the failure surfaces as "the other peer
sends garbage", not as "we disagree about seat flags". This is the single field in
the corpus where guessing wrong is least recoverable, and it is the only
undefined one.

It is also the field **H1 turns on**. If `seat_flags` includes `Absent`, H1 is a
chain fork; if it does not, H1 is a stage-completion failure. The corpus cannot
say which.

**Correction.** §3.1 defines the byte: which statuses it encodes, in which bits,
and — the part that matters for H1 — **whether a status a peer derived locally may
enter it at all**. The safe answer, and the one consistent with §3.1's own
reasoning about not deriving agreed values from disputed ones, is that
`seat_flags` carries only what `HAND_INIT`'s `n(11) ledger_delta` and the accepted
`PLAYER_SIT_OUT` / `PLAYER_SIT_IN` / `PLAYER_LEAVE` chain events establish —
never a status set by T46. That choice closes H1(b) as a side effect. **It is
`PROTOCOL.md`'s to make** (D-011 rule 1: the wire, the chain, canonical bytes).

## H3 — §5.3's anti-replay store is a lossy projection of §5.2.1's key, in the same document, and claims to be exact

**Severity: medium.** Same owner, so it is an internal inconsistency rather than
cross-document drift — but it is the shape D-011 exists to catch, and it is in the
section an implementer actually builds from.

> `PROTOCOL.md` §5.3 (l. 3043–3048): "**The stored state is one structure per
> `event_class`, and each is indexed by exactly the components of §5.2.1's key
> that vary within one `(table_id, hand_id, sender)` — nothing more and nothing
> less.**"
>
> l. 3050–3059: "**`event_class == 0`.** One `Vec<Option<event_hash>>` … indexed
> by `(stage, seat, event_type_slot)`, where `event_type_slot` has capacity
> **2** — a seat's own contribution to stage `s`, and the terminal `HAND_ABORT`."

`event_type` is a `u16` with 39 live values; `event_type_slot` has capacity 2.
"Nothing more and nothing less" is false, and the projection changes behaviour on
exactly the paths §5.2.1 spends a page arguing about:

* Two **different** `ACTION_*` types claimed for one turn map to one
  `event_type_slot`. §5.2.1 says they are **two slots** and must be rejected by
  §4.0 step 12's stage rule; §5.3's store makes them a **slot collision**, which
  §4.0 step 10a labels a violation and §5.2.2's predicate would then be applied
  to. The predicate is defined over §5.2.1, so the resulting artefact **verifies
  as nothing at any other peer** — a client following §5.3 emits a proof no
  receiver accepts.
* `SHOWDOWN_REVEAL` against `SHOWDOWN_MUCK`: identical analysis.

Both events are rejected either way, so this is not a liveness or a chip defect,
and the false accusation lands on a peer that did commit a stage violation rather
than on an honest one — which is why it is medium and not high. It is still the
D-009 rule 1 machinery producing an object that does not verify, from a
specification that says it derived its index from the key.

**Correction.** Either state that `event_type_slot` is the two-valued projection
`is_terminal_abort(event_type)` and that it is a **storage optimisation whose
collisions are resolved by §4.0 step 12, not by §5.2.2** — which is true and is
one sentence — or index on the full `event_type` and pay the memory. The first is
correct and smaller; what may not stand is the claim that the projection is the
key.

## H4 — §4.10's two `cause = 1, attributed = [], cert_hash = None` acceptance-gate rows are not distinguishable from the wire body

**Severity: low. A sane default is available and is the permissive one.**

> §4.10's gate (l. 2315–2317):
> | `1`, hand-deadline path (`attributed = []`, `cert_hash = None`) | its **own** `hand_deadline_ms` has expired | buffer |
> | `1`, §6.3 case (b) | it has itself reached case (b), **or** its own `hand_deadline_ms` has expired | buffer |

`AbortKind::HandDeadline` (T57) and `AbortKind::UnobtainableEvents` (T60) both
emit `cause = 1`, `attributed = []`, `cert_hash = None`, and neither carries
`n(3) evidence` — §4.10 requires evidence only for causes 2 and 3. **The two
bodies are byte-identical**, so a receiver cannot tell which row of its own gate
to apply.

The default is obvious and safe: take the union, since row two's trigger is a
strict superset of row one's. A receiver accepts when *either* its own deadline
has expired or it has itself reached case (b). Nothing is lost, because case (b)
is derivable from chained content.

**Correction.** Collapse the two rows into one, or give `UnobtainableEvents` a
distinguishing field. The first is smaller and loses nothing: the wire already
cannot tell them apart, so the distinction was never on the wire.

## H5 — §5.2.4's G2 paragraph prescribes the superseded fix, one paragraph below the box that supersedes it

**Severity: low.**

> `PROTOCOL.md` §5.2.4 box (l. 3005): "**Nothing consumes it.** There is no
> transition, in any document, that takes an `EquivocationProof` as its input
> event."
>
> §5.2.4 prose (l. 3023–3024): "T55 loses its `HandAborted` destination and its
> `AbortRecord` and becomes **T56's shape**".

T56's shape *was* a consuming transition that recorded `Fault{Equivocation}`. The
box forbids it; the paragraph prescribes it. `STATE_MACHINE.md` followed the box —
deleting both rows and removing `Fault{Equivocation}` from the alphabet — which is
the correct reading and the stricter one. The paragraph is the gate's own
recommended correction, carried into the document that then decided to go further,
and not updated.

## H6 — `THREAT_MODEL.md` is the last document on the old `OQ-*` letters, and its refusal paragraph now asserts two things that are false

**Severity: low, and it is R-2's residual.** Detail in Part 1's R-2 entry. The
paragraph's reasoning — patching one document deepens the collision — was right
when written and has expired: two of the three others have patched, and
`THREAT_MODEL.md` is now the outlier rather than a co-holder of a shared scheme.

## H7 — `CRYPTOGRAPHY.md` was not swept for D-011 and still operates the pre-D-011 discipline in its own ownership table

**Severity: medium, and it is the fifth pass's G3 pattern repeating with a
different document.**

Zero occurrences of `D-011` against 35 / 74 / 26 / 20 in its four siblings. Its
ownership table (l. 2251) declares "**owns, and this document reproduces:**" for
four `PROTOCOL.md`-owned constructions, which is the arrangement D-011 rule 1
ended, while getting the D-009-touched half right in the very next sentence
("**Owns and this document deliberately does not restate:** the equivocation
predicate and its anti-replay slot key"). §6.4 prints the `ctx` block twice on one
screen. Nothing has drifted yet, and the document's mechanical byte-comparison
(l. 955–962) is good practice — but D-011's finding is that a verified copy is
tomorrow's drift, and this is now the corpus's largest concentration of
un-swept copies.

**Correction.** One sweep: §6.4's two `ctx` blocks become a pointer to
`PROTOCOL.md` §4.5 plus the sentence about what `ctx` must bind and why; §2.4's
map becomes a pointer to §4.5; §2.8 and §13's registers become pointers. Add the
D-011 row its four siblings carry — the absence of that row is how the same class
was missed in `NETWORK_STACK.md` last pass, and it is how it was missed here.

## H8 — two surviving restatements in `STATE_MACHINE.md`, one of them argued against in place

**Severity: low.**

**(a)** §8 (l. 2005–2008) prints `commitment_i` and `seed`, which `PROTOCOL.md`
§4.4 and §2.8 own, and says why: "the constructions are printed here because this
document previously printed a third, incompatible version of both and that is what
made them diverge (C-2)". A defect caused by a copy was fixed with a corrected
copy. Under D-011 rule 1 the fix is a pointer.

**(b)** §3.2 (l. 452) prints the ordering-buffer key `(table_id, hand_id,
sequence, previous_event_hash)` with no section pointer. It is not the slot key
and must not be confused with it — but it is `PROTOCOL.md`'s (§4.0, §5.3), it is
a four-field tuple sitting one column from a description of the replay filter, and
a reader who mistakes it for the anti-replay index makes exactly the M2 error.
A pointer and a one-line "this is not §5.2.1's slot key" would close both risks.

## Two things checked and found not to be defects, recorded so the next pass does not re-file them

* **`AbortKind::BadKeyProof` has no exact `cause`.** §4.10's four values do not
  cover a `DECK_INIT` whose key-share proof fails. `STATE_MACHINE.md` §8.6
  (l. 2417–2424) names the gap, adopts `cause = 2` with the offending event in
  `n(3) evidence` as a **named default** on §4.10's own reasoning that `2` and `3`
  are "the two causes a single chained event proves on its own", and refers the
  widening to `PROTOCOL.md`. That is a flagged interim, not a guess. The gate's
  acceptance table should gain the case when §4.10 next moves.
* **§12.1's `Paused` row idles rather than exits.** No hand is live, no chips are
  committed, no deadline is armed, `Σ committed_hand == 0`. Nothing is owed to
  anybody and a client closes the window. Idle is not frozen, and the row says so.

## One bookkeeping error

`STATE_MACHINE.md` §5.1 (l. 837) states "20 phases, **56** numbered transitions,
29 invariants" and §9.5 (l. 2745) states "All 20 phases and **57** transitions
(§5.1)". Counted directly: 56 `| T` rows, highest number T60, four retired (T8,
T12, T55, T56); 29 `| **I**` rows with no gaps; 20 phases. §5.1 is right, §9.5 is
stale by one edit, and §5.1's own rule — "stated once here and referenced
elsewhere" — is what the second site violates.

---

# Part 8 — The readiness verdict

## **NOT-READY**

The corpus is closer than it has been in six passes, and the reason it is not
ready has changed character. Five passes were blocked by **disagreements**: two
documents giving different answers to one question. **This pass finds none.** The
D-011 sweep worked: `PROTOCOL.md` and `STATE_MACHINE.md` agree about the terminal
stage's shape, its `sequence`, what triggers it, what it hashes to and what it
costs; every one of P3, G1, G2, G3, G4, G5, R-1 and R-4 is closed by an edit that
names the finding, quotes what it replaced, and says what it gives up.

What blocks now is **two silences**. Not two documents disagreeing — two questions
the corpus does not answer at all, both of them about the same thing: what state
survives an aborted hand into the next one.

### Blocking — must be settled before the engine is written

1. **H1 — `status := Absent` is derived from a per-receiver quantity.**
   Name the transition: **T46**, from the `attributed` field of whichever
   `HAND_ABORT` copy that receiver accepted. Name the invariant: none exists —
   there is no invariant asserting that two peers hold the same seat status at a
   hand boundary, and **that absence is the defect**. §3.1 removed the fork from
   `TERMINAL(k)` and the last quantity carrying it moved one link downstream, into
   `HAND_INIT`'s `dealt_in` and `bb_seat`.
   **An implementer could not pick a sane default.** Deleting the marking and
   keeping it are both defensible readings of the corpus as it stands —
   `PROTOCOL.md` §4.10 forbids deriving seat state from `attributed`,
   `STATE_MACHINE.md` T46 does it, and neither document acknowledges the other on
   this point. Choosing wrong stalls every hand after the first abort, or forks
   the chain silently, depending on H2.

2. **H2 — `seat_flags` is undefined.**
   Name the construction: `roster_hash(k)`, `PROTOCOL.md` §3.1 l. 721–725. Name
   the invariant: **I1**'s and **I28**'s stack vector is bound into it, and every
   event in the corpus chains to it.
   **An implementer could not pick a sane default**, and this is the clearest case
   of that in the report: any two guesses that differ produce different
   `GENESIS(k)`, so the two clients share not one verifying event, and the failure
   presents as an unexplained total incompatibility rather than as a disagreement
   about seat flags. It is one field and one paragraph of work, and it is the only
   undefined term in a normative construction anywhere in the five documents.

### An implementer would additionally have to decide these

3. **H3 — whether §5.3's `event_type_slot` is the key or a projection of it.**
   Sane default available: treat it as a projection and resolve its collisions
   under §4.0 step 12, never under §5.2.2.
4. **H4 — which gate row applies to a `cause = 1, attributed = []` abort.**
   Sane default available: the union, which is the permissive and safe read.
5. **H5 — §5.2.4's stale G2 prose.** Sane default available: the box wins, which
   is what `STATE_MACHINE.md` already did.
6. **H7 — `CRYPTOGRAPHY.md`'s four un-swept reproductions.** No guess required
   today; every copy currently agrees with its owner. It is scheduled drift.
7. **H8 — `STATE_MACHINE.md`'s two surviving restatements.** Same.
8. **H6 / R-2 — `THREAT_MODEL.md`'s `OQ-*` letters.** Not a guess; it misroutes a
   reader following a cross-reference, and two of the paragraph's stated facts are
   now false.
9. **R-3 — `CONTRIBUTING.md`.** The authority order is still self-inserted and
   still reads "D-001 … D-008", three decisions stale, in the file that tells the
   next editor which document wins.
10. **The `56` / `57` transition count**, and **`BadKeyProof`'s `cause`** — both
    flagged in place with named defaults, both correctly identified by the corpus
    as interim rather than settled.

### Is `STATE_MACHINE.md` implementable without guessing?

**Almost, and the gap is now two fields wide rather than four mechanisms wide.**

Everything the last four gates listed as unbuildable is buildable. The 20 phases,
56 transitions over the numbering T1–T60 with four retired, and 29 invariants are
internally consistent and were re-counted directly. Every declared `Event` variant
has a consuming row or an explicit statement that it deliberately has none, and
each such case is argued in place. Every `AbortKind` maps to a `cause`, in a table
written specifically because the previous revision had one that did not. §6's
legal-action computation, §7's blinds, button, minimum raise, incomplete all-in,
side pots, odd chips and showdown are specified to the level an implementer codes
against, with TDA citations attached. The chip half of D-010 is complete,
enforceable at the receiver, and conserves on every path in both modes. §12.1's
termination walk exists, is a table rather than a claim, and each of its rows was
independently re-derived here and holds.

The two blockers are both about the boundary between hands, and both are absences
rather than contradictions — which is why they survived a pass whose whole method
was finding contradictions. H1 is the mechanism; H2 is the field it writes into.
An editor who answers H2 in the safe direction — `seat_flags` carries only what
chained events establish — closes half of H1 as a side effect, and the remaining
half is T46 losing one line.

### The pattern, sixth iteration

D-011 was adopted with a closing rule: *if a sixth pass finds a defect of either
shape — a copy that drifted, or a consequence that made an attack worth mounting —
the right response is not a sixth rule but to cut the mechanism out of the MVP.*

**Neither shape appears.** No copy has drifted: every reproduction the restatement
sweep found still agrees with its owner, and the slot key has exactly one site.
No consequence makes an attack worth mounting: the eviction sweep is clean at
every layer, chips move on no abort path, and the worst outcome an adversary
buys anywhere in this corpus is a wasted hand.

What appears instead is the *third* shape, the one `PHASE1_VERIFY3.md` named and
this corpus has now confirmed six times: **the defect is never in the rule; it is
in the path the rule newly makes load-bearing.** P3's fix made the terminal stage
witness-independent, which made "which abort copy did this peer accept" a
per-receiver fact for the first time, which turned `attributed` into a disputed
quantity — and `attributed` was already wired into canonical state by a line
written when the stage was collective and every completing peer held the same
body. Q7 contains the exactly correct argument against deriving state from a
disputed quantity, applied to `owed` and not to `attributed`, because when Q7 was
written `attributed` was not one.

D-011 rule 2 is the answer to this shape and it demonstrably worked where it was
applied: `event_type` in the key closed **four** collisions, two of which did not
exist when the rule was written, because a definition covers paths its author has
not seen. The lesson to carry is that the same treatment is owed to the *other*
quantity that crosses a hand boundary. The corpus has one rule for a value that
enters a hash — §4.1's, that a set peers disagree about must not enter a hashed
body — and it is applied to `owed`, to the terminal `stage_hash`, and to
`TERMINAL(k)`. **It is not applied to seat status, and seat status enters
`roster_hash`.** Stating that rule once, as D-011 rule 2 stated the key once, is
the shape of the next edit.

D-010 and D-011 are both the right decisions and both are executed well. Nothing
in this verification reverses either, or any ruling of `research/PHASE0_FIXPLAN.md`.
The two things to record for whatever revisits them: D-011 was applied as a *wire
and eviction* decision, so the two documents whose subject is neither were not
swept; and making `TERMINAL(k)` independent of an aborted hand's content is
correct and does not, on its own, make the *next hand's starting state*
independent of it — because the abort body is still read by the engine after the
terminal value stops depending on it.

---

# Part 9 — What must happen before Phase 2

Ordered by what endangers the most.

1. **H2 — define `seat_flags` in `PROTOCOL.md` §3.1.** Which statuses, which bits,
   and whether a locally derived status may enter it. Answering it in the safe
   direction — only what chained events establish — closes H1(b) in the same edit.
2. **H1 — T46 stops setting `status := Absent` from `abort.attributed`**, or the
   corpus states an agreed basis for the marking. Delete is the smaller edit, is
   already the shipped behaviour heads-up, and costs only what Q7 already accepts.
   Q7 should be rewritten to carry the general rule rather than the `owed` instance
   of it. `PROTOCOL.md` §4.10's "no receiver may derive a seat's state from it"
   becomes true corpus-wide, and an invariant asserting seat status agrees across
   peers at a hand boundary should join §10.
3. **H3 — one sentence in §5.3** saying `event_type_slot` is a projection and its
   collisions resolve under §4.0 step 12.
4. **H4 — collapse §4.10's two `cause = 1, attributed = []` gate rows into one.**
5. **H7 — sweep `CRYPTOGRAPHY.md` for D-011.** Four pointers replace four copies;
   add the D-011 row its siblings carry. The missing row is how the same class was
   missed twice now.
6. **H5, H8, and the `56`/`57` count** — three one-line edits in whatever pass
   touches those sections.
7. **H6 / R-2 — one editor moves `THREAT_MODEL.md` §9.2 onto `DECISIONS.md`'s
   letters** and deletes the two claims that are now false.
8. **R-3 — move the authority order into `DECISIONS.md` as a numbered decision**
   and delete it from `CONTRIBUTING.md`, whose §5 checklist also needs D-010 and
   D-011.

The poker layer remains ready and has been for three passes: phases, transitions,
the determinism contract, betting rules, pot arithmetic, blinds, button and
showdown. Implementation continues there while the two hand-boundary questions
close.
