# PHASE1_VERIFY3.md — the Phase 2 readiness gate

**Scope.** The five partials left by `PHASE1_VERIFY2.md` (N2, N7, N8, N9, D-008
point 2) and the five defects it opened (M1–M5), all closed by four editors under
**D-009**. Then the four sweeps this pass was commissioned for — the equivocation
property message by message, the inert-certificate sweep, the chip-conservation
sweep, and a new-defect hunt aimed at the intersection of the dispute path, the
certificate machinery and D-005 forfeiture. Then a readiness verdict on
`STATE_MACHINE.md`.

**Method.** Every verdict is quoted from the document as it now stands, with line
numbers. Crate claims were re-read in
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`. No
specification document was edited.

**Files as read.** `PROTOCOL.md` 13:26, `STATE_MACHINE.md` 13:47,
`CRYPTOGRAPHY.md` 13:25, `THREAT_MODEL.md` 13:24, `DECISIONS.md` 13:16,
`NETWORK_STACK.md` 12:41 (untouched this pass), `DEPENDENCIES.md` 12:53,
`CONTRIBUTING.md` 12:55, `research/CRYPTO_LIBS.md` 12:42.

## Verdict counts

| Verdict | Count | Items |
|---|---:|---|
| **RESOLVED** | 8 | N2, N7, N9(a), N9(b), D-008 point 2, M1, M2, M4, M5 (N9 counted once) |
| **PARTIAL** | 2 | N8, M3 |
| **UNRESOLVED** | 0 | — |
| **REGRESSED** | 0 | — |

**Eight new defects, P1–P8.** The instruction was to assume a fourth worse defect
exists. It does, and it is **P1**: the divergence-resolution procedure requires
every honest peer that reconciles to sign a *second, different* `STATE_HASH` into
the slot its first one already occupies, which is D-009 rule 1's fourth
recurrence and manufactures a verifying `EquivocationProof` against a peer whose
only fault was dropping one stream. **P2** and **P3** are also
consensus-critical. None of the three was reachable before this pass: P1 was
created by the edit that gave T53/T54 a real trigger, P2 by N4's move of
`DISPUTE` to `chain_scope = 0` combined with N5's widening of T55, P3 by the M1
fix routing every below-floor stall through a collective `HAND_ABORT`.

---

# Part 1 — Per-item verdicts

## N2 — "adjudicable offline" — **RESOLVED**

Both surviving sentences are withdrawn, in the two places the last pass named, in
the same form `PROTOCOL.md` §6.4 used.

> `STATE_MACHINE.md` §5.2, T54's note (l. 1140–1152): "**A stronger claim stood
> here and is withdrawn (N2), in the same form and for the same reason as
> `PROTOCOL.md` §6.4 withdrew it.** It read: *'is publicly adjudicable
> **offline** by anyone who runs the reference engine over it.'* **There is no
> reference engine.**"
> `STATE_MACHINE.md` §12 (l. 2387–2394): "the evidence is preserved and is
> sufficient for a human, or for a future adjudicator, to diagnose the
> divergence; **no adjudication procedure is specified, offline or live, and none
> is claimed**. This sentence previously read 'the evidence is adjudicable
> offline and not live', which asserted a reference engine that does not exist"

§12 also gained the positive statement the finding implied but did not ask for
(l. 2396–2402): "This document specifies *the* deterministic engine as a thing
every client implements — not a reference implementation a third party can be
pointed at… Any claim elsewhere in the corpus that evidence is 'adjudicable by
anyone who runs the reference engine over it' is appealing to something this
document does not provide, and is to be read as withdrawn." `PROTOCOL.md` §6.3
case (c) (l. 2604) now says "Live adjudication is not available and is not
claimed; neither is offline adjudication", closing the gap the previous wording
left between the two adverbs. A corpus-wide grep for "adjudicable" returns only
withdrawal notices and OQ-F.

## N7 — the checkpoint-placement rule — **RESOLVED**

`PROTOCOL.md` §6.2 (l. 2508–2515), the section that owns the rule normatively:

> "**No checkpoint is placed after a hole card has been opened to anyone but its
> owner.** The rule is about *public* opening and is stated that way rather than
> in the shorter form an earlier revision used — 'no checkpoint is placed after
> any hole card has been opened' — which is false on its face against the table
> directly above it: every player opens its own two cards at `DEAL_PRIVATE`, and
> `DEAL_PRIVATE` precedes checkpoints 3 through 7."

`THREAT_MODEL.md` X29 (l. 871) carries it in the same words, and the section adds
the explicit step-in-step instruction ("this is the section that states the rule
normatively and the two must stay in step").

## N8 — `DECISIONS.md`'s open list — **PARTIAL**

**What was fixed.** Sub-point 1's second half: `STATE_MACHINE.md` §11's OQ-A row
no longer points at a list that does not contain it (l. 2331: "`DECISIONS.md` —
**and, as of this revision, it is not yet in that document's open list**… Adding
the row is `DECISIONS.md`'s to do; this cell must not claim it is there until it
is"). `PROTOCOL.md` §12's OQ-A and OQ-D entries likewise claim only
`THREAT_MODEL.md` §9.2 and `STATE_MACHINE.md` §11. Nothing now points falsely.

**Why still PARTIAL.** `DECISIONS.md` was edited this pass — D-009 was added at
13:16 — and its open list was not touched. All three original sub-points stand,
verified by reading `DECISIONS.md` l. 885–894 in full and by
`grep -n "OQ-A\|OQ-D\|OQ-F" DECISIONS.md`, which returns **nothing**:

1. **OQ-A is still absent.** The question that reverses D-005 wherever
   `|V| < 2` — and which D-009 rule 2 has now made the disposition of *every*
   below-floor stall rather than of the certificate branch alone — has no row.
2. **OQ-D is still absent.** `cause = 4`'s chip disposition has no owner row.
3. **The N4 row is still there**, l. 892: "`DISPUTE` is chained, unsequenced and
   legal at any time, so the two disputes an honest peer is *required* to emit
   are an equivocation proof against itself (verify N4)". N4 is closed;
   `DISPUTE` is unchained with sentinels and its own anti-replay. And the OQ-F
   row (l. 891) still frames itself on the withdrawn text — "since four sections
   claim disputes are 'deterministically adjudicable'" — which is now describing
   a state of the corpus that no longer exists.

This is the one item where three passes have now recorded the same defect and no
edit has been made. It is bookkeeping, but it is the bookkeeping that schedules
the two decisions Phase 4 is blocked on.

## N9(a) — `HAND_ABORT`'s emitter set — **RESOLVED**

`PROTOCOL.md` §4.11 l. 2169 now reads "the `HAND_INIT` set minus every seat named
in `n(1) attributed` (§4.10)", and §4.10 (l. 2004–2021) states the subtraction
normatively with the reason:

> "a collective stage completes only when every required emitter is heard (§3.2),
> and the attributed seat is by construction the one that is silent, so under
> that cell an abort against a vanished seat could never complete its own stage
> and the whole abort path would deadlock behind the failure it exists to dispose
> of."

The set is stated on `attributed` rather than on "whose failure is the reason",
which is the right choice: `attributed` is a body field every peer recomputes
identically, so every peer derives the same emitter set from the same bytes.

The finding is closed. What it did not reach is the case where `attributed` is
**empty**, which is now the disposition of every below-floor stall — that is
**P3**.

## N9(b) — `TimeoutCertificate{Join}` — **RESOLVED**

`PROTOCOL.md` §8.4 ruled, and `STATE_MACHINE.md` followed rather than arguing:

> `STATE_MACHINE.md` §2.6 (l. 302–304): "**`DeadlineKind` has exactly two
> variants, and `Join` is not one of them (N9(b)).**"
> T4 (l. 809): "| T4 | `Seating` \| `AwaitingSeatRngCommit` \|
> `AwaitingSeatRngReveal` | `FormationAbandoned` | no `HAND_INIT` has happened …"
> §13 item 5 (l. 2489–2492): "the objection this section carried against T4 is
> therefore closed, not carried forward."

`Event::FormationAbandoned` is declared (§4.1 l. 545) with an unusually careful
soundness argument for a purely local timer — admissible only in phases 1–3,
where `ledger_in == ledger_out == 0`, no card exists and the only next phase is
absorbing, so "two peers reaching it at different wall-clock moments therefore
disagree about nothing that is ever hashed or compared".

The finding is closed. The same normative sentence that closed it invalidates two
other rows nobody re-checked — that is **P4**.

## D-008 point 2 / M1 — the below-floor certificate — **RESOLVED**

The two documents now give the same answer, and it is `PROTOCOL.md`'s.

> `STATE_MACHINE.md` §8.4 rule 6 (l. 1693–1699): "**if `|V(subject)| < 2` the
> certificate is rejected, whatever its `kind`** — `Action` … and `Crypto` …
> alike. It is inert: not accepted, not chained, not evidence, no `Fault`, no
> `AbortRecord`, no forfeiture, no entry in `certified_subjects`, and the state is
> bit-identical afterwards (I21). Neither T34 nor T16/T22/T27/T41/T44 is reachable
> when the voter set is one seat or none."
> §8.4 (l. 1740–1746): "**D-008 point 2 is applied literally, and the carve-out
> that stood here is deleted (M1, D-009 rule 2).**"

The five transitions lost their branches — T16 (l. 878), T22 (l. 894), T27
(l. 904), T41 (l. 936), T44 (l. 944) each now carry
`∧ **|V(subject)| ≥ 2**` and nothing else — and **I29(b)** (l. 2276) is restated:
"The previous form of this clause allowed a below-floor `kind == Crypto`
certificate through with `attributed == []`; that carve-out is deleted, and the
only `AbortRecord{kind: HandDeadline}` producer is now T57."

The certificate-free carrier §13 recorded as missing exists:
`Event::HandDeadlineAbort` (§4.1 l. 544) and **T57** (l. 964). `THREAT_MODEL.md`
§7.3(c) (l. 1171–1181) withdrew its "may still end the hand" sentence in terms
and explains why the difference is a security property; `CRYPTOGRAPHY.md` §2.10
(l. 529–548) splits its one sentence into the two separate halves D-009 rule 2
requires. The transition count moved 56 → 59 and §5.1 (l. 683) states the new
figure once.

The residual is not in the ruling but in the carrier: `HandDeadlineAbort` is
raised only when a **collective** `HAND_ABORT` stage completes, and on this path
that stage's required emitter set contains the silent seat. That is **P3**.

## M1 — see D-008 point 2 — **RESOLVED**

## M2 — the timeout vote's slot — **RESOLVED**

Correction option 2 was taken — the subject is in the key — and it was made in one
place and quoted everywhere, which is what the finding asked for.

> `PROTOCOL.md` §5.2 (l. 2237–2245), canonical for the corpus:
> ```
> slot(E) = (protocol_version, table_id, hand_id, sequence, event_class,
>            sender_public_key == K)  ++  subject(E)
> subject(E) = ()                            when event_class == 0
>            = (payload n(1) subject_seat)    when event_class == 1
>            = (payload n(0) subject_digest)  when event_class == 2
> ```

`TIMEOUT_CERT` was given the same treatment even though the finding did not ask
for it, and the case is real rather than hypothetical (§4.8 l. 1836–1846: "at a
stage where seats `A` and `B` have each voted against the other — two honest,
mutually partitioned peers is enough — a third seat whose timers expired against
both is entitled to certify both"). §5.3 (l. 2419–2427) carries the matching
sparse per-stage map keyed by `(seat, subject_seat)` and `(seat, subject_digest)`,
bounded at `MAX_SEATS × (MAX_SEATS − 1) = 90` per stage per class. §2.3
(l. 351–352) carries the discriminator note. §8.4 (l. 3167–3175) re-derives the
deadlock argument on the voter sets rather than on a tie-break, and states
explicitly that "**A voter is not asked to choose one subject, and no rule
anywhere in this document limits it to one vote per stage.**"

`THREAT_MODEL.md` gained **X31** (l. 873) as its own row rather than folding it
into X7, with the right class (D&A, "and only because of a slot key our own
client enforces") and an honest residual. `STATE_MACHINE.md` §8.6 (l. 1963–1974)
now states its dependency instead of asserting the conclusion: "**the equivocation
path needs no floor of its own for exactly as long as §5.2's property holds, and
its standing test is the honest-peer mirror of `CheaterEquivocation`**".

M2 as filed is closed. The *class* is not — see P1.

## M3 — the `SmallRng` absence claim — **PARTIAL**

The two documents that own the reasoning are fixed and their evidence is correct.
Re-verified in crate source:

```
rand-0.9.5/Cargo.toml   [features] default = [ "std", "std_rng", "os_rng",
                                               "small_rng", "thread_rng" ]
