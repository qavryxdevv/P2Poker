# `vendor/ziffle` — provenance

**Status:** the record of where these bytes came from and why they are in the tree.
Written for `ZIFFLE_VERDICT.md` condition **C-0**.

Authority: `SPEC_CS.md` binds; `DECISIONS.md` outranks this file; the dependency
register at `docs/DEPENDENCIES.md` §3.1 and §5.1 owns ziffle's *risk* rating and this
file does not restate it (D-011). This file owns one thing only: **which bytes**.

---

## 1. Identity

| | |
|---|---|
| Crate | `ziffle` |
| Version | `0.1.0` — the only release ever published |
| Source | crates.io, `https://static.crates.io/crates/ziffle/ziffle-0.1.0.crate` |
| crates.io sha256 | `ba79285194a16b02512566a9a64d885567646045b144bb0efeef662001cd83a5` |
| Upstream git sha1 | `bcb8e61651cd6d4140c895a414d7ed8bc06d32bd` |
| Upstream repository | `https://github.com/v26-solutions/ziffle` |
| Author | Chris Ricketts `<chris_ricketts@proton.me>` |
| Declared licence | `MIT OR Apache-2.0` — **we take Apache-2.0**, see §4 |
| Vendored | 2026-08-29 |

The git sha1 is not our claim about upstream. It is the value in
`.cargo_vcs_info.json`, which cargo writes into the tarball at publish time from the
publishing working tree:

```json
{
  "git": {
    "sha1": "bcb8e61651cd6d4140c895a414d7ed8bc06d32bd"
  },
  "path_in_vcs": ""
}
```

That file is inside the archive covered by the sha256 above, so the binding from
digest to commit is as strong as the digest — but it is a claim *by the publisher*,
not an independent attestation. Nobody has checked out that commit and compared it to
these files. Recorded so a later reader does not mistake it for verification.

---

## 2. The digest, verified rather than copied

The sha256 was recomputed here, not transcribed from the review.

```
$ sha256sum ~/.cargo/registry/cache/index.crates.io-1949cf8c6b5b557f/ziffle-0.1.0.crate
ba79285194a16b02512566a9a64d885567646045b144bb0efeef662001cd83a5
```

Three values had to agree and did:

1. the digest computed from the `.crate` file on this machine — above;
2. the `checksum` field of the `[[package]] name = "ziffle"` entry that stood in
   `Cargo.lock` **before** vendoring;
3. the digest named in `ZIFFLE_VERDICT.md` C-0.

**They match.** The vendored tree is an extraction of exactly that archive:

```
$ tar --force-local -xzf ziffle-0.1.0.crate -C vendor/ && mv vendor/ziffle-0.1.0 vendor/ziffle
$ diff -r --exclude=.cargo-ok ~/.cargo/registry/src/index.crates.io-*/ziffle-0.1.0 vendor/ziffle
(no output)
```

`.cargo-ok` is a seven-byte marker cargo writes after unpacking. It is not in the
archive and is not vendored.

### 2.1 The lockfile no longer carries the digest — which is why this file must

Once `[patch.crates-io]` redirects ziffle to a path, cargo rewrites the lockfile
entry as a path dependency and **drops the `source` and `checksum` lines**. After
vendoring, `ba79285…` appears nowhere in `Cargo.lock`. The digest's only home in this
repository is this file, and the per-file table below is what makes it checkable
without the tarball.

### 2.2 Per-file digests

Nine files, `120171` bytes. Recompute with `sha256sum` from `vendor/ziffle`:

| sha256 | file | bytes |
|---|---|---|
| `4ca7890d9ba6b2a11176e4ce1efb61ee46a6ab0ce1a2e33bcdfee285b3e15011` | `.cargo_vcs_info.json` | 94 |
| `c338b42066a3924b8687e27d39e14812c899b1a6e01ee4dd62916d2bfd58c23e` | `Cargo.lock` | 11 868 |
| `adfeaeb14dce29997b10d09d54c0c214efc837a438e3489341cf8dbec3ced362` | `Cargo.toml` | 1 801 |
| `14fc67be7f53f156352c4bd0e2c8dd812edfc7e9336c28de1eaca256de60bda8` | `Cargo.toml.orig` | 1 142 |
| `62fb8a3a9621dc2388174caaabe9c2317b694bb9a1d46c98bcf5655b68f51be3` | `LICENSE-APACHE` | 11 359 |
| `f123ebf0ce60977a8fe5c43b1ca6308cdb7208ddc199a205a524b1e9a4892570` | `LICENSE-MIT` | 1 071 |
| `173c3ad5bf101a056916fc7ee2e0cf5fb1c0590d3c9bf68368e2d62c1f3d808d` | `README.md` | 5 283 |
| `41c7bcbb7cb31c2f24d042853c883ceb3e204303a85d46094f7a800e7c0d9f68` | `src/lib.rs` | 62 908 |
| `3dcfad3d9a86923103b5a66f402d914b07f7a52697a56304f798b512476b4add` | `src/test.rs` | 24 645 |

This file is not in the table; it is ours, not upstream's. Nothing else in
`vendor/ziffle/` is ours, and nothing upstream shipped has been edited — the review
is a review of `src/lib.rs` at digest `41c7bcbb…`, and a diff is the check.

> **Correction to `ZIFFLE_VERDICT.md` C-0.** The row says *"nine files, 141 KB"*. The
> file count is right; the size is not. The nine files are **120 171 bytes**
> (117.4 KiB), and `du -sk` on this filesystem reports 81 KB. 141 KB is not
> reproducible by any measurement taken here and is not what the tree contains. The
> substance of C-0 is unaffected — the digest is what identifies the artefact, and it
> matched.

---

## 3. Why vendored rather than depended on

`docs/DEPENDENCIES.md` §3.1 states the risk. Three properties of it make a registry
dependency the wrong shape, and none of them is about trusting the author less than
any other author:

* **One author, one release.** `num_versions: 1`, a single crates.io owner, no
  organisation and no release cadence. There is no second maintainer who could
  publish 0.1.1 if 0.1.0 were yanked, and no history to infer a policy from. A yank
  or a repository deletion upstream is otherwise an unmitigated availability risk on
  the crate the entire mental-poker layer stands on.
* **A review is only auditable if the reviewed bytes are in the tree.**
  `docs/research/ZIFFLE_VERDICT.md` and its six supporting reports argue line by line
  about `MultiExpArg`, `SingleValueProductArg` and the forked Fiat–Shamir transcript.
  A review whose subject lives in a registry cache is a review of something a later
  reader cannot open. With the source here, `git log` on `src/lib.rs` is the record of
  whether the reviewed thing changed, and the answer to "is this still what was
  reviewed?" is a diff rather than a download.
* **Some of the verdict's conditions are edits.** C-1 and C-2 (`*_unchecked` paths,
  non-canonical encodings) and the D-1 fix are described in the verdict as small
  changes in a vendored fork. A fork needs a base commit in the tree with a recorded
  provenance to be a fork *of*. Today the vendored bytes are upstream-identical and
  the difference is exactly nothing; when that stops being true, this file's per-file
  digests are what makes the delta legible.

The mechanism is a path patch in the workspace manifest:

```toml
[patch.crates-io]
ziffle = { path = "vendor/ziffle" }
```

`ziffle = "=0.1.0"` in `[dependencies]` is unchanged, so the version constraint still
documents what we resolve to and the patch supplies it locally.

**What vendoring does not do.** It does not audit the crate, does not close OQ-1,
OQ-2 or OQ-5, and does not make ziffle fit for real money — `ZIFFLE_VERDICT.md` §1 is
explicit that it is not, and its own README says so too. It freezes bytes. That is
all it claims.

---

## 4. Licence: rely on the Apache-2.0 branch

The crate declares `license = "MIT OR Apache-2.0"` and ships both texts. **We take
the Apache-2.0 branch and rely on `LICENSE-APACHE` alone.**

The reason is not a preference. The shipped `LICENSE-MIT` begins:

```
Copyright (c) 2015 The cargo-readme Developers
```

ziffle has nothing to do with cargo-readme, and 2015 predates the crate by a decade.
The file is a copy-paste from an unrelated project, present upstream, in the published
tarball, covered by the sha256 above.

