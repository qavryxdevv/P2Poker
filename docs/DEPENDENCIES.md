# Dependency register — `SPEC_CS.md` §28

**Status:** authoritative. This document is the permanent home of the `SPEC_CS.md`
§28 register, assigned by `research/PHASE0_FIXPLAN.md` ruling **D-2**. It replaces
the two interim halves — `CRYPTOGRAPHY.md` §9.1 (cryptographic side) and
`NETWORK_STACK.md` §5.1.1 (transport side) — both of which said in their own text
that they were incomplete and temporary. Those two sections stay where they are as
the *reasoning* for their own crates; this document is the register.

**Authority order.** `SPEC_CS.md` binds. `DECISIONS.md` (D-001 … D-008) outranks
everything below it. The five specification documents come next. `docs/research/`
is **evidence, not authority** — three of its statements are corrected in §9 of this
document, and a research note is never a reason to leave a measured number wrong.

**Measured on:** 2026-08-28, Windows 10, `x86_64-pc-windows-msvc`, 24 cores,
`rustc 1.95.0 (59807616e 2026-04-14)` / `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`,
RustSec advisory database with 1 226 advisories loaded. Every number in this
document is reproducible by §10.

**Verification codes**, as used across the corpus:
**(a)** compiled or executed here; **(b)** read from the crate's own source under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`;
**(c)** registry — the crates.io API or the local advisory database.

---

## 1. The two numbers, and why they differ

| Figure | Value | How |
|---|---|---|
| Crates **compiled into the client** | **425** | `cargo tree --edges normal`, unique name+version, minus `p2p-poker` itself |
| Crates **recorded in `Cargo.lock`** | **611** | `[[package]]` entries, minus `p2p-poker` itself |
| Locked but never compiled | **186** | the difference |

`cargo audit` prints "**612** crate dependencies" because it counts the root package
too. 612 − 1 = 611; 426 − 1 = 425. The two figures in this table both exclude the
root, so they are directly comparable. **(a)**

**425 is a lot** against §28's "use the minimum number of dependencies", and it is
recorded here rather than glossed over. It is what libp2p, arkworks and egui cost
together. The open decision in `DECISIONS.md` to run all discovery through Mainline
DHT and drop the libp2p `dns` and `kad` features would remove roughly twelve of
them; nothing else on the table moves the number materially.

### Why 186 crates are in the lockfile and not in the build

A lockfile is **feature-independent, target-independent, and includes build-only
edges**. It records the union of the resolved graph, not what your binary contains.
Three distinct causes, each with a verified example:

1. **Optional features that are off.** `snow 0.9.6` is pulled by `libp2p-noise
   0.46.1` with `default-features = false, features = ["ring-resolver"]`, so snow's
   pure-Rust `default-resolver` is never built — and with it `chacha20poly1305
   0.10.1`, `chacha20 0.9.1`, `poly1305 0.8.0`, `aead 0.5.2`, `cipher 0.4.4`,
   `aes-gcm 0.10.3`, `ghash`, `polyval` sit in the lockfile compiled by nobody.
   Likewise **`libp2p-mdns 0.48.0` is locked but not compiled** — the `mdns` feature
   is not in our `Cargo.toml` — and `argon2`'s `password-hash` feature is off, so
   `password-hash 0.6.1`, `phc 0.6.1`, `pkcs8`, `spki` and `der` are locked and
   unbuilt. **(a)(b)**
2. **Other targets.** `objc2-*` (26 crates), `wayland-*` (11), `android-*`, `jni-*`,
   `ndk*`, `x11rb`, `wasm-bindgen`, `web-sys`, `redox_syscall`, `netlink-*` — all
   locked, none reachable from `x86_64-pc-windows-msvc`. **(a)**
3. **Build-only edges.** `--edges normal` deliberately excludes `[build-dependencies]`,
   so `cc 1.4.4`, `autocfg`, `jobserver`, `find-msvc-tools`, `gl_generator`,
   `khronos_api`, `version_check`, `pkg-config` and `rustc_version` are compiled
   during the build but contribute no code to the shipped binary. **(a)**

**This distinction is load-bearing for security status.** `cargo audit` reads the
lockfile and has no feature awareness, so it over-reports: it will flag a crate that
is not in the binary and will keep flagging it after the feature that pulled it is
removed. "Flagged by audit but not compiled" is a *different* security status from
"compiled and vulnerable", and this register uses both. `cargo tree --edges normal
-i <crate>` is the tool that answers what is actually in the build.

---

## 2. What "security-critical" means here

§28 asks for a register of security-critical dependencies. A crate is
security-critical in this project if a defect or a hostile change in it could break
`SPEC_CS.md` §35's invariant, or one of these four:

1. it **implements or backs a cryptographic primitive** — the deck construction,
   signatures, hashes, AEAD, KDF, or the transport's own encryption;
2. it **parses attacker-controlled bytes** — wire decoding, DNS, ASN.1/X.509,
   multiaddr, protobuf, bencode;
3. it **enforces a resource limit or a liveness property** the protocol relies on;
4. it **holds or protects key material**.

**122 crates** meet that bar and are registered in §5 with all six §28 columns. The
remaining 303 compiled crates — GUI rendering, windowing, fonts, clipboard, logging,
proc-macro plumbing, integer and string utilities — are not individually registered;
they are covered by the whole-tree licence sweep in §6 and by `cargo audit` over the
whole lockfile in §4. §7 states plainly what that exclusion assumes and where it is
weakest.

---

## 3. The seven entries a reader must not skip

These are read first and none of them is softened. Full rows follow in §5.

### 3.1 `ziffle 0.1.0` — the deck itself, unaudited, one author, one release

This is the crate the entire mental-poker layer stands on, and it is the weakest
link in the corpus.

* **One release ever**: 0.1.0, published 2025-11-01, `num_versions: 1`, 293
  downloads, not yanked. **(c)** crates.io API.
* **One owner**: a single crates.io user (`chris-ricketts`). There is no
  organisation, no second maintainer, no release cadence. **(c)**
* **Unaudited, and it says so itself.** Its own `README.md` carries a section headed
  "⚠️ Security Warning" whose text includes *"This code has not been independently
  audited"* and, in bold, an instruction not to use the library to play for
  non-trivial amounts of money or in any high-stakes scenario. **(b)** read from
  `README.md` lines 15–23 of the published crate.
* 1 779 lines in a single `src/lib.rs`, `no_std`, no allocation; 16 unit tests and
  15 doctests pass on rustc 1.95.0. **(b)(a)**

**The in-house review is a prerequisite, not a nice-to-have.** `research/MENTAL_POKER.md`
§9 risk 1 states it in those words, and `CRYPTOGRAPHY.md` OQ-1 repeats it. What was
verified is that ziffle *rejects 13 specific attacks*. That is emphatically not
soundness: a subtly wrong exponent or a missing check leaves a proof system that
accepts all honest proofs, rejects all naive attacks, and still admits a clever
forgery. Nothing done so far rules that out.

> **Gate.** A line-by-line review of `src/lib.rs` against Bayer–Groth 2012 by
> someone who knows the paper, focused on **`MultiExpArg`** and
> **`SingleValueProductArg`**, and including a check that the `m = 1` instantiation
> preserves the paper's soundness bound, **must complete before the mental-poker
> layer ships**. It is an absolute gate before any real-money use. 1 779 lines makes
> this a few days of work, not a research project.

Two further ziffle risks are open and are **not** closed by that review alone:
the forked Fiat–Shamir transcript (`ShuffleProof::new` clones the transcript and
derives the multi-exponentiation and product arguments from the same state,
separated only by the domain tags `ziffle/BG12MultiExpArgX/v1` and
`ziffle/BG12ProductArgX/v1`; the author flags the fork in a comment) — `CRYPTOGRAPHY.md`
OQ-2; and the absence of fuzzing over `ark-serialize`'s deserialisers on hostile
input, which `SPEC_CS.md` §27 requires — OQ-5.

**State of the mitigation, stated honestly.** `CRYPTOGRAPHY.md` §9 says ziffle is
"vendored into the repository at `vendor/ziffle/` with a `[patch.crates.io]` entry".
**That is the plan and it is not yet done.** There is no `vendor/` directory, no
`[patch.crates.io]` section in `Cargo.toml`, and `Cargo.lock` resolves ziffle from
`registry+https://github.com/rust-lang/crates.io-index` with checksum
`ba79285194a16b02512566a9a64d885567646045b144bb0efeef662001cd83a5`. **(a)(b)** Until
vendoring lands, a yank or a repository deletion upstream is an unmitigated
availability risk, and the review has no frozen artefact to be a review *of*. See §8.