yamux-0.13.10/Cargo.toml:57-58    [dependencies.rand] version = "0.9.0"   # defaults taken
igd-next-0.16.2/Cargo.toml:148-149 [dependencies.rand] version = "0.9.0"  # defaults taken
rand-0.8.8/Cargo.toml   [features] default = [ "std", "std_rng" ]         # small_rng absent
```

> `CRYPTOGRAPHY.md` §7.2 (l. 1278–1279): "**`SmallRng` *is* compiled in, and no
> choice open to this project removes it.**"
> `THREAT_MODEL.md` A7 (l. 283–287): "**Correction, twice made, and it weakens the
> assumption both times (D-009 rule 3).** … **Both are false, and no restatement
> of an absence belongs here.**"

The enforcement is real and was seen to fail: `src/security/rng.rs` exists and
carries `tests::our_own_code_uses_no_generator_but_the_os_one` (l. 103), with the
module doc stating in its own words that "`SmallRng` is in the binary and there is
nothing we can do about it".

**Why PARTIAL.** `CRYPTOGRAPHY.md` §7.2 issues the instruction (l. 1374–1376) —
"`PROTOCOL.md` §4.4 and `THREAT_MODEL.md` A7 must therefore drop '`SmallRng` is
absent' in every form, and `research/CRYPTO_LIBS.md` §7.3's 0.8-only check must
gain the 0.9.5 line above" — and two of the three targets did not act.

* `PROTOCOL.md` §4.4 l. 1299, unchanged and unqualified: "**`SmallRng` is absent
  because rand 0.8's `small_rng` feature is not enabled**". The reason clause is
  scoped to the 0.8 major; the claim is not. This is a false absence claim
  surviving in the second-ranked document of the corpus, which is precisely what
  D-009 rule 3 forbids, and it now contradicts `CRYPTOGRAPHY.md` §7.2 and
  `THREAT_MODEL.md` A7 head-on.
* `research/CRYPTO_LIBS.md` §7.3 l. 188–192 still carries only the 0.8 check, and
  its boxed deviation note names `StdRng` alone: "`StdRng` is compiled into the
  binary through `rand` 0.8's default features." Research, not authority, but it
  is the file the manifests get copied from.

## M4 — `PLAYER_LEAVE`'s chain scope — **RESOLVED**

The clause was deleted rather than repaired, which is the disposition the finding
preferred.

> `PROTOCOL.md` §4.10 (l. 2085–2103): "**The 'courtesy notice that is not chained'
> clause is deleted (M4).** … **A `PLAYER_LEAVE` received outside a hand boundary
> is rejected under §4.0 like any other out-of-stage chained event** … a UI hint
> may not have an envelope no rule in this document defines."
> §4.11 (l. 2175): "`PLAYER_LEAVE` is on the chained side of that line with no
> exception of any kind (M4, §4.10)."

`STATE_MACHINE.md` followed with T17/T35 as rejection rows and **T58**/**T59** for
the boundary phases (l. 1012–1013), which closed a second hole in passing: the one
phase where a `PLAYER_LEAVE` is legal was the phase in which the engine had no row
for it. §4.1 (l. 632–637) states the three-event rule once.

## M5 — the emitter gate versus the receiver check — **RESOLVED**

> `PROTOCOL.md` §4.8 (l. 1774–1786): "**The legality condition is per subject,
> normatively (M5).** The condition is *'nothing from `subject_seat` at that
> stage'*, and it is stated in exactly those words here, in the receiver check
> below, and in §8.3. The reading discarded is … *'has not accepted any valid
> event for that stage'* … A gate that deletes the machinery it guards is not the
> gate that was meant."

The receiver check (l. 1841–1844) and §8.3's race paragraph (l. 3107–3112) use the
same words. `STATE_MACHINE.md` §8.4 (l. 1863–1873) records the question as closed
rather than defaulted.

---

# Part 2 — The equivocation property, checked message by message

D-009 rule 1: *no sequence of emissions the protocol requires or permits of an
honest peer may produce two `SignedEvent`s in one slot*
(`PROTOCOL.md` §5.2 l. 2262–2270). The check below is what §4.11's own table
(l. 2189–2196) claims to have run. **Its verdict is wrong in one row.**

| Type(s) | What an honest peer is required to emit | Verdict |
|---|---|---|
| The 13 `chain_scope = 0` types — `HELLO`, `CAPABILITIES`, `JOIN_REQUEST`, `JOIN_ACCEPT`, `JOIN_REJECT`, `PLAYER_LIST`, the six `LOBBY_*`, `DISPUTE` | Any number, at any time | **Clean.** Outside the predicate by §5.2 and the §2.3 sentinel envelope. `DISPUTE`'s two mandatory emissions per hand (§6.3 steps 2 and 4a) occupy no slot |
| `TABLE_READY` | One per seat, setup-chain stage | **Clean.** One contribution, one stage |
| `RNG_COMMIT`, `RNG_REVEAL` | One per seat each, two consecutive stages | **Clean.** Distinct `sequence` |
| `HAND_INIT` | One derived copy per present seat, stage 0 of chain `k` | **Clean** for equivocation. See R-1 for the separate stall hazard when peers derive different `ledger_delta` |
| `DECK_INIT`, `DECK_COMMIT` | One per dealt-in seat per stage | **Clean** |
| `SHUFFLE_STEP`, `SHUFFLE_PROOF` | One each, single-writer, two consecutive stages (§3.2 l. 758–763) | **Clean** |
| `DEAL_PRIVATE` | One event per dealt-in seat carrying **exactly** `2(m−1)` entries (§4.6 l. 1608) | **Clean.** The "exactly" is what forbids incremental publication, which would otherwise be two bodies in one slot |
| `BOARD_REVEAL` | One per dealt-in seat per street, "exactly the indices of that street" | **Clean.** Each street is its own stage |
| `SHOWDOWN_REVEAL` / `SHOWDOWN_MUCK` | Exactly one of the two per showdown seat | **Clean, and correctly reasoned.** §4.6 l. 1697–1706 says why this pair may stay in one slot: "nothing in this protocol ever requires or permits a seat to both show and muck, so a seat that emitted both did something no rule allows and the resulting proof is the predicate working, not a false positive" |
| The five `ACTION_*` | One per turn, single-writer | **Clean.** Two bodies here is the genuine article |
| `TIMEOUT_VOTE` | One per **subject** per stage; two simultaneous subjects are normal (§8.4) | **Clean since M2.** Subject in the key. §4.8 l. 1815–1822 re-derives capacity one by showing every other field is a function of `(subject_sequence, subject_seat)` |
| `TIMEOUT_CERT` | One per **subject_digest** per stage; a third seat may legitimately certify two mutually-accusing seats | **Clean since M2.** `subject_digest` in the key. The one remaining variable field, `n(1) votes`, is pinned by the receiver check "the voter set is exactly `V(subject)`" plus §5.2's re-emission rule, so an honest certifier signs one body |
| `PLAYER_SIT_OUT`, `PLAYER_SIT_IN`, `PLAYER_LEAVE` | One per seat at a hand boundary, single-writer, and **only** there since M4 | **Clean** |
| `HAND_COMPLETE` | One derived copy per present seat at the terminal stage | **Clean** for one peer's own emissions. See **P2** for the two-honest-peers case |
| `HAND_ABORT` | One derived copy per required emitter at the terminal stage | **Clean** for one peer's own emissions. See **P2** and **P3** |
| **`STATE_HASH`, `STATE_ACK`** | One per seat per checkpoint stage — **and, per `STATE_MACHINE.md` T53/T54, a second, re-derived one for the same checkpoint after reconciliation** | **FAILS. This is P1.** |

§4.11's arithmetic (`13 + 13 + 10 + 1 + 2 = 39`) is correct and the table covers
every row. Its verdict for the collective-ordinary group is what is wrong:

> `PROTOCOL.md` §4.11 l. 2192: "The emitter set is one contribution per seat per
> stage, and each street's or checkpoint's stage has its own `sequence`. **Capacity
> one by the stage rule**"

That is true of every member of the group except `STATE_HASH`, and it is false of
`STATE_HASH` because of a rule in another document. See P1.

Two structural observations, recorded so a future pass does not re-derive them:

* The property is now **stated in one place and pointed at from the others**,
  which is the shape that makes it maintainable. `CRYPTOGRAPHY.md` §16's ownership
  table (l. 2100) is explicit that it does not restate the predicate and explains
  why the `ctx` block is unaffected by the slot-key change — the two are separately
  domain-separated (`"p2p-poker v1 deck-ctx"` against
  `"p2p-poker v1 timeout-cert"`).
* §5.2's **re-emission obligation** (l. 2272–2277) — "A peer that must send an
  event again sends the stored bytes; `emitted_at_unix_ms` is advisory (§2.6) and
  varies freely, so a re-signed copy is a second body in the same slot" — is the
  right rule and is the rule P1 violates. The corpus contains exactly one place
  where a peer is required to send an event again with **different content**, and
  nobody checked it against this obligation.

---

# Part 3 — The inert-certificate sweep

Every place in all five documents where a certificate could be given an effect was
read. **No survivor.** The sweep pattern was `|V| < 2`, `|V| >= 2`, `|V| ≥ 2`,
`inert`, `below the floor`, `may still end the hand`, `HandDeadline`,
`AbortRecord`, `certified_subjects`, plus every `TimeoutCertificate` row in
`STATE_MACHINE.md` §5.2.

| Document | Where a below-floor certificate is spoken of | Effect granted |
|---|---|---|
| `PROTOCOL.md` §8.3 (l. 3051–3070), the canonical box | "not chained, not evidence, no terminating effect … produces **no `AbortRecord` of any kind**, and triggers **no forfeiture**. It is silently ignored. It is not an error either" | none |
| `PROTOCOL.md` §8.3 effect table (l. 3140) | "either kind, `\|V\| < 2` \| **no effect.**" | none |
| `PROTOCOL.md` §4.8 `TIMEOUT_CERT` (l. 1849–1853) | "A certificate with `\|V\| < 2` is **inert** … §8.3's boxed below-the-floor rule is canonical for that and this line restates nothing beyond the pointer" | none |
| `PROTOCOL.md` §8.5 (l. 3284) | "**Where `\|V\| < 2` it proves only that one peer said so, which is why §8.3 gives it no effect of any kind**" | none |
| `STATE_MACHINE.md` §8.4 rule 6 (l. 1693) | rejected, both kinds, bit-identical state | none |
| `STATE_MACHINE.md` T8, T12, T16, T22, T27, T34, T41, T44 | every guard carries `∧ \|V\| ≥ 2` | none below the floor |
| `STATE_MACHINE.md` unanimity table (l. 1720) | "0 or 1 \| … \| **no effect at all** (rule 6)" | none |
| `STATE_MACHINE.md` I29(b) (l. 2276) | "never accepted, of either `kind` … no `AbortRecord`, no `FaultRecord`, no entry in `certified_subjects` and no stack change at all" | none |
| `STATE_MACHINE.md` §8.5 (l. 1876), §8.6 (l. 1939), §8.7 (l. 2046), §9.5 (l. 2190), §12 (l. 2366) | all five restate inertness and route to T57 | none |
| `THREAT_MODEL.md` X8 (l. 851) | "A certificate below the floor is inert (D-009 rule 2), so it cannot void the hand" | none |
| `THREAT_MODEL.md` X10 (l. 853) | "a below-floor certificate **does not end a hand**" | none |
| `THREAT_MODEL.md` §7.3(c) (l. 1171–1181) | the "may still end the hand" sentence is quoted and withdrawn | none |
| `THREAT_MODEL.md` §5.5 `CheaterDisconnect` row (l. 947) | requires the test to assert "the hand still running afterwards" | none |
| `CRYPTOGRAPHY.md` §2.10 (l. 529–548) | the sentence is split into two explicitly separate halves | none |
| `NETWORK_STACK.md` §1 (l. 106–109), §9.5 (l. 1207–1215), §16 (l. 1956) | "whenever `\|V\| < 2` that certificate has no effect"; "the test is `\|V\| < 2` and never `n = 2`" | none |

M1 is not reopened. Two residuals, neither an effect:

1. **`THREAT_MODEL.md` X10 l. 853 still carries a pre-D-009 half-sentence**: "a
   `kind = 1` certificate **is invalid at two seats and must be rejected by every
   receiver**". It is scoped on the seat count, and it calls the certificate
   *invalid* where `PROTOCOL.md` §8.3 explicitly withdrew that word — "An earlier
   revision of this bullet said 'invalid; every receiver rejects it', which …
   gave the two kinds two dispositions where D-009 rule 2 gives them one. The
   disposition is uniform: ignored, not an error." The surrounding sentences in
   X10 are correct and carry the D-008/D-009 scoping, so this is stale wording
   inside a right paragraph, not a live rule. Low.
2. `STATE_MACHINE.md` §8.4 rule 6 uses the word "rejected" and `PROTOCOL.md` §8.3
   uses "silently ignored". They agree on every observable — no acceptance, no
   chaining, no `Fault`, bit-identical state (I21) — because a `Rejection` in this
   engine is defined as exactly that. Worth one clarifying sentence, not a defect.

---

# Part 4 — The chip-conservation sweep

Every path that moves a chip was followed. **Conservation holds on every path in
tournament mode, and cash mode's variant is stated correctly — but two paths do
not work as written**, and one of them (P8) is what makes cash mode's own variant
unreachable.

## Conserved, checked by hand

**§8.6's forfeiture formula** (l. 1988–2005). With
`culprits = attributed`, `others = { s ∉ culprits : committed_hand[s] > 0 }`,
`base = Σ_{others} committed_hand`, `forfeit = Σ_{culprits} committed_hand`:

```
Σ returns = Σ_{others}(committed_hand[s] + share[s]) + remainder
          = base + (forfeit − remainder) + remainder
          = base + forfeit
          = Σ_s committed_hand[s]                                 ✓
