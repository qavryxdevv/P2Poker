# CRYPTOGRAPHY.md

Binding specification of the cryptography used by the p2p-poker client.

Required by `SPEC_CS.md` §29, which demands that this document state **exactly which
existing mental-poker protocol is used and which scholarly sources it comes from**.
It also discharges §6 (no home-made cryptography), §7 (distributed randomness and the
OS CSPRNG), §8 (verifiable shuffle), §9 (private hole cards), §10 (board hidden until
its street), §21 (local key protection), §28 (dependency register) and §36 (stop and
document rather than improvise).

**Status:** specification, Phase 1. No implementation exists yet. Every code fragment
below is illustrative or a verification artefact, not shipped source.

**Evidence base:** `docs/research/MENTAL_POKER.md` and `docs/research/CRYPTO_LIBS.md`,
plus direct source reading and one new probe crate written for this document
(`probe-cryptodoc`, §13).

**Verification convention.** Every significant claim carries a `Verification:` line
that is one of:

* **(a) compiled** — code exercising the claim was compiled and, where stated, run;
* **(b) source** — read in the unpacked crate under
  `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/<crate>-<version>/`;
* **(c) registry** — crates.io metadata or the RustSec advisory database;
* **(d) literature** — a published paper. Papers are cited, not re-derived. Where a
  security property rests only on (d) and has **not** been re-checked against the
  implementation, this document says so.

docs.rs and recollection were used as leads only, never as evidence.

**Honesty clause.** Per §18 and the closing paragraph of the spec, nothing here claims
the system makes cheating impossible. §11 states precisely which attack classes are
cryptographically prevented, which are only detected, and which are outside the reach
of the protocol.

**D-010 governs every consequence in this document.** A cryptographic failure — an
invalid shuffle proof, a bad DLEQ, a token published early or for an index that is not
due, a share that never arrives — makes the hand **stop**, and the stop is **neutral**:
stacks are restored to their start-of-hand values and no chips move, in any direction,
for any cause. The transcript still records which peer failed; that record is
**evidence with no automatic consequence**. Nothing in this version block-lists,
unseats or penalises a peer on the strength of a proof, and no proof or certificate
moves a chip. Wherever this document says a peer is *attributed*, it means exactly
that a signed observation enters the transcript — never that anything is taken from
them. The accepted cost is stated plainly in §2.10 and §11: the rage-quit escape
returns, and a losing player who goes silent gets their chips back.

**`DECISIONS.md` D-009 to D-013, and where each one lands here.** This block exists
because the sixth pass found that this document had **zero** mentions of D-011 while
four sibling documents had 74, 35, 26 and 20 (finding H7). It was left out of the
D-011 sweep on the judgement that a decision about document ownership did not touch a
cryptography document. That judgement was wrong — the ownership table of §14 was still
declaring the pre-D-011 discipline — and it was the *second* such miss, which is why
D-012 now requires every sweep to cover every document. A reader who wants to know
whether this document is in scope for a decision should find the answer here rather
than infer it from silence.

**It happened a third time, in this document, one decision later.** The Phase 3
gate found **zero** mentions of D-013 here, and this document was not opened in
that pass — the same omission that produced D-012's process rule, in the same
document that produced it, immediately after it was written. The rule was
therefore not merely broken; it was broken by the pass that would have been its
first test. Two things follow, and both are recorded rather than resolved by
resolve. First, **a rule that depends on an editor remembering to apply it is not
a control**, which is why the coverage check is now a count — occurrences of the
decision's identifier in each document, reported before and after every sweep —
rather than an assurance that the sweep happened. Second, a document is in scope
for a decision **by default**, and scoping one out is a judgement that must be
written down with its reason so a later reader can see it was made. Silence is
the failure mode all three misses share.

* **D-009 rule 1** — mandatory honest behaviour may never satisfy the equivocation
  predicate. **Nothing to mirror here, and that is the design working.** This document
  names equivocation twice (§7.1, §10.1) and both times defers to `PROTOCOL.md` §5.2
  without restating the predicate; the anti-replay slot key is `PROTOCOL.md`
  §5.2.1's alone.
* **D-009 rule 2** — a below-floor certificate is inert everywhere. Carried in §2.10,
  and re-checked at §2.8 item 2, §2.9 and §5.2 item 8.
* **D-015** — `TIMEOUT_VOTE`, `TIMEOUT_CERT` and `EquivocationProof` are defined on
  the wire and **not produced in version 1**. **The sweep of this document found one
  occurrence, not zero, and it is edited rather than reported clean: §2.10's
  disconnect analysis** branched on whether `|V| >= 2` let a certificate form and
  therefore whether the abort carried a name. It does not, at any `|V|`. Everything
  else in this document was checked and is genuinely untouched, for a reason worth
  stating because it is D-011 rule 1 paying off: this document owns constructions,
  and **no construction here takes a certificate or a proof as an input**. `apk`,
  the shuffle argument, `ctx`, the DLEQ proofs, the beacon and the key hierarchy are
  all functions of chained deck content, and the two domain strings are separate
  (`"p2p-poker v1 deck-ctx"` against `"p2p-poker v1 timeout-cert"`, `PROTOCOL.md`
  §2.8), so a change to the second cannot reach the first. §8.1's D-014 analysis is
  likewise untouched: its evidence is a **failed proof**, self-authenticating from
  the offending event's own bytes, and D-014 excludes an equivocation proof by name.
* **D-009 rule 3** — state the discipline, never an absence in the dependency tree.
  Carried in §7.2, §11 OQ-8 and §12 item 6.
* **D-010** — the paragraph above; every consequence in this document.
* **D-011 rule 1** — one normative owner per concept. Applied in the sixth pass and
  completed in this one. **Five** constructions that `PROTOCOL.md` owns were
  **reproduced** here, and every reproduction is now a reference by section number: the
  `ctx` construction (§6.4 → `PROTOCOL.md` §4.5), the deck-index map (§2.4 →
  `PROTOCOL.md` §4.5), the hash constructor `h` and the domain-string register (§6.4,
  §7.3 → `PROTOCOL.md` §2.8), the `DOMAIN_EVENT` prefix bytes (already a reference,
  `PROTOCOL.md` §13), and — the one the sixth pass missed — the RNG beacon's
  `commitment_i` and `seed` blocks (§7.3 → `PROTOCOL.md` §4.4), which is `K-6`. That
  miss is worth naming: the sweep deleted the `ctx` copies because `ctx` was what the
  finding named, and left two copies of a different construction standing on the same
  page. **A sweep run against the finding rather than against the rule finds what the
  finding named and nothing else.** What
  this document keeps and owns is the **constructions and the arguments about them**:
  why `ctx` must carry what it carries, what ziffle does and does not bind, why n-of-n
  and not `t`-of-`n`, why the map must be fixed before the shuffle chain. What it no
  longer holds is a second copy of anybody else's byte layout. **D-011 rule 2** (the
  slot key written once) and **rule 3** (no automated eviction at any layer) need no
  edit here: this document never wrote a slot key, and D-010's neutrality already
  removed every eviction sentence.
* **D-012** — no canonical state derived from a per-receiver quantity. Swept in the
  sixth pass. Three sites were checked and one needed the pin made explicit; see §2.1,
  §2.10 and §7.3, and the summary in §15 note 5. **One of those three checks passed
  for the wrong reason and is corrected under D-013 below.**
* **D-013** — liveness is inherited from the chain, not from a seat's status. The
  rule, the required emitter set and its notation are `PROTOCOL.md` §3.2's, and this
  document restates none of them, because none of them is cryptography: no
  construction here consults a seat's status, and the joint key's membership is
  already `dealt_in`, which is chained content. **What D-013 changes here is the
  record, not the wire, and the distinction is the whole of the item.** Its J1 rule
  removed `advert_hash` from `GENESIS(0)`, from `session_id` and transitively from
  `ctx`, which made §6.4's discipline item 1 **true**. That item asserted the
  condition as though it had been checked, and it had not been — the violating path
  ran through `session_id`, two documents away, and neither document knew the path
  existed. The correction is at §6.4 item 1 and it is written in the past tense
  deliberately: **a claim that was false when made and is true now must not be left
  reading as evidence that the check was once run**, because the next editor will
  treat it as a discharged obligation and will not re-run it. That is `J-5`.
  D-013 also carries the process finding that a defect assigned across an owner
  boundary is recorded in `DECISIONS.md`'s open list in the same pass that finds it,
  which is where `K-6` was recorded and how §7.3 came to be fixed in this one.
* **D-014** — a cheater is removed from the table on self-authenticating evidence.
  **This document owns three of the six tier-1 clauses** — the shuffle proof, the
  decryption-share proof and the key-ownership proof — and carried **zero** occurrences
  of the identifier before this pass, while the decision's whole safety argument is about
  what those three proofs establish. That is `N9`, and it is the third consecutive pass
  in which this document was the one a decision did not reach: D-012's rule was written
  here, D-013 missed it (`J-5`), and D-014 missed it again. The count is reported rather
  than an assurance given, per the paragraph above: **0 before this pass**, and the
  occurrences after it are in the sweep record you are reading, in §2.1, §6.1, §8 rule 3
  and in **§8.1**, which is the new section and the substance of the item. What §8.1
  states for each proof is what a verification failure **proves** — evidence against its
  signer, needing no quorum, which is why tier 1 can act on arrival — and what it does
  **not** prove, in particular that a failure says *either the prover cheated or we are
  not looking at the same inputs*, so the public inputs must be accepted chain content or
  the verdict is a divergence and not a removal. §8.1.5 adds the one rule this document
  owes the decision: **a removal may rest only on a verifier that ran and returned
  invalid, never on a verification that could not be performed.** **Re-checked in the
  following pass and recorded as §8.1.6**, on the discipline of the paragraph above —
  the two properties D-014's safety rests on are asserted for this document as a whole
  rather than only where they are argued: **no tier-1 clause here needs receiver state**
  (the nearest candidate, §8 rule 3's undue index, is classified tier 2 by name), and
  **each of the three failures is stated as evidence against its own signer, needing no
  quorum, vote, certificate or timing**, which is what makes tier 1 safe to act on at
  arrival. One thing changed underneath the section and cost it nothing: D-014's tier-1
  list lost the clause *a message whose chain parent does not exist*, which is decidable
  only against the receiver's own store — nothing in §8.1 cited or depended on it, the
  six-bullet count this document quotes is unchanged, and the general admissibility test
  is now `THREAT_MODEL.md` §5.1.1's.

---

## 1. Summary table

**Module governed by this document (`SPEC_CS.md` §23, interim mapping).** This
document governs `src/mental_poker/` (`protocol.rs`, `deck.rs`, `shuffle.rs`,
`proofs.rs`, `reveal.rs`) and `src/security/`. The full §23 source tree is deferred to
`docs/ARCHITECTURE.md` at the start of Phase 2; this line exists so the separation of
transport, poker engine and cryptographic deck has an owner before the tree does.

| Layer | Construction | Library / pin | Origin (full citation) |
|---|---|---|---|
| Mental-poker card protocol | Threshold (n-of-n) ElGamal cards under an aggregate public key; per-card decryption shares | `ziffle 0.1.0` | Adam Barnett and Nigel P. Smart, *"Mental Poker Revisited"*, in Kenneth G. Paterson (ed.), **Cryptography and Coding — 9th IMA International Conference (IMACC 2003)**, LNCS 2898, Springer, 2003, pp. 370–383. |
| Verifiable shuffle | Bayer–Groth 2012 shuffle argument, instantiated at `m = 1` (single-value product argument, §5.3, plus the multi-exponentiation argument, §5) | `ziffle 0.1.0` | Stephanie Bayer and Jens Groth, *"Efficient Zero-Knowledge Argument for Correctness of a Shuffle"*, **EUROCRYPT 2012**, LNCS 7237, Springer, pp. 263–280. Author's copy: `http://www0.cs.ucl.ac.uk/staff/J.Groth/MinimalShuffle.pdf` — the URL cited in ziffle's own source. |
| Key-ownership proof | Schnorr proof of knowledge of a discrete logarithm, Fiat–Shamir transformed | `ziffle 0.1.0` | Claus-Peter Schnorr, *"Efficient Signature Generation by Smart Cards"*, **Journal of Cryptology** 4(3), 1991, pp. 161–174. Non-interactive transform: Amos Fiat and Adi Shamir, *"How to Prove Yourself"*, **CRYPTO '86**, LNCS 263, pp. 186–194. |
| Decryption-share correctness | Chaum–Pedersen proof of discrete-logarithm equality (DLEQ) | `ziffle 0.1.0` | David Chaum and Torben Pryds Pedersen, *"Wallet Databases with Observers"*, **CRYPTO '92**, LNCS 740, Springer, pp. 89–105. |
| Commitments inside the shuffle argument | Pedersen vector commitments over secp256k1, generators derived nothing-up-my-sleeve | `ziffle 0.1.0` | Torben Pryds Pedersen, *"Non-Interactive and Information-Theoretic Secure Verifiable Secret Sharing"*, **CRYPTO '91**, LNCS 576, Springer, pp. 129–140. |
| Group | secp256k1 (prime order, cofactor 1) | `ark-secp256k1 0.5.0` via `ark-ec`/`ark-ff` 0.5.0 | Certicom Research, **SEC 2: Recommended Elliptic Curve Domain Parameters**, v2.0, 2010, §2.4.1. |
| Public-key encryption of a card | ElGamal in an elliptic-curve group, additive notation | `ziffle 0.1.0` | Taher ElGamal, *"A Public Key Cryptosystem and a Signature Scheme Based on Discrete Logarithms"*, **IEEE Transactions on Information Theory** 31(4), 1985, pp. 469–472. |
| Application event signatures | Ed25519, `verify_strict` only | `ed25519-dalek 3.0.0` | Bernstein, Duif, Lange, Schwabe, Yang, *"High-speed high-security signatures"*, **CHES 2011** / J. Cryptographic Engineering 2(2), 2012. Standardised as RFC 8032. |
| Protocol hash: transcript, `state_hash`, RNG commitments | BLAKE3, keyed and `derive_key` modes for domain separation | `blake3 1.8.7` | O'Connor, Aumasson, Neves, Wilcox-O'Hearn, **BLAKE3: one function, fast everywhere**, specification, 2020. |
| Fiat–Shamir hash *inside* ziffle | SHA-256, `DefaultFieldHasher<Sha256>` hash-to-field | `sha2 0.10.9` (pulled by ziffle) | FIPS 180-4. Hash-to-field: Faz-Hernández, Scott, Sullivan, Wahby, Wood, **RFC 9380**, *Hashing to Elliptic Curves*. |
| Canonical bytes for signing | Deterministic CBOR: definite-length arrays only, no maps, no floats, plus a re-encode gate | `minicbor 2.3.0` | Carsten Bormann and Paul Hoffman, **RFC 8949**, *Concise Binary Object Representation (CBOR)*, §4.2 "Deterministic Encoding". |
| OS entropy | `getrandom::SysRng` / `getrandom::fill` | `getrandom 0.4.3` | Platform CSPRNG (`BCryptGenRandom` on Windows, `getrandom(2)` on Linux). |
| Profile secret at rest | Argon2id KEK + XChaCha20-Poly1305 AEAD, multiple key slots | `argon2 0.6.0`, `chacha20poly1305 0.11.0` | Biryukov, Dinu, Khovratovich, Josefsson, **RFC 9106**, *Argon2*. XChaCha20-Poly1305: Arciszewski, **draft-irtf-cfrg-xchacha**; ChaCha20-Poly1305 base: Nir and Langley, **RFC 8439**. |
| Windows convenience key slot | DPAPI `CryptProtectData` / `CryptUnprotectData` with application entropy | `windows-sys 0.61.2` | Microsoft Win32 `crypt32.dll` API. |

**Verification of the "Library / pin" column:** (a) compiled — `probe-cryptodoc`
(§13) builds against `ziffle = "=0.1.0"` and runs the whole card protocol; the
remaining pins were compiled together in `probe-crypto-final`
(`docs/research/CRYPTO_LIBS.md` §10). **Verification of the ziffle→paper mapping:**
(b) source — `ziffle-0.1.0/src/lib.rs` names Bayer–Groth 2012 and its section
numbers in comments, and its challenge domain tags are literally
`b"ziffle/BG12X/v1"`, `b"ziffle/BG12MultiExpArgX/v1"`,
`b"ziffle/BG12ProductArgX/v1"`, `b"ziffle/DLEQ/v1"`, `b"ziffle/DLOG/v1"`.

---

## 2. The chosen protocol, step by step

**Name, in one line: Barnett–Smart (IMACC 2003) mental poker, instantiated over
secp256k1 with an n-of-n aggregate ElGamal key and a Bayer–Groth (EUROCRYPT 2012)
shuffle argument, taken from the `ziffle 0.1.0` crate.**

### 2.0 Notation and the group

Let `G` be the standard secp256k1 generator and `F` the scalar field of order

```
n = 115792089237316195423570985008687907852837564279074904382605163141518161494337
  ≈ 2^256
```

**Verification: (b) source** — `ark-secp256k1-0.5.0/src/fields/fr.rs`, the
`#[modulus = "…"]` attribute on `FrConfig`, and `src/curves/mod.rs` line 20
(`type ScalarField = Fr`).

Group operations are written additively: `x·G` is scalar multiplication, `P + Q` is
point addition. The curve has cofactor 1, so every non-identity point is a generator
of the full prime-order group and there is no small-subgroup class of attack.

`N = 52` throughout; `ziffle`'s `Shuffle<const N: usize>` fixes the deck size at
compile time and statically asserts `N > 1`.
**Verification: (b) source** — `ziffle-0.1.0/src/lib.rs`,
`const _N_GREATER_THAN_1: () = assert!(N > 1);`.

A **card ciphertext** is an ElGamal pair `c = (c1, c2)` of curve points.

### 2.1 Key setup — the per-hand joint deck key

Every player `i` seated **and dealt in** for this hand generates a fresh key pair:

```
sk_i ←$ F          (from the OS CSPRNG, §7)
pk_i = sk_i · G
```

and a Schnorr proof of knowledge of `sk_i`:

```
w  ←$ F
a  = w · G
e  = H_dlog(ctx, pk_i, a)                       // hash-to-field, DST "ziffle/DLOG/v1"
z  = w + e · sk_i
proof = (a, z)                                   // 65 bytes compressed
verify:  z · G  ==  a + e · pk_i
```

**Verification: (b) source** — `ziffle-0.1.0/src/lib.rs`, `OwnershipProof::new` and
`OwnershipProof::verify`; the verifier line is literally
`let lhs = (GENERATOR.into_group() * self.z).into_affine();` and
`let rhs = (self.a.into_group() + (pk.0 * e)).into_affine();`.
**(a) compiled** — `probe-cryptodoc`, both proofs verify.

The proof of knowledge is not decoration. Without it a malicious player could publish
`pk_j = X − Σ_{i≠j} pk_i` for a target `X` whose discrete log it knows — a rogue-key
attack that would hand it sole control of the aggregate key. Requiring knowledge of
`sk_j` closes this.

**Under D-014 this proof is also evidence, and §8.1.1 states its limits.** A failed
key-ownership proof is one of the three tier-1 clauses `DECISIONS.md` D-014 rests on: it
is decidable from the message and `ctx` alone, with no game state in the verdict, which
is why it needs no quorum. What it does and does not establish — in particular that the
rejection is *itself* the mitigation, so nothing is lost by refusing to over-read it —
is §8.1.1.

Every player verifies every other player's proof, obtaining a `Verified<PublicKey>`,
and then all form the same **aggregate public key**:

```
apk = Σ_i pk_i = (Σ_i sk_i) · G
```

**Verification: (b) source** — `AggregatePublicKey::new(pks: &[Verified<PublicKey>])`;
the argument type is `Verified<PublicKey>`, so an unverified key cannot be aggregated,
and `Verified<T>` has a private field and no `CanonicalDeserialize` impl, so it cannot
be forged from outside the crate nor deserialised off the wire.
**Verification of the negative:** (a) compiled — `probe-ziffle/src/bin/bypass.rs`
(research) fails with `error[E0423]: cannot initialize a tuple struct which contains
private fields`.

The corresponding joint secret `Σ_i sk_i` **is never assembled anywhere**. It exists
only as a sum that no participant computes; decryption is done from shares (§2.6).

**Decisions bound to this step.**

* The key set is **exactly the set of players dealt into this hand**. Per **D-005**, a
  disconnected or sitting-out seat is *not* a party to the cryptography: it is not in
  `apk`, receives no cards, and nothing waits for it. Its stack still pays blinds and
  antes and drains until it busts.
* **D-012: that set is the `dealt_in` of this hand's chained `HAND_INIT`, and it is
  read from nowhere else.** This is the load-bearing instance of D-012 in this
  document, because `apk = Σ pk_i` is appended to ziffle's Fiat–Shamir transcript
  under the label `"apk"` (§6.4), so the key set is a *hashed* quantity. If two honest
  peers computed membership differently they would derive different challenges, every
  shuffle proof in the hand would fail verification for at least one of them, and the
  table would fault. Membership therefore may not be derived from any quantity that
  can differ between honest receivers — in particular **not from the `attributed`
  field of a previous hand's abort**, which is per-receiver (`STATE_MACHINE.md` T46,
  finding H1) and which under **D-010** is evidence with no automatic consequence
  anyway. A seat leaves the key set because a chained event every participant accepted
  put it outside `dealt_in`, never because this peer's copy of an abort named it.
* **D-013: the pin is unchanged, and what it rests on is not.** `dealt_in` is still
  read from the chained `HAND_INIT` and from nowhere else, so nothing in this section
  needs an edit. But under D-013 `dealt_in` is constrained by the required emitter set
  of the *previous* hand, so this pin is now exactly as agreed as `HAND_INIT` is, and
  `DECISIONS.md` **K-1** holds that that set can differ between honest peers on the
  abort path. **That is `PROTOCOL.md` §3.2's to settle, not this document's**, and it
  is recorded rather than worked around: an implementer must not "harden" the key set
  by deriving membership here from anything local, because a second derivation is how
  the two peers stop even failing in the same way. The full note, including why this
  layer's loud failure never actually fires on that path, is §15 note 5's D-013 item.
* Keys are **fresh per hand**, not long-lived. `ctx` (`PROTOCOL.md` §4.5) contains
  `hand_id`, so a
  key generated for hand `k` cannot have its ownership proof replayed into hand `k+1`.
  Fresh keys also mean that compromising a player's machine after a hand does not
  retroactively open that hand's transcript.
* This key pair is **not** the player's identity. §10 keeps the three identities
  separate as `SPEC_CS.md` §20 requires: the libp2p `PeerId` key, the long-term
  application Ed25519 signing key, and this ephemeral per-hand deck key.

### 2.2 Deck initialisation

The 52 plaintext "cards" are 52 fixed, publicly known curve points, identical in every
client:

```
open_deck[i] = s_i · G,   s_i drawn from StdRng seeded with SHA-256(b"CARDS-V1")
```

**Verification: (b) source** — `ziffle-0.1.0/src/lib.rs`, `fn open_deck<const N>()`.

These points are constants of the protocol, not secrets. `s_i` is recomputable by
anyone, which is harmless: ElGamal hides the message under DDH regardless of whether
the message's discrete logarithm is public, and every player must know the 52 points
anyway in order to perform the final table lookup at reveal time (§2.6).

