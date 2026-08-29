# ziffle 0.1.0 — hostile input, group membership, and the deserialisers

Reviewer: Phase-1 `ziffle-input` agent
Date: 2026-08-28
Subject: `ziffle` 0.1.0, `src/lib.rs` (1779 lines), unpacked at
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ziffle-0.1.0/`
Dependencies read as source: `ark-serialize`, `ark-ec`, `ark-ff`, `ark-secp256k1`, all 0.5.0,
same registry directory.

Scope of *this* review: what happens to attacker-supplied bytes. Point validation, scalar
validation, cofactor handling, deserialiser robustness, and reachable panics. The algebra of
`MultiExpArg` and `SingleValueProductArg` against the Bayer–Groth paper is a **different agent's
task** and is not assessed here, except where a degenerate *input* makes an algebraically correct
proof useless — §4.

**Environment.** Windows 10, x86_64-pc-windows-msvc, 24 cores, rustc 1.95.0, `--release`.
All probes in `…/scratchpad/zr-input/`, outside the repo. Nothing in
`p2p-poker` was modified.

Every claim below is marked **(measured)** — I compiled and ran code that produced the quoted
output — or **(source)** — I read the code and reasoned, without running it. Where I could not
settle a question I say so in §9.

---

## 1. Findings at a glance

| # | Finding | Where | Exploitable? |
|---|---|---|---|
| F1 | Degenerate re-masking (`ρ = 0`, or one shared `ρ`) produces an unmasked or fully-linked deck that `verify_shuffle` **accepts**. Demonstrated end to end. | `lib.rs:1173` `ShuffleProof::verify`, `lib.rs:1221` `remask_card` | **Yes, with a caveat.** Attack executed; a later honest shuffle repairs it. Fatal only if the integration layer gets the chain wrong. §4 |
| F2 | `pk = identity` passes `OwnershipProof::verify` with a proof anyone can write, and then contributes nothing to the aggregate key. | `lib.rs:349` | **Yes**, forged and verified. Direct impact low; it destroys the invariant `Verified<PublicKey> ⇒ peer knows a secret key`. §5 |
| F3 | `Validate::Yes` is **inert** for compressed secp256k1 points: 0 disagreements in 2 000 000 random inputs. It is the only defence for *uncompressed* points, where `Validate::No` accepted **100 %** of 2 000 000 random 65-byte strings as off-curve points. | `lib.rs:1649`, `lib.rs:1724`; `ark-ec/short_weierstrass/mod.rs:139` | Uncompressed + `Validate::No` is a loaded gun. Not reachable through the algebra by the naive route (§6), but the rule is: compressed only, never `_unchecked`. |
| F4 | The wire encoding is **not canonical**, badly: six bits per point are read then discarded, so a `MaskedDeck<52>` has ≥ 2⁶²⁴ byte encodings that all verify. Plus: trailing bytes ignored, and a quarter of all random 33-byte strings decode to the point at infinity. | `ark-ff/const_helpers.rs:156`; `ark-ec/short_weierstrass/mod.rs:145-152` | Message malleability. Breaks any dedup/signature/transcript that hashes wire bytes. §7 |
| F5 | The `assert!` in `Transcript::update_with_serialized` is **unreachable** by an adversary. | `lib.rs:211` | **No.** Argued statically and measured. §8 |
| F6 | The compile-time guard `const _N_GREATER_THAN_1: () = assert!(N > 1)` is **dead code**: `Shuffle::<0>` and `Shuffle::<1>` compile, then panic at runtime. | `lib.rs:1324` | Not remote — `N` is ours. A footgun, not a vulnerability. §8 |
| F7 | Rejecting one bogus 5 547-byte `ShuffleProof` costs 10–36 ms of CPU. | `lib.rs:1173` | **Yes**, as denial of service. 5.5 KB in, tens of ms of CPU out, at every seat. §10 |
| F8 | `mul_by_cofactor_to_group` is a no-op on secp256k1 and ziffle never calls it. | — | No issue. §3.4 |
| P1 | *Positive:* no length prefixes anywhere — every type is fixed-size. Unbounded allocation from hostile input is **structurally impossible**. | §3.5 | — |
| P2 | *Positive:* `MaskedCard`, `AggregatePublicKey`, `AggregateRevealToken` and `Verified<T>` have **no** `CanonicalDeserialize`. The type-state boundary cannot be crossed by parsing. | §3.6 | — |
| P3 | *Positive:* non-canonical scalars are rejected; 143 490 088 hostile parse-and-verify calls caused **0 panics and 0 forgeries**, with a flat heap. | §2, §3.3 | — |

---

## 2. Fuzzing: what was actually run

**`cargo-fuzz` could not be used.** It installed (`cargo-fuzz 0.13.2`), but it requires a nightly
toolchain and this machine has only `stable-x86_64-pc-windows-msvc`. `rustup toolchain install
nightly` failed at the network layer:

```
error: could not download file from 'https://static.rust-lang.org/dist/channel-rust-nightly.toml.sha256'
… tcp connect error … (os error 10013)
```

os error 10013 is a local firewall/permission block, not a transient failure. So there was no
coverage-guided fuzzing and **no coverage feedback at all** — this is the single biggest weakness
of this section and I am not going to dress it up. What follows is blind structured random input,
which finds shallow crashes and nothing deep.

**What I ran instead:** `src/bin/t3_fuzz.rs`, a structured random-input loop with a counting global
allocator and `catch_unwind` around every parse-and-verify.

* **Targets:** `ShuffleProof<52>`, `MaskedDeck<52>`, `RevealToken`, `OwnershipProof`,
  `RevealTokenProof`, `PublicKey`, `SecretKey`.
* **Every input tried in all four modes:** `Compress::{Yes,No}` × `Validate::{Yes,No}`.
* **Six input generators:** uniform random bytes at a random length; uniform random bytes at
  exactly the right length; 1–16 bit-flips of a valid encoding; byte-splice of two valid
  encodings; a tiling of "interesting" 32/33-byte units (identity, generator, ±generator, all-zero,
  all-`0xFF`, the illegal `(negative ∧ infinity)` flag combination, the bare infinity flag, a random
  point, a random scalar); and a valid encoding with whole units overwritten by interesting ones.
* **Anything that parsed was then fed to a verifier** (`verify_initial_shuffle`, `verify_shuffle`,
  `OwnershipProof::verify`, `RevealTokenProof::verify`, `MaskedCard::reveal_token`) — a parser that
  does not crash is not the whole requirement; the code that consumes the parsed value has to
  survive too.
* **The "FORGERIES" column counts real forgeries only** — the post-check requires the fuzzed value
  to *differ* from the honest one and still verify. This guard matters, and getting it wrong cost me
  an extra run; see below.

**Run 1:** 12 worker processes, distinct seeds 1–12, 1 800 s each, all 12 to completion.

```
12 workers x 1800s, 3 751 820 outer iterations, 105 050 960 deserialise(+verify) calls

