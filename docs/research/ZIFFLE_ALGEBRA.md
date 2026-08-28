# ziffle 0.1.0 — line-by-line algebra review of `MultiExpArg` and `SingleValueProductArg`

**Scope of this document.** The exponents. Everything the prover computes and everything
the verifier checks in the two Bayer–Groth sub-arguments, diffed term by term against
the paper. Transcript/Fiat–Shamir design, serialization, and the protocol layer above
the proof are named only where they touch the algebra; they are other reviews' subjects.

**Sources.**

- Code: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ziffle-0.1.0/src/lib.rs`
  (1779 lines). All line numbers below refer to that file.
- Paper: Bayer & Groth, *Efficient Zero-Knowledge Argument for Correctness of a Shuffle*,
  EUROCRYPT 2012, full text fetched from `http://www0.cs.ucl.ac.uk/staff/J.Groth/MinimalShuffle.pdf`.
  Relevant sections: **§3** (shuffle argument), **§4** (multi-exponentiation argument),
  **§5.3** (single value product argument). Note that the multi-exponentiation argument
  is the paper's **§4**, not §5 — the task brief's "§5" is §5.3 plus §5's framing.
- Probe crate: `~/AppData/Local/Temp/claude/<session>/<session-id>/scratchpad/zr-algebra/probe/`
  — a vendored copy of `lib.rs` with `verify` split into per-check booleans, plus
  forgery attempts. Built with `rustc 1.95.0`, `cargo test --release -j 19`.

---

## 0. Verdict

**I found no exponent error, no off-by-one, no wrong summation bound, and no missing
algebraic check in either sub-argument.** Both are faithful instantiations of the paper
at `m = 1, n = N`. I traced the paper's witness-extended emulator by hand through the
`m = 1` specialisation and it goes through with the code's exact equations (§4 below).

Everything I did find is either (a) a deviation that is provably harmless, (b) a
comment that contradicts correct code, or (c) a guard the author wrote that does not
fire. These are listed in §6 with severities. **Nothing here is exploitable.** The
things that would worry me about this crate are elsewhere — the Fiat–Shamir transcript,
the absent group-membership checks in `verify` (§6.3), and the plain fact that it is
unaudited.

The confidence this earns: the *algebra as written* is right. That is narrower than
"the shuffle proof is sound", because soundness also needs the transcript to be right,
the deserializer to reject junk, and the commitment key to be a genuine NUMS key
(the last one I did verify — §5.4).

---

## 1. Notation and the index shift

The paper is 1-based: `π ∈ Σ_N` with `π(i) ∈ {1..N}`, exponents `x^1 … x^N`.

The code is 0-based: `perm: [usize; N]` with `perm[i] ∈ {0..N-1}`, and it shifts by one
in two places, consistently:

| Paper | Code | Line |
|---|---|---|
| `a_i = π(i) ∈ {1..N}` | `pi[i] = Scalar::new(perm[i] + 1)` | 1117 |
| `b_i = x^{π(i)}` | `xpi[i] = x.pow([perm[i] + 1])` | 1125 |
| `C^x` target `∏_{i=1}^N C_i^{x^i}` | `xs[i] = x_base.pow([i + 1])`, `i ∈ 0..N-1` | 816 |
| `∏_{i=1}^N (y·i + x^i − z)` | `(1..=N).map(|i| y·Scalar::new(i) + x_base.pow([i]) − z)` | 1018–1023 |

The shift is applied to `pi`, to `xpi`, and to the verifier's public product together,
so the three agree. The comment at 1124 states the reason (avoid `x^0 = 1`) and is
correct.

`Scalar::new(BigInt::from(i))` in ark-ff 0.5.0 **does** perform Montgomery reduction
(`ark-ff-0.5.0/src/fields/models/fp/montgomery_backend.rs:690` multiplies by `R2`), so
`Scalar::new(i)` is the field element `i`, not `i·R^{-1}`. Probe `scalar_new_is_the_integer`
asserts this for `i ∈ 1..=52`. Even had it been `new_unchecked`, the map `i ↦ i·R^{-1}`
is injective and `F`-linear and both sides use it, so the permutation argument would
have survived — but it is not an issue: the value is correct.

The shuffle direction: `shuffle_remask_prove` (1245–1249) sets
`next[i] = prev[perm[i]] · E(1; rho[i])`, i.e. `C'_i = C_{π(i)} E(1; ρ_i)` — the paper's
orientation in §3, not its inverse. Consistent with `xpi[i] = x^{π(i)}` throughout.

---

## 2. `SingleValueProductArg` vs BG12 §5.3

The paper's §5.3, verbatim in structure:

> **Statement:** `c_a ∈ G` and `b ∈ Z_q`. **Witness:** `a ∈ Z_q^n`, `r` with
> `c_a = com(a; r)` and `b = ∏_{i=1}^n a_i`.
>
> **Initial message:** `b_1 = a_1`, `b_2 = a_1a_2`, …, `b_n = ∏a_i`. Pick
> `d_1..d_n, r_d ← Z_q`. Define `δ_1 = d_1` and `δ_n = 0` and pick `δ_2..δ_{n-1} ← Z_q`.
> Pick `s_1, s_x ← Z_q` and compute
> `c_d = com(d; r_d)`,
> `c_δ = com(−δ_1 d_2, …, −δ_{n-1} d_n; s_1)`,
> `c_Δ = com(δ_2 − a_2δ_1 − b_1d_2, …, δ_n − a_nδ_{n-1} − b_{n-1}d_n; s_x)`.
>
> **Challenge:** `x`. **Answer:** `ã_i = x a_i + d_i`, `r̃ = x r + r_d`,
> `b̃_i = x b_i + δ_i`, `s̃ = x s_x + s_1`.
>
> **Verification:** accept if `c_d, c_δ, c_Δ ∈ G` and `ã_i, b̃_i, r̃, s̃ ∈ Z_q` and
> `c_a^x c_d = com(ã_1..ã_n; r̃)`,
> `c_Δ^x c_δ = com(x b̃_2 − b̃_1 ã_2, …, x b̃_n − b̃_{n-1} ã_n; s̃)`,
> `b̃_1 = ã_1`, `b̃_n = x b`.

### 2.1 Prover, line by line

| # | Paper (1-based, `i = 1..n`) | Code (0-based, `i = 0..N-1`) | Line | Match |
|---|---|---|---|---|
| P1 | `d_1..d_n ← Z_q`, `c_d = com(d; r_d)` | `d: [Scalar; N] = from_fn(rand)`; `ck.vector_commit(rng, &d)` | 914–915 | ✓ |
| P2 | `δ_1 = d_1` | `sdelta[0] = d[0]` | 920 | ✓ |
| P3 | `δ_2..δ_{n-1} ← Z_q` | `(1..N-1).for_each(\|i\| sdelta[i] = rand)` | 921 | ✓ — range `1..N-1` is exactly indices `1..=N-2`, i.e. paper's `2..=n-1` |
| P4 | `δ_n = 0` | `sdelta` init `[Scalar::zero(); N]`, index `N-1` never written | 919 | ✓ |
| P5 | `c_δ = com(−δ_i d_{i+1}, i = 1..n-1; s_1)` | `v[i] = -sdelta[i] * d[i+1]` for `i in 0..N-1`; `v[N-1] = 0` | 926–928 | ✓ — `0..N-1` yields `i = 0..=N-2`, paper's `i = 1..=n-1`. `N-1` slot is zero-padding |
| P6 | `a_i` (from the shuffle: `y·π(i) + x^{π(i)} − z`) | `a[i] = (y * pi[i]) + xpi[i] - z` | 935 | ✓ |
| P7 | `b_1 = a_1`, `b_i = b_{i-1}a_i` | `b[0] = a[0]`; `(1..N)`: `b[i] = b[i-1] * a[i]` | 936–938 | ✓ (the `[Scalar::ONE; N]` initialiser is fully overwritten) |
| P8 | `c_Δ = com(δ_{i+1} − a_{i+1}δ_i − b_i d_{i+1}, i = 1..n-1; s_x)` | `v[i] = sdelta[i+1] - (a[i+1] * sdelta[i]) - (b[i] * d[i+1])` for `i in 0..N-1` | 941–942 | ✓ — paper's `j = 2..n` reindexed as `i = j-2 = 0..n-2` |
| P9 | `ã_i = x a_i + d_i` | `a_tilde[i] = (x * a[i]) + d[i]` | 951 | ✓ |
| P10 | `b̃_i = x b_i + δ_i` | `b_tilde[i] = (x * b[i]) + sdelta[i]` | 953 | ✓ |
| P11 | `r̃ = x r + r_d` where `r` opens `c_a` | `w_a = (y * w_pi) + w_xpi`; `r_tilde = (x * w_a) + w_d` | 955–956 | ✓ — `c_a = c_π^y · c_{xπ} · com(−z; 0)`, so its randomness is `y·r_π + r_{xπ} + 0` |
| P12 | `s̃ = x s_x + s_1` | `s_tilde = (x * w_cdelta) + w_sdelta` | 958 | ✓ — `w_cdelta` is `c_Δ`'s randomness (`s_x`), `w_sdelta` is `c_δ`'s (`s_1`); the order is right |

The one place a sign or an index could plausibly have slipped — P8's
`δ_{i+1} − a_{i+1}δ_i − b_i d_{i+1}` — is written out correctly, including the fact that
the `a` index and the `d` index are `i+1` while the `b` index is `i`.

### 2.2 Verifier, line by line

