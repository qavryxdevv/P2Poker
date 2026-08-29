# Where to pick up

Updated 2026-08-29, evening. Tree is clean, everything committed.

    cargo clippy --all-targets                  0 warnings
    cargo test --release -- --test-threads=19   363 unit + 42 harness, 0 failed

## D-003 is demonstrated, not argued

Two processes on one machine, no server, no shared state:

```bash
./target/release/p2p-poker --host Riverside
```

```bash
./target/release/p2p-poker
```

The watcher prints `table bfbf8a1e…`. It found the host over mDNS, completed a
libp2p handshake, received a 337-byte advert over GossipSub, checked
`verify_strict` under the table key, put it through §7.2's admission rules and
filed it under the key that signed it.

**Two instances on one machine is the hardest case for NAT, not the easiest.**
The DHT dials in that run fail — both nodes get each other's *external* address
and dialling it inwards is hairpinning, which many routers do not do. The DHT
half is proven separately: both nodes announce under `LOBBY_INFOHASH` and each
discovers the other's address. What has **not** been run is two machines on two
different networks, which is where relay and DCUtR earn their place.

## What is built

| Layer | State |
|---|---|
| Poker engine | complete — blinds, order, dead button, pots, TDA reopening, ~120 000 random hands |
| Protocol | envelopes, canonical CBOR, signatures, transcript, anti-replay, checkpoints, staleness |
| Mental poker | complete — the forked `ziffle`, the chain, reveal gating, `n`-of-`n` |
| The seam | `table::dealing` — deck to engine, three peers, one hand to showdown |
| Discovery | Mainline announce/lookup, mDNS, libp2p QUIC + TCP, GossipSub |
| Lobby | signed adverts across the wire, §7.2 rules 2–7, rate limits |
| GUI | **not started** |

## Next actions, in order

1. **Two machines on two networks.** The one claim in the table above that rests
   on a single-host run. Relay and DCUtR are configured and have never carried a
   connection.

2. **The join handshake.** A table is visible; nothing sits down at one.
   `JOIN_REQUEST` / `JOIN_ACCEPT`, the seat allocation, and `DECK_INIT`'s key
   exchange — after which `table::dealing` already works.

3. **The GUI**, which is `SPEC_CS.md` §22 and the thing that makes it a client
   rather than a demonstration.

4. Phase 11's audit.

## Open items

`docs/DECISIONS.md` carries D-001 to D-017.

**D-017 is withdrawn** — the owner raised two ways to survive a mid-hand
disconnection and withdrew the request the same day. Nothing from it is built;
the fallback stands, which is that heads-up needs no mechanism at all and
multi-way the hand aborts with every stack restored. The analysis is kept because
it records why sharing *key* shares with outside nodes is unsafe.

**F-1** is still open: D-015 removed the timeout machinery, which made `Q-01`'s
answer load-bearing. If the answer is `TDA_MUCK`, showdown gains a human decision
with no automatic rule behind it.

**The project licence** has never been chosen. It is the owner's, and it gates
`deny.toml` and `ZR-4(a)`.

## Two habits that have paid, kept here so they survive a context break

**Every guard is verified to bite.** A check is disabled, the test that claims to
cover it is run, and it must fail — then the check is restored. This has caught
four things today that would otherwise have shipped as covered: `is_identity_c1`
tested a flag bit nobody had compared against arkworks, `worth_parsing` was
written and never called, a `verify_strict` comment was folklore that no test
distinguished, and a token set was sized `n - 1`.

**A measurement beats a recollection.** The relay defaults, the deck sizes, the
proof timings, the identity encoding and the two signature checks were all
measured rather than quoted, and three of the five turned out to differ from what
the surrounding text said.
