# ziffle 0.1.0 — the verdict

Author: `ziffle-review` verdict agent
Date: 2026-08-29
Inputs: `ZIFFLE_ALGEBRA.md`, `ZIFFLE_FIAT_SHAMIR.md`, `ZIFFLE_INPUT_HANDLING.md`,
`ZIFFLE_PROTOCOL_FIT.md`, `ZIFFLE_ALTERNATIVES.md`, `ZIFFLE_ATTACKS.md` — all six read in
full. Charter: `MENTAL_POKER.md` §9 risk 1 / `CRYPTOGRAPHY.md` OQ-1, the prerequisite
review. Governing standards: `SPEC_CS.md` §36 (stop and document rather than substitute
your own construction) and §18 (claim no more than is true).

This document decides. It does not re-derive the six reviews; it weighs them, records
what they left open, and states the conditions.

---

## 1. The verdict

> **Fit for the play-money MVP only with the changes in §5. Not fit for real money, and
> §6 is a different and much higher bar that nothing in this review reaches.**

### The reasoning, in four sentences

The **Bayer–Groth argument itself held.** A line-by-line diff of `MultiExpArg` and
`SingleValueProductArg` against the paper found no wrong exponent, no off-by-one, no
wrong summation bound and no missing algebraic check; the forked Fiat–Shamir transcript
binds every statement element the paper requires, in the right order, under six distinct
domain tags, and the fork is the paper's own parallel composition rather than an
improvisation; twenty-two attack families built by an **independent re-implementation of
the prover** — not by perturbing ziffle's own output — all failed against the real
verifier, including an exhaustive 2¹⁶ hybrid of the two strongest cheating strategies.

**Everything that broke is around the argument, not in it.** Six demonstrated breaks
(B1–B6) are cases where the crate proves exactly what it claims and *the claim is not
what a poker game needs*: a reveal token that decrypts a different card, a "shuffle"
that leaves chosen slots in plaintext, a shuffle whose permutation an observer can read
off, a public key anyone can forge possession of, a deck with 2⁶²⁴ accepted encodings,
and a proof any peer can rebroadcast as his own.

**Every one of those six is closed by a small number of checks in our layer**, the most
important of which is a three-line structural test on each shuffled deck costing 0.019 ms
against ~36 ms of proof verification — so the exit price of the verdict is low and known.

**What is not closed is the thing an audit would close:** nobody proved witness-extended
emulation, nobody re-derived the soundness bound at `m = 1` (which is half of what OQ-1
asked for), and a check that is *wrong but self-consistent* would have passed every test
in this review — so what we have is two careful readings and a failed attack campaign,
which is evidence and is not a proof.

That combination — construction sound as read, surface unsafe as shipped, no proof and no
audit — is exactly what "play money yes, real money no" means, and it is what ziffle's
own README says about itself.

### Why not the two alternatives

**Swapping now to `paritytech/mental-poker` is rejected.** It is better shaped — no
transcript fork, `m = 4, n = 13` instead of `m = 1`, 25 % smaller proofs, 30 % faster
verification, a reproducible CRS with a regeneration test that was run and diffed
byte-identical, two maintainers, +6 packages and no arkworks split. But it is 7 764 lines
against ziffle's 1 779, unpublished, untagged, has moved organisation twice with a
dangling URL in its own comments, and **is equally unreviewed**. Swapping today would
retire nothing and quadruple the surface. It is the right fallback and it should be
registered as such (§5, C-12).

**Hand-rolling cut-and-choose is rejected, more firmly than before.** `ZIFFLE_ALTERNATIVES.md`
§6.3 corrected `MENTAL_POKER.md` §5.3 on the point that matters: `t = 40` repetitions give
2⁻⁴⁰ only against an *interactive* verifier, and under Fiat–Shamir a pool-and-reorder grind
costs a measured 0.94 µs per attempt — **twelve core-days to forge a shuffle**. A third
party building the same product (`ImperialBower/pkmental`, 91 passing tests) shipped exactly
that parameter. The sound non-interactive parameter is `t = 128`: 4.8 s and 705 KB per
six-handed hand, over budget on both. This is `SPEC_CS.md` §36 arguing for itself.

---

## 2. What the verdict rests on — the positive results

Recorded first, because a defect list read without them is misleading.

| # | Established | By |
|---|---|---|
| 1 | Both sub-arguments are faithful instantiations of BG12 §4 and §5.3 at `m = 1`. Every prover line and every verifier line mapped to a paper term, sign for sign and index for index. | `ALGEBRA` §2, §3 |
| 2 | All eight verification checks are **falsifiable** — a probe splitting `verify` into per-check booleans shows each of the eight is broken by at least one perturbation. None is vacuous or dead. | `ALGEBRA` §5.7 |
| 3 | The `m = 1` extractor was hand-traced through the paper's construction and goes through with the code's exact equations. `c_β` is hashed before the challenge, which is what makes step 1 of the extraction work, and it is there. | `ALGEBRA` §4 |
| 4 | The transcript absorbs `apk`, the whole input deck, the whole output deck, `c_π` before `x`, `c_xπ` before `y,z`, and each sub-argument's first message before its own challenge. Nothing the paper requires is missing. Confirmed by an **independent re-implementation** that reproduced all five challenges bit-for-bit and the 5 547-byte serialisation exactly. | `FIAT_SHAMIR` §2, `ATTACKS` §1 |
| 5 | **The fork is sound.** The paper authorises parallel composition; neither sub-argument's statement depends on the other's proof; both branches diverge at their first link independently of the domain tags. The Frozen-Heart-class bug is not here. | `FIAT_SHAMIR` FS-1, corroborated by `ATTACKS` §7 |
| 6 | **The Pedersen commitment key is genuinely nothing-up-my-sleeve.** `Projective::rand` samples a base-field x-coordinate and lifts it; it does not compute `s·G`. Had it been the latter, `dlog_G(h)` would be publicly recomputable from a fixed seed, binding would evaporate and every other finding would be irrelevant. **Checked independently three times.** | `ALGEBRA` §5.4, `FIAT_SHAMIR` FS-9, `INPUT` §3.4 |
| 7 | The one structural shortcut that would have forged the multi-exponentiation argument in a single step was located exactly and is closed: `MultiExpArg::challenge_x` absorbs ciphertexts as **whole tuples** (lib.rs:727–728). A hypothetical transcript absorbing only `c1` was built and the forgery lands on it immediately. | `ATTACKS` §5 |
| 8 | 22 attack families, 2¹⁶ exhaustive hybrid, 1.2 M-guess grind: **zero forgeries of the shuffle argument.** Each rejection at a check that can be named. | `ATTACKS` §2–§4 |
| 9 | 143 490 088 hostile parse-and-verify calls across 7 wire types × 4 modes × 6 generators: **0 panics, 0 forgeries, flat heap** (peak 51 862 B). Unbounded allocation is structurally impossible — no length prefix exists anywhere in the wire format. | `INPUT` §2, §3.5 |
| 10 | The type-state boundary holds: `MaskedCard`, `AggregatePublicKey`, `AggregateRevealToken` and `Verified<T>` have **no** `CanonicalDeserialize`, proven by negative compile test. A card cannot be taken off the wire; it must come out of a deck this peer verified itself. | `INPUT` §3.6, `PROTOCOL_FIT` §2.1 |
| 11 | Selective opening works per card; `n`-of-`n` is structural — every `(n−1)`-subset returns `None` at n = 2, 6, 10, and `n−1` plus a duplicate fails too. Fail-closed on the token set. | `PROTOCOL_FIT` §2, §3 |
| 12 | All seven fields of `PROTOCOL.md` §4.5's `ctx` were individually measured to bite. The rogue-key attack is properly blocked (`OwnershipProof::challenge` absorbs `pk`). | `PROTOCOL_FIT` §4.3, `ATTACKS` B5 |
| 13 | Cost fits with ~300× headroom at the tightest step and ~3 700× across a hand at the rated preset. Wire sizes fit every `PROTOCOL.md` §9.3 cap. | `PROTOCOL_FIT` §5 |

