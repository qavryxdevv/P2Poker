# Dependency register — `SPEC_CS.md` §28

**Status:** authoritative. This document is the permanent home of the `SPEC_CS.md`
§28 register, assigned by `research/PHASE0_FIXPLAN.md` ruling **D-2**. It replaces
the two interim halves — `CRYPTOGRAPHY.md` §9.1 (cryptographic side) and
`NETWORK_STACK.md` §5.1.1 (transport side) — both of which said in their own text
that they were incomplete and temporary. Those two sections stay where they are as
the *reasoning* for their own crates; this document is the register.

**Authority.** `SPEC_CS.md` binds. **`DECISIONS.md` outranks this document**, and
this document does not restate what is in it — not the decisions, not their numbers.
That is D-011 rule 1 applied here, and it is applied because the line that stood in
this place read *"D-001 … D-008"* four decisions after D-012 was accepted. The five
specification documents come next. `docs/research/` is **evidence, not authority** —
three of its statements are corrected in §9 of this document, and a research note is
never a reason to leave a measured number wrong.

**D-009 rule 3 governs this register**, and is restated in §0 below rather than left
to be inferred, because this is the document where a claimed absence does the most
damage.

**Measured on:** 2026-08-28, `x86_64-pc-windows-msvc`, with
cargo held to **19** by `.cargo/config.toml` (`[build] jobs = 19`; the test harness
has its own pool and needs `--test-threads=19` passed separately),
`rustc 1.95.0 (59807616e 2026-04-14)` / `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`,
RustSec advisory database with 1 226 advisories loaded. Every number in this
document is reproducible by §10. The job ceiling changes nothing in the tables — it
is recorded because §10's commands are run under it and a reproducer should be
running the same build.

---

## 0. What may not be claimed here — D-009 rule 3

> **No security property of this project may be stated as the absence of something
> from the dependency tree.** State the discipline our own code follows, and enforce
> it mechanically.

This register is the document where breaking that rule is most expensive, because a
register is exactly where a reader goes to be told what is and is not present. Three
claimed absences have already been measured false after integration — B-1, B-3 and
M3, the last being *"`SmallRng` is not compiled in"*, which `rand 0.9.5`'s **default**
`small_rng` feature and four dependencies falsify. The pattern is always the same: an
absence verified in an isolated probe does not survive integration, and nobody
re-checks it.

So, for everything in this document:

* A **fact about the build** — "`libp2p-mdns` is locked but not compiled",
  "the DNSSEC module is behind a feature we do not enable" — is allowed, is marked
  **(a)** or **(b)**, and is **never** the same sentence as a security conclusion. It
  is a fact about today's feature set, and §8.1 trigger 1 exists because a feature
  flip elsewhere reverses it silently.
* A **security property** is stated as a discipline our own code follows, with the
  mechanism that enforces it named. §5.4's `getrandom`-only rule is the model:
  `src/security/rng.rs` scans this crate's own sources on every test run, and it was
  verified to fail by injecting a violation. `CONTRIBUTING.md` §1.4 has that record —
  a gate that has never been observed to fail is not a gate.
* The two are never merged. "Not compiled, therefore safe" is the forbidden shape.

**The register was swept against this rule on 2026-08-28. One row needed changing**
— `argon2` in §5.3, which read a parser's absence as a security conclusion; it is
corrected in place and the correction is recorded in §9.7.

**Verification codes**, as used across the corpus:
**(a)** compiled or executed here; **(b)** read from the crate's own source under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`;
**(c)** registry — the crates.io API or the local advisory database.

---

## 1. The two numbers, and why they differ

| Figure | Value | How |
|---|---|---|
| Crates **compiled into the client** | **442** | `cargo tree --edges normal --target x86_64-pc-windows-msvc`, unique name+version, minus `p2p-poker` itself |
| Crates **recorded in `Cargo.lock`** | **645** | `[[package]]` entries, minus `p2p-poker` itself |
| Locked but never compiled | **203** | the difference |

**Re-measured 2026-09-14, after `libp2p` 0.57** (`S1-FF`): 442 / 645 / 203, where the
2026-08-31 figures were 461 / 659 / 198. **Re-measured 2026-08-31** before that: the
compiled figure was right; the other two were not, and had been 646 / 185. `tests/corpus_dependencies.rs` now measures all three on
every `cargo test`, so the next reader gets a failure rather than a number.

`cargo audit` counts the root package too, so its figure is one higher than the
lockfile row above: 660 − 1 = 659; 462 − 1 = 461. The two figures in this table both
exclude the root, so they are directly comparable. **(a)**

**These numbers were 425 / 611 / 186 when this register was first written**, and the
drift is worth naming rather than quietly overwriting: **+13** came from local
discovery and port mapping (`libp2p` `mdns` and `upnp`, `crab_nat`, `netdev`), and
**+23** from the second renderer — `wgpu 30.0.1` and its Direct3D 12 backend, which
is what lets the client start on a machine with no graphics driver. Neither addition
brought a new licence into the tree; §6 has the measurement.

**461 is a lot** against §28's "use the minimum number of dependencies", and it is
recorded here rather than glossed over. It is what libp2p, arkworks and egui cost
together. The option that would have removed roughly twelve of them — all discovery
through Mainline DHT, dropping the libp2p `dns` and `kad` features — went the other
way in `56b0b50`: Mainline left the build and `kad` is the discovery mechanism, and
`DECISIONS.md`'s open list marks the question void. Nothing on the table moves the
number materially.

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

> **What D-014 changed about this entry, and it is not the risk rating.** Under D-010 a
> proof this crate wrongly rejected cost a rejected message and, at worst, a stalled hand.
> Under **D-014** a failed shuffle proof, decryption-share proof or key-ownership proof is
> **tier-1 evidence that removes a player from the table** (`CRYPTOGRAPHY.md` §8.1,
> `STATE_MACHINE.md` T64/T65). So this crate's *false-reject* behaviour, not only its
> soundness, is now load-bearing against a person, and two implementations that verify
> differently would each eject the other and each be right by its own rules. Nothing in the
> row below moves — the version, the pin and the vendoring are unchanged — but **OQ-1**,
> the blocking in-house review, now discharges a second obligation besides deck integrity,
> and `DECISIONS.md` **D-014-2**'s mirror test (no honest peer is evictable under any legal
> interleaving) is the gate that must pass before the removal feature ships.

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

**State of the mitigation — vendored 2026-08-29, and the risk rating does not move.**
`CRYPTOGRAPHY.md` §9 says ziffle is "vendored into the repository at `vendor/ziffle/`
with a `[patch.crates.io]` entry". **That is now done**, as `ZIFFLE_VERDICT.md`
condition **C-0**. `vendor/ziffle/` holds the nine files of the crates.io 0.1.0
tarball, byte-identical to the registry checkout, and `Cargo.toml` carries
`[patch.crates-io] ziffle = { path = "vendor/ziffle" }`. The crates.io sha256
`ba79285194a16b02512566a9a64d885567646045b144bb0efeef662001cd83a5` was recomputed from
the archive and agreed with the pre-vendoring `Cargo.lock` checksum and with the
verdict. **(a)(b)**

**Read `vendor/ziffle/PROVENANCE.md` before touching anything under `vendor/`.** It
owns the digests, the upstream git sha `bcb8e61651cd6d4140c895a414d7ed8bc06d32bd`, and
the licence choice — **we rely on the Apache-2.0 branch**, because the shipped
`LICENSE-MIT` names *"The cargo-readme Developers"*, a copy-paste defect present
upstream that makes the MIT branch unusable as shipped (§6 note 5).

**Two things this changed and two it did not.** It closes the availability risk — a
yank or a repository deletion upstream can no longer take the deck away — and it gives
the review a frozen artefact to be a review *of*, which is what made the review worth
commissioning. It does **not** audit the crate and it does not soften one word of the
rating above: one author, one release, unaudited, and its own README says not to play
for non-trivial money. Vendoring freezes bytes; that is the whole claim. See §8, and
§11 for the fallback if the bytes ever have to be replaced.

### 3.2 `hickory-proto 0.25.2` — two open advisories, one with no fix. **Spent 2026-09-14: `libp2p` 0.57 moved to `hickory` 0.26.3**

> **Both advisories left the lockfile with the 0.25 line** (`S1-FF`). `libp2p` 0.57's
> `libp2p-dns` 0.45 and `libp2p-mdns` 0.49 depend on `hickory` 0.26, and the lockfile holds
> `hickory-proto` and `hickory-resolver` 0.26.3: outside RUSTSEC-2026-0118's affected range
> and past RUSTSEC-2026-0119's fix (`>= 0.26.1`). The entry is kept as written below: it is
> the reasoning the acceptance rested on for as long as it stood.

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
capability to make a scanner quiet is the wrong trade, and since `56b0b50` it is also
how the client reaches the public DHT at all: its one compiled-in entry point is
`/dnsaddr/bootstrap.libp2p.io`. The alternative that was on `DECISIONS.md`'s open
list — all discovery through Mainline DHT, relay volunteers under a second infohash,
dropping both `dns` and `kad` — is void: that commit removed Mainline and made `kad`
the discovery mechanism.

### 3.3 `lru 0.16.4` — RUSTSEC-2026-0253, unsound. **Spent: the crate left the build.**

