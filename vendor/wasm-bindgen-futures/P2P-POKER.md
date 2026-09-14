# Why this crate is vendored

**Only so the dependency graph resolves.** It is never compiled for this project: its one
dependent here, `libp2p-swarm`, uses it on `wasm32` only, and p2p-poker builds for Windows.

`libp2p` 0.57 is the version that leaves three advisories behind (GitHub's Dependabot on
2026-09-14): `yamux` < 0.13.10 (GHSA-vxx9-2994-q338, a remote panic on a malformed data frame),
and `hickory-proto` 0.25.x (GHSA-3v94-mw7p-v465, an unbounded loop in NSEC3 validation;
GHSA-q2qq-hmj6-3wpp, quadratic name compression). But `libp2p-swarm` 0.48.0 pins
`wasm-bindgen-futures = "=0.4.58"` -- upstream's own note: newer wasm-bindgen breaks
`gloo-timers` 0.2.6 on wasm -- and 0.4.58 pins `js-sys = "=0.3.85"`, while `eframe` 0.36
needs `js-sys ^0.3.103`. Cargo resolves every target's dependencies into one lockfile, so
the two cannot share it as published.

This copy is wasm-bindgen-futures 0.4.58 from crates.io with its exact pins turned into
carets, and nothing else changed; `Cargo.toml`'s `[patch.crates-io]` points at it.

**Remove it** when a `libp2p-swarm` release relaxes that pin: delete this directory and the
`wasm-bindgen-futures` line under `[patch.crates-io]`, and `cargo update`.
