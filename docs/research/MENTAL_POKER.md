# Phase 0 research: mental poker / verifiable shuffle

Author: Phase-0 `mentalpoker` agent
Date: 2026-08-28
Binding spec sections: 5, 6, 8, 9, 10, 35, 36

**Environment for every measurement in this document:** Windows 10, x86_64-pc-windows-msvc,
24 cores, rustc 1.95.0 / cargo 1.95.0, `--release`, single-threaded unless stated.

Every claim below carries a **Verification** line stating how it was checked. Two forms are used:

* **(a) compiled** — I compiled and ran code that exercises the claim.
* **(b) source** — I read the actual crate source unpacked under
  `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/<crate>-<version>/`
  or a git checkout under `~/.cargo/git/checkouts/`.

docs.rs and recollection were used only as leads, never as evidence.

Probe crates written for this report (all outside the repo):

```
<scratch>/probe-mentalpoker/    ristretto255 ElGamal + group benchmarks + cut-and-choose
<scratch>/probe-ziffle/         ziffle benchmarks (src/main.rs) + 13 adversarial tests (src/bin/adv.rs)
                                + a negative compile test (src/bin/bypass.rs)
<scratch>/probe-mp-old/         mental-poker 0.1.0 build attempt
<scratch>/probe-zshuffle/       zshuffle 0.1.2 build attempt
<scratch>/probe-curdle/         curdleproofs 0.0.1 build attempt
<scratch>/gx-mental-poker/      geometryxyz/mental-poker clone + examples/bench52.rs
<scratch>/vendor/               unpacked .crate sources for reading
```

where `<scratch>` is
`~/AppData/Local/Temp/claude/<session>/<session-id>/scratchpad`.

---

## 1. Recommendation up front

**Use the Barnett–Smart (2003) card protocol instantiated with a Bayer–Groth (2012) shuffle
argument, taken from the `ziffle` 0.1.0 crate — vendored, pinned, independently reviewed, and
hidden behind our own trait so it can be swapped.**

| Decision | Choice |
| --- | --- |
| Construction | Barnett–Smart "Mental Poker Revisited" (IMA IMACC 2003), threshold ElGamal cards |
| Shuffle argument | Bayer–Groth 2012, `m = 1` instantiation (§5.3 single-value product + multi-exponentiation) |
| Group | secp256k1 (prime order, cofactor 1) — **inherited from ziffle, not independently chosen** |
| Library | `ziffle = "0.1.0"` (MIT OR Apache-2.0) over `ark-ec`/`ark-ff`/`ark-secp256k1` 0.5.0 |
| We compose ourselves | transcript/hash-chain, signing, dealing policy, board gating, disconnect handling |
| Confidence | **Medium.** High on performance and fit; medium-low on the library's crypto correctness. |

Measured cost of a full hand's shuffle+reveal crypto, 52 cards, one node's own work:
**~195 ms heads-up, ~250 ms 3-handed, ~420 ms 6-handed.** Comfortably inside "a hand starts in a
couple of seconds" even with network latency on top.

The honest caveat, stated here rather than buried: ziffle is a **single-author crate with 3 GitHub
stars, 8 commits, one release, 293 downloads and no audit**, whose own README says *"DO NOT use
this library to play for non-trivial amounts of money."* Section 9 explains why I still recommend
it for a play-money prototype and exactly what would have to change before real money.

---

## 2. Academic constructions

### 2.1 What spec §6 demands

| §6 requirement | Which construction supplies it |
| --- | --- |
| distributed random deck creation | shuffle chain (every player permutes in turn) |
| cryptographically secure randomness | OS CSPRNG per player, §7 below |
| verifiable shuffle | Bayer–Groth / Neff / Groth–Lu / cut-and-choose |
| re-encryption or equivalent | ElGamal re-randomisation |
| proof no card added/removed/changed | the shuffle argument's soundness |
| private hole cards | threshold ElGamal, n-of-n |
| selective opening | per-card decryption shares + DLEQ |
| board not knowable early | withhold shares until the street |
| robust against one malicious client | all proofs verified by everyone |
| verifiable transcript | our layer, not the crypto library's |

### 2.2 Survey

**Barnett & Smart, "Mental Poker Revisited" (IMA IMACC 2003).**
The reference construction. Cards are group elements; a card is masked as an ElGamal ciphertext
under an *aggregate* public key `apk = Σ pk_i`. Every player in turn re-randomises and permutes the
whole deck, proving correctness in zero knowledge. Opening a card requires every player to publish a
decryption share `sk_i · c1` with a Chaum–Pedersen DLEQ proof. Security rests on DDH in the chosen
group, plus the soundness of the shuffle argument. It gives every property in §6 except robustness
against a player who simply stops talking (see §8 below). Deck size 52 is trivial for it.
Implementations: `geometryxyz/mental-poker` (Rust), `ziffle` (Rust), various academic prototypes.
This is the construction the whole modern field builds on, and it is what I recommend.

**Bayer & Groth, "Efficient Zero-Knowledge Argument for Correctness of a Shuffle" (EUROCRYPT 2012).**
Not a poker protocol — a *shuffle argument* that plugs into Barnett–Smart. Proves that a list of
`N` output ciphertexts is a permutation-and-re-randomisation of `N` input ciphertexts. Arranges the
deck as an `m × n` matrix and achieves `O(sqrt(N))` communication; with `m = 1` it degenerates to a
linear-size but much simpler argument, which is what ziffle implements. Assumptions: DDH (for the
encryption) and the discrete-log hardness underlying the Pedersen commitments. It is a public-coin
argument, made non-interactive by Fiat–Shamir. For `N = 52` both variants measure in the tens of
milliseconds and a few kilobytes (§5). This is the right shuffle argument for poker: it is the
best-studied, and it is the one both available Rust implementations chose.

**Wei & Wang, "Fast Mental Poker" (ePrint 2009/439), and Wei's TDP-based protocols.**
Drops the per-shuffle zero-knowledge proof in favour of a scheme where correctness is checked only
at the end, buying large constant-factor speedups. The trade-off is that misbehaviour is detected
*late* — after cards have been dealt — which is exactly the failure mode §8 of the spec forbids
("if the proof does not match, the hand must not continue"). It also has a thinner analysis record
than Barnett–Smart. No maintained Rust implementation. **Rejected.**

