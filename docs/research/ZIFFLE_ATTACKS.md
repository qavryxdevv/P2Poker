# ziffle 0.1.0 — adversarial review: trying to forge a shuffle

Reviewer: Phase-1 `ziffle-attack` agent
Date: 2026-08-29
Subject: `ziffle` 0.1.0, unpacked at
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ziffle-0.1.0/`
Companion to `ZIFFLE_ALGEBRA.md`, `ZIFFLE_FIAT_SHAMIR.md`, `ZIFFLE_INPUT_HANDLING.md`,
`ZIFFLE_PROTOCOL_FIT.md`, `ZIFFLE_ALTERNATIVES.md` — all five read before starting.

**The question this document answers, and only this one:** can a proof be produced that
`ziffle`'s own verifier accepts for a deck that is **not** a permutation of the input?

---

## 0. Verdict

**No. I could not forge one, and I do not think one exists at this level.**

Twenty-two attack families were built and run against the real, unmodified crate.
Every attempt on the shuffle argument's *soundness* failed, and each failed at a check I
can name and explain. That is a materially stronger result than the Phase-0 agent's
"13 adversarial tests passed", for three reasons:

- the attacks were built by an **independent re-implementation of the prover** that can lie
  at any point, rather than by perturbing the crate's own honest output (§1);
- the two strongest strategies were then **combined exhaustively** — all 2¹⁶ field-level
  hybrids, every one put through the real verifier (§4);
- and I got the forgery **arithmetically complete** and reduced it to a single hash
  preimage, then showed precisely which two lines of code close that last gap (§3.5, §5).

That last point is the useful one. The soundness of the multi-exponentiation argument
against an unconstrained attacker comes down to `MultiExpArg::challenge_x` absorbing the
ciphertexts as **whole tuples** rather than one coordinate (lib.rs:727–728). I built the
hypothetical where it does not, and the forgery lands in one step with no search. It does,
so it doesn't.

What did break is **everything around the shuffle argument**:

| # | What broke | Exploitable? | Where the fix goes |
|---|---|---|---|
| **B1** | A reveal token issued for one card decrypts a *different* card — the proof binds `c1` and drops `c2` | **Yes — executed end to end at N = 52** | ziffle (2 lines), or our wrapper |
| **B2** | A shuffler can leave *chosen* deck slots in plaintext, masking the other fifty, and the proof verifies | **Yes — executed** | our wrapper |
| **B3** | A shuffler can publish a proof from which **anyone** recovers his whole permutation | **Yes — executed, all 52 positions recovered from public data** | our wrapper |
| **B4** | A shuffle proof has ≥ 2⁶⁶ distinct byte encodings, all accepted; trailing garbage is ignored | **Yes — measured, 63/63** | our wrapper |
| **B5** | `pk = identity` passes `OwnershipProof::verify` with a proof anyone can write | **Yes — forged** | our wrapper |
| **B6** | A player can rebroadcast another player's `(next, proof)` as his own | **Yes** | our `ctx` |

None of B1–B6 is a break of the Bayer–Groth argument. All six are cases where the crate
proves exactly what it claims and the *claim is not what a poker game needs*. Every one
is closed by rules in §9, and none of those rules costs more than a few lines.

**B1 deserves more weight than "medium".** §5 shows that the identical mistake — binding
one coordinate of a ciphertext instead of both — would, if made in the shuffle transcript,
have produced a one-step forgery of the whole shuffle. The author made it once, in the
reveal proof. It is the same class of bug, in a place where the blast radius is a player's
hole cards rather than the deck's integrity.

**The one sentence for the Phase-2 decision:** the shuffle argument held under every
attack I could construct; the crate's *surface* did not, and using it safely means
wrapping it in the checks in §9, not trusting `Some(Verified<…>)` on its own.

---

## 1. The harness: an independent forger, not a perturbed prover

The crate's prover is private (`ShuffleProof::new` takes a `perm: &[usize; N]` and is not
`pub`), so an attacker cannot ask it to prove a lie. But `ShuffleProof<N>` and
`MaskedDeck<N>` both implement `CanonicalSerialize`/`CanonicalDeserialize`, which is
exactly the surface a hostile peer has. So I wrote the prover again, from scratch:

- `PedersonCommitKey::default` re-derived from the same two seeds;
- `Transcript` re-implemented byte for byte, including the `usize::to_be_bytes()` framing;
- both sub-argument provers re-implemented with cheat knobs at every step;
- a wire writer matching `impl_valid_and_deser!`'s field order.

The forged bytes are then handed to the **real crate** — parsed by
`ziffle::ShuffleProof::<52>::deserialize_compressed` and judged by
`ziffle::Shuffle::<52>::verify_initial_shuffle` / `verify_shuffle`. **Every accept/reject
verdict in this document comes from the crate's own verifier.** A local mirror of `verify`
is used only to report *which of the eight checks* fired; it never decides a verdict.

**Calibration (measured).** The forger's honest output must be accepted, or nothing below
means anything:

```
honest shuffle, forged by MY prover      *** ACCEPTED ***  mexp[c1=1 c2=1 c3=1 c4=1] svp[c1=1 c2=1 c3=1 c4=1]
proof size = 5547 bytes (ziffle docs say 5547)
honest shuffle, ziffle's own prover      *** ACCEPTED ***
```

5547 bytes on the nose, and the crate accepts it. The transcript, the commitment key and
the wire format are reproduced exactly; had any of the three been off by one byte the
challenge would have differed and the proof would have been rejected.

**Notation used throughout.** `mexp[c1..c4]` are `MultiExpArg::verify`'s four checks in
source order (lib.rs:815, 823, 830, 833); `svp[c1..c4]` are
`SingleValueProductArg::verify`'s four (lib.rs:994, 1001, 1014, 1017). `1` = passed.

---

## 2. Every attack, and what happened

| # | Attack | Result |
|---|---|---|
| A0 | calibration — honest forged proof | accepted (as required) |
| A1 | deck with a **duplicated card**, two prover strategies | **rejected** |
| A2 | duplicated card + `ct_mxp1` forced to the verifier's own recomputation | **rejected** |
| A3 | one ciphertext replaced by a **fresh encryption** of a chosen card | **rejected** |
| A4 | **homomorphic plaintext offset** — turn a card into another in place | **rejected** |
| A4b | two-slot *cancelling* offset (needs `x` before `next`) | **rejected** |
| A5 | grinding the shuffle challenge `x` by re-rolling `c_pi`'s blinding | **rejected**, 0 of 3 000 |
| A6 | identity points / zero scalars in every proof field, round 1 and round 2 | **rejected**, 0/11 and 0/10 |
| A7 | replay across `ctx`, `apk`, rounds, and provers | 3 accepted — §6 (B6) |
| A8 | proof malleability: sub-argument grafting and wire encoding | 2 accepted — §6 (B4), §7 |
| A9 | degenerate decks (`ρ = 0`, shared `ρ`, identity permutation, no-op) | **all accepted** — §6 (B2, B3) |
| A10 | identity public key, rogue key, `apk = O` | 2 accepted — §6 (B5) |
| A11 | off-curve points on the wire | **rejected** by the algebra; parse rejected under `Validate::Yes` |
| A12 | **reveal-token confusion** — one card's tokens decrypt another | **SUCCEEDED** — §6 (B1) |
| A13 | Fiat–Shamir grinding on the multi-exp challenge, 1.2 M guesses | **rejected**, 0 hits — but the forgery is otherwise *complete*, §3.5 |
| A14 | grafting a sub-argument across two *different* statements | **rejected** |
| A15 | targeted repair of the single value product argument | **rejected** |
| A16 | **selective plaintext slots** — two cards in the clear, fifty masked | **SUCCEEDED** — §6 (B2) |
| A17 | does an honest re-shuffle repair a poisoned deck? can a later shuffler poison one? | repairs; cannot poison — §6 (B1) |
| A18 | **recovering the permutation from public data** on a shared-`ρ` deck | **SUCCEEDED** — §6 (B3) |
| A19 | the whole tampering battery against `verify_shuffle` (round 2, real masked `prev`) | **rejected**, same as round 1 |
| A20 | reveal-token aggregation: empty, partial, doubled | fails closed — §8 |
| A21 | **exhaustive 2¹⁶ hybrid** of the two best cheating strategies | **rejected**, 0 of 65 536 accepted |
| A22 | the **near miss** — making `ct_mxp0` independent of the guessed challenge | **rejected**, but only by one line of the transcript — §5 |

---

## 3. The forgery attempts, in detail

### 3.1 A1 — a deck with a duplicated card

`next[1] := remask(prev[0])`. Card 0 appears twice, card 1 is gone. Two prover strategies:

```
commit to the non-permutation itself           rejected   mexp[c1=0 c2=1 c3=1 c4=1] svp[c1=1 c2=1 c3=1 c4=0]
commit to a real permutation, deck duplicated  rejected   mexp[c1=0 c2=1 c3=1 c4=1] svp[c1=1 c2=1 c3=1 c4=1]
```

The two rows are the two halves of Bayer–Groth doing their separate jobs:

- **Row 1** commits `c_π` to the map `(0,0,2,3,…)`, which is not a permutation. *Both*
  halves fire. The interesting one is `svp c4`:
  `b̃_{N-1} = x·∏(y·π(i) + x^{π(i)} − z)` no longer equals the public
  `x·∏_{i=1}^{52}(y·i + x^i − z)`, because the multiset `{(π(i), x^{π(i)})}` is not
  `{(i, x^i)}`. That is the paper's §3 argument, and `y, z` are derived *after* `c_π` and
  `c_{xπ}` (lib.rs:1093 → 1100), so the prover cannot steer them.
- **Row 2** commits to a genuine permutation instead, so the product argument becomes
  honest and passes all four of its checks — and the multi-exponentiation argument catches
  the deck on its own, at `mexp c1`: `ct_mxp1` no longer equals `∏ prev_i^{x^i}`.

Row 2 is the one that matters: it isolates the multi-exponentiation argument by removing
every reason for the product argument to complain. Neither half alone would be enough;
the composition is what closes it, and §3.2 attacks the surviving half directly.

### 3.2 A2 — forcing `ct_mxp1`, the strongest shape available

Row 2 above failed at `mexp c1`, which compares the prover's `ct_mxp1` against a value the
verifier recomputes from `prev` alone. So force it: set `ct_mxp1` to the recomputed value
and make check 1 pass **by construction**, while keeping the product argument fully honest
on a real permutation. Only `mexp c4` is left to catch the lie.

```
ForceMxp1                              rejected  mexp[c1=1 c2=1 c3=1 c4=0] svp[c1=1 c2=1 c3=1 c4=1]
ForceMxp1AndTauC1                      rejected  mexp[c1=1 c2=1 c3=1 c4=0] svp[c1=1 c2=1 c3=1 c4=1]
[control] ForceMxp1 on an honest deck  *** ACCEPTED ***  mexp[c1=1 c2=1 c3=1 c4=1] svp[c1=1 c2=1 c3=1 c4=1]
```

**The control row is the important one.** It proves the rejection is caused by the *deck*,
not by the forcing mechanism: on an honest deck the forced `ct_mxp1` equals the true one,
and the same code path is accepted. So `mexp c4` is genuinely measuring the deck.

**Why `c4` cannot be repaired.** Write `P(s) = (Σ next_{i,1}s_i, Σ next_{i,2}s_i)`. Check 4
rearranges to

```
U + x·V  ∈  { (G·t, apk·t) : t ∈ Z_q }
U = ct_mxp0 − P(α) − (0, G·β)      fixed before x
V = ct_mxp1 − P(a)                 fixed before x, a = c_xpi's opening
```

After the challenge the prover's only free response is `τ`; `o_α` and `o_r` are pinned to
an opening of `c_α + x·c_xpi` by check 2, and `β, o_β` are pinned to `c_β` by check 3.
One free scalar cannot satisfy two independent group equations unless `U` and `V` each lie
on that line separately — and "`V` on the line" *is* the true statement
`∏ prev^{x^i} = ∏ next^{a} · E(1;t)`.

**Knowing the aggregate secret key does not help.** I worked this through: if the attacker
knew `a = dlog_G(apk)`, membership of the line would be the test `V_2 = a·V_1`, and
knowing `a` lets him *check* that but not *change* `V_2`. `V` is fixed by the statement.
(And an attacker who knows `a` can decrypt the deck anyway, so the case is moot.)

### 3.3 A3 — a substituted card

Slot 0 replaced by a fresh encryption of a card of my choosing:

```
fresh encryption in slot 0, honest prover   rejected  mexp[c1=0 c2=1 c3=1 c4=1] svp[...all 1]
fresh encryption in slot 0, ForceMxp1       rejected  mexp[c1=1 c2=1 c3=1 c4=0] svp[...all 1]
honest proof, deck swapped afterwards       rejected
```

Same two-front rejection. The third row confirms the deck is bound into the transcript at
`ts3` (lib.rs:1092), so a proof cannot be detached from the deck it was made for.

### 3.4 A4 — the homomorphic offset, and why the ordering saves it

This is the attack that *should* work if the transcript ordering were wrong, so it is
worth stating carefully.

ElGamal here is additively homomorphic on `c2`. Every card plaintext is `s_i·G` where
`s_i` comes from `StdRng::from_seed(SHA256(b"CARDS-V1"))` — a **public, deterministic
seed**, so *every player knows every `s_i`*. An attacker can therefore turn slot `k`'s card
into any other card in place:

```
next[k].c2 += (s_target − s_source)·G
```

leaving `c1` — and therefore the re-masking randomness `ρ_k` he must prove — completely
intact.

```
one-slot offset, honest prover  rejected  mexp[c1=0 c2=1 c3=1 c4=1]
one-slot offset, ForceMxp1      rejected  mexp[c1=1 c2=1 c3=1 c4=0]
```

**The cancelling version.** With the plaintexts reduced to known scalars, check 4's message
component is the scalar equation `Σ_j s_j x^j = Σ_i s'_i x^{σ(i)}`. Two offsets `δ_a, δ_b`
cancel exactly when `δ_a x^{σ(a)} + δ_b x^{σ(b)} = 0`, i.e. `δ_b = −δ_a x^{σ(a)−σ(b)}` —
which requires knowing `x` **before** fixing `next`. It is not available: `next` is absorbed
at `ts3` and `x` is derived from `ts4`. I ran the circular attempt anyway, choosing the
`δ`s from an `x` computed on the unmodified deck:

```
two-slot cancelling offset (x guessed first)  rejected  mexp[c1=1 c2=1 c3=1 c4=0]
x used to pick the deltas != x the verifier derives: true
residual plaintext error under the real x is zero: false
```

**This is the single most load-bearing ordering in the whole construction**, and it is
correct: `prev` at `ts2`, `next` at `ts3`, `c_π` at `ts4`, `x` from `ts4`.

### 3.5 A5 and A13 — grinding, measured rather than asserted

Two independent grinding surfaces, because they are the only two places a cheating prover
can resample for free.

**A5 — grinding `x`.** `c_π`'s blinding factor `w_π` is entirely the prover's choice and
is not otherwise constrained, so he can resample `x` at the cost of one commitment. He
needs a degree-52 polynomial identity in `x` to hold; it has at most 52 roots in a field
of size ≈ 2²⁵⁶.

```
3000 fresh challenges in 301.0s, best checks passed = 7/8, ACCEPTED = 0
expected acceptances if sound: 3000 * 52/2^256 ~ 2^-242
```

Read "best 7/8" correctly: it is not a near miss. The `ForceMxp1` strategy passes seven
checks on *every* attempt and fails `mexp c4` on every attempt; re-rolling the challenge
never moves it off seven. That flatness is the result — the failing check does not
fluctuate with `x`, which is what you would expect if the check is measuring the deck
rather than an accident of the challenge.

