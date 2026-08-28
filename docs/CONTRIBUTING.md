# Contributing — Git discipline and the rules of evidence

**Status:** authoritative for process. Assigned by `research/PHASE0_FIXPLAN.md` ruling
**D-6** as the owner of `SPEC_CS.md` §31, created at the close of Phase 1.
`CRYPTOGRAPHY.md` §12 and `PROTOCOL.md` §9.6 already point here; this is the target
those pointers were waiting for.

**Authority.** `docs/DECISIONS.md` outranks this file and every specification
document, and **this file does not restate its contents** — not the decisions, not
their numbers, not their count. That is D-011's one-normative-owner rule applied to a
process document, and it is applied here because this document failed it: the
authority order that stood in this place until now still read *"D-001 … D-008"* four
decisions after D-012 was accepted. A list of D-numbers copied into a second file is
precisely the copy that drifts.

So read `DECISIONS.md` there, not here. `SPEC_CS.md` is the binding specification;
`docs/research/` is **evidence and never authority** (§2.2); where a specification
document and a numbered decision disagree, the decision wins and the document is
corrected. **D-009 to D-012** are the four that decide how a change is written and
reviewed, and §3 below is the checklist they produce.

This is a security project before it is a poker project. Most of what follows exists
because `SPEC_CS.md` §35's invariant has to survive a modified client, and process is
one of the things that keeps it alive between commits.

---

## The two rules this project actually paid for

Both were learned by getting them wrong, repeatedly, and both matter to a
contributor more than anything below about commit style.

**1. Every API claim is verified by compiling or by reading crate source — never
from documentation, never from a changelog, never from memory.** One documentation
sentence taken on trust propagated a wrong number through four documents. §2.1 has
the rule, the two acceptable forms of evidence, and the worked example.

**2. Every decision sweep covers every document, including this one and
`DEPENDENCIES.md`.** Three times a sweep was scoped by judgement to the documents a
decision "obviously" touched, and three times an omitted document was the next
blocking defect. §2.5 has the rule and the three instances.

---

## 1. `SPEC_CS.md` §31, in full

The section is four sentences. All four are binding.

> Use Git from the beginning.
> Make small logical commits.
> Never rewrite or delete large parts of a working implementation without
> justification.
> Before a security-critical change, add a regression test that reproduces the
> problem.

### 1.1 Small logical commits

One commit does one thing and leaves the tree building and green. The unit is a
**logical change**, not a file and not a work session.

* A commit that adds a rule and the test that pins it is one commit. A commit that
  adds a rule and reformats four unrelated files is two, and the second one hides
  the first.
* Commit messages in this repository are already consistent and that convention
  holds: a subject line of the form `Phase N: what changed`, lower case after the
  colon, no trailing full stop, under ~72 characters; then a blank line; then a body
  that explains **why**, in prose, wrapped near 80 columns. Look at
  `git log` before writing one.
* The body is where the reasoning goes. `Phase 2: side pots, split pots and the
  odd-chip rule` explains in its body why pots are derived rather than accumulated
  and what that buys — that is the standard. A body that only restates the subject
  is a body that will not help the person who bisects to it.
* Where tests were run, the message records the command and the result, e.g.
  `cargo test -- --test-threads=19   22 passed, 0 failed`.
* Commits are in English. So is the code, and so are the documents. (Conversation is
  in Czech; the repository is not.)

### 1.2 Never rewrite or delete large parts of a working implementation without justification

**Correct; do not rewrite.** Keep what is not at fault. This is the rule that is
easiest to break with good intentions — a rewrite feels like progress and looks like
a large diff, and it silently discards every constraint the old code had absorbed
that nobody wrote down.

Before deleting or rewriting a working block, the commit must answer, in its body:

1. **What is wrong with the existing code**, concretely — not "it was messy".
2. **What behaviour is preserved**, and how you know. A test that passed before and
   after is the good answer.
3. **What behaviour changes**, deliberately, and why that is correct.

If you cannot write those three paragraphs, the rewrite is not ready. This applies
with full force to the documents as well as the code: a specification paragraph that
took a position is not "tidied up" into one that takes none.

The prohibition covers two failure modes seen in this corpus already:

* **Softening by attrition.** An estimate quoted as a bare number in three documents
  becomes a measurement. `CRYPTOGRAPHY.md` §6.5 now labels its estimates for exactly
  this reason. Never remove a hedge without saying in the commit why the hedge is no
  longer true.
* **Widening by copy.** A shared constant retyped rather than copied drifts by one
  character and the two sides stop agreeing. See §4.3.

### 1.3 The load-bearing clause: a regression test lands first

> **A security-critical change is preceded by a regression test that reproduces the
> problem.**

Not "accompanied by". **Preceded by.** The test is written first, it is run first,
and **it must fail** for the reason you think it fails. A test that passes before the
fix proves nothing at all — it is a test of something else, and it will keep passing
after the defect returns.

The procedure, and there is no shortcut through it:

1. Write a test that reproduces the defect.
2. Run it. **Watch it fail**, and read the failure message to confirm it is failing
   for the stated reason rather than for a typo, a missing fixture or a panic
   somewhere earlier.
3. Commit the failing test if it can be committed `#[ignore]`d or behind the fix in
   the same series; otherwise commit test and fix together, with the message stating
   that the test was observed to fail first and what the failure said.
4. Apply the fix.
5. Run the test. Watch it pass.
6. Run the whole suite (§5). A fix that greens one test and reds another is not a
   fix.

**What counts as security-critical.** Any change touching:

* anything enumerated in `CRYPTOGRAPHY.md` §12 or `PROTOCOL.md` §9.6 — those two
  sections point here precisely because everything they list is in scope;
* the deck construction, shuffle proofs, reveal tokens, or the `ctx` binding;
* signatures, the signed envelope, the transcript, or `state_hash`;
* the anti-replay slot key, the equivocation predicate, or any message type added to
  the wire — D-009 rule 1 has been violated five times, once per pass;
* the timeout certificate, the voter set `V`, the abort path, or attribution.
  **D-010 left all of this specified and produced but gave it no automatic
  effect**, which makes a change that quietly restores an effect — a chip moved, a
  seat removed, a peer blocked — the most dangerous kind in the corpus;
* anything that enters a state hash, a roster hash, a chained event body or the next
  hand's genesis (**D-012**);
* pot construction, the eligible-set rule, or the odd-chip rule — chips moving
  wrongly is a security failure, not a cosmetic one;
