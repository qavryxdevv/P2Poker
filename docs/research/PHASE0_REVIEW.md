# PHASE0_REVIEW.md — adversarial review of the five Phase 0/1 specification documents

Target documents:
`docs/THREAT_MODEL.md`, `docs/PROTOCOL.md`, `docs/CRYPTOGRAPHY.md`,
`docs/NETWORK_STACK.md`, `docs/STATE_MACHINE.md`.

Read alongside: all seven research notes in `docs/research/`, `docs/SPEC_CS.md`
(all 36 sections), and `docs/DECISIONS.md` (D-001 … D-006).

**Method.** Every API claim that a finding turns on was re-checked against the
unpacked crate source under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.
Where a finding is an attack, the attack is written out in full with the exact
document sentences it falsifies. Nothing here is a style objection.

**Findings: A = 8, B = 4, C = 11, D = 7. Total 30.**

| Class | Meaning | Count |
|---|---|---:|
| **A** | A security claim that is not true, or stated more strongly than the evidence supports | 8 |
| **B** | An invented or unverified API/fact, or one contradicted by the research | 4 |
| **C** | A contradiction between two documents, or with `SPEC_CS.md` | 11 |
| **D** | A `SPEC_CS.md` requirement no document covers | 7 |

Ordered by severity, not by class. A-1 through A-4 are the ones that change the
security posture of the design and should be fixed before Phase 2.

---

## A-1 — Heads-up, the "unanimous" timeout certificate is a single signature, so a modified opponent can fold your hand at will

**Where.** `PROTOCOL.md` §8.3 (required voter set), §4.8 (`TIMEOUT_VOTE`,
`TIMEOUT_CERT`); `STATE_MACHINE.md` §8.4 (validity rule 4) and T34;
`THREAT_MODEL.md` §5.3 X10, X11, and §9.1 limitation 3; `DECISIONS.md` D-006.

**What is claimed.**

> `PROTOCOL.md` §8.3: "One peer asserting 'time is up' cannot be enough — it
> would let anyone steal the action from a player who was about to act. A timeout
> takes effect only through a **unanimous** certificate."

> `THREAT_MODEL.md` X10 classifies the forged certificate as requiring "**a
> malicious majority**", and §9.1 limitation 3 says the effect is "bounded to an
> auto check/fold".

**Why it is wrong.** The required voter set is defined as

> `PROTOCOL.md` §8.3: "every seat that is a cryptographic party to the hand
> (`dealt_in`), minus the subject seat"

and identically in `STATE_MACHINE.md` §8.4 rule 4
(`signers == deck.participants \ {subject}`). At a two-seat table
`|dealt_in| = 2`, so the required voter set has **exactly one member**:
the opponent. "Unanimity" and "one peer asserting time is up" are the same
statement heads-up. The sentence quoted above is self-falsifying in the
configuration `SPEC_CS.md` §32 mandates as the *first* implemented mode.

**Concrete attack.** Two-seat play-money table, Mallory modified, Alice honest.

1. Alice faces a bet on the river and is deciding.
2. Mallory does not wait. She signs `TIMEOUT_VOTE { subject_sequence = s,
   subject_seat = Alice, parent_event_hash = stage_hash(s-1), kind = 1 }`,
   immediately assembles `TIMEOUT_CERT` from her own single vote — the voter set
   is `{Mallory}` and it is complete — and chains it at stage `s`.
3. Per `PROTOCOL.md` §8.3 "Effect of a certificate", `kind = 1` with `to_call > 0`
   applies **fold**. Alice's hand is folded. Mallory takes the pot.
4. Alice broadcasts her real `ACTION_CALL` for stage `s`. It is not equivocation
   (different signer — see A-2), so no evidence exists against Mallory. Alice
   raises `DISPUTE`. §6.3 step 4 case (b) says the hand then "aborts with
   `cause = 1`, attributed to the peers that would not serve, **via the §8
   certificate machinery**" — which requires a certificate signed by every other
   active player, i.e. by Mallory. She does not sign. The dispute cannot resolve.
5. `hand_deadline_ms` expires. Its certificate has the same unanimity
   requirement over the same one signer. It never forms.

So heads-up, a modified client can (a) fold its opponent's hand at any decision
point, and (b) make every dispute unresolvable. This is not a bounded
"auto check/fold"; it is the whole game.

**The correction.**