**Castellà-Roca et al. (several protocols, ~2003–2006).**
Mental poker without a trusted dealer using verifiable mixing and/or commitment chains. Academically
respectable, but the line has less follow-up scrutiny than Barnett–Smart, and there is no Rust (or
really any maintained) implementation to build on. Choosing it would mean implementing everything
from scratch, which §6 and §36 tell us not to do. **Rejected on implementation availability.**

**Neff, "A Verifiable Secret Shuffle and its Application to E-Voting" (CCS 2001)** and
**Groth–Lu.**
The predecessors to Bayer–Groth, from the e-voting world. Neff's shuffle is sound and well studied,
but larger and slower than Bayer–Groth for the same soundness, and its Rust implementations are tied
to voting systems. Groth–Lu is pairing-based, which forces a pairing-friendly curve and a bigger
dependency. Bayer–Groth supersedes both for our purpose. **Rejected as strictly dominated.**

**Curdleproofs (Ethereum Foundation, 2022).**
A modern shuffle argument built for Whisk, targeting shuffles of hundreds-to-thousands of validator
trackers over BLS12-381. The trackers `(rG, k·rG)` have a similar shape to our card ciphertexts, so
it is not absurd on paper. In practice it is tuned for large `N` and its API is Whisk-shaped, its
crate is `0.0.1` from September 2022, and it pins arkworks 0.3.0. See §4.5. **Rejected.**

**Cut-and-choose sigma protocols (the "obvious" home-made option).**
Shuffle the deck `t` times independently; the verifier challenges half of them and the prover opens
the permutation and masking factors for the challenged ones. Soundness `2^-t`. It needs no clever
algebra, which makes it tempting. Section 5.3 measures what it actually costs. Short version: the
*time* is competitive but the *proof size* is 195 KB at 2^-40 and 390 KB at 2^-80, versus 5.5 KB for
Bayer–Groth. Over GossipSub and Circuit-Relay-v2 fallback that is a serious bandwidth problem.
**Rejected on proof size.**

**Anything newer with real analysis?**
The recent work that touches mental poker is mostly zkSNARK-based shuffles (Groth16/PLONK circuits
proving a permutation), which is what `zshuffle`/uzkge does. These give tiny proofs and fast
verification, at the cost of a trusted setup (for Groth16), a much larger and less reviewable
codebase, and a proving cost that is worse than Bayer–Groth at `N = 52`. For a 52-card deck the
SNARK machinery is not earning its complexity. See §4.3.

---

## 3. Group selection

Task item 3 asked me to verify the claim that curve25519-dalek's ristretto255 is a prime-order group
with addition, subtraction and negation — exactly what additive ElGamal needs — by compiling ElGamal
over it. I did.

### 3.1 ristretto255 ElGamal, compiled and run

`<scratch>/probe-mentalpoker/src/main.rs` builds and runs the full card-crypto shape: an aggregate
`n`-of-`n` key, encryption, re-randomisation (what a shuffle does per card), and threshold decryption
from shares.

```rust
use curve25519_dalek::{RistrettoPoint, Scalar, constants::RISTRETTO_BASEPOINT_TABLE};
use curve25519_dalek::traits::Identity;
use getrandom::rand_core::UnwrapErr;
use getrandom::SysRng;

let mut rng = UnwrapErr(SysRng);
let (sk_a, sk_b) = (Scalar::random(&mut rng), Scalar::random(&mut rng));
let apk = &sk_a * RISTRETTO_BASEPOINT_TABLE + &sk_b * RISTRETTO_BASEPOINT_TABLE;

let m  = RistrettoPoint::random(&mut rng);          // the card
let r  = Scalar::random(&mut rng);
let c1 = &r * RISTRETTO_BASEPOINT_TABLE;
let c2 = m + r * apk;                                // encrypt

let r2  = Scalar::random(&mut rng);                  // re-randomise
let c1b = c1 + &r2 * RISTRETTO_BASEPOINT_TABLE;
let c2b = c2 + r2 * apk;

let dec = c2b - (sk_a * c1b + sk_b * c1b);           // n-of-n decrypt
assert_eq!(dec, m);
```

Program output:

```
[ristretto] aggregate key pk_a+pk_b == (sk_a+sk_b)G : OK
[ristretto] encrypt -> re-randomise -> n-of-n decrypt round trip: true
[ristretto] single share alone does NOT recover plaintext: true
[ristretto] point negation available: true
```

**Verification: (a) compiled** — `probe-mentalpoker`, `cargo build --release` clean, output above.
The claim in the task is confirmed: ristretto255 gives a prime-order group with `Add`, `Sub` and
`Neg` on `RistrettoPoint`, and additive ElGamal over it works exactly as needed.

### 3.2 API facts about curve25519-dalek 5.0.0 that differ from 4.x

These matter for the whole project, not just this document, because the 5.x ecosystem moved.

* **curve25519-dalek max stable is 5.0.0 (2026-07-06)**, not 4.x. `edition = "2024"`,
  `rust-version = "1.85.0"`. **Verification: (b) source** — `curve25519-dalek-5.0.0/Cargo.toml`.
* Real feature names: `alloc`, `precomputed-tables`, `zeroize`, `digest`, `group`, `rand_core`,
  `lizard`, `legacy_compatibility`. Default = `alloc` + `precomputed-tables` + `zeroize`.
  **Verification: (b) source** — `[features]` block, `curve25519-dalek-5.0.0/Cargo.toml` lines 59–77.
* It depends on **`rand_core` 0.10**, and `rand_core` 0.10 has **no features at all** — in
  particular no `os_rng`. Asking for it is a hard resolver error:
  `package ... depends on rand_core with feature os_rng but rand_core does not have that feature`.
  **Verification: (a) compiled** — the failing `cargo fetch`, plus the crates.io API showing
  `"features": {}` for rand_core 0.10.1.
* **`OsRng` no longer lives in `rand_core`.** The OS RNG is now `getrandom::SysRng` from
  `getrandom` 0.4 with feature `sys_rng`; it implements `TryRng`/`TryCryptoRng`, and you get an
  infallible `Rng` by wrapping it: `getrandom::rand_core::UnwrapErr(getrandom::SysRng)`.
  **Verification: (b) source** — `getrandom-0.4.3/src/sys_rng.rs`, plus **(a) compiled** in the probe.
  This is the spec §7 "use OsRng, never SmallRng" requirement — the current spelling of it.
