# Where to pick up

Paused 2026-08-29, early morning. Tree is clean, everything committed.

    cargo clippy --all-targets                  0 warnings
    cargo test --release -- --test-threads=19   165 unit + 4 harness, 0 failed

## The one thing to read first

`docs/research/ZIFFLE_ATTACKS.md` §0. Twenty-two attack families were run
against the real crate. The Bayer-Groth shuffle argument **held** - no forgery,
and the forger got arithmetically complete before being stopped by one specific
line. But **six attacks around it succeeded**, B1 to B6, and they are the real
result:

* **B1** a reveal token for one card decrypts a *different* card. Executed end
  to end at 52 cards. The proof binds one ciphertext coordinate and drops the
  other - the same mistake that, made in the shuffle transcript, would have
  forged the whole deck in one step. Blast radius here is a player's hole cards.
* **B2** a shuffler can leave chosen slots in plaintext and the proof verifies.
* **B3** a shuffler can publish a proof from which anyone recovers his whole
  permutation. All 52 positions were recovered from public data.
* **B4** a proof has 2^66 accepted encodings; trailing garbage ignored.
* **B5** `pk = identity` passes `OwnershipProof::verify` with a proof anyone
  can write.
* **B6** a player can rebroadcast another's proof as his own.

None is a break of Bayer-Groth. All six are the crate proving exactly what it
claims where the claim is not what a poker game needs. §9 of that report has
the rules that close them; most cost a few lines and belong in our wrapper.

## Next actions, in order

1. **Re-run the verdict agent.** It was the only one still running when work
   stopped, and its five inputs are all on disk:

       Workflow({scriptPath: "~/.claude/projects/<session>/<session-id>/workflows/scripts/p2p-poker-ziffle-review-wf_3a03927b-f55.js",
                 resumeFromRunId: "wf_3a03927b-f55"})

   The five review agents and the attacker replay from cache; only the verdict
   runs. It owes the `DeckCrypto` trait and a row in `DECISIONS.md`'s open list.

2. **Decide on ziffle** - the owner's call, and the report gives the evidence
   for it. Fit for a play-money MVP behind the §9 wrapper rules, or look
   further. Note the B1 argument: the author made the coordinate-binding
   mistake once, and its severity depended only on where.

3. **Finish Phase 4**: the three anti-replay stores, `CheckpointStore` and
   `CheckpointState`, T49/T50/T51, the freeze path with T62/T63, and validation
   steps 12 and 12a. All were cleared to build by `PHASE4_GATE2.md`.

4. Then Phase 5 (mental poker), Phase 6 (the adversarial suite, including
   D-014's mirror test: no honest peer is ever evictable), then network and GUI.

## Open items

`docs/DECISIONS.md` carries D-001 to D-014 and 55 open rows. Nothing in the
list blocks item 3.