| # | Paper | Code | Line | Match |
|---|---|---|---|---|
| V0 | (implicit) `c_{−z} = com(−z,…,−z; 0)` from §3 | `c_mz = ck.vector_commit_with_r(&[-z; N], Scalar::zero())` | 988 | ✓ randomness is exactly zero |
| V0b | `c_D = c_A^y c_B` from §3 | `c_a = (c_pi * y) + c_xpi` | 991 | ✓ |
| V1 | `c_a^x c_d = com(ã; r̃)` | `c_d + ((c_a + c_mz) * x) == vector_commit_with_r(a_tilde, r_tilde)` | 995–997 | ✓ |
| V2 | `c_Δ^x c_δ = com(x b̃_{i+1} − b̃_i ã_{i+1}, i = 1..n-1; s̃)` | `c_sdelta + (c_cdelta * x) == vector_commit_with_r(v, s_tilde)` with `v[i] = (x * b_tilde[i+1]) - (b_tilde[i] * a_tilde[i+1])`, `i in 0..N-1`, `v[N-1] = 0` | 1002–1010 | ✓ |
| V3 | `b̃_1 = ã_1` | `self.b_tilde[0] == self.a_tilde[0]` | 1014 | ✓ |
| V4 | `b̃_n = x·b` | `b_tilde[N-1] == x * ∏_{i=1}^{N}(y·i + x_base^i − z)` | 1018–1024 | ✓ |
| V5 | `c_d, c_δ, c_Δ ∈ G`, `ã, b̃, r̃, s̃ ∈ Z_q` | **absent from `verify`** — delegated to deserialization | — | see §6.3 |

### 2.3 Completeness, worked out

The paper's completeness identity (§5.3 proof):
`x b̃_i − b̃_{i-1}ã_i = x(δ_i − δ_{i-1}a_i − b_{i-1}d_i) − δ_{i-1}d_i`.

In the code's 0-based indices, V2's `v[i]`:

```
x·b̃_{i+1} − b̃_i·ã_{i+1}
  = x(x b_{i+1} + δ_{i+1}) − (x b_i + δ_i)(x a_{i+1} + d_{i+1})
  = x²b_{i+1} + xδ_{i+1} − x²b_i a_{i+1} − x b_i d_{i+1} − xδ_i a_{i+1} − δ_i d_{i+1}
```

`b_{i+1} = b_i a_{i+1}` kills both `x²` terms, leaving

```
  = x·(δ_{i+1} − a_{i+1}δ_i − b_i d_{i+1})  +  (−δ_i d_{i+1})
  = x·(c_Δ slot i)                          +  (c_δ slot i)
```

which is exactly `c_δ + x·c_Δ` in the message slots, with randomness
`w_sdelta + x·w_cdelta = s̃`. The identity holds slot-for-slot for `i = 0..N-2`, and
slot `N-1` is zero on both sides. **Verified.**

### 2.4 The `N-1` padding slot

Both prover commitments and the verifier's `v` leave index `N-1` at zero
(926, 940, 1005 and the loops that stop at `N-1`). The paper commits to an
`(n-1)`-vector; the code commits to an `n`-vector whose last entry is zero, under the
same key. These are the same commitment. A prover who puts a nonzero value `u` in slot
`N-1` of `c_δ` and `w` in slot `N-1` of `c_Δ` would need `u + x·w = 0` for the
Fiat–Shamir challenge `x`, which is itself a hash of `c_δ` and `c_Δ` — so he would have
to guess `x`. Probe `svp_last_slot_is_padding` bumps `c_sdelta` by `g_{N-1}^{99}` and
V2 rejects. **Not a hole.**

### 2.5 `N = 2` edge

`(1..N-1)` is empty and `(0..N-1)` is the single index `0`, giving
`δ = (d_0, 0)`, `c_δ = com(−δ_0 d_1, 0)`, `c_Δ = com(δ_1 − a_1δ_0 − b_0 d_1, 0)`.
Paper at `n = 2`: `δ_1 = d_1`, `δ_2 = 0`, no random `δ`s. ✓

### 2.6 One wrong comment

**`src/lib.rs:1000`** — the doc comment on V2 reads

```
// Step 5: check comm(δ) + comm(𝛥)·x == comm([x·b~(i + 1) - b~(i)·a~(i + 2)] for all i in [0; N - 2]; s~)
```

`a~(i + 2)` is wrong. The code on line 1007 computes `a_tilde[i + 1]`, which is what the
paper requires (`ã_i` paired with `b̃_{i-1}`, i.e. index `i+1` in 0-based). **The code is
right and the comment is wrong.** No exploit; but it is exactly the kind of comment that
would lead a future maintainer to "fix" correct code into an unsound state, and it is
worth correcting in a fork.

---

## 3. `MultiExpArg` vs BG12 §4 at `m = 1`

The paper's §4 is stated for an `m × n` matrix. `ziffle` fixes `m = 1` (comment at
1069–1070), so `A` is a single column `a_1 ∈ Z_q^n`, `r ∈ Z_q`, and `2m−1 = 1`, meaning
there are exactly two `E_k`: `E_0` and `E_1`, with `E_m = E_1`.

Specialising the paper at `m = 1`:

- `x = (x)^T`, a single power.
- `E_k = E(G^{b_k}; ρ_k) ∏_{i=1..m, j=k−m+i} C_i^{a_j}` collapses (`i = 1`, `j = k`) to
  `E_k = E(G^{b_k}; ρ_k) · C_1^{a_k}`.
- Required: `b_m = b_1 = 0`, `s_m = s_1 = 0`, `ρ_m = ρ_1 = ρ`. Hence
  `E_1 = E(1; ρ) C_1^{a_1} = C`, the statement's target.