Item 5 is the answer to `CRYPTOGRAPHY.md` **OQ-2**, and it is a clean one: two reviewers
reached it independently, one by reading and argument, one by executing a graft attack
across statements and finding it rejected. OQ-2 can be closed.

---

## 3. Defects, ranked, each with its exploitability

Rank is by *how much a reader should let it change what gets built*, which is severity
weighted by how cheap the fix is and how easy it is to forget.

### D-1 — A reveal token issued for one card decrypts a different card

**Where.** `RevealTokenProof::challenge`, lib.rs:428–444, absorbs `pk, share, c1, t_g,
t_c1`. `RevealTokenProof::verify`, lib.rs:484, destructures the card as
`let MaskedCard((c1, _)) = card;` — **`c2` is discarded.** The proof binds `c1` alone and
is valid for every card sharing that `c1`. Nothing forces `c1` to be unique: on the
initial deck every `c1` is the point at infinity (lib.rs:1327), so the first shuffler need
only reuse one re-masking scalar across two slots.

**Exploitable: yes. Demonstrated end to end, twice, independently** — once through the
crate's own prover driven by a constant RNG (`FIAT_SHAMIR` E6a–E6d), once at N = 52
through an independent forger (`ATTACKS` A12). Both show a token set gathered for card 0
decrypting card 1, with every proof verifying.

**The attack in game terms.** Collide a card the protocol will legitimately open to
everyone — a flop card, a burn card, a showdown card — with a victim's hole card. When the
honest players hand over their tokens for the public card, as the rules require, those
same tokens decrypt the hole card. The victim cannot notice.

**The limit, and do not lean on it silently.** The collision can only be created by the
**first** shuffler; a later one would have to solve a discrete log. One honest re-shuffle
destroys it (measured: 0 colliding pairs remaining). So it needs every shuffler malicious,
or a chain of length one. That mitigation depends entirely on our layer getting the chain
right, which is why C-3 and C-6 below are both required rather than either alone.

**Fix.** Two lines in a vendored fork (absorb the whole ciphertext, pass the full card
through), or closed from outside by C-3. Do both.

**Why it ranks first.** `ATTACKS` §5 shows the identical mistake — binding one coordinate
of a ciphertext instead of both — would, if made in the *shuffle* transcript, have produced
a one-step forgery of the entire shuffle argument with no search. The author made that
mistake once, in the reveal proof. It is not a stylistic nit; it is the same class of bug,
in the one place where its blast radius is a player's hole cards rather than deck integrity.

### D-2 — A "shuffle" can leave chosen slots in plaintext, and the proof verifies

**Where.** `remask_card`, lib.rs:1221–1230, accepts any `r` including zero; `ShuffleProof`
constrains `r` not at all.

**This is not a deviation from the paper.** BG12's statement is `∃ π, ρ : C'ᵢ = C_{π(i)}
E(1; ρᵢ)` and says nothing about `ρᵢ` being non-zero, distinct or random. The gap is
between what the argument proves and what a deck needs. An algebra reviewer checking
against the paper correctly finds nothing here — which is precisely why it has to be
written down.

**Exploitable: yes. Demonstrated.** `ρ = 0` everywhere gives a deck in the clear, readable
with no key material at all, accepted by `verify_initial_shuffle` (`INPUT` attack A,
`ATTACKS` A9). The version an attacker would actually use is surgical: the first shuffler
chooses the permutation, so he knows which card lands where, and can leave **exactly the
slots he wants** in the clear while masking the other fifty (`ATTACKS` A16 —
`[Some(9), Some(22), None, None]` read straight off the wire against true cards
`[9, 22, 1, 29]`). The whole-deck version is visible by eye; this one is not.

Also accepted: the identity permutation, and `next == prev` exactly — **a player can
appear to shuffle while provably not touching the deck.**

**Fix.** C-3. Reachable only from the initial deck for the plaintext variants; the no-op
variant is reachable at any position.

### D-3 — A shuffler's permutation can be recovered from public data

**Where.** Same root as D-2: one shared `ρ` across all slots preserves the ciphertext
differences, so the map from `prev` to `next` is public.

**Exploitable: yes. Demonstrated** — all 52 positions recovered by an observer holding
only `next`, the public open deck and `apk`; and at round 2 from `prev` and `next` alone
(`ATTACKS` A18, A19).

**The consequence to internalise.** *"k players produced valid shuffle proofs" does not
imply "k independent secret permutations were applied."* Unpredictability comes only from
the honest shufflers, and our layer cannot tell from the proofs which those were. This is
correct BG12 behaviour and a fatal integration trap.

**Fix.** C-3 for the shared-`ρ` tell, plus the design rule C-11: never read a valid
shuffle proof as evidence that the shuffler randomised anything.

### D-4 — The transcript is a different function on 32-bit targets

**Where.** `Transcript` frames every field with `usize::to_be_bytes()` — lib.rs:218
(`serialized_size`), :224 (`label.len()`), :238 (element index `i`). That is 8 bytes on
x86_64/aarch64 and **4 on wasm32/armv7/i686.** The absorbed byte string, hence the state,
hence every challenge, hence proof validity, is a function of the target's pointer width.

**Exploitable: no — it fails closed. But it is a hard interoperability break**, and a
certain one: a proof produced by a browser client is rejected by every 64-bit peer and
vice versa, splitting a table into two mutually unverifiable halves. ziffle is `#![no_std]`
and leads its README with that; the obvious reason to make a mental-poker crate `no_std`
is to ship it to wasm.

**Measured** (`FIAT_SHAMIR` E3): the same shuffle transcript yields
`x = 79962580…` under 8-byte framing and `x = 38522517…` under 4-byte framing, with the
8-byte value asserted bit-identical to the one ziffle itself used.