The **card ordering** — which of the 52 points means `2♣`, which means `A♠` — is a
constant of *our* protocol, fixed once in `PROTOCOL.md` as the identity mapping
`index i → rank/suit i` over ziffle's `open_deck`, and covered by
`protocol_version`. It is not randomised and must never be.

The initial deck presented to the first shuffler is the trivial encryption

```
D_0[i] = (O, open_deck[i])          // O = point at infinity, r = 0
```

**Verification: (b) source** — `fn initial_deck(&self)`:
`array::from_fn(|i| (CurveAffine::identity(), self.open_deck[i]))`.

So `D_0` is public and identical for everybody. Nothing is hidden yet; hiding begins
with the first shuffle.

### 2.3 The shuffle chain

Players shuffle **in turn**, in a deterministic order fixed by the protocol
(`PROTOCOL.md`: seat order starting left of the button, so it is derivable by every
peer from state that is already agreed). Player `k` takes the previous verified deck
`D_{k-1}` and produces `D_k`:

```
π  ←$ uniform permutation of {0..N-1}          (Fisher–Yates over the OS CSPRNG)
for i in 0..N:
    ρ_i  ←$ F
    D_k[i] = ( D_{k-1}[π(i)].c1 + ρ_i · G ,
               D_{k-1}[π(i)].c2 + ρ_i · apk )
```

That is exactly the statement in ziffle's own doc comment, which follows the paper's
notation:

> `𝒞'[i] = 𝒞[𝜋(i)] · ℰ(1; ρ(i))` for all `i` in `[0; N-1]`

**Verification: (b) source** — `fn shuffle_remask_prove` and `fn remask_card`
(`(c1, c2) <- (c1 + r·G, c2 + r·pk)`), plus the `ShuffleProof` doc comment.

Two facts make this sound as a *joint* shuffle:

1. **Re-masking is what breaks linkability.** Permuting alone would be useless — the
   ciphertext bytes would be recognisable. Adding a fresh encryption of the identity
   element re-randomises each ciphertext, and under DDH in secp256k1 the output deck
   is computationally unlinkable to the input deck for anyone who does not know `ρ`.
2. **The composition is random if *any one* player is honest.** The final order is
   `π_n ∘ … ∘ π_1`. A single honest, uniformly chosen `π_i` makes the composition
   uniform, whatever the other `n−1` players do. This is the structural answer to
   `SPEC_CS.md` §7 and is discussed further in §7.

Along with `D_k` the shuffler emits a `ShuffleProof<52>` (§6). **Every** other player
verifies it. A proof that fails is `INVALID_SHUFFLE_PROOF`: the hand stops, and the
shuffler is named as the source of the protocol failure, per `SPEC_CS.md` §8. There is
no "continue anyway" path.

The stop is a **neutral abort** (D-010): stacks return to their start-of-hand values
and nothing is taken from the shuffler. Naming it is a record, not a sanction. This
costs nothing here, because a bad shuffle proof is caught *before* any card is opened
and before the pot has grown — the shuffle chain runs ahead of the first betting round
(§2.9) — so there is no committed money for a forfeiture rule to redistribute anyway.

```rust
// illustrative, from probe-cryptodoc — the whole chain for two players
let (d1, p1) = shuffle.shuffle_initial_deck(&mut rng, apk, ctx);
let v1 = shuffle.verify_initial_shuffle(apk, d1, p1, ctx).expect("shuffle 1");
let (d2, p2) = shuffle.shuffle_deck(&mut rng, apk, &v1, ctx);
let v2 = shuffle.verify_shuffle(apk, &v1, d2, p2, ctx).expect("shuffle 2");
```

**Verification: (a) compiled and ran** — `probe-cryptodoc`, release build, output in
§13.

`verify_shuffle` returns `Option<Verified<MaskedDeck<N>>>`, and only a
`Verified<MaskedDeck<N>>` exposes `get(idx) -> Option<MaskedCard>`. A deck that has
not been verified therefore cannot have a card taken out of it — a type error, not a
review convention. This directly implements `SPEC_CS.md` §22's "never display a
cryptographically unverified card".
**Verification: (b) source** — `impl<const N: usize> Verified<MaskedDeck<N>> { pub fn
get(&self, idx: usize) -> Option<MaskedCard> }`; `MaskedDeck<N>` has
`CanonicalDeserialize` but `Verified<MaskedDeck<N>>` does not.

### 2.4 The dealing map is fixed **before** the shuffle chain starts

This is our layer's responsibility and it is load-bearing. Deck index → destination is
fixed deterministically from `(table_id, hand_id, button, seat order)` and recorded in
the `HAND_INIT` stage *before* the first shuffle.

**The canonical deck-index map is `PROTOCOL.md` §4.5's, and is not reproduced here**
(D-011 rule 1). That section states the map, its `index_map_hash`, and the symbol `m`
for the dealt-in count; a table that used to stand here has been deleted rather than
kept in sync, because a copy is what drifts. Note only that where this document writes
`n` for the number of parties to the cryptography (§2.1) it is the same count as
`PROTOCOL.md` §4.5's `m` — the two symbols name one set, the dealt-in seats — and that
at most 25 of the 52 indices are ever used, which is the fact §2.5 and §2.7 rely on.

**There are no burn cards, and that is `PROTOCOL.md` §4.5's rule**, recorded as entry 3
of the deviation register in `THREAT_MODEL.md` §9.1.1. The cryptographic reason not to
restate it here is the same reason it is not a free per-document choice: a burn
consumes a deck index, so it changes the map, so it changes `index_map_hash` inside
`DECK_COMMIT` — and two conforming clients with different maps would mismatch every
hand.

**What this document does own about the map** is the argument for its *timing*, which
is cryptographic rather than a layout: the map must be fixed before any shuffle
happens, and everything in the rest of this section is that argument.

If the map were chosen *after* the final deck existed, the last shuffler — who chose
the last permutation — would gain a lever. Fixing it first removes the degree of
freedom entirely. There is no randomness in the map, so there is nothing to
manipulate.

### 2.5 Why the last shuffler gains nothing

The classic worry is that whoever shuffles last has the final say over the order. It
holds no advantage here, and the argument is worth stating because it is the reason no
extra beacon is needed for the deck:

The last shuffler sees only ElGamal ciphertexts under `apk`. It holds one of `n`
secret shares, so it can decrypt nothing (§2.6, and the measured `None` in §13). Under
DDH every ciphertext is indistinguishable from every other, so although it may choose
*any* permutation it likes, it has no information about which slot holds which card
and therefore nothing to bias toward. Grinding permutations buys it nothing.

This is an argument from DDH plus the n-of-n property, **(d) literature** for the
former and **(a) compiled** for the latter (one share alone returns `None`).

### 2.6 Opening a card: decryption shares and their DLEQ proofs

To open the card at deck index `j`, let `c = (c1, c2) = D_n[j]`. Each player `i`
publishes a **reveal token** (a decryption share)

```
share_i = sk_i · c1
```

together with a Chaum–Pedersen proof that the same `sk_i` relates `pk_i` to `G` and
`share_i` to `c1` — i.e. `log_G(pk_i) = log_{c1}(share_i)`:

```
w    ←$ F
t_G  = w · G
t_c1 = w · c1
e    = H_dleq(ctx, pk_i, share_i, c1, t_G, t_c1)   // DST "ziffle/DLEQ/v1"
z    = w − e · sk_i
proof = (t_G, t_c1, z)                              // 98 bytes

verify:  t_G  == z · G  + e · pk_i
         t_c1 == z · c1 + e · share_i
```

**Verification: (b) source** — `ziffle-0.1.0/src/lib.rs`, `RevealTokenProof::new` and
`RevealTokenProof::verify`; the two verifier equations are transcribed verbatim from
the `if self.t_g != …` and `if self.t_c1 != …` guards.
**(a) compiled** — `probe-cryptodoc`, both DLEQ proofs verify.

Given **all** `n` verified tokens, the plaintext point is recovered and matched
against the open deck:

```
m = c2 − Σ_i share_i = c2 − (Σ_i sk_i) · c1
index = position of m in open_deck        // Some(idx) or None
```

**Verification: (b) source** — `Shuffle::reveal_card`, whose comment is
`m = c2 − Σ[sk(i)·c1]`, ending in `self.open_deck.iter().position(|&oc| oc == pt)`.

The lookup returning `Option<usize>` rather than a card is the last line of defence:
if the tokens are wrong or incomplete, the result is `None`, never a wrong card that
could be displayed as real.

**The n-of-n property, measured:**

```
[2] one share alone opens the card: None
[3] n-of-n shares open the card:    Some(24)
```

**Verification: (a) compiled and ran** — `probe-cryptodoc`. This is the empirical form
of `SPEC_CS.md` §9 and §35: a proper subset of the players cannot open a card.

### 2.7 Hole cards — selective opening

Alice's hole cards are the deck indices assigned to her seat by the §2.4 map. Tokens
for them are **broadcast**, not sent point-to-point: `DEAL_PRIVATE` is a collective
broadcast stage (`PROTOCOL.md` §3.4 and §4.6, `STATE_MACHINE.md` §7.8). Each dealt-in
seat emits one message carrying its token for every **other** dealt-in seat's two hole
indices; it emits no token for its own.

```
each i:  broadcast { (share_i(j), dleq_i(j)) : j ∈ hole_indices(s), s ≠ i }
alice:   verifies every DLEQ, adds share_alice(j) for her own indices,
         then looks the recovered plaintext point up in open_deck (§2.6)
```

**The counting argument — this, and not delivery secrecy, is why it is safe:**

> Opening deck index `i` requires a reveal token from **every one of the `n`
> dealt-in seats**. For a hole index owned by seat `P`, every seat except `P`
> broadcasts its token, so at most `n-1` tokens for that index ever exist
> publicly. Any other seat `Q` holds only its own token, which is already among
> the `n-1`; the one missing is `P`'s, and producing it is equivalent to
> computing `sk_P` from `pk_P`. `P` completes the set with its own share and
> reads its card. At showdown `P` publishes its own token, completing the set for
> everyone — the only moment a seat ever publishes a token for its own card, and
> any earlier such token is a protocol violation.

Consequences:

* Alice needs `n−1` tokens plus her own; the broadcast supplies the `n−1` and she holds
  the last one, so she reads her cards.
* **A modified client does not help.** Bob's client, however modified, is short exactly
  one share — Alice's — for every one of Alice's indices, and no quantity of public
  material substitutes for it. This is the property `SPEC_CS.md` §9 demands and the one
  the adversarial test `CheaterReadOpponentCard` (§25) must assert. Note that the
  earlier form of this argument in this document — *"Bob receives no token for Alice's
  indices from anybody"* — was **false** under the shipped broadcast design and has
  been deleted; the counting argument above is the correct one and is the basis of
  `THREAT_MODEL.md` G1.

**Threat to guard at our layer:** ziffle has no notion of *when* or *for which index* a
token is legitimate. A player who publishes a token for its own hole index before
showdown is doing something ziffle considers perfectly valid. Our envelope therefore
binds each token to `(table_id, hand_id, street, card_index)` and the rule is:

* a token is legal **only for the indices due at the current stage**; and
* a token from `P` for `P`'s own hole index before `SHOWDOWN_REVEAL` is the specific
  violation to reject and attribute to `P`.

Any token failing either test is rejected and its sender attributed. See §8 rule 3 and
§8 rule 4. **Rejection is the whole enforcement.** An illegitimate token never reaches
the aggregation step, so it cannot open a card it should not open; that is the property
that matters and it is complete on its own. The attribution that follows is a signed
line in the transcript and nothing more — under **D-010** it moves no chips and removes
no player.

### 2.8 The board, street by street

Board indices are known from §2.4 but their ciphertexts are opaque. At each street the
players broadcast tokens for **only that street's indices**:

| Event | Indices opened | Who publishes |
|---|---|---|
| `FLOP_REVEAL` | the 3 flop indices | every player still holding a key share |
| `TURN_REVEAL` | the 1 turn index | " |
| `RIVER_REVEAL` | the 1 river index | " |
| `SHOWDOWN_REVEAL` | the hole indices of players who must show | " |

Before its street, a board card is an ElGamal ciphertext under `apk` and no proper
subset of players can open it (§2.6). This is `SPEC_CS.md` §10, discharged by
withholding, not by a UI flag.

**Again, the gating is ours, not the library's.** ziffle will happily produce a valid
token for the river index during the pre-flop betting round. Our rules:

1. A token whose `card_index` is not due at the current street is **rejected** and its
   sender attributed. It is a detectable protocol violation, exactly like an invalid
   shuffle proof. Rejecting it is what protects the card; the attribution is a record
   with no automatic consequence (D-010).