> **Corrected 2026-08-31.** `lru` is not in `Cargo.lock` and neither is `mainline`,
> the dependency that brought it. `56b0b50` — *"Discovery is libp2p's now, and
> BitTorrent is out of the binary"* — removed the crate; this section, §3.7, §4's
> table row and §5.8 were not told, so an accepted unsoundness stood in the register
> for a crate that is not there. **An accepted advisory is a decision a reader
> relies on**, which is why this is corrected in place rather than deleted: the
> acceptance below was sound when it was made, and what changed is that there is
> nothing left to accept. The measurement is `tests/corpus_dependencies.rs`, which
> now fails if any row in §5 names a crate the lockfile does not have.
>
> §4 carries the confirming run: `cargo audit` on 2026-08-31, 1 233 advisories
> against 660 lockfile packages, and RUSTSEC-2026-0253 is not in the output.

**What was accepted, in the past tense.** Use-after-free / double-free from missing panic safety in `LruCache::pop()`: if a
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

Recorded as **accepted with justification** rather than fixed, because we could not
fix it: the justification was a fact about mainline's usage, not a guarantee, which
is why §8 lists "a pinned crate moves" as a re-audit trigger. What actually ended it
was neither an upgrade nor a re-audit — the dependency was removed for an unrelated
reason, and the register kept the acceptance. **That is the failure this section is
now an example of**, and it is why §8 gains "a crate leaves the build" as a trigger
of its own. The gap was one day — `56b0b50` on 2026-08-30, this correction on
2026-08-31 — which is the argument for the trigger rather than against it: a
register that is wrong within a day of a routine commit is wrong by default, and
only a check that runs makes it right by default.

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

### 3.5 `libp2p-stream 0.5.0-alpha` — semver-exempt, no stable release

An `-alpha` pre-release. Cargo's semver rules do not apply to it: any republish may
change the API or the behaviour with no version signal, and there is no compatibility
promise to appeal to. `libp2p` 0.57 needed `0.5.0-alpha`, the newest published; crates.io
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

### 3.7 `mainline 8.0.0` — panics on IPv6. **Historical: the crate is not in the build.**

> **Corrected 2026-08-31.** `mainline` is not in `Cargo.lock`. Two claims below were
> live instructions to a reader and are now false: the **hard constraint on
> `src/net/dht.rs`** names a file that was deleted with the crate, and
> **"discovery is IPv4-only"** was a property of this dependency, not of the client.
> Discovery is a libp2p Kademlia provider record (`net::run::lobby_namespace`), which
> carries whatever multiaddrs a node has and is not IPv4-only. Kept, in the past
> tense, because §8's re-audit triggers are argued from it.

**What the crate did, while it was in the build.**

`KrpcSocket::new` reads the bound socket's local address and matches on it:

```
SocketAddr::V6(_) => unimplemented!("KrpcSocket does not support Ipv6"),
```

`mainline-8.0.0/src/rpc/socket.rs:54`. **(b)** `unimplemented!` is a panic. If the
client binds the DHT socket to an IPv6 address, the DHT thread dies — not with an
error we can handle, with an unwind. Binding the DHT socket had to be IPv4-only, and
that was a hard constraint on the `net::dht` module rather than a preference. Both
the module and the constraint are gone.

Inbound IPv6 packets are handled differently and are not a panic: `src/rpc/socket.rs:207`
matches `Ok((_, SocketAddr::V6(_)))` and only emits a trace. **(b)** So the failure
mode is entirely on our side of the API — it is a bind-time constraint.

The wider consequence, which `research/MAINLINE_DHT.md` §107–109 already states:
BEP 32's IPv6 DHT is a separate routing table, so discovery **was** IPv4-only and on
an IPv6-only network the client discovered nobody. That was a permanent limitation of
this dependency; it left with it.

`mainline` is also `=`-pinned because `NETWORK_STACK.md` §11.4 depends on internals
(`RequestFilter`, adaptive server mode) that are not semver-stable in practice.

---

## 4. Open advisories — the complete `cargo audit` result

**Re-run 2026-09-14 against `Cargo.lock`, after `libp2p` 0.57 and `rustls` 0.23.45, 1 246
advisories loaded, 646 lockfile packages scanned. This is the whole output. (a)**

```
Scanning Cargo.lock for vulnerabilities (646 crate dependencies)
paste         1.0.15  RUSTSEC-2024-0436  Warning: unmaintained
warning: 1 allowed warning found
```

**`cargo audit` exits zero for the first time since this register was written.** Both
`hickory-proto` rows are spent (§3.2), and so is the advisory GitHub raised on `yamux`
0.12.1 (GHSA-vxx9-2994-q338, a remote panic on a malformed data frame, fixed `>= 0.13.10`):
`libp2p-yamux` 0.48 links one major, `yamux` 0.14.0. One advisory appeared the same day and
was fixed before it was ever in a release: **RUSTSEC-2026-0285** on `rustls` 0.23.43 (TLS 1.3
handshake messages accepted across encryption-level boundaries, fixed `>= 0.23.45`), taken by
`cargo update -p rustls`. The earlier runs are kept below as they stood.

> **Superseded 2026-09-14** -- the rest of this section describes the 0.56 tree.

**Re-run 2026-08-31 against `Cargo.lock`, 1 233 advisories loaded, 660 lockfile
packages scanned. This is the whole output, not a selection. (a)** The count matches
§1's `[[package]]` figure exactly, which is the cheapest check that the scanner saw
the file this document describes.

```
Scanning Cargo.lock for vulnerabilities (660 crate dependencies)
hickory-proto 0.25.2  RUSTSEC-2026-0118  Solution: No fixed upgrade is available!
hickory-proto 0.25.2  RUSTSEC-2026-0119  Solution: Upgrade to >=0.26.1
paste         1.0.15  RUSTSEC-2024-0436  Warning: unmaintained
error: 2 vulnerabilities found!
warning: 1 allowed warning found
```

**One row left the set and it is the one this pass was looking for.** `lru 0.16.4`
/ RUSTSEC-2026-0253 is gone, because `lru` is gone: it arrived under `mainline`, and
`56b0b50` took BitTorrent out of the binary. §3.3 had kept it as an **accepted**
unsoundness ever since. The allowed-warning count moved with it, 2 to 1, and that is
the number CI gates on.

**The two `hickory-proto` vulnerabilities and the `paste` warning are unchanged** in
ID, version and solution from the 2026-08-28 run, so §3.2's and §3.4's acceptances
stand as written.

> **The superseded 2026-08-28 run is kept below**, because §1's *"locked but never
> compiled"* argument is made against its figures and because a register that
> silently replaces a measurement teaches a reader to trust the next one without
> checking. It scanned 612 packages where there are now 660.

Run 2026-08-28 against `Cargo.lock`, 1 226 advisories loaded, 612 lockfile packages
scanned. This was the whole output, not a selection. **(a)**

**Re-run 2026-08-28 at the close of the D-009…D-012 sweep, and the result is
unchanged**: same four IDs, same versions, same solutions, same exit. Verbatim, the
lines that decide the table:

```
Scanning Cargo.lock for vulnerabilities (612 crate dependencies)
hickory-proto 0.25.2  RUSTSEC-2026-0118  Solution: No fixed upgrade is available!
hickory-proto 0.25.2  RUSTSEC-2026-0119  Solution: Upgrade to >=0.26.1
paste         1.0.15  RUSTSEC-2024-0436  Warning: unmaintained
lru           0.16.4  RUSTSEC-2026-0253  Warning: unsound
error: 2 vulnerabilities found!
warning: 2 allowed warnings found
```

**`cargo audit` reads `Cargo.lock`, so it is feature-blind, and that has not changed
either.** The current run scanned all 660 lockfile packages, not the 461 that are
compiled. Two of the rows below are only decidable by `cargo tree --edges normal -i`
— see §1, and the "Compiled?" column exists because of it. The `lru` row is struck
through rather than deleted: it is the worked example of why the column is there,
and of a register outliving the thing it registers.

| ID | Crate | Version | Class | Fix available | Compiled? | Status |
|---|---|---|---|---|---|---|
| RUSTSEC-2026-0118 | `hickory-proto` | 0.25.2 | vulnerability — DoS | **none** | crate yes, **vulnerable module no** | accepted, §3.2 |
| RUSTSEC-2026-0119 | `hickory-proto` | 0.25.2 | vulnerability — DoS | `>= 0.26.1`, blocked by `libp2p-dns` | **yes, reachable** | accepted, §3.2 |
| RUSTSEC-2024-0436 | `paste` | 1.0.15 | warning — unmaintained | none, ever | build-time proc-macro only | accepted, §3.4 |
| ~~RUSTSEC-2026-0253~~ | ~~`lru`~~ | ~~0.16.4~~ | warning — unsound | — | **no — not in `Cargo.lock`** | **spent 2026-08-31, §3.3** |

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

