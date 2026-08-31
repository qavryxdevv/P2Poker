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

1. ~~Three seats and more over the real network~~ **done at three, and it paid
   for itself in one run** - see below. It found the re-send loop still
   publishing to the mesh on a Tox table, which cost a seat its place at the
   table. Four hands at three seats now, exact agreement, no certificates. What
   is still untried is **more** than six, and a table that fills to ten is where
   the next one of these will be. Six seats plays: seven hands in a hundred and
   fifty seconds, all six agreeing, no certificate. See the timing section above
   for what a hand costs at each size, and why the earlier "slow at six seats"
   reading was wrong.
2. `STATE_HASH` / `STATE_ACK` checkpoints. Checkpoint 8's hash is computed and
   carried inside `HAND_COMPLETE`; the checkpoint **stage** is not there, and
   `PROTOCOL.md` §12 says T61 then fires at `hand_deadline_ms` after every
   settled hand.
3. The RNG beacon, replacing `provisional_button`.
4. **The Tox transport (D-019).** The relay is why: a hand carried over a
   libp2p circuit dies at 128 KiB, measured. c-toxcore builds and links here
   now; see the D-019 section below for what it found and what is left.
5. `HAND_ABORT` causes 2 and 3 — **done**, see below. What is left of the cause
   register is `4` and `6`, and both need machinery that does not exist rather
   than evidence handling that does.
6. Two machines on two networks. **Done** - see below; the remaining
   coverage hole is a NAT, not a second network.

(This list ran 1, 3, 4, 5, 6 for several passes: item 2 - a hand between two
real processes - was struck when it was measured and the numbering was never
closed up. Worth noting only because a gap in a numbered list reads as a lost
item, and somebody will eventually go looking for the one that was never
there.)

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
4. **Two machines on two networks.** Done, measured, and written up below.
   What is still untested is a NAT between the two, which needs an endpoint
   outside this building.
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

## The two-network run's real finding: a relayed hand dies at 128 KB

The second run across the boundary is more interesting than the first, because
it failed — and it failed in the one way only two networks can show.

| | run 1, 120 s | run 2, 300 s | run 3, 300 s |
|---|---|---|---|
| table formed | yes | yes | yes |
| hands agreed | 1, `55f97eeb` | 1, `1a2db2ab` | **5**, all five hashes |
| direct connections, far end | 8 | **0** | 13 |
| hands the far end finished | 1 | **0** | 5 |

Run 3 is the strongest cross-network result so far and it is the same finding
from the other side: thirteen direct connections at the far end, five whole
hands, and both ends agreeing on all five genesis hashes. The variable that
moves is not the run length - runs 2 and 3 were both 300 s - it is whether DCUtR
got through.

In run 2 the far end reached `hand #1: the deck is shuffled and sealed` and
then said nothing about that hand ever again. This end reached `your turn — 50
to call`, then `not published: NoPeersSubscribedToTopic` five times, then `your
clock ran out — Fold for you`, then `the hand ran out of time; every stack is
restored`. It opened hand #2, waited for seat 1, and timed that one out too.

**The cause was printed on line 20, three thousand lines earlier, at both
ends:**

```
relay 12D3KooWJYusw…: 131072 bytes / 120 s, NOT enough to carry a hand
```

Every public relay either refused a reservation or granted the libp2p circuit
relay v2 defaults — 128 KB of data and two minutes of circuit — and
`relay::adequate` correctly compares that against what a hand costs and says
no. In run 1 DCUtR hole-punched the far peer and the circuit stopped mattering.
In run 2 it did not, the hand was carried over the circuit, and the circuit ran
out somewhere around the deal: the deck messages are 9 KB per shuffler and
`DEAL_PRIVATE` is ~2 KB heads-up, which is most of a 128 KB budget once the
lobby traffic sharing that circuit is counted.

**So the game across two networks depends on hole punching, not on the relay.**
The relay finds the peer and carries the formation; it cannot carry the hand.
That is a property of the public relays that exist, not of this protocol, and
nothing in the client can raise their limits.

### What was changed, and what deliberately was not

Changed: **the deadline says so.** `run.rs` now tracks whether the reservation
was judged inadequate and which poker peers DCUtR failed for, and when a hand
dies at its deadline with both true it names the relay as the likely cause and
says it is *"not the opponent being slow"*. The facts were all present before
and each was said once, far apart, in a log where three thousand lines of DHT
chatter separate them — a player reading `the hand ran out of time` had no
route back to line 20.

Also changed: `tools/two-network-ssh.ps1` reports direct connections at **both**
ends, peers left on the relay, the count of too-small reservations and the count
of `NoPeersSubscribedToTopic`. It reported direct connections at the far end
only, which is the number the whole result turns on, and a run that says "a
table formed" without saying how it was carried invites the wrong conclusion.

Not changed, and these are decisions rather than omissions:

* **No refusal to start a hand over a thin circuit.** It is tempting and it is
  wrong: the reservation limits are what the relay *reported*, hole punching
  can succeed at any moment, and a client that refuses to deal would turn a
  hand that usually works into a table that never starts.
* **No relay of our own.** A reachable peer already becomes a relay for the
  others (`--port` exists for exactly that), so the answer already in the design
  is *one player who can forward a port*, not a piece of infrastructure - and
  the owner's answer to *that* is D-019, because a client whose playability
  rests on somebody having forwarded a port is a client most people cannot use.
  See the Tox section below: the work has started.
* **Nothing about hole-punch success rate.** Two runs are two runs. What is
  solid is the mechanism and the numbers above; the rate needs many more.

## `HAND_ABORT cause = 2` is implemented, and it found a size the corpus cannot send

A refused shuffle proof used to cost ninety seconds and name nobody.
`accept_step` spends a seat's one attempt **before** verifying the argument
(C-9), deliberately, so a bogus proof cannot be retried — which means the chain
can never complete once one is refused. There was nothing left to wait for and
every peer waited anyway, until `hand_deadline_ms`, and then aborted with
`attributed = []`.

§4.10 has always had the answer: `cause = 2`, whose evidence *carries its own
disproof*, and whose acceptance row is **accept at once** — no deadline, no
certificate, no other seat's agreement. That row is also the most dangerous
sentence in the abort table, because one message with it would otherwise let any
seat void any hand it disliked. What makes it safe is that the permission is
conditional on arithmetic every receiver redoes.

### The gate, and what a hostile accuser cannot reach

`Hand::bad_shuffle_holds` accepts only when **this client's own** run over the
two carried frames returns `VerifyOutcome::Invalid`. In order:

1. Exactly two entries, the step then the proof.
2. Both open **in this hand** — table, hand, catalogue envelope, signature — and
   both are signed by the seat the abort names. Position is *not* checked and
   must not be: the accused's frames sit at the stalled stage, which is exactly
   where the receiver's cursor is not.
3. The receiver's chain must be at the accused's position, and the proof's two
   deck hashes must match the step's deck and the receiver's own input deck.
   Otherwise it **holds** the abort rather than refusing it — refusing would
   punish a peer for the receiver's own position, and `run.rs` turns anything
   but `NotYet` into a GossipSub `Reject`.
4. The argument, against the receiver's **own** input deck, at the context its
   own chain derives, from the **proof's own signed `sequence`**. The accuser
   supplies none of those three. It supplies two frames the accused signed, and
   nothing it chooses enters the verification.

**`CouldNotVerify` holds, it does not confirm.** The first version tested
`verdict.is_ok()`, which accepted on *any* error — including the one outcome
`VerifyOutcome` exists to separate out, *verification could not be attempted,
never evidence against anybody*. That would have let an accuser end a hand at
any receiver whose verifier was unavailable: the gate's whole purpose, defeated
through the arm that looks like agreement. It matches now, and the third arm
holds.

Four tests on the gate, and the one that matters most is the refusal: seat 1's
own genuine, verifying frames dressed up as evidence against seat 1 are rejected
and **the hand goes on**. The confirming test re-seals a real proof at its own
slot with the argument replaced by zeroes - correctly signed by the accused,
wrong only in the argument, which is the only shape that tests anything.

A fifth test changed rather than appeared, and the change is the feature:
`an_argument_that_does_not_hold_is_refused_as_a_shuffle` asserted
`Failed::BadShuffle` and now asserts an `Ok` carrying a `cause = 2` naming the
seat, with both frames, no certificate, and a body every receiver's own
`consistent` admits. It is renamed to say what it now checks. The distinction
its old name drew is still drawn next door: a mismatched deck hash is
`Failed::BadDeck`, because only a failed *argument* is decidable from the frames
themselves and therefore only that is evidence.

### Cause 3, the same shape over one frame

Done in the same pass. A reveal share carries its own token and its own proof,
and the deck they are checked against is the committed one every seat already
holds - so the evidence is **one** frame where cause 2 needs two.

Three pieces made it possible without duplicating a verification:

* `Dealing::would_verify` is `accept` with the recording half cut off, and
  `accept` now calls **through** it. A second copy of a check is a second copy
  that can drift, and the whole value of the cause is that the accuser and every
  judge ran the same one.
* `Failed::RevealDisproved` is split out of `BadToken` for exactly
  `DidNotVerify(VerifyOutcome::Invalid(_))`. `NotDue`, `UnknownSeat` and
  `CouldNotVerify` stay `BadToken`, because a receiver that is behind or cannot
  run the check has found nothing about the sender.
* It is caught in `on_event`, above the three reveal handlers, because that is
  where the offending frame still exists - the handlers all run inside a borrow
  of `self.phase`, where neither the abort nor the note could be built. One
  catch costs three restructurings and one variant.

And one hazard the shuffle case did not have: `deck_ctx` builds the context from
**this client's own** `slot.sequence`. Re-deriving evidence that way would make a
perfectly good share fail wherever the receiver had moved on - turning a false
accusation into one that *succeeds*, at exactly the peers that were ahead. The
gate uses `deck_ctx_at` with the frame's own signed `sequence` instead.

The gate answers three ways rather than two, and the third is the one that
matters: any entry `Invalid` accepts, every entry verified refuses, and anything
else - `NotDue`, `UnknownSeat`, `CouldNotVerify`, an index or a token this client
cannot decode - **holds**. Undecodable bytes are attributable under §4.0, but as
a tier-1 `cause = 6` finding, not this one, so they are held rather than quietly
promoted to a different accusation.

### The finding: §4.10's evidence bound does not fit in the frame that carries it

§4.10's field table bounds `n(3) evidence` at *two `SignedEvent`s, each ≤
`MAX_EMBEDDED_EVENT` = 32 768 B*. **Two of those is 65 536 bytes, which is
`GOSSIP_MAX_TRANSMIT` exactly** — before the envelope, the signature,
`attributed`, `cert_hash`, `deltas`, `final_stacks`, and CBOR's own framing. A
conforming peer can build a `HAND_ABORT` this corpus calls legal that the
table's transport cannot carry: the sender's own GossipSub refuses it at
`publish`, so the hand it was meant to end runs to its deadline instead and
nobody ever sees the message.

The corpus is not self-contradictory so much as split across two documents.
`PROTOCOL.md` §13 sizes embedded evidence against `TABLE_FRAME_MAX` (262 144 B,
the **table stream**); `NETWORK_STACK.md` §6.2 caps GossipSub at 65 536. A table
whose traffic is on the stream — or on the Tox group D-019 moves it to — has the
room. This client publishes hand events on GossipSub, so this client does not.

What was done about it is stated and is not a fix: `ABORT_EVIDENCE_MAX = 24 576`
is a **transport-honest** bound, tighter than §4.10's, so it refuses nothing
§4.10 permits *that could have arrived* — a larger one could not have been sent.
Three compile-time assertions hold it there, and the second is the one that
matters:

```rust
const _: () = assert!(HAND_ABORT_MAX <= GOSSIP_MAX_TRANSMIT);
const _: () = assert!(2 * MAX_EMBEDDED_EVENT + ABORT_FIXED_MAX > GOSSIP_MAX_TRANSMIT);
const _: () = assert!(ABORT_EVIDENCE_MAX <= MAX_EMBEDDED_EVENT);
```

The middle one asserts that the conflict **exists**, so a later pass that raises
`ABORT_EVIDENCE_MAX` to `MAX_EMBEDDED_EVENT` — which looks exactly like bringing
the client into line with the protocol — breaks the build instead of shipping
aborts nobody can send.

Real sizes are far below either bound: a `SHUFFLE_STEP` is about 9 KB and a
`SHUFFLE_PROOF` about 5.6 KB, so no cause-2 abort this client builds comes near
it. The bound decides what is *refused*, not what is sent.

**Owed to `PROTOCOL.md`, and an implementation may not decide it:** either a
smaller per-element bound in §4.10's table, or hand traffic on the stream. Both
are wire decisions.

### Two smaller things the same pass turned up

* **`consistent` did not bound `evidence` at all.** The decoder's cap bounds the
  whole body, so a two-entry `evidence` could never be enormous — but a
  hundred-entry one of small elements passed every check and reached a gate that
  reads `evidence[0]` and `evidence[1]`. The count and the per-element size are
  both refused now, before anything is opened.