target                    runs    accepted    rejected   panics  FORGERIES
MaskedDeck<52>        15007280       99535    14907745        0          0
OwnershipProof        15007280     5525066     9482214        0        166   <-- chased down below
PublicKey             15007280     6415904     8591376        0          0
RevealToken           15007280     6418582     8588698        0          0
RevealTokenProof      15007280     2541527    12465753        0          0
SecretKey             15007280    13570277     1437003        0          0
ShuffleProof<52>      15007280      437702    14569578        0          0

largest single input fed: 13 482 bytes
max peak live heap across workers: 51 862 bytes
stderr across all 12 workers: 0 bytes  (the harness prints to stderr on panic)
```

**The 166 had to be chased down, and they are not forgeries.** `OwnershipProof` was the one target
where I had not yet added the differs-from-honest guard, so an input that reproduced the honest
65-byte encoding exactly would be counted. The bit-flip generator can do that by flipping the same
bit twice. **Run 2** added the guard, plus an explicit counter for how many *accepted* inputs
differed from the honest encoding — 12 workers x 600 s:

```
12 workers x 600s, 38 439 128 deserialise(+verify) calls

target                    runs    accepted    rejected   panics  FORGERIES
MaskedDeck<52>         5491304       36237     5455067        0          0
OwnershipProof         5491304     2022108     3469196        0          0
PublicKey              5491304     2347675     3143629        0          0
RevealToken            5491304     2348071     3143233        0          0
RevealTokenProof       5491304      929095     4562209        0          0
SecretKey              5491304     4964760      526544        0          0
ShuffleProof<52>       5491304      160062     5331242        0          0

OwnershipProof: accepted 2 022 108, of which 2 022 052 differed from the honest encoding
             => 56 inputs were byte-identical to the honest proof, and 0 forgeries