- Responses: `a = a_0 + x a_1`, `r = r_0 + x r`, `b = b_0 + b_1 x = b_0`,
  `s = s_0 + s_1 x = s_0`, `ρ̄ = ρ_0 + x ρ`.
- Verification: `c_{B_m} = com(0; 0)`; `E_m = C`;
  `c_{A_0} c_A^x = com(a; r)`;
  `c_{B_0} c_{B_1}^x = com(b; s)`;
  `E_0 E_1^x = E(G^b; ρ̄) ∏_i C_i^{x^{m−i} a}` → at `m=1`, `= E(G^b; ρ̄) C_1^{a}`.

### 3.1 Symbol map

| Paper (`m=1`) | Code | Line |
|---|---|---|
| `a_0` (blinder vector) | `alpha` | 747 |
| `c_{A_0}` | `c_alpha`, randomness `w_alpha` | 749 |
| `a_1` (committed exponents) = `b_i = x^{π(i)}` | `xpi` | 1125 |
| `c_A` | `c_xpi`, randomness `w_xpi` | 1126 |
| `b_0` | `beta` | 748 |
| `c_{B_0}`, `s_0` | `c_beta`, `w_beta` (sent as `o_beta`) | 750 |
| `b_1 = 0`, `s_1 = 0`, `c_{B_1} = com(0;0) = O` | **elided** (constants) | — |
| `ρ_0` | `tau0` | 754 |
| `ρ = ρ_1` | `rho_agg` | 764 |
| `E_0` | `ct_mxp0` | 755–761 |
| `E_1 = C` | `ct_mxp1` | 765–771 |
| `a` (response) | `o_alpha` | 779 |
| `r` (response) | `o_r` | 781 |
| `b` (response) `= b_0` | `beta` | 792 |
| `s` (response) `= s_0` | `o_beta` | 793 |
| `ρ̄` | `tau` | 783 |

### 3.2 Prover

| # | Paper | Code | Line | Match |
|---|---|---|---|---|
| P1 | `c_{A_0} = com(a_0; r_0)`, `c_{B_0} = com(b_0; s_0)` | `ck.vector_commit(rng, &alpha)`, `ck.commit(rng, beta)` | 749–750 | ✓ |
| P2 | `E_0 = E(G^{b_0}; ρ_0) C_1^{a_0}` | `c1 = G·τ0 + Σ next_{i,1}·α_i`; `c2 = G·β + pk·τ0 + Σ next_{i,2}·α_i` | 755–761 | ✓ |
| P3 | `ρ = −ρ·b` (§3: `ρ' = −ρ·b`) | `rho_agg = -(0..N).map(\|i\| rho[i] * xpi[i]).sum()` | 764 | ✓ — sum over all `N`, sign negative |
| P4 | `E_1 = E(1; ρ) C_1^{a_1} = C` | `c1 = G·ρ_agg + Σ next_{i,1}·xpi_i`; `c2 = pk·ρ_agg + Σ next_{i,2}·xpi_i` | 765–771 | ✓ — no `G^b` term, i.e. `b_1 = 0` |
| P5 | `a = a_0 + x·a_1` | `o_alpha[i] = alpha[i] + (x * xpi[i])` | 779 | ✓ |
| P6 | `r = r_0 + x·r` | `o_r = w_alpha + (x * w_xpi)` | 781 | ✓ |
| P7 | `ρ̄ = ρ_0 + x·ρ` | `tau = tau0 + (x * rho_agg)` | 783 | ✓ |
| P8 | `b = b_0`, `s = s_0` | `beta`, `o_beta = w_beta` | 792–793 | ✓ |

The identity `E_1 = C` holds because
`∏_i C'^{x^{π(i)}} = ∏_i (C_{π(i)}E(1;ρ_i))^{x^{π(i)}} = ∏_j C_j^{x^j} · E(1; Σρ_i x^{π(i)})`,
so `E(1; −Σρ_i x^{π(i)}) ∏_i C'^{x^{π(i)}} = ∏_j C_j^{x^j}`. ✓

### 3.3 Verifier

| # | Paper | Code | Line | Match |
|---|---|---|---|---|
| V1 | `E_m = C`, i.e. `E_1 = ∏_{i=1}^N C_i^{x^i}` | `xs[i] = x_base^{i+1}`; `ct_mspp!(prev, xs) == ct_mxp1` (both coordinates) | 815–820 | ✓ |
| V2 | `c_{A_0} c_A^x = com(a; r)` | `(c_xpi * x) + c_alpha == vector_commit_with_r(o_alpha, o_r)` | 823–827 | ✓ |
| V3 | `c_{B_0} c_{B_1}^x = com(b; s)` with `c_{B_1} = com(0;0)` | `c_beta.0 == ck.commit_with_r(beta, o_beta)` | 830 | ✓ — `c_{B_1}` is the identity, so the product equation degenerates to exactly this |
| V4 | `E_0 E_1^x = E(G^b; ρ̄) ∏_i C_i^{x^{m−i}a}` | `ct_mxp0 + x·ct_mxp1 == (G·τ + Σ next_{i,1} oα_i, G·β + pk·τ + Σ next_{i,2} oα_i)` | 833–843 | ✓ — `x^{m−i} = x^0 = 1` at `m=1, i=1`, and the code uses `o_alpha` unscaled |
| V5 | `c_{A_0}, c_{B_k} ∈ G`, `E_k ∈ H`, `a ∈ Z_q^n`, `r,b,s,ρ̄ ∈ Z_q` | **absent from `verify`** | — | §6.3 |
| V6 | `c_{B_m} = com(0;0)` | structurally satisfied — `c_{B_1}` is a constant that is never sent | — | ✓ (see §3.4) |

