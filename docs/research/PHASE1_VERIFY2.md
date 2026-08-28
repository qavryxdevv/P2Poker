# PHASE1_VERIFY2.md — closing verification of Phase 1

**Scope.** The one regression (A-8), the five partials (B-2, B-3, C-6, D-2, D-6),
the nine defects `PHASE1_VERIFY.md` found (N1–N9), and the application of **D-008**
across `THREAT_MODEL.md`, `PROTOCOL.md`, `CRYPTOGRAPHY.md`, `NETWORK_STACK.md`,
`STATE_MACHINE.md`, plus `DECISIONS.md`, the new `DEPENDENCIES.md` and
`CONTRIBUTING.md`, and `research/CRYPTO_LIBS.md`. Then six independent sweeps.

**Method.** Every crate claim a verdict turns on was re-read in the unpacked source
under `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`. Where
a document and the crate disagree, the crate wins and the line numbers are given.
No specification document was edited.

## Verdict counts

| Verdict | Count | Items |
|---|---:|---|
| **RESOLVED** | 11 | A-8, B-2, B-3, C-6, D-2, D-6, N1, N3, N4, N5, N6 |
| **PARTIAL** | 5 | N2, N7, N8, N9, D-008 application |
| **UNRESOLVED** | 0 | — |
| **REGRESSED** | 0 | — |

Five defects that no earlier finding named are recorded as **M1 … M5**. **M1** and
**M2** are more severe than anything the previous pass raised: M2 lets a passive
attacker forfeit an honest player's committed chips using only behaviour the
protocol itself describes as normal, and M1 is a consensus-critical disagreement
between the two documents an implementer would read side by side.

---

# Part 1 — New defects

Ordered by severity.

## M2 — two simultaneous timeout subjects at one stage make an honest voter's own votes an `EquivocationProof` against it, and T55 then forfeits that voter's chips

**Severity: highest.** This is N4's defect class — a predicate that mandatory or
routine honest behaviour satisfies — surviving in `TIMEOUT_VOTE`, the message type
D-008's rewrite made it reachable through. Unlike N4 it is not merely a
false-positive: it moves chips.

**The slot.** `PROTOCOL.md` §5.2 (l. 2103–2110) is canonical:

> "Peer `K` equivocates when two `SignedEvent`s `E1 != E2` exist such that … both
> carry `chain_scope == 1`, their bodies agree on all six of
> `(protocol_version, table_id, hand_id, sequence, event_class,
> sender_public_key == K)` … and `event_hash(E1) != event_hash(E2)`."

and it justifies itself on a capacity argument:

> "a sender emits at most one event per stage per class, so
> `(sender, table_id, hand_id, sequence, event_class)` is a slot with capacity
> one for `chain_scope == 1`, and two distinct bodies in one slot is a
> contradiction the sender created and signed."

`PROTOCOL.md` §5.3 (l. 2228–2230) encodes exactly that capacity: "one
`Vec<Option<event_hash>>` indexed by `(stage, seat, event_class)`". One slot, one
`Option`.

**The message that overflows it.** `PROTOCOL.md` §4.8 (l. 1755–1767) gives
`TIMEOUT_VOTE` `chain_scope = 1`, `event_class = 1`, and

> "*Envelope:* `sequence = subject_sequence`"

with the subject seat carried in the **body** (`n(1) subject_seat`). So *which seat
a vote is about does not change its slot*. Two votes by one voter about one stage
are two distinct bodies in one slot — §5.2's predicate satisfied exactly.

**When that happens is a state the corpus says is normal.** `PROTOCOL.md` §8.4
(l. 2931–2946), as rewritten for D-008:

> "If two seats become subjects at once, neither certificate can reach unanimity,
> because each required voter set contains the other subject."
> "**Two simultaneous subjects therefore deadlock, by design.**"

For two seats to *be* subjects at one stage, votes must exist against both. Every
crypto stage is collective (`DECK_INIT`, `DECK_COMMIT`, `DEAL_PRIVATE`,
`BOARD_REVEAL`, `SHOWDOWN_REVEAL` — §4.11), so two seats failing at one stage is
the ordinary shape of the case, and §8.4 spends a section on it. An honest voter
that does the obvious thing — vote against each seat that owed it something — has
signed two bodies in one slot.

