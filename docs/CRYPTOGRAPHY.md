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

---

## 1. Summary table

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
* Keys are **fresh per hand**, not long-lived. `ctx` (§6.4) contains `hand_id`, so a
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
the signed `HAND_INIT` event *before* the first shuffle:

```
index 0 .. 2n-1 : hole cards, dealt seat-by-seat in two passes, as at a live table
index 2n        : burn
index 2n+1 .. 2n+3 : flop
index 2n+4      : burn
index 2n+5      : turn
index 2n+6      : burn
index 2n+7      : river
```

(The exact layout is `PROTOCOL.md`'s to state; what matters cryptographically is only
that it is fixed and signed before any shuffle happens.)

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

Alice's hole cards are the deck indices assigned to her seat by the §2.4 map. To let
*only* Alice read them, every **other** player sends Alice their token for exactly
those two indices, privately, over the direct authenticated libp2p stream to Alice.
Alice adds her own share and completes the sum.

```
for j in hole_indices(alice):
    each i ≠ alice  → alice :  (share_i(j), dleq_i(j))       // point-to-point, signed
    alice: verifies every DLEQ, adds share_alice(j), looks up m
```

Properties:

* Alice needs `n−1` tokens plus her own; she has them, so she reads her cards.
* Bob receives no token for Alice's indices from anybody, so for Bob those two
  ciphertexts stay ciphertexts. He is missing at least Alice's own share and cannot
  produce it, since producing it is equivalent to computing `sk_alice` from
  `pk_alice`.
* **A modified client does not help.** Bob's client never receives the material needed
  to decrypt Alice's cards; there is nothing hidden in his memory to un-hide. This is
  the property `SPEC_CS.md` §9 demands and the one the adversarial test
  `CheaterReadOpponentCard` (§25) must assert.

**Threat to guard at our layer:** ziffle has no notion of *who is entitled to* a
token. A player who sends Bob a token for Alice's index is doing something ziffle
considers perfectly valid. Our envelope therefore binds each token to
`(table_id, hand_id, street, card_index, recipient)`, and a receiver **rejects** any
token for an index it is not entitled to, attributing the violation to the sender. See
§9 item 4.

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
   shuffle proof.
2. Publishing a token is an **automatic client action**, never a human decision. This
   is the load-bearing observation behind **D-006**: a player who walks away from the
   keyboard still cooperates cryptographically, so the board opens on schedule, the
   showdown works, and the hand plays to the end. The only thing missing is a betting
   decision, and the answer to that is an auto check/fold via a timeout certificate —
   never an abort, and never anything that ends the tournament.
3. Burn cards are simply never opened. Their indices are consumed by the map and no
   token is ever published for them, in the hand or afterwards.

### 2.9 The complete per-hand sequence

```
HAND_INIT            table_id, hand_id, button, seat order, dealing map, ctx      (signed)
  ↓
KEY_SETUP            each player: (sk_i, pk_i, OwnershipProof) ; verify all       (65 B each)
                     apk = Σ pk_i                                                  (33 B each)
  ↓
SHUFFLE_STEP × n     player k: D_k + ShuffleProof<52>, everyone verifies       (5547 + 3432 B)
  ↓   any proof fails → INVALID_SHUFFLE_PROOF, hand stops, shuffler attributed
DECK_COMMIT          hash of the final verified deck enters the transcript
  ↓
DEAL_PRIVATE         n−1 tokens per hole index, point-to-point           (131 B per token)
  ↓
betting …            auto check/fold on a timeout certificate (D-006)
FLOP_REVEAL          tokens for 3 indices, broadcast
betting … TURN_REVEAL … betting … RIVER_REVEAL … betting …
  ↓
SHOWDOWN_REVEAL      tokens for the hole indices that must be shown
  ↓
HAND_COMPLETE        winner, pot, side pots; transcript closed
```

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

Per **D-005** and `SPEC_CS.md` §19:

* If the hand can still be decided without opening anything — everyone else folds to
  one player — it completes normally. No tokens are needed, so a player who quits to
  escape a loss does not escape if the others simply fold.