* message size limits, collection bounds, rate limiting, or connection limits
  (`SPEC_CS.md` §27's "introduce maximum sizes for all messages and collections");
* key generation, key storage, key slots, or randomness;
* anything that changes which crate does the above — a dependency bump into that set
  is a security-critical change, see §6.

When in doubt it is in scope. The cost of an unnecessary regression test is a few
minutes; the cost of the other error is the record D-010 sets out — four adversarial
passes, each finding a defect worse than the one before, every one of them ending in
an honest peer's chips being taken for following the rules.

### 1.4 A gate that has never been observed to fail is not a gate

§1.3 is one instance of a rule D-009 states generally. **A check that has only ever
been seen to pass is indistinguishable from a check that cannot fail.** So when a
change adds a *standing* gate — a lint, a source scan, an invariant assertion, an
adversarial test meant to catch a class rather than an instance — the gate is made
to fail on purpose, once, and the observed failure is recorded in the commit body.

The worked example is in the tree, and it is the one D-009 rule 3 cites.
`src/security/rng.rs` carries
`tests::our_own_code_uses_no_generator_but_the_os_one`, which walks this crate's own
`src/` on every test run and fails the build on `SmallRng`, `StdRng`, `thread_rng`,
`from_seed`, `seed_from_u64` or `rand::rngs`. It was verified by injecting a
violation and watching the assertion fire:

```
SPEC_CS.md section 7 forbids these generators for cryptographic use;
draw from security::rng::fill instead:
  poker\actions.rs:8: SmallRng
test result: FAILED. 0 passed; 1 failed
```

and then removing the violation and watching it pass. Without that step, a green
tick is equally consistent with a scan that walked no files, matched no needle, or
was handed the wrong directory. The same test guards against the first of those
directly — `assert!(files.len() > 10)` — because a scan whose coverage collapsed
returns a pass that looks exactly like a clean tree.

Note what that gate asserts and what it does not. It asserts **a discipline in our
own code**. It does not assert that `SmallRng` is absent from the binary: it is in
the binary, it cannot be removed, and claiming otherwise is what D-009 rule 3 was
written to stop. See §4.5.

---

## 2. Evidence: how a claim earns its place

`SPEC_CS.md` §36 closes with *"security-critical claims must be demonstrable"*. These
rules are how that is enforced in day-to-day work, and all four came from this
project getting it wrong first.

### 2.1 Every API claim is verified by compiling or by reading crate source

**Never from documentation, never from a changelog, never from memory.** Rust crate
documentation lags, changelogs lie by omission, and a model's memory of an API is a
plausible-sounding guess.

Two acceptable forms of evidence, and no third:

* **(a) compiled** — you wrote a probe in a scratch directory and `cargo check` or
  `cargo test` succeeded, or failed in the way you claim. A verified *negative* is
  evidence too: `DEPENDENCIES.md` §3.6 records that `rs_poker 5.1.0` does not build
  on stable 1.95.0, with the `error[E0658]` it produces, and that negative is why the
  crate is `=`-pinned.
* **(b) source** — you read the crate's own code under
  `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`, and you cite
  the file and the line.

A third form, **(c) registry**, is acceptable for facts *about* a crate rather than
about its API: version, licence, publication date, owner, advisory ranges — read from
the crates.io API or from `~/.cargo/advisory-db/`. It never establishes behaviour.

Mark which one you used, in the document or in the commit body. The corpus already
uses `(a)` / `(b)` / `(c)`, and `NETWORK_STACK.md` uses `[SOURCE]` /
`[RESEARCH+COMPILED]`; either notation is fine, an unmarked claim is not.

**Worked example of why this rule exists.** The fix plan asserted that libp2p's
`max_circuit_bytes` was a budget "per circuit **and per direction**". Reading
`libp2p-relay-0.21.1/src/copy_future.rs` shows a single `bytes_sent: u64` field that
**both** `forward_data` calls increment — src→dst at lines 88–95 and dst→src at
97–104 — so it is a **bidirectional total** and the default 131 072 is 128 KiB of
combined traffic, not 128 KiB each way. Every figure derived from it had to be
recomputed and halved. Kubo's `docs/config.md` describes the same knob as "in each
direction"; either the Go and Rust implementations differ or that wording is loose.
Our relay is Rust, so the Rust behaviour binds, and for third-party relays the
stricter reading is the safe assumption. **One documentation sentence, taken on
trust, propagated a wrong number through four documents.**

**Never invent an API, a type or a cargo feature.** If you cannot find it, it does not
exist, and the correct output is to say so. `NETWORK_STACK.md` §5.1 records a real
instance: there is no `connection-limits` cargo feature in libp2p — asking for it is
a hard resolver error — while `memory-connection-limits` does exist. That distinction
is only findable by trying it.

### 2.2 Research documents are evidence; `DECISIONS.md` is authority

Everything under `docs/research/` is a record of what somebody measured on some day.
It is the best evidence we have and it is **not** binding. A research document can be
wrong, and three of them are: `DEPENDENCIES.md` §9 lists the corrections.

When evidence and authority disagree:

* If `DECISIONS.md` says it, it holds until a new numbered decision changes it.
  Nothing in a research note, a specification document or a commit message overrides
  a D-number.
* If a research note is contradicted by a measurement, **the note is corrected and
  the correction is also carried in the authoritative document**, so a reader never
  has to open the research note to learn that a row in it is wrong. That is why
  `CRYPTOGRAPHY.md` §9.2 restates the `rand` correction rather than linking to it.
* A new decision that changes what ships, or that narrows a security claim, is a
  **numbered entry in `DECISIONS.md`** in the same commit. Not a paragraph in a
  specification document, not a comment in the code.
* **Correct the owner, and only the owner.** D-011 rule 1 gives every concept one
  normative document — `PROTOCOL.md` the wire, `STATE_MACHINE.md` transitions,
  `CRYPTOGRAPHY.md` constructions, `NETWORK_STACK.md` transport, `THREAT_MODEL.md`
  classifications. A non-owner references the owner's section by number and never
  reproduces its text. Five passes each found the same shape, and a copy is what
  drifts.

### 2.3 "Prevented" and "detected" are different claims, and the stronger one must be justified in the commit

`SPEC_CS.md` §36's closing line forbids ever claiming the system makes all cheating
impossible, and requires each attack class to be sorted into **cryptographically
prevented (CP)**, **detected and attributable (D&A)**, or **out of scope**.

So: **a commit that claims an attack is *prevented* rather than *detected* must
justify that word in its body.** Name the mechanism, name the assumption it rests on,
and name what would have to be true for the claim to fail. "Prevented" is a claim
about mathematics — a forgery would have to break a hardness assumption or a
signature scheme. "Detected" is a claim about the protocol — someone deviates, the
transcript shows it, the hand stops, and the deviation is attributable to a peer.

If the mechanism is a *check that some peer performs*, it is detection, not
prevention, no matter how reliable the check is. If the honest party has to be
present and paying attention for the guarantee to hold, it is detection.

**Prefer weakening a claim to defending it.** `SPEC_CS.md` §18, §36 and its closing
paragraph all forbid smoothing over a gap. A downgraded claim with an open question
attached is a good commit. A claim defended with an argument you had to invent is
the failure mode the whole specification is written against — and §36 says what to
do instead: stop, describe the problem, find an established or peer-reviewed
construction, compare it against our threat model, and only then implement.

### 2.4 Open questions stay open

An `OQ-n` or an entry on `DECISIONS.md`'s open list is closed by evidence or by a
decision, never by a rewrite that stops mentioning it. If a commit removes an open
question, its body says which of the two happened.

### 2.5 Every decision sweep covers every document

When a numbered decision lands, **every document in `docs/` is read against it** —
the five specification documents, `SPEC_CS.md`, this file, and `DEPENDENCIES.md`.
Not the documents the decision appears to touch. All of them.

There is no judgement call to make here, and that is the point. The rule exists
because the judgement call has now failed three times, and each failure was found
by a later pass as a blocking defect rather than by the sweep that caused it:

1. **D-010 skipped `NETWORK_STACK.md`** on the reasoning that neutral abort was a
   state-machine concern. D-010 point 3 forbids automated eviction, eviction is a
   transport action, and that document held ten places that blocked a peer on a
   proof — with zero mentions of D-010. Found by the Phase 2 gate; it became D-011
   rule 3.
2. **D-011 skipped `CRYPTOGRAPHY.md`**, whose ownership table still declared the
   pre-D-011 discipline while having zero mentions of D-011. Found by the sixth
   pass; it is why D-012 states this rule.
3. **`CONTRIBUTING.md` and `DEPENDENCIES.md` were skipped by every sweep there has
   ever been.** Both had zero occurrences of D-009, D-010, D-011 and D-012 until the
   pass that wrote this section. §3's checklist had been directing reviewers at a
   scoping rule that D-010 had already made inert.

The arithmetic is not close. The cost of including a document that turns out to be
unaffected is one reader reporting that they found nothing — and **reporting it, in
the commit body, is part of the sweep**; silence is indistinguishable from having
skipped the file. The cost of excluding one has three times been a blocking defect.

---

## 3. Review checklist for a security-critical change

Run through this before pushing. Each line is here because something in this corpus
failed it, and the four decisions that shape it are **D-009, D-010, D-011 and
D-012** — read them in `DECISIONS.md`, not from this summary.

1. **Does a regression test reproduce the problem, and was it observed to fail
   first?** And if the change adds a standing gate, **was that gate made to fail on
   purpose?** (§1.3, §1.4)

2. **Can any sequence of actions the protocol *requires* of an honest peer produce
   a valid `EquivocationProof` against that peer?** (D-009 rule 1.) This is the
   defect that has come back five times, once per pass, wearing a different message
   name each time — lobby traffic, `DISPUTE`, `TIMEOUT_VOTE`, `STATE_HASH`
   re-emission, terminal `HAND_ABORT`. If the change adds or alters a message type,
   check its honest emissions against the **one literal slot-key tuple in
   `PROTOCOL.md`** (D-011 rule 2). Do not re-derive the key from the principle, and
   do not reproduce the tuple anywhere else: re-derivation by each editor is exactly
   how the fifth instance happened. A message whose honest emission can collide in
   that key is a defect in the message, not in the key.

3. **Does anything give a below-floor timeout certificate an effect?** (D-009
   rule 2.) A certificate with `|V| < 2` is **inert everywhere and at every table
   size** — not chained, not evidence, no `AbortRecord`, no terminating effect, no
   forfeiture, including for `kind = Crypto` and for the hand deadline. It is
   silently ignored. Where a certificate *is* gated, D-008's scoping still binds:
   the gate is on `|V|`, never on `n`, and a seat leaves `V` only once a **completed,
   valid** certificate names it.

4. **Is any security property stated as the absence of something from the dependency
   tree?** (D-009 rule 3.) Never. State the discipline our own code follows and
   enforce it mechanically. Three claimed absences have now been measured false in
   the integrated tree; §4.5 and `DEPENDENCIES.md` §5.4 are the standing examples.

5. **Does the change move a chip, unseat a player, or block a peer on the strength
   of a proof?** (D-010, and D-011 rule 3 for the transport layer.) An abort is
   **neutral**: stacks are restored to their start-of-hand values, attribution is
   recorded as evidence with no automatic consequence, and there is no automated
   eviction at any layer — no `block_peer`, no unseating, no allow/block list driven
   by a protocol proof. A proof is evidence for a human; the user may always decline
   to play with someone, and the protocol may not decide that for them. D-010
   dissolved a four-pass attack class by removing the prize, so restoring a
   consequence restores the class.

6. **Does the change restate something another document owns?** (D-011 rule 1.)
   `PROTOCOL.md` the wire, `STATE_MACHINE.md` transitions, `CRYPTOGRAPHY.md`
   constructions, `NETWORK_STACK.md` transport, `THREAT_MODEL.md` classifications.
   Reference the owner's section by number; never reproduce its text. Where two
   documents disagree today, the owner wins and the other is corrected — not
   reconciled.

7. **Does any canonical state derive from a quantity that can differ between honest
   receivers?** (D-012.) Nothing entering a **state hash, a roster hash, a chained
   event body, or the next hand's genesis** may come from one: not which copy of a
   message this peer happened to accept, not a local timer, not a per-peer view, not
   a field of an unchained record. Canonical state changes only through a chained
   event every participant accepted; everything else is a local view and belongs in
   the structure the state machine already separates for it. This is the shape a
   *fix* creates — H1 sat on the path P3's fix had newly made load-bearing — so it
   is checked hardest on the code a previous change just started to rely on.

8. **Does anything new read the wall clock inside the state machine?**
   `STATE_MACHINE.md` I22 asserts replay determinism. This is item 7's most common
   special case: a clock read is a per-receiver quantity by definition, and removing
   the last one was the point of making the certificate stage collective.

9. **Does any claim say "prevented" where the mechanism is a check?** (§2.3)

10. **Is every new API claim marked (a) compiled or (b) source, with a citation?**
    (§2.1)

11. **Is every shared constant and domain-separation string byte-identical
    everywhere it appears?** (§4.3)

12. **Is there a new unbounded collection, an unbounded loop, or a message with no
    size limit?** `SPEC_CS.md` §27.

13. **Does the change alter what a peer can assert unilaterally?** That is the shape
    of every A-class finding in this project so far.

14. **If this commit carries a numbered decision, was every document read against
    it, and does the body say so — including the ones where nothing changed?**
    (§2.5)

15. **Did the whole suite run at 19 test threads, and does the commit body record
    what it actually said?** (§5.) A suite that is red is recorded as red.

---

## 4. Repository conventions

### 4.1 Branches and history

`master` is the mainline. History is linear and readable; every commit on it builds
and its tests pass. **Do not rewrite published history** — no force-push, no rebase of
anything already pushed. That is the same rule as §1.2, applied to the commit graph.

**Never push unasked**, and never before a real build has succeeded. `cargo check`
and `-fsyntax-only`-style checks do not link and do not run tests; they are not a
build.

### 4.2 What never enters the repository

`.gitignore` is not advisory. In particular `/profile/` holds identity keys, settings
and hand history, and the client is portable so the profile lives next to the binary.
**A committed private key is a security incident, not a mistake to quietly amend
away** — it is rotated, and the incident is recorded.

`Cargo.lock` **is** committed, deliberately: `SPEC_CS.md` §28 requires reproducible
pinning, and `DEPENDENCIES.md` is measured from it.

### 4.3 Shared constants and domain strings: copy, never retype

A constant or a domain-separation string that appears in more than one place must be
**byte-identical** in all of them. Copy and paste it; do not retype it and do not
"fix" its capitalisation, spacing or Unicode.

This is not style. `PROTOCOL.md` §13 is the single definition of every two-sided
numeric constant, and the Fiat–Shamir domain tags (for example
`ziffle/BG12MultiExpArgX/v1` and `ziffle/BG12ProductArgX/v1`) are what separate two
sub-arguments that otherwise share a transcript state. A one-character drift in a
domain tag does not fail to compile and does not fail a happy-path test; it silently
removes a separation the security argument depends on.

The same applies to `ctx`. Its canonical form is normative in `PROTOCOL.md` §4.5 and
reproduced in §6.4; if you need it, copy it from there.

### 4.4 Layering

`SPEC_CS.md` §23 requires the separation of transport, poker engine and cryptographic
deck to be preserved. In practice:

* nothing libp2p-typed appears above the `NETWORK_STACK.md` §1.3 `Transport` trait;
* no module outside `src/mental_poker/` names a `ziffle` type — the `DeckCrypto`
  trait in `CRYPTOGRAPHY.md` §9 is the boundary, and it is the mitigation for
  ziffle's unaudited state, not speculative generality;
* `src/poker/evaluator.rs` is the only file that names `rs_poker`.

A commit that punches through one of these boundaries has to say why in its body, and
usually should not exist.

### 4.5 Randomness

`SPEC_CS.md` §7: our own code draws cryptographic randomness only from
`getrandom::SysRng`. `rand`'s `SmallRng`, `StdRng` and any self-seeded generator are
never used for keys, masking factors, permutations or commitments.

Note what this is **not**: it is not a claim that `rand` is absent. Three majors of
`rand` are compiled — `DEPENDENCIES.md` §5.4 lists them and their parents, and
`rs_poker::core`'s own deck and sampler are among the things that must never be
called. `SmallRng` in particular **is** in the binary and cannot be removed short of
dropping libp2p features we need. The rule is a lint on our code, because that is
the version of it we can actually enforce — which is **D-009 rule 3**: state the
discipline we can enforce, never an absence we cannot. The lint is
`src/security/rng.rs`, and §1.4 records the injected violation that proved it
bites.

---

## 5. Build and test

`.cargo/config.toml` holds cargo to **19 of this machine's 24 cores** so the box stays
usable and a runaway job cannot starve everything else:

```toml
[build]
jobs = 19
```

**Keep it.** `cargo test`'s harness has its own thread pool that ignores `jobs`, so
the same ceiling has to be passed explicitly every time:

```bash
export PATH="~/.cargo/bin:$PATH"   # cargo is not on the Bash PATH

cargo check                                    # fast type check, does NOT link
cargo build                                    # a real build
cargo test -- --test-threads=19                # the suite, held to 19 threads
cargo test <filter> -- --test-threads=19       # one module while iterating
cargo build --release                          # what gets profiled (SPEC_CS.md §33)
```

**Current state, measured 2026-08-28 on rustc 1.95.0 — the suite is red.**
`cargo test --no-fail-fast -- --test-threads=19`:

| Target | Result |
|---|---|
| unit tests (`src/`) | **54 passed, 0 failed, 0 ignored** — including `security::rng::tests::our_own_code_uses_no_generator_but_the_os_one` |
| `tests/random_hands.rs` | **0 passed, 4 failed** — `heads_up_hands_conserve_chips`, `six_handed_hands_conserve_chips`, `ten_handed_hands_conserve_chips`, `tiny_stacks_exercise_the_all_in_paths`, all on `assertion left == right failed: chips moved on Raise(...)` at `tests/random_hands.rs:161` |

An earlier version of this section recorded *"22 passed, 0 failed"*. That figure is
from a smaller tree and is not what the suite says today; it is corrected here rather
than left standing, because §1.3's whole procedure depends on a contributor knowing
which failures are theirs. **The four failures above are pre-existing and are not a
baseline to build on** — chip conservation is a `SPEC_CS.md` §35 property, so under
§1.3 these are security-critical failures and they are the tree's current blocker,
not a flaky test to work around. Do not push a change that adds a fifth.

This section is measured, not remembered. Re-run the command and correct the table
in the same commit whenever it moves.

Dependency and supply-chain checks, before any commit that touches `Cargo.toml` or
`Cargo.lock`:

```bash
cargo audit                                    # lockfile-based; over-reports, see below
cargo tree --edges normal   -i <crate>         # is it actually in the build?
cargo tree --edges features -i <crate>         # which of its features are on?
cargo tree --duplicates --edges normal         # duplicate majors
cargo deny check licenses                      # needs a deny.toml first — not written yet
```

**`cargo audit` currently exits non-zero, and that is the expected state.** Re-run
2026-08-28, 1 226 advisories, 612 lockfile packages: four findings, unchanged, all
recorded and justified in `DEPENDENCIES.md` §4:
RUSTSEC-2026-0118 and RUSTSEC-2026-0119 (`hickory-proto`), RUSTSEC-2024-0436
(`paste`), RUSTSEC-2026-0253 (`lru`). A clean `cargo audit` is not the goal and would
currently mean something had gone wrong. Note also that a `Cargo.lock` is
feature-independent, so `cargo audit` reports crates that are not compiled at all;
`cargo tree --edges normal -i` is the tool that answers what is really in the binary.

**`clippy` is installed; `rustfmt` is not.** Measured 2026-08-28: `cargo clippy
--version` reports `clippy 0.1.95 (59807616e1 2026-04-14)`, while `cargo fmt
--version` fails with *"'cargo-fmt.exe' is not installed for the toolchain
'stable-x86_64-pc-windows-msvc'"*. An earlier version of this section said neither
was installed; that has drifted. Add the missing one with
`rustup component add rustfmt`, and **do not claim a lint or format pass that did not
run** — `cargo deny` is installed too (`cargo-deny 0.20.2`) but has no `deny.toml`,
so it is not yet a gate either (`DEPENDENCIES.md` §7).

---

## 6. Changing a dependency

A dependency change is a security-critical change whenever the crate is in
`DEPENDENCIES.md` §5. It follows §1.3 like any other, and additionally:

1. **Update `Cargo.lock` and commit it** in the same commit.
2. **Run `DEPENDENCIES.md` §10's commands** and update that register: the 425 / 611 /
   186 counts, the affected §5 rows, and §4's advisory table.
3. **Do not remove a `=` pin without reading why it is there.** `ziffle`, `mainline`
   and `rs_poker` are exact-pinned for recorded reasons — an unaudited single-author
   crate, an internals dependency that is not semver-stable in practice, and a
   version of `rs_poker` that a `^` requirement would upgrade into a build failure.
   "Modernising" any of those three to a caret is a defect.
4. **Determine reachability in source, not by severity label.** If a new advisory
   lands, find the vulnerable function, check whether the feature that compiles it is
   on, and check whether our call pattern meets its preconditions. Cite file and
   line. `DEPENDENCIES.md` §3.2 and §3.3 are the worked examples and set the bar.
5. **Accepting an advisory follows `DEPENDENCIES.md` §8.3's three tiers.** Anything
   touching the deck, our signatures, the transcript, key storage or the transport's
   encryption **cannot be accepted** — it blocks, and if there is no upstream fix the
   answer is to replace the crate or to stop. "Low severity" and "denial of service
   only" are not arguments; `SPEC_CS.md` §19 ranks security above finishing a hand
   conveniently.
6. **An acceptance expires with the version it was written against**, including
   across a patch bump, because the arguments are about specific lines of upstream
   source.

---

## 7. Phases

`SPEC_CS.md` §30 runs Phase 0 to Phase 11 and the order is deliberate: threat model
and specification before code, deterministic engine before networking, adversarial
tests before transport. **Run the tests after every phase, and do not push past a
failure just to get the application "running somehow".**

Work that belongs to a later phase does not land early because it is easy, and work
that a phase depends on does not slip because it is hard. Where a phase is genuinely
blocked, the blocker is written down with its destination named — a requirement with
no owner is how a requirement gets lost, which is exactly what ruling D-2 and this
document's own ruling D-6 were opened to fix.

(**A note on two numbering schemes**, because they are one keystroke apart and this
document now cites both. `D-2` and `D-6` are *rulings* in
`research/PHASE0_FIXPLAN.md`, which assign a home to a `SPEC_CS.md` section. `D-002`
and `D-006` are *decisions* in `DECISIONS.md`, which bind. Three digits means
binding; one digit means a filing decision in a research note, and a research note is
evidence, not authority.)
