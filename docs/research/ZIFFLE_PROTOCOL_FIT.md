# ziffle 0.1.0 — protocol fit

Does the crate provide what our protocol actually needs?

Author: `ziffle-review` protocol-fit agent
Date: 2026-08-28
Scope: `docs/CRYPTOGRAPHY.md` in full; `docs/SPEC_CS.md` §5–§10; `docs/PROTOCOL.md` §4.5, §9.3;
`src/protocol/constants.rs`.
Companion to the correctness review of `MultiExpArg` / `SingleValueProductArg` — **this
document does not review the algebra of the Bayer–Groth argument and says nothing about
its soundness.** It reviews the fit of the crate's *interface and cost* to the game.

**Environment for every measurement.** Windows 10, x86_64-pc-windows-msvc, 24 cores,
rustc/cargo 1.95.0, `--release`, single-threaded, builds held to 19 cores.
Probe crate (outside the repo):

```
<scratch>/zr-protofit/  src/main.rs      selective opening, n-of-n, ctx binding, degenerate keys
                        src/bin/cost.rs  per-stage cost against the deadline budget
                        src/bin/shape.rs intermediate decks, key-set coupling, in-memory sizes
```

`<scratch>` =
`<scratchpad>`.
Crate source read at
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ziffle-0.1.0/src/lib.rs`.

Verification labels follow `MENTAL_POKER.md`: **(a) compiled** — I built and ran code that
exercises the claim; **(b) source** — I read the crate source. Every output block below is
reproduced verbatim from a run.

---

## 1. Verdict

| Question | Answer |
|---|---|
| Selective opening — can one position open without opening others? | **Yes**, and the API makes the wrong thing hard. §2. |
| n-of-n — does one share leak? | **No.** Every `(n−1)`-subset returns `None` at n = 2, 6, 10. §3. |
| Is `ctx` binding sufficient? | `PROTOCOL.md` §4.5's seven fields are **individually load-bearing and all seven were measured to bite**. §4. Two residuals stay open and are already named in `CRYPTOGRAPHY.md` §6.4 item 2. |
| Does the cost fit the deadline budget? | **Yes, with three orders of magnitude to spare** at the rated preset. **No, marginally**, at the legal minimum `crypto_step_timeout_ms = 1 000`. §5. |
| Anything `CRYPTOGRAPHY.md` assumes that the crate does not provide? | **Six items**, of which two are new obligations, not restatements. §6. |

**Nothing here blocks the design.** The construction fits the game. What comes out of this
review is a short list of checks our layer must add that no document currently names — the
most important being that **ziffle accepts the identity point as a player's deck public key,
together with an ownership proof anybody can forge for it** (§7 F1).

---

## 2. Selective opening

### 2.1 The API path

Opening is per **card**, never per deck. The path, with the type at each step:

```
Verified<MaskedDeck<52>>            // only a verified deck yields cards
    .get(j) -> Option<MaskedCard>   // pick exactly the index you are entitled to
MaskedCard
    .reveal_token(rng, &sk_i, pk_i, ctx) -> (RevealToken, RevealTokenProof)
RevealTokenProof
    .verify(vpk_i, token, card, ctx) -> Option<Verified<RevealToken>>