* **`peek` runs before the type is known**, so it cannot use a per-type cap and
  now takes the largest chained frame any type may have. What that gives an
  attacker is written down where the constant is: one CBOR decode of up to
  `HAND_ABORT_CAP` instead of `FRAME_CAP`, for bytes libp2p has already read and
  buffered, retaining nothing — every `open` still uses its own type's cap.

## D-019 has started: c-toxcore compiles and links here

The relay finding above is what D-019 exists to answer, and the owner confirmed
the direction on 2026-08-31. The first step is done and it is the one that could
have failed: **`c-toxcore` builds and links on this machine**, and a Tox instance
comes up.

| | |
|---|---|
| `c-toxcore` | `v0.2.23`, pinned at `d9ca3c57` |
| `third_party/cmp` | pinned at `52bfcfa1` |
| `libsodium` | `1.0.20-RELEASE`, pinned at `9511c982` |
| built archive | 6.75 MB, 58 core sources |
| tests | an instance starts with a distinct identity; a bootstrap address is accepted; the loop turns |

Behind `--features tox`, which is **not** in `default`. The feature flag is the
line where the licence changes: `c-toxcore` is GPL-3.0 and linking it makes this
whole client GPL-3.0. D-019 takes that decision and calls it a one-way door;
what this adds is that the door is visible in the build rather than implicit.

### Three things the build did not need, and one it did

`c-toxcore`'s own CMake build cannot be used on MSVC without **pkg-config**
(`find_package(PkgConfig REQUIRED)` fails outright when it is absent), a
**libsodium CMake package config**, and in practice **vcpkg**. None of the three
is on this machine and none of them should become a requirement for building a
poker client. `build.rs` compiles the sixty C files with the `cc` crate instead,
reading the file list out of the vendored `CMakeLists.txt` — parsed, not
transcribed, so a version bump cannot leave the two out of step, and only the
first `set(toxcore_SOURCES` block, because the second is `toxav` and this client
carries no audio or video.

What it did need is **pthreads**, which MSVC does not have.
`tools/msvc-shim/pthread.h` is 150 lines and supplies exactly the fifteen calls
`grep -rho "pthread_[a-z_]*"` finds in the tree — recursive mutexes and
reader-writer locks, nothing else. Two decisions in it are worth reading:
`pthread_mutex_t` is a `CRITICAL_SECTION` because toxcore asks for
`PTHREAD_MUTEX_RECURSIVE` explicitly and an `SRWLOCK` deadlocks on the second
take; and `pthread_rwlock_t` is *also* a `CRITICAL_SECTION`, exclusive in both
directions, because POSIX has one `pthread_rwlock_unlock` for both modes and
Win32 has two — recording the mode in the lock is wrong the moment two readers
hold it, which is precisely the case a reader-writer lock exists for. An
exclusive lock is always correct and only less parallel, and these guard short
list operations rather than I/O.

Nothing in the shim is stubbed to a no-op. A future toxcore that uses threads or
condition variables fails to compile here rather than linking against something
that does not do what its name says.

### The finding: c-toxcore has no UPnP and no NAT-PMP

D-019's requirements say, in the owner's own words, that the Tox instance
*"opens its own port, through NAT-PMP and UPnP, and both are on without anybody
choosing them … not an option, not a setting, not a build flag somebody has to
remember"*.

**`c-toxcore` has neither.** The whole vendored tree — every `.c`, every `.h`,
every CMake file — contains **one** occurrence of either word, and it is a
sentence in `docs/TCP_Network.txt` observing that they *can help*. There is no
`tox_options_set_*` for it, no build flag, and nothing to compile in. The
requirement cannot be met by configuring toxcore, because toxcore does not do
it.

What Tox does instead is UDP hole punching plus TCP relays — and that is the
answer to the problem the requirement was written for, since a Tox TCP relay
carries a session with **no per-circuit byte cap**, which is the whole reason
for D-019. `hole_punching_enabled` and `local_discovery_enabled` are both set
explicitly in `Tox::new` rather than left to the default, so the setting is
visible in the code.

If genuine port mapping is still wanted — and the owner's reason for asking is
sound, since a player whose router would have opened a port and did not is a
player who cannot host — it belongs to **this client** and not to toxcore: map a
port with the IGD machinery already in the tree for libp2p (`libp2p`'s `upnp`
feature is in `Cargo.toml` today), then pin Tox to it with
`tox_options_set_start_port` / `set_end_port`. That is a separate piece of work
and is not pretended to.

### Two more findings from reading the API, both requirement-level

Neither sinks D-019. Both change what has to be built, and both were invisible
until somebody read `tox.h` with the sizes of this protocol's messages in mind.

**1. Every Tox channel caps at ~1372 bytes, so fragmentation is mandatory.**

```
TOX_GROUP_MAX_CUSTOM_LOSSLESS_PACKET_LENGTH   1373
TOX_GROUP_MAX_CUSTOM_LOSSY_PACKET_LENGTH      1373
TOX_GROUP_MAX_MESSAGE_LENGTH                  1372
TOX_MAX_CUSTOM_PACKET_SIZE                    1373
TOX_MAX_MESSAGE_LENGTH                        1372
```

There is no larger channel. File transfer is the only unbounded path and it is a
file transfer, not a message. So the choice is not *which* Tox channel avoids
fragmenting — it is that this protocol fragments.

What that costs, in this protocol's own numbers: a `SHUFFLE_STEP` is about 9 KB,
so **seven** fragments; a `SHUFFLE_PROOF` about 5.6 KB, **five**; a `cause = 2`
`HAND_ABORT` carrying both is about 15 KB, **twelve**; and the largest this
client will build one is `HAND_ABORT_MAX` = 51 200 B, **thirty-eight**.

It is not hard — the channel is lossless and ordered per sender — but it is real
machinery and it is fed from the network, so `SPEC_CS.md` §27 applies to every
part of it: the fragment count a sender may claim is bounded before anything is
allocated, the number of part-built messages per peer is bounded, and a stream
that stops half way is dropped on a timer rather than held. A reassembly buffer
that trusts a sender's `total` is the same defect as a container keyed on a
sender-chosen quantity, which this project has refused twice already.

One consolation, and it is the reason the relay problem is still solved: **a
fragment count is not a byte cap.** A libp2p circuit stops at 128 KiB and the
hand dies; Tox has no such ceiling, so a hand costs more packets and no
deadline.

**2. An invitation needs a friendship, so a Tox public key has to reach the
roster.**

`tox_group_invite_friend` takes a **friend number**, not an address — there is
no "invite this public key". The owner's step *"a player joining the table is
automatically invited into the Tox group"* therefore has a hidden prerequisite:
the founder and the joiner must already be Tox friends.

