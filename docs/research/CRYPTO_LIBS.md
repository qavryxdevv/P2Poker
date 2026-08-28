# CRYPTO_LIBS.md — general cryptographic and serialisation dependency set

> ## Read this first: the dependency block below was verified in ISOLATION
>
> Every result in this document was produced in a standalone probe crate
> (`probe-crypto-final`) containing **only** the crates recommended here. It
> was never compiled alongside `libp2p`, `mainline`, `ziffle` or `eframe`.
>
> **`docs/research/INTEGRATION.md` holds the combined result and outranks
> this document wherever the two disagree.** The isolated results are kept,
> because they were true of the probe; each one the integrated tree changes
> is now labelled *Probe* and paired with the *Integrated* reality beside it.
>
> The largest correction, because it was stated as an absence and absences
> get copied into manifests: this document claimed `rand` had been **dropped
> entirely** and was absent "at any depth". In the integrated tree **`rand`
> is present at three majors — 0.8.8, 0.9.5 and 0.10.2 — and `rand = "0.8"`
> is a direct dependency of `p2p-poker`.** See §1.2, §7.3 and §10. The
> discipline behind the claim survives. The absence does not, and must not
> be restated anywhere.

Phase 0 research. Binding spec sections: 6, 7, 12, 21, 28.

Scope: everything **except** the mental-poker / verifiable-shuffle protocol itself,
which is pinned in a separate document.

**Toolchain used for all verification**

```
rustc 1.95.0 (59807616e 2026-04-14)
cargo 1.95.0 (f2d3ce0bd 2026-03-21)
host: x86_64-pc-windows-msvc, Windows 10, 24 cores
```

**Probe crates** (built outside the repo):

| Probe | Path | What it proves |
|---|---|---|
| `probe-crypto` | `…/scratchpad/probe-crypto/src/main.rs` | Wide survey: every candidate crate incl. ciborium, cbor4ii, dcbor, k256, keyring |
| `probe-crypto` (bin `cbordet`) | `…/scratchpad/probe-crypto/src/bin/cbordet.rs` | CBOR determinism evidence |
| `probe-crypto` (bin `canoncheck`) | `…/scratchpad/probe-crypto/src/bin/canoncheck.rs` | The re-encode canonicality gate |
| `probe-crypto-final` | `…/scratchpad/probe-crypto-final/src/main.rs` | **The recommended set, compiled and run together** |
| `probe-crypto-final` (bin `allocbound`) | `…/scratchpad/probe-crypto-final/src/bin/allocbound.rs` | Spec §27 allocation bounding |

Scratchpad root:
`~/AppData/Local/Temp/claude/<session>/<session-id>/scratchpad/`

Throughout, **Verification** is one of:

- **(a) compiled** — code using the API compiled and, where stated, ran;
- **(b) source** — read in `~/.cargo/registry/src/index.crates.io-…/<crate>-<version>/`;
- **(c) registry** — crates.io API metadata (versions, licence, dates) or the RustSec advisory-db.

docs.rs and recollection were used only as leads, never as evidence.

---

## 0. Executive summary

| Decision | Outcome |
|---|---|
| OS CSPRNG | `getrandom::SysRng` — **`OsRng` no longer exists** in rand 0.10 / rand_core 0.10 |
| `rand` crate | *Probe:* dropped entirely. *Integrated:* **present at 0.8.8, 0.9.5 and 0.10.2**, plus `rand = "0.8"` as a direct dependency. RUSTSEC-2026-0097 does not bite — but because all three are patched versions, not because the crate is gone (§1.2, §7.3) |
| Protocol hash | **BLAKE3** for our own hashes; `sha2` stays because Ed25519 mandates SHA-512 |
| Signatures | `ed25519-dalek` 3.0.0, **`verify_strict` only** |
| Canonical bytes | **`minicbor` 2.3.0, `#[cbor(array)]` only, no maps, no floats**, plus a re-encode gate |
| Deterministic CBOR out of the box | **Only `dcbor` fully implements RFC 8949 §4.2. `ciborium`, `cbor4ii` and `serde_cbor` do not.** |
| Secret storage | One file format, two key slots: Argon2id passphrase (portable) + DPAPI (Windows convenience) |
| `cargo-deny` | 0.20.2. Clean on the runtime set; the optional `dcbor` dev-dep needs one scoped ignore |
| `cargo-audit` | Installs **only with `--locked`** — 0.22.2. *Probe:* clean. *Integrated:* **2 vulnerabilities, 2 warnings**, all of them in `hickory-proto`, `lru` and `paste` — none in this document's set (§7.3.2) |
| Could not build | Nothing in the recommended set. Two candidates were rejected on evidence (below) |

---

## 1. The single most important finding: `OsRng` is gone

Spec §7 says: *"Použij OS CSPRNG. … používej OsRng, ne SmallRng ani generátor se vlastním
seedem."* That instruction names an API that **no longer exists** in the current release line.

- `rand` 0.10.2 has no `src/rngs/os.rs` and no `OsRng` symbol anywhere in `src/`.
- `rand_core` 0.10.1 has no `OsRng` either, and has **no `os_rng` feature** — asking for it
  is a hard resolver error:

```
package `probe-crypto` depends on `rand_core` with feature `os_rng`
but `rand_core` does not have that feature.
```

- The OS interface now lives in `getrandom` as the zero-sized struct **`getrandom::SysRng`**,
  re-exported as `rand::rngs::SysRng`.

**Verification:** (b) source — `rand-0.10.2/src/rngs/mod.rs:119` reads
`pub use getrandom::{Error as SysError, SysRng};`; `getrandom-0.4.3/src/sys_rng.rs` defines
`pub struct SysRng` with `impl TryRng for SysRng` and `impl TryCryptoRng for SysRng`.
Also (a) compiled — the `os_rng` feature error above is real resolver output.

The trait names changed too. `rand_core` 0.10.1:

| Old | New | Status |
|---|---|---|
| `RngCore` | `Rng` | `RngCore` still exists as a `#[deprecated(since = "0.10.0")]` stub |
| `TryRngCore` | `TryRng` | likewise deprecated |
| `CryptoRng` | `CryptoRng` | now `Rng + TryCryptoRng<Error = Infallible>` |

**Verification:** (b) source — `rand_core-0.10.1/src/lib.rs` lines 257–276 carry the
literal `#[deprecated(since = "0.10.0", note = "use \`Rng\` instead")]` attributes.

### 1.1 The exact compiling incantation

`SysRng` is **fallible** (`TryRng`), which is correct: an OS entropy failure must be an
error, never a silently weaker key. Most crypto APIs want an infallible `CryptoRng`, so
wrap it in `rand_core::UnwrapErr`.

```rust
use getrandom::SysRng;
use rand_core::{Rng, TryRng, UnwrapErr};

/// The one and only entropy source in the whole client.
fn os_random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    SysRng.try_fill_bytes(&mut b).expect("OS CSPRNG unavailable");
    b
}

// Where an API demands `rand_core::CryptoRng` (e.g. SigningKey::generate):
let mut csprng = UnwrapErr(SysRng);
let signing = ed25519_dalek::SigningKey::generate(&mut csprng);
```

**Verification:** (a) compiled and ran, `probe-crypto-final`, step `[1]` and `[2]`.

Cargo entries — note `sys_rng` is the feature that turns `SysRng` on:

```toml
rand_core = { version = "0.10.1", default-features = false }
getrandom = { version = "0.4.3", default-features = false, features = ["std", "sys_rng"] }
```

**Verification:** (b) source — `getrandom-0.4.3/Cargo.toml` `[features]` lists
`sys_rng = ["dep:rand_core"]`. (a) compiled.

### 1.2 Recommendation: draw randomness only from `getrandom::SysRng`

> **This section was rewritten.** Its earlier title was "do not depend on `rand`
> at all", and that recommendation **cannot be met in the integrated tree**. See
> `INTEGRATION.md` §2, which is the authority. What survives is the discipline,
> which is what spec §7 actually asks for.

The `rand` crate buys us `RngExt` (`random_range`, shuffling, distributions). We must not
use any of it: a poker shuffle is produced by the mental-poker protocol, not by a local
PRNG. That much is unchanged. What changed is how the rule is enforced.

**Probe result — true of `probe-crypto-final`, and only of it.** Dropping `rand`:

- removes `ThreadRng`, `StdRng` and `SmallRng` from the tree, so spec §7's prohibition is
  enforced by the dependency graph rather than by reviewer discipline;
- removes **RUSTSEC-2026-0097** (unsoundness in `rand::rng()` with a custom `log` logger,
  patched in ≥ 0.10.1, but simply absent if the crate is absent);
- drops `chacha20` and friends from the non-AEAD path.

**Verification:** (a) compiled — `probe-crypto-final` builds and runs with no `rand`;
`cargo tree -i rand` returns
`error: package ID specification 'rand' did not match any packages`.