> **Re-run 2026-08-31, and it does not hold: 7 of the 122 rows name a crate that is
> not in `Cargo.lock`.** `serde_bencode` and `serde_bytes` in §5.7, and five of
> §5.8's six. They left with `mainline` in `56b0b50`, and the register was not
> told. The pass below is kept because its *method* is the right one, and because
> deleting a verification claim that turned out false is how the next one gets
> believed too easily - but read it as a record of a run, not as the state of this
> document.
>
> The claim also disagreed with §1 before this correction: it asserts §1's
> figures are **425 / 611 / 186** while §1's own table said 461 / 646 / 185, so
> two statements of the same three numbers had already drifted apart inside one
> file. Measured today: **461 / 659 / 198**. `tests/corpus_dependencies.rs` runs the
> row check and the three counts on every `cargo test`, which is the difference
> between a claim and a gate.
>
> **What follows is the 2026-08-28 run, unedited.**
>
> **Re-verified 2026-08-28, mechanically, against the lockfile and the registry
> checkouts rather than against the earlier text of this document.** Every
> `name`+`version` in every §5 table was matched against a `[[package]]` entry in
> `Cargo.lock`, and every `licence` and `repository` cell was re-read from
> `<crate>-<version>/Cargo.toml` under the local registry checkout. **All 122 rows
> matched; nothing had drifted.** The 25 four-column rows in §5.5 were checked
> against their section's blanket claim as well — all 25 really do carry licence
> `MIT` and repository `github.com/libp2p/rust-libp2p`. The lockfile holds 612
> `[[package]]` entries (611 excluding the root) and `cargo tree --edges normal`
> yields 426 unique `name version` pairs (425 excluding the root), so §1's
> 425 / 611 / 186 all still hold. **(a)(b)**
>
> A matching pass is worth as little as its coverage, so the coverage is stated: the
> check parsed 126 version-bearing rows, of which 4 belong to §6's licence-count
> table and are not crate rows, leaving the 122 the register claims. A run that
> matched fewer rows than that would be a silent pass, not a clean one.
>
> `rs_poker =5.0.0` is among them and is current: it is registered in §5.10, its `=`
> pin is justified in §3.6, `Cargo.toml` declares
> `rs_poker = { version = "=5.0.0", default-features = false }`, and `Cargo.lock`
> resolves exactly `5.0.0`. It was **not** missing from this register.

### 5.1 Mental poker — the deck (8)

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `ziffle` | 0.1.0 | Barnett–Smart mental poker; Bayer–Groth 2012 shuffle proof, DLEQ, Schnorr | `github.com/v26-solutions/ziffle` | declared `MIT OR Apache-2.0`; **effective: Apache-2.0** — §6 note 5 | **unaudited, semver-unstable (0.x, one release, one author); its own README says not to play for non-trivial money.** In-house review of `MultiExpArg` / `SingleValueProductArg` is a **prerequisite** (§3.1, OQ-1). **Vendored** at `vendor/ziffle/`, sha256 `ba79285…` verified — `vendor/ziffle/PROVENANCE.md`. Designated fallback: §11 |
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
| `ed25519-dalek` | 3.0.0 | application event signatures, `verify_strict` only | `github.com/dalek-cryptography/curve25519-dalek/tree/main/ed25519-dalek` | BSD-3-Clause | no open advisory; RUSTSEC-2022-0093 patched `>= 2`. **Never enable `hazmat`** (`CRYPTOGRAPHY.md` §10.1). `mainline` also linked it, for DHT mutable items, until that crate left the build in `56b0b50` |
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
| `argon2` | 0.6.0 | passphrase → KEK for the profile key slots | `github.com/RustCrypto/password-hashes` | MIT OR Apache-2.0 | no open advisory. **The discipline:** our code calls Argon2 as a raw KDF only and never parses or emits a PHC string, so no attacker-supplied encoding reaches a parser through this crate. **The build fact, which is not the same claim:** the `password-hash` feature is off today, so `password-hash`, `phc`, `pkcs8`, `spki` and `der` are locked but not compiled **(a)(b)** — a feature flip restores them silently (§8.1 trigger 1), and the discipline is what still holds when it does. §0 |
| `blake2` | 0.11.0 | BLAKE2b inside Argon2 | `github.com/RustCrypto/hashes` | MIT OR Apache-2.0 | no open advisory |
| `chacha20poly1305` | 0.11.0 | XChaCha20-Poly1305 profile AEAD | `github.com/RustCrypto/AEADs` | Apache-2.0 OR MIT | no open advisory |
| `chacha20` | 0.10.2 | the stream cipher under it | `github.com/RustCrypto/stream-ciphers` | MIT OR Apache-2.0 | no open advisory |
| `poly1305` | 0.9.1 | the MAC under it | `github.com/RustCrypto/universal-hashes` | Apache-2.0 OR MIT | no open advisory |
| `aead` | 0.6.1 | AEAD traits | `github.com/RustCrypto/traits` | MIT OR Apache-2.0 | no open advisory |
| `cipher` | 0.5.2 | block/stream cipher traits | `github.com/RustCrypto/traits` | MIT OR Apache-2.0 | no open advisory |
| `universal-hash` | 0.6.1 | universal-hash traits | `github.com/RustCrypto/traits` | MIT OR Apache-2.0 | no open advisory |
| `windows-sys` | 0.61.2 | DPAPI key slot on Windows; since D-048 also the table's sounds through WinMM (`waveOut*`, features `Win32_Media` and `Win32_Media_Audio`) | `github.com/microsoft/windows-rs` | MIT OR Apache-2.0 | no open advisory; Windows-only path, and the only OS-keystore path implemented |

### 5.4 Randomness — the whole set, including what we do not use (8)

`SPEC_CS.md` §7 is a rule about *our* code, not an absence in the tree. All eleven of
these are compiled (eleven until `libp2p` 0.57 took the `rand 0.9` line out). Only the first two may ever be a source of protocol randomness.

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `getrandom` | 0.4.3 | **the OS CSPRNG** — `getrandom::fill`, `getrandom::SysRng` | `github.com/rust-random/getrandom` | MIT OR Apache-2.0 | no open advisory. **The only permitted source of cryptographic randomness.** API verified: `pub use sys_rng::SysRng` at `src/lib.rs:34`, `pub fn fill` at `:87` **(b)** |
| `rand_core` | 0.10.1 | the generator traits our OS-CSPRNG adapters implement -- `security::rng::OsRng10` is what `libp2p` 0.57's AutoNAT v2 client is handed | `github.com/rust-random/rand_core` | MIT OR Apache-2.0 | no open advisory |
| `rand` | 0.8.8 | required by `ark-std` with `std_rng`; **not** a source of protocol randomness | `github.com/rust-random/rand` | MIT OR Apache-2.0 | RUSTSEC-2026-0097 (`unsound`) patched at `>= 0.8.6`; 0.8.8 is patched, and the unsound path (`thread_rng` inside a custom `log` impl) does not occur here — §4 |
| `rand` | 0.10.2 | pulled by `quinn-proto`, **`rs_poker`**, and since `libp2p` 0.57 by most of `libp2p` (`-core`, `-swarm`, `-identity`, `-gossipsub`, `-kad`, `-noise`, `-autonat` ...), `hickory` and `igd-next` | `github.com/rust-random/rand` | MIT OR Apache-2.0 | no open advisory. See §3.6: `rs_poker::core`'s deck and sampler use it and must never be called |
| `rand_chacha` | 0.3.1 | backs `StdRng` inside `rand 0.8` | `github.com/rust-random/rand` | MIT OR Apache-2.0 | no open advisory |
| `rand_core` | 0.6.4 | under `rand 0.8` and `rand_chacha 0.3`, for `ark-std` | `github.com/rust-random/rand` | MIT OR Apache-2.0 | no open advisory; coexists with 0.10 as a distinct Rust type |
| `rand_pcg` | 0.10.2 | PCG generator used by `quinn-proto` for connection IDs | `github.com/rust-random/rngs` | MIT OR Apache-2.0 | no open advisory; **a non-cryptographic generator in the tree** — never reachable from our code |
| `ark-std` | 0.5.0 | `no_std` shims; **the crate that reintroduces `rand`** | `github.com/arkworks-rs/std` | `MIT/Apache-2.0` (deprecated SPDX form — §6) | no open advisory; not audited |

`getrandom 0.2.17` (via `rand_core 0.6.4` and `ring`) and `getrandom 0.3.4` (via `snow`)
are also compiled,
same repository and licence as 0.4.3, no open advisory. **(a)**

> **The claim that must not be restated.** `research/CRYPTO_LIBS.md` §10 says of its
> dependency block: *"No `rand` at any depth."* That was true of its isolated probe
> and is **false in the integrated tree** — two majors of `rand` and two of
> `rand_core` are compiled (three of each before `libp2p` 0.57). The discipline survives; the absence does not. What
> `SPEC_CS.md` §7 actually requires is enforceable as a lint (`CRYPTOGRAPHY.md` §12
> item 6, OQ-8): our own code draws cryptographic randomness only from
> `getrandom::SysRng`; `SmallRng`, `StdRng` and any self-seeded generator are never
> used for keys, masking factors, permutations or commitments.

### 5.5 Transport — libp2p (25)

All 25 share repository `github.com/libp2p/rust-libp2p` and licence **MIT**. All are
resolved by the `libp2p 0.57.0` umbrella except `libp2p-stream`, which is declared
directly (§3.5). No open advisory on any of them at these versions; the historical
`libp2p` and `libp2p-core` advisories are in §4.