The good news is that it needs no user interaction and no friend *request*.
`tox_friend_add_norequest(tox, public_key)` adds a friend from a 32-byte public
key alone. So both ends add each other from the ratified roster and the
invitation follows:

1. The table advertisement carries the founder's Tox **public key** and the
   group's `chat_id`.
2. The join RPC carries the joiner's Tox public key.
3. Both sides call `tox_friend_add_norequest` on the other, from the roster.
4. When the friend connection comes up, the founder calls
   `tox_group_invite_friend`; the joiner's `group_invite` callback answers with
   `tox_group_invite_accept`.

**This is what keeps D-019's central claim true.** The decision says group
discovery through Tox's DHT is not on the critical path *because members arrive
by invitation*, and that matters because a public NGC group is findable only
while it is new (measured 2026-08-27: a host up 20 s found in 31 s, a host up
six minutes never found in 300 s). Joining by `chat_id` with
`tox_group_join` **is** that decaying path. Joining by invitation is not, and
now there is a route to an invitation that no user has to click.

It also uses the path that was measured to work: friend connections have LAN
discovery and hole punching, and the same two nodes that could not exchange a
byte over a stale group went `UDP direct` in three seconds as friends.

**The wire change this asks for** is one field in the advertisement and one in
the join request, both 32 bytes. That is `PROTOCOL.md`'s to make, not an
implementation's, and it is the third thing D-019 now owes a document.


### Measured across two networks: the group carries 3.4 MB each way

`tools/two-network-tox.ps1`, 150 s, the same two machines the libp2p runs used:

```
here : self 2  sent 2930  received 2524  bytes_in 3 445 260
there: self 2  sent 2610  received 2599  bytes_in 3 547 635
here : friend up after 13.6s        there: friend up after 12.7s
there: joined the group after 12.7s
here : FIRST PACKET after 17.1s     there: FIRST PACKET after 16.0s
RESULT  the group carried traffic BOTH ways across the boundary
```

**A libp2p relay circuit stops at 128 KiB.** This run moved twenty-seven times
that in each direction and did not stop — which is the whole of D-019's case,
now measured rather than argued. The invitation crossed in under thirteen
seconds and nobody searched the DHT for the group: it is created `PRIVATE` and
its `chat_id` is never looked up.

### It failed three times first, and each failure is worth keeping

**1. A Tox friendship is two-sided.** The first probe had only the joiner call
`tox_friend_add_norequest`. The other end has never heard of the caller and does
not answer it, so the result was 150 s of silence that read exactly like an
unreachable machine. In the client this cannot happen — both ends take both keys
off the ratified roster — but the probe had no roster, so identities are now
seeded and each end is told the other's key with `--peer` before either starts.

**2. Nothing said whether either end had reached the Tox network at all.**
`tox_self_get_connection_status` was not bound, so "no friend connection" and
"never bootstrapped" looked identical. It is bound now and printed on every
change, and it immediately showed both ends UDP-connected in nine seconds — which
moved the question from *"is the network there"* to *"why can these two not find
each other"*.

**3. And the answer to that was: the relay list was missing.** toxcore keeps
**two** lists. `tox_bootstrap` takes DHT nodes, reached over UDP.
`tox_add_tcp_relay` takes relays, and it had never been called. With
`udp_enabled(false)` and no relay the connection status stays at `none` for
ever; with UDP on, both ends reach the DHT and still cannot reach *each other*.

The owner's correction is what closed it: **a client holds connections to
several relays at all times, even with UDP working** — qTox does — because a
relay is the fallback for a peer that cannot be reached directly, and that is
the answer to a NAT. Every node is now added twice, to the DHT and to the relay
list on every TCP port it advertises, from the public list at
`https://nodes.tox.chat/json` filtered to nodes reporting both up. Ten nodes,
twenty-six relay entries.

### Why it needed the relays here, and it is the interesting part

**Both machines share one public address**, measured: `198.51.100.17` from each.
They are on different subnets behind one router. So Tox's DHT publishes both
under the same address and a hole punch asks the router to hairpin a packet back
to itself, which consumer routers generally will not do; and Tox's only
non-public path is LAN discovery, which is multicast and does not cross a subnet
boundary.

**libp2p succeeds in that topology and Tox does not**, because DCUtR exchanges
*private* address candidates as well as public ones and the two subnets are
routable to each other — that is why the libp2p runs got direct connections here
at all. Tox has no equivalent. What it has instead is the relay, which is a
third party with an address of its own, so nothing needs to hairpin.

So this bench is **harder** for Tox than a genuine pair of networks would be,
and the result stands anyway. What it does not prove remains what it did not
prove for libp2p: NAT traversal between two different NATs, which needs an
endpoint outside this building.


### Measured: hands of poker played across the boundary, over Tox

`tools/two-network-tox.ps1 -Hand`, the same two machines, 200 s:

```
here : HAND 1 DONE after 35.3s   there: HAND 1 DONE after 33.8s
here : HAND 2 DONE after 52.2s   there: HAND 2 DONE after 49.9s
                                 there: HAND 3 DONE after 61.3s
RESULT  2 hand(s) of poker played across the boundary over Tox
```

That is `HAND_INIT`, the deck chain, `DECK_COMMIT`, `DEAL_PRIVATE`, four streets
of betting, the showdown and `HAND_COMPLETE` — the real protocol, verified at
both ends — over a private Tox group, with every message over 1 365 bytes
fragmented and put back together. About **seventeen seconds a hand** after the
first, which carries the connection setup. Locally the same probe plays a hand
in the same time, so the boundary costs nothing measurable once the two are
connected.

The near end shows two hands where the far end shows three: `tox_hand` exits
when **it** has played its count, and the seat that finishes first leaves before
the other has the last message. That is the probe, not the protocol — in a
client neither end leaves at the end of a hand.

`examples/tox_hand.rs` is the hand driver on the Tox transport and nothing else:
no lobby, no join RPC, no ratification. Both ends are handed the same `Opening`,
which is what the formation would have agreed, because what is under test is
whether the transport carries a hand and not whether two clients can agree to
start one.

### Two defects it found, and the second is the one worth keeping

**1. Leaving threw away what had not been sent.** `Command::Leave` broke the
driver's loop, and what is still queued when a client stops is the *last* message
it produced — the `HAND_COMPLETE` that ends the hand. Measured: a hand played to
the river, the seat that finished first left, and the other sat at the settlement
stage until its deadline waiting for a message that had been built, queued and
discarded. The driver now flushes before leaving, bounded at thirty turns so that
leaving cannot become waiting.

