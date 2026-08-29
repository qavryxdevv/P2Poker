# Where to pick up

Updated 2026-08-29, evening. Tree clean, everything committed.

    cargo clippy --all-targets --release        0 warnings
    cargo test --release -- --test-threads=19   546 unit + 49 harness, 0 failed
    tools/check-portable.ps1                    6/6

## Two processes now form a table

```bash
cargo build --release
```

Terminal one:

```bash
./target/release/p2p-poker --headless --profile ./A --host Riverside --for 45
```

Terminal two:

```bash
./target/release/p2p-poker --headless --profile ./B --join Riverside --for 40
```

Both print `TABLE FORMED session=…` with **the same session identity**. Run three
times on this machine, three tables, three matching identities. That is the join
RPC on `/p2p-poker/join/1`, `PLAYER_LIST` and `TABLE_READY` on the table's own
GossipSub topic, and every §4.3 admission rule running on bytes that travelled.

`--profile` matters: the profile lives beside the executable so the folder can be
copied, and two copies of one folder are one player — which §4.3's `peer_id` rule
then correctly refuses a second seat to.

The window does the same thing through **Create table** and **Join table**.

## What has been demonstrated, and what has not

**Demonstrated.** Two processes, no server: mDNS, a QUIC handshake, a GossipSub
mesh, a signed advert crossing it, §7.2's admission rules on real bytes, both
nodes announcing under `LOBBY_INFOHASH` and finding each other through the public
DHT — and now a whole table formed, ratified and settled on a `session_id`.

A **relay circuit carries traffic**: `tests/relay_circuit.rs` runs three nodes,
takes a reservation, dials the circuit address and connects over it. Its first
run found that nothing in this client ever recorded a confirmed external address,
so a node volunteering as a relay granted reservations carrying no address at
all.

**Not demonstrated, and this is the first item below.** Two machines on two
networks. Loopback has no NAT and nothing on one machine simulates one, so DCUtR
and hole punching are exercised by a run across two networks and by nothing else.

`tools/two-network-test.ps1` runs one node here and one in a Hyper-V VM on a
different subnet. It must be run elevated — the Hyper-V cmdlets return an empty
list under a UAC-filtered token, which reads exactly like "there are no VMs".

## What is built

| Layer | State |
|---|---|
| Poker engine | complete — blinds, order, dead button, pots, TDA reopening, ~120 000 random hands |
| Protocol | envelopes, canonical CBOR, signatures, transcript, anti-replay, checkpoints, staleness |
| Mental poker | complete — the forked `ziffle`, the chain, reveal gating, `n`-of-`n` |
| The seam | `table::dealing` — three peers, one hand from deck to pot |
| Discovery | Mainline, mDNS, QUIC + TCP, GossipSub, relay adequacy, a circuit that carries |
| Lobby | signed adverts across the wire, §7.2 rules 2–7, rate limits, eviction |
| Formation | **complete and wired** — join RPC, roster, ratification, `session_id`, over a real connection |
| GUI | lobby and table, both drawn; the table window has no engine behind it yet |

## Next actions, in order

1. **The hand, wired.** Formation ends at `session_id` and the engine starts at
   `GENESIS(1)`. `table::dealing` already runs a hand between three peers in
   memory; nothing carries `HAND_INIT` and the deck messages between two.
2. **Two machines on two networks.** The one claim above that rests on nothing.
3. **The table window's engine.** It draws a sample hand and says so. §22's rule
   — never display an unverified card as valid — is enforced by the type
   (`Facing::up` takes the verdict), so the wiring cannot break it by omission.
4. Phase 11's audit.

## Open items

`docs/DECISIONS.md` carries D-001 to D-018.

**D-018** lists twenty-three normative terms used and defined nowhere. Four are
closed; **nineteen remain**, seven of them High.

**D-017** is withdrawn. **F-1** is open: D-015 made `Q-01`'s answer load-bearing.
**The project licence** has never been chosen and gates `deny.toml`.

## Four habits that paid, and one that cost

**Every guard is verified to bite.** Disable the check, run the test that claims
to cover it, require a failure, restore.

**A measurement beats a recollection.** The relay defaults, the deck sizes, the
proof timings, the import table, the folder size — measured, and several
disagreed with the surrounding text.

**Read every normative source independently, then reconcile.** One parallel
reading of the formation path found a live bug and twenty-three undefined terms.

**And the newest, which cost the most to learn: run it for longer than a test
runs, on more processes than a test uses.** Formation passed in memory, passed
over a real connection in one process, and still could not seat two real
clients. Three defects, none of which any unit test could have found:

* a joiner names the copy of the advertisement **it** heard, and re-broadcasts
  every thirty seconds mean that is never the founder's newest one — invisible to
  any test that finishes inside thirty seconds;
* GossipSub delivers to whoever is on a topic **at the moment of publishing**, so
  a roster announced in the same breath as the acceptance was never sent at all;
* the mesh does not order two messages, so a ratification overtakes the roster it
  ratifies — and refusing it left both sides waiting for each other for ever.

**And the one that cost.** `git add -A` while a review workflow was running
committed two of its verify agents' mutations. **Never `git add -A` while
anything else can write to the tree, and read `git status` for foreign
modifications before every commit.**