2. Publishing a token is an **automatic client action**, never a human decision. This
   is the load-bearing observation behind **D-006**: a player who walks away from the
   keyboard still cooperates cryptographically, so the board opens on schedule, the
   showdown works, and the hand plays to the end. The only thing missing is a betting
   decision, and the answer to that is an auto check/fold **emitted by that player's
   own client** — an ordinary signed action by the seat that owed one, single-writer,
   needing no vote, no voter set and no shared clock (D-015). It is never an abort and
   never anything that ends the tournament. **The sentence that stood here said the
   auto check/fold came *via a timeout certificate* when `|V| >= 2`, and was advisory
   below that; no certificate is produced in version 1** (`PROTOCOL.md`'s header box),
   so the `|V|` branch is gone and the same thing happens at every table size. The
   observation this item rests on is unchanged and is now doing more work than before:
   the client that publishes tokens automatically is the same client that folds
   automatically, which is why removing the certificate strands nobody. See §2.10. An auto check/fold is **not** a penalty and D-010 does not remove
   it: it is the ordinary poker treatment of a missed decision, identical to the live
   rule, and it moves chips only as normal play does — never as a sanction, never
   beyond what the player had already committed.
3. **There are no burn cards.** Indices `2m+5 … 51` are never opened and a reveal token
   for any of them is a protocol violation, rejected, and attributed to its sender as a
   record only.

### 2.9 The complete per-hand sequence

```
HAND_INIT            table_id, hand_id, button, seat order, dealing map, ctx
                     collective: every present seat signs a byte-identical body   (n copies)
  ↓
KEY_SETUP            each player: (sk_i, pk_i, OwnershipProof) ; verify all       (65 B each)
                     apk = Σ pk_i                                                  (33 B each)
  ↓
SHUFFLE_STEP × n     player k: D_k + ShuffleProof<52>, everyone verifies       (5547 + 3432 B)
  ↓   any proof fails → INVALID_SHUFFLE_PROOF, hand stops NEUTRALLY (stacks restored,
  ↓                     D-010), shuffler named in the transcript as a record only
DECK_COMMIT          hash of the final verified deck enters the transcript
  ↓
DEAL_PRIVATE         broadcast: each dealt-in seat emits one message carrying its token
                     for every other dealt-in seat's two hole indices — 2(n−1) tokens
                     per sender, one message per sender, one collective stage
                                                                        (131 B per token)
  ↓
betting …            auto check/fold emitted by the away seat's OWN client when its own
                     timer expires: an ordinary ACTION_CHECK / ACTION_FOLD, single-writer,
                     no vote and no certificate at any table size (D-015; was "on a
                     timeout certificate when |V| >= 2", D-006 to D-008)
FLOP_REVEAL          tokens for 3 indices, broadcast
betting … TURN_REVEAL … betting … RIVER_REVEAL … betting …
  ↓
SHOWDOWN_REVEAL      tokens for the hole indices that must be shown
  ↓
HAND_COMPLETE        winner, pot, side pots; transcript closed
                     collective, like HAND_INIT                                   (n copies)
```

`HAND_INIT`, `HAND_COMPLETE` and `HAND_ABORT` are **collective** stages, not
single-writer events: every field is derived from the state before the stage, so every
present seat computes the same body, signs its own copy and emits it, and there is no
writer holding a veto over the hand-to-hand transition (`PROTOCOL.md` §3.2 and §4.4,
`STATE_MACHINE.md` §3.4). One signed event in the sequence above therefore costs `n`
copies of a small message at each of those three stages.

`DEAL_PRIVATE` costs `2n(n−1)` tokens per hand in total — `2(n−1)` from each of `n`
senders — at 131 B per token: 262 B per sender heads-up, 1 310 B per sender six-handed.
Two consequences that only broadcast delivery gives us: a showdown is **one message per
revealing seat** rather than a fan-out, and **mucking is a policy question rather than
a cryptographic one**, because the tokens a muck withholds are the revealing seat's own
and nobody else's.

Every one of these is a signed, hash-chained event per `SPEC_CS.md` §12 and §13. The
cryptographic objects (proofs, tokens, decks) travel **inside** the signed CBOR
envelope; they are never trusted on their own.

**What must never enter the transcript:** any `sk_i`, and any token for a card that
was not legitimately opened. The transcript must be publishable after the hand without
revealing a mucked hand — §13 of the spec says so explicitly. Tokens for opened cards
*are* in the transcript, and that is fine, because those cards are public by then.

### 2.10 Disconnect: what the cryptography can and cannot do

Barnett–Smart with an n-of-n key has a hard, structural consequence: **any dealt-in
player who stops publishing tokens makes it impossible for anyone to open any further
card.** Not the board, not the showdown. This is inherent, not a defect of ziffle.

Per **D-005**, **D-010** and `SPEC_CS.md` §19:

* If the hand can still be decided without opening anything — everyone else folds to
  one player — it completes normally. No tokens are needed. This is the one case where
  a player who quits to escape a loss does not escape, and it is worth noting that it
  costs no machinery at all: the others simply fold, and ordinary poker finishes the
  hand.
* Otherwise the hand **aborts, and the abort is neutral.** Stacks are restored to their
  start-of-hand values. **No chips move on an abort, in any direction, for any cause**
  (D-010 point 1). The transcript still records which peer stopped publishing, and that
  record is still signed and verifiable — but it is **evidence with no automatic
  consequence** (D-010 point 2). Nothing block-lists, unseats or penalises the named
  peer (D-010 point 3).
* **This is the same outcome whatever `|V|` is, and since D-015 `|V|` selects nothing
  at all.** The abort is neutral at every `|V|`, so `|V|` never selected between two
  chip outcomes; what it used to select was whether a peer got **named**, and
  **nothing names anybody now**. No `TIMEOUT_CERT` is produced (`PROTOCOL.md`'s header
  box, §4.8), so no certificate can form at any `|V|`, and **every abort of this
  version that follows a stalled cryptographic stage carries `attributed = []`**. The
  hand ends at `hand_deadline_ms` under `PROTOCOL.md` §8.4, on a local timer expiry
  every peer reaches from the same signed `HAND_INIT` and the same relative duration.
  Liveness is not owed in between: `SPEC_CS.md` §19 ranks security above finishing a
  hand conveniently.

  **The `|V|` branch is retained below as the specification a later version restores**,
  and the M1 discipline inside it is the part that must survive intact:
  * With `|V| >= 2` a complete, valid certificate could form and the abort carried the
    name of the peer that failed. That name was a record and nothing more (D-010).
  * With `|V| < 2` **the certificate is inert, and the two halves of that must not be
    run together into one sentence** (D-009 rule 2, finding M1). First: the certificate
    itself has *no effect at all* — it is not chained, it is not evidence, it produces
    no `AbortRecord`, it terminates nothing. Second, and separately: the hand ends
    later, at `hand_deadline_ms`, on a timer and not on anybody's certificate. **D-015
    generalises the second half to every `|V|` and makes the first half moot**, which
    is why the paragraph above states the timer path without a condition on it.

  The reason a below-floor certificate stays inert even though it can no longer take
  anything is unchanged and is not about chips: it rests on one peer's unilateral
  assertion that a deadline passed — or, at `|V| = 0`, on nobody at all — and no peer
  can check that assertion, because there is no trusted clock and no third party. A
  claim nobody can check does not belong in the chain whether or not it pays.
* **The scope is `|V|`, never the seat count `n` (D-008) — and under D-015 no live rule
  in this corpus is scoped on either, which is why the rule survives only as a bar on
  future edits.** Heads-up is the case where
  `|V| = 1` always — the voter set is the one opponent — but it is not the only one.
  `V` is the dealt-in seats minus the subject minus every seat a *completed, valid*
  certificate has already named, so at a larger table enough completed attributions
  reduce `|V|` to one and the same rule applies there. Being voted against is not
  exclusion, which is what stops `V` being collapsed by assertion. Any rule in this
  corpus still written on `n` is a defect. D-010 shrinks what this scoping protects —
  a collapsed `V` no longer wins anybody's chips — but it does not retire it: a
  manufactured certificate could still put a false name in the transcript, and the
  floor is what stops that. **D-015 retires it in this version by removing the
  message**, and the floor is kept in `PROTOCOL.md` §8.3 for the version that brings
  the message back. A rule written on `n` remains a defect even now that nothing is
  written on `|V|` either.
* From the **next** hand a seat that is outside that hand's `dealt_in` is simply not in
  `apk` (§2.1), and nothing waits for it. **D-012: `dealt_in` is settled by the next
  hand's chained `HAND_INIT`, never derived from the `attributed` field of the abort
  that ended this one.** Two honest peers can hold different copies of the same abort —
  the certificate path carried a name, the hand-deadline path carries none, and there
  is no abort-versus-abort precedence rule (finding H1) — so `attributed` is a
  per-receiver quantity. **Under D-015 only the second path exists, so the two copies
  now agree by construction** — and the prohibition stands unchanged anyway, because
  a rule that is currently unfalsifiable is exactly the kind this corpus has twice
  found reintroduced by an editor who noticed it never fired. Reading a key-set membership from it would put a per-receiver
  quantity into `apk`, which is hashed into every shuffle challenge, and the next hand
  would simply never verify for somebody. The name in an abort is evidence and nothing
  else, which is what **D-010** already says it is.

**The upgrade we must refuse.** The textbook fix for this is `t`-of-`n` threshold
ElGamal with Feldman or Pedersen VSS, so a quorum can finish without the missing
player. We do **not** do this: with `t < n`, any `t` colluding players can decrypt
*every* hole card at the table. That is precisely the trade `SPEC_CS.md` §19 forbids
and it breaks the §35 main invariant. n-of-n stands; **abort neutrally, and name
nobody** — the clause that stood here, *"name the failing peer where `|V| >= 2`
allows a certificate to form"*, is withdrawn by D-015: no certificate forms at any
`|V|`, so every abort that follows a stalled cryptographic stage carries
`attributed = []`. What the transcript still shows is the stage that stalled and whose
token is missing from it, unsigned and legible to anyone holding it.

**The cost, stated plainly, because D-010 requires it and `SPEC_CS.md` §18 requires
it.** **The rage-quit escape is back, at every table size.** A player who is losing a
big pot can stop publishing tokens, the hand aborts, and their chips come back. D-005
closed that with forfeiture; D-010 reopens it knowingly, because forfeiture is the
prize that made four rounds of attacks worth mounting, and every one of those attacks
ended in an *honest* peer's chips being taken. An exploit that lets a dishonest player
escape a loss is worse for fairness and better for safety than one that robs an honest
player, and that is the trade being made. It is a real regression, it is not solved,
and no sentence in this corpus may imply otherwise.

Recorded honestly: **a malicious player can always force a hand to abort by going
silent, and always recovers its own commitment when it does.** It cannot steal cards —
that property is cryptographic and is untouched — and it cannot take another player's
chips either, because nothing moves. What it gets is a free exit from a losing pot and
a line in the transcript. The mitigations are social and not cryptographic: the
**missing contribution** is visible to everyone and permanent in the transcript — the
clause *"the attribution is visible to everyone and permanent where a certificate
formed"* is withdrawn, because no certificate forms (D-015) and the visibility is now
a gap in a stage rather than a signed name — the client should show a per-identity
abort count in the lobby, and sitting a repeat aborter out is a **user** decision,
never a protocol action (D-010).

---

## 3. Constructions considered and rejected

Required by §29 so a reviewer can see this was a comparison, not a first hit. Each row
is expanded below. Full measurements are in `docs/research/MENTAL_POKER.md` §2–§5.

| Construction / implementation | Verdict | Reason |
|---|---|---|
| Barnett–Smart 2003 + Bayer–Groth 2012 | **chosen** | Best-analysed construction; satisfies every §6 property; two Rust implementations exist |
| Wei & Wang, *Fast Mental Poker* (ePrint 2009/439) | rejected | Detects misbehaviour only at the end of the hand, after cards are dealt — §8 requires the hand to stop at the bad proof; thinner analysis record; no maintained Rust implementation |
| Castellà-Roca et al. (~2003–2006) | rejected | Less follow-up scrutiny than Barnett–Smart, and no maintained implementation in any language — adopting it means writing the cryptography ourselves, which §6 and §36 forbid |
| Neff, *A Verifiable Secret Shuffle* (CCS 2001) | rejected | Strictly dominated: larger and slower than Bayer–Groth at equal soundness; Rust implementations are welded to e-voting systems |
| Groth–Lu shuffle | rejected | Pairing-based, forcing a pairing-friendly curve and a heavier dependency, for no gain at `N = 52` |
| Curdleproofs (Ethereum Foundation, 2022) | rejected | Tuned for large-`N` validator shuffles; crate is `0.0.1` (2022-09-14), pins arkworks **0.3.0** — a second, incompatible arkworks tree; solves only the shuffle half, leaving threshold decryption and DLEQ to us |
| SNARK shuffles (`zshuffle 0.1.2` / uzkge) | rejected | Groth16 needs a **trusted setup**, reintroducing exactly the trusted party this project exists to remove; GPL-3.0-only; and it depends on 13+ privately forked arkworks crates (`ark-*-zypher`), which makes review mean reviewing a forked arkworks — the opposite of §28 |
| Hand-rolled cut-and-choose sigma protocol | rejected | Measured: time-competitive (84.6 ms vs 100 ms to prove at 2⁻⁴⁰) but **195 KB at 2⁻⁴⁰ and 390 KB at 2⁻⁸⁰**, against Bayer–Groth's 5 547 B — 35–70× larger on the wire. At 6 players that is ≈2.3 MB of proof traffic per hand, hostile to GossipSub's 64 KiB cap and to the relay fallback of D-001. Also forbidden by §6/§36 as our own construction |
| SRA commutative encryption (`distributed-cards 0.5.2`, `sra-wasm 0.1.0`) | rejected | **No verifiable shuffle at all** — grepping `distributed-cards`' `src/` for `proof\|Proof\|zero.knowledge\|verify` returns zero hits. Fails §8 outright: a shuffler can substitute a card undetectably. Also LGPL-3.0 / GPL-3.0 |
| `mental-poker 0.1.0` (2022) | rejected | Contains **no cryptography**: zero hits for `curve\|elgamal\|shuffle_proof\|zero.knowledge\|Proof` in `src/`; its only game implementation is `src/game/trusting.rs`, the trusted-dealer model §5 forbids. Also does not compile on stable (`error[E0554]: #![feature] may not be used on the stable release channel`) and its nightly APIs have drifted |
| `pokerproof 0.1.0` | rejected | "Provably fair" is the online-casino commit-and-reveal pattern: it presumes an operator who deals and lets you audit them afterwards. That is the model §5 rejects. Not a mental-poker protocol |
| `barnett-smart-card-protocol` (geometryxyz/mental-poker) | **runner-up, not chosen** | Correct construction, more complete (full `O(√N)` Bayer–Groth), and measurably faster (58.0 ms prove / 27.2 ms verify at `m=2,n=26`). Blocked on three practical grounds: (1) not on crates.io, and its manifest points at `ssh://git@github.com/geometryresearch/proof-toolbox.git` — a **stale org name** whose SSH fetch fails outright here, buildable only by rewriting the manifest to `geometryxyz` and setting `CARGO_NET_GIT_FETCH_WITH_CLI=true`, which is not something to pin a security-critical build to (§28 requires reproducible pinning); (2) arkworks **0.3.0**, a second incompatible tree; (3) ~6 600 lines to review versus ziffle's 1 779 |

**Verification:** all rows **(a) compiled** or **(b) source** in
`docs/research/MENTAL_POKER.md` §4 — the build failures, the greps and the benchmark
numbers quoted above are that document's verbatim results.

**If ziffle fails review**, `barnett-smart-card-protocol` is the fallback, with all
three of its repositories vendored. §9's trait boundary exists to make that a
contained change. This is a live contingency, not a formality — see §11 risk 1.

---

## 4. The group, and why

**secp256k1.** Prime order, cofactor 1, no small-subgroup concerns, enormous
deployment and scrutiny.

**It is inherited, not independently chosen.** ziffle hardcodes the curve:

```rust
type Curve       = ark_secp256k1::Config;
type CurveAffine = ark_secp256k1::Affine;
type Scalar      = <Curve as CurveConfig>::ScalarField;
```

**Verification: (b) source** — `ziffle-0.1.0/src/lib.rs`, type aliases at the top of
the file. There is no generic parameter and no feature to change it.

On the merits, **ristretto255 would be the better group**, and the difference is
measured, not asserted:

| Operation (release, 2000 iterations, sink-accumulated) | ristretto255 | k256 (secp256k1) | ark-secp256k1 |
|---|---:|---:|---:|
| variable-base scalar mul | **28.88 µs** | 56.04 µs | 98.90 µs |
| fixed/generator-base scalar mul | **14.82 µs** | 51.06 µs | — |
| point addition | **0.209 µs** | 0.269 µs | 0.479 µs |
| MSM of 52, constant time | 614.23 µs | — | — |
| MSM of 52, variable time | 422.23 µs | — | — |

**Verification: (a) compiled and ran** — `probe-mentalpoker`
(`docs/research/MENTAL_POKER.md` §3.3).

So `ark-secp256k1` is **3.4× slower than ristretto255** on the operation that dominates
a shuffle. ristretto255 also has the stronger assurance record: `curve25519-dalek`'s
README states that the 280 functions of version 4.1.3 reachable from Signal Messenger
have been formally verified with Verus (certificate `verilib.org/cert/5132`), and the
optional `fiat` backend uses formally verified field arithmetic from MIT Fiat-Crypto.
**Verification: (c)** — fetched
`raw.githubusercontent.com/dalek-cryptography/curve25519-dalek/main/curve25519-dalek/README.md`
(`docs/research/CRYPTO_LIBS.md` §2.2).

**The decision, and why we accept the slower group.** The absolute cost that matters
is ~100 ms to prove and ~42 ms to verify one 52-card shuffle (§6.4). That is fast
enough by a wide margin, and it does not justify forking a cryptographic library to
change its curve — forking would put us squarely in the territory §6 and §36 tell us
to stay out of. Recorded for later: **a ristretto255 port of ziffle is the single
highest-leverage optimisation available**, plausibly ~3×, if performance ever becomes
a real constraint.

**Consequence for the rest of the stack.** The general dependency set
(`CRYPTO_LIBS.md`) chose ristretto255 via `curve25519-dalek 5.0.0`. Both curves are now
in the build: ristretto255 through Ed25519 signing, secp256k1 through ziffle. They
never meet — different layers, different objects — but both appear in the dependency
register and both must be audited.

**Verification of the ElGamal shape over the group** (that additive ElGamal with
aggregate keys, re-randomisation and share decryption is exactly what the group
supports): (a) compiled and ran — `probe-mentalpoker` did this over ristretto255
before the group was settled, and `probe-cryptodoc` does it over secp256k1 through
ziffle's API.

---

## 5. Layer map: our composition vs library code

`SPEC_CS.md` §6 forbids writing our own cipher, hash, RNG, zero-knowledge system or
elliptic-curve construction. Every row in the "ours" column below must therefore be
*composition only*: gluing verified primitives together, applying policy, or
serialising. None of it invents a primitive.

### 5.1 Library code — we write none of this

| Object | Crate | What it is |
|---|---|---|
| ElGamal masking and re-masking | `ziffle` | `remask_card`, `shuffle_remask_prove` |
| Bayer–Groth shuffle argument | `ziffle` | `ShuffleProof<N>`, `MultiExpArg<N>`, `SingleValueProductArg<N>` |
| Schnorr key-ownership proof | `ziffle` | `OwnershipProof` |
| Chaum–Pedersen DLEQ | `ziffle` | `RevealTokenProof` |
| Aggregate key / aggregate token | `ziffle` | `AggregatePublicKey`, `AggregateRevealToken` |
| Pedersen commitments and their generators | `ziffle` | `PedersonCommitKey` |
| secp256k1 field and group arithmetic | `ark-ff`, `ark-ec`, `ark-secp256k1` 0.5.0 | — |
| Canonical point/scalar serialisation | `ark-serialize` 0.5.0 | `CanonicalSerialize`/`CanonicalDeserialize` |
| SHA-256 and hash-to-field for ziffle's Fiat–Shamir | `sha2` 0.10.9, `ark-ff` `DefaultFieldHasher` | — |
| Ed25519 signing and verification | `ed25519-dalek` 3.0.0 | `verify_strict` only |
| BLAKE3 keyed hashing and `derive_key` | `blake3` 1.8.7 | our transcript/state hashes |
| Argon2id | `argon2` 0.6.0 | passphrase → KEK |
| XChaCha20-Poly1305 | `chacha20poly1305` 0.11.0 | profile AEAD |
| OS entropy | `getrandom` 0.4.3 | `fill`, `SysRng` |
| Constant-time comparison, zeroisation | `subtle` 2.6.1, `zeroize` 1.9.0 | — |
| Deterministic CBOR codec | `minicbor` 2.3.0 | arrays only |

### 5.2 Our composition — each item justified

| # | What we build | Why it is composition, not new cryptography |
|---|---|---|
| 1 | **The `ctx` string** fed to every ziffle proof (§6.4) | A domain-separation byte string. It changes no algebra; ziffle already hashes `ctx` into every challenge. Choosing what goes in it is protocol policy |
| 2 | **The OS-CSPRNG adapter** into ziffle's `R: Rng` bound (§7.2) | Byte forwarding. No generation, no seeding, no internal state: `fill_bytes` calls `getrandom::fill` and returns. It is strictly narrower than writing an RNG — it removes the possibility of one |
| 3 | **The RNG commit/reveal beacon** for seating and the initial button (§7.3) | A hash commitment, then a hash combine, both over BLAKE3's keyed mode through §2.8's constructor. **Both constructions are `PROTOCOL.md` §4.4's and are not reproduced here or in §7.3** (D-011 rule 1, `K-6`) — this row's claim is about the *composition*, which is what §5.2 is for: textbook commit-and-reveal over a library hash, with no bespoke primitive and no novel construction, so §6 and §36 are satisfied. §7.3 points 1 to 5 carry why each part of the binding is there. Not used for the deck — the shuffle chain handles that |
| 4 | **The deterministic deck index → seat/board map** (§2.4) | Pure bookkeeping over integers. No randomness, so nothing to attack |
| 5 | **Street gating and index entitlement checks** on reveal tokens (§2.7, §2.8) | Policy: *when*, and *for which index*, a legitimate library operation may be applied. It adds no primitive; it constrains one |
| 6 | **The signed, hash-chained event envelope** (§12/§13 of the spec) | Deterministic CBOR + Ed25519 + BLAKE3, all library primitives, composed in the standard way: length-prefixed, domain-separated, `previous_event_hash` chained. The signature prefix is `p2p-poker/v1/event`, defined byte-for-byte in `PROTOCOL.md` §13; this document does not restate it |
| 7 | **The `DeckCrypto` trait boundary** (§9) | A Rust trait. No cryptographic content at all; it exists so ziffle can be swapped |
| 8 | **Timeout certificates** (D-006) — **not produced in version 1 (D-015); this row is the retained specification** | `\|V\|`-of-`\|V\|` Ed25519 signatures — one from every member of the required voter set `V` (`PROTOCOL.md` §8.3) — over a canonical CBOR body naming seat, sequence and `previous_event_hash`. A multi-signature by concatenation, not an aggregate signature scheme — no new algebra. D-008's floor `\|V\| >= 2` is a protocol rule and not a cryptographic one: one signature is a perfectly valid multi-signature over one key, so nothing in this row rejects it and nothing here may be scoped on the seat count `n`. Under **D-010** a certificate may still be *produced* — it is how a human or a later version adjudicates — but consuming one never moves a chip and never removes a player. Whether it should still be produced at all in the MVP is `DECISIONS.md` OQ-F, and it is `PROTOCOL.md`'s to settle, not this document's; the cryptography is the same multi-signature either way |
| 9 | **The profile key-slot file format** (§10) | An envelope around library AEAD and library KDF. The AEAD's associated data binds the header, so no slot can be stripped or swapped |

**Nothing in this table defines a cipher, a hash function, an RNG, a zero-knowledge
system, or a curve.** If a future requirement seems to need one, §36 applies: stop,
document the gap, find a published construction, and only then implement.

---

## 6. The shuffle argument

### 6.1 What is proven

For public inputs `apk`, the input deck `𝒞 = D_{k-1}` and the output deck
`𝒞' = D_k`, the prover demonstrates knowledge of a permutation `π` of `{0..N-1}` and
randomness `ρ ∈ F^N` such that

```
𝒞'[i] = 𝒞[π(i)] · ℰ_apk(1; ρ_i)      for all i ∈ [0, N-1]
```

in zero knowledge — `π` and `ρ` are not revealed. Written out, `𝒞'[i].c1 =
𝒞[π(i)].c1 + ρ_i·G` and `𝒞'[i].c2 = 𝒞[π(i)].c2 + ρ_i·apk`.

**Verification: (b) source** — this is the statement in `ziffle-0.1.0/src/lib.rs`'s
`ShuffleProof` doc comment, using the paper's own symbols.

Because `π` must be a permutation of the whole index set, a shuffler cannot drop a
card, duplicate a card, or substitute a card. That is `SPEC_CS.md` §8's requirement
("remove 2c, add a second As must be cryptographically detectable"), and it is
exactly what the *product* half of the argument enforces. **It is also D-014's tier-1
clause about a deck that gains or loses a card — the same verdict and not a second
check (§8.1.4) — and §8.1.2 names the three public inputs a failure is only evidence
against its signer *given*.**

**Measured, not assumed:** the research suite serialised a valid deck, overwrote card
5's bytes with card 3's, re-deserialised and presented the honest proof — rejected
(`T4 duplicated-card deck with honest proof rejected: true`). Opening all 52 cards
after two chained shuffles found each card exactly once (`T8`).
**Verification: (a) compiled and ran** — `probe-ziffle/src/bin/adv.rs`.

### 6.2 The construction

ziffle implements Bayer–Groth 2012 with the matrix parameter `m = 1`, so the deck is
one row of `N` ciphertexts rather than an `m × n` matrix. Its own comment states this:

> NOTE: in the paper, N is split into m rows for proof-size optimization, however for
> simplicity we construct the proof over 1 row of N ciphertexts, i.e. we set m = 1.

and

> NOTE: As we set m = 1, only the Single Value Product Argument protocol (Section 5.3)
> is required.

This trades the paper's `O(√N)` proof size for a linear one. At `N = 52` the result is
5 547 bytes, which is small enough that the simplification is the right engineering
call — and it removes a large amount of code from the surface a reviewer must check.

The proof object is

```rust
pub struct ShuffleProof<const N: usize> {
    c_pi:     PedersonCommitment,       // commitment to π (as 1-based scalars)
    c_xpi:    PedersonCommitment,       // commitment to [x^{π(i)+1}]
    mexp_arg: MultiExpArg<N>,           // multi-exponentiation argument (BG12 §5)
    prod_arg: SingleValueProductArg<N>, // single-value product argument (BG12 §5.3)
}
```

The two halves do different jobs:

* the **multi-exponentiation argument** proves each output ciphertext is an input
  ciphertext re-masked with `ℰ(1; ρ_i)` — no card was replaced by something outside
  the input deck;
* the **single-value product argument** proves the committed vector really is a
  permutation, i.e. every index `0..N−1` appears exactly once — no card was dropped or
  duplicated.

Neither alone is sufficient; the security claim rests on both, bound to the same
challenges.

**Verification: (b) source** — `ziffle-0.1.0/src/lib.rs`, `ShuffleProof` definition and
`ShuffleProof::new` steps 1–6 with their comments.

**Pedersen generators.** `PedersonCommitKey::default()` derives `h` and `gs[i]`
deterministically from the hardcoded seeds `b"PEDERSON-H-V1"` and
`b"PEDERSON-VECTOR-G-V1"` via `StdRng::from_seed(Sha256::digest(seed))` and
`CurveProj::rand(&mut drng)`.

This is the classic place such a scheme breaks: if `CurveProj::rand` sampled a
*scalar* and multiplied `G`, then anyone could recompute that scalar from the public
seed, the Pedersen commitment would not be binding, and the whole argument would
collapse. It does not. arkworks samples a random **base-field x-coordinate**, solves
for `y`, rejects on failure, and multiplies by the cofactor (1 here):

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

So the generators have no known discrete-log relation to `G`, and binding holds under
the discrete-log assumption. This is a legitimate nothing-up-my-sleeve derivation.
**Verification: (b) source** — `ark-ec-0.5.0/src/models/short_weierstrass/group.rs`
lines 95–108, and `ziffle-0.1.0/src/lib.rs`
`impl<const N: usize> Default for PedersonCommitKey<N>`.

### 6.3 Soundness parameter, stated with its uncertainty

Bayer–Groth is a **computationally sound argument of knowledge**, not a statistically
sound proof. Its guarantees are:

* **Binding** of the Pedersen commitments rests on the hardness of discrete
  logarithms in secp256k1 — roughly a 128-bit security level for a ~256-bit prime-order
  group. This is where "computationally" bites: an adversary who could compute
  discrete logs could open a commitment two ways and forge a shuffle.
* **Hiding** of the permutation is information-theoretic for the Pedersen commitments;
  unlinkability of the decks rests on DDH in secp256k1.
* **Challenge space.** Challenges are field elements, so the challenge space is `F` of
  order `n ≈ 2^256`. They are derived by `DefaultFieldHasher<Sha256>` hash-to-field
  from the transcript, per-argument DSTs.
  **Verification: (b) source** — `Transcript::derive_challenge_scalars`.
* **Soundness error per proof.** The paper's analysis bounds the error of the
  interactive argument by a small polynomial in `N` over `|F|` — for `N = 52` and
  `|F| ≈ 2^256` that is on the order of `2^-248`, i.e. negligible.
  **Verification: (d) literature only.** This document has *not* re-derived the bound
  from the paper, and has *not* checked that ziffle's `m = 1` instantiation preserves
  it. That check is part of the review named in §11 risk 1.

**What this is not.** Unlike a cut-and-choose protocol, there is no repetition count
to tune — soundness is not `2^-t` for a chosen `t`. There is exactly one proof per
shuffle. This is one of the reasons cut-and-choose was rejected (§3): matching a 2⁻⁸⁰
soundness target there costs 390 KB, and even that is *weaker* than what the algebraic
argument gives for 5.5 KB.

**Fiat–Shamir caveat.** All of the above is soundness of the *interactive* argument.
Non-interactive soundness additionally requires the Fiat–Shamir transform to be
applied correctly — which is §6.4, and which is a live risk, not a formality.

### 6.4 Fiat–Shamir transcript discipline — what must be bound

This section is the one where a mistake silently destroys the proof system, so it is
stated exhaustively.

**What ziffle binds, verified in source.** The challenge `x` for a shuffle is derived
from a transcript initialised as `SHA-256(ctx)` and then extended with, in order:

| Appended | Label |
|---|---|
| the aggregate public key | `"apk"` |
| **the entire previous deck** (all `N` ciphertexts, index-prefixed) | `"prev"` |
| **the entire next deck** (all `N` ciphertexts, index-prefixed) | `"next"` |
| the commitment to the permutation | `"c_pi"` |

then `x = hash_to_field(transcript, DST = "ziffle/BG12X/v1")`, after which `c_xpi` is
appended and `(y, z) = hash_to_field(transcript, DST = "ziffle/BG12YZ/v1")`.

**Verification: (b) source** — `ShuffleProof::challenge_x` and `challenge_yz`. Each
`append` hashes `len(label) ‖ label ‖ len(bytes) ‖ bytes`, and `append_vec`
additionally hashes the element index — so the encoding is unambiguous and neither
labels nor values can be re-parsed at a different boundary.
**Verification: (b) source** — `Transcript::append`, `append_vec`,
`update_with_serialized`.

Because both decks are bound, a proof is welded to the exact transition it was made
for. The research suite confirmed this empirically: a valid chained proof presented
against the *wrong* previous deck is rejected (`T6b`).
**Verification: (a) compiled and ran** — `probe-ziffle/src/bin/adv.rs`.

**What ziffle does NOT bind — and therefore what `ctx` must carry.** The transcript
contains no protocol version, no table, no hand, no shuffle round and no shuffler
identity. All of that reaches the challenge only through the `ctx` byte string we
supply. **If `ctx` is weak, the proof system is weak**, and no amount of correctness
inside ziffle compensates.

**The construction is `PROTOCOL.md` §4.5's, and this document does not carry it**
(D-011 rule 1). Go there for the seven fields, their order, their widths, the `0xFF`
sentinel that marks a `ctx` which is not a shuffle-chain step, and the statement that
the separator is the length prefix and nothing else. The domain string
`"p2p-poker v1 deck-ctx"` and the constructor `h` are `PROTOCOL.md` §2.8's, likewise by
reference.

**Two copies of that block used to stand here, one after the other on a single
screen** — the reproduction, and then the same bytes again quoted from `PROTOCOL.md`
§4.5 under a paragraph asserting the two were byte-identical. That arrangement was the
pre-D-011 discipline at its most explicit: it needed a re-verification every pass, it
got one after D-010, and the pass before that it had already drifted once (§11 OQ-3
records the drift — a restatement that had lost the `[ … ]` brackets around
`u8(shuffle_round)`). **Both copies are deleted, and the byte-identity claim with
them**, because a claim that two copies agree is only worth making while two copies
exist. There is now one site, `PROTOCOL.md` §4.5, and nothing here to keep in sync
with it.

What this section keeps is the part `PROTOCOL.md` does not own and should not carry:
**why** `ctx` has to hold those fields at all, which is the argument immediately above
about what ziffle's transcript leaves unbound, and the per-field rationale below. If
`PROTOCOL.md` §4.5 ever gains or loses a field, that is a change to the construction
and this section's rationale is then incomplete — it is never authoritative against
§4.5.

Earlier drafts of this section carried a raw-concatenation form
(`"p2ppoker/v1" ‖ 0x00 ‖ … ‖ session_nonce ‖ …`). `PROTOCOL.md` §4.5 supersedes it,
that form is deleted, and it must not be reintroduced. The field name
`session_nonce` is retired with it: there is one name, `session_id`, defined by
`PROTOCOL.md` §4.3, and it denotes the same object `SPEC_CS.md` §14 and §20 call the
session nonce.

**The constructor `h` is `PROTOCOL.md` §2.8's**, and its Rust body — a `derive_key`
BLAKE3 key, then an 8-byte big-endian length prefix ahead of each part — was
reproduced here and has been deleted with the `ctx` block, for the same reason
(D-011 rule 1). The property this document relies on is the one that section
guarantees: length prefixing means no two distinct field tuples can produce the same
`ctx` bytes. Every domain string comes from that section's register, which is the
single register for the whole corpus; a document that invents one has a bug. Note that
`DOMAIN_EVENT = "p2p-poker/v1/event"` is **not** one of those strings — it is a
literal 24-byte signature prefix rather than a `derive_key` domain, which is why its
separators differ; its bytes are `PROTOCOL.md` §13's and were never restated here.

**Verification: (a) compiled and ran** — `probe-crypto-final` step `[5]` asserts that
the constructor's length prefixing separates `["AB","C"]` from `["A","BC"]`, and that
two domains separate (`docs/research/CRYPTO_LIBS.md` §3.3).

Each field's role. **This table is rationale, not a definition** — the field list it is
keyed on is `PROTOCOL.md` §4.5's, and if the two ever disagree, §4.5 has the fields and
this table is stale:

| Field | Prevents |
|---|---|
| `protocol_version` | replay across incompatible protocol revisions |
| `table_id` | replay of a proof from another table |
| `session_id` | replay from an earlier session at the *same* `table_id` |
| `hand_id` | replay of last hand's shuffle into this hand |
| `sequence` | replay of a proof into a different stage slot of the same hand |
| `shuffle_round` | replaying shuffle 1's proof as shuffle 3's, and mixing a shuffle-step `ctx` with a non-shuffle one (`0xFF`) |
| `sender_public_key` | one player presenting another's proof as their own |

**Verified end-to-end, by us, for this document:** a shuffle proof produced under
`ctx` with `hand=7` and presented under `ctx` with `hand=8` is rejected.

```
[1] proof bound to ctx, replay into another hand_id rejected: true
```

**Verification: (a) compiled and ran** — `probe-cryptodoc` (§13). This must become a
permanent regression test in `tests/adversarial/`, not a one-off probe.

**Additional discipline, binding on the implementation:**

1. The **same** `ctx` must be used by prover and every verifier for a given step. It
   is derived from signed state, never sent as a free parameter alongside the proof —
   a `ctx` taken from the wire would let an attacker choose the domain. **This is also
   the D-012 obligation on `ctx`:** every one of `PROTOCOL.md` §4.5's fields must be
   either a protocol constant or a value fixed by a chained event every participant
   accepted, and none of them a local view. Adding a field that could differ between
   honest receivers would not merely be a D-012 violation in principle — it would make
   honest verifiers derive different challenges and reject each other's proofs.

   **The condition holds today. It did not hold when this item was written, and the
   record is owed here even though nothing is owed on the wire.** This item was added
   in the D-012 sweep and stated the condition as satisfied. It was not: `ctx` names
   `session_id`, `session_id` named `advert_hash`, and `advert_hash` was the
   `event_hash` of whichever copy of the founder's re-broadcast advertisement a joiner
   happened to hold — a per-receiver quantity of the purest kind, since the
   re-broadcast rule obliges every copy to differ. Two honest players joining thirty
   seconds apart derived different `ctx` values and would have rejected each other's
   shuffle proofs, and this item asserted the opposite. **D-013's J1 rule removed
   `advert_hash` from all three places** and `PROTOCOL.md` §4.5 records that its block
   needed no edit for the third, because `ctx` never named the field directly. With
   that removal all seven parts satisfy the condition and the item is true as written
   above.

   **Why this correction is worth its space rather than a silent edit.** The claim was
   not wrong about `ctx`'s own seven parts — each of those was, individually, exactly
   what it appeared to be. It was wrong because the property is **transitive** and the
   check was not: a part that is "fixed by a chained event" can still be derived from a
   local view one or two hops upstream, and no reader of §4.5's field list could see
   that. An editor who meets this item in future must therefore read it as an
   obligation that is **re-checked whenever any part's own definition changes**, not as
   a box that was ticked once. Concretely: if `session_id`, `table_id` or
   `protocol_version` ever gains a component, this item is open again until somebody
   walks the new component to its own source. The failure this closes is catalogued as
   `THREAT_MODEL.md` **X35**.
2. The DLEQ challenge binds `pk`, `share`, `c1`, `t_G`, `t_c1` and `ctx` — **but not
   the card index or the street**. A token is therefore bound to a ciphertext, not to
   a position in the game. Our signed envelope must carry
   `(table_id, hand_id, street, card_index)` and receivers must check them (§2.7,
   §2.8). **Verification: (b) source** — `RevealTokenProof::challenge` parameter list.
3. **Never verify a proof against a deck we did not verify ourselves.** The
   `Verified<T>` type makes this hard to get wrong: `Verified<MaskedDeck<N>>` has no
   `CanonicalDeserialize` impl and a private field, so it can only be produced by a
   local successful verification.
4. `ziffle`'s `Transcript` is a hand-rolled SHA-256 chain rather than a reviewed
   transcript library such as merlin/STROBE. It looks correct — length-prefixed labels
   and length-prefixed values — but it is bespoke, and it raises the stakes on risk 2
   in §11.
5. The transcript is deliberately **forked** between the two sub-arguments: both are
   derived from the same state, separated only by different DSTs
   (`ziffle/BG12MultiExpArgX/v1` vs `ziffle/BG12ProductArgX/v1`), and the author flags
   it in a comment: *"NOTE: The transcript is forked here."* Distinct domain-separation
   tags are the standard defence and this is probably fine, but transcript forking is
   exactly where weak-Fiat–Shamir bugs of the "Frozen Heart" class live. See §11
   risk 2. **Verification: (b) source** — `ShuffleProof::new`, steps 5 and 6.

### 6.5 Size and timing

Sizes, from `compressed_size()` on this machine:

| Object | Bytes |
|---|---:|
| `PublicKey` | 33 |
| `OwnershipProof` | 65 |
| `RevealToken` | 33 |
| `RevealTokenProof` | 98 |
| `MaskedDeck<52>` | 3 432 (66 B/card) |
| `ShuffleProof<52>` | 5 547 |

**Verification: (a) compiled and ran** — `probe-cryptodoc` printed
`PublicKey=33 OwnershipProof=65 RevealToken=33 RevealTokenProof=98
MaskedDeck<52>=3432`; the 5 547-byte `ShuffleProof<52>` was measured by an actual
serialise round-trip in `probe-ziffle` (`T5 proof wire round-trip verifies: true
(5547 bytes)`).

Timing, release build, single-threaded:

| Metric | 2 players | 3 players | 6 players |
|---|---:|---:|---:|
| keygen + ownership proofs, all players | 1.2 ms | 1.4 ms | 2.3 ms |
| shuffle **prove**, per shuffle | 94–111 ms | 97–105 ms | 94–100 ms |
| shuffle **verify**, per shuffle | 37–43 ms | 39–43 ms | 37–43 ms |
| reveal one card (all tokens + proofs + lookup) | 1.39 ms | 2.08 ms | 4.34 ms |
| **full hand, one node's own crypto work** | **~195 ms** | **~248 ms** | **~422 ms** |

**Verification: (a) compiled and ran** — `probe-ziffle/src/main.rs`
(`docs/research/MENTAL_POKER.md` §5.1).

Per-shuffle cost is flat in the player count — the deck is always 52 — so players
enter only through how many shuffles must be verified and how many tokens aggregated.

**Wall-clock hand startup.** The shuffle chain is inherently sequential: player `k+1`
cannot start until `k`'s output exists. Roughly

```
n × (prove 100 ms + one-way latency) + (n−1) × verify 42 ms
```

≈ **340 ms heads-up** and ≈ **1.1 s six-handed** at 100 ms RTT, excluding DHT and
GossipSub join. **These two figures are estimates at an assumed 100 ms RTT, not
measurements.** Wherever they appear in this corpus they must be marked as such;
`PROTOCOL.md` §3.2 and §4.4 carry the same marking. Phase 8 measures the real value.

The estimate must additionally be revised upward for the collective form of
`HAND_INIT` (§2.9): making `HAND_INIT` a collective stage replaces one message with
`n`, which adds one collective round trip — one further one-way latency plus the time
to hear `n−1` copies — ahead of the shuffle chain. The same applies at `HAND_COMPLETE`,
after the hand rather than before it. The figures above have **not** been re-derived
with that term included; they are a lower bound until Phase 8 measures.

**Two consequences that are requirements, not observations:**

1. **`SPEC_CS.md` §33: proving and verifying must run on a worker thread.** At ~100 ms
   per proof this would visibly freeze a GUI event loop. Results reach the UI thread as
   messages.
2. **Bandwidth interacts with D-001, per circuit and over both directions together.**
   A relay's `max_circuit_bytes` is a **per-circuit budget counted across both
   directions at once**, and a table is a full mesh, so one circuit connects exactly
   one pair of seats. Two errors have been made here and both are recorded rather than
   quietly overwritten. The first was comparing *table-wide* traffic against a
   per-circuit cap — *"so a public relay carries on the order of two six-handed hands
   before it resets"* — which is deleted. Its replacement called the cap "per circuit
   **and per direction**", and that is false too: `libp2p-relay 0.21.1` relays a
   circuit with a single `CopyFuture` that holds one `bytes_sent: u64`, and **both**
   `forward_data` calls — src→dst and dst→src — increment that same counter before it
   is compared against `max_circuit_bytes`. The default 131 072 B is therefore 128 KiB
   of **combined** traffic, not 128 KiB each way, and every figure derived from it
   halves.
   **Verification: (b) source** — `libp2p-relay-0.21.1/src/copy_future.rs`: the single
   `bytes_sent: u64` field on `CopyFuture` at line 47, the guard
   `if this.max_circuit_bytes > 0 && this.bytes_sent > this.max_circuit_bytes` at line
   78, and the two `forward_data` calls — src→dst at line 88, dst→src at line 98 — each
   followed by `this.bytes_sent += i` (lines 92 and 102) into that one counter. There
   is no second counter and no per-direction accounting anywhere in the file. The
   crate's own `quickcheck` property settles it beyond reading: `copy_future.rs:241`
   asserts `a.len() + b.len() > max_circuit_bytes as usize` — the two directions
   **summed** against one cap, in the test the maintainers wrote for this behaviour.
   A-8's own
   citations (`src/behaviour.rs`, `impl Default for Config`; `src/behaviour/handler.rs`)
   establish the default value and its delivery to each circuit; they do not reach the
   accounting. The recorded objection is `NETWORK_STACK.md` §16.1 and §15 note 3 below.

   The corrected figures, derived from this document's own measured proof sizes:

   | Quantity | Value | Basis |
   |---|---:|---|
   | `ShuffleProof<52>` + `MaskedDeck<52>`, one shuffler | 8 979 B | 5 547 + 3 432, measured — the size table at the head of this section |
   | Shuffle traffic over **one circuit**, **both directions together**, per hand | **17 958 B** | the circuit joins two seats and each of them shuffles once per hand, sending its own step and proof the other way: `2 × 8 979`. **Independent of `n`** |
   | Hands per circuit against a 131 072 B public-relay budget, shuffle only | **~7** | `131 072 / 17 958 = 7.30`; seven hands cost `7 × 17 958 = 125 706 B`, eight cost `143 664 B` and exceed the cap |
   | The same including the signed event stream | **~5 hands** | order-of-magnitude, **not measured**. The withdrawn per-direction estimate was ~10 hands, i.e. `131 072 / 10 ≈ 13 107 B` of traffic per seat per hand once the event stream is added to the 8 979 B of shuffle. The same allowance counted both ways gives `2 × 13 107 = 26 214 B` per circuit per hand and `131 072 / 26 214 = 5.0` hands. `THREAT_MODEL.md` OQ12 must measure this **bidirectionally**; measured one way it reports twice the headroom that exists |
   | A relayed peer's **total** per-hand outbound at an `n`-seat table | `(n-1) × 8 979 B` | 44 895 B at six seats — a bandwidth figure, spread over `n-1` separate circuits each with its own budget, never a single cap |
   | The binding public-relay limit | **`max_circuit_duration = 120 s`**, not the byte cap | a session lasts far longer than two minutes |

   So **duration, not bytes, is the binding public-relay limit**, and it stays binding
   at 7 hands as it was at the withdrawn 14: the byte cap would have to be reached
   inside 120 s, which means seven hands in two minutes, under 17 s per hand including
   every human betting decision. A public circuit expires long before that. That is
   still concrete evidence for D-002's split — public relays are fine as hole-punch
   rendezvous and cannot carry a session — but for the right reason, and with half the
   margin previously claimed.

   **The working, end to end, so the hands-per-circuit figure is never quoted without
   the measurement it stands on.** Every step comes from this document's own measured
   sizes — the table at the head of §6.5 — and one crate constant. Re-derived at the
   close of Phase 1 and unchanged:

   ```
   ShuffleProof<52>                    5 547 B   measured, probe-ziffle T5 round-trip
   MaskedDeck<52>                    + 3 432 B   measured, probe-cryptodoc [4]
                                     ---------
   one shuffler's step + proof         8 979 B

   a circuit joins exactly 2 seats; each shuffles once per hand and sends its own
   step + proof the other way, and libp2p-relay counts both directions into one
   bytes_sent:
   per circuit per hand              2 x 8 979 = 17 958 B     (independent of n)

   max_circuit_bytes default          1 << 17  = 131 072 B    behaviour.rs:163
   hands per circuit               131 072 / 17 958 = 7.298…  -> 7
   check the floor and the ceiling  7 x 17 958 = 125 706 B <= 131 072 B   fits
                                    8 x 17 958 = 143 664 B  > 131 072 B   does not

   with the signed event stream (order-of-magnitude, NOT measured):
   allowance per seat per hand         13 107 B  = the withdrawn 131 072 / 10
   per circuit per hand              2 x 13 107 = 26 214 B
   hands per circuit               131 072 / 26 214 = 5.0     -> 5
                                    5 x 26 214 = 131 070 B <= 131 072 B   fits, barely

   a peer's own per-hand outbound at n seats:  (n-1) x 8 979 B
                                    n = 6:    5 x 8 979 = 44 895 B
                                    spread over n-1 circuits, n-1 separate budgets
   ```

   Two properties of this derivation are the ones a later reader must not lose. The
   `2 ×` is the bidirectional cap, **not** two shufflers' work being double-counted:
   the same 8 979 B crosses the circuit once in each direction and both crossings hit
   one counter. And the `(n−1) × 8 979 B` row is a *bandwidth* figure that must never
   be compared against `max_circuit_bytes`, because it is spread across `n−1` circuits
   each with its own budget — comparing a table-wide or peer-wide total against a
   per-circuit cap is the original A-8 error, and it is the one that would come back.

   **Third-party relays: assume the stricter reading.** kubo's `docs/config.md`
   describes its equivalent `ConnectionDataLimit` as applying "in each direction".
   Either the Go and the Rust implementations differ or that wording is loose; this has
   not been checked in the Go source. Our own relay is the Rust one, so the Rust
   accounting above is what binds us, and for a third-party relay of unknown
   implementation the bidirectional reading is the safe assumption because it is the
   smaller budget.

   The D-002 relay configuration itself is transport-local and is stated once, in
   `NETWORK_STACK.md` §9.6. Note also that a `ShuffleProof<52>` plus deck is 8 979
   bytes, comfortably under GossipSub's 64 KiB `max_transmit_size` — but hand traffic
   goes over direct libp2p streams to table participants, never over the lobby topic,
   per `SPEC_CS.md` §1.

**`SPEC_CS.md` §33's profiling targets — what is measured and what is not.**

§33 names seven quantities. Four are measured above; three are not, and saying so is
the point of this table. An estimate repeated as a bare number becomes a measurement by
attrition, so each unmeasured row names the phase that will measure it.

| §33 target | Status |
|---|---|
| shuffle generation | measured, 94–111 ms |
| shuffle verification | measured, 37–43 ms |
| private deal | measured, within the 1.39–4.34 ms per-card reveal |
| board reveal | measured, 1.39–4.34 ms per card |
| **signature verification** | **not measured** — Phase 4, one `verify_strict` per event, and the per-hand event count from `PROTOCOL.md` §3 |
| **network latency** | **not measured** — Phase 8, two-network test; `NETWORK_STACK.md` §12.12 confirms none has been run |
| **hand startup latency** | **estimated only** — ~340 ms heads-up, ~1.1 s six-handed at an assumed 100 ms RTT, and not yet revised for the collective `HAND_INIT` of §2.9. Phase 8 measures it. |

These are three measurement obligations, not open questions: nothing about the design
is undecided, only unmeasured.

---

## 7. Distributed randomness (`SPEC_CS.md` §7)

### 7.1 The deck: the shuffle chain *is* the distributed randomness

§7 requires that no single player decides the RNG for a hand. The shuffle chain
achieves this structurally, and it is worth being explicit that this is the real
mechanism rather than a bolt-on:

* every player contributes a secret permutation and fresh re-masking randomness from
  the OS CSPRNG;
* the final order is the composition `π_n ∘ … ∘ π_1`, uniform if **at least one**
  player is honest;
* a coalition of `n−1` cannot determine the order, because the honest player's
  permutation is uniform and independent;
* the last mover gains nothing (§2.5), because it cannot distinguish the ciphertexts
  it is permuting;
* **and a player cannot change its contribution after seeing the others'** — not
  because of a commitment, but because a shuffle is *published together with a proof
  bound to the exact `(prev, next)` pair*. Once `D_k` is out, changing it means
  producing a second valid proof for a different `next` against the same `prev` at the
  same `(hand_id, round)`, which is an equivocation: two conflicting signed events in
  one chained stage slot, detectable and attributable per `SPEC_CS.md` §14. The exact
  predicate is `PROTOCOL.md` §5.2's and is not restated here.

The adversarial test `CheaterPredictableRNG` and the required test *"a single
malicious player must not be able to choose the future board by manipulating the last
RNG/shuffle step"* (§25) target exactly this property.

### 7.2 OS CSPRNG only — and the spec-deviation note

`SPEC_CS.md` §7 says: *use the OS CSPRNG; use `OsRng`, never `SmallRng` nor a
self-seeded generator.*

**`OsRng` no longer exists.** `rand 0.10.2` has no `src/rngs/os.rs` and no `OsRng`
symbol; `rand_core 0.10.1` has neither the symbol nor an `os_rng` feature — asking for
that feature is a hard resolver error. The OS interface is now
`getrandom::SysRng` (re-exported as `rand::rngs::SysRng`), and the raw function
`getrandom::fill(&mut [u8])`.
**Verification: (b) source** — `rand-0.10.2/src/rngs/mod.rs:119`
(`pub use getrandom::{Error as SysError, SysRng};`) and `getrandom-0.4.3/src/sys_rng.rs`;
**(a) compiled** — the resolver error is real output
(`docs/research/CRYPTO_LIBS.md` §1).

> **Recorded spec deviation.** We implement §7's *requirement* (OS CSPRNG; never a
> self-seeded or non-cryptographic generator) under its current name. `OsRng` is not
> dropped, it is renamed. This note exists because `CRYPTO_LIBS.md` §11 item 5 asked
> for it to be recorded here so no later reader thinks the requirement was weakened.

