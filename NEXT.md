# Where to pick up

Updated 2026-08-30.

    cargo clippy --all-targets --release        0 warnings
    cargo test --release -- --test-threads=19   647 unit + 54 harness, 0 failed
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

### Three seats play, and the bug they found was the transport

Measured, three headless processes, one host and two joiners:

```
x1: hands=3 clocks=3    hand #4: your turn - 100 to call
x2: hands=3 clocks=2    hand #4 opens at genesis e1d3d0e9 with seats [0, 1, 2]
x3: hands=3 clocks=2    hand #4: your turn - 50 to call
```

Three hands settled and a fourth in progress, a three-link shuffle chain each
time, one genesis, and the correct three-handed pre-flop ladder — 100 to call
under the gun, 50 for the small blind. Before the fix below it was **zero
hands**, every time.

**GossipSub has no history.** A peer grafted into the topic mesh after a
publish never sees it, and nothing re-sent. At two seats it rarely bites — both
are meshed before either speaks. At three it is the ordinary case: seats 0 and
1 ratify and publish `HAND_INIT` while the last joiner is still meshing, and
that seat then waits for two messages that will never come again.

It is **silent by construction**, which is why it survived so long. A missing
parent reads as "a peer one stage ahead", which the driver holds rather than
refuses — correctly — and `replay_early` runs only after a *successful* event,
so a peer that received nothing replays nothing. No error, no refusal, three
logs indistinguishable from a healthy one. The asymmetry was the only tell:
everybody had seat 2's message, seat 2 had nobody's.

Every message a client emits in a hand is now kept and re-sent every five
seconds until the hand ends. **All of them**, not the newest: a peer stuck at
stage 0 is not helped by stage 1 — it is already holding stage 1, and only the
stage-0 bytes release it. **The stored bytes, never a re-signature**: re-signing
changes `emitted_at_unix_ms` and makes a second distinct body at one slot, which
is an equivocation proof against an honest peer.

This is where three milestones of duplicate handling paid off. A re-send *is* a
duplicate, and duplicates have been weather rather than faults since the
exact-repeat check went in.

### Measured: a client is killed mid-hand and the table plays on

Three headless processes, seat 2's client killed with `Stop-Process` while a
hand was running. Both survivors' logs:

```
my clock has run out on seat 2
the table acted for seat 2: Fold
hand #1 is over
...
hand #7 opens at genesis c7217231 with seats [0, 1]
hand #7 has begun
```

So the whole path ran over GossipSub: both survivors' own timers expired, both
voted, the votes assembled into a certificate, and **the table folded a seat
whose client no longer exists**. Six hands completed in total, and from the next
hand `GENESIS` is derived with `seats [0, 1]` — the dead seat is outside `P(k)`,
takes no cards, and its stack sits there for the blinds. That is D-013 and D-022
doing exactly what a tournament does with a dead seat.

**And the run showed a defect in the same breath.** The second stall ended with
*"the hand ran out of time; every stack is restored"* rather than a certificate:
the stage deadline and the vote fall due at the same moment and the stall tick
does both, so the local abort pre-empted the certificate on the very tick that
produced the vote. The hand ended anonymously where it could have ended naming
the seat. Fixed by giving the certificate one more stage's worth wherever one is
achievable — and firing at once where it is not, which is heads-up and below the
floor generally, because there nothing better is coming.

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

**D-023 changed the first half of that.** `TIMEOUT_VOTE` and `TIMEOUT_CERT` are
produced again where `|V(subject)| >= 2`: a vote says only *"my own timer
expired and I have accepted nothing from that seat"*, only a **complete** set
becomes a certificate, and only a certificate moves anything — so one honest
third party is enough to protect a victim, and a test asserts that a lone vote
leaves the same player to act on all three peers. The floor is on `|V|` and
never on the seat count, because the attack needs a one-member voter set rather
than a two-seat table.

Heads-up the mechanism is **inert by design** and the hand deadline is the only
terminus. The honest statement is now: the reconnection bank cannot be cheated;
the action clock cannot be cheated at three seats or more; heads-up it still
can, and heads-up there is nobody to appeal to, so it always will be. What
bounds it there is the hand deadline and the fact that an abort moves no chips.

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
3. `STATE_HASH` / `STATE_ACK` checkpoints. Checkpoint 8's hash is computed and
   carried inside `HAND_COMPLETE`; the checkpoint **stage** is not there, and
   `PROTOCOL.md` §12 says T61 then fires at `hand_deadline_ms` after every
   settled hand.