**Not observed on a real wasm binary** — the target is not installed on this machine. The
finding rests on `usize::to_be_bytes()` semantics plus E3, which together are conclusive
but are not an end-to-end run.

**Why it ranks this high despite not being a security defect.** It is a **wire-format
change**, so it is nearly free today and expensive after the first proof is persisted or
exchanged. The author already wrote `usize_to_u64!` (lib.rs:660–664) for exactly this
problem elsewhere and did not apply it here. This is the defect that forces the fork
decision in §7 and it is the reason that decision cannot be deferred.

### D-5 — `pk = identity` passes the ownership proof, with a proof anyone can write

**Where.** `OwnershipProof::verify`, lib.rs:349–355. With `pk = O` the term `e·pk`
vanishes and `z·G == a + e·pk` collapses to `z·G == a`, satisfied by `(a = w·G, z = w)`
for any `w`, with no secret and without ever computing the challenge. The compressed
infinity branch returns before `check()` is reached, so `PublicKey = O` parses in both
validate modes. `AggregatePublicKey::new` then sums it in as a no-op.

**Exploitable: yes, forged and run** (three reviews, independently). The direct
confidentiality impact is bounded and I will not inflate it: the attacker contributes
nothing to `apk`, so he cannot decrypt anything, and a genuine rogue key
`pk = X − Σ pkᵢ` is still blocked — the ownership proof does its job, and that is a
positive worth recording.

What it destroys is the invariant **`Verified<PublicKey>` ⇒ this peer knows a secret key**.
Anything built on it — accountability, seat identity, an `n`-of-`n` count — is weakened by
a participant who provably knows nothing. And it is self-harm delivered by a broken or
backdoored third-party build, invisible to every other peer, producing a transcript that
verifies perfectly: **heads-up, a seat submitting `pk = O` hands its own hole cards to a
modified opponent**, since `apk` becomes the opponent's key alone. Measured at n = 3:
two of three "players" open a card.

**Fix.** C-4. One comparison, plus a pairwise-distinctness check on the key set.

### D-6 — The wire encoding is not canonical, by a very large factor

**Where.** Three independent sources, all measured.
(a) `SerBuffer::to_bigint` (`ark-ff/const_helpers.rs:156`) never reads byte 32's other six
bits after the two flag bits are stripped — **every compressed point has 64 encodings.**
A `MaskedDeck<52>` carries 104 points → **≥ 2⁶²⁴ byte strings** that all parse to the
identical deck and all verify; a `ShuffleProof<52>` carries 11 → ≥ 2⁶⁶.
(b) The infinity flag discards the parsed x-coordinate, so a uniformly random 33-byte
string is a valid `PublicKey` **half** the time and the point at infinity **a quarter** of
the time (measured: 49.96 % / 24.99 % over 10⁶ inputs).
(c) Trailing bytes are ignored (a proof plus 1 000 garbage bytes is accepted). Truncation,
by contrast, is clean — all 5 547 proper prefixes rejected.

**Exploitable: yes, for message-layer attacks, not for forgery.** It does not let anyone
produce a *different* proof — the parsed value is identical, and the transcript hashes the
canonical re-serialisation, not the received bytes. It does let **anyone, with no witness
at all**, take a proof off the wire and emit unboundedly many distinct byte strings every
peer accepts. That defeats a dedup cache, a GossipSub message id, a "have we seen this"
set, a hash-chained transcript that hashes received bytes, or a signature over the
encoding — all of which this project has.

**Fix.** C-2: exact expected length before parsing, then re-serialise and compare, then
hash and sign only the canonical form.

### D-7 — `Validate::Yes` is inert on the compressed path; the uncompressed path is a loaded gun

**Where.** secp256k1 has cofactor 1, so `is_in_correct_subgroup_assuming_on_curve` is
unconditionally true and `check()` reduces to `is_on_curve()` — which decompression already
guaranteed. Measured: 0 disagreements between `Validate::Yes` and `Validate::No` over
2 000 000 random compressed inputs. On the **uncompressed** path with `Validate::No`,
`Affine::new_unchecked` is called with no curve check and **2 000 000 of 2 000 000** random
65-byte strings were accepted as off-curve `PublicKey`s.

`MultiExpArg::verify` and `SingleValueProductArg::verify` perform **none** of the paper's
own `∈ G` / `∈ Z_q` membership checks (BG12 §4 and §5.3 both open with them); the crate
delegates entirely to deserialisation and its macros forward whatever mode the caller
passes. **The decision is ours and the crate will not make it for us.**

**Exploitable: not by the naive route, and not established either way beyond that.** An
off-curve deck built by the ported prover was rejected — by the *algebra*, not by
`Validate`, because the multi-exponentiation identity depends on a homomorphism that stops
holding once a point from another curve enters the sum. **Nobody attempted the harder
construction with every point on the same twist**, so this is a negative result, not a
proof. The stake if it were reachable is real: `MaskedCard::reveal_token` computes
`share = c1 · sk` and is the only place a secret key multiplies a peer-supplied point —
the textbook invalid-curve key-recovery setup.

**Fix.** C-1. `Validate::Yes` costs 0.001 ms on a 5 547-byte proof, so there is no
performance argument for ever skipping it.

### D-8 — The shuffle transcript binds no prover identity and no round index

**Where.** `ShuffleProof::challenge_x` absorbs `apk, prev, next, c_π` and the caller's
`ctx`, and nothing else. `Transcript::init(ctx)` is `SHA-256(ctx)` — no length prefix, no
domain tag, so a `ctx` built by concatenation is ambiguous (`"game7"+"hand3"` collides with
`"game"+"7hand3"`, and the proof was measured to transfer between them).