| Name | Version | Purpose | Security status |
|---|---|---|---|
| `libp2p` | 0.57.0 | umbrella: transport, encryption, NAT traversal, gossip, relay | RUSTSEC-2022-0084 patched `>= 0.45.1`; pinned version is patched |
| `libp2p-core` | 0.44.0 | transport traits, connection upgrades | RUSTSEC-2019-0004 (`>= 0.8.1`) and RUSTSEC-2022-0009 (`>= 0.31.1`) both far below 0.43.2 |
| `libp2p-identity` | 0.3.0 | `PeerId`, transport `Keypair` | no open advisory. **Never depend on it directly**: `libp2p-identity 0.3.0` exists outside the umbrella's `^0.2.12` range and linking it gives a second, incompatible `PeerId` as a *type mismatch*, not a resolver error |
| `libp2p-swarm` | 0.48.0 | swarm driver | no open advisory |
| `libp2p-swarm-derive` | 0.36.0 | `#[derive(NetworkBehaviour)]` | no open advisory; proc-macro |
| `libp2p-quic` | 0.14.0 | primary transport; has a real `hole_punching` module | no open advisory |
| `libp2p-tcp` | 0.45.0 | fallback transport for UDP-blocked networks | no open advisory |
| `libp2p-dns` | 0.45.0 | `/dnsaddr/` resolution for relay bootstrap | the path to `hickory` 0.26.3; both 0.25 advisories spent — §3.2 |
| `libp2p-noise` | 0.47.0 | Noise XX security upgrade; mandatory for relay | no open advisory. Selects `snow`'s `ring-resolver`, so the handshake AEAD and hash come from `ring`, not from RustCrypto — §5.6 |
| `libp2p-tls` | 0.7.0 | TLS 1.3 security upgrade | no open advisory. **Since 0.7 its `rustls` runs on AWS-LC**, post-quantum key exchange preferred -- §5.6 |
| `libp2p-yamux` | 0.48.0 | stream muxer; mandatory for relay | no open advisory. links one `yamux` major since 0.48 (two before, §5.6) |
| `libp2p-gossipsub` | 0.50.0 | lobby topics | no open advisory. Parses attacker-controlled protobuf from unauthenticated peers |
| `libp2p-kad` | 0.49.0 | Kademlia DHT | no open advisory. **Enabled in `Cargo.toml`; `NETWORK_STACK.md` §5.2 says it is not** — §9 |
| `libp2p-identify` | 0.48.0 | address candidates | no open advisory. Peer-supplied addresses are attacker-controlled input |
| `libp2p-ping` | 0.48.0 | liveness | no open advisory |
| `libp2p-autonat` | 0.16.0 | reachability probing | no open advisory. Pulls `rand_core 0.6.4` |
| `libp2p-dcutr` | 0.15.0 | direct connection upgrade through relay (hole punching) | no open advisory |
| `libp2p-relay` | 0.22.0 | Circuit Relay v2 (**D-001**, **D-002**) | no open advisory. **`max_circuit_bytes` is a bidirectional total, not per direction** — see the box below |
| `libp2p-request-response` | 0.30.0 | snapshot RPC, join RPC | no open advisory |
| `libp2p-stream` | 0.5.0-alpha | per-table streams | **unaudited and semver-exempt; no stable release exists** — §3.5 |
| `libp2p-upnp` | 0.7.0 | IGD port mapping | no open advisory. Speaks HTTP to a LAN device that is not authenticated — §5.9 |
| `libp2p-connection-limits` | 0.7.0 | connection caps | no open advisory. **There is no `connection-limits` cargo feature** — it is a non-optional dependency and `libp2p::connection_limits` is always available; asking for the feature is a hard resolver error |
| `libp2p-memory-connection-limits` | 0.6.0 | memory-based caps | no open advisory. This one *is* a feature, spelled `memory-connection-limits` |
| `libp2p-allow-block-list` | 0.7.0 | peer blocklist | no open advisory; non-optional, like `connection-limits`. **Its presence in the build is not permission to drive it from a protocol proof** — D-010 point 3 and D-011 rule 3 forbid automated eviction at every layer, transport included, and this crate is the transport-layer mechanism they were written about. A user-initiated block is a user decision and is fine; a block triggered by an `EquivocationProof` or a `TIMEOUT_CERT` is a defect. `NETWORK_STACK.md` owns the rule (D-011). **D-014 does not reopen this and the distinction is the layer:** a removal for cause takes a seat out of the *table* — dead, blinded off, one-way (`STATE_MACHINE.md` T64, T65, I34) — and never out of the *transport*. The removed peer keeps its connections, and an implementation that reaches for this crate on a `CheatProven` has rebuilt the eviction D-011 rule 3 forbids, with the one decision that sounds like a licence for it |
| `libp2p-metrics` | 0.18.0 | Prometheus metrics | no open advisory. Enabled in `Cargo.toml`; absent from `NETWORK_STACK.md` §5.1.1 — §9 |

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

### 5.6 Transport cryptography and muxing, transitive (23)

This is where the wire encryption actually lives. None of it is named in
`Cargo.toml`, and none of it was in either interim register — which is precisely why
a register generated from the build graph, not from the manifest, is the right shape.

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `ring` | 0.17.14 | the primitives behind Noise and the transport identity -- AEAD, X25519, digests, RNG; still linked by `libp2p-tls`, `libp2p-quic`, `quinn-proto` and `rcgen` | `github.com/briansmith/ring` | Apache-2.0 AND ISC | no open advisory (RUSTSEC-2025-0010 is "< 0.17 unmaintained"; ours is 0.17.14). BoringSSL-derived C and assembly |
| `aws-lc-rs` | 1.18.1 | **the primitives behind TLS and QUIC since `libp2p` 0.57** -- `rustls`' provider, with ML-KEM for the post-quantum exchange | `github.com/aws/aws-lc-rs` | ISC AND (Apache-2.0 OR ISC) | no open advisory. Chosen by `libp2p-tls` 0.7 and `libp2p-quic` 0.14 in their own manifests; no feature of ours turns it off |
| `aws-lc-sys` | 0.45.0 | AWS-LC itself, compiled from C by its build script | `github.com/aws/aws-lc-rs` | ISC AND (Apache-2.0 OR ISC) AND Apache-2.0 AND MIT AND BSD-3-Clause AND (Apache-2.0 OR ISC OR MIT) AND (Apache-2.0 OR ISC OR MIT-0) | no open advisory. **Now the largest non-Rust attack surface in the build**, beside `ring`. Its C put the builder's profile path into the first release 144 times (MSVC keeps `__FILE__`); `tools/remap-build-paths.ps1` adds `CFLAGS=/d1trimfile:` and `build.rs` refuses a release without it (`S1-FF`) |
| `untrusted` | 0.9.0 | `ring`'s bounds-checked input reader; every byte `ring` parses goes through it | `github.com/briansmith/untrusted` | **ISC** | no open advisory. Tiny by design and that is the point — it exists so `ring` cannot read past a buffer |
| `rustls` | 0.23.45 | TLS 1.3 for `libp2p-tls` and QUIC | `github.com/rustls/rustls` | Apache-2.0 OR ISC OR MIT | no open advisory; RUSTSEC-2024-0399 patched `>= 0.23.18`, RUSTSEC-2026-0285 patched `>= 0.23.45`. **On `aws-lc-rs` since `libp2p` 0.57** (`libp2p-tls` 0.7 names the feature itself), with `prefer-post-quantum` |
| `rustls-webpki` | 0.103.15 | certificate path validation | `github.com/rustls/webpki` | **ISC** | no open advisory; parses attacker-supplied certificates |
| `rustls-pki-types` | 1.15.1 | PKI type definitions | `github.com/rustls/pki-types` | MIT OR Apache-2.0 | no open advisory |
| `futures-rustls` | 0.26.0 | futures adapter for rustls | `github.com/quininer/futures-rustls` | `MIT/Apache-2.0` (deprecated SPDX form) | no open advisory |
| `snow` | 0.10.0 | Noise protocol framework | `github.com/mcginty/snow` | Apache-2.0 OR MIT | no open advisory; RUSTSEC-2024-0011 patched below this version. Built with `ring-resolver`, not `default-resolver`, so its pure-Rust resolver is not compiled (§1) |
| `x25519-dalek` | 3.0.0 | Noise static keypair DH | `github.com/dalek-cryptography/curve25519-dalek/tree/main/x25519-dalek` | BSD-3-Clause | no open advisory |
| `curve25519-dalek` | 5.0.0 | group arithmetic for `x25519-dalek`, `ed25519-dalek` and so `libp2p-identity` | as §5.2 | BSD-3-Clause | no open advisory. **Since `libp2p` 0.57 the transport identity is on the 5.x line too**; 4.1.3, which sat exactly on RUSTSEC-2024-0344's patch boundary, is locked and compiled by nobody |
| `ed25519-dalek` | 3.0.0 | `PeerId` signatures inside `libp2p-identity` | as §5.2 | BSD-3-Clause | no open advisory. **Since `libp2p` 0.57 the same version as ours** -- it was a different Rust type (2.2.0), and that no longer separates the identities. What does is `libp2p-identity`'s own newtypes: the transport key is a `libp2p::identity::Keypair`, never a `SigningKey`, so `SPEC_CS.md` §20's separation still holds at every signature in our code |
| `ed25519` | 3.0.0 | signature encoding for the above | `github.com/RustCrypto/signatures/tree/master/ed25519` | Apache-2.0 OR MIT | no open advisory |
| `signature` | 3.0.0 | signer/verifier traits for the above | `github.com/RustCrypto/traits/tree/master/signature` | Apache-2.0 OR MIT | no open advisory |
| `sha2` | 0.10.9 | Fiat–Shamir hash inside ziffle; digests in libp2p | `github.com/RustCrypto/hashes` | MIT OR Apache-2.0 | no open advisory |
| `digest` | 0.10.7 | hash traits for `sha2 0.10` | `github.com/RustCrypto/traits` | MIT OR Apache-2.0 | no open advisory |
| `hkdf` | 0.13.0 | key derivation in the transport handshakes | `github.com/RustCrypto/KDFs/` | MIT OR Apache-2.0 | no open advisory |
| `hmac` | 0.13.0 | MAC under HKDF | `github.com/RustCrypto/MACs` | MIT OR Apache-2.0 | no open advisory |
| `rcgen` | 0.13.2 | generates the self-signed libp2p TLS certificate | `github.com/rustls/rcgen` | MIT OR Apache-2.0 | no open advisory; handles our transport private key |
| `quinn` | 0.11.11 | QUIC endpoint | `github.com/quinn-rs/quinn` | MIT OR Apache-2.0 | no open advisory |
| `quinn-proto` | 0.11.17 | QUIC state machine — parses every UDP datagram | `github.com/quinn-rs/quinn` | MIT OR Apache-2.0 | no open advisory; RUSTSEC-2026-0037 (`>= 0.11.14`) and RUSTSEC-2026-0185 (`>= 0.11.15`) both patched here |
| `quinn-udp` | 0.5.15 | platform UDP socket layer | `github.com/quinn-rs/quinn` | MIT OR Apache-2.0 | no open advisory |
| `yamux` | 0.14.0 | stream muxer | `github.com/paritytech/yamux` | Apache-2.0 OR MIT | no open advisory. **The only major since `libp2p-yamux` 0.48**: 0.12.1, the compatibility copy `libp2p-yamux` 0.47 linked on purpose, carried GitHub's GHSA-vxx9-2994-q338 (a remote panic on a data frame with SYN set and a length of 262 145) and left with it |