**A13 — grinding the multi-exponentiation challenge.** The sharper version. Choose a target
challenge `g`, then *solve* for the `ct_mxp0` that makes check 4 hold identically at `g`:

```
ct_mxp0 := (G·τ + P1(α) + g·P1(xπ) − g·ct_mxp1.c1,
            G·β + apk·τ + P2(α) + g·P2(xπ) − g·ct_mxp1.c2)
```

With `ct_mxp1` already forced so check 1 holds, this construction satisfies **every**
multi-exponentiation check — provided the transcript hashes that `ct_mxp0` to exactly `g`.

**I verified the construction rather than assuming it**, because "0 hits" from a broken
forgery proves nothing:

```
SANITY: at the assumed challenge, mexp check 4 holds: c1=true c2=true
SANITY: mexp check 1 holds by construction: true
=> the forgery is complete EXCEPT that the transcript must hash
   ct_mxp0 to exactly the g it was built from. That is the only gap.
```

So on a deck with a duplicated card, **the multi-exponentiation forgery is complete and
correct, and the single thing standing between it and acceptance is a 256-bit hash
preimage.** That is the reduction, executed. Then the search:

```
 200000 guesses in 182.7s (1095/s): hits = 0, longest matching hex prefix = 3/64
1000000 guesses in 555.6s (1800/s): hits = 0, longest matching hex prefix = 5/64
```