* `RistrettoPoint::identity()` requires importing `curve25519_dalek::traits::Identity`; it is not an
  inherent method. `traits` also exports `MultiscalarMul` and `VartimeMultiscalarMul`, which are the
  fast paths a shuffle verifier wants. **Verification: (a) compiled** — the E0599 error before I
  added the import, then a clean build after.

### 3.3 Measured group operations

All figures from `probe-mentalpoker`, release build, 2000 iterations each (20 000 for point
addition), results accumulated into a sink so nothing is optimised away.

| Operation | ristretto255 | k256 (secp256k1) | ark-secp256k1 |
| --- | ---: | ---: | ---: |
| variable-base scalar mul | **28.88 µs** | 56.04 µs | 98.90 µs |
| fixed/generator-base scalar mul | **14.82 µs** | 51.06 µs | — |
| point addition | **0.209 µs** | 0.269 µs | 0.479 µs |
| MSM of 52 (constant time) | 614.23 µs | — | — |
| MSM of 52 (variable time) | 422.23 µs | — | — |

**Verification: (a) compiled** — `probe-mentalpoker`, output reproduced verbatim.

### 3.4 What this means

ristretto255 is **1.9× faster than k256 and 3.4× faster than ark-secp256k1** on the operation that
dominates a shuffle (variable-base scalar multiplication), and it has a genuine fixed-base
precomputed-table path that is another 2× on top. It is also the group with the best audit and
review history of the three, and its prime-order abstraction removes the cofactor and
point-validation footguns by construction.

On the merits, **ristretto255 is the better group.** The honest complication is that the library I
am recommending does not offer the choice: `ziffle` hardcodes

```rust
type Curve       = ark_secp256k1::Config;
type CurveAffine = ark_secp256k1::Affine;
type Scalar      = <Curve as CurveConfig>::ScalarField;
```

**Verification: (b) source** — `ziffle-0.1.0/src/lib.rs`, type aliases near the top.

So taking ziffle means taking the *slowest* of the three groups. Section 5 shows this still lands at
~100 ms per shuffle proof, which is fast enough, so I accept it rather than fork the crate. It is
worth recording that a ristretto255 port of ziffle would plausibly be ~3× faster — that is the
single highest-leverage optimisation available later, if we ever need it. secp256k1 is not a *bad*
choice: prime order, cofactor 1, no small-subgroup concerns, and enormous deployment.

---

## 4. Rust implementations, judged honestly

### 4.1 `ziffle` 0.1.0 — **recommended**

| | |
| --- | --- |
| Version / released | 0.1.0, 2025-11-01 (only release) |
| Licence | MIT OR Apache-2.0 |
| Downloads | 293 |
| Repo | github.com/v26-solutions/ziffle — 3 stars, 0 forks, 0 open issues, 8 commits |
| Deps | `ark-ec`, `ark-ff`, `ark-secp256k1`, `ark-serialize`, `ark-std` 0.5.0; `sha2` 0.10; `zeroize` 1.8 |
| Code size | **1779 lines** in a single `src/lib.rs`, plus 736 lines of tests |
| `no_std` | yes, no allocation |
| Audit | **none**, explicitly disclaimed in the README and crate docs |

**Compiles today on stable 1.95: yes, cleanly.** Its own suite passes: **16 unit tests + 15 doctests,
0 failures.** The unit tests include `verify_tampered_shuffle_fails`,
`verify_tampered_initial_shuffle_fails`, `reveal_card_with_wrong_aggregate_token_fails`,
`three_way_shuffle`, `reveal_all_cards_in_deck`.
**Verification: (a) compiled** — `cargo test --release` in `<scratch>/vendor/ziffle-0.1.0`.

**What it actually implements.** Reading the source rather than the marketing: it is genuinely
Barnett–Smart with a Bayer–Groth shuffle argument, and the author's comments cite the paper sections
directly. The proof object is

```rust
pub struct ShuffleProof<const N: usize> {
    c_pi:     PedersonCommitment,      // commitment to the permutation
    c_xpi:    PedersonCommitment,      // commitment to [x^{pi(i)+1}]
    mexp_arg: MultiExpArg<N>,          // multi-exponentiation argument (BG12 §5)
    prod_arg: SingleValueProductArg<N> // single-value product argument (BG12 §5.3)
}
```

with an in-source note: *"As we set m = 1, only the Single Value Product Argument protocol
(Section 5.3) is required."* That is the honest linear-size instantiation of Bayer–Groth, not the
`O(sqrt(N))` matrix version. For `N = 52` that is the right engineering call.
**Verification: (b) source** — `ziffle-0.1.0/src/lib.rs`.

**Can a careful reviewer actually review it?** Yes — and this is the strongest argument for it. 1779
lines of single-file, heavily commented, `no_std`, allocation-free code that names the paper sections
it implements is genuinely reviewable by one competent cryptographer in a few days. Compare
`zshuffle`, which drags in 13 forked arkworks crates. Reviewability is the property spec §28 and §36
are really asking for.

**Points I checked in the source because they are the classic places this goes wrong:**

* *Pedersen generators.* `PedersonCommitKey::default()` derives `h` and `gs[]` deterministically from
  hardcoded seeds (`b"PEDERSON-H-V1"`, `b"PEDERSON-VECTOR-G-V1"`) via
  `StdRng::from_seed(Sha256::digest(seed))` and `CurveProj::rand(&mut drng)`. My worry was that if
  `CurveProj::rand` sampled a *scalar* and multiplied the generator, anyone could recompute that
  scalar from the public seed and the Pedersen commitment would not be binding — a total break.
  It does not. arkworks samples a random **base-field x-coordinate**, solves for y, rejects on
  failure, and multiplies by the cofactor (1 for secp256k1):

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

  So the generators have no known discrete-log relation to `G` and binding holds under DL. This is a
  legitimate nothing-up-my-sleeve derivation. **Verification: (b) source** —
  `ark-ec-0.5.0/src/models/short_weierstrass/group.rs` lines 95–108.