**The adapter, and why it is composition (§5.2 item 2).** ziffle's API takes
`R: ark_std::rand::RngCore`, which is `rand_core 0.6`'s trait — a different, older
trait from the one `getrandom::SysRng` implements. We therefore need a shim. It
forwards bytes and does nothing else:

```rust
struct OsCsprng;

impl rand_core::RngCore for OsCsprng {          // rand_core 0.6, ark-std's version
    fn next_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        getrandom::fill(&mut b).expect("OS CSPRNG unavailable");
        u32::from_le_bytes(b)
    }
    fn next_u64(&mut self) -> u64 { /* same, 8 bytes */ }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        getrandom::fill(dest).expect("OS CSPRNG unavailable");
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> { … }
}
impl rand_core::CryptoRng for OsCsprng {}
```

**Verification: (a) compiled and ran** — `probe-cryptodoc`; this exact type drives
keygen, both shuffles and all reveal tokens in §13's output.

It holds no state and no seed, so there is nothing to predict and no generator to get
wrong. An OS entropy failure is a hard error, never a silently weaker key.

**This RNG feeds three secrets inside ziffle** — the permutation `π`, the re-masking
scalars `ρ`, and every proof's blinding scalar `w`. All three are drawn from the
caller's `rng`. The permutation is produced by rand 0.8's `SliceRandom::shuffle`,
which is Fisher–Yates/Durstenfeld (`for i in (1..len).rev() { swap(i, gen_index(rng,
i+1)) }`), so its uniformity is exactly the uniformity of the supplied RNG.
**Verification: (b) source** — `rand-0.8.8/src/seq/mod.rs` lines 586–592. Passing anything other than an OS
CSPRNG here would break the protocol completely, so the adapter is the single most
security-critical shim in the client.
**Verification: (b) source** — `fn shuffle_remask_prove` (`perm.as_mut_slice().shuffle(rng)`
then `remask_card(rng, …)`), `OwnershipProof::new`, `RevealTokenProof::new`.

**Finding that corrects `CRYPTO_LIBS.md` §1.2.** That document recommended dropping the
`rand` crate entirely so that `SmallRng`/`StdRng` could not be reached. **Adding
ziffle brings `rand` back**: `ark-std 0.5.0` depends on `rand 0.8` with feature
`std_rng`, so the tree contains `rand 0.8.8`, `rand_chacha 0.3.1` and `rand_core 0.6.4`.
**Verification: (a) compiled** — `cargo tree -i rand` in `probe-cryptodoc` shows
`rand v0.8.8 └── ark-std v0.5.0 └── … └── ziffle v0.1.0`; **(b) source** —
`ark-std-0.5.0/Cargo.toml` `[dependencies.rand] version = "0.8", features =
["std_rng"], default-features = false`.

**And it is not only ziffle.** In the integrated workspace `libp2p-autonat 0.15.0`
depends on `rand 0.8` directly, so `rand 0.8.8` would be in the tree even if the deck
library were removed, and `rand 0.9.5` and `rand 0.10.2` are there as well.
`docs/research/INTEGRATION.md` §2 is the authority on the integrated tree and records
all three majors; §9's register above reproduces the provenance. The recommendation
`CRYPTO_LIBS.md` §1.2 makes is therefore not merely unmet, it is **unreachable**: no
choice available to this project removes `rand` from the build.

One fact that was got wrong twice, one that is benign, and the action that replaces
both:

* **`SmallRng` *is* compiled in, and no choice open to this project removes it.** An
  earlier form of this bullet read *"`SmallRng` is **not** compiled in: it lives behind
  rand 0.8's separate `small_rng` feature, which nothing in the workspace enables."*
  That is **withdrawn** (**D-009 rule 3**, finding M3). Its evidence covered
  `rand 0.8.8` only, and the conclusion was stated over the whole build. `rand 0.9.5`
  lists `small_rng` — and `thread_rng` — among its **default** features, and it enters
  the integrated tree through four dependencies, all of them under libp2p features we
  use:

  ```
  rand v0.9.5
  ├── hickory-proto v0.25.2
  │   └── hickory-resolver v0.25.2
  │       └── libp2p-dns v0.44.0
  │           └── libp2p v0.56.0
  │               └── p2p-poker v0.1.0
  ├── hickory-resolver v0.25.2 (*)
  ├── igd-next v0.16.2
  │   └── libp2p-upnp v0.5.0
  │       └── libp2p v0.56.0 (*)
  └── yamux v0.13.10
      └── libp2p-yamux v0.47.0
          └── libp2p v0.56.0 (*)
  ```

  Two of the four take `rand` with its default feature set — `igd-next` and `yamux`,
  neither of which passes `default-features = false` — and cargo unifies features
  additively across the graph, so their `default` enables `small_rng` on the single
  `rand 0.9.5` every consumer shares. The two hickory crates do pass
  `default-features = false`; that suppresses nothing, which is precisely why an
  absence argued from one consumer's manifest does not survive integration.
  **Verification: (b) source** — `rand-0.9.5/Cargo.toml` l. 66–72,
  `default = ["std", "std_rng", "os_rng", "small_rng", "thread_rng"]`, with
  `small_rng = []` at l. 81; `igd-next-0.16.2/Cargo.toml` l. 148–149 and
  `yamux-0.13.10/Cargo.toml` l. 57–58, both plain `version = "0.9.0"` with no
  `default-features` key; `hickory-proto-0.25.2/Cargo.toml` l. 318–324 and
  `hickory-resolver-0.25.2/Cargo.toml` l. 217–220, both
  `default-features = false`. **(a) executed** —
  `cargo tree --edges normal -i rand@0.9.5` prints the tree above **verbatim** — all
  four consumers arrive under `libp2p 0.56.0`, through the `dns`, `upnp` and `yamux`
  features this project turns on — and `cargo tree -e features -i rand@0.9.5` resolves `alloc`, `default`,
  `os_rng`, **`small_rng`**, `std`, `std_rng`, **`thread_rng`**, with the enabling
  edge shown as `small_rng ← default ← {igd-next v0.16.2, yamux v0.13.10}`.

  The narrower 0.8 fact is kept for what it actually is, a fact about one major and
  not about the build: `small_rng` is **not** enabled on `rand 0.8.8`.
  **Verification: (b) source** — `rand-0.8.8/Cargo.toml` `[features]`,
  `small_rng = []`; **(a) executed** — `cargo tree -e features -i rand@0.8.8` resolves
  exactly `alloc`, `default`, `getrandom`, `libc`, `rand_chacha`, `std`, `std_rng`
  across both its consumers, `ark-std 0.5.0` and `libp2p-autonat 0.15.0`.

  **`rand 0.10.2` has no `small_rng` feature at all — and a third pass nearly wrote
  that down as a third absence.** It is not one. The feature was removed because the
  type stopped being optional: `rand-0.10.2/src/rngs/mod.rs` declares `mod small;` at
  l. 97 and `pub use self::small::SmallRng;` at l. 106 with **no `cfg` attribute**,
  where `rand-0.9.5/src/rngs/mod.rs` has `#[cfg(feature = "small_rng")]` on both
  (l. 87, l. 102). The same file also exports `Xoshiro128PlusPlus` and
  `Xoshiro256PlusPlus` ungated. So `SmallRng` is compiled in at 0.10.2 **more**
  unconditionally than at 0.9.5, not less.
  **Verification: (b) source** — `rand-0.10.2/Cargo.toml` `[features]` l. 58-74 (no
  `small_rng` key) and `rand-0.10.2/src/rngs/mod.rs` l. 97-110 read in full; **(a)
  executed** — `cargo tree -e features -i rand@0.10.2` resolves `alloc`, `default`,
  `getrandom`, `std`, `std_rng`, `sys_rng`, `thread_rng`.

  So across the three majors — and this table, not any sentence about a feature, is the
  form the fact may be stated in:

  | | `rand` 0.8.8 | `rand` 0.9.5 | `rand` 0.10.2 |
  |---|---|---|---|
  | `StdRng` | in the build | in the build | in the build |
  | `SmallRng` | not in the build | **in the build** (default feature) | **in the build**, ungated |
  | `ThreadRng` | not in the build | in the build | in the build |
  | Xoshiro128/256++ | — | — | in the build, ungated |
* `StdRng` *is* present, and ziffle uses it deliberately and correctly for
  *deterministic public constants only* — the Pedersen generators and the 52 open-deck
  points, each seeded from a fixed SHA-256 of a public label. That is a
  nothing-up-my-sleeve derivation, not a source of protocol randomness.
* **Done, and it is what the security property now rests on.** Since the dependency
  graph does not enforce §7 and cannot be made to, our own code does, mechanically.
  `src/security/rng.rs` is the only source of cryptographic randomness in the crate —
  `pub fn fill(dest: &mut [u8])` over `getrandom::SysRng` — and its test
  `tests::our_own_code_uses_no_generator_but_the_os_one` walks every `.rs` file under
  `src/` on every test run and fails the build on `SmallRng`, `StdRng`, `thread_rng`,
  `from_seed`, `seed_from_u64` or `rand::rngs`. `rng.rs` itself is the single exempt
  file, because it must name the identifiers in order to forbid them. See §12 item 6.

  **The gate was verified to bite**, by injecting a violation into an unrelated module
  and observing the failure, then removing it and observing the pass — a gate never
  seen to fail is not a gate (`DECISIONS.md` D-009 rule 3):

  ```
  SPEC_CS.md section 7 forbids these generators for cryptographic use;
  draw from security::rng::fill instead:
    poker\actions.rs:8: SmallRng
  test result: FAILED. 0 passed; 1 failed
  ```

  **Verification: (a) executed** — with the violation removed,
  `cargo test --lib security::rng -- --test-threads=19` gives
  `test security::rng::tests::our_own_code_uses_no_generator_but_the_os_one ... ok`,
  `test result: ok. 3 passed; 0 failed`.

**This section is the source, and two other documents were corrected to match it.**
`PROTOCOL.md` §4.4 and `THREAT_MODEL.md` assumption A7 both claimed that *"the `rand`
crate is deliberately absent from the dependency tree"*, so that `SmallRng` and
`StdRng` were structurally unavailable and `SPEC_CS.md` §7 was enforced by the
dependency graph. That claim is false — see the `cargo tree` output above — and both
were corrected to the text of this section. `SPEC_CS.md` §7 is enforced by the test of
§12 item 6, **not** by the dependency graph, and no document may claim a structural
guarantee. The residual risk is OQ-8.

**And the same correction had to be made a second time, which is the reason D-009
rule 3 exists.** The first pass replaced *"the `rand` crate is absent"* with
*"`rand 0.8.8` is present but `small_rng` is not enabled"* — a narrower absence, still
stated over the whole build, still false at `rand 0.9.5`. Any document restating this
section carries the *discipline* and the test that enforces it, never an absence:
`PROTOCOL.md` §4.4 and `THREAT_MODEL.md` A7 must therefore drop "`SmallRng` is absent"
in every form, and `research/CRYPTO_LIBS.md`'s 0.8-only check must gain the 0.9.5 line
above. **Done in the research note, 2026-08-28:** that check has been replaced by
`CRYPTO_LIBS.md` §1.2.1, which carries the integrated picture including the 0.10.2
ungated export, and §1.2.2, which carries the discipline and the test; its §7.3.4
tabulates every remaining absence claim in that file against `Cargo.lock` and
`cargo tree`. **No security property of this project may be stated as the absence of
something from the dependency tree** (`DECISIONS.md` D-009 rule 3).

**The enforceable discipline, in the words of the document that owns the integrated
tree.** `docs/research/INTEGRATION.md` §2, quoted rather than paraphrased so the two
cannot drift:

> Our own code draws cryptographic randomness only from `getrandom::SysRng`.
> `rand`'s `SmallRng`, `StdRng` and any self-seeded generator are never used
> for keys, masking factors, permutations or commitments.

That is what §12 item 6's test checks, and it is a rule about *our* code, which is the
only thing we control — the tree contains three `rand` majors and will keep them, with
`SmallRng` reachable at 0.9.5 and `StdRng` at all three. The enforceable statement is
this discipline and the scan in `src/security/rng.rs` that fails the build on a
violation. Nothing weaker, and nothing phrased as an absence.

### 7.3 The commit/reveal beacon — where it *is* needed

> **Recorded spec deviation.** `SPEC_CS.md` §16 places `RNG_COMMIT` / `RNG_REVEAL` per
> hand. We run them **once per table**, in the setup chain, not per hand. The shuffle
> chain (§7.1) is the per-hand distributed randomness and is strictly stronger for that
> job; a per-hand beacon would add two stages of latency and buy nothing. The beacon
> covers seating and the initial button only. This note exists because §36 requires a
> deviation from a binding spec section to be recorded rather than absorbed. Recorded
> also in `PROTOCOL.md` §4.4 and in the deviation register of `THREAT_MODEL.md` §9.1
> (entry 2).

`SPEC_CS.md` §16 names `RNG_COMMIT` and `RNG_REVEAL`, and §7 asks for the mechanism.
It is needed for the **non-deck** randomness: seat assignment at table start, and the
initial button position. (Subsequent buttons rotate deterministically; only the first
needs randomness.)

**Ownership, settled: `PROTOCOL.md` §4.4 owns `commitment_i` and `seed`, and this
section owns the argument for why they are shaped that way.** This is the
disposition of `K-6`. Until this pass **both** documents claimed to be the
canonical source of the same two constructions — §4.4 saying *“this is the canonical
form for the whole corpus”*, and this section saying the five-part binding here was
the canonical one and that §4.4's earlier two-part form *“was corrected to match this
section”*. They were byte-identical, which is exactly the condition D-011 rule 1 was
adopted for and not a defence: two canonical claims about one construction is a
drifted copy that has not drifted **yet**, and every copy in this corpus that
eventually diverged was byte-identical first. **The two code blocks that stood here
are deleted**; §4.4 carries the commit and combine constructions, the field order and
the receiver's recomputation check, and this section reproduces no part of them.

The reason the wire owner wins rather than the construction owner is D-011 rule 1
applied without exception: `commitment_i` and `seed` are **message field contents**,
they are recomputed by receivers as a validation step, and a receiver reading a stale
copy rejects honest peers. The reasoning below is what this document keeps, and it is
not a copy of anything — §4.4 states *what the bytes are*, and points 1 to 5 state
*why they must be those bytes and what breaks if they are not*.

Both constructions use `h`, the length-prefixed domain-separated constructor defined
once in `PROTOCOL.md` §2.8, with domain strings taken from that same section's
register — the single register. **One retirement is this document's own record and is
kept here for that reason:** `p2p-poker v1 rng-seed`, which an earlier draft of this
section used for the combine step, is not in the register and must never be valid.
§2.8 lists it among the retired strings, and the note stays here so that a reader who
finds the old string in an old branch learns where it came from and that it was ours.

**Why a player cannot change its contribution after seeing the others':**

1. The commitment is **binding**: producing a different `r_i'` with the same
   `commitment_i` requires a BLAKE3 collision.
2. The commitment is **hiding**: `salt_i` is 32 fresh OS-CSPRNG bytes, so
   `commitment_i` leaks nothing about `r_i` even though `r_i` may come from a small
   space.
3. The commitment binds `table_id`, `session_id` and the committer's application public
   key, so a commitment cannot be lifted from another table, another session or another
   player. **Those three parts are the whole of this point and the reason §4.4's
   earlier two-part form `h(domain, [r_i, salt_i])` was insufficient**: it bound
   neither the table, nor the session, nor the committer, so a commitment observed at
   one table could be replayed at another by a peer who had not yet drawn. Which form
   is normative is §4.4's to state and it states it; what this point contributes is
   the attack each of the three parts is there to stop, which is the thing an editor
   tempted to shorten the part list needs to read first.
4. **All** commitments must be published and accepted into the hash chain *before* any
   reveal is accepted. The ordering is enforced by the state machine and by
   `previous_event_hash`, not by wall-clock timing.
5. Failing to reveal after committing is a protocol failure attributable to that peer,
   handled exactly like a missing decryption share (§2.10): the table setup **stops
   neutrally** — no chips are at stake yet, since this beacon runs once per table in
   the setup chain and before any hand — and the peer that did not reveal is named in
   the transcript as a record. Under **D-010** nothing further follows: it is not
   unseated, not block-listed and not penalised. It is *not* an opportunity to
   re-randomise either: a non-revealer must never be able to force a re-run that
   resamples the draw, and the rule that closes that is point 4's — every commitment is
   chained before any reveal is accepted — not a sanction.

Point 5 matters. A "last revealer" who can stall and force a fresh beacon gets to
resample the seat draw. Because this beacon decides only seating and the initial
button — never cards — the value of that attack is small; and because the commit set is
fixed in the chain before any reveal is seen, a re-run starts from the same committed
values rather than from a free hand.

**D-012, checked here.** `seed` is a hashed value that decides seating and the initial
button, so it is canonical state and may not be derived from a quantity that can differ
between honest receivers. The rule that keeps it safe is point 4's, and it is worth
naming as a D-012 rule and not only as an anti-grinding one: the combine step runs over
the committer set fixed by the **chained** commit stage, in ascending seat order, and
never over "the reveals this peer happened to receive". A partial or receiver-local
seed is not a permitted outcome — a missing reveal stops the setup neutrally (point 5)
rather than producing a shorter list. Two honest peers therefore either compute the
same `seed` or compute none, which is the only pair of outcomes D-012 allows.

Note honestly what is *not* closed: a peer that
withholds its reveal denies the table its beacon, and under D-010 that costs it
nothing. That is the same liveness/DoS trade as §2.10, at a point in the session where
no chips are committed, so it is strictly cheaper than the in-hand case.

`ziffle` provides none of this; it is ours (§5.2 item 3), and it is a plain hash
commitment over a library hash, so §6 and §36 are satisfied.

---

## 8. Decryption shares and their DLEQ proofs

The algebra is in §2.6. This section states the operational rules.

**A share without a proof is worthless and must be rejected.** Without the
Chaum–Pedersen proof, a player could publish `share_i = random` and make the sum
resolve to `None`, or — worse, if it knew the target — steer the lookup. The DLEQ
proof ties `share_i` to the same `sk_i` that appears in `pk_i`, which is in `apk`,
which was fixed before the shuffle chain. The chain from "this token" back to "this
player's committed key" is unbroken.

**Verification of the negatives, from the research suite:**

```
T7a A's token verified against B's pubkey rejected:   true
T7b token replayed onto a different card rejected:    true
T7c full reveal (both tokens) works:                  Some(50)
T7d ONE player alone cannot open the card:            true (got None)
T7e other player alone cannot open the card:          true (got None)
```

**Verification: (a) compiled and ran** — `probe-ziffle/src/bin/adv.rs`; `T7d`/`T7e`
independently reproduced in `probe-cryptodoc` as `[2] … None`.

**Operational rules, binding on the implementation:**

1. Verify **every** token's DLEQ before aggregating. `AggregateRevealToken::new` takes
   `&[Verified<RevealToken>]`, so the type system already forces this — an unverified
   token cannot be aggregated, and `Verified<RevealToken>` cannot be deserialised.
   **Verification: (b) source** — `AggregateRevealToken::new(pks: &[Verified<RevealToken>])`;
   the `impl_valid_and_serde_unit!` invocations cover `PublicKey`, `SecretKey`,
   `PedersonCommitment`, `RevealToken` and `MaskedDeck<N>` — **not** any `Verified<_>`.
2. Produce a token **only** against a card taken from a deck this client verified
   itself, i.e. from our own `Verified<MaskedDeck<52>>`. Never against a `MaskedCard`
   handed to us on the wire.
3. Publish a token **only** for an index that is due (§2.7, §2.8), and **never for
   one's own hole index before `SHOWDOWN_REVEAL`**. Both are protocol violations: the
   receiver **rejects** the token, which is the enforcement, and names the sender in
   the transcript, which under **D-010** is a record with no automatic consequence.
   **Neither is a proof failure and neither is D-014 tier 1**: the token verifies, and
   what is wrong with it is a judgement against protocol state — which street it is and
   which indices this hand's deal map made due — so it is **tier 2** and reaches a
   removal only against state fixed by a checkpoint both peers signed (§8.1.3).
4. Tokens are carried inside the signed CBOR envelope, bound to
   `(table_id, hand_id, street, card_index)`, because the DLEQ itself does not bind
   them (§6.4 item 2).
5. Token publication is **automatic and never gated on the human** (D-006). Only a
   client that is gone or deliberately withholding produces the D-005 case.
6. A share for a card that is *not* opened this hand — a mucked hole card, or any of
   the unused indices `2m+5 … 51` (there are no burns, §2.4) — is never published,
   never logged, and the scalar is zeroised.

**Cost:** 131 bytes per player per revealed card (33 + 98), and 1.39 ms to 4.34 ms to
open one card end to end for 2 to 6 players (§6.5).

### 8.1 What a failed proof proves, and what it does not (D-014)

**Why this section exists.** **D-014** removes a player from the table on
*self-authenticating* evidence — *"a message signed by the accused, whose illegality any
peer can decide alone, from that message plus state the peers provably share"* — and
its tier-1 list has six clauses, of which **three are this document's**: a failed
**shuffle proof** (§6), a failed **decryption-share proof** (§2.6, §8) and a failed
**key-ownership proof** (§2.1). A fourth, *a deck that gains, loses or duplicates a card
across a shuffle*, is not a separate check at all and §8.1.4 says why. Until this pass
this document carried **zero** occurrences of `D-014` while owning half of the evidence
the decision rests on, which is the omission `N9` names. What follows states, for each
proof, the two things an implementer has to know before a verdict is allowed to cost
somebody their seat: **what a verification failure proves**, and **what it does not**.

**The general shape, stated once.** A verifier here is a pure function of
`(the signed message, public inputs, the Fiat–Shamir transcript)`. Where every public
input is either a constant or accepted chain content, two honest peers run the *same*
function on the *same* arguments and get the *same* answer — which is exactly D-014's
condition, and it is why **a failed proof needs no quorum, no vote, no certificate and
no timing**: the evidence is a signature by the accused over an object that is invalid
on its face, and framing it would mean forging that signature (§2.4's `verify_strict`).
That is also why **tier 1 can act on arrival** — the verdict does not depend on arrival
order, on how many peers received the message, or on anything a slow peer might still
send. Compare the artefacts D-014 excludes by name: a timeout needs a clock nobody
shares, an equivocation predicate needs a slot key that has been wrong five times, and
`attributed` is per-receiver. None of those is a function of shared inputs; all three of
these are.

**And the one condition on that, which is the whole of the fine print.** The public
inputs must be the *agreed* ones. Each proof below names them explicitly. A verifier
that substitutes a locally reconstructed input for a chained one has stopped computing
the shared function, and its *invalid* verdict then proves nothing about the signer —
it proves the two peers disagree about the input, which is a divergence
(`STATE_MACHINE.md` T50, `PROTOCOL.md` §6.3) and never a removal.

#### 8.1.1 The key-ownership proof (§2.1) — the purest tier 1

**What a failure proves.** The signer published a `DECK_INIT` carrying `pk_i` and a
Schnorr proof that does not verify under `(ctx, pk_i)`. Because the proof is checked
against the message's own fields plus `ctx`, and `ctx` is a function of `table_id`,
`hand_id` and the shuffle-round sentinel alone (`PROTOCOL.md` §4.5, §6.4 discipline item
1), **no game state whatever enters the verdict**. It is decidable from the message and
constants, which is the strictest reading of D-014 tier 1 and the only one of the three
that needs no chain content at all. What it establishes is precisely §2.1's stated
threat: a party that cannot demonstrate knowledge of `sk_i` may be mounting the rogue-key
construction `pk_j = X − Σ_{i≠j} pk_i`, and the proof is what closes it.

**What it does not prove.** Not that the rogue-key attack succeeded — it cannot have,
because `AggregatePublicKey::new` takes `Verified<PublicKey>` and an unverified key is
not aggregable, so the honest peer's failure is *also* the mitigation. Not that any other
key in the hand is honest. Not that the signer is the party who benefits: a relayed or
replayed body is still signed by its author, and §2.4's envelope plus `hand_id` in `ctx`
is what makes a cross-hand replay fail here rather than elsewhere. And **not** that the
hand is unplayable — the hand is voided by `STATE_MACHINE.md` T64 because a removal voids
it, not because the cryptography could not continue.

#### 8.1.2 The shuffle proof (§6) — tier 1, with its inputs named

**What a failure proves.** The signer published a `DECK_SHUFFLE` whose proof does not
verify for the statement of §6.1 against **the input deck `D_{k-1}` this peer accepted as
chain content**, the aggregate key `apk` of this hand, and the transcript of §6.4. It
proves that this signer's output deck is not a re-encryption permutation of that input —
which, by §6.1's permutation argument, covers dropping, duplicating and substituting a
card in one verdict.

**What it does not prove, and the first item is the one that matters.** It does **not**
prove misbehaviour if `D_{k-1}`, `apk` or `ctx` differ between the two peers: completeness
is unconditional, so an honest prover's proof always verifies *against the inputs it
proved over*, and a failure therefore says either *the prover cheated* or *we are not
looking at the same deck*. The corpus has produced the second case with no adversary at
all (`DECISIONS.md` K-1, `THREAT_MODEL.md` X33–X37). This is why the three inputs must be
read off accepted chain content and never re-derived: `apk` is pinned to the chained
`HAND_INIT`'s `dealt_in` (§2.1's D-012 bullet), `D_{k-1}` is the accepted previous stage,
and every part of `ctx` is a constant or accepted chained content — **which is true now
and was not when §6.4's discipline item 1 was written** (`DECISIONS.md` J-5: `session_id`
carried `advert_hash` transitively until J1 was fixed, so the binding item 1 demands was
violated through a field item 1 did not mention). Second, a failure does not localise
*what* was wrong in the deck: the argument is one statement about the whole permutation,
and an implementation must not report "card 5 was duplicated" from a failed verification,
because it did not learn that. Third, it says nothing about earlier shufflers in the
chain, whose proofs stand or fall on their own inputs.

#### 8.1.3 The decryption-share proof (§2.6, §8) — tier 1 for the share, tier 2 for the timing

**What a failure proves.** The signer published a reveal token whose Chaum–Pedersen DLEQ
does not tie `share_i` to the `sk_i` behind the `pk_i` that is in this hand's `apk`. It is
evidence against that signer alone: §8's negatives are measured, and both of the
misbehaviours the proof exists to catch — a random share (`T7a`) and a token replayed onto
a different card (`T7b`) — fail verification, so the failure is not a diagnosis of which
one it was, only that this signer's token is not a share of this card under its committed
key.

**What it does not prove.** Not that the card is lost: an invalid token is simply not
aggregated (rule 1), and the card opens as soon as the honest shares are in. Not anything
about a **missing** token — silence is not a proof failure, it is D-005's disconnect case
and D-006's timeout case, and neither is ever a removal (D-014 excludes them by name). And
— this is the boundary an implementer will get wrong — **a token that verifies but arrives
for an index that is not due is not tier-1 evidence at all.** §8's rule 3 forbids
publishing a token for an undue index and for one's own hole index before
`SHOWDOWN_REVEAL`; those are judgements against *protocol state* — which street it is,
which indices this hand's deal map made due — so they are **D-014 tier 2** and reach a
removal only once the state they are judged against is fixed by a checkpoint both peers
signed. Rule 3's own sentence stands as written for the tier-1 half: the receiver
**rejects** the token and names the sender, and under D-010, as narrowed by D-014, nothing
further follows automatically until the tier-2 precondition is met.

#### 8.1.4 The fourth clause is not a fourth check