**Concrete attack, and it needs no modified voter.** Mallory seats two identities
at one table (`PROTOCOL.md` §11.3: "**Sybil and multi-accounting.** Keys are
free"). Both go silent at one collective crypto stage `s`. Honest Bob's timers
expire and he emits `TIMEOUT_VOTE{subject_seat = M1, sequence = s}` and
`TIMEOUT_VOTE{subject_seat = M2, sequence = s}`. Mallory holds both, builds an
`EquivocationProof{accused = Bob, event_a, event_b}` — which `PROTOCOL.md` §5.2
says verifies with "No table state, no transcript, no knowledge of the game" —
and files it. `STATE_MACHINE.md` T55 (l. 820), widened in this very pass:

> "| T55 | **any phase except `TableClosed`**, when a hand is live … |
> `EquivocationProof` | the proof verifies at the protocol layer ∧ it names the
> current `hand_id` | `HandAborted` | `Fault{Equivocation}`;
> `AbortRecord{kind: Equivocation, attributed: [accused]}`; **the §8.6 forfeiture
> formula applies**"

Bob's committed chips are forfeited and distributed to the remaining seats — two
of which are Mallory's — and `PROTOCOL.md` §5.2 adds Bob to
`libp2p::allow_block_list`. Repeatable every hand, against a different honest seat
each time.

**And the alternative reading kills the machinery instead.** §4.8's emitter gate
and its receiver check state different conditions:

> l. 1755: "*Legal:* only when the local monotonic timer for the subject stage has
> expired (§8.2) **and** this client has not accepted any valid event for that
> stage."
> l. 1781: "*Receiver must validate:* … the receiver has not itself accepted an
> event for that stage **from `subject_seat`**."

Read the emitter gate literally — "any valid event for that stage" — and no
`kind = 2` vote is ever legal at a collective stage, because at a collective stage
every peer has accepted several events for it. That makes the entire
cryptographic-step deadline path dead, and §8.4's simultaneous-failure analysis
describes a state that can never arise. Read it as the receiver check does
("from `subject_seat`") and M2 is live. Exactly one of the two must be normative
and the documents do not say which. This is **M5**, recorded separately below,
because it is a defect in its own right.

**Why the D-008 pass is where this surfaced.** The deleted exclusion rule
previously removed an already-subject seat from `V`, which at least gestured at a
one-subject-at-a-time discipline. D-008 deletes it and replaces it with "Two
simultaneous subjects therefore deadlock, by design" — so the double-subject stage
is now the specified outcome rather than an edge the exclusion rule swept away.
The rescoping did not create the slot collision; it made it the normal path.

**Correction**, one of:

1. Give `TIMEOUT_VOTE` and `TIMEOUT_CERT` `chain_scope = 0` with §2.3's sentinels
   and their own anti-replay — the disposition N4 took for `DISPUTE`, and the
   votes already name their stage in the body (`n(0) subject_sequence`,
   `n(3) parent_event_hash`). §5.3's array then needs a per-`(sender, stage)` vote
   bound in place of the `event_class` axis.
2. Or make the vote's slot include its subject: index §5.3's array by
   `(stage, seat, event_class, subject_seat)` and add `subject_seat` to §5.2's
   six-tuple for `event_class == 1`. This is a change to the canonical
   equivocation predicate and must be made in one place and quoted everywhere.
3. Or make it normative that a voter emits **at most one** vote per stage, with a
   deterministic subject-selection rule (lowest failing seat index). This is the
   cheapest edit but it must then be stated in §4.8, and §8.4's
   "neither certificate can reach unanimity" argument must be re-derived under it.

Whichever is taken, `STATE_MACHINE.md` §8.6 l. 1590 must lose its present
justification:

> "Two attribution paths remain that do **not** depend on `V` at all, and both are
> correct that way: `AbortKind::Equivocation` (T55), whose evidence is a
> self-contained proof signed under the accused's own key rather than anybody's
> claim about what they saw"

That sentence is true only while the predicate is unsatisfiable by honest
behaviour. Until M2 is closed the equivocation path needs a floor of its own, and
`SPEC_CS.md` §36 forbids carrying the claim in the meantime.

---

## M1 — `PROTOCOL.md` and `STATE_MACHINE.md` disagree on what a `kind = 2` certificate does below D-008's floor; the engine's reading violates D-008 point 2 and hands a losing player an on-demand hand-void

**Severity: high.** Two conforming implementations diverge on the same input event.

`PROTOCOL.md` §8.3, the normative statement, l. 2853–2855 and the effect table at
l. 2906:

> "* **`kind = 2` with `|V| < 2`.** Inert, like any other certificate below the
> floor. **The hand does not end here**; it ends at `hand_deadline_ms` under §8.4"
>
> "| either kind, `\|V\| < 2` | **no effect.** Not an error, not evidence, **not
> chained.** …|"

`STATE_MACHINE.md` §8.4 validity rule 6, l. 1366–1372:

> "6. **if `|V(subject)| < 2`**, a certificate with `kind == Action` … is
> **rejected**, and a certificate with `kind == Crypto` … is **accepted** only if
> it names no subject for attribution … and a `kind == Crypto` certificate
> accepted under this rule produces `AbortRecord{kind: HandDeadline,
> attributed: []}`"

and T16 (l. 728), T22 (l. 744), T27 (l. 754), T41 (l. 786), T44 (l. 794) each
carry the branch "`|V(subject)| < 2`: `AbortRecord{kind: HandDeadline,
attributed: []}` … restoration (§8.6)" with `HandAborted` as the next phase.
Invariant **I29(b)** codifies the same carve-out.

One document says the event is inert and unchained; the other says it is accepted
and terminates the hand. This is not a wording difference — it is the engine's
accept/reject decision on a signed event, which is precisely what
`STATE_HASH` compares. A peer implementing `PROTOCOL.md` stays in
`AwaitingDeal`; a peer implementing `STATE_MACHINE.md` is in `HandAborted`. The
next checkpoint produces two `state_hash` values, §6.3 case (c) fires, and the
table is faulted with `cause = 4` — the outcome X29 describes as unfixed.

**D-008 point 2 is unambiguous and `STATE_MACHINE.md` does not follow it:**

> "**A certificate with `|V| < 2` has no effect.** It is not an error and not
> evidence; the deadline simply stays advisory, exactly as D-007 has it heads-up."

`DECISIONS.md` outranks `research/PHASE0_FIXPLAN.md`, and §0.3 of the fix plan is
what `STATE_MACHINE.md` cites (l. 1405–1420) to keep the carve-out. That is an
evidence document overriding a decision document.

**The disagreement is three-way and one document holds both positions.**

| Where | Position |
|---|---|
| `PROTOCOL.md` §8.3 (l. 2853, 2906) | inert; the hand ends at `hand_deadline_ms` |
| `STATE_MACHINE.md` §8.4 rule 6, T16/T22/T27/T41/T44, I29(b) | accepted; the hand ends now |
| `THREAT_MODEL.md` X10 (l. 779) | "a certificate whose required voter set has fewer than two members has no effect at all — not an error, not evidence, the deadline simply stays advisory" |
| `THREAT_MODEL.md` §7.3(c) (l. 1082) | "A `kind = 2` certificate **may still end the hand**, because otherwise a vanished opponent deadlocks the table forever" |
| `CRYPTOGRAPHY.md` §2.10 (l. 529–534) | "Such a certificate therefore has no effect: the abort … names nobody" — asserts both halves in one sentence |

**What turns on it, contrary to the note that says nothing does.**
`STATE_MACHINE.md` §13 (l. 2061–2063) disposes of the gap:

> "The chip outcome is identical either way (restoration, nobody named, I27), so
> nothing about D-005 or D-008 turns on it."

The chip *arithmetic* is identical. Three other things are not:

1. **Timing and unilateral availability.** Under `PROTOCOL.md` a heads-up player
   escaping a losing pot must actually stall the hand for `hand_deadline_ms` =
   600 000 ms — visible, attributable, and costly to itself. Under
   `STATE_MACHINE.md` rule 6 it emits one self-signed certificate naming the
   opponent as subject and the hand ends at once, with restoration. §8.3 concedes
   the assertion is uncheckable — "There is no artefact that settles the race when
   a voter lies about what it saw" — so nothing establishes the deadline ever
   passed. That converts a ten-minute griefing path into a one-message *escape
   button*, which is the shape of exploit D-005 exists to close.
2. **The transcript record.** An accepted certificate names the opponent as the
   seat that failed. `THREAT_MODEL.md` X30 states as a mitigation that "under
   D-008 an inert certificate is *not evidence of misbehaviour*" — true under
   `PROTOCOL.md`'s reading, false under the engine's, where the certificate is a
   valid accepted chain event naming its victim.
3. **Consensus.** See above.

**Correction.** Apply D-008 point 2 literally in `STATE_MACHINE.md`: rule 6 rejects
below the floor for **both** kinds, T16/T22/T27/T41/T44 lose their `|V| < 2`
branches, I29(b) is restated, and the `HandDeadline` abort gains the certificate-free
carrier that §13 already identifies as missing —
`Event::HandDeadlineExpired` or equivalent, raised by the protocol layer's local
timer, which is what `PROTOCOL.md` §8.4 describes ("a local timer expiry that every
peer reaches from the same signed `HAND_INIT` and the same relative duration").
`THREAT_MODEL.md` §7.3(c) must then be brought into line with its own X10, and
`CRYPTOGRAPHY.md` §2.10's sentence split.

---

## M3 — "`SmallRng` is not compiled in" is false: `rand 0.9.5` enters the tree with `small_rng` and `thread_rng` in its default feature set

This is the B-1 / B-3 defect one turn further on. The absence claim was corrected
from "the `rand` crate is deliberately absent" to "`rand 0.8.8` is present but
`small_rng` is not enabled". The evidence for the new claim covers one of the three
`rand` majors in the tree.

`CRYPTOGRAPHY.md` §7.2, l. 1224–1230, which the document names as the source that
`PROTOCOL.md` §4.4 and `THREAT_MODEL.md` A7 were corrected to match:

> "* `SmallRng` is **not** compiled in: it lives behind rand 0.8's separate
> `small_rng` feature, which nothing in the workspace enables. **Verification: (b)
> source** — `rand-0.8.8/Cargo.toml` `[features]`, `small_rng = []`; **(a)
> executed** — `cargo tree -e features -i rand@0.8.8` … This is now checked across
> *every* consumer of `rand 0.8.8`, not only ark-std"

The headline is unqualified; the evidence is scoped to `rand@0.8.8`.
`PROTOCOL.md` §4.4 l. 1290 and `THREAT_MODEL.md` A7 l. 266 carry the same
unqualified "`SmallRng` is absent".

**Verified in source, (b).** `DEPENDENCIES.md` §5.4 (l. 829–830) and §7 (l. 433)
record `rand 0.9.5 ← hickory-proto, igd-next, yamux`. Of those three:

```
igd-next-0.16.2/Cargo.toml:148   [dependencies.rand]  version = "0.9.0"      # no default-features = false
yamux-0.13.10/Cargo.toml:57      [dependencies.rand]  version = "0.9.0"      # no default-features = false
hickory-proto-0.25.2/Cargo.toml:318  [dependencies.rand] version = "0.9", features = ["alloc","std_rng"], default-features = false
```

and `rand-0.9.5/Cargo.toml`:

```
[features]
default = [ "std", "std_rng", "os_rng", "small_rng", "thread_rng" ]
```

Two of the three consumers take the defaults, features unify additively, so
`rand::rngs::SmallRng` **and** `rand::rng()` are compiled into the integrated tree
at `rand 0.9.5`. `igd-next` is not optional in this build — `DEPENDENCIES.md`
l. 674 records it arriving via `igd-next ← libp2p-upnp ← libp2p` — and
`yamux 0.13.10` is mandatory for the relay (`DEPENDENCIES.md` l. 474).

**What is unaffected.** The conclusion of the section survives intact and is the
reason this is a claim defect rather than a design defect: `CRYPTOGRAPHY.md` §7.2
already rules that "`SPEC_CS.md` §7 is enforced by the CI lint of §12 item 6,
**not** by the dependency graph, and no document may claim a structural
guarantee." M3 is that same ruling not fully applied to its own bullet. The
advisory position is also unchanged — `DEPENDENCIES.md` l. 433 gives 0.9.5 "no
open advisory at 0.9.x", which is correct.

**Correction.** In all three places, replace the absence with the true, narrower
statement: *`small_rng` is not enabled on `rand 0.8.8`; it **is** enabled on
`rand 0.9.5`, whose default features `igd-next` and `yamux` take, so `SmallRng`
and `rand::rng()` are in the build and §7 is enforced by the §12 item 6 lint
alone.* The lint's disallowed-symbol list is already right; only the justification
is wrong. `CRYPTO_LIBS.md` §7.3 (l. 189–193) carries the same 0.8-only check and
should gain the 0.9 line.

---

## M4 — `PLAYER_LEAVE` is described as unchained in §4.10 and as `chain_scope = 1` in §2.3 and §4.11

`PROTOCOL.md` §4.10, l. 2008–2010:

> "**`0x0805 PLAYER_LEAVE`** — single-writer stage at a hand boundary, **or
> accepted at any time as a courtesy notice that is not chained.**"

against §2.3's list, which the document calls exhaustive:

> "Unchained message types, exhaustively: `HELLO`, `CAPABILITIES`, `JOIN_REQUEST`,
> … `LOBBY_CHAT`, **`DISPUTE`**. Everything else is chained."

and §4.11 l. 2060, which gives `0x0805 PLAYER_LEAVE` `chain_scope = 1`,
"single", "the seat" — under a heading that says in terms:

> "**`DISPUTE` is the one table-mesh message with `chain_scope = 0`,** and this
> column is the normative statement of it".

So a mid-hand courtesy `PLAYER_LEAVE` has no defined envelope. With
`chain_scope = 0` it is an unchained type absent from an exhaustive list and must
be rejected; with `chain_scope = 1` it occupies a stage slot outside its stage,
and a peer that sends a courtesy notice mid-hand and a real `PLAYER_LEAVE` at the
boundary risks the §5.2 collision N4 closed for `DISPUTE`. This is the same defect
class as N4 and M2, in the third message type that claims to be legal outside its
stage.

**Correction.** Either delete the courtesy clause — §4.10 already says "A leave is
never *required*, because a client that vanishes must be handled identically", so
the clause buys nothing — or add `PLAYER_LEAVE` to §2.3's exhaustive list and to
§4.11 as a second `chain_scope = 0` row, and correct §4.11's "the one table-mesh
message" sentence with it.

---

## M5 — `TIMEOUT_VOTE`'s emitter legality and its receiver check state different conditions

`PROTOCOL.md` §4.8:

> l. 1755, *Legal:* "only when the local monotonic timer for the subject stage has
> expired (§8.2) **and** this client has not accepted any valid event for that
> stage."
> l. 1781, *Receiver must validate:* "… the receiver has not itself accepted an
> event for that stage **from `subject_seat`**."

At a collective stage the emitter condition is "no event at all from anyone" and
the receiver condition is "no event from the subject". Every cryptographic stage
is collective (§4.11), so under the emitter reading the whole `kind = 2` deadline
path is unreachable and §8.4's simultaneous-failure section describes an
impossible state; under the receiver reading it is reachable and M2 follows. One
of the two sentences is wrong and nothing in the corpus says which. Recorded
separately from M2 because it must be fixed even if M2 is closed another way.

---

# Part 2 — Per-item verdicts

## A-8 / N1 — the relay byte budget — **RESOLVED**

The bidirectional correction is applied in every place `PHASE1_VERIFY.md` listed,
and re-verified against the crate.

**Crate, (b) source**, `libp2p-relay-0.21.1/src/copy_future.rs`:

```rust
41-48:  pub(crate) struct CopyFuture<S, D> { src, dst, max_circuit_duration: Delay,
                                             max_circuit_bytes: u64, bytes_sent: u64 }