The prefix column is the useful part. Three matching hex characters (12 bits) out of 64 for
2·10⁵ ≈ 2¹⁷·⁶ draws, and five (20 bits) for 10⁶ ≈ 2¹⁹·⁹ — both are what uniform draws
should produce, and the growth from 3 to 5 as the sample size rises by 5× is the shape of a
random function, not of a function with exploitable structure. That behaviour is the whole
requirement.

The fork makes this grind *cheaper to iterate* — the multi-exp challenge does not depend on
the product argument's messages, so the search never has to redo the product argument — and
not *more likely to succeed*. That distinction is the crux of the Fiat–Shamir question, and
it matches `ZIFFLE_FIAT_SHAMIR.md`'s FS-1.

### 3.6 A15 — trying to repair the product argument directly

A1 row 1 failed only at `svp c4` (`b̃_{N-1} ≠ x·public_product`). The obvious next move is
to set `b̃_{N-1}` to exactly what check 4 demands:

```
baseline: non-permutation, ct_mxp1 forced   rejected  mexp[c1=1 c2=1 c3=1 c4=0] svp[c1=1 c2=1 c3=1 c4=0]
+ b~[N-1] := x_svp * public product         rejected  mexp[c1=1 c2=1 c3=1 c4=0] svp[c1=1 c2=0 c3=1 c4=1]
+ b~[N-1] and a~[N-1] both shifted          rejected  mexp[c1=1 c2=1 c3=1 c4=0] svp[c1=0 c2=0 c3=1 c4=1]
rescaling s~ recovers the proof: false
```

Check 4 flips to `1` and check 2 immediately flips to `0`. Check 2 is the chained identity
`x·b̃_{i+1} − b̃_i·ã_{i+1}` over all 51 consecutive pairs, committed to by `c_δ` and `c_Δ`
**before** `x_svp` is derived. Moving one entry of `b̃` breaks the chain at that link, and
the only response that could absorb it (`s̃`) is a single scalar facing a 52-slot vector
equation. The two checks are not independently satisfiable, which is the point of the
construction.

### 3.7 A19 — the same battery against the chained path

Round 1 is a special case: every `prev.c1` is the point at infinity (lib.rs:1327), so a
bug that only shows on a genuinely masked deck would be invisible there. I re-ran the whole
battery against `verify_shuffle` with a `prev` produced and verified by the crate itself:

```
prev is a real masked deck: c1 all distinct = true, none is the identity = true
[control] honest permutation                   *** ACCEPTED ***  all 8 checks
duplicated card, committed as itself           rejected   mexp[c1=0 ...] svp[... c4=0]
duplicated card, committed as a permutation    rejected   mexp[c1=0 ...] svp[all 1]
duplicated card + ct_mxp1 forced               rejected   mexp[c1=1 c2=1 c3=1 c4=0] svp[all 1]
plaintext offset on slot 3                     rejected   mexp[c1=0 ...]
plaintext offset + ct_mxp1 forced              rejected   mexp[... c4=0]
[control] ct_mxp1 forced, honest deck          *** ACCEPTED ***
a card turned into an unrevealable plaintext   rejected
```

Identical behaviour to round 1. The identity `c1` of the initial deck is not hiding
anything.

### 3.8 A6 — degenerate proof elements

Every point in a `ShuffleProof` set to the identity, one at a time; every scalar set to
zero, one at a time; then all at once. Round 1 and round 2:

```
ROUND 1 (prev = initial deck, whose c1 are ALL the point at infinity)
   honest ct_mxp1 = (O, point)  [c1 already the identity? true]
   ct_mxp1.c1   := O  ACCEPTED   (proof bytes changed: false)
   identity substitutions accepted: 1/11 (1 of them were no-ops)
   zero-scalar substitutions accepted: 0/10

ROUND 2 (prev = a real masked deck, c1 uniformly random)
   honest ct_mxp1 = (point, point)  [c1 already the identity? false]
   identity substitutions accepted: 0/11
   zero-scalar substitutions accepted: 0/10

all-identity / all-zero proof            rejected  mexp[c1=0 c2=1 c3=1 c4=1] svp[c1=0 c2=1 c3=1 c4=0]
```

The single round-1 acceptance is **not a finding**: on the initial deck the honest
`ct_mxp1.c1` already *is* the identity, so the substitution changed nothing — the harness
reports `proof bytes changed: false`, and round 2 shows 0/11. I flag it because it is
exactly the kind of row that would be misreported as a hole.

**Small-order points do not exist here.** `secp256k1` has prime order and
`COFACTOR = [1]` (printed by the probe), so the identity is the only point of order below
`q`. There is nothing of small order to plant.

**Non-canonical scalars are rejected at parse time:**

```
o_alpha[0] = 0xff..ff (>= q) parses: false
o_alpha[0] = q exactly parses: false
```

**One legal degeneracy is accepted, and it is harmless.** A prover who sets `β = 0`,
`o_β = 0`, `c_β = O` and is otherwise honest is accepted (both rounds). This is correct:
at `m = 1` the paper's `b_k` blinders do no work (`ZIFFLE_ALGEBRA.md` §6.4 reaches the same
conclusion analytically), and `c_β = com(0;0) = O` is a genuine commitment. It is worth
recording only so that a future reader does not mistake `c_β = O` on the wire for tampering.

### 3.9 A11 — off-curve points

```
uncompressed round-trip, Validate::Yes -> Some(true)     (honest proof, sanity)
uncompressed round-trip, Validate::No  -> Some(true)
deck with an off-curve c1, Validate::Yes -> None         (parse rejected)
deck with an off-curve c1, Validate::No  -> Some(false)  (parsed, verify rejected)
33 arbitrary bytes with the infinity flag decode to the identity: true
```

Consistent with `ZIFFLE_INPUT_HANDLING.md` §6. `Validate::Yes` rejects the point at parse
time; `Validate::No` lets it through and the *algebra* rejects it, because the
multi-exponentiation identity depends on a homomorphism that stops holding once a point
from a different curve enters the sum.