### 3.4 The two elisions, and why neither is a gap

**(a) `c_{B_1}` and the `c_{B_m} = com(0;0)` check.** At `m=1` this commitment is a
fixed public constant (the identity element) and the check is a tautology. Omitting both
the element and the check changes nothing. The verification equation
`c_{B_0} c_{B_1}^x = com(b;s)` correctly degenerates to V3.

**(b) `E_m = C` sent rather than recomputed.** `ct_mxp1` is fully determined by public
data (`prev`, `x_base`), so transmitting it wastes 66 bytes. It is *not* a soundness
problem, because V1 pins it to the recomputed public product. But note the ordering:
`ct_mxp1` is hashed into the sub-argument's challenge `x` (line 774/812) *before* V1 is
evaluated, so a prover who lies about it changes `x` and fails everywhere — the probe's
`ct_mxp1 bumped` row shows mexp `c1, c2, c4` all failing.

---

## 4. Does the extraction still work at `m = 1`? (hand-traced)

This is the part that matters, so I did it explicitly rather than trusting the shape.

Rewind on the `MultiExpArg` challenge with the same first message
`(c_α, c_β, E_0, E_1)` and two challenges `x ≠ x'`:

1. **V3 in both runs** gives `com(β; o_β) = c_β` and `com(β'; o_β') = c_β`. Pedersen
   binding ⇒ `β = β'`. **`c_β` being hashed into `x` (line 774) is what makes this
   true**; it is there.
2. **V2 in both runs** gives `(x − x')·c_xpi = com(o_α − o_α'; o_r − o_r')`, so
   `a := (o_α − o_α')/(x − x')` is an opening of `c_xpi`, and `α := o_α − x·a` opens `c_α`.
3. **V4 differenced.** The message terms are `G·β` in both runs and **cancel**:
   `E(G^β; τ) − E(G^β; τ') = (G(τ−τ'), pk(τ−τ')) = E(1; τ−τ')`.
   And `∏C'^{o_α} − ∏C'^{o_α'} = ∏C'^{(x−x')a}`. Dividing by `(x − x')`:
   ```
   E_1 = E(1; (τ−τ')/(x−x')) · ∏_i C'_i^{a_i}
   ```
4. **V1** gives `E_1 = ∏_i C_i^{x_base^i}`.

Together: `∏_i C_i^{x_base^i} = E(1; ρ) ∏_i C'^{a_i}` with `a` the extracted opening of
`c_xpi` — precisely the §3 multi-exponentiation statement. The `β` machinery does no
work beyond being *fixed before the challenge*; that is its whole job, and it does it.

Complementary view (why V4 is the real gate). Write
`P(s) = (Σ next_{i,1}s_i, Σ next_{i,2}s_i)`. V4 rearranges to
```
U + x·V ∈ { (G·t, pk·t) : t ∈ Z_q }        where
U = E_0 − P(α) − (0, G·β)   (fixed before x)
V = E_1 − P(a)              (fixed before x)
```
`τ` is a free response, so the prover needs `U + xV` on the line spanned by `(G, pk)`.
Both `U` and `V` are committed before `x`, so in the ROM he needs each on the line
separately. `V` on the line is *exactly* the true statement
`∏C^{x^i} = ∏C'^{a}·E(1;t)`. Nothing else gets him there.

I confirmed this empirically: probe `forced_ct_mxp1_attack` builds a prover who
**forces** `ct_mxp1` to the recomputed public product (so V1 passes by construction) on a
deck where `next[0]` carries an extra plaintext offset `12345·G`, and answers honestly
otherwise. Result:

```
forced ct_mxp1 + tampered deck     mexp[c1=1 c2=1 c3=1 c4=0]  svp[c1=1 c2=1 c3=1 c4=1]
```

V1 satisfied, V4 catches it. That is the predicted behaviour.

The §3-level composition (SVP forces `π` to be a permutation and `b_i = x^{π(i)}`) is
also as the paper describes: `c_pi` is committed before `x_base` (1118 → 1121), `c_xpi`
after `x_base` and before `y, z` (1126 → 1129), and both are fixed before the SVP runs.
Probe `mismatched_permutations_attack` commits `c_pi` to one permutation and `c_xpi` to
`x^σ` for a different one; SVP V4 rejects (and mexp V1 too).

---

## 5. Everything I checked that is *not* a finding

Recorded so a later reader knows it was looked at.