AggregateRevealToken::new(&[Verified<RevealToken>; n])
Shuffle::reveal_card(art, card) -> Option<usize>     // index into open_deck, or None
```

**Verification: (b) source** — `lib.rs` lines 655–657 (`Verified<MaskedDeck<N>>::get`),
602–614 (`MaskedCard::reveal_token`), 475–500 (`RevealTokenProof::verify`), 552–555, 1618–1624.

Three properties of that path matter to us and all three are structural, not conventions:

1. **A card comes only from a deck this peer verified itself.** `get` is implemented on
   `Verified<MaskedDeck<N>>`, not on `MaskedDeck<N>`; `Verified<T>` has a private field and
   no `CanonicalDeserialize` impl. `SPEC_CS.md` §22's "never display a cryptographically
   unverified card" is a type error rather than a review item.
2. **`MaskedCard` has no `CanonicalSerialize` at all.** A card cannot be put on the wire.
   The compiler says so:
   `error[E0277]: the trait bound MaskedCard: CanonicalSerialize is not satisfied`.
   **Verification: (a) compiled** — the failing build of an early `zr-protofit/src/main.rs`.
   The consequence is exactly what we want: a receiver *cannot* verify a DLEQ against a card
   the sender supplied. It must take the card out of its own final verified deck at the index
   the envelope names. That is the check `CRYPTOGRAPHY.md` §6.4 item 2 asks for, and the API
   forces it rather than merely permitting it.
3. **Aggregation is over `Verified<RevealToken>`**, so an unverified token cannot enter the
   sum.

### 2.2 Measured, six-handed, with the `PROTOCOL.md` §4.5 index map at `m = 6`

Seat 0's hole indices are 0 and 6; seat 1's are 1 and 7; the flop starts at `2m = 12`.

```
[A] index 0: all 6 tokens -> Some(49) ; the other 5 seats alone -> None
[A] index 6: all 6 tokens -> Some(43) ; the other 5 seats alone -> None
[A] seat 1 index 1 with only seats 0,2..5 (seat 1 withholds) -> None
[A] seat 1 index 7 with only seats 0,2..5 (seat 1 withholds) -> None
[A] board index 12, 5 of 6 tokens -> None ; 6 of 6 -> Some(51)
[A] deck.get(52) -> true          // out of range yields None
```

**Verification: (a) compiled** — `zr-protofit`, section A.

This is `CRYPTOGRAPHY.md` §2.7's counting argument realised: the five broadcast tokens for
seat 0's index do not open it, seat 0 completes the set locally, and doing so for index 0
tells nobody anything about index 6 or index 12. Opening one position is independent of every
other position because the shares are per-ciphertext.

**Board gating is ours, and the library will not help.** `deck.get(j)` accepts any `j < 52`
and `reveal_token` will mint a river token during pre-flop without complaint. That is already
`CRYPTOGRAPHY.md` §2.8 rule 1 and §12 obligation 5; this review confirms there is no
library-side alternative. **Verification: (b) source** — `get` and `reveal_token` take no
street, stage or index argument.

### 2.3 One thing the API does not distinguish: intermediate decks

`verify_shuffle` returns a `Verified<MaskedDeck<52>>` for **every** step of the chain, not
just the last one. A card from shuffler 2's output is as usable as a card from shuffler `n`'s:

```
[S] token minted on the intermediate deck verifies against that deck's card: true
[S] an intermediate deck's card opens fully if all n cooperate: Some(30)
[S] the same token checked against the FINAL deck's card at the same index: false
```

**Verification: (a) compiled** — `zr-protofit/src/bin/shape.rs`.

Why it matters: the last shuffler knows the permutation from `D_{n-1}` to `D_n`, so a card
opened in `D_{n-1}` at a known index tells the last shuffler a card at a known index of the
final deck. The third line is the defence and it is automatic — the DLEQ binds `c1`, and
`c1` differs between decks because every step re-masks, so a token minted on an intermediate
deck fails against the final deck's card at the same index. **This defence only operates if
the verifier takes the `MaskedCard` from its own final verified deck**, which §2.1 item 2
shows the API forces. Recorded as an obligation in §8 rather than a risk.

---

## 3. n-of-n: does one share leak anything?

Built the full chain, then for each seat in turn produced every share **except** that seat's
and attempted to open. All `n` subsets tested at each seat count, not just one.

```
[B] n=2:  full set -> Some(47); every (n-1)-subset opens: 0/2;  n-1 plus a duplicate -> None
[B] n=6:  full set -> Some(15); every (n-1)-subset opens: 0/6;  n-1 plus a duplicate -> None
[B] n=10: full set -> Some(31); every (n-1)-subset opens: 0/10; n-1 plus a duplicate -> None
```

**Verification: (a) compiled** — `zr-protofit`, section B. "`n-1` plus a duplicate" is the
obvious cheat of counting one player twice to reach `n` tokens; it fails, because the sum
must equal `(Σ sk_i)·c1` and a doubled share does not.

A second, sharper form of the same question — can a set of `n` *validly proved* tokens open a
card if the keys are not the `apk` key set?

```
[S] n tokens, every DLEQ valid, but one key outside apk: reveal -> None
```

**Verification: (a) compiled** — `shape.rs`.

The system is fail-closed on the token set: `reveal_card` returns `Option<usize>`, so a wrong
or incomplete set produces `None` and never a wrong card. This is `SPEC_CS.md` §9 and §35
discharged at the implementation level for the cases a test can reach. It is **not** a proof
of the DDH argument, which is literature, nor of shuffle soundness, which is the companion
review.

**A canary worth wiring in.** Given a chain of *verified* shuffles and `n` tokens whose DLEQs
all verify against exactly the `apk` key set, `reveal_card` returning `None` is impossible
unless the shuffle argument's soundness has failed — the verified chain guarantees the
plaintext is one of the 52 `open_deck` points. A `None` in that state is therefore not a
"card could not be opened" condition to retry; it is evidence of a break, and should raise a
distinct, loud, non-recoverable fault rather than an abort. §8 obligation 5.

---

## 4. The `ctx` binding

### 4.1 What `ctx` covers

`Transcript::init(user_ctx)` is literally `Self(Sha256::digest(user_ctx).into())` — the whole
of `ctx`, hashed, with no length prefix and no domain tag of its own, as the seed of the
Fiat–Shamir chain. **Verification: (b) source** — `lib.rs` lines 204–206.

Everything the challenge sees, per proof type:

| Proof | Bound by the transcript, besides `ctx` |
|---|---|
| `OwnershipProof` (Schnorr) | `pk`, `a` — DST `ziffle/DLOG/v1` |
| `RevealTokenProof` (DLEQ) | `pk`, `share`, `c1`, `t_g`, `t_c1` — DST `ziffle/DLEQ/v1` |
| `ShuffleProof` | `apk`, **the entire previous deck**, **the entire next deck**, `c_pi`, then `c_xpi` — DSTs `ziffle/BG12X/v1`, `ziffle/BG12YZ/v1` |

**Verification: (b) source** — `OwnershipProof::challenge` (302–308),
`RevealTokenProof::challenge` (428–444), `ShuffleProof::challenge_x` / `challenge_yz`
(1082–1102).

### 4.2 What `ctx` does **not** cover

Nothing else reaches the challenge. In particular the crate binds **no** protocol version,
table, session, hand, stage, street, card index, shuffle round, shuffler identity, deck size
or library version. All of it arrives only through `ctx`, and there is no secondary
freshness anywhere in the construction.

The last point deserves its own line, because it is easy to assume the chain is self-freshening
and it is not. The initial deck handed to the first shuffler is a protocol *constant*
(`initial_deck()` = `(O, open_deck[i])`), so an initial-shuffle proof has a fixed `prev`. Under
a repeated `ctx` it replays verbatim:

```
[C3] replay of hand-7 initial deck+proof at hand 7 again: true
     (the chain has no internal freshness; ctx is the only nonce)