**I did not attempt the harder construction** where every point lies on the same twist —
see §10.

### 3.10 A14 — grafting across statements

```
A's multi-exp half + B's SVP half, on deck A   rejected  mexp[all 1] svp[c1=0 c2=0 c3=1 c4=0]
A's SVP half + B's multi-exp half, on deck A   rejected  mexp[c1=0 c2=0 c3=1 c4=0] svp[all 1]
```

Both branches of the fork start from a state that has already absorbed `apk`, `prev`,
`next`, `c_π` and `c_{xπ}`, so a sub-argument cannot be moved between statements. Only a
graft between two proofs with *identical* first messages works — §7.2.

---

## 4. The exhaustive hybrid search (A21)

The two strategies of §3.1–3.2 each satisfy a different half of the multi-exponentiation
argument on the *same* false statement:

- **P1** (true `ct_mxp1`) satisfies `mexp c4`, fails `mexp c1`;
- **P2** (forced `ct_mxp1`) satisfies `mexp c1`, fails `mexp c4`.

Both are built on the same duplicated-card deck with the **same `c_π` and `c_{xπ}` blinding
factors**, so they share the transcript state right up to the fork and every hybrid between
them is a well-formed candidate rather than an obviously mis-framed one. There are 16
sub-argument fields, so 2¹⁶ hybrids. I enumerated all of them and put every one through the
real verifier:

```
P1 and P2 share c_pi: true, share c_xpi: true
P1 (true ct_mxp1)     rejected  mexp[c1=0 c2=1 c3=1 c4=1] svp[c1=1 c2=1 c3=1 c4=1]
P2 (forced ct_mxp1)   rejected  mexp[c1=1 c2=1 c3=1 c4=0] svp[c1=1 c2=1 c3=1 c4=1]
65536 hybrids checked by the real verifier in 48.0s, ACCEPTED = 0
(mask 0x0000 is P1 and 0xffff is P2, so the count excludes nothing)
```

This is the combinatorial core of the forgery question — "can the two things a cheating
prover *can* do be combined into the one thing he needs?" — settled by exhaustion rather
than by argument.

---

## 5. The near miss: which line of the transcript is holding it up (A22)

> **Read this section carefully: it does not report a bug.** It reports a forgery that
> works against a *hypothetical* transcript I wrote, and does not work against ziffle's.
> The point is to say what ziffle's soundness is resting on, not to claim it is broken.

§3.5 reduced the forgery to "make `ct_mxp0` hash to the `g` it was built from". There is one
way to attack that other than brute force: **make `ct_mxp0` not depend on `g` at all.**

The attacker can achieve that for *half* of `ct_mxp0`. Choose the deck so every
`next[i].c1` is the point at infinity — `ρ = 0` on the initial deck, which the verifier
accepts (B2). Then

```
P1(alpha)  = sum next[i].c1 * alpha[i] = O
P1(xpi)    = sum next[i].c1 * xpi[i]   = O
ct_mxp1.c1 = sum prev[i].c1 * x^i      = O      (the initial deck's c1 are all O)
```

and the whole first coordinate collapses to `ct_mxp0.c1 = τ·G`, with no `g` in it. Measured
on a duplicated-card deck:

```
every next[i].c1 is the point at infinity: true
ct_mxp1.c1 is the identity: true
P1(alpha) is the identity:  true
P1(xpi)   is the identity:  true
plaintext error the deck introduces (nonzero => the statement is false): true
ct_mxp0.c1 is the SAME for two different guesses g: true
ct_mxp0.c2 differs for those guesses:               true
```

So **if the transcript absorbed only the first coordinate of a ciphertext**, the multi-exp
challenge would be fixed before `g` was chosen, the attacker would set `g := x_mexp` in one
step, and the argument would be forged outright with no search at all. I built that
hypothetical transcript and closed the loop on it:

```
HYPOTHETICAL transcript absorbing only ct_mxp0.c1:
  challenge identical across guesses: true   <- a one-step forgery
REAL transcript (absorbs the whole ciphertext tuple):
  challenge identical across guesses: false
  and check 4 would then hold: c1=true c2=true
  check 1 holds by construction: true
```

Under the hypothetical transcript the forgery lands: both multi-exponentiation checks hold,
on a deck with a duplicated card, in a single step. Under the real transcript the same
construction needs a full preimage.

`ShuffleProof`'s transcript absorbs `ct_mxp0` and `ct_mxp1` as whole tuples
(`MultiExpArg::challenge_x`, lib.rs:727–728), so the attack does not exist. **The entire
soundness of the multi-exponentiation argument, against an attacker who is otherwise
unconstrained, rests on those two lines.**

**Why this matters beyond the shuffle proof.** This is exactly the mistake that *was* made
in `RevealTokenProof::challenge`, which absorbs `c1` and not `c2` (lib.rs:428–444) — and
which produces the one real card-secrecy break in this review (B1). The author made the
one-coordinate mistake once and did not make it in the place where it would have been
fatal. That is worth knowing when weighing how seriously to take B1: it is not a stylistic
nit, it is the same class of bug that would have broken the shuffle argument.

---

## 6. What did succeed

None of these forges a shuffle. All of them let a malicious player learn or reveal a card
he is not entitled to, or misrepresent what he did, **through proofs the crate accepts**.

### B1 — a reveal token for one card decrypts another (A12)

`RevealTokenProof::challenge` (lib.rs:428–444) absorbs `pk`, `share`, `c1`, `t_g`, `t_c1`.
`RevealTokenProof::verify` (lib.rs:484) destructures the card as
`let MaskedCard((c1, _)) = card;` — **`c2` is discarded**. A token proof is therefore bound
to `c1` alone and is valid for every card sharing that `c1`.