### 5.7 Hostile-input parsers (19)

Everything here decodes bytes an attacker chooses. `SPEC_CS.md` §27 wants these
fuzzed; none of them has been fuzzed by us.

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `hickory-proto` | 0.26.3 | DNS wire format | `github.com/hickory-dns/hickory-dns` | MIT OR Apache-2.0 | no open advisory; RUSTSEC-2026-0118 and -0119, which 0.25.2 carried, are both behind it — §3.2 |
| `hickory-resolver` | 0.26.3 | DNS resolution for `/dnsaddr/` | `github.com/hickory-dns/hickory-dns` | MIT OR Apache-2.0 | no advisory of its own; with `hickory-net` 0.26.3, the importers of `hickory-proto` |
| `x509-parser` | 0.18.1 | parses peer TLS certificates | `github.com/rusticata/x509-parser.git` | MIT OR Apache-2.0 | no open advisory |
| `asn1-rs` | 0.7.2 | ASN.1 decoding under the above | `github.com/rusticata/asn1-rs.git` | MIT OR Apache-2.0 | no open advisory |
| `der-parser` | 10.0.0 | DER decoding under the above | `github.com/rusticata/der-parser.git` | MIT OR Apache-2.0 | no open advisory |
| `oid-registry` | 0.8.1 | OID lookup for the above | `github.com/rusticata/oid-registry.git` | MIT OR Apache-2.0 | no open advisory |
| `yasna` | 0.5.2 | ASN.1 writer used by `rcgen` | `github.com/qnighy/yasna.rs` | MIT OR Apache-2.0 | no open advisory |
| `prost-codec` | 0.4.0 | protobuf framing for gossipsub, identify, relay, dcutr, kad and autonat since `libp2p` 0.57 | `github.com/libp2p/rust-libp2p` | MIT | no open advisory; **every libp2p behaviour message passes through it**, and through `prost` beneath it. `quick-protobuf` left with 0.56 |
| `prost` | 0.14.4 | protobuf decoding of the peer public key inside `libp2p-identity` | `github.com/tokio-rs/prost` | **Apache-2.0** (not dual) | no open advisory. Decodes bytes offered by an unauthenticated dialer **before** the `PeerId` is established, so it runs earlier than any authentication we control |
| `unsigned-varint` | 0.8.0 | length prefixes on every libp2p frame | `github.com/paritytech/unsigned-varint` | MIT | no open advisory |
| `asynchronous-codec` | 0.7.0 | framed codec around the above | `github.com/mxinden/asynchronous-codec` | MIT | no open advisory; **where a missing length cap becomes an allocation DoS** |
| `multihash` | 0.19.5 | `PeerId` encoding | `github.com/multiformats/rust-multihash` | MIT | no open advisory |
| `multiaddr` | 0.19.0 | parses peer-supplied addresses | `github.com/multiformats/rust-multiaddr` | MIT | no open advisory; input arrives from `identify` and from the DHT |
| `bs58` | 0.5.1 | base58 in `PeerId` text form | `github.com/Nullus157/bs58-rs` | `MIT/Apache-2.0` (deprecated SPDX form) | no open advisory |
| `data-encoding` | 2.11.1 | base32/base64 in multiaddr | `github.com/ia0/data-encoding` | MIT | no open advisory |
| `idna` | 1.1.0 | IDNA in URL parsing | `github.com/servo/rust-url` | MIT OR Apache-2.0 | no open advisory; RUSTSEC-2024-0421 patched `>= 1.0.0` |
| `url` | 2.5.8 | URL parsing for UPnP control endpoints | `github.com/servo/rust-url` | MIT OR Apache-2.0 | no open advisory |
| `serde` | 1.0.229 | derive-based decoding across the tree | `github.com/serde-rs/serde` | MIT OR Apache-2.0 | no open advisory. **Not used for our signed envelope** — that is `minicbor`, because `serde` does not guarantee a canonical encoding |
| `bytes` | 1.12.1 | buffer type carrying all decoded frames | `github.com/tokio-rs/bytes` | MIT | no open advisory |

### 5.8 Discovery — Mainline DHT. **Removed from the build; kept as a record (0)**

> **Corrected 2026-08-31.** Five of the six crates below are not in `Cargo.lock`:
> `mainline`, `lru`, `sha1_smol`, `crc` and `flume`. `futures-lite` is still in the
> lockfile but arrives by another road entirely — `async-io` under `if-watch`, which
> `libp2p-mdns`, `libp2p-quic` and `libp2p-tcp` pull on non-Windows targets — and it
> is **not in the host build** that §1 measures, so it is not re-registered here. `56b0b50` — *"Discovery is libp2p's now, and BitTorrent is
> out of the binary"* — removed the group; this section, §3.3, §3.7, §4's table and
> §5.7's two bencode rows all kept describing it as current, and §3.3 kept an
> **accepted security advisory** alive for a crate that is not there.
>
> Discovery today is a libp2p Kademlia provider record. `net::run::lobby_namespace`
> keys it on `sha2-256("p2p-poker/main-lobby/v1")` and `relay_namespace` on
> `sha2-256("/libp2p/relay")`; the crates are in §5.5. The register's own infohash
> constants outlived the crate in `src/protocol/constants.rs` too, guarded by two
> compile-time assertions and a test that all passed while reaching nothing — they
> are deleted in the same pass as this correction.
>
> **The rows are not restored here**, because a register lists what is compiled and
> none of these are. What is kept is why they were here, since §8's re-audit triggers
> and §3.3's reasoning are argued from them.

The group that was here held `mainline 8.0.0` (BitTorrent Mainline DHT client) with
`lru` for its caches, `sha1_smol` for infohash computation, `crc` for BEP 42 secure
node IDs, `flume` for the channel between the DHT thread and the client, and
`futures-lite` for its async adapters. `mainline` also linked `ed25519-dalek 3.0.0`
— the *same* crate as our application signatures (§5.2) — for BEP 44 mutable items;
the crate was shared and the keys were not, and must not be.

### 5.9 UPnP / IGD — the LAN attack surface (2)

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `igd-next` | 0.17.1 | IGD port mapping via `libp2p-upnp` | `github.com/dariusc93/rust-igd` | MIT | no open advisory. Talks SSDP/HTTP/XML to whatever on the LAN answers as a gateway — the device is discovered, never authenticated |
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

The remaining **339** compiled crates are not individually registered. They are
dominated by the GUI and its platform stack (`eframe 0.36.1`, `egui_extras 0.36.1`,
`image 0.25.10`, `rust-embed 8.12.0`, winit, glow, fonts, clipboard), the second
renderer (`wgpu 30.0.1` with its `dx12` backend only, `wgpu-core`, `wgpu-hal`,
`naga`, `gpu-allocator`, `egui-wgpu` — 23 crates, all of them shader compilation and
graphics-API plumbing), plus logging
(`tracing 0.1.44`), proc-macro machinery, and small utility crates; a handful are
duplicate minor versions of crates already registered above (`getrandom 0.2.17` and
`0.3.4`, `minicbor-derive 0.19.5`, `ark-serialize-derive 0.5.0`), named in the prose
of §5 without a row of their own. They are covered by the whole-tree licence sweep
(§6) and by `cargo audit` over the whole lockfile (§4), and by nothing else.

**The assumption that makes that exclusion defensible, stated so it can be
attacked:** no attacker-controlled bytes reach them. The table image and the fonts
are compiled into the binary by `rust-embed`, so the PNG and font decoders only ever
see our own assets. The same holds for what D-048 took from PokerTH: the table
picture, the font, the sounds and the icons are compiled in with `include_bytes!` and
`include_str!`, and our own readers of them -- the WAV reader in `sound.rs`, the SVG
path filler in `gui/table/icons.rs` -- never see a byte from a peer. The notes about
players (`storage/notes.rs`) are the local player's own file. The same holds for the renderer: `naga` compiles shaders, and the
only shaders it ever sees are `egui`'s own, compiled in. **This assumption fails the moment anything peer-supplied is
rendered** — an avatar, a table skin, a chat message with an image, a downloaded
theme. If any of that is ever added, `image`, the font stack and the clipboard path
move into §5.7 and are registered individually. That is a §8 re-audit trigger.