**Integrated result — true of the repository root, and what binds.** `rand` is in
the tree at three majors, all of them compiled, and none of them removable:

| Version | Reached through | Features actually enabled |
|---|---|---|
| 0.8.8 | **direct dependency of `p2p-poker`**; also `ark-std` 0.5.0 (via `ziffle`), `libp2p-autonat` 0.15.0, `libp2p-core` 0.43.2 | `alloc`, `getrandom`, `libc`, `rand_chacha`, `std`, `std_rng` |
| 0.9.5 | `hickory-proto` / `hickory-resolver` (libp2p `dns`), `igd-next` (libp2p `upnp`), `yamux` | — |
| 0.10.2 | `quinn-proto` (libp2p `quic`), `rs_poker` 5.0.0 | `alloc`, `getrandom`, `std`, `std_rng`, `sys_rng`, `thread_rng` |

`rand_chacha` follows at 0.3.1 (under `rand` 0.8.8) and 0.9.0 (under `rand` 0.9.5);
`rand_core` at 0.6.4, 0.9.5 and 0.10.1.

**Verification:** (a) executed at the repository root —
`cargo tree --edges normal -i rand@0.8.8` (and `@0.9.5`, `@0.10.2`) for provenance,
`cargo tree --edges features -i rand@<version>` for the feature columns.

The direct `rand = "0.8"` is not an accident and not ours to delete: `libp2p-autonat`'s
v2 behaviours are generic over an RNG defaulting to `rand_core` 0.6's `OsRng` —
`libp2p-autonat-0.15.0/src/v2/client/behaviour.rs:60` declares
`pub struct Behaviour<R = OsRng>` — and `rand` 0.8 is where we get that type from.
**Verification:** (b) source.

Two things about what is *compiled*, which the crate list alone does not tell you:

- `small_rng` is **not** enabled on `rand` 0.8.8, so `SmallRng` is not in the build at
  that major. `std_rng` **is** enabled — it is a default feature of `rand` 0.8 — so
  `StdRng` and `rand_chacha` 0.3.1 are. **Verification:** (b) source,
  `rand-0.8.8/Cargo.toml` `[features]`: `default = ["std", "std_rng"]`,
  `std_rng = ["rand_chacha"]`, and `small_rng = []` is not named by any of them.
- `thread_rng` **is** enabled on `rand` 0.10.2, by `quinn-proto`. `rand`'s own `log`
  feature is not, and 0.10.2 is past the RUSTSEC-2026-0097 patch line anyway (§7.3.1).

> **Spec deviation to record.** Spec §7 names `OsRng`. We implement the requirement
> (OS CSPRNG, never a self-seeded or non-cryptographic generator) under its current
> name `getrandom::SysRng`.
>
> **The prohibition on `SmallRng` and `StdRng` is no longer satisfied structurally.**
> `StdRng` is compiled into the binary through `rand` 0.8's default features. The rule
> is now a reviewable boundary rather than an impossibility: all of our own randomness
> is routed through one module, `src/security/rng.rs`, which exists precisely to make
> that boundary explicit. Do not write "enforced by the dependency graph" anywhere
> again — it is not true, and it is the kind of sentence that stops people looking.

---

## 2. Signatures and group operations

### 2.1 `ed25519-dalek` 3.0.0

| | |
|---|---|
| Version | 3.0.0, published 2026-07-06 |
| Licence | BSD-3-Clause |
| Repository | https://github.com/dalek-cryptography/curve25519-dalek/tree/main/ed25519-dalek |
| Maintainer | dalek-cryptography (isislovecruft, Tony Arcieri, Michael Rosenberg) |
| MSRV | 1.85 |
| Downloads | ~197 M |

**Verification:** (c) registry.

Advisory history: **RUSTSEC-2022-0093** (CVE-2022-50237) — "Double Public Key Signing
Function Oracle Attack". Pre-2.0 APIs allowed a decoupled private/public keypair as
signing input, from which two signatures sharing `R` leak the private key.
`patched = [">= 2"]`. We are on 3.0.0. **Verification:** (c) RustSec advisory-db,
`crates/ed25519-dalek/RUSTSEC-2022-0093.md`.

The lesson survives the fix: **never** touch the `hazmat` feature, and never reconstruct a
key from separately-sourced secret and public halves.

API actually present (all **(b) source**, `signing.rs` / `verifying.rs`, and **(a) compiled**):

```rust
pub type SecretKey = [u8; SECRET_KEY_LENGTH];          // 32-byte seed
impl SigningKey {
    pub fn generate<R: CryptoRng + ?Sized>(csprng: &mut R) -> SigningKey;
    pub fn from_bytes(secret_key: &SecretKey) -> Self;
    pub fn to_bytes(&self) -> SecretKey;
    pub fn verifying_key(&self) -> VerifyingKey;
}
impl VerifyingKey {
    pub fn to_bytes(&self) -> [u8; PUBLIC_KEY_LENGTH];
    pub fn from_bytes(bytes: &[u8; PUBLIC_KEY_LENGTH]) -> Result<VerifyingKey, SignatureError>;
    pub fn verify_strict(&self, message: &[u8], signature: &Signature) -> Result<(), SignatureError>;
}
```

**Rule for this project: always `verify_strict`, never `verify`.** Plain `verify` accepts
signatures under small-order / non-canonical public keys, which permits signature
malleability. A malleable signature is an equivocation hole under spec §14: two distinct
byte strings validating for one logical event.

Persistence: store **only the 32-byte seed**. `SigningKey::from_bytes(&seed)` reproduces
the verifying key exactly. **Verification:** (a) compiled and ran, `probe-crypto-final`,
the `assert_eq!` on `SigningKey::from_bytes(&seed).verifying_key()`.

Features we enable: `default-features = false, features = ["fast", "zeroize", "rand_core"]`.
`fast` pulls `curve25519-dalek/precomputed-tables`; `zeroize` wipes key material on drop;
`rand_core` provides the `generate` constructor. We deliberately do **not** enable
`serde`, `pkcs8`, `pem`, `batch`, `legacy_compatibility` or `hazmat`.
**Verification:** (b) source — `ed25519-dalek-3.0.0/Cargo.toml` `[features]`; (a) compiled.

### 2.2 `curve25519-dalek` 5.0.0

| | |
|---|---|
| Version | 5.0.0, published 2026-07-06 |
| Licence | BSD-3-Clause |
| Repository | https://github.com/dalek-cryptography/curve25519-dalek |
| MSRV | 1.85.0 |
| Downloads | ~240 M |

Advisory history: **RUSTSEC-2024-0344** (CVE-2024-58262) — timing variability in
`Scalar29::sub` / `Scalar52::sub`, where LLVM inserted a `jns` branch that bypassed a
masked code section. `patched = [">= 4.1.3"]`. We are on 5.0.0.
**Verification:** (c) RustSec advisory-db.

**Assurance evidence** (this is unusually strong, and worth quoting precisely): the
repository README states that a large chunk of the crate has been formally verified with
Verus — specifically the 280 Rust functions in version 4.1.3 reachable from the Signal
Messenger app — with the certificate at `verilib.org/cert/5132`. Separately, the optional
`fiat` backend integrates formally verified field arithmetic generated by the MIT
Fiat-Crypto project. **Verification:** (c) fetched
`raw.githubusercontent.com/dalek-cryptography/curve25519-dalek/main/curve25519-dalek/README.md`,
lines 86 / 243 / 306.

Verified group operations (needed by the mental-poker layer for ElGamal-style
re-encryption):

```rust
let g = RISTRETTO_BASEPOINT_POINT;
assert_eq!(g * (x + y), g * x + g * y);   // homomorphism
assert_eq!((g * x) * y, (g * y) * x);     // commutative re-encryption
```

**Verification:** (a) compiled and ran, `probe-crypto-final` step `[4]`, with `x`, `y`
drawn from `Scalar::from_bytes_mod_order_wide(&os_random::<64>())`.

Use **ristretto255**, not raw Edwards points. Ristretto gives a prime-order group, which
removes the cofactor-8 small-subgroup pitfalls that would otherwise be an attack surface
in a shuffle protocol.

### 2.3 `k256` 0.14.0 — available, not adopted

| | |
|---|---|
| Version | 0.14.0, published 2026-07-08 |
| Licence | Apache-2.0 OR MIT |
| Repository | https://github.com/RustCrypto/elliptic-curves |
| RustSec advisories | none |

secp256k1 group arithmetic compiles and is correct
(`ProjectivePoint::GENERATOR`, `k256::Scalar`, `GroupEncoding::to_bytes`).
**Verification:** (a) compiled and ran, `probe-crypto` `[k256]` line.

