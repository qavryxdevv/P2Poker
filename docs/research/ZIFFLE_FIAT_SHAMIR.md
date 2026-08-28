# ziffle 0.1.0 — Fiat–Shamir transcript review

**Scope.** The Fiat–Shamir transcript only. Whether `MultiExpArg` and
`SingleValueProductArg` are *individually* sound as algebra is a separate review;
if either of them is broken, nothing below saves it. What this review answers is
narrower and prior: **does the transcript bind everything it must, and is the
fork legitimate?**

**Sources.**

- Code: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ziffle-0.1.0/src/lib.rs`
  (1779 lines; all line numbers below refer to it).
- Paper: Bayer & Groth, *Efficient Zero-Knowledge Argument for Correctness of a
  Shuffle*, EUROCRYPT 2012. I worked from **both** the 13-page proceedings
  version (Springer) **and** the 30-page full version (recovered from
  web.archive.org, since `www0.cs.ucl.ac.uk` is unreachable from here). §5.3 —
  the single value product argument that ziffle actually implements — appears
  only in the full version.
- Probe crate: `~/AppData/Local/Temp/claude/<session>/<session-id>/scratchpad/zr-fiatshamir/probe/`
  — an independent re-implementation of the transcript and of both
  sub-argument verifiers, used to confirm by execution (not by reading) what is
  absorbed and in what order.

**Bottom line.** The fork is sound, and I can say that with more confidence than
I expected to. The transcript binds every statement element the paper requires,
in the right order, with unambiguous framing, from the full state rather than a
prefix, under six pairwise-distinct domain tags. The Frozen-Heart-class bug is
not here. What *is* here is one hard interoperability defect (FS-2), one genuine
missing binding in a different proof (FS-3), and a set of integration
obligations the caller must discharge (FS-4, FS-5).

---

## 0. Verdict table

| ID | Finding | Class | Exploitable? |
|----|---------|-------|--------------|
| FS-1 | The transcript fork is sound | — | No — the paper explicitly permits it; argued and demonstrated below |
| FS-2 | Transcript framing uses `usize::to_be_bytes()`; 4 bytes on 32-bit, 8 on 64-bit | Interop, hard break | Not a forgery. **Every** cross-arch proof is rejected. Certain to bite a wasm client |
| FS-3 | `RevealTokenProof` absorbs `c1` but not `c2` — the proof is not bound to the card | Missing binding | **Yes, demonstrated** (probe E6a–E6d) — but only when every shuffler is malicious |
| FS-4 | `ShuffleProof` binds no prover identity and no round index | Integration obligation | Yes if the caller's `ctx` is weak; the fix is in the caller |
| FS-5 | No check that a re-mask actually masks (`r = 0` accepted) | Missing sanity check | **Yes, demonstrated** (probe E7) — same "all shufflers malicious" caveat |
| FS-6 | Challenges not rejected when zero; paper says `Z_q^*` | Spec deviation | No — 2⁻²⁵⁶ |
| FS-7 | 256-bit chained state ⇒ 128-bit collision resistance | — | No — matches secp256k1's level |
| FS-8 | arkworks' `expand_message_xmd` deviates from RFC 9380 in `Z_pad` length | Upstream non-conformance | No known attack; I could not construct one |
| FS-9 | Pedersen key `h`, `gs` are sampled by x-coordinate, so their dlogs are unknown | **Positive** | The one thing that would have been catastrophic; it is correct |

---

## 1. What the paper requires to be bound

From the shuffle argument (full version p. 8; proceedings p. 7), the interactive
protocol is:

| Round | Prover sends | Verifier replies | The challenge must be chosen **after** |
|-------|--------------|------------------|-----------------------------------------|
| 1 | `c_A` = com(π) | `x ← Z_q^*` | CRS (`pk`, `ck`), statement (`C`, `C′`), and `c_A` |
| 2 | `c_B` = com(x^{π(i)}) | `y, z ← Z_q^*` | everything above, plus `c_B` |
| 3 | *(two sub-arguments, in parallel)* | | |

and then, in parallel:

**Multi-exponentiation argument** (§4, full version p. 9). Statement:
`C′_1..C′_m`, `C`, `c_A`. With ziffle's `m = 1` the prover's first message is
`c_{A0}`, `c_{B0}`, `c_{B1}`, `E_0`, `E_1`, with `b_m = s_m = 0` (so
`c_{B1} = com(0;0)` is the identity and carries no prover choice) and
`τ_m = ρ`, and the verifier checks `E_m = C`. Challenge `x` follows all of
those. Response: `a, r, b, s, τ`.

**Single value product argument** (§5.3, full version p. 25). Statement:
`c_a`, `b`. First message: `c_d`, `c_δ`, `c_Δ`. Challenge `x`. Response:
`ã, b̃, r̃, s̃`.

So the complete list of things that must be absorbed before the challenge that
depends on them:

1. before `x`: `pk`, `C` (input deck), `C′` (output deck), `c_A`
2. before `y, z`: all of the above, plus `c_B`
3. before the multi-exp challenge: all of the above, plus `c_{A0}, c_{B0}, E_0, E_1`
4. before the product challenge: all of round 1–2, plus `c_d, c_δ, c_Δ`

`ck` is CRS, not a statement element chosen by anyone (see FS-9/FS-10).

## 2. What the code actually absorbs

`Transcript` is a SHA-256 hash *chain*: state is 32 bytes, and each `append`
starts a fresh `Sha256`, feeds it the previous 32-byte state, then the new data,
then finalises (lib.rs:222–229). A challenge is `expand_message_xmd(state, DST)`
via arkworks `DefaultFieldHasher<Sha256>` (lib.rs:245–247).

Writing `H(·)` for one link in the chain, the shuffle transcript is exactly:

```
ts0 = SHA256(ctx)                                              lib.rs:204
ts1 = H(ts0 ‖ len("apk")  ‖ "apk"  ‖ len ‖ apk)                lib.rs:1090
ts2 = H(ts1 ‖ len("prev") ‖ "prev" ‖ ∀i: i ‖ len ‖ C_i)        lib.rs:1091
ts3 = H(ts2 ‖ len("next") ‖ "next" ‖ ∀i: i ‖ len ‖ C'_i)       lib.rs:1092
ts4 = H(ts3 ‖ len("c_pi") ‖ "c_pi" ‖ len ‖ c_pi)               lib.rs:1093
      x     = H2F(ts4, "ziffle/BG12X/v1")                      lib.rs:1094
ts5 = H(ts4 ‖ len("c_xpi") ‖ "c_xpi" ‖ len ‖ c_xpi)            lib.rs:1099
      (y,z) = H2F(ts5, "ziffle/BG12YZ/v1")                     lib.rs:1100

  ─── FORK at ts5 (lib.rs:1133 "NOTE: The transcript is forked here.") ───

branch A (MultiExpArg, lib.rs:717–731)
  ts5 → +c_alpha → +c_beta → +ct_mxp0 → +ct_mxp1
      x_mexp = H2F(·, "ziffle/BG12MultiExpArgX/v1")

branch B (SingleValueProductArg, lib.rs:886–898)
  ts5 → +c_d → +c_sdelta → +c_cdelta
      x_svp  = H2F(·, "ziffle/BG12ProductArgX/v1")
```

**Diff against §1 — nothing is missing.**

| Required | Absorbed? | Where |
|---|---|---|
| `pk` (aggregate) | yes | ts1 |
| `C` (input deck, all N ciphertexts) | yes | ts2 |
| `C′` (output deck, all N ciphertexts) | yes | ts3 |
| `c_A` ≡ `c_pi` — before `x` | yes | ts4, and `x` derived from ts4 |
| `c_B` ≡ `c_xpi` — before `y,z` | yes | ts5 |
| multi-exp `c_{A0}` ≡ `c_alpha` | yes | branch A |
| multi-exp `c_{B0}` ≡ `c_beta` | yes | branch A |
| multi-exp `c_{B1}` | n/a — forced to `com(0;0)`, not transmitted, not prover-chosen |
| multi-exp `E_0` ≡ `ct_mxp0` | yes | branch A |
| multi-exp `E_1` ≡ `ct_mxp1` | yes | branch A |
| product `c_d`, `c_δ`, `c_Δ` | yes | branch B |
| `x`, `y`, `z` reaching the sub-arguments | yes, deterministically — both are functions of ts4 ⊂ ts5, which is the fork base |
| `ck` | **no** — see FS-9/FS-10; it is a fixed CRS, not adaptive |
| `N` | not explicitly — but implied by the byte length of ts2/ts3 |

The verifier recomputes `x`, `y`, `z` from the *proof's own* `c_pi` and `c_xpi`
(lib.rs:1181–1182), not from anything it was handed separately, so there is no
way to feed the verifier one commitment and the challenge for another.

**I did not take this from reading alone.** The probe re-implements the
transcript and both sub-argument verification equations from scratch, derives
`x, y, z, x_mexp, x_svp` itself, and checks all eight of ziffle's verification
equations against a real proof parsed out of its serialisation. All eight pass
(probe E1). Had I mis-read the order of a single `append`, a single label, a
single DST, or the fork topology, E1 would have failed. The serialised proof
layout is independently corroborated by arithmetic: 33·2 + (33+33+66+66+52·32+4·32)
+ (33·3+2·52·32+2·32) = **5547** bytes, exactly the `ShuffleProof<52>` size in
the crate's own docs table (lib.rs:113).

---

## FS-1 — The fork is sound

**Status: no defect.** This is the question the review was commissioned to
answer, so here is the answer in full.

### The paper authorises it

Immediately after specifying the shuffle argument, Bayer & Groth write that
"The two arguments can be run in parallel" (full version, p. 8), and add that
the multi-exponentiation argument may start as early as round 3, once `c_B`
exists. ziffle's fork is precisely that: both sub-arguments are seeded from
`ts5`, the state that exists exactly after `c_B` (`c_xpi`) has been absorbed.
The fork is not an improvisation; it is the paper's own composition.

### The adaptivity the fork invites — and why it does not bite

The task asked specifically: after cloning, can a prover choose one
sub-argument's inputs *after* seeing the other's challenge?

**Yes, it can, and I demonstrated it.** Probe E2a–E2d: changing the product
argument's `c_d` leaves `x_mexp` bit-identical, and changing the multi-exp's
`c_alpha` leaves `x_svp` bit-identical. The two branches are genuinely
independent. A malicious prover may therefore run branch B to completion, read
`x_svp`, and only then choose branch A's first message — an ordering that the
*interactive* protocol (where the verifier sends both challenges at once) does
not permit. So the Fiat–Shamir prover here is strictly more adaptive than the
interactive one.

That extra adaptivity buys nothing, for one reason: **neither sub-argument's
statement depends on the other's proof.** Concretely (lib.rs:704–712, 873–881):

- The multi-exp statement is `(ck, apk, prev, next, x, c_xpi)`.
- The product statement is `(ck, x, y, z, c_pi, c_xpi)`.

Every element of both is fixed at or before `ts5`, and both are recomputed by
the verifier from `ts5`-committed values. Probe E2e/E2f confirm the direction of
dependency is right: perturbing `c_pi` moves `x`, `y`, `z` and *both* branch
challenges; perturbing `c_xpi` leaves `x` alone (correct — `x` precedes it) but
moves `y`, `z` and both branch challenges.

Given that, soundness of each branch is the soundness of a Fiat–Shamir'd Σ
protocol whose first message is fixed before its own challenge. The knowledge
extractor for either branch rewinds only that branch's challenge, holding
everything else — including the other branch's entire proof — fixed; the other
branch is just auxiliary input to a prover the extractor already tolerates
arbitrary auxiliary input for. The shuffle's extractor then combines the two
extracted openings, which refer to the same `c_pi`/`c_xpi`, and Pedersen
binding (FS-9) makes those openings unique. Nothing in that argument needs the
challenges to be jointly derived.

The grinding cost is also unchanged in order. With `T` random-oracle queries a
cheating prover gets `T` attempts at each branch rather than `T` attempts at the
pair, giving `T·(ε_mexp + ε_prod)` instead of `T·max(...)` — the same union
bound either way, and both `ε` are `O(N/q)` with `q ≈ 2²⁵⁶`.

**Honest caveat.** That paragraph is my argument, not a cited theorem about this
specific instantiation. The general machinery (Fiat–Shamir for multi-round
public-coin arguments with witness-extended emulation, à la Attema–Fehr–Klooß)
applies to a tree of depth 3 with two independent leaves, which is what this is,
but nobody has written the reduction down for ziffle specifically. If you want a
belt-and-braces change, see the recommendation at the end of FS-1.

### The domain tags

All six challenge derivations in the crate, and their tags:

| Call site | Tag | Line |
|---|---|---|
| `OwnershipProof::challenge` | `ziffle/DLOG/v1` | 300, 306 |
| `RevealTokenProof::challenge` | `ziffle/DLEQ/v1` | 426, 442 |
| `ShuffleProof::challenge_x` | `ziffle/BG12X/v1` | 1079, 1094 |
| `ShuffleProof::challenge_yz` | `ziffle/BG12YZ/v1` | 1080, 1100 |
| `MultiExpArg::challenge_x` | `ziffle/BG12MultiExpArgX/v1` | 715, 729 |
| `SingleValueProductArg::challenge_x` | `ziffle/BG12ProductArgX/v1` | 884, 896 |

There is no branch, anywhere in the file, that derives a challenge without a
tag: `derive_challenge_scalars` (lib.rs:245) takes `dst` as a required argument
and there are exactly six call sites, all listed above. The six tags are
pairwise distinct. Probe E4 confirms that the *same* transcript state under
three different tags yields three unrelated scalars.

The separation is genuine at the arkworks level too: `DefaultFieldHasher::new`
builds an `ExpanderXmd` that computes RFC-9380 `DST_prime = DST ‖ I2OSP(len(DST),1)`
and feeds it into every block of `expand_message_xmd`
(`ark-ff-0.5.0/src/fields/field_hashers/expander/mod.rs:53–57, 100–125`). The
DST is length-framed, so no two tags can be confused by concatenation.

Note also that domain separation is *not the only* thing keeping the two
branches apart. Their post-fork chains differ at the very first link — branch A
appends label `"c_alpha"` (7 bytes) and branch B appends `"c_d"` (3 bytes) —
and they differ in depth (4 appends versus 3). Even with identical tags the
states would diverge. The separation is doubled.

### Recommendation (optional hardening, not a fix for a defect)

If you want to remove the extra adaptivity entirely at zero cost, absorb both
first messages into a single state before deriving either challenge — i.e.
compute `ts6 = ts5 + c_alpha + c_beta + ct_mxp0 + ct_mxp1 + c_d + c_sdelta +
c_cdelta` and derive both challenges from `ts6` under the two existing tags.
That restores exactly the interactive protocol's simultaneity, costs three
extra SHA-256 compressions, and makes the composition argument a one-liner
instead of the paragraph above. It is a wire-format break, so it belongs in a
fork of the crate, not a patch.

---

## FS-2 — The transcript is a different function on 32-bit targets

**Severity: high (interoperability). Not a soundness break.**

`Transcript` frames every field with `usize::to_be_bytes()`:

- `lib.rs:218` — `h.update(serialized_size.to_be_bytes());`
- `lib.rs:224` — `h.update(label.len().to_be_bytes());`
- `lib.rs:238` — `h.update(i.to_be_bytes());`

`usize::to_be_bytes()` returns `[u8; size_of::<usize>()]`: **8 bytes on
x86_64/aarch64, 4 bytes on wasm32/armv7/i686.** The absorbed byte string, hence
the state, hence every challenge, hence proof validity, is therefore a function
of the *target pointer width*. A proof produced by a browser client compiled to
`wasm32-unknown-unknown` is rejected by every 64-bit peer, and vice versa.

This is not hypothetical for this project. `ziffle` is `#![no_std]`
(lib.rs:116) and its README leads with that; the obvious reason to make a
mental-poker crate `no_std` is to ship it to wasm. `p2p-poker` has a GUI story;
the day any part of it runs in a browser, the table splits into two mutually
unverifiable halves.

The author was clearly aware of the width problem elsewhere — `usize_to_u64!`
(lib.rs:660–664) exists precisely to normalise `usize` to `u64` in the exponent
arithmetic — and just did not apply it in the transcript.

**Demonstration.** Probe E3 builds the shuffle transcript twice over identical
inputs, differing only in whether the length prefixes are `u64::to_be_bytes()`
or `u32::to_be_bytes()` — which is exactly what `usize::to_be_bytes()` compiles
to on the two target classes — and derives `x` from each:

```
x (usize = 8 bytes, x86_64) = 79962580772362969643464447915091989271899695752550059070740515457728281935778
x (usize = 4 bytes, wasm32) = 38522517836310535001386719601806074359955548383843974483909882867863973043289
```

The 8-byte value is bit-identical to the `x` ziffle itself used for that proof
(the probe asserts this), which is what makes the 4-byte value the wasm answer
rather than an artefact.

**Honest limit.** I could not run an actual `wasm32` build — the target is not
installed on this machine and I did not install one. The finding rests on the
`usize::to_be_bytes()` semantics plus E3, which together are conclusive, but it
has not been observed end to end on a real wasm binary. If you want that
confirmation it is a `rustup target add wasm32-unknown-unknown` plus a
`wasm-bindgen-test` away.

**Fix.** Change all three sites to `u64::try_from(..).unwrap().to_be_bytes()`
(or reuse `usize_to_u64!`). It is a wire-format change, so it must land before
any proof is persisted or exchanged.

---

## FS-3 — `RevealTokenProof` binds `c1` but not `c2`

**Severity: medium — high only under an all-malicious-shufflers assumption.
Exploitable, demonstrated.**

This is a missing-statement-element finding, which is exactly the class this
review was looking for; it just lives in the reveal proof rather than the
shuffle proof.

`RevealTokenProof::challenge` (lib.rs:428–444) absorbs `pk`, `share`, `c1`,
`t_g`, `t_c1`. `RevealTokenProof::verify` (lib.rs:475–500) takes a whole
`MaskedCard` but destructures it as `let MaskedCard((c1, _)) = card;`
(lib.rs:484) and throws `c2` away. A reveal-token proof is therefore bound to
`c1` alone, and is valid for **every** card in the deck that shares that `c1`.

Binding only `c1` would be adequate if `c1` uniquely identified a card. Nothing
enforces that. `remask_card` (lib.rs:1221–1230) computes
`c1 ← c1 + r·G` with `r` chosen entirely by the shuffler, and the BG12 proof
constrains `r` not at all — any `r` is a legal re-masking.

**Demonstration (probe E6).** The initial deck has `c1 = ∞` for every card
(lib.rs:1327), so the first shuffler can set `r_i = r_j` and collide two cards'
`c1` while their `c2` differ. I drove ziffle's *own* prover into that state by
handing `shuffle_initial_deck` an RNG that returns a constant byte pattern — a
real attacker simply writes `r_i = r_j` in their own prover; the constant RNG is
only a way to reach the state through the public API. Results:

- E6a — the resulting deck, in which all eight cards share one `c1`, **passes
  `verify_initial_shuffle`**. The shuffle argument is untroubled by it, as it
  should be: it is a correct permutation and a correct re-mask.
- E6b — `card0.c1 == card1.c1` while `card0.c2 != card1.c2`.
- E6c — a `RevealTokenProof` issued for `card0` is **accepted** for `card1`.
- E6d — end to end: the `AggregateRevealToken` gathered only for `card0`
  decrypts `card1` as well. The probe prints `card0 index = Some(1),
  card1 index via card0's token = Some(2)` — two different cards, one set of
  tokens.

**The attack this is.** Collide a card that the protocol will legitimately
reveal to everyone (a flop card, a burn card, a showdown card) with a card that
must stay secret (a victim's hole card). When the honest players hand over their
reveal tokens for the public card — which they are supposed to do — those same
tokens decrypt the victim's hole card, and every proof involved verifies. The
victims never consented to reveal the second card and have no way to notice.

**The limit, stated honestly.** This needs the collision to survive into the
final deck, and it does not survive an honest re-shuffle. Probe E6e confirms:
one honest `shuffle_deck` after the malicious one destroys the collision. A
later shuffler who wants to *create* a collision must solve
`(r_i − r_j)·G = c1_prev[π(j)] − c1_prev[π(i)]`, i.e. know a discrete log he
does not have — unless every earlier shuffler colluded with him. **So the attack
requires all shufflers to be malicious** (or that only one player shuffles). In
a game with at least one honest shuffler, it does not work. That is a real
mitigation, and it is also exactly the kind of mitigation you do not want to be
relying on silently.

**Fix.** Absorb the whole ciphertext. In `RevealTokenProof::challenge`, replace
the `c1` append with an append of `(c1, c2)`, and pass the full card through.
Two lines, no cost, and it makes the proof bind the card it names regardless of
what the shuffler did. Additionally, `Shuffle::reveal_card` (lib.rs:1618) accepts any aggregate token with any card and has no way to tell whether the
token was gathered for that card — the `Verified<…>` type discipline that
protects the shuffle path is absent here. Worth wrapping in the integration.

---

## FS-4 — Nothing in the shuffle transcript identifies the prover or the round

**Severity: medium. Integration obligation — the fix belongs in `p2p-poker`, not
in ziffle.**

`ShuffleProof::challenge_x` absorbs `apk`, `prev`, `next`, `c_pi`, `ctx`
(lib.rs:1089–1093). It does **not** absorb the public key of the player doing
the shuffling, nor a round/step index. The `Verified<MaskedDeck>` type chains
shuffles (`verify_shuffle` takes `prev: &Verified<MaskedDeck<N>>`), which stops
reordering within a linear chain, but:

- If two players are ever offered the *same* `prev` — a protocol fork, a retry
  after a timeout, a re-deal — player B can rebroadcast player A's `(next,
  proof)` verbatim and it verifies. B has "proved" a shuffle B did not perform
  and does not know the permutation of. In a protocol that infers "this player
  contributed entropy" from "this player produced a valid shuffle proof", that
  is an entropy-contribution forgery.
- `ctx` is the only session binding, and it is opaque to the crate.

**Requirements on the caller.** `ctx` must be a per-session, per-round,
per-prover value agreed before shuffling begins, and it must be derived from a
transcript that all players contributed to — at minimum
`H(game_id ‖ hand_number ‖ all players' verified pks ‖ shuffle_step_index ‖
shuffling_player_pk)`. If any player can choose `ctx` freely after seeing
`c_pi`, they can grind it; the grinding gain is negligible here (`T·O(N/q)` with
`q ≈ 2²⁵⁶`), so this is about replay and identity, not about challenge bias.

Note the same applies to `OwnershipProof` and `RevealTokenProof`, which also
take `ctx` and bind nothing else about the session.

---

## FS-5 — A "re-mask" with `r = 0` is accepted

**Severity: low–medium. Same threat model as FS-3. Demonstrated.**

Nothing in `verify` requires that a card's `c1` be different from the point at
infinity, or that the re-masking randomness be non-zero. A first shuffler who
uses `r_i = 0` for every card produces a deck in which every ciphertext is
`(∞, opencard_{π(i)})` — i.e. **the entire deck is in the clear**, readable by
anyone, with no reveal tokens needed — and `verify_initial_shuffle` accepts it.
Probe E7 confirms: `r == 0 deck accepted = true, every c1 == infinity = true`.

Consequences beyond the obvious: with `c1 = ∞` the DLEQ reveal-token proof
degenerates — `share = ∞`, `t_c1 = ∞`, and the second verification equation
(lib.rs:495) holds for *any* `z` — so the token proof collapses to a plain
Schnorr on `pk` and proves nothing about the decryption share.

Again this is destroyed by any honest subsequent shuffler, and again that is a
mitigation you should not be leaning on unknowingly.

**Fix (cheap, in the caller).** After every `verify_shuffle` / `verify_initial_shuffle`,
reject the deck unless all `c1` values are distinct and none is the identity.
That single check kills both FS-3's exploit path and this one, without touching
the crate. It is the highest-value two lines in this report.

---

## FS-6 — Challenges are not rejected when zero

The paper specifies `x, y, z ← Z_q^*`. `derive_challenge_scalars` can return
zero. If `x = 0` then `xpi[i] = 0` for all `i` and the argument degenerates.
Probability 2⁻²⁵⁶ per challenge, and a prover cannot steer it without ~2²⁵⁶ hash
queries. **Not exploitable.** Worth a one-line guard for spec fidelity, nothing
more.

## FS-7 — The chain compresses to 256 bits

The transcript state is a 32-byte SHA-256 digest re-hashed at each step, so the
transcript's collision resistance is ~128 bits, not 256. This matches
secp256k1's ~128-bit security level, so it is not the weak link. Noted for
completeness.

## FS-8 — arkworks' `expand_message_xmd` is not RFC-9380-conformant in `Z_pad`

`ExpanderXmd::expand` prepends `Z_pad` of `self.block_size` bytes
(`ark-ff-0.5.0/.../expander/mod.rs:110`), where `block_size` is set to
`len_per_base_elem` = 48 (`.../field_hashers/mod.rs:57`). RFC 9380 requires
`Z_pad = I2OSP(0, s_in_bytes)` with `s_in_bytes` the hash's **input block size**,
i.e. 64 for SHA-256. This is a known upstream deviation. `b_0` is finalised
before being reused, so the length-extension property `Z_pad` exists to defuse
is not reachable here.

**I could not turn this into an attack and I do not believe one exists**, but I
am flagging it because it means ziffle's challenges are not RFC-9380 values, so
any future cross-implementation of the verifier (a different language, an
on-chain verifier) must replicate arkworks' quirk rather than the RFC.

## FS-9 — The Pedersen key is genuinely nothing-up-my-sleeve *(positive finding)*

This is not strictly transcript work, but it is the single assumption the whole
transcript analysis rests on, it is derived from a public seed the way the
challenges are, and it would have been catastrophic if wrong — so I checked it.

`PedersonCommitKey::default` (lib.rs:156–166) derives `h` and every `gs[i]` as
`CurveProj::rand(&mut StdRng::from_seed(Sha256::digest(SEED)))`. If arkworks'
`rand` for a curve point were implemented as "sample a scalar `s`, return
`s·G`", then `dlog_G(h)` would be computable by anyone from the public seed
`b"PEDERSON-H-V1"`, Pedersen binding would evaporate, `c_pi` and `c_xpi` could
be opened to anything, and **every finding about the transcript would be
irrelevant because the proof system would be trivially forgeable.**

It is not. `ark-ec-0.5.0/src/models/short_weierstrass/group.rs:95–108`:

```rust
fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Projective<P> {
    loop {
        let x = P::BaseField::rand(rng);
        let greatest = rng.gen();
        if let Some(p) = Affine::get_point_from_x_unchecked(x, greatest) {
            return p.mul_by_cofactor_to_group();
        }
    }
}
```

It samples a random **x-coordinate** and decompresses. The discrete log relative
to `G` is unknown to everyone, including the crate author. Binding holds. This
is the correct construction and it is worth recording that it was checked.

## FS-10 — `ck` and `N` are not absorbed

`ck` never enters the transcript. That is acceptable here only because `ck` is a
compile-time constant: `Shuffle` exposes no constructor but `Default`
(lib.rs:1314–1321), and the key is derived from the two fixed seeds. **If a
future version ever lets the caller supply a commitment key, it must be absorbed
before `x`** — otherwise a prover could pick a key after seeing the statement,
which is a straightforward soundness break.

`N` is not absorbed explicitly, but `append_vec` (lib.rs:231–243) emits one
index-plus-length-prefixed record per element, so two decks of different sizes
produce different byte strings. Adequate.

## FS-11 — `Transcript::init` has no protocol tag

`Transcript::init(ctx) = SHA256(ctx)` (lib.rs:204–206) — no version string, no
protocol identifier. Cross-protocol confusion is prevented downstream by the
labels and the six DSTs, so this is cosmetic today. It becomes load-bearing the
moment a `v2` of any of these proofs exists: prefix the init with something like
`b"ziffle/v1"` so that a v1 and a v2 transcript over the same `ctx` cannot share
a state.

---

## 3. Framing and delimitation — checked, correct

The task asked specifically whether two different statements can produce the
same absorbed byte string. They cannot.

`append` (lib.rs:222–229) emits `prev_state ‖ len(label) ‖ label ‖ len(data) ‖ data`.
Both the label and the data carry an explicit length prefix, so the boundary
between them is never inferred from content. `update_with_serialized`
(lib.rs:208–220) takes the length from `compressed_size()` and writes exactly
that many bytes.

`append_vec` (lib.rs:231–243) emits `prev_state ‖ len(label) ‖ label ‖
[ i ‖ len(elem) ‖ elem ]*`. The element count is not written explicitly, which
looked worth checking, but it is not a gap: every record is
`8 + 8 + 66 = 82` fixed bytes for a ciphertext and carries its own index, so two
vectors yield the same string only if they have the same length and the same
elements in the same order.

Cross-`append` ambiguity is structurally impossible: each `append` **finalises**
a SHA-256 and the next one starts from the 32-byte digest, so data from
different rounds can never be re-split across a round boundary.

Every value ever appended is fixed-width for its type (compressed secp256k1
affine = 33 bytes including the point at infinity, which probe E7 exercised;
`Ciphertext` tuple = 66; scalars are not appended at all), so even the length
prefixes are constant. The `assert!` in `update_with_serialized` guarding the
256-byte buffer cannot fire for any current call site — the largest appended
value is 66 bytes — so it is not a remote panic vector.

## 4. The challenge is derived from the full transcript, not a prefix

`derive_challenge_scalars` (lib.rs:245–247) hashes `&self.0`, the complete
running state. Because `append` seeds each new hash with the previous digest,
that state transitively covers every byte ever absorbed. No call site derives a
challenge from a saved earlier state — the only place an earlier state is reused
is `ts5`, and it is reused *forward* into both branches, which is the fork
(FS-1), not a truncation. Probe E2g/E2h confirm the transitivity empirically:
swapping two ciphertexts in `next`, or changing `ctx`, moves `x`, `x_mexp` and
`x_svp` alike.

## 5. End-to-end tamper tests

Through the public API only (probe E5):

| Test | Result |
|---|---|
| honest second shuffle | accepted |
| same proof, different `ctx` | rejected |
| same proof, wrong `prev` deck | rejected |
| same proof, different `apk` | rejected |
| proof with a foreign product argument spliced into its bytes | rejected |

---

## 6. What I did not check

Stated plainly, because a confident summary that skipped these would be worse
than useless:

1. **The algebra of `MultiExpArg` and `SingleValueProductArg`.** Out of scope
   here by design. I read them closely enough to map every value onto the
   paper's `c_{A0}, c_{B0}, c_{B1}, E_0, E_1` and `c_d, c_δ, c_Δ` and to confirm
   the *transcript* covers them, but I did not verify a single exponent. If the
   companion review finds a broken exponent, FS-1's conclusion is vacuous.
2. **A real `wasm32` build** for FS-2. The target is not installed here. The
   finding is established from `usize::to_be_bytes()` semantics plus probe E3,
   which I consider conclusive, but it has not been observed on a wasm binary.
3. **A formal reduction for the forked Fiat–Shamir.** §FS-1 gives an argument,
   not a proof. I am confident in it — the paper authorises parallel
   composition and no statement crosses the fork — but it is reasoning, not a
   citation, and the optional hardening at the end of FS-1 removes the need for
   it at a cost of three hash compressions.
4. **RNG quality, zeroization, timing side channels, and the `no_std`/`alloc`
   story** (ark-ff's expander allocates a `Vec`, so this crate is `no_std` but
   not allocation-free despite its `no-std::no-alloc` crates.io category).
   Different review.
5. **Whether the soundness error of the composed argument is small enough for
   52 cards.** I asserted `O(N/q)` from the paper's structure without redoing
   the accounting.

## 7. Recommendations, in priority order

1. **Add the deck sanity check in `p2p-poker`** — after every shuffle
   verification, reject unless all `c1` are pairwise distinct and none is the
   identity. Closes FS-3's exploit path and FS-5 entirely, in two lines, without
   forking the crate. Do this one regardless of everything else.
2. **Fix FS-2 before any proof crosses a wire or is persisted.** Replace the
   three `usize::to_be_bytes()` with `u64`. This is a wire-format break, so it
   is nearly free now and expensive later.
3. **Fix FS-3 in the crate** (absorb `c2` in the DLEQ challenge) if you fork it;
   otherwise recommendation 1 covers you.
4. **Define `ctx` properly** (FS-4): session id, hand number, step index,
   shuffling player's pk, and a commitment to all players' verified public keys.
5. **Optional:** derive both sub-challenges from a joint state (end of FS-1) to
   remove the fork's extra adaptivity, and prefix `Transcript::init` with a
   protocol tag (FS-11). Both are wire-format breaks; bundle them with 2.

Items 2, 3 and 5 together mean that if you are going to fork `ziffle` at all —
and given "3 GitHub stars, 8 commits, no audit", you probably should — fork it
once, now, and change the wire format once.