Nothing forces `c1` to be unique. On the initial deck every `c1` is the identity
(lib.rs:1327), so the first shuffler need only reuse one re-masking scalar across two slots:
`ρ_a = ρ_b` gives `c1_a = c1_b` while the `c2` differ. Executed at N = 52 through the
public API:

```
colliding-c1 deck passes verify_initial_shuffle *** ACCEPTED ***
tokens issued for card 0 verify against card 0: true true
the SAME tokens verify against card 1 too:      true true
card0 index = Some(37)   card1 index from card0's tokens = Some(40)
=> two distinct cards revealed from one set of tokens: *** YES ***
```

**The attack in game terms.** Collide a card the protocol will legitimately open to
everybody — a flop card, a burn card, a showdown card — with a card that must stay secret,
a victim's hole card. When the honest players hand over their reveal tokens for the public
card, as the rules require them to, those same tokens decrypt the victim's hole card. Every
proof involved verifies and the victim has no way to notice.

This independently reproduces `ZIFFLE_FIAT_SHAMIR.md` FS-3, at N = 52 and through a
different code path (my forger rather than a constant-output RNG).

**The threat model, stated precisely — I measured it (A17):**

```
poisoned deck 1 accepted, c1 collision present: true
after ONE honest re-shuffle, colliding c1 pairs remaining: 0
brute-forcing that discrete log over 2^17.6 candidates: found = false
colluding second shuffler preserves the collision (rho = 0) *** ACCEPTED ***
collision still present after that shuffle: true
```

So: **the collision can only be created by the first shuffler** (a later one would have to
solve `r_a − r_b = dlog(prev[π(b)].c1 − prev[π(a)].c1)`), and **one honest re-shuffle
destroys it**. It therefore requires every shuffler to be malicious — or a chain of length
one. That is a real mitigation. It is also exactly the kind of mitigation you must not be
leaning on without knowing it, because it depends entirely on our layer getting the chain
right.

**Fix.** Two lines in ziffle: absorb the whole ciphertext in `RevealTokenProof::challenge`
and pass the full card through. Failing that, rule 4 in §9 — reject any deck with a
duplicate or identity `c1` — closes it from outside.

### B2 — chosen slots left in plaintext (A16)

`remask_card` accepts any `r`, including zero, and the shuffle argument constrains `r` not
at all — proving that *a* permutation with *some* randomizers was applied, not that either
was chosen well. On the initial deck, `ρ_i = 0` leaves slot `i` as `(O, plaintext)`.

The first shuffler chooses the permutation, so he knows which card lands in which slot. He
can therefore leave *exactly the slots he wants* in the clear and mask the other fifty:

```
two slots unmasked, fifty correctly masked  *** ACCEPTED ***
slots 0..3 read straight off the wire: [Some(9), Some(22), None, None]
true cards at those slots:             [Some(9), Some(22), Some(1), Some(29)]
c1 at slots 0,1 is the identity: true true   c1 at slot 2 is a real point: true
```

Slots 0 and 1 are readable by anyone; slots 2 and 3 are not. The whole-deck version (`ρ = 0`
everywhere) is obvious enough to spot by eye; **this one is not**, and it is the version an
attacker would actually use.

The `ρ = 0` degeneracies are all accepted (A9, A19):

```
rho = 0 for every card                                *** ACCEPTED ***  (whole deck in the clear)
identity permutation (player contributes no shuffling) *** ACCEPTED ***
full no-op: next == prev exactly                       *** ACCEPTED ***  (next == prev: true)
full no-op at round 2 (next == prev)                   *** ACCEPTED ***
```

A player can *appear* to shuffle while provably not touching the deck. This is correct
BG12 behaviour and a fatal integration trap: **a valid shuffle proof is not evidence that
the shuffler randomised anything.**

### B3 — the shuffler's permutation recovered from public data (A18, A19)

A shuffler who uses one `ρ` for all 52 slots produces a deck whose ciphertext *differences*
are preserved, so the mapping from `prev` to `next` is public. The proof verifies. I
recovered the permutation as an observer holding only `next`, the public open deck and
`apk` — no witness, no secret:

```
shared-rho deck accepted                     *** ACCEPTED ***
tell-tale: all 52 c1 values are equal: true
permutation recovered from `next`, the public open deck and nothing else: CORRECT, all 52 positions
```

and at round 2, from `prev` and `next` alone:

```
one shared rho at round 2  *** ACCEPTED ***
the shuffler's permutation recovered from prev and next: true
```

The consequence for the game: **"k players produced valid shuffle proofs" does not imply
"k independent secret permutations were applied."** Secrecy of the composed deck rests on
the honest shufflers only, and our layer cannot tell from the proofs which those were.

### B4 — proof and deck bytes are massively malleable (A8)

The transcript hashes the *canonical re-serialisation* of each element
(`Transcript::update_with_serialized`, lib.rs:208), not the bytes that arrived. So any
wire-level non-canonicality is invisible to the challenge, and the proof still verifies.

```
canonical proof accepted: true
c_pi byte 32: 63/63 alternative low-6-bit patterns also ACCEPTED
proof with 16 trailing garbage bytes accepted: true
```

A compressed secp256k1 point spends a 33rd byte on two flag bits; `ark-ff`'s
`SerBuffer::to_bigint` never reads that byte's other six (`const_helpers.rs:156`). So every
point has 64 encodings. A `ShuffleProof<52>` contains 11 points → **2⁶⁶ byte strings that
all parse to the same proof and all verify**; a `MaskedDeck<52>` contains 104 → 2⁶²⁴. Add
unlimited trailing bytes and the count is unbounded.

Note what this is and is not. It does **not** let anyone produce a *different* proof — the
parsed value is identical. It does let **anyone, with no witness at all**, take a proof off
the wire and emit unboundedly many distinct byte strings that every peer accepts. Anything
keyed on wire bytes — a dedup cache, a message id, a signature over the encoding, a "have
we seen this proof" set — is defeated for free.