D-014's tier-1 list also names *"a deck that gains, loses or duplicates a card across a
shuffle"*. That is **the same verdict as §8.1.2 and not an independent one**: §6.1's
permutation statement is exactly what excludes it, and the research suite measured it
(`T4`: a deck with card 5 overwritten by card 3, presented with an honest proof, is
rejected). An implementation that adds a hand-rolled card-conservation check beside the
proof has built a **second verifier** for one statement, which is the drifted-copy shape
D-011 rule 1 exists to stop — and under D-014 a drifted copy no longer costs a rejected
message, it costs an honest player their seat.

#### 8.1.5 The rule this section adds, and it is the one D-014 makes load-bearing here

> **A removal may rest only on a verifier that ran and returned *invalid*. It may never
> rest on a verification that could not be performed.** A deserialisation failure, a
> truncated body, a length that does not match, an allocation failure, a panic caught at
> the boundary, a missing input, a library version that cannot parse the encoding — none
> of these is a proof failure. They are indistinguishable, at the verifier's call site,
> from *our own* defect, and D-014's safety argument covers only the case where every
> honest peer computes the same verdict from the same inputs. Where verification cannot
> be completed, the message is rejected or deferred and **no `CheatProven` is produced**.

The reason to write that down in this document rather than in the engine's is that this is
where the call sites are, and the failure mode is ours and not the adversary's: with
D-014, *"a validator that is too strict now ejects honest players rather than merely
rejecting a message"*. `DECISIONS.md` **C-1 to C-3** are three live instances — the shipped
`src/protocol/` signs a digest where §2.4 signs a domain-prefixed body, and its domain
register matches `PROTOCOL.md` §2.8 in no string — and two implementations differing that
way would each remove the other on sight, each correctly by its own rules. The gate on
that is `THREAT_MODEL.md`'s mirror test (`DECISIONS.md` **D-014-2**): under every legal
interleaving, no honest peer is evictable. **Nothing in this section may be read as
clearing the removal feature to ship; it states what the evidence means, and the mirror
test is what says it is safe to act on.**

**And what none of the three proofs answers, recorded because it is somebody else's and is
open:** how a removal reaches canonical state at all, given that whether the offending
message arrived is per-receiver while a seat's status is hashed. That is
`DECISIONS.md` **D-014-3**, owned by `PROTOCOL.md`. This section defines what the evidence
proves; it does not decide how the verdict is carried, and an implementer must not infer
from *"any peer can decide alone"* that a peer may act alone on canonical state.

#### 8.1.6 This section audited against itself, and one thing changed underneath it

Two questions, asked of this document as a whole rather than of one subsection,
because D-014's safety rests on the answers being *yes* everywhere and not merely
in the paragraph where they are argued.

**(1) Does anything here describe a tier-1 violation that needs receiver state? No.**
Every removal-bearing verdict this document owns is computed by a verifier over
`(the signed message, public inputs, the Fiat–Shamir transcript)` and reads nothing
the receiver stores. The public inputs are named per proof and are constants or
**accepted chain content** — `ctx` from `table_id`, `hand_id` and the round sentinel
(§8.1.1); `apk`, the accepted input deck `D_{k-1}` and `ctx` (§8.1.2); the `pk_i` inside
this hand's `apk` (§8.1.3). Where such an input differs between two peers the verdict is
a **divergence and not a removal**, which is the fine print stated at the top of §8.1 and
repeated in §8.1.2 because that is the proof it bites hardest. The nearest thing to a
state-dependent clause in this document is §8 rule 3's *undue index*, and it is
classified **tier 2** by name (§8.1.3) precisely because deciding it needs the street and
the deal map. There is no clause here that a dropped frame or a slow peer could flip.

**(2) Is each of the three failures stated as evidence against its signer, needing no
quorum? Yes, and that is the property doing the work.** §8.1.1, §8.1.2 and §8.1.3 each
open with *what a failure proves* about **the signer of that message**, and §8.1's general
shape states once, for all three, that a failed proof needs **no quorum, no vote, no
certificate and no timing** — which is why tier 1 may act on arrival rather than waiting
for a checkpoint. The counterpart is stated just as plainly: a **missing** token is
silence and never evidence (§8.1.3), and a verification that could not be *performed* is
never evidence either (§8.1.5).

**What changed underneath this section in this pass.** D-014's tier-1 list carried a
seventh kind of clause — *a message whose chain parent does not exist* — which is
decidable only against the receiver's own store, so one dropped frame would have removed
an honest player. It is deleted from `DECISIONS.md` D-014, `PROTOCOL.md` and
`THREAT_MODEL.md`, and it was never this document's: **nothing in §8.1 cited it, depended
on it, or has to change for it.** The count this section quotes is unaffected — D-014's
list still has six bullets, three of them this document's — and `THREAT_MODEL.md` §5.1.1
is now the normative owner of the admissibility test a future tier-1 clause must pass.
That test is the general form of the property §8.1 argues for these three proofs, and a
new verifier added here must be held to both: it must return `invalid` from a run that
happened (§8.1.5), and its verdict must read nothing the receiver stores.

---

## 9. Library boundary and the swap plan

The concrete library is **`ziffle = "=0.1.0"`, vendored into the repository** at
`vendor/ziffle/` with a `[patch.crates.io]` entry, not merely pinned. Rationale: one
release, one author, three GitHub stars — the crate could be yanked or the repository
deleted; and a vendored copy is what makes the §11 risk-1 review meaningful and
auditable by third parties. `Cargo.lock` is committed.

All ziffle types are wrapped behind our own trait in `src/mental_poker/`, so that no
other module names a ziffle type:

```rust
pub trait DeckCrypto {
    type SecretKey; type PublicKey; type MaskedDeck; type ShuffleProof;
    type RevealToken; type RevealProof;
    fn keygen(&self, rng: &mut impl Rng, ctx: &Ctx) -> (Self::SecretKey, Self::PublicKey, KeyProof);
    fn shuffle(&self, …, ctx: &Ctx) -> (Self::MaskedDeck, Self::ShuffleProof);
    fn verify_shuffle(&self, …, ctx: &Ctx) -> Result<VerifiedDeck, ShuffleError>;
    fn reveal_token(&self, …, ctx: &Ctx) -> (Self::RevealToken, Self::RevealProof);
}
```

Given risk 1 in §11 this is not speculative generality — it is the mitigation. If the
review of ziffle finds problems that cannot be fixed cheaply, the fallback is
`barnett-smart-card-protocol` with all three of its repositories vendored (§3), and
the trait boundary makes that a single-module change.

**Dependency register entry (`SPEC_CS.md` §28):**

| Field | Value |
|---|---|
| Name | `ziffle` |
| Version | 0.1.0 (only release, 2025-11-01) |
| Purpose | Barnett–Smart mental poker with Bayer–Groth shuffle proofs |
| Repository | `github.com/v26-solutions/ziffle` |
| Licence | MIT OR Apache-2.0 |
| Security status | **unaudited**; vendored; pending in-house review (§11 risk 1) |
| Size | 1 779 lines in a single `src/lib.rs` + 736 lines of tests; `no_std`, no allocation |
| Upstream test suite | 16 unit tests + 15 doctests, 0 failures on rustc 1.95.0 |

**Verification: (c) registry** for version, licence and date; **(b) source** for the
line counts and `no_std`; **(a) compiled** — `cargo test --release` in the unpacked
crate (`docs/research/MENTAL_POKER.md` §4.1).

Transitive crates that enter with it, all MIT OR Apache-2.0: `ark-ec`, `ark-ff`,
`ark-poly`, `ark-secp256k1`, `ark-serialize`, `ark-std` 0.5.0, plus `sha2 0.10.9`,
`zeroize`, `rand 0.8.8`, `rand_chacha 0.3.1`, `rand_core 0.6.4`.
**Verification: (b) source** — each crate's `Cargo.toml` `license` field. Note
`ark-std 0.5.0` uses the deprecated SPDX form `"MIT/Apache-2.0"` (slash, not `OR`),
which a strict `cargo-deny` licence policy may not parse; it needs an explicit
clarification entry.

### 9.1 The cryptographic side of the `SPEC_CS.md` §28 register

§28 requires a register with six columns — name, version, purpose, repository, licence,
security status — for every crate the client links. **The eventual home of that register
is a new `docs/DEPENDENCIES.md`, generated from `cargo metadata` and checked in CI so it
cannot drift**; a hand-maintained register is wrong within a month, and the stale `rand`
row corrected below is the proof. Until that document exists, the cryptographic side is
carried here and the transport side in `NETWORK_STACK.md` §5.1.

**This table is not complete and does not claim to be.** It covers the cryptographic
crates this document is responsible for; it does not cover the transport tree, the GUI,
or build-only tooling.

| Name | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `ziffle` | 0.1.0 | Barnett–Smart mental poker, Bayer–Groth shuffle proof, DLEQ, Schnorr | `github.com/v26-solutions/ziffle` | MIT OR Apache-2.0 | **unaudited, semver-unstable (0.x, one release)**; vendored at `vendor/ziffle/`; blocking in-house review is OQ-1 |
| `ark-ec` | 0.5.0 | elliptic-curve group traits | `github.com/arkworks-rs/algebra` | MIT OR Apache-2.0 | no advisory; not audited |
| `ark-ff` | 0.5.0 | finite-field arithmetic, `DefaultFieldHasher` | `github.com/arkworks-rs/algebra` | MIT OR Apache-2.0 | pulls `paste 1.0.15` (RUSTSEC-2024-0436, unmaintained) into the **runtime** tree; build-time proc macro only |
| `ark-poly` | 0.5.0 | polynomial arithmetic used by the shuffle argument | `github.com/arkworks-rs/algebra` | MIT OR Apache-2.0 | no advisory; not audited |
| `ark-secp256k1` | 0.5.0 | the secp256k1 curve instance | `github.com/arkworks-rs/algebra` | MIT OR Apache-2.0 | no advisory; not audited |
| `ark-serialize` | 0.5.0 | canonical point/scalar serialisation | `github.com/arkworks-rs/algebra` | MIT OR Apache-2.0 | hostile input not fuzzed — OQ-5 |
| `ark-std` | 0.5.0 | `no_std` shims; the crate that reintroduces `rand` | `github.com/arkworks-rs/std` | `MIT/Apache-2.0` (deprecated SPDX form; needs a `deny.toml` clarification) | no advisory; not audited |
| `rand` | 0.8.8 | transitively required by `ark-std` with feature `std_rng`, and independently by `libp2p-autonat 0.15.0`; **not** a source of protocol randomness | `github.com/rust-random/rand` | MIT OR Apache-2.0 | RUSTSEC-2026-0097 — see the finding below; patched at this version |
| `rand_chacha` | 0.3.1 | backs `StdRng` inside `rand 0.8` | `github.com/rust-random/rand` | MIT OR Apache-2.0 | no advisory |
| `rand_core` | 0.6.4 | the `RngCore` trait our OS-CSPRNG adapter implements (§7.2) | `github.com/rust-random/rand` | MIT OR Apache-2.0 | no advisory; coexists with `rand_core 0.10` — see the duplicate-versions note below |
| `blake3` | 1.8.7 | transcript hash, `state_hash`, RNG commitments, `ctx` | `github.com/BLAKE3-team/BLAKE3` | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | **no public third-party audit** — OQ-6 |
| `ed25519-dalek` | 3.0.0 | application event signatures, `verify_strict` only | `github.com/dalek-cryptography/curve25519-dalek/tree/main/ed25519-dalek` | BSD-3-Clause | past RUSTSEC-2022-0093 patched in `>= 2`; never enable `hazmat` (§10.1) |
| `sha2` | 0.10.9 | Fiat–Shamir hash inside ziffle | `github.com/RustCrypto/hashes` | MIT OR Apache-2.0 | no advisory; RustCrypto, widely reviewed |
| `getrandom` | 0.4.3 | the OS CSPRNG (`fill`, `SysRng`) | `github.com/rust-random/getrandom` | MIT OR Apache-2.0 | no advisory |
| `minicbor` | 2.3.0 | deterministic CBOR for the signed envelope | `github.com/twittner/minicbor` | BlueOak-1.0.0 | no advisory; licence needs an allow-list entry |
| `zeroize` | 1.9.0 | scrubbing secret scalars and seeds | `github.com/RustCrypto/utils` | Apache-2.0 OR MIT | no advisory; cannot reach allocator/OS copies (§10.2) |
| `subtle` | 2.6.1 | constant-time comparison | `github.com/dalek-cryptography/subtle` | BSD-3-Clause | no advisory |
| `argon2` | 0.6.0 | passphrase → KEK for the profile key slots | `github.com/RustCrypto/password-hashes` | MIT OR Apache-2.0 | no advisory |
| `chacha20poly1305` | 0.11.0 | XChaCha20-Poly1305 profile AEAD | `github.com/RustCrypto/AEADs` | Apache-2.0 OR MIT | no advisory |
| `windows-sys` | 0.61.2 | DPAPI key slot on Windows | `github.com/microsoft/windows-rs` | MIT OR Apache-2.0 | no advisory; Windows-only path |

**Verification: (b) source** — every `repository`, `license` and version cell was read
from the crate's own `Cargo.toml` under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.

**What this table's `rand` row does not cover, and where the authority is.** The row
above is the `rand 0.8` line only. `docs/research/INTEGRATION.md` §2 is the authority on
what the *integrated* tree contains, and it records **three `rand` majors at once —
0.8.8, 0.9.5 and 0.10.2 — pulled in by `libp2p-autonat` and by the arkworks crates
under `ziffle`**. Re-verified here against the workspace `Cargo.lock` and
`cargo tree --edges normal -i`: `0.8.8` has two independent parents, `ark-std 0.5.0`
under ziffle and `libp2p-autonat 0.15.0` (which declares `rand = "0.8"` directly in its
`Cargo.toml`), so dropping ziffle would *not* remove it; `0.9.5` arrives via
`hickory-proto`/`hickory-resolver` (`libp2p-dns`), `igd-next` (`libp2p-upnp`) and
`yamux`; `0.10.2` via `quinn-proto` (`libp2p-quic`) and `rs_poker`.
**Verification: (a) executed** — `cargo tree --edges normal -i rand@0.8.8`,
`@0.9.5`, `@0.10.2`; **(b) source** — `libp2p-autonat-0.15.0/Cargo.toml`
`[dependencies.rand] version = "0.8"`. No claim of a structural guarantee follows from
any of this; the enforceable discipline is §7.2's, quoted there from INTEGRATION.md §2.

**The two unaudited, semver-unstable entries in the whole client are `ziffle 0.1.0` and
`libp2p-stream 0.4.0-alpha`** (the latter on the transport side, `NETWORK_STACK.md`
§5.1). Both are flagged in both places deliberately: they are the two crates where a
breaking change or an undiscovered defect lands directly on a security or liveness
property, and neither has a third-party review behind it.

### 9.2 Supply-chain findings that correct the research notes

`docs/research/CRYPTO_LIBS.md` is evidence, not authority, so both corrections below are
carried here — in the authoritative document — as well as in the research note. A reader
of this document must not have to open the research note to learn that a row there is
wrong.

**Finding 1, changing `CRYPTO_LIBS.md` §4.4.** That document expected
RUSTSEC-2024-0436 (`paste 1.0.15`, unmaintained) to appear only via the optional
`dcbor` **dev**-dependency. It is in the **runtime** tree once ziffle lands:

```
paste v1.0.15 (proc-macro)
└── ark-ff v0.5.0
    └── … └── ziffle v0.1.0
```

**Verification: (a) executed** — `cargo tree -i paste -e normal,build` and
`cargo audit` in `probe-cryptodoc`; audit output:
`Crate: paste / Version: 1.0.15 / Warning: unmaintained / ID: RUSTSEC-2024-0436 /
warning: 1 allowed warning found` (exit 0).

`paste` is a proc-macro, so it runs at build time and ships in no binary — but the
scoped `deny.toml` ignore is now required unconditionally, not only when `dcbor` is
present, and the ignore comment must say so. `cargo audit` still exits 0; `cargo deny
check advisories` treats unmaintained as a failure without the ignore.

**Finding 2, changing `CRYPTO_LIBS.md` §7.3.** That document's `rand` row read *"crate
not in the tree at all"*, which was stale: `rand` re-enters through `ark-std 0.5.0`
(§7.2). The advisory was never evaluated against the version actually pulled in. The
correction that was issued read:

> `rand` — RUSTSEC-2026-0097 — **in the runtime tree at 0.8.8 via `ark-std 0.5.0`.
> `informational = "unsound"`, patched at `>= 0.8.6`, so the pinned version is
> patched. The unsound path requires `rand::thread_rng` inside a custom `log`
> implementation, which does not occur here.**

**That correction was itself too narrow**, in the same way and for the same reason as
the `SmallRng` one: `rand` is in the runtime tree at **three** majors, not one, and only
one of the three arrives through `ark-std`. `CRYPTO_LIBS.md` §7.3.1 now carries the
three-major row, and its §1.2.1 carries the feature picture. The advisory conclusion is
unchanged — `0.8.8 >= 0.8.6`, `0.9.5 >= 0.9.3`, `0.10.2 >= 0.10.1`, so every resolved
version clears a patch line — but it clears it *three times over*, and each one has to
be re-checked after a `cargo update` rather than filed away.

**Verification: (c) registry** — the local advisory database,
`~/.cargo/advisory-db/crates/rand/RUSTSEC-2026-0097.md`: `informational = "unsound"`,
`patched = [">= 0.10.1", "< 0.10.0, >= 0.9.3", "< 0.9.0, >= 0.8.6"]`. **(a) executed** —
`cargo tree --edges normal -i rand@{0.8.8,0.9.5,0.10.2}` at the repository root.
`rand_chacha 0.3.1` and `rand_core 0.6.4` enter with 0.8.8 and are in
`CRYPTO_LIBS.md` §10's version list.

Note what this does *not* say: the advisory being benign here is a fact about our usage,
not a structural guarantee. §7 is enforced by the §12 item 6 lint (OQ-8), not by the
absence of `rand`.

### 9.3 Duplicate versions

**Duplicate versions are unavoidable and must be tolerated, not fought.** Once libp2p,
the general crypto set and ziffle are in one workspace, cargo links two `sha2` (0.10
via ziffle and libp2p, 0.11 for our own layer), two `ed25519-dalek` (2.x via
`libp2p-identity`, 3.0 for our application signatures), and two `rand_core` (0.6 via
ark-std, 0.10 via getrandom). Set `multiple-versions = "warn"` in `deny.toml`;
`"deny"` would fail the build on a condition we cannot fix without forking libp2p.
Types do not interoperate across these boundaries — cross them as raw `[u8; 32]` and
reconstruct, which is what `SPEC_CS.md` §20 wants anyway.
**Verification: (c) registry** — the version requirements
(`docs/research/CRYPTO_LIBS.md` §8); **(a) compiled** — the `rand_core` 0.6/0.10
coexistence in `probe-cryptodoc`.

---

## 10. Key management (`SPEC_CS.md` §21)

### 10.1 Three key types, deliberately separate

`SPEC_CS.md` §20 requires the identities to be separated. Concretely:

| Key | Lifetime | Storage | Purpose |
|---|---|---|---|
| libp2p identity keypair (Ed25519, `libp2p::identity::Keypair`) | long-term, persisted | profile, key slot file | derives `PeerId`; authenticates the *transport*. Says which socket you are talking to, not who is playing |
| Application signing key (Ed25519, `ed25519-dalek 3.0.0`) | long-term, persisted | profile, key slot file | signs every protocol event, table advertisement and timeout certificate. **This** is the poker identity |
| Per-hand deck key (`ziffle::SecretKey`, a secp256k1 scalar) | one hand | **memory only, never persisted** | the player's share of `apk`; produces reveal tokens |

They must never be conflated. In particular, GossipSub's `MessageAuthenticity::Signed`
+ `ValidationMode::Strict` signs with the *libp2p* key: that is transport hygiene, not
the §4 table-advertisement signature. Both signatures are required and neither
substitutes for the other.
**Verification: (b) source** — `libp2p-gossipsub` `MessageAuthenticity` /
`ValidationMode` (`docs/research/LIBP2P.md`).

Persisted Ed25519 keys are stored as the **32-byte seed only**;
`SigningKey::from_bytes(&seed)` reproduces the verifying key exactly.
**Verification: (a) compiled and ran** — `probe-crypto-final`, the `assert_eq!` on
`SigningKey::from_bytes(&seed).verifying_key()`.

**Always `verify_strict`, never `verify`.** Plain `verify` accepts signatures under
small-order or non-canonical public keys, permitting malleability — two distinct byte
strings validating for one logical event, which is an equivocation hole under §14.
Never enable the `hazmat` feature and never reconstruct a key from separately-sourced
secret and public halves (the shape of RUSTSEC-2022-0093 / CVE-2022-50237, patched in
`>= 2`; we are on 3.0.0). **Verification: (c)** — RustSec advisory-db.

**The per-hand deck key is a footgun to guard.** `ziffle::SecretKey` derives
`Zeroize` and `ZeroizeOnDrop` — good — but it *also* implements `CanonicalSerialize`
and `CanonicalDeserialize`, so nothing in the type system stops it from being written
to a file or a log.
**Verification: (b) source** — `impl_valid_and_serde_unit!(SecretKey);` at
`ziffle-0.1.0/src/lib.rs`. Our wrapper type must therefore not expose serialisation
for it at all, and its `Debug`/`Display` must print `<redacted>`.

### 10.2 What must never be logged

Private keys of any of the three kinds; other players' hole cards; decryption shares
that must stay secret (any token for a card not legitimately opened); raw RNG secrets
before their safe reveal; the passphrase; the DEK or KEK. Enforce with a newtype whose
`Debug` and `Display` print `"<redacted>"`, so it is a type property rather than a
thing to remember.

Zeroise after use: the Ed25519 seed, the Argon2-derived KEK, per-hand RNG secrets
before reveal, decryption shares, and every mental-poker private scalar. **Honest
limitation:** `zeroize` cannot reach copies the allocator, the compiler or the OS swap
file already made. It narrows the window; it does not close it.

### 10.3 The portability tension, and how it is resolved

§21 asks for an OS mechanism such as DPAPI on Windows. §22 requires the client to be
**portable**: one copyable directory, no installer, no registry writes, nothing
written outside its own folder, profile next to the program so the whole directory can
be copied to another machine.

These contradict each other naively: a DPAPI blob is bound to the user's Windows logon
credential and machine state and **will not decrypt** after the folder is copied.

**Resolution: one file, several key slots** (the LUKS keyslot idea).

```
profile/identity.key
├── header (plaintext, authenticated as AEAD associated data)
│     magic "P2PPOKER-ID\0", format_version, slot descriptors
├── slot[0]  passphrase : argon2id params + 16-byte salt + wrapped DEK   [PORTABLE]
├── slot[1]  dpapi      : DPAPI blob of the DEK (Windows only)           [CONVENIENCE]
├── slot[2]  plaintext  : DEK in the clear (prototype / CI only)         [INSECURE]
└── payload  XChaCha20-Poly1305(DEK, nonce, key material), AAD = header
```

* The key material is encrypted **once**, under a random 32-byte DEK from the OS
  CSPRNG. Each slot independently wraps that same DEK; slots are additive.
* The header is bound as associated data, so a slot cannot be stripped or swapped
  without failing the Poly1305 tag.
* **Portability (§22) is guaranteed by slot 0.** Copy the directory anywhere; the
  passphrase opens it. Nothing is written outside the folder and nothing touches the
  registry.
* **OS protection (§21) is honoured by slot 1.** On the machine where the profile was
  created the client opens silently.
