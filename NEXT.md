# Where to pick up

Updated 2026-08-29. Tree is clean, everything committed.

    cargo clippy --all-targets                  0 warnings
    cargo test --release -- --test-threads=19   255 unit + 38 harness, 0 failed

Harness breakdown: 16 adversarial (mock), 10 adversarial (real backend),
8 deck constants, 4 random-hand drivers over ~120 000 hands.

## What landed since the last note

**The ziffle decision was taken: fork.** `docs/DECISIONS.md` **D-016**, closing
`ZR-1`. `vendor/ziffle/` is no longer the crates.io tarball; four changes, each
marked `FORK(x)` and recorded in `vendor/ziffle/PROVENANCE.md`. The two that
forced it were wire-format defects — a 32/64-bit transcript split, and a reveal
token that bound one ciphertext coordinate and dropped the other.

**Phase 5 is written.** `src/mental_poker/` is no longer stubs:

* `protocol.rs` — the boundary. `structural_check` and its initial-link sibling,
  the `DeckWire` length-and-canonicality gate, the `DeckCtx` construction site,
  `Verified`/`Final`, and the `Invalid` versus `CouldNotVerify` distinction.
* `deck.rs` — the §4.5 index map, `CardIndex`, `index_map_hash`.
* `shuffle.rs` — the chain. C-6's five rules enforced rather than described, and
  C-11 written where the chain is driven.
* `reveal.rs` — who may open what, to whom, and the `SoundnessFault`.
* `backend.rs` — the real ziffle implementation. The only module in the crate
  that names a library type.

**Phase 6 gained its real half.** `tests/adversarial_backend.rs` runs the
review's B1, B2, B4, B5 and B6 against the shipped code rather than a mock.

**Verdict conditions:** C-0 to C-15 are done except the differential test half of
C-14, which needs `paritytech/mental-poker` linked in.

## Two things that were found by writing this, and are worth remembering

`role_code` was used normatively in `PROTOCOL.md` §4.5's `index_map_hash` and was
**never defined anywhere in the corpus**. The construction reads complete, so
seven passes did not ask. Two conforming clients would have picked two reasonable
encodings and mismatched `DECK_COMMIT` every hand.

`is_identity_c1` — the check that refuses a deck position left in the clear, the
highest-value function in the whole review — tested a flag bit that nobody had
ever compared against what arkworks emits. It was right. Had it been wrong the
check would not have failed; it would have **silently never fired**. Now measured
in `tests/deck_constants.rs`.

## Next actions, in order

1. **Finish Phase 4**: `CheckpointStore` and `CheckpointState`, T49/T50/T51, the
   freeze path with T62/T63, and validation steps 12 and 12a. All were cleared to
   build by `PHASE4_GATE2.md` and none of them is blocked.

2. **Wire the deck to the engine.** `mental_poker` and `poker` do not touch yet:
   nothing deals a hand from a verified deck into `BettingRound`. That seam is
   where the index map, the reveal policy and the engine meet, and it is the
   first place an integration defect would live.

3. **Phase 7, the network**: libp2p transport, then the DHT lobby under the fixed
   `LOBBY_INFOHASH`, then GossipSub for the table mesh.

4. Phase 9 GUI, Phase 10 integration, Phase 11 audit.

## Open items

`docs/DECISIONS.md` carries D-001 to D-016 and the open list. Nothing in it
blocks item 1 or item 2.

**F-1** is still open and is the one to watch: D-015 removed the timeout
machinery, which made `Q-01`'s answer load-bearing. If the answer is `TDA_MUCK`,
showdown gains a human decision with no automatic rule behind it — and D-015 took
away the machinery that would have covered it.

**The project licence** has never been chosen. It is the owner's, and it gates
`deny.toml` and `ZR-4(a)`.