**Not adopted for the general layer.** ristretto255 is the better default: prime order by
construction, faster, and formally verified field arithmetic. Note the cross-layer
constraint though — `k256` may become mandatory if the mental-poker layer settles on a
secp256k1-based proof system, and today's leading candidate `ziffle` 0.1.0 depends on
`ark-secp256k1 ^0.5.0`, not on dalek at all. **Verification:** (c) crates.io dependencies
endpoint for `ziffle/0.1.0`. That is the mental-poker document's call, not this one's;
this document only records that both curves build here.

---

## 3. Hashing — recommend BLAKE3, keep sha2

### 3.1 The two candidates

| | `sha2` 0.11.0 | `blake3` 1.8.7 |
|---|---|---|
| Licence | MIT OR Apache-2.0 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception |
| Repository | https://github.com/RustCrypto/hashes | https://github.com/BLAKE3-team/BLAKE3 |
| Published | 2026-03-25 | 2026-08-20 |
| MSRV | 1.85 | not declared |
| RustSec | RUSTSEC-2021-0100 (AVX2 miscomputation, `patched = [">= 0.9.8"]`) | none |

**Verification:** (c) registry + advisory-db. SHA-256 and SHA-512 known-answer tests for
`"abc"` pass (`ba7816bf…`, `ddaf35a1…`) — (a) compiled and ran.

### 3.2 Recommendation

**Use BLAKE3 for every hash the protocol itself defines** — the hash-chain transcript
(§13), `state_hash` (§15), RNG commitments (§7), and deck commitments. Reasons:

1. **Domain separation is a first-class library feature, not a convention we invent.**
   `blake3::derive_key(context: &'static str, key_material: &[u8]) -> [u8; 32]` and
   `Hasher::new_keyed(&[u8; 32])` give keyed and KDF modes natively. With SHA-2 we would
   have to hand-roll HMAC or prefix conventions — exactly the kind of own construction
   spec §6 and §36 tell us to avoid.
2. **No length-extension.** SHA-256 and SHA-512 are Merkle–Damgård and length-extendable;
   any bare `SHA256(secret ‖ msg)` construction is unsafe. BLAKE3 finalises with domain
   flags, so the naive construction is not a trap.
3. **XOF.** `finalize_xof().fill(&mut buf)` gives arbitrary-length deterministic expansion
   for deck-sized material. Verified that the first 32 bytes of the XOF equal `blake3::hash`.
4. **Speed**, which spec §33 makes a requirement: crypto must not block the GUI loop, and
   transcript verification is hash-bound.

**Verification of all four:** (a) compiled and ran — `probe-crypto` `[hash]` line exercises
`hash`, `keyed_hash`, `derive_key` and `finalize_xof`, and asserts the XOF prefix equality.

**Keep `sha2` anyway — it costs nothing.** Ed25519 *is* defined over SHA-512, so
`ed25519-dalek` pulls `sha2` unconditionally (`ed25519-dalek-3.0.0/src/lib.rs:279` reads
`pub use sha2::Sha512;` — **(b) source**). libp2p additionally needs SHA-256 for
multihash/PeerId. So `sha2` is in the tree whether we list it or not; listing it explicitly
just pins it.

**Honest limitation.** I found **no public third-party security audit of BLAKE3** — the
upstream README contains no "audit" or "security" section. **Verification:** (c) fetched
`raw.githubusercontent.com/BLAKE3-team/BLAKE3/master/README.md` and grepped; zero hits.
Its design assurance rests on the BLAKE3 specification and on its BLAKE2/ChaCha lineage,
not on a published audit report. Record this in `docs/CRYPTOGRAPHY.md`. If the
mental-poker proof system mandates a specific hash for Fiat–Shamir, that mandate wins for
those bytes; BLAKE3 governs only our own hashes.

### 3.3 Discipline: domain separation and length prefixing

Both are mandatory, and both are cheap. Concatenating variable-length fields without
length prefixes is ambiguous: `"AB" ‖ "C"` and `"A" ‖ "BC"` are the same byte string, so
two different logical events would collide to the same transcript hash — a direct §13/§14
break.

```rust
/// Every hash in the protocol is domain-separated and length-prefixed.
fn transcript_hash(domain: &'static str, parts: &[&[u8]]) -> [u8; 32] {
    let key = blake3::derive_key(domain, b"p2p-poker/v1");
    let mut h = blake3::Hasher::new_keyed(&key);
    for p in parts {
        h.update(&(p.len() as u64).to_be_bytes());   // 8-byte big-endian length prefix
        h.update(p);
    }
    *h.finalize().as_bytes()
}
```

Asserted in the probe:

- `transcript_hash(D, &["AB", "C"]) != transcript_hash(D, &["A", "BC"])` — length prefixing works;
- `transcript_hash("…transcript", …) != transcript_hash("…state", …)` — domains separate.

**Verification:** (a) compiled and ran, `probe-crypto-final` step `[5]`.

Use one distinct `domain` constant per hash purpose, e.g.
`"p2p-poker v1 transcript"`, `"p2p-poker v1 state"`, `"p2p-poker v1 rng-commit"`,
`"p2p-poker v1 deck-commit"`.

---

## 4. Deterministic CBOR — the critical evaluation

Spec §12 demands canonical serialisation and forbids signing non-canonical JSON.
RFC 8949 §4.2.1 "Core Deterministic Encoding Requirements" says:

1. preferred (shortest) integer encoding;
2. definite-length arrays, maps and strings only;
3. **map keys sorted in the bytewise lexicographic order of their deterministic encodings.**

I tested all four candidates against all three. **Every claim below was executed.**

### 4.1 Results

| Crate | Version | Shortest ints | Definite lengths | Sorted map keys | Verdict |
|---|---|---|---|---|---|
| `serde_cbor` | 0.11.2 | — | — | — | **Rejected**, see 4.5 |
| `ciborium` | 0.2.2 | yes | **no** | **no** | Not deterministic |
| `cbor4ii` | 1.2.2 | yes | **no** | **no** | Not deterministic |
| `minicbor` | 2.3.0 | yes | yes | n/a (no maps used) | **Deterministic by construction — chosen** |
| `dcbor` | 0.25.2 | yes | yes | **yes** | Fully RFC 8949 §4.2, both directions |

### 4.2 `ciborium` and `cbor4ii` fail on map key ordering

Two `HashMap`s holding identical logical content, populated in different orders:

```
ciborium HashMap  order-stable? false
  h1 = a3647a756c750165616c70686102646d696b6503     ("zulu","alpha","mike")
  h2 = a3647a756c7501646d696b650365616c70686102     ("zulu","mike","alpha")
```

Worse, the ordering is not even stable **across process runs**, because `HashMap`'s
`RandomState` is seeded per process. Running the same binary twice:

```
run 1: ciborium HashMap = a567636861726c6965026564656c74610365616c7068610065627261766f01646563686f04
run 2: ciborium HashMap = a567636861726c69650265627261766f016564656c74610365616c70686100646563686f04
```

`cbor4ii` produced byte-identical output to `ciborium` in every case, including this one —
it inherits the same defect.

**A `HashMap` reaching a signing or hashing path is therefore a live protocol bug**, not a
theoretical one. Two honest clients would compute different `state_hash` values for
identical state and trigger the §15 dispute mechanism against each other.

`BTreeMap` is stable within and across runs, but its order is **still not RFC 8949 §4.2.1**.
Rust sorts `String` keys by decoded value; CBOR requires sorting by *encoded* bytes:

```
ciborium BTreeMap {"aaa":1,"b":2,"cc":3} = a3 63616161 01 6162 02 626363 03   -> aaa, b, cc
RFC 8949 §4.2.1 requires                   a3 6162 02 626363 03 63616161 01   -> b, cc, aaa
```

(Encoded keys are `63616161`, `6162`, `626363`; bytewise, `0x61 62` < `0x62 63 63` < `0x63 61 61 61`.)

**Verification:** (a) compiled and ran, `probe-crypto` `[cbor]` block and `cbordet` bin,
executed twice.

### 4.3 `ciborium` and `cbor4ii` also emit indefinite lengths

`ciborium`'s serializer passes serde's length hint straight through:

```rust
fn serialize_seq(self, length: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
    self.0.push(Header::Array(length))?;   // None => indefinite
    ...
}
```

**Verification:** (b) source — `ciborium-0.2.2/src/ser/mod.rs`. Confirmed **(a) compiled and
ran**: the same `Vec<u32>` value serialised through `collect_seq` on an iterator with no
exact size hint yields a different encoding.

```
ciborium Vec<u32> (known len) = 83010203        definite   array(3)
ciborium collect_seq(iter)    = 9f010203ff      indefinite array + break
cbor4ii  Vec<u32>             = 83010203
cbor4ii  collect_seq(iter)    = 9f010203ff
```

RFC 8949 §4.2.1 forbids indefinite lengths outright. This is a footgun that fires whenever
someone hands an iterator to serde — nothing in the type system prevents it.

### 4.4 `dcbor` 0.25.2 — the only fully conformant option