**2. A Tox group keeps no history either, and that is not a GossipSub quirk.**

`run.rs` re-broadcasts everything this client has said every five seconds,
because GossipSub keeps no history and a peer grafted after a publish never sees
it. The obvious reading is that this is a workaround for GossipSub. It is not: a
Tox group has the same property, and the first version of this probe proved it
the hard way.

The joiner published its `HAND_INIT` while the group was still empty — the
founder learns a peer has joined after the peer does — so it went to nobody. The
joiner then heard the founder's `HAND_INIT`, counted stage 0 complete against
its own copy, and stopped re-sending. The founder held one event it could not
place and waited out the hand. **Both ends were healthy, neither reported an
error, and the hand was dead.**

So the re-send buffer is a property of any transport without history, not of
libp2p, and D-019 does not remove the need for it — it changes which transport
lacks the history. Worth stating plainly, because the natural thing to do while
moving off GossipSub is to leave that loop behind.






### The table's own topic was never re-announced, and a seat can still miss a ratification

A three-seat run went 0 hands at the founder and 17 at the other two, playing as
seats `[1, 2]`. The founder had connected to both, seated both, and joined the
Tox group; what it never got was either `TABLE_READY`. Its own log carried the
tell, twice:

```
lobby topic: 0 of 2 subscribed ["QiQTLr", "cHbDFq"]; connected ["cHbDFq", "QiQTLr"]
```

Connected to two peers and knowing neither of them to be subscribed to anything.

**One real gap, now closed.** `run.rs` re-announces its subscriptions when a
poker peer appears — and it re-announced **only the lobby topics**, guarded by a
check on the *lobby* hash alone. A peer subscribed to the lobby and not to the
table therefore read as "already fine", and the table's topic is the one
`TABLE_READY` travels on. The check now covers every topic this client holds and
the re-announce includes the table's, plus up to three retries at five seconds
while a table is forming.

**And a regression of mine, caught by the next batch.** The first version of
that retry said *all three* topics every five seconds for as long as a table was
forming. Every re-announce prunes this client from every peer's mesh for that
topic, so the lobby churned hard enough that adverts came back `RateLimited` —
at the founder, against its own advertisement — and the table never formed at
all. The retry now says the **table's topic only**, which disturbs the seats
already at the table and nobody else, and it is bounded at three.

Batches of three-seat runs, `--autoplay`, sixty seconds each:

| | clean |
|---|---|
| before | (the failure above, one in five) |
| all three topics every tick | 5 of 6, and `RateLimited` in the failure |
| the table's topic, three times | **7 of 8, no `RateLimited` anywhere** |

**What is left is not closed and should not be read as closed.** The one failure
in eight was a *joiner* this time, not the founder: `3 seated` three times, in
the Tox group, and then `NO TABLE` — with `0 of 2 subscribed` in its log again.
So the residual is not about which seat it is; it is that a client on this host
sometimes never learns its peers' subscriptions at all. The existing note about
one process's multicast being dead for a run points the same way, and both say
the same thing: it is at least partly the environment, and a client cannot make
a peer tell it something.

### The counters earned their keep in the first batch

From the same run, at the founder:

```
the table's transport is behind: 1 message(s) queued, 84 fragment(s) refused of 84 offered
the table's transport is behind: 0 message(s) queued, 211 fragment(s) refused of 214 offered
```

**Every fragment of hand 1 refused**, because the founder opens hand 1 the
moment the roster ratifies and the joiners are not in the Tox group yet — the
founder learns a peer has joined after the peer does. The re-send loop covers it
and three fragments eventually went, but it is why the first hand of a table
takes longest, and without the counters it was invisible.

### How long a hand takes, and it was not the network

Six seats looked slow — one hand in three hundred seconds, then a stall after
the deal. Two things were read into that which were not true, and finding out
which cost less than fixing the wrong one would have.

**It was the clock, not the transport.** A headless client has nobody at the
keyboard, so every seat sits out its whole allowance. Measured at six seats:

```
41,1s  the deck is shuffled and sealed
41,5s  n5 your turn        96,5s  n5 your clock ran out   <- 55 s
96,5s  n4 your turn       151,5s  n4 your clock ran out   <- 55 s
```

Fifty-five seconds a seat: `action_timeout_ms` 20, `action_grace_ms` 5 and a
30-second time bank. Six seats is 330 s for **one betting round** and over
twenty minutes for a hand, none of it network. Two counters say the same from
the other side: **no send was ever refused by toxcore and the queue never had
anything waiting**, across every six-seat run.

`--autoplay` was added for exactly this and nothing else. It acts as soon as it
is this client's turn, or after `N` ms, and it **calls** where the ordinary
timeout folds — a table that folds every hand preflop measures nothing about how
long a hand takes. It is a measurement flag and it says so: it puts its owner's
chips in, which is what the ordinary timeout deliberately refuses to do. The
timeout's own behaviour is untouched.

**The floor, measured:**

| seats | per hand |
|---|---|
| 2 | 8.6 s |
| 3 | 12.9 s |
| 6 | 13.5 s steady state |

And where a six-handed hand's time goes, stamped from one run:

```
56,7s  hand #2 opens        60,4s  the deck is sealed   <- 3.7 s, six shuffle links
60,4s                       61,1s  the first turn      <- 0.7 s, the deal
61,1s                       63,9s  the hand is over    <- 2.8 s, four streets
63,9s                       68,9s  hand #3 opens       <- 5.0 s, the pause
```

**Seven seconds of work and five of pause.** The single largest lever on how
fast a table plays is therefore not the protocol and not Tox: it is the hold
between hands, which is a policy dial. The shuffle chain is 3.7 s at six seats
and is the part that cannot be shortened without changing the cryptography — it
is `m` sequential links by construction, because each shuffler must shuffle the
previous output.

The 36 s per hand quoted from an earlier run was an arithmetic mistake: 180 s
divided by five hands, counting a 27-second startup and a hand the run cut in
half.

### What was changed for it, and what was deliberately not

**The re-send loop backs off and narrows.** It repeated every event of the hand
to every member every five seconds — at six seats about fifty fragments per
client per tick, three hundred deliveries a second across the group, without
pause. It now re-sends on ticks 1, 2, 4, 8… since the chain position last moved,
resetting when it does, and only events from the last three stages. A table that
is advancing re-sends nothing at all.

**No catch-up from a cache, and no chat history.** It was considered and is not
needed: the counters say the transport was never the constraint. It is also less
available than it looks — qTox keeps history in its **own** database, and Tox
delivers nothing old to a member who was not there, so "the client has history"
would not have let a peer fetch what it missed. If a future measurement shows
the transport behind, the counters will say so and a targeted request over
`tox_group_send_custom_private_packet` is the shape to build; until then it is
machinery for a problem that has not appeared.