### B5 — `pk = identity` and a forgeable ownership proof (A10)

```
SecretKey(0) parses: true
forged ownership proof for pk = O accepted: true
rogue-key ownership proof found in 5000 tries: false  (Schnorr binds pk)
```

`sk = 0` is a genuine discrete log of the identity, so the Schnorr goes through with
`z = w`. `Verified<PublicKey>` therefore does **not** mean "this peer holds a secret key".

The **rogue-key attack is properly blocked**: `OwnershipProof::challenge` absorbs `pk`
(lib.rs:303–304), so a late player cannot register `pk = t·G − Σ others` without knowing
its discrete log. That is the standard proof-of-possession defence and it is correctly
implemented — a genuine positive.

The degenerate end state, where *every* player uses the identity key, is fully broken and I
drove it end to end:

```
apk is the identity point: true
shuffle under apk = O verifies                *** ACCEPTED ***
reveal token under sk = 0 verifies: true
card 0 decrypted by anyone, no real key involved: Some(14)
```

That configuration needs every player to cooperate in their own undoing, so it is a
soundness-of-the-*setup* problem, not an attack on honest players. The practical bite is
narrower and still real: a seat can contribute a key that adds nothing to the encryption
while appearing to, and anyone can write the proof for it.

### B6 — proofs carry no prover identity or round index (A7)

```
same ctx + same prev, replayed verbatim          *** ACCEPTED ***
different ctx                                    rejected
ctx differing in one byte                        rejected
ctx with one extra trailing NUL byte             rejected
same ctx, different aggregate public key         rejected
round-2 proof offered as an initial shuffle      rejected
round-1 proof replayed at round 2 (prev = deck1) rejected
player B rebroadcasts A's (next, proof) verbatim *** ACCEPTED ***
```

The `ctx` binding is doing real work: six of the eight replays above are rejected, and the transcript
correctly binds `apk`, `prev`, `next` and `c_π`. What it does not bind is **who** shuffled
and **which round** this is. If two players are ever offered the same `prev` — a protocol
fork, a retry after a timeout, a re-deal — B can rebroadcast A's `(next, proof)` and B has
"proved" a shuffle he did not perform, whose permutation he does not know. In a protocol
that reads "valid shuffle proof" as "this player contributed entropy", that is an
entropy-contribution forgery.

And `Transcript::init` hashes `ctx` **raw** — no length prefix, no domain tag:

```
concat("game7","hand3") == concat("game","7hand3") : true
proof under ctx=game7|hand3 accepted under ctx=game|7hand3 *** ACCEPTED ***
```

A `ctx` built by concatenating fields is ambiguous. Ours must be a fixed-width hash, never
a concatenation.

---

## 7. What the fork actually buys and costs (A8, A13, A14)

`ShuffleProof::new` clones the transcript at lib.rs:1133 and derives both sub-arguments from
the same state, separated by the domain tags `ziffle/BG12MultiExpArgX/v1` and
`ziffle/BG12ProductArgX/v1`. Three measurements pin down what that does:

**It does not admit a cross-statement graft** (A14): both branches fork from a state that
already absorbed `apk`, `prev`, `next`, `c_π` and `c_{xπ}`, so a sub-argument moved between
statements fails all four of its own checks.

**It does admit a same-first-message graft** (A8):

```
c_pi equal: true, c_xpi equal: true, whole proof equal: false
A2's SVP half + B2's multi-exp half   *** ACCEPTED ***
```

Two proofs on the same statement that share `c_π` **and** `c_{xπ}` have identical fork
bases, so their halves are interchangeable. With a sequential transcript the product
argument's challenge would depend on the multi-exp messages and the graft would fail. But
producing two such proofs requires the openings of `c_π` and `c_{xπ}` — i.e. the witness —
so **only the prover can do this**, and a prover can already mint arbitrarily many valid
proofs by resampling. **I could not turn this into an advantage for a non-prover.** It is a
precise characterisation of the fork's structural cost, not an exploit. The malleability
that a non-prover *can* exploit is B4, and that has nothing to do with the fork.

**It makes the grind cheaper to iterate but not more likely to succeed** (A13, §3.5).

On this question I agree with `ZIFFLE_FIAT_SHAMIR.md` FS-1, and I now have execution behind
the agreement rather than only reading.

---

## 8. Loose ends I closed, briefly

**Reveal-token aggregation (A20).** `AggregateRevealToken::new` sums whatever it is given
and never learns how many players exist. Empty, partial and double-counted aggregates all
fail closed:

```
correct aggregate       -> Some(8)
EMPTY aggregate         -> None
one-of-two aggregate    -> None
player 1 counted twice  -> None
```

Failing closed is the right direction, but it fails *silently and identically* to a
genuinely corrupt token, so our layer must count seats itself rather than infer anything
from a `None`.

**`Transcript` framing.** Every `append`/`append_vec` writes
`state ‖ len(label) ‖ label ‖ len(data) ‖ data`, with an element index in `append_vec`, and
every element is fixed-size. Unambiguous. The only raw input anywhere is `ctx` (B6).

---

## 9. Rules our integration layer must enforce

Consolidated from what actually broke. All of these belong in one wrapper function that
runs **before** `verify_shuffle` is ever called, and all of them are cheap.