---

## 6. Licence sweep over the whole compiled tree

Every one of the 461 compiled dependencies has a licence expression in its own
`Cargo.toml`. **There is no dependency whose licence could not be established.**
**(a)** `cargo deny check licenses` reports exactly one `unlicensed` error, and it is
`p2p-poker` itself.

Distribution over the 461, counted from `cargo metadata`'s `license` field. The
groups sum to 461 exactly. **(a)**

| Licence expression | Count | Notes |
|---|---|---|
| MIT and/or Apache-2.0 only, in any spelling or order | 391 | includes 93 MIT-only, 9 Apache-2.0-only (among them **`rs_poker`**, `prost`, `winit`, `glutin*`, `codespan-reporting`), and the 20 deprecated slash forms in note 3 below |
| `Unicode-3.0` | 18 | the ICU crates under `idna` |
| `BSD-3-Clause` | 7 | `ed25519-dalek` ×2, `curve25519-dalek` ×2, `x25519-dalek`, `subtle`, `sha1_smol` — six of the seven are registered in §5 |
| `Unlicense OR MIT` / `Unlicense/MIT` | 7 | `memchr`, `aho-corasick`, `byteorder`, `byteorder-lite`, `walkdir`, `same-file`, `winapi-util` |
| `MIT OR Apache-2.0 OR Zlib` | 5 | `glow`, `cursor-icon`, `raw-window-handle`, `tinyvec_macros`, `lru-slab` |
| `Zlib OR Apache-2.0 OR MIT` | 3 | `bytemuck`, `bytemuck_derive`, `tinyvec` |
| `ISC` | 3 | **`rustls-webpki`**, `untrusted`, `libloading` |
| `MIT OR Zlib OR Apache-2.0` | 2 | `miniz_oxide` ×2 — the same three licences in a third word order |
| `BSD-2-Clause OR Apache-2.0 OR MIT` | 2 | `zerocopy`, `zerocopy-derive` |
| `BSD-3-Clause OR Apache-2.0` | 2 | `moxcms`, `pxfm` (colour management under `image`) |
| `BSD-3-Clause OR MIT OR Apache-2.0` | 2 | `num_enum`, `num_enum_derive` |
| `Zlib` | 2 | `foldhash` ×2 |
| `BSL-1.0` | 2 | `clipboard-win`, `error-code` |
| `BlueOak-1.0.0` | 2 | **`minicbor`**, `minicbor-derive` |
| `(MIT OR Apache-2.0) AND Apache-2.0` | 1 | `moka` — a choice **and** an obligation; the Apache-2.0 half is not optional |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 | `unicode-ident` |
| `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0` | 1 | `epaint_default_fonts` — font licences, not code |
| `CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception` | 1 | **`blake3`** |
| `CC0-1.0 OR MIT-0 OR Apache-2.0` | 1 | `constant_time_eq` |
| `Apache-2.0 OR ISC OR MIT` | 1 | **`rustls`** |
| `Apache-2.0 AND ISC` | 1 | **`ring`** — a conjunction, not a choice |
| `Apache-2.0 AND MIT` | 1 | `dpi` (winit's) — the second conjunction in the tree |
| `BSD-2-Clause` | 1 | `arrayref` (used by `blake3`) |
| `MIT OR BSD-3-Clause` | 1 | `if-addrs` |
| `0BSD OR MIT OR Apache-2.0` | 1 | `adler2` |
| `Apache-2.0 OR GPL-2.0-only` | 1 | `self_cell` — the Apache branch is taken; **the only appearance of GPL in the tree is as the unchosen half of a dual licence** |
| **`MPL-2.0`** | **1** | **`attohttpc`** |

**What the second renderer did to this table: nothing but arithmetic.** All 23 of the
`wgpu` crates are MIT-or-Apache-2.0 in one spelling or another, save
`codespan-reporting 0.13.1`, which is Apache-2.0-only and joins the eight already
there. No new licence expression, no copyleft, nothing that a policy allowing the
tree before would reject now. **(a)**

Five things a `deny.toml` has to handle, all verified:

1. **`attohttpc 0.30.1` is MPL-2.0** — the only copyleft licence in the tree, weak
   and file-scoped. It arrives via `igd-next ← libp2p-upnp ← libp2p`, so dropping
   the `upnp` feature removes it. It must be an explicit, deliberate allow.
2. **`minicbor` and `minicbor-derive` are BlueOak-1.0.0** and `rustls-webpki` is
   **ISC**; neither is on any default allow-list.
3. **Twenty crates use the deprecated SPDX slash form** `MIT/Apache-2.0`, which a
   strict licence policy may fail to parse: `ark-std`, `asn1-rs-impl`, `bs58`,
   `curve25519-dalek-derive`, `enum-as-inner`, `futures-rustls`, `futures-timer`,
   `guillotiere`, `hex_fmt`, `ipconfig`, `minimal-lexical`, `pollster`, `rustc-hash`,
   `rusticata-macros`, `siphasher`, `tagptr`, `type-map`, `winapi` — plus `flume`
   (`Apache-2.0/MIT`) and `fnv` (`Apache-2.0 / MIT`, with spaces). The last three of
   those arrived with the second renderer. `CRYPTOGRAPHY.md` §9 names only `ark-std`;
   nineteen more need the same clarification. **(a)**
4. **Three expressions are conjunctions, not choices.** `ring 0.17.14` is
   `Apache-2.0 AND ISC` and also carries BoringSSL/OpenSSL-derived files; `dpi 0.1.2`
   is `Apache-2.0 AND MIT`; `moka 0.12.16` is `(MIT OR Apache-2.0) AND Apache-2.0`,
   where the choice is real but the Apache-2.0 obligation stands whichever way it
   goes. All three need clarification entries rather than plain allows.
5. **`ziffle 0.1.0` declares `MIT OR Apache-2.0` but only the Apache-2.0 branch is
   usable, and this is the one row in the sweep where the declared expression and the
   effective licence differ.** The crate ships both texts; its `LICENSE-MIT` opens
   *"Copyright (c) 2015 The cargo-readme Developers"* — an unrelated project, and a
   year that predates the crate by a decade. A licence file names the party granting
   the rights, so an MIT grant issued in the name of someone who does not hold the
   copyright in this work grants nothing; **that is a compliance defect, not a typo**,
   and it is present upstream, inside the published tarball covered by the sha256 in
   §3.1. The Apache-2.0 branch is the stock text with no such defect, `Cargo.toml`'s
   own `MIT OR Apache-2.0` is the author's offer of either branch, so
   **`vendor/ziffle/PROVENANCE.md` §4 takes Apache-2.0 and relies on it alone.**
   Consequences: a `deny.toml` clarification pinning ziffle to `Apache-2.0` rather
   than accepting the dual expression; Apache-2.0 §4's obligation to redistribute the
   licence text (`vendor/ziffle/LICENSE-APACHE`; upstream ships no `NOTICE`); and
   **the broken file is not to be edited** — a third party's licence text stays
   byte-identical to what was published, including when it is wrong. The counts in
   the table above are of *declared* expressions and are unchanged.

**Our own licence is decided, declared, and unconditional.** The crate's
`license` field says `GPL-3.0-or-later` and `LICENSE` carries the text; the Tox
feature is in `default` because the owner has settled that a build without it is
not released, so there is no configuration in which this client is anything
else. `--no-default-features` exists for a contributor with no C toolchain and
is never a release.

**And it was decided by a transport choice, not a licensing one.** D-019 puts the
table's traffic on a Tox group, `c-toxcore` is **GPL-3.0 and not LGPL** — verified
against the repository's own `LICENSE` on 2026-08-30 — and linking it makes this
whole client GPL-3.0. MIT, Apache-2.0 and the dual form are foreclosed. Nothing in
the tree below blocks that: `rs_poker`'s Apache-2.0-only is compatible in that
direction. **The paragraph that follows was written while the choice was still
open and is kept as the record of what the tree looked like before**; what it
says about the dependency set remains true, and what it implies about the choice
being free no longer is. Nothing in the dependency set forecloses any of those five: the
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
| `cargo deny` configured | **`deny.toml` does not exist.** With no config its allow-list is empty, so `cargo deny check licenses` emits 588 `rejected` errors over the graph it walks — every crate, including MIT. **The one `unlicensed` error, which was for `p2p-poker` itself, is fixed:** the crate now declares `license = "GPL-3.0-or-later"` and the repository carries the verbatim GPLv3 as `LICENSE`. It had neither, which is a gap `cargo deny` was reporting correctly and nobody had read as a finding about *us* rather than about the tree. The rest is still not a usable gate | Phase 7 |
| `ziffle` vendored with `[patch.crates.io]` | **done 2026-08-29** — §3.1, `vendor/ziffle/PROVENANCE.md`; `cargo build` and `cargo test` clean against the vendored copy | landed |
| The vendored digest survives its own lockfile | **done, and it needed doing.** Patching ziffle to a path makes cargo drop the `source` and `checksum` lines from its `Cargo.lock` entry, so `ba79285…` is now in **no** machine-checked file. `PROVENANCE.md` §2.2's per-file sha256 table is the replacement and is what a re-audit compares against | landed; §8.2 |
| A designated fallback for ziffle, with a rev | **done** — §11, `paritytech/mental-poker` @ `e05744b4…` | landed |
| Register generated from `cargo metadata` and checked in CI | **not done**; this document is hand-written from measured output | Phase 7 |
| Fuzzing of the deserialisers (`SPEC_CS.md` §27) | **not done** — OQ-5 | Phase 6 |

**This register is hand-written, and a hand-written register is wrong within a
month.** That is not a hypothetical: §9 lists six drift findings this document found
in other files on its first pass, plus two it found in **itself** on the second. The
generated register is the fix; §8 is what holds the line until it exists.

The measured tables held up better than the prose around them. On 2026-08-28 every
one of the 122 rows still matched the lockfile and the registry checkouts (§5), while
both self-corrections were in sentences nothing mechanical checks — an authority list
and a security conclusion. **The parts of this document a script can verify are the
parts that stayed right**, which is the argument for the generated register stated as
evidence rather than as a preference.

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
3. **A pinned crate moves.** `ziffle`, `rs_poker` and `libp2p-stream` are the three
   whose pins carry reasoning; a new release of any of them, or of `libp2p` or
   `hickory-*`, is a trigger even if we do not take it. Some of §3's justifications
   are facts about the *current* upstream code (hickory's feature gating) and a
   release invalidates them. `mainline` and `lru` were on this list until 2026-08-31
   and are not in the build; see §5.8.
4. **Phase transition.** `SPEC_CS.md` §30's phase boundaries, each of which changes
   what the code does with the tree. Phase 7 (libp2p transport) and Phase 11
   (security audit) are the two that must not be crossed on a stale register.
5. **Any change that lets peer-supplied bytes reach a crate not in §5** — the §5.12
   assumption breaking. Rendering a peer avatar is the canonical example.
6. **Toolchain change.** A new stable rustc can change what resolves and what
   compiles; §3.6 is an existing example of a crate that a compiler version decides.
7. **A crate leaves the build.** The one this list did not have, added 2026-08-31
   after it fired unnoticed. `56b0b50` removed `mainline`, and with it `lru` — and
   §3.3 went on carrying `lru`'s unsoundness as **accepted with justification**,
   §3.7 went on stating a hard constraint on a deleted module, §4's table went on
   listing the advisory as compiled, and §5.8 went on registering six crates. Every
   one of those reads as a live decision. A removal fires trigger 1 as well, which
   is the point: **trigger 1 fired and nothing ran**, so this entry exists to name
   the case where the checklist is cheapest and the stale text is most misleading.
   The mechanical half is now `tests/corpus_dependencies.rs`.
8. **Twelve months since the last full pass**, whichever comes first.

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
   from the registry checkout, never from memory. Run §10's mechanical row check over
   the whole register, not only the changed rows, and check its match count.
5. Update §5's rows and §5.12's count so they still sum to 425.
6. **Sweep the register against D-009 rule 3** (§0). Any cell that has become "not
   compiled, therefore safe" is rewritten as a discipline plus a separately labelled
   build fact. A feature flip is the change most likely to create one, and a feature
   flip is trigger 1.
7. **Read every document against any decision this change carries, including this
   one and `CONTRIBUTING.md`** — D-012's process rule, `CONTRIBUTING.md` §2.5. A
   document read with nothing found is reported as such; silence is
   indistinguishable from a skipped file, and both documents were skipped by every
   sweep until 2026-08-28 — and then by the two sweeps after it, for D-013 and D-014,
   which is why the record above reports a **count per decision, before and after**
   rather than an assurance that the reading happened. **Report the count even when it
   is zero and stays zero**: a zero with a reason is a discharged sweep, and a zero with
   nothing beside it is the state this file was in for three passes.

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

**Entries 7 and 8 are corrections to this document itself**, from the D-009…D-012
sweep of 2026-08-28. They are in the same list as the rest deliberately: a register
that only ever records other documents' errors is a register nobody has audited.

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
7. **This document's own `argon2` row stated a security property as an absence**,
   which D-009 rule 3 forbids: *"the `password-hash` feature is off, so
   `password-hash`, `phc`, `pkcs8`, `spki` and `der` are locked but not compiled — no
   PHC string format is parsed"*. The build fact is true and stays; the conclusion
   drawn from it was the wrong shape, because a feature flip in a transitive
   dependency restores those crates without touching our code, and the row would then
   assert a safety we no longer have. The corrected row states the discipline — we
   call Argon2 as a raw KDF and never parse or emit a PHC string — and keeps the
   feature state separately, labelled as a fact about today's tree. §0, §5.3.
8. **This document's own authority line read "D-001 … D-008"** while D-012 was in
   force. It has been replaced by a pointer rather than an updated list, per D-011
   rule 1: a copied list is what drifts, and this is the third document in which that
   exact copy went stale.

---

## 10. Reproducing every number in this document

Run from the repository root, so `.cargo/config.toml`'s `[build] jobs = 19` applies —
every number below was measured under that ceiling, and it is also the reason a
reproducer's build is slower than 24 cores would suggest rather than differently
resolved. `cargo test` ignores `jobs` and needs `--test-threads=19` passed by hand.

```bash
export PATH="$HOME/.cargo/bin:$PATH"      # cargo is not on the Bash PATH

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

**Checking §5 against the lockfile without re-reading it by eye.** Every `| \`name\` |
version |` row in this file is a claim that can be matched mechanically, and a
re-audit should match it that way — a hand-check of 122 rows is how drift survives a
pass. Parse `Cargo.lock`'s `[[package]]` entries into `name → {versions}`, parse this
document's table rows with `^\|\s*\`([\w-]+)\`\s*\|\s*([0-9][^|]*?)\s*\|`, and report
every row whose version is not among the lockfile's for that name. Then, for the
six-column rows, re-read `license` and `repository` from the registry checkout and
compare. **Print the number of rows the parser matched and check it against 122
before trusting a clean result**: a regex that silently matched nothing reports the
same "no drift" as a register that is genuinely current. §6's licence-count table
contributes 4 rows that look like crate rows and are not; 126 matches minus those 4
is the expected figure.

Licence and repository cells: read `license` and `repository` from
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/<crate>-<version>/Cargo.toml`.

**Two carve-outs a matcher must know about, or it reports drift that is not there.**
`ziffle 0.1.0` is patched to `vendor/ziffle`, so (i) its `Cargo.lock` entry has no
`source` and no `checksum` line — compare `vendor/ziffle/PROVENANCE.md` §2.2's per-file
sha256 table instead, and against `vendor/ziffle/Cargo.toml` for the licence and
repository cells rather than the registry checkout, which is a different copy of the
same bytes and may be evicted; and (ii) its §5.1 licence cell deliberately carries both
the declared expression and the narrower effective one (§6 note 5), so an exact-string
comparison against `license` fails and a containment check is what is wanted.

Vendored bytes, independently of the register:

```bash
cd vendor/ziffle && find . -type f ! -name PROVENANCE.md | sort | xargs sha256sum
cargo test --test deck_constants -- --test-threads=19   # C-7: the deck did not move
```
Advisories: `~/.cargo/advisory-db/crates/<crate>/RUSTSEC-*.md`, whose `[versions]`
block carries the authoritative `patched` and `unaffected` ranges — `cargo audit`'s
one-line "Solution" is a summary of them, not a substitute.

crates.io metadata (release count, owners, newest version) needs a User-Agent:

```bash
curl -s -H "User-Agent: p2p-poker-dependency-register (<your-email>)" \
     https://crates.io/api/v1/crates/<crate>
```

---

## 11. The designated fallback for `ziffle`

`ZIFFLE_VERDICT.md` condition **C-14**. This section registers a dependency the
project **does not compile** and is not proposing to adopt. It is here because §3.1's
crate has one author and one release, and "what do we do if that stops being viable"
is a question that is cheap to answer now and expensive to answer in the week it is
asked. Nothing below is a recommendation to swap — `ZIFFLE_ALTERNATIVES.md` §8 says
plainly **no**, because swapping trades 1 779 unreviewed lines for 7 764 unreviewed
lines. It is a recommendation to have the answer written down.

### 11.1 The entry

| | |
|---|---|
| Project | `paritytech/mental-poker` |
| Repository | `https://github.com/paritytech/mental-poker` |
| **Rev** | **`e05744b4cc431088ec2fda769a73b067b4664893`** |
| Crates we would take | `proofs`, `protocol`, `deck`, `deck-secp256k1` — ~8 530 lines, ~280 KB (`ez` is 31 lines and needed by none of them; `play` is optional) |
| Distribution | **not on crates.io**, no tags, no releases. Consumption is by git `rev` or by vendoring — and if adopted it is **vendored**, for the reasons in §3.1 and in `ZIFFLE_ALTERNATIVES.md` §7.4 |
| Declared licence | `MIT OR Apache-2.0` in every crate — **with two open questions, §11.3** |
| Lineage | forked from `geometryxyz/mental-poker` (Kobi Gurkan, Nicolas Mohnblatt); maintained by Jeff Burdges with `coax1d` as second committer |
| Status | **unaudited.** Nothing in this space is audited. Adopting it does not retire `MENTAL_POKER.md` §9 risk 1; it moves it onto a surface **4.4×** larger |

**The rev is the entry.** A short rev is not enough — `ZIFFLE_ALTERNATIVES.md` §4.2
measured cargo rejecting `rev = "e05744b4"` with *"revision e05744b4 not found"*; the
full 40-hex sha is required. And the rev is not a version number: the project has no
tags, so `e05744b4…` is the **only** durable name for the reviewed state. It was
measured to build clean on `rustc 1.95.0` in 21.4 s with 30 tests passing, and to
resolve and build as a pinned git dependency from a cold cache over plain HTTPS. **(a)**

Cost of adoption, so the entry is not read as free: **+6 lockfile packages**
(`ark-poly`, `ark-transcript`, `sha3`, `keccak`, `generic-array`, `rand 0.8.8`), with
**no arkworks version split** — the tree stays on a single `ark-* 0.5.0` reused from
ziffle. Roughly 300–500 lines of adapter, 2–4 days, most of it two specific things:
`AggregatedPublicKeys<'p, C>` borrows its parameters and so cannot sit behind a plain
associated type, and parity has **no `Verified<T>` typestate** — which is ziffle's best
API idea and the one place a swap makes a safety property weaker unless the typestate
is rebuilt in our own layer. `ZIFFLE_ALTERNATIVES.md` §4.5 owns those numbers.

### 11.2 A migration would be differential, not blind — and that is measured

The two libraries were **built into one binary and run together**: ziffle,
`cards-protocol`, `deck-secp256k1`, `ed25519-dalek 3`, `curve25519-dalek 5`,
`sha2 0.11`, `blake3`, `minicbor` and `getrandom 0.4`, all linked, all executing.
**(a)** `ZIFFLE_ALTERNATIVES.md` §4.5.

This is the property that makes the fallback worth registering rather than merely
naming. A swap does not have to be a cut-over: both implementations can run side by
side over the same decks, the same contexts and the same adversarial inputs, and
disagree loudly, before either becomes the one the client ships. Given that a false
*reject* now ejects a player (§3.1, D-014), being able to compare two verifiers on the
same input is not a convenience — it is the only cheap way to tell a bug in the new
one from a bug in the old.

It also has an expiry date. The co-linking was measured at ziffle 0.1.0 against
`e05744b4…` on `ark-* 0.5.0`. If either side moves major arkworks versions the split
returns and this paragraph stops being true; that is a §8.1 re-audit trigger for this
section specifically.

### 11.3 Two licence questions to ask upstream **now**

`ZIFFLE_ALTERNATIVES.md` §4.6 found both. Neither is a reason not to adopt, and both
are the kind of thing that takes one GitHub issue and a fortnight's patience when
nothing is on fire, and is a blocker when something is. **They are recorded here as
work to do now, not as findings to file.** One issue, two questions:

1. **Where is the grant for the four crates that ship no licence text?** Every crate
   declares `license = "MIT OR Apache-2.0"` in its own `Cargo.toml`, and `proofs/` and
   `protocol/` ship both licence texts — but `deck/`, `deck-secp256k1/`, `ez/` and
   `play/` ship **none**, and there is **no LICENSE file at the repository root**. Two
   of the four unlicensed crates, `deck/` and `deck-secp256k1/`, are crates we would
   actually take. A manifest field is a declaration; the licence file is the grant. Ask
   for the texts to be added at the root or per crate.
2. **What does "Through 2025" mean in the README?** The README reads *"Through 2025,
   this crate is licensed under either of…"*. That is almost certainly a copyright-year
   statement and not a term limit on the grant — but "almost certainly" is not a
   licence, and the sentence can be read as a permission that expired. Ask for it to be
   rephrased.

**Why now and not at adoption.** The project has already moved organisation twice:
`[workspace.package] homepage` still says `github.com/w3f/mental-poker` (301-redirects)
and in-source comments in `protocol/src/shuffle.rs` cite a `peer3to/mental-poker` URL
that **404s**. A maintainer who answers a licence question today may be unreachable on
the day this fallback is needed, and the answer is worth nothing if it arrives after
the decision. Two committers is better than one, but "in Parity's GitHub organisation"
is not "Parity supports this": zero stars, zero forks, no CI badge, no release process.

Record the answers **in this section** when they arrive, with the issue URL and the
date, so the next reader sees the resolution and not the question.

---

**Sweep record, 2026-08-28 — D-009 to D-012 against this document.** Reported in
full, including the decisions that changed nothing, because D-012's process rule
counts silence as a skipped file.

| Decision | Found here | Action |
|---|---|---|
| **D-009 rule 1** (honest behaviour must not satisfy the equivocation predicate) | nothing — this document defines no message and no slot key | none |
| **D-009 rule 2** (a below-floor certificate is inert) | nothing — no certificate logic here | none |
| **D-009 rule 3** (never state a security property as an absence) | **one row**: `argon2` in §5.3 | rewritten; §0 added; §9.7 |
| **D-010** (neutral abort, no automated forfeiture or eviction) | `libp2p-allow-block-list 0.6.0` is registered in §5.5 with no note that it may not be driven from a proof | note added to the row |
| **D-011 rule 1** (one normative owner) | the authority line restated `DECISIONS.md`'s contents, and had gone stale at "D-001 … D-008" | replaced with a pointer; §9.8 |
| **D-011 rule 2** (the slot key written once) | nothing — the key is not reproduced here | none |
| **D-011 rule 3** (no eviction at any layer) | same finding as D-010 above; `NETWORK_STACK.md` owns the rule, this register points at it | pointer, not a restatement |
| **D-012** (no canonical state from a per-receiver quantity) | **nothing.** Nothing in this document enters a state hash, a roster hash, a chained event body or a genesis; crate versions, licences and advisory IDs are the same for every receiver by construction | none |

Before this pass the document contained zero occurrences of D-009, D-010, D-011 and
D-012. That is the condition D-012's process rule exists to prevent, and it was the
third instance.

**Sweep record, 2026-08-28 — D-013 and D-014 against this document.** Same form, same
reason, and the count first: before this sweep the file contained **zero** occurrences of
`D-013` and **zero** of `D-014`, while the sweep record above cited the rule that every
decision is read against every document. That is the **fourth and fifth** instance of the
same omission, in the same file, and it was recorded as `L8` in `DECISIONS.md`'s open list
in three consecutive passes without being acted on. `CONTRIBUTING.md`'s header says what
that pattern costs and §2.5 item 4 there now carries it as a numbered failure of the rule.

| Decision | Found here | Action |
|---|---|---|
| **D-013** (liveness is inherited from the chain, not from a seat's status) | **nothing on the wire or in the register.** This document names no seat, no status, no required emitter set and no hand; a crate's presence in the tree is not a per-hand quantity. The one place it could have reached — `lru 0.16.4` in §3.3 — is registered for the receiver-side caches `PROTOCOL.md` §5.3 bounds, and D-013 changed which caches exist without changing the crate's status | none; recorded so silence is not read as a skip |
| **D-014** (removal on self-authenticating evidence) | **one row, and it is a narrowing rather than a finding.** `libp2p-allow-block-list 0.6.0` in §5.5 carries a D-010 note that it *"may not be driven from a proof"*. D-014 does not change that note: a removal under D-014 is a **table-level** disposition carried by the poker layer — the seat is dead and blinded off (`STATE_MACHINE.md` T64, T65, I34) — and it is **not** a transport-layer block. D-011 rule 3 stands unamended: no `block_peer`, no allow/block list driven by a protocol proof, at any layer, including for tier-1 evidence | note extended to say D-014 does not reopen it |
| **D-014, second half** (the verifier is now load-bearing against a person) | **the whole register, read a new way, and this is the item worth the sweep.** D-014 makes a removal rest on a proof verifier's verdict, so **a dependency that verifies a proof is now a dependency that can eject a player**: `ziffle 0.1.0` (§3.1) — unaudited, one author, one release, semver-unstable — is the code behind all three of the tier-1 clauses `CRYPTOGRAPHY.md` §8.1 owns. A false *reject* in it was previously a rejected message and a stalled hand; it is now an ejected honest player. Nothing in the register changes, and the standing item that already covers it is **OQ-1**, the blocking in-house review of `MultiExpArg` and `SingleValueProductArg` — which §3.1 grades a prerequisite and which D-014 promotes from *deck integrity* to *nobody is ejected wrongly* | §3.1 pointer added; OQ-1's justification restated, its status unchanged |

---

*This register is complete for the 122 security-critical dependencies it names and
for the licence status of all 425 compiled crates. §11 registers one project that is
deliberately **not** compiled and adds no row to §5. It is **not** an audit: of the
crates it names, exactly one has been read line by line by us — ziffle, whose
`MultiExpArg` and `SingleValueProductArg` were reviewed against Bayer–Groth 2012 in
`docs/research/ZIFFLE_VERDICT.md`, and whose reviewed bytes are now in the tree at
`vendor/ziffle/`. That review is **not** a clean bill: it found fourteen defects
around the argument, sets fifteen conditions, and says the crate is fit for the
play-money MVP only and **not fit for real money**. Whether it fully discharges
`SPEC_CS.md` §36's and `MENTAL_POKER.md` §9's prerequisite is `DECISIONS.md`'s open
item `ZR-1` and is not settled here — witness-extended emulation and the soundness
bound at `m = 1`, which `CRYPTOGRAPHY.md` OQ-1 asked for by name, were not proved.
Every other crate in this register is unreviewed.*