| | |
|---|---|
| Version | 0.25.2, published 2026-03-16 |
| Licence | BSD-2-Clause-Patent |
| Repository | https://github.com/BlockchainCommons/bc-dcbor-rust |
| Downloads | ~144 k |
| RustSec advisories | none |

It sorts correctly, across key types, by encoded bytes:

```
dcbor {"delta":4,"a":1,"bb":2,1:"int key",h'010203':"bytes key"}
  = a5 01 67696e74206b6579  6161 01  626262 02  6564656c7461 04  83010203 696279746573206b6579
     ^int key first (major type 0 < 3), then "a" < "bb" < "delta", then the array key
dcbor map order-stable across insertion orders? true
```

And — the property no other candidate has — **it rejects non-deterministic input on decode**:

```
dcbor REJECTED indefinite array on decode: unsupported value in CBOR header
dcbor REJECTED non-preferred int encoding: a CBOR numeric value was encoded in non-canonical form
dcbor REJECTED out-of-order map keys: the decoded CBOR map has keys that are not in canonical order
```

**Verification:** (a) compiled and ran, `probe-crypto` `probe_dcbor` and `cbordet` bin.

**Why it is not the primary choice.** It pulls `chrono`, `thiserror`, and (via
`iana-time-zone`) `windows-core`, `windows-implement`, `windows-interface`,
`windows-result`, `windows-strings` — 16 additional crates for date handling we do not
need on a security-critical path, against spec §28's "minimal number of dependencies".
Its API is also a dynamic value model (`CBOR`, `Map`) rather than a derive, so every event
type would be hand-marshalled — more code, more places to get a field wrong.
**Verification:** (a) `cargo add dcbor@0.25.2` reported exactly those 16 additions.

**Keep it as a second-source oracle.** It is a good differential-testing partner: in the
adversarial suite (§25), assert that our encoder's output is accepted by
`dcbor::CBOR::try_from_data` and re-encodes to itself. That gets us independent RFC 8949
conformance checking without putting `dcbor` on the hot path.

**Cost of doing so, measured.** Adding `dcbor` as a `[dev-dependencies]` entry makes
`cargo deny check advisories` **fail**:

```
paste 1.0.15 — unmaintained advisory detected
├ ID: RUSTSEC-2024-0436
├ The creator of the crate `paste` has stated ... that this project is no longer
  maintained as well as archived the repository
├ Solution: No safe upgrade is available!
```

`cargo audit` reports it as an allowed warning and still exits 0; `cargo deny` treats it as
a failure. It is *informational/unmaintained*, not a vulnerability, and it is a
compile-time proc-macro helper in a **dev-dependency** — it ships in no release binary.
**Verification:** (a) executed — `cargo deny check advisories` on `probe-crypto-final`
with the dev-dependency present.

Resolution: scope the ignore narrowly rather than globally, and revisit if `dcbor` drops
`paste`:

```toml
# deny.toml
[advisories]
ignore = [
    # dcbor -> paste, dev-dependency only (differential CBOR oracle), never in a shipped binary
    "RUSTSEC-2024-0436",
]
```

If the team would rather keep `cargo deny` unconditionally green, drop `dcbor` and write
the RFC 8949 §4.2.1 conformance assertions by hand against the fixed test vectors recorded
in §4.2 and §4.4 of this document. Both are defensible; the ignore is the cheaper one.

### 4.5 `serde_cbor` 0.11.2 — rejected

Two RustSec advisories, both verified against the advisory-db:

- **RUSTSEC-2021-0127** — *unmaintained*; the author archived the repository and himself
  proposes `ciborium` and `minicbor` as replacements. `patched = []`.
- **RUSTSEC-2019-0025** (CVE-2019-25001, CVSS 7.5) — excessively nested semantic tags let a
  sub-1 kB document cause a **stack overflow**. Patched in 0.10.2, but the crate is dead.

Last published 2021-08-15. Spec §28 forbids exactly this. **Do not use.**

### 4.6 The chosen scheme: `minicbor` with arrays only

| | |
|---|---|
| Version | 2.3.0, published 2026-07-23 |
| Licence | **BlueOak-1.0.0** (permissive; needs an explicit `cargo-deny` allow entry) |
| Repository | https://github.com/twittner/minicbor |
| RustSec advisories | none |

The insight: **the map-ordering problem disappears if there are no maps.** Encode every
signed structure as a definite-length CBOR *array* with a fixed field order fixed by the
derive. Determinism becomes structural — there is nothing left to sort, and no encoder
setting anyone can get wrong.

```rust
#[derive(minicbor::Encode, minicbor::Decode, PartialEq, Debug, Clone)]
#[cbor(array)]                      // definite-length array, fixed field order
struct EventBody {
    #[n(0)] protocol_version: u16,
    #[n(1)] table_id: u64,
    #[n(2)] hand_id: u64,
    #[n(3)] sequence: u64,
    #[cbor(n(4), with = "minicbor::bytes")] sender_public_key: [u8; 32],
    #[n(5)] event_type: u16,
    #[cbor(n(6), with = "minicbor::bytes")] payload: Vec<u8>,
    #[cbor(n(7), with = "minicbor::bytes")] previous_event_hash: [u8; 32],
    #[n(8)] deadline_ms: u64,
}
```

`with = "minicbor::bytes"` matters: without it, serde-style encoders emit `Vec<u8>` as a
CBOR *array of integers* (`83010203`) rather than a byte string (`43010203`) — roughly
double the bytes. `ciborium` does exactly this. **Verification:** (a) ran —
`ciborium Vec<u8> = 83010203`; the minicbor field emits `5820…` (definite 32-byte bstr).

Canonical integer encoding across the full width range, all shortest-form:

```
0 = 00     23 = 17     24 = 1818     255 = 18ff     256 = 190100
65535 = 19ffff     65536 = 1a00010000     u32::MAX = 1affffffff
u64::MAX = 1bffffffffffffffff
```

**Verification:** (a) compiled and ran, `cbordet` bin.

**Rules to enforce in review (and ideally with a lint):**

1. `#[cbor(array)]` on every signed or hashed struct. Never `#[cbor(map)]`.
2. Never `HashMap` in a signed or hashed type. If a mapping is unavoidable, use a
   `Vec<(K, V)>` sorted by encoded key, and validate sortedness on decode.
3. **No floats anywhere in the protocol.** Chips are integers. Floats bring
   preferred-float-shortening and NaN-canonicalisation rules we do not want to litigate.
   (`ciborium` shortens `1.0f64` to the half-precision `f93c00` — correct per RFC 8949, and
   exactly the kind of subtlety to avoid.)
4. Field indices `#[n(..)]` are **append-only forever**. Reusing an index silently changes
   the meaning of historical signed bytes.

### 4.7 The canonicality gate

`minicbor` does not validate canonicality on decode — it is a codec, not a validator. It
happily accepts non-preferred integers, indefinite lengths and trailing garbage. Left
alone that is an equivocation hole under §14: a hostile peer could ship two byte encodings
of one logical event to two different peers.

The fix is four lines. Decode, re-encode, and require the bytes to match:

```rust
/// Bytes that do not re-encode to themselves are rejected BEFORE any signature
/// check, so a hostile peer cannot smuggle two byte encodings of one logical event
/// past the equivocation detector.
fn decode_canonical(bytes: &[u8]) -> Result<EventBody, &'static str> {
    let ev: EventBody = minicbor::decode(bytes).map_err(|_| "malformed CBOR")?;
    let re = minicbor::to_vec(&ev).map_err(|_| "re-encode failed")?;
    if re.as_slice() != bytes {
        return Err("non-canonical encoding");
    }
    Ok(ev)
}
```

Proven to catch all three hostile encodings:

```
canonical   Ev{1,24} = 82011818
non-canon   Ev{1,24} = 8218011a00000018     (1 as 0x1801, 24 as 0x1a00000018)
  minicbor ACCEPTS it, decodes to Ev { v: 1, seq: 24 }
  re-encode = 82011818 ; re-encode == received? false   <-- gate fires
indefinite array (9f…ff)      -> re-encode == received? false   <-- gate fires
trailing bytes (…dead)        -> re-encode == received? false   <-- gate fires
truncated inputs (all lengths)-> all rejected, no panic
```

**Verification:** (a) compiled and ran, `canoncheck` bin and `probe-crypto-final` step `[3]`.

Alongside this, the **signature must always be verified over the exact received bytes**,
never over a re-encoding. Canonicalise-then-verify would let a peer's signature migrate
onto bytes it never signed.

### 4.8 Allocation bounding (spec §27)

A hostile length prefix must not cause an unbounded allocation. `minicbor` validates the
claimed length against the remaining input **before** allocating:

```
bstr claims 4 GiB, 0 bytes present     input=  7 B  allocated=         0 B  Err(end of input bytes)
bstr claims u64::MAX, 0 bytes present  input= 11 B  allocated=         0 B  Err(end of input bytes)
array claims 4 GiB elements            input=  6 B  allocated=         0 B  Err(end of input bytes)
20000-deep nesting                     input=20001 B allocated=        12 B  Err(unexpected type array…)
honest 3-byte payload                  input=  6 B  allocated=         3 B  Ok
```

Zero bytes allocated for every hostile input, no panic, no stack overflow at 20 000 levels
of nesting. **Verification:** (a) compiled and ran, `allocbound` bin, measured with a
counting `GlobalAlloc`.

This is a good property but **not a substitute for message size caps**. Still impose an
explicit maximum frame size at the transport boundary per §27.

---

## 5. `zeroize` and `subtle`

| | `zeroize` 1.9.0 | `subtle` 2.6.1 |
|---|---|---|
| Licence | Apache-2.0 OR MIT | BSD-3-Clause |
| Repository | https://github.com/RustCrypto/utils | https://github.com/dalek-cryptography/subtle |
| Published | 2026-06-12 | 2024-06-24 |
| RustSec | none | none |

Verified usage — the derive, the wrapper, the explicit call, and constant-time comparison:

```rust
#[derive(Zeroize, ZeroizeOnDrop)]
struct DecryptionShare([u8; 32]);        // wiped on drop

let mut kek = Zeroizing::new([0u8; 32]); // wiped on drop, derefs to [u8; 32]
let mut scratch = os_random::<32>();
scratch.zeroize();
assert_eq!(scratch, [0u8; 32]);

use subtle::ConstantTimeEq;
assert!(bool::from(a.ct_eq(&b)));         // no early-exit timing leak
```

**Verification:** (a) compiled and ran, `probe-crypto-final` step `[6]` and `probe-crypto`.

`subtle` has not been released since 2024-06-24. That is stability, not abandonment — it is
a tiny, feature-complete crate maintained by dalek-cryptography and depended on by most of
the Rust crypto ecosystem. Worth a note in the dependency register rather than a concern.

Spec §21 targets for `Zeroizing` / `ZeroizeOnDrop`: the Ed25519 seed, the Argon2-derived
KEK, per-hand RNG secrets before reveal, decryption shares, and every mental-poker private
scalar. Caveat to document honestly: `zeroize` cannot reach copies the allocator, the
compiler, or the OS swap file already made. It reduces the window, it does not close it.

---

## 6. Secret storage — resolving the DPAPI ↔ portability tension

### 6.1 The tension is real

- **Spec §21**: on Windows, prefer an OS mechanism such as DPAPI/credential storage.
- **Spec §22**: the client must be *portable* — one copyable directory, no installer, no
  registry writes, no writes outside its own folder, profile stored next to the program so
  the whole directory can be copied to another machine.

DPAPI keys are derived from the user's Windows logon credential and machine state. A
DPAPI-wrapped blob **will not decrypt** after the directory is copied to another machine or
another user account. Taken naively, §21 and §22 contradict each other.

### 6.2 Recommendation: one file, multiple key slots

Borrowing the LUKS keyslot idea. The profile holds **one** file,
`profile/identity.key`, always in the same format on every OS:

```
identity.key
├── header (plaintext, authenticated as AAD)
│     magic "P2PPOKER-ID\0", format_version, slot descriptors
├── slot[0]  passphrase   : argon2id params + 16-byte salt + wrapped DEK   [PORTABLE]
├── slot[1]  dpapi        : DPAPI blob of the DEK (Windows only)           [CONVENIENCE]
├── slot[2]  plaintext    : DEK in the clear (prototype / CI only)         [INSECURE]
└── payload  XChaCha20-Poly1305(DEK, nonce, ed25519_seed), AAD = header
```

- The **Ed25519 seed is encrypted exactly once**, under a random 32-byte DEK.
- Each slot independently wraps that same DEK. Slots are additive.
- The header is bound as **associated data**, so a slot cannot be stripped or swapped
  without failing the AEAD tag.

This resolves the tension cleanly:

- **Portability (§22) is guaranteed** by slot 0. Copy the directory anywhere; the
  passphrase opens it. Nothing is written outside the folder, and nothing touches the registry.
- **OS protection (§21) is honoured** by slot 1. On the machine where the profile was
  created, the client opens silently with no prompt.
- Copying a profile with only slot 1 populated fails **loudly** at slot-1 unwrap. The
  client then falls back to slot 0, or — if slot 0 is empty — tells the user plainly that
  this profile was machine-bound and offers to generate a new identity. The failure is
  explicit, never silent.
- Slot 2 exists so the play-money prototype and CI can run unattended. It must be
  opt-in, and the GUI must show a persistent warning while it is active.

Spec §3 also requires that two installations never share a PeerId. The DEK and the seed are
both drawn from `SysRng` at first run; copying a profile deliberately copies the identity,
which is the documented meaning of "copy the directory".

### 6.3 Portable path: `argon2` 0.6.0 + `chacha20poly1305` 0.11.0

| | `argon2` 0.6.0 | `chacha20poly1305` 0.11.0 |
|---|---|---|
| Licence | MIT OR Apache-2.0 | Apache-2.0 OR MIT |
| Repository | https://github.com/RustCrypto/password-hashes | https://github.com/RustCrypto/AEADs |
| Published | 2026-08-27 | 2026-06-28 |
| MSRV | 1.85 | 1.85 |
| RustSec | none | none |

```rust
let salt: [u8; 16] = os_random();
let params = Params::new(19 * 1024, 2, 1, Some(32)).unwrap();  // m = 19 MiB, t = 2, p = 1
let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
let mut kek = Zeroizing::new([0u8; 32]);
a2.hash_password_into(passphrase, &salt, kek.as_mut())?;

// hybrid-array 0.4: &[u8; N] -> &Array<u8, UN> via From
let cipher = XChaCha20Poly1305::new((&*kek).into());
let nb: [u8; 24] = os_random();
let nonce: &XNonce = (&nb).into();
let ct = cipher.encrypt(nonce, &seed[..])?;
```

`m_cost` is in **KiB**, so `19 * 1024` is 19 MiB — the OWASP Argon2id baseline
(m = 19 MiB, t = 2, p = 1). Argon2id, not Argon2i or Argon2d: it is the hybrid recommended
by RFC 9106 for password-based key derivation.

Use **XChaCha20-Poly1305**, not ChaCha20-Poly1305: the 192-bit nonce makes a random nonce
per write safe without a counter that portable file copies would desynchronise.

Verified including negative cases: flipping one ciphertext bit makes `decrypt` fail the
Poly1305 tag. **Verification:** (a) compiled and ran, `probe-crypto-final` step `[7]`.

**API gotcha worth recording.** RustSec-era `Key::from_slice` / `Nonce::from_slice` are now
`#[deprecated]` under `hybrid-array` 0.4, and `kek.as_ref().into()` does **not** compile —
`&[u8]` has no `From` impl for `&Array<u8, U32>`, only `&[u8; N]` does:

```
error[E0277]: the trait bound `&Array<u8, …>: From<&[u8]>` is not satisfied
warning: use of deprecated associated function `Array::<T, U>::from_slice`: use `TryFrom` instead
```

Use `(&*kek).into()` and `(&nb).into()` on fixed-size arrays. **Verification:** (a) that is
literal compiler output from the first probe build, fixed in the second.

### 6.4 Windows convenience path: DPAPI via `windows-sys` 0.61.2

`windows-sys` is the right binding. `winapi` 0.3.9 is last-published 2020 and effectively
unmaintained; the higher-level `windows` crate pulls far more than we need. No dedicated
DPAPI wrapper crate is worth the supply-chain surface for two function calls.

Both functions exist in `windows-sys` 0.61.2 under the feature
`Win32_Security_Cryptography`:

```rust
windows_link::link!("crypt32.dll" "system" fn CryptProtectData(
    pdatain: *const CRYPT_INTEGER_BLOB, szdatadescr: PCWSTR,
    poptionalentropy: *const CRYPT_INTEGER_BLOB, pvreserved: *const c_void,
    ppromptstruct: *const CRYPTPROTECT_PROMPTSTRUCT, dwflags: u32,
    pdataout: *mut CRYPT_INTEGER_BLOB) -> BOOL);
```

**Verification:** (b) source —
`windows-sys-0.61.2/src/Windows/Win32/Security/Cryptography/mod.rs`, lines 283 and 315.
Note the blob struct is named `CRYPT_INTEGER_BLOB`, **not** `DATA_BLOB` as in the C headers.

Working wrapper (full source in `probe-crypto-final/src/main.rs`, module `dpapi`):