78:     if this.max_circuit_bytes > 0 && this.bytes_sent > this.max_circuit_bytes { … }
88-96:  let src_status = match forward_data(&mut this.src, &mut this.dst, cx) { … this.bytes_sent += i … };
98-106: let dst_status = match forward_data(&mut this.dst, &mut this.src, cx) { … this.bytes_sent += i … };
241:    assert!(a.len() + b.len() > max_circuit_bytes as usize);   // the crate's own quickcheck
```

Line 241 is decisive and `NETWORK_STACK.md` §16.1 is right to cite it: the crate's
own property test asserts the two directions **summed** against one cap.
`src/behaviour.rs:163` confirms `max_circuit_bytes: 1 << 17`.

Every document now carries the corrected figures and none says "per direction"
except to record its withdrawal:

> `NETWORK_STACK.md` §16.1 (l. 2047–2049): "**Status: objection upheld, correction
> applied.** §9.5 no longer carries the per-direction figure."
> `CRYPTOGRAPHY.md` §6.5 (l. 1068–1070): "Shuffle traffic over **one circuit**,
> **both directions together**, per hand | **17 958 B** … | Hands per circuit
> against a 131 072 B public-relay budget, shuffle only | **~7** |
> `131 072 / 17 958 = 7.30`"
> `PROTOCOL.md` §9.3 (l. 3112–3117): "**`max_circuit_bytes` is a single
> bidirectional total for the whole circuit.** … 131 072 is therefore 128 KiB of
> **combined** traffic, not 128 KiB each way"
> `THREAT_MODEL.md` X20 (l. 788): "`max_circuit_bytes = 1 << 17` is per circuit and
> **bidirectional** … roughly 7 hands of shuffle traffic, or about 5 hands
> including the signed event stream. An earlier revision gave these as ~14 and ~10"
> `THREAT_MODEL.md` §3.5 (l. 140, 526): "**128 KiB per circuit counted over both**"
> … "(These figures were previously stated per direction and were)"

The measurement questions were restated as bidirectional, which is the half that
protects the person who runs them:

> `THREAT_MODEL.md` OQ12 (l. 1372): "**The measurement must be bidirectional**,
> because `max_circuit_bytes` is one budget for the whole circuit … measuring one
> direction reports twice the headroom there is."
> `NETWORK_STACK.md` OQ-7 (l. 1977): "compare it against `Limit::data_in_bytes()`,
> which is the same bidirectional figure."

The kubo caveat is carried in all four places the figure appears
(`PROTOCOL.md` l. 3120–3121, `CRYPTOGRAPHY.md` l. 1083, `THREAT_MODEL.md` l. 529,
`NETWORK_STACK.md` l. 1373, §16.1 l. 2100–2105), with the right disposition: the
Rust behaviour binds our own relay, the stricter reading is assumed for
third-party relays, and no claim is made about the Go source.

Arithmetic checked: `5 547 + 3 432 = 8 979`; `2 × 8 979 = 17 958`;
`131 072 / 17 958 = 7.30`; `7 × 17 958 = 125 706 < 131 072 < 143 664 = 8 × 17 958`;
`131 072 / 26 214 = 5.0`; `(6−1) × 8 979 = 44 895`. All correct.
`STATE_MACHINE.md` §13 item 3 records that the document carries no byte figure at
all and states the absence explicitly rather than leaving it to be re-derived.

## B-2 — `relay::RateLimiter` — **RESOLVED**

The one stale word is gone. `NETWORK_STACK.md` §15 (l. 2028):

> "**D-002** publicly reachable client volunteers as relay, off by default,
> admission via a **named type implementing `libp2p::relay::RateLimiter`** — the
> trait is re-exported at the crate root (`libp2p-relay-0.21.1/src/lib.rs:42`), the
> type holds the live admitted-peer set, and it is installed into **both**
> `reservation_rate_limiters` and `circuit_src_rate_limiters`"

Re-verified: `src/lib.rs:42` is `pub use behaviour::{rate_limiter::RateLimiter, …}`;
`src/behaviour/rate_limiter.rs:38` is `pub trait RateLimiter: Send`, with the
blanket `impl<T: FnMut(…) -> bool + Send> RateLimiter for T` at `:56`. The
citation in the register is byte-accurate.

## B-3 — the stale RustSec `rand` row — **RESOLVED**

`research/CRYPTO_LIBS.md` was corrected, with a correction banner at the head
rather than a silent edit:

> l. 15–20: "The largest correction, because it was stated as an absence and
> absences get copied into manifests: this document claimed `rand` had been
> **dropped entirely** and was absent 'at any depth'. In the integrated tree
> **`rand` is present at three majors — 0.8.8, 0.9.5 and 0.10.2 — and
> `rand = "0.8"` is a direct dependency of `p2p-poker`.** … The absence does not,
> and must not be restated anywhere."

The three specific places the finding named are fixed: l. 63 pairs *Probe* against
*Integrated*; the advisory table at l. 999 now reads "**0.8.8, 0.9.5, 0.10.2** |
RUSTSEC-2026-0097 | No — **but not because the crate is absent**"; §10's version
list carries `rand_chacha` 0.3.1/0.9.0 and `rand_core` 0.6.4/0.9.5/0.10.1; and the
copy-verbatim `Cargo.toml` block (l. 1255–1265) now carries `rand = "0.8"` with a
comment explaining why, in place of "The `rand` crate is deliberately ABSENT".

Residual, low: `PROTOCOL.md` §4.4 (l. 1288) and `THREAT_MODEL.md` A7 (l. 264–266)
still attribute `rand 0.8.8` only to `ark-std 0.5.0` and name only the 0.8 line,
where `CRYPTOGRAPHY.md` §7.2 (l. 1213–1216) and `DEPENDENCIES.md` §7 give the full
three-major provenance. Not a contradiction of fact, but it is the same
incompleteness that produced M3 and it should be levelled in the same edit.

## C-6 / N5 — the dispute path in `STATE_MACHINE.md` — **RESOLVED**

T55 is widened exactly as the finding asked, and the phases with no live hand got
their own row rather than being swept into it:

> T55 (l. 820): "| T55 | **any phase except `TableClosed`**, when a hand is live —
> 2–15 and 20 | `EquivocationProof` | … | `HandAborted` |"
> T56 (l. 821): "| T56 | **any phase except `TableClosed`**, when no hand is live —
> `Seating`, `HandComplete`, `Paused` — **or** any such phase when the proof names
> a hand that has already ended | `EquivocationProof` | … | unchanged |
> `Fault{Equivocation}` naming `accused`; **no abort, no chip movement** — there is
> no live commitment to forfeit"

I13 gained the clause that stops the invariant undoing the widening (l. 1861: "**A
row whose State column is a scope rather than one phase … is read as instantiated
at every phase in that scope**"), and `PROTOCOL.md` §5.2 now carries the normative
sentence the engine reproduces rather than paraphrases. Counts re-derived by hand
and correct: T1–T56 with no gaps, I1–I29 with no gaps, 20 phase discriminants —
matching §5.1's "20 phases, 56 numbered transitions, 29 invariants" and §9.5's
"All 20 phases and 56 transitions".

The one thing T55 now enables that nobody checked is **M2**: with the proof
consumable from nineteen more phases, a proof manufactured out of an honest
voter's own votes reaches the forfeiture formula from anywhere in the hand. That
is a defect in the vote's envelope, not in the widening, and the widening is
correct.

## D-2 — the §28 dependency register — **RESOLVED**

`docs/DEPENDENCIES.md` exists and is the register the ruling designated:

> l. 1–8: "**Status:** authoritative. This document is the permanent home of the
> `SPEC_CS.md` §28 register, assigned by `research/PHASE0_FIXPLAN.md` ruling
> **D-2**. It replaces the two interim halves — `CRYPTOGRAPHY.md` §9.1 … and
> `NETWORK_STACK.md` §5.1.1 … Those two sections stay where they are as the
> *reasoning* for their own crates; this document is the register."

It carries the six columns, the measurement environment and toolchain
(l. 15–18), reproduction commands in §10, and — the part that matters — §9
corrects three research statements rather than inheriting them. Both former halves
point at it (`CRYPTOGRAPHY.md` l. 1433, `NETWORK_STACK.md` l. 627, 663).

## D-6 — §31 Git discipline — **RESOLVED**

`docs/CONTRIBUTING.md` exists and is the target the two pointers were waiting for:

> l. 3–6: "Assigned by `research/PHASE0_FIXPLAN.md` ruling **D-6** as the owner of
> `SPEC_CS.md` §31, created at the close of Phase 1. `CRYPTOGRAPHY.md` §12 and
> `PROTOCOL.md` §9.6 already point here; this is the target those pointers were
> waiting for."

§1 quotes §31's four sentences in full and binds each. Both pointers
(`CRYPTOGRAPHY.md` l. 1837–1838, `PROTOCOL.md` l. 3198–3202) now resolve.

Minor, recorded not counted: `CONTRIBUTING.md` l. 10–15 states an authority order
that inserts itself and `DEPENDENCIES.md` at rank 4, above `docs/research/`. That
is almost certainly right, but it is an authority claim made by the documents
themselves rather than by `SPEC_CS.md` or a numbered decision, and it belongs in
`DECISIONS.md` if it is to bind.

## N2 — "adjudicable offline" — **PARTIAL**

The claim is withdrawn where the finding said it did the most damage, and it was
withdrawn rather than repaired, which is the disposition `SPEC_CS.md` §36 points
at. `PROTOCOL.md` §6.3 case (c) (l. 2386–2391), §6.4 (l. 2442–2450), §11.3
(l. 3363–3368) and `THREAT_MODEL.md` X29 (l. 797) all now carry:

> "the evidence is preserved and is sufficient for a human, or for a future
> adjudicator, to diagnose the divergence; no adjudication procedure is specified"

and X29 explicitly stops counting it as a bound: "this third bound is **evidence
preservation, not recourse**". `PROTOCOL.md` §11.3 correspondingly says "Bounded by
two things and closed by neither". **OQ-F** is opened (`PROTOCOL.md` §12,
l. 3468–3487) and is genuinely in `DECISIONS.md`'s open list (l. 789), which is
what X29 claims.

**Why PARTIAL.** `STATE_MACHINE.md` — one of the five, not an outlying research
file — still asserts the withdrawn sentence twice, in the two places an engine
implementer reads:

> §5.2, T54's note (l. 858–860): "The evidence … is written to the profile
> directory and **is publicly adjudicable offline by anyone who runs the reference
> engine over it.** Live adjudication is not available and is not claimed."
> §12 (l. 1978–1979): "No peer is named … **The evidence is adjudicable offline and
> not live.** Not solved; see OQ-D in §11"

Both are the exact form `PROTOCOL.md` §6.4 records as withdrawn: "A third bound was
previously claimed here and is **withdrawn**: *'the evidence is deterministically
adjudicable offline by any third party running the reference engine over the
unanimous transcript.'* No reference engine exists to run". §12 is the document's
own "what this document does not claim" section, which makes it the worst place for
the claim to survive.

## N3 — the `|V|` collapse — **RESOLVED**

D-008 is issued, binding, and applied. The exclusion rule is deleted, not narrowed:

> `PROTOCOL.md` §8.4 (l. 2934–2952): "**The exclusion rule this section used to
> carry is deleted.** It read: *Rule: a seat already named as the subject of an
> outstanding, older unmet deadline is excluded from `V`.* … D-008 deletes the rule
> and replaces it with §8.3's inductive exclusion: **a seat leaves `V` only once a
> completed, valid certificate already names it as attributed.**"

The replacement is normative and boxed:

> `PROTOCOL.md` §8.3 (l. 2810–2818): "`V(subject)` is the dealt-in seats, minus
> `subject`, minus every seat that a **completed, valid certificate earlier in this
> hand already names as attributed**. Nothing else removes a seat from `V`. …
> **Being voted against is not exclusion.**"

The nominal table is explicitly demoted (l. 2827–2830: "**That table is the nominal
set, not the operative rule.** … A reader who takes `|V| = n - 1` as a rule rather
than as a starting value has reconstructed the defect D-008 exists to close").
`STATE_MACHINE.md` makes the shrinkage *state* rather than belief —
`certified_subjects` (§2.6, §5.3), §8.4 rules 4/6/7, and invariant **I29**, whose
(a) half is the induction and whose (b) half is the floor, with the right note that
"a `proptest` over legal play never reaches it". The verifier can check it offline
(`PROTOCOL.md` §8.5: "including that every seat missing from `V` was removed by a
completed, valid certificate that the same transcript contains"). The wire message
carries the check too (§4.8 `TIMEOUT_CERT` receiver validation: "**every seat absent
from it is absent because a completed, valid certificate in this hand already names
that seat as attributed** — a `V` shrunk by bare votes is not a `V`").
`THREAT_MODEL.md` catalogues the attack as **X30**, D&A, with an honest
"and only because of a rule our own client enforces" and a correctly weak
attribution half.

The residual M1 is about what a certificate does *below* the floor, not about
whether the floor can be manufactured; N3's own attack is closed.

## N4 — `DISPUTE`'s envelope — **RESOLVED**

`DISPUTE` moved to the unchained side, with sentinels, payload-carried scope and
its own bound:

> `PROTOCOL.md` §4.9 (l. 1836–1843): "*Envelope:* **unchained** —
> `chain_scope = 0`, `event_class = 0`, `table_id = ZERO32`,
> `hand_id = 0xFFFF_FFFF_FFFF_FFFF`, `sequence = 0`,
> `previous_event_hash = ZERO32` (§2.3). The table and hand a dispute concerns are
> named by the payload's `n(5)` and `n(6)`, never by the envelope, so that a
> dispute occupies no stage slot and **can never appear in an `EquivocationProof`
> (§5.2)**."

Anti-replay is defined rather than left implicit (l. 1861–1868: idempotent
de-duplication by `event_hash` within `(sender_public_key, table_id, hand_id)`,
capped at `MAX_DISPUTES_PER_SENDER_PER_HAND = 8`), the constant is in
`PROTOCOL.md` §13, §5.3 gives it its own bound with the reason ("`DISPUTE` is
unchained, so it has no slot in the array above and needs its own bound"), and the
three places that must agree do: §2.3's exhaustive list names `DISPUTE`, §4.11's
row gives `chain_scope = 0` with a normative sentence, and §6.3 now states the
consequence positively — "Emitting the second dispute is mandatory, not
optional-and-risky, and an implementation that withholds it to stay safe is wrong."

The `DECISIONS.md` open-list row for N4 is now stale; see N8.

## N5 — see C-6 — **RESOLVED**

## N6 — §11.3's unqualified forfeiture claim — **RESOLVED**

> `PROTOCOL.md` §11.3 bullet 1 (l. 3345–3355): "Whether it costs the quitter
> anything depends entirely on whether a certificate with an effect can form:
> **only where the required voter set satisfies `|V| >= 2` does quitting cost
> exactly what folding would have cost, by D-005's forfeiture rule.** Where
> `|V| < 2` … the quitter recovers its commitment … On the `cause = 4` divergence
> path it costs nothing either."

Both qualifications the finding asked for are present, and it is scoped on `|V|`
rather than on `n`, which is more than the finding asked for.

## N7 — the checkpoint-placement rule — **PARTIAL**

`THREAT_MODEL.md` X29 (l. 797) is corrected, and correctly:

> "no checkpoint is placed after a hole card has been opened **to anyone but its
> owner** (every player opens its own two cards at `DEAL_PRIVATE`, which precedes
> checkpoints 3–7, so the rule is about *public* opening and is stated that way
> rather than in the falsifiable shorter form an earlier revision used)"

**Why PARTIAL.** `PROTOCOL.md` §6.2 — the section that owns the checkpoint table
and states the rule normatively — is unchanged, l. 2308:

> "**No checkpoint is placed after any hole card has been opened.**"

Every player opens its own two hole cards at `DEAL_PRIVATE`, which precedes
checkpoints 3 through 7 in the table immediately above the sentence. The rule is
false on its face in the one document that states it as a rule, and X29's
parenthesis now describes a correction that was applied only in X29.

## N8 — `DECISIONS.md`'s open list — **PARTIAL**

**OQ-F is in** (l. 789), which is the row X29 and `PROTOCOL.md` §12 both point at,
and it is the one the finding rated as mattering most after OQ-A.

**Why PARTIAL**, three ways:

1. **OQ-A is still absent**, and it is the question that reverses D-005 wherever
   `|V| < 2`. `THREAT_MODEL.md` OQ-A is honest about it (l. 1382: "Still open and
   **not yet in `docs/DECISIONS.md`'s open list**. The number `D-008` was taken by
   a different decision"), but `STATE_MACHINE.md` §11's OQ-A row (l. 1931) gives
   its destination as "| `DECISIONS.md` open list, `PROTOCOL.md` §12,
   `THREAT_MODEL.md` §9.2 |" — a pointer at a list that does not contain it. Two
   documents disagree about a bookkeeping fact, and the one that is wrong is the
   one an implementer would follow.
2. **OQ-D is still absent.** `PROTOCOL.md` §12 was edited to claim only
   "recorded here, in `THREAT_MODEL.md` §9.2 and in `STATE_MACHINE.md` §11", so
   nothing now points falsely — but the question that decides the disposition of
   chips on `cause = 4` has no owner row.
3. **The N4 row is stale.** `DECISIONS.md` l. 790 still carries "`DISPUTE` is
   chained, unsequenced and legal at any time, so the two disputes an honest peer
   is *required* to emit are an equivocation proof against itself (verify N4)" as
   an **open decision**. N4 is closed; `DISPUTE` is unchained with a defined
   envelope and its own anti-replay. An open list that carries settled questions
   is the same failure as one that omits live ones. (The OQ-F row's wording —
   "since four sections claim disputes are 'deterministically adjudicable'" — is
   stale in the same way: those four claims are withdrawn, and the row should ask
   the question rather than cite the withdrawn text.)

## N9 — two internal mismatches — **PARTIAL**

**(a) `HAND_ABORT`'s emitter set — UNRESOLVED.** Unchanged in both places.

> `PROTOCOL.md` §4.10 (l. 1930–1932): "*Direction:* **collective stage**, terminal
> for the hand; the required emitter set is the same set that emitted `HAND_INIT`,
> **minus any seat whose failure is the reason for the abort**."
> `PROTOCOL.md` §4.11 (l. 2057): "| `0x0802` | `HAND_ABORT` | table mesh | 1 |
> collective | **all present seats** |"

A collective stage completes only when every required emitter is heard (§3.2), and
the attributed seat is by construction the one that is silent. Under §4.11 an abort
against a vanished seat can never complete its own stage — the whole abort path
deadlocks. §4.10 is right; §4.11 is stale. The distinction is real here in a way it
is not for `HAND_COMPLETE`, whose two cells do describe the same set.

**(b) `TimeoutCertificate{Join}` — PARTIAL.** The floor was added and the objection
recorded, which is the right shape for a disagreement an editor cannot settle:

> `STATE_MACHINE.md` T4 (l. 676): "| T4 | `Seating` | `TimeoutCertificate{Join}` |
> occupied < `min_players_to_start` ∧ `|V| ≥ 2`, where `V` = the seated seats minus
> the subject (D-008) | `TableClosed` |"
> l. 697–701: "**Objection, recorded rather than silently fixed.**
> `DeadlineKind::Join` has no counterpart in `PROTOCOL.md` §4.8 … The `|V| ≥ 2`
> floor above is stated so that T4 is not a D-008 hole if the kind survives; the
> better fix is for T4's trigger to become a local lobby-layer timer expiry, which
> is `PROTOCOL.md`'s to make."

But `PROTOCOL.md` has now made the opposite ruling, flatly, in §8.4 (l. 2984–2986):

> "**A certificate kind for a join deadline does not exist in this document, and
> none should be invented to fill the gap — there is no gap.**"

So the two documents now contradict each other head-on: one says the kind does not
exist and must not be invented, the other keeps a transition triggered by it. The
objection is well founded and the deference is correct in principle, but the state
left behind is a live contradiction rather than an open question, and the engine's
event alphabet contains a variant no wire message can produce.

## D-008's application across the corpus — **PARTIAL**

The rescoping itself is thorough, disciplined and, on the `n` sweep, complete
(Part 3.1). Every document states the rule in its own register and none paraphrases
it into something weaker:

> `PROTOCOL.md` §8.3 (l. 2801–2806): "**Everything in this section is scoped on
> `|V|`, the size of the required voter set, and never on `n`, the seat count.**
> That is D-008, it is binding, and the scoping *is* the protection. The seat count
> is not a quantity an attacker can change; the effective voter set is."
> `STATE_MACHINE.md` §5.2 (l. 630–637): "**No guard in this table is scoped on the
> seat count (D-008).** … A guard in this document that reads `m == 2` or
> `|deck.participants| == 2` is a defect, and the last one — T34's — was removed in
> this pass."
> `NETWORK_STACK.md` §1 (l. 107–109): "the test is `|V| < 2` and never `n = 2`"
> `CRYPTOGRAPHY.md` §2.10 (l. 540–545): "**The scope is `|V|`, never the seat count
> `n` (D-008).** … Any rule in this corpus still written on `n` is a defect."
> `THREAT_MODEL.md` X10 (l. 779): "every rule in this row is written on `\|V\|`
> rather than on `n`, per **D-008**."

Points 1, 3 and 4 of the decision are fully applied. **Point 2 is not**:
`STATE_MACHINE.md` §8.4 rule 6, T16/T22/T27/T41/T44 and I29(b) give a
`kind == Crypto` certificate a terminating effect below the floor, where D-008
point 2 says it "has no effect", and `PROTOCOL.md` §8.3 says it is "not chained".
`THREAT_MODEL.md` holds both positions in one document. That is **M1**, and it is
the reason this item is PARTIAL rather than RESOLVED.

---

# Part 3 — Independent sweeps

## 3.1 The `n` sweep — **clean**

Every occurrence of `n = 2`, `n == 2`, `n >= 3`, `two seats`, `heads-up`,
`|deck.participants| == 2`, `m == 2` and `>= 3` in all five specification
documents, plus `DECISIONS.md`, `DEPENDENCIES.md` and `CONTRIBUTING.md`, was read
in context and classified. **No rule, guard, transition or classification is still
scoped on the seat count where D-008 requires `|V|`.** N3 is not reopened.

The surviving mentions fall into four legitimate classes:

| Class | Examples | Why it is not a hit |
|---|---|---|
| Arithmetic identity | `PROTOCOL.md` l. 2819 "With no prior attribution `\|V\| = n - 1`"; `THREAT_MODEL.md` X10's ceiling table | Stated as the nominal starting value and explicitly demoted three lines later ("**That table is the nominal set, not the operative rule**") |
| Instantiation of a `\|V\|` rule | `PROTOCOL.md` l. 2851 "At two dealt-in seats `\|V\| = 1` always, so this reproduces D-007's heads-up rule exactly — **as a consequence of the general scoping, not as a special case**"; `THREAT_MODEL.md` l. 344, 1028, 1308; `STATE_MACHINE.md` l. 1798, 1806; `NETWORK_STACK.md` l. 1209, 1956; `CRYPTOGRAPHY.md` l. 541 | The rule is on `\|V\|`; the seat count appears only to say when `\|V\|` takes that value. Required, not residual |
| Recorded history | `PROTOCOL.md` §8.4's quoted deleted rule; `STATE_MACHINE.md` l. 1377 "The earlier form of this rule read *'if `\|deck.participants\| == 2`'*"; `CRYPTOGRAPHY.md` §15 l. 1957–1972; `DECISIONS.md` D-007 point 3 | Quotation of a superseded text inside the record of its supersession |
| Poker rules, not the certificate | `STATE_MACHINE.md` §7.1 heads-up blinds, T29's `\|live\| ≥ 3`, §7.5's "Side pots require at least three seats", §5.3's `\|dealt_in\| == 1` | Genuine table-size properties of Hold'em. D-008 does not reach them and must not |

Two items are correct but worth naming because a future reader may mistake them
for hits. `STATE_MACHINE.md` §9.5 l. 1821 — "Asserting only that a well-formed
certificate is *accepted* at `n >= 3` would pass while the shrinkage bug was live"
— is a statement about a **test** that would be insufficient, i.e. it argues
against `n`-scoping. And `THREAT_MODEL.md` X10's title still reads "by all other
dealt-in seats", which describes the coalition D-006 requires, not a rule.

The one thing the sweep does turn up is not an `n`-scoped survivor but a
below-the-floor **effect** granted where D-008 point 2 forbids one: **M1**.

## 3.2 The byte-budget sweep — **clean**

Every occurrence of `max_circuit_bytes`, `131072` / `131 072`, `128 KiB`,
`8 979`, `17 958`, `per direction` / `per-direction` / `each direction` and
`ConnectionDataLimit` across all eight documents was read.

* **No document says or implies per-direction as a live claim.** The phrase
  survives only inside sentences that withdraw it (`PROTOCOL.md` l. 3109,
  `CRYPTOGRAPHY.md` l. 1047, `THREAT_MODEL.md` l. 526, `NETWORK_STACK.md` §16.1)
  and inside the kubo caveat.
* **The seven places `PHASE1_VERIFY.md` listed are all corrected**, with the same
  figures: `17 958 B` per circuit per hand, ~7 hands shuffle-only, ~5 with the
  event stream.
* **`NETWORK_STACK.md` no longer contradicts itself.** §9.5 (l. 1348–1352) and
  §16.1 now give the same number, and §16.1 is retitled "**Status: objection
  upheld, correction applied.**"
* **The unaffected figures were not disturbed**, which is the half a careless fix
  would have got wrong: `8 979 B` per shuffler and `(n−1) × 8 979 B` as a peer's
  per-hand outbound are unchanged everywhere, with the reason stated in three
  places ("spread over `n−1` separate circuits and therefore `n−1` separate
  budgets, **never** a single cap", `NETWORK_STACK.md` l. 1352).
* **The conclusion is stated as surviving rather than as vindicated**: at 7 hands
  as at 14, duration binds first. `NETWORK_STACK.md` §16.1 adds the honest note
  that the correction *strengthens* D-001's addendum.
* `STATE_MACHINE.md` §13 item 3 records that it carries no such figure and states
  the absence deliberately.

## 3.3 Constant and `ctx` sweep — **clean**

**`ctx`.** The block in `PROTOCOL.md` §4.5 (l. 1497–1508) and `CRYPTOGRAPHY.md`
§6.4 was diffed character by character and is **byte-identical**, field order,
comments, the `0xFF` sentinel and all. The only difference is in the prose after
the block, where `CRYPTOGRAPHY.md` adds the cross-reference to `PROTOCOL.md` §2.8
— which is the ownership rule working, not drift. `THREAT_MODEL.md` A11/OQ3 name
the same seven fields.

**Numeric constants.** Every name in `PROTOCOL.md` §13 was matched against every
occurrence in the other seven documents. All agree:

| Constant | §13 | Restated in | Agree |
|---|---:|---|:--:|
| `GOSSIP_MAX_TRANSMIT` | 65 536 | `NETWORK_STACK.md` §11.3 | ✓ |
| `LOBBY_MSG_MAX` | 8 192 | `NETWORK_STACK.md` §6.5, §11.3 | ✓ |
| `TABLE_AD_MAX` | 1 024 | `NETWORK_STACK.md` §6.5, §7.3, §11.3 | ✓ |
| `TABLE_AD_SIGNED_MAX` | 1 536 | `NETWORK_STACK.md` §6.5, §7.3, §8.4, §11.3 | ✓ |
| `LOBBY_CHAT_MAX` | 2 048 | `NETWORK_STACK.md` §11.3, `THREAT_MODEL.md` l. 783 | ✓ |
| `SNAPSHOT_REQ_MAX` / `SNAPSHOT_RESP_MAX` | 1 024 / 262 144 | `NETWORK_STACK.md` §7.1, §7.3, §11.3 | ✓ |
| `SNAPSHOT_MAX_ADS` | 128 | `NETWORK_STACK.md` §7.3, §11.3 | ✓ |
| `JOIN_REQ_MAX` / `JOIN_RESP_MAX` | 4 096 / 16 384 | `NETWORK_STACK.md` §8.4, §11.3 | ✓ |
| `TABLE_FRAME_MAX` | 262 144 | `NETWORK_STACK.md` §8.4, §11.3 | ✓ |
| `MAX_EMBEDDED_EVENT` | 32 768 | `PROTOCOL.md` §4.9, §4.10, §9.4; `NETWORK_STACK.md` §11.3 | ✓ |
| `MAX_DISPUTES_PER_SENDER_PER_HAND` | 8 | `PROTOCOL.md` §4.9, §5.2, §5.3, §9.4 | ✓ (new, one value everywhere) |
| `MAX_SEATS` / `MAX_STAGES_PER_HAND` / `MAX_CBOR_NESTING_DEPTH` | 10 / 2 048 / 8 | not restated | ✓ |
| every TTL, interval, timeout, `SNAPSHOT_PEER_COUNT` | §13 | not restated | ✓ |
| `CERT_SETTLE_MS` | **retired** | absent everywhere but the retirement notes | ✓ |

Internal arithmetic re-checked: `128 × 1 536 = 196 608 ≤ 262 144`;
`4 × 32 768 = 131 072 <` `DISPUTE`'s 140 000 cap `<` `TABLE_FRAME_MAX`;
`hand_deadline_ms 600 000 ≥ 10 × action_timeout_ms 20 000` (`STATE_MACHINE.md`
§9.4's config bound). `STATE_MACHINE.md` §9.4's bounds are validity ranges, not
competing values, and do not collide with §13.

**Domain strings.** `DOMAIN_EVENT`'s 24 bytes are identical in `PROTOCOL.md` §2.4
and §13 and decode to `p2p-poker/v1/event` + 6 NUL. The `h` constructor, the
`commitment_i` / `seed` bindings and the deck-index map are identical across
`PROTOCOL.md`, `CRYPTOGRAPHY.md` and `STATE_MACHINE.md`, with `m` as the symbol
everywhere. The new strings introduced this pass —
`h("p2p-poker v1 timeout-cert", …)` in §4.8's `subject_digest` — appear once and
are in §2.8's register. Retired strings appear only in the retirement table.

## 3.4 Claim sweep — **two survivors, both new**

Every "prevented", "impossible", "guaranteed" and "cannot" in `THREAT_MODEL.md`
and `CRYPTOGRAPHY.md` was read in context.

* Every "impossible" either quotes `SPEC_CS.md`, denies the claim, or states the
  true `n`-of-`n` liveness property ("any player who goes silent makes it
  impossible for anyone to open any further card", X7) which is the reason the
  abort path exists.
* No unjustified "prevented". `THREAT_MODEL.md` §9.3 qualification 4 still states
  it outright, and §5.1's CP definition is scoped on A1–A7.
* `THREAT_MODEL.md` §5.4's totals were re-derived by hand and are correct:
  X1–X10 + X12–X30 = 29 extended rows; `6 + 7 + 3 + 1 + 12 = 29`;
  `19 + 29 = 48`; `17 + 14 + 3 + 1 + 13 = 48`. X30's addition and the reason for
  each recount are recorded in the paragraph below the table.
* X30's class cell is the model of what this sweep is for: **D&A**, "and only
  because of a rule our own client enforces", with an explicit refusal to round up
  — "under D-008 an inert certificate is *not evidence of misbehaviour*, so no seat
  is sanctioned for emitting one … what is transferable is the artefact, not a
  verdict. And the rejection rests on our own clients enforcing the `|V|` floor, an
  implementation obligation in the class of A12 and A14, not on A1–A7 — which is
  exactly why this row is not CP."

**Two claims are stronger than their mechanism, both introduced by this pass and
both in `STATE_MACHINE.md`:**

1. §8.6 l. 1590: "Two attribution paths remain that do **not** depend on `V` at
   all, and **both are correct that way**: `AbortKind::Equivocation` (T55), whose
   evidence is a self-contained proof signed under the accused's own key rather
   than anybody's claim about what they saw". True only while the §5.2 predicate
   cannot be satisfied by honest behaviour — see **M2**.
2. §13 l. 2061: "The chip outcome is identical either way … so **nothing about
   D-005 or D-008 turns on it**". The chip arithmetic is identical; the
   availability, the timing, the transcript record and the accept/reject decision
   are not — see **M1**.

A third, milder, is `CRYPTOGRAPHY.md` §7.2's "`SmallRng` is **not** compiled in" —
**M3** — where the claim is corpus-wide and the evidence is one crate major.

## 3.5 New-defect hunt

Five, recorded in Part 1: **M1** (below-the-floor certificate effect, three-way
contradiction, on-demand hand-void), **M2** (double timeout vote is an
equivocation proof against an honest voter, and T55 forfeits its chips), **M3**
(`SmallRng` is in the build via `rand 0.9.5`), **M4** (`PLAYER_LEAVE`'s
contradictory chain scope), **M5** (`TIMEOUT_VOTE`'s emitter gate versus its
receiver check).

Areas searched and found **clean**, recorded so the next pass does not repeat
them:

* **The inductive exclusion itself.** `certified_subjects` is canonical state, is
  emptied at hand init, is written only on acceptance at `|V| ≥ 2` (rule 7), and
  I29(a) asserts the induction. A verifier can check it offline (§8.5). There is no
  path by which a bare vote, a rejected certificate or an incomplete one mutates
  it, and I21 makes a rejection bit-identical.
* **`certified_subjects` across hands.** Emptied at hand init (§5.3), so a
  cross-hand accumulation attack does not exist.
* **The `hand_deadline_ms` path.** `PROTOCOL.md` §8.4's claim that it is "safe at
  every `|V|`, including `|V| = 0`" holds: it names nobody, moves nothing between
  seats, needs no voter set, and I27 pins the arithmetic. §4.10's `cert_hash` row
  carries the one exception cleanly.
* **Chip conservation on every abort path.** §8.6's formula, the `culprits == {}`
  branch, I27, I28 and G9 agree, and the ledger identity is byte-identical in
  `STATE_MACHINE.md` §10, `THREAT_MODEL.md` G9 and `PROTOCOL.md` §4.10.
* **T56's restraint.** Consuming an equivocation proof with no live hand as a
  `Fault` and **no** chip movement — "there is no live commitment to forfeit" — is
  the right call and forecloses a whole class of between-hands forfeiture attack.
* **`TableClosed` is absorbing for `EquivocationProof` too** (`STATE_MACHINE.md`
  l. 865), which stops a proof reopening a closed table.
* **`DISPUTE`'s new unchained form.** Payload-carried `table_id` / `hand_id`,
  session-matched on receipt, idempotent by `event_hash`, bounded per sender per
  hand, `note` never parsed. The evidence still reaches the chain, because
  `HAND_ABORT cause = 5` carries it in `n(3)`.
* **The equivocation predicate against lobby and join traffic** (A-4's ground).
  Still correctly scoped; the sentinel rule is normative and `NETWORK_STACK.md`
  §7.4 and §11.5 defer to it and say what their own rule is *not*.

## 3.6 Phase 2 readiness

**`STATE_MACHINE.md` is close but is not yet a sound basis for implementing the
deterministic engine.** One defect must be fixed first; the rest are things an
implementer would have to guess.

**The blocker is M1.** An engine is a total function from `(state, event)` to
`(state', effects)`, and the two documents an implementer reads together give
different answers for one event that a modified client can emit at will:
`TimeoutCertificate{Crypto}` with `|V(subject)| < 2`. `PROTOCOL.md` §8.3 says
reject and do not chain; `STATE_MACHINE.md` §8.4 rule 6 says accept and abort the
hand. There is no reading under which both clients agree, and the disagreement
surfaces as a `state_hash` mismatch — the one fault the corpus classifies as
unresolvable. This is not a documentation tidy-up: it is the engine's accept
predicate.

**M2 is not a `STATE_MACHINE.md` defect** — the engine takes `EquivocationProof`
as an opaque input, exactly as it should — but an engine built to the current text
will forfeit honest players' chips whenever two seats stall at one stage, and the
fix is in `PROTOCOL.md`'s envelope rules, so it should land before the engine is
written against T55.

**What an implementer would still have to guess, listed so the list is finite:**

1. **The carrier for a certificate-free `HandDeadline` abort.** `STATE_MACHINE.md`
   §13 records the gap in terms: "§4.1 declares no event that could carry that into
   `step`". Whichever way M1 is settled, `Event` needs a variant, and it needs a
   rule saying which layer raises it and from what.
2. **`DeadlineKind::Join`.** N9(b). The engine declares a deadline kind that
   `PROTOCOL.md` §4.8 does not define and §8.4 forbids inventing. An implementer
   must guess whether to build T4 on a certificate that has no wire form or on a
   lobby timer that the state machine does not model.
3. **Whether a voter may vote about two subjects at one stage.** M5. §4.8's two
   sentences give opposite answers, and one of them makes the whole `kind = 2`
   path unreachable.
4. **`HAND_ABORT`'s required emitter set.** N9(a). §4.10 and §4.11 disagree, and
   under §4.11's answer the abort stage cannot complete against the seat it is
   about. The engine's stage-completion predicate depends on which is right.
5. **`PLAYER_LEAVE` outside a hand boundary.** M4. Whether a mid-hand courtesy
   notice exists at all, and if so with what envelope.
6. **Q3 / Q-02 / OQ-E**, the multi-subject deadline. Carried openly with a written
   interim rule, which is the right disposition — an implementer can build the
   interim rule and knows it is interim. Not a guess, but it is blocking for
   Phase 4 and the engine will need re-work when it is answered.
7. **OQ-A.** Restoration versus forfeiture wherever `|V| < 2`. The interim answer
   is written and implementable; the decision reverses D-005 and is not in
   `DECISIONS.md`'s open list at all (N8), so nothing schedules it.
8. **`showdown_policy`** (Q-01). `MANDATORY_REVEAL` is pinned in
   `RATED_SNG_POKERTH_V1` and `TDA_MUCK` has a declared `SHOWDOWN_MUCK` message
   and a "policy-gated" emitter set, so the engine must either implement both
   branches or refuse the value.

**What is genuinely ready.** The 20 phases, 56 transitions and 29 invariants are
complete and internally consistent, with no orphan events and no unconsumed
variants — the two defects C-6 raised (`RevealRejected`, `StateAck`) are closed and
the pass that closed them declared its own completions. §3's determinism contract
is precise (`step` pure, no `std::time`, no RNG, no `HashMap` iteration reachable
from `TableState`). §6's legal-action computation, §7's blinds, button, minimum
raise, incomplete all-in, side pots, odd chips and showdown are specified to the
level an implementer can code against with the TDA citations attached. §10's
invariants are written as property-test assertions, and I29 comes with the right
warning that random legal play never reaches it. §2.6's canonical/`LocalView`
split is load-bearing and correct. The engine is well within reach; it needs the
four two-document contradictions above settled first, and none of them is hard —
they are all "which of these two sentences is normative".

---

# Part 4 — What must happen before Phase 2

Ordered by what endangers the most.

1. **M2 — give `TIMEOUT_VOTE` (and `TIMEOUT_CERT`) a slot that a second subject
   cannot collide with.** Until then two Sybil seats stalling at one collective
   stage make every honest voter's own signed votes an `EquivocationProof` against
   it, and T55 plus §8.6 forfeit that voter's committed chips. This is the worst
   defect found in any of the three passes.
2. **M1 — settle what a `kind = 2` certificate does below D-008's floor**, and
   settle it as D-008 point 2 states it. Then add the certificate-free
   `HandDeadline` carrier, and bring `THREAT_MODEL.md` §7.3(c) into line with its
   own X10.
3. **M5 — make §4.8's emitter gate and receiver check say the same thing.** One
   line, and it decides whether the crypto-deadline path exists.
4. **N9(a) — `PROTOCOL.md` §4.11's `HAND_ABORT` emitter cell.** One cell. As
   written the abort path cannot complete against the seat it is about.
5. **N9(b) / M4 — two envelope contradictions**: `TimeoutCertificate{Join}`
   against §8.4's prohibition, and `PLAYER_LEAVE`'s "not chained" against §2.3 and
   §4.11.
6. **N2 — the two surviving "adjudicable offline" sentences in
   `STATE_MACHINE.md`** §5.2 (l. 859) and §12 (l. 1979), which are the exact form
   `PROTOCOL.md` §6.4 records as withdrawn.
7. **N7 — `PROTOCOL.md` §6.2's bolded rule.** Add "to anyone but its owner", as
   `THREAT_MODEL.md` X29 already has it.
8. **N8 — `DECISIONS.md`'s open list**: add OQ-A and OQ-D, remove the settled N4
   row, restate the OQ-F row on the question rather than on the withdrawn text,
   and fix `STATE_MACHINE.md` §11's OQ-A pointer.
9. **M3 — the `SmallRng` absence claim** in `CRYPTOGRAPHY.md` §7.2,
   `PROTOCOL.md` §4.4, `THREAT_MODEL.md` A7 and `CRYPTO_LIBS.md` §7.3. The
   conclusion (a CI lint, not the dependency graph) is already right; only the
   justification is false.

Nothing in this verification reverses a ruling of `PHASE0_FIXPLAN.md` or of
D-008. D-008 is the right decision, it is applied faithfully in four documents and
in three of its four points in the fifth, and the `n` sweep it was written for
comes back clean. What the pass did not reach is the second-order consequence of
its own change: deleting the exclusion rule made two simultaneous timeout subjects
the specified behaviour, and no document then asked what two simultaneous subjects
do to the equivocation predicate. That is M2, and it is the third pass in a row to
find a worse bug than the pass before it.
