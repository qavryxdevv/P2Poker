# Where to pick up

Updated 2026-08-30.

    cargo clippy --all-targets --release        0 warnings
    cargo test --release -- --test-threads=19   591 unit + 49 harness, 0 failed
    tools/check-portable.ps1                    8/8, 27.8 MB

    RUST_LOG=libp2p_kad=debug,libp2p_relay=debug ./target/release/p2p-poker --headless

There is a `tracing` subscriber now, off unless `RUST_LOG` is set. Until today
there was none, so libp2p's account of itself went nowhere and every network
question was answered by adding a temporary `eprintln!` and rebuilding. That
cost more hours than any bug in this file.

## A client behind a NAT now has a public address

    /ip4/149.102.131.48/tcp/4001/p2p/12D3KooWM8dGCW1r.../p2p-circuit/p2p/12D3KooWK85eVcrB...

Measured. The client reaches the public libp2p network, reads the `/libp2p/relay`
namespace out of the DHT — **that is the list of relays, and it is not a file** —
finds around twenty, and takes a reservation. Their limits are 128 KiB and two
minutes, which is D-001's recorded figure, and the client says so itself: *NOT
enough to carry a hand*. Enough to carry the introduction and the lobby, which
is what D-004's layers 2 and 3 are for.

**Correction to D-001's addendum**, measured: the four `bootstrap.libp2p.io`
nodes advertise the hop protocol and answer `RESERVATION_REFUSED`. They are
operated infrastructure. Ordinary public nodes, found through the DHT, do grant
reservations.

**A hand costs 18 KB heads-up** (`MENTAL_POKER.md` §5.1, `n × (5547 + 3432)`),
54 KB six-handed, against a 128 KiB circuit budget — about seven hands per
circuit. Carrying a game over a public relay is arithmetically fine; what it
needs is two or three reservations held at once and a session that survives a
circuit being reset under it.

### Mainline is gone

Discovery is the public libp2p Kademlia DHT and nothing else. `--no-mdns` turns
multicast off, which is how the DHT path gets tested at all — with it on, two
clients on one wire find each other in a second whatever the DHT does.

**It worked twice, end to end**, before the removal was finished: two clients,
multicast off, each holding a reservation on a *different* public relay, agreed
a table — `TABLE FORMED session=f503d44809bfb094 seats=2` on both.

**And it has not worked since, and that is where tomorrow starts.** Four runs of
eight to ten minutes each, after the removal was tidied up:

* both clients announce themselves — `start_providing` succeeds and the log says
  *listed in the public lobby*;
* both read the lobby a hundred and thirty times and get real answers: five or
  six other players, which are this machine's own earlier test profiles still
  advertised in the DHT;
* and **never each other**. One run connected to 325 peers and not once to its
  counterpart.

So the mechanism is not dead — records land and queries return them. Two
particular records are not reaching the queries that want them. What has not
been established, in order of suspicion:

1. Whether go-libp2p stores an `ADD_PROVIDER` whose only addresses are circuit
   addresses. It certainly drops one with **no** addresses — `handlers.go`, `if
   len(pi.Addrs) < 1 { continue }` — and whether a relay address survives its
   filtering was not checked.
2. Whether two `get_providers` walks for one key converge on the same nodes from
   a routing table this thin. Kademlia's client mode was the first suspect and
   has been changed to automatic (`set_mode(None)`), which did not fix it.
3. Whether the record needs longer than a ten-minute run to settle.

The cheapest experiment for (1) is a third party: announce from this client and
look for the record with a tool that is not this client — `ipfs dht findprovs`
against the same key would settle it in one command.

## Two processes now form a table

```bash
cargo build --release
```

Terminal one:

```bash
./target/release/p2p-poker --headless --profile ./A --host Riverside --seats 2 --for 45
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
then correctly refuses a second seat to. `--seats 2` matters too: without it the
command line founds a **rated Sit-and-Go**, and that needs all ten seats before
it deals.

The window does the same thing through **Create table** and **Join table**, and
the table opens in a window of its own beside the lobby with the other players
arriving in it as they sit down.

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
| GUI | lobby and table in two windows, settings, live roster; no hand engine behind the table yet |
| Renderer | OpenGL, falling back by itself to Direct3D 12 on WARP where there is no graphics driver |

## It starts on a machine with no graphics driver

A virtual machine without acceleration has the OpenGL 1.1 that Windows ships,
and the window needs 2.0. The client no longer stops there: it starts itself
again on Direct3D 12, which with no driver present resolves to WARP — `Microsoft
Basic Render Driver`, part of Windows rather than of any driver — and says which
adapter it landed on.

A **second process**, because a process gets one event loop and no more: `winit`
swaps a global flag the first time one is built and never clears it, so the
renderer cannot be retried in place. The decision is taken in `main`, after
`windowed` has returned and its tokio runtime is gone, so the second process
cannot start a second node under the same identity while the first still holds
the ports.

Measured, not assumed: `Microsoft Basic Render Driver` is enumerated on this
machine alongside its two real GPUs, driver version `10.0.19041` — the Windows
build number, not a driver's — and the client draws a window on it when told
`--renderer software`. What is **not** yet measured is the automatic hop, which
needs a machine where OpenGL is genuinely absent. The two decisions it turns on
are unit-tested; the hop itself is not.

Cost: 21.3 MB → 26.4 MB, and 23 crates, none of which brings a new licence into
the tree (`DEPENDENCIES.md` §6).

### And then it burned a core, which was our fault, not the renderer's

A client sitting at a table with no hand running took ~100% of a processor in
the VM. The renderer was the trigger, not the cause. Measured, idle, full-size
window:

| | before | after |
|---|---|---|
| OpenGL, on a card | 1.4% of one core | **0.0%** |
| Direct3D 12 on WARP | 660% — 6.6 cores | **~41%** |

Three separate faults, each found by measurement and each invisible on a GPU:

1. **A repaint on a timer.** `request_repaint_after(250 ms)` ran at the end of
   every frame whether or not anything had changed. A frame in software takes
   longer than 250 ms, so the next was always already due: it never stopped.
   Now the window is woken by the node, through a relay task that holds the
   egui context — and the wake names `ViewportId::ROOT`, because a bare
   `request_repaint` wakes whichever viewport is on top of a stack the task is
   not on, which while the table window is open is the table.
2. **Chatter treated as news.** `NodeEvent::changes_more_than_the_log` splits
   what a player is watching from what is only a line in the log. A seat
   filling repaints at once; a dial that failed waits up to three seconds on
   the software renderer, 200 ms on a card. The match is exhaustive with no
   wildcard, so a new event will not compile until somebody chooses a side.
   mDNS also stopped announcing the same neighbour every few seconds — it is
   announced once and forgotten again on `Expired`, which is now handled.
3. **A scrollbar that could not make up its mind.** `VisibleWhenNeeded` fades
   the bar; the fade changes the content width; rewrapped content is a
   different height; a different height wants a different answer. The animation
   never settled, and an animation in flight asks egui for another frame for
   ever. That alone was half the idle cost — 78% against 39%.

What is **not** fixed: one full redraw of the window still costs about half a
second of processor time under WARP against four milliseconds on a card, and a
controlled experiment says that is the price of the window itself rather than of
anything in particular that we draw — a bare `CentralPanel` with no fill, no
stroke and no rounding costs the same as the whole lobby. Moving the mouse over
the client in a VM will be slow. Cutting that means drawing less, and the first
candidates are `paint.rs`'s `RINGS = 16` and `BANDS = 14`.

## Next actions, in order

1. **Make two clients find each other in the lobby, repeatably.** See above for
   what is known and the three things that are not. Everything below waits on
   this: a lobby that lists strangers and not your friend is not a lobby.
2. **The hand, wired.** Formation ends at `session_id` and the engine starts at
   `GENESIS(1)`. `table::dealing` already runs a hand between three peers in
   memory; nothing carries `HAND_INIT` and the deck messages between two.
3. **Two machines on two networks.** Still rests on nothing.
4. **The automatic renderer hop, in a VM.** Everything around it is tested; the
   hop wants a machine with no OpenGL to prove itself on.
5. **The table window's engine.** It draws a sample hand and says so. §22's rule
   — never display an unverified card as valid — is enforced by the type
   (`Facing::up` takes the verdict), so the wiring cannot break it by omission.
6. Phase 11's audit.

### Done since, and worth not re-deriving

* **A fixed port**: `--port N` binds both transports to it, so a player who can
  forward one on their router becomes reachable — and a reachable player is a
  relay for everyone else under D-002. Verified: `--port 47777` binds
  `/ip4/…/tcp/47777` and `/ip4/…/udp/47777/quic-v1`.
* **The volunteer relay path is fixed by deletion.** It built its circuit
  address without `/p2p/<PeerId>`, which the transport refuses before a packet
  leaves while reporting an empty string, so it had never worked. Reservations
  now come from the `identify` path, which is where a public relay and a
  volunteer arrive by the same road.
* **The relay-budget headline is gone.** *"Relay found, but it cannot carry a
  hand — see the note"* became the permanent first line the moment this client
  learned to find a relay, named a table's problem to somebody reading a lobby,
  and pointed at a note that did not exist. `net::relay` still decides it, at
  the point it belongs.

### Also found and not yet fixed

`quic_port` matches a circuit address, so once reservations are routine this
client could announce a relay's port as its own and ask the router to open it.
UPnP's four distinct outcomes all fall into `_ => {}` and are indistinguishable
from never having tried; on success the crate calls `add_external_address`,
which makes it a second, undocumented arbiter of reachability beside AutoNAT.

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