4. The RNG beacon, replacing `provisional_button`.
5. `HAND_ABORT` causes 2 and 3 — a failed shuffle or reveal proof. They embed
   up to two 32 768 B `SignedEvent`s, so they need a larger `FRAME_CAP` than
   this client opens. It emits neither and refuses one it is sent.
6. Two machines on two networks. Still rests on nothing.

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


## The decision clock, made to work over a real mesh

D-023 restored `TIMEOUT_VOTE` and `TIMEOUT_CERT`. Every unit test passed and the
machinery did not work between processes. Four defects, in the order they were
found, and each one hid the next.

**The tally read `1 of 2` for ever.** Both peers voted; each held only its own
vote. Not a digest mismatch, which was the obvious guess and was wrong — the
digests were identical, and a one-line diagnostic that printed the digest
settled that in one run instead of an afternoon of reasoning. The peers were a
stage apart.

**They were a stage apart because a client forwards nothing.**
`validate_messages()` means GossipSub passes a message on only after the
application reports a verdict, and every arm of the hand-event branch returned
to the top of the loop without reporting one. Between three directly-meshed
peers this is invisible: delivery to a direct peer needs no forwarding. It stops
being invisible the moment a peer dies mid-broadcast. Its last event reaches one
survivor, that survivor relays nothing, and the two live peers sit one sequence
apart for the rest of the hand, each naming a different seat as late.

**`certifying` was never cleared when a stage advanced**, and
`certify_if_unanimous` refuses to start while one is in progress — so a stage
that opened a certificate and moved on blocked every certificate for the rest of
the hand.

**And the one that had nothing to do with the machinery.** With all of the
above fixed, both survivors voted, agreed, reached unanimity and each emitted a
certificate — and then each refused the other's `HAND_ABORT`, because
`apply_certificate` built it with a named subject and no `cert_hash` and §4.10
forbids that shape. Everything was right up to the last message.

### What that cost, and the four habits that would have shortened it

* **Grep for the failure, not for the success.** `hand: {e}` was printed on
  every refusal for the whole investigation and was never grepped for, because
  the greps were written to look for the thing that was supposed to happen. The
  answer had been in the log for three runs.
* **Instrument the decision, not the state.** Reading `certify_if_unanimous` and
  reasoning about it produced two wrong hypotheses. Printing what it *decided*
  produced the answer on the first run.
* **A silent path is where bugs live.** Of the twelve places a vote can be
  dropped between the wire and `take_vote`, five say nothing at all. Two of
  those five were firing.
* **A test must be shown to fail.** Both regression tests here were verified by
  backing the fix out and requiring the failure — and the one written for the
  reserve cap failed immediately on a value picked by hand, which is why that
  constant is now derived.

### And a third, which is not about files

**`git add -A` while a review workflow is running commits the workflow's
working files.** It happened twice in one session: a 106-line probe reached
`origin/master` as `tests/zz_replay_poc.rs`, and 178 lines of an agent's
`// ATTACK PROBES (temporary)` block went into `src/table/hand.rs`'s test module
under a commit message about GossipSub subscriptions.