* Otherwise the hand **aborts**, with a signed record naming the peer that failed to
  publish. The absent player's committed chips are forfeited and distributed to the
  remaining players in proportion to their own contributions. Restoring stacks would
  hand every player a free escape from a losing pot, which is an in-protocol exploit
  available to anyone; forfeiture closes it, at the cost of a documented DoS incentive
  that the threat model classifies as out of scope, not solved.
* From the **next** hand the absent seat is simply not in `apk` (§2.1). Nothing waits
  for it.

**The upgrade we must refuse.** The textbook fix for this is `t`-of-`n` threshold
ElGamal with Feldman or Pedersen VSS, so a quorum can finish without the missing
player. We do **not** do this: with `t < n`, any `t` colluding players can decrypt
*every* hole card at the table. That is precisely the trade `SPEC_CS.md` §19 forbids
and it breaks the §35 main invariant. n-of-n stands; abort and attribute.

Recorded honestly: **a malicious player can always force a hand to abort by going
silent.** It cannot steal cards or chips by doing so, but it is a griefing vector, and
the mitigations are social (visible attribution, repeated-abort reputation), not
cryptographic.

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
| 3 | **The RNG commit/reveal beacon** for seating and the initial button (§7.3) | A hash commitment `H(r_i ‖ salt_i)` using BLAKE3's keyed mode, then `seed = H(r_1 ‖ … ‖ r_n)`. Textbook commit-and-reveal over a library hash. Not used for the deck — the shuffle chain handles that |
| 4 | **The deterministic index → recipient map** (§2.4) | Pure bookkeeping over integers. No randomness, so nothing to attack |
| 5 | **Street gating and entitlement checks** on reveal tokens (§2.7, §2.8) | Policy: *when* and *to whom* a legitimate library operation may be applied. It adds no primitive; it constrains one |
| 6 | **The signed, hash-chained event envelope** (§12/§13 of the spec) | Deterministic CBOR + Ed25519 + BLAKE3, all library primitives, composed in the standard way: length-prefixed, domain-separated, `previous_event_hash` chained |
| 7 | **The `DeckCrypto` trait boundary** (§9) | A Rust trait. No cryptographic content at all; it exists so ziffle can be swapped |
| 8 | **Timeout certificates** (D-006) | `k`-of-`k` Ed25519 signatures over a canonical CBOR body naming seat, sequence and `previous_event_hash`. A multi-signature by concatenation, not an aggregate signature scheme — no new algebra |
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
exactly what the *product* half of the argument enforces.

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

The binding rule, normative:

```
ctx = "p2ppoker/v1" ‖ 0x00 ‖ protocol_version ‖ table_id ‖ session_nonce
                    ‖ hand_id ‖ shuffle_round ‖ shuffler_application_pubkey
```

built with the same length-prefixed, domain-separated encoder used for the transcript
hash, so that no two distinct field tuples can produce the same `ctx` bytes:

```rust
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

**Verification: (a) compiled and ran** — `probe-crypto-final` step `[5]` asserts
`transcript_hash(D, ["AB","C"]) != transcript_hash(D, ["A","BC"])` (length prefixing
works) and that two domains separate (`docs/research/CRYPTO_LIBS.md` §3.3).

Each field's role:

| Field | Prevents |
|---|---|
| `protocol_version` | replay across incompatible protocol revisions |
| `table_id` | replay of a proof from another table |
| `session_nonce` | replay from an earlier session at the *same* `table_id` |
| `hand_id` | replay of last hand's shuffle into this hand |
| `shuffle_round` | replaying shuffle 1's proof as shuffle 3's |
| `shuffler_application_pubkey` | one player presenting another's proof as their own |

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
   a `ctx` taken from the wire would let an attacker choose the domain.
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
GossipSub join. Inside the "a hand starts in a couple of seconds" budget.

**Two consequences that are requirements, not observations:**

1. **`SPEC_CS.md` §33: proving and verifying must run on a worker thread.** At ~100 ms
   per proof this would visibly freeze a GUI event loop. Results reach the UI thread as
   messages.
2. **Bandwidth interacts with D-001.** Shuffle traffic alone is `n × (5547 + 3432)`
   bytes per hand: **18 KB heads-up, 54 KB six-handed**, before the signed event
   stream. A *public* Circuit Relay v2 circuit is capped at **128 KiB and 2 minutes**
   by default in both kubo and rust-libp2p (D-001 addendum), so a public relay carries
   on the order of **two six-handed hands** before it resets. This is concrete evidence
   for D-002's split: public relays are fine as hole-punch rendezvous, and cannot carry
   a session. Note also that a `ShuffleProof<52>` plus deck is 8 979 bytes, comfortably
   under GossipSub's 64 KiB `max_transmit_size` — but hand traffic goes over direct
   libp2p streams to table participants, never over the lobby topic, per `SPEC_CS.md`
   §1.

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
  same `(hand_id, round)`, which is an equivocation: two conflicting signed events for
  one sequence number, detectable and attributable per `SPEC_CS.md` §14.

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

Two mitigating facts and one required action:

* `SmallRng` is **not** compiled in: it lives behind rand 0.8's separate `small_rng`
  feature, which ark-std does not enable. **Verification: (b) source** —
  `rand-0.8.8/Cargo.toml` `[features]`, `small_rng = []`.
* `StdRng` *is* present, and ziffle uses it deliberately and correctly for
  *deterministic public constants only* — the Pedersen generators and the 52 open-deck
  points, each seeded from a fixed SHA-256 of a public label. That is a
  nothing-up-my-sleeve derivation, not a source of protocol randomness.
* **Required:** since the dependency graph no longer enforces §7, a lint must. Add a
  CI check (a `clippy.toml` disallowed-type entry or a grep test) that fails the build
  if our own crates mention `StdRng`, `SmallRng`, `thread_rng`, `from_seed` or
  `test_rng` outside the vendored ziffle. See §12 item 6.

### 7.3 The commit/reveal beacon — where it *is* needed

`SPEC_CS.md` §16 names `RNG_COMMIT` and `RNG_REVEAL`, and §7 asks for the mechanism.
It is needed for the **non-deck** randomness: seat assignment at table start, and the
initial button position. (Subsequent buttons rotate deterministically; only the first
needs randomness.)

```
commit phase:  each i draws r_i, salt_i from the OS CSPRNG
               publishes C_i = BLAKE3_keyed(derive_key("p2p-poker v1 rng-commit"),
                                            len‖table_id ‖ len‖session_nonce
                                            ‖ len‖peer_pubkey ‖ len‖r_i ‖ len‖salt_i)
reveal phase:  each i publishes (r_i, salt_i); everyone recomputes C_i and checks
combine:       seed = BLAKE3_keyed(derive_key("p2p-poker v1 rng-seed"),
                                   len‖r_1 ‖ … ‖ len‖r_n)     // fixed peer-key order
```

**Why a player cannot change its contribution after seeing the others':**

1. The commitment is **binding**: producing a different `r_i'` with the same `C_i`
   requires a BLAKE3 collision.
2. The commitment is **hiding**: `salt_i` is 32 fresh OS-CSPRNG bytes, so `C_i` leaks
   nothing about `r_i` even though `r_i` may come from a small space.
3. The commitment binds `table_id`, `session_nonce` and the committer's public key, so
   a commitment cannot be lifted from another table, another session or another
   player.
4. **All** commitments must be published and accepted into the hash chain *before* any
   reveal is accepted. The ordering is enforced by the state machine and by
   `previous_event_hash`, not by wall-clock timing.
5. Failing to reveal after committing is a protocol failure attributable to that peer,
   handled exactly like a missing decryption share (§2.10). It is *not* an opportunity
   to re-randomise: the seed is computed over the committed set, and a non-revealer is
   excluded and named, never allowed to force a re-run.

Point 5 matters. A "last revealer" who can abort and force a fresh beacon gets to
resample the seat draw. Because this beacon decides only seating and the initial
button — never cards — the value of that attack is small, but the rule closes it
anyway.

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
3. Publish a token **only** for an index that is due (§2.7, §2.8). Publishing early is
   a protocol violation, attributed to the sender.
4. Tokens are carried inside the signed CBOR envelope, bound to
   `(table_id, hand_id, street, card_index)`, because the DLEQ itself does not bind
   them (§6.4 item 2).
5. Token publication is **automatic and never gated on the human** (D-006). Only a
   client that is gone or deliberately withholding produces the D-005 case.