* *Type-state guard.* `Verified<T>` has a private field and no `CanonicalDeserialize` impl, so a
  `Verified<PublicKey>` or `Verified<MaskedDeck<N>>` can only come from an actual successful
  verification — it cannot be forged from outside the crate and cannot be smuggled in off the wire.
  Attempting `Verified(pk)` in a downstream crate fails to compile:
  `error[E0423]: cannot initialize a tuple struct which contains private fields`.
  **Verification: (a) compiled** — `probe-ziffle/src/bin/bypass.rs`, negative compile test, plus
  **(b) source** for the absence of a deserialize impl (only `MultiExpArg`, `SingleValueProductArg`,
  `ShuffleProof`, `RevealTokenProof`, `OwnershipProof` get one).
  This is a real API-design win: it makes "never display a cryptographically unverified card"
  (spec §22) a *type error* rather than a discipline.

* *Fiat–Shamir binding.* `challenge_x` absorbs `apk`, the **whole previous deck**, the whole next
  deck, and `c_pi` before deriving the challenge, so a proof is bound to the specific transition.
  Challenges are derived with domain-separated tags (`ziffle/BG12X/v1`, `ziffle/DLEQ/v1`, …) through
  arkworks' `DefaultFieldHasher<Sha256>` hash-to-field. **Verification: (b) source.**

**Adversarial results.** I wrote 13 attacks (`probe-ziffle/src/bin/adv.rs`). All 13 behaved
correctly:

```
T1  keygen PoK replayed to other ctx rejected:                  true
T2  honest initial shuffle verifies:                            true
T3  shuffle proof replayed to other ctx rejected:               true
T4  duplicated-card deck with honest proof rejected:            true
T5  proof wire round-trip verifies:                             true (5547 bytes)
T6a honest chained verifies:                                    true
T6b chained proof against WRONG prev deck rejected:             true
T7a A's token verified against B's pubkey rejected:             true
T7b token replayed onto a different card rejected:              true
T7c full reveal (both tokens) works:                            Some(50)
T7d ONE player alone cannot open the card:                      true (got None)
T7e other player alone cannot open the card:                    true (got None)
T8  all 52 cards present exactly once after 2 chained shuffles: true
```

T4 is the spec §8 attack verbatim (remove a card, duplicate another): I serialised a valid deck,
overwrote card 5's bytes with card 3's, re-deserialised, and presented it with the honest proof —
rejected. T7d/T7e are the spec §9 and §35 core invariant: a single player holding one valid
decryption share cannot open a card. T8 confirms no card is lost or duplicated across a two-player
shuffle chain by opening all 52 and checking the multiset.
**Verification: (a) compiled** — `probe-ziffle/src/bin/adv.rs`, output verbatim.

**Weaknesses, stated plainly.** One author, 3 stars, 8 commits, one release, 293 downloads, no
audit, and a README that tells you not to use it for money. `Transcript` is a hand-rolled
SHA-256 chain rather than a reviewed transcript library like merlin/STROBE (it looks correct —
length-prefixed labels and length-prefixed values — but it is bespoke). There is an `assert!` panic
path in `Transcript::update_with_serialized` if a serialised element exceeds a 256-byte buffer;
unreachable for 33-byte points, but it is a panic, and panics on network-derived data need care
(spec §27). The transcript is deliberately *forked* between the multi-exponentiation and product
arguments — see §9.

### 4.2 `mental-poker` 0.1.0 (2022) — **dead, rejected**

Two independent disqualifications.

First, **it contains no cryptography at all.** Grepping its `src/` for
`curve|elgamal|ElGamal|shuffle_proof|zero.knowledge|Proof` returns **zero hits**. The only game
implementation in the crate is `src/game/trusting.rs` — i.e. the trusted-dealer model that spec §5
explicitly forbids. **Verification: (b) source** — `<scratch>/vendor/mental-poker-0.1.0/`.

Second, **it does not compile on stable.** It opens with
`#![feature(step_trait)] #![feature(generic_const_exprs)] #![feature(generic_associated_types)]
#![feature(type_alias_impl_trait)]`, giving `error[E0554]: #![feature] may not be used on the
stable release channel`, and its nightly APIs have since drifted anyway:

```
error[E0053]: method `steps_between` has an incompatible type for trait
   --> mental-poker-0.1.0/src/deck.rs:103:51
    = note: expected signature `fn(&DeckPosition<_>, &DeckPosition<_>) -> (usize, Option<usize>)`
               found signature `fn(&DeckPosition<_>, &DeckPosition<_>) -> Option<usize>`
error: could not compile `mental-poker` (lib) due to 7 previous errors
```

**Verification: (a) compiled** — `probe-mp-old`, verbatim failure. Last touched 2022-02-25.

### 4.3 `zshuffle` 0.1.2 / uzkge — **rejected**

Released 2024-06-03, 4771 downloads, **GPL-3.0-only**. It *does* compile on stable 1.95
(**Verification: (a) compiled** — `probe-zshuffle`, `Finished dev profile in 59.43s`).

Two disqualifiers.

**Supply chain.** It does not use arkworks; it uses a private fork of essentially the whole arkworks
stack, republished by one vendor under `-zypher` names. The build pulls
`ark-std-zypher`, `ark-ff-zypher`, `ark-ff-asm-zypher`, `ark-ff-macros-zypher`,
`ark-serialize-zypher`, `ark-serialize-derive-zypher`, `ark-ec-zypher`, `ark-poly-zypher`,
`ark-relations-zypher`, `ark-snark-zypher`, `ark-r1cs-std-zypher`, `ark-crypto-primitives-zypher`,
`ark-crypto-primitives-macros-zypher`, `ark-groth16-zypher`, `ark-bn254-zypher`,
`ark-ed-on-bn254-zypher` — 13+ forked crates, all pinned to arkworks 0.4.x lineage.
**Verification: (a) compiled** — the build log lists every one; **(b) source** — the
`package = "ark-*-zypher"` renames in `zshuffle-0.1.2/Cargo.toml`.
Reviewing this means reviewing a forked arkworks. That is the opposite of spec §28's "minimal
dependencies" and §36's "security-critical claims must be demonstrable".

