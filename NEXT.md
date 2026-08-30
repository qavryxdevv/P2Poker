# Where to pick up

Updated 2026-08-30.

    cargo clippy --all-targets --release        0 warnings
    cargo test --release -- --test-threads=19   641 unit + 54 harness, 0 failed
    tools/check-portable.ps1                    8/8, 28 MB

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

### Betting runs, and the board opens

Two clients heads-up play the pre-flop round out and the flop appears. The five
`ACTION_*` messages are single-writer stages, one per turn; `BOARD_REVEAL` is
collective and **every dealt-in seat owes one, folded seats included** — the
price of `n`-of-`n`.

`Hand` has a second entry point now, and it is the first thing in the whole
driver that waits for a human: `act(action, key, now)`. It applies the player's
action through the **same** `BettingRound::apply` a receiver runs, so a client
cannot send itself something a peer would refuse. `turn()` hands the window
seat, street, `to_call`, legal actions and pot in one call.

Two quantities live in `Play` because the engine does not hold them:

* `paid`, the per-hand committed totals. `BettingRound::committed` is per
  **round** and `build_pots` wants the hand.
* `aggressor`, cleared when a round *opens* and never when a street is
  *skipped* — which is what makes it the right seat to show first after an
  all-in run-out where the last streets had no betting (D-021).

### A whole hand plays out

Two clients go from an empty table to a settled hand: four streets, five board
cards, a showdown in TDA order, and the chips moving. Twelve runs over random
decks assert chips conserved, the pot to the best hand shown, and a tie split
back.

The showdown is **one collective stage**, which is `PROTOCOL.md` §4.6's shape:
`SHOWDOWN_REVEAL` and `SHOWDOWN_MUCK` are the only `event_class = 0` pair that
shares a `sequence`. TDA order is an **emission discipline** — a client waits
until every seat ahead of it has spoken, then speaks — so no extra stages, no
extra deadlines, and a seat that speaks early has committed a live-poker
irregularity and nothing more.

A beaten hand mucks without asking. Three things force a show and all three are
rules: being first, anybody being all in (TDA 16), and not being beaten — a tie
shows, because a split pot is won by showing. A muck is the **absence** of a
share, so no peer can open that hand and the forfeiture needs no enforcement.

### Measured: a hand crosses the real network

Two headless processes on one machine, `--host` and `--join`, a two-seat
Sit-and-Go. Both logs, from one run:

```
A: hand #1: the deck is being prepared     B: hand #1 is waiting for seat 0
   hand #1: seat 1 is shuffling               hand #1 has begun
   hand #1: the deck is shuffled and sealed   hand #1: seat 0 is shuffling
   hand #1: your cards are dealt              hand #1: the deck is shuffled and sealed
                                              hand #1: your turn — 50 to call
                                              hand #1: your cards are dealt
```

So `HAND_INIT`, `DECK_INIT`, a two-link shuffle chain — a 3432 B step and a
5547 B proof each way — `DECK_COMMIT` and `DEAL_PRIVATE` all crossed GossipSub
between two processes, and the betting reached a human decision. The transport
carries a hand. That had never been shown before and was the largest open
question in this file.

`50 to call` is also the heads-up rule working: the button is the small blind
and acts first pre-flop.

It stopped there because a headless client has nobody to press a button — which
is what the action clock answers. With the clock in, the same two processes
play **hand after hand**:

```
hand #1: your turn — 50 to call
your clock ran out — Fold for you
hand #1 is over
hand #2 is waiting for seat 1
hand #2 has begun
```

The button alternates every hand, which is the heads-up dead-button rule, and a
hundred-and-thirty-second run gets through several hands end to end: crypto,
betting, settlement, rotation, next deal.

### Three seats found a bug two seats could not

Worth stating because it is the argument for running three in the first place.
A three-seat run formed a table, seated all three, agreed one session — and
hand one stalled at stage zero. Nothing was refused and nothing errored. One
seat simply never emitted its `HAND_INIT`.

The `ever_dealt` guard — added the same day to stop the formation path dealing
hand one again after the table had already played — set its flag **before**
asking whether the opening existed:

```rust
if !ever_dealt {
    ever_dealt = true;
    begin_hand(match opening_for_hand_one(f) { Some(o) => o, None => continue }, …)
```

A peer whose roster had not ratified at that instant took the `continue` with
the flag already set, and the guard — whose whole job is *only once* — was then
closed against it for the life of the table. Intermittent by construction: it
turns on which table message arrives first, which is why one three-seat run
played two hands and a longer one played none.

Heads-up hid it because both peers ratify at almost the same moment. That is
the second time heads-up has hidden a class of defect: it hid the duplicate
handling for two milestones before this.

### How to read a two-process run, and how not to

Two mistakes were made repeatedly today and both produced **confident wrong
conclusions**, so they are written down rather than left to be repeated.

**Do not count anything from a log while the process is still running.** A
three-seat run was reported as "no hands completed" from a mid-run read; by the
end every peer had played two and was into a third. A two-process run was
reported as three hands; it finished five. Wait for the process to exit, then
count.

**Do not conclude from a filtered tail.** The same three-seat run was reported
as "the table never formed", from a `grep | tail` whose window happened to hold
only the host's early lines. The table had formed, agreed one session, and run
a three-link shuffle chain. Read the whole non-lobby log before saying what
happened:

```
grep -vE "in the public lobby|^found |^listening on|^connection budget|^relay |player\(s\)" run.log
```

Both mistakes have the same shape — a cheap partial read treated as evidence —
and the cost each time was a claim that had to be withdrawn.