**Two counters, because two stalls in a row were diagnosed from logs that said
nothing.** The driver counts fragments refused, fragments sent and whole
messages waiting, and the node says *"the table's transport is behind"* when
either is non-zero. A seat certified late for something it did say looks, in
every other line this client writes, exactly like a seat that said nothing.

### Three seats on Tox, and the defect it found in one run

The top of the list for several passes — *everything measured is heads-up, and
heads-up hides a whole class of defect* — and the first three-seat run on Tox
found one immediately.

**Before:**

```
node 0: certificates 2, hands cfe61e0e c6b7d73b
node 1: certificates 0, hands cfe61e0e
node 2: certificates 2, hands cfe61e0e c6b7d73b
```

Seat 1 was certified late **twice** by the other two, unanimously; its two
strikes exhausted its grace and the roster dropped it —
`required [0, 1, 2] -> [0, 2]`, `by_certificate=true`. Its own log records one
expired clock, not two, so its messages were not reaching the other seats. A
seat that was alive, in the group, and dealt in, lost the table.

**The cause was mine, and it was the thing I had already written down.** The
five-second re-send loop published to the **mesh only**. A table on Tox
therefore re-sent nothing at all — and a Tox group keeps no history exactly as
GossipSub keeps none, which is the entire reason that loop exists. It is
recorded two sections above, from the probe that proved it, and the integration
left the loop on the old transport anyway.

Heads-up hid it perfectly: two peers that are both in the group before the first
hand have nothing to re-send.

**After**, and a longer run:

```
node 0: certs 0, aborts 0, hands 5efd05c1 28bcfcc9 ff81cd29 efae6fc3
node 1: certs 0, aborts 0, hands 5efd05c1 28bcfcc9 ff81cd29 efae6fc3
node 2: certs 0, aborts 0, hands 5efd05c1 28bcfcc9 ff81cd29 efae6fc3
```

Four hands, three seats, exact agreement, no certificate and no abort in seven
minutes. The libp2p path at three seats is unchanged: `--no-default-features`,
two hands, all three agreeing, no certificates.

One thing was fixed alongside it rather than after the next run finds it: the
driver's outbound channel was sixty-four deep and the re-send burst is up to
sixty-four messages, so a fresh action could be the one refused. It is 512 now.
Refusing a re-send is free; refusing a new event costs a seat its deadline.

### And a coverage item that Tox retires

`AnotherHand => Accept` and the three-node forwarding test were about GossipSub,
where a message to the third seat may have to travel *through* the second, and a
client that fails to relay is invisible to any harness that delivers to
everybody. **A Tox group delivers to every member**, so there is no relaying by
peers to get wrong and no forwarding property to test.

The verdict machinery stays exactly as it is, because the mesh still carries the
formation and because `--no-default-features` still plays whole hands on it. What
goes is the *missing test*: it would now be testing a path the released client
does not use for hands.

### The result D-019 was taken for: four hands, two networks, one real client

`tools/two-network-ssh.ps1 -Seconds 300` — the same script that measured the
libp2p path, running the same released binary, which now carries Tox by default:

```
here  : this table's traffic rides a Tox group, 382e1d5b
        in the table's Tox group 382e1d5b; the hand rides it from here
there : this table's traffic is on a Tox group, 382e1d5b; waiting to be invited
        in the table's Tox group 382e1d5b; the hand rides it from here

hands here 4, there 4, at the same genesis 4
agreed on: ad111fde, b2e971ae, ad8d44af, d0baf112
```

All four **opened and finished** at both ends, on identical genesis hashes,
across the boundary. No probe and no example: this is the client.

Set that beside what the same script measured on 2026-08-31 before any of this,
in the run where DCUtR did not get through:

> the far end reached `the deck is shuffled and sealed` and never spoke about
> that hand again; this end reached `your turn`, then
> `NoPeersSubscribedToTopic` five times, then the deadline — **0 hands**.

That run is why D-019 exists, and this one is its answer. The relay counters are
still in the output and still say the same thing — *five reservations too small
to carry a hand* — and it no longer matters, because the hand is not on a
circuit that has a byte cap.

**What is still not proven** is unchanged and worth repeating, because it is the
first thing lost when a result is passed on: the two machines share one public
address, so this is two subnets with a router between them and not a NAT to
traverse. That is a **harder** case for Tox than two real networks, since the
DHT publishes both under one address and a hole punch would ask the router to
hairpin — which is exactly why the relay list is not optional. It is not a
weaker one. Hole punching between two different NATs needs an endpoint outside
this building.

### The client itself now plays on Tox

Two real clients, `--features tox`, no probe and no example:

```
host : this table's traffic rides a Tox group, f4473d33
       in the table's Tox group f4473d33; the hand rides it from here
join : this table's traffic is on a Tox group, f4473d33; waiting to be invited
       in the table's Tox group f4473d33; the hand rides it from here
both : the table is set: session 954e3d40
       hand #1 opens at genesis 97e8121b
       the deck is shuffled and sealed ... your turn - 50 to call
       hand #2 opens at genesis d3334d08
```

The table forms on libp2p exactly as it did — discovery, the lobby, the join
RPC, the roster, the ratification — and the **hand** rides the group. Hand 1
completed with an agreed terminal at both ends, because hand 2 opened at the
same genesis at both; hand 2 then reached the deal and the betting.

**The whole route, end to end and unattended:** the founder creates a private
group, names it in the advertisement, and takes each joiner's Tox key off its
`JOIN_REQUEST`; the joiner reads the founder's key and the chat id from the
advertisement, adds the founder, and waits; the founder adds each seat and
invites when its friend connection comes up; the joiner accepts, **reads the id
back and compares it against the advertisement**, and only then takes the group
as its own. Nobody clicked anything and nobody searched a DHT for a group.

### What the loop looks like now, and what it deliberately does not

`net::toxsink::TableSink` is the only place with a `#[cfg]`. `run.rs` reads
identically in both builds: an empty sink says no to `is_on_tox`, refuses
`try_broadcast`, and has a `next()` that **never resolves**, so its `select!`
branch is inert. A build with no C toolchain compiles and behaves exactly as it
did.

`publish_hand` is the one door. Two call sites choosing a transport separately
is how one hand ends up half on each, so the choice is made once and `said` is
appended either way — the re-send loop is a property of any transport without
history, which both of these are.

The inbound handling is one macro expanded at two sites rather than a function,
because it reads and writes a dozen of the loop's own locals and threading them
through a signature would be a struct refactor across the file whose most
delicate property — one verdict, one place to report it — was a day's debugging
to arrive at. The Tox branch discards the verdict, which is the only real
difference: GossipSub withholds forwarding until the application reports one,
and a Tox group forwards nothing on this client's behalf.

