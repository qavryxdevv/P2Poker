# What else exists, and what the exit costs

Author: `zr-alternatives` agent (ziffle review workflow)
Date: 2026-08-28
Companion to: `docs/research/MENTAL_POKER.md` (§4, §5.3, §9, §10)

`MENTAL_POKER.md` recommended putting `ziffle` behind a `DeckCrypto` trait
precisely so it could be replaced, and named a line-by-line review of
`src/lib.rs` as a prerequisite. This document answers the other half of that
plan: **what replacing `ziffle` would cost, and whether anything better has
appeared.**

---

## 0. The answer up front

Something better has appeared, and it did not exist when `MENTAL_POKER.md` was
written.

**`github.com/paritytech/mental-poker`** is Jeff Burdges' (Web3 Foundation /
Parity) fork of the `geometryxyz/mental-poker` line, modernised to arkworks
0.5.0 and Rust edition 2024, with all three repositories collapsed into one, and
the bespoke Fiat–Shamir transcript replaced by `ark-transcript` (3.08 M
downloads, the transcript used across Polkadot's crypto). §4.8 of
`MENTAL_POKER.md` rejected `geometryxyz` on three blockers. **All three are
gone**, and I verified that by building it.

| §4.8 blocker | Status today | Evidence |
| --- | --- | --- |
| unpublished git dep on a stale org via SSH, needs a patched manifest | Gone — one repo, HTTPS, `rev`-pinnable | built a probe with `cards-protocol = { git = "https://github.com/paritytech/mental-poker", rev = "e05744b4…" }`; clean build, ran |
| arkworks 0.3.0 → two incompatible arkworks trees | Gone — arkworks **0.5.0**, same as `ziffle` | `Cargo.toml` `[workspace.dependencies]`; adding it to the project's real lock adds **6 packages**, no arkworks split |
| ~6600 lines to review | Still large: 5143 (`proofs`) + 2621 (`protocol`) | `find -name '*.rs' | xargs wc -l` |

And on the same curve as `ziffle` (secp256k1, arkworks 0.5.0) it is **faster and
smaller on every axis**, measured on this machine in this session:

| 52-card deck, secp256k1, release, single-threaded | `ziffle` 0.1.0 | `paritytech/mental-poker` |
| --- | ---: | ---: |
| shuffle **prove** | 94–98 ms | **84.6 ms** |
| shuffle **verify** | 37–42 ms | **28.8 ms** |
| shuffle proof bytes | 5 547 | **4 172** |
| full hand, 6-handed, one node's own crypto | ~414 ms | **~291 ms** |
| Bayer–Groth instantiation | `m = 1` (linear) | `m = 4, n = 13` (the real √N version) |
| Fiat–Shamir transcript | bespoke SHA-256 chain, **forked** between sub-arguments | `ark-transcript`, **one sequential transcript, no fork** |
| public parameters | derived from hardcoded seeds | derived from a public label **and the repo contains a test that regenerates them** — I ran it and diffed: byte-identical |

The last two rows are the ones that matter for this workflow. `MENTAL_POKER.md`
§9 risk 2 is the forked transcript. **The parity implementation does not fork
it**: `shuffle/prover.rs` threads one `&mut Transcript` through the product
argument and then the multi-exponentiation argument in sequence. That entire
risk category simply does not arise there.

**But it is not a free swap, and it is not obviously the right one.** It is
unpublished on crates.io, untagged, unreleased, two contributors, zero stars,
zero external users, and it moved organisation at least twice already (`w3f` →
`paritytech`, with in-source comments pointing at a `peer3to/mental-poker` that
404s). Swapping to it **triples the code that still needs a line-by-line
review** — 7 764 lines instead of 1 779 — and that review has not been done
either. It trades a small unreviewed thing for a bigger, better-shaped
unreviewed thing.

My recommendation is therefore split:

1. **Do not swap now.** Finish the `ziffle` review that this workflow exists to
   produce. The trait boundary is what makes waiting safe.
2. **Vendor `ziffle` immediately** — it is nine files, 141 KB, MIT OR
   Apache-2.0, and I verified the whole procedure end to end. There is no reason
   to defer this; see §7.
3. **If the review finds `ziffle` unsound, go to `paritytech/mental-poker`, not
   to cut-and-choose.** The adapter is contained (§4.5) and I proved both
   libraries link into one binary alongside the project's existing dalek/sha2/
   blake3 stack, so a differential test between them during migration is
   possible.
4. **The cut-and-choose fallback is worse than `MENTAL_POKER.md` §5.3
   concluded**, for a reason that section missed. See §6 — this is the most
   important correction in this document.

---

## 1. Method and environment

Same machine and toolchain as `MENTAL_POKER.md`: Windows 10,
x86_64-pc-windows-msvc, 24 cores, **rustc 1.95.0 / cargo 1.95.0**, `--release`,
single-threaded timings, builds held to 19 cores.

Everything below is either **(a) compiled** — I built and ran it — or
**(b) source** — I read the actual code or manifest. Registry metadata came from
the crates.io and GitHub REST APIs with a `User-Agent` header (without one
crates.io returns 403).

Probe directories, all outside the repo, under
`…/scratchpad/zr-alternatives/` and `~/AppData/Local/Temp/`:

```
zr-alternatives/parity-mp/           clone of paritytech/mental-poker + my protocol/examples/bench52.rs
zr-alternatives/probe-cutchoose/     a complete non-interactive cut-and-choose shuffle argument (mine)
zr-alternatives/vendor-test/         the vendoring procedure, executed
Temp/zt/                             ziffle unpacked at a short path, own test suite
Temp/gitdep/                         git-dependency resolution probe
Temp/coexist/                        the project's real Cargo.toml + cards-protocol, lock resolution
Temp/coexist2/                       ziffle + cards-protocol + the project's crypto stack in one binary
Temp/cand/pkmental, Temp/cand/pkcore an independent cut-and-choose implementation, built and benchmarked
```

The `ziffle` baseline was **re-measured in this session**, not copied from
`MENTAL_POKER.md`, so the head-to-head table is like-for-like:

```
=== ziffle 0.1.0, 52-card deck, secp256k1 (arkworks) ===
  proof bytes: shuffle=5547 deck=3432
  shuffle prove: init 94.7 ms, chained ["98.1", "98.4", "99.0", "98.2", "98.7"]
  shuffle verify: init 36.6 ms, chained ["41.6", "41.7", "42.0", "41.7", "42.2"]
  6 players TOTAL ~ 413.8 ms
```

**Verification: (a) compiled** — `probe-ziffle`, unchanged from Phase 0, re-run.

---

## 2. The field as it stands today

### 2.1 crates.io

Ten keyword searches (`mental poker`, `verifiable shuffle`, `shuffle proof`,
`elgamal shuffle`, `bayer groth`, `mixnet`, `threshold elgamal`, `sigma
protocol`, `permutation argument`, `trustless card game`, …). **Nothing new
has been published.** The relevant set is exactly the set `MENTAL_POKER.md` §4
already judged:

| crate | max ver | licence | last release | downloads | verdict |
| --- | --- | --- | --- | ---: | --- |
| `ziffle` | 0.1.0 | MIT OR Apache-2.0 | 2025-11-01 | 295 | incumbent |
| `mental-poker` | 0.1.0 | — | 2022-02-25 | 1 636 | dead, nightly-only, no crypto |
| `zshuffle` | 0.1.2 | GPL-3.0-only | 2024-06-03 | 4 772 | 13 forked arkworks crates, trusted setup |
| `distributed-cards` | 0.5.2 | LGPL-3.0 | 2022-07-06 | 14 150 | no shuffle proof at all |
| `curdleproofs` | 0.0.1 | MIT | 2022-09-14 | 7 855 | arkworks 0.3.0, large-N, shuffle only |
| `sra-wasm` | 0.1.0 | GPL-3.0 | 2023-11-04 | 5 040 | SRA, no verifiable shuffle |
| `pokerproof` | 0.1.0 | MIT | 2026-02-10 | 28 | commit–reveal "provably fair", wrong trust model |

`ziffle` itself is **unchanged**: still 0.1.0, still the only release, still one
owner (`chris-ricketts`), repo last pushed **2025-11-02**, 8 commits, 3 stars, 1
contributor. Downloads moved 293 → 295 since Phase 0, which is our own probes.
**Verification: (b) source** — crates.io `/api/v1/crates/ziffle`,
`/owners`, and the GitHub commits/contributors endpoints.

**One genuinely new datum: `ziffle` now has a downstream consumer.**
`rainyflash/token-poker` (`crates/token-holdem-mental-poker/Cargo.toml`, dep
`ziffle.workspace = true`, workspace pin `ziffle = "0.1"`) is a peer-to-peer
Hold'em client over libp2p — architecturally almost our project. Before this
turns into reassurance: the repo was **created 2026-08-22**, is **five days
old**, has one contributor, zero stars, eighteen releases in five days, and its
own README says its cryptography "has not received an independent production
audit". Its CI is the bulk of `ziffle`'s 205 recent downloads. It is a second
unaudited user, not evidence of soundness. It is, however, a second party with a
direct interest in the review this workflow is producing.
**Verification: (b) source** — raw manifests from GitHub; repo metadata.

New-but-irrelevant crates the searches turned up, listed so nobody re-searches
them: `cryptego{,-core,-backend,-commit}` (P2P Stratego, hash commitments, no
shuffle), `zerosum` (commit–reveal dice), `vrand-vrf-core` (ECVRF fairness
beacon), `chaum-pedersen-zkp`, `si-zero-knowledge`, `sigma-proofs`,
`sigma-compiler` (sigma toolkits, no shuffle), `vhe` (verifiable ElGamal ops, no
shuffle).

### 2.2 The best-maintained *partial* component

**`elastic-elgamal` 0.4.0-beta.1** (2026-07-19) deserves naming because it is
the only crate in this space with real maintenance signals: 7 releases over 5
years, 6 contributors, dependabot, CI, `no_std` tested, MIT OR Apache-2.0,
`rust-version = 1.85`, 15 384 downloads.

It does not help. Its feature list is ElGamal, zero/Boolean-encryption proofs,
range proofs, ciphertext↔Pedersen equivalence, m-of-n choice encryption,
quadratic voting, and **threshold ElGamal via Feldman VSS with verifiable
distributed decryption** — but **no verifiable shuffle**. Its threshold is
`t`-of-`n`, which spec §35 and `MENTAL_POKER.md` §8 forbid for hole cards. So it
could supply the reveal half of Barnett–Smart and nothing else, leaving the
shuffle argument to be built by hand — i.e. it collapses into the cut-and-choose
fallback of §6. Its own README carries the same warning as `ziffle`'s: not
independently verified.
**Verification: (b) source** — crates.io metadata, GitHub contributors, README.

### 2.3 GitHub

| repo | stars | contributors | pushed | licence | verdict |
| --- | ---: | ---: | --- | --- | --- |
| **`paritytech/mental-poker`** | 0 | **2** (burdges 49, coax1d 5) | 2026-08-19 | MIT OR Apache-2.0 (per-crate) | **the candidate** — §4 |
| `geometryxyz/mental-poker` | 122 | 4 | 2025-01-29 | MIT/Apache | superseded by the above |
| `ImperialBower/pkmental` | 0 | 1 | 2026-08-23 | MIT OR Apache-2.0 | **cut-and-choose**, see §6.5 |
| `CisaSettle/bluffking` | 0 | 1 | 2026-08-25 | **AGPL-3.0-only** | licence blocker |
| `rainyflash/token-poker` | 0 | 1 | 2026-08-27 | Apache-2.0 | *uses ziffle*; not a library |
| `Imperfect-Protocol/crumble` | 5 | 1 | 2026-03-14 | **"All Rights Reserved"** | licence blocker + novel construction |
| `gear-foundation/zk-mental-poker` | 1 | 2 | 2026-01-16 | none declared | Groth16 circuits + Gear smart contracts |
| `wawwior/bytecards` | 0 | 1 | 2025-10-13 | GPL-3.0 | SRA-shaped, no ZK |
| `yuxi16/Verifiable-shuffle` | 0 | 1 | 2026-08-04 | MPL-2.0 | dusk-plonk + KZG, needs an SRS |
| `asn-d6/curdleproofs` | 70 | — | 2023-10-16 | none | as §4.5 of MENTAL_POKER.md |
| `akonradi/mental-poker` | 7 | 1 | 2023-11-18 | none | dead |

Rejection reasons in full in §5.

**No implementation in this space has an audit.** I searched for one and found
none. The strongest external assurance artefact that exists is not an audit of
any implementation — it is §2.4.

### 2.4 Literature: the machine-checked Bayer–Groth verifier

Haines, Goré and Tiwari, *"Machine-checking Multi-Round Proofs of Shuffle:
Terelius-Wikström and Bayer-Groth"*, USENIX Security 2023 (ePrint 2025/461),
machine-checked the Bayer–Groth proof of shuffle in Coq and **extracted a
verifier**, which they ran against Swiss Post e-voting transcripts. The
artefact is `github.com/gerlion/secure-e-voting-with-coq` (a `BayerGroth/`
directory, 2.9 MB of Coq, 3.3 MB of extracted OCaml; 9 stars, last pushed
2023-01-23, **no licence declared**).

This is not a drop-in oracle for us — it is OCaml, it speaks e-voting transcript
formats, and it carries no licence — but it is the single most useful thing the
line-by-line reviewer can have: a machine-checked statement of exactly which
equations a Bayer–Groth verifier must check. **A reviewer working against the
paper alone should also work against this formalisation.** I am flagging it here
because it is the highest-value external artefact I found and the review
workflow should know it exists.

Also worth knowing, because it bears on how much comfort "Bayer–Groth is
well-studied" is worth: a soundness-proof issue in Bayer–Groth has been
identified in the course of adapting it to lattices (present in Aranha et al.
CCS 2023 and Hough et al. CiC 2025). That is a statement about the *lattice*
adaptation, not about the group-based construction we use — but it is a reminder
that "well-studied" is not "closed".

Nothing newer than Bayer–Groth is better for a 52-card deck. The recent work is
SNARK-based shuffles (trusted setup, larger codebase, worse proving cost at
N = 52) and lattice mixnets (large-N, immature). `MENTAL_POKER.md` §2 already
reached that conclusion and I did not find anything to overturn it.

Sources for this section:
[USENIX](https://www.usenix.org/conference/usenixsecurity23/presentation/haines) ·
[ePrint 2025/461](https://eprint.iacr.org/2025/461) ·
[secure-e-voting-with-coq](https://github.com/gerlion/secure-e-voting-with-coq) ·
[Geometry HackMD writeup](https://hackmd.io/@nmohnblatt/SJKJfVqzq)

---

## 3. `ziffle` today: has anything changed?

No. For the record, so the review has a fixed target:

* crates.io: 0.1.0 only, published 2025-11-01 by `chris-ricketts`, **not
  yanked**, sole owner, `license = "MIT OR Apache-2.0"`, crate size 26 057 B.
* GitHub `v26-solutions/ziffle`: 3 stars, 0 forks, 0 open issues, 8 commits, 1
  contributor, last push **2025-11-02**, i.e. ten months stale.
* `.crate` sha256 `ba79285194a16b02512566a9a64d885567646045b144bb0efeef662001cd83a5`
  — **identical to the `checksum` line in the project's `Cargo.lock` (line
  6421)**, verified with `sha256sum` against the file in the cargo cache.
* `.cargo_vcs_info.json` pins upstream git `bcb8e61651cd6d4140c895a414d7ed8bc06d32bd`,
  the second-from-top commit. The only later commit is `f24f38e6` "repo: update
  readme with badges" — so the published crate is the current code.
* Transitive dependency tree: **43 packages**.
* Own suite: **16 unit tests + 15 doctests, 0 failures** on rustc 1.95.

**Verification: (a) compiled** and **(b) source**.

---

## 4. The candidate: `paritytech/mental-poker`

### 4.1 What it is

A six-crate workspace, `MIT OR Apache-2.0`, forked from and crediting
`geometryxyz/mental-poker` (Kobi Gurkan and Nicolas Mohnblatt), maintained by
Jeff Burdges (`jeff@web3.foundation` — Web3 Foundation's cryptographer; ring-VRF,
Sassafras, schnorrkel) with Andrew (`coax1d`) as a second committer.

```
proofs/          5 143 lines  the full Bayer–Groth machinery: hadamard_product,
                              matrix_elements_product, multi_exponentiation,
                              single_value_product, zero_value_bilinear_map, shuffle
                              + Pedersen, ElGamal, Schnorr, Chaum–Pedersen DLEQ
protocol/        2 621 lines  Barnett–Smart: keys, masking, remasking, reveal,
                              shuffle, AccumulateShuffles / AccumulateReveals
deck/              280 lines  card ↔ group-element encoding, SortedDeck
deck-secp256k1/    486 lines  @generated static parameters + 200-card deck for secp256k1
play/              709 lines  higher-level play helpers + wasm bindings
ez/                 31 lines  serialization helper
```

Created 2025-10-08 (three weeks before `ziffle` was published), 54 commits, most
recent 2026-08-19, two open issues from this month. No tags, no releases, **not
published on crates.io** (`cards-proofs`, `cards-protocol`, `cards-deck`,
`deck-secp256k1` all return "does not exist").
**Verification: (b) source** — clone at `e05744b4cc431088ec2fda769a73b067b4664893`.

### 4.2 Does it compile today on rustc 1.95?

Yes, cleanly, in 21.4 s, and its suite passes:

```
cargo build --release --workspace -j19
    Finished `release` profile [optimized] target(s) in 21.44s

cargo test --release --workspace -j19
cards-proofs      22 passed; 0 failed   (incl. test_shuffle_argument, test_multi_exp,
                                          test_hadamard_product_argument, test_zero_argument,
                                          test_single_product_argument, test_complete_product_argument)
cards-protocol     7 passed; 0 failed   (incl. test_shuffle with a negative case)
deck-secp256k1     1 passed; 0 failed
```

**Verification: (a) compiled** — output above, verbatim.

A pinned git dependency also resolves and builds from a clean cache over plain
HTTPS, with no manifest surgery and no `CARGO_NET_GIT_FETCH_WITH_CLI` workaround
needed for the fetch itself:

```toml
cards-protocol = { git = "https://github.com/paritytech/mental-poker",
                   rev = "e05744b4cc431088ec2fda769a73b067b4664893" }
```
```
   Compiling cards-proofs v0.1.0 (https://github.com/paritytech/mental-poker?rev=e05744b4…)
   Compiling cards-protocol v0.1.0 (…)
    Finished `release` profile in 22.69s
(4, 13, 0)
```

**Verification: (a) compiled** — `Temp/gitdep`. Note the short-`rev` form is
rejected (`revision e05744b4 not found`); the full 40-hex sha is required.

### 4.3 Measured cost, 52 cards, secp256k1

`protocol/examples/bench52.rs`, written by me against `AggregatedPublicKeys::
shuffle_and_remask` / `verify_shuffle` / `prove_single_reveal_token` / `unmask`,
best of 5, and deliberately mirroring the shape of the `probe-ziffle` numbers:

```
setup(52)                    :    1.203 ms
mid_factor(52)               : (4, 13, 0)          <- m=4, n=13, no padding
mid_factor(47)               : (6, 8, 1)           <- a 47-card reshuffle needs 1 pad

===== 6 players =====
keygen+ownership (all)       :    2.232 ms
shuffle prove  (best of 5)   :   84.125 ms
shuffle verify (best of 5)   :   28.670 ms
  ZKProofShuffle             : 4172 B
  ShuffleMessage (proof+deck+pk+sig): 7710 B
  deck of 52 masked cards    : 3440 B (66.2 B/card)
reveal 1 card: 6 tokens+proofs 1.733 ms, verify+unmask 1.985 ms, total 3.718 ms
  RevealMessage              : 230 B
FULL HAND (own work)         :  290.678 ms  [prove 1 + verify 5 + 17 reveals]
```

Full table against the re-measured `ziffle` baseline:

| | ziffle 2p | parity 2p | ziffle 3p | parity 3p | ziffle 6p | parity 6p |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| keygen + ownership (all) | 1.3 ms | 0.8 ms | 1.2 ms | 1.1 ms | 2.3 ms | 2.2 ms |
| shuffle prove | 94–98 ms | 84.6 ms | 93–98 ms | 84.5 ms | 95–99 ms | 84.1 ms |
| shuffle verify | 37–42 ms | 28.8 ms | 37–42 ms | 28.7 ms | 37–42 ms | 28.7 ms |
| reveal 1 card | 1.40 ms | 1.14 ms | 2.06 ms | 1.71 ms | 4.13 ms | 3.72 ms |
| full hand, own work | 187 ms | **124 ms** | 240 ms | **161 ms** | 414 ms | **291 ms** |
| shuffle proof | 5 547 B | **4 172 B** | — | — | — | — |
| deck | 3 432 B | 3 440 B | | | | |

**Verification: (a) compiled** — both programs, this session, same machine.

The 25 % smaller proof is exactly what §5.2 of `MENTAL_POKER.md` predicted for
`m=4, n=13` on a different curve (it measured 4 120 B on starknet-curve); here
it is confirmed on secp256k1, so the difference is the Bayer–Groth
instantiation, not the curve. **`ziffle`'s `m=1` choice costs 33 % in proof size
and ~45 % in verification time.**

### 4.4 What it fixes that `MENTAL_POKER.md` §9 flagged

**Risk 2, the forked transcript — eliminated by construction.** `ziffle`'s
`ShuffleProof::new` clones the transcript and derives both sub-arguments from
the same state (`ziffle/BG12MultiExpArgX/v1` vs `ziffle/BG12ProductArgX/v1`).
The parity prover does not:

```rust
// proofs/src/zkp/arguments/shuffle/prover.rs
t.append(&b_commits);
let mut tr = t.challenge(b"yz");
let y: Scalar = tr.read_uniform();
let z: Scalar = tr.read_uniform();
…
let product_argument_proof = product_argument_prover.prove(rng, &mut *t)?;
…
let multi_exp_proof = multi_exponentiation::MultiExponentiation::prove(
    rng, &multi_exp_parameters, &multi_exp_statement, &multi_exp_witness, t,
)?;
```

One transcript object `t`, threaded through the product argument and *then* the
multi-exponentiation argument. `y` and `z` come from one `challenge(b"yz")`
reader, which is `ark-transcript`'s idiom, not a fork. There is no `clone()` in
that file. The whole "Frozen Heart" question the review is chartered to
investigate does not arise here.
**Verification: (b) source** — full file read.

**Public parameters are reproducible, and the repo proves it.** `ziffle` derives
its Pedersen generators from hardcoded seeds; sound, but you have to take the
code's word for it. The parity repo ships `deck-secp256k1/src/deck_secp256k1.rs`
(420 lines of `@generated` constants: the 200-card deck, the ElGamal generator,
and the Pedersen commit key) and a test that regenerates the whole file from a
public label:

```rust
let mut rng = ark_transcript::Transcript::new_labeled(b"Peer3 secp256k1 card deck")
    .challenge(b"200 cards");
let deck = SortedDeck::<ark_secp256k1::Affine>::from_rng(200, &mut rng);
…
assert_eq!(deck.0, crate::DECK_SECP256K1.0);
cards_protocol::Parameters::<ark_secp256k1::Projective>::setup(&mut rng, 200)
    .to_code(&mut w, "PARAMS_SECP256K1")
```

The `assert_eq!` covers only the deck. I extended the check: I ran the test,
which writes the regenerated file to `deck-secp256k1/fresh/`, and diffed it
against the shipped `src/` copy. **Byte-identical, all 420 lines, including
`PARAMS_SECP256K1` and the 200-element Pedersen `TMP` table** (`diff` exit 0;
the `fresh/` file's mtime is my test run and `git ls-files` shows it is not
tracked). So the entire common reference string is a verifiable
nothing-up-my-sleeve derivation from a published string. That is a stronger
position than `ziffle`'s, and it is checkable by anyone in one command.
**Verification: (a) compiled** — test run and diff.

*Caveat for the reviewer:* the argument now rests on `ark_transcript::
Transcript::challenge(...)` being a sound public-randomness source and on
arkworks' `Projective::rand` sampling a base-field x-coordinate rather than a
scalar. `MENTAL_POKER.md` §4.1 already checked the second point in
`ark-ec-0.5.0/src/models/short_weierstrass/group.rs`; the first I did not check.

### 4.5 What the swap would actually cost

**Dependency cost: near zero.** Adding `cards-protocol` as a git dep to the
project's *real* `Cargo.toml` and re-resolving its *real* `Cargo.lock`:

```
packages in current lock: 612
packages after adding cards-protocol: 618
```

Six new packages — `ark-poly`, `ark-transcript`, `sha3`, `keccak`,
`generic-array`, `rand 0.8.8` — and **no arkworks version split**: the tree
stays on a single `ark-* 0.5.0`, reused from `ziffle`. This is the single most
important practical fact about the swap and it is the direct opposite of the
`curdleproofs`/`geometryxyz` situation.
**Verification: (a) compiled** — `Temp/coexist`, using a copy of the project's
manifest and lock.

**They coexist in one binary.** I built `ziffle` + `cards-protocol` +
`deck-secp256k1` + `ed25519-dalek 3` + `curve25519-dalek 5` + `sha2 0.11` +
`blake3` + `minicbor` + `getrandom 0.4` together and ran it:

```
ziffle keygen ok: true
parity params size = 200
parity deck[0] bytes = 33
ristretto ok: true
```

**Verification: (a) compiled** — `Temp/coexist2`. This means a migration can run
both implementations side by side and differential-test them, rather than
cutting over blind.

**API cost: a real but bounded adapter.** The two APIs differ in shape:

| | `ziffle` | `cards-protocol` |
| --- | --- | --- |
| deck size | const generic `MaskedDeck<const N: usize>`, `no_std` no-alloc | `Vec<MaskedCard<C>>`, needs `alloc` |
| curve | hardcoded `ark_secp256k1` | generic over `C: CurveGroup`; `deck-secp256k1` supplies the concrete instance |
| parameters | implicit, `Default` | explicit `Parameters<C>` value threaded through every call (`deck_secp256k1::PARAMS` is a `&'static`) |
| aggregate key | `AggregatePublicKey` (a plain point) | `AggregatedPublicKeys<'p, C>` — **borrows the parameters, carries a lifetime** |
| context binding | `ctx: &[u8]` | `impl IntoTranscript` (a `&[u8]` satisfies it, so this maps directly) |
| "verified" typestate | `Verified<T>`, private field, no `Deserialize` — cannot be forged or smuggled off the wire | none; `verify_shuffle` returns `(&PlayerPublicKey, &[MaskedCard])` |
| matrix dims | fixed `m = 1` | `mid_factor(len)`; 52 → (4, 13, 0), 47 → (6, 8, 1) with padding |

Two things are real work rather than typing:

1. **The lifetime on `AggregatedPublicKeys<'p, C>`.** A `DeckCrypto` trait with
   plain associated types cannot hold a borrowing type without GATs. The
   practical fix is for the impl struct to own `Parameters<C>` and store only
   the owned `AggregatePublicKey<C>` (which is just an affine point),
   reconstructing `AggregatedPublicKeys` inside each method from `&self`. That
   is mechanical but it is the shape of the adapter.
2. **`Verified<T>` is `ziffle`'s best API idea and parity does not have it.**
   `MENTAL_POKER.md` §4.1 correctly called it out: it makes "never display an
   unverified card" a compile error. Porting to parity means **re-creating that
   typestate in our own trait layer**, which is where it arguably belonged all
   along. Budget for it explicitly; it is the one place where the swap makes the
   safety property weaker unless we do the work.

Honest estimate: **300–500 lines of adapter, plus re-pointing the 13 adversarial
tests, plus re-establishing the `Verified` typestate — call it 2–4 days.**

**Review cost: this is the real price.** 7 764 lines (`proofs` + `protocol`)
versus `ziffle`'s 1 779. Whoever reviews it reviews roughly **4.4×** as much
code. The mitigating facts are that the code is the `geometryxyz` lineage, which
has had more public exposure (122 stars, a published HackMD design writeup, and
a `zero_value_bilinear_map` / `hadamard_product` decomposition that matches the
paper's section structure directly), and that its transcript handling is
simpler. But it is unaudited too. **Swapping does not retire risk 1 of
`MENTAL_POKER.md` §9; it moves it onto a larger surface.**

### 4.6 Supply-chain risks, stated plainly

* **Not on crates.io, no tags, no releases.** Consumption is by git `rev` or by
  vendoring. There is no checksum in `Cargo.lock` for a git dep beyond the rev.
* **It has already moved organisation twice.** `[workspace.package] homepage`
  says `github.com/w3f/mental-poker` (which now 301-redirects); in-source
  comments in `protocol/src/shuffle.rs` cite
  `https://github.com/peer3to/mental-poker/blob/main/...` (which **404s**); the
  live copy is `paritytech/mental-poker`, mirrored at `coax1d/mental-poker`.
  This is exactly the failure mode that made `geometryxyz` un-pinnable. **If we
  adopt it, we vendor it — a git `rev` dependency is not enough.**
* **Licence hygiene needs one question answered.** Every crate declares
  `license = "MIT OR Apache-2.0"`, and `proofs/` and `protocol/` ship both
  licence texts — but `deck/`, `deck-secp256k1/`, `ez/` and `play/` ship none,
  there is **no LICENSE file at the repo root**, and the README says *"Through
  2025, this crate is licensed under either of…"*. That "Through 2025" phrasing
  is almost certainly about copyright years, but it is the kind of ambiguity to
  resolve with the author (one GitHub issue) before building on it, not after.
* Two contributors is better than one, but "Parity's org" is not "Parity
  supports this": zero stars, zero forks, no CI badge, no release process.

---

## 5. Every other candidate, and why not

**`geometryxyz/mental-poker` (the original).** Superseded. Its manifest still
points at `ssh://git@github.com/geometryresearch/proof-toolbox.git` under the
old org name, and it is still arkworks 0.3.0. Everything it offers,
`paritytech/mental-poker` offers on a current stack in one repository. **Take
the fork, not the original.**

**`Imperfect-Protocol/crumble` — rejected on licence, before anything else.**
BLS12-381 pairing-based mental poker, 5 stars, one author, 13 commits. Its
README states *"Copyright (c) 2026 Sonia Kolasinska / Imperfect Protocol. All
Rights Reserved."* and GitHub reports no licence. **All rights reserved means we
have no right to use it**, so I did not build or evaluate it further. Even with a
licence it would be a novel construction ("out-of-order peeling" via bilinear
pairings) with no paper, no review, and a blockchain "Sovereign Referee" in the
trust model, pinned to `rand_core = "=0.6.4"` and `digest 0.9`. Not a candidate.
**Verification: (b) source** — README and workspace manifest.

**`CisaSettle/bluffking` — rejected on licence.** **AGPL-3.0-only**, and the
README is explicit that the AGPL comes from its `postflop-solver` dependency and
that the site serves this repo as §13 Corresponding Source. Adopting any of it
makes our client AGPL-3.0 with a network-use clause — a decision only the
project owner can make, and one the spec has not made. Technically it is the
most substantial of the new entrants (`mental-poker/src/crypto_real/shuffle.rs`
is 52 KB, plus `dkg.rs`, `decrypt.rs`, an offline verifier, and a wasm surface),
and it describes an "n-of-n re-encryption-mixnet shuffle". Its stated assurance
is *"cross-vendor AI-audited (Claude + OpenAI Codex)"* — which is not an audit,
and this document is not going to treat it as one. **Verification: (b) source**.

**`gear-foundation/zk-mental-poker` — rejected.** Groth16 circuits plus Gear
(Vara) smart contracts and a TypeScript/`package.json` build. Two contributors,
173 commits, active to 2026-01. It is an on-chain application, not a Rust
library: it needs a trusted setup (Groth16 SRS) and a blockchain runtime, both
of which the project exists to avoid. No licence declared.

**`rainyflash/token-poker` — not a candidate, but relevant.** An application,
not a library, and it depends on `ziffle` (§2.1). Five days old.

**`wawwior/bytecards` — rejected.** GPL-3.0, `crypto-bigint`/`crypto-primes`
(SRA-shaped commutative encryption), README is two lines. No verifiable shuffle.

**`yuxi16/Verifiable-shuffle` — rejected.** A research implementation of
"Pairing-Based Verifiable Shuffles with Logarithmic-Size Proofs" on
`dusk-plonk`, MPL-2.0, 0 stars, one author, `cargo check --lib --examples` is
the documented build. KZG means a structured reference string, i.e. a trusted
setup. Interesting literature, not a dependency.

**`akonradi/mental-poker`, `wak31415/mental-poker`,
`Kristofferjoha/Mental-Poker-Math`, `cyotee/fairplay-crypto-godot`,
`NFTconfig/op-poker`** — dead, unlicensed, toy, or Godot-targeted. Triaged and
dropped.

**`curdleproofs`, `zshuffle`, `distributed-cards`, `sra-wasm`, `pokerproof`,
`mental-poker` 0.1.0** — unchanged since `MENTAL_POKER.md` §4; no new releases,
no new commits. Those rejections stand.

---

## 6. The cut-and-choose fallback, measured properly

`MENTAL_POKER.md` §5.3 costed this from a single measured repetition times a
repetition count. I implemented the whole protocol and measured it. Two
conclusions differ from §5.3, one in each direction.

### 6.1 What I built

`probe-cutchoose` is a complete, runnable non-interactive Sako–Kilian
cut-and-choose shuffle argument over ristretto255 (the fastest of the three
candidate groups, so this is the charitable case), 52 ElGamal ciphertexts, with
the two standard size optimisations:

* the `b = 0` branch reveals a **32-byte seed**, from which the verifier
  re-derives the permutation and all 52 masking scalars and rebuilds `D_i`;
* the `b = 1` branch reveals `σ_i` (52 bytes) and 52 scalars, and the verifier
  **reconstructs `D_i` backwards from `D_out`** rather than receiving it — which
  removes the intermediate deck from the wire entirely.

Expected wire size is therefore `t × (32 + 16 + 858) = t × 906` bytes, against
the naive `t × ~5 KB` that §5.3 extrapolated. It passes its negative tests:

```
honest proof against real D_out : true
honest proof against tampered   : false     (D_out with two cards swapped)
proof from another ctx          : false
```

### 6.2 Correction 1 — it is much smaller than §5.3 said

```
one repetition (52 remasks + commit): 3.13 ms

    t     prove ms    verify ms      proof B   proof KB
    1          3.2          3.2          836        0.8
   10         27.3         26.0         9692        9.5
   40        113.3        104.1        37643       36.8
   64        183.2        176.7        58265       56.9
   80        240.3        240.7        69042       67.4
  100        325.1        310.1        90249       88.1
  128        412.3        401.5       116880      114.1
```

(sizes averaged over 24 challenge draws, because the size depends on the
challenge's Hamming weight; times are best-of-24.)

At `t = 40` that is **36.8 KB, not the 195 KB** §5.3 projected — a 5.3×
correction. §5.3's "35–70× larger on the wire" should read "**7× larger at
t = 40**". The conclusion it drew from that number does not survive unchanged,
which is why correction 2 matters so much.

### 6.3 Correction 2 — 40 repetitions do not give 2⁻⁴⁰ soundness

This is the important one, and §5.3 missed it.

`t` repetitions give soundness error `2^-t` **against an interactive verifier who
supplies the challenge**. Under Fiat–Shamir the prover computes the challenge
itself, so it can retry. The retry is cheap, via a pool attack:

> A cheating prover prepares a pool of `M > t` repetitions once — half that can
> only answer bit 0, half that can only answer bit 1 — and then submits ordered
> `t`-tuples drawn from that pool. Each distinct tuple hashes to a fresh
> challenge, there are `M!/(M−t)!` of them, and the marginal cost of an attempt
> is **one hash over the `t` commitments**, on top of a precomputed midstate for
> the fixed `(ctx ‖ D_in ‖ D_out)` prefix. The one-off pool cost amortises to
> nothing.

Measured marginal cost per attempt, and the total work for `2^t` attempts:

```
t= 40: marginal grind attempt =   0.94 us | 2^t attempts = 1.04e6  core-s = 0.033 core-years
t= 64: marginal grind attempt =   1.41 us | 2^t attempts = 2.60e13 core-s = 8.2e5  core-years
t= 80: marginal grind attempt =   1.72 us | 2^t attempts = 2.08e18 core-s = 6.6e10 core-years
t=128: marginal grind attempt =   2.65 us | 2^t attempts = 9.01e32 core-s = 2.9e25 core-years
```

**`t = 40` costs an attacker twelve core-days on this CPU** — under an hour on a
modest GPU, and pocket change on rented cloud. A forged shuffle proof lets a
player add an ace to the deck. Twelve core-days is not a security margin, it is
a weekend.

This is a textbook result — parallel-repetition sigma protocols made
non-interactive need `t ≈ λ` — but it is exactly the sort of thing that gets
lost when a protocol is costed rather than built. The honest parameters are:

| target | reps | prove | verify | proof | verdict |
| --- | ---: | ---: | ---: | ---: | --- |
| "2⁻⁴⁰", **interactive challenge only** | 40 | 113 ms | 104 ms | 36.8 KB | needs an unbiasable beacon and 2 extra round trips per shuffle |
| non-interactive floor | 80 | 240 ms | 241 ms | 67.4 KB | 6.6e10 core-years on general-purpose CPUs — sound against any realistic poker cheat, but see the margin note below |
| matches the 128-bit posture of the rest of the stack | 128 | 412 ms | 402 ms | 114 KB | the number to actually use |

*Note on the `t = 80` margin.* The 6.6e10 core-years figure is general-purpose
CPU time. The grind is pure SHA-256 over ~2.5 KB per attempt (~40 compression
calls at `t = 80`), which is the one workload the world has purpose-built
hardware for. Scaling the measured per-attempt cost to Bitcoin-network-scale
SHA-256 throughput puts `2^80` attempts on the order of half a day. That is far
outside a poker cheat's reach and firmly inside a nation-state's, which is why
`t = 128` — where the same scaling gives ~10^19 seconds — is the number to use
if the shuffle's margin should match the rest of the stack. I did not benchmark
GPU or ASIC hashing; this is an order-of-magnitude scaling of my CPU
measurement, and I am flagging it as such.

One design note that came out of building it, worth recording because it is a
trap: **do not salt the per-repetition commitments.** If `C_i = H(D_i ‖ salt_i)`
rather than `H(D_i)`, the attacker can vary `salt_i` without touching `D_i`, and
the grind gets cheaper still while the commitment gains nothing (an intermediate
deck of 52 fresh ciphertexts is already high-entropy, so `H(D_i)` is hiding).

### 6.4 Does it fit the deadline budget?

The shuffle chain is sequential — player `i+1` cannot start until player `i`'s
output exists — so, with 100 ms RTT and verifications overlapping across peers:

```
wall ≈ n × (prove + 50 ms one-way) + (n−1) × verify
```

Six-handed, plus the per-hand proof bandwidth (`n × (proof + deck)`):

| construction | wall clock, 6 players | proof traffic per hand |
| --- | ---: | ---: |
| Bayer–Groth, `paritytech/mental-poker` | **0.96 s** | **45 KB** |
| Bayer–Groth, `ziffle` | **1.10 s** | 53 KB |
| cut-and-choose, `t = 40` + beacon round trips (interactive) | ~2.7 s | 241 KB |
| cut-and-choose, `t = 80` | ~2.9 s | 425 KB |
| cut-and-choose, `t = 128` | **~4.8 s** | **705 KB** |

**No.** At the only parameter that is actually sound non-interactively
(`t = 128`), a six-handed hand takes ~4.8 s of pure crypto-and-propagation
before any UI, against a budget of "a couple of seconds", and pushes 705 KB of
proofs per hand through GossipSub — hostile to the Circuit-Relay-v2 fallback
that spec §1 requires for CGNAT players. At `t = 80` it is ~2.9 s and 425 KB:
over budget on time and badly over on bandwidth.

The interactive variant at `t = 40` is the only version that is both sound and
near-affordable, and it buys that by adding an unbiasable challenge beacon —
which means a commit–reveal round among all players between every shuffle and
its proof, two extra round trips per shuffle per player, plus a new class of
failure (a player who will not contribute to the beacon) and a transcript that
an offline auditor can only replay if the beacon is recorded in it. That is a
lot of new protocol to review in exchange for avoiding a library review.

**§5.3's verdict stands, on firmer ground than it had.** Cut-and-choose is a
genuine emergency fallback — it is simple enough to review completely, and I
reviewed my own implementation of it in an afternoon — but it is not a
substitute for Bayer–Groth, and if it is ever used it must be at `t = 128`, with
the latency and bandwidth consequences accepted explicitly.

### 6.5 An independent implementation, and what it gets wrong

`ImperialBower/pkmental` (Apache-2.0/MIT, 3 222 lines, arkworks 0.6, ark-pallas,
edition 2024, one author, pushed 2026-08-23) independently arrived at almost
exactly the architecture `MENTAL_POKER.md` §10 recommends: a `CardCrypto` trait
seam with a real backend and a plaintext mock, `Binding`-scoped context, and a
`VerifiedToken` typestate whose doc comment says *"'I forgot to verify' is a
compile error rather than a silent corruption"* — the same idea as `ziffle`'s
`Verified<T>`. It builds and passes **91 tests** on rustc 1.95 (needs `pkcore`
cloned as a sibling; neither `pkmental` nor its path dep is on crates.io).

Its shuffle is Sako–Kilian cut-and-choose. I benchmarked it:

```
pkmental cut-and-choose, ROUNDS=40, ark-pallas, 52 cards:
  prove  (best of 5): 403.1 ms
  verify (best of 5): 370.8 ms
  proof wire size   : 221456 B (216.3 KB)
```

**Verification: (a) compiled** — my `examples/bench52.rs` against its public
`CardCrypto` + `Wire` API.

Two things follow.

First, **216 KB vindicates `MENTAL_POKER.md` §5.3's 195 KB extrapolation** for
the naive shape: `pkmental` sends all 40 intermediate decks in full
(`ShuffleProof { intermediates: Vec<Vec<MaskedCard>>, responses }`). My 36.8 KB
at the same `t` is what the two size optimisations buy. So §5.3 was right about
the construction it costed and I was right about the one it did not.

Second, and this is why it belongs in this document: **`ROUNDS = 40` with a
Fiat–Shamir challenge is the parameter §6.3 shows is grindable.** Its own module
doc says *"`ROUNDS` rounds drive the soundness error to `2^-ROUNDS`"* — true
interactively, not true for the non-interactive protocol it actually implements
(`fiat_shamir_bits(b"pkmental/shuffle", ctx, &transcript_points(...), ROUNDS)`).
By §6.3's measurement that is ~12 core-days of grinding to forge a shuffle.

I am reporting this as an observation from reading a third party's public source
in the course of costing our own fallback, not as an audit of their project, and
I have not attempted the attack. But it is the single best argument in this
document for a claim `MENTAL_POKER.md` makes and this workflow embodies:
**hand-rolling the shuffle argument is where the bugs are, and "simple enough to
review completely" does not mean "hard to get wrong".** An independent, careful,
well-documented, well-tested implementation by someone building the same thing we
are picked the wrong parameter, and its 91 passing tests did not notice, because
no test can.

---

## 7. Vendoring

`MENTAL_POKER.md` §10 recommends vendoring `ziffle` into the repo rather than
depending on it, because a one-release single-author crate can be yanked. I
executed the whole procedure. It works, it is small, and the licence permits it.

### 7.1 Does the licence permit it?

**Yes.** `Cargo.toml` declares `license = "MIT OR Apache-2.0"`, the package
ships both licence texts (`include = ["src/**/*", "LICENSE-*", "README.md"]`),
and the README repeats the dual grant with the standard Rust-project wording.
Both licences permit redistribution of source, modified or not, so vendoring —
including patching — is squarely within the grant. Under Apache-2.0 §4 the
obligations are: keep the licence, keep the notices, and mark modified files as
changed.

**One defect to know about.** The shipped `LICENSE-MIT` reads:

```
Copyright (c) 2015 The cargo-readme Developers
```

That is a copy-paste artefact — the MIT text names the wrong copyright holder,
who does not own `ziffle`. I confirmed it is not a packaging accident: the same
file is in the upstream repo at
`raw.githubusercontent.com/v26-solutions/ziffle/master/LICENSE-MIT`. The
`LICENSE-APACHE` file is the unmodified Apache-2.0 boilerplate with the appendix
placeholder left blank, which is normal and correct.

Practical consequence: **rely on the Apache-2.0 branch of the dual licence.**
The SPDX expression in `Cargo.toml` is a valid grant from the publisher either
way, and the Apache-2.0 text has no defect. Record the choice in the dependency
register and, if anyone wants it tidy, open an upstream issue. This does not
block vendoring.

### 7.2 The procedure, executed

```
1. cp -r ~/.cargo/registry/src/index.crates.io-…/ziffle-0.1.0  vendor/ziffle
2. rm vendor/ziffle/.cargo-ok                     # cargo's own extraction marker
3. Cargo.toml:
     [dependencies]
     ziffle = "=0.1.0"                            # unchanged
     [patch.crates-io]
     ziffle = { path = "vendor/ziffle" }
4. cargo build   # re-resolves; commit the new Cargo.lock
```

What lands in the repo: **9 files, 141 KB.**

```
.cargo_vcs_info.json   Cargo.lock   Cargo.toml   Cargo.toml.orig
LICENSE-APACHE         LICENSE-MIT  README.md
src/lib.rs (1779)      src/test.rs (736)
```

Note `src/test.rs` is inside `include = ["src/**/*"]`, so **the vendored copy
carries the crate's own 736-line test suite**. That is exactly what makes the
vendored copy reviewable and re-verifiable.

Verified:

```
Compiling ziffle v0.1.0 (…/vendor-test/vendor/ziffle)
Finished `release` profile in 27.16s
vendored ziffle ok: ownership proof verifies = true
```

and the vendored copy still passes its own suite unchanged:

```
running 16 tests   test result: ok. 16 passed; 0 failed
Doc-tests ziffle
running 15 tests   test result: ok. 15 passed; 0 failed
```

**Verification: (a) compiled** — `zr-alternatives/vendor-test` and `Temp/zt`.

### 7.3 The one thing vendoring costs, and how to pay it

**`Cargo.lock` loses the crates.io provenance.** After `[patch.crates-io]`, the
lock entry becomes a path package:

```toml
[[package]]
name = "ziffle"
version = "0.1.0"
dependencies = [ "ark-ec", "ark-ff", … ]
```

— no `source`, no `checksum`. The `ba79285194…` line that ties the source to
crates.io today is gone. Record it manually instead. Put a
`vendor/ziffle/PROVENANCE.md` next to the code with:

```
Source     : crates.io, ziffle 0.1.0, published 2025-11-01 by chris-ricketts
.crate sha256 : ba79285194a16b02512566a9a64d885567646045b144bb0efeef662001cd83a5
             (verified equal to the Cargo.lock checksum on 2026-08-28)
Upstream git  : github.com/v26-solutions/ziffle @ bcb8e61651cd6d4140c895a414d7ed8bc06d32bd
             (from the package's .cargo_vcs_info.json)
Licence    : MIT OR Apache-2.0 — we rely on Apache-2.0; see ZIFFLE_ALTERNATIVES.md §7.1
Modified   : no  /  yes, see CHANGES below (Apache-2.0 §4(b) requires marking modified files)
Review     : docs/research/<the review this workflow produces>
```

I verified the sha256 and the git sha rather than copying them: `sha256sum` on
`~/.cargo/registry/cache/index.crates.io-…/ziffle-0.1.0.crate` matches
`Cargo.lock` line 6421 exactly, and `.cargo_vcs_info.json` names `bcb8e616`,
which is a real commit in the upstream repo.

Two smaller notes:

* `[patch.crates-io]` vs a plain `ziffle = { path = "vendor/ziffle" }`: both
  work. Prefer the patch form — it keeps `version = "=0.1.0"` in
  `[dependencies]`, documents which upstream version the vendored tree *is*, and
  makes going back to the registry a one-line deletion.
* Windows path length: `cargo test` inside the vendored copy failed with
  `LINK : fatal error LNK1104` when the crate sat at the bottom of my deep
  scratch path, and succeeded immediately at a short path. `vendor/ziffle` inside
  the repo is short, so this will not bite — but it explains the failure if
  anyone reproduces my probes.

### 7.4 If we ever adopt `paritytech/mental-poker`, vendor it too

Same argument, more force: it is not on crates.io at all, it has no tags, and it
has already moved organisation twice with a dangling `peer3to` reference in its
own comments (§4.6). Vendoring it means copying four crates (`proofs`,
`protocol`, `deck`, `deck-secp256k1`; `ez` is 31 lines and is a dependency of
none of them we need; `play` is optional) — about 8 530 lines and ~280 KB — plus
adding `LICENSE-APACHE`/`LICENSE-MIT` to the two crates that ship none, and
recording the rev `e05744b4cc431088ec2fda769a73b067b4664893`. The vendored tree
should keep `deck-secp256k1`'s regeneration test, since that test is what makes
the common reference string checkable (§4.4).

---

## 8. Recommendation

| question | answer |
| --- | --- |
| Has anything better appeared? | **Yes** — `paritytech/mental-poker`. Faster, smaller proofs, no transcript fork, reproducible CRS, two maintainers, current arkworks. |
| Is anything audited? | **No.** Nothing in this space is. The nearest thing is a Coq formalisation of the *construction* (§2.4), not of any implementation. |
| Is anything in production use? | **No.** `ziffle`'s one downstream consumer is five days old. |
| Should we swap now? | **No.** Finish the `ziffle` review first. Swapping trades 1 779 unreviewed lines for 7 764 unreviewed lines. |
| Should we vendor now? | **Yes, immediately.** §7 — nine files, licence permits it, procedure verified, no downside. |
| If the review fails `ziffle`, where do we go? | **`paritytech/mental-poker`, vendored.** Adapter is 300–500 lines + restoring the `Verified` typestate; 2–4 days; +6 packages; the two can run side by side for differential testing. |
| Is cut-and-choose a viable fallback? | **Only as an emergency.** At the sound non-interactive parameter (t = 128) it is 4.8 s and 705 KB per six-handed hand. Over budget. |
| What should change in `MENTAL_POKER.md`? | §5.3's proof sizes (36.8 KB not 195 KB at t = 40, §6.2) **and** its repetition counts (t = 40 is grindable in ~12 core-days, §6.3). The second correction is the one that matters. |

Concretely, in order:

1. **Vendor `ziffle` now** (§7), with `PROVENANCE.md`, before the review starts —
   so the review has a stable, committed, auditable target rather than a
   registry cache.
2. **Give the reviewer `gerlion/secure-e-voting-with-coq`** alongside the paper
   (§2.4). A machine-checked statement of the verifier's obligations is a better
   checklist than the paper's prose.
3. **Add `paritytech/mental-poker` to the dependency register as the designated
   fallback**, with the rev pinned in writing, and ask its author the two
   licence-hygiene questions from §4.6 now rather than in a crisis.
4. **Keep the `DeckCrypto` trait boundary honest.** Everything in this document
   is only cheap because that boundary exists. In particular, put the `Verified`
   typestate in *our* layer, not in the library's, so it survives a swap.
5. **Do not build the cut-and-choose fallback speculatively.** If it is ever
   needed, it is `t = 128`, unsalted commitments, and an explicit acceptance of
   ~4.8 s and ~705 KB per hand.

---

## 9. What I did not check

Stated plainly, per spec §36.

* **I did not review `paritytech/mental-poker` for correctness.** I built it, ran
  its 30 tests, benchmarked it, read `shuffle/prover.rs`, `shuffle/mod.rs`,
  `setup.rs` and the Pedersen/ElGamal setup paths, and verified the CRS
  regeneration. That is enough to say the transcript is not forked and the
  parameters are nothing-up-my-sleeve. **It is not enough to say the Bayer–Groth
  algebra in its 5 143 lines is right.** Nobody has said that about any of these
  implementations.
* **I did not attempt a forgery** against `ziffle`, against
  `paritytech/mental-poker`, or against `pkmental`. §6.5's claim about
  `ROUNDS = 40` is an argument from the measured cost of a grinding attempt, not
  a demonstrated forgery.
* **I did not verify `ark-transcript`'s `challenge()` is a sound public-randomness
  source.** §4.4's CRS argument depends on it. It is a widely used crate
  (3.08 M downloads, `w3f/ark-transcript`), but "widely used" is what got us
  here.
* **`probe-cutchoose` is my own unreviewed code.** It passes tamper and
  cross-context negative tests and its numbers are internally consistent with
  `pkmental`'s independent implementation, but its permutation sampler uses
  `u64 % (i+1)` — modulo-biased, negligible at n = 52, and deliberate in a cost
  probe. Do not lift it into the repo.
* **No constant-time analysis anywhere.** Same gap as `MENTAL_POKER.md` §9
  risk 4, unchanged.
* **I did not evaluate `crumble` or `bluffking` technically** — both are blocked
  on licence before the question arises, and in `crumble`'s case "All Rights
  Reserved" means I should not be building on it at all.
* **I could not determine where `paritytech/mental-poker` will live.** It has
  moved `w3f` → `paritytech`, its own manifest points at the old home, and its
  comments point at a third URL that 404s. That is a reason to vendor, not a
  reason to reject — but it means the git rev in §8 is a snapshot, not an
  address.
* **The full project build was not re-run with `cards-protocol` added.** I
  proved the lock resolves (612 → 618 packages, no arkworks split) and that both
  libraries compile and link together with the project's crypto stack in one
  binary. I did not compile the whole 618-package tree including `libp2p` and
  `eframe`.