```rust
fn blob(v: &mut [u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: v.len() as u32, pbData: v.as_mut_ptr() }
}

pub fn protect(data: &[u8], entropy: &[u8]) -> Result<Vec<u8>, u32> {
    let (mut d, mut e) = (data.to_vec(), entropy.to_vec());
    let (din, ein) = (blob(&mut d), blob(&mut e));
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe {
        CryptProtectData(&din, std::ptr::null(), &ein, std::ptr::null(),
                         std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
    };
    if ok == 0 { return Err(unsafe { GetLastError() }); }
    let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe { LocalFree(out.pbData as _) };
    Ok(v)
}
```

Verified **actually protecting and unprotecting** a byte string on this machine:

```
[8] DPAPI wrap/unwrap ok (48 -> 278 B), wrong entropy rejected
```

**Verification:** (a) compiled and ran, `probe-crypto-final` step `[8]`. Three properties
asserted: round-trip equality; wrong optional entropy is rejected; `LocalFree` is called on
the output blob (the API allocates it with `LocalAlloc`, so not freeing it leaks).

Two details that matter:

- Always pass `CRYPTPROTECT_UI_FORBIDDEN`. A background thread must never be able to raise
  a modal Windows prompt.
- Always pass application entropy (`b"p2p-poker/identity/v1"`). Without it, any other
  process running as the same user can unprotect our blob.

### 6.5 Linux side

**Recommendation: a 0600 file, not the `keyring` crate.**

`keyring` 4.1.6 (MIT OR Apache-2.0, MSRV 1.88.0) is a real option — its
`keyring-core` 1.0.0 `Entry` exposes `new`, `set_secret`, `get_secret`,
`delete_credential` (**(b) source**, `keyring-core-1.0.0/src/lib.rs` lines 130/236/281/358),
and `Entry::new` succeeds on this Windows machine (**(a) compiled and ran**, `probe-crypto`).

But its default `v1` feature set routes Linux through `zbus-secret-service-keyring-store`
(**(b) source**, `keyring-4.1.6/Cargo.toml`). That needs a D-Bus session bus and a running
Secret Service daemon — absent on headless Linux, in containers, and over plain SSH. That
directly contradicts spec §22's "just copy the directory and run it". It also drags `zbus`,
`tokio` and the D-Bus stack into a build we are trying to keep minimal per §28.

So: slot 1 on Linux is a file with mode `0600` inside the profile directory, set via
`std::os::unix::fs::PermissionsExt`, with the parent directory at `0700`. The passphrase
slot remains the real protection; the file mode is defence in depth against other local
users, exactly as spec §21 permits ("bezpečný keyring **nebo** soubor s přísnými permissions").

**Honest limitation: the Linux path is not compile-verified.** Only
`x86_64-pc-windows-msvc` is installed here (`rustup target list --installed`), so the
`cfg(unix)` branch has never been through a compiler. It must be verified on a Linux host
before Phase 7. Do not treat the 0600 code as proven.

`keyring` can be revisited later as an *optional* extra slot behind a cargo feature; it is
not in the recommended dependency block.

### 6.6 What must never be logged (spec §21)

Private keys; other players' hole cards; decryption shares that must stay secret; raw RNG
secrets before their safe reveal; the passphrase; the DEK or KEK. Implement this as a
newtype whose `Debug` and `Display` print `"<redacted>"`, so it is enforced by the type
system rather than by remembering.

---

## 7. Supply-chain tooling (spec §28)

### 7.1 `cargo-deny` 0.20.2 — installs and runs clean

```
$ cargo install cargo-deny
   Installed package `cargo-deny v0.20.2` (executable `cargo-deny.exe`)

$ cargo deny check advisories bans sources
advisories ok, bans ok, sources ok        (exit 0)
```

**Verification:** (a) executed against `probe-crypto-final` with the runtime dependency set.

Adding the optional `dcbor` **dev**-dependency flips `advisories` to FAILED via
`paste 1.0.15` / RUSTSEC-2024-0436 (unmaintained, no vulnerability, dev-only). See §4.4 for
the measured output and the scoped `ignore` entry.

### 7.2 `cargo-audit` 0.22.2 — **needs `--locked`**

The plain install **fails** on this toolchain. Full error:

```
error: failed to compile `cargo-audit v0.22.2`
Caused by:
  rustc 1.95.0 is not supported by the following package:
    kstring@2.0.4 requires rustc 1.96.0
  Try re-running `cargo install` with `--locked`
```

A transitive dependency resolved to a version whose MSRV is ahead of our rustc 1.95.0.
`--locked` uses the versions in the crate's shipped `Cargo.lock` and succeeds:

```
$ cargo install cargo-audit --locked
   Installed package `cargo-audit v0.22.2` (executable `cargo-audit.exe`)

$ cargo audit
      Loaded 1226 security advisories (from ~\.cargo\advisory-db)
    Scanning Cargo.lock for vulnerabilities (52 crate dependencies)
                                          (exit 0, no findings)
```

**Verification:** (a) both the failure and the fix are literal terminal output.

*That clean run is the **probe's** lockfile — 52 crates.* The integrated repository
resolves 612 and does **not** exit 0. See §7.3.2.

**Document `--locked` in the contributor README and use it in CI.** Anyone following the
obvious instructions on this toolchain will otherwise hit the same wall.

### 7.3 Advisory status

Two different questions, two different answers. Keeping them apart is the whole point
of this section.

#### 7.3.1 The recommended set, queried crate by crate

Queried against the RustSec advisory-db (**(c)**, `github.com/rustsec/advisory-db`),
and re-checked against the versions `Cargo.lock` actually resolves at the repository
root (**(a)**):

| Crate | Resolved in `Cargo.lock` | Advisories | Affects us? |
|---|---|---|---|
| `ed25519-dalek` | 2.2.0, **3.0.0** | RUSTSEC-2022-0093 | No — `patched >= 2`; both resolved majors are past it |
| `curve25519-dalek` | 4.1.3, **5.0.0** | RUSTSEC-2024-0344 | No — `patched >= 4.1.3`; both resolved versions are at or past it |
| `sha2` | 0.10.9, **0.11.0** | RUSTSEC-2021-0100 | No — `patched >= 0.9.8` |
| `rand` | **0.8.8, 0.9.5, 0.10.2** | RUSTSEC-2026-0097 | No — **but not because the crate is absent.** All three resolved versions are past a patch line |
| `rand_chacha` | 0.3.1, 0.9.0 | none | — |
| `rand_core` | 0.6.4, 0.9.5, **0.10.1** | none | — |
| `getrandom` | 0.2.17, 0.3.4, **0.4.3** | none | — |
| `serde_cbor` | **absent** | RUSTSEC-2019-0025, RUSTSEC-2021-0127 | Rejected, and confirmed not in the lockfile |
| `k256`, `ciborium`, `keyring`, `dcbor` | **absent** | none / n.a. | Never adopted; confirmed not in the lockfile |
| `blake3` 1.8.7, `zeroize` 1.9.0, `subtle` 2.6.1, `minicbor` 2.3.0, `argon2` 0.6.0, `chacha20poly1305` 0.11.0 | as pinned | none | — |

Bold marks the version this document pins; the others are what libp2p, arkworks and
the DHT bring with them (`INTEGRATION.md` §2 explains why the duplicates are not a
defect).

**The `rand` row is a correction.** An earlier revision of this table said
"**Crate not in the tree at all**". That was true of `probe-crypto-final` and is
**false of the integrated tree**. The conclusion happens to survive, but on a weaker
and more fragile basis: the advisory's `[versions] patched` field reads

```toml
patched = [">= 0.10.1", "< 0.10.0, >= 0.9.3", "< 0.9.0, >= 0.8.6"]
```

and 0.8.8 ≥ 0.8.6, 0.9.5 ≥ 0.9.3, 0.10.2 ≥ 0.10.1. Every one of the three clears a
patch line by a small margin. An absence cannot regress; a patched version can. This
row must therefore be re-checked after every `cargo update`, not filed away as
structurally impossible.

**Verification:** (b) source —
`~/.cargo/advisory-db/crates/rand/RUSTSEC-2026-0097.md`, quoted above
verbatim; (a) executed — `Cargo.lock` at the repository root, and
`cargo tree --edges normal` for what is compiled.

Two further notes the advisory scan does not raise but a reader of this table should
have:

- `chacha20poly1305` also appears in `Cargo.lock` at **0.10.1** (via `snow` ←
  `libp2p-noise`), but it is **not compiled** on this host target:
  `cargo tree --edges normal -i chacha20poly1305@0.10.1` prints
  `warning: nothing to print.`, and only `--target all` reveals the edge. It carries
  no advisory either way. It is a clean illustration of §7.3.3.
- `cbor4ii` **is** in the integrated tree at 0.3.3, pulled in by
  `libp2p-request-response`'s `cbor` feature. §4.2 and §4.3 rejected it as *our*
  canonical encoder and that still holds — but "not used by us" is now the accurate
  phrasing, not "not present".

#### 7.3.2 `cargo audit` on the integrated repository — **not clean**

Run at the repository root, `cargo-audit-audit 0.22.2`, advisory-db as of 2026-08-28:

```
$ cargo audit
    Fetching advisory database from `https://github.com/RustSec/advisory-db.git`
      Loaded 1226 security advisories (from ~\.cargo\advisory-db)
    Updating crates.io index
    Scanning Cargo.lock for vulnerabilities (612 crate dependencies)

Crate:     hickory-proto
Version:   0.25.2
Title:     NSEC3 closest-encloser proof validation enters unbounded loop on cross-zone responses
Date:      2026-05-01
ID:        RUSTSEC-2026-0118
Solution:  No fixed upgrade is available!

Crate:     hickory-proto
Version:   0.25.2
Title:     CPU exhaustion during message encoding due to O(n²) name compression
Date:      2026-05-01
ID:        RUSTSEC-2026-0119
Solution:  Upgrade to >=0.26.1

Crate:     paste
Version:   1.0.15
Warning:   unmaintained
ID:        RUSTSEC-2024-0436

Crate:     lru
Version:   0.16.4
Warning:   unsound
Title:     Potential use-after-free due to lack of panic safety in `LruCache::pop()`
ID:        RUSTSEC-2026-0253

error: 2 vulnerabilities found!
warning: 2 allowed warnings found
```

**Verification:** (a) literal terminal output, elided only in the `URL:` lines.

None of the four is in this document's set. `hickory-proto` arrives with libp2p's
`dns` feature and `lru` with `mainline`.

`paste` is the one whose story this document got wrong in a second way: §4.4 treats it
as a `dcbor` **dev**-dependency, to be silenced with a scoped `ignore`. In the integrated
tree it is a **runtime** proc-macro dependency —
`cargo tree --edges normal -i paste@1.0.15` gives
`paste ← ark-ff 0.5.0 ← ark-ec/ark-poly/ark-secp256k1 ← ziffle ← p2p-poker` — and it is
there whether or not `dcbor` is ever added. The scoped ignore in §4.4 is therefore no
longer "dev-only", and that is a different risk decision from the one §4.4 recorded.
It is unmaintained, not vulnerable, so nothing is on fire; but the justification has to
be rewritten before it is used. **Verification:** (a) executed.

**The provenance of all four, the decision to keep the `dns` feature, and the
Mainline-only alternative that would remove both hickory advisories are recorded in
`INTEGRATION.md` §3, which is the authority for them.** Do not re-litigate them here.

#### 7.3.3 `cargo audit` reads the lockfile, so it is feature-blind

`cargo audit` scans `Cargo.lock`, not the build graph, and a lockfile records the
union of the resolved graph's optional dependencies regardless of which features are
switched on. `INTEGRATION.md` §3 measured this directly: with libp2p's `dns` feature
removed, `hickory-proto` is no longer compiled at all —
`cargo tree --edges normal -i hickory-proto` prints `warning: nothing to print.` —
and yet `cargo audit` still reports both hickory advisories. The
`chacha20poly1305` 0.10.1 case in §7.3.1 is the same effect on the target axis.

Two consequences, and they cut in opposite directions:

- **A clean `cargo audit` is not evidence that a vulnerable crate is out of the
  binary, and a dirty one is not evidence that it is in.** It has no feature
  awareness at all.
- `cargo tree --edges normal -i <crate>` is the tool that answers what is actually
  compiled. Spec §28 asks for a security status per dependency, and "flagged by
  audit but not compiled" is a different status from "compiled and vulnerable".

Note the gap in the header line itself: **612 crate dependencies scanned**, against
**426** crates actually compiled (`cargo tree --edges normal`, unique name+version
pairs). `INTEGRATION.md` §3 records 611 and §1 records 425 for its own run; `Cargo.lock`
has been re-resolved since (its mtime is later than that document's), so the tree has
drifted by one crate. The finding is unaffected — but re-run the command before
quoting either number, rather than copying it from here.

### 7.4 Licences requiring an explicit `cargo-deny` allow-list

`cargo deny list` over the final tree found these beyond the usual MIT/Apache-2.0:

| Licence | Crates |
|---|---|
| BSD-3-Clause | `curve25519-dalek`, `ed25519-dalek`, `subtle` |
| **BlueOak-1.0.0** | `minicbor`, `minicbor-derive` |
| CC0-1.0 | `blake3`, `constant_time_eq` |
| MIT-0 | `constant_time_eq` |
| BSD-1-Clause | `fiat-crypto` |
| Unicode-3.0 | `unicode-ident` |
| Apache-2.0 WITH LLVM-exception | `blake3` |
| LGPL-2.1-or-later | `r-efi` (also MIT/Apache; **UEFI target only, never compiled on Windows or Linux**) |

All are permissive and compatible with an open-source client. **BlueOak-1.0.0 is the one to
watch** — it is uncommon enough that a default `cargo-deny` licence policy will reject it,
and `r-efi`'s LGPL facet will trip a naive scan even though that crate is never built for
our targets. Both need explicit entries:

```toml
# deny.toml
[licenses]
allow = [
    "MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception",
    "BSD-1-Clause", "BSD-2-Clause", "BSD-3-Clause", "BSD-2-Clause-Patent",
    "BlueOak-1.0.0", "CC0-1.0", "MIT-0", "Unicode-3.0", "ISC",
]

[bans]
multiple-versions = "warn"   # see section 8

[advisories]
yanked = "deny"
ignore = [
    # dcbor -> paste, dev-dependency only (differential CBOR oracle); see section 4.4
    "RUSTSEC-2024-0436",
]
```

---

## 8. Cross-layer warning: duplicate crate versions are coming

> **This section was a prediction, and it has since been measured.**
> `INTEGRATION.md` §2 lists the duplicates that actually resolved and explains why
> they are not a defect. Where it and this section differ, it wins. The prediction
> is kept because it was substantially right and the reasoning still applies.

Not a blocker, but the integration agent must know. Verified via crates.io dependency
endpoints (**(c)**):

| Consumer | Wants |
|---|---|
| `libp2p-identity` (used by libp2p 0.56) | `ed25519-dalek ^2.1`, `k256 ^0.13.4`, `sha2 ^0.10.8`, `rand ^0.8`, `zeroize ^1.8` |
| `ziffle` 0.1.0 (mental-poker candidate) | `ark-*` 0.5, `sha2 ^0.10.9`, `zeroize ^1.8.2` |
| **This document** | `ed25519-dalek 3.0`, `sha2 0.11`, `rand_core 0.10` — *and, as originally written, "no `rand`"; see §1.2, that part did not survive integration* |

Cargo will therefore link **two copies of `ed25519-dalek`** (2.x and 3.x), **two of `sha2`**
(0.10 and 0.11), and pull `rand 0.8` back in through libp2p. That compiles — the versions
are semver-incompatible so they coexist — but three consequences must be handled:

1. **Types do not interoperate across versions.** An `ed25519_dalek::VerifyingKey` from 3.0
   cannot be passed to a libp2p API expecting the 2.x type. Cross the boundary as raw
   `[u8; 32]` and reconstruct. This is *good* design anyway: spec §20 insists the libp2p
   PeerId and the poker application identity stay separate, so the two key types should
   never meet.
2. **Binary size and audit surface roughly double** for those crates.
3. `rand 0.8` returns via libp2p, so RUSTSEC-2026-0097 must be re-checked at integration
   time. It does not affect 0.8 (`affected.functions` lists `rand::thread_rng` for
   `>= 0.7.0, < 0.10.0` with `patched >= 0.8.6`), but confirm the resolved version is ≥ 0.8.6.

   **Re-checked, and the prediction was too narrow.** `rand` came back at *three*
   majors, not one: 0.8.8 (≥ 0.8.6, clear), 0.9.5 (≥ 0.9.3, clear) and 0.10.2
   (≥ 0.10.1, clear). It is also a **direct** dependency of `p2p-poker`, not only a
   transitive one. §1.2 has the provenance table and §7.3.1 the advisory arithmetic.

Set `multiple-versions = "warn"` rather than `"deny"` in `deny.toml`, or the build will fail
on a condition we cannot fix without forking libp2p.

---

## 9. Nothing failed to build

Every crate in the recommended block compiles and runs on rustc 1.95.0 /
x86_64-pc-windows-msvc. Two crates were **rejected on evidence**, not on failure to build:

- **`serde_cbor` 0.11.2** — unmaintained (RUSTSEC-2021-0127) plus a stack-overflow CVE
  (RUSTSEC-2019-0025). Not evaluated further.
- **`keyring` 4.1.6** — compiles and `Entry::new` succeeds here, but its Linux backend
  requires a D-Bus Secret Service, which breaks spec §22 portability. Deferred, not adopted.

And two were **evaluated and set aside as primary**, both fully working:

- **`ciborium` 0.2.2` / `cbor4ii` 1.2.2** — work fine, but are not deterministic (§4.2, §4.3).
- **`dcbor` 0.25.2** — fully conformant, kept as a differential-testing oracle rather than a
  hot-path dependency (§4.4).