**Exploitable: yes if the caller's `ctx` is weak.** Measured: `player B rebroadcasts A's
(next, proof) verbatim → ACCEPTED`. B has "proved" a shuffle he did not perform and whose
permutation he does not know. In a protocol that reads "valid shuffle proof" as "this
player contributed entropy", that is an entropy-contribution forgery.

**Status: already closed by the protocol as written, and this is the reason it must not be
"simplified".** `PROTOCOL.md` §4.5's `ctx` is a 32-byte length-prefixed domain-separated
hash carrying `shuffle_round` and `sender_public_key`; all seven of its fields were
measured to reject a proof offered under a `ctx` differing in that field alone. The
requirement is therefore: **keep it exactly as it is.** Six of eight replay variants were
rejected outright by the `ctx` binding, which is real work the construction is doing.

### D-9 — Rejecting one bogus proof costs 10–36 ms of CPU at every seat

**Where.** `ShuffleProof::verify`. Measured idle: `verify_initial_shuffle` 36.3 ms valid /
31.1 ms invalid; `verify_shuffle` 10.1 ms invalid; parse 0.095 ms.

**Exploitable: yes, as denial of service.** A bogus proof costs the attacker under a
millisecond to assemble (11 valid points, 162 valid scalars) and 5 547 bytes to send. One
peer on a 1 Mbit/s uplink pushes ~22 proof-shaped messages per second, burning 0.2–0.8
CPU cores of verification **at every other seat**, since everyone verifies every shuffle.
Two such peers pin a core each.

**Fix.** C-9: exact length and canonicality first, the 0.019 ms structural check second
(≈1 900× cheaper than the proof), rate limiting third, `verify_shuffle` last.

### D-10 — The 52 cards and the commitment key are defined by two crates' internals and nothing pins them

**Where.** `open_deck` and the Pedersen key are `pub(crate)` state of `Shuffle<N>` with no
public accessor, and both are derived from `rand 0.8`'s `StdRng` plus `ark-ec 0.5.0`'s
rejection-sampling loop. `StdRng` is explicitly not reproducible across `rand` majors, and
the loop's byte consumption is an arkworks implementation detail.

**Exploitable: no. A correctness and compatibility time bomb.** Two clients with different
`open_deck` values disagree about what card index 17 *is*; two clients with different
Pedersen generators reject each other's every proof. Vendoring plus `Cargo.lock` freezes
it for our build and not for a second implementation, and would not catch an accidental
bump.

**Fix.** C-7 — freeze the two digests, independently recomputed and confirmed:

```
blake3(open_deck, 52 compressed points)  = 4e931f9e24cf0cc9b511525c165ebd9bd1454c2aa0fe9eb74ef315a3dedfae7c
blake3(pedersen h || 52 generators)      = 14c8e144b43285da721b4ec2fe8fd8576849c6befdc9f5f8a7457174018d1dd4
```

### D-11 — Every failure is `None`; `invalid` and `could not verify` are indistinguishable

**Where.** `verify_shuffle`, `verify_initial_shuffle`, `OwnershipProof::verify`,
`RevealTokenProof::verify` and `reveal_card` all return `Option`. A `None` from
`verify_shuffle` means "the proof did not verify against the four inputs you supplied" and
nothing more: the wrong `prev` deck, the wrong `apk`, or a `ctx` built from a stale field
produce the identical `None` as a genuine forgery. Both were measured.

**Exploitable: no directly — but it is an attribution hazard, and `D-014`'s tier-1
clauses make ziffle's false-*reject* behaviour load-bearing against a person.**
`CRYPTOGRAPHY.md` §8.1.5 requires `invalid` (which can cost a seat its name in the
transcript) to be distinguished from *could not verify* (which is never evidence). The
crate cannot make that distinction for us.

**Fix.** C-8 — establish every input *before* the call, so that `None` can only mean
`invalid`; and make the distinction a type in our layer (§7's `VerifyError`).

### D-12 — A `reveal_card` `None` on a fully verified token set is a soundness signal, not an error

Given a chain of verified shuffles and `n` DLEQ-verified tokens from exactly the `apk` key
set, `reveal_card` returning `None` is **unreachable unless the shuffle argument's
soundness has failed** — the verified chain guarantees the plaintext is one of the 52
`open_deck` points. Treating it as an ordinary abort hides the one signal that would tell
us OQ-1 went badly. Not a defect in the crate; a defect that would be ours. Fix: C-10.

### D-13 — Two proofs on the same statement have interchangeable halves

**Where.** The fork. Two proofs sharing `c_π` **and** `c_xπ` have identical fork bases, so
their sub-arguments can be swapped: measured, `A2's SVP half + B2's multi-exp half →
ACCEPTED`.

**Exploitable: no, and this was pushed on.** Producing two such proofs requires the
openings of `c_π` and `c_xπ`, i.e. the witness, so only the prover can do it — and a
prover can already mint arbitrarily many valid proofs by resampling. No advantage for a
non-prover was found. It is a precise characterisation of the fork's structural cost, and
it is closed for free by the optional joint-state hardening in C-12(d). Recorded because
it is the one thing the fork demonstrably costs.

### D-14 — Lower-severity and informational

| # | Item | Exploitable? |
|---|---|---|
| a | `const _N_GREATER_THAN_1: () = assert!(N > 1)` (lib.rs:1324) is **dead code** — never referenced, so never monomorphised. `Shuffle::<1>` and `Shuffle::<0>` compile and then panic at runtime, the former inside the crate's own self-check. | No. `N` is ours and is 52. A footgun; fix in a fork by referencing the const. |
| b | The comment at lib.rs:1000 says `a~(i + 2)`; the code computes `a_tilde[i + 1]`, which is what the paper requires. **Code right, comment wrong.** | No — but it is exactly the comment that leads a maintainer to "fix" correct code into an unsound state. Fix it in a fork. |
| c | Challenges are not rejected when zero; the paper says `Z_q^*`. | No — 2⁻²⁵⁶ per challenge, unsteerable. |
| d | `Transcript::init` has no protocol/version tag. | No, today. Load-bearing the moment a v2 of any proof exists. |
| e | `ck` is never absorbed into the transcript. Safe **only** because it is a compile-time constant with no public constructor. If a fork ever lets a caller supply a commitment key, absorbing it before `x` becomes mandatory or soundness breaks outright. | No, today. A standing constraint on any fork. |
| f | arkworks' `expand_message_xmd` uses `Z_pad` of 48 bytes where RFC 9380 requires the hash's 64-byte input block size. `b_0` is finalised before reuse so the length-extension property it defuses is unreachable. | No known attack, and none was constructible. Means ziffle's challenges are **not** RFC-9380 values: any second implementation must replicate arkworks' quirk, not the RFC. |
| g | `AggregatePublicKey::new` sums duplicate keys silently. | No — two seats with one `pk` need one `sk`, i.e. one player holding two seats, who already has both shares. Fold the rejection into C-4. |
| h | `apk`'s "ascending seat order" in `DECK_INIT` is unenforceable and does not need to be: point addition is commutative. Worth one clarifying word so no reader takes it for a check. | No. |
| i | `Shuffle::<52>::default()` costs **5.9 ms** and is absent from the timing tables. The natural way to write the impl — `Shuffle::default()` inside each method — adds 14 % to every 42 ms verify, `n` times per hand per node. The type is `Copy` at 8 832 B. | No. Construct once per process, hold by reference. |
| j | The `assert!` in `Transcript::update_with_serialized` (lib.rs:211) is **unreachable by an adversary** — the largest value ever appended is a 66-byte ciphertext against a 256-byte buffer, and the size is a property of the type, not the value. `CRYPTOGRAPHY.md` **OQ-5**'s concern about it can be closed. | No. Argued statically and measured. |
| k | `crypto_step_timeout_ms` is legal down to 1 000 ms; at that value a `SHUFFLE_STEP` needs 100 ms proving + 9 KB to every peer + 42 ms verifying at each, on a 24-core desktop. A 4× slower client on a Circuit-Relay-v2 path will not make it, and the failure mode is an abort attributed to a peer that did nothing wrong. | Not a library defect. Document a practical floor around 5 000 ms. |

---

## 4. What was checked, and what was not

The second half of this section is the more important one. A reader will otherwise assume
everything not listed as a defect was checked.

### 4.1 Checked

* **The algebra**, exponent by exponent, both sub-arguments, prover and verifier, against
  BG12 §4 and §5.3 at `m = 1`, including the `N−1` padding slot, the `N = 2` edge, the
  index shift, the commitment bases (no basis confusion), `c_{−z}`'s zero randomness, and
  the `ct_mspp!` bounds.
* **That each of the eight checks is live**, by perturbation.
* **The extraction at `m = 1`**, hand-traced through the paper's construction.
* **The complete Fiat–Shamir transcript**, re-implemented twice independently and matched
  bit-for-bit, including framing, delimitation, full-state derivation, the six domain
  tags, and the fork topology.
* **The Pedersen key's NUMS derivation**, three times.
* **Hostile input**: 143 M parse-and-verify calls, 7 types × 4 modes × 6 generators, with
  allocation tracking and `catch_unwind`; differential `Validate::Yes`/`No` over 2 M
  inputs per mode; truncation and trailing-byte behaviour; the type-state boundary by
  negative compile test.
* **Forgery**: 22 attack families with an independent prover that can lie at any step;
  an exhaustive 2¹⁶ hybrid of the two strongest strategies; two grinding surfaces measured
  at 3 000 and 1 200 000 attempts with the matching-prefix distribution recorded.
* **Protocol fit**: selective opening, `n`-of-`n` at n = 2/6/10 including every
  `(n−1)`-subset, the seven `ctx` fields individually, intermediate-deck usability,
  per-stage cost against the deadline budget, wire sizes against every §9.3 cap.
* **The alternatives landscape** and the measured exit cost, including a full build,
  test run, benchmark and CRS-regeneration diff of the designated fallback, and a
  complete from-scratch implementation of the cut-and-choose fallback.

### 4.2 Not checked — and this is what a reader must not assume

1. **Witness-extended emulation was not proved.** Two careful readings — the algebra
   review's hand-trace and the attack review's exhaustion — are evidence. Neither is a
   proof, and **both reviewers said so in the same words.** This is the single largest
   remaining gap and it is the one an audit exists to close.
2. **A wrong-but-self-consistent check would have passed everything here.** The attack
   review tested the checks *as written*; a check that accepts a family of false
   statements the paper's checks reject is invisible to it. Only the algebra review
   addresses that, and it is a reading.
3. **The soundness bound at `m = 1` was not re-derived.** `MENTAL_POKER.md` OQ-1 asked
   explicitly for *"a check that the `m = 1` instantiation preserves the paper's soundness
   bound"*. It was asserted `O(N/q)` from the paper's structure; the accounting was not
   redone. **OQ-1 is therefore substantially but not wholly discharged**, and this is the
   residual.
4. **The paper's `N`-fold rewinding over `x_base`** (the transposed-Vandermonde argument
   in BG12 §3) was not verified against anything in the code — it is a property of the
   protocol, not of a line.
5. **No coverage-guided fuzzing.** `cargo-fuzz` requires nightly; the nightly download is
   blocked by a local firewall (os error 10013). 143 M blind inputs with no coverage
   feedback is a large smoke test, not a campaign, and the acceptance rates suggest some
   targets spent most of their budget being rejected at the first bad point.
6. **No miri, no ASan** — same blocker. The one `unsafe` site on the deserialisation path
   (`SerBuffer::as_slice`) was traced by reading and judged sound because the struct is
   `#[repr(C, align(1))]`.