### 3.2 `hickory-proto 0.25.2` — two open advisories, one with no fix

| | |
|---|---|
| **RUSTSEC-2026-0118** | NSEC3 closest-encloser proof validation enters an unbounded loop on cross-zone responses. `cargo audit`: **"No fixed upgrade is available!"** The advisory's `patched` list is literally empty; `unaffected = ["< 0.25.0-alpha.3", ">= 0.26.0-beta.1"]`. Denial of service. |
| **RUSTSEC-2026-0119** | CPU exhaustion during message encoding, O(n²) name compression in `BinEncoder`. Fixed in `>= 0.26.1`. Denial of service. |

**Provenance:** `hickory-proto ← hickory-resolver ← libp2p-dns ← libp2p`, reached
solely through libp2p's **`dns`** feature. **(a)**

**Why we cannot simply upgrade.** `libp2p-dns 0.44.0` declares
`hickory-resolver = "0.25.2"` (a caret requirement), so 0.26.x is outside the
resolvable range. Moving past it requires libp2p to move first. **(b)**

**Reachability, measured rather than assumed.** These two advisories are *not*
equally live in our build:

* **RUSTSEC-2026-0118 is not compiled.** The vulnerable `DnssecDnsHandle` lives in
  `hickory_proto::dnssec`, which is behind
  `#[cfg(any(feature = "dnssec-aws-lc-rs", feature = "dnssec-ring"))]`
  (`hickory-proto-0.25.2/src/lib.rs:55–56`). `libp2p-dns` depends on
  `hickory-resolver` with `default-features = false, features = ["system-config"]`,
  and the only hickory-proto features enabled anywhere in our build are
  `std`, `futures-io` and `tokio` (`cargo tree --edges features -i hickory-proto`).
  The DNSSEC module is not in the binary. **(a)(b)**
* **RUSTSEC-2026-0119 is compiled and reachable.** `BinEncoder` sits in
  `hickory_proto::serialize::binary`, which `lib.rs:74` exports unconditionally.
  An attacker who can shape DNS responses we receive can reach it. **(a)(b)**

Neither statement is a claim that we are safe. Reachability is a fact about today's
feature set, and a feature flip elsewhere in the tree turns 0118 back on silently.
That is exactly the kind of change §8's trigger list is written to catch.

**Dropping `dns` removes them from the build but not from `cargo audit`.**
Verified: with `"dns"` removed from the libp2p feature list, `cargo check` still
succeeds, the compiled tree drops from 425 to 413 crates, and
`cargo tree --edges normal -i hickory-proto` prints *"nothing to print"* — hickory is
not compiled at all. **`cargo audit` still reports both advisories**, because a
lockfile is feature-independent (§1) and `cargo audit` reads the lockfile, not the
build graph. Anybody treating a clean `cargo audit` as the evidence should read §1
first.

**Decision in force.** The `dns` feature is **kept for now**. Removing it would
quietly narrow **D-004**: layers 2 and 3 of the all-NAT story lean on reaching public
relays, whose usual bootstrap addresses are `/dnsaddr/` names. Silently dropping that
capability to make a scanner quiet is the wrong trade. The alternative — all
discovery through Mainline DHT, relay volunteers under a second infohash, dropping
both `dns` and `kad` — is on `DECISIONS.md`'s open list with its cost stated.

### 3.3 `lru 0.16.4` — RUSTSEC-2026-0253, unsound

Use-after-free / double-free from missing panic safety in `LruCache::pop()`: if a
stored key's `Drop` panics, `detach()` is skipped and the linked list keeps dangling
pointers, which a later eviction dereferences. Both are UB reachable from safe Rust.
Fixed in `lru >= 0.18.2`.

**Provenance:** `lru ← mainline`. **(a)**

**Why we cannot upgrade:** `mainline 8.0.0` declares `lru = "0.16.2"`, so `0.18.x` is
outside the resolvable range. The newest published `lru` is 0.18.3. Upgrading
requires `mainline` to move, and `mainline` is `=`-pinned for reasons in §5.8. **(b)(c)**

**Reachability, measured.** The advisory's two preconditions are both structurally
absent in `mainline 8.0.0`:

* Every `LruCache` key in mainline is `Id`, declared `pub struct Id([u8; 20])`
  (`mainline-8.0.0/src/common/id.rs:20`). A plain byte array has no `Drop`, so no
  key `Drop` can panic. **(b)**
* mainline never calls the affected function. The only pop in the crate is
  `self.cached_iterative_queries.pop_lru()` (`src/rpc.rs:914`); `LruCache::pop()`
  does not appear. **(b)**

Recorded as **accepted with justification** rather than fixed, because we cannot fix
it. The justification is a fact about mainline's current usage, not a guarantee: a
mainline release that changes either the key type or the pop call re-arms it, which
is why §8 lists "a pinned crate moves" as a re-audit trigger.

### 3.4 `paste 1.0.15` — RUSTSEC-2024-0436, unmaintained

The author archived the repository and states in its `README.md` that the crate is no
longer maintained. `patched = []` — there will not be a fix. Alternatives
(`pastey`, `with_builtin_macros`) exist upstream but are not ours to choose.

**Provenance:** `paste ← ark-ff ← … ← ziffle`, in the **runtime** tree. **(a)**

This corrects `research/CRYPTO_LIBS.md` §4.4, which expected `paste` only via an
optional `dcbor` **dev**-dependency. It arrives unconditionally with ziffle, so the
`deny.toml` ignore is required unconditionally, and its comment must say so.

`paste` is a proc-macro: it runs at build time and ships in no binary. `cargo audit`
treats it as a warning (exit 0); `cargo deny check advisories` treats unmaintained as
a failure unless it is explicitly ignored.

### 3.5 `libp2p-stream 0.4.0-alpha` — semver-exempt, no stable release

An `-alpha` pre-release. Cargo's semver rules do not apply to it: any republish may
change the API or the behaviour with no version signal, and there is no compatibility
promise to appeal to. The newest published version is still `0.4.0-alpha`; crates.io
reports **no stable version at all** (`max_stable_version: null`). **(c)**

It is also not re-exported by the `libp2p` umbrella, so it is the **one deliberate
exception** to `NETWORK_STACK.md` §5.1's rule against naming libp2p sub-crates
directly. That works only because its own requirement ranges resolve to the same
`libp2p-core`/`libp2p-swarm` instances the umbrella picked; a resolver change there
produces a type mismatch, not a resolver error.

Containment: it is used behind the `NETWORK_STACK.md` §1.3 transport trait, so
replacing it is a one-file change. That is the mitigation, and it is the only one.

Together with `ziffle 0.1.0` these are **the corpus's two unaudited, semver-unstable
dependencies**, and they are the two where a breaking change or an undiscovered
defect lands directly on a security or a liveness property.

### 3.6 `rs_poker =5.0.0` — pinned because 5.1.0 does not build