**5.1 Commitment bases.** `commit_with_r` (169) uses `GENERATOR` + `h`;
`vector_commit_with_r` (181) uses `gs[0..N]` + `h`. Two different bases. This is safe
only because they are never mixed: `c_beta` is the sole user of `commit_with_r`, and it
is compared only against `commit_with_r` (line 830). Every other commitment
(`c_pi`, `c_xpi`, `c_alpha`, `c_d`, `c_sdelta`, `c_cdelta`, `c_mz`) uses the vector basis
and is only ever compared against `vector_commit_with_r`. **No basis confusion.**

**5.2 `c_{−z}` randomness.** Paper requires `com(−z,…,−z; 0)`. Code passes
`Scalar::zero()` (988). ✓ A nonzero randomness here would break the §3 composition
(`t' = y r + s` would no longer open `c_D c_{−z}`).

**5.3 `ct_mspp!` bounds.** The macro (684–692) zips `cts.iter()` with the scalar array
and reduces — length `N` on both sides, no truncation, `.expect("N > 0")` on the reduce.
Used at 756, 766, 817, 838. All four call sites pass length-`N` arrays.

**5.4 The commitment key is genuinely NUMS — checked, because it would have been fatal.**
`PedersonCommitKey::default` (157–165) derives `h` and every `gs[i]` with
`CurveProj::rand(&mut StdRng::from_seed(Sha256(seed)))`. If `Projective::rand` were
`GENERATOR * Scalar::rand(rng)`, the discrete log `log_G(h)` would be publicly
recomputable from the fixed seed, Pedersen binding would collapse, and every equation in
this document would be forgeable. It is not: `ark-ec-0.5.0/src/models/short_weierstrass/group.rs:95-108`
samples a random *x-coordinate* and lifts it to the curve. Discrete logs unknown.
(Contrast `open_deck`, line 1216, which deliberately *does* use `GENERATOR * rand` —
correct there, since those are plaintexts, not commitment bases. The author knew the
difference.)

**5.5 `y`, `z`, `x` are not checked for being `0` or `1`.** The paper does not require it.
A degenerate `x_base` (e.g. of tiny multiplicative order, making `x^{π(i)}` collide
across positions) would weaken the single-transcript argument, but the probability that a
hash-derived scalar has order below `2^80` is about `2^{-176}`. Not a finding.