```

with `remainder = forfeit − Σ share[s] < |others|` distributed one chip each
clockwise from `succ(button_pos)`, so the remainder loop terminates and cannot
over-distribute. Seats outside both sets have `committed_hand == 0` by the
definition of `others`, so the untouched seats owe nothing. The `others is empty`
branch returns the culprits' own commitments, and the `culprits == {}` branch
(I27) returns every seat exactly its own. **I2** asserts this step-locally and
names both restoration producers (T54 and T57) explicitly.

**§7.5 `build_pots`.** For levels `l₀ < l₁ < …` over positive commitments,
`Σ (lᵢ − lᵢ₋₁) × |contributors_i| = Σ_s committed[s]` exactly, and the
single-contributor band becomes a refund rather than a pot. **I6** asserts
`Σ pot.size + Σ refunds == Σ committed_hand`. `HAND_COMPLETE`'s receiver check
(§4.10 l. 1985) requires `sum(deltas) == 0` and the C-9 ledger identity.

**§7.6 odd chips.** `share = pot.size / |W|`, `remainder = pot.size % |W|`,
`0 ≤ remainder < |W|`, one chip each clockwise from the first seat left of
`button_pos`. Exact, and the tie-break is a pure function of public state, which
is the property the `[OUR CHOICE]` note gives as its reason.

**Blinds off an absent seat.** §5.3 steps 6 and 7 have `Absent` and `SittingOut`
seats post antes and blinds while step 4 leaves them `dealt_in = false`. They
enter `committed_hand`, so they are inside every sum above; §8.6's `others`
comment says so in terms ("includes blinded-off absent seats"). The drain
terminates: step 2 busts a zero stack, §9.3 condition 1 ends the tournament.

**The ledger identity.** **I1** is
`Σ stack + Σ committed_hand == ledger_in − ledger_out`, with both counters moving
only in hand-init step 0 (**I28**), and the corollary is stated rather than
assumed: "tournament mode: no entry or exit occurs after the first hand, so the
right-hand side is constant and equals `players_at_start × start_stack` — which is
the old I1, now derived rather than assumed." **Cash mode's variant is stated
correctly** (§9.4 l. 2138–2156): the right-hand side changes only at a hand
boundary, the change is carried by `PROTOCOL.md` §4.4's `n(11) ledger_delta`
recomputed by every receiver, and **I5** is what keeps I1 checkable mid-hand.
T58 marks and does not move, so the announcement and the application are two
separate moments with one artefact between them.

**T4 / `FormationAbandoned`** moves nothing because `ledger_in == 0` there, and the
guard says so.

## Two paths that do not work as written

* **P7** — an `Absent` or `SittingOut` seat that posts a blind is **eligible to win
  the pot it posted into**, because `build_pots` filters `eligible` on `folded`
  and not on `dealt_in`. Chips are not destroyed; they are awarded to a seat that
  by **I20** has no cards. Detail below.
* **P8** — cash mode's seat entry, which is the only thing that can move
  `ledger_in` after the first hand, has no event and no transition. Detail below.

Neither breaks conservation arithmetically. P7 breaks settlement determinism and
contradicts a deviation the corpus says it implements; P8 makes I1's cash-mode
form unreachable rather than wrong.

---

# Part 5 — New defects

Ordered by severity. Continuing the letter series: **P**.

## P1 — the divergence procedure requires an honest peer to sign a second, *different* `STATE_HASH` into a slot its first one occupies; that is a verifying `EquivocationProof` against it, and T55 forfeits its chips

**Severity: highest. This is D-009 rule 1's fourth recurrence**, after A-4
(lobby/join), N4 (`DISPUTE`) and M2 (`TIMEOUT_VOTE`). It is worse than M2 in
reachability: M2 needed two Sybil seats to stall a stage deliberately; this needs
one dropped stream, which `PROTOCOL.md` §6.3 calls the common case.

**The slot.** `STATE_HASH` is `0x0701`, `chain_scope = 1`, `event_class = 0`,
collective stage (§4.11 l. 2165; §4.9 l. 1901–1902), payload
`n(0) checkpoint`, `n(1) state_hash`, `n(2) transcript_head`. Its slot key is
therefore the plain six-tuple with `subject(E) = ()`:
`(protocol_version, table_id, hand_id, sequence, event_class = 0, sender)`.
Capacity one.

**The second body.** `STATE_MACHINE.md` §5.2, the paragraph that gives T53 and T54
a real trigger (l. 1059–1064):

> "Step 3 has peers exchange the events they are missing; each then re-derives and
> **re-emits its `StateHash` for the disputed checkpoint**, and that signed value
> is what the engine consumes. T53 fires when every required signer's re-emitted
> value agrees, T54 when two distinct values survive after every required signer
> has re-emitted."

T53 (l. 1052) is the *successful* resolution. For it to fire, at least one peer's
re-emitted value must differ from the value it emitted the first time — otherwise
there was no divergence to resolve and no reconciliation happened. So on the happy
path of divergence recovery, **the peer that was behind necessarily signs two
distinct bodies for one checkpoint**: same `table_id`, same `hand_id`, same
`sequence` (it is "for the disputed checkpoint"), same `event_class = 0`, same
sender, different `state_hash` and different `transcript_head`, therefore different
`event_hash`. `PROTOCOL.md` §5.2's predicate is satisfied exactly.

It is also a direct violation of the obligation §5.2 states two paragraphs later
(l. 2272–2274): "**Re-emission is re-transmission, never re-signing.** A peer that
must send an event again sends the stored bytes … a re-signed copy is a second
body in the same slot and is indistinguishable from an equivocation." Here the
protocol requires a re-signed copy with *different content*, which is not merely
indistinguishable from an equivocation — under the predicate it **is** one.

**What it costs the victim.** Any peer holding both copies builds
`EquivocationProof{accused = the reconciled peer, event_a, event_b}`. Verification
is self-contained by design and needs no table state. `STATE_MACHINE.md` T55
(l. 1054) consumes it from every live phase including `Diverged` — phase 20 is in
T55's scope, “2–15 and 20”:

> "| T55 | **any phase except `TableClosed`**, when a hand is live — 2–15 and 20 |
> `EquivocationProof` | the proof verifies at the protocol layer ∧ it names the
> current `hand_id` | `HandAborted` | `Fault{Equivocation}`;
> `AbortRecord{kind: Equivocation, attributed: [accused]}`; the §8.6 forfeiture
> formula applies |"

So the honest peer's committed chips are forfeited to the others, and
`PROTOCOL.md` §5.2 adds its key to `libp2p::allow_block_list`. The attacker does
not even have to cause the divergence: a peer that merely watches a genuine
dropped-stream reconciliation collects the proof for free. If it *does* cause it,
X29 already concedes that faulting a table costs one false `state_hash`, so the
whole sequence — fault the table, wait for the victim to reconcile honestly, file
the proof it just manufactured — is available to one modified client at any table
size.

**Why the last pass could not see it.** `PHASE1_VERIFY2.md` §3.5 recorded T55's
widening as correct and M2 as the only thing it enabled. The re-emission did not
exist as a specified artefact at that point: T53/T54 were triggered by a *phrase*
("transcript reconciliation completes"), which the editors correctly identified as
not being an event and replaced — §13 item 10(b), l. 2513–2515: "they now consume
the `StateHash` that `PROTOCOL.md` §6.3 step 3 already has peers re-emit." The fix
is right about the engine and wrong about the slot, and the check D-009 rule 1
mandates for every new chained emission was not run on it because nobody thought a
new emission had been added.

**A second half, which must be fixed either way.** The artefact the fix appeals to
does not exist in `PROTOCOL.md`. §6.3 step 3 (l. 2566–2570) reads in full:

> "**Step 3 — reconcile transcripts.** Peers exchange the events they are missing.
> Every event is individually verifiable … Most real divergences end here — a peer
> missed one event because of a dropped stream, fills the gap, re-derives, and
> matches. **Play resumes from the checkpoint with no further action.**"

"No further action" is the opposite of "re-emits its `StateHash`". So as the two
documents stand, T53 and T54 are triggered by an event no wire rule produces —
which is exactly the defect N9(b) closed for `TimeoutCertificate{Join}` and T48
closed for `RevealRejected`, in a third place. One of the two sentences must go,
and whichever way it goes the slot question has to be answered first.

**Correction.** The re-emission needs an envelope that is not the checkpoint's
slot. Three candidates, in the order this pass would rank them:

1. **Give the re-emission its own stage.** A reconciliation round is a new
   `sequence` chained from the disputed checkpoint's `stage_hash`, not a second
   copy of it. This is the shape `STATE_ACK` already uses relative to
   `STATE_HASH`, it keeps the value chained and comparable, and it makes T53/T54's
   "every required signer has re-emitted" a stage-completion predicate the engine
   already knows how to express.
2. **Make the re-emission unchained**, like `DISPUTE`, carried as the `evidence`
   of a `DISPUTE { kind = 1 }` — which §6.3 step 2 already obliges every peer to
   emit and which already carries "its own `STATE_HASH` event". That is nearly
   free: the artefact is already in the dispute. It costs the ordering property,
   which is P2's subject.
3. **Add `checkpoint_round` to the slot key** for `event_type = 0x0701`, the way
   `subject_seat` was added for `event_class = 1`. Cheapest edit; but it extends
   the canonical predicate a third time and must then be quoted everywhere, and
   §5.2's key is already carrying two special cases.

Whichever is taken, §4.11's per-group check table (l. 2192) must lose its claim
that the collective-ordinary group is "capacity one by the stage rule", because
that is the sentence that certified this row as clean.

## P2 — an `EquivocationProof` has no position in the total order the determinism contract claims, so two honest peers can apply T55 at different points and produce two incompatible `HAND_ABORT` bodies for one stage

**Severity: high, consensus-critical.** This is the second-order consequence of
N4's fix that nobody checked, amplified by N5's widening of T55.

`STATE_MACHINE.md` §3.2 (l. 437) is the determinism contract's own statement of
how arrival order is removed:

> "| Network arrival order | `protocol` ordering buffer keyed by
> `(table_id, hand_id, sequence, previous_event_hash)` | **the engine sees one
> total order**; out-of-order events are buffered or rejected before `step` |"

N4 moved `DISPUTE` — the only carrier of an `EquivocationProof` (§5.2 l. 2350) —
to `chain_scope = 0`, which by §2.3 forces `table_id = ZERO32`,
`hand_id = 0xFFFF_FFFF_FFFF_FFFF`, `sequence = 0`, `previous_event_hash = ZERO32`.
**All four components of the ordering key are sentinels.** The buffer cannot place
it. So `Event::EquivocationProof` and `Event::Dispute` reach `step` at whatever
local moment the network delivered them, and §3.2's guarantee does not cover the
one event class that can abort a hand from any phase.

`PROTOCOL.md` §5.2's normative acceptance sentence makes the consequence
unavoidable rather than theoretical:

> "A verifying `EquivocationProof` naming a seated participant of this table
> aborts the hand with `cause = 5` **from any phase in which a hand is live, not
> only from a divergence**."

**The concrete failure.** Seat `X` stalls at a crypto stage at a four-seat table,
`|V(X)| = 3`, and a `kind = 2` certificate completes. Independently, a genuine
`EquivocationProof` against seat `Y` is in flight. Peer `A` applies the
certificate first: it is in `HandAborted` and derives
`HAND_ABORT{cause = 1, attributed = [X], cert_hash = Some(h)}`. Peer `B` applies
the proof first: T55 fires, it is in `HandAborted`, and it derives
`HAND_ABORT{cause = 5, attributed = [Y], evidence = […]}`. Two honest peers, two
different bodies for the terminal stage. No equivocation — different signers — but:

* §4.10 obliges every receiver to recompute every field, so each rejects the
  other's copy and **the collective `HAND_ABORT` stage never completes**;
* §3.1 (l. 662) makes `TERMINAL(k)` "the `stage_hash` of the … `HAND_ABORT`
  stage", and `GENESIS(k+1)` is a function of `TERMINAL(k)`, so **the session
  cannot start another hand either**;
* if the divergence is instead noticed at a checkpoint, §6.3 case (c) fires and
  the table faults with `cause = 4` — the outcome X29 calls unresolvable.

An attacker chooses this: deliver the proof to half the table before the
certificate and to the other half after. It needs no invalid signature and no
forged artefact — the certificate and the proof are both genuine.

**Why the last two passes could not see it.** N4 was assessed on the equivocation
predicate alone ("§5.2 cannot reach an unchained event at all"), which is true and
was the point. The ordering property that `chain_scope = 1` was *also* providing
went with it, and §3.2's row was not re-read. T55's widening then made the
unordered event admissible from nineteen more phases.

**Correction.** The engine needs a rule fixing where an unchained event is applied
in the order. The natural one, and the one consistent with §5.2's "verification
needs no table state": **an `EquivocationProof` is applied at the earliest stage
boundary at or after the later of its two events' `sequence`**, which every peer
derives from the proof itself, since both embedded events carry the same
`sequence`. That is a pure function of the proof's own bytes, it needs no table
state, and it makes T55 deterministic. Whatever rule is chosen, §3.2's row must
stop claiming a total order it does not deliver, and §4.10 needs a precedence rule
between `cause = 5` and any other cause competing for one terminal stage.

## P3 — a `HAND_ABORT` with `attributed = []` requires the silent seat's own emission, so the path D-009 rule 2 routes every below-floor stall through cannot complete

**Severity: high, consensus-critical, and it is a live contradiction between two
documents rather than an omission.** `STATE_MACHINE.md` records it honestly as an
open objection, which is the right conduct and does not make it less blocking.

`PROTOCOL.md` §4.10 (l. 2019–2023):

> "Where `attributed` is empty — `cause = 4`, and `cause = 1` on the §8.4
> `hand_deadline_ms` path — **the set is the whole `HAND_INIT` set**, which is
> what those two cases want. The set is never empty."

§3.2 (l. 708, 751): "A fixed set `R` of seats each emit exactly one event, and the
stage is complete only when every seat in `R` has been heard. … A stage does not
advance until it is complete."

The `hand_deadline_ms` path is reached **because a seat is silent**. So `R`
contains the seat whose silence is the reason for the abort, and the abort stage
cannot complete. `STATE_MACHINE.md` §4.1 (l. 586–590) makes that fatal rather than
cosmetic:

> "The protocol layer raises `HandDeadlineAbort` **when, and only when, the
> collective `HAND_ABORT{cause = 1, attributed = [], cert_hash = None}` stage of
> `PROTOCOL.md` §4.10 has completed over its required emitter set.**"

So under `PROTOCOL.md`'s reading, T57 never fires, the hand never ends,
`TERMINAL(k)` is never defined, and no further hand is dealt. **This is the
deadlock D-009 rule 2 was chosen in preference to a one-signature hand-void, and
it lands one layer below where anyone looked.** It applies to every heads-up
stall, which §9.5 says is the regime the MVP actually ships in.

`STATE_MACHINE.md` §5.2 (l. 995–1006) and §13 (l. 2530–2542) name the defect and
state a default:

> "**Named default:** the engine treats the stage as complete over the `HAND_INIT`
> set minus `attributed` **minus every seat the stalled stage still owes an
> artefact from**, which is exactly the `owed` list the `AbortRecord` carries and
> which every peer derives identically from the same accepted events. … Whether
> §4.10's cell is restated that way is `PROTOCOL.md`'s to settle."

**The named default is not yet safe either, and this is the part the objection
does not say.** The claim "every peer derives identically" is false in precisely
the situation the default is for. The stalled stage is stalled because peers
disagree about what arrived: a peer that accepted `X`'s late artefact computes
`owed` without `X`; a peer that did not computes `owed` with `X`. That is not a
cosmetic difference, because §3.2's collective `stage_hash` is taken **over the
seat set `R` itself**:

```
stage_hash(s) = h("p2p-poker v1 stage",
                  [ u64_be(s), u16_be(stage_type),
                    for each seat s_i in R in ascending seat index:
                        u8(s_i) || event_hash(s_i) ])