```

**Verification: (a) compiled** — `zr-protofit`, section C3. Read this as: if `ctx` ever
repeats across two hands, an entire hand's shuffle chain replays — every proof in it,
including the chained ones, since each step's `prev` then also repeats — and with it every
reveal token, because the DLEQ binds `c1` and the ciphertexts would be identical. Cards
revealed at the first hand's showdown would be known at known positions in the second. That
is the whole argument for why `hand_id` is not decoration.

### 4.3 The composition our protocol must pass

`PROTOCOL.md` §4.5 already specifies it, and it is adequate. Restating the field list here
would violate D-011 rule 1, so this section instead reports **what each of its seven fields
was measured to defend**. Every row is a proof produced under the canonical `ctx` and offered
for verification under a `ctx` differing in that one field only, using a local reproduction of
§2.8's `h` (`blake3::derive_key("p2p-poker v1 deck-ctx", b"p2p-poker/v1")` keyed hasher,
8-byte big-endian length prefix per part).

```
[C2] initial-shuffle proof under same ctx              : true
[C2] initial-shuffle proof under protocol_version 1->2 : false
[C2] initial-shuffle proof under table_id              : false
[C2] initial-shuffle proof under session_id            : false
[C2] initial-shuffle proof under hand_id 7->8          : false
[C2] initial-shuffle proof under sequence 10->11       : false
[C2] initial-shuffle proof under shuffle_round 0->1    : false
[C2] initial-shuffle proof under sender_public_key     : false
[C1] ownership proof: own ctx true, hand_id 7->8 false
[C5] DLEQ: own ctx true, other ctx false, other card false, other player's pk false
```

**Verification: (a) compiled** — `zr-protofit`, sections C1, C2, C5. This closes
`CRYPTOGRAPHY.md` OQ-3 items (i) through (v): replay across `hand_id`, `shuffle_round`,
`table_id` and `session_id` all reject, and a reveal token offered against a different card
in the same hand rejects. Item (vi) — a token published one street early — is not a
cryptographic question at all and cannot be: see §4.4.

Two of these are worth naming for what they buy, because the reason is not obvious:

* **`sender_public_key`** is what stops a key-copy registration. Without it, Mallory could
  lift Alice's `(hand_public_key, ownership_proof)` pair straight out of her `DECK_INIT` and
  submit it as her own, since a Schnorr proof is public and `DECK_INIT`'s stated checks
  (exact lengths, on-curve, proof verifies, one entry per seat) all pass on a copy. With it:

  ```
  [S] Alice's DECK_INIT under her own ctx true; the same (pk, proof) replayed as Mallory's false
  ```

  **Verification: (a) compiled** — `shape.rs`. (The attack it prevents is a stall rather than
  a card break — Mallory could not produce reveal tokens for a key she does not hold, so the
  hand would abort — but it is a free impersonation and an attributable-looking fault pinned
  on nobody.)
* **`sequence`** is what separates the reveal stages from each other. All of `DEAL_PRIVATE`,
  `FLOP_REVEAL`, `TURN_REVEAL`, `RIVER_REVEAL` and `SHOWDOWN_REVEAL` carry
  `shuffle_round = 0xFF`, so `sequence` is the only field that differs between them. Drop it
  and a `SHOWDOWN_REVEAL` token replays as a `FLOP_REVEAL` one.

### 4.4 What is left over, and why it cannot be fixed in `ctx`

Two residuals, both already in `CRYPTOGRAPHY.md` §6.4 item 2, both confirmed here:

1. **The DLEQ binds the ciphertext, not the position.** `RevealTokenProof::challenge` takes
   `c1`, not an index. Cross-index replay *within one stage* is nevertheless blocked, because
   a re-masked deck has a distinct `c1` at every position — `[C5] other card false` measures
   it. What is genuinely unbound is the *meaning* of the index: whether index 12 is due at
   this street, and whether seat 0 is entitled to publish for index 0. No `ctx` field can
   express that, because `ctx` is chosen by the prover's own layer and a cheat would simply
   compute the "correct" `ctx` for the stage it is cheating in. **Street and entitlement
   gating is irreducibly ours**, exactly as §2.8 rule 1 says.
2. **The token is bound to the player's key but not to the key set.** `apk` is in the
   shuffle transcript; it is not in the DLEQ transcript. The system is fail-closed on this
   (`[S] one key outside apk -> None`), so the residual is a diagnosis problem, not a
   security one.

One more that is not in the corpus: **`ctx` is hashed raw**. `Transcript::init` applies no
length prefix and no domain tag, so a `ctx` built by concatenation would be ambiguous
(`table="AB",hand="C"` and `table="A",hand="BC"` collide). §4.5's construction hashes with a
length-prefixed, domain-separated `h` and hands over 32 fixed bytes, so the ambiguity cannot
arise — but the reason it cannot is `h`, never ziffle, and an implementer who "simplifies"
`ctx` to a concatenation would silently reintroduce it. The library will accept anything,
including nothing:

```
[C6] empty ctx accepted by keygen/verify: true
```

**Verification: (a) compiled** — `zr-protofit`, section C6.

---

## 5. Cost against the deadline budget

### 5.1 Raw figures, re-measured

`hand_deadline_floor_ms` allows `crypto_step_timeout_ms` per cryptographic step, and the
rated preset sets that to **30 000 ms** (`PROTOCOL.md` §13; `crypto_step_timeout_ms` is bounded
`1_000 ≤ … ≤ 120_000` by §7.2's advert rules, `PROTOCOL.md` line 5607).

| Operation | n = 2 | n = 6 | n = 10 |
|---|---:|---:|---:|
| `Shuffle::<52>::default()` (one-time) | 5.9 ms | 5.9 ms | 5.9 ms |
| keygen + verify, all `n` keys, one node | 0.77 ms | 2.96 ms | 3.81 ms |
| shuffle **prove** | 100.4 ms | 98.5 ms | 99.3 ms |
| shuffle **verify** | 43.5 ms | 42.4 ms | 42.3 ms |
| reveal token generate | 0.280 ms | 0.285 ms | 0.280 ms |
| DLEQ verify | 0.316 ms | 0.316 ms | 0.334 ms |
| aggregate `n` tokens + `open_deck` lookup | 0.005 ms | 0.007 ms | 0.008 ms |

| Object | Wire bytes | In-memory bytes |
|---|---:|---:|
| `PublicKey` | 33 | — |
| `OwnershipProof` | 65 | — |
| `RevealToken` | 33 | 72 |
| `RevealTokenProof` | 98 | 176 |
| `MaskedDeck<52>` | 3 432 | 7 488 |
| `ShuffleProof<52>` | 5 547 | 5 976 |
| `AggregatePublicKey` | 33 | — |
| `Shuffle<52>` | not serialisable | 8 832 (and `Copy`) |

**Verification: (a) compiled** — `zr-protofit/src/bin/cost.rs` and `shape.rs`. Wire sizes
reproduce `CRYPTOGRAPHY.md` §6.5 exactly; the per-shuffle timings reproduce it within noise.

### 5.2 Per crypto step, against the 30 000 ms the rated preset allows one

| Step | n = 2 | n = 6 | n = 10 | worst as % of budget |
|---|---:|---:|---:|---:|
| `SHUFFLE_STEP`/`SHUFFLE_PROOF` as the shuffler | 100.4 ms | 98.5 ms | 99.3 ms | **0.33 %** |
| the same step as a verifier | 43.5 ms | 42.4 ms | 42.3 ms | 0.15 % |
| `DEAL_PRIVATE` (emit `2(n−1)`, verify `2(n−1)²`, open my two) | 1.2 ms | 18.7 ms | 59.2 ms | 0.20 % |
| `FLOP_REVEAL` | 1.8 ms | 5.6 ms | 9.9 ms | 0.03 % |
| `TURN_REVEAL` / `RIVER_REVEAL` | 0.6 ms | 1.9 ms | 3.3 ms | 0.01 % |
| `SHOWDOWN_REVEAL`, every seat shows both cards | 2.4 ms | 22.5 ms | 65.9 ms | 0.22 % |

Whole hand, one node's own compute, against `hand_deadline_floor_ms(n, 20000, 5000, 30000, 7000)`:

| | n = 2 | n = 6 | n = 10 |
|---|---:|---:|---:|
| own crypto compute, whole hand | 151 ms | 361 ms | 622 ms |
| `hand_deadline_floor_ms` | 1 017 000 ms | 1 657 000 ms | 2 297 000 ms |
| compute as a share of the floor | 0.015 % | 0.022 % | **0.027 %** |

**Verification: (a) compiled** — `cost.rs`. The accounting differs from `CRYPTOGRAPHY.md`
§6.5's "full hand, one node's own crypto work" row (195/248/422 ms at 2/3/6): that row counts
`(2n+5)` cards each opened by generating all `n` tokens locally, which is a single node doing
everybody's work. Mine counts the real division of labour — generate my own token, verify the
other `n−1` — but adds a worst-case showdown where every seat shows. The two are the same
order and neither changes any conclusion; the figures above are the more faithful ones and
§6.5's row should be re-derived on this accounting when it is next touched.

**Answer: the measured cost fits, with roughly a factor of 300 in hand at the tightest step
and a factor of 3 700 across the whole hand.** Compute is not the binding constraint and will
not become one; the binding constraint is the sequential shuffle chain's `n` network round
trips, which `CRYPTOGRAPHY.md` §6.5 already carries as an estimate rather than a measurement.

### 5.3 Wire, against `PROTOCOL.md` §9.3's caps

| Message | Cap | Measured payload | Fits |
|---|---:|---:|---|
| `SHUFFLE_STEP` (deck only) | 8 192 | 3 432 | yes |
| `SHUFFLE_PROOF` (proof only) | 16 384 | 5 547 | yes |
| `DECK_INIT` | 256 | 98 (33 + 65) | yes |
| `DEAL_PRIVATE`, n = 10 (`2(n−1)` tokens) | 4 096 | 18 × 131 = 2 358 | yes |
| `BOARD_REVEAL` | 1 024 | 3 × 131 = 393 | yes |

Splitting deck and proof into two messages is load-bearing and correct: **8 979 B together
would exceed `SHUFFLE_STEP`'s 8 192 B cap.** The split is what keeps both inside their caps.

Whole-hand shuffle traffic, per shuffler, is 8 979 B; across a full-mesh table that is
`n(n−1) × 8 979` = 18 KB at n = 2, 269 KB at n = 6, 808 KB at n = 10. Reveal tokens add
2.9 / 21 / 56 KB. A late joiner or a dispute reviewer that re-verifies the whole chain pays
`n × 42 ms` — 87 ms at n = 2, 254 ms at n = 6, 423 ms at n = 10.

### 5.4 Where it does **not** fit: `crypto_step_timeout_ms = 1 000`

§7.2 admits `crypto_step_timeout_ms` down to 1 000 ms, and a `Custom` preset may advertise it.
At that value a `SHUFFLE_STEP` must fit, inside one second: 100 ms of proving, an 8 979 B
transfer to every peer, and 42 ms of verifying at each of them. That is 142 ms of compute on
**this** machine — a 24-core desktop. A four-times-slower client (a laptop on battery, a
mid-range phone-class CPU) is at ~570 ms of compute alone, leaving ~400 ms for a 9 KB
transfer plus a round trip, which the Circuit-Relay-v2 fallback path `SPEC_CS.md` §1 requires
for CGNAT players will not reliably meet.

This is not a library defect and not a spec contradiction — the bound is legal and the rated
preset is nowhere near it. It is a fact that belongs next to the bound: **a table advertising
`crypto_step_timeout_ms` below roughly 5 000 ms is not playable by slow clients on relayed
paths, and the failure mode is an abort attributed to a peer that did nothing wrong.** See §8
obligation 6.

---

## 6. What the crate does not provide that `CRYPTOGRAPHY.md` assumes

Four of these the corpus already names; I confirmed them and they need no action. Two are new.

**Already named, confirmed:**

1. **Street and entitlement gating** — §2.8 rule 1, §12 obligation 5. Confirmed: no API
   takes a street, stage or index.
2. **The RNG commit/reveal beacon, signatures, the hash-chain transcript, canonical CBOR** —
   §7.3, §2.9. Confirmed absent; all ours.
3. **Disconnect robustness** — §2.10. Confirmed: n-of-n is structural, and there is no
   partial-opening API of any kind.
4. **`ziffle::SecretKey` implements `CanonicalDeserialize`** — §12 obligation 12 already
   forbids exposing it. Confirmed: `impl_valid_and_serde_unit!(SecretKey)` at line 1688 makes
   the per-hand deck secret a wire-reachable type. The obligation is correct and necessary.

**Not named anywhere, and needed:**

5. **The crate exposes neither `open_deck` nor the Pedersen commitment key.** Both are
   `pub(crate)` state of `Shuffle<N>` with no accessor. **Verification: (b) source** — lines
   1308–1321, no public getter. This matters because both are *wire-visible protocol
   constants*: `PROTOCOL.md` §4.5 fixes the card meaning as the identity map over ziffle's
   `open_deck`, so two clients with different `open_deck` values disagree about what card
   index 17 is, and two clients with different Pedersen generators reject each other's every
   proof. And both are derived from **`rand 0.8`'s `StdRng` plus `ark-ec 0.5.0`'s rejection
   sampling** — `StdRng::from_seed(Sha256::digest(b"CARDS-V1"))`, then
   `CurveProj::rand` / `GENERATOR * Scalar::rand` (lines 156–166, 1214–1217). `StdRng` is
   explicitly not reproducible across `rand` majors, and the rejection-sampling loop's byte
   consumption is an arkworks implementation detail. **The 52 cards of this protocol are
   defined by the internals of two crates, and no test in ziffle or in our corpus pins them.**
   Vendoring plus `Cargo.lock` freezes it for our build; it does not freeze it for a second
   implementation, nor would it catch an accidental bump. Independently recomputed and
   confirmed equal to ziffle's by decrypting a card and matching the point:

   ```
   [F] open_deck recomputed independently matches ziffle at revealed index 34: true
   [F] blake3(open_deck 52 compressed points) = 4e931f9e24cf0cc9b511525c165ebd9bd1454c2aa0fe9eb74ef315a3dedfae7c
   [F] blake3(pedersen h || 52 generators)     = 14c8e144b43285da721b4ec2fe8fd8576849c6befdc9f5f8a7457174018d1dd4
   ```

   **Verification: (a) compiled** — `zr-protofit`, section F. These two digests are the test
   vector the corpus is missing. §8 obligation 3.

6. **No typed errors — every failure is `None`.** `verify_shuffle`, `verify_initial_shuffle`,
   `OwnershipProof::verify`, `RevealTokenProof::verify` and `reveal_card` all return `Option`.
   `CRYPTOGRAPHY.md` §12 obligation 0 (and §8.1.5, on which D-014's tier-1 clauses rest)
   requires distinguishing **`invalid`** — which is evidence and can cost a seat its name in
   the transcript — from **could not verify**, which is not. The crate cannot make that
   distinction for us: a `None` from `verify_shuffle` means "the proof did not verify against
   the four inputs you supplied" and nothing more, and if the caller supplied the wrong `prev`
   deck, the wrong `apk`, or a `ctx` built from a stale field, the `None` is identical.
   Measured, so the ambiguity is not hypothetical:

   ```
   [C4] chained proof against the wrong prev deck: false
   [S]  verify_shuffle with the wrong apk:         false
   ```

   Both are the same `None` an actually-forged proof produces. The obligation this creates is
   in §8 item 4: every input must be established as correct *before* the call, so that a
   `None` can only mean `invalid`.

---

## 7. Findings

Each with the file and line, what the code does, what our protocol requires, and — the part
that matters — whether it is exploitable and how.

### F1. The identity point is accepted as a player's deck public key, with an ownership proof anybody can forge

**Where.** `lib.rs` 349–355 (`OwnershipProof::verify`), 408–411 (`AggregatePublicKey::new`),
475–500 (`RevealTokenProof::verify`).

**What the code does.** The Schnorr check is `z·G == a + e·pk`. When `pk` is the identity `O`,
`e·pk = O` for every challenge `e`, so the check collapses to `z·G == a` — satisfied by
`(a = w·G, z = w)` for any `w`, with no knowledge of anything and without ever computing the
challenge. There is no `pk != identity` guard. `AggregatePublicKey::new` then sums it in as a
no-op, and the same collapse makes the DLEQ accept the identity as that player's reveal token.

**What our protocol requires.** `CRYPTOGRAPHY.md` §2.1 states the ownership proof's purpose as
closing the rogue-key attack and calls the key set "exactly the set of players dealt into this
hand". `PROTOCOL.md`'s `DECK_INIT` receiver checks are: exact lengths, `Validate::Yes`
deserialisation, the Schnorr proof verifies against `pk_i` and the hand's `ctx`, exactly one
entry per `dealt_in` seat. **None of those rejects `pk_i = O`.**

**Measured.**

```
[D] identity public key + (a=wG, z=w) ownership proof accepted: true
[D] apk unchanged by the identity key: true
[D] zero player's reveal token + DLEQ verifies: true
[D] 2 of the 3 'players' open the card: Some(51)
```

**Verification: (a) compiled** — `zr-protofit`, section D. The `PublicKey`, `SecretKey` and
`OwnershipProof` values were built by serialising the identity point, the zero scalar and the
pair `(w·G, w)` and deserialising them through the crate's own `CanonicalDeserialize` impls —
which is exactly the route a non-conforming client takes, since those impls exist precisely so
these types can come off the wire.

**Is it exploitable, and how.** Only against the seat that does it, but against that seat
completely. A seat submitting `pk = O` removes itself from the threshold: the deck key becomes
`Σ_{i≠M} pk_i`, `M`'s share is the identity which anybody can supply, and the effective
threshold drops from `n`-of-`n` to `(n−1)`-of-`(n−1)` over the remaining seats — the last line
above is that, measured, at n = 3. **Heads-up this is a total break of `SPEC_CS.md` §9 and §35
for the submitting seat**: with n = 2 the aggregate key becomes the opponent's own key alone,
so a modified opponent client reads every card in the deck, including both of `M`'s hole
cards, from material it already holds.

It is **not** remotely triggerable. A coalition cannot force an honest seat's key to the
identity, and cannot make `apk = O` itself: doing so needs `pk_M = −Σ_{i≠M} pk_i`, whose
discrete log the attacker does not know, which is what the ownership proof genuinely does
close. So this is a self-harm bug — but self-harm delivered by a broken or backdoored
third-party build, invisible to every other peer, producing a transcript that verifies
perfectly for everyone, and detected by nothing in the corpus as written. It is also a
two-line fix.

**Fix.** Add to `DECK_INIT`'s receiver checks: reject `hand_public_key` that deserialises to
the identity point, and reject a `DECK_INIT` set whose `hand_public_key` values are not
pairwise distinct. Both are cheap and both are checks on data every peer already holds.

### F2. Duplicate public keys aggregate silently

**Where.** `lib.rs` 408–411.

```
[D] duplicate pk aggregated silently (apk differs from single: true)
```

`AggregatePublicKey::new(&[vpk, vpk])` yields `2·pk` with no complaint. **Verification: (a)
compiled.** Not remotely exploitable: two seats presenting the same `pk` must both produce
tokens for it, which requires the same `sk`, which means one player holding two seats — a
colluder who already has both shares and gains nothing. Rejecting it costs nothing and is
folded into F1's fix.

### F3. `apk`'s "ascending seat order" is unenforceable, and does not need to be enforced

`PROTOCOL.md`'s `DECK_INIT` says every peer derives `apk = Σ pk_i` "in ascending seat order".
Point addition is commutative, so the order has no effect on the result and no peer could
detect another using a different one. Harmless — but a reader may take it for a check that
constrains something, and it does not. Worth one clarifying word when that block is next
touched; no code follows from it.

### F4. `Shuffle::<52>::default()` costs 5.9 ms and is absent from the timing table

**Where.** `lib.rs` 156–166, 1214–1217, 1314–1321.

Constructing the `Shuffle` derives 53 rejection-sampled curve points and 52 fixed-base scalar
multiplications: **5.9 ms, mean of ten** (`[setup]`, `zr-protofit`). It is not in
`CRYPTOGRAPHY.md` §6.5's table, and the natural way to write a `DeckCrypto` impl —
`Shuffle::default()` inside each method — adds 5.9 ms to every call, which is 14 % on top of a
42 ms verify and is paid `n` times per hand per node. The type is `Copy` at 8 832 B, so an
`&self` field costs nothing to hold and everything to copy accidentally. Construct once,
store by reference, never by value.

### F5. Panic paths on the *prover* side

`shuffle_remask_prove` verifies its own proof and calls `.expect("invalid shuffle proof")`
(lines 1264–1266); `ct_mspp!` carries `.expect("N > 0")`; `usize_to_u64!` carries an `expect`;
`Transcript::update_with_serialized` asserts the serialised element fits a 256-byte buffer
(lines 211–215). The last is unreachable at N = 52 — the largest item ever appended is a
`Ciphertext` at 66 B, and every other is 33 B — so `CRYPTOGRAPHY.md` OQ-5's concern about it
is over-stated for our deck size, though it remains true that it is an assert on a code path
that touches network-derived deck bytes. The first is the live one: a library defect or memory
corruption panics the *prover*, on our own data, not an attacker's. Per §12 obligation 0 a
caught panic is "could not verify" and must not become evidence. The worker thread §12
obligation 4 already requires is also the panic boundary; say so where that obligation is
written.

---

## 8. Obligations this review adds

Numbered to continue `CRYPTOGRAPHY.md` §12's list in spirit; the numbering there is that
document's to assign.

1. **Reject `hand_public_key == identity` in `DECK_INIT`, and reject a non-pairwise-distinct
   key set.** F1, F2. Adversarial test: a seat submitting the identity point with an
   `(a = w·G, z = w)` proof is rejected at `DECK_INIT`, and the hand does not start.
2. **Always take the `MaskedCard` for a DLEQ check from this peer's own final verified deck,
   at the index the envelope names — never from anything the sender supplied.** §2.3. The API
   makes the wrong version impossible to write, so this is a rule for the `DeckCrypto` wrapper:
   its `verify_reveal_token` must take a deck and an index, not a card.
3. **Freeze `open_deck` and the Pedersen commitment key as test vectors.** §6 item 5. The two
   BLAKE3 digests are in §6; a unit test recomputing them from the vendored ziffle's
   dependencies fails the build if a `rand` or arkworks bump moves either. Without it, a
   dependency bump silently redefines what card index 17 means.
4. **Establish every input before calling a `verify_*`, so that `None` can only mean
   `invalid`.** §6 item 6. Concretely: `input_deck_hash` and `output_deck_hash` checked first
   (`PROTOCOL.md` already requires this for `SHUFFLE_PROOF`), `apk` derived from the completed
   `DECK_INIT` stage, `ctx` derived from chained state, deserialisation succeeded with
   `Validate::Yes`. A `None` reached by any other route is *could not verify* and must not
   produce evidence under D-014.
5. **Treat `reveal_card` returning `None`, after `n` DLEQ-verified tokens from exactly the
   `apk` key set on a card from a verified chain, as a soundness fault and not an abort.** §3.
   It is unreachable unless the shuffle argument has been broken. A distinct, loud,
   non-resumable fault; the ordinary neutral abort is the wrong response because it hides the
   one signal that would tell us OQ-1 went badly.
6. **Record a practical floor for `crypto_step_timeout_ms` above the protocol's legal
   1 000 ms.** §5.4. Either raise the advert bound, or document that below ~5 000 ms the table
   is not playable on slow clients over relayed paths, and that the resulting aborts name
   innocent peers.
7. **Construct `Shuffle::<52>` once per process and hold it by reference.** F4.

---

## 9. What I did not check

Stated plainly, per `SPEC_CS.md` §36.

* **The algebra.** I did not review `MultiExpArg` or `SingleValueProductArg` against
  Bayer–Groth 2012, did not check the `m = 1` soundness bound, and did not attempt the forked
  Fiat–Shamir attack. Everything in §2–§5 is consistent with the argument being unsound in a
  way that accepts a clever forgery, and would remain true if it were. `CRYPTOGRAPHY.md`
  OQ-1 and OQ-2 are untouched by this document.
* **Hostile deserialisation.** I round-tripped honest objects and built three malformed-by-
  intent objects by hand (F1). I did not fuzz. OQ-5 stands.
* **Timing.** I measured wall-clock throughput. Nothing here says anything about
  constant-time behaviour, and `reveal_card`'s `open_deck.iter().position(...)` is an early-
  exit linear scan whose duration is a function of the card index — irrelevant remotely, since
  the party running it already knows the card, but it is not constant time and I did not
  quantify it. OQ-4 stands.
* **Real network conditions.** Every latency statement in §5 is arithmetic over measured
  compute, not a measurement. The sequential-chain estimates in `CRYPTOGRAPHY.md` §6.5 remain
  estimates; Phase 8 owns them.
* **Multi-process determinism.** I confirmed `open_deck` is reproducible by recomputing it in
  the same process from the same crate versions. I did **not** verify that a proof produced by
  one build verifies under a differently-compiled build (different optimisation level, different
  target). Nothing suggests it would not — the derivation is deterministic arithmetic — but I
  did not run it, and obligation 3's test vector is what would catch it if it were false.
* **Deck sizes other than 52** and player counts above 10.

---

## 10. Reproduction

```bash
export PATH="~/.cargo/bin:$PATH"
cd <scratch>/zr-protofit
cargo run --release -j 19                 # selective opening, n-of-n, ctx, degenerate keys
cargo run --release -j 19 --bin cost      # per-stage cost against the budget
cargo run --release -j 19 --bin shape     # intermediate decks, key-set coupling, sizes
```

`Cargo.toml` pins `ziffle = "=0.1.0"` and resolves the rest from the same cache the workspace
uses. The `ctx` construction in the probe is a local reproduction of `PROTOCOL.md` §2.8's `h`
and §4.5's field list; if either changes, the probe's `deck_ctx` must change with it, and the
§4.3 results are then stale.