**Licence and setup.** GPL-3.0-only would force our entire client to GPL-3.0 — a decision for the
project owner, not a research agent, but worth flagging since the spec says "open-source" without
choosing a licence. And it is Groth16-based, so it needs a **trusted setup / SRS**, which introduces
exactly the kind of trusted party the whole project exists to eliminate. For a 52-card deck the SNARK
machinery buys nothing that Bayer–Groth does not already give us at 5.5 KB and 42 ms.

### 4.4 `distributed-cards` 0.5.2 — **rejected**

LGPL-3.0, 690 lines, last release 2022-07-06. It implements an SRA-style commutative-encryption
shuffle over `num-bigint-dig` primes. Grepping `src/` for `proof|Proof|zero.knowledge|verify`
returns **zero hits** — there is no verifiable shuffle whatsoever. It fails spec §8 outright: a
malicious shuffler can substitute a card and nothing detects it.
**Verification: (b) source** — `<scratch>/vendor/distributed-cards-0.5.2/src/`.

### 4.5 `curdleproofs` 0.0.1 — **rejected**

MIT, released 2022-09-14, 7854 downloads. It compiles on stable 1.95
(**Verification: (a) compiled** — `probe-curdle`), but only by pulling **arkworks 0.3.0** — five
major versions behind the 0.5.0 the rest of our stack would use, which means we would carry two
incompatible arkworks trees. It is a `0.0.1` with `-alpha` predecessors, untouched for ~4 years, and
it is engineered for Whisk's large-`N` validator shuffles rather than a 52-card deck. It also solves
only the shuffle half — we would still need to build the threshold-decryption and DLEQ layer
ourselves. Not worth it when ziffle and barnett-smart-card-protocol both give the whole protocol.

### 4.6 `sra-wasm` 0.1.0 — **rejected**

GPL-3.0, 2023-11-04. Implements the SRA commutative-encryption protocol. SRA has no verifiable
shuffle (same §8 failure as `distributed-cards`), and it is a WASM-targeted crate. Not applicable.

### 4.7 `pokerproof` 0.1.0 — **rejected**

MIT, 2026-02-10, **28 downloads**. Described as a "provably fair Texas Hold'em poker engine with
cryptographic verification". "Provably fair" is the online-casino commit-reveal pattern — the
*operator* commits to a seed and reveals it afterwards — which is a fundamentally different (and
much weaker) trust model: it presumes a dealer and only lets you audit them after the fact. That is
precisely the model spec §5 rejects. Not a mental-poker protocol.

### 4.8 `barnett-smart-card-protocol` (geometryxyz/mental-poker) — **strong runner-up**

Not on crates.io (`crate barnett-smart-card-protocol does not exist`; likewise `proof-essentials`).
It exists only as a GitHub repo, `geometryxyz/mental-poker`, MIT OR Apache-2.0, **12 commits**, last
commit `b1313f3` 2025-01-29.
**Verification: (a) compiled** — cloned and built; **(b) source** — the checkout.

This is the more *complete* implementation. Its `proof-essentials` companion implements the **full**
Bayer–Groth machinery — `hadamard_product`, `matrix_elements_product`, `multi_exponentiation`,
`single_value_product`, `zero_value_bilinear_map`, `shuffle` — i.e. the real `m × n` matrix version
with `O(sqrt(N))` proofs, at 4843 lines. It also ships Schnorr identification and Chaum–Pedersen DLEQ
as first-class modules. It genuinely works: its `round` example plays a real 4-player hand.

```
Andrija: 6♥
Kobi: A♣
Nico: 7♠
Tom: Q♦
```

**Why it is nonetheless the runner-up and not the pick — three practical blockers:**

1. **It cannot be depended on reproducibly.** Its manifest points at
   `ssh://git@github.com/geometryresearch/proof-toolbox.git` for both `proof-essentials` and
   `starknet-curve`. That org was renamed to `geometryxyz`, and the SSH URL fails outright on this
   machine:
   ```
   error: failed to get `proof-essentials` as a dependency of package `barnett-smart-card-protocol`
   Caused by: unable to update ssh://git@github.com/geometryresearch/proof-toolbox.git
   Caused by: failed to set hostkey preference: The requested method(s) are not currently supported; class=Ssh (23)
   ```
   I only got it to build by rewriting the manifest to
   `https://github.com/geometryxyz/proof-toolbox.git` and setting
   `CARGO_NET_GIT_FETCH_WITH_CLI=true`. A dependency on an unpublished git repo under a *stale org
   name*, reachable only via a patched URL, is not something to pin a security-critical build to
   (spec §28 requires reproducible pinning). We would have to vendor three repositories.
   **Verification: (a) compiled** — both the failure and the patched success.
2. **arkworks 0.3.0**, released 2022 — same two-arkworks-trees problem as curdleproofs, plus
   `warning: the following packages contain code that will be rejected by a future version of Rust:
   ark-poly-commit v0.3.0`. **Verification: (a) compiled.**
3. **Surface area.** 1731 + 4843 = ~6600 lines to review versus ziffle's 1779.

It is also **not faster** in a way that would justify the cost — see §5.2. If ziffle's correctness
fails review, this is where we go next, and §9 says so.

---

## 5. Cost model

### 5.1 ziffle, 52-card deck, measured

From `probe-ziffle/src/main.rs`, release, single-threaded.

| Metric | 2 players | 3 players | 6 players |
| --- | ---: | ---: | ---: |
| keygen + ownership proofs (all players) | 1.2 ms | 1.4 ms | 2.3 ms |
| shuffle **prove** (per shuffle) | 94–111 ms | 97–105 ms | 94–100 ms |
| shuffle **verify** (per shuffle) | 37–43 ms | 39–43 ms | 37–43 ms |
| reveal one card (all tokens + proofs + lookup) | 1.39 ms | 2.08 ms | 4.34 ms |
| **full hand, one node's own crypto work** | **~195 ms** | **~248 ms** | **~422 ms** |

Sizes, from `compressed_size()` / an actual serialise round-trip:

| Object | Bytes |
| --- | ---: |
| `ShuffleProof<52>` | **5547** |
| `MaskedDeck<52>` | **3432** (66 B/card) |
| `PublicKey` | 33 |
| `OwnershipProof` | 65 |
| `RevealToken` | 33 |
| `RevealTokenProof` | 98 |