**5.6 Challenge derivation ordering.** Every commitment that must precede a challenge
does: `c_pi` before `x_base`; `c_xpi` before `y,z`; `(c_alpha, c_beta, ct_mxp0, ct_mxp1)`
before the mexp `x`; `(c_d, c_sdelta, c_cdelta)` before the SVP `x`. `y` and `z` are not
themselves appended to the transcript before the sub-arguments derive their challenges
(1098–1102), but they are deterministic functions of the same transcript state, so
appending them would be redundant. Not a gap. *(The forked-transcript question proper is
another reviewer's.)*

**5.7 Per-check falsifiability.** Probe `check_matrix` perturbs each response field and
records which of the eight checks fire:

```
honest                             mexp[c1=1 c2=1 c3=1 c4=1]  svp[c1=1 c2=1 c3=1 c4=1]
dup card (non-permutation)         mexp[c1=0 c2=1 c3=1 c4=1]  svp[c1=1 c2=1 c3=1 c4=0]
plaintext offset on next[0]        mexp[c1=0 c2=1 c3=1 c4=1]  svp[c1=1 c2=1 c3=1 c4=1]
substituted next[0]                mexp[c1=0 c2=1 c3=1 c4=1]  svp[c1=1 c2=1 c3=1 c4=1]
post-hoc deck tamper               mexp[c1=0 c2=0 c3=1 c4=0]  svp[c1=0 c2=0 c3=1 c4=0]
beta bumped                        mexp[c1=1 c2=1 c3=0 c4=0]  svp[c1=1 c2=1 c3=1 c4=1]
o_alpha[0] bumped                  mexp[c1=1 c2=0 c3=1 c4=0]  svp[c1=1 c2=1 c3=1 c4=1]
ct_mxp1 bumped                     mexp[c1=0 c2=0 c3=1 c4=0]  svp[c1=1 c2=1 c3=1 c4=1]
b_tilde[N-1] bumped                mexp[c1=1 c2=1 c3=1 c4=1]  svp[c1=1 c2=0 c3=1 c4=0]
b_tilde[0] bumped                  mexp[c1=1 c2=1 c3=1 c4=1]  svp[c1=1 c2=0 c3=0 c4=1]
a_tilde[N-1] bumped                mexp[c1=1 c2=1 c3=1 c4=1]  svp[c1=0 c2=0 c3=1 c4=1]
a_tilde[0] bumped                  mexp[c1=1 c2=1 c3=1 c4=1]  svp[c1=0 c2=1 c3=0 c4=1]
identity, no remask                mexp[c1=1 c2=1 c3=1 c4=1]  svp[c1=1 c2=1 c3=1 c4=1]
```

**Every one of the eight checks is falsified by at least one perturbation.** None is
vacuous, none is dead code that always returns `true`. Repeated at `N = 52` (probe
`deck_52`): honest passes, duplicate card rejected by mexp V1 and SVP V4.

The `identity, no remask` row is *correct* behaviour, not a bug: BG12 proves that *a*
permutation with *some* randomizers was applied, not that either was chosen at random.
`ShuffleProof` accepts `π = id, ρ = 0`. **The poker layer must not read a valid shuffle
proof as evidence that the shuffler randomised anything.** Unpredictability comes from
composing shuffles by different players, at least one of whom is honest — which is what
the crate's example does, and what the protocol design must preserve.

---

## 6. Findings

None of these is exploitable. Ordered by how much they should change what gets built.

### 6.1 The `N > 1` guard never fires — `Shuffle::<1>` compiles and then panics

**`src/lib.rs:1324`**

```rust
impl<const N: usize> Shuffle<N> {
    const _N_GREATER_THAN_1: () = assert!(N > 1);
```

An associated `const` in an impl block is evaluated only if it is *referenced*.
`_N_GREATER_THAN_1` is referenced nowhere in the crate (grepped: one hit, its own
definition). It is therefore never monomorphised and the assertion never runs.

**What the paper requires:** §5.3 is stated for `n ≥ 2` (`δ_1 = d_1` and `δ_n = 0` are
distinct constraints); the multi-exponentiation argument needs `n ≥ 1`.

**Consequence, measured.** Probe `n_equals_one_compiles` constructs `Shuffle::<1>`
successfully. Probe `n_equals_one_shuffle` then panics inside the crate:

```
thread panicked at src/lib.rs:1294: invalid shuffle proof
```

At `N = 1` the SVP degenerates (`δ_0 = d_0 = δ_{n-1}`, forcing V4 to require `d_0 = 0`)
and the prover's own self-check at line 1264–1266 aborts. At `N = 0` the `N - 1`
expressions underflow and it indexes out of bounds.

**Exploitable?** No. `N` is a compile-time constant chosen by the application, and the
failure is a panic in the prover, not an accepted proof. It is a **liveness / DoS**
issue, not a soundness one: `p2p-poker` will use `N = 52` and never touch it.

**Action:** in any fork, reference the const (`let _ = Self::_N_GREATER_THAN_1;` at the
top of `initial_deck`) so the guard actually fires. Upstream this as an issue.

### 6.2 A comment that contradicts correct code (`src/lib.rs:1000`)

Described in §2.6. The comment says `a~(i + 2)`; the code computes `a_tilde[i + 1]`;
the paper wants `a_tilde[i + 1]`. **Code correct, comment wrong.** Low severity today,
but it is a trap for anyone who "corrects" the code to match the comment. Fix the
comment in any fork.

### 6.3 The paper's group/range membership checks are absent from `verify`

**`src/lib.rs:799-846` and `972-1028`**

BG12 §4 verification opens with *"Check `c_{A_0}, c_{B_0}, …, c_{B_{2m−1}} ∈ G` and
`E_0, …, E_{2m−1} ∈ H` and `a ∈ Z_q^n` and `r, b, s, ρ ∈ Z_q`"*, and §5.3 with
*"accept if `c_d, c_δ, c_Δ ∈ G` and `ã_1, b̃_1, …, r̃, s̃ ∈ Z_q`"*. §3 adds
*"the verifier checks `c_A, c_B ∈ G^m`"*.

`MultiExpArg::verify` and `SingleValueProductArg::verify` perform **none** of these.
They operate on already-typed `CurveAffine` and `Scalar` values and rely entirely on
deserialization having enforced membership.

**Is that safe?** It depends on how the caller deserializes, and the crate cannot
enforce it:

- `deserialize_compressed` (`Compress::Yes`) — **safe**. The x-only encoding is lifted
  via `get_ys_from_x_unchecked`, so an off-curve point cannot be represented
  (`ark-ec-0.5.0/src/models/short_weierstrass/mod.rs:144-164`). secp256k1 has cofactor 1,
  so on-curve implies in-subgroup. `Fp` deserialization rejects values ≥ modulus.
- `deserialize_uncompressed_unchecked` (`Compress::No, Validate::No`) — **not safe**.
  `mod.rs:166-180` reads `(x, y)` and calls `Affine::new_unchecked` with **no on-curve
  check** when `Validate::No`. That admits invalid-curve points into the verification
  equations.

The crate's `impl_valid_and_deser!` macros (1766–1776) faithfully forward whatever
`compress`/`validate` the caller passes, so this is the caller's decision.

**Exploitable?** Not as ziffle is used today — `p2p-poker/Cargo.toml:37` declares
`ziffle = "=0.1.0"` but no code deserializes ziffle types yet (grepped: zero hits for
any `deserialize_*` call in the repo). So this is a **forward-looking constraint**, not
a live bug. Whether an invalid-curve point actually yields a forgery I did not work out
— it would require finding a low-order point on a suitable twist that satisfies the
verification equations, and I did not attempt it.

**Action for `p2p-poker`:** wire-format decoding of `ShuffleProof`, `MaskedDeck`,
`PublicKey`, `RevealToken` and friends **must** use `deserialize_compressed`
(equivalently `deserialize_with_mode(_, Compress::Yes, Validate::Yes)`).
`deserialize_uncompressed_unchecked` must not appear anywhere near network input. Make
this a lint or a wrapper type; do not leave it to discipline.

### 6.4 `β`, `c_β`, `o_β` carry no soundness weight at `m = 1` (informational)

`beta` is a uniformly random scalar that the prover **reveals in the clear** (line 792),
together with its commitment randomness (793). §4's extraction (§4 above) shows the `G·β`
terms cancel when V4 is differenced across two challenges, so `β` contributes nothing
beyond being fixed before the challenge — and at `m = 1` it could be fixed to `0` with
no loss. A simulator without it works: pick `o_α, τ` uniform and set
`ct_mxp0 = E(1; τ)∏C'^{o_α} − x·ct_mxp1`, which is identically distributed to the real
`ct_mxp0` because `α` and `τ0` are uniform.

This is the paper's structure faithfully carried over from general `m` (where the `b_k`
*are* needed to blind the off-diagonal `E_k`), so it is not a deviation. It costs 97
bytes per proof (33 + 32 + 32) plus the 66 redundant bytes of `ct_mxp1` from §3.4(b) —
163 of the advertised 5,547 bytes for `ShuffleProof<52>`. **Do not "optimise" this away
in a fork**; the saving is 3% and the risk of getting the `m = 1` degeneration wrong is
not worth it. Recorded only so a future reader does not mistake `β` for a check that is
doing something it is not.