1. `THREAT_MODEL.md` X10 must be restated: the attack requires **all other
   dealt-in seats**, which at `n = 2` is **one** seat, and heads-up is therefore
   not protected at all by unanimity. The current wording ("a malicious
   majority") reads as though it needed several colluders and is misleading for
   the MVP.
2. `THREAT_MODEL.md` §9.1 must add: *at `n = 2` the timeout certificate provides
   no protection whatsoever; a modified opponent can force a fold at will.*
   Effect is **not** bounded to a harmless check.
3. `PROTOCOL.md` §8.3 must either (i) forbid `kind = 1` certificates entirely at
   `n = 2` — heads-up has no way to agree a deadline without a trusted clock, so
   the honest answer is that a heads-up action deadline can only ever be advisory
   and never a signed state transition — or (ii) require the *subject's* own
   counter-signature on the parent, which converts the mechanism into something
   else and must be designed, not asserted.
4. The dispute escalation in §6.3 case (b) must not be defined in terms of the
   §8 certificate machinery when the accused peer is a required signer of that
   certificate. That is circular for every `n`, not only `n = 2`.

---

## A-2 — The mechanism that is said to make the timeout race safe does not exist

**Where.** `PROTOCOL.md` §5.2 (equivocation, third bullet) and §8.3
(unanimity settles the race); `THREAT_MODEL.md` X11 (classified **CP**).

**What is claimed.**

> `PROTOCOL.md` §5.2: "a `TIMEOUT_VOTE` and a real event for the same stage,
> because a vote occupies the stage slot it is about (§4.8). This is what makes
> the D-006 race safe: **a peer that both accepted an action and voted that it
> timed out has equivocated and is provably at fault.**"

> `PROTOCOL.md` §8.3: "if a peer produces both, it has equivocated in the sense
> of §5.2 and that is provable from the two bodies alone."

> `THREAT_MODEL.md` X11: class **CP** — "no such window exists".

**Why it is wrong.** §5.2's own definition of equivocation requires the two
conflicting events to share `sender_public_key`:

> "their bodies agree on all four of `(protocol_version, table_id, hand_id,
> sequence)` **and `sender_public_key == K`**"

The action at stage `s` is signed by the **subject**. The `TIMEOUT_VOTE` at stage
`s` is signed by the **voter**. Different senders, therefore not equivocation
under the document's own predicate. Worse, "accepted an action" is not a signed
artefact at all — a voter that receives, verifies and accepts the subject's
action leaves no signature attesting to that acceptance. There is nothing to pair
the vote against.

So the safety property "there is no state in which both a valid action and a
valid certificate exist for the same parent" holds only if every voter is honest,
which is exactly the assumption the threat model forbids (`THREAT_MODEL.md`
§3.1). X11 is not CP; at best it is D&A when the voter set contains an honest
peer that saw the action, and nothing at all when it does not.

**The correction.**

* Delete the third bullet of `PROTOCOL.md` §5.2 or replace it with the true
  statement: a voter's `TIMEOUT_VOTE` and the subject's action are two events by
  two different keys and constitute no proof against anyone.
* `PROTOCOL.md` §8.3's sentence "if a peer produces both, it has equivocated"
  must go.
* `THREAT_MODEL.md` X11 must be reclassified out of CP. Given A-1 it collapses
  into X10 and should be merged with it, carried as the same unfixed limitation
  (OQ8), not as a prevented attack.
* If a real race-settling artefact is wanted, it has to be one the voter signs —
  e.g. a signed `ACTION_SEEN { sequence, event_hash }` that a voter must publish
  before it may vote, so that vote-and-seen *are* two events by one key in one
  slot. That is a design change, not a restatement.

---

## A-3 — `HAND_ABORT cause = 4` restores start-of-hand stacks and is reachable on demand by a single heads-up player: the rage-quit escape D-005 closed is reopened

**Where.** `PROTOCOL.md` §6.3 step 4 case (c), §6.4, §4.10 `HAND_ABORT`;
`THREAT_MODEL.md` §5.3 X8 ("**D&A**, and economically closed") and §7.4;
`DECISIONS.md` D-005.

**What is claimed.**

> `PROTOCOL.md` §4.10: "`cause = 4` (unresolvable divergence) is the single
> exception where stacks are **restored to their start-of-hand values**."

> `PROTOCOL.md` §6.4: "This is the one place where restoration is correct rather
> than exploitable, **because reaching it requires a genuine engine disagreement
> among at least two peers and is not available on demand to a single player.**"

> `THREAT_MODEL.md` X8: "quitting costs exactly what folding would have cost, so
> the escape has no value."

**Why it is wrong.** §6.3 point 1 states that heads-up tables must treat case (c)
as `cause = 4`, because unanimity-minus-one is meaningless at `n = 2`. Case (c)
is entered whenever the transcripts are byte-identical after reconciliation and
the derived states still differ. Publishing a wrong `state_hash` is a one-field
lie in a message every peer is *required* to emit; it needs no invalid signature,
no invalid proof, and no divergent transcript. "At least two peers" disagreeing
is satisfied by one liar plus the honest victim.

**Concrete attack.** Two-seat table. Checkpoint 7 is emitted
"immediately before `HAND_COMPLETE`" (`PROTOCOL.md` §6.2), i.e. **after** the
showdown reveal.

1. Mallory loses the showdown for a large pot. She has already seen Alice's cards
   and the settlement.
2. At checkpoint 7 Mallory emits a `STATE_HASH` containing 32 random bytes.
   Signature valid, canonicality gate passes, stage legal.
3. Both peers freeze (§6.3 step 1), declare (step 2), reconcile (step 3) — the
   transcripts are byte-identical, because Mallory withheld nothing.
4. Step 4 case (c). `n = 2`, so unanimity-minus-one does not apply and the hand
   aborts with `cause = 4`.
5. **Stacks are restored to their start-of-hand values.** Mallory gets the money
   back. No attribution, no forfeiture, no evidence naming her.

The table is faulted and closed, which costs Mallory a table — trivially
re-created in cash play, and strictly better than busting in a tournament. At
`n ≥ 3` the same escape costs only a second colluder (two liars make the
disagreement non-unanimous-minus-one, so nobody is removed and `cause = 4` fires
again).

This is precisely the exploit D-005 rejected: *"a free, in-protocol escape from a
losing pot, available to anyone, at will, with no special capability."* It has
been reintroduced through the divergence path.

**The correction.**

1. `PROTOCOL.md` §6.4's justification sentence is false and must be removed.
2. `cause = 4` must not restore stacks. Options, each of which needs a numbered
   decision: (i) treat a `state_hash` that does not match the value derived from
   the agreed transcript as an **attributable protocol violation** — it is, since
   the transcript is unanimous and the derivation is deterministic, so
   "unanimity-minus-one" is really "the transcript says who is wrong" and works at
   `n = 2` as well; (ii) forfeit the divergent peer's commitment as in every other
   abort. Option (i) is strictly better and removes the need for the
   unanimity-minus-one rule at all.
3. Move checkpoint 7 to **before** `SHOWDOWN_REVEAL`, or accept that any
   checkpoint placed after the cards are known is an abort trigger the loser can
   pull with full information.
4. `THREAT_MODEL.md` X8 must not say "economically closed" until this path is
   closed.

---

## A-4 — The equivocation predicate's slot namespace collides with lobby and join traffic, so honest, mandatory behaviour manufactures valid "equivocation proofs" against honest keys

**Where.** `PROTOCOL.md` §5.2 (definition and `EquivocationProof`), §4.3
(`JOIN_REQUEST` envelope), §7.2 (`LOBBY_TABLE_AD` rebroadcast),
§4.3 `PLAYER_LIST`; `THREAT_MODEL.md` G7 and §5.2 row 16.

**What is claimed.**

> `PROTOCOL.md` §5.2: "**Verification is self-contained.** A checker needs
> nothing but the two byte strings and Ed25519: gate both, verify both signatures
> under `accused`, check the four fields agree, check the two `event_hash`es
> differ. No table state, no transcript, no knowledge of the game."

> `THREAT_MODEL.md` G7: "the pair constitutes a self-authenticating, transferable
> proof of that player's misbehaviour."

Consequence per §5.2: `HAND_ABORT` with `cause = 5`, the accused attributed,
chips forfeited under D-005, and the peer blocklisted.

**Why it is wrong.** The four-field slot `(protocol_version, table_id, hand_id,
sequence)` is shared by three unrelated sequence spaces:

| Message | `table_id` | `hand_id` | `sequence` (per §4) |
|---|---|---|---|
| `TABLE_READY` | the table | 0 | 0 (setup-chain stage) |
| `RNG_COMMIT` | the table | 0 | 1 (setup-chain stage) |
| `RNG_REVEAL` | the table | 0 | 2 (setup-chain stage) |
| `JOIN_REQUEST` | **the table** | **0** | **"per-connection counter"** |
| `LOBBY_TABLE_AD` | the table (= `table_public_key`) | unspecified | **unspecified** |
| `PLAYER_LIST` | the table | unspecified | **unspecified** |

Two constructible false proofs follow.

**(i) Against a joining player.** `JOIN_REQUEST` is defined with
`table_id` = the target table, `hand_id = 0`, and `sequence` = a per-connection
counter. On the founder's connection that counter is 2 (`HELLO` 0,
`CAPABILITIES` 1). `RNG_REVEAL` is setup-chain stage 2, also `hand_id = 0`, also
that `table_id`, also signed by the joiner's application key, with a different
body. The pair satisfies §5.2's predicate exactly. Any peer that holds the
joiner's `JOIN_REQUEST` — the founder always does — can publish a
self-contained, third-party-verifiable `EquivocationProof` against a player who
did nothing wrong, and the specified consequence is that the victim's committed
chips are forfeited and their key blocklisted network-wide.

**(ii) Against every honest table founder, every 30 seconds.** §7.2 requires the
founder to rebroadcast the advert every `AD_REBROADCAST_MS = 30 000` with
`timestamp_unix_ms` **strictly greater** than the previous one. The envelope
`sequence` for a lobby message is never specified anywhere in `PROTOCOL.md`. If
it stays 0 (the natural reading, since lobby messages are not in any chain), two
successive honest adverts are two distinct bodies in one slot under one key —
equivocation by the definition, forever, for every honest founder.
`NETWORK_STACK.md` §7.4 hints at a different rule ("same `(table_id,
sequence/timestamp)`"), which is a third, incompatible predicate.

**The correction.**

1. Scope the equivocation predicate to **chained events only**. Add
   `previous_event_hash != ZERO32` (or an explicit `chain_scope` discriminator) to
   the definition, and state that unchained messages — `HELLO`, `CAPABILITIES`,
   `JOIN_*`, `PLAYER_LIST`, and every lobby message — are outside it and have
   their own anti-replay (`list_serial`, `timestamp` monotonicity, `join_nonce`).
2. Give unchained messages a `table_id` of `ZERO32`, or a distinct
   `hand_id` sentinel (e.g. `u64::MAX`), so their slots cannot collide with a
   chain slot. `JOIN_REQUEST` in particular must not carry the real `table_id`
   with `hand_id = 0`.
3. Specify the envelope `sequence` for every lobby message type. Leaving it
   undefined in a document whose central evidence object is keyed on it is not
   acceptable.
4. `THREAT_MODEL.md` G7 must record the false-positive class explicitly. §5.2
   already lists "a client ran twice against one profile" as a false-positive
   source; the namespace collision is a larger one and is not a bug in the user's
   setup but in the specification.

---

## A-5 — `TIMEOUT_VOTE` occupies the subject's stage slot, so at a **collective** stage every honest voter equivocates against itself; the D-005 abort machinery is unusable exactly where it is needed

**Where.** `PROTOCOL.md` §4.8 (`TIMEOUT_VOTE` envelope), §3.2 (collective
stages), §8.3 (voter set for `kind = 2`), §5.2; `STATE_MACHINE.md` T27, T41, T44.

**What is claimed.**

> `PROTOCOL.md` §4.8: "The envelope's own `sequence` for a `TIMEOUT_VOTE` is
> `subject_sequence`, and its `previous_event_hash` is `parent_event_hash` — a
> vote occupies the same stage slot it is about. That is what makes a vote and an
> action mutually exclusive for one sender (§5.1): a peer physically cannot sign
> both without equivocating."

**Why it is wrong.** That reasoning holds only for a **single-writer** stage,
where a voter has nothing of its own to emit at that stage. But the stages whose
failure D-005 exists to handle are all **collective** (`PROTOCOL.md` §3.2 and
§4.4–§4.9): `DECK_INIT`, `DECK_COMMIT`, `DEAL_PRIVATE`, `BOARD_REVEAL`,
`SHOWDOWN_REVEAL`, `STATE_HASH`, `STATE_ACK`. At a collective stage `s` **every
dealt-in seat emits exactly one event at `(table_id, hand_id, s)`**.

So when seat 4 fails to publish its `DEAL_PRIVATE` tokens:

* seat 1 has already emitted `DEAL_PRIVATE` at `(T, k, s)`;
* seat 1 must now emit `TIMEOUT_VOTE` at `(T, k, s)` — the same four fields, the
  same key, a different body;
* by §5.2 this **is** equivocation, and any peer can assemble a valid
  `EquivocationProof` against seat 1 for doing what §8.3 requires it to do.

Every honest voter self-incriminates. The `kind = 2` certificate — the *only*
route to `HAND_ABORT cause = 1`, which is D-005's entire answer to a peer that
stops publishing decryption shares (`THREAT_MODEL.md` §7.3 case (c),
`STATE_MACHINE.md` T27/T41/T44) — cannot be produced without every voter forging
evidence against itself.

**The correction.** Give a `TIMEOUT_VOTE` its own slot. Two workable shapes:

* a distinct sub-slot: extend the equivocation key to
  `(protocol_version, table_id, hand_id, sequence, event_class)` where
  `event_class` separates chain events from votes and certificates; or
* place the vote at `sequence = subject_sequence` but in a separate *vote chain*
  keyed by `subject_digest`, with the certificate — not the vote — being the
  chain event at `subject_sequence`.

Either way `PROTOCOL.md` §4.8's "mutual exclusion" claim must be dropped, since
per A-2 it never bought anything.

---

## A-6 — The unanimity-minus-one eviction attack is described in `PROTOCOL.md`, is required to be in `THREAT_MODEL.md`, and is not there

**Where.** `PROTOCOL.md` §6.3 step 4 case (c) and its "The attack this rule
enables" paragraph; `THREAT_MODEL.md` §5.3 (X1–X28) and §9.1.

**What is claimed.**

> `PROTOCOL.md` §6.3: "`n-1` colluding players can falsely claim a divergent
> state hash, invoke this rule against one honest player, remove them, and take
> their committed chips under D-005's forfeiture. This is real. … **`THREAT_MODEL.md`
> must carry this as a named, unsolved attack.**"

**Why it is wrong.** It does not. Grepping `THREAT_MODEL.md` for
`unanimity`, `unanimity-minus-one`, `removed from the table` and
`falsely claim` returns nothing. The nearest entry, X22, covers only *honest
implementation-bug* divergence and is classified DNA with no chip movement. The
catalogue therefore omits an in-protocol attack in which a coalition takes an
honest player's committed chips at any point in a hand, at will, without ever
sending an invalid signature or an invalid proof. It is materially worse than
X10, which is in the catalogue.

Note also that at `n = 3` a coalition of 2 is `n-1`, so this is available at the
smallest multi-player table the design targets.

**The correction.** Add it to `THREAT_MODEL.md` §5.3 as a new row (suggest X29),
classified honestly: *the eviction is not adjudicable live; the evidence is
publicly adjudicable offline only, and the chips have already moved.* Add it to
§9.1 as a permanent limitation and to §9.2 as an open question. Fixing A-3
option (i) — attributing a `state_hash` that contradicts the unanimous transcript
— removes both this attack and the `cause = 4` escape, so the two findings should
be resolved together.

---

## A-7 — `X7` (withholding shares) is classified "detected **and attributed**" while two documents record that attribution is unresolved for simultaneous failures

**Where.** `THREAT_MODEL.md` §5.3 X7 (class **D&A**), §7.3 case (c)2;
`PROTOCOL.md` §8.4 and Q-02; `STATE_MACHINE.md` §8.4 open question Q3.

**What is claimed.**

> `THREAT_MODEL.md` X7: "**D&A** … The missing `REVEAL_TOKEN` for a given
> `(hand_id, card_index)` is publicly visible and every other player's signed
> events prove they did their part, **so attribution is sound.**"

**Why it is unsupported.** Attribution is only produced by a `TIMEOUT_CERT`, and
a certificate requires signatures from every other still-active dealt-in seat.
Both downstream documents record that this is unachievable when more than one
seat is silent:

> `PROTOCOL.md` §8.4: "If two seats become subjects at once, neither certificate
> can reach unanimity … **D-006 did not settle simultaneous failure and this rule
> is this document's construction, not the owner's decision. It is OPEN QUESTION
> Q-02.**"

> `STATE_MACHINE.md` §8.4: "`signers == participants \ {subject}` becomes
> unachievable if two seats are gone. … **This document does not fix it.**"

Two colluders going silent together — the cheapest possible version of X7 — is
therefore neither reliably detected-with-attribution nor reliably aborted; it
stalls until `hand_deadline_ms`, whose certificate has the same problem, and
`PROTOCOL.md` §8.4's fallback ("every non-voting seat is attributed") is an
unratified construction that would attribute honest seats caught behind a
partition. It also punishes them with D-005 forfeiture.

**The correction.** Downgrade X7 in `THREAT_MODEL.md` to **D&A for a single
silent seat, DNA when two or more seats stop simultaneously**, and cite Q-02/Q3
in the justification column. Add the multi-subject certificate to
`THREAT_MODEL.md` §9.2 as a blocking open question (it currently appears only in
the two downstream documents, so the threat model's own risk register does not
show it).

---

## A-8 — The relay byte budget compares table-wide traffic against a **per-circuit** cap, overstating the constraint that justifies D-002 by a factor of `n`; and the D-002 relay config as written cannot serve a full table

**Where.** `CRYPTOGRAPHY.md` §6.5 consequence 2; `PROTOCOL.md` §9.3 closing
paragraph; `THREAT_MODEL.md` §3.5, X20, OQ12; `NETWORK_STACK.md` §9.5, §9.6.

**What is claimed.**

> `CRYPTOGRAPHY.md` §6.5: "Shuffle traffic alone is `n × (5547 + 3432)` bytes per
> hand: **18 KB heads-up, 54 KB six-handed** … so a public relay carries on the
> order of **two six-handed hands** before it resets."

> `PROTOCOL.md` §9.3: "Total shuffle traffic per hand is `n × 8 979` B … which is
> the number that makes the 128 KiB default relay budget of D-001 a real
> constraint."

**Why it is wrong.** `max_circuit_bytes` is a **per-circuit, per-direction**
limit, not a per-table one. Verified in source:
`libp2p-relay-0.21.1/src/behaviour.rs` `impl Default for Config`
(`max_circuit_bytes: 1 << 17`), enforced per connection in
`src/behaviour/handler.rs:59-60, 421-422, 927-945`.

The table is a full mesh (`PROTOCOL.md` §1.5, `NETWORK_STACK.md` §8.2), so a
given circuit connects exactly one pair. Over the circuit A↔B, the shuffle
traffic per hand is A's own `SHUFFLE_STEP` + `SHUFFLE_PROOF` (8 979 B) in one
direction and B's (8 979 B) in the other — **independent of `n`**. Against
131 072 B that is ~**14 hands per direction**, not "two six-handed hands". The
`n ×` factor is spread across `n-1` separate circuits, each with its own budget.

The real public-relay constraint is the **2-minute `max_circuit_duration`**, not
the byte cap. That should be said plainly, because the byte figure is the number
three documents quote as the evidence for D-002's necessity.

**A second, harder problem in the same area, not currently stated anywhere.**
`NETWORK_STACK.md` §9.6's normative relay config raises
`max_circuit_duration`, `max_circuit_bytes` and `max_reservations` but leaves

```
max_circuits:          16   (default)
max_circuits_per_peer:  4   (default)
```

Verified in the same `impl Default for Config`. A relayed peer at a six-seat
table needs **five** simultaneous circuits (one per table-mate) through its
relay; the fifth is refused by `max_circuits_per_peer = 4`. And 16 total circuits
means one D-002 volunteer can serve roughly three relayed peers at a six-max
table. As written, the D-002 relay cannot carry the case it exists for.

**The correction.** Fix the arithmetic in `CRYPTOGRAPHY.md` §6.5,
`PROTOCOL.md` §9.3 and `THREAT_MODEL.md` §3.5/OQ12 to a per-circuit basis; state
that duration, not bytes, is the binding public-relay limit; and raise
`max_circuits_per_peer` to at least `MAX_SEATS - 1 = 9` and `max_circuits`
correspondingly in `NETWORK_STACK.md` §9.6, with the bandwidth implication
disclosed in the D-002 consent screen.

---

## B-1 — "The `rand` crate is deliberately absent from the dependency tree" is false, and `CRYPTOGRAPHY.md` already says so

**Where.** `PROTOCOL.md` §4.4 (`RNG_COMMIT`); `THREAT_MODEL.md` §2 assumption A7.
Contradicted by `CRYPTOGRAPHY.md` §7.2 and OQ-8.

**What is claimed.**

> `PROTOCOL.md` §4.4: "the `rand` crate is deliberately absent from the
> dependency tree so that `SmallRng` and `StdRng` are structurally unavailable —
> which is `SPEC_CS.md` §7's requirement **enforced by the dependency graph**
> rather than by reviewer discipline."

> `THREAT_MODEL.md` A7: "The `rand` crate is deliberately absent from the
> dependency tree, which enforces §7's prohibition on `SmallRng`/`StdRng`
> **structurally** rather than by reviewer discipline."

**Why it is wrong.** Adding `ziffle` puts `rand 0.8` in the **runtime** tree, as
a non-optional dependency of `ark-std`. Verified by reading
`ark-std-0.5.0/Cargo.toml`:

```toml
[dependencies.rand]
version = "0.8"
features = ["std_rng"]
default-features = false
```

`std_rng` is what gates `StdRng`, so `StdRng` is compiled in. `ziffle` itself
imports it: `ziffle-0.1.0/src/lib.rs:127`
`use ark_std::rand::{RngCore as Rng, SeedableRng, rngs::StdRng, seq::SliceRandom};`

`CRYPTOGRAPHY.md` §7.2 states this correctly, calls it a "Finding that corrects
`CRYPTO_LIBS.md` §1.2", and records in OQ-8 that "the structural guarantee is
gone" and "§7 is enforced by review discipline alone" until a CI lint exists.

**The correction.** `PROTOCOL.md` §4.4 and `THREAT_MODEL.md` A7 must be rewritten
to match `CRYPTOGRAPHY.md` §7.2: `rand 0.8.8` **is** in the runtime tree via
`ark-std 0.5.0`; `SmallRng` is out only because the `small_rng` feature is not
enabled (verified, `rand-0.8.8/Cargo.toml` `[features] small_rng = []`);
`StdRng` is present and used by `ziffle` for nothing-up-my-sleeve public
constants; and §7 is enforced by the CI lint of `CRYPTOGRAPHY.md` §12 item 6, not
by the dependency graph. `THREAT_MODEL.md` A7's "*If false*" clause should name
the lint as the mitigation.

---

## B-2 — `relay::RateLimiter` is publicly re-exported; the claim that it "cannot be named from outside the crate" is wrong

**Where.** `NETWORK_STACK.md` §9.6; `THREAT_MODEL.md` §5.3 X21
(and carried from `DECISIONS.md` D-002).

**What is claimed.**

> `NETWORK_STACK.md` §9.6: "Their element type lives in a `pub(crate)` module and
> **cannot be named from outside**, but the crate carries a blanket
> implementation … so a closure coerces into the vector without naming the trait."

> `THREAT_MODEL.md` X21: "a closure coerces into those vectors through a blanket
> impl **without naming the crate-private trait**".

**Why it is wrong.** The *module* is `pub(crate)`
(`libp2p-relay-0.21.1/src/behaviour.rs:24` — `pub(crate) mod rate_limiter;`),
but the trait itself is re-exported at the crate root:

```rust
// libp2p-relay-0.21.1/src/lib.rs:42
pub use behaviour::{rate_limiter::RateLimiter, Behaviour, CircuitId, Config, Event, StatusCode};
```

So `libp2p::relay::RateLimiter` is nameable, and a downstream type may implement
it directly rather than relying on the blanket impl. The blanket impl is real and
the closure approach works — verified,
`src/behaviour/rate_limiter.rs:38` (`pub trait RateLimiter: Send`) and `:56`
(`impl<T: FnMut(PeerId, &Multiaddr, Instant) -> bool + Send> RateLimiter for T`) —
but the stated reason for using it is not.

**The correction.** Say that the closure form is chosen for brevity and that
`libp2p::relay::RateLimiter` is a public trait available for a named
implementation with state (which admission control will want, since the closure
must capture an `Arc<Mutex<HashSet<PeerId>>>` and be `Send`). Remove
"cannot be named from outside" from both documents.

---

## B-3 — The RustSec status table's `rand` row is stale, and nobody re-checked the advisory against the version actually pulled in

**Where.** `research/CRYPTO_LIBS.md` §7.3, relied on by `CRYPTOGRAPHY.md` §9 and
`THREAT_MODEL.md` A7.

**What is claimed.**

> `CRYPTO_LIBS.md` §7.3: "| `rand` | RUSTSEC-2026-0097 | **Crate not in the tree
> at all** |"

`CRYPTOGRAPHY.md` §9's "Supply-chain finding that changes `CRYPTO_LIBS.md` §4.4"
corrects only the `paste`/RUSTSEC-2024-0436 row and leaves this one standing.

**Why it matters.** Per B-1 the crate **is** in the runtime tree at 0.8.8. The
advisory must therefore be evaluated rather than dismissed. It happens to be
benign here — verified in the local advisory database,
`~/.cargo/advisory-db/crates/rand/RUSTSEC-2026-0097.md`:

```toml
informational = "unsound"
[versions]
patched = [">= 0.10.1", "< 0.10.0, >= 0.9.3", "< 0.9.0, >= 0.8.6"]
```

`0.8.8 >= 0.8.6`, so the pin is patched, and the unsound path requires
`rand::thread_rng` inside a custom `log` implementation, which does not apply.

**The correction.** Update the row to "in the tree at 0.8.8 via `ark-std`;
patched (`>= 0.8.6`); the affected function `rand::thread_rng` is unreachable
here", and add the `rand`/`rand_chacha`/`rand_core` chain to the dependency
register (see D-3). A row that says "not in the tree at all" about a crate that
is in the tree is the kind of thing that survives into a future audit unnoticed.

---

## B-4 — `SPEC_CS.md` §16's `TIMEOUT` message and §12's field list are satisfied only partially, and `PROTOCOL.md` §2.8 contradicts §2.4 on the literal domain bytes

**Where.** `PROTOCOL.md` §2.4 vs §2.8.

**What is claimed.**

> §2.4: "`DOMAIN_EVENT` = the 24 ASCII bytes `"p2p-poker/v1/event\0\0\0\0\0\0"`"

> §2.8 domain table: "| `p2p-poker v1 event` | the 24-byte `DOMAIN_EVENT`
> signature prefix (§2.4) — a literal prefix, not this hasher |"

**Why it matters.** These are two different byte strings (`/` vs space) for a
value that every signature in the protocol is computed over. `"p2p-poker/v1/event"`
is 18 bytes + 6 NULs = 24, which is self-consistent; `"p2p-poker v1 event"` is
also 18 bytes, so the length check does not disambiguate them. Two
implementations reading different sections of the same document produce
signatures that do not verify against each other.

**The correction.** Fix §2.8's table entry to quote `p2p-poker/v1/event`
verbatim, or change §2.4. Add the literal 24 hex bytes to the §13 constants block
so there is one unambiguous source.

---

## C-1 — Every shared size constant differs between `PROTOCOL.md` and `NETWORK_STACK.md`, and both call them two-sided protocol constants

**Where.** `PROTOCOL.md` §7.1, §9.2, §9.3, §13; `NETWORK_STACK.md` §6.2, §6.5,
§7.1, §8.4, §11.3, §14.

| Constant | `PROTOCOL.md` | `NETWORK_STACK.md` |
|---|---:|---:|
| GossipSub `max_transmit_size` | **16 384** (§7.1, §9.2 `LOBBY_MAX_MESSAGE`, §13) | **65 536** (§6.2, §11.3, §14 `GOSSIP_MAX_TRANSMIT`) |
| Any lobby message, application cap | — (16 384 is the only figure) | **8 192** (`LOBBY_MSG_MAX`) |
| `LOBBY_TABLE_AD` payload cap | **2 048** (§9.3) | **1 024** (§6.5, §14 `TABLE_AD_MAX`) |
| Table stream max frame | **262 144** (§9.2 `TABLE_MAX_FRAME`, §13) | **131 072** (§8.4, §11.3, §14 `TABLE_FRAME_MAX`) |
| Snapshot request max | **1 024** (§7.5, §13) | **4 096** (§7.1, §14) |
| Snapshot response max | **524 288** (§7.5, §13) | **262 144** (§7.1, §14) |
| Adverts per snapshot | **128**, each ≤ 2 560 B (§7.5, §9.4) | **256**, each ≤ 1 024 B (§7.3, §14) |
| Join request/response cap | — (`JOIN_ACCEPT` payload 8 192, §9.3) | **16 384 / 16 384** (§8.4, §14) |

Both documents insist these are not tuning knobs:

> `PROTOCOL.md` §7.1: "**This is a two-sided protocol constant, not a tuning
> knob**: a peer with a different value rejects our frames. Changing it is a
> major-version change."

> `NETWORK_STACK.md` §14: "Changing any value in this table is a
> **protocol-version change**, not a tuning knob, because both sides must agree."

The `LOBBY_TABLE_AD` pair is internally inconsistent as well: `PROTOCOL.md` §4.3
requires `JOIN_ACCEPT` to embed "the complete `SignedEvent` of the
`LOBBY_TABLE_AD`" at up to **2 560 B**, which exceeds `NETWORK_STACK.md`'s
1 024 B advert cap.

**The correction.** One constants table, in one document, referenced by the other.
`PROTOCOL.md` §13 is the natural home since it already carries the wire-format
constants; `NETWORK_STACK.md` §14 should point at it rather than restate it.
Note that on the merits `NETWORK_STACK.md` is right about `max_transmit_size`:
65 536 is the crate default (verified,
`libp2p-gossipsub-0.49.5/src/config.rs:244-246`, `default_max_transmit_size() -> 65536`),
and pinning at the default maximises interoperability.

---

## C-2 — Two incompatible normative definitions of `ctx`, the string on which every proof's replay resistance depends

**Where.** `CRYPTOGRAPHY.md` §6.4 vs `PROTOCOL.md` §4.5.

> `CRYPTOGRAPHY.md` §6.4, "The binding rule, normative":
> ```
> ctx = "p2ppoker/v1" ‖ 0x00 ‖ protocol_version ‖ table_id ‖ session_nonce
>                     ‖ hand_id ‖ shuffle_round ‖ shuffler_application_pubkey
> ```

> `PROTOCOL.md` §4.5:
> ```
> ctx = h("p2p-poker v1 deck-ctx",
>         [ u16_be(protocol_version), table_id, session_id,
>           u64_be(hand_id), u64_be(sequence), sender_public_key ])
> ```

These differ in construction (raw concatenation vs a 32-byte BLAKE3 digest), in
one field (`session_nonce` vs `session_id`; `shuffle_round` vs `sequence`), and
in the domain string. `CRYPTOGRAPHY.md` §14's cross-reference table says
`PROTOCOL.md` "must carry … the exact `ctx` construction (§6.4)"; `PROTOCOL.md`
§4.5 instead defines its own without noting the divergence.

This is not cosmetic. `THREAT_MODEL.md` A11 makes the whole cross-hand and
cross-table replay resistance of every shuffle proof and every reveal token
depend on `ctx`, and both documents' own open questions (`CRYPTOGRAPHY.md` OQ-3,
`THREAT_MODEL.md` OQ3) say it is "our bug to make". Two implementers reading two
documents will build two incompatible protocols in which no proof verifies.

**The correction.** Pick one — `PROTOCOL.md` §4.5's hashed form is better, since
it is fixed-length and reuses the one audited length-prefixed hasher — and make
the other document reference it. Confirm that `session_id` (`PROTOCOL.md` §4.3)
and `session_nonce` (`SPEC_CS.md` §14, §20) are the same object, and say so.

---

## C-3 — Burn cards: `CRYPTOGRAPHY.md` has them, `PROTOCOL.md` and `STATE_MACHINE.md` explicitly do not

**Where.** `CRYPTOGRAPHY.md` §2.4 and §2.8 item 3 vs `PROTOCOL.md` §4.5 vs
`STATE_MACHINE.md` §7.8.

> `CRYPTOGRAPHY.md` §2.4:
> ```
> index 2n        : burn
> index 2n+1 .. 2n+3 : flop
> index 2n+4      : burn
> index 2n+5      : turn
> index 2n+6      : burn
> index 2n+7      : river
> ```
> and §2.8 item 3: "Burn cards are simply never opened."

> `PROTOCOL.md` §4.5: "**There are no burn cards.** … This is a deliberate
> departure from live procedure and is recorded as such." Flop = `2m, 2m+1,
> 2m+2`; turn = `2m+3`; river = `2m+4`.

> `STATE_MACHINE.md` §7.8: "**No burn cards** (**[OUR CHOICE]**)." Same indices.

The deck-index map is hashed into `index_map_hash` (`PROTOCOL.md` §4.4
`DECK_COMMIT`) and is the object `THREAT_MODEL.md` A10 and X2 are about, so a
disagreement here is a guaranteed `DECK_COMMIT` mismatch between two conforming
clients, i.e. a manufactured §15 dispute at every hand.

**The correction.** `CRYPTOGRAPHY.md` §2.4 and §2.8 item 3 must be corrected to
the no-burn layout. Its own parenthetical — "(The exact layout is `PROTOCOL.md`'s
to state …)" — is not enough when it prints a conflicting concrete table.

---

## C-4 — `DEAL_PRIVATE`: point-to-point in `CRYPTOGRAPHY.md`, broadcast in `PROTOCOL.md`, and the security argument given in `CRYPTOGRAPHY.md` does not apply to the shipped design

**Where.** `CRYPTOGRAPHY.md` §2.7 and §2.9 vs `PROTOCOL.md` §3.4 and §4.6.

> `CRYPTOGRAPHY.md` §2.7: "every **other** player sends Alice their token for
> exactly those two indices, **privately, over the direct authenticated libp2p
> stream to Alice**." And: "**Bob receives no token for Alice's indices from
> anybody**, so for Bob those two ciphertexts stay ciphertexts."

> `PROTOCOL.md` §4.6: `DEAL_PRIVATE` is a **collective stage**, "every `dealt_in`
> seat, **to every other**", carrying "exactly the hole-card indices of every
> dealt-in seat *other than the sender*". §3.4: "Broadcasting the `n-1` tokens is
> therefore not a leak."

The conclusion (Bob cannot open Alice's card) survives in both designs, but the
*reason* stated in `CRYPTOGRAPHY.md` §2.7 — that Bob receives no token — is false
under the design that is actually specified. `PROTOCOL.md`'s counting argument
(at most `n-1` tokens for any unopened index) is the correct one and is the one
the threat model's G1 rests on.

Broadcast also has a real consequence `CRYPTOGRAPHY.md` does not carry: it is what
makes a showdown one message (`PROTOCOL.md` §4.6 `SHOWDOWN_REVEAL`, 2 × 131 B) and
what makes mucking cryptographically free rather than a policy question at the
protocol level. `CRYPTOGRAPHY.md` §2.9's per-hand budget ("`n−1` tokens per hole
index, point-to-point") is likewise sized for the wrong topology.

**The correction.** Rewrite `CRYPTOGRAPHY.md` §2.7 and §2.9 for the broadcast
design and replace the "Bob receives no token" argument with the counting rule.

---

## C-5 — `HAND_INIT` / `HAND_COMPLETE`: signed single-writer events in `PROTOCOL.md`, unsigned peer-derived events in `STATE_MACHINE.md`

**Where.** `STATE_MACHINE.md` §3.4 and Q2 vs `PROTOCOL.md` §4.4 (`HAND_INIT`),
§4.10 (`HAND_COMPLETE`), §4.0 step 9; `SPEC_CS.md` §12.

> `STATE_MACHINE.md` §3.4: "`HAND_COMPLETE`, `HAND_INIT` for the next hand, and
> the settlement step are *computed*, not decided … The engine emits them in
> `Step::derived`; every peer emits byte-identical derived events independently
> and inserts them into the hash chain at the same sequence number. **They carry
> no signature, because there is no signer.**"

> `PROTOCOL.md` §4.4: "`0x0303 HAND_INIT` — *Direction:* **single-writer stage 0**
> of chain `hand_id = k` … The writer is the seat holding the button position",
> and §4.0 step 9 makes `verify_strict` mandatory for every `SignedEvent`, with
> §3.2 `stage_hash` for a single-writer stage taking `u8(writer_seat)`.

Both cannot be true: a single-writer stage hash needs one writer's `event_hash`,
whereas `n` independently derived unsigned copies have no writer and, per
§4.0 step 9, would all be dropped. `SPEC_CS.md` §12 requires `sender_public_key`
and `signature` on every critical event, which the unsigned form does not
provide.

`STATE_MACHINE.md` flags this as its own Q2 "for `PROTOCOL.md`", but
`PROTOCOL.md` answers it differently and silently, so the open question is
recorded as open in one document and as closed-the-other-way in the other.

**The correction.** `PROTOCOL.md`'s answer (signed, single-writer, every field
recomputed and rejected on mismatch) is the right one and should be adopted in
`STATE_MACHINE.md` §3.4, with Q2 closed and the `Step::derived` mechanism
redescribed as *"the button seat serialises the stage; every other peer
recomputes and rejects a mismatch"*.

---

## C-6 — `STATE_MACHINE.md` has no representation of `SPEC_CS.md` §15's dispute path, and two of `PROTOCOL.md`'s five abort causes have no state-machine counterpart

**Where.** `STATE_MACHINE.md` §4.1 `Event`, §5.2 transition table, §8.6
`AbortKind` vs `PROTOCOL.md` §4.9, §4.10 `HAND_ABORT`, §6.3.

`STATE_MACHINE.md` lists `SPEC_CS.md` §15 in its binding inputs, yet:

* the `Event` enum contains no `StateHash`, `StateAck` or `Dispute` variant, so
  the checkpoint and divergence machinery cannot enter the engine at all;
* `AbortKind` is `NoKey | InvalidShuffleProof | NoShuffle | NoDealTokens |
  NoBoardTokens | NoShowdownToken | HandDeadline` — there is no
  `StateDivergence` and no `Equivocation`, so `PROTOCOL.md`'s `HAND_ABORT`
  `cause = 4` and `cause = 5` are unrepresentable;
* the 47 transitions contain no divergence freeze, no reconciliation, no
  unanimity-minus-one removal and no faulted-table transition, although
  `PROTOCOL.md` §6.4 specifies all four.

**A dangling event.** `Event::RevealRejected { seat, index, reason }` is declared
in §4.1 and consumed by **no** transition. By the table's own rule — "A transition
not listed does not exist; any event arriving in a state with no matching row is a
`Rejection`" — a failed Chaum–Pedersen DLEQ verification produces a verdict the
engine silently discards, and the hand then stalls until a timeout instead of
aborting. `PROTOCOL.md` §4.10 `cause = 3` ("invalid reveal proof") expects an
immediate attributed abort, matching `T21`'s treatment of `ShuffleRejected`.
`CRYPTOGRAPHY.md` §8 rule 1 also treats an unverifiable token as a violation.

**The correction.** Add `StateHash`, `StateAck`, `Dispute` to the event alphabet;
add a `Diverged` (frozen) phase and the §6.3 step 1–4 transitions; add
`StateDivergence` and `Equivocation` to `AbortKind`; and add a transition
`AwaitingDeal | next_reveal(S) | AwaitingShowdownReveal + RevealRejected →
HandAborted` with `Fault{InvalidRevealProof}`. Update the "19 phases, 26
invariants, 47 transitions" counts accordingly.

---

## C-7 — The join flow uses a different transport and a different protocol string in each document

**Where.** `PROTOCOL.md` §1.1, §1.4, §4.3, §4.11 vs `NETWORK_STACK.md` §5.5,
§8.4, §14.

> `PROTOCOL.md` §1.1 lists exactly four protocol strings and none of them is a
> join protocol; §4.3 places `JOIN_REQUEST` / `JOIN_ACCEPT` / `JOIN_REJECT` on the
> **table stream** (`/p2p-poker/table/1`), and §4.11's channel column confirms it.
> §1.4: "A message arriving on the wrong channel is **dropped** and its sender is
> rate-limited."

> `NETWORK_STACK.md` §5.5 declares a dedicated behaviour
> `join: request_response::cbor::Behaviour<JoinRequest, JoinResponse>`, §8.4 says
> "**Join** goes over `request-response` on `/p2p-poker/join/1` … 16 KiB caps both
> ways, 20 s timeout", and §14 pins `JOIN_PROTOCOL = /p2p-poker/join/1`.

Given §1.4's rule, a client built from one document drops the other's join
messages outright. `PROTOCOL.md`'s framing rules (`u32` length prefix on the table
stream) and `NETWORK_STACK.md`'s (`request-response` codec framing) are also
incompatible on the wire.

**The correction.** Choose one. `request-response` is the better fit — join is a
one-shot RPC with a natural timeout and free size caps, and it avoids opening a
table stream to a peer that has not been admitted (which `NETWORK_STACK.md` §8.4's
own membership gate would otherwise have to special-case). Then add
`/p2p-poker/join/1` to `PROTOCOL.md` §1.1 and §13, and move the three join
messages to a "lobby RPC"-style channel row in §1.4 and §4.11.

---

## C-8 — Lobby chat exists in the transport document and in nothing else

**Where.** `NETWORK_STACK.md` §6.1, §6.5, §6.6, §14
(`LOBBY_CHAT_TOPIC = /p2p-poker/lobby-chat/1`, 2 048 B cap, 1 message per 2 s)
vs `PROTOCOL.md` §1.1, §4 (38 message types), §4.11, §13.

`SPEC_CS.md` §22 requires a chat pane in the lobby, and `research/GUI_STACK.md`
§9 item 5 records it as part of the PokerTH-style lobby. `NETWORK_STACK.md`
provisions a topic, a size cap and a rate limit for it. `PROTOCOL.md` defines no
chat `event_type`, no payload, no validation rule, and does not list the topic
among its protocol strings — and its §4.11 note says codes `0xF000`–`0xFFFF` are
"never accepted inside a table session", which leaves chat with no code at all.

Chat is also the one message class that is pure attacker-controlled UTF-8 reaching
a UI, so it needs the §9.4 string rules (length, no control characters, no bidi
overrides) applied explicitly.

**The correction.** Add `0x0106 LOBBY_CHAT` to `PROTOCOL.md` §4, with the sender's
application signature, the §9.4 string constraints, a cap consistent with C-1, and
the §7.6 rate limit; or delete the topic from `NETWORK_STACK.md` and record chat
as deferred.

---

## C-9 — Invariant I1 (chip conservation) is false for cash mode, which the same document specifies

**Where.** `STATE_MACHINE.md` §10 I1 vs §9.4; `PROTOCOL.md` §7.2 fields
`n(7) min_buyin`, `n(8) max_buyin`, `n(9) start_stack` ("tournament modes only;
`0` for cash"); `THREAT_MODEL.md` G9.

> `STATE_MACHINE.md` I1: "`Σ_s stack[s] + Σ_s committed_hand[s] == total_chips`,
> where `total_chips = players_at_start × start_stack`, **constant for the table's
> whole life**"

> `STATE_MACHINE.md` §9.4: "a seat may be added or removed at a **hand boundary**
> only. `PlayerSeated` and `PlayerLeft` arriving mid-hand are recorded and applied
> at the next T47"

A seat added at a hand boundary arrives with a `buyin` (`PROTOCOL.md` §4.3
`JOIN_REQUEST` `n(6) buyin`), which changes the total; a seat leaving removes its
stack. And in cash mode `start_stack = 0`, so I1's formula evaluates to
`total_chips = 0`. As written, the property test fails on every cash hand.

`THREAT_MODEL.md` G9 has the same problem: it states chip conservation "for the
whole tournament" and then requires it on "**every** path", including a D-005
abort.

**The correction.** Restate I1 as a **step-local** invariant plus an explicit
ledger: `Σ stacks + Σ committed == Σ(buy-ins) − Σ(cash-outs)`, with buy-in and
cash-out as signed, chained events so the ledger itself is verifiable. I2 already
carries the step-local half; I1 should become the ledger identity. Update
`THREAT_MODEL.md` G9 to match.

---

## C-10 — `CERT_SETTLE_MS` puts a wall-clock read back into the chain-building rule the same documents say has no clocks

**Where.** `PROTOCOL.md` §4.8 (certificate tie-break), §13
(`CERT_SETTLE_MS = 2 000`), Q-04 vs `PROTOCOL.md` §2.6 and §8.2;
`STATE_MACHINE.md` §3.1, §3.2, §8.2.

> `PROTOCOL.md` §4.8: "**the certificate that closes the stage is the one from the
> lowest seat index that emitted a valid one within `CERT_SETTLE_MS`**, and every
> peer waits that long before chaining."

> `PROTOCOL.md` §8.2: "**The engine contains no clock.** Time enters the state
> machine only as a signed `TIMEOUT_CERT`."

> `STATE_MACHINE.md` §3.1: "inside the `poker` crate: **no clock** —
> `std::time::{Instant, SystemTime}`, `chrono`, any OS time source: forbidden".

Which certificate becomes `stage_hash(s)` is chain content, and here it is decided
by a local 2-second timer. Two peers whose timers differ by more than the
propagation delay of a competing certificate chain different bodies at stage `s`
and diverge — with byte-identical transcripts up to `s-1` — landing in
`PROTOCOL.md` §6.3 case (c), which per A-3 restores stacks heads-up and per A-6
evicts an honest player at `n ≥ 3`. `STATE_MACHINE.md` has no `CERT_SETTLE_MS`
and models the certificate as an ordinary event with no tie-break at all.

`PROTOCOL.md` records this as Q-04, so it is flagged; what is not flagged is that
the failure mode of getting it wrong is the A-3/A-6 exploit path, not merely a
retry.

**The correction.** Make the certificate stage **collective** (Q-04's own second
option): the required emitter set is `V(subject)`, each voter emits its own
certificate, and `stage_hash` is the collective form. That removes the timer, the
tie-break and the fork, at the cost of `|V|` small messages. Record the resolution
in `STATE_MACHINE.md` too.

---

## C-11 — `SPEC_CS.md` §16 places `RNG_COMMIT`/`RNG_REVEAL` per hand; the design moves them to once per table, and only one of four documents explains the deviation to the reader who starts from the spec

**Where.** `SPEC_CS.md` §16 (message order: `HAND_INIT`, then `RNG_COMMIT`,
`RNG_REVEAL`, then `DECK_INIT`) vs `PROTOCOL.md` §4.4 (setup chain, "run exactly
once per table"), `STATE_MACHINE.md` T6–T12 ("The beacon runs **once per table**,
not per hand"), `CRYPTOGRAPHY.md` §7.3.

The deviation is well argued — `research/MENTAL_POKER.md` §6 establishes that the
shuffle chain *is* the per-hand randomness — and `PROTOCOL.md` §4.4 states it
plainly. It is listed here because it is a **deviation from a binding spec
section** and, unlike the `OsRng` rename (which `CRYPTOGRAPHY.md` §7.2 records
under an explicit "Recorded spec deviation" heading), it is not marked as one
anywhere. `SPEC_CS.md` §36 and the Phase 0 brief both require deviations to be
recorded rather than absorbed.

**The correction.** Add a short "recorded spec deviation" note in
`PROTOCOL.md` §4.4 and in `CRYPTOGRAPHY.md` §7.3 in the same form used for
`OsRng`, and list it in `THREAT_MODEL.md` §9.1 so the deviation register is in one
place.

---

## D-1 — `SPEC_CS.md` §24 (`InMemoryTransport`) is named in one sentence and specified nowhere

**Where.** `SPEC_CS.md` §24; the only mention across all twelve documents is
`NETWORK_STACK.md` §1.3: "the whole poker and cryptographic stack can run on
`InMemoryTransport` with no DHT and no libp2p (`SPEC_CS.md` §24)".

§24 requires a named component and enumerates eight capabilities it must support:
delay, duplicate, packet loss, reordered events, disconnect, reconnect, malicious
packets, conflicting messages — and states the acceptance bar ("thousands of hands
tested automatically without DHT and without the network"). No document defines
the trait `net/` exposes upward, the fault-injection API, determinism/seeding of
the simulated network, or how a *conflicting message* (i.e. an equivocation) is
injected.

This matters more than a test-harness gap normally would, because it is the only
vehicle for `SPEC_CS.md` §25's adversarial suite, and because every finding above
(A-1 … A-5) is exactly the kind of thing a conflicting-message and
timeout-injection harness would have caught at specification time.

**The correction.** `NETWORK_STACK.md` §1.3 should specify the upward trait
(`send(PeerId, Bytes)`, `events() -> Stream<TransportEvent>`,
`connection_state(PeerId)`) and an `InMemoryTransport` with a seeded, deterministic
scheduler plus the eight injection modes named in §24. Determinism of the harness
is a requirement, not a nicety, since `STATE_MACHINE.md` I22 asserts replay
determinism.

---

## D-2 — `SPEC_CS.md` §28's dependency register exists for exactly one crate

**Where.** `SPEC_CS.md` §28 requires, *for every security-critical dependency*:
name, version, purpose, repository, licence, security status.

`CRYPTOGRAPHY.md` §9 provides that table for **`ziffle` only**.
`research/CRYPTO_LIBS.md` has an advisory table (§7.3) and a licence table (§7.4)
and a version list (§10), but no register with repository and purpose per crate,
and it predates the ziffle-induced changes (see B-3). Nothing covers the libp2p
side at all — `libp2p 0.56.0` and its fourteen sub-crates, `libp2p-stream
0.4.0-alpha` (semver-exempt, explicitly flagged as a risk) and `mainline 8.0.0`
are security-critical by any reading and appear in no register.

**The correction.** One `docs/DEPENDENCIES.md` with the six §28 columns for every
crate the client links, generated from `cargo metadata` and checked in CI so it
cannot drift. Flag `libp2p-stream 0.4.0-alpha` and `ziffle 0.1.0` explicitly as
the two unaudited, semver-unstable entries.

---

## D-3 — `SPEC_CS.md` §25's eleven named malicious peers are never mapped to tests

**Where.** `SPEC_CS.md` §25 names `CheaterDuplicateAce`, `CheaterReplaceCard`,
`CheaterInvalidShuffle`, `CheaterPredictableRNG`, `CheaterReplayAction`,
`CheaterIllegalRaise`, `CheaterFakeStack`, `CheaterEquivocation`,
`CheaterReadOpponentCard`, `CheaterFutureBoard`, `CheaterDisconnect`, plus two
mandatory tests.

Coverage is partial and scattered: `THREAT_MODEL.md` §4 names nine of them as
falsifiers of G1–G7; `CRYPTOGRAPHY.md` §12 item 8 lists library-level regression
probes (T1, T3, T4, T6b, T7a/b/d/e, T8) plus the six OQ-3 replay cases;
`STATE_MACHINE.md` §10 attaches four cheaters to I20/I21/I23. No document contains
the mapping *cheater → the test that runs it → the assertion it must satisfy*, and
§25's requirement that each test prove "the cheat is cryptographically impossible
**or** unambiguously detected and the hand stopped, per the threat model" is
nowhere discharged per-cheater. `CheaterPredictableRNG` in particular has no test
described anywhere.

**The correction.** A table in `THREAT_MODEL.md` §5 with a fourth column: cheater
name → catalogue row → test module → asserted outcome (CP or D&A). It also makes
the CP/D&A classification falsifiable, which §4 says it wants to be.

---

## D-4 — `SPEC_CS.md` §23 (source architecture) is covered by no Phase 0/1 document

Only `NETWORK_STACK.md` §1.1 sketches `net/` (`dht.rs`, `swarm.rs`, `lobby.rs`,
`streams.rs`) and `CRYPTOGRAPHY.md` §9 mentions `src/mental_poker/`. The `gui/`,
`protocol/`, `poker/`, `security/`, `storage/`, `tests/{protocol,adversarial,fuzz}`
and `assets/{table,cards}` layout of §23 appears nowhere, and §23's binding part —
"preserve the separation of transport, poker engine and cryptographic deck" — has
no document that states where each of the five specification documents lands in
the tree. Low severity (it is Phase 2 work), but it is a spec section with no
owner.

---

## D-5 — `SPEC_CS.md` §33's profiling list is only half covered

`CRYPTOGRAPHY.md` §6.5 gives measured numbers for shuffle generation, shuffle
verification, private deal and board reveal, and §6.5 consequence 1 discharges
"crypto must not block the GUI event loop". Of §33's seven named profiling
targets, **signature verification**, **network latency** and **hand startup
latency** have no measured figure and no owner: `PROTOCOL.md` §4.0 calls step 9
"one Ed25519 verification" without a number, and hand-startup latency is
*estimated* (~340 ms heads-up, ~1.1 s six-handed) from an assumed 100 ms RTT
rather than measured. `NETWORK_STACK.md` §12.12 confirms no two-network test has
been run.

**The correction.** Add the three missing metrics to a Phase 5/8 measurement plan,
and mark the hand-startup figure as an estimate wherever it is quoted (it is
currently repeated as a bare number in `PROTOCOL.md` §4.4 and §3.2).

---

## D-6 — `SPEC_CS.md` §31 (Git discipline) is covered by no document

§31 requires small logical commits, a prohibition on deleting or rewriting large
parts of a working implementation without justification, and — the load-bearing
part — **a regression test that reproduces the problem before any
security-critical change**. No document carries it. Given that
`CRYPTOGRAPHY.md` §12 and `PROTOCOL.md` §9.6 both enumerate security-critical
tests, the §31 rule belongs next to them as a contribution rule.

---

## D-7 — Nothing states what happens to a table when the *founder* leaves before `TABLE_READY`, or how `min_players_to_start = 10` interacts with `SPEC_CS.md` §32's "heads-up first"

Two smaller gaps in the same area.

**(i)** `PROTOCOL.md` §4.3 gives the table key sole authority over `JOIN_ACCEPT`,
`JOIN_REJECT` and `PLAYER_LIST` until `TABLE_READY` freezes the roster. No
document says what happens if the founder disappears during formation: the
advert stays live until `AD_TTL_MS`, joiners hold `JOIN_ACCEPT`s, and nobody can
issue the final `PLAYER_LIST`. `STATE_MACHINE.md` T4 covers only the
join-deadline-expired case ("`TableClosed`; table simply never started"), which is
the right outcome but is not connected to the lobby-side cleanup or to the
`JOIN_ACCEPT` holders.

**(ii)** `RATED_SNG_POKERTH_V1` pins `seats = 10` and `min_players_to_start = 10`
(`PROTOCOL.md` §13, `STATE_MACHINE.md` §9.1), while `SPEC_CS.md` §32 requires the
first supported mode to be two-player heads-up and `PROTOCOL.md` §1.3 marks
`nlhe/2-6` as "MVP scope". So the *named preset the spec asks for* cannot be
played by the MVP, and no document says which table configuration the Phase 8
acceptance test of D-003/D-004 actually uses. This should be an explicit statement
("the MVP ships `CUSTOM` two-seat tables; `RATED_SNG_POKERTH_V1` becomes playable
when `nlhe/7-10` lands"), not left to inference.

---

## Summary of what should change before Phase 2

Ordered by what unblocks the most:

1. **Fix the timeout certificate** (A-1, A-2, A-5, C-10). It is currently
   unusable at collective stages, vacuous heads-up, and its stated race-safety
   mechanism does not exist. This is one coherent redesign, not four patches.
2. **Fix the divergence path** (A-3, A-6, C-6). Attributing a `state_hash` that
   contradicts the unanimous transcript closes the `cause = 4` restoration
   escape *and* removes the need for unanimity-minus-one, so one change resolves
   both. Then `STATE_MACHINE.md` has to grow the states to match.
3. **Scope the equivocation predicate** (A-4). Until it excludes unchained
   messages, the design's strongest evidence object can be forged against honest
   players and is produced spuriously by honest founders.
4. **Reconcile the constants and the `ctx` definition** (C-1, C-2, C-3, C-7).
   These are mechanical, but every one of them makes two conforming clients
   incompatible on the wire.
5. **Correct the three factual claims** (B-1, B-2, B-3) and the relay arithmetic
   (A-8), since B-1 and A-8 are both quoted as evidence for other decisions.

None of the findings above touches the core cryptographic construction. The
n-of-n Barnett–Smart/Bayer–Groth design, the `Verified<T>` type-state discipline,
the counting rule of `PROTOCOL.md` §3.4, the canonicality gate, and the refusal of
`t`-of-`n` are all sound as specified and are supported by the research. What
fails is the *agreement layer built around it* — deadlines, divergence and
equivocation — which is where every A-class finding lives. That layer is also the
one part of the design that has no published construction behind it and was
assembled in these documents; `SPEC_CS.md` §36's instruction applies to it as much
as to the cryptography.