6. A share for a card that is *not* opened this hand — a mucked hole card, a burn —
   is never published, never logged, and the scalar is zeroised.

**Cost:** 131 bytes per player per revealed card (33 + 98), and 1.39 ms to 4.34 ms to
open one card end to end for 2 to 6 players (§6.5).

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

**Supply-chain finding that changes `CRYPTO_LIBS.md` §4.4.** That document expected
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
ziffle binds proofs to `ctx`, and §6.4 fixes what `ctx` contains — but that is only as
strong as our own construction, and `ctx` is *our* bug to make, not ziffle's. In
particular the DLEQ challenge does not bind card index or street, so token replay
within a hand is prevented by our envelope alone.
*What would settle it:* an adversarial test suite that replays (i) a shuffle proof
across `hand_id`, (ii) across `shuffle_round`, (iii) across `table_id`,
(iv) across `session_nonce`, (v) a reveal token onto a different card index in the
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

**OQ-8 — `rand 0.8` re-enters the dependency tree via ark-std.**
This contradicts `CRYPTO_LIBS.md` §1.2's plan to enforce §7 through the dependency
graph. `SmallRng` stays out (feature not enabled) and `StdRng` is used only for
nothing-up-my-sleeve constants, but the structural guarantee is gone.
*What would settle it:* a CI lint that fails on `StdRng`/`SmallRng`/`thread_rng`/
`from_seed`/`test_rng` in our own crates. Until that lint exists, §7 is enforced by
review discipline alone.

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
  forces a hand to abort. Detected and attributed, never prevented.
* **Traffic analysis**, made materially worse by relaying (D-001): a relay operator
  learns who talks to whom, when, and how much.
* **Nothing above is fixed by making the cryptography stronger.** They belong in
  `THREAT_MODEL.md`, classified honestly as out of scope.

---

## 12. Implementation obligations arising from this document

Numbered so `PROTOCOL.md`, `STATE_MACHINE.md` and the test suites can reference them.

1. Vendor `ziffle 0.1.0` at `vendor/ziffle/`, `[patch.crates.io]`, `Cargo.lock`
   committed. Complete the OQ-1 review before the mental-poker layer is considered
   done.
2. Wrap every ziffle type behind `DeckCrypto` (§9). No other module names a ziffle
   type.
3. Build `ctx` exactly as §6.4 specifies, from signed state only, never from a wire
   parameter.
4. Run shuffle proving and verification on a worker thread (§6.5, spec §33).
5. Reject, and attribute, any reveal token that is early, for a wrong index, or for a
   recipient not entitled to it (§2.7, §2.8).
6. Add a CI lint banning `StdRng`, `SmallRng`, `thread_rng`, `from_seed` and
   `test_rng` in our own crates (OQ-8).
7. Deserialise every arkworks wire object with `Validate::Yes`, and impose an explicit
   maximum frame size at the transport boundary (OQ-5).
8. Port the research probes into `tests/adversarial/` as permanent regression tests:
   T1, T3, T4, T6b, T7a, T7b, T7d, T7e, T8, plus `probe-cryptodoc`'s `ctx` replay
   test, plus the six replay cases of OQ-3.
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
rand-0.8.8/Cargo.toml             small_rng is a separate, unenabled feature
getrandom-0.4.3/src/lib.rs, src/sys_rng.rs           fill(), SysRng
```

---

## 14. Cross-references

| Document | What it must carry from here |
|---|---|
| `THREAT_MODEL.md` | §11's OQ list; the §11 "does not solve" list; the abort attack of §2.10 with its DoS trade from D-005; relay metadata exposure from D-001 |
| `PROTOCOL.md` | the exact `ctx` construction (§6.4); the dealing map (§2.4); the signed envelope fields carrying proofs and tokens; the entitlement and street-gating rules (§2.7, §2.8); the timeout-certificate fields (D-006) |
| `STATE_MACHINE.md` | the per-hand sequence of §2.9; the absent-seat states and abort path (D-005); deadlines as explicit state, never a wall-clock read inside the engine (D-006) |
| `NETWORK_STACK.md` | the per-hand crypto byte budget of §6.5 against the 128 KiB / 2 min public-relay cap (D-001), and hand traffic never crossing the lobby topic |
```