### 6.5 `usize::to_be_bytes` in the transcript is platform-width dependent (informational)

**`src/lib.rs:218, 223, 238`** — `serialized_size.to_be_bytes()`, `label.len().to_be_bytes()`
and the element index `i.to_be_bytes()` are `usize`, so they emit 8 bytes on a 64-bit
target and 4 on a 32-bit one. Two peers on different pointer widths would derive
**different challenges from the same proof** and each would reject the other's proofs.
Not an algebra finding and not a soundness issue (it fails closed), but it is a real
interoperability hazard for a P2P protocol whose whole point is heterogeneous peers.
Flagging it here because it lives in the challenge derivation that both sub-arguments
depend on; the transcript reviewer should own it.

---

## 7. What I did not manage to check

Stated plainly, per the brief.

1. **I did not prove witness-extended emulation formally.** I hand-traced the paper's
   extractor through the `m = 1` specialisation (§4) and every step goes through with
   the code's exact equations. That is a careful reading, not a proof. In particular I
   did not verify the paper's own `N`-fold rewinding over `x_base` (the transposed
   Vandermonde argument in §3) against anything in the code — it is a property of the
   protocol, not of any line I could point at.

2. **I did not review the Fiat–Shamir transcript or the fork.** `Transcript::append`,
   `append_vec`, `derive_challenge_scalars`, the `ziffle/BG12MultiExpArgX/v1` vs
   `ziffle/BG12ProductArgX/v1` domain separation, and the `ts.clone()` at line 1143 are
   another reviewer's subject. My §4 extraction *assumes* the challenges are independent
   random oracle outputs on distinct inputs. If that assumption fails, the algebra being
   right does not save it. **This is the load-bearing dependency of this whole
   document.**

3. **I did not audit `DefaultFieldHasher<Sha256>`** (line 246) for uniformity or for
   correct `expand_message_xmd` behaviour when producing two scalars at once (`[y, z]`,
   line 1100). If that hasher produced correlated or low-entropy outputs the §3
   Schwartz–Zippel argument over `y` and `z` would weaken. I took arkworks' hash-to-field
   as correct.

4. **I did not attempt an invalid-curve attack** against §6.3. I established that
   `deserialize_uncompressed_unchecked` admits off-curve points and that the verifier
   performs no membership check of its own; I did not construct a forgery from that.

5. **I did not analyse composition across sequential shuffles** — each `verify_shuffle`
   is checked independently against the previous `Verified<MaskedDeck>`, and I did not
   look for cross-proof malleability (e.g. replaying one player's `mexp_arg` into
   another's proof). The `ctx` binding and the `prev`/`next` in the transcript make this
   look fine, but "looks fine" is all I have.

6. **Timing and side channels are out of scope** and were not examined. `Scalar::rand`
   quality, `zeroize` coverage, and the prover's use of the caller's RNG likewise.

---

## 8. Reproducing this

```bash
export PATH="~/.cargo/bin:$PATH"
cd "~/AppData/Local/Temp/claude/<session>/<session-id>/scratchpad/zr-algebra/probe"
cargo test --release -j 19 -- --nocapture --test-threads=1
```

`src/lib.rs` there is ziffle 0.1.0 verbatim except that `MultiExpArg::verify` and
`SingleValueProductArg::verify` are split into `verify_checks` returning `[bool; 4]`
(the check expressions themselves are untouched, byte for byte), with the original
`verify` restored as an `.all()` over them. `src/probe.rs` holds the tests.
`n_equals_one_shuffle` is *expected to fail* — that failure is finding 6.1.

The paper text used for the diff is at
`.../scratchpad/zr-algebra/bg12.txt` (`pdftotext -layout` of the UCL PDF).