Pinned with `=`, not caret. **`rs_poker 5.1.0` does not compile on stable rustc
1.95.0**: `src/core/card_bit_set.rs:49` calls `mask.isolate_lowest_one()`, which is
behind the unstable library feature `isolate_most_least_significant_one`
(rust-lang/rust#136909), producing `error[E0658]`. The call is in `pdep_fallback`,
the software fallback for the BMI2 `_pdep_u64` path, and it is **not behind any cargo
feature**, so `--no-default-features` does not help. **(a, compiled — failed)(b)**

A caret requirement would let a routine `cargo update` pull 5.1.0 and break the build
outright. The `=` pin is therefore load-bearing and must not be "tidied up" into `^`.
`5.1.0` is the newest published version, so this pin holds the crate one minor
version back deliberately. **(c)**

**A second reason to keep it pinned.** `rs_poker` is a non-optional consumer of
`rand ~0.10.1`, and its `core` module uses it: `core::deck::Deck` and
`core::card_bit_set::CardBitSet::sample` draw from `rand`. **(b)** Our facade in
`src/poker/evaluator.rs` uses only `Rankable`, `Card`, `Suit`, `Value` and `Rank`,
and `SPEC_CS.md` §7 forbids protocol randomness from any such source. Calling
`rs_poker`'s deck or sampler anywhere in this project is a defect, and this is one of
the things the §7 lint (`CRYPTOGRAPHY.md` §12 item 6, OQ-8) exists to catch.

### 3.7 `mainline 8.0.0` — panics on IPv6

`KrpcSocket::new` reads the bound socket's local address and matches on it:

```
SocketAddr::V6(_) => unimplemented!("KrpcSocket does not support Ipv6"),
```

`mainline-8.0.0/src/rpc/socket.rs:54`. **(b)** `unimplemented!` is a panic. If the
client binds the DHT socket to an IPv6 address, the DHT thread dies — not with an
error we can handle, with an unwind. **Binding the DHT socket must be IPv4-only, and
that is a hard constraint on `src/net/dht.rs`, not a preference.**

Inbound IPv6 packets are handled differently and are not a panic: `src/rpc/socket.rs:207`
matches `Ok((_, SocketAddr::V6(_)))` and only emits a trace. **(b)** So the failure
mode is entirely on our side of the API — it is a bind-time constraint.

The wider consequence, which `research/MAINLINE_DHT.md` §107–109 already states:
BEP 32's IPv6 DHT is a separate routing table, so **discovery is IPv4-only**. On an
IPv6-only network the client discovers nobody. That is a permanent limitation of this
dependency, not a bug to be fixed downstream.

`mainline` is also `=`-pinned because `NETWORK_STACK.md` §11.4 depends on internals
(`RequestFilter`, adaptive server mode) that are not semver-stable in practice.

---

## 4. Open advisories — the complete `cargo audit` result

Run 2026-08-28 against `Cargo.lock`, 1 226 advisories loaded, 612 lockfile packages
scanned. This is the whole output, not a selection. **(a)**

| ID | Crate | Version | Class | Fix available | Compiled? | Status |
|---|---|---|---|---|---|---|
| RUSTSEC-2026-0118 | `hickory-proto` | 0.25.2 | vulnerability — DoS | **none** | crate yes, **vulnerable module no** | accepted, §3.2 |
| RUSTSEC-2026-0119 | `hickory-proto` | 0.25.2 | vulnerability — DoS | `>= 0.26.1`, blocked by `libp2p-dns` | **yes, reachable** | accepted, §3.2 |
| RUSTSEC-2024-0436 | `paste` | 1.0.15 | warning — unmaintained | none, ever | build-time proc-macro only | accepted, §3.4 |
| RUSTSEC-2026-0253 | `lru` | 0.16.4 | warning — unsound | `>= 0.18.2`, blocked by `mainline` | yes, preconditions absent | accepted, §3.3 |

`cargo audit` exits non-zero: *"error: 2 vulnerabilities found! warning: 2 allowed
warnings found"*. **A non-zero `cargo audit` is the current expected state of this
repository.** Anyone wiring this into CI must gate on *this exact set* — an
allow-list of these four IDs with the justifications above — and fail on anything
new, rather than on the exit code alone.

One advisory is **not** in this table and is easy to mis-read as clean:
**RUSTSEC-2026-0097** (`rand`, informational `unsound`) is patched at `>= 0.8.6`, and
the version in our tree is 0.8.8, so `cargo audit` does not report it. The unsound
path requires `rand::thread_rng` inside a custom `log` implementation, which does not
occur here. `research/CRYPTO_LIBS.md` §7.3's original row said `rand` was *"not in
the tree at all"*, which was false; the corrected row is carried in
`CRYPTOGRAPHY.md` §9.2 and in §9 below.

**Historical advisories on registered crates, all patched below the pinned version**,
recorded so a reader does not have to re-derive them: `libp2p` RUSTSEC-2022-0084
(patched `>= 0.45.1`); `libp2p-core` RUSTSEC-2019-0004 (`>= 0.8.1`) and
RUSTSEC-2022-0009 (`>= 0.31.1`); `rustls` RUSTSEC-2024-0399 (`>= 0.23.18`, ours
0.23.43); `quinn-proto` RUSTSEC-2026-0037 (`>= 0.11.14`) and RUSTSEC-2026-0185
(`>= 0.11.15`, ours 0.11.17); `ed25519-dalek` RUSTSEC-2022-0093 (`>= 2`); `idna`
RUSTSEC-2024-0421 (`>= 1.0.0`, ours 1.1.0); `ring` RUSTSEC-2025-0010 (unaffected
`>= 0.17`, ours 0.17.14). **(c)**

**One of these sits exactly on its patch boundary.** `curve25519-dalek` **4.1.3**,
pulled by `libp2p-identity`, is the *first* version patched against RUSTSEC-2024-0344
(timing variability in `Scalar29::sub`/`Scalar52::sub`; `patched = [">= 4.1.3"]`).
There is no margin: any resolution that moves that crate down a patch release
reintroduces a timing side channel into the transport identity. This is a reason to
watch the 4.x line specifically, not only the 5.x one we chose for ourselves.

---

## 5. The register

Six columns per §28: name, version, purpose, repository, licence, security status.
Licence and repository cells are read from each crate's own `Cargo.toml` under the
local registry checkout — **(b)** — not from crates.io metadata, so a rendering
difference between the two cannot silently change a cell here. Versions are the ones
actually compiled, from `cargo tree --edges normal` — **(a)**.

"no open advisory" throughout means: no entry for that crate+version in the
2026-08-28 RustSec database, and it is absent from §4's table. It does **not** mean
audited, and it does not mean reviewed by us.

### 5.1 Mental poker — the deck (8)

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `ziffle` | 0.1.0 | Barnett–Smart mental poker; Bayer–Groth 2012 shuffle proof, DLEQ, Schnorr | `github.com/v26-solutions/ziffle` | MIT OR Apache-2.0 | **unaudited, semver-unstable (0.x, one release, one author); its own README says not to play for non-trivial money.** In-house review of `MultiExpArg` / `SingleValueProductArg` is a **prerequisite** (§3.1, OQ-1). Not yet vendored. |
| `ark-ec` | 0.5.0 | elliptic-curve group traits | `github.com/arkworks-rs/algebra` | MIT OR Apache-2.0 | no open advisory; **not audited** |
| `ark-ff` | 0.5.0 | finite-field arithmetic, `DefaultFieldHasher` | `github.com/arkworks-rs/algebra` | MIT OR Apache-2.0 | no open advisory; **pulls `paste 1.0.15` (RUSTSEC-2024-0436) into the runtime tree** — §3.4 |
| `ark-ff-asm` | 0.5.0 | assembly backend macros for `ark-ff` | `github.com/arkworks-rs/algebra` | MIT OR Apache-2.0 | no open advisory; proc-macro, build time |
| `ark-ff-macros` | 0.5.0 | derive macros for `ark-ff` | `github.com/arkworks-rs/algebra` | MIT OR Apache-2.0 | no open advisory; proc-macro, build time |
| `ark-poly` | 0.5.0 | polynomial arithmetic in the shuffle argument | `github.com/arkworks-rs/algebra` | MIT OR Apache-2.0 | no open advisory; not audited |
| `ark-secp256k1` | 0.5.0 | the secp256k1 curve instance ziffle is generic over | `github.com/arkworks-rs/algebra` | MIT OR Apache-2.0 | no open advisory; not audited. ~3.4× slower than ristretto255 and inherited from the library, not chosen |
| `ark-serialize` | 0.5.0 | canonical point/scalar serialisation, `Validate::Yes` | `github.com/arkworks-rs/algebra` | MIT OR Apache-2.0 | no open advisory; **hostile input not fuzzed** — `SPEC_CS.md` §27 requires it, OQ-5 |

`ark-std 0.5.0` is registered in §5.4 rather than here, because the effect that makes
it security-relevant — it is what reintroduces `rand` into the tree — belongs with
the randomness set. `ark-serialize-derive 0.5.0` (MIT OR Apache-2.0, same repository,
no open advisory) is a proc-macro and is counted with the unregistered remainder.

### 5.2 Application cryptography — our own signatures, hashes and envelope (13)

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `ed25519-dalek` | 3.0.0 | application event signatures, `verify_strict` only | `github.com/dalek-cryptography/curve25519-dalek/tree/main/ed25519-dalek` | BSD-3-Clause | no open advisory; RUSTSEC-2022-0093 patched `>= 2`. **Never enable `hazmat`** (`CRYPTOGRAPHY.md` §10.1). Also linked by `mainline` for DHT mutable items — same crate, different keys |
| `ed25519` | 3.0.0 | signature encoding types for the above | `github.com/RustCrypto/signatures` | Apache-2.0 OR MIT | no open advisory |
| `signature` | 3.0.0 | `Signer` / `Verifier` traits | `github.com/RustCrypto/traits` | Apache-2.0 OR MIT | no open advisory |
| `curve25519-dalek` | 5.0.0 | the Ed25519 group arithmetic under the above | `github.com/dalek-cryptography/curve25519-dalek/tree/main/curve25519-dalek` | BSD-3-Clause | no open advisory at 5.x; constant-time discipline is this crate's stated design goal |
| `curve25519-dalek-derive` | 0.1.1 | internal derive for the above | `github.com/dalek-cryptography/curve25519-dalek` | `MIT/Apache-2.0` (deprecated SPDX form — §6) | no open advisory |
| `sha2` | 0.11.0 | SHA-256/512 for our own layer | `github.com/RustCrypto/hashes` | MIT OR Apache-2.0 | no open advisory; RustCrypto, widely reviewed |
| `digest` | 0.11.3 | hash traits for `sha2 0.11` | `github.com/RustCrypto/traits` | MIT OR Apache-2.0 | no open advisory |
| `blake3` | 1.8.7 | transcript hash, `state_hash`, RNG commitments, `ctx` | `github.com/BLAKE3-team/BLAKE3` | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | no open advisory; **no public third-party audit** — OQ-6 |
| `constant_time_eq` | 0.4.2 | constant-time comparison inside `blake3` | `github.com/cesarb/constant_time_eq` | CC0-1.0 OR MIT-0 OR Apache-2.0 | no open advisory |
| `subtle` | 2.6.1 | constant-time comparison in our own code | `github.com/dalek-cryptography/subtle` | BSD-3-Clause | no open advisory |
| `zeroize` | 1.9.0 | scrubbing secret scalars and seeds | `github.com/RustCrypto/utils` | Apache-2.0 OR MIT | no open advisory; **cannot reach allocator or OS copies** (`CRYPTOGRAPHY.md` §10.2) |
| `zeroize_derive` | 1.5.0 | `#[derive(Zeroize)]` | `github.com/RustCrypto/utils` | Apache-2.0 OR MIT | no open advisory; proc-macro |
| `minicbor` | 2.3.0 | deterministic CBOR for the signed envelope | `github.com/twittner/minicbor` | **BlueOak-1.0.0** | no open advisory; licence needs an explicit `deny.toml` allow entry — §6 |

`minicbor-derive 0.19.5` carries the same BlueOak-1.0.0 licence and repository.

### 5.3 Profile key storage (9)

`SPEC_CS.md` §21. These protect the identity keys at rest.

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `argon2` | 0.6.0 | passphrase → KEK for the profile key slots | `github.com/RustCrypto/password-hashes` | MIT OR Apache-2.0 | no open advisory. Used as a raw KDF: the `password-hash` feature is off, so `password-hash`, `phc`, `pkcs8`, `spki` and `der` are locked but not compiled — no PHC string format is parsed |
| `blake2` | 0.11.0 | BLAKE2b inside Argon2 | `github.com/RustCrypto/hashes` | MIT OR Apache-2.0 | no open advisory |
| `chacha20poly1305` | 0.11.0 | XChaCha20-Poly1305 profile AEAD | `github.com/RustCrypto/AEADs` | Apache-2.0 OR MIT | no open advisory |
| `chacha20` | 0.10.2 | the stream cipher under it | `github.com/RustCrypto/stream-ciphers` | MIT OR Apache-2.0 | no open advisory |
| `poly1305` | 0.9.1 | the MAC under it | `github.com/RustCrypto/universal-hashes` | Apache-2.0 OR MIT | no open advisory |
| `aead` | 0.6.1 | AEAD traits | `github.com/RustCrypto/traits` | MIT OR Apache-2.0 | no open advisory |
| `cipher` | 0.5.2 | block/stream cipher traits | `github.com/RustCrypto/traits` | MIT OR Apache-2.0 | no open advisory |
| `universal-hash` | 0.6.1 | universal-hash traits | `github.com/RustCrypto/traits` | MIT OR Apache-2.0 | no open advisory |
| `windows-sys` | 0.61.2 | DPAPI key slot on Windows | `github.com/microsoft/windows-rs` | MIT OR Apache-2.0 | no open advisory; Windows-only path, and the only OS-keystore path implemented |

### 5.4 Randomness — the whole set, including what we do not use (11)

`SPEC_CS.md` §7 is a rule about *our* code, not an absence in the tree. All eleven of
these are compiled. Only the first two may ever be a source of protocol randomness.

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `getrandom` | 0.4.3 | **the OS CSPRNG** — `getrandom::fill`, `getrandom::SysRng` | `github.com/rust-random/getrandom` | MIT OR Apache-2.0 | no open advisory. **The only permitted source of cryptographic randomness.** API verified: `pub use sys_rng::SysRng` at `src/lib.rs:34`, `pub fn fill` at `:87` **(b)** |
| `rand_core` | 0.10.1 | the `RngCore` trait our OS-CSPRNG adapter implements | `github.com/rust-random/rand_core` | MIT OR Apache-2.0 | no open advisory |
| `rand` | 0.8.8 | required by `ark-std` with `std_rng`; **not** a source of protocol randomness | `github.com/rust-random/rand` | MIT OR Apache-2.0 | RUSTSEC-2026-0097 (`unsound`) patched at `>= 0.8.6`; 0.8.8 is patched, and the unsound path (`thread_rng` inside a custom `log` impl) does not occur here — §4 |
| `rand` | 0.9.5 | pulled by `hickory-proto`, `igd-next` and `yamux` | `github.com/rust-random/rand` | MIT OR Apache-2.0 | no open advisory at 0.9.x |
| `rand` | 0.10.2 | pulled by `quinn-proto` **and by `rs_poker`** | `github.com/rust-random/rand` | MIT OR Apache-2.0 | no open advisory. See §3.6: `rs_poker::core`'s deck and sampler use it and must never be called |
| `rand_chacha` | 0.3.1 | backs `StdRng` inside `rand 0.8` | `github.com/rust-random/rand` | MIT OR Apache-2.0 | no open advisory |
| `rand_chacha` | 0.9.0 | backs `StdRng` inside `rand 0.9` | `github.com/rust-random/rand` | MIT OR Apache-2.0 | no open advisory |
| `rand_core` | 0.6.4 | required by `libp2p-autonat` and by `ark-std` | `github.com/rust-random/rand` | MIT OR Apache-2.0 | no open advisory; coexists with 0.9 and 0.10 as three distinct Rust types |
| `rand_core` | 0.9.5 | under `rand 0.9` | `github.com/rust-random/rand` | MIT OR Apache-2.0 | no open advisory |
| `rand_pcg` | 0.10.2 | PCG generator used by `quinn-proto` for connection IDs | `github.com/rust-random/rngs` | MIT OR Apache-2.0 | no open advisory; **a non-cryptographic generator in the tree** — never reachable from our code |
| `ark-std` | 0.5.0 | `no_std` shims; **the crate that reintroduces `rand`** | `github.com/arkworks-rs/std` | `MIT/Apache-2.0` (deprecated SPDX form — §6) | no open advisory; not audited |

`getrandom 0.2.17` (via `libp2p`, `libp2p-gossipsub`, `libp2p-metrics` and
`rand_core 0.6.4`) and `getrandom 0.3.4` (via `rand_core 0.9.5`) are also compiled,
same repository and licence as 0.4.3, no open advisory. **(a)**

> **The claim that must not be restated.** `research/CRYPTO_LIBS.md` §10 says of its
> dependency block: *"No `rand` at any depth."* That was true of its isolated probe
> and is **false in the integrated tree** — three majors of `rand` and three of
> `rand_core` are compiled. The discipline survives; the absence does not. What
> `SPEC_CS.md` §7 actually requires is enforceable as a lint (`CRYPTOGRAPHY.md` §12
> item 6, OQ-8): our own code draws cryptographic randomness only from
> `getrandom::SysRng`; `SmallRng`, `StdRng` and any self-seeded generator are never
> used for keys, masking factors, permutations or commitments.

### 5.5 Transport — libp2p (25)

All 25 share repository `github.com/libp2p/rust-libp2p` and licence **MIT**. All are
resolved by the `libp2p 0.56.0` umbrella except `libp2p-stream`, which is declared
directly (§3.5). No open advisory on any of them at these versions; the historical
`libp2p` and `libp2p-core` advisories are in §4.

| Name | Version | Purpose | Security status |
|---|---|---|---|
| `libp2p` | 0.56.0 | umbrella: transport, encryption, NAT traversal, gossip, relay | RUSTSEC-2022-0084 patched `>= 0.45.1`; pinned version is patched |
| `libp2p-core` | 0.43.2 | transport traits, connection upgrades | RUSTSEC-2019-0004 (`>= 0.8.1`) and RUSTSEC-2022-0009 (`>= 0.31.1`) both far below 0.43.2 |
| `libp2p-identity` | 0.2.14 | `PeerId`, transport `Keypair` | no open advisory. **Never depend on it directly**: `libp2p-identity 0.3.0` exists outside the umbrella's `^0.2.12` range and linking it gives a second, incompatible `PeerId` as a *type mismatch*, not a resolver error |
| `libp2p-swarm` | 0.47.1 | swarm driver | no open advisory |
| `libp2p-swarm-derive` | 0.35.1 | `#[derive(NetworkBehaviour)]` | no open advisory; proc-macro |
| `libp2p-quic` | 0.13.1 | primary transport; has a real `hole_punching` module | no open advisory |
| `libp2p-tcp` | 0.44.1 | fallback transport for UDP-blocked networks | no open advisory |
| `libp2p-dns` | 0.44.0 | `/dnsaddr/` resolution for relay bootstrap | **the sole path to both hickory advisories** — §3.2 |
| `libp2p-noise` | 0.46.1 | Noise XX security upgrade; mandatory for relay | no open advisory. Selects `snow`'s `ring-resolver`, so the handshake AEAD and hash come from `ring`, not from RustCrypto — §5.6 |
| `libp2p-tls` | 0.6.2 | TLS 1.3 security upgrade | no open advisory |
| `libp2p-yamux` | 0.47.0 | stream muxer; mandatory for relay | no open advisory. **Links two yamux majors on purpose** — §5.6 |
| `libp2p-gossipsub` | 0.49.5 | lobby topics | no open advisory. Parses attacker-controlled protobuf from unauthenticated peers |
| `libp2p-kad` | 0.48.0 | Kademlia DHT | no open advisory. **Enabled in `Cargo.toml`; `NETWORK_STACK.md` §5.2 says it is not** — §9 |
| `libp2p-identify` | 0.47.0 | address candidates | no open advisory. Peer-supplied addresses are attacker-controlled input |
| `libp2p-ping` | 0.47.0 | liveness | no open advisory |
| `libp2p-autonat` | 0.15.0 | reachability probing | no open advisory. Pulls `rand_core 0.6.4` |
| `libp2p-dcutr` | 0.14.1 | direct connection upgrade through relay (hole punching) | no open advisory |
| `libp2p-relay` | 0.21.1 | Circuit Relay v2 (**D-001**, **D-002**) | no open advisory. **`max_circuit_bytes` is a bidirectional total, not per direction** — see the box below |
| `libp2p-request-response` | 0.29.0 | snapshot RPC, join RPC | no open advisory |
| `libp2p-stream` | 0.4.0-alpha | per-table streams | **unaudited and semver-exempt; no stable release exists** — §3.5 |
| `libp2p-upnp` | 0.5.0 | IGD port mapping | no open advisory. Speaks HTTP to a LAN device that is not authenticated — §5.9 |
| `libp2p-connection-limits` | 0.6.0 | connection caps | no open advisory. **There is no `connection-limits` cargo feature** — it is a non-optional dependency and `libp2p::connection_limits` is always available; asking for the feature is a hard resolver error |
| `libp2p-memory-connection-limits` | 0.5.0 | memory-based caps | no open advisory. This one *is* a feature, spelled `memory-connection-limits` |
| `libp2p-allow-block-list` | 0.6.0 | peer blocklist | no open advisory; non-optional, like `connection-limits` |
| `libp2p-metrics` | 0.17.0 | Prometheus metrics | no open advisory. Enabled in `Cargo.toml`; absent from `NETWORK_STACK.md` §5.1.1 — §9 |

> **Correction, verified in source, that must be carried everywhere it appears.**
> The fix plan's A-8 replacement text described `max_circuit_bytes` as "per circuit
> **and per direction**". **That is false.** `libp2p-relay-0.21.1/src/copy_future.rs`
> declares a single `bytes_sent: u64` on `CopyFuture`, and **both** `forward_data`
> calls — src→dst at lines 88–95 and dst→src at lines 97–104 — increment that same
> counter, which is then compared once against `max_circuit_bytes`. So
> `max_circuit_bytes` is a **bidirectional total for the whole circuit**. The default
> is `1 << 17` (`behaviour.rs:163`) = 131 072 bytes = **128 KiB of combined traffic,
> not 128 KiB each way**, and every figure derived from it must be recomputed and
> halved. **(b)**
>
> Note also, because a reader will find it and be misled: kubo's `docs/config.md`
> describes `ConnectionDataLimit` as *"in each direction"*. Either the Go and Rust
> implementations differ, or that wording is loose. **Our own relay is Rust, so the
> Rust behaviour binds us**, and for third-party relays the stricter reading — a
> bidirectional total — is the safe assumption.

### 5.6 Transport cryptography and muxing, transitive (22)

This is where the wire encryption actually lives. None of it is named in
`Cargo.toml`, and none of it was in either interim register — which is precisely why
a register generated from the build graph, not from the manifest, is the right shape.

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `ring` | 0.17.14 | **the primitives behind Noise, TLS and QUIC** — AEAD, X25519, digests, RNG | `github.com/briansmith/ring` | Apache-2.0 AND ISC | no open advisory (RUSTSEC-2025-0010 is "< 0.17 unmaintained"; ours is 0.17.14). BoringSSL-derived C and assembly — the largest non-Rust attack surface in the build |
| `untrusted` | 0.9.0 | `ring`'s bounds-checked input reader; every byte `ring` parses goes through it | `github.com/briansmith/untrusted` | **ISC** | no open advisory. Tiny by design and that is the point — it exists so `ring` cannot read past a buffer |
| `rustls` | 0.23.43 | TLS 1.3 for `libp2p-tls` and QUIC | `github.com/rustls/rustls` | Apache-2.0 OR ISC OR MIT | no open advisory; RUSTSEC-2024-0399 patched `>= 0.23.18` |
| `rustls-webpki` | 0.103.15 | certificate path validation | `github.com/rustls/webpki` | **ISC** | no open advisory; parses attacker-supplied certificates |
| `rustls-pki-types` | 1.15.1 | PKI type definitions | `github.com/rustls/pki-types` | MIT OR Apache-2.0 | no open advisory |
| `futures-rustls` | 0.26.0 | futures adapter for rustls | `github.com/quininer/futures-rustls` | `MIT/Apache-2.0` (deprecated SPDX form) | no open advisory |
| `snow` | 0.9.6 | Noise protocol framework | `github.com/mcginty/snow` | Apache-2.0 OR MIT | no open advisory; RUSTSEC-2024-0011 patched below this version. Built with `ring-resolver` only, so its pure-Rust resolver is not compiled (§1) |
| `x25519-dalek` | 2.0.1 | Noise static keypair DH | `github.com/dalek-cryptography/curve25519-dalek/tree/main/x25519-dalek` | BSD-3-Clause | no open advisory |
| `curve25519-dalek` | 4.1.3 | group arithmetic for `x25519-dalek` and `libp2p-identity` | as §5.2 | BSD-3-Clause | **exactly on the RUSTSEC-2024-0344 patch boundary (`>= 4.1.3`)** — no margin; §4 |
| `ed25519-dalek` | 2.2.0 | `PeerId` signatures inside `libp2p-identity` | as §5.2 | BSD-3-Clause | no open advisory. Deliberately a *different Rust type* from our 3.0.0, which is what mechanically enforces `SPEC_CS.md` §20's identity separation |
| `ed25519` | 2.2.3 | signature encoding for the above | `github.com/RustCrypto/signatures/tree/master/ed25519` | Apache-2.0 OR MIT | no open advisory |
| `signature` | 2.2.0 | signer/verifier traits for the above | `github.com/RustCrypto/traits/tree/master/signature` | Apache-2.0 OR MIT | no open advisory |
| `sha2` | 0.10.9 | Fiat–Shamir hash inside ziffle; digests in libp2p | `github.com/RustCrypto/hashes` | MIT OR Apache-2.0 | no open advisory |
| `digest` | 0.10.7 | hash traits for `sha2 0.10` | `github.com/RustCrypto/traits` | MIT OR Apache-2.0 | no open advisory |
| `hkdf` | 0.12.4 | key derivation in the transport handshakes | `github.com/RustCrypto/KDFs/` | MIT OR Apache-2.0 | no open advisory |
| `hmac` | 0.12.1 | MAC under HKDF | `github.com/RustCrypto/MACs` | MIT OR Apache-2.0 | no open advisory |
| `rcgen` | 0.13.2 | generates the self-signed libp2p TLS certificate | `github.com/rustls/rcgen` | MIT OR Apache-2.0 | no open advisory; handles our transport private key |
| `quinn` | 0.11.11 | QUIC endpoint | `github.com/quinn-rs/quinn` | MIT OR Apache-2.0 | no open advisory |
| `quinn-proto` | 0.11.17 | QUIC state machine — parses every UDP datagram | `github.com/quinn-rs/quinn` | MIT OR Apache-2.0 | no open advisory; RUSTSEC-2026-0037 (`>= 0.11.14`) and RUSTSEC-2026-0185 (`>= 0.11.15`) both patched here |
| `quinn-udp` | 0.5.15 | platform UDP socket layer | `github.com/quinn-rs/quinn` | MIT OR Apache-2.0 | no open advisory |
| `yamux` | 0.13.10 | stream muxer, current | `github.com/paritytech/yamux` | Apache-2.0 OR MIT | no open advisory |
| `yamux` | 0.12.1 | stream muxer, compatibility | `github.com/paritytech/yamux` | Apache-2.0 OR MIT | no open advisory. **`libp2p-yamux 0.47.0` links both majors deliberately**, aliased `yamux012` and `yamux013` in its own `Cargo.toml`. This duplicate is intended upstream, not resolver damage **(b)** |

### 5.7 Hostile-input parsers (22)

Everything here decodes bytes an attacker chooses. `SPEC_CS.md` §27 wants these
fuzzed; none of them has been fuzzed by us.

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `hickory-proto` | 0.25.2 | DNS wire format | `github.com/hickory-dns/hickory-dns` | MIT OR Apache-2.0 | **RUSTSEC-2026-0118 (no fix; module not compiled) and RUSTSEC-2026-0119 (reachable)** — §3.2 |
| `hickory-resolver` | 0.25.2 | DNS resolution for `/dnsaddr/` | `github.com/hickory-dns/hickory-dns` | MIT OR Apache-2.0 | no advisory of its own; the sole importer of `hickory-proto` |
| `x509-parser` | 0.17.0 | parses peer TLS certificates | `github.com/rusticata/x509-parser.git` | MIT OR Apache-2.0 | no open advisory |
| `asn1-rs` | 0.7.2 | ASN.1 decoding under the above | `github.com/rusticata/asn1-rs.git` | MIT OR Apache-2.0 | no open advisory |
| `der-parser` | 10.0.0 | DER decoding under the above | `github.com/rusticata/der-parser.git` | MIT OR Apache-2.0 | no open advisory |
| `oid-registry` | 0.8.1 | OID lookup for the above | `github.com/rusticata/oid-registry.git` | MIT OR Apache-2.0 | no open advisory |
| `yasna` | 0.5.2 | ASN.1 writer used by `rcgen` | `github.com/qnighy/yasna.rs` | MIT OR Apache-2.0 | no open advisory |
| `quick-protobuf` | 0.8.1 | protobuf for gossipsub, identify, relay, dcutr | `github.com/tafia/quick-protobuf` | MIT | no open advisory; **every libp2p behaviour message passes through it** |
| `prost` | 0.14.4 | protobuf decoding of the peer public key inside `libp2p-identity` | `github.com/tokio-rs/prost` | **Apache-2.0** (not dual) | no open advisory. Decodes bytes offered by an unauthenticated dialer **before** the `PeerId` is established, so it runs earlier than any authentication we control |
| `unsigned-varint` | 0.8.0 | length prefixes on every libp2p frame | `github.com/paritytech/unsigned-varint` | MIT | no open advisory |
| `unsigned-varint` | 0.7.2 | the same, for crates still on 0.7 | `github.com/paritytech/unsigned-varint` | MIT | no open advisory |
| `asynchronous-codec` | 0.7.0 | framed codec around the above | `github.com/mxinden/asynchronous-codec` | MIT | no open advisory; **where a missing length cap becomes an allocation DoS** |
| `multihash` | 0.19.5 | `PeerId` encoding | `github.com/multiformats/rust-multihash` | MIT | no open advisory |
| `multiaddr` | 0.18.2 | parses peer-supplied addresses | `github.com/multiformats/rust-multiaddr` | MIT | no open advisory; input arrives from `identify` and from the DHT |
| `bs58` | 0.5.1 | base58 in `PeerId` text form | `github.com/Nullus157/bs58-rs` | `MIT/Apache-2.0` (deprecated SPDX form) | no open advisory |
| `data-encoding` | 2.11.1 | base32/base64 in multiaddr | `github.com/ia0/data-encoding` | MIT | no open advisory |
| `idna` | 1.1.0 | IDNA in URL parsing | `github.com/servo/rust-url` | MIT OR Apache-2.0 | no open advisory; RUSTSEC-2024-0421 patched `>= 1.0.0` |
| `url` | 2.5.8 | URL parsing for UPnP control endpoints | `github.com/servo/rust-url` | MIT OR Apache-2.0 | no open advisory |
| `serde` | 1.0.229 | derive-based decoding across the tree | `github.com/serde-rs/serde` | MIT OR Apache-2.0 | no open advisory. **Not used for our signed envelope** — that is `minicbor`, because `serde` does not guarantee a canonical encoding |
| `serde_bencode` | 0.2.4 | **bencode for every DHT packet** | `github.com/toby/serde-bencode` | MIT | no open advisory; small crate, unaudited, and the first thing an unauthenticated UDP packet touches |
| `serde_bytes` | 0.11.19 | byte-string support under the above | `github.com/serde-rs/bytes` | MIT OR Apache-2.0 | no open advisory |
| `bytes` | 1.12.1 | buffer type carrying all decoded frames | `github.com/tokio-rs/bytes` | MIT | no open advisory |

### 5.8 Discovery — Mainline DHT (6)

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `mainline` | 8.0.0 | BitTorrent Mainline DHT client | `github.com/pubky/mainline` | MIT | no open advisory. **`unimplemented!()` panic on an IPv6 bind; discovery is IPv4-only** — §3.7. `=`-pinned because `NETWORK_STACK.md` §11.4 uses internals (`RequestFilter`, adaptive server mode) that are not semver-stable in practice. 8.0.0 is the newest of 43 published versions **(c)** |
| `lru` | 0.16.4 | peer, value and query caches inside `mainline` | `github.com/jeromefroe/lru-rs.git` | MIT | **RUSTSEC-2026-0253, unsound.** Upgrade blocked by `mainline`'s `lru = "0.16.2"`; preconditions structurally absent — §3.3 |
| `sha1_smol` | 1.0.1 | infohash computation | `github.com/mitsuhiko/sha1-smol` | BSD-3-Clause | no open advisory. SHA-1 here is BitTorrent's protocol-mandated infohash, **not** a security hash — no collision resistance is claimed or needed |
| `crc` | 3.4.0 | CRC32C for BEP 42 secure node IDs | `github.com/mrhooray/crc-rs.git` | MIT OR Apache-2.0 | no open advisory |
| `flume` | 0.12.0 | channels between the DHT thread and the client | `github.com/zesterer/flume` | `Apache-2.0/MIT` (deprecated SPDX form) | no open advisory |
| `futures-lite` | 2.6.1 | async adapters for `mainline`'s async API | `github.com/smol-rs/futures-lite` | Apache-2.0 OR MIT | no open advisory |

`mainline 8.0.0` also links `ed25519-dalek 3.0.0` — the *same* crate as our
application signatures (§5.2) — for BEP 44 mutable items. The crate is shared; the
keys are not, and must not be. `dyn-clone 1.0.20` (MIT OR Apache-2.0) and
`tracing 0.1.44` (MIT) arrive with it and are not security-critical.

### 5.9 UPnP / IGD — the LAN attack surface (2)

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `igd-next` | 0.16.2 | IGD port mapping via `libp2p-upnp` | `github.com/dariusc93/rust-igd` | MIT | no open advisory. Talks SSDP/HTTP/XML to whatever on the LAN answers as a gateway — the device is discovered, never authenticated |
| `attohttpc` | 0.30.1 | the HTTP client under `igd-next` | `github.com/sbstp/attohttpc` | **MPL-2.0** | no open advisory. **The only copyleft licence in the whole compiled tree** — §6 |

### 5.10 Poker rules (1)

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `rs_poker` | 5.0.0 | 7-card hand evaluator, behind our own facade | `github.com/elliottneilclark/rs-poker` | **Apache-2.0** (not dual) | no open advisory. **`=`-pinned: 5.1.0 does not build on stable 1.95.0** — §3.6. Contained in `src/poker/evaluator.rs`; its suit numbering differs from ours and a silent cast would corrupt every card |

### 5.11 Runtime, metrics and time (3)

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `tokio` | 1.53.1 | async runtime for the swarm and the DHT | `github.com/tokio-rs/tokio` | MIT | no open advisory; five historical advisories all patched far below this version |
| `futures` | 0.3.34 | future/stream combinators | `github.com/rust-lang/futures-rs` | MIT OR Apache-2.0 | no open advisory. **Never depend on it as a libp2p type source** — `NETWORK_STACK.md` §5.1 |
| `web-time` | 1.1.0 | `Instant` in the `RateLimiter` signature | `github.com/daxpedda/web-time` | MIT OR Apache-2.0 | no open advisory. Security-relevant because a rate limiter is a §11 resource control, and because `STATE_MACHINE.md` I22's replay determinism forbids wall-clock reads inside the state machine |

`prometheus-client 0.23.1` (`github.com/prometheus/client_rust`, Apache-2.0 OR MIT,
no open advisory) is compiled via `libp2p-metrics`. It is registered here rather than
counted as security-critical only if metrics are ever exposed on a socket; today
nothing serves them, and if that changes it becomes an unauthenticated endpoint and
moves into §5.9's category.

**Registered total: 122 crates.**

### 5.12 What is deliberately not registered

The remaining **303** compiled crates are not individually registered. They are
dominated by the GUI and its platform stack (`eframe 0.36.1`, `egui_extras 0.36.1`,
`image 0.25.10`, `rust-embed 8.12.0`, winit, glow, fonts, clipboard), plus logging
(`tracing 0.1.44`), proc-macro machinery, and small utility crates; a handful are
duplicate minor versions of crates already registered above (`getrandom 0.2.17` and
`0.3.4`, `minicbor-derive 0.19.5`, `ark-serialize-derive 0.5.0`), named in the prose
of §5 without a row of their own. They are covered by the whole-tree licence sweep
(§6) and by `cargo audit` over the whole lockfile (§4), and by nothing else.

**The assumption that makes that exclusion defensible, stated so it can be
attacked:** no attacker-controlled bytes reach them. The table image and the fonts
are compiled into the binary by `rust-embed`, so the PNG and font decoders only ever
see our own assets. **This assumption fails the moment anything peer-supplied is
rendered** — an avatar, a table skin, a chat message with an image, a downloaded
theme. If any of that is ever added, `image`, the font stack and the clipboard path
move into §5.7 and are registered individually. That is a §8 re-audit trigger.

---

## 6. Licence sweep over the whole compiled tree

Every one of the 425 compiled dependencies has a licence expression in its own
`Cargo.toml`. **There is no dependency whose licence could not be established.**
**(a)** `cargo deny check licenses` reports exactly one `unlicensed` error, and it is
`p2p-poker` itself.

Distribution over the 425, counted from `cargo metadata`'s `license` field. The
groups sum to 425 exactly. **(a)**

| Licence expression | Count | Notes |
|---|---|---|
| MIT and/or Apache-2.0 only, in any spelling or order | 360 | includes 87 MIT-only, 8 Apache-2.0-only (among them **`rs_poker`**, `prost`, `winit`, `glutin*`), and the 15 deprecated slash forms in note 3 below |
| `Unicode-3.0` | 18 | the ICU crates under `idna` |
| `BSD-3-Clause` | 7 | `ed25519-dalek` ×2, `curve25519-dalek` ×2, `x25519-dalek`, `subtle`, `sha1_smol` — six of the seven are registered in §5 |
| `Unlicense OR MIT` / `Unlicense/MIT` | 7 | `memchr`, `aho-corasick`, `byteorder`, `byteorder-lite`, `walkdir`, `same-file`, `winapi-util` |
| `MIT OR Apache-2.0 OR Zlib` | 5 | `glow`, `cursor-icon`, `raw-window-handle`, `tinyvec_macros`, `lru-slab` |
| `Zlib OR Apache-2.0 OR MIT` | 3 | `bytemuck`, `bytemuck_derive`, `tinyvec` |
| `MIT OR Zlib OR Apache-2.0` | 2 | `miniz_oxide` ×2 — the same three licences in a third word order |
| `ISC` | 3 | **`rustls-webpki`**, `untrusted`, `libloading` |
| `Zlib` | 2 | `foldhash` ×2 |
| `BSL-1.0` | 2 | `clipboard-win`, `error-code` |
| `BlueOak-1.0.0` | 2 | **`minicbor`**, `minicbor-derive` |
| `BSD-3-Clause OR Apache-2.0` | 2 | `moxcms`, `pxfm` (colour management under `image`) |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 | `unicode-ident` |
| `CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception` | 1 | **`blake3`** |
| `CC0-1.0 OR MIT-0 OR Apache-2.0` | 1 | `constant_time_eq` |
| `Apache-2.0 OR ISC OR MIT` | 1 | **`rustls`** |
| `Apache-2.0 AND ISC` | 1 | **`ring`** — a conjunction, not a choice |
| `BSD-2-Clause OR Apache-2.0 OR MIT` | 1 | `zerocopy` |
| `BSD-2-Clause` | 1 | `arrayref` (used by `blake3`) |
| `MIT OR BSD-3-Clause` | 1 | `if-addrs` |
| `0BSD OR MIT OR Apache-2.0` | 1 | `adler2` |
| `Apache-2.0 OR GPL-2.0-only` | 1 | `self_cell` — the Apache branch is taken; **the only appearance of GPL in the tree is as the unchosen half of a dual licence** |
| `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0` | 1 | `epaint_default_fonts` — font licences, not code |
| **`MPL-2.0`** | **1** | **`attohttpc`** |

Four things a `deny.toml` has to handle, all verified:

1. **`attohttpc 0.30.1` is MPL-2.0** — the only copyleft licence in the tree, weak
   and file-scoped. It arrives via `igd-next ← libp2p-upnp ← libp2p`, so dropping
   the `upnp` feature removes it. It must be an explicit, deliberate allow.
2. **`minicbor` and `minicbor-derive` are BlueOak-1.0.0** and `rustls-webpki` is
   **ISC**; neither is on any default allow-list.
3. **Fifteen crates use the deprecated SPDX slash form** `MIT/Apache-2.0`, which a
   strict licence policy may fail to parse: `ark-std`, `asn1-rs-impl`, `bs58`,
   `curve25519-dalek-derive`, `enum-as-inner`, `futures-rustls`, `futures-timer`,
   `guillotiere`, `hex_fmt`, `ipconfig`, `minimal-lexical`, `rusticata-macros`,
   `siphasher`, `tagptr`, `winapi` — plus `flume` (`Apache-2.0/MIT`) and `fnv`
   (`Apache-2.0 / MIT`, with spaces). `CRYPTOGRAPHY.md` §9 names only `ark-std`;
   sixteen more need the same clarification. **(a)**
4. **`ring 0.17.14` is `Apache-2.0 AND ISC`** — a conjunction, not a choice, and it
   also carries BoringSSL/OpenSSL-derived files. It needs a clarification entry, not
   a plain allow.

**Our own licence is undecided.** `Cargo.toml` has no `license` field, and the choice
(MIT / Apache-2.0 / dual / GPL-3.0 / AGPL-3.0) is on `DECISIONS.md`'s open list,
blocking publication. Nothing in the dependency set forecloses any of those five: the
tree is permissive throughout, and the single MPL-2.0 crate is file-scoped weak
copyleft that does not propagate to our sources. `rs_poker`'s Apache-2.0-only licence
is the one entry that would matter against a GPL-2.0-only choice, which is not on the
list. **This is a record of the licence facts, not legal advice**, and the decision
stays with the maintainer.

---

## 7. What §28 requires and is not yet in place

Recorded as obligations rather than quietly omitted, because §28 asks for
`cargo-deny` and `cargo-audit` by name and only one of them runs today.

| Obligation | State | Where it lands |
|---|---|---|
| `Cargo.lock` committed | **done** | repository root |
| `cargo audit` clean or explicitly justified | **runs; 4 findings, all justified in §4**; no CI job | Phase 7 |
| `cargo deny` configured | **`deny.toml` does not exist.** With no config its allow-list is empty, so `cargo deny check licenses` emits 588 `rejected` errors over the graph it walks — every crate, including MIT — plus one `unlicensed` error for `p2p-poker`. It is installed but is not currently a usable gate | Phase 7 |
| `ziffle` vendored with `[patch.crates.io]` | **not done** — §3.1 | before the OQ-1 review |
| Register generated from `cargo metadata` and checked in CI | **not done**; this document is hand-written from measured output | Phase 7 |
| Fuzzing of the deserialisers (`SPEC_CS.md` §27) | **not done** — OQ-5 | Phase 6 |

**This register is hand-written, and a hand-written register is wrong within a
month.** That is not a hypothetical: §9 lists three drift findings this document
found on its first pass. The generated register is the fix; §8 is what holds the line
until it exists.

---

## 8. Review and refresh procedure

### 8.1 What triggers a re-audit

Any one of these. No judgement call about whether it "seems" relevant — the trigger
fires and the checklist in §8.2 runs.

1. **`Cargo.lock` changes at all.** Any `cargo update`, any added or removed
   dependency, any feature flip in `Cargo.toml`. A feature flip counts because it
   moves crates between "compiled" and "locked only", which is the distinction §1
   shows the tooling cannot see.
2. **A new advisory lands against anything in the lockfile**, whether or not it is
   compiled, and whether `cargo audit` calls it a vulnerability or a warning.
3. **A pinned crate moves.** `ziffle`, `mainline`, `rs_poker` and `libp2p-stream`
   are the four whose pins carry reasoning; a new release of any of them, or of
   `libp2p` or `lru` or `hickory-*`, is a trigger even if we do not take it. Some of
   §3's justifications are facts about the *current* upstream code (mainline's key
   type, hickory's feature gating) and a release invalidates them.
4. **Phase transition.** `SPEC_CS.md` §30's phase boundaries, each of which changes
   what the code does with the tree. Phase 7 (libp2p transport) and Phase 11
   (security audit) are the two that must not be crossed on a stale register.
5. **Any change that lets peer-supplied bytes reach a crate not in §5** — the §5.12
   assumption breaking. Rendering a peer avatar is the canonical example.
6. **Toolchain change.** A new stable rustc can change what resolves and what
   compiles; §3.6 is an existing example of a crate that a compiler version decides.
7. **Twelve months since the last full pass**, whichever comes first.

### 8.2 What a re-audit does

Run §10 in full and diff against this document. Then, for every difference:

1. Recount 425 / 611 / 186 and correct §1 if any moved.
2. Re-run `cargo audit`; every finding goes into §4's table with a compiled/not
   compiled determination made by `cargo tree --edges normal -i <crate>`, not by
   assumption.
3. For every new or changed advisory, determine **reachability in source** the way
   §3.2 and §3.3 do — find the vulnerable function, check whether the feature that
   compiles it is on, check whether our call pattern meets the preconditions. Write
   down what was read, with file and line.
4. Re-read licence and repository cells for anything whose version changed. **(b)**,
   from the registry checkout, never from memory.
5. Update §5's rows and §5.12's count so they still sum to 425.

### 8.3 Who decides an advisory is acceptable, and what they may not accept

**The maintainer decides, and the decision is written down before it takes effect.**
There is one maintainer; pretending otherwise would be theatre. What makes this
reviewable is not a second signature, it is that every acceptance is recorded with
its reasoning where a third party can attack it.

**Three tiers, and the tier is decided by what the crate touches, not by the
advisory's own severity label:**

* **Tier 1 — cannot be accepted.** An advisory affecting the deck construction, the
  application signatures, the transcript hash, the profile key storage, or the
  transport's encryption — anything in §5.1, §5.2, §5.3, §5.4's first two rows, or
  §5.6. These block. Not "raise the priority": the affected code does not ship, and
  if there is no fix upstream the answer is to replace the crate or stop. This is
  what the `CRYPTOGRAPHY.md` §9 trait boundary and the `NETWORK_STACK.md` §1.3
  transport trait exist to make possible.
* **Tier 2 — acceptable only with a reachability argument in source.** Everything
  else in §5. An acceptance must name the vulnerable function, state why it is not
  compiled or why its preconditions do not hold in our call pattern, cite file and
  line, and record what would re-arm it. §3.2, §3.3 and §3.4 are the three worked
  examples and set the bar. **"Low severity" is not an argument. "Denial of service
  only" is not an argument.** `SPEC_CS.md` §19 ranks security above finishing a hand
  conveniently, and a DoS on the DHT thread is how a table fails to form.
* **Tier 3 — recorded, no argument required.** A crate in §5.12 with no path from
  attacker-controlled bytes. The recording still happens, because the §5.12
  assumption is exactly what a Tier 3 acceptance is betting on.

**Where the record goes.** A Tier 2 or Tier 3 acceptance is a row in §4 plus, if it
needs more than a cell, a subsection in §3 — in this document, in the same commit
that changes `Cargo.lock`. A Tier 1 finding, or any acceptance that changes what
ships or narrows a security claim, is **a numbered decision in `DECISIONS.md`**,
because that is the file that outranks this one. Dropping the libp2p `dns` feature
would be such a decision: it removes two advisories and narrows D-004, and it is
already on `DECISIONS.md`'s open list rather than being taken quietly here.

**An acceptance expires.** It is valid for the crate version it was written against
and no other. A version bump re-opens it, even a patch bump, because §3's arguments
are about specific lines of upstream source.

---

## 9. Corrections to documents this register supersedes

`docs/research/` is evidence, not authority; the five specification documents are
authority but are not immune. Recorded here so no reader has to open the older file
to learn that a row in it is wrong.

1. **`NETWORK_STACK.md` §5.1.1 registers `libp2p-mdns 0.48.0`, which is not
   compiled**, and **omits `libp2p-kad 0.48.0` and `libp2p-metrics 0.17.0`, which
   are**. `Cargo.toml` enables `kad` and `metrics` and does not enable `mdns`;
   `cargo tree --edges normal` confirms `libp2p-mdns` is absent from the build and
   the other two are present. §5.1.1's own §5.2 feature block also lists `mdns` and
   omits `kad` and `metrics`, and its prose says *"`kad` is **not** enabled in v1"*,
   which the manifest contradicts. **Either the manifest or `NETWORK_STACK.md` §5.2
   is wrong, and this register cannot settle which** — it records what is built. The
   discrepancy needs an owner in `NETWORK_STACK.md`, and if `kad` is meant to be off
   then §3.2's trade-off changes with it. **(a)(b)**
2. **`NETWORK_STACK.md` §5.1.1's closing verification note says "No `cargo audit`
   run has been performed against the assembled transport tree — the tree does not
   exist yet, there is no `Cargo.lock`".** Both conditions are now false: the tree
   compiles, `Cargo.lock` is committed, and §4 is the audit. The note is stale, not
   wrong-in-spirit, and should point here.
3. **`research/INTEGRATION.md` §2 attributes all three `rand` majors to
   `libp2p-autonat` and the arkworks crates.** Measured provenance is wider and only
   partly overlaps: `rand 0.8.8 ← ark-std`; `rand 0.9.5 ← hickory-proto, igd-next,
   yamux`; `rand 0.10.2 ← quinn-proto` **and `rs_poker`**. `libp2p-autonat` pulls
   `rand_core 0.6.4`, not `rand` directly. The `rs_poker` edge is the one that
   matters, because it puts a `rand`-backed deck and sampler inside a crate our own
   poker code calls — §3.6. **(a)**
4. **`research/CRYPTO_LIBS.md` §10's "No `rand` at any depth"** is false in the
   integrated tree — §5.4. **§7.3's `rand` row saying "crate not in the tree at
   all"** is likewise false; the corrected row is in `CRYPTOGRAPHY.md` §9.2. Both
   were already caught; they are repeated here because this is the register a reader
   consults for the tree.
5. **`CRYPTOGRAPHY.md` §9's opening sentence describes ziffle as vendored.** It is
   not, yet — §3.1. The sentence describes the intended mitigation and reads as a
   statement of fact.
6. **The fix plan's A-8 replacement text calls `max_circuit_bytes` "per circuit and
   per direction".** It is a bidirectional total — §5.5, verified in
   `copy_future.rs`. Every derived figure halves.

---

## 10. Reproducing every number in this document

```bash
export PATH="~/.cargo/bin:$PATH"      # cargo is not on the Bash PATH

cargo check                                        # the whole stack builds

# 425: crates compiled into the client (subtract 1 for p2p-poker itself)
cargo tree --edges normal --prefix none | sed 's/ (\*)//' \
  | awk '{print $1" "$2}' | sort -u | wc -l

# 611: crates in the lockfile (subtract 1 for p2p-poker itself)
grep -c '^name = ' Cargo.lock

cargo tree --duplicates --edges normal             # duplicate majors
cargo audit                                        # lockfile-based; over-reports, see §1
cargo tree --edges normal   -i <crate>             # is it really in the build?
cargo tree --edges features -i <crate>             # which features of it are on?
cargo deny check licenses                          # needs a deny.toml first, see §7
```

Licence and repository cells: read `license` and `repository` from
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/<crate>-<version>/Cargo.toml`.
Advisories: `~/.cargo/advisory-db/crates/<crate>/RUSTSEC-*.md`, whose `[versions]`
block carries the authoritative `patched` and `unaffected` ranges — `cargo audit`'s
one-line "Solution" is a summary of them, not a substitute.

crates.io metadata (release count, owners, newest version) needs a User-Agent:

```bash
curl -s -H "User-Agent: p2p-poker-dependency-register (<your-email>)" \
     https://crates.io/api/v1/crates/<crate>
```

---

*This register is complete for the 122 security-critical dependencies it names and
for the licence status of all 425 compiled crates. It is **not** an audit: no crate
in it has been reviewed line by line by us, and the one review `SPEC_CS.md` §36 and
`MENTAL_POKER.md` §9 both call a prerequisite — ziffle's `MultiExpArg` and
`SingleValueProductArg` against Bayer–Groth 2012 — has not been done.*