The "full hand" row counts what a single node actually does: prove its own shuffle once, verify the
other `n-1` shuffles, and participate in revealing `2n + 5` cards (hole cards plus board). Note the
per-shuffle cost is flat in the number of players — the deck is always 52 — so player count enters
only through *how many* shuffles must be verified and how many reveal tokens are aggregated.

Wire cost per hand is modest: `n × (5547 + 3432)` bytes of shuffle traffic — 18 KB heads-up, 54 KB
6-handed — plus ~131 bytes per player per revealed card.

**Verification: (a) compiled** — `probe-ziffle`, output reproduced in full above.

### 5.2 barnett-smart-card-protocol, same deck, measured

From `<scratch>/gx-mental-poker/barnett-smart-card-protocol/examples/bench52.rs`, which I wrote
against its `BarnettSmartProtocol` trait:

```
barnett-smart (starknet-curve, BG12 m=2 n=26, 52 cards): prove 58.0 ms | verify 27.2 ms | proof 5496 bytes
barnett-smart (starknet-curve, BG12 m=4 n=13, 52 cards): prove 81.9 ms | verify 27.2 ms | proof 4120 bytes
```

**Verification: (a) compiled** — `cargo run --release --example bench52`.

So the full `sqrt(N)` Bayer–Groth on a different curve is ~1.3–1.7× faster to prove and ~1.5× faster
to verify, with a 25% smaller proof at `m=4, n=13`. Real, but not remotely enough to outweigh the
three blockers in §4.8. Both implementations put a 52-card shuffle in the same tens-of-milliseconds
bucket, which is the finding that matters: **the choice of implementation is not
performance-limited, so it should be made on reviewability and supply chain.**

### 5.3 Cut-and-choose, for comparison

Task item 4 asked specifically how many repetitions a cut-and-choose sigma protocol needs and what
that costs. I measured one repetition (re-mask all 52 cards over ristretto255 — the fastest of the
three groups, so this is the *charitable* case) at **2.115 ms**, and extrapolated:

| Soundness | Repetitions | Prover | Verifier | Proof size |
| --- | ---: | ---: | ---: | ---: |
| 2^-40 | 40 | 84.6 ms | 84.6 ms | 199 680 B (**195 KB**) |
| 2^-80 | 80 | 169.2 ms | 169.2 ms | 399 360 B (**390 KB**) |
| — Bayer–Groth (ziffle, measured) | — | 100 ms | 42 ms | **5 547 B** |

**Verification: (a) compiled** — `probe-mentalpoker/src/bin/cutchoose.rs`; the per-repetition figure
is measured, the totals are that figure multiplied by the repetition count, and the size column is
`reps × (52 × 64 + 52 × 32)` bytes for the intermediate decks plus opened scalars.

The conclusion is clean and slightly counter-intuitive: cut-and-choose is **time-competitive**
(84.6 ms vs 100 ms to prove at 2^-40) but **35–70× larger on the wire**. At 6 players that is
6 × 390 KB ≈ 2.3 MB of proof traffic per hand at 2^-80, which is unacceptable over GossipSub and
outright hostile to the Circuit-Relay-v2 fallback path that spec §1 requires for CGNAT players.
Bayer–Groth's 5.5 KB is the reason to use it. This also disposes of the temptation to hand-roll
cut-and-choose "because it is simple" — it is simple and it is the wrong trade.

### 5.4 Does this meet the latency requirement?

Task item 4 set the bar at "a hand starts in a couple of seconds with 2–6 players, on top of network
latency". The shuffle chain is inherently **sequential** — player `i+1` cannot shuffle until player
`i`'s output exists — so wall-clock hand startup is roughly

```
n × (prove 100 ms + one-way latency) + (n-1) × verify 42 ms   [verifications parallelise across peers]
```

Heads-up at 100 ms RTT: ~2 × (100 + 50) + 42 ≈ **340 ms**. Six-handed at 100 ms RTT:
~6 × 150 + 5 × 42 ≈ **1.1 s**. Both inside budget, with the caveat that this is the crypto and
propagation only — it excludes DHT/GossipSub join, which the network agent measures separately.

Spec §33 forbids blocking the GUI event loop with this. At 100 ms per proof that is a real
requirement, not a nicety: **shuffle proving and verification must run on a worker thread**, with
results delivered to the UI thread as messages.

---

## 6. Distributed randomness (spec §7)

This deserves a precise answer rather than a reflexive "add commit/reveal".