### The three runs that lied, and why

Runs two, three and four all looked like the same failure — a hand dealt,
somebody to act, then `hand #N is over` with no action and no clock in between.
The table was playing perfectly well. **The log was broken.**

`AppState::note` pops the front when the log is at its five-hundred-line cap
and pushes the new line, so `log.len()` does not change — and the headless
client worked out what was new by comparing `log.len()` before and after
applying an event. Once five hundred lines had gone by, which is a few minutes
of an ordinary lobby, every line routed through `note` stopped being printed.
That is every `Warning`, including *"your clock ran out"*.

Two runs of seven hands each were already in those logs and unreadable.

The lesson is worth keeping: **a diagnostic that is derived from a bounded
buffer's length is not a diagnostic.** `AppState::emitted` counts lines ever
written and the printer uses that. Three of the hand handlers were also pushing
to the log directly rather than through `note`, so those lines bypassed the cap
and the log grew without bound; both halves of the defect were in the same
place.

### The buttons, the next hand and the clock

The action bar reaches the engine: `TableAction` → `NodeCommand::Act` →
`Hand::act`, through the same `BettingRound::apply` every receiver runs.

A table plays more than one hand. `next_hand()` derives hand `k+1` from the
chain alone — settled stacks, `TERMINAL(k)`, `P(k)` — and two peers agree on
`GENESIS(k+1)` down to the byte. The button rotates by the dead-button rule
through `engine::advance_positions`. D-020's hold sits between the hands: five
seconds when somebody showed, a beat when everybody folded.

And there are two clocks. A seat that goes quiet in a **betting** stage is
answered by its own client checking or folding for it — version 1 has no
`TIMEOUT_VOTE` and no `TIMEOUT_CERT` (D-015), so that is the whole of the
answer. A seat that goes quiet in a **cryptographic** stage is answered by
`HAND_ABORT` on the hand's own deadline.

`HAND_ABORT`'s shape is the interesting part and it is not a convenience. It is
a **witness-independent terminal**: no required emitter set, any seat may emit,
and the stage closes at a receiver on the first copy that verifies. An abort is
by definition the outcome in which the peers could not agree about the middle of
the hand, so a terminal needing their agreement could not be reached.
`TERMINAL(k) = ABORT_TERMINAL(k)` is a function of `GENESIS(k)` and nothing
else, which is why two peers that gave up a second apart still derive one
`GENESIS(k+1)`.

*"Buffer, do not reject"* is the acceptance gate, and it maps onto
`Failed::NotYet` — which the caller already holds and replays. Without it one
peer could end everybody's hand by claiming a deadline that had not passed.

D-010 is enforced at the **receiver**: an abort whose deltas are not all zero,
or whose `final_stacks` are not this receiver's own start-of-hand stacks, is
refused whoever signed it.

**The abort path is unit-tested and has never run over the network, and it
cannot be seen in a short run by design.** `sng_hand_deadline_ms` gives a
three-seat table tens of minutes — the rated ten-handed value is 3 300 000 ms —
because `STATE_MACHINE.md` budgets a whole legal hand of human action time plus
one reopening. That is the accepted cost of D-015 deleting the certificate
path: *"a stalled hand takes tens of minutes to end rather than seconds"*.
Exercising it against two real processes needs a run of that length, or a table
advertising a shorter deadline than any preset offers.

### What is next, in order

1. **Three seats and more over the real network.** Everything measured so far
   is heads-up, and heads-up hides a whole class of defect — it hid the
   duplicate handling for two milestones. This is now the cheapest way to find
   the next real bug, because the machinery to run it already exists.
2. `STATE_HASH` / `STATE_ACK` checkpoints. Checkpoint 8's hash is computed and
   carried inside `HAND_COMPLETE`; the checkpoint **stage** is not there, and
   `PROTOCOL.md` §12 says T61 then fires at `hand_deadline_ms` after every
   settled hand.
3. The RNG beacon, replacing `provisional_button`.
4. `HAND_ABORT` causes 2 and 3 — a failed shuffle or reveal proof. They embed
   up to two 32 768 B `SignedEvent`s, so they need a larger `FRAME_CAP` than
   this client opens. It emits neither and refuses one it is sent.
5. Two machines on two networks. Still rests on nothing.

### Four defects found by pointing a critic at the design, not at the code

Worth recording because none of them would have been found by testing what was
written — they are all about what was *not*.

1. **`mental_poker::reveal` implemented the point-to-point model §4.6
   withdrew.** Every peer discarded the `m-1` shares it received for another
   seat's hole card, so `SHOWDOWN_REVEAL` — one message, because the rest is
   already on the transcript — would have had nothing to combine with. The
   showdown could not have been built on top of it.
2. **The hand driver kept only its own two token sets**, for the same reason,
   and had reimplemented a slice of `table::dealing` rather than using it.
3. **Shares were recorded before `stage.hear` ran**, so an ordinary mesh
   redelivery came back as a fault against an honest peer. Moving `hear` first
   only moved the defect. The rule now, in all four collective stages: exact
   repeat → done; then verify; then hear.
4. **`blind_positions` keyed heads-up on `occupied.len() == 2`.** A busted seat
   is still occupied. `engine::initial_positions` — the tested TDA 32
   implementation, keyed on chips — was called by nothing.

The test that caught (3) is the first three-handed hand in the tree, with every
message delivered twice. Heads-up hid all of it: at two seats every collective
stage completes on the first message, so the duplicate lands at a sequence the
hand has already left.

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