**Verified that the old path is untouched**: three headless processes with the
feature *off* formed a table and agreed on genesis `412e4e13` and `b738bc9b`,
all three.

### The mixed-transport table, and why it is not a shipping concern

A table can in principle be on Tox for some seats and not others: a joiner whose
build has no Tox joins over the mesh, and its hand events go to a GossipSub
topic the others have stopped reading. It is told so at the moment it joins
rather than at the deadline — *"this table's traffic is on Tox and this build
has none; the hand will not reach it"*.

**The owner has settled it: a build without Tox is not released.** So the
feature is in `default` and the case is a development one — `--no-default-features`
for a contributor with no C toolchain, or a test run with no business opening a
socket. The warning stays because a developer build can still wander onto a real
table and should say so, not because a player will ever see it.

Three things follow from that and are done:

* **`license = "GPL-3.0-or-later"` and a `LICENSE` file.** The repository had
  neither, which is why `cargo deny check licenses` reported this crate itself
  as `unlicensed`. `c-toxcore` declares `GPL-3.0-or-later` in every source file
  and linking it makes this client that, so the licence is now stated without an
  "if" — D-019 closed *"the project licence has never been chosen"* by a
  transport decision rather than a licensing one, and the tree now says so.
* **`README.md`'s "Licence: not yet chosen" is gone**, with the standard notice
  and the one-line build instruction in its place.
* **`build.rs` says what to do** when the vendored C is missing, instead of
  panicking on a path nobody recognises — which is now the first thing a fresh
  clone hits, because the default build needs it.

### What is next, in order

1. ~~The fragmentation layer~~ **done**: `table::fragment`, eleven tests, and
   it is deliberately **not** under `src/tox` - nothing in it knows what Tox is,
   the MTU and the sender are parameters, so its tests run on a machine with no
   C toolchain. The bounds are the point: the claimed fragment count is refused
   against a `MAX_FRAGMENTS` **derived from `HAND_ABORT_MAX`** before a byte is
   allocated, part-built messages per sender are capped and the **oldest** is
   dropped rather than the newest refused (refusing the newest lets a sender
   that once filled the table wedge itself out of it), a stalled stream is
   swept on a timer, a duplicate fragment is free and a *rewritten* one costs
   the whole message.
2. ~~The NGC group itself~~ **done, and measured end to end.**
   `the_group_carries_a_packet` runs the whole route between two instances and
   it took **ten seconds**: both add each other from public keys with
   `tox_friend_add_norequest` - no request sent, nothing to accept - the friend
   connection comes up, the founder calls `tox_group_invite_friend`, the joiner
   answers its `group_invite` callback with `tox_group_invite_accept`, and a
   custom **lossless** packet crosses. It is `#[ignore]`d because it needs a
   network and tens of seconds:

   ```text
   cargo test --features tox -- --ignored the_group_carries_a_packet
   ```

   That is the first end-to-end evidence D-019 can have, and it is evidence for
   the specific thing the decision rests on: **the group was created `PRIVATE`
   and nobody searched the DHT for it.** The decaying announce path measured on
   2026-08-27 is not on this route at all.

   Five more tests run offline in a millisecond: an instance starts with its own
   identity, a group has a stable non-zero `chat_id`, a packet over the MTU is
   refused rather than truncated, and two instances add each other with no
   request passing between them.
3. ~~The Tox public key and `chat_id` in the advertisement and the join
   request~~ **done.** Three fields, all `Option`, all append-only:
   `LOBBY_TABLE_AD` `n(30) founder_tox_key` and `n(31) tox_chat_id`, and
   `JOIN_REQUEST` `n(9) tox_key`. Absent means *not on Tox*, which a build
   without the feature is, and which has to stay distinguishable from
   thirty-two zero bytes - a decoder that conflated them would hand a founder a
   key belonging to nobody and then wait for it.

   **Both advert fields are outside `table_params_hash`,** beside
   `founder_peer_id` and for its reason: identity and routing. Hashing them
   would have been `table_name`'s bug with a different field in it - a founder
   that restarts comes back with a different Tox identity and a different
   group, which is legitimate, and §7.2 rule 7 would see changed parameters and
   mark the table permanently unjoinable at every client holding it. The
   exclusion test carries both now, so it cannot be undone quietly.

   **The `chat_id` is not how anybody joins**, and that is the point of
   publishing it. Members arrive by invitation because joining by chat id goes
   through Tox's DHT and that path decays; what the published id is *for* is
   the comparison - an invitation says nothing about which group it is for, so
   without it a founder could put the table on a group nobody advertised and
   nothing would look wrong. `TableAd::on_tox` fills both after the group
   exists, which is possible only because neither is hashed.

   What is owed to `PROTOCOL.md`: the three field indices and the sentence that
   they are excluded from §3.1's digest. The decision is D-019's and is made;
   the numbering is the document's to record.
4. ~~A `TableTransport` implementation over it~~ **done, the whole stack is
   proven end to end, and both directions of the group check are measured.** `tox::table` owns the instance on a dedicated thread -
   `tox_iterate` wants a steady loop on one thread, which is the same
   arrangement `ChannelTransport` has with the swarm and for the same reason -
   and the client reaches it through channels. Fragmentation lives in the
   driver: a caller hands over a whole message and nothing above the module
   ever sees a fragment.

   Measured: `a_whole_message_crosses_the_group_in_one_piece` hands 9 000 bytes
   - a `SHUFFLE_STEP` - to `broadcast` at one end and takes them whole out of
   `next` at the other, seven fragments across a private group, in **11.8 s**.

   `FromTable::claimed` is left `None`, deliberately. A group peer id resolves
   to a **Tox** key, which is not a player's signing key and is not evidence
   about one; who signed is settled after reassembly against the ratified
   roster. Three other transport-only decisions all read from that roster: who
   to add as a friend, when to invite, and - **only from the founder the
   advertisement named** - when to accept one. A friend on the roster who is
   not the founder has no business inviting anybody, and the driver refuses it.

   Two smaller things worth keeping: a full inbox **drops** rather than blocks,
   because blocking there would stop `tox_iterate` and a transport that stalls
   the network to wait for its own reader loses the connection as well as the
   message; and the driver sends one message at a time while its fragments are
   being accepted, because pushing the next onto a refusal would interleave two
   half-sent ones.

   **The `chat_id` comparison is implemented and measured in both directions**,
   which closes a gap between what was written down and what ran: the field was
   documented as the thing a joiner checks and for one commit nothing checked
   it. Tox will not say which group an invitation is for until it is accepted,
   so the driver accepts, reads the id back, and **leaves at once** on a
   mismatch. Measured: the right id carries 9 000 bytes in 12.8 s; a wrong one
   is left, nothing arrives in sixty seconds, and the driver's own `chat_id()`
   stays `None`. The founder's id comes out through a `watch` channel, because
   the group does not exist when `spawn` returns and `TableAd::on_tox` needs it.

   **And the Tox identity is persisted**, in a third profile file beside the
   network identity and the player key — three identities, three secrets, for
   §20's reason. A Tox key that changed on every start would take the table with
   it: both ends add each other from keys carried in the advert and the join
   request, a friendship is two-sided, and a founder that restarted would come
   back as a stranger every seated player is waiting for and none of them can
   reach.
