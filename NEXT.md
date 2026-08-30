# Where to pick up

Updated 2026-08-30.

    cargo clippy --all-targets --release        0 warnings
    cargo test --release -- --test-threads=19   626 unit + 54 harness, 0 failed
    tools/check-portable.ps1                    8/8, 27.5 MB

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

## The lobby: three fixed, one not

**Stale tables.** The interface keeps its own copy of the lobby and swept it
only when a *new* advertisement arrived. When the last founder went away nothing
arrived, so a row for a table that no longer existed stayed until the client was
restarted — which is the state a player is in the moment somebody closes their
client. The node now says `Swept` on its own clock and the interface ages its
copy on that. There is a test, and it fails if the sweep is taken out again.

**New tables appearing late.** The founder re-published for a newcomer only
"unless my own table is already in my own lobby", which is true from the first
successful publish onwards — so the *second* player to arrive, and everyone
after, was told nothing and waited up to half a minute. Now every arrival is
answered, bounded to one publish every three seconds.

**mDNS was broken by my own dedupe, and this one is worth remembering.** The fix
for the log flood — announce a local peer once rather than every few seconds —
also dialled only the *first* address that peer was ever seen at. On a machine
with a Hyper-V or WSL adapter that is a coin toss, and when it came up wrong the
peer was never reached and never retried. Two clients on one machine stopped
connecting at all, silently, and it looked like a DHT problem for hours. Every
address is dialled again; only the announcement is once.

**The routing table is kept between runs.** `profile/peers.txt`, written every
ten minutes and read at start. Measured: 172 peers, 76 KB. The compiled entry
point is still dialled every start — the book is *beside* it, never instead of
it, because a saved book goes stale and a first run has none.

### Chat and presence: written, tested, and not delivering

`net::lobbytalk` fills `LobbyPlayerPresence` and `LobbyChat`, the two event
types the protocol reserved and nothing ever used. Signed by the player key,
capped, rate limited, eight unit tests including a tampered message and a name
of forty emoji. The interface has a chat box; the panes were already there.

**It does not work on the wire yet.** Two clients on one machine, connected to
each other and both subscribed to the chat topic, and every publish comes back
`NoPeersSubscribedToTopic`. Neither sees a `Subscribed` from the other for that
topic, while the *lobby* topic works — tables flow between them. Same
subscription code, same start-up, one topic works and the other does not. That
is where to start: log every `gossipsub::Event::Subscribed` with its topic hash
and find out whether the announcement is not sent, not received, or not matched.

Nothing is harmed by shipping it in this state: a failed publish is discarded
and the panes read *quiet* and *nobody yet*, exactly as before.

## The table's traffic is moving to Tox (D-019)

The owner's decision, after the case against it was put and answered: a public
libp2p relay grants 128 KiB and two minutes per circuit, a hand costs 18 KB
heads-up, and a client whose playability depends on somebody having forwarded a
port is a client most people cannot use. So once a table forms, its game traffic
rides a **Tox NGC group** whose `chat_id` is published in the table's lobby
advertisement. Discovery, the lobby, the join RPC and the ratification stay on
libp2p.

**The price is written down in D-019 and it is not reversible:** `c-toxcore` is
GPL-3.0 and not LGPL — verified against its own `LICENSE` — so linking it makes
this whole client GPL-3.0. That closes the open "project licence" item by a
decision that was not about licensing, and `DEPENDENCIES.md` §6 has been
corrected: the tree is no longer permissive throughout.

**Why the narrowed proposal is buildable where the earlier one was not:** the
defect measured in the Python predecessor was group *discovery* decaying — a
host up six minutes was never found in 300 s, because a group's onion key is
`random_bytes()` per start. This design never searches for the group: the chat
id arrives in the lobby advertisement and members arrive by invitation.

### The order, and the seam is done

1. **`table::transport` — done.** `TableTransport`, six tests. `TableSession`,
   the protocol and the state machine speak through it and never name libp2p,
   Tox, GossipSub or a peer id. `FromTable::claimed` is an `Option` and is
   *named* `claimed` because a Tox group can be joined by anybody holding a chat
   id that travels in a public advertisement, so "it arrived over the group" is
   worth nothing as a claim about authorship. The signature decides.
2. **The hand over the existing transport**, so there is something to carry.
3. **Then Tox**: build `libtoxcore`, the FFI, the group.

Written in that order deliberately: the alternative leaves the project with no
working game and a second network stack at the same time.

### What step 3 needs from this machine, and does not have

Visual Studio 2022 is installed. `cmake`, `libsodium` and a `vcpkg` are not, and
`cl` is not on the path outside a developer prompt. There is no maintained Rust
binding to lean on: `rstox` predates NGC, `quininer/tox-rs` is marked deprecated,
and `tox-rs/tox` is a pure-Rust reimplementation that is GPLv3+ as well and whose
own README says the client part is still being worked on. So the FFI is ours to
write.

## A hand begins

The screen said *no hand in progress* and it was telling the truth: `TableReal`
fired and nothing followed. Formation ended at `session_id`, the engine started
at `GENESIS(1)`, and nothing carried a byte between them.

Now it does. Measured between two processes over the real network:

```
A: hand #1 is waiting for seat 1  ->  hand #1 has begun
B: hand #1 is waiting for seat 0  ->  hand #1 has begun
```

Stage 0 of hand 1 — `HAND_INIT`, collective — completes, and both peers derive
the **same** parent for stage 1. Three modules, all synchronous, bytes in and
bytes out, the shape `net::formation` already uses: `table::stage`,
`table::handwire`, `table::hand`. Plus `TERMINAL(0)` and `GENESIS(1)` on the
formation, which nothing had ever computed.

Two things in there are load-bearing rather than decoration:

* **A bystander is kept out of the stage hash.** A seat that may speak but is
  not in `R` is heard and does not enter the hash — if it did, two peers who
  heard different bystanders would fork the chain with nobody lying.
* **One seat with two stories is a finding, not an overwrite.** Last-one-wins
  leaves the two peers who saw them in different orders disagreeing for ever,
  and neither of them knowing why.

And `handwire::disagreement` returns **which field** differs. The symptom of
getting this wrong is a table that does not move; "seat 3 says the button is at
2 and I say 1" is something a person can act on.

### The one thing that is provisional, and it is named

`provisional_button` decides where the button sits from `session_id`.
Deterministic, agreed by everybody, and **not the rule**: `STATE_MACHINE.md` T10
gives it to the RNG beacon of §7.9, which no code here performs. A beacon exists
so no single seat's contribution decides the button, and a hash of the
ratifications is not that guarantee. One function, so the beacon replaces it in
one change.

### The hand now reaches hole cards

Stages 0 through `2m+3` all run. Two clients, two independent states, no
network in the test and no server anywhere in the design, go from an empty
table to each holding two cards the other cannot read
(`two_clients_reach_their_own_hole_cards`).

| Stage | Message | Shape |
|---|---|---|
| 0 | `HAND_INIT` | collective |
| 1 | `DECK_INIT` | collective — key and ownership proof, per-sender `DeckCtx` |
| `2+2j`, `3+2j` | `SHUFFLE_STEP`, `SHUFFLE_PROOF` | single-writer, one pair per shuffler |
| `2m+2` | `DECK_COMMIT` | collective — the barrier |
| `2m+3` | `DEAL_PRIVATE` | collective — shares for everybody else's cards |

Four things in there are load-bearing:

* **A step is held, not applied, until its proof verifies.** The proof carries
  the input and output deck hashes, checked before the 42 ms of verification, so
  an argument lifted from another hand costs a hash.
* **A refused argument abandons the hand rather than shortening the chain.**
  `ZIFFLE_VERDICT.md` C-6 rule 4. A chain that can be shortened is a chain an
  attacker shortens to one honest shuffler.
* **`DECK_COMMIT` is the last cheap moment.** Three values every seat derived
  independently. After it a hole card exists, and a disagreement is a dispute.
* **Privacy is one missing share and nothing else.** `DEAL_PRIVATE` is
  broadcast; each seat withholds only its own share of its own two cards. The
  index set must be *exactly* what a sender owes — the dangerous "more" is a
  board index published early.

Round 0's `input_deck_hash` is a constant standing for the open deck, which the
library owns and never surfaces. It binds nothing and is not meant to: the
context does all of round 0's binding. Written into `PROTOCOL.md` §4.5 so the
next reader does not have to rediscover it.

`vendor/ziffle` gained **FORK(d)**: `AggregatePublicKey` had a private field and
no serialisation, so the one value every seat must agree on before a card is
opened could not be put on a wire. A getter, no arithmetic, recorded in
`PROVENANCE.md`.

### What is next in the hand, in order

1. Betting: one stage per action, with the engine that already exists.
2. `BOARD_REVEAL` for the three post-flop streets — the same reveal machinery
   as `DEAL_PRIVATE`, with every dealt-in seat contributing to every index.
3. Showdown: `SHOWDOWN_REVEAL` / `SHOWDOWN_MUCK`, then `HAND_COMPLETE`.
4. The RNG beacon, replacing `provisional_button`.
5. The stage-timeout path. Every stage seals a `next_deadline_ms` from the
   table's own parameters and nothing yet acts when one passes.

## Next actions, in order

1. **Betting.** The hand reaches hole cards and stops there. `poker::state`
   already holds the engine; what is missing is one stage per action and the
   turn order that decides whose stage it is.
2. **A hand between two real processes, end to end.** The deal is proved
   between two states in one test; the same thing over GossipSub is not. A
   `DEAL_PRIVATE` is ~2 KB heads-up and the deck messages are 9 KB per
   shuffler, so this is also the first time the transport is asked to carry
   something that does not fit in one small frame.
3. **Why two clients on one machine do not find each other**, when two on
   different VLANs do. Not blocking, but a test that only passes on two
   computers is a test nobody runs.
4. **Two machines on two networks.** Still rests on nothing.
5. **The automatic renderer hop, in a VM.** Everything around it is tested; the
   hop wants a machine with no OpenGL to prove itself on.
6. **The rest of the table window.** The hero's hole cards are drawn from the
   hand now, and the other seats' backs with them; the board, the pot and the
   action are still the sample. §22's rule
   — never display an unverified card as valid — is enforced by the type
   (`Facing::up` takes the verdict), so the wiring cannot break it by omission.
7. Phase 11's audit.

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
