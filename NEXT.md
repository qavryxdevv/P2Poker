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

### Mainline is gone, and the lobby works across networks

Discovery is the public libp2p Kademlia DHT and nothing else.

**Proven by the owner's own two machines, which is the configuration that
matters:** two PCs on **different VLANs**, with a firewall blocking inbound
connections between them, and each client's lobby lists the other's tables.
Multicast does not cross a VLAN, so mDNS cannot be what found them — it was the
DHT, and the connection is carried by a relay. That is D-003's acceptance
criterion met by the awkward case rather than the easy one.

**Two clients on one machine is a bad proxy for this, and it misled me.** Four
runs of eight to ten minutes, both with `--no-mdns`, both announcing themselves
successfully, both reading the lobby a hundred and thirty times and getting real
answers of five or six other players — and never each other. On the evidence
above that is an artefact of running both behind one NAT with one external
address, not a broken lobby. It is still worth understanding, because a test
that cannot be run on one machine is a test that will not be run.

`--no-mdns` exists for exactly that isolation, and the client now says which
road a peer arrived by — *found … on this network* for multicast, *found … in
the public lobby* for the DHT — so an ordinary run answers the question that
used to need a special one.

## Next actions, in order

1. **The hand, wired.** Formation ends at `session_id` and the engine starts at
   `GENESIS(1)`. `table::dealing` already runs a hand between three peers in
   memory; nothing carries `HAND_INIT` and the deck messages between two. This
   is now the thing standing between a lobby that works and a game.
2. **Why two clients on one machine do not find each other**, when two on
   different VLANs do. Not blocking, but a test that only passes on two
   computers is a test nobody runs. Formation ends at `session_id` and the engine starts at
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