* Copying a profile that has only slot 1 fails **loudly** at slot-1 unwrap; the client
  falls back to slot 0, or — if slot 0 is empty — tells the user plainly that the
  profile was machine-bound and offers to generate a new identity. Never a silent
  failure.
* Slot 2 exists so the play-money prototype and CI can run unattended. Opt-in, with a
  persistent GUI warning while active.

Parameters: Argon2**id** (RFC 9106's recommendation for password-based KDF), OWASP
baseline `m = 19 MiB, t = 2, p = 1` (`Params::new(19 * 1024, 2, 1, Some(32))` — `m_cost`
is in KiB). **XChaCha20-Poly1305**, not ChaCha20-Poly1305: the 192-bit nonce makes a
fresh random nonce per write safe without a counter that copying a profile would
desynchronise.
**Verification: (a) compiled and ran** — `probe-crypto-final` step `[7]`, including the
negative case: flipping one ciphertext bit makes `decrypt` fail the tag.

DPAPI specifics: both `CryptProtectData` and `CryptUnprotectData` exist in
`windows-sys 0.61.2` under feature `Win32_Security_Cryptography`, and the blob struct
is named **`CRYPT_INTEGER_BLOB`**, not `DATA_BLOB` as in the C headers. Always pass
`CRYPTPROTECT_UI_FORBIDDEN` so a background thread can never raise a modal prompt, and
always pass application entropy (`b"p2p-poker/identity/v1"`) — without it any other
process running as the same user can unprotect the blob. `LocalFree` the output blob;
the API allocates it with `LocalAlloc`.
**Verification: (b) source** —
`windows-sys-0.61.2/src/Windows/Win32/Security/Cryptography/mod.rs` lines 283 and 315;
**(a) compiled and ran** — `probe-crypto-final` step `[8]`:
`DPAPI wrap/unwrap ok (48 -> 278 B), wrong entropy rejected`.

Linux slot 1 is a `0600` file inside the profile directory with the parent at `0700`,
**not** the `keyring` crate: `keyring 4.1.6`'s default `v1` features route Linux
through the D-Bus Secret Service, which is absent on headless hosts, in containers and
over plain SSH — directly contradicting §22.
**Verification: (b) source** — `keyring-4.1.6/Cargo.toml`.
**Honest limitation: the Linux path has never been compiled.** Only
`x86_64-pc-windows-msvc` is installed on this machine, so the `cfg(unix)` branch is
unproven and must be verified on a Linux host before Phase 7.

§3 of the spec also requires that two installations never share a `PeerId`. The DEK
and both long-term seeds are drawn from the OS CSPRNG at first run; copying a profile
deliberately copies the identity, which is the documented meaning of "copy the
directory".

---

## 11. What we are not sure about (`SPEC_CS.md` §36)

§36 requires that where we are not cryptographically certain we stop and document
rather than improvise. Doing that here. Confidence in the **construction** is high;
confidence in the **implementation** is medium-low and contingent on risk 1.

### High confidence

* Barnett–Smart + Bayer–Groth is the right construction for this problem, and it
  satisfies every §6 property. — *(d) literature plus the property mapping in §2.*
* Performance is a non-issue: ~100 ms prove, ~42 ms verify, 5 547 B, on the slowest of
  the three candidate groups. — *(a) compiled.*
* Cut-and-choose is the wrong trade at 195–390 KB per proof. — *(a) compiled.*
* The n-of-n privacy property holds in this implementation: one share does not open a
  card. — *(a) compiled, `probe-cryptodoc` and `probe-ziffle` T7d/T7e.*
* Tampered decks are rejected, including the §8 remove-a-card/duplicate-an-ace attack.
  — *(a) compiled, T4/T6b.*
* Proofs are bound to `ctx` and do not transfer across hands. — *(a) compiled,
  `probe-cryptodoc` `[1]`.*
* ziffle's Pedersen generators are soundly derived, with no known discrete-log
  relation to `G`. — *(b) source.*
* ristretto255 is faster and better assured than secp256k1 here; the choice is
  inherited from the library, not a judgement that secp256k1 is better. — *(a)
  compiled.*

### OPEN QUESTIONS

**OQ-1 — Is ziffle's Bayer–Groth implementation actually *sound*?** *(the blocking
one)*
We verified that it rejects 13 specific attacks. That is emphatically not soundness. A
subtly wrong exponent or a missing check can leave a system that accepts all honest
proofs and rejects all naive attacks while still admitting a clever forgery. Nothing
done so far rules that out, and ziffle is a single-author crate with 3 stars, 8
commits, one release, 293 downloads and no audit, whose own README says *"DO NOT use
this library to play for non-trivial amounts of money."*
*What would settle it:* a line-by-line review of `src/lib.rs` against Bayer–Groth 2012
by someone who knows the paper, focused on `MultiExpArg` and `SingleValueProductArg`,
and including a check that the `m = 1` instantiation preserves the paper's soundness
bound (§6.3). 1 779 lines makes this a few days, not a research project. **This review
is a prerequisite for shipping the mental-poker layer, and an absolute gate before any
real-money use.**

**OQ-2 — Is the forked Fiat–Shamir transcript safe?**
`ShuffleProof::new` clones the transcript and derives the multi-exponentiation and
product arguments from the same state, separated only by DSTs, and the author flags
the fork in a comment. Distinct domain separation is the standard defence and this is
probably fine, but this is exactly where weak-Fiat–Shamir ("Frozen Heart") bugs live,
and a bespoke SHA-256 transcript rather than merlin raises the stakes.
*What would settle it:* the OQ-1 review, plus a deliberate attempt to construct a
proof valid under one sub-argument's challenges and invalid under the other's.

**OQ-3 — Is our `ctx` binding sufficient against every replay?**
The canonical form is settled: seven fields, hashed with the length-prefixed
domain-separated constructor under `"p2p-poker v1 deck-ctx"`, normative in
`PROTOCOL.md` §4.5 and, under **D-011 rule 1**, written **nowhere else**. *(Two copies
used to sit in §6.4 and a third, reflowed, sat here; the reflowed one had already
drifted, having lost the `[ … ]` brackets around `u8(shuffle_round)`. All three are
deleted rather than repaired: one reproduction of a byte-exact construction is a
divergence waiting to happen, and two side by side are the divergence plus a
re-verification chore every pass. This document owns nothing of `ctx` but the argument
for why its fields are the right ones.)* **What is not
settled is whether that binding is sufficient**, which is a
different question and stays open. It is only as strong as our own construction, and
`ctx` is *our* bug to make, not ziffle's. In particular the DLEQ challenge does not bind
card index or street, so token replay within a hand is prevented by our envelope alone.
*What would settle it:* an adversarial test suite that replays (i) a shuffle proof
across `hand_id`, (ii) across `shuffle_round`, (iii) across `table_id`,
(iv) across `session_id`, (v) a reveal token onto a different card index in the
same hand, (vi) a reveal token published one street early — each asserting rejection.
Item (i) is already proven (`probe-cryptodoc` `[1]`); the rest are not.

**OQ-4 — Timing side channels.**
We measured throughput, not constant-time behaviour. arkworks is not written with
curve25519-dalek's constant-time discipline, and a timing channel on the secret
permutation or on `sk_i` during share generation would matter for a networked game.
*What would settle it:* `dudect`-style analysis of `shuffle_deck` and `reveal_token`,
**or** an explicit decision in `THREAT_MODEL.md` that remote timing attacks are out of
scope. That decision is defensible for play money; it is not for real money.

**OQ-5 — Hostile input to the arkworks deserialisers.**
An honest round-trip with `Validate::Yes` was verified; malformed input was **not**
fuzzed, and `SPEC_CS.md` §27 requires it. Note also the `assert!` panic path in
ziffle's `Transcript::update_with_serialized` when a serialised element exceeds its
256-byte buffer — unreachable for 33-byte points, but it is a panic, and panics on
network-derived data need care.
*What would settle it:* `cargo-fuzz` over `ShuffleProof`, `MaskedDeck`, `RevealToken`
and `OwnershipProof` deserialisation, plus an explicit maximum frame size at the
transport boundary. Always deserialise with `Validate::Yes`.

**OQ-6 — BLAKE3 has no public third-party security audit.**
Grepping the upstream README for "audit"/"security" returns zero hits. Its assurance
rests on the specification and the BLAKE2/ChaCha lineage, not on a published audit
report. **Verification: (c)** — fetched
`raw.githubusercontent.com/BLAKE3-team/BLAKE3/master/README.md`.
*What would settle it:* an audit appearing, or a decision to use SHA-256 for our own
hashes as well. Note the trade: SHA-2 is Merkle–Damgård and length-extendable, so
switching would mean hand-rolling HMAC or prefix conventions — the kind of own
construction §6 tells us to avoid. Recorded rather than resolved.

**OQ-7 — The group is inherited, not chosen.**
secp256k1 comes from ziffle and is ~3.4× slower than ristretto255 with a weaker
formal-verification record. A ristretto255 port of ziffle is the single
highest-leverage optimisation available. *What would settle it:* nothing needs
settling for the prototype; revisit only if hand-startup latency becomes a real
complaint or if OQ-1's review results in a fork anyway.

**OQ-8 — `rand` is in the dependency tree at three majors, and cannot be removed.**
`rand 0.8.8` enters through both `ark-std` (under ziffle) and `libp2p-autonat`;
`rand 0.9.5` and `rand 0.10.2` enter through other libp2p subtrees
(`research/INTEGRATION.md` §2, re-verified in §9). This contradicts
`CRYPTO_LIBS.md` §1.2's plan to enforce §7 through the dependency graph, and no
dependency choice open to us restores that plan. `SmallRng` does **not** stay out, and
saying so has now taken three attempts: an earlier form of this row claimed the feature
was "not enabled anywhere in the workspace", which is false at `rand 0.9.5`, where
`small_rng` is a **default** feature that `igd-next` and `yamux` take; and the
replacement risked implying it was therefore confined to 0.9.5, which is false at
`rand 0.10.2`, where `SmallRng` is exported with **no feature gate at all** (§7.2,
D-009 rule 3). `StdRng` is in the build at all three majors and is used by ziffle only
for nothing-up-my-sleeve constants. The structural guarantee is gone and cannot be
recovered, and no narrowing of the absence recovers any part of it.
*Settled, and the risk is now residual rather than open:* the source scan in
`src/security/rng.rs`
(`tests::our_own_code_uses_no_generator_but_the_os_one`) fails the build if any file
under `src/` mentions `StdRng`, `SmallRng`, `thread_rng`, `from_seed`, `seed_from_u64`
or `rand::rngs`, and it was verified to bite by injecting a violation (§7.2, §12
item 6). What remains open is only its reach, and there are now three known limits
rather than two: the scan is textual, so it catches a use and not an obfuscation; it
covers this crate's own sources and says nothing about the vendored ziffle, which is
reviewed under OQ-1 instead; and its deny-list does not yet name `Xoshiro128PlusPlus`
or `Xoshiro256PlusPlus`, which `rand 0.10.2` also exports ungated. The third is two
lines of fix and is carried as item 7 of `research/CRYPTO_LIBS.md` §11.

**OQ-9 — RUSTSEC-2024-0436 is now a runtime-tree finding.**
`paste 1.0.15` (unmaintained) arrives through `ark-ff 0.5.0`, not only through the
optional `dcbor` dev-dependency as previously assumed. It is a build-time proc macro
and ships in no binary. *What would settle it:* arkworks dropping `paste`, or an
accepted, narrowly-scoped and correctly-commented `deny.toml` ignore.

**OQ-10 — Licence of the project itself.**
Still open in `DECISIONS.md`. It does not block the cryptography — every crate in the
chosen set is permissive (MIT/Apache-2.0/BSD/BlueOak/CC0) — but it did eliminate
GPL-3.0-only candidates from consideration in §3, and that reasoning should be
revisited if the project chooses a copyleft licence after all.

**OQ-11 — The Linux key-slot path has never been compiled.**
Only the Windows target is installed here. Must be verified on a Linux host before
Phase 7. Do not treat the `0600` code as proven.

### What the cryptography does **not** solve (`SPEC_CS.md` §18)

Stated here because §18 forbids claiming otherwise, and because several of these look
like cryptography problems and are not:

* **Collusion out of band.** Two players sharing hole cards over a phone call is
  invisible to the protocol. Every player legitimately holds their own cards; sharing
  them is outside the protocol entirely.
* **Malware, screen sharing, physical coercion** on a player's own machine — the
  endpoint holds its own plaintext by design.
* **Multi-accounting / Sybil.** Nothing here establishes that two identities are
  different humans. That needs an identity or reputation layer this project does not
  have.
* **Denial of service, including the abort attack of §2.10.** A silent player always
  forces a hand to abort. Detected, never prevented — and under **D-010** never
  punished: the abort is neutral, stacks return to their start-of-hand values, and the
  transcript records the missing contribution. **Since D-015 the failing peer is never
  *named*, at any `|V|`**: the clause that stood here made naming depend on whether a
  certificate could form at `|V| >= 2`, and none forms. Every such hand ends on the
  `hand_deadline_ms` timer with `attributed = []` and every stack restored, so neither
  the chip outcome nor the naming depends on `|V|` any more — which is one fewer thing
  for this bullet to be wrong about, and one less signed artefact for a human to read
  (`THREAT_MODEL.md` §9.1.0 item 5(b)).