```

Two peers with different `R` produce different `stage_hash` for the terminal stage
even when every body agrees, so `TERMINAL(k)` forks and the next hand's
`GENESIS(k+1)` differs. The default converts a deadlock into a chain fork.

**Correction.** `R` for an unattributed abort must be a function of state that is
already agreed, not of what each peer accepted at the stage that stalled. Two
workable shapes:

1. **`R` = the `HAND_INIT` set minus every seat that emitted nothing at the
   stalled stage `s`, where "emitted nothing" is read off `stage_hash(s−1)`'s
   successors as they stand in the chain** — i.e. derived from chain content, not
   from local acceptance. This is checkable offline by §8.5's verifier, which is
   the property the certificate path already has.
2. **Make the unattributed abort a single-writer stage.** Its body is derived and
   every field is recomputable, so §3.2's stage-kind principle argues against it —
   but the principle's reason is that a single writer holds a veto over an
   automatic transition, and here every peer emits its own and the *first* one to
   verify closes the hand. That is a genuine change to §3.2 and needs its own
   ruling; it is listed because option 1 may not survive contact with a partition.

Whichever is taken, `PROTOCOL.md` §4.10's cell and its "which is what those two
cases want" sentence must be restated, and `cause = 4` needs the same treatment —
a diverged peer is exactly the one that will not sign the abort that names nobody.

## P4 — T8 and T12 consume a `TimeoutCertificate` in the setup chain, which `PROTOCOL.md` §8.4 now forbids in terms

**Severity: high.** This is N9(b)'s defect, one section over, created by the
sentence that closed N9(b).

`PROTOCOL.md` §8.4's new normative box (l. 3231–3234):

> "**There is no join-deadline certificate and no `DeadlineKind::Join`.**
> `TIMEOUT_VOTE` and `TIMEOUT_CERT` are defined only over stages of a hand chain
> and are **never emitted in the setup chain (`hand_id = 0`)**."

`PROTOCOL.md` §3.1 (l. 631): "`hand_id = 0` is the *setup chain*, covering
everything from `JOIN_REQUEST` to `TABLE_READY` **plus the seating beacon**."
`STATE_MACHINE.md` agrees, in the paragraph immediately below T4 (l. 840):
"`PROTOCOL.md` §2 puts the seating beacon in that same setup chain, `hand_id = 0`."

And yet:

> `STATE_MACHINE.md` T8 (l. 855): "| T8 | `AwaitingSeatRngCommit` |
> `TimeoutCertificate{Crypto}` | subject has not committed ∧ `|V| ≥ 2`, where `V`
> = the seated seats minus the subject (D-008) | `Seating` | subject unseated;
> `Fault{NoRngCommit}` |"
> T12 (l. 860), identically, for `AwaitingSeatRngReveal`.

`RNG_COMMIT` and `RNG_REVEAL` are "collective stage 1/2 of the setup chain"
(`PROTOCOL.md` §4.4 l. 1279, 1314). So T8 and T12 are triggered by a wire message
that `PROTOCOL.md` says is never emitted where they live — the engine's alphabet
again containing a variant no legal message can produce, which is the exact
condition §8.4's box was written to eliminate. `STATE_MACHINE.md` even reasons
*from* those rows two paragraphs later (l. 836–839): "A table that stalls in
`AwaitingSeatRngCommit` or `AwaitingSeatRngReveal` **below the floor (T8, T12,
where the certificate is inert)** …", treating them as live rows that merely have
a floor.

**What is lost if `PROTOCOL.md` is right**, and it is not stated anywhere: a seat
that commits and never reveals can no longer be unseated. §8.4's own beacon
paragraph, `STATE_MACHINE.md` l. 862–865, says "At `|V| < 2` the certificate is rejected, the beacon
stalls, and the table closes by the §4.3 abandonment timer (T4)" — under the
ruling, that is the outcome at **every** `|V|`, so one seat can close any forming
table at will and the `Fault{NoRngCommit}` / `Fault{NoRngReveal}` records never
exist. That may be the right answer (T4 names nobody and no chips exist yet,
which is the same argument that makes `FormationAbandoned` sound), but it is a
behavioural change nobody chose.

**Correction**, one of: extend §8.4's box to say the beacon is the one setup-chain
stage a certificate may close, and give `TIMEOUT_VOTE` a defined envelope at
`hand_id = 0` — which reopens the sentinel question §2.3 settled; **or** delete T8
and T12, widen T4 to cover the beacon (§5.2's named default already does), and
record that a stalled beacon closes the table with no attribution. The second is
smaller and matches the ruling already made; it needs the `Fault` variants marked
unreachable rather than left declared.

## P5 — `Diverged` is a freeze with no liveness exit, and T57 is explicitly inadmissible in it

**Severity: medium-high.** Chips are committed and the phase has no timer.

`Diverged` (phase 20) is left only by T53 or T54, and both guards require
**every required signer** to have re-emitted (l. 1052–1053). T57 is explicitly
excluded (l. 964: "**not** `Diverged`"), with the reason given at l. 991: "a
hand-deadline abort must not race with reconciliation." No other row consumes any
event in `Diverged` except T52, which records a `Dispute` and stays frozen.

So if any required signer never re-emits — it crashed, it is partitioned, or it is
the peer that faulted the table on purpose and has no reason to co-operate — the
table sits in `Diverged` forever with `Σ committed_hand` locked and no transition
available. `PROTOCOL.md` §6.3 step 1 confirms there is nothing else: "it stops
accepting and stops emitting hand events. No card opens, no action is applied, no
chips move."

This is not the same limitation as §12's "liveness is not owed". That paragraph
bounds a stall at `hand_deadline_ms` per hand — "below the floor a stall costs
`hand_deadline_ms` per hand and can be repeated every hand" — which presumes the
hand *ends*. In `Diverged` it does not end at all, and X29's own bound ("it costs
the attacker the table") assumes the table closes, which requires reaching T54.

**Correction.** `Diverged` needs a bounded exit. The obvious one is that the
disarmed `hand_deadline_ms` is not disarmed on entry to `Diverged`: on expiry the
table takes T54's outcome — `cause = 4`, `attributed = []`, restoration, table
faulted — which is where an unresolved reconciliation ends anyway, so nothing is
decided by the timer that is not already decided by the phase. The reason given
for excluding T57 (the abort "must not race with reconciliation") is satisfied by
using T54's disposition rather than T57's, and by making the timer long enough
that a real reconciliation finishes first. It also needs P3's answer, since the
`cause = 4` abort is a collective stage over the whole `HAND_INIT` set and the
diverged peer is the one least likely to sign it.

## P6 — §6.3 case (b) specifies an abort shape §4.10 says does not exist, and no transition consumes it

**Severity: medium.**

`PROTOCOL.md` §6.3 case (b) (l. 2578–2588):

> "**(b) The transcripts differ, but only because one peer is missing events it
> cannot obtain** — every holder refuses to serve them. **The hand aborts with
> `cause = 1` and `attributed = []`.**"

Against §4.10, two rules away:

> l. 2032, `n(2) cert_hash`: "**required for `cause = 1`**, with the **single
> exception** of the §8.4 `hand_deadline_ms` path, where unanimity was never
> reached and no certificate exists"
> l. 2054: "An abort with `attributed = []` returns exactly `committed_hand[s]` to
> every seat `s` and moves no chips between seats. **There are exactly two such
> cases** and each is a consequence of a ruling that could not name a culprit
> honestly" — and the table below names `cause = 4` and the `hand_deadline_ms`
> path only.

Case (b) is a third such case. It has no certificate, so `cert_hash` cannot be
filled and the event cannot be well-formed; and it is not in the enumeration that
calls itself exhaustive. `STATE_MACHINE.md` has no row for it either: T53 and T54
are the only exits from `Diverged`, and T54 produces `StateDivergence` /
`cause = 4`, not `cause = 1`. An implementer reading §6.3 finds a terminal outcome
the engine cannot represent — the defect class C-6 raised for `RevealRejected` and
`StateAck`, and N5 for `EquivocationProof`, in a fourth place.

Note that case (b) is also the case **OQ-B** exists for ("`PROTOCOL.md` §6.3 case
(b) currently depends on exactly that circularity"), so it cannot simply be
deleted; it needs a well-formed abort shape and a transition.

**Correction.** Either fold case (b) into `cause = 4` — it is a divergence nobody
can attribute, which is what `cause = 4` means, and OQ-B keeps the open question
alive independently — or add it as a third row to §4.10's `attributed = []` table
with `cert_hash = None` permitted, and give `STATE_MACHINE.md` the transition.

## P7 — an absent seat that posts a blind is eligible to win the pot it posted into, so the deviation §12 states is unimplemented

**Severity: medium.** Found by the chip-conservation sweep. No chips are
destroyed; the settlement is undefined.

`STATE_MACHINE.md` §7.5 (l. 1435, 1445):

```
fn build_pots(committed: &[Chips], folded: &[bool]) -> …
    let eligible = contributors.iter().copied().filter(|&s| !folded[s]).collect();