```

56 in 5 491 304 is a rate of 1.0e-5; scaled to run 1's 15 007 280 `OwnershipProof` runs that
predicts about 154, against the 166 observed. **The 166 were the fuzzer regenerating the honest
proof byte for byte.** I am recording the false alarm rather than quietly deleting it, because the
same trap caught me once before in this session — an unguarded `PublicKey` post-check reported 161
"verified" keys in an early smoke run, for exactly the same reason. If you re-run this harness,
check the guard before you believe a non-zero column.

**Combined result over both runs: 143 490 088 hostile parse-and-verify calls, zero panics, zero
forgeries, and a flat heap.** Peak live heap never exceeded **51 862 bytes**, against a largest
single input of **13 482 bytes**.

Against `SPEC_CS.md` §27, on the evidence available:

| §27 requirement | Verdict |
|---|---|
| must not crash | Held over 143 490 088 hostile parse-and-verify calls. **(measured)** |
| must not allocate unbounded memory | Structurally impossible — no length prefixes, all types fixed-size. **(source, §3.5)** and peak heap flat **(measured)** |
| must not execute code | ziffle contains **zero** `unsafe` (`grep -c unsafe src/*.rs` → 0). `ark-serialize` is `#![forbid(unsafe_code)]`. `ark-ff` has 13 `unsafe` sites; exactly one is on the deserialisation path — `SerBuffer::as_slice` (`const_helpers.rs:124-127`) reinterprets the struct as `8N+1` bytes, which is sound because the struct is `#[repr(C, align(1))]`. The rest are the x86-64 asm backend, not enabled in this build. **(source)** |
| must not read outside the buffer | All reads go through `ark_std::io::Read for &[u8]`, which returns `UnexpectedEof`. All 5 547 proper prefixes of a valid 5 547-byte proof were rejected, and no run panicked. **(measured)** |
| must not bypass schema validation | Partially. See F3 and F4 — the *encoding* is non-canonical and `Validate` is inert on the compressed path. |
| maximum sizes for all messages and collections | ziffle gives them for free: every type has one exact size. §3.5 |

Coverage caveat, stated plainly: 143 million blind inputs with no coverage feedback is still only a
smoke test — volume is not depth. It rules out the obvious crash; it says nothing about a deep state
a coverage-guided fuzzer would have reached. If a nightly toolchain becomes reachable, `cargo-fuzz` on these same seven targets is
still worth a day.

---

## 3. What the deserialisers actually do

### 3.1 ziffle delegates everything

ziffle writes no parsing logic of its own. Two macros generate all of it:

* `impl_valid_and_serde_unit!` (`lib.rs:1627-1685`) — for newtypes. Its whole deserialiser is
  `lib.rs:1649-1655`:
  ```rust
  fn deserialize_with_mode<R>(reader: R, compress: Compress, validate: Validate) -> Result<Self, _> {
      ark_serialize::CanonicalDeserialize::deserialize_with_mode(reader, compress, validate).map(Self)
  }
  ```
  Applied to `PublicKey`, `SecretKey`, `PedersonCommitment`, `RevealToken`, `MaskedDeck<N>`
  (`lib.rs:1687-1691`).
* `impl_valid_and_deser!` (`lib.rs:1698-1763`) — for structs; deserialises each field in
  declaration order, passing `compress` and `validate` straight through (`lib.rs:1724-1735`).
  Applied to `MultiExpArg<N>`, `SingleValueProductArg<N>`, `ShuffleProof<N>`, `RevealTokenProof`,
  `OwnershipProof` (`lib.rs:1766-1776`).

So **every validation decision is the caller's**, made by choosing `deserialize_compressed`
(`Compress::Yes, Validate::Yes`) versus `deserialize_compressed_unchecked` (`Validate::No`) versus
the uncompressed pair. ziffle neither documents this nor picks a default. Our layer must.

### 3.2 What `Validate::Yes` buys — and does not (F3)

`Affine<P>::check()` is `is_on_curve() && is_in_correct_subgroup_assuming_on_curve()`
(`ark-ec/src/models/short_weierstrass/affine.rs:372-378`). On secp256k1:

* `COFACTOR = &[0x1]` (`ark-secp256k1/src/curves/mod.rs:23`), so `cofactor_is_one()` is true
  (`ark-ec/src/models/mod.rs:29`) and `is_in_correct_subgroup_assuming_on_curve` returns `true`
  **unconditionally** (`ark-ec/src/models/short_weierstrass/mod.rs:74-80`). This is *correct*, not
  a bug: with cofactor 1, every on-curve point except infinity generates the prime-order group, and
  infinity is the group's own neutral element. There is no small-subgroup confinement attack to
  defend against here.
* `is_on_curve()` is therefore the only real content of `check()`.

Now the compressed path (`ark-ec/src/models/short_weierstrass/mod.rs:139-181`): it reads `x`, then
calls `get_ys_from_x_unchecked(x)` and errors with `InvalidData` if `x³ + 7` is not a square. The
recovered point is **on the curve by construction**. `check()` can only re-confirm it.

Measured, `src/bin/t9_diff.rs`:

```
compressed 33B x2000000:    accepted Validate::Yes=1000794, Validate::No=1000794, DISAGREEMENTS=0
uncompressed 65B x2000000:  accepted Validate::Yes=0,       Validate::No=2000000, DISAGREEMENTS=2000000
MaskedDeck<52> bitflips x20000: accepted Validate::Yes=119, Validate::No=119,     DISAGREEMENTS=0
```

Read that second line carefully. With the two flag bits forced to a legal combination, **every
single one of two million random 65-byte strings is accepted by `Validate::No` as a `PublicKey`**,
and every one of them is off the curve. The uncompressed path does
`Affine::new_unchecked(x, y)` and only calls `check()` when `Validate::Yes`
(`ark-ec/src/models/short_weierstrass/mod.rs:174-180`).

Why an off-curve point is dangerous here specifically: `MaskedCard::reveal_token`
(`lib.rs:602-615`) computes `share = c1 · sk`. For a short-Weierstrass curve with `a = 0` the
arkworks addition formulas never touch `b`, so scalar multiplication of an off-curve point
`(x, y)` runs happily in the group of the *twist* `y² = x³ + b'` with `b' = y² − x³`. If that group
has small-order subgroups, `sk · c1` leaks `sk mod n` — the textbook invalid-curve attack, repeated
and CRT'd into full key recovery. `reveal_token` is the **only** place in ziffle where a secret key
multiplies a peer-supplied point (checked: `OwnershipProof::new` uses only `GENERATOR`;
`RevealTokenProof::new` multiplies `c1` by a fresh nonce, not by `sk`). §6 covers whether an
attacker can actually reach it.

**Rule for our layer, non-negotiable:** compressed encoding only, `Validate::Yes` always, and no
`*_unchecked` deserialiser anywhere in the codebase — enforce it with a lint or a grep in CI. On the
compressed path `Validate::Yes` costs nothing measurable (§10), so there is no reason to skip it,
and if the group is ever swapped for one with cofactor > 1 the flag stops being inert.

### 3.3 Scalars (P3)

`Fp::deserialize_with_mode` **ignores `validate` entirely** and `Fp::check()` is `Ok(())`
(`ark-ff/src/fields/models/fp/mod.rs:630-644`). Canonicality is enforced elsewhere: the bytes go to
`Self::from_bigint(self_integer)`, which returns `None` — and hence `SerializationError::InvalidData` —
for any value ≥ the modulus (`ark-ff/.../fp/mod.rs:605-628`). Measured, `src/bin/t2_validate.rs`:

```
SecretKey = Fr::MODULUS exactly: accepted=false
SecretKey = 0xFF*32:             accepted=false
SecretKey = Fr::MODULUS-1:       accepted=true
SecretKey = 0:                   accepted=true
```

So scalars are reduced-or-rejected, never silently wrapped. Good.

**Zero is accepted everywhere**, and ziffle rejects it nowhere. Where does that matter?

* `SecretKey = 0` → `pk = identity` → F2, §5.
* `z = 0` in an `OwnershipProof` or `RevealTokenProof`: harmless. The verification equation still
  binds `z` to the challenge, and `z = 0` is a measure-zero honest output, not a free pass.
* `ρ_i = 0` in a shuffle: **this is F1**, and it is the one that matters. §4.
* `o_alpha`, `a_tilde`, `b_tilde` containing zeros: these are prover responses checked against
  commitments; a zero there has no special power. (The algebra is the other agent's call.)

### 3.4 `mul_by_cofactor_to_group` (F8)

Confirmed a no-op, three ways.

* **(source)** `mul_by_cofactor_to_group` is `P::mul_affine(self, Self::Config::COFACTOR)`
  (`ark-ec/src/models/short_weierstrass/affine.rs:242-244`) and secp256k1's `COFACTOR` is `&[0x1]`.
* **(measured)** `src/bin/t2_validate.rs`:
  ```
  mul_by_cofactor_to_group is identity map: true
  clear_cofactor is identity map: true
  ```
* **(source)** `grep -n "mul_by_cofactor" ziffle-0.1.0/src/lib.rs` → **no matches.** ziffle never
  calls it and nothing in ziffle depends on it doing anything.

It *is* reached indirectly, once, and it is worth knowing where: `Projective::rand`
(`ark-ec/src/models/short_weierstrass/group.rs:95-108`) rejection-samples an `x`, lifts it to a
point, and returns `p.mul_by_cofactor_to_group()`. ziffle calls `CurveProj::rand` exactly twice, in
`PedersonCommitKey::default()` (`lib.rs:153-162`), to derive the Pedersen bases `h` and `gs` from
`SHA-256("PEDERSON-H-V1")` and `SHA-256("PEDERSON-VECTOR-G-V1")`. Because the cofactor is 1 this is
a pure "hash to an x, lift" nothing-up-my-sleeve construction, so `h` and the `gs` have unknown
discrete log with respect to `G` and Pedersen binding is not obviously broken. Whether that is
*enough* is a setup question for the algebra reviewer, not for me. One non-security note in
passing: `StdRng` is ChaCha12 and rand explicitly does not guarantee its stream across major
versions, so a future ziffle that bumps `rand` would silently change the commitment key and split
the network. Pin it.

### 3.5 No unbounded allocation, structurally (P1)

There is **no length prefix anywhere in ziffle's wire format.** Every serialised type has one exact
size fixed at compile time by `N`:

```
PublicKey          compressed 33     uncompressed 65
SecretKey                     32                  32
OwnershipProof                65                  97
MaskedDeck<52>              3432                6760
ShuffleProof<52>            5547                5899
RevealToken                   33                  65
RevealTokenProof              98                 162
```
**(measured, `src/bin/t1_sizes.rs`)** — the compressed column reproduces the crate's own doc table
exactly.

`[T; N]::deserialize_with_mode` (`ark-serialize/src/impls.rs:455-473`) fills a stack
`ArrayVec<T, N>` — `N` iterations, no heap growth driven by input. Nothing in ziffle deserialises a
`Vec`. An attacker therefore cannot make a peer allocate: the peak live heap over the whole fuzz run
was flat (§2). §27's "define maximum sizes for all messages and collections" is satisfied by
construction — but our framing layer must still *enforce* these exact lengths before handing bytes
to ziffle, because of F4.

### 3.6 The type-state boundary holds (P2)

`MaskedCard`, `AggregatePublicKey`, `AggregateRevealToken` and `Verified<T>` deliberately have no
`CanonicalDeserialize`. `AggregatePublicKey` gets serialize-only, with the author's comment at
`lib.rs:1693`: *"only implement serialize so it can only be constructed from verified public keys"*.
Verified by negative compile test (`neg/neg.rs`):

```
error[E0599]: no function or associated item named `deserialize_compressed` found for struct `ziffle::MaskedCard`
error[E0599]: … for struct `ziffle::AggregateRevealToken`
error[E0599]: … for struct `ziffle::AggregatePublicKey`
error[E0599]: … for struct `ziffle::Verified<T>`
```

This is the strongest thing in the crate. It means the only route to a `MaskedCard` — and hence to
`reveal_token`, the one primitive that multiplies our secret key by a peer's point — is through a
`Verified<MaskedDeck<N>>`, which only `verify_initial_shuffle`/`verify_shuffle` can mint. Keep it:
do not add a `Deserialize` for any of these in a vendored fork, and do not add a `From` that
fabricates a `Verified`.

---

## 4. F1 — degenerate re-masking passes verification

### What the code does

`remask_card` (`lib.rs:1221-1230`) is the honest re-masking step:

```rust
let r = Scalar::rand(rng);
let c1 = c1 + GENERATOR * r;
let c2 = c2 + (pk * r);
```

and `shuffle_remask_prove` (`lib.rs:1232`) calls it once per card with a fresh `r`. `ShuffleProof`
then proves knowledge of `(π, ρ)` with `C'ᵢ = C_{π(i)} · E(1; ρᵢ)`.

### What the paper requires

Exactly that, and no more. Bayer–Groth's statement is `∃ π ∈ Σ_N, ρ ∈ Z_q^N` such that
`C'ᵢ = C_{π(i)} E(1; ρᵢ)`. **It says nothing about `ρᵢ` being non-zero, or distinct, or random.**

So this is **not a deviation from the paper.** ziffle implements the paper's statement. The gap is
between what the Bayer–Groth argument proves and what a poker deck needs, and ziffle's API neither
closes it nor warns about it. An algebra reviewer checking `MultiExpArg` line by line against the
paper would correctly find nothing here.

### Why it matters

A malicious prover writes their own prover — ziffle's fields are private, but the crate is
open source, `ShuffleProof` implements `CanonicalDeserialize`, and the algebra is 200 lines. I did
exactly that: `src/bin/t4_degenerate.rs` is a line-for-line port of `ShuffleProof::new` with `ρ`
supplied by the caller instead of sampled. The port's fidelity is proven by its own sanity line —
an honest proof from my port is accepted by *ziffle's* verifier:

```
[SANITY] ported prover, honest random rho -> ziffle verify_initial_shuffle: true
```

Then, all against `ziffle::Shuffle::<52>::verify_initial_shuffle` **(measured)**:

```
[ATTACK A] rho == 0 for all 52 cards -> verify_initial_shuffle accepts: true
[ATTACK A] cards recovered with NO secret keys at all: [19, 20, 21, 22, 23, 24, 25, 26, 27, 28]
[ATTACK A] all 52 recovered: true

[ATTACK B] identical rho for all 52 cards -> verify_initial_shuffle accepts: true
[ATTACK B] all 52 c1 values identical: true
[ATTACK B] card 0 opened normally -> index Some(31)
[ATTACK B] cards opened with card 0's aggregate token: 52/52

[ATTACK C] rho[3] == 0, others honest -> accepts: true
[ATTACK C] c1 of card 3 is the identity: true
[ATTACK C] card 3 readable with no keys: index Some(8)
```

* **A** — `ρ = 0` everywhere. Every `c₁` is the point at infinity, so the aggregate reveal token is
  also infinity and `c₂` *is* the plaintext card. All 52 cards read with no key material at all.
* **B** — one shared `ρ`. All 52 `c₁` are the same point, so **one** legitimately-obtained aggregate
  reveal token opens the entire deck. Card 0 was opened by the book, with real verified tokens from
  both players; that same token then opened all 52.
* **C** — the surgical version: `ρ₃ = 0`, the other 51 honest. One card is public, the deck looks
  normal.

Why the attacker can do this only from the *initial* deck: `Shuffle::initial_deck()`
(`lib.rs:1326-1328`) is `(identity, open_deck[i])`, i.e. every `c₁` has known discrete log zero.
That is what lets the first shuffler steer `c₁` to a chosen value. A later shuffler faces `c₁`
values with unknown discrete log and cannot force infinity or collisions.

### How far the shuffle chain saves us

`src/bin/t6_chain.rs` **(measured)**:

```
[chain] malicious FIRST shuffle (rho=0) accepted; c1 all identity: true
[chain] honest SECOND shuffle verifies: true
[chain] after the honest shuffle: any c1 identity = false, distinct c1 = 52/52
[chain] malicious LAST shuffle (rho=0, i.e. NO re-masking) accepted: true
[chain]   its c1 set is exactly the previous deck's c1 set: true
```

So an honest re-mask afterwards **does** repair A, B and C, and the classical Barnett–Smart
"one honest shuffler suffices" theorem survives. I want to be careful not to oversell the last
line: a null re-mask by the *final* shuffler lets whoever knew the previous ordering read the final
permutation off by matching ciphertexts — but knowing the previous ordering already requires the
whole prefix of the chain to be colluding, so this degrades an n-shuffle chain to n−1 rather than
adding a new attack. It does, however, mean an honest client cannot tell that a peer's "shuffle"
was a no-op.

### Verdict

**Exploitable, conditionally.** The condition is entirely in the integration layer we have not
written yet. It becomes a total loss of hole-card privacy — with every proof verifying — if any of
these is true:

1. fewer than every seated player shuffles (e.g. "only the button shuffles" as an optimisation) —
   with a single shuffler, attack A hands the whole deck to every observer;
2. cards are dealt or revealed from an intermediate deck rather than the final one;
3. the chain can be truncated because a peer times out or disconnects, so that **no honest shuffle
   follows the attacker's**. This is the one to worry about: disconnect handling is exactly where a
   chain gets shortened, and the attacker only has to be the last one standing;
4. a shuffle can be replayed or reordered, letting the attacker's degenerate deck be re-presented
   as the final one.

### The fix, and it is cheap

Do not rely on chain ordering for this. Before accepting any `next` deck, check three structural
properties. All three are outside ziffle's statement and cost **0.019 ms for 52 cards**
**(measured, `src/bin/t8_cost.rs`)** against 10–36 ms for the proof verification itself:

1. no `next[i].c₁` is the point at infinity — kills A and C;
2. the 52 `next[i].c₁` are pairwise distinct — kills B;
3. `{next[i].c₁} ∩ {prev[j].c₁} = ∅` — kills the null re-mask, and on the initial deck (where every
   `prev.c₁` is infinity) it subsumes check 1.

Check 3 is exactly "every `ρᵢ ≠ 0`", expressed in points instead of scalars. Together they make the
whole class impossible regardless of where the attacker sits in the chain, which is worth doing
precisely because it takes a subtle ordering dependency out of the security argument.

---

## 5. F2 — the identity public key

### What the code does

`OwnershipProof::verify` (`lib.rs:349-355`):

```rust
let e = Self::challenge(&pk, &self.a, ctx);
let lhs = (GENERATOR.into_group() * self.z).into_affine();
let rhs = (self.a.into_group() + (pk.0 * e)).into_affine();
(lhs == rhs).then_some(Verified(pk))
```

With `pk = O`, the term `pk·e` vanishes and the equation collapses to `z·G == a`. Anyone can
satisfy that: pick any `z`, set `a = z·G`. No secret is involved. `PublicKey` deserialises the
identity happily in **both** validate modes, because the compressed infinity branch returns early
without calling `check()` (`ark-ec/src/models/short_weierstrass/mod.rs:145-152, 173-180`).

Measured, `src/bin/t2_validate.rs` and `src/bin/t5_edges.rs`:

```
PublicKey = identity (compressed, Validate::Yes): accepted=true
PublicKey = identity (compressed, Validate::No):  accepted=true
FORGED OwnershipProof for pk=IDENTITY verifies: true
sk=0 => pk=identity, forged OwnershipProof accepted: true
  and its RevealTokenProof for a real card verifies: true
  that reveal token is the identity point: true
  aggregate key with the identity player == aggregate without them: true
```

The last line is the interesting one: a player who joins with `pk = O` is **silently absent from
the aggregate key**, and every honest peer's verification passes at every step.

### What is required

`AggregatePublicKey::new` (`lib.rs:408`) takes `&[Verified<PublicKey>]`, and the whole point of
requiring `Verified` is proof-of-possession — the standard defence against a rogue-key attack on
`apk = Σ pkᵢ`. The identity is the one key for which that proof is free.

### Is it exploitable?

**The forgery is real and I ran it.** The direct confidentiality impact is small, and I would rather
say that than inflate it: with `pk = O` the attacker contributes nothing to `apk`, so `apk` is the
sum over the honest players' keys and the attacker still cannot decrypt anything. Their reveal token
is `O`, which verifies and is also the honest value, so they cannot block a reveal either. A rogue
key `pk = X − Σ pkᵢ` with a useful `X` is still blocked, because the forgery works only when `pk·e`
vanishes and the identity is the only such point.

What it *does* destroy is the invariant `Verified<PublicKey> ⇒ this peer knows a secret key`. Any
protocol logic we build on that — accountability ("this peer must produce a token or be blamed"),
seat identity, key-derived commitments, an n-of-n threshold count — is weakened by a participant
who provably knows nothing. And `apk = O` (the whole deck in plaintext) is one all-identity table
away.

**Fix:** reject `pk == identity` at parse time, in our wrapper, before `OwnershipProof::verify` is
ever called. One comparison. Also reject a duplicate `pk` across seats while you are there — ziffle
does not check that either, and `AggregatePublicKey::new` is happily order- and duplicate-blind.

---

## 6. Can an off-curve point reach `reveal_token`?

This is the question F3 raises, and I tried to settle it rather than leave it hanging.

The route would be: peer sends a `MaskedDeck` containing an off-curve `c₁`, our side parses it
uncompressed with `Validate::No`, `verify_shuffle` accepts, `get(i)` yields a `MaskedCard`, and
`reveal_token` computes `sk · c₁` on the twist.

`src/bin/t10_offcurve.rs` builds exactly that: an honest-shaped shuffle of the initial deck with
`next[7].c₁` replaced by the off-curve point `(1, 1)`, proved with the ported prover. Result
**(measured)**:

```
crafted point is_on_curve = false
forged proof parsed (uncompressed, Validate::No)
MaskedDeck with an off-curve c1 parses (uncompressed, Validate::No): true
  same bytes with Validate::Yes: false
verify_initial_shuffle on the off-curve deck ACCEPTS: false
[control] same construction, all points on curve, accepts: true
```

Two things worth noting. First, the forged proof itself would not even round-trip through the
*compressed* encoding — mixing points from two curves produces coordinates that are not valid
compressed `x` values — so this attack is confined to the uncompressed path from end to end.
Second, what rejected the deck was the algebra, not `Validate`: the multi-exponentiation identity
relies on the homomorphism `E(1;ρ)`, which does not hold once a point from a different curve is in
the sum.

**This is a negative result, not a proof.** It shows the naive smuggle fails. It does not show that
no adaptive attacker can construct a `(deck, proof)` pair containing off-curve points that verifies
— for instance by putting *every* point on the same twist, which I did not attempt. Given that the
compressed path makes the whole question moot, I stopped here rather than spend the day on it, and
the mitigation is the same either way: **compressed only, `Validate::Yes`, no `_unchecked`.**

---

## 7. F4 — the encoding is not canonical

This is worse than I first assumed, and the reason is one line in `ark-ff`. Three independent
sources of malleability, all **(measured)**.

### 7.1 Six bits per point are read, cleared, and thrown away

secp256k1's base field is 256 bits, so a compressed point needs a 33rd byte purely to carry the two
`SWFlags` bits. `Fp::deserialize_with_flags` (`ark-ff/src/fields/models/fp/mod.rs:606-627`) reads
all 33 bytes into a `SerBuffer<4>`, strips the flag bits from byte 32, and then calls
`to_bigint()` — which is (`ark-ff/src/const_helpers.rs:156-164`):

```rust
pub(super) fn to_bigint(self) -> BigInt<N> {
    let mut self_integer = BigInt::from(0u64);
    self_integer.0.iter_mut().zip(self.buffers)
        .for_each(|(other, this)| *other = u64::from_le_bytes(this));
    self_integer
}
```

`SerBuffer<N>` is `{ buffers: [[u8; 8]; N], last: u8 }` (`const_helpers.rs:89-92`). `to_bigint`
iterates `buffers` only. **`last` — byte 32 — is never used.** Its two flag bits were consumed; its
other six bits are silently discarded.

So **every compressed point has 64 valid encodings**, and this is not a corner case, it is every
point on every message. `src/bin/t11_malleable.rs`:

```
canonical encoding, byte 32 = 0x80
PublicKey, low 6 bits of byte 32 varied: 64/64 accepted, 64/64 decode to the SAME point

MaskedDeck<52> with all 104 flag bytes dirtied: accepted=true
  and it is == the original deck: true
  re-serialising gives back the CANONICAL bytes: true
  the mutated bytes differ from the canonical ones: true
  and the mutated deck still passes verify_initial_shuffle: true

ShuffleProof<52> is 5547 bytes = 11 points x 33 + 162 scalars x 32 = 5547
ShuffleProof with all 11 flag bytes dirtied: accepted, == original: true
  bytes differ from canonical: true
  still verifies: true
```

A `MaskedDeck<52>` carries 104 points, so it has at least 64¹⁰⁴ ≈ 2⁶²⁴ distinct byte encodings, all
parsing to the identical deck and all passing verification. A `ShuffleProof<52>` carries 11 points,
so at least 64¹¹ ≈ 2⁶⁶.

### 7.2 The infinity flag discards the x-coordinate

`ark-ec/src/models/short_weierstrass/mod.rs:149-152`:

```rust
SWFlags::PointAtInfinity => (
    Affine::<Self>::identity().x,
    Affine::<Self>::identity().y,
    flags,
),
```

The `x` that was just parsed is thrown away, and `check()` is never reached because the function
returns from the `flags.is_infinity()` branch above it. So every canonical `Fq` element paired with
the infinity flag decodes to the same point:

```
8 DIFFERENT 33-byte strings with the infinity flag: 8 accepted, decoding to 1 distinct point(s)
```

Combining 7.1 and 7.2 gives the arithmetic that a naive parser should know about: **a uniformly
random 33-byte string is a valid `PublicKey` half the time, and is the point at infinity a quarter
of the time.** Measured over a million inputs:

```
random 33B x1000000: accepted=499622 (49.96%), of which the identity=249854 (24.99% of all inputs)
```

This exactly matches the model — flags `(1,1)` rejected (¼), flags `(0,1)` → infinity, accepted
unconditionally (¼), the other half accepted iff `x³+7` is a square (½ × ½). Read next to F2, it
says that one random byte in four hands an attacker a `PublicKey` that will pass an ownership proof
they can write themselves.

### 7.3 Trailing bytes are ignored

`deserialize_*` reads exactly as many bytes as the type needs from the slice and never checks EOF.

```
ShuffleProof<52> + 1000 trailing bytes: accepted = true
PublicKey        + 4096 trailing bytes: accepted = true
```

Truncation, by contrast, is clean: **all 5 547 proper prefixes of a valid 5 547-byte `ShuffleProof`
were rejected.**

**Why this matters for us.** None of this breaks ziffle. It breaks anything *we* build that assumes
"same object ⟺ same bytes":

* a hash-chained game transcript that hashes received bytes rather than re-serialised values;
* message deduplication or replay detection keyed on a message digest;
* a signature over the wire bytes — an attacker can re-pad or re-encode a signed-looking message
  into a different byte string that parses identically;
* GossipSub message-id computation.

**Fix:** in our framing layer, (a) require the exact expected length for every message type before
parsing — the table in §3.5 gives them — and (b) canonicalise by re-serialising the parsed value and
comparing to the input, rejecting on mismatch. Then hash and sign the canonical form only.

---

## 8. F5, F6 — the panic paths

The research note asked specifically about the `assert!` in `Transcript`. It is at `lib.rs:208-219`:

```rust
const SERIALIZE_BUFFER_SIZE: usize = 256;

fn update_with_serialized<T: CanonicalSerialize>(h: &mut Sha256, label: &str, t: &T) {
    let mut serialize_buffer = [0u8; Self::SERIALIZE_BUFFER_SIZE];
    let serialized_size = t.compressed_size();
    assert!(
        serialized_size <= Self::SERIALIZE_BUFFER_SIZE,
        "serialize buffer too small to serialize {label}: {serialized_size} < {}",
        Self::SERIALIZE_BUFFER_SIZE
    );
    t.serialize_compressed(&mut serialize_buffer[..serialized_size])
        .expect("infallible serialization");
```

**An adversary cannot reach it.** `Transcript` is private, and every call site appends one of
exactly two shapes: a single `CurveAffine` (33 bytes) or a `Ciphertext` tuple (66 bytes). The size
is a property of the *type*, not of the value — the identity serialises to 33 bytes like any other
point — and `N` never enters, because `append_vec` (`lib.rs:231-242`) serialises element by element
rather than the array as a whole. Measured, `src/bin/t5_edges.rs`:

```
compressed_size Affine(random)      = 33
compressed_size Affine(identity)    = 33
compressed_size (Affine,Affine)     = 66
compressed_size (identity,identity) = 66
```

66 ≤ 256 with a wide margin, for every possible input. The neighbouring
`.expect("infallible serialization")` is unreachable for the same reason. **F5: not a remote DoS.**

The other `expect`s: `usize_to_u64!` (`lib.rs:662`) cannot fail on a 64-bit target;
`ct_mspp!`'s `.expect("N > 0")` (defined `lib.rs:690`, panicking at its expansion in
`MultiExpArg::new`, `lib.rs:756`) needs `N == 0`; `shuffle_remask_prove`'s
`.expect("invalid shuffle proof")` (`lib.rs:1266`) is on our *own* proving path, not a peer's input.

**F6** is a real defect, just not a remotely reachable one. `lib.rs:1324` declares

```rust
const _N_GREATER_THAN_1: () = assert!(N > 1);
```

but that associated const is never referenced anywhere in the crate, so it is never monomorphised
and never evaluated. The guard does nothing. Measured:

```
Shuffle::<1>::default() COMPILED and ran -> the N>1 const guard is dead code
Shuffle::<1>::shuffle_initial_deck -> panicked: true    (lib.rs:1266 "invalid shuffle proof")
Shuffle::<0>::default() COMPILED and ran too
Shuffle::<0>::shuffle_initial_deck -> panicked: true    (lib.rs:756 "N > 0")
```

Note *which* panic `N = 1` produces: not the intended compile error and not `N > 0`, but a failure
of the crate's own proof to verify its own output. `N` is chosen by us and will be 52, so this is a
footgun rather than a vulnerability — but a vendored fork should make the guard live by referencing
it (`let _ = Self::_N_GREATER_THAN_1;` in `default()`).

---

## 9. What I did not manage to check

Stated plainly, because a confident summary that skipped these would be worth less than the gaps.

1. **No coverage-guided fuzzing.** `cargo-fuzz` needs nightly; nightly could not be downloaded
   (os error 10013, §2). 143 million blind inputs is a large smoke test, not a fuzzing campaign.
   There is no coverage data, so I cannot tell you what fraction of the parse-and-verify paths were
   reached — and the acceptance rates in §2 suggest some targets (`MaskedDeck`, 0.7 % accepted) spent
   most of their budget being rejected at the first bad point, never reaching the verifier at all.
2. **Whether an all-on-one-twist off-curve deck can pass `verify_shuffle`.** §6 shows one naive
   construction fails. I did not attempt the harder one where every point lies on the same twist, so
   I cannot claim the uncompressed path is safe — only that the compressed path makes it moot.
3. **The `unsafe` audit is source-only.** I traced the one `unsafe` site reachable from
   deserialisation — `SerBuffer::as_slice` — and satisfied myself it is sound because the struct is
   `#[repr(C, align(1))]`. I did not run miri or ASan, and could not: both want nightly, which is
   the same blocker as item 1.
4. **The algebra.** I deliberately did not assess whether `MultiExpArg` and
   `SingleValueProductArg` are sound. My degenerate-deck attacks (§4) work *with* the algebra, not
   against it. If the algebra is broken, everything in this document is beside the point.
5. **Timing side channels.** Not examined at all. `Fp` comparison, `into_affine`, and the
   `open_deck.iter().position()` linear scan in `reveal_card` (`lib.rs:1623`) are all plausibly
   data-dependent. For a P2P game over the network this is probably not the top risk, but nobody has
   looked.
6. **`zeroize` coverage.** `SecretKey` derives `Zeroize`/`ZeroizeOnDrop`, but intermediate scalars
   in the prover (`rho`, `perm`, the witnesses) are plain stack values. Not assessed.

---

## 10. F7 — the denial-of-service budget

`src/bin/t8_cost.rs` **(measured)**:

```
                                                           idle   under load
parse ShuffleProof<52> compressed, Validate::Yes          0.095       0.157 ms/op
parse ShuffleProof<52> compressed, Validate::No           0.094       0.156 ms/op
parse MaskedDeck<52>   compressed, Validate::Yes          0.923       1.501 ms/op
parse MaskedDeck<52>   compressed, Validate::No           0.918       1.443 ms/op
verify_initial_shuffle, VALID proof                      36.305      49.900 ms/op
verify_initial_shuffle, INVALID proof (worst case)       31.098      41.127 ms/op
verify_shuffle,         INVALID proof (worst case)       10.133      13.552 ms/op
proposed structural check (identity/dup/reuse), 52 cards   0.019       0.028 ms/op
```

Same binary, twice: "under load" was taken while the 12 fuzz workers were saturating the machine,
"idle" once they had finished. Treat the idle column as the honest cost and the loaded column as a
reminder of what a peer that is already busy will actually experience.

Two things fall out.

**`Validate::Yes` is free.** 0.095 ms versus 0.094 ms for a 5 547-byte proof — within noise, which
is exactly what §3.2 predicts, since on the compressed path it re-checks a property that
decompression already guaranteed. There is no performance argument for ever passing `Validate::No`.

**Rejection is expensive.** A bogus proof costs the attacker nothing to produce — assemble 11 valid
compressed points and 162 valid scalars, well under a millisecond — and costs the victim 10–36 ms.
Put in bandwidth terms: one peer on a 1 Mbit/s uplink can push about 22 proof-shaped messages per
second, which is 0.2–0.8 CPU cores of verification burned at *every other seat*, since everyone
verifies every shuffle. Two such peers pin a core each.

Our layer needs, in this order, before ziffle's verifier is ever called:

1. exact-length check (§3.5 table) and canonical re-encoding check (§7);
2. the 0.019 ms structural check from §4 — roughly 1 900× cheaper than the proof verification, and
   it rejects the F1 class outright;
3. per-peer rate limiting on shuffle messages, and a rule that a peer gets one shuffle proof per
   hand per position — a second one is not verified, it is a protocol violation;
4. only then `verify_shuffle`.

---

## 11. Required changes to the integration layer

Consolidated, in the order they should appear in the wrapper. None of these are ziffle changes;
all of them are ours, and all are cheap.

| # | Rule | Closes |
|---|---|---|
| 1 | Compressed encoding only. `Validate::Yes` always. Ban every `*_unchecked` deserialiser in CI. | F3 |
| 2 | Enforce the exact expected byte length per message type before parsing. | F4, P1 |
| 3 | Re-serialise after parsing and compare to the input; reject on mismatch. Hash and sign only the canonical form. | F4 |
| 4 | Reject `pk == identity`; reject duplicate `pk` across seats. | F2 |
| 5 | On every `next` deck: no `c₁` is the identity; the `c₁` are pairwise distinct; no `c₁` appears in `prev`. | F1 |
| 6 | Every seated player shuffles, in a fixed committed order, and cards are only ever dealt from the last deck in the chain. A disconnect must abort the hand, not truncate the chain. | F1 |
| 7 | Rate-limit shuffle messages; one proof per peer per position per hand. | F7 |
| 8 | If vendoring: make `_N_GREATER_THAN_1` live by referencing it. | F6 |

Rules 4 and 5 are three-line functions. They are the difference between a security argument that
depends on getting the chain ordering right and one that does not.

---

## Appendix — probe inventory

All under
`<scratchpad>`.

| File | What it establishes |
|---|---|
| `src/bin/t1_sizes.rs` | serialised sizes, four-mode round trips, truncation, trailing bytes |
| `src/bin/t2_validate.rs` | identity acceptance, off-curve acceptance, scalar canonicality, cofactor no-op, first identity-pk forgery |
| `src/bin/t3_fuzz.rs` | the structured fuzzer: 7 targets × 4 modes × 6 generators, allocation tracking, `catch_unwind` |
| `src/bin/t4_degenerate.rs` | ported malicious prover; attacks A, B, C |
| `src/bin/t5_edges.rs` | transcript sizes, dead `N > 1` guard, infinity malleability, `sk = 0` end to end |
| `src/bin/t6_chain.rs` | attacker's position in the shuffle chain; repair by an honest re-mask |
| `src/bin/t8_cost.rs` | parse and rejection costs, structural-check cost |
| `src/bin/t9_diff.rs` | `Validate::Yes` vs `Validate::No` differential over 2 M inputs per mode |
| `src/bin/t10_offcurve.rs` | off-curve smuggling attempt through `verify_initial_shuffle` |
| `src/bin/t11_malleable.rs` | 64 encodings per point; deck and proof malleability; identity frequency |
| `neg/neg.rs` | negative compile test for the type-state boundary |