7. **The all-on-one-twist off-curve deck** was not attempted. The naive smuggle fails; the
   harder construction is untested. The compressed path makes it moot, which is why it was
   stopped, but "moot given a rule we have not written yet" is not "safe".
8. **No real `wasm32` build** for D-4.
9. **arkworks' `DefaultFieldHasher` / `hash_to_field` was taken as correct.** If it
   produced correlated or low-entropy `[y, z]`, the Schwartz–Zippel argument that makes
   the product argument bite would weaken and several rejections in the attack review
   would become reachable. Nobody checked it. `ark-transcript`, which the fallback
   depends on, was likewise not checked.
10. **Timing and side channels: not examined at all, by anybody.** `verify` short-circuits
    with `&&`, so a peer learns which check failed from timing; `reveal_card`'s
    `open_deck.iter().position()` is an early-exit linear scan; field comparisons and
    `into_affine` are plausibly data-dependent. `CRYPTOGRAPHY.md` **OQ-4 stands untouched.**
11. **`zeroize` coverage of prover intermediates** (`rho`, `perm`, the witnesses — plain
    stack values) not assessed.
12. **Cross-build determinism not run.** `open_deck` was confirmed reproducible in one
    process from the same crate versions. Whether a proof from one build verifies under a
    differently-compiled build (different opt level, different target) was not tested.
    Nothing suggests it would not; D-10's test vector is what would catch it.
13. **Cross-proof malleability across sequential shuffles** beyond the replay battery —
    the `ctx` and `prev`/`next` bindings make it look fine, and "looks fine" is what the
    algebra reviewer had.
14. **Grinding was measured at 10³–10⁶, not 2⁸⁰.** The prefix-length growth matches a
    random function over the range tested; structure appearing only at a larger scale
    cannot be ruled out. No third cheating strategy was sought to feed the hybrid search.
15. **The discrete log was not attempted.** Everything assumes secp256k1 is hard.
16. **`paritytech/mental-poker` was not reviewed for correctness** — built, tested,
    benchmarked, transcript and CRS paths read, algebra untouched.
17. **Deck sizes other than 52; more than 10 seats.** Not exercised.
18. **No implementation in this space has an audit.** A search was made and none exists.
    The strongest external artefact is a Coq formalisation of the *construction*
    (Haines–Goré–Tiwari, USENIX Security 2023, with an extracted verifier), not of any
    implementation.

---

## 5. The conditions

These are required **before the mental-poker layer is written against ziffle**, not
before it ships. They are ordered so that the wire-format decisions come first, because
those get expensive the moment a proof is persisted.

