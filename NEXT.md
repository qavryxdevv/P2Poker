# Where to pick up

Updated 2026-08-29, late. Tree clean, everything committed.

    cargo clippy --all-targets                  0 warnings
    cargo test --release -- --test-threads=19   460 unit + 43 harness, 0 failed
    tools/check-portable.ps1                    6/6

## It runs, and it is portable

```bash
cargo build --release
```

```bash
./target/release/p2p-poker --headless --host Riverside
```

```bash
./target/release/p2p-poker
```

The window opens on the lobby. `--headless` is the same client without one, and
both fold the node's events through the same state so they cannot disagree.

One 20 MB file, copied into an empty folder, runs and creates exactly
`profile/identity.key` beside itself. Two folders are two clients; the identity
survives a restart. `tools/check-portable.ps1` checks all of that by
measurement, including that the binary imports no C runtime — it did, until
today, which would have made "copy it and it runs" true here and false on a
fresh machine.

## What has been demonstrated, and what has not

**Demonstrated.** Two processes, no server: mDNS discovery, a QUIC handshake, a
GossipSub mesh, a 337-byte signed table advert crossing it, and the receiver
running §7.2's admission rules on bytes that actually travelled. Both nodes also
announce under `LOBBY_INFOHASH` and find each other's addresses through the
public DHT. `tests/two_nodes.rs` runs the transport half automatically on every
`cargo test`.

**Not demonstrated, and this is the first item below.** Two machines on two
networks. The relay and DCUtR are configured, `net::relay` decides adequacy from
measured per-hand bytes, and **no circuit has ever carried anything.** Two
instances on one machine are the *hardest* case for NAT rather than the easiest:
both learn the same external address, and dialling it inwards is hairpinning.

`tools/two-network-test.ps1` runs one node here and one in a Hyper-V VM on a
different subnet. It must be run elevated — the Hyper-V cmdlets return an empty
list under a UAC-filtered token, which reads exactly like "there are no VMs" —
and its header says what it proves and what it does not. The second list is
longer: the VM can reach this host directly, so no hole needs punching, and a
real test of that needs **one endpoint outside this house**.

## What is built

| Layer | State |
|---|---|
| Poker engine | complete — blinds, order, dead button, pots, TDA reopening, ~120 000 random hands |
| Protocol | envelopes, canonical CBOR, signatures, transcript, anti-replay, checkpoints, staleness |
| Mental poker | complete — the forked `ziffle`, the chain, reveal gating, `n`-of-`n` |
| The seam | `table::dealing` — three peers, one hand from deck to pot |
| Discovery | Mainline, mDNS, QUIC + TCP, GossipSub, relay adequacy |
| Lobby | signed adverts across the wire, §7.2 rules 2–7, rate limits, eviction |
| Formation | roster, join messages and every admission rule — **not wired to the transport** |
| GUI | lobby pane and network status; **the table window is not started** |

## Next actions, in order

1. **Two machines on two networks.** The one claim above that rests on nothing.
2. **Wire formation to the transport.** Every rule is written and tested;
   nothing carries the messages, and the Join button says so rather than
   pretending.
3. **The table window** — `SPEC_CS.md` §22's second half, and the place where
   *never display an unverified card as valid* has to be obeyed rather than
   avoided.
4. Phase 11's audit.

## Open items

`docs/DECISIONS.md` carries D-001 to D-018.

**D-018** lists twenty-three normative terms used and defined nowhere. Four are
closed; **nineteen remain**, seven of them High. Three of those block work that
is otherwise ready: what *"payload bytes, verbatim"* means in
`table_params_hash`, what *"input deck bytes"* are, and where the canonical
52-card ordering lives — which is circular, with two documents deferring to each
other.

**D-017** is withdrawn. **F-1** is open: D-015 made `Q-01`'s answer load-bearing.
**The project licence** has never been chosen and gates `deny.toml`.

## Three habits that paid, and one that cost

**Every guard is verified to bite.** Disable the check, run the test that claims
to cover it, require a failure, restore. Today that caught `is_identity_c1`
testing an unmeasured flag bit, `worth_parsing` written and never called, a
`verify_strict` comment that was folklore, and a token set sized `n - 1`.

**A measurement beats a recollection.** The relay defaults, the deck sizes, the
proof timings, the identity encoding, the two signature checks, the import
table, the folder size — measured, and several disagreed with the surrounding
text.

**Read every normative source independently, then reconcile.** One parallel
reading of the formation path found a live bug — `table_params_hash` hashed
`table_name`, which §3.1 excludes by name, so a founder fixing a typo made the
table permanently unjoinable — plus twenty-three undefined terms. Do it for the
hand path and the dispute path before building either.

**And the one that cost.** `git add -A` while a review workflow was running
committed two of its verify agents' mutations: `ValidationMode::Permissive` in
place of `Strict`, and two domains mapped to one context string. Both sat under
doc comments asserting the opposite; the domain guard would have caught its half
and my process defeated it, and the gossip settings had no guard at all until
now. **Never `git add -A` while anything else can write to the tree, and read
`git status` for foreign modifications before every commit.**