**This is a real compliance hazard, not a typo.** A licence file names the party
granting the rights. This one grants under the name of a copyright holder who does not
hold the copyright in this work and has not, so far as anything here shows, authorised
anything. Relying on it would mean relying on a permission from someone with no
standing to give it — the grant is void on its face for this crate, and no amount of
"obviously they meant themselves" fixes that, because the whole point of a licence
file is that intent does not have to be inferred. The MIT branch of the dual licence
is, as shipped, unusable.

The Apache-2.0 branch has no such defect. `LICENSE-APACHE` is the stock Apache License
2.0 text — 201 lines, `https://` forms of the apache.org URLs, no project-specific
edits — with the appendix's `Copyright {yyyy} {name of copyright owner}` placeholders
left as placeholders, which is the normal, correct way to ship it, since
Apache-2.0's grant comes from the act of distribution under the licence and does not
depend on a filled-in copyright line the way the MIT text does. `Cargo.toml`'s
`license = "MIT OR Apache-2.0"` is the author's own statement that either branch is
offered, so taking Apache-2.0 needs nothing further from anyone.

Consequences to observe:

* Apache-2.0 §4 requires the licence text and any `NOTICE` file to travel with
  redistribution. `LICENSE-APACHE` is vendored here; upstream ships no `NOTICE`.
* Any licence sweep of this repository (`docs/DEPENDENCIES.md` §6) must record
  ziffle's *effective* licence as **Apache-2.0**, not `MIT OR Apache-2.0`. The
  declared SPDX expression stays as upstream wrote it; the branch we exercise is
  narrower, and that narrowing is a decision recorded here rather than an accident.
* Do not "fix" `vendor/ziffle/LICENSE-MIT` by editing the name. Editing a third
  party's licence file is worse than leaving a broken one lying next to the branch we
  do not rely on. It stays byte-identical to what was published.
* Worth one upstream issue, at leisure — this is not blocking, because the Apache-2.0
  branch is sound on its own.

---

## 5. Verification performed at vendoring time

Machine: Windows 10, `x86_64-pc-windows-msvc`, `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`,
`rustc 1.95.0 (59807616e 2026-04-14)`, cargo held to 19 jobs by `.cargo/config.toml`.

| Check | Result |
|---|---|
| sha256 of the `.crate` recomputed locally | matches `ba79285…`, and matches the pre-vendoring `Cargo.lock` checksum |
| `diff -r` vendored tree vs. registry checkout (less `.cargo-ok`) | no differences — the vendored source is byte-identical to what was reviewed |
| file count / bytes | 9 files, 120 171 bytes |
| `cargo build` against the patched manifest | clean |
| `cargo test -- --test-threads=19` | clean |
| `tests/deck_constants.rs` (C-7) | 5 tests pass; both D-10 digests reproduce from this vendored tree |

`tests/deck_constants.rs` is the standing check that the vendored bytes and their
dependency versions still define the same 52 cards. If a `rand` or arkworks bump ever
moves `open_deck` or the Pedersen key, that test fails before anything ships.

---

## 6. Refreshing this file

`docs/DEPENDENCIES.md` §8.1's re-audit triggers apply. In addition, **any** change
under `vendor/ziffle/` — a fork carrying C-1, C-2 or the D-1 fix, or a bump to a later
upstream release — must, in the same commit:

1. update §2.2's per-file digests, and §1 if the upstream version or git sha moved;
2. state in §2 what changed and against which base, so the delta from the reviewed
   bytes stays readable;
3. re-run `tests/deck_constants.rs` — a fork that moves either D-10 constant is a wire
   break and needs a decision in `DECISIONS.md`, not a refreshed constant.

Never edit a digest to make a check pass.

---

## The fork

Applied 2026-08-29, on the owner's decision, under condition **C-12** of
`docs/research/ZIFFLE_VERDICT.md`. The verdict's reasoning for doing it now:
every item is a wire-format break, so it costs almost nothing before the first
proof is persisted or exchanged and a great deal afterwards. **Fork once.**

Each change is marked `FORK(x)` in the source.

### FORK(a) — the transcript was a different function on 32-bit targets

Four sites — not the three the review counted — framed lengths with
`usize::to_be_bytes()`, which is eight bytes on x86-64 and **four on wasm32**.
Two peers of different word widths derive different challenges from the same
statement, so every cross-width proof is rejected and the table splits in two
the day anything runs in a browser. No forgery: it fails closed. All four now
use `u64`.