* **Escaping a losing pot by going silent — the rage-quit escape.** This is *not*
  solved, at any table size, and D-010 reopened it deliberately. A player about to lose
  a big pot can stop publishing tokens and get their chips back. The alternative,
  forfeiture, was measured over four adversarial passes and repeatedly took chips from
  *honest* peers instead; that is the worse failure, so this one is accepted and
  recorded rather than hidden (`SPEC_CS.md` §18, D-010's cost section). The only
  mitigations are social: a visible, permanent transcript record, a per-identity abort
  count in the lobby, and a user's own decision not to sit with a repeat aborter.
* **Traffic analysis**, made materially worse by relaying (D-001): a relay operator
  learns who talks to whom, when, and how much.
* **Nothing above is fixed by making the cryptography stronger.** They belong in
  `THREAT_MODEL.md`, classified honestly as out of scope.

---

## 12. Implementation obligations arising from this document

Numbered so `PROTOCOL.md`, `STATE_MACHINE.md` and the test suites can reference them.

**Every change to anything enumerated in this section is a security-critical change
under `SPEC_CS.md` §31 and requires a reproducing regression test to land first. See
`docs/CONTRIBUTING.md`**, which owns `SPEC_CS.md` §31 in full — small logical commits;
no deletion or rewrite of large parts of a working implementation without written
justification; and the load-bearing rule restated above, that **a regression test
reproducing the problem must land before any security-critical change**. That rule is
repeated here rather than only linked, so that this list stays usable if the pointer
ever breaks; `docs/CONTRIBUTING.md` is where the process it belongs to is specified,
and `PROTOCOL.md` §9.6 carries the same sentence for its own list.

0. **A proof verdict that can cost a seat is produced only by a verifier that ran.**
   §8.1.5: `invalid` is evidence; *could not verify* — a decode failure, a truncated
   body, a resource failure, a caught panic, a missing input — is **not**, and must
   reject or defer rather than produce evidence. The three tier-1 clauses D-014 rests on
   are this document's (§8.1), and this is the obligation their call sites carry.
   Numbered `0` so the existing numbering is not disturbed, on the rule
   `STATE_MACHINE.md` §9.3 uses for its conditions `0.5` and `0.6`.
1. Vendor `ziffle 0.1.0` at `vendor/ziffle/`, `[patch.crates.io]`, `Cargo.lock`
   committed. Complete the OQ-1 review before the mental-poker layer is considered
   done.
2. Wrap every ziffle type behind `DeckCrypto` (§9). No other module names a ziffle
   type.
3. Build `ctx` exactly as `PROTOCOL.md` §4.5 specifies — that section is the only
   normative statement of it (D-011 rule 1) — from signed state only, never from a
   wire parameter. §6.4 carries the reasoning, not the layout.
4. Run shuffle proving and verification on a worker thread (§6.5, spec §33).
5. Reject, and attribute, any reveal token that is early, for an index not due at the
   current stage, or published by a seat for its own hole index before
   `SHOWDOWN_REVEAL` (§2.7, §2.8). **Rejection is the enforcement; attribution is a
   transcript record with no automatic consequence.** No code path may take a chip
   from, or unseat, the attributed peer (D-010). An implementation that computes a
   forfeiture or an eviction from any proof, certificate or attribution in this
   document is wrong, and the adversarial suite should assert the neutrality directly:
   after any abort, every stack equals its start-of-hand value.
6. **Done** — the ban on `StdRng`, `SmallRng`, `thread_rng`, `from_seed`,
   `seed_from_u64` and `rand::rngs` in our own crates (OQ-8) is enforced by
   `src/security/rng.rs`, whose test
   `tests::our_own_code_uses_no_generator_but_the_os_one` scans every `.rs` file under
   `src/` on each test run and fails the build on a hit; `rng.rs` is the single exempt
   file, since it names the identifiers in order to forbid them. That deny-list is the
   normative one and supersedes the earlier draft's, which named `test_rng` and omitted
   `seed_from_u64` and `rand::rngs`. What it enforces is the discipline quoted in §7.2
   from `research/INTEGRATION.md` §2: *"Our own code draws cryptographic randomness
   only from `getrandom::SysRng`."* It is scoped to our own crates because the tree
   carries three `rand` majors and always will, with `SmallRng` compiled in at
   `rand 0.9.5` (a default feature) **and at `rand 0.10.2` (ungated)** — the absence is
   not available to be asserted at any width (D-009 rule 3). The deny-list should gain
   `Xoshiro128PlusPlus` and `Xoshiro256PlusPlus`, which 0.10.2 exports ungated
   alongside `SmallRng` and which the current list does not name (OQ-8).
   **Verification: (a) executed** — the test passes clean, and was verified to fail on
   an injected violation (§7.2).
7. Deserialise every arkworks wire object with `Validate::Yes`, and impose an explicit
   maximum frame size at the transport boundary (OQ-5).
8. Port the research probes into `tests/adversarial/` as permanent regression tests:
   T1, T3, T4, T6b, T7a, T7b, T7d, T7e, T8, plus `probe-cryptodoc`'s `ctx` replay
   test, plus the six replay cases of OQ-3. This list is **subordinate to
   `THREAT_MODEL.md` §5.5**, which maps all eleven of `SPEC_CS.md` §25's named
   malicious peers to catalogue rows, test modules and asserted outcomes. §5.5 is the
   one home for that map; the list here is the library-level subset of it.
9. `deny.toml`: `multiple-versions = "warn"`; licence allow-list including
   `BlueOak-1.0.0`, `CC0-1.0`, `MIT-0`, `BSD-*`, `Unicode-3.0`, `Apache-2.0 WITH
   LLVM-exception`, and a clarification for `ark-std`'s `"MIT/Apache-2.0"`; the
   RUSTSEC-2024-0436 ignore, commented as a **runtime**-tree proc-macro finding.
10. `cargo install cargo-audit --locked` — the plain install fails on rustc 1.95.0
    (`kstring@2.0.4 requires rustc 1.96.0`). Document it in the contributor README and
    use `--locked` in CI. **Verification: (a) executed.**
11. Never log anything in §10.2; enforce with redacting newtypes.
12. Never expose serialisation for the per-hand deck secret key, even though
    `ziffle::SecretKey` provides it (§10.1).

---

## 13. Verification artefacts

New probe written for this document:

```
<scratchpad>/probe-cryptodoc/     ziffle 0.1.0 driven by an OS-CSPRNG adapter:
                                  keygen -> aggregate key -> two chained shuffles ->
                                  ctx replay rejection -> DLEQ tokens -> n-of-n reveal
```

where `<scratchpad>` is
`~/AppData/Local/Temp/claude/<session>/<session-id>/scratchpad`.

`cargo run --release`, rustc 1.95.0, x86_64-pc-windows-msvc, verbatim output:

```
[1] proof bound to ctx, replay into another hand_id rejected: true
[2] one share alone opens the card: None
[3] n-of-n shares open the card:    Some(24)
[4] sizes: PublicKey=33 OwnershipProof=65 RevealToken=33 RevealTokenProof=98 MaskedDeck<52>=3432
```

`cargo audit` on the same crate: 52 crate dependencies scanned, exit 0, one allowed
warning (`paste 1.0.15`, RUSTSEC-2024-0436).

Probes inherited from Phase 0 research and relied on above:

```
<scratchpad>/probe-mentalpoker/   ristretto255 ElGamal, group benchmarks, cut-and-choose costs
<scratchpad>/probe-ziffle/        ziffle benchmarks + 13 adversarial tests + a negative compile test
<scratchpad>/probe-crypto-final/  the general crypto dependency block, 8 checks, all passing
<scratchpad>/probe-mp-old/        mental-poker 0.1.0 build failure
<scratchpad>/probe-zshuffle/      zshuffle 0.1.2 build + forked-arkworks evidence
<scratchpad>/probe-curdle/        curdleproofs 0.0.1 build (arkworks 0.3.0)
<scratchpad>/gx-mental-poker/     geometryxyz/mental-poker clone + bench52.rs
```

Source read directly for this document, all under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`:

```
ziffle-0.1.0/src/lib.rs           complete: Transcript, OwnershipProof, RevealTokenProof,
                                  MaskedCard, MaskedDeck, ShuffleProof, Shuffle, open_deck,
                                  remask_card, shuffle_remask_prove, the serde macros
ark-secp256k1-0.5.0/src/fields/fr.rs      scalar field modulus
ark-secp256k1-0.5.0/src/curves/mod.rs     ScalarField association
ark-ec-0.5.0/src/models/short_weierstrass/group.rs   the point-sampling routine
ark-std-0.5.0/Cargo.toml          rand 0.8 with std_rng, default-features = false
rand-0.8.8/Cargo.toml             small_rng is a separate, unenabled feature -- at 0.8 only
rand-0.9.5/Cargo.toml             l. 66-72: small_rng and thread_rng are DEFAULT features
rand-0.10.2/Cargo.toml            l. 58-74: no small_rng feature exists at 0.10
rand-0.10.2/src/rngs/mod.rs       l. 97-110: SmallRng and both Xoshiro types exported
                                  UNGATED -- the feature is gone because the type is
                                  no longer optional, not because the type is gone
rand-0.9.5/src/rngs/mod.rs        l. 87, 102: #[cfg(feature = "small_rng")] on both
igd-next-0.16.2/Cargo.toml        l. 148: rand "0.9.0", defaults taken -> small_rng on
yamux-0.13.10/Cargo.toml          l. 57:  rand "0.9.0", defaults taken -> small_rng on
hickory-proto-0.25.2/Cargo.toml   l. 318: rand "0.9", default-features = false
hickory-resolver-0.25.2/Cargo.toml l. 217: rand "0.9", default-features = false
getrandom-0.4.3/src/lib.rs, src/sys_rng.rs           fill(), SysRng
libp2p-relay-0.21.1/src/copy_future.rs   one bytes_sent counter, both directions (§6.5)
libp2p-autonat-0.15.0/Cargo.toml         `[dependencies.rand] version = "0.8"` (§9)
```

---

## 14. Cross-references

| Document | Direction and content |
|---|---|
| `THREAT_MODEL.md` | **carries from here:** §11's OQ list; the §11 "does not solve" list; the abort attack of §2.10 in its **D-010** form — the abort is neutral, stacks are restored whatever `|V|` is, attribution is evidence with no automatic consequence, and the **rage-quit escape is reopened and unsolved**; the `|V|`-scoped question of whether a peer is *named* (never of what it pays); relay metadata exposure from D-001; the §9.1/§9.2 supply-chain findings. **This document points at it for:** the §25 cheater-to-test map (`THREAT_MODEL.md` §5.5) and the deviation register (`THREAT_MODEL.md` §9.1), which own those two lists |
| `PROTOCOL.md` | **owns, and this document references by section number and does not reproduce (D-011 rule 1):** the `ctx` construction — `PROTOCOL.md` §4.5, pointed at from §6.4, §2.1, §11 OQ-3 and §12 item 3; the deck-index map and the no-burn rule — `PROTOCOL.md` §4.5, pointed at from §2.4; the hash constructor `h` and the domain-string register — `PROTOCOL.md` §2.8, pointed at from §6.4 and §7.3; the `DOMAIN_EVENT` signature prefix bytes — `PROTOCOL.md` §13, pointed at from §5.2 item 6 and §6.4; and, added in this pass as the disposition of **K-6**, the RNG beacon's `commitment_i` and `seed` constructions — `PROTOCOL.md` §4.4, pointed at from §7.3. **This is a change of discipline, not of content.** Until this pass the row read *"owns, and this document reproduces"*, and §6.4 carried the `ctx` block twice on one screen — once as a reproduction and once quoted from §4.5 to prove the two agreed. Every copy is deleted; the byte-identity claims that policed them are deleted with the copies, because there is nothing left to compare. What survives here is the *argument*: why `ctx` must carry what ziffle's transcript leaves unbound (§6.4), why the map must be fixed before the shuffle chain (§2.4), and why length prefixing is load-bearing (§6.4). If a construction and this document's reasoning about it ever disagree, `PROTOCOL.md` has the construction. **K-6, and why it is the same defect as the `ctx` blocks one pass later.** Two code blocks survived the D-011 sweep in §7.3, and both documents claimed to be canonical for them. The sweep deleted §6.4's `ctx` blocks and missed these, which is the ordinary way a sweep fails: it is run against the *finding* that prompted it rather than against the rule. **They were byte-identical when found**, and that is the argument for deleting them rather than the argument for keeping them — every copy this corpus has lost was byte-identical up to the pass in which it was not. **Carries from here:** the signed envelope fields that must bind proofs and tokens; the entitlement and street-gating rules (§2.7, §2.8); the *reasoning* for the five-part `RNG_COMMIT` binding and the attack each part stops, the construction itself now being §4.4's (§7.3); the `rand`/`SmallRng`/`StdRng` correction (§7.2), which under **D-009 rule 3** is a *discipline plus a test*, never an absence. **Owns and this document deliberately does not restate:** the equivocation predicate and its anti-replay slot key (`PROTOCOL.md` §5.2, §5.3). §6.4 and §7.1 name equivocation and defer — *"The exact predicate is `PROTOCOL.md` §5.2's and is not restated here"* — so **D-009 rule 1**'s move of the subject seat into the slot key for `TIMEOUT_VOTE` needs no mirror edit here. It could not have reached `ctx` in any case: `ctx` is the Fiat–Shamir binding for a *deck proof*, a `TIMEOUT_VOTE` carries no deck proof and no `ctx`, and the two are separately domain-separated at `PROTOCOL.md` §2.8 — `"p2p-poker v1 deck-ctx"` against `"p2p-poker v1 timeout-cert"` — so a change inside one cannot reach the other. Since this pass there is also no `ctx` block here for such a change to reach |
| `STATE_MACHINE.md` | **carries from here:** the per-hand sequence of §2.9, including the collective form of `HAND_INIT` / `HAND_COMPLETE`; the absent-seat states and abort path (D-005's requirement that the game continues and the absent seat is blinded off, which D-010 leaves untouched); the **neutral** abort of **D-010** — one chip rule for every abort, stacks restored to their start-of-hand values, no `AbortRecord` consumer that moves a chip or unseats a peer, and no eviction transition at all; the `|V| < 2` form in which the abort additionally names nobody (D-007, D-008 — the scope is the required voter set, never the seat count); deadlines as explicit state, never a wall-clock read inside the engine (D-006) |
| `NETWORK_STACK.md` | **carries from here:** the corrected per-circuit **bidirectional** byte budget of §6.5 — `2 × 8 979 = 17 958 B` per hand per circuit, both directions counted against one 131 072 B cap, giving ~7 hands shuffle-only and ~5 with the event stream, against which the **120 s duration limit is still the binding one** (D-001) — and hand traffic never crossing the lobby topic. The earlier "per circuit **per direction**" form of this row, and its ~14 hands, are withdrawn: `max_circuit_bytes` is one counter for both directions (§6.5, `NETWORK_STACK.md` §16.1). **This document points at it for:** the normative D-002 relay configuration (`NETWORK_STACK.md` §9.6) and the transport-side dependency register (`NETWORK_STACK.md` §5.1) |

---

## 15. Objections to the fix plan

Recorded per the editing rule: the rulings of `docs/research/PHASE0_FIXPLAN.md` were
applied as written, and where a ruling looks wrong it is noted here rather than
silently deviated from. Four notes.

**0. D-010 supersedes the chip rule that notes 1 and 4 below were written around, and
this document was cut down rather than patched.** `DECISIONS.md` **D-010** replaces
D-005's forfeiture arithmetic with a neutral abort, and removes automated attribution
consequences and the eviction path outright. Notes 1 and 4 are kept unedited, because
`DECISIONS.md`'s convention is that a superseded record is superseded and never edited
away — but every forfeiture sentence they argue about is now gone from the body of this
document. What changed here, and what did not:

* **Deleted.** *"the absent player's committed chips are forfeited and distributed to
  the remaining players in proportion to their own contributions"* (§2.10), and with it
  the whole two-branch chip rule that made `|V| >= 2` and `|V| < 2` pay differently.
  There is now one chip rule for every abort from every cause: restoration.
* **Weakened, deliberately.** Every *"rejected and attributed"* now says what
  attribution is — a signed line in the transcript — and says explicitly that nothing
  acts on it. §11's DoS bullet lost *"attributed"* as though attribution were a
  sanction, and gained a separate bullet stating the reopened rage-quit escape as an
  unsolved limitation.
* **Kept, and this is the part worth defending.** The `|V|` scoping of D-008 stays,
  even though a collapsed voter set can no longer win anybody's chips. It now protects
  a smaller thing — the integrity of a *name* in the transcript — but the transcript is
  the entire remaining sanction under D-010, so letting one peer manufacture a false
  attribution would hollow out the only mitigation left. Deleting the floor because the
  prize shrank would have been the wrong deletion.
* **Untouched.** Every cryptographic claim in §2 to §10. D-010 is a rule about
  consequences, and this document's cryptography never had any: an invalid proof was
  always rejected by verification, and rejection is what protects a card. Not one
  `Verification:` line changed, and no security property was strengthened to
  compensate — §11's OQ list is exactly as it was.

D-010's accepted cost is stated in §2.10 and again in §11, per `SPEC_CS.md` §18: a
losing player can stall and get their chips back, that is worse for fairness and better
for safety, and it is not solved.

**Two confirmations the pass owed, both re-run over the whole file rather than
recalled.**

* **No `n`-scoped rule survives (D-008 point 4).** Every gate on the certificate
  machinery in the body reads `|V|`: §2.8 item 2, §2.9's sequence line, §2.10's four
  bullets, §5.2 item 8 and §11's DoS bullet. The only remaining occurrences of `n = 2`
  and `n >= 3` are inside notes 1 and 4 below, which are the superseded historical
  record of how the scoping was fixed, and note 1 already carries the addendum saying
  D-008 generalised it. `n` elsewhere in this document means the number of parties to
  the cryptography (§2.1) or the deck size — never a rule's scope.
* **No below-floor certificate has an effect anywhere (D-009 rule 2).** §2.10 is the
  only place one is described and it denies the effect in full: not chained, not
  evidence, no `AbortRecord`, terminates nothing, silently ignored at every table size
  and for `kind = Crypto` as much as for anything else. §2.8 item 2, §2.9, §5.2 item 8
  and §11 were re-read and none of them grants one. Under D-010 there is also nothing
  left for such a certificate to *win* — it can no longer take a chip even in
  principle — but the floor is kept anyway, because a false name in the transcript is
  now the only harm left and the floor is what prevents it.

**1. A-1's "No other change" to this document is too narrow, and I went slightly
beyond it.** A-1 instructs this document to add *"at `n >= 3`; heads-up the deadline is
advisory (D-007)"* to §2.8 item 2 and §2.10, and to change nothing else. But §2.10's
D-005 disposition — *"a signed record naming the peer that failed to publish; the absent
player's committed chips are forfeited"* — is stated unconditionally, and under the
plan's own §0.3 it is **false at `n = 2`**, where a `kind = 2` certificate carries
`attributed = []` and produces no forfeiture. §11's "does not solve" list carried the
same unconditional *"detected and attributed"*. Leaving either would have this document
asserting exactly what A-3's edit to `THREAT_MODEL.md` X8 deletes. I therefore added the
heads-up case to §2.10 and qualified §11's bullet, citing §0.3 and OQ-A. No claim was
strengthened; two were weakened.

*Superseded in scope, not in substance.* **D-008** has since generalised the scoping
from the seat count to the size of the required voter set: every rule that weakens,
disables or gates the certificate now reads `|V| < 2` where it read `n = 2`. §2.8,
§2.9, §2.10, §5.2 item 8 and §11 are written on `|V|` accordingly. The qualification
this note argued for is unchanged — it simply now covers the cases at `n >= 3` where
`|V|` has fallen to one, which is the hole D-008 exists to close.

**2. A-8's per-document instruction and A-8's own normative table disagree, and I
followed the table.** The instruction says to keep the `n × (5547 + 3432)` figure and
relabel it *"total per-hand outbound for one peer, spread over `n-1` circuits"*. The
plan's normative table gives that same quantity as `(n-1) × 8 979 B` — 44 895 B at six
seats, not 53 874 B. `n × 8 979` counts all `n` shufflers' objects and is the
**table-wide** aggregate, which is precisely the figure whose comparison against a
per-circuit cap produced the original error. Relabelling it as a per-peer figure would
reintroduce the defect with a new name. §6.5 therefore uses `(n-1) × 8 979 B` for a
peer's own outbound; the decomposition `5 547 + 3 432 = 8 979` is kept as the
per-shuffler object size it actually is. The table's *other* half — "per circuit per
direction" — did not survive verification either; see note 3.

**3. A-8's "per circuit and per direction" is false, and the per-circuit figures in
§6.5 are recomputed on the bidirectional total.** This is the one place where a ruling
was not applied as written, because the crate contradicts it. `libp2p-relay 0.21.1`
counts a circuit's traffic in a single `bytes_sent: u64` on `CopyFuture`, incremented
by both `forward_data` calls before the comparison against `max_circuit_bytes`
(`src/copy_future.rs:47, 78, 88, 92, 98, 102`) — so the 131 072 B default is a
combined-traffic cap, not a per-direction one. `NETWORK_STACK.md` §16.1 raised this
objection independently and is right. Applying it: the circuit between two seats
carries `2 × 8 979 = 17 958 B` of shuffle per hand, the public-relay budget is
`131 072 / 17 958 ≈ 7` hands rather than ~14, and the "including the event stream"
figure halves from ~10 to ~5. The ruling's **decision** is untouched — duration binds
either way, and the D-002 split stands — so what changed is a comfort margin and a
number that four documents had begun to quote. `THREAT_MODEL.md` OQ12 must now be
stated as a bidirectional measurement; measured one way it would report twice the
headroom that exists, which is exactly how a withdrawn claim comes back.

**4. D-009's three rules, applied to this document, and what each one cost.**
`DECISIONS.md` **D-009** is a decision document and outranks the fix plan; its rules
were applied as written and not re-derived. What they changed here:

* **Rule 3 — never state a security property as an absence** (finding M3). §7.2's
  bullet *"`SmallRng` is **not** compiled in"* is **withdrawn**, and so is OQ-8's
  *"`SmallRng` stays out (feature not enabled anywhere in the workspace)"*. Both were
  false: `rand 0.9.5` carries `small_rng` among its **default** features and
  `igd-next 0.16.2` and `yamux 0.13.10` take those defaults, so `SmallRng` is in the
  binary and cannot be got out. This is the *second* time the absence was narrowed
  rather than dropped — the first pass replaced "`rand` is absent" with "`small_rng` is
  not enabled on `rand 0.8.8`", which is true of one major and was still stated over
  the whole build. What replaces it is the discipline plus the mechanism that enforces
  it: `src/security/rng.rs` and its source scan, verified to fail on an injected
  violation. No claim was strengthened; one was replaced by a weaker claim with a test
  behind it.

  *Addendum, 2026-08-28 — it happened a **third** time, and the third one was caught
  before it was written down.* Having established that `SmallRng` is in the build at
  `rand 0.9.5`, the natural next sentence is *"and `rand 0.10.2` has no `small_rng`
  feature at all"* — which is true of the feature and false of the type. Reading
  `rand-0.10.2/src/rngs/mod.rs` rather than only its `Cargo.toml` settles it: `mod
  small;` (l. 97) and `pub use self::small::SmallRng;` (l. 106) carry **no `cfg`
  attribute**, where `rand-0.9.5` gates both. The feature disappeared because the type
  became unconditional. §7.2 now states the three majors as a table rather than as a
  sentence, because a table cannot be quoted half-way; OQ-8 and §12 item 6 carry the
  same, and `research/CRYPTO_LIBS.md` §1.2.1 is the research-side home. The lesson D-009
  rule 3 draws is confirmed a third time: **a manifest is not a build, and a feature is
  not a type.**
* **Rule 2 — a below-floor certificate is inert everywhere** (finding M1). §2.10 said
  *"Such a certificate therefore has no effect: the abort … names nobody"* — asserting
  in one sentence both that nothing happens and that an abort happens. The two are now
  separate statements: the certificate does nothing at all, and the hand ends later at
  `hand_deadline_ms` on a local timer, which is the event that names nobody and
  restores stacks. Nothing else in this document gives a `|V| < 2` certificate an
  effect; §2.8 item 2, §2.9's sequence, §5.2 item 8 and §11's "does not solve" bullet
  were re-read and all four are already scoped on `|V|` and already deny the effect.
* **Rule 1 — mandatory honest behaviour may never satisfy the equivocation predicate**
  (finding M2). **No edit was needed here, and that is the point.** This document names
  equivocation twice (§7.1 on a re-shuffle, §10.1 on signature malleability) and both
  times defers to `PROTOCOL.md` §5.2 rather than restating the predicate, so moving the
  subject seat into the anti-replay slot key does not have to be mirrored. §6.4's `ctx`
  block was re-diffed against `PROTOCOL.md` §4.5 after the change and is
  **byte-identical**, field order, comments, `0xFF` sentinel and all. It could not have
  drifted: `ctx` is the Fiat–Shamir binding for a *deck proof* under
  `"p2p-poker v1 deck-ctx"`, a `TIMEOUT_VOTE` carries no deck proof and no `ctx`, and
  the certificate's own digest is domain-separated under `"p2p-poker v1 timeout-cert"`
  (`PROTOCOL.md` §2.8). Ownership working as designed is what kept the blast radius to
  one document.

  *Superseded in scope, not in substance, by D-011 — see note 5.* The reasoning above
  stands: deferring to `PROTOCOL.md` §5.2 is exactly why no mirror edit was needed, and
  that is the same principle D-011 rule 1 later made general. But the sentence *"§6.4's
  `ctx` block was re-diffed against `PROTOCOL.md` §4.5 … and is byte-identical"* no
  longer describes this document, and must not be read as though it did: **there is no
  `ctx` block in §6.4 any more.** D-011 deleted both of the copies that existed, so
  there is nothing left to re-diff and no byte-identity to re-verify each pass. The
  half of this bullet that generalised — do not restate what another document owns — is
  now the rule; the half that policed a copy is retired with the copy.

**5. D-011 and D-012, applied in the sixth pass — and the reason this note exists at
all is that the fifth pass never reached this document (finding H7).**
`DECISIONS.md` D-011 was written on 2026-08-28 and swept the corpus; this file ended
that sweep with **zero** occurrences of the string `D-011`, against 74, 35, 26 and 20
in `THREAT_MODEL.md`, `PROTOCOL.md`, `NETWORK_STACK.md` and `STATE_MACHINE.md`. The
omission was a judgement call — a decision about which document owns which concept did
not look like a cryptography change — and the judgement was wrong twice over: the
ownership table of §14 was the clearest surviving statement of the discipline D-011
abolishes, and §6.4 held the corpus's only instance of one construction printed twice
on a single screen. D-012 records this as the second such miss (`NETWORK_STACK.md` and
D-010 was the first) and makes the process rule: **every sweep covers every document**,
because one agent reading a file and reporting nothing is cheaper than a blocking
defect.

* **D-011 rule 1 — what was deleted.** Four constructions `PROTOCOL.md` owns were
  reproduced here and are now references: the `ctx` block (twice in §6.4 — the
  reproduction *and* the verbatim quotation of §4.5 that existed to prove the two
  agreed), the deck-index map table and the no-burn rule (§2.4), and the Rust body of
  the hash constructor `h` (§6.4). The byte-identity paragraphs went with them. This
  is the deletion D-011 asks for and not a weakening: **no claim in this document
  rested on holding a second copy.** What every one of those copies cost was a
  re-verification every pass, and §11 OQ-3 records that the copies had already drifted
  once, in the direction copies always drift — a reflowed restatement that had quietly
  lost the `[ … ]` brackets around `u8(shuffle_round)`.
* **D-011 rule 1 — what was kept, and why it is not a restatement.** The per-field
  rationale table in §6.4, the argument about what ziffle's transcript leaves unbound,
  the timing argument for the deck-index map in §2.4, and the `m = n` symbol note.
  These are reasoning about somebody else's construction, which is what a
  constructions document is for; each is now explicitly labelled non-authoritative
  against its owner.
* **D-011 rules 2 and 3 — nothing to do, reported rather than assumed.** Rule 2: this
  document has never written an anti-replay slot key; §6.4 and §7.1 name equivocation
  and defer to `PROTOCOL.md` §5.2 and §5.2.1. Rule 3: no automated eviction survives at
  any layer here, and it did not survive D-010 either — every `block_peer`,
  unseating and proof-driven list was gone before this pass began. Both were re-read
  in full rather than recalled.
* **D-012 — three sites, one pin made explicit.** The rule is that nothing entering a
  hash, a chain or the next hand's genesis may come from a quantity that can differ
  between honest receivers. Swept:
  1. **The key set (§2.1), and this is the sharp one.** `apk = Σ pk_i` is appended to
     ziffle's Fiat–Shamir transcript under the label `"apk"`, so *who is in the key
     set* is a hashed quantity. §2.10's line *"from the next hand the absent seat is
     simply not in `apk`"* was safe only while every peer agreed on which seat that
     was, and finding H1 shows they need not: two honest peers can accept different
     copies of the same abort, one naming a peer and one naming nobody. Both sites now
     pin membership to the chained `HAND_INIT`'s `dealt_in` and say in terms that the
     `attributed` field of an abort is not an input. The failure this prevents is not
     subtle — it is every shuffle proof in the next hand failing to verify for one
     honest peer.
  2. **`ctx` (§6.4) — and this check was recorded as passing when it was failing.**
     The sweep read §4.5's seven parts, found each of them a protocol constant or a
     value fixed by a chained event, and wrote item 1 to say so. The property is
     **transitive** and the reading was not: `session_id` contained `advert_hash`, and
     `advert_hash` was whichever copy of the founder's re-broadcast a joiner happened
     to hold. One hop past the field list was a per-receiver quantity, and no reader of
     the field list could have seen it. Closed by **D-013**'s J1 rule, which removed
     `advert_hash` from `GENESIS(0)`, `session_id` and `ctx`; the correction and the
     obligation it leaves behind — re-check whenever any part's own definition changes
     — are at §6.4 item 1, and the failure is `THREAT_MODEL.md` **X35**. **A sweep that
     stops at the field list of the thing it is checking is not a sweep**, and that,
     rather than the missing field, is what this entry is kept for.
  3. **The RNG beacon seed (§7.3).** Clean already, by point 4's rule that every
     commitment is chained before any reveal is accepted, but the reason is now stated
     as a D-012 reason: the combine runs over the chained committer set, never over the
     reveals a given peer happened to receive, and a missing reveal stops the setup
     rather than shortening the list.

  No fourth site exists. The deck-index map (`PROTOCOL.md` §4.5) and `index_map_hash`
  derive from `dealt_in` and `button_position` and are covered by site 1's pin; the
  `ctx`, `event_hash` and `state_hash` inputs are `PROTOCOL.md`'s to police and this
  document no longer holds a copy of any of them. `seat_flags`, the other half of the
  D-012 pair (finding H2), never appeared in this document at any point — checked, not
  assumed.
* **D-013 — nothing owed on the wire, two things owed on the record, and one
  observation this document is the only one positioned to make.**
  1. **The record (`J-5`).** §6.4 item 1 and note 5's site 2 above, both corrected
     rather than quietly rewritten, for the reason D-009 rule 3 gives about absences:
     a claim that reads as a discharged check will not be re-run.
  2. **The ownership (`K-6`).** §7.3's two code blocks are deleted in favour of
     `PROTOCOL.md` §4.4; see the D-011 bullet at the head of this document.
  3. **Site 1 of note 5 now rests on something D-013 made load-bearing, and it is not
     yet agreed.** The `apk` pin says membership of the joint key comes from the
     chained `HAND_INIT`'s `dealt_in`, which was the safe answer under D-012 because
     `HAND_INIT` is a collective stage whose bodies must be byte-identical. Under
     D-013 `dealt_in` is constrained by the required emitter set of the previous hand,
     and `DECISIONS.md` **K-1** holds that that set can differ between honest peers on
     the abort path. The pin is therefore exactly as agreed as `HAND_INIT` is, and no
     more — which is a statement about `PROTOCOL.md` §3.2, not about this document, and
     is why it is recorded here rather than fixed here.

     **The observation, which cuts the other way and is worth having.** If two peers
     ever disagreed about `dealt_in` **while a deck was being shuffled**, this layer
     would catch it immediately and unmistakably: `apk` is appended to ziffle's
     Fiat–Shamir transcript under the label `"apk"`, so a differing key set means a
     differing challenge, and **every shuffle proof in that hand fails to verify for
     the other peer**. That is the loudest failure the corpus can produce. **It never
     fires on the path K-1 describes**, because that path is the drain hand: with one
     dealt-in seat the state machine goes straight to settlement, so there is no
     `DECK_COMMIT`, no joint key, no shuffle chain and no proof to fail. The
     cryptography that would have made the fork loud is bypassed precisely where the
     fork happens — the same shape as `K-3`'s finding about checkpoints, arrived at
     from a different layer, and an independent reason to think K-3's cheap fix is
     worth taking on its own merits.
* **Nothing was strengthened to pay for any of this.** Every OPEN QUESTION in §11
  still asks what it asked, the "does not solve" list is untouched, and not one
  `Verification:` line in §2 to §10 changed. The only edit inside §11 is OQ-3's
  parenthetical, which now says the `ctx` construction lives at one site rather than
  two, and that is a weaker statement about this document rather than a stronger one
  about the protocol — the question OQ-3 asks, *whether that binding is sufficient*,
  is verbatim. D-011 is a rule about where a sentence lives and D-012 a rule about
  what may feed a hash; neither makes a cryptographic claim safer, and this pass did
  not pretend otherwise.