| # | Condition | Closes |
|---|---|---|
| **C-0** | **Vendor `ziffle` 0.1.0 at `vendor/ziffle/`** with a `[patch.crates-io]` entry and a `PROVENANCE.md` recording the crates.io sha256 `ba79285194a16b02512566a9a64d885567646045b144bb0efeef662001cd83a5` (verified equal to `Cargo.lock`), the upstream git sha `bcb8e61651cd6d4140c895a414d7ed8bc06d32bd`, the licence choice (**rely on the Apache-2.0 branch** — the shipped `LICENSE-MIT` names *"The cargo-readme Developers"*, a copy-paste defect present upstream), and a link to this review. Nine files, 141 KB, procedure verified end to end. | makes the review auditable; removes yank/deletion risk |
| **C-1** | **Compressed encoding only, `Validate::Yes` always.** Ban every `*_unchecked` deserialiser in CI — a grep, not discipline. | D-7 |
| **C-2** | **Exact expected byte length before parsing** (the size table is in `INPUT` §3.5), then **re-serialise the parsed value and compare to the input, rejecting on mismatch**. Hash and sign **only** the canonical form; never key a cache, message id or signature on received bytes. | D-6 |
| **C-3** | **The structural deck check, inside `verify_shuffle` and not beside it**: no `next[i].c1` is the identity; the `c1` are pairwise distinct; no `c1` appears in `prev`; `next != prev`. Runs **before** the argument. 0.019 ms against ~36 ms. **This is the highest-value function in the whole review** — on its own it closes D-1's exploit path, D-2 and D-3. | D-1, D-2, D-3, and half of D-9 |
| **C-4** | **Reject `hand_public_key == identity`; reject a key set that is not pairwise distinct.** At `DECK_INIT` receipt, before `OwnershipProof::verify` is called. | D-5, D-14(g) |
| **C-5** | **Keep `PROTOCOL.md` §4.5's `ctx` exactly as written** — a 32-byte length-prefixed domain-separated hash, never a concatenation, carrying `shuffle_round` and `sender_public_key`. Add a comment at the construction site saying that simplifying it to a concatenation reintroduces a measured proof-transfer attack. | D-8 |
| **C-6** | **Chain discipline, normative:** every seated player shuffles, in a fixed committed order; cards are dealt **only** from the last deck in the chain; a disconnect **aborts the hand** rather than truncating the chain; a shuffle cannot be replayed or reordered. C-3 makes the security argument independent of this, which is exactly why both are required. | D-1, D-2, D-3 |
| **C-7** | **Freeze `open_deck` and the Pedersen key as test vectors** — a unit test recomputing the two BLAKE3 digests in D-10 from the vendored tree's dependencies, failing the build if a `rand` or arkworks bump moves either. | D-10 |
| **C-8** | **Establish every input before any `verify_*` call**, so a rejection can only mean `invalid`: `input_deck_hash`/`output_deck_hash` checked first, `apk` derived from the completed `DECK_INIT` stage, `ctx` from chained state, deserialisation already succeeded. A rejection reached by any other route is *could not verify* and must not produce evidence under D-014. Encode the distinction as a type (§7). | D-11 |
| **C-9** | **DoS ordering and limits:** length + canonicality → structural check → rate limit → `verify_shuffle`. One shuffle proof per peer per position per hand; a second is a protocol violation, not something to verify. | D-9 |
| **C-10** | **A full verified token set that fails to open a card is a soundness fault, not an abort** — distinct, loud, non-resumable. Never attributed to a peer. | D-12 |
| **C-11** | **Write down, where the shuffle chain is implemented, that a valid shuffle proof is evidence of *a* permutation and never of a *random* one.** Unpredictability comes only from the honest shufflers. The identity permutation and `next == prev` both verify. | D-2, D-3 |
| **C-12** | **The fork decision — see §7.** Whichever way it goes, it must be made before the first proof is persisted or exchanged. Its contents if taken: (a) three `usize::to_be_bytes()` → `u64` in `Transcript`; (b) absorb the whole ciphertext in `RevealTokenProof::challenge` and pass the full card to `verify`; (c) reference `_N_GREATER_THAN_1` so the guard fires, and fix the lib.rs:1000 comment; (d) optional, bundled: derive both sub-challenges from one joint state, and prefix `Transcript::init` with a protocol tag. All are wire-format breaks; fork once. | D-4, D-1, D-13, D-14(a,b,d) |
| **C-13** | **Construct `Shuffle::<52>` once per process and hold it by reference.** Never by value — it is `Copy` at 8 832 B and `default()` costs 5.9 ms. | D-14(i) |
| **C-14** | **Register `paritytech/mental-poker` @ `e05744b4cc431088ec2fda769a73b067b4664893` in the dependency register as the designated fallback**, with the rev written down and the two licence-hygiene questions (§4.6 of `ALTERNATIVES`) asked upstream now rather than in a crisis. Both libraries were proven to link into one binary, so a migration can differential-test rather than cut over blind. | keeps the exit cheap |
| **C-15** | **Port the adversarial probes into `tests/adversarial/`** — the 22 attack families, the degenerate-deck attacks, the identity-key forgery, the malleability cases and the canonicality checks — as real regression tests over the wrapper, so that C-1…C-4 are asserted rather than remembered. | all of the above, permanently |

C-3 and C-4 together are perhaps twenty lines. They are the difference between a security
argument that depends on getting the chain ordering right and one that does not.

---

## 6. What must be true before real money

A different and much higher bar. Nothing in this review reaches it, and ziffle's own README
speaks to exactly this: *"DO NOT use this library to play for non-trivial amounts of
money."* Treat that as binding until every item below is discharged.

1. **An independent professional audit** of the vendored crate (or of whatever replaces
   it), by people who do this for a living, covering the full extraction argument — not a
   re-run of this review. **This review is not an audit and does not substitute for one.**
2. **Witness-extended emulation written down** for the composed argument at `m = 1`,
   including the forked Fiat–Shamir composition — or C-12(d) taken, which removes the need
   for the fork half of it at a cost of three SHA-256 compressions.
3. **The soundness bound re-derived** at `N = 52, m = 1`. The residual on OQ-1 (§4.2 item 3).
4. **Cross-checking against the machine-checked verifier.** Haines–Goré–Tiwari's Coq
   formalisation of Bayer–Groth (USENIX Security 2023; `gerlion/secure-e-voting-with-coq`,
   extracted OCaml verifier) is a machine-checked statement of exactly which equations a
   verifier must check. Diff ziffle's eight checks against it. **Licence not declared on
   that repository — resolve before using its output in any artefact we publish.**
5. **Coverage-guided fuzzing** — `cargo-fuzz` on all seven wire types, plus miri on the
   deserialisation path. Both need a nightly toolchain, which needs the firewall issue
   (os error 10013) fixed.
6. **A constant-time / side-channel review.** OQ-4 is untouched: nobody has looked at
   `verify`'s short-circuiting, `reveal_card`'s linear scan, field comparisons, or
   `zeroize` coverage of prover intermediates. For real money this is not optional.
7. **The wire format frozen and versioned**, with C-12 already taken, and an end-to-end
   `wasm32` interoperability test that actually runs.
8. **A second implementation for differential testing**, run in anger — the fallback and
   ziffle in one binary over the same statements, on a corpus of adversarial decks.
9. **The `SPEC_CS.md` §18 limits documented and shown to the user**, unchanged and
   unsoftened: collusion by out-of-band channel, endpoint malware, screen sharing, Sybil
   without an identity layer, coercion, traffic analysis, DoS and deliberate disconnection
   are **outside** what any of this fixes. Real money makes every one of them worth money
   to an attacker.