```

`dealt_in` is not a parameter and not a filter. §5.3 step 4 (l. 1188–1189) leaves
`Absent`, `SittingOut` and `Busted` seats with `dealt_in = false`; step 5 sets
`folded := false` for **every** seat; steps 6 and 7 have `Absent` and
`SittingOut` seats with chips post antes and blinds. So in any hand with three or
more dealt-in seats and an absent seat in the blinds, that seat is a contributor
with `folded == false` and lands in `eligible` for the main pot.

By **I20** (l. 2267) it has `revealed_hole == None` and holds no deal-map index,
so at T45 ("`build_pots` (A7), evaluate, award") the evaluator is asked for a hand
that does not exist. Neither **I7** ("`eligible ⊆ contributors ∧ eligible ∩ folded
== ∅ ∧ eligible ≠ ∅`") nor **I8** ("no folded winner") catches it, because the
seat is not folded.

And `STATE_MACHINE.md` §12 (l. 2401) states the opposite as a **deviation it has
implemented**:

> "An absent seat cannot win the blind it posts. This is a **deliberate,
> documented deviation from TDA rules** (D-005)"

`THREAT_MODEL.md` §9.1.1 row 4 carries the same deviation. Nothing implements it.
§5.3 step 9's `|dealt_in| == 1` branch handles the drain case ad hoc — "the single
dealt-in seat wins every posted blind and ante" — which is presumably where the
belief that the general case was covered came from; it is not.

**Correction.** One line: `eligible` filters on `dealt_in[s] && !folded[s]`, with
`build_pots` taking `dealt_in` as a third argument. **I7** then gains
`eligible ∩ ¬dealt_in == ∅`, which is the assertion that would have caught it. The
chip arithmetic is unchanged — a non-eligible contributor's chips stay in the pot,
which is what the deviation intends and what `Σ pot.size + Σ refunds ==
Σ committed_hand` (I6) already requires.

## P8 — cash mode's seat entry has no event, no transition and no message type

**Severity: medium.** It is the only thing that can move `ledger_in` after the
first hand, which is the whole of cash mode's difference from tournament mode.

`STATE_MACHINE.md` §5.3 step 0 (l. 1172–1177): "**Every accepted seat entry** adds
its buy-in: `stack := buyin`, `ledger_in += buyin`, `status := Active` (§9.4; in
tournament mode no entry occurs after the first hand)."

But `Event::PlayerSeated` (§4.1 l. 516) is consumed by exactly two rows, **T1** and
**T2** (l. 808–809), both in `Seating`. There is no row for it in `HandComplete` or
`Paused`, where T58/T59 handle the other three seat events, and §5.2's own rule is
"A transition not listed does not exist; any event arriving in a state with no
matching row is a `Rejection`". So after the table leaves `Seating`, no seat entry
can ever be accepted, step 0's first clause is unreachable, and I1's cash-mode
right-hand side can only ever decrease.

§9.4 (l. 2142) compounds it by citing a message type that does not exist:

> "`PLAYER_SEAT` and `PLAYER_LEAVE` are hand-boundary stages and only that
> (`PROTOCOL.md` §4.10)"

`PROTOCOL.md` §4.11's table has 39 rows and contains no `PLAYER_SEAT`; §4.10
declares `PLAYER_SIT_OUT` (`0x0803`), `PLAYER_SIT_IN` (`0x0804`) and
`PLAYER_LEAVE` (`0x0805`) and nothing else. Mid-session seating currently has only
the join RPC, which is unchained, is gated by the table key, and has no defined
hand-boundary stage.

**Correction.** Either declare cash-mode re-seating out of scope for now — which
is defensible, `RATED_SNG_POKERTH_V1` is a tournament preset and §11's Q5 already
takes that line for rebuys — and say so in §9.4 instead of describing a mechanism;
**or** add the message type to §4.10/§4.11 as a hand-boundary single-writer stage
and the matching `HandComplete | Paused` row to §5.2. The half-state is the
problem: §9.4 and I1 describe a capability the alphabet does not have.

---

# Part 6 — Residuals, recorded not counted

* **R-1 — the `HAND_INIT` stage is covered by no timer.** `hand_deadline_ms` runs
  *from* `HAND_INIT` (`PROTOCOL.md` §8.2 l. 2996), §4.3's abandonment timer covers
  the setup chain only, and §8.4 says in terms that the hand deadline "says nothing
  about a stall in the setup chain". A `HAND_INIT` collective stage that never
  completes — which §9.4 (l. 2151–2153) admits is possible, "a copy that disagrees
  does not complete the stage", when peers hold different accepted T58 events and
  therefore derive different `ledger_delta` — sits between the two timers. This is
  the same uncovered-timer shape `STATE_MACHINE.md` §5.2 patched for the beacon
  with a named default; it has no default.
* **R-2 — `THREAT_MODEL.md` OQ-D's owner column** (l. 1563) lists
  "`docs/DECISIONS.md`" without the "not yet in that document's open list"
  qualifier that its own OQ-A row (l. 1561) and `STATE_MACHINE.md` §11 both carry.
  One-word fix, same family as N8.
* **R-3 — `CONTRIBUTING.md` l. 10–15** still states an authority order that inserts
  itself and `DEPENDENCIES.md` at rank 4. Raised in the last pass as "almost
  certainly right, but an authority claim made by the documents themselves". D-009
  was the opportunity to put it in `DECISIONS.md` and it was not taken.
* **R-4 — `PROTOCOL.md` §5.3's first bullet** describes the `event_class == 0`
  structure as "indexed by `(stage, seat)`" and then explains that "the
  `event_class` axis is what lets a seat hold both its own contribution … and a
  `TIMEOUT_VOTE`". The class axis is now expressed by having separate structures
  rather than by an index component; the sentence is true in effect and loose in
  wording. Cosmetic.

---

# Part 7 — Readiness verdict

**`STATE_MACHINE.md` is not yet implementable without guessing, and the reason has
moved.** Last pass the blocker was a disagreement about an accept predicate (M1).
That is fixed, and fixed well. What blocks now is that three of the paths the
fix *created or widened* have no defined completion:

**Blocking, must be settled before the engine is written:**

1. **P3 — the required emitter set of an unattributed `HAND_ABORT`.** T57 is the
   terminus of every below-floor stall, which after D-009 rule 2 is every
   heads-up stall, which §9.5 says is the MVP's regime. `PROTOCOL.md`'s rule
   deadlocks it; `STATE_MACHINE.md`'s named default forks the chain. Name the
   transition: **T57's trigger cannot be raised**, and `stage_hash` of the
   `HAND_ABORT` stage is not a function of agreed state.
2. **P1 — the envelope of the reconciliation `STATE_HASH`.** Name the transition:
   **T53 and T54**. Their trigger is an event `PROTOCOL.md` does not specify, and
   if it is specified as written it manufactures an `EquivocationProof` against
   the honest peer that took T53's path.
3. **P2 — where an unchained event sits in the total order.** Name the invariant:
   **I22** ("`step(s,e)` is pure … replaying a transcript with effects disabled
   reproduces the identical final state and `STATE_HASH`") is not achievable for
   `Event::EquivocationProof` and `Event::Dispute` as long as §3.2's ordering key
   cannot place them. Name the transition: **T55**.
4. **P4 — whether T8 and T12 exist.** The engine cannot both honour §8.4's box and
   keep two rows triggered by a certificate that box forbids.

**An implementer would additionally have to decide these for themselves:**

5. **P5** — how a table leaves `Diverged` when a required signer never re-emits.
   Name the phase: **20**. There is no exit and no timer.
6. **P6** — what event carries §6.3 case (b), and what `cert_hash` it holds.
7. **P7** — whether `eligible` filters on `dealt_in`. Name the invariant: **I7**,
   which as written permits the state §12 says cannot occur.
8. **P8** — whether a seat may be added after `Seating`. Name the transition: the
   missing `HandComplete | Paused × PlayerSeated` row, and step 0's first clause
   that depends on it.
9. **R-1** — which timer covers a stalled `HAND_INIT`.
10. **OQ-A and OQ-D** have implementable interim answers (restoration, T57/T54)
    and are correctly flagged as interim, so they are not guesses — but neither is
    in `DECISIONS.md`'s open list (N8), so nothing schedules either, and OQ-A now
    governs strictly more of the protocol's behaviour than it did before D-009
    rule 2.
11. **Q3 / Q-02 / OQ-E**, the multi-subject deadline, and **Q-01 / Q1**,
    `showdown_policy`. Both carry named defaults an implementer can build against
    and know are interim. Not guesses.

**What is genuinely ready, and it is a lot.** The 20 phases, 59 transitions and 29
invariants are internally consistent: T1–T59 with no gaps, I1–I29 with no gaps,
20 phase discriminants, and §5.1 states the counts once with the derivation of
each change from 55 → 56 → 59. Every declared `Event` variant now has a consuming
row or an explicit statement that it deliberately has none (`Show`, §4.1
l. 621–628) — the class of defect C-6 raised has been closed four times and the
document now closes each one out loud. §3.1's determinism contract is precise and
the one wall-clock read left in the chain-building path (`CERT_SETTLE_MS`) is
gone. §6's legal-action computation, §7's blinds, button, minimum raise,
incomplete all-in, side pots, odd chips and showdown are specified to the level an
implementer can code against, with TDA citations attached. §8.6's forfeiture
formula conserves chips on every branch and I1's ledger form is correct in both
modes. §2.6's canonical / `LocalView` split is load-bearing and correct. §10's
invariants are written as assertions, and I29 comes with the right warning that
random legal play never reaches it.

None of the four blockers is hard. Three of them are one editorial decision each
plus its consequences — where the re-emission's envelope lives, what `R` is for an
unattributed abort, and where an unchained event sits in the order. The fourth is
a two-row deletion. What they have in common is that each is a **completion** of a
path that D-009 created, and each was left to the next document to settle.

---

# Part 8 — What must happen before Phase 2

Ordered by what endangers the most.

1. **P1 — give the reconciliation `STATE_HASH` an envelope that is not the
   checkpoint's slot**, and make `PROTOCOL.md` §6.3 step 3 specify the emission
   that `STATE_MACHINE.md` T53/T54 consume. Until then the divergence-recovery
   happy path signs an `EquivocationProof` against the honest peer that recovered,
   and T55 plus §8.6 take that peer's chips. Also correct §4.11's per-group check
   table, which certified this row as clean.
2. **P3 — settle the required emitter set of a `HAND_ABORT` with
   `attributed = []`**, in `PROTOCOL.md` §4.10, on state every peer already agrees
   on. Without it the terminus of every heads-up stall either deadlocks or forks
   the chain, and `TERMINAL(k)` is undefined either way.
3. **P2 — give unchained events a defined position in the engine's total order**,
   and a precedence rule between `cause = 5` and any competing terminal cause.
   Correct §3.2's row, which claims a property it no longer delivers.
4. **P4 — delete T8 and T12, or carve the beacon out of §8.4's box.** Two rows,
   one ruling, and record what a stalled beacon now does.
5. **P5 — give `Diverged` a bounded exit**, with T54's disposition rather than
   T57's.
6. **P6 — give §6.3 case (b) a well-formed abort and a transition**, or fold it
   into `cause = 4`.
7. **P7 — `eligible` filters on `dealt_in`**, and I7 gains the clause that would
   have caught it.
8. **P8 — decide whether a seat may be added after `Seating`**, and either
   implement it or stop describing it in §9.4 and I1.
9. **M3 — `PROTOCOL.md` §4.4 l. 1299** must drop "`SmallRng` is absent", which
   `CRYPTOGRAPHY.md` §7.2 already instructs and `THREAT_MODEL.md` A7 already did;
   `research/CRYPTO_LIBS.md` §7.3 gains the 0.9.5 line.
10. **N8 — `DECISIONS.md`'s open list.** Add OQ-A and OQ-D, delete the settled N4
    row, restate the OQ-F row on the question rather than on the withdrawn text.
    Three passes have now recorded this and no edit has been made.
11. **R-1 to R-4**, in the same edit as whatever they sit next to.

Nothing in this verification reverses D-009 or any ruling of
`research/PHASE0_FIXPLAN.md`. D-009's three rules are the right rules and all
three were applied faithfully at the sites the last pass named: rule 1 closed M2
in the canonical predicate rather than at the consumer, rule 2 closed M1 in the
document that was wrong rather than by compromise, and rule 3 replaced an absence
with a test that was seen to fail. What the pass did not reach is, once again, the
second-order consequence of its own change — rule 2 routed every below-floor stall
through a collective stage whose emitter set nobody re-derived (P3), and giving
T53/T54 a real trigger introduced the corpus's only required re-emission with
changed content, against a predicate that had just been sharpened to catch exactly
that (P1). That is the fourth pass in a row to find a worse defect than the pass
before it, and the pattern is stable enough to name: **the defect is never in the
rule; it is in the path the rule newly makes load-bearing.**