One tooling install failed and was fixed: `cargo-audit` needs `--locked` (§7.2).

---

## 10. The dependency block

> **Do not copy this block blind any more.** The workspace `Cargo.toml` now exists at
> the repository root, it already carries every line below, and **it is the
> authoritative manifest** — this section is a record of what the probe compiled,
> reconciled against it. If the two ever differ, `Cargo.toml` and `INTEGRATION.md`
> are right and this is stale.

Compiled **together in one probe crate** (`probe-crypto-final`), which then ran all eight
checks successfully.

```toml
[dependencies]
# --- signatures and group operations -------------------------------------
ed25519-dalek    = { version = "3.0.0",  default-features = false, features = ["fast", "zeroize", "rand_core"] }
curve25519-dalek = { version = "5.0.0",  default-features = false, features = ["alloc", "precomputed-tables", "zeroize", "digest"] }

# --- hashing --------------------------------------------------------------
# blake3 is the protocol hash (keyed + derive_key modes give domain separation).
# sha2 is mandatory anyway: Ed25519 is defined over SHA-512.
sha2   = { version = "0.11.0", default-features = false, features = ["alloc"] }
blake3 = { version = "1.8.7",  default-features = false, features = ["std"] }

# --- OS CSPRNG ------------------------------------------------------------
# NOTE: `OsRng` no longer exists. The OS source is `getrandom::SysRng`.
# Our own code draws cryptographic randomness ONLY from `getrandom::SysRng`,
# through src/security/rng.rs. Spec section 7 forbids SmallRng, StdRng and any
# self-seeded generator; that is a reviewable boundary, NOT an absence -
# see CRYPTO_LIBS.md section 1.2.
rand_core = { version = "0.10.1", default-features = false }
getrandom = { version = "0.4.3",  default-features = false, features = ["std", "sys_rng"] }

# Present only because libp2p-autonat needs rand_core 0.6's OsRng. Our own
# code must never draw cryptographic randomness from it - spec section 7
# forbids SmallRng and self-seeded generators. See CRYPTO_LIBS.md section 1.2.
rand = "0.8"

# --- secret hygiene -------------------------------------------------------
zeroize = { version = "1.9.0",  default-features = false, features = ["alloc", "derive"] }
subtle  = { version = "2.6.1",  default-features = false }

# --- canonical serialisation (spec section 12) ----------------------------
# Arrays only, never maps: determinism is structural, so there is nothing to sort.
# Always pair with the decode-re-encode-compare canonicality gate.
minicbor = { version = "2.3.0", default-features = false, features = ["std", "derive"] }

# --- profile secret storage ----------------------------------------------
argon2           = { version = "0.6.0",  default-features = false, features = ["alloc", "zeroize"] }
chacha20poly1305 = { version = "0.11.0", default-features = false, features = ["alloc", "zeroize"] }

[target.'cfg(windows)'.dependencies]
# DPAPI convenience keyslot: CryptProtectData / CryptUnprotectData.
windows-sys = { version = "0.61.2", features = ["Win32_Security_Cryptography", "Win32_Foundation"] }

[dev-dependencies]
# Second-source RFC 8949 section 4.2 oracle for the adversarial suite.
# NOT a runtime dependency: it pulls chrono and 15 other crates.
dcbor = "0.25.2"
```

**Probe tree:** **41 runtime crates** (`cargo tree --edges normal`), 61 counting build-
and dev-dependencies. No `rand` at any depth. `cargo audit` exits 0; `cargo deny check
advisories bans sources` is clean for the runtime set and needs the one `RUSTSEC-2024-0436`
ignore once the `dcbor` dev-dependency is added (§4.4).
**Verification:** (a) `cargo tree`, `cargo audit` and `cargo deny` executed on
`probe-crypto-final` with exactly the block above, minus the `rand = "0.8"` line, which
the probe did not have.

**Integrated tree:** **426 crates compiled**, **612 recorded in `Cargo.lock`**, `rand`
present at three majors, and `cargo audit` exits **1** with two vulnerabilities that
belong to libp2p's DNS stack and to `mainline` (§7.3.2, `INTEGRATION.md` §1 and §3).
The sentence "No `rand` at any depth" above is a property of the probe and of nothing
else.

### Locked versions — the probe

```
argon2 0.6.0            curve25519-dalek 5.0.0   minicbor 2.3.0      signature 3.0.0
blake3 1.8.7            ed25519 3.0.0            rand_core 0.10.1    subtle 2.6.1
chacha20poly1305 0.11.0 ed25519-dalek 3.0.0      sha2 0.11.0         windows-sys 0.61.2
                        getrandom 0.4.3          zeroize 1.9.0
```

### Locked versions — the integrated tree

What the repository's `Cargo.lock` actually resolves for the crates this document
governs. **Bold** is the version pinned above; the rest arrive with libp2p, arkworks,
`mainline` or `rs_poker`. The `rand` family was missing from the probe list entirely
and is spelled out here because that omission is what made §7.3 wrong.

| Crate | Versions in `Cargo.lock` |
|---|---|
| `rand` | 0.8.8, 0.9.5, 0.10.2 — *none of them in the probe* |
| `rand_chacha` | 0.3.1, 0.9.0 — *neither in the probe* |
| `rand_core` | 0.6.4, 0.9.5, **0.10.1** |
| `getrandom` | 0.2.17, 0.3.4, **0.4.3** |
| `ed25519-dalek` | 2.2.0, **3.0.0** |
| `ed25519` | 2.2.3, **3.0.0** |
| `curve25519-dalek` | 4.1.3, **5.0.0** |
| `signature` | 2.2.0, **3.0.0** |
| `sha2` | 0.10.9, **0.11.0** |
| `digest` | 0.10.7, 0.11.3 |
| `chacha20poly1305` | 0.10.1 (locked, not compiled — §7.3.1), **0.11.0** |
| `cbor4ii` | 0.3.3 — *present via libp2p, not used by us* |
| `windows-sys` | 0.52.0, 0.59.0, 0.60.2, **0.61.2** |
| `argon2` | **0.6.0** |
| `blake3` | **1.8.7** |
| `minicbor` | **2.3.0** |
| `subtle` | **2.6.1** |
| `zeroize` | **1.9.0** |
| `serde_cbor`, `k256`, `ciborium`, `keyring`, `dcbor` | absent |

**Verification:** (a) read from `Cargo.lock` at the repository root; the
compiled-versus-locked distinction cross-checked with `cargo tree --edges normal`.

### Final probe output

```
[1] getrandom::SysRng ok (no `rand` crate in the tree)
[2] event 91 B, signed, verify_strict ok
[3] canonicality gate: trailing bytes + indefinite length rejected
[4] ristretto255 homomorphism + commutativity ok (d03b86fabb3d...)
[5] sha2 KATs pass; blake3 domain separation + length prefixing verified
[6] zeroize (derive + explicit) and subtle ct_eq ok
[7] argon2id + XChaCha20Poly1305 portable store ok, tag rejects tampering
[8] DPAPI wrap/unwrap ok (48 -> 278 B), wrong entropy rejected

FINAL PROBE: ALL CHECKS PASSED
```

Line `[1]`'s parenthetical is literal probe output and is left as it was printed. It
was true of the probe. In the integrated tree the `rand` crate **is** in the tree; what
the check still proves there is the part that matters — that the bytes came from
`getrandom::SysRng`.

---

## 11. Open items for later phases

1. **Verify the Linux 0600 path on an actual Linux host.** Never compiled here (§6.5).
2. ~~**Re-run `cargo audit` / `cargo deny` after libp2p and the mental-poker crate land**~~
   — **done.** `cargo audit` was re-run on the integrated tree; the result is in §7.3.2
   and it is not clean. `cargo deny` on the integrated tree has **not** been re-run and
   remains open: §8's `multiple-versions = "warn"` advice now has to cover the `rand`,
   `rand_core`, `rand_chacha`, `getrandom`, `digest` and `windows-sys` families as well.
3. **The mental-poker layer may override the curve and the hash.** If its proof system
   mandates secp256k1 (as `ziffle`'s arkworks dependency suggests) or a specific
   Fiat–Shamir hash, that mandate wins for those bytes. This document governs the general
   layer only.
4. **Add the `dcbor` differential test** to the adversarial suite (§25): assert our encoder's
   output is accepted by `dcbor::CBOR::try_from_data` and re-encodes to itself.
5. **Record the §7 spec deviation** (`OsRng` → `getrandom::SysRng`) in
   `docs/CRYPTOGRAPHY.md` so no later reader thinks the requirement was dropped.
6. **Enforce the CBOR rules mechanically** if possible — a test that reflects over every
   signed type and fails on `HashMap`, floats, or a missing `#[cbor(array)]` is worth more
   than a review checklist.