10. **Legal, regulatory and custody questions**, which are not this document's and are not
    cryptographic.

Items 1, 2, 3 and 6 are the ones that cannot be bought with engineering time alone.

---

## 7. The `DeckCrypto` trait

The boundary that makes replacement a one-module change. Shaped by **what a replacement
looks like**, not by ziffle's API. Concretely, four things in the design come from the
fallback rather than from the incumbent:

* **No lifetimes and no borrowing associated types.** `paritytech/mental-poker`'s
  `AggregatedPublicKeys<'p, C>` borrows its parameters; a trait with plain associated
  types cannot hold that without GATs. The impl **owns** its parameters (which also
  discharges C-13) and reconstructs whatever the library wants inside each method.
* **No const generic for the deck size.** ziffle is `MaskedDeck<const N: usize>`; the
  fallback takes a `Vec` and pads. `DECK_LEN` is an associated const.
* **`Verified<T>` lives in *our* layer.** It is ziffle's best API idea and the fallback
  does not have it. Putting it here is what makes "never display an unverified card" stay
  a compile error across a swap.
* **Parsing is inside the trait.** Both libraries delegate to `ark-serialize` and neither
  chooses a mode. C-1 and C-2 are therefore properties of the boundary, not habits of the
  caller.

```rust
//! src/mental_poker/protocol.rs
//!
//! The single boundary between the poker protocol and whatever library carries the
//! mental-poker construction. No module outside `src/mental_poker/` names a library
//! type (CONTRIBUTING.md §4.4). Reviewed against ziffle 0.1.0 in
//! docs/research/ZIFFLE_VERDICT.md; the conditions C-1 … C-15 there are discharged
//! *by this trait's contract*, not by its callers.

use getrandom::SysRng;              // SPEC_CS.md §7 / CONTRIBUTING.md §4.5: the only
                                    // randomness source our code may use. Concrete on
                                    // purpose — an `impl Rng` parameter would let a
                                    // caller pass StdRng.
use zeroize::ZeroizeOnDrop;

/// The 32-byte context binding of `PROTOCOL.md` §4.5. Opaque and fixed-width by
/// construction, so no caller can hand the library a concatenation — which was
/// measured to transfer a proof between two different sessions (D-8).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DeckCtx([u8; 32]);

/// An index into the deck. Minted only by the dealing map; never parsed from the wire.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CardIndex(u8);

/// A card, as the poker engine understands it. The crypto layer's only output.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Card(u8);

/// Our verified typestate, not the library's. Private field, no `Deserialize`, no
/// public constructor: the only way to obtain one is to have run the verification in
/// this module. Survives a swap to a library that has no such type (C-14).
pub struct Verified<T>(T);
impl<T> Verified<T> {
    pub fn as_ref(&self) -> &T { &self.0 }
}

/// A deck that is the *last* link of a completed shuffle chain. `verify_shuffle`
/// mints `Verified<MaskedDeck>` for every step, and an intermediate deck's cards are
/// as usable as the final deck's — so the chain driver, and only it, mints this.
/// Reveal tokens are issued and checked against `Final` decks alone (C-6).
pub struct Final<T>(T);
impl<T> Final<T> {
    pub fn as_ref(&self) -> &T { &self.0 }
}

/// Every type that crosses the network. The implementation of `decode` **must**:
/// require exactly `LEN` bytes (no trailing bytes — upstream ignores them);
/// use the compressed, validated form and no `*_unchecked` path (C-1); and
/// re-encode and compare, rejecting a non-canonical input — upstream accepts
/// >= 2^624 encodings of one `MaskedDeck<52>` (C-2, D-6).
/// `encode` is the **only** form that is ever hashed, signed, deduplicated or
/// used as a message id.
pub trait DeckWire: Sized {
    const LEN: usize;
    fn decode(bytes: &[u8]) -> Result<Self, DecodeError>;
    fn encode(&self) -> Vec<u8>;
}

/// Why a byte string was refused. Always `Invalid` — a decode that ran is evidence.
#[derive(Debug, Clone, Copy)]
pub enum DecodeError {
    WrongLength { expected: usize, got: usize },
    NotCanonical,
    NotOnCurve,
    NonCanonicalScalar,
    Malformed,
}

/// The distinction `CRYPTOGRAPHY.md` §8.1.5 and D-014's tier-1 clauses rest on, and
/// which the library cannot make for us: every one of its verifiers returns a bare
/// `Option` (D-11).
#[derive(Debug)]
pub enum VerifyError {
    /// The check ran to completion against inputs this peer established itself, and
    /// it did not hold. **Admissible as evidence against the emitter.**
    Invalid(Invalid),
    /// The check did not run, or an input was not established. **Never evidence.**
    CouldNotVerify(&'static str),
}

#[derive(Debug)]
pub enum Invalid {
    Decode(DecodeError),
    /// The zero-knowledge argument itself did not verify.
    ProofRejected,
    /// The structural deck test of C-3: a `c1` is the identity, two `c1` collide, a
    /// `c1` is carried over from `prev`, or `next == prev`. Closes D-1, D-2, D-3.
    DegenerateDeck(Degeneracy),
    /// `pk` is the identity, or two seats presented the same `pk` (D-5).
    DegenerateKey,
}

#[derive(Debug, Clone, Copy)]
pub enum Degeneracy { IdentityC1, DuplicateC1, ReusedC1, NoOpShuffle }

/// Not a `VerifyError`, and never attributed to a peer. Returned only by `open_card`,
/// and only in a state that is unreachable unless the shuffle argument's soundness has
/// failed (D-12). The caller raises a distinct, loud, non-resumable fault; it never
/// retries and never blames anyone.
#[derive(Debug)]
pub struct SoundnessFault { pub what: &'static str }

pub trait DeckCrypto: Send + Sync + Sized + 'static {
    // ---- opaque handles -------------------------------------------------------
    /// Never `DeckWire`: the per-hand deck secret must not be a wire-reachable type
    /// even though the library makes it one.
    type SecretKey: ZeroizeOnDrop;
    type PublicKey:      DeckWire + Clone + PartialEq;
    type OwnershipProof: DeckWire;
    type AggregateKey:   Clone;
    type MaskedDeck:     DeckWire + Clone + PartialEq;
    type ShuffleProof:   DeckWire;
    type RevealToken:    DeckWire + Clone;
    type RevealProof:    DeckWire;

    const DECK_LEN: usize;

    // ---- construction ---------------------------------------------------------
    /// Once per process (C-13). Owns the library's parameters, so no associated type
    /// carries a lifetime and a borrowing implementation still fits.
    fn new() -> Self;

    /// The two nothing-up-my-sleeve constants the wire meaning of a card depends on.
    /// A test pins both digests, so a `rand`/arkworks bump cannot silently redefine
    /// what card index 17 is (C-7, D-10).
    fn open_deck_digest(&self) -> [u8; 32];
    fn commit_key_digest(&self) -> [u8; 32];

    // ---- key setup ------------------------------------------------------------
    fn keygen(&self, rng: &mut SysRng, ctx: &DeckCtx)
        -> (Self::SecretKey, Self::PublicKey, Self::OwnershipProof);

    /// Rejects the identity key and any key equal to one already in `already_seated`
    /// **before** the ownership proof is checked (C-4). `Verified<PublicKey>` then
    /// means what it should: this peer holds a secret key.
    fn verify_public_key(
        &self,
        pk: Self::PublicKey,
        proof: &Self::OwnershipProof,
        already_seated: &[Self::PublicKey],
        ctx: &DeckCtx,
    ) -> Result<Verified<Self::PublicKey>, VerifyError>;

    /// `n`-of-`n`. There is deliberately no `t`-of-`n` variant: `SPEC_CS.md` §35
    /// forbids one for hole cards, and a library that offered it would still not be
    /// allowed to be called that way here.
    fn aggregate_key(&self, seats: &[Verified<Self::PublicKey>]) -> Self::AggregateKey;

    // ---- the shuffle chain ----------------------------------------------------
    fn initial_deck(&self, apk: &Self::AggregateKey) -> Verified<Self::MaskedDeck>;

    fn shuffle(
        &self,
        rng: &mut SysRng,
        apk: &Self::AggregateKey,
        prev: &Verified<Self::MaskedDeck>,
        ctx: &DeckCtx,
    ) -> (Self::MaskedDeck, Self::ShuffleProof);

    /// The structural check of C-3 runs **inside** this call and **before** the
    /// argument — both because a caller must not be able to forget it, and because it
    /// costs 0.019 ms against ~36 ms, which is the cheap rejection D-9 needs.
    ///
    /// Contract: a returned `Invalid` means the emitter is at fault. Every input must
    /// have been established by the caller first (C-8); if one was not, the caller
    /// returns `CouldNotVerify` and never calls this.
    fn verify_shuffle(
        &self,
        apk: &Self::AggregateKey,
        prev: &Verified<Self::MaskedDeck>,
        next: Self::MaskedDeck,
        proof: &Self::ShuffleProof,
        ctx: &DeckCtx,
    ) -> Result<Verified<Self::MaskedDeck>, VerifyError>;

    // ---- reveal ---------------------------------------------------------------
    /// Takes the deck and an index — never a card. A card supplied by a sender is not
    /// a value this trait can be handed, which is what forces the check to be made
    /// against this peer's own final deck and makes an intermediate-deck token fail.
    fn reveal_token(
        &self,
        rng: &mut SysRng,
        sk: &Self::SecretKey,
        pk: &Verified<Self::PublicKey>,
        deck: &Final<Verified<Self::MaskedDeck>>,
        index: CardIndex,
        ctx: &DeckCtx,
    ) -> (Self::RevealToken, Self::RevealProof);

    fn verify_reveal_token(
        &self,
        pk: &Verified<Self::PublicKey>,
        deck: &Final<Verified<Self::MaskedDeck>>,
        index: CardIndex,
        token: Self::RevealToken,
        proof: &Self::RevealProof,
        ctx: &DeckCtx,
    ) -> Result<Verified<Self::RevealToken>, VerifyError>;

    /// `tokens` must be exactly one per key in the aggregate, counted by the caller —
    /// the library sums whatever it is given and never learns how many players exist.
    /// A failure here is a `SoundnessFault`, not a `VerifyError` (D-12, C-10).
    fn open_card(
        &self,
        deck: &Final<Verified<Self::MaskedDeck>>,
        index: CardIndex,
        tokens: &[Verified<Self::RevealToken>],
    ) -> Result<Card, SoundnessFault>;
}
```