**The shuffle chain is already the distributed randomness.** Each player applies a secret permutation
and fresh re-randomisation factors drawn from the OS CSPRNG. The final deck order is the composition
of all `n` permutations, so it is uniformly random as long as **at least one** player is honest — no
player, and no coalition of `n-1`, controls it. This is precisely the §7 requirement ("one player
must not decide the RNG for the whole hand"), achieved structurally rather than by a bolt-on
protocol.

**Does the last shuffler get an advantage?** No, and it is worth being explicit because this is the
classic last-mover worry. The last shuffler sees only ElGamal ciphertexts under the aggregate key.
It holds one of `n` decryption shares, so it cannot decrypt anything, and every ciphertext is
indistinguishable from every other. It can choose *any* permutation it likes, but it has no
information about which slot holds which card, so there is nothing to bias *toward*. Grinding
permutations gains it nothing. My T7d/T7e results are the empirical form of this argument: one
share alone yields `None`.

**What genuinely does need pinning down — and this is our job, not the library's:** the mapping from
deck index to recipient must be fixed **before** the shuffle chain starts, not chosen afterwards.
Otherwise a malicious last shuffler could, after seeing the final deck, argue about which index is
"the button's first hole card". Fix it deterministically in the protocol: index 0 → seat 0 card 1,
index 1 → seat 1 card 1, …, then the burn/board indices, all derived from `(table_id, hand_id,
button)`. No randomness needed, therefore nothing to manipulate.

**Where a commit/reveal beacon is still worth having:** for the *non-deck* randomness — seat
assignment at table start, and the initial button position. Spec §16 already names `RNG_COMMIT` and
`RNG_REVEAL` messages, and §7 asks for the mechanism, so implement the standard construction there:
each player commits `H(r_i || salt_i)`, all reveal, combine as `seed = H(r_1 || … || r_n)`, and
treat non-revelation as a protocol failure attributable to that peer. Use `getrandom::SysRng` for
`r_i` (§3.2). This satisfies §7 literally while §7's *substantive* requirement is met by the shuffle
chain.

**ziffle provides none of this** — it has no commit/reveal and no notion of seats. It is ours to
build, and it is straightforward, non-novel cryptography (a hash commitment), so it does not run
foul of §36.

---

## 7. What ziffle gives us for the rest of task item 6

| Requirement | Provided by ziffle? | Detail |
| --- | --- | --- |
| n-of-n key setup | **yes** | `keygen()` → `(SecretKey, PublicKey, OwnershipProof)`; `AggregatePublicKey::new(&[Verified<PublicKey>])` sums verified keys. Requires **all** `n` shares to decrypt. |
| Schnorr proof of knowledge of `sk` | **yes** | `OwnershipProof`, textbook Schnorr: `a = wG`, `e = H(ctx, pk, a)`, `z = w + e·sk`, verify `zG == a + e·pk`. 65 bytes. |
| Chaum–Pedersen DLEQ share proofs | **yes** | `RevealTokenProof { t_g, t_c1, z }` proves `share = sk·c1` and `pk = sk·G` share the same `sk`. 98 bytes. |
| Per-card selective opening | **yes** | `MaskedCard::reveal_token()` per card; aggregate only the tokens for the cards you are entitled to. |
| Verifiable shuffle | **yes** | `shuffle_initial_deck` / `shuffle_deck` + `verify_initial_shuffle` / `verify_shuffle`. |
| Board hidden until its street | **mechanism yes, policy no** | ziffle gives per-card tokens; *withholding* them until FLOP/TURN/RIVER is our state machine's job. |
| Distributed randomness commit/reveal | **no** | ours (§6). |
| Signed events, hash-chain transcript | **no** | ours (spec §12, §13). |
| Canonical serialisation | **partial** | `CanonicalSerialize`/`CanonicalDeserialize` on all wire types, round-trip verified (T5). Must be wrapped in our deterministic-CBOR signed envelope. |
| Disconnect / abort robustness | **no** | see §8. |

**Verification for the "yes" rows: (a) compiled** — every one of these is exercised in
`probe-ziffle/src/main.rs` and `src/bin/adv.rs`; **(b) source** for the algebraic descriptions.

**How the board stays hidden.** Deck indices for flop/turn/river are fixed before the shuffle (§6).
At each street, every player publishes reveal tokens **only** for that street's indices. Before that
moment the board cards are ElGamal ciphertexts under the aggregate key and no proper subset of
players can open them (T7d/T7e). A player who publishes a token for a future board index early is
committing a detectable protocol violation — our layer must reject such messages and attribute the
violation, since ziffle has no concept of "too early".

**Deserialisation hygiene.** Always deserialise wire objects with `Validate::Yes` so arkworks runs
its point checks. secp256k1 has cofactor 1, so small-subgroup attacks are not a concern here, but
on-curve validation still is. **Verification: (a) compiled** — T5 uses
`deserialize_with_mode(..., Compress::Yes, Validate::Yes)` successfully.

---

## 8. The disconnect problem (spec §19)

Barnett–Smart is `n`-of-`n`. **Any** player who stops responding makes it impossible to open any
further card — including the board — so the hand cannot complete. ziffle does nothing about this and
neither does the underlying construction.

Spec §19 anticipated exactly this and gives the right answer, which I endorse without modification:
define `timeout`, `hand abort`, `evidence of which peer failed`, and `reputation penalty`. The
transcript makes attribution sound — the missing `REVEAL_TOKEN` for a given `(hand_id, card_index)`
is publicly visible, and everyone else's signed events prove they did their part.

**The tempting fix that must be refused.** The standard robustness upgrade is `t`-of-`n` threshold
ElGamal (Feldman/Pedersen VSS), so a quorum can finish the hand without the missing player. Do
**not** do this: with `t < n`, any `t` colluding players can decrypt *every* hole card at the table.
That directly violates spec §35's main invariant and §19's own instruction never to trade
disconnect-robustness for early decryption. Keep `n`-of-`n`; abort and attribute. Security over
convenience, exactly as §19 says.

This is a genuine, permanent limitation of the recommended design and belongs in `THREAT_MODEL.md`:
**a malicious player can always force a hand to abort by going silent.** It cannot steal cards or
money by doing so, but it is a griefing vector, and the only mitigations are social (reputation,
disconnect penalties), not cryptographic.

---

## 9. Confidence, and what I am not sure about (spec §36)

Spec §36 requires that where I am not cryptographically certain, I stop and document rather than
improvise. Doing that now.

**Confident (high):**

* Barnett–Smart + Bayer–Groth is the correct construction for this problem. It is the best-analysed
  option and it satisfies every property in §6. — *literature plus the property mapping in §2.1.*
* Performance is a non-issue. ~100 ms prove / ~42 ms verify / 5.5 KB, measured, on the slowest of the
  three candidate groups. — *(a) compiled.*
* Cut-and-choose is the wrong trade, and rolling our own shuffle argument is forbidden by §6/§36
  anyway. — *(a) compiled, §5.3.*
* The `n`-of-`n` privacy property holds in the implementation: one share does not open a card. —
  *(a) compiled, T7d/T7e.*
* Tampered decks are rejected. — *(a) compiled, T4/T6b.*
* ziffle's Pedersen generators are soundly derived (unknown discrete logs). — *(b) source.*
* ristretto255 is the faster and better-audited group; secp256k1 is acceptable but ~3.4× slower. —
  *(a) compiled.*

**Not confident (these are the real risks):**

1. **That ziffle's Bayer–Groth implementation is *sound*, in the formal sense.** I verified it
   *rejects 13 specific attacks*. That is emphatically not the same as soundness. A subtly wrong
   exponent or a missing check can leave a proof system that accepts all honest proofs and rejects
   all naive attacks while still admitting a clever forgery. Nothing I did rules that out.
   *What would settle it:* a line-by-line review of `src/lib.rs` against the Bayer–Groth paper by
   someone who knows the paper, focused on `MultiExpArg` and `SingleValueProductArg`. 1779 lines
   makes this a few days of work, not a research project. This review is a **prerequisite**, not a
   nice-to-have.

2. **The forked Fiat–Shamir transcript.** `ShuffleProof::new` clones the transcript and derives the
   multi-exponentiation and product arguments from the *same* state, separated only by different
   domain tags (`ziffle/BG12MultiExpArgX/v1` vs `ziffle/BG12ProductArgX/v1`). The author flags it in
   a comment: *"NOTE: The transcript is forked here."* Distinct domain separation tags are the
   standard defence and this is probably fine, but transcript forking is exactly where weak
   Fiat–Shamir bugs (the "Frozen Heart" class) live, and a bespoke SHA-256 transcript rather than
   merlin raises the stakes. *What would settle it:* the same review, plus a deliberate attempt to
   produce a proof valid under one sub-argument's challenges and invalid under the other's.

3. **Whether `ctx` binding is sufficient for cross-hand and cross-table replay.** ziffle binds every
   proof to a caller-supplied `ctx` byte string, and T1/T3 confirm proofs do not transfer across
   different `ctx` values. But that is only as strong as what *we* put in `ctx`. It must include at
   minimum `protocol_version || table_id || hand_id || shuffle_round || shuffler_peer_id`.
   *What would settle it:* an adversarial test in our own suite that replays a valid shuffle proof
   from hand `n` into hand `n+1` and asserts rejection. This is our bug to make, not ziffle's.

4. **Side channels.** I measured throughput, not constant-time behaviour. arkworks is not written
   with the same constant-time discipline as curve25519-dalek, and a timing side channel on the
   secret permutation or on `sk_i` during share generation would matter for a networked game.
   *What would settle it:* `dudect`-style timing analysis on `shuffle_deck` and `reveal_token`, or a
   decision that remote timing attacks are out of scope in `THREAT_MODEL.md` (defensible for play
   money; not for real money).

5. **`mul_by_cofactor_to_group` / point validation on hostile input.** T5 shows `Validate::Yes`
   round-trips honest data. I did **not** fuzz the deserialisers with malformed input, which spec §27
   requires. *What would settle it:* `cargo-fuzz` over `ShuffleProof`, `MaskedDeck`, `RevealToken`
   and `OwnershipProof` deserialisation. Note the `assert!` panic path in `Transcript` (§4.1).

**Overall confidence: medium.** The *construction* choice is high-confidence and I would defend it
strongly. The *implementation* choice is medium-low and is contingent on risk 1 being retired by
review. That is why the recommendation below is structured to make the implementation swappable.

---

## 10. Concrete recommendation

**Construction.** Barnett–Smart with a Bayer–Groth shuffle argument. `n`-of-`n` threshold ElGamal.
No threshold degradation to `t`-of-`n` (§8).

**Group.** secp256k1, inherited from ziffle. Prime order, cofactor 1. Not my first choice on the
merits — ristretto255 is 3.4× faster and better audited — but the difference is ~60 ms per shuffle
and does not justify forking the library. Record it as the top optimisation if we ever need one.

**Library.** `ziffle = "=0.1.0"`, **vendored into the repo** (`vendor/ziffle/`) with a
`[patch.crates.io]` entry, not merely pinned. Rationale: one release, one author, 3 stars — the crate
could be yanked or the repo deleted, and a vendored copy is also what makes the §9 review meaningful
and auditable by others. Commit `Cargo.lock`. Record it in the dependency register per spec §28 as:
name `ziffle`, version 0.1.0, purpose "Barnett–Smart mental poker with Bayer–Groth shuffle proofs",
repo `github.com/v26-solutions/ziffle`, licence MIT OR Apache-2.0, security status **unaudited,
vendored, reviewed in-house**.

**Isolation.** Put ziffle behind our own trait in `src/mental_poker/`, e.g.

```rust
pub trait DeckCrypto {
    type SecretKey; type PublicKey; type MaskedDeck; type ShuffleProof;
    type RevealToken; type RevealProof;
    fn keygen(&self, rng: &mut impl Rng, ctx: &Ctx) -> (Self::SecretKey, Self::PublicKey, KeyProof);
    fn shuffle(&self, ..., ctx: &Ctx) -> (Self::MaskedDeck, Self::ShuffleProof);
    fn verify_shuffle(&self, ..., ctx: &Ctx) -> Result<VerifiedDeck, ShuffleError>;
    fn reveal_token(&self, ..., ctx: &Ctx) -> (Self::RevealToken, Self::RevealProof);
}
```

so that swapping to `barnett-smart-card-protocol` (§4.8) or a future audited crate is a
single-module change. Given risk 1 in §9, this is not speculative generality — it is the mitigation.

**What we take from the library:** ElGamal masking/re-masking, the Bayer–Groth shuffle argument,
Schnorr key-ownership proofs, Chaum–Pedersen DLEQ reveal-token proofs, aggregate key and aggregate
token construction, canonical point serialisation.

**What we build ourselves** (none of it novel cryptography, all of it standard engineering, so §6 and
§36 are satisfied):

* the `ctx` construction — `protocol_version || table_id || hand_id || round || shuffler` (risk 3);
* the RNG commit/reveal beacon for seating and button (§6), a plain hash commitment;
* the deterministic index→recipient dealing map, fixed before the shuffle (§6);
* street gating for board reveals, and rejection of early tokens (§7);
* the Ed25519-signed, deterministic-CBOR event envelope and hash-chain transcript (spec §12, §13);
* timeout / abort / attribution / reputation for disconnects (§8);
* the adversarial suite (spec §25) — my 13 probes in `probe-ziffle/src/bin/adv.rs` are a starting
  point and should be ported into `tests/adversarial/` as real regression tests;
* worker-thread offload so the ~100 ms proofs never touch the GUI loop (spec §33).

**Gate before any real-money use.** The §9 risk 1 review must happen before this is used for
anything but play money — which is exactly the scope spec §5 sets for the first version, and exactly
what ziffle's own README asks. If the review finds problems that cannot be fixed cheaply, fall back
to `barnett-smart-card-protocol` with all three of its repos vendored (§4.8); the trait boundary
above makes that a contained change.