This file already said *"never `git add -A` while anything else can write to
the tree, and read `git status` for foreign modifications before every
commit"*. Saying it again is not the fix. **Name the paths**: `git add
src/net/run.rs src/table/hand.rs` and nothing else, every time, for as long as
anything else can write. `git status --porcelain` before each commit, and read
it — the second time, the evidence was on screen and went past unread because
the test count had gone *up*.

### And two ways to destroy a file, both self-inflicted here

* `io.open(p, 'w').write(io.open(p).read())` truncates before it reads. It
  emptied `hand.rs` completely. Read into a variable first, write through a
  temporary file, `os.replace`.
* `git checkout -- <file>` to undo a *deliberate* temporary edit also discards
  every uncommitted change in that file. It threw away the same file's work a
  second time, ten minutes later. Revert the one line, or commit first.

## The one that mattered most — closed

**Two survivors ran different rosters for five hands under identical genesis
hashes, and neither said a word.** One certified a live seat that had gone quiet
for one stage and dropped it; that seat never applied the certificate about
itself, because it had already moved past the stage the certificate named —
`subject_of` rebuilds from the current slot, and `on_event` dropped the
certificate at `sequence < slot.sequence` before its handler was ever reached.
Even the diagnostic written for exactly this was unreachable.

Both halves are now done and both are recorded as **D-024**:

* The genesis commits to `R(k)`, so two peers that disagree about who is playing
  cannot open the same hand. That turned a silent split into a refusal.
* A certificate proves its own position instead of being checked against the
  receiver's cursor — every carried vote's *signed* envelope must name the stage
  its payload names — so the subject can verify and apply one about itself. Its
  roster effect is banked wherever the receiver stands; its stage effect runs
  only in position.

Found by an adversarial review workflow: three independent designs, each
attacked from two lenses, all three refuted, and the fourth built from what
survived. The same review found three live defects in the tree that no design
was responsible for — `certs` fed from `on_hand_init`, no `HAND_COMPLETE`
precedence, and the roster mutated ahead of a fallible engine call — each of
which is now fixed with a test shown to fail without it.

## Why a table sometimes did not form at all

GossipSub exchanges subscriptions **once**, when a connection is established,
and measured on three clients on one machine that exchange is unreliable. Every
failed formation had the same signature, and it stayed invisible until both
sides of the comparison were named in the log rather than counted:

```
founder : lobby topic: 0 of 0 subscribed []; connected ["yTmZ9s", "MwT6dL"]
joiner 2: lobby topic: 0 of 1 subscribed ["7GxK5Q"]; connected ["7GxK5Q", "yTmZ9s"]
joiner 3: lobby topic: 0 of 1 subscribed ["MwT6dL"]; connected ["7GxK5Q", "MwT6dL"]
```

Both joiners connected and identified, and the founder knew of **no** peer
subscribed to the lobby — so `publish` answered `NoPeersSubscribedToTopic` and
the advert went nowhere. Each joiner meanwhile saw exactly one of its two
neighbours. A table formed whenever the founder saw both and never when it did
not.

There is no per-peer *"send my subscriptions"* call in the library, so the fix
is the primitive that exists: when a peer is first identified as a poker client,
drop and retake the lobby topics, which re-announces them to everyone connected.
Once per peer, because `poker_peers` is a set.

**Measured, three clients, table of three: 10 of 12 runs formed, against roughly
2 of 6 before, the fastest in 9 s.** The subscribed set now matches the
connected set in every run but one, and that one had a peer that never connected
at all.

The lesson is the same one this file keeps recording: *a count could not say
which peer was missing, and the question was entirely about which.*

## Measured and not explained: the roster moves without a certificate

Three clients, one killed, everything of D-024 in place. Both survivors agreed
at every step — same genesis, same seats — so this is not a fork. It is the
derivation being unstable:

```
hand #1 opens at genesis 460d01bc with seats [0, 1, 2]
my clock has run out on seat 1 — seat 1 @8b47b283: 1/2 agree
a peer ended the hand on its own deadline; every stack is restored
hand #2 opens at genesis 510f1640 with seats [1, 2]      <- seat 0 dropped
hand #3 opens at genesis b407894e with seats [0, 1]      <- seat 0 back, 2 gone
hand #4 ... [0, 1]
hand #5 ... [0, 1]
```

**No certificate was ever accepted** (`certs=0` on both), so `certified` is empty
and `took_part` is true for every seat on the `by_certificate` branch. `dealt_in`
is a subset of `required`, and hand 1's `required` is the ratifiers, so it had
three members and the branch should have been taken. On that branch hand 2 could
only be `[0, 1, 2]`. It was not.

And hand 3 **re-adds** seat 0, which the `by_certificate` branch cannot do — it
only ever filters `self.open.required`. So hand 3 used the observation branch,
which means hand 2's `required` had fewer than three members and
`by_certificate` had already flipped off. Once the required set reaches two the
derivation reverts to *"whoever I heard from"*, which is exactly when agreement
matters most, and it can put a seat back.

Two things to establish, with instrumentation and not by reading:

* what dropped seat 0 at hand 2, given an empty `certified` and restored stacks;
* whether `by_certificate = self.open.required.len() >= 3` is the right gate at
  all. It was chosen because D-023's floor makes certification impossible below
  three seats, but a three-seat table that loses one is exactly the case, and
  the fallback it lands in is the unstable one.

## The replays the anti-replay review listed, and what became of them

It named seven that actually work against the running code. Five are closed,
each with a test shown to fail without the fix:

| | Closed by |
|---|---|
| Unsigned junk floods the hold queue, and the victim forwards it in its own name | `Hand::hold` opens the event before keeping it, and the node's verdict follows what was kept |
| `said` is never cleared at a hand boundary, so hand *k* poisons hand *k+1* | cleared where the next hand opens |
| `ratified.insert` overwrites, and `session_id` is built from those hashes | first copy wins, second differing one named |
| An expired advert is re-admitted | refused with the same predicate the sweep already uses |
| A stale `PLAYER_LIST` is accepted while this client's serial is 0 | bounded against the founder's own signed envelope time |
| A stale-ratification recording refills the formation queue | one that can never become valid is not held |

**Two are open, and neither is a queue:**

* **A withheld `HAND_ABORT`, released at a stage of the attacker's choosing.**
  Since D-024 the abort is routed above the sequence guards on purpose, so an
  old one is accepted at a later stage — but `on_hand_abort` still gates cause 1
  on this receiver's **own** expired deadline, so the attacker chooses only the
  moment inside a window the receiver had already entered. Whether that is worth
  closing, and at what cost to the witness-independent terminal, is not settled.
* **Naming a double signer at a single-writer stage**, which is knowingly given
  up: the second body arrives under the stage cursor and is dropped without
  comparison. Accepted because detection has no consumer — no `EquivocationProof`
  object exists, and `Failed::Equivocation` becomes a warning and a `Reject`
  aimed at whoever relayed it.

## The shuffle refusal, closed — and the one behind it

**Closed.** `ShuffleAdmission::admit` bounded a seat index and a position in the
shuffle order with one argument, and `accept_step` filled it with the order's
length. Drop a **low** seat and the survivors keep their numbers: at three seats
with seat 0 certified absent the order is `[1, 2]` and seat 2 was refused at
position 1 for being seat 2 — first attempt, nothing submitted, every hand,
for as long as the table kept dealing. And both `NotAdmitted` answers were
mapped to `AlreadySubmitted`, so it reported a duplicate that never existed,
which is what sent two investigations the wrong way.

It was found by an instrument, not by reading: every diagnostic for this family
was on the path that verifies a **peer's** proof, and the refusal was about the
client's own. The note added to `shuffle_if_mine` printed
`chain step 1, turn Some(2)` on the first run that hit it — the chain waiting
for exactly the seat it was refusing. The regression test plays the hand after
the drop, which nothing did before, and reproduces that line verbatim when one
bound is put back.

### And the one it was hiding

With that gone, a live run shows a different refusal, once:

```
n2: shuffle refusal from seat 0: chain at step 0, slot sequence 3, round 0
n2: hand: seat 0's shuffle: the argument does not hold for this pair of decks
```

A genuine verification failure of a peer's first shuffle proof. Both peers hold
the same genesis for that hand — `90c6a1f0`, seats `[0, 1]` — so it is not a
roster disagreement, and by C-6 rule 4 the chain is then abandoned and the hand
costs one deal. The two agreed again at the next hand, because an abort's
terminal is a function of the genesis alone.

Checked and **not** the cause: prover and verifier derive the context the same
way. `next_ctx(seq)` is `ctx_for(steps_taken(), seq)`, the prover passes
`slot.sequence + 1` at its step and the verifier passes its own slot at the
proof stage, which is the same number. `ctx_for` reads only chain parameters,
the position and `keys[k]`.

### Both are now instrumented, and neither has recurred

`ShuffleChain::ctx_report` prints the three parts of a proof's context that can
vary — position, sequence, and the signer key at that position, with the order
beside them. The verifier prints it when it refuses; **the prover prints it on
success**, because one side's account of a disagreement is not a comparison.

A settlement mismatch now names what differs: both `final_stacks` vectors and
both pot counts. It used to say only *"seat N holds a different settlement"*, so
two engines disagreeing about a pot, about a winner, or about a fold one of them
never saw all read identically in a log.

**Neither has fired since.** Five kill runs after the shuffle-admission fix: no
shuffle refusal at all, and the settlement mismatch did not recur. Both were
last seen at the moment the killed client leaves, which is the most turbulent
point of a run, so the next occurrence is the thing to wait for rather than to
provoke — and when it comes it will say what it disagreed about.

Two things checked and ruled out along the way, so they are not re-derived:
`on_shuffle_proof` compares the input deck hash **before** it verifies and that
check passes, so prover and verifier hold the same deck; and this client's own
timeout action follows the same rule the certificate does — check when nothing
is owed, fold when facing a bet — so a self-fold and a certified check cannot
be two different hands.

The step that remains is to make a failing argument say — the two `DeckCtx` field sets side by side, and the
input deck hash each side used — because the note as written says where the
chain was and not what the proof was checked against.

## Still open

Checked against the tree on the day this was written, and three entries that
stood here are gone because they are done: the anti-replay modules are deleted
(D-025), §4.10's second half is implemented, and the shuffle refusal that was
*"seen once and not explained"* is explained and fixed — it was
`ShuffleAdmission::admit` bounding a seat index and a chain position with one
argument.

### Protocol features that do not exist yet

* **`STATE_HASH` / `STATE_ACK`.** The hash is computed inside `HAND_COMPLETE`
  and the stage that carries it does not exist, so §12's T61 fires after every
  settled hand and has nothing to fire on. `src/protocol/checkpoint.rs` and
  `seats.rs` are the draft of §6.2's records and are deliberately kept unwired
  for this; `tests/anti_replay_authority.rs` holds them to that.
* **The RNG beacon.** `provisional_button` stands in for it, in nine places in
  `table/hand.rs`.
* **`HAND_ABORT` causes 2 and 3.** This build refuses them rather than check
  evidence it cannot verify, and the embedded evidence needs a larger
  `FRAME_CAP`.

### Rules questions nobody has answered

* **A seat certified absent never re-enters.** `next_hand` only ever filters
  `self.open.required`, in both branches since the roster was made monotone.
  D-013 says one silent seat should cost one hand and not the table, and this
  costs the table. It is consistent and it may be wrong.
* **The kind-1 fork is unrepairable at the receiver.** A betting stage is
  single-writer, so a subject certified out of position and the voters are on
  two branches at one sequence, and §3.2 forbids redefining a `stage_hash`
  already chained from. It is reported by name and nothing more.
* **A withheld `HAND_ABORT`, released at a stage of the attacker's choosing.**
  The receiver's own expired deadline still gates it, so the attacker chooses
  the moment inside a window the receiver had already entered. Whether that is
  worth closing, and at what cost to the witness-independent terminal, is not
  settled.

### Two disagreements that have instruments and no explanation

Both were last seen at the moment a killed client leaves, which is the most
turbulent point of a run, and neither has recurred in the five runs since.

* **A shuffle proof that fails verification** (`the argument does not hold for
  this pair of decks`). Ruled out: the input deck hash is compared before
  verification and passes, so both hold the same deck. `ctx_report` now prints
  the three parts of the context that can vary, from **both** sides.
* **A settlement mismatch.** Ruled out: this client's own timeout action follows
  the same rule the certificate does, so a self-fold and a certified check
  cannot be two different hands. The refusal now prints both `final_stacks`
  vectors and both pot counts.

### Coverage that is missing rather than broken

* **The forwarding fix has no test, and now has a shape instead.** The
  in-process harness delivers every message to every survivor, which is a mesh
  that forwards — by construction it cannot see a client that fails to — and the
  real thing needs three libp2p nodes with two of them unmeshed *and* a formed
  table between them, which is a large and flaky test guarding three lines.

  So the hand branch was restructured rather than tested: the match now yields
  `Option<MessageAcceptance>` and there is **one** report and **one** `continue`
  after it. An arm cannot leave without saying what became of the message,
  because there is no way out of the match that reaches the top of the loop.
  `None` is the single deliberate exception and it means *this was not a hand
  event* — the formation handler below reports for it.

  The three-node test is still the only thing that would prove forwarding
  end to end, and it is still not written. What changed is that the defect it
  would catch can no longer be written by accident.
* **Formation: five of five most recently, ten of twelve before that, and the
  failure is characterised.** The best single run so far is 8 s. The number
  moves between batches by more than the change between them, so treat any one
  batch as weak evidence — what is solid is the failure's shape. From an earlier
  batch of five, the one that failed:

  ```
  founder : (no lobby-topic line at all — it was connected to nobody)
  joiner 2: 0 of 1 subscribed ["R5oyWW"]; connected ["R5oyWW"]     <- sees joiner 3
  joiner 3: 0 of 1 subscribed ["W6wVZG"]; connected ["W6wVZG"]     <- sees joiner 2
  ```

  **The two joiners found each other and neither found the founder, and the
  founder found nobody.** Its log holds no `found … on this network` and no
  `another poker client` — only failed dials to internet peers. `mdns` queries
  every 15 s, so it had six chances in the window and its responder did not
  answer the joiners' queries either. That is one process's multicast being
  dead for a run, on a Windows host with several virtual adapters
  (`172.27.224.1` is in its listen set), and it is not fixable from inside the
  client.

  What *is* fixable is that it said nothing. `hosting T`, then `public lobby:
  nobody else yet`, then silence — indistinguishable from a table waiting for
  players when it is the opposite. The housekeeping tick now says so once.

  This is also the clearest argument for the item below it: across two networks
  mDNS is irrelevant, and the DHT and relay path — the one that would carry a
  real game — is exercised by nothing here.
* **Two machines on two networks.** Everything measured so far is three
  processes on one, which is the environment least likely to show a NAT,
  relay or forwarding fault.