**What is deliberately absent, and why.**

* **No street, stage or entitlement argument.** `deck.get(j)` will happily mint a river
  token pre-flop and no `ctx` field can express whether index 12 is due now or whether
  seat 0 may publish for index 0 — a cheat would simply compute the "correct" `ctx` for
  the stage it is cheating in. Gating is irreducibly the protocol layer's, and putting a
  parameter here would suggest otherwise.
* **No `t`-of-`n` reveal, and no partial-opening API of any kind.**
* **No accessor returning a `MaskedCard`.** A card must not become a value that can be
  passed around or serialised.
* **No `Deserialize` for `Verified`, `Final`, `AggregateKey` or `SecretKey`.**
* **No blanket `verify(&[u8])`.** Decoding and verifying are separate steps because
  C-8 requires every input to be established between them.

A **mock implementation** of this trait — plaintext cards, proofs that are one byte — must
exist alongside the real one, so the poker engine, the state machine and the GUI can be
tested without 100 ms proofs, and so that a differential test against the fallback (C-14)
has somewhere to plug in.

---

## 8. The decision this hands the owner

Recorded as a row on `DECISIONS.md`'s open list. In summary:

**Fork the vendored ziffle now, or ship upstream-identical bytes?** The changes are small
and known (C-12 a–d), the reasons are D-4 (32/64-bit transcript split — a certain break the
day anything runs in a browser) and D-1 (the reveal proof drops `c2`), and the cost of
deferring is that the wire format is frozen by the first proof that is persisted or
exchanged. Against forking: it puts us on a private wire format that no upstream fix will
match, it makes ziffle's own 16 unit tests and 15 doctests no longer a check on the code we
run without re-verification, and it forfeits interoperability with the one other project
using the crate. The verdict does not decide this; it is a project-shape decision.

Whichever way it goes, **C-1 through C-11, C-13 and C-15 are required regardless**, and
C-3 is required even if the fork happens.

---

## 9. Sources

Six reviews in `docs/research/`: `ZIFFLE_ALGEBRA.md`, `ZIFFLE_FIAT_SHAMIR.md`,
`ZIFFLE_INPUT_HANDLING.md`, `ZIFFLE_PROTOCOL_FIT.md`, `ZIFFLE_ALTERNATIVES.md`,
`ZIFFLE_ATTACKS.md`, each with its own probe crate under
`…/scratchpad/zr-{algebra,fiatshamir,input,protofit,alternatives,break}/` and its own
reproduction instructions. Crate under review:
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ziffle-0.1.0/src/lib.rs`,
1 779 lines. Paper: Bayer & Groth, *Efficient Zero-Knowledge Argument for Correctness of a
Shuffle*, EUROCRYPT 2012 — the **full version** (§5.3, the single value product argument,
appears only there), read by two reviewers from independent copies.

Every measurement quoted here was taken on Windows 10, x86_64-pc-windows-msvc, 24
cores, rustc 1.95.0, `--release`, builds held to 19 cores. Nothing under
`p2p-poker` was modified by any of the six reviews.

**This document is a review, not an audit, and it says so in §6 item 1.**