| # | Rule | Closes |
|---|---|---|
| 1 | **Compressed encoding only, `Validate::Yes` always.** Ban every `*_unchecked` deserialiser in CI. | A11 |
| 2 | **Enforce the exact expected byte length** per message type before parsing. | B4 |
| 3 | **Re-serialise after parsing and compare to the input; reject on mismatch.** Hash and sign only the canonical form. Never key a cache, id or signature on received bytes. | B4 |
| 4 | On every `next` deck: **no `c1` is the identity; the `c1` are pairwise distinct; no `c1` appears in `prev`; `next != prev`.** | B1, B2, B3 |
| 5 | **Reject `pk == identity`; reject duplicate `pk` across seats.** Authenticate the seat separately from the ownership proof. | B5 |
| 6 | **Every seated player shuffles, in a fixed committed order, and cards are dealt only from the last deck in the chain.** A disconnect must abort the hand, not truncate the chain. | B1, B2, B3 |
| 7 | `ctx` must be a **fixed-width hash** of `(game_id, hand_number, all verified seat pks, shuffle_step_index, shuffling_player_pk)` — never a concatenation. | B6 |
| 8 | **One shuffle proof per peer per position per hand**; a second is a protocol violation, not something to verify. Rate-limit shuffle messages. | DoS (`ZIFFLE_INPUT_HANDLING.md` F7) |
| 9 | Treat a valid shuffle proof as evidence of **a** permutation, never of a *random* one. Unpredictability comes only from the honest shufflers in the chain. | B2, B3 |

**Rule 4 is the highest-value line in this document.** It is a three-line function; on
`ZIFFLE_INPUT_HANDLING.md` §10's measurements it costs about 0.02 ms against ~36 ms of
proof verification, roughly 1 900× cheaper; and on its own it kills B1, B2 and B3 — the
only three attacks in this review that let a player see a card he should not.

---

## 10. What I did not manage to check

Stated plainly, because a confident summary that skipped these would be worth less than the
gaps.

1. **I did not prove soundness.** Twenty-two attack families failing is evidence, not a
   proof. The extraction argument is `ZIFFLE_ALGEBRA.md` §4's hand-trace, and it is a
   careful reading rather than a proof either. **Neither document establishes witness-extended
   emulation, and between us that gap is still open.** What I can say is narrower and still
   worth something: every degree of freedom I could identify for a cheating prover is closed,
   each by a named check; the two strongest strategies do not combine (§4); and the last
   remaining gap reduces to a hash preimage whose closure I located exactly (§5).

   The specific thing an attack review cannot do: I tested the checks **as written**. A
   check that is wrong but self-consistent — one that accepts a family of false statements
   the paper's checks would reject — passes every test in this document. That is why §5's
   reduction matters more than the count of failed attacks: it says what the multi-exp
   argument is resting on, not merely that I could not push it over.

2. **The all-on-one-twist off-curve deck.** `ZIFFLE_INPUT_HANDLING.md` §9 left this open and
   so do I. I confirmed the naive smuggle fails and that the compressed path makes it moot,
   but I did not attempt the construction where every point lies on the same twist. The
   mitigation is the same either way (rule 1), which is why I stopped.

3. **Grinding was measured at 10³–10⁶ scale, not 2⁸⁰.** The numbers in §3.5 show the attack
   surface behaves as a 256-bit preimage search over the range I could reach. They cannot
   rule out structure that only appears at a scale I cannot test. What raises my confidence
   above the raw count is §5: I found the one structural shortcut that would have removed the
   search entirely, and it is closed.

   I also did not search for a *third* cheating strategy to feed into §4's hybrid. The
   exhaustive result covers the two strategies I found; if a third exists, its hybrids with
   these two were not enumerated.

4. **I did not attack `DefaultFieldHasher<Sha256>` itself.** If arkworks' `hash_to_field`
   produced correlated or low-entropy outputs for `[y, z]`, the §3 Schwartz–Zippel argument
   would weaken and several of my rejections would be reachable. I took it as correct, as
   `ZIFFLE_ALGEBRA.md` §7.3 did.

5. **I did not attempt the discrete log.** Every attack here assumes secp256k1 is hard. The
   one brute-force in A17 covers 2¹⁷·⁶ candidates and is a demonstration, not a search.

6. **`ZIFFLE_FIAT_SHAMIR.md` FS-2 (the 32-bit/64-bit `usize` transcript split) was not
   exercised.** Everything here ran on one x86-64 target, so every proof was produced and
   verified under the same 8-byte framing. That finding stands on the other reviewer's
   reading; I neither confirmed nor refuted it.

7. **Timing and side channels: not examined at all.** `verify` short-circuits with `&&`, so
   a peer can learn *which* check failed from timing. That leaks little (the prover already
   knows), but nobody has looked at `open_deck.iter().position()` in `reveal_card`
   (lib.rs:1623) or at the field comparisons.

8. **I did not review the algebra against the paper.** That is `ZIFFLE_ALGEBRA.md`'s job and
   this document depends on it. If an exponent is wrong there, my attacks would not have
   found it — they target the *checks as written*, and a wrong-but-self-consistent check
   passes every test in here.

---

## 11. Reproducing this

```bash
export PATH="~/.cargo/bin:$PATH"
cd "<scratchpad>"
cargo build --release -j 19
./target/release/zr-break.exe                        # all sections
./target/release/zr-break.exe a12 a16 a18            # the three that succeeded
./target/release/zr-break.exe a22                    # the near miss (fast, the sharpest result)
./target/release/zr-break.exe a21                    # the exhaustive hybrid, ~48s on 19 threads
GRIND_A5=3000 GRIND_A13=1000000 ./target/release/zr-break.exe a5 a13
```

| File | What it is |
|---|---|
| `src/forge.rs` | the independent prover: commitment key, transcript, both sub-arguments, cheat knobs, wire writer, and a local mirror of `verify` used only to report which check fired |
| `src/main.rs` | sections A0–A14 |
| `src/extra.rs` | A15–A18 |
| `src/extra2.rs` | A19–A20 |
| `src/extra3.rs` | A21, the exhaustive hybrid |
| `src/extra4.rs` | A22, the near miss |

Nothing under `p2p-poker` was modified. `ziffle` is a
plain `=0.1.0` dependency of the probe, unpatched and unvendored — the crate under attack
is the one the project will ship against.