5. ~~The measurement across two networks~~ **done, and it passed** - see
   above. `tools/two-network-tox.ps1` runs both ends and reports it.
6. ~~The node list has to update itself~~ **done**: `tox::nodes`, seven tests.
   A bundled list for the first run and for an offline one, a cache refreshed
   from `https://nodes.tox.chat/json` once a day, written through a temporary
   and renamed so an interrupted write leaves the old one. Measured: 20 nodes
   fetched, 45 relay entries, and the two-network run still passes with it -
   2.7 MB each way in 120 s.

   **And one rule that is this client's own: the fetched list is added to the
   bundled one and never replaces it.** A node list decides who a client
   bootstraps from, so whoever serves it can choose a client's whole view of the
   network; a client that swapped its list could be moved onto an attacker's DHT
   by one bad response, without anything looking wrong. The union costs a few
   duplicate entries and removes that outcome - a hostile list can add nodes and
   cannot take away the ones compiled in, and reaching one honest node is
   enough. qTox replaces. The difference is one line of code and a failure mode.
   The same reasoning makes `(host, port, key)` the identity, so a list cannot
   quietly re-key a bundled entry.

   Getting there cost two wrong turns in one dependency, both recorded in
   `Cargo.toml` because the second looks like the fix for the first:
   `tls-rustls` takes rustls's default provider (`aws-lc-rs`) while libp2p
   brings `ring`, and two providers make rustls refuse to choose *at run time*;
   `tls-rustls-webpki-roots-ring` is broken upstream in attohttpc 0.30.1 - it
   sets `rustls/ring` but not `__rustls`, which is what `src/tls/mod.rs` gates
   the implementation on, so it builds with TLS compiled out and every request
   answers "TLS is disabled".

The vendored trees are **fetched at pinned commits by `tools/build-tox.ps1`,
not committed** — 12.7 MB for a transport whose whole point is a measurement
that has not been taken. `vendor/ziffle` is committed, so this is the exception;
the script carries the argument and says what would change the answer.

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
* **`HAND_ABORT` causes 2 and 3 are done** - see above. What is left of the
  cause register is `4` (the §6.3 divergence terminus) and `6` (D-014's
  anti-cheat void), and both need machinery that does not exist yet rather than
  evidence handling that does.

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
  cannot be two different hands.

  **And the instrument had a hole exactly where it would have mattered.** It
  printed two `final_stacks` vectors and two pot counts, which is two of the
  six fields a receiver compares. A hand disagreeing only about `deltas`,
  `busted` or `state_hash` printed two **identical** vectors under the word
  "disagreement" - which reads as a broken instrument rather than as a
  difference somewhere the instrument does not look, and `state_hash` is the
  one field that can differ while all five others agree, because checkpoint 8
  carries the board, the button, the transcript head and `signed_this_hand`
  and the settlement repeats none of them. Two engines that agree about the
  money and disagree about the hand look exactly like that.

  `HandComplete::disagreement` now names the field and prints both sides, and
  it **destructures `Self`**, so a seventh field is a compile error in the
  report rather than a line that silently stops covering it. A test would only
  have checked the fields somebody remembered to list, which is the same memory
  that would have failed.

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
* **Two machines on two networks — done, and it works.** This was the last
  coverage hole worth the name: everything else ever measured was three
  processes on one host, which is the one environment where NAT, the relay and
  forwarding have almost no way to show themselves.

  The far end is `user@172.16.0.20`, a machine on a VLAN behind the router,
  reached over SSH. `tools/two-network-ssh.ps1` runs a host node here and a
  joining node there, **with `--no-mdns` at both ends**, and compares the two
  logs. Measured, 120 s:

  | | here | there |
  |---|---|---|
  | poker peers met | 1 | 1 |
  | table formed | yes | yes |
  | hands | 1 | 1 |
  | relay reservations | 1 | 1 |
  | direct connections | — | 8 |

  Session `888f295d` at both ends and one hand at genesis `55f97eeb` at both
  ends. Equal hand counts would only say the two played; **equal genesis says
  they played the same hand**, which is why the script compares hashes and not
  counts.

  So the DHT lobby, the relay reservation and cross-subnet discovery have now
  all run. With multicast taken away there is nothing else that could have done
  the finding.

  **What it still does not prove: hole punching.** The route to the far end
  goes through `vEthernet (lan)` — a Hyper-V switch onto the LAN — and the far
  address is private. Two subnets with a router between them is a real
  boundary and not a NAT to traverse. The script now says so at the top of
  every run against a private address, next to the tun/tap warning it already
  had - that caveat is the first thing lost when a result is repeated to
  somebody else. A test that traverses a NAT needs an endpoint outside this
  building.

  **And one number worth keeping:** `TABLE FORMED` was the *last* line of a
  120 s run at both ends. On one machine a table forms in seconds. Across the
  boundary it took nearly the whole window, and the whole of that delay is
  before the table exists — the DHT lobby has to publish and the far end has to
  find it. Every error line in either log is a failed dial to an unrelated
  public IPFS bootstrap node, so the delay is not failure and retry inside this
  protocol; it is provider-record propagation. Anybody measuring across
  networks should budget for it rather than reading a short run as "they never
  met".

  Packaging this found four defects, all in the script and none in the
  protocol, recorded in ee4b269 because each is the kind that reads as a
  protocol failure: reachability probed by ping against a host that drops ping
  and permits 22; the far node started under `Start-Process
  -RedirectStandardOutput`, which over ssh captures nothing and looks like a
  crash; Windows OpenSSH silently refusing a key on removable media, reported
  as `Permission denied (publickey)`; and `$isWindows`, which PowerShell 7 owns
  as read-only.

  `tools/two-network-test.ps1` — the Hyper-V one — never could have
  substituted: its own notes say both endpoints share one external address, so
  nothing needed punching there either.