Sites: `update_with_serialized`'s `serialized_size`, `append`'s `label.len()`,
`append_vec`'s `label.len()` and its element index.

### FORK(b) — the reveal token bound one ciphertext coordinate and dropped the other

`RevealTokenProof::challenge` absorbed `c1` and discarded `c2`, so a token
issued for one card was valid for **every** card sharing that first coordinate.
A malicious shuffler arranges the collision by reusing a re-masking scalar, and
the review demonstrated the attack end to end at 52 cards: collide a card the
protocol will legitimately open with a victim's hole card, and the honest
players' own tokens decrypt both.

This is the same mistake — binding one coordinate of a ciphertext instead of
both — that would have forged the **entire shuffle argument in one step** had it
been made in the shuffle transcript. It was made here, where the blast radius is
a player's hole cards rather than the deck's integrity.

The challenge now absorbs the whole pair, and `new`, `reveal_token` and `verify`
pass the card rather than a coordinate.

**Carries its own regression test**, `fork_reveal_token_bound_to_a_sibling_sharing_c1_fails`
in `src/test.rs`. The crate's existing `verify_reveal_token_wrong_card_fails`
does not catch this, because two cards from a shuffled deck differ in *both*
coordinates; the defect needs a collision in `c1` alone, so the test builds one
directly.

**Verified to reproduce the defect.** Reverting only the `c2` absorption makes
the test fail with its own message and restoring it makes it pass — checked, not
assumed.

### FORK(d) — the aggregate public key could not leave the process

`AggregatePublicKey` wraps a curve point in a private field and derives no
serialisation, while the individual `PublicKey` it aggregates derives both. So
the one value every seat must agree on before a card is opened was the one
value no caller could put on a wire.

`DECK_COMMIT` (`docs/PROTOCOL.md` §4.4) has each dealt-in seat commit to the
final deck hash, the index-map hash **and the aggregate key it derived**. That
stage is the cheap barrier that catches two peers holding different decks
before any hole card exists, which is the only moment at which catching it is
still cheap.

Added `AggregatePublicKey::as_public_key`, returning the point it already holds
as the `PublicKey` type the individual keys use. No arithmetic, no new
encoding: it goes through the same `serialize_compressed` every other point in
this protocol goes through. It is not a wire-format break — nothing that
verified before verifies differently now — and it is recorded here because
`vendor/ziffle/` is a reviewed artefact and an unrecorded edit to one is
indistinguishable from tampering.

### FORK(c) — a guard that never fired, and a comment that misleads

`Shuffle::<N>`'s `const _N_GREATER_THAN_1: () = assert!(N > 1)` was never
referenced, and an associated const is only evaluated where it is used — so
`Shuffle::<1>` and `Shuffle::<0>` compiled and then panicked at run time inside
the argument. It is now touched in `initial_deck`, on the path every deck takes.

*Honest limitation:* that the guard now fires is the standard idiom for forcing
evaluation, but **it is not asserted by a test** — a compile-fail assertion would
need `trybuild`, which the project does not depend on. It is reasoned, not
measured, and is recorded that way rather than counted as covered.

The step-5 comment read `a~(i + 2)` where the code computes `a~(i + 1)`, which is
what the paper requires. **The code was right and the comment was wrong**, which
is the dangerous direction: it is the comment that would lead a maintainer to
"fix" correct code into an unsound state. Corrected.

### The protocol tag

`Transcript::init` hashed `user_ctx` raw, with no length prefix and no tag. It
is now `"p2p-poker/ziffle-fork/v1"`, the context length, then the context — so
two callers using different context conventions cannot collide, and this
protocol's transcripts are distinguishable from another's built on the same
library.

### What was deliberately **not** changed

The verdict listed, as optional, deriving both sub-challenges from one joint
state instead of the forked transcript. **It is not done.** Two reviewers
independently established that the fork is the paper's own parallel composition
and binds every statement element the construction requires. Restructuring
something a review found sound is risk without benefit, and the review is only
worth having if its positive findings are respected as well as its negative
ones.

### Consequence

Proofs made by this fork do not verify under upstream `ziffle` 0.1.0, and
upstream's do not verify here. That is intended and is why the fork happens
before anything is persisted.
