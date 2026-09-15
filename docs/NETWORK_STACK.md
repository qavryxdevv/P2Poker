# NETWORK_STACK.md

Specification of the transport and discovery layer of `p2p-poker`.

> ## Discovery: Mainline is history here, as it is in the client
>
> **Until 2026-09-15 this box warned that the discovery layer specified below was
> not the one the client runs.** `56b0b50` (2026-08-30) replaced Mainline DHT
> discovery with a **libp2p Kademlia provider record** and removed the
> `mainline` crate, `src/net/dht.rs` and both infohashes from the build, and this
> document went on specifying Mainline. The survey behind `S1-E` counted **98
> discrete Mainline claims in 27 sections**. The largest blocks — §§2, 3, 3.5, 4,
> 5.7, 5.8, 10.1, 11.2, 11.4 and 12 — were rewritten on 2026-09-02, the three
> claims that were inverted rather than stale first among them: §5.7's
> *"libp2p Kademlia is not enabled in v1"*, §11.4's deny-all request filter, and
> §10.1's *"the application owns the re-announce loop"*. The residue went on
> 2026-09-15: §§0.3, 1.2, 1.3, 2, 2.1, 3, 4.1, 4.6, 5.2, 5.3, 8, 9.5, 9.7, 9.8,
> 11.5.2, 12, 14 and 15. A sentence that still names Mainline, `get_peers`, an
> infohash or a `SocketAddrV4` says in the same breath that it is history.
>
> What the client does: `net::run::lobby_namespace` provides and looks up
> `sha2-256("p2p-poker/main-lobby/v1")` on the public IPFS Kademlia
> (`/ipfs/kad/1.0.0`), and `relay_namespace` does the same for
> `sha2-256("/libp2p/relay")`, both keyed the way go-libp2p's routing discovery
> keys a namespace (§3.2). A provider record carries whatever multiaddrs a node
> has — circuit addresses included — which is why the change was made: a
> Mainline announcement can say one `IP:port`, and a player behind a NAT has
> none worth saying.
>
> **`SPEC_CS.md` still names Mainline, and it stays that way on the owner's word
> (2026-09-15).** It is the owner's original assignment and is kept unamended, as
> history. Where it says *Mainline DHT*, `LOBBY_INFOHASH`, `announce_peer()` or
> `get_peers()` it names the mechanism the first implementation used; the
> requirements those sentences carry — one fixed rendezvous every client can
> recompute, a peer list that is a hint and never an identity, no central lobby
> server, the disclosure told to the player, no game state on the DHT — bind the
> provider record unchanged. The passages here that quote it (§1.2, §2, §4.1 and
> §8) keep its words and say so. What `S1-E` still holds open is §3.5's three
> unmeasured figures.

**Status:** Phase 0 output, binding for Phase 7 (libp2p transport) and Phase 8
(discovery + GossipSub lobby). **The status line said "No implementation exists
yet" until 2026-08-31**; the transport is implemented, and two-network runs are
measured in `NEXT.md`.

**Authority order.** `docs/SPEC_CS.md` is the specification and wins over
everything here — read as the owner fixed on 2026-09-15: it is the original
assignment, kept unamended, and where it names the Mainline DHT the requirement
wins and the mechanism is history (the note above). `docs/DECISIONS.md`
(D-001 … D-013) is binding owner decision and outranks the research documents
and any preference of this document. **D-007
corrects D-006, and D-008 generalises D-007**; both win over D-006. An action
deadline is advisory and a fold-effect timeout certificate is forbidden whenever
the required voter set `V` (`PROTOCOL.md` §8.3) has fewer than two members. That
is always the case at two seats and is reachable at any seat count, which is why
every such rule is scoped on `|V|` and **never on `n`** (§8.4, §15).
**D-010, D-011, D-012, D-013 and D-014 bind this document and this layer**, which
earlier revisions denied by omission four times over: the first carried no
mention of D-010 and ten places where the transport removed a peer; the second
carried no mention of D-012 while this layer is the corpus's largest producer of
per-receiver quantities; the third carried no mention of D-013 while holding
the most tempting wrong answer to the question D-013 asks — connection state as a
liveness signal; and the fourth carried no mention of **D-014** while stating, in
two places, the every-layer form of the rule D-014 narrows, so that the document
did not merely lag the decision, it **asserted its negation** and survived three
further passes that each named it by line number. **Four for four**, and each time
on the judgement that transport was unaffected. That judgement is not to be made
again: this document is in every sweep, and its coverage is checked by counting the
decision's occurrences here, not by asserting that the sweep ran — **and a count of
zero must be read for which of the two it is**, a gap or a contradiction, because
the second is the more urgent and looks identical in the count. §0 records what all
six of D-009…D-014 change here, §0.5 is the D-012 pass, §0.5.7 is D-013's, §0.6 is
D-014's, and §11.5 is where the boundary between a *protocol* verdict and a
*transport* defence is drawn.
Under **D-011 rule 1** this document is the normative owner of **transport,
discovery and connectivity** and of nothing else; where it needs a wire
definition it names the owning section of `PROTOCOL.md` and does not reproduce
it. The research
notes under `docs/research/` are the evidence base:
`LIBP2P.md`, `MAINLINE_DHT.md`, `NAT_AND_DISCOVERY.md` are the three this document
is built on — `MAINLINE_DHT.md` now as the record of the mechanism `56b0b50`
replaced, and of the one first-hand measurement of DHT surveillance this project
has; `MENTAL_POKER.md` is used only for measured per-hand byte counts.
All seven research documents exist — nothing was missing and nothing here is
invented to paper over a gap.

**Evidence rules.** Every load-bearing claim carries a `Verification:` line with
one of:

* **[COMPILED]** — code using it was compiled, and where stated executed, on this
  machine (`rustc 1.95.0`, `cargo 1.95.0`, `x86_64-pc-windows-msvc`). The probe
  written for *this* document is
  `…/scratchpad/probe-netstack/` and it builds and runs the entire behaviour stack
  specified in §5–§11.
* **[SOURCE]** — read in the unpacked crate under
  `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/<crate>-<version>/`.
* **[MEASURED]** — a live network measurement recorded in a research document,
  with the section reference.
* **[RESEARCH]** — carried over from a research document, which carries its own
  verification line.

docs.rs and recollection are leads, never evidence. Where something is unknown it
is written as **OPEN QUESTION** and carried to §13 rather than guessed
(`SPEC_CS.md` §36).

---

## 0. D-009 to D-014 as they bind this layer

This section exists because this document has now **four times** been left out of
a decision sweep on the judgement that transport was unaffected, and all four
times that judgement was wrong.

The first time was D-010. **Eviction is a transport action**, so a decision that
forbids automated eviction lands here whether or not the decision mentions
transport; this document had zero mentions of D-010 and ten places where a peer
was removed, disconnected, refused or block-listed, and D-011 recorded that as a
blocker. The rule is §0.1, the boundary that keeps the transport's own defences
intact is §0.2, D-011 rule 1's ownership consequence is §0.3, and the
site-by-site disposition of all ten eviction sites is §0.4.

The second time was D-012, and the omission was worse reasoned than the first.
D-012 says no canonical state may be derived from a quantity that can differ
between honest receivers — and **this layer is where almost every such quantity
in the system is produced**: who is connected, in what order bytes arrived, what
a timer says, what AutoNAT concluded, whether a link is relayed, how many peers
answered. A rule about which quantities may become canonical reaches the layer
that manufactures the disqualified ones first. §0.5 is that pass. It also
carries the one site where the omission had already cost something.

The third time was D-013, and it is the one that should have been predictable
from the other two. D-013 replaces a question about a seat's *status* with a
question about its *participation*, and participation is a liveness question —
which is the one kind of question this layer is always able to answer cheaply and
always wrong to answer canonically. A decision that redefines who must speak next
reaches the layer that knows who is currently reachable. §0.5.7 is that pass, and
it also carries the disposition of §0.5.6, which D-013's J1 rule closed.

The fourth time was D-014, and it is the first of the four where the omission was
not silence but **contradiction**, which is why it survived three further passes
that each named it and none of which opened this file. D-014 narrows D-010
point 3 — and §0.1 and §1.2 prohibition 7 state D-010 point 3 in the
**corpus-wide, every-layer** form D-011 rule 3 gave it. A document that carries
that form and **zero** occurrences of `D-014` is therefore not merely behind the
sweep: it asserts the negation of a binding decision, and an implementer reading
it alone would refuse to build a thing `STATE_MACHINE.md` T64 requires. §0.6 is
that pass. Its result is a **re-scoping and not a weakening**: everything this
layer was forbidden to do it is still forbidden to do, without exception and
without a proof it may consult, and D-014's one exception lives entirely above
this layer and reaches a seat rather than a socket.

D-009's three rules are honoured in §1.3.2(iii), §5.1 and §5.1.1 and are
registered in §15; nothing in this pass changed them.

### 0.1 The rule, stated once

> **D-010 point 3, as D-011 rule 3 extends it to every layer, and as D-014
> narrows it above this one.** No automated eviction **at this layer**, ever: no
> `block_peer`, no disconnect, no dial refusal, no persisted mark, no unseating,
> and no allow-list or block-list populated by the poker protocol. An
> `EquivocationProof` (`PROTOCOL.md` §5.2), a timeout certificate
> (`PROTOCOL.md` §8.3), a signed abort attribution, an invalid application
> signature — **none of them causes this layer to disconnect, refuse, block or
> unseat anybody.** A proof is evidence for a human. The user may always choose
> not to play with someone; the protocol may not choose for them.
>
> **D-015 makes two of the four objects named above objects that never arrive, and
> the prohibition is kept in full anyway.** `TIMEOUT_VOTE`, `TIMEOUT_CERT` and
> `EquivocationProof` are defined on the wire and **not produced in version 1**
> (`PROTOCOL.md`'s header box, §4.8, §5.2.4), so no certificate and no equivocation
> proof reaches this layer to be ignored. **The sweep of this document for anything
> that *assumes the machinery exists* found no such assumption**: every occurrence of
> the two objects here is a **prohibition** — a list of things that must not cause a
> `block_peer`, a disconnect, a dial refusal, a persisted mark or a list entry — and a
> prohibition over a set that has shrunk is still binding over what remains. Nothing
> at this layer *waits for*, *counts*, *forwards specially*, *sizes a buffer for* or
> *derives a tier from* either object, which is what a real dependency would have
> looked like. **The two names stay in the box on purpose**: a later version may
> reinstate the producer, and this layer's answer to it must already be written down
> when it does — deleting the names now and re-deriving them then is exactly how
> `block_peer` survived D-010 in two places here for a whole pass.
>
> **The one exception is not this layer's, and it takes nothing away from this
> box (D-014).** A **tier-1** finding — an event **signed by the accused**, whose
> illegality any peer decides alone from that event's own bytes — removes its
> sender **from the table**, and from nothing else. That is a seat disposition
> (`STATE_MACHINE.md` T64, T65, I34), reaching canonical state through
> `HAND_INIT(k+1)`'s collective body, and it produces **no transport action of
> any kind**. Every one of the artefacts named in the paragraph above stays
> forbidden as an input to anything here, including the invalid application
> signature that is a tier-1 trigger by name above this layer. §0.6 states the
> exception in full, and states why the word *anywhere* had to go.

The reason is D-010's, and it is worth repeating where an implementer will meet
it: automated eviction was the *second prize* that made four rounds of attacks
worth mounting. Every one of the severe findings ended with an honest peer's key
on a block list for doing what the protocol required. Remove the prize and the
attack class evaporates — the worst an adversary achieves is a wasted hand, which
is the same outcome as a flaky connection, which this layer must survive anyway.

### 0.2 What this does **not** remove, and the boundary that matters

Read alone, §0.1 sounds like the transport has been disarmed. It has not, and an
implementer who concludes that will build something that falls over to the first
flood. The distinction is **what supplies the reason**, not what the action looks
like:

| Reason for the action | Status | Where |
|---|---|---|
| A protocol proof, verdict or attribution — equivocation, a timeout certificate, an abort record, an invalid application signature | **forbidden.** No transport consequence of any kind, and D-014 does not change this row: its tier-1 exception costs a **seat**, never a socket | §0.1, §0.6, §6.6, §7.4, §11.5 |
| A **resource** fact — connection count, pending-dial count, process memory share | **kept.** Refuse the connection | §11.1 |
| A **rate** fact — this socket is spending more of our budget than we allot it | **kept.** `Ignore` the excess, then stop dialling, then disconnect | §6.6 |
| A **size or malformation** fact — over cap, non-canonical, unparseable | **kept.** Drop the message | §6.4, §11.3 |
| **Admission** — a stranger asking us to spend our own bandwidth relaying them | **kept.** Refuse the reservation (D-002) | §9.6 |
| **Membership** — a table stream from a peer the signed roster does not list | **kept.** Close the stream | §8.4 |
| **Capacity** — a relayed circuit whose advertised `Limit` cannot carry a hand | **kept.** Do not seat; say so honestly | §9.5 |
| **The user's own explicit choice** not to play with, or connect to, a named identity | **kept.** This is the only thing that may ever populate a block list | §11.5 |

The line is: **the protocol may not choose for the user, but the transport may
still defend itself against a flood.** Refusing to spend our memory, our sockets,
our bandwidth or our relay slots on somebody is a decision about *our own
resources* and needs no proof about anybody's honesty. Removing a player from a
game because we believe they cheated is a verdict, and this layer does not issue
verdicts (§1.2 prohibition 3 and prohibition 7).

Two corollaries an implementer must not blur:

* **Refusal is not eviction.** D-002's relay admission control (§9.6) refuses a
  stranger a reservation. That is not driven by any proof and takes nothing away
  from anybody — the stranger is exactly as able to play as before, they simply
  do not get to spend this user's uplink. It stays, unchanged, and D-011 does not
  reach it.
* **A disconnect on rate is not a penalty.** §6.6 stops dialling and then
  disconnects a peer that keeps exceeding its lobby budget. That peer may
  reconnect, is not recorded anywhere, and carries no mark. It is backpressure,
  and it must never be described, logged or displayed as a sanction.
* **A D-014 removal is not a transport action, and the table's row is not in
  this table.** The one automated removal the corpus permits takes a **seat** and
  nothing else: no socket is closed, no dial refused, no reservation withdrawn
  and no name persisted because of it, and a removed player's connections are
  handled exactly as anyone else's until the user chooses otherwise. It is
  therefore neither a "forbidden" row nor a "kept" row above — it is not this
  layer's action at all (§0.6).

### 0.3 D-011 rule 1 as it applies here

`PROTOCOL.md` owns the wire: message shapes, the event envelope, the chain,
sequence numbers, the anti-replay slot key, canonical bytes, framing, and what a
receiver validates. This document **references those by section number and
reproduces none of them.** Where a revision restated one, the restatement is
deleted and replaced with a pointer, on the D-011 finding that a copy is what
drifts. Seven sites have now been through that treatment:

| Restatement | Was in | Now points at |
|---|---|---|
| The anti-replay slot key | §1.3.2(iii) | `PROTOCOL.md` §5.2 |
| The unchained sentinel envelope | §6.4 | `PROTOCOL.md` §2.3 |
| Every two-sided size constant | §6.5, §11.3, §14 | `PROTOCOL.md` §13 |
| The lobby receiver's validation checklist | §6.4 | `PROTOCOL.md` §7.2 |
| The snapshot request and response bodies | §7.3 | `PROTOCOL.md` §7.5 |
| The table-stream framing and its sizing argument | §8.4 | `PROTOCOL.md` §2, §13 |
| The lobby freshness, skew and monotonicity rules | §10.3 | `PROTOCOL.md` §7.2 |

The last four were found by the D-012 sweep rather than the D-011 one, which is
the process point D-012 makes: the cheap check that occasionally finds nothing
found four more copies here, and one of them — §7.3's invented
`SnapshotResponse` — had **already drifted into disagreement** with the owner,
carrying a field the wire does not have and missing two it does. That is the
D-011 failure mode caught in the act, in a document the sixth pass had recorded
as free of disagreements.

What this document does own, and what no other document may restate: the
transport and behaviour configuration of §5, the discovery mechanisms of §3, §4
and §10.1, the connectivity mechanisms of §9 including the D-002 relay `Config`
of §9.6, and the resource limits of §11. One pair of values is deliberately written
twice and the reason is argued where it stands: the two derived rendezvous keys of
§3.2, which `PROTOCOL.md` §13 publishes as `LOBBY_NAMESPACE_KEY` and
`RELAY_NAMESPACE_KEY` (the two infohashes they replaced left with `56b0b50`).

### 0.4 The sweep: all ten sites and what happened to each

Kept for the next reader, because the useful thing about a fix of this shape is
being able to check that none of it was missed.

| # | Site | What it said | Disposition |
|---|---|---|---|
| 1 | §1.1 stack diagram | `net/swarm.rs` listed a bare "blocklist" among its behaviours | **Renamed** to "user block list (§11.5)". The mechanism stays; the label now says who may fill it |
| 2 | §1.2 | prohibition list stopped at six and said nothing about eviction | **Added** prohibition 7: no removal, block, unseat, refusal or penalty on the strength of a protocol proof |
| 3 | §4.6 hostile-input table | a non-poker libp2p node is disconnected and "never admitted to the relay allow-list" | **Kept, reclassified.** Both are decisions about our own sockets and uplink, driven by an `identify` protocol name and never by a proof (§0.2). Wording now says so |
| 4 | §5.1 | `libp2p-allow-block-list` pinned as a non-optional dependency | **Kept**, with the point made explicit that the crate is unavoidable in the link, so what a decision can constrain is what *populates* it |
| 5 | §5.1.1 dependency register | purpose column read "resource limits and blocklist" | **Reworded** to "resource limits, and the user-populated block list of §11.5 — never populated by a protocol proof" |
| 6 | §5.5 behaviour struct | field `blocklist:` | **Renamed** to `user_blocklist`, with the single-caller rule in a comment. The name is the constraint |
| 7 | §6.6 rate limits | a proven violation — invalid signature or `EquivocationProof` — "goes further and lands the peer in `BlockedPeers` via `block_peer`" | **Deleted.** A proven violation now produces no transport action at all; an invalidly signed message is dropped by §6.4 as a malformed message and nothing follows |
| 8 | §6.6 rate limits | repeated budget overrun "downgrades the peer locally: stop dialling it, then disconnect" | **Kept, reclassified** as backpressure: not persisted, not a mark, reversible on reconnect, and never to be presented as a sanction |
| 9 | §7.4 conflict resolution | the founder contradiction "does not feed `block_peer` (§11.5)" | **Rewritten.** The negative reference is gone with the thing it referenced; the rule now states its whole effect positively — we decline to display one self-contradictory table, and nothing happens to the founder |
| 10 | §11.5 "Blocklist" | the section itself: `block_peer` on a proven protocol violation, persisted in the profile | **Replaced** by §11.5.1 (forbidden), §11.5.2 (the resource defences that stay) and §11.5.3 (the user's own block list, the only caller of `block_peer`) |

Two things were checked and deliberately left alone, because neither is
protocol-proof driven and both would be missed if they went: **D-002 relay
admission** (§9.6 — refusing a stranger a reservation is not eviction, and
deleting it makes the client an open relay) and the **membership gate** (§8.4 —
closing a stream from a peer the signed roster does not list is admission, not
the removal of a seated player).

### 0.5 D-012 at the transport layer

#### 0.5.1 The rule, and why it lands hardest here

> **No canonical state — nothing that enters a state hash, a roster hash, a
> chained event body, or the next hand's genesis — may be derived from a quantity
> that can differ between honest receivers.** Canonical state changes only
> through a chained event every participant accepted. Everything else is a
> **local view**.

Read from this layer, D-012 is not a rule about hashes. It is a rule about
*this document's outputs*, because nearly every quantity it disqualifies is
manufactured here. The transport's entire product is per-receiver by
construction: which peers happen to be connected to *us*, the order bytes
happened to arrive at *us*, what *our* clock reads, what AutoNAT concluded about
*our* address, whether *our* link to a peer is relayed, how many peers answered
*our* snapshot request, which copy of a re-broadcast advert *we* hold. Two
honest clients differ on every one of those at almost every instant, and that is
correct behaviour, not a fault.

So the transport is not asked to make any of them agree. It is asked to make
sure none of them is ever read as though it did. §1.2 prohibitions 3, 4 and 5
already forbid the three cases that were foreseen — a verdict about a player,
a reordering, a `PeerId` treated as an authorisation. D-012 is the general form,
and it is added as prohibition 8.

#### 0.5.2 The catalogue: every per-receiver quantity this layer produces

The point of a list rather than a principle is that the next reader can check a
new mechanism against it. Each row states where the quantity is allowed to go,
and every row's answer to "may it enter a hash, a roster, a chained body or a
genesis" is **no**.

| Per-receiver quantity | Produced in | Permitted destinations |
|---|---|---|
| Connection up/down/relayed for a peer | §1.3.1 `ConnectionState`, `TransportEvent` | the GUI (§2.2); a *hint* the layers above may consider (§1.2 rule 3, §8.4). Never a seat's status, never an input to a signed event |
| Arrival order of bytes on a stream | §1.3.1 (per-peer FIFO and nothing more) | nothing. Ordering is the hash chain's (§1.2 prohibition 4) |
| The local clock, and every TTL and skew test built on it | §6.4, §10.1, §10.3 | the local lobby view only, and only for **unchained** lobby traffic (§0.5.4) |
| AutoNAT v2's reachability verdict | §9.2, §9.3 | our own decision to volunteer as a relay (§9.6), the GUI (§2.2), and the self-reported `reachability` hint of `PROTOCOL.md` §7.4, which that section already declares worthless as a claim and never an input to a game decision |
| Relay reservation and circuit status, and a circuit's advertised `Limit` | §9.5 | our own dialling and our own decision whether to *ask* for a seat (§0.5.5). Never another peer's seat |
| Number of snapshot responders, and multiplicity of a DHT candidate across responders | §4.6, §7.2 | dial ordering and local sync-completion only. §7.4 already states the rule that matters: *counting is never evidence* |
| The merged lobby view — which table ads and which copy of each we hold | §7.4, §10.3 | display, and choosing a table to try to join. **This row carried the corpus's one live exception until the Phase 3 gate; it is closed (§0.5.6), and the row now has the same unqualified answer as every other** |
| A peer's D-002 relay tier (A/B) | §9.6 | our own relay admission. Never leaves this client, never gossiped, never persisted |
| A table marked `EQUIVOCATION` by the founder-contradiction rule | §7.4 rule 4 | this client declining to display one table as joinable. Never gossiped, never persisted, never hashed, and explicitly not an `EquivocationProof` |
| The user's own block list | §11.5.3 | this client's own dialling. Local by construction, and D-011 rule 3 forbids sharing it |

#### 0.5.3 The one transport quantity that *is* in a chained body, and why it is safe

`JOIN_REQUEST`'s `n(2) peer_id` and `JOIN_ACCEPT`'s `SeatEntry.peer_id`
(`PROTOCOL.md` §4.3) carry a libp2p `PeerId`, which this layer produced. That is
not a D-012 violation, and the reason is worth stating so that nobody "fixes" it:

* it is **self-declared in a signed event**, not measured by the receiver — the
  joiner names its own `PeerId` and signs it, so every participant reads one
  agreed value out of one signed body rather than each measuring its own;
* the receiver's own check (`peer_id` matches this connection) is a **local
  admission test on a local socket**, and its outcome enters nothing;
* decisively, `peer_id` is **not a component of `roster_hash(k)`**
  (`PROTOCOL.md` §3.1) and not of `GENESIS(k)`. It is a dial hint carried inside
  the roster, not part of what the roster hashes.

**Which is also the standing rule: no `PeerId`, connection state, address,
reachability class or relay status may ever be proposed as a component of
`roster_hash`, `GENESIS`, a `stage_hash` or any other protocol hash.**
`PROTOCOL.md` §3.1 says there is no third case beside "chained event" and "local
view"; every quantity in §0.5.2's table is the second kind, and an implementer
who finds one being hashed has found a bug, not an optimisation.

#### 0.5.4 Clock-derived acceptance is confined to unchained lobby traffic

The lobby applies local-clock tests — a skew allowance on `timestamp`, an upper
bound on `expires_at`, and relative-freshness expiry (`PROTOCOL.md` §7.2, §7.4;
§6.4 and §10.3 here). Two honest receivers with different clocks can accept and
reject differently under those tests, which is exactly why they are confined to
traffic that is **unchained** (`chain_scope = 0`, §6.4) and to a view that is
purely local.

**No clock-derived test may be applied to table-stream traffic.** The table
stream's admission is a `u32` length prefix and a canonical decode
(`PROTOCOL.md` §2), and its validity is the signature and the chain. If the
transport ever refused a chained event because it looked early or late by the
local clock, two honest receivers would hold different chains — the H1 failure
in a new place, and total rather than partial, because neither would verify a
single one of the other's events afterwards.

#### 0.5.5 Relay capacity may gate our own request for a seat, never anyone else's seat

§9.5 requires a client to read the `Limit` a relay actually returns, and says a
circuit that cannot carry a hand must not be used to seat a player. Under D-012
that sentence needs a subject, because the `Limit` on *our* circuit is a
per-receiver quantity and seating is canonical.

**It is this client declining to ask for a seat.** A client whose only route to a
table is a circuit that cannot carry a hand does not send `JOIN_REQUEST`, and
says so honestly in the GUI, rather than joining a hand it will drop out of
mid-street. That decision is about our own connectivity, is taken before any
chained event exists, and forks nothing.

**It is never a participant withholding a signed event from somebody else.** No
peer may refuse, delay or condition a `JOIN_ACCEPT`, a `PLAYER_LIST`, a
`TABLE_READY` or any other chained event on what its own circuit to the joiner
advertises. Seating is decided by signed events every participant accepts
(§8.4), and a seated participant whose circuit degrades is reported as a
disconnect and **remains seated** — a rule §8.4 already states and which D-012
now also requires, since the alternative is a roster that differs by who is
behind which relay.

#### 0.5.6 The site where a local view reached canonical state, and how it was closed

**Status: closed.** `PROTOCOL.md` acted on this in the Phase 3 gate, under
**D-013**'s J1 rule. The problem statement below is kept as the record of the
finding, in the past tense; the disposition follows it, and it is **neither of
the two remedies this section proposed**.

**What was wrong.** `GENESIS(0)` and `session_id` both contained `advert_hash`,
which was the `event_hash` of the `LOBBY_TABLE_AD` the participants joined under,
and which each joiner named for itself in `JOIN_REQUEST`. A joiner took that value
**out of its local lobby view**, which is this document's §7.4 and §10.3: the union
of what several snapshot peers happened to return, plus live gossip, keeping the
validly signed ad with the highest `timestamp` per `table_id`.

That view is per-receiver, and there it was not merely per-receiver in principle:
the founder re-broadcasts every `AD_REBROADCAST_MS`, and `PROTOCOL.md` §7.2's own
rule 6 requires each re-broadcast to carry a strictly greater `timestamp_unix_ms`
or be discarded. So each re-broadcast was a **different signed event with a
different `event_hash`**, and which one a joiner held was decided by nothing but
when it happened to be listening. Two honest players joining a minute apart
therefore named two different `advert_hash` values, both validly signed by the
founder, both unexpired, and both accepted. `JOIN_ACCEPT` echoing `advert_event`
verbatim did not converge them: it echoes back *the joiner's own* copy, which is
what made the joiner's local view canonical rather than replacing it.

Downstream, `advert_hash` was a component of `GENESIS(0)` and of `session_id`, and
`session_id` is a component of every `GENESIS(k)`. Peers holding different copies
therefore derived different `GENESIS(0)`, so no `TABLE_READY` verified against any
other's, the collective stage never completed, and the table never started —
silently, and for a reason no participant could see. It was H1's shape exactly:
agreed content, one per-receiver field, total divergence. `THREAT_MODEL.md`
catalogues it as **X35**.

**The disposition, and it is `PROTOCOL.md` §3.1's.** `advert_hash` is **removed
from `GENESIS(0)`, from `session_id` and transitively from `ctx`**, and replaced by
`table_params_hash` — a hash over the agreed table parameters and nothing else,
specified in a normative box in `PROTOCOL.md` §3.1 which this document references
by number and reproduces no part of (D-011 rule 1, D-011 rule 2). The two fields
that made the old value per-receiver, `timestamp_unix_ms` and `expires_at_unix_ms`,
are excluded from it **by name**. Every joiner therefore derives the identical
value from whichever re-broadcast reached it, because the parameters are what it
agreed to and the timestamp is not.

**`advert_hash` did not become normative elsewhere; it became a lobby-layer
pointer, which is this layer's business.** `PROTOCOL.md` §2.8's domain register
now records that it names which advertisement a joiner is answering and enters no
chained hash at all. That is precisely the destination §0.5.2's merged-lobby-view
row permits, so the row is now clean rather than carrying an exception.

**Neither remedy this section proposed was adopted, and recording that is the
point of keeping this paragraph.** It proposed either *"the founder's
`PLAYER_LIST` value made normative and `TABLE_READY` required to equal it"* or
*"`advert_hash` dropped from `GENESIS(0)` in favour of `table_public_key`"*. The
first pins one copy but pins the *timestamp* with it, so the table's identity
still depends on when the founder last re-broadcast, and it makes the founder's
view canonical rather than the agreement. The second binds too little: dropping to
`table_public_key` would have left **nothing** binding the blind schedule, the
starting stack or the timing values into the genesis, which is what J1(b) needed —
the founder could then re-sign with a different `small_blind` and hand two joiners
two rule sets. The adopted fix is strictly better than both because it changes
*what* is hashed rather than *whose copy* is hashed: it keeps everything the
participants agreed to and drops only the quantity that varies. A reader who
thinks this section's remedies were reasonable should notice that the document
that *found* the defect proposed two fixes and the document that *owned* it
adopted neither — which is an argument for the owner boundary, not against it.

**J1(b), which this section did not find, closed with it.** Nothing had forbidden
the founder re-signing with different parameters, and the check that catches it is
a lobby receiver rule and therefore reaches this layer: `PROTOCOL.md` §7.2's
receiver rule 7 discards a re-broadcast whose `table_params_hash` differs from the
one already held for that `table_id`. This document's §7.4 merge and §10.3
freshness rules are downstream of that check and do not restate it; what matters
here is that the merge may **not** be written to prefer the newest advert
unconditionally, because rule 7 makes newness insufficient on its own.

**What this document owed and still owes.** This document's half was and remains
done: the merged lobby view is a local view (§0.5.2, §7.4, §7.5), the transport
asserts nothing canonical about which copy is current, and TTL and re-broadcast
govern display only (§10.3, §10.4). The standing obligation is the one in §0.5.3:
**no quantity from §0.5.2's table may be proposed as a component of any protocol
hash**, and a future field added to `LOBBY_TABLE_AD` that varies per re-broadcast
would re-open this exact defect, because §3.1's box excludes the two time fields by
name rather than by a general rule. If such a field is added, it is this layer's
business to say so, since this layer is where per-broadcast variation is produced.

#### 0.5.7 D-013 at the transport layer, and the one trap it sets here

**D-013 outranks D-012 and this layer is bound by it**, for the third consecutive
decision on which this document's involvement was not obvious in advance. Its rule
is that **liveness is inherited from the chain, not from a seat's status**: a seat
is required to emit in the next hand only if it signed at least one chained event
in the current one. `PROTOCOL.md` §3.2 owns the rule, the set and its notation, and
this document reproduces none of them.

**Why it reaches transport at all, which is the part a sweep would skip.** D-013
replaces a *status* question with a *participation* question, and this layer holds
the single most tempting wrong answer to it. Connection state is exactly a
liveness signal, it is available here, it is cheap, it is usually right, and it is
**per-receiver** — §0.5.2's first row. An implementer who has just read "a seat
that is not participating is skipped" and who has a `ConnectionState` to hand will
be tempted to wire the two together.

> **Connection state may never contribute to the participation record.** Whether
> this client currently holds a connection to a peer, when the connection dropped,
> whether the link is relayed, and what any keep-alive or ping concluded, are local
> views and stay local views. Whether a seat signed a chained event in hand `k` is
> a function of the **accepted chain** and of nothing this layer measures. The two
> agree most of the time, which is what makes the substitution attractive and what
> would make the resulting fork rare, late and hard to reproduce.

This is prohibition 8 in §1.2 applied to a new consumer rather than a new rule,
and it is the same line §8.4 already draws for seating: a seated participant whose
circuit degrades is reported as a disconnect and **remains seated**. D-013 extends
what that sentence protects. Before it, a wrong answer here cost a seat's *status*,
which nothing acted on after D-010. Now the same wrong answer would decide who is
**required to emit in the next hand**, and two clients that disagree about that
derive different `HAND_INIT` bodies — the failure `THREAT_MODEL.md` catalogues as
**X36**, whose payoff is a silent permanent fork rather than a wasted hand. The
prohibition did not change; **what it is now load-bearing for did**, and that is
the check this section exists to record.

**Re-entry is signed, never sensed.** Under D-013 a seat that went quiet returns to
the required set by **signing a chained event**, and by nothing else. A socket that
reconnects, a relay reservation that is re-established, an `identify` exchange that
completes, a peer that reappears in a snapshot — none of these is a re-entry, none
may synthesise, prompt or stand in for one, and this layer must not report one to
the layers above as though it were. What the transport may do on reconnect is
exactly what it does on any dial: deliver bytes. `THREAT_MODEL.md` §7.3(b) carried
the opposite claim — that a reconnecting player rejoins the key-holder set at the
next hand boundary — for four passes, and it is corrected there.

**What D-013 retires here.** §0.5.6's open site, and with it the only exception in
§0.5.2's table. `GENESIS(0)` no longer depends on which copy of a re-broadcast this
client holds, so the merged lobby view is now a local view with no downstream
qualification at all.

**The process rule, and this document is on both sides of it.** D-013 records that
*a finding correctly assigned across an owner boundary is a finding nobody owns*,
and it names this document as the example: §0.5.6 found J1, wrote it up in full,
named two candidate fixes and marked it a blocker for `PROTOCOL.md`, which did not
act for a pass. The other half of that hand-off is the half this section is: **the
finding document must be told when the owner acts**, and it was not — §0.5.6 stood
as a live blocker notice, with a wrong problem statement and two wrong remedies,
for a full gate after the defect was fixed. A reader sent there was worse off than
one who never followed the pointer. The mechanism now is `DECISIONS.md`'s open
list, which is the one place every editor of every document reads; a finding
assigned elsewhere is recorded there in the same pass that finds it, and its
disposition is written back here when the owner acts.

### 0.6 D-014 at the transport layer — the exception is above it, and stays above it

**What D-014 decides, in one sentence, and it is not this document's to restate
beyond it (D-011 rule 1).** A player who sends a **provably illegal** message is
removed from the **table**; the attacked hand is voided neutrally, the seat is
dead and blinded off, the exit is one-way, and the evidence stays in the
transcript. `DECISIONS.md` **D-014** is the decision, `PROTOCOL.md` §4.0 and §4.9
own the wire, `STATE_MACHINE.md` T64, T65 and I34 own the seat, and
`CRYPTOGRAPHY.md` §8.1 owns what a failed proof does and does not prove. This
section states one thing and only one thing: **what changes here, which is
nothing, and why that needed saying.**

#### 0.6.1 What had to change in this document, and it is a scope word

Nothing in the transport gained or lost a capability. What was wrong was the
**scope** two sentences claimed:

| Site | Claimed | Now claims |
|---|---|---|
| §0.1 | *"No automated eviction, **anywhere**"*, on D-011 rule 3's every-layer extension | No automated eviction **at this layer**, ever — and the one exception, above this layer, named and pointed at |
| §1.2 prohibition 7 | *"remove, block, unseat, refuse or penalise a peer on the strength of a protocol proof"*, unscoped | the same, **at this layer**, with D-014's tier-1 removal named as a table disposition that never reaches here |
| §11.5.1 | correct as written — genuinely transport-only | unchanged, plus the paragraph that stops an implementer arriving from T64 reading it as licence to call `block_peer` |

The distinction the corrected wording rests on is the one §0.2 already draws, in
its own terms: **what supplies the reason.** D-014 did not add a reason this
layer may act on. It added a consequence a *different* layer may reach, from a
reason this layer must still refuse to look at.

#### 0.6.2 The exception, stated exactly, so it is not read wider than it is

> **Only self-authenticating evidence removes anybody, and it removes them from
> the table.** Evidence is self-authenticating when it is a **message signed by
> the accused, whose illegality any peer decides alone**, from that message plus
> state the peers provably share. That is D-014's **tier 1**: a signature that
> does not verify, a non-canonical encoding, a malformed message or an
> out-of-range field, a failed shuffle, decryption-share or key-ownership proof,
> a deck that is not a permutation, a signer that is not a party to this table.
> Removal is `STATE_MACHINE.md` T64/T65 and reaches canonical state only through
> `HAND_INIT(k+1)`'s collective body. **It produces no transport action of any
> kind, at any layer of `net/`, ever.**

Two properties do the whole of the safety work, and both are absent from every
judgement this layer is able to make:

1. **No quorum, no vote, no timing.** The verdict is a pure function of the
   accused's own signed bytes and inputs the peers provably share, so every
   honest peer computes the same answer and a replaying third party computes it
   too. Framing an honest peer would mean forging its signature.
2. **No receiver state.** Tier 1 is decidable from the offending message alone.
   A clause that needs the receiver's own store is not tier 1 and may not be
   treated as one — *whether the event's chain parent exists* was such a clause,
   it was tier 1 in three documents, and it is deleted from all of them and from
   `src/security/validation.rs`, because one dropped frame would have removed an
   honest player.

**Tier 2 — illegality decidable only against game state — is not this layer's
business at all**, and is named here only so it is not confused with tier 1: it
removes nobody until the state it is judged against is fixed by a checkpoint both
peers signed (`PROTOCOL.md` §4.9).

#### 0.6.3 What stays prohibited here, and it is every judgement this layer can make

D-014 narrowed D-010 point 3 **once**, for one input, above this layer. Every
other prohibition in §0.1, §1.2 prohibition 7 and §11.5.1 stands unamended, and
the reason is exactly the reason D-010 existed: each of these is a judgement two
honest receivers can reach differently, which is the property that produced every
severe finding of four adversarial passes.

| Judgement this layer can cheaply make | Status under D-014 |
|---|---|
| A peer is disconnected, unreachable, or slow to answer | **still no removal, of any kind.** A liveness judgement, and D-014 excludes liveness by name |
| A peer's stream dropped mid-hand | **still a hint** (§8.4), and the verdict remains D-006's certificate, inert whenever `\|V\| < 2` (D-008) |
| A peer equivocated | **still no removal.** D-014 excludes `EquivocationProof` by name; the predicate has been wrong five times, twice against honest peers |
| A `HAND_ABORT` attributes somebody | **still no removal.** `attributed` is per-receiver (D-012) |
| A timeout certificate names a seat | **still no removal**, at this layer or any other |
| A count of any of the above | **still no removal.** A count of per-receiver facts is a per-receiver fact |

And the transport's own defences are untouched by all of it. §0.2's "kept" rows —
resource limits, rate limits, connection and dial limits, size and canonicality
checks, relay admission, relay capacity, the DHT deny-all filter, the signed
roster's membership gate — are driven by facts about **our own** memory, sockets,
budget and consent, need no belief about anybody's honesty, and are neither
strengthened nor weakened here. §11.5.3's user block list keeps its single caller:
an explicit action the user took in the GUI. **The separation is the point**: a
D-014 removal and a rate-limit disconnect must never be described, logged or
displayed as the same event, because one is a verdict about a person and the
other is a statement about our uplink.

#### 0.6.4 D-013 as it bears on this, recorded here because the two compose

D-013 is registered in §15 and its transport pass is §0.5.7; what it adds *to
D-014* is one prohibition worth stating where a reader meets the removal. A
removed seat leaves the required emitter set, and **participation is inherited
from the chain, never sensed** — so no quantity this layer produces may be read as
evidence that a seat is gone, whether the seat left by D-014's one-way exit or by
any other route. A reconnected socket is not a re-entry (§0.5.7), and a silent
socket is not a removal. Both directions of that are §1.2 prohibition 8.

D-013's **process rule** is why this section exists at all rather than a fourth
open-list entry: a defect belonging to another owner is recorded in the pass that
finds it, and its disposition is written back to the finding document when the
owner acts. `DECISIONS.md`'s `G4-P5` row filed this one, with the three line numbers,
and was carried for three passes without this file being opened. The disposition
is written back there in the same pass as this section.

---

## 1. Layers, and the rule that constrains all of them

**Module this document governs** (`SPEC_CS.md` §23, interim mapping pending
`docs/ARCHITECTURE.md` in Phase 2): `src/net/` — `swarm.rs`, `run.rs`, `lobby.rs`,
`streams.rs` — plus the `InMemoryTransport` of §1.3, which sits behind the same
upward trait. **`dht.rs` stood in this list until 2026-08-31 and had been deleted
with `mainline`** (§5.8 of `DEPENDENCIES.md`); global discovery is a Kademlia
provider record in `run.rs`. The four named are representative, not the whole
module, which now holds sixteen files. The other four specification documents name their own modules in
their own §1; the separation of transport, poker engine and cryptographic deck
that `SPEC_CS.md` §23 requires is the separation between those five mappings.

### 1.1 The stack

```
                        ┌────────────────────────────────────────┐
                        │ GUI            (egui/eframe, docs/GUI) │
                        └───────────────────┬────────────────────┘
                                            │  commands / view models
   ┌────────────────────────────────────────┴────────────────────────────────┐
   │ Lobby / matchmaking      table list, presence, chat, join flow          │
   ├─────────────────────────────────────────────────────────────────────────┤
   │ Poker state machine      NLHE rules, pots, showdown, tournament clock   │
   ├─────────────────────────────────────────────────────────────────────────┤
   │ Mental Poker             joint deck, shuffle proofs, decryption shares  │
   ├─────────────────────────────────────────────────────────────────────────┤
   │ Protocol / signed event log   canonical CBOR, Ed25519, hash chain       │
   ╞═════════════════════════════════════════════════════════════════════════╡
   │ P2P transport  ── THIS DOCUMENT ────────────────────────────────────────│
   │                                                                         │
   │   net/run.rs      libp2p Kademlia: start_providing / get_providers     │
   │   net/swarm.rs    libp2p swarm: QUIC+TCP, Noise/TLS, identify, ping,    │
   │                   AutoNAT v2, DCUtR, Circuit Relay v2 (client+server),  │
   │                   connection & memory limits, user block list (§11.5), │
   │                   mDNS, UPnP                                            │
   │   net/lobby.rs    GossipSub topics, snapshot request-response           │
   │   net/streams.rs  per-table direct streams (libp2p-stream)              │
   └───────────────────┬─────────────────────────────────────────────────────┘
                       │
              ┌────────┴────────┐
              │                 │
   public IPFS/Amino DHT     libp2p (QUIC / TCP, GossipSub, relay)
   (discovery hints only)    (everything else)
              │                 │
              └────────┬────────┘
                       ▼
              other poker clients
```

The double line is a hard boundary. Everything below it moves bytes and
connections. Everything above it decides meaning.

### 1.2 The rule (`SPEC_CS.md` §1)

> *Síťová vrstva NESMÍ rozhodovat o pokerových pravidlech ani vytvářet karty.*
> The network layer must never decide a poker rule and never create a card.

Stated as prohibitions the implementation can be reviewed against. The network
layer must **not**:

1. create, derive, permute, encrypt, decrypt, or in any way produce a card, a
   deck, a shuffle, a permutation, or randomness that ends up in a card;
2. decide that an action is legal, that a betting round is over, that a street
   advances, that a hand is over, or who won;
3. decide that a player is absent, has timed out, has folded, or has forfeited.
   Transport-level connection loss is a **hint** the layers above may consider; the
   verdict is the D-006 timeout certificate, which is an application event signed
   by the required voter set `V` — and whenever `|V| < 2` there is no such verdict
   at all, because D-008 leaves the deadline advisory there (§8.4). At two seats
   that is always so (D-007); at larger tables it is reachable too, which is why
   the test is `|V| < 2` and never `n = 2`. The prohibition is
   unchanged either way: the transport must not supply a verdict the layer above
   does not have;
4. reorder, deduplicate, filter, merge or "repair" application events on the basis
   of anything other than the bytes' own signatures and size caps. Ordering is the
   hash chain's job (`SPEC_CS.md` §13);
5. treat a `PeerId`, a connection, or a DHT record as an authorisation to act as a
   player (`SPEC_CS.md` §20 — "PeerId říká, s jakým socketem mluvíš, ne kdo hraje");
6. hold, forward, log or persist any secret from the mental-poker layer;
7. **remove, block, unseat, refuse or penalise a peer *at this layer* on the
   strength of a protocol proof.** An `EquivocationProof` (`PROTOCOL.md` §5.2), a
   timeout certificate (`PROTOCOL.md` §8.3), a signed abort attribution or an
   invalid application signature are evidence for a human and produce **no
   transport action at all** (D-010 point 3, D-011 rule 3, §0.1). This
   prohibition is about the *reason*, not the action: the same disconnect is
   permitted when its reason is a resource, rate, size or admission fact (§0.2),
   and forbidden when its reason is a verdict about whether somebody cheated.
   **D-014 does not amend this prohibition and cannot reach it**, and the words
   *at this layer* are what this pass added: a tier-1 self-authenticating finding
   — an invalid application signature among its triggers by name — removes its
   sender from the **table** (`STATE_MACHINE.md` T64, T65), and never from a
   socket, a dial queue, a relay reservation or a list of names here. An
   implementer who reaches a tier-1 verdict and then reaches into `net/` for it
   has crossed this prohibition; §0.6 draws the boundary and §11.5.1 restates it
   where the deleted `block_peer` path used to be;
8. **supply any quantity of its own as canonical state.** Nothing this layer
   measures — which peers are connected, in what order bytes arrived, what the
   local clock reads, what AutoNAT concluded, whether a link is relayed, how many
   peers answered, which copy of a re-broadcast advert we hold — may enter a state
   hash, a roster hash, a chained event body or a hand's genesis (D-012, §0.5).
   All of it is a **local view**: it may be displayed, and it may be a hint the
   layers above consider, and it is never a fact two honest clients are required
   to agree on. The catalogue of every such quantity this layer produces, and
   where each is allowed to go, is §0.5.2.

The network layer **must**:

1. bootstrap the public Kademlia and the libp2p swarm (one swarm, §5.7);
2. publish and read the fixed lobby rendezvous as presence hints (§3). `SPEC_CS.md`
   §1 words items 1 and 2 as *bootstrap do Mainline DHT* and discovery *pod pevným
   LOBBY_INFOHASH*: the fixed rendezvous is the requirement and still binds, and
   Mainline with its infohash was the mechanism, which is history (the note at the
   head of this document);
3. carry the GossipSub lobby;
4. do NAT traversal (AutoNAT v2, DCUtR) and relay fallback (Circuit Relay v2);
5. give every peer connection confidentiality, integrity and a cryptographically
   proven `PeerId` (Noise or TLS 1.3, over QUIC or TCP);
6. deliver application messages to named peers, and report delivery facts
   (connected, disconnected, relayed, direct) upward without interpreting them;
7. enforce resource limits (§11) — the one place the network layer is allowed to
   drop a message or a connection on its own authority, and only for size, rate,
   malformation, resource exhaustion or admission (§0.2). Never for a verdict.

### 1.3 The consequence that makes it testable

Because the layer above only ever sees "bytes arrived from `PeerId` X" and
"connection to X is up/down/relayed", the whole poker and cryptographic stack can
run on `InMemoryTransport` with no DHT and no libp2p (`SPEC_CS.md` §24). The
interface between `net/` and everything above it is therefore a narrow trait, and
that trait — not the libp2p types — is what the rest of the program is written
against. One more reason the same boundary is required: `libp2p-stream` is an
alpha release and semver-exempt. A second reason stood here until 2026-09-15 —
that `mainline` might one day need replacing by a hand-written KRPC client, an
escape hatch `MAINLINE_DHT.md` §6 sized at 1500–2400 lines — and it left with the
crate in `56b0b50`.

> Verification: [RESEARCH] `LIBP2P.md` §7 (alpha containment);
> `MAINLINE_DHT.md` §6 for the withdrawn second reason.

#### 1.3.1 The upward trait, specified

This is the whole surface the layers above `net/` may use. **Nothing
libp2p-typed may appear above this trait** — no `Multiaddr`, no `SwarmEvent`, no
`StreamProtocol`, no `libp2p::PeerId`. `PeerId` above the boundary is our own
newtype over the 32-byte Ed25519 public key material that the libp2p handshake
proved, and the libp2p implementation converts at the boundary.

```rust
/// The complete transport surface. Implemented by the libp2p stack (§5–§11)
/// and by `InMemoryTransport` (§1.3.2), and by nothing else.
trait Transport {
    /// Deliver `bytes` to `peer`. Returns as soon as the bytes are queued;
    /// success is NOT a delivery receipt, and the caller must never treat it
    /// as one (§1.2 prohibition 3).
    fn send(&self, peer: PeerId, bytes: Bytes) -> Result<(), SendError>;

    /// The single event stream. Ordering is per-peer FIFO and nothing more:
    /// events from two different peers may interleave arbitrarily.
    fn events(&self) -> impl Stream<Item = TransportEvent>;

    /// A fact about a socket, never a fact about a player.
    fn connection_state(&self, peer: PeerId) -> ConnectionState;
}

enum ConnectionState { Direct, Relayed, Down }

enum TransportEvent {
    Connected    { peer: PeerId, state: ConnectionState },
    Disconnected { peer: PeerId },
    Message      { peer: PeerId, bytes: Bytes },
    ListenerClosed { reason: ListenerCloseReason },
}
```

Three rules that make the trait the boundary rather than a formality:

* `Message` carries **bytes**, never a decoded event. Decoding, canonicality and
  signature verification are the protocol layer's (`PROTOCOL.md` §4.0).
* `Disconnected` is a hint. It is never an input to a poker decision
  (§1.2 prohibition 3, §8.4, §10.4).
* `ListenerClosed` exists because a relay reservation denial closes a circuit
  listener with no automatic retry (§9.6); the layer above must be able to see it
  and re-issue `listen_on` with backoff.

#### 1.3.2 `InMemoryTransport` (`SPEC_CS.md` §24), specified

`SPEC_CS.md` §24 requires a simulated network that implements the same trait and
runs the whole poker and cryptographic stack with no DHT and no libp2p. It is the
only vehicle for §25's adversarial suite, and it is **not deferred**: the
in-memory path is the only one exercised before Phase 7 (§12), so an unspecified
harness blocks Phases 3–6.

**(i) A seeded, deterministic scheduler.** Determinism is a requirement, not a
nicety: `STATE_MACHINE.md` I22 asserts replay determinism, and an
under-determined harness produces flaky adversarial tests that get muted rather
than fixed. Concretely — one `u64` seed per run, printed in the failure output
and sufficient to reproduce the run byte-for-byte; a single logical clock advanced
only by the scheduler, never by wall time; every scheduling decision (which queued
message is delivered next, which injection fires) drawn from that seed; and no
use of thread scheduling, `Instant::now`, or any OS entropy source anywhere in
the harness.

**(ii) The eight injection modes `SPEC_CS.md` §24 names**, each parameterised and
each reproducible from the seed:

| Mode | What it does |
|---|---|
| delay | holds a message for a bounded number of logical ticks |
| duplicate | delivers a message twice, at different ticks |
| packet loss | drops a message entirely |
| reordered events | delivers a peer's messages out of the order it sent them |
| disconnect | emits `Disconnected` and drops subsequent sends to that peer |
| reconnect | emits `Connected` again, with a chosen `ConnectionState` |
| malicious packets | delivers arbitrary attacker-chosen bytes as a `Message`, including truncated, over-cap, non-canonical CBOR and invalid-signature bodies |
| conflicting messages | see (iii) |

**(iii) How a conflicting message is injected**, since it is the one mode with a
protocol meaning. The harness must be able to hand **two different
`SignedEvent`s whose slot keys are equal** to two different receivers — which is
exactly the input the `EquivocationProof` predicate of `PROTOCOL.md` §5.2 is
defined over. **The slot key is that section's literal tuple and this document
does not reproduce it** (D-011 rule 2): a harness written against a copy tests
the copy, and an earlier revision of this paragraph carried a five-field tuple
that had already fallen behind the definition. Read the tuple from
`PROTOCOL.md` §5.2 at implementation time. The harness therefore needs the sender's
application signing key, because both copies must carry valid signatures; a test
that injects an invalidly signed second copy tests the signature check, not the
equivocation predicate, and the two must not be confused. Both copies carry
`chain_scope = 1`; the predicate is not defined over unchained traffic and the
harness must not attempt to produce a "conflict" there (§6.4, §7.4).

**(iv) The acceptance bar of `SPEC_CS.md` §24:** thousands of hands run
automatically, with no DHT and no libp2p process involved, each run reproducible
from its seed, and every `THREAT_MODEL.md` §5.5 cheater exercised against it.

---

## 2. Startup sequence

The chain, as the client runs it:

```
start → load persistent libp2p keypair → bootstrap the public Kademlia
      → start_providing(lobby key) → get_providers(lobby key)
      → dial the providers by PeerId → subscribe GossipSub lobby topic
      → request table snapshot from several peers → receive available tables
```

This is `SPEC_CS.md` §3's chain with its three Mainline steps replaced. The
assignment draws them as *bootstrap Mainline DHT → announce_peer(LOBBY_INFOHASH) →
get_peers(LOBBY_INFOHASH)*, and that drawing is kept there as history: the
original assignment stays unamended on the owner's word of 2026-09-15 (the note at
the head of this document), and `56b0b50` replaced those steps with the provider
record §§3 and 4 specify. Every other step is the assignment's, in its order.

The implementation follows it in that order, with two structural additions that
the spec's chain implies but does not draw: the libp2p listeners must be bound
**before** the announce, and the DHT and the swarm run concurrently from that
point on, not in sequence. The reason for the first has changed — it is no longer
*"we announce the port we listen on"* but that `libp2p-kad` publishes the swarm's
external address set, which is empty until something is bound and confirmed.

**This is where the sequence stopped being purely outbound, and it reverses a
layer ordering.** The old claim was: *"nothing in this sequence requires the
client to be reachable from outside; every step is an outbound operation"*, and
that was the property D-003 rests on.

It is still true of **reading** the lobby. It is **false of announcing**. A
provider record carries `external_addresses` and nothing else, so a client with
none announces itself with no way to be reached — which is indistinguishable from
not announcing — and `run.rs` therefore gates `start_providing` on the swarm
holding a confirmed external address. A NATed player has none until a relay
accepts a reservation and hands back a circuit address.

So **appearing** in the lobby now depends on §9.5's relay, which is layer 3 of
D-004, while §9.7 places lobby discovery at layer 1. Seeing the lobby is layer 1
and unaffected; being seen in it is not. That is a real condition under D-003 and
D-004 rather than a documentation detail, and it is why a client that cannot
reach a relay is invisible rather than merely unplayable.

### 2.1 Step table

| # | Step | Failure mode | What the client does |
|---|---|---|---|
| 0 | **Load or create the persistent libp2p Ed25519 keypair** from the portable profile directory next to the executable (`SPEC_CS.md` §22). Encoded with `Keypair::to_protobuf_encoding()`, read back with `from_protobuf_encoding`. On Windows the file is wrapped with DPAPI (`SPEC_CS.md` §21). | file missing → first run | generate a new keypair, write it, continue. **Never** generate a fresh identity when a keypair file exists but fails to decode — that silently forks the identity. Report the error and refuse to start, so a corrupted or foreign profile is visible rather than silently replaced. |
| 1 | **Load the cached DHT bootstrap list** and the per-peer DCUtR failure cache from the profile. | missing/corrupt | fall back to the compiled defaults; not fatal. |
| 2 | **Build the swarm** (§5) and `listen_on` `/ip4/0.0.0.0/udp/P/quic-v1`, `/ip4/0.0.0.0/tcp/P`, plus the `/ip6/::` equivalents. `P` is **0 unless `--port N` is given**: 0 lets the operating system choose afresh at every start, so two instances on one machine never collide, and a player who forwards a port on their router names it with `--port`, which binds that one number on QUIC and TCP alike (`run::listen_addrs`, §4.4). Nothing is persisted. Until 2026-09-15 this row said *`P` is chosen once, persisted, reused every run*, which the client has never done. **The `/ip6/::` half of this row was specified here and not implemented for the whole life of the client** — `S1-Y`, fixed 2026-09-02, and an IPv6 bind failure is reported rather than fatal. | `--port N` already in use | the IPv4 `listen_on` returns the bind error and the node does not start; the IPv6 half is reported and carried on without. There is no fallback to another port — this row used to promise one, persisted, and nothing implements it. |
| 3 | ~~**Start the Mainline DHT** on its own UDP socket, `.port(0)`, with a deny-all `RequestFilter`.~~ **Gone.** There is no separate DHT socket and no request filter: discovery is a `kad::Behaviour` inside the same swarm, on the same transports, and §11.4 explains why this client deliberately *does* answer strangers' queries. | — | there is no "no-discovery mode" any more: if the swarm cannot bind, nothing runs. |
| 4 | **Bootstrap the public Kademlia.** Dial the compiled entry point — one name, `/dnsaddr/bootstrap.libp2p.io` (`run::PUBLIC_ENTRY`), never a list of addresses — beside the peers the profile remembers (step 1). On the first `identify` from a peer that speaks `/ipfs/kad/1.0.0`, add its reachable listen addresses to the routing table and call `bootstrap()` once; after that `libp2p-kad`'s own periodic bootstrap keeps the table up (§11.4.1). | the entry is unreachable, or `bootstrap()` has no peer to start from (*"public DHT has no peers yet"*) | **normal**, never fatal, and there is no backoff ladder: while no relay has been seen after three relay searches, every discovery cycle dials the entry again (§9.5). The remembered peers are the way in on a day the entry is down. The failure figures this row carried until 2026-09-15 — *~3 of 35 cold starts failed on the first attempt*, *`router.bittorrent.com` is dead from this network* — were measured against Mainline's bootstrap and describe nothing that runs. |
| 5 | **`start_providing(lobby_namespace())`** (§3.2) at the moment an external address first exists — for a client behind a NAT, the relay circuit's arrival — and **again every 300 s (`REANNOUNCE_EVERY`) until the announcement is confirmed**: a walk of this session has handed the record to at least one node, and a node has since answered a lookup of the lobby key with this client's own record. From then on `libp2p-kad` owns the republish loop at its 12 h interval (§10.1). | announce error; or a walk that reached few storing nodes, which `start_providing`'s `Ok` cannot show — it comes back `Ok` from a walk that asked nobody | the next walk is the repair: the first happens seconds after the relay reservation, when the routing table is thinnest, and the next five minutes later against a fuller one. **Until 2026-09-15 this row said *once … there is no repair*, and for a client behind a NAT that was what ran** although `79ea1d5` (2026-09-03) had written the repair — the circuit arm latched at dispatch, and the self-sighting meant to confirm was answered by the client's own store (§10.1, `S1-FI`). |
| 6 | **`get_providers(lobby_namespace())`** → `HashSet<PeerId>` per responding node, emitted as each answers. Repeated every **60 s**. | zero providers | not an error, and the client says so out loud — *"public lobby: nobody else yet"* — because an answer of nobody and a question never asked look identical in a log that only reports findings. |
| 7 | **Dial the providers** (§4.3), by `PeerId`, at most `DIALS_PER_ANSWER = 8` fresh ones **per answer, not per cycle** — the counter is declared inside the `FoundProviders` arm and resets on every response, and one query draws one response per node that answers (707 of them in a measured 420-second run). This table said *per cycle* until 2026-09-02, and so did the constant's own name; both were wrong, and the effect was to make the crawl read sixty times slower than it is. Start on the first responder's answer — do not wait for the walk to finish. | most candidates fail | expected: a lobby key holds providers who left up to 48 h ago. |
| 8 | **identify + AutoNAT v2 settle our external address.** Filter private/reserved addresses ourselves — AutoNAT v2 has no such guard and was measured confirming an RFC 1918 address as external [MEASURED]. `run::reachable` is that filter, and its IPv6 arm was blind to `fc00::/7` until 2026-09-02 (`S1-Y`). | no confirmation | stay in `Reachability::Unknown`; continue — but note that with no external address this client is **not in the lobby at all**, per the paragraph above. |
| 9 | **Subscribe the GossipSub lobby topics** (§6). Can be done immediately after step 2; messages only flow once peers connect. | `SubscriptionError` | fatal configuration bug, not a runtime condition — fail loudly. |
| 10 | **Snapshot request to several peers** (§7) over `request-response`. | fewer than 2 usable responses | retry once with a fresh peer set, then proceed on live gossip alone and show "lobby syncing". |
| 11 | **Live GossipSub sync**, and the table list is displayed. | — | steady state. |

Steps 3–7 and 9–11 run concurrently with the GUI; the GUI is never blocked
(`SPEC_CS.md` §33).

### 2.2 The network status the GUI must be given (`SPEC_CS.md` §22)

The transport layer publishes a single observable status struct:

* **DHT**: `NotStarted | Bootstrapping | Ready | Failed`, last successful announce
  time, storing-node health count (§2.1 step 5), number of unique candidates from
  the last lookup.
* **libp2p**: local `PeerId`, listen addresses, confirmed external addresses,
  reachability (`Unknown | Public | Private`), number of established connections,
  how many are relayed.
* **Lobby**: subscribed yes/no, GossipSub mesh peer count per topic, number of
  live table ads, time since last snapshot.
* **Relay**: whether we hold reservations and on which relays; whether we are
  *acting* as a relay and for how many peers (D-002 requires this to be visible);
  and for every table peer, whether the connection is direct or relayed (D-001).

"Still relayed after three DCUtR attempts" is a normal steady state, not an error
(`MAX_NUMBER_OF_UPGRADE_ATTEMPTS = 3` [SOURCE]) and must be displayed as such.

**Every field of this struct is a local view and it is a display surface only**
(D-012, §0.5.2). Reachability, relay status, mesh size, connection counts and
the DHT health number are things this client measured about itself and its own
sockets; two honest clients report different values at every instant. None of
them may be consumed as a fact about a *table* — not by the GUI, which must
phrase them as facts about this client's connection, and not by any layer above,
which may read them as hints and never as state (§1.2 prohibitions 3 and 8).

---

## 3. The lobby rendezvous

> **Rewritten 2026-09-02 to the mechanism the client runs.** §§3.1–3.4 specified
> a 20-byte BitTorrent infohash and a Mainline announce; `56b0b50` replaced that
> with a libp2p Kademlia **provider record** and deleted both infohashes from the
> build. §3.5 was re-derived for the provider record the same day; its own note
> says how, and which three figures are still unmeasured.

### 3.1 Requirement

`SPEC_CS.md` §3: every installation shares **one fixed rendezvous**, and the
client must not create a new lobby on each start — so it is a compiled-in
constant. It must also be **verifiable**: anyone auditing the client should be
able to recompute it from first principles rather than trust a magic number.

**That requirement is mechanism-independent, and only the thing that discharges
it moved.** What follows is a different construction of the same fixed, shared,
recomputable meeting place.

### 3.2 Derivation

Two namespaces, and the key each becomes:

```
lobby namespace = ASCII("p2p-poker/main-lobby/v1")
relay namespace = ASCII("/libp2p/relay")          -- go-libp2p's own

key(ns) = 0x12 || 0x20 || SHA-256(ns)             -- 34 bytes
```

`0x12 0x20` is the multihash prefix for sha2-256 and a 32-byte digest, so the
key is exactly the multihash of `CIDv1(raw, sha2-256(ns))`. That mapping is
go-libp2p's routing discovery: it turns a namespace into that CID and
go-libp2p-kad-dht keys the provider record on the CID's multihash, so a client
written against either implementation lands on the same key.

The published values, and `the_lobby_rendezvous_key_is_the_published_one` pins
them:

```
lobby  12207e342925602a7c6558d6ac574207bcc56f7989e17ad52b7694f2c964d772c4b6
relay  1220245eebd20d2cd4c81b5d4ac27c73746279f436d62f3ef52c452a369e6ef7b610
```

Every client calls `start_providing(key)` for the lobby key and `get_providers`
on it; a client that also serves as a relay provides the relay key as well. That
is the whole of the lobby: a place all clients agree on, where each finds the
others' addresses, after which table advertisements travel over GossipSub exactly
as §6 describes.

A lobby cut into slices (`S1-EX`) adds one key per slice it listens to,
`p2p-poker/main-lobby/v1/<slice>`, derived the same way: the peers of a slice
provide and look up that key, so the slice's GossipSub mesh has connected peers
to form from. The slice strings and depths are `PROTOCOL.md` §1.1's; at depth
zero there are no slices and only the lobby key above is used.

**The derivation is in `src/net/run.rs`'s `namespace`, computed rather than
pasted**, and the test above writes the expected bytes out rather than
recomputing them — a test that recomputes the thing it checks passes whatever the
code does. This is the one constant on which two independent implementations find
each other or do not, and until that test existed a change to the namespace
string, the hash or the prefix would have moved the lobby in silence while every
client kept working perfectly alone.

### 3.3 Why this construction

**A BitTorrent announcement can say one thing — `IP:port` — and a player behind
a NAT does not have one worth saying.** What such a player has is a circuit
address through a relay: a multiaddr, which does not fit in four bytes and a
port. A provider record carries whatever addresses the node has, so **one
mechanism serves the reachable player and the unreachable one**, and that is the
difference between a lobby most people can be seen in and a lobby only a minority
can.

It also removes a dependency and a divergence: the client already runs a libp2p
Kademlia for peer routing, so the lobby rides a table it maintains anyway rather
than a second DHT with its own crate, its own bootstrap set and its own failure
modes.

The namespace strings are versioned (`/v1`) so that a future incompatible lobby
is a different key rather than a mixed one.

### 3.4 Distribution and integrity

**A provider record is a hint and not a credential**, exactly as §4.1 says of the
address list it produces. What a provider record asserts is that some peer
claimed to provide this key; it is not evidence about a table, a roster, or a
player. Everything that matters is checked afterwards and elsewhere:

* the **peer id** is bound to the connection by libp2p's own handshake, so an
  address that answers is the peer it claims to be or the dial fails;
* a **table** is a signed `LOBBY_TABLE_AD` under the table key (§6, `PROTOCOL.md`
  §7.2), which the DHT never sees and cannot forge;
* a **seat** is the roster's, ratified under §4.3, and no discovery answer
  changes it.

So a hostile provider record costs a dial and nothing else. Its cost in
**privacy** is a different question and is §3.5's.

### 3.5 What a fixed public rendezvous costs — and it must be told to the user

> **Re-derived 2026-09-02 for the provider record.** Everything here used to be
> Mainline's: BEP 5's token, LRU eviction, a measured `~45 minutes`. The
> **obligation** never moved — `SPEC_CS.md` §3 requires these limits to be
> documented and forbids answering them with a central server — but every fact
> that discharged it did. What follows was derived from `libp2p-kad 0.48.0` and
> re-read on 2026-09-15 against `0.49.0`, the version `Cargo.lock` pins now — no
> default or behaviour below moved, only line numbers — and from what
> `src/net/run.rs` and `src/net/swarm.rs` actually do with it. Three figures are
> marked `[UNMEASURED]` and are the honest remainder of `S1-E`.

**Start with the fact that frames all the others: the lobby is not our network.**
The rendezvous lives on the **public IPFS DHT**. `swarm.rs` builds a second
Kademlia speaking `/ipfs/kad/1.0.0` and `run.rs` bootstraps it from
`/dnsaddr/bootstrap.libp2p.io`. The nodes that store this client's record, and
the nodes that answer its questions, are strangers running unrelated software for
unrelated reasons. That is the price of not running a server, and it is the same
price §3.3 says is worth paying — but the user is the one paying it.

* **The record names the player, not just an address, and that is the biggest
  change.** A Mainline announce stored four bytes and a port. A provider record
  stores a **`PeerId` plus multiaddrs** (`dht.proto`, `Message.Peer{id, addrs,
  connection}`), and that `PeerId` is this project's **persistent** identity
  (§5.6) — the same key that later sits at the table and signs in the lobby. So
  the fixed rendezvous now carries a **durable pseudonym that links every session
  a player ever has**, and the storing nodes watch it return day after day with
  whatever IP it holds that day. The Mainline announce disclosed an address and
  forgot who you were; this one remembers.

* **It leaks more addresses than it publishes — through one door now, where there
  were two.** The outgoing `ADD_PROVIDER` carries confirmed *external* addresses
  only (`behaviour.rs:1563`). But when this node **answers** a `GET_PROVIDERS`
  about a key it provides, `libp2p-kad` fills in its own record with
  `listen_addresses ∪ external_addresses` (`behaviour.rs:1267-1274`, the listen set
  kept from every swarm event at `:2684`), and until 2026-09-02 `identify` sent the
  same union to **every peer it connected to**, because `hide_listen_addrs`
  defaults to `false` (`libp2p-identify-0.48.0/src/behaviour.rs:204, 342-348`).
  Measured on the development machine, 2026-09-02: that union contained
  `/ip4/192.168.1.20/...`, `/ip4/172.27.224.1/...` — a Hyper-V "Default Switch"
  address — and two `fdc9:…` IPv6 ULAs [MEASURED]. **The external half is filtered
  and the listen half is not.** `run::reachable` gates what AutoNAT may add as an
  external address, so §5.6's publish filter is applied where the section says it
  is; but `listen_addresses` is the raw bound set. `S1-Z` closed the `identify`
  door the same day — `net::swarm::build` sets `with_hide_listen_addrs(true)` —
  and this bullet went on saying *this project never sets it* until 2026-09-15.
  **The `libp2p-kad` door is still open**: the identify setting does not reach it,
  so a stranger who asks this client, while it serves the DHT, who provides the
  lobby key learns this player's LAN topology and which hypervisor they run.
  `S1-FJ`.

* **The same connections announce what this software is.** `identify` sends
  `protocol_version = "/p2p-poker/1"` and `agent_version = "p2p-poker/<version>"`
  to every peer it speaks to on the public DHT. Under Mainline the client was one
  more KRPC speaker among millions and looked like a BitTorrent node. Here it
  **says what it is, by name, to every stranger it meets**, whether or not that
  stranger ever sees the lobby key.

* **The record *is* authenticated — in one narrow way, and it is not the way that
  matters.** `behaviour.rs:2415-2421`: *"Only accept a provider record from a
  legitimate peer"*, `if provider.node_id != source { return; }`, where `source`
  is the peer id the libp2p handshake bound to the connection. **Nobody can
  announce somebody else's `PeerId` in our lobby**, which BEP 5's token could not
  prevent. That is the whole of it. Anyone may still provide the key **for
  themselves**, the multiaddrs inside are self-asserted and checked by nobody, and
  the record remains a **hint** (§3.4) — never evidence about a table, a roster,
  or a seat. The forgery it stops is impersonation; the flooding it does not stop
  is a stranger in the list, and that still costs a dial and nothing else.

* **The record outlives the player by up to two days, and there is no way to
  withdraw it.** `provider_record_ttl` defaults to **48 hours** and
  `provider_publication_interval` to **12 hours** (`behaviour.rs:231-232`), and
  the expiry is stamped by the **storing** node from its own config, not the
  publisher's (`behaviour.rs:1959`). `stop_providing` is documented as *"a local
  operation"* — other nodes go on considering you a provider until the record
  expires (`behaviour.rs:1047-1054`) — and **`run.rs` never calls it at all**. So
  closing the client changes nothing anyone else can see: the player's `PeerId`
  and addresses stay in the public lobby, findable, for as long as the storing
  nodes keep them. **Against Mainline's measured ~45 minutes that is roughly sixty
  times longer.** `DIALS_PER_ANSWER = 8` exists precisely because of this: *"a
  lobby key outlives the clients in it"*.

* **And while it is running it refreshes only until somebody returns it.**
  `AddProviderJob` waits a full interval before its first run
  (`jobs.rs:268-279`), so inside a session shorter than 12 hours the crate never
  republishes. The client walks the announcement itself: on its first external
  address, and again every 300 s until a node returns this client's own record
  (§10.1). Measured on 2026-09-15, a node returned it within about a minute of
  the first walk, so a session ordinarily publishes once, to the 20 nodes closest
  to the key at that moment; one whose record nobody returns publishes every five
  minutes, more often than Mainline's 10-minute re-announce did. Until
  2026-09-15 this bullet said *publishes exactly once*, which was what ran for a
  client behind a NAT whatever happened to its walk (`S1-FI`). Those 20 nodes
  churn, and once confirmed nothing notices. `[UNMEASURED]`

* **Reading the lobby discloses more than writing it, and far more often.** Every
  `get_providers` runs over an authenticated libp2p connection, so each node on
  the walk learns the querier's **`PeerId`**, its **IP**, its **identify banner**,
  and the **exact key asked for**. The client asks every **60 seconds**
  (`run::run`'s discovery timer) — about **1440 times a day**, against the ~150 the Mainline
  section counted — plus a second query for the relay namespace whenever there is
  no reservation. **How many distinct nodes one walk contacts is `[UNMEASURED]`;**
  the Mainline figure of 105-176 has no counterpart here and must not be assumed
  to carry over in either direction.

* **The audience is worse than "a rotating set of strangers": it is a fixed one,
  and anyone may join it.** The key is a compiled-in constant, so the ~20 nodes
  closest to it are the same nodes for everybody and are **computable by anyone
  who can read §3.2**. Placement is `sha2-256(PeerId)` (`kbucket/key.rs:115-120`),
  so an observer grinds an Ed25519 keypair until its digest lands near the key,
  runs one always-on node, and from then on **receives every `ADD_PROVIDER` and
  every republish, passively, without ever announcing itself.** The grind cost is
  set by the size of the public DHT and is `[UNMEASURED]` here. This is the
  Kademlia analogue of the crawl §3.5 used to report as observed, and it is
  cheaper, quieter and more complete.

* **A cheaper enumeration needs no grinding at all.** Poll the key exactly as our
  own client does. Each responder returns up to 20 providers
  (`behaviour.rs:1295`, the replication factor, `K_VALUE = 20` by default), the union over the closest nodes is the
  live lobby, and a script left running for a month has the **historical** lobby —
  every `PeerId` that ever sat there, the addresses each was reachable at, and the
  times it appeared. **A `PeerId` is stable, so that log is a per-player
  attendance record.** Nothing in this design prevents it and nothing detects it.

* **A player in the lobby also works for strangers.** `set_mode(None)`
  (`net::swarm::build`) restores libp2p's rule: **server** once there is a confirmed
  external address. In server mode this machine answers public-DHT queries and
  stores other people's provider records — for arbitrary IPFS content, having
  nothing to do with poker — and its address sits in strangers' routing tables.
  `run::dht_effort` turns this down to client mode at a closed table, which is
  a bandwidth decision and happens to be the only mitigation that exists.

* **One eviction rule is worth knowing because it is the reverse of Mainline's.**
  When a rust-libp2p storing node already holds 20 providers for a key it
  **ignores the newcomer** rather than evicting anyone
  (`record/store/memory.rs:170-175`, *"This strategy can mitigate Sybil attacks"*).
  Mainline's LRU pushed the oldest out; this pushes the **newest** away. Under a
  flood the player who cannot be seen is the one who just arrived. Expired entries
  are pruned only lazily, on a query that touches the key
  (`behaviour.rs:1245-1249`). **What the go-libp2p nodes that actually store our
  record do instead is `[UNMEASURED]`**, and it is the same gap as the TTL.

* **What is unchanged, and should not be softened.** It is still one fixed public
  key that no one can un-publish. It is still a hint and not a credential. It
  still tells strangers that this address is interested in this one value. There
  is still **no central server**, and none of the above is a reason to add one.
  And there is still **no way to remove the disclosure — only not to announce.**

**Therefore — and of this list only the identify half of item 4 is built:**

1. a **one-time, plain-language consent screen** before the first announce, which
   must now say *forty-eight hours* and *a name that follows you between
   sessions*, not *forty-five minutes* and *your IP address*;
2. a **direct-invite mode** that never announces and never queries the public key;
3. **announce only while the user is actually looking for a game** — and, because
   `stop_providing` is local, tell them plainly that leaving does not take the
   record back;
4. keep LAN and virtual-adapter addresses out of **both** the identify banner and
   the provider record. **Half built**: `with_hide_listen_addrs(true)` has closed
   the identify half since 2026-09-02 (`S1-Z`) — this item, and the heading above
   it, went on listing it as not implemented until 2026-09-15. It does not reach
   `libp2p-kad`, which fills this client's own record from the listen set when it
   answers a lookup, so the provider-record half is open (`S1-FJ`). §5.6's publish
   filter is already applied on the external-address path and needs no change
   there; what leaks is the listen half. The cost of hiding it is a delay rather
   than a loss — a genuinely public host's address is advertised once AutoNAT
   confirms it, and the LAN case is served by mDNS (§9.8), which does not go
   through `identify` at all;
5. `SPEC_CS.md` §3's own words remain the test: *"Nic z toho není důvod k
   centrálnímu serveru, ale hráč to má vědět."*

An epoch-rotating **namespace** (`"p2p-poker/main-lobby/v1" ‖ floor(day)`) is the
same trade as the epoch-rotating infohash was, plus one new cost this mechanism
adds: with a 48-hour record TTL the previous day's key stays populated for two
more days, so rotation blunts a historical crawl far less than it looks. Still
**not adopted**; OQ-2 is carried forward against the new mechanism.

> **Verification:** `[SOURCE]` `libp2p-kad-0.49.0/src/behaviour.rs:231-232`
> (TTL 48 h, republish 12 h), `:1959` (the storing node stamps expiry),
> `:2415-2421` (provider must equal the authenticated sender), `:1047-1054`
> (`stop_providing` is local), `:1267-1274`, `:2684` and `:1563` (which addresses
> go where), `:1295` and `lib.rs:91` (20 providers per response),
> `record/store/memory.rs:170-175` (a full list ignores the newcomer),
> `jobs.rs:268-279` (first republish after a full interval),
> `kbucket/key.rs:115-120` (`sha2-256(PeerId)` placement);
> `libp2p-identify-0.48.0/src/behaviour.rs:204, 342-348` (`hide_listen_addrs`
> default false, and what it withholds). Line numbers re-read on 2026-09-15; they
> named `libp2p-kad-0.48.0` and `libp2p-identify-0.47.0` before, and no value moved.
> `[SOURCE]` this project: `net::swarm::build` (both Kademlias, `set_mode(None)`,
> `with_hide_listen_addrs(true)`), `run::PUBLIC_ENTRY`, `run::DIALS_PER_ANSWER`,
> `run::reachable`, `run::dht_effort`, and the announce and lookup arms of
> `run::run` — by name, because this file's line numbers into `run.rs` had all
> gone stale as it grew.
> **`[UNMEASURED]`, and these are what keeps `S1-E` open:** (i) the TTL the
> **go-libp2p** nodes that actually store our record apply — the 48 h above is
> rust-libp2p's default and is almost certainly not the operative number;
> (ii) how many distinct DHT nodes one `get_providers` walk contacts, which is the
> figure the Mainline section had and this one does not; (iii) the
> keypair-grinding cost of placing a node among the 20 closest to the lobby key,
> which sets the price of complete passive enumeration. **Do not quote a number
> for any of the three until it has been measured.**

---

## 4. The DHT-to-libp2p bridge

This is the load-bearing mechanism of the whole discovery story. Everything else
in this document assumes it works.

> **Rewritten 2026-09-02, and it was the largest stale block in the file.**
> §§4.2–4.4 specified a `get_peers` → `SocketAddrV4` → synthesised-multiaddr
> path built on the `mainline` crate, which `56b0b50` deleted. There is no such
> path in the client and there is no such crate. This is the section a second
> implementer would have followed line by line, and the head warning's own
> reading list did not mention it. `S1-E`.
>
> §§4.1, 4.5, 4.6 and 4.7 were **not** rewritten wholesale, because most of what
> they say is about hints, hostile inputs and identity rather than about
> Mainline, and that survives the change of mechanism. Where they name a
> `get_peers` response or a `SocketAddrV4` they are corrected in place, and
> §4.5 carries a finding rather than a correction.

### 4.1 The rule it must obey

`SPEC_CS.md` §1, restated verbatim in force:

> The peer list from the Mainline DHT is **not verified**: anyone can write
> anything there. Treat it as a hint about **where to try connecting**, never as a
> claim about **who is there**. Identity is decided only by the libp2p handshake
> and by the signature on the application message.

The quotation keeps the assignment's words, and its first sentence names a
mechanism that is history: `SPEC_CS.md` stays unamended as the original
assignment (the note at the head of this document), and the DHT in question is
now the public Kademlia of §3. The rule binds that DHT without a word changed.

Nothing in the bridge may weaken this. In particular the bridge must not add a
"signed DHT record" scheme to make the hint trustworthy — that would be a new
protocol where none is needed. The measurement that first argued it was
Mainline's — `dht 7.0.0`'s signed-peer variant reached only **2–3 storing nodes
versus 19–32** for a plain announce [MEASURED, `MAINLINE_DHT.md` §3.4] — and the
conclusion does not rest on it: a provider record already names its sender, as
the next paragraph says, and nothing a table needs from discovery goes beyond a
place to try connecting.

**The rule survives the change of mechanism, and one clause of it got stronger
without anybody deciding that it should.** A Mainline record was six bytes that
anybody could write under anybody's name. A libp2p provider record names a
`PeerId`, and a storing node refuses one whose `node_id` is not the peer on the
authenticated connection that sent it (`libp2p-kad-0.49.0` `behaviour.rs:2415-2421`,
*"Only accept a provider record from a legitimate peer"*). So **nobody can
announce somebody else's identity in our lobby**, which BEP 5's token could not
prevent.

That is the whole of the improvement and it must not be over-read. Anyone may
still provide the key **for themselves**; the multiaddrs inside are self-asserted
and checked by nobody; and the record is still a **hint** about where to try
connecting, never a claim about who is there or a licence to sit down. §4.7 is
unchanged and is what carries the rest.

### 4.2 What the DHT actually gives us

**A set of `PeerId`s, and not one address.**

```rust
// libp2p_kad::Event::OutboundQueryProgressed { result, .. }
QueryResult::GetProviders(Ok(GetProvidersOk::FoundProviders { key, providers, .. }))
//                                                            ^^^^^^^^^ HashSet<PeerId>
```

Each `FoundProviders` is the answer of **one responding node**, emitted the
moment it arrives rather than at the end of the walk, so multiplicity across
responders is still observable — the property §4.6 leans on. A node returns every
non-expired provider record it holds for the key except the asker's own, and its
store caps that at twenty per key.

The **addresses** are not in that event and the application never sees them. They
travel in the same DHT messages and are put straight into `libp2p-kad`'s routing
table, which is where the dial in §4.3 finds them. This is the single largest
difference from the Mainline bridge: there is no `SocketAddrV4`, no port, and
nothing for application code to parse, filter or synthesise.

> Verification: [SOURCE] `libp2p-kad-0.49.0/src/behaviour.rs:1242-1252` (a
> responder returns every non-expired provider except the asker), `:2365-2399`
> (each batch emitted as it arrives, an empty one included),
> `record/store/memory.rs:69` (twenty per key); this client: `src/net/run.rs`
> `SwarmEvent::Behaviour(… IpfsKad(OutboundQueryProgressed))`. Re-read 2026-09-15
> from the `0.48.0` lines 1238-1248 and 2362-2396; one exception is worth knowing,
> and both versions have it — a stored provider with no addresses that is neither
> the responder nor in its routing table is left out of the answer (`:1277-1281`).

### 4.3 The bridge, precisely

For each `PeerId` in the answer:

```rust
let opts = DialOpts::peer_id(peer)
    .condition(PeerCondition::DisconnectedAndNotDialing)
    .build();
let _ = swarm.dial(opts);
```

**By peer id, not by address.** The addresses arrived with the query and are in
the routing table; asking for them by hand would be asking a second time for what
is already known. `DisconnectedAndNotDialing` is what makes a 60-second discovery
cycle idempotent — a provider that answers in three consecutive cycles is dialled
once.

Two consequences follow, and the first is the reason the old §4.3 needed no
`unknown_peer_id` equivalent:

* **The dial names the identity in advance.** libp2p refuses a connection whose
  handshake proves a different `PeerId` than the one dialled, so the record's
  claim and the handshake's proof are compared by the transport rather than by
  us. Under Mainline the dial was `DialOpts::unknown_peer_id()` and the identity
  was whatever answered.
* **A provider with no reachable address is a dial that fails immediately**,
  rather than a synthesised `/ip4/…/udp/…/quic-v1` that has to time out. That is
  the ordinary case, because a lobby key holds providers who left up to 48 hours
  ago (§10.1).

At most `DIALS_PER_ANSWER = 8` **fresh** providers are dialled per DHT answer. A lobby
key outlives the clients in it, so dialling every provider at once fills a finite
connection budget with the dead and leaves none for the living — measured as a
table that stopped forming at all. Whoever is not reached this cycle is reached
the next.

### 4.4 Ports, and why there is no longer a rule about them

**The whole of the old §4.4 is gone, and nothing replaced it.** It specified
`announce_peer(info_hash, port: Option<u16>)`, why `implied_port` was refused,
that the DHT socket could not share libp2p's port, how the external port was
learned from `observed_addr`, and a **one-port rule** binding QUIC and TCP to the
same number so that one six-byte record could serve both. Every one of those
sentences was about a mechanism that announced an `IP:port`.

A provider record announces **addresses**, plural, in whatever form the swarm
holds them — QUIC, TCP, IPv4, IPv6 and circuit addresses side by side — so there
is nothing to choose between and no port to get right. What the client publishes
is `external_addresses`, and `run::reachable` decides what may enter that set
(§5.6).

**One rule of the old section survives for a different reason.** `run::listen_addrs`
still binds the same numeric port for QUIC and TCP, because a player who forwards
one port on their router should have to write one rule and not two. That is a
convenience for a human, not a requirement of the discovery mechanism, and
`listen_addrs_v6` binds the same number again on IPv6 (§5.8).

**And one of its properties is lost rather than replaced.** A Mainline announce
could be made by a client that could not be dialled — it published an `IP:port`
whether or not that address worked. `start_providing` is gated in `run.rs` on the
swarm holding a confirmed external address, because `libp2p-kad` publishes the
external set and nothing else, so **a NATed player cannot appear in the lobby
until a relay has given it a circuit address**. That reverses §9.7's layer
ordering — layer 1 now depends on layer 3 — and it is why §2's *"every step is an
outbound operation"* is no longer true of the announce half.

### 4.5 Filtering candidates before dialling

> **Built 2026-09-02, and not where this section expected it.** For two days
> this filter had no counterpart at all: under Mainline the candidate was a
> `SocketAddrV4` that application code parsed, and under Kademlia the addresses
> never pass through application code — they go from the DHT message into
> `libp2p-kad`'s routing table, and `DialOpts::peer_id` asks the swarm to use
> whatever is there. The provider handler sees a `HashSet<PeerId>` and nothing
> else. So the attack rule 2 names — *"an attempt to make us scan our own LAN"*
> — was live. `S1-AC`.
>
> **libp2p records the provenance this section needed.**
> `DialOpts::peer_id(p).build()` sets `extend_addresses_through_behaviour: true`
> and every address-carrying builder sets it `false`, including
> `From<Multiaddr>`, which is what `swarm.dial(addr)` uses; `Swarm::dial` appends
> what a behaviour returns **only** when that flag is true. So a filter in
> `NetworkBehaviour::handle_pending_outbound_connection` sees the DHT path and is
> **structurally unable** to reach the mDNS path. `net::swarm::Bogonless<B>` is
> that filter: it wraps each `kad::Behaviour`, delegates every other method, and
> refuses what `not_a_bogon` refuses.
>
> **`not_a_bogon` is deliberately not `run::reachable`.** That one asks *"could
> the rest of the internet dial this"* and answers **no** for
> `/dnsaddr/bootstrap.libp2p.io`, which carries no IP — right where it is used,
> and wrong here, because it would cut names and the bootstrap shape out of the
> routing table. This refuses an address only when it **carries an IP and that IP
> is somebody's own network**, and keeps anything it cannot judge. RFC 6598 is
> spelled out because `Ipv4Addr::is_private` does not cover it; the IPv6 masks
> are spelled out because `is_unique_local` is unstable.
>
> **It counts what it drops**, on the status line and only when the count moves,
> so *how many private addresses the real Amino DHT hands this client* — a figure
> nobody had — arrives as a byproduct of the defence.
>
> Rules 1 and 3 are enforced by a different mechanism than this section
> describes: `PeerCondition::DisconnectedAndNotDialing` deduplicates, and libp2p
> will not dial this node's own `PeerId`. Rule 4's port floor is gone with the
> ports.
>
> The paragraph below the list is the part that made the design necessary: mDNS
> dials RFC 1918 addresses on the LAN deliberately, and a filter that could not
> tell a DHT-derived address from an mDNS-derived one would break the
> two-players-behind-one-router case. That is exactly what the provenance flag
> buys.

Applied to **DHT-derived** candidates only:

1. **Deduplicate** across all responders, and against candidates already connected
   or already known to be dead this session.
2. **Drop bogons**: RFC 1918 (`10/8`, `172.16/12`, `192.168/16`), RFC 6598
   (`100.64/10`), loopback `127/8`, link-local `169.254/16`, multicast, broadcast,
   `0.0.0.0/8`, and reserved ranges. A record in the public DHT claiming a private
   address is either pollution or an attempt to make us scan our own LAN.
3. **Drop our own external address** (any address AutoNAT/identify has confirmed
   as ours) and any address on the local machine.
4. **Drop port 0** and, as a matter of hygiene, ports below 1024.

This filter is **not** the same as the publish filter of §5.6 and the two must
never be merged. Same-LAN peers are found by mDNS with a full `/p2p/<PeerId>`
address and are dialled on their RFC 1918 address deliberately
(`NAT_AND_DISCOVERY.md` §5.2). Conflating "never dial a private address" with
"never publish a private address" breaks the two-players-behind-one-router case,
which is a case this project explicitly cares about.

### 4.6 Hostile inputs: stranger, stale, attacker

| Input | What actually happens | Cost | Handling |
|---|---|---|---|
| **A real BitTorrent client** squatting on the port, or any non-libp2p listener | **cannot happen any more.** The dial names a `PeerId` and the addresses come from a libp2p DHT record, so a non-libp2p listener is not in the candidate set at all | none | this row is kept because the *lobby key is still public* and anything may provide it — but what it provides is a libp2p peer, and that is the row below. |
| **A libp2p node that is not a poker client** | the handshake succeeds and we get a proven `PeerId`, but `identify` reports a different protocol set | one connection | disconnect after `identify` unless the peer advertises `/p2p-poker/1`; never admit it to the D-002 relay admission set (§9.6). Both are decisions about *our own* sockets and uplink, not verdicts about the peer — no proof is involved, nothing is recorded against it, and it may reconnect (§0.2). |
| **Stale entry** — the provider went offline (its record outlives it, §10.1), or the addresses it published no longer answer | a dial that fails, at once or at the handshake timeout | one dial slot | expected and normal. Bounded budget (§11.2); a provider that did not answer is tried again after a cooldown, never in a storm. |
| ~~**Wrong-port entry** — a NATed peer announced a port that is not its external port~~ | **cannot happen any more**: a provider record carries addresses, not a chosen port (§4.4) | — | kept for the record. It was a Mainline row, self-healing on that peer's next announce cycle; the nearest case today is an address that went stale, which is the row above. |
| **Attacker provides the lobby key with a third party's addresses** (reflection) | the dial names the attacker's `PeerId` and uses the addresses the attacker published, so we send a QUIC Initial / TCP SYN to an innocent host, whose handshake fails because it does not hold that key | small packet, no amplification beyond one handshake attempt per address per dial | bounded dial budget per answer, a cooldown before the same provider is dialled again, deduplication by `PeerId` (§4.3), and §4.5's filter, which refuses private and reserved addresses but not a public victim. No per-IP dial limit exists (§11.2). This is inherent to any DHT whose records carry self-asserted addresses — the provider record as much as the BEP 5 announce it replaced (§4.1). We reduce our contribution; we cannot remove it. |
| **Attacker floods the lobby key with thousands of junk provider records** (discovery DoS / eclipse attempt) — one generated identity per record, because a record must name its sender (§4.1) | our candidate list is mostly junk, so real peers are found slowly or not at all | latency, wasted dials | (a) hard cap on providers dialled per answer; (b) prefer candidates returned by **more independent responding nodes** — `FoundProviders` is emitted once per responder, so multiplicity is observable and a single Sybil writer is visible as a low-multiplicity set. **Not implemented**: this client takes the first `DIALS_PER_ANSWER` fresh providers in iteration order and weighs nothing; (c) random sample from the remainder so a flooder cannot deterministically fill the sample; (d) keep a persistent list of peers that previously completed a poker handshake and dial those first. **None of this defeats a well-resourced flooder** — see §12. |
| **Attacker announces its own address and completes the handshake** | it is now a connected libp2p peer with a proven `PeerId` | one connection slot | this is *allowed*. The DHT was never an authorisation. The peer can now gossip, and everything it says is subject to application signature verification and to the lobby validation of §6.4. It cannot forge another player's table ad, and it cannot join a table it is not admitted to. |

### 4.7 What the bridge establishes, and what it does not

After `ConnectionEstablished` we know: **this socket holds the private key for this
`PeerId`.** That is all.

We do **not** know: that the peer is honest, that the peer is a distinct human,
that the peer runs an unmodified client, that the DHT record we dialled belonged to
them, or that they are entitled to a seat. `SPEC_CS.md` §20's three-identity
separation is what carries the rest:

| Identity | What it proves | Where it comes from |
|---|---|---|
| libp2p `PeerId` | which socket we are talking to | persistent Ed25519 keypair, proven in the transport handshake |
| poker application identity | who signed this event | a **separate** Ed25519 application keypair, over canonical CBOR (`SPEC_CS.md` §12) |
| table/session identity | which table and which session an event belongs to | `table_id` + per-session nonce (`SPEC_CS.md` §14, §20) |

A GossipSub message signature (§6.3) authenticates the *first* of those and only
the first. It is transport hygiene and is **never** accepted in place of the
application signature.

---

## 5. Transport configuration

### 5.1 Pinned versions

`libp2p = "0.57.0"` — in `Cargo.lock` since `S1-FF` (2026-09-14), which moved to
it to clear three dependency advisories — resolving `libp2p-gossipsub 0.50.0`,
`libp2p-relay 0.22.0`, `libp2p-dcutr 0.15.0`, `libp2p-autonat 0.16.0`,
`libp2p-quic 0.14.0`, `libp2p-swarm 0.48.0`, `libp2p-core 0.44.0`,
`libp2p-identity 0.3.0`, `libp2p-identify 0.48.0`, `libp2p-kad 0.49.0`,
`libp2p-mdns 0.49.0`, `libp2p-request-response 0.30.0`,
`libp2p-connection-limits 0.7.0`, `libp2p-allow-block-list 0.7.0`.
Plus `libp2p-stream = "0.5.0-alpha"` (not re-exported by the umbrella) and
~~`mainline = "=8.0.0"`~~ — **removed in `56b0b50`**; discovery is `libp2p-kad`, and its version is whatever `libp2p` resolves to (`0.49.0` in `Cargo.lock`).

> **Re-read against the 0.57 set on 2026-09-15.** This section and every
> `[SOURCE]` citation into a libp2p crate in this document named the 0.56 set
> until then — `libp2p 0.56.0`, `libp2p-kad 0.48.0`, `libp2p-relay 0.21.1`,
> `libp2p-dcutr 0.14.1`, `libp2p-identify 0.47.0` and the rest — while the build
> had moved the day before. Each cited line was found again in the version
> `Cargo.lock` pins and the citation now names it; where a value or a behaviour
> moved, the passage says so in place, and where only a line number moved,
> nothing else changed.

**Never depend on a second instance of `libp2p-identity`, `libp2p-core`,
`libp2p-swarm` or `multiaddr`.** The umbrella now resolves `libp2p-identity 0.3.0`
(its requirement is `^0.3.0`); a direct dependency at any other minor would link a
second, incompatible `Keypair`/`PeerId` and produce a type-mismatch error rather
than a resolver error. Use the umbrella re-exports. Until 2026-09-15 this rule also
named `futures`, and warned that `libp2p-identity 0.3.0` was outside the old
umbrella's `^0.2.12` range; the client does depend on `futures = "0.3"` directly,
and that is safe for the reason the rule exists — it resolves to the one `0.3.x`
instance libp2p uses. `libp2p-stream` is the other deliberate exception and it
works for the same reason: its own ranges resolve to the same instances.

There is **no `connection-limits` cargo feature** — `libp2p-connection-limits` and
`libp2p-allow-block-list` are non-optional dependencies and
`libp2p::connection_limits` / `libp2p::allow_block_list` are always available.
Note that this makes `allow_block_list` **unavoidable in the link, and therefore
not something a decision can delete**: what D-010 and D-011 constrain is what may
*populate* it, which is the user and nothing else (§11.5).
`memory-connection-limits` (with hyphens) *is* a feature. Asking for
`connection-limits` is a hard resolver error.

> Verification: [RESEARCH+COMPILED] `LIBP2P.md` §1, §8, including the reproduced
> resolver error.

#### 5.1.1 Dependency register — the transport side (`SPEC_CS.md` §28)

`SPEC_CS.md` §28 requires a register with six columns for every crate the client
links. Its home is **`docs/DEPENDENCIES.md`**, which now exists and whose rows
`tests/corpus_dependencies.rs` checks against `Cargo.lock` on every `cargo test`;
this section keeps the transport side for a reader of this document, and
`CRYPTOGRAPHY.md` §9 the cryptographic side. **Neither is complete on its own**,
and both are hand-written and therefore subject to exactly the drift that
`PHASE0_REVIEW.md` B-3 found — this table named the 0.56 set for a day after the
build had left it.

| Crate | Version | Purpose | Repository | Licence | Security status |
|---|---|---|---|---|---|
| `libp2p` (umbrella) | `0.57.0` | transport, encryption, NAT traversal, gossip, relay | `github.com/libp2p/rust-libp2p` | MIT | RUSTSEC-2022-0084 (resource-management DoS) patched at `>= 0.45.1`; pinned version is patched |
| `libp2p-core` | `0.44.0` | transport traits, upgrades | as above | MIT | RUSTSEC-2019-0004 patched `>= 0.8.1`, RUSTSEC-2022-0009 patched `>= 0.31.1`; both far below the pinned version |
| `libp2p-identity` | `0.3.0` | `PeerId`, `Keypair` | as above | MIT | no advisory in the local advisory database |
| `libp2p-swarm` `0.48.0`, `libp2p-swarm-derive` `0.36.0` | — | swarm driver, `#[derive(NetworkBehaviour)]` | as above | MIT | as above |
| `libp2p-quic` `0.14.0`, `libp2p-tcp` `0.45.0`, `libp2p-dns` `0.45.0` | — | base transports (§5.3) | as above | MIT | as above |
| `libp2p-noise` `0.47.0`, `libp2p-tls` `0.7.0`, `libp2p-yamux` `0.48.0` | — | security and muxer upgrades (mandatory for relay, §5.3) | as above | MIT | as above |
| `libp2p-gossipsub` | `0.50.0` | lobby topics (§6) | as above | MIT | as above |
| `libp2p-kad` | `0.49.0` | the lobby rendezvous and the relay namespace on the public DHT (§3, §11.4) | as above | MIT | as above |
| `libp2p-identify` `0.48.0`, `libp2p-ping` `0.48.0` | — | address candidates, liveness (§5.5) | as above | MIT | as above |
| `libp2p-autonat` `0.16.0`, `libp2p-dcutr` `0.15.0`, `libp2p-relay` `0.22.0` | — | reachability, hole punching, relay (§9) | as above | MIT | as above |
| `libp2p-request-response` | `0.30.0` | snapshot RPC (§7), join RPC (§8.4) | as above | MIT | as above |
| `libp2p-mdns` `0.49.0`, `libp2p-upnp` `0.7.0` | — | LAN discovery (§9.8), IGD mapping (§9.9) | as above | MIT | as above |
| `libp2p-connection-limits` `0.7.0`, `libp2p-memory-connection-limits` `0.6.0`, `libp2p-allow-block-list` `0.7.0` | — | resource limits, and the **user-populated** block list of §11.5 — never populated by a protocol proof (§0.1) | as above | MIT | as above |
| `libp2p-metrics` | `0.18.0` | enabled as the umbrella's `metrics` feature; nothing in `src/` reads it | as above | MIT | as above |
| **`libp2p-stream`** | **`0.5.0-alpha`** | per-table streams (§8.1) | as above | MIT | **unaudited and semver-exempt.** An alpha crate carries no stability guarantee; contained behind the §1.3 trait so replacing it is a one-file change |
| ~~`mainline`~~ | ~~`=8.0.0`~~ | **Removed in `56b0b50`.** The row is kept because the reason for the `=` pin is worth keeping: §11.4 depended on crate internals (`RequestFilter`, adaptive server mode) that are not semver-stable. `libp2p-kad` replaces it and this client pins none of its internals — the one thing §11.4 now depends on, `set_mode`, is public API. | — | — | — |
| `web-time` | `1.1.0` | `Instant` in the `RateLimiter` signature (§9.6). In the lock through libp2p and not a direct dependency: the named rate limiter §9.6 specifies is not built | `github.com/daxpedda/web-time` | MIT OR Apache-2.0 | no advisory in the local advisory database |

The two entries a reader must not skip are **`libp2p-stream 0.5.0-alpha`** here
and **`ziffle 0.1.0`** in `CRYPTOGRAPHY.md` §9: those are the corpus's two
unaudited, semver-unstable dependencies, and they are flagged explicitly in both
places.

> Verification: [SOURCE] versions read from `Cargo.lock` on 2026-09-15, and
> `license` and `repository` fields from each crate's own `Cargo.toml` at that
> version under `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`;
> advisories from the local RustSec database under `~/.cargo/advisory-db/crates/`
> (updated 2026-09-14), where nothing matches a version in this table. These are
> per-crate lookups. **The tree audit of record is `DEPENDENCIES.md` §4**, the
> complete `cargo audit` result; this note said until 2026-09-15 that no audit had
> been run because the tree and `Cargo.lock` did not exist yet, which stopped being
> true long before.

### 5.2 Features

```toml
libp2p = { version = "0.57.0", features = [
    "tokio", "macros",
    "quic", "tcp", "noise", "tls", "yamux", "dns",
    "gossipsub", "kad", "identify", "ping", "autonat", "dcutr", "relay", "mdns",
    "request-response", "cbor",
    "upnp", "metrics", "memory-connection-limits",
    "ed25519",
    "rsa",
    "serde",
] }
libp2p-stream = "0.5.0-alpha"
```

This is `Cargo.toml`'s block as it stands, copied on 2026-09-15; until then this
section showed the 0.56 block without `metrics` and `rsa`. **`rsa` is needed and
used for nothing else:** the public entry points onto the libp2p network still
have `Qm…` peer ids, multihashes of RSA public keys, and without the feature their
TLS certificate cannot be verified — which fails as *invalid peer certificate:
UnknownIssuer* and reads like a broken certificate. **`metrics`** is enabled and
nothing in `src/` reads it (§5.1.1).

`kad` **is** enabled, and it is the discovery mechanism (§5.7). Until 2026-09-15
this block left it out, said *"`kad` is not enabled in v1"* and carried a
commented-out `mainline = "=8.0.0"` line; that crate left the build in `56b0b50`.

### 5.3 Why QUIC first and TCP anyway

QUIC is the primary transport: UDP (so hole punching is possible at all — and
`libp2p-quic 0.14.0` has a real `hole_punching` module wired into the transport,
`src/lib.rs:65` and `src/transport.rs:324` [SOURCE]), 1-RTT handshake, native stream multiplexing with no head-of-line
blocking across per-table streams.

TCP stays in the build for three structural reasons, not as a preference:

1. UDP is blocked outright on many corporate, hotel and school networks. A
   QUIC-only client there reaches nothing, not even a relay, because the relay
   itself must be dialled over a base transport.
2. It costs almost nothing: `noise`, `tls` and `yamux` are **already mandatory**
   because `with_relay_client` needs a security upgrade and a muxer upgrade even
   when QUIC is the only base transport — a relayed connection is a stream through
   the relay, not a QUIC connection. This was proven by removing `yamux` from a
   QUIC-only probe and watching the build fail with
   `could not find yamux in libp2p … found an item that was configured out`.
3. Peers learned from the DHT or from a relay may be listening only on TCP.

~~Note honestly: TCP does not rescue discovery, because the Mainline DHT is UDP.~~ **That stopped being true in `56b0b50`.** Kademlia is a behaviour on this same swarm, so it rides QUIC *and* TCP: a UDP-blocked network now keeps its discovery as well as its transport. What TCP does not rescue is *playing* against another unreachable peer, which needs §9.
§12 item 2 records the same correction.

> Verification: [RESEARCH+COMPILED] `LIBP2P.md` §9 (both the failing and passing
> states of the QUIC-only probe), §5 (`hole_punching` module).

### 5.4 The builder shape

Compiled and executed as written in `probe-netstack`:

```rust
let swarm = SwarmBuilder::with_existing_identity(keypair)
    .with_tokio()
    .with_tcp(
        tcp::Config::default().nodelay(true),
        (tls::Config::new, noise::Config::new),   // function pointers, not values
        yamux::Config::default,
    )?
    .with_quic_config(|mut c| {
        c.handshake_timeout   = Duration::from_secs(10);
        c.max_idle_timeout    = 30_000;           // u32 milliseconds
        c.keep_alive_interval = Duration::from_secs(5);
        c
    })
    .with_dns()?
    .with_relay_client((tls::Config::new, noise::Config::new), yamux::Config::default)?
    .with_behaviour(|key, relay_client| { /* §5.5 */ })?   // 2 args because of relay
    .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
    .build();
```

Two traps the type-state builder sets: the upgrade arguments are **function
pointers** (`noise::Config::new`, not `noise::Config::new()`), and once
`with_relay_client` is used the `with_behaviour` closure takes a **second
argument** — the ready-made `relay::client::Behaviour` — which must be stored in
the behaviour struct or the relay transport is dead.

`with_idle_connection_timeout` is set to 60 s (`IDLE_CONNECTION_TIMEOUT_MS`)
deliberately: the default is 10 s (`libp2p-swarm-0.48.0/src/connection/pool.rs:1018`)
and silently closes lobby connections that are merely quiet. `ping` keeps
NAT mappings warm; consumer-router UDP mappings commonly expire in 30–120 s, so the
keep-alive interval must stay well under 30 s and `ping`'s default is suitable —
do not raise it.

### 5.5 The behaviour struct

```rust
#[derive(NetworkBehaviour)]
struct PokerBehaviour {
    gossipsub:      gossipsub::Behaviour,                     // §6
    identify:       identify::Behaviour,                      // prerequisite for AutoNAT v2 + DCUtR
    ping:           ping::Behaviour,                          // liveness, keep-alive
    autonat_client: autonat::v2::client::Behaviour,           // §9.2
    autonat_server: autonat::v2::server::Behaviour,
    dcutr:          dcutr::Behaviour,                         // §9.4
    relay_client:   relay::client::Behaviour,                 // from the builder
    relay_server:   relay::Behaviour,                         // §9.6, off by default
    snapshot:       request_response::cbor::Behaviour<SnapshotRequest, SnapshotResponse>,  // §7
    join:           request_response::cbor::Behaviour<JoinRequest, JoinResponse>,          // §8.4
    table_streams:  libp2p_stream::Behaviour,                 // §8
    conn_limits:    connection_limits::Behaviour,             // §11
    mem_limits:     libp2p::memory_connection_limits::Behaviour,
    // §11.5. Named `user_blocklist`, not `blocklist`, because the name is the
    // constraint: the ONLY thing that may call `block_peer` on it is an
    // explicit user action in the GUI. No protocol proof reaches it (§0.1).
    user_blocklist: libp2p::allow_block_list::Behaviour<libp2p::allow_block_list::BlockedPeers>,
    mdns:           libp2p::mdns::tokio::Behaviour,           // §9.8
    upnp:           libp2p::upnp::tokio::Behaviour,           // best effort
}
```

`#[derive(NetworkBehaviour)]` generates `PokerBehaviourEvent` with one variant per
field, named in PascalCase of the field name (`relay_client` → `RelayClient`).

**`identify` is a hard prerequisite, not decoration.** The address-candidate chain
is: `identify` receives the remote's `observed_addr` and emits
`ToSwarm::NewExternalAddrCandidate`; the swarm turns that into
`FromSwarm::NewExternalAddrCandidate`; `autonat::v2::client` consumes it, probes
it, and on success emits `ToSwarm::ExternalAddrConfirmed`; `dcutr` consumes the
same event to fill its LRU(20) of hole-punch candidates. Without `identify` in the
behaviour, neither AutoNAT v2 nor DCUtR ever sees a candidate and both are inert.

**Which address `identify` offers changed with `libp2p-identify 0.48.0`** (its
changelog's *"check actual port reuse instead of original intent"*). In 0.47.0 an
observed address was translated to a listening port only for an outbound
connection dialled from an ephemeral port, and offered raw otherwise, inbound
connections included. In 0.48.0 it is offered raw when its port matches one this
node listens on — and not at all if it is already a confirmed external address —
and otherwise translated to each listening port of the same transport, raw only
if translation yields nothing (`src/behaviour.rs:350-395`). The chain above is
unchanged; what feeds AutoNAT's probes and DCUtR's LRU is, for inbound
connections especially.

**`libp2p::autonat::Behaviour` silently means v1.** The crate re-exports v1 at the
module root. Only `autonat::v2::*` paths may appear in our source; enforce it as a
review rule.

> Verification: [COMPILED+RUN] `probe-netstack` builds exactly this struct with
> every field constructed as specified, subscribes both lobby topics, registers the
> table stream protocol, and binds both listeners. `cargo check` finishes clean;
> `cargo run` prints `ALL OK`. [SOURCE] the candidate chain, `LIBP2P.md` §3 with
> file:line references into `libp2p-identify-0.47.0`, `libp2p-autonat-0.15.0` and
> `libp2p-dcutr-0.14.1` — the probe's versions, and the research note's line
> numbers stay theirs. Re-read on 2026-09-15 against `libp2p-identify-0.48.0`
> (`src/behaviour.rs:350-395`, the change above), `libp2p-autonat-0.16.0` and
> `libp2p-dcutr-0.15.0` (`src/behaviour.rs:341-342` feeds `address_candidates`,
> an `LruCache::new(20)` at `:364`): the chain holds in all three.

### 5.6 Persistent identity, and the publish filter

`identity::Keypair::to_protobuf_encoding() -> Result<Vec<u8>, DecodingError>` and
`from_protobuf_encoding(&[u8])` are the persistence pair; a round trip preserves
the `PeerId` (68 bytes for Ed25519, measured). Two installations must never share a
keypair (`SPEC_CS.md` §3); the file lives in the portable profile directory beside
the executable and is DPAPI-wrapped on Windows (`SPEC_CS.md` §21, §22).

**Publish filter** (distinct from the dial filter of §4.5): before an address is
announced to the DHT or advertised in the lobby it must be a globally routable
address. AutoNAT v2 has **no** private-address guard — `only_global_ips` exists in
v1 and has no v2 equivalent, and v2 was measured confirming
`/ip4/192.168.1.21/udp/55598/quic-v1` as an external address because the dial-back
came from a LAN peer [MEASURED]. So the filter is ours to implement, and it must
also drop known-virtual adapter ranges: a Hyper-V "Default Switch" address was
measured being advertised, dialled, and burning a full handshake timeout
[MEASURED].

> Verification: [COMPILED+RUN] keypair round trip in `probe-netstack`
> (`persistent identity round-trip OK (68 bytes)`); [SOURCE]
> `libp2p-identity-0.3.0/src/keypair.rs:215, 257` (the pair, unchanged in
> signature and line from the `0.2.14` the probe used); [MEASURED]
> `NAT_AND_DISCOVERY.md` §2.4(ii), §5.3.

### 5.7 libp2p Kademlia is the discovery mechanism

**This section said the opposite until 2026-09-02, and it was the most misleading
sentence in the file.** It read *"libp2p Kademlia is not enabled in v1"* and gave
three reasons — `SPEC_CS.md` §1 names Mainline, a second DHT is a second eclipse
surface, and it has no mandated role. `56b0b50` removed the `mainline` crate and
put discovery on `kad`; `Cargo.toml` enables the feature and `net::swarm` builds
**two** `kad::Behaviour`s. An implementer reading the old text would have built a
client that cannot find ours and would have had a documented reason for it.

Two behaviours, and only one of them runs:

* **`ipfs_kad`** — the public Amino DHT, the same network Kubo and go-libp2p are
  on. It is what `start_providing` and `get_providers` are called against, and
  it is the lobby. §3 specifies the key.
* **`kademlia`** — a private table under `/p2p-poker/kad/1` (`swarm::PokerBehaviour::kademlia`). It
  is constructed and **never driven**: no `set_mode`, no query, no record. It
  costs a behaviour slot and does nothing, and this document says so rather than
  leaving a reader to infer a design from a field name.

The old section's *"peer-set resilience comes instead from"* list is still true
and still load-bearing, because a DHT lookup is one source among four: the
addresses `identify` carries from already-connected peers, mDNS on the LAN
(§9.8), the GossipSub mesh itself, and the peer book on disk
(`net::peerbook`) which the old text predates entirely.

Its remaining concern is real and is not answered by having shipped: **a second
DHT is a second eclipse surface**. What changed is that the alternative was
measured and was worse — a Mainline announce can carry one `IP:port`, and a
player behind a NAT has none worth announcing, so the discovery mechanism
`SPEC_CS.md` §1 names cannot make a NATed player findable at all.

### 5.8 IPv6

**The reason this section gave died with the `mainline` crate, and the
conclusion outlived it by two days.** It read: *"`mainline`'s `KrpcSocket::new`
does `unimplemented!("KrpcSocket does not support Ipv6")` … so the libp2p layer
may be dual-stack, but discovery is IPv4-only."* That crate was deleted in
`56b0b50`. libp2p's QUIC and TCP transports are both dual-stack, and Kademlia
now rides them, so nothing about discovery is IPv4-only any more.

**But the client was, and for a different reason nobody had written down.**
`run::listen_addrs` returned exactly two addresses, `/ip4/0.0.0.0/udp/<p>/quic-v1`
and `/ip4/0.0.0.0/tcp/<p>`, so no IPv6 socket was ever bound. The limitation had
outlived its cause and become a plain omission: a player on an IPv6-only network
was unreachable by anybody, and so was a player whose ISP hands out CGNAT on v4
and a routable address on v6 — the case this is worth most to.

**Fixed 2026-09-02.** `run::listen_addrs_v6` adds `/ip6/::/udp/<p>/quic-v1` and
`/ip6/::/tcp/<p>` on the same port number, and a failure to bind either is
**reported, never fatal**: a host with no IPv6 stack is ordinary and must still
be able to play. Measured on the development machine the same day: *"listening
on IPv6 as well as IPv4 (2 of 2 transports)"*, with both a `fdc9:…` ULA and
`::1` bound [MEASURED]. That machine still has no **global** IPv6, so what is
proved here is that the listeners bind and the client survives them — reachability
over a routable v6 address is untested and is `S1-Y`.

One thing had to change with it. `run::reachable` — the filter that decides
whether an address is worth offering as a relay endpoint or publishing as an
external address — tested only loopback and `::` on the IPv6 side, so `fc00::/7`
read as publicly dialable. That was inert while nothing bound a v6 socket and
stopped being inert in the same commit that did, so the v6 arm now excludes
unique-local, link-local, multicast and the documentation range, which is what
the v4 arm always did.

---

## 6. The GossipSub lobby

### 6.1 Topics

Two topics, named by the constants `LOBBY_TOPIC` and `LOBBY_CHAT_TOPIC`; the
strings themselves are two-sided and live in `PROTOCOL.md` §13.

| Topic (`IdentTopic`) | Carries | Publisher | Cadence |
|---|---|---|---|
| `LOBBY_TOPIC` | `LOBBY_TABLE_AD`, `LOBBY_TABLE_REMOVE`, player presence heartbeat | table founder / each client | ad every `AD_REBROADCAST_MS`, presence every `PRESENCE_HEARTBEAT_MS` (§10) |
| `LOBBY_CHAT_TOPIC` | lobby chat (`SPEC_CS.md` §22 requires chat) | any client | user-driven, rate-limited |

Chat is a **separate topic** so that chat volume can never crowd out table
discovery, so it can be muted without losing the lobby, and so its scoring and rate
limits are independent. `IdentTopic` (topic string on the wire in the clear) is
used because the strings are public constants anyway and debuggability is worth
more than a hash here.

Per-table traffic is **never** on a lobby topic (§8.3).

### 6.2 Configuration

```rust
gossipsub::ConfigBuilder::default()
    .validation_mode(gossipsub::ValidationMode::Strict)
    .validate_messages()
    .message_id_fn(content_hash_of(data, topic))
    .max_transmit_size(GOSSIP_MAX_TRANSMIT)          // = the crate default; see below
    .heartbeat_interval(Duration::from_secs(1))
    .mesh_n(8).mesh_n_low(6).mesh_n_high(12).mesh_outbound_min(3)
    .duplicate_cache_time(Duration::from_secs(120))
    .flood_publish(false)
    .build()?
```

| Setting | Value | Why |
|---|---|---|
| `MessageAuthenticity` | `Signed(libp2p keypair)` | every message carries author + seqno + signature |
| `validation_mode` | `Strict` | reject anything unsigned or missing author/seqno |
| `validate_messages()` | on | **mandatory**: nothing is forwarded until the application has validated it (`SPEC_CS.md` §4 anti-spam) |
| `message_id_fn` | hash of `(data, topic)` | **mandatory**: the default id is `source ‖ seqno`, so one peer can republish byte-identical content forever under new seqnos and it is never deduplicated |
| `max_transmit_size` | `GOSSIP_MAX_TRANSMIT` (`PROTOCOL.md` §13) | two-sided — a peer configured lower rejects a larger frame outright — so it is **not** local tuning and its value is not written here. It is pinned at the crate default, which maximises interoperability and minimises the DoS surface. Nothing we publish comes near it (§6.5). |
| `mesh_n / low / high` | 8 / 6 / 12 | denser than the 6/5/12 default; the lobby is small and latency-sensitive |
| `mesh_outbound_min` | 3 | raises the cost of an eclipse by inbound-only Sybils |
| `duplicate_cache_time` | 120 s | local; must exceed `AD_REBROADCAST_MS` with margin |
| `flood_publish` | **false** | the default `true` sends every publish to *every* known peer in the topic, not just the mesh — an amplification lever we do not want on a public lobby |

Subscription flooding is bounded with
`Behaviour::new_with_subscription_filter` and a `MaxCountSubscriptionFilter` /
`WhitelistSubscriptionFilter` over our two topic names, so a peer cannot subscribe
us into thousands of junk topics.

> **What runs is not that, and the crate's default now does most of it (noted
> 2026-09-15).** `net::swarm` builds `gossipsub::Behaviour::new`, whose filter type
> defaults to `MaxCountSubscriptionFilter<AllowAllSubscriptionFilter>`. From
> `libp2p-gossipsub 0.50.0` that default caps a peer at **100** subscribed topics
> and **100** subscriptions per request, down from **2000** and **2000** in
> `0.49.5` (`src/subscription_filter.rs:169-170` in both); a peer over the cap has
> its whole subscription message ignored. A whitelist over two names would now be
> wrong as well as unbuilt: the lobby's slice topics (`S1-EX`,
> `/p2p-poker/lobby/1/<slice>`) are topics of ours it would refuse.

> Verification: [COMPILED+RUN] the whole builder chain plus `subscribe` on both
> topics in `probe-netstack`; [RESEARCH+SOURCE] `LIBP2P.md` §6 for the defaults and
> the `message_id_fn` trap.

### 6.3 Two signatures, never one

GossipSub's `Strict` mode authenticates **the libp2p identity keypair that owns
the `PeerId`** — i.e. *which socket said this*. `SPEC_CS.md` §12 requires a
**separate application signing keypair** over canonically serialised bytes, and
`SPEC_CS.md` §4 requires the table advertisement itself to be signed. These are two
independent signatures and both are required. `ValidationMode::Strict` must never
be allowed to create the impression that a message is application-authentic.

Concretely: a peer relaying somebody else's validly-signed table ad is normal and
correct — the gossip signature will be the *forwarder's*, and the application
signature will be the *founder's*. Validation looks at the application signature.

### 6.4 Validation and the accept/reject decision

**The predicates a receiver checks are `PROTOCOL.md` §7.2's and this section
reproduces none of them** (D-011 rule 1). What this section owns is the
transport-side question no other document answers: *where in the GossipSub
pipeline the check runs, and what its outcome does to the mesh.* An earlier
revision listed the predicates here as an eight-step checklist; it was a copy,
and a copy is what drifts.

Every received lobby message runs, before anything is forwarded:

1. **the transport's own size gate** — over-size is rejected by
   `max_transmit_size` before the application sees it, and the application
   re-checks the per-type cap that applies to this message (§6.5, §11.3);
2. **`PROTOCOL.md` §7.2's validation checklist in full**, in that section's
   order: canonical decode, version, envelope, signature, signer identity, and
   the timestamp and `expires_at` rules. Two of its steps carry a transport
   consequence worth naming here and nowhere else:
   * the envelope must be **unchained** — `chain_scope == 0` with §2.3's
     sentinels. Every lobby message type is unchained, so a chained envelope on
     a lobby topic is either a bug or an attempt to make lobby traffic collide
     with the chain namespace `PROTOCOL.md` §5.2's equivocation predicate is
     defined over. The topic is not a namespace the predicate reaches, and the
     transport must not let one become the other;
   * the `timestamp` and `expires_at` tests are **local-clock tests**, so their
     outcome is a per-receiver quantity. That is admissible here and only here:
     lobby traffic is unchained and the view it feeds is local (§0.5.4). No
     clock-derived test may be applied to table-stream traffic;
3. **per-peer, per-type rate budget** not exceeded (§6.6). This is ours: it is a
   fact about our own budget, not about the message.

Then:

```rust
gossipsub.report_message_validation_result(&message_id, &propagation_source, acceptance);
```

| Outcome | `MessageAcceptance` | Effect |
|---|---|---|
| all checks pass | `Accept` | forwarded to the mesh |
| signature invalid, schema invalid, expired, version mismatch, over cap | `Reject` | not forwarded; applies the topic P₄ invalid-message penalty **if** topic score params are configured (§6.7) |
| valid but uninteresting (duplicate we already hold, table we already have at a newer timestamp) | `Ignore` | not forwarded, no penalty |

### 6.5 Size caps

**The numbers live in `PROTOCOL.md` §13 and are not reproduced here** (D-011
rule 1; `PROTOCOL.md` §13 says the same from its side). What this section owns is
*which* cap the transport applies *where*, and why each one bounds the object it
does — the part an implementer of `net/lobby.rs` needs and the part no other
document carries.

| Message | Constant (`PROTOCOL.md` §13) | What the cap bounds, and why |
|---|---|---|
| `LOBBY_TABLE_AD` | `TABLE_AD_MAX` | the **payload** only. The fields of `SPEC_CS.md` §4 plus a custom-parameter block total under 500 B at worst case, so the constant carries over 2× headroom |
| a **complete signed** `LOBBY_TABLE_AD` | `TABLE_AD_SIGNED_MAX` | payload + envelope + the 64-byte signature. This is the cap applied wherever a whole signed advert is **forwarded or embedded** rather than freshly parsed: the snapshot elements of §7.3, and `JOIN_ACCEPT`'s `advert_event` (`PROTOCOL.md` §4.3). The two caps bound different objects, and conflating them is what produced the 2 560 / 1 024 conflict `PHASE0_REVIEW.md` C-1 found — which is also why neither number is written twice any more |
| `LOBBY_TABLE_REMOVE`, presence heartbeat | — | small fixed-shape bodies; no separate constant, bounded by `LOBBY_MSG_MAX` |
| lobby chat message | `LOBBY_CHAT_MAX` | payload cap. The message type is `PROTOCOL.md` §4/§7's `0x0106 LOBBY_CHAT`; its `display_name` and `text` are display-only strings bounded by `PROTOCOL.md` §9.4's string rules |
| any lobby message | `LOBBY_MSG_MAX` | hard ceiling; anything larger is a protocol error regardless of type |

The application ceiling sits far below the `GOSSIP_MAX_TRANSMIT` transport
ceiling, so the transport limit is a backstop and never the operative limit. A
lobby snapshot does **not** go over gossip — see §7.

### 6.6 Rate limits (application-level, `SPEC_CS.md` §4)

Per remote `PeerId`, token buckets refilled continuously:

| Budget | Rate | Burst |
|---|---|---|
| table ads (any table) | 1 per 10 s | 4 |
| distinct `table_id`s advertised by one identity | 4 concurrently | — |
| presence heartbeats | 1 per 20 s | 2 |
| chat messages | 1 per 2 s | 5 |
| total lobby bytes from one peer | 8 KiB/s | 64 KiB |

Exceeding a budget produces `Ignore` for the excess (not `Reject` — the message may
be perfectly valid, we simply refuse to spend on it), and repeated excess
downgrades the peer locally: stop dialling it, then disconnect.

**That escalation is backpressure, and it is the whole of it.** It is driven by
one fact — this socket is spending more of our budget than we allot it — and by
nothing else. It is not recorded, not persisted, not shown as a sanction, and the
peer may reconnect immediately; a peer that stays inside its budget after
reconnecting is treated exactly like any other (§0.2).

**A proven protocol violation produces no transport action at all.** An earlier
revision of this section escalated an invalid application signature or an
`EquivocationProof` (`PROTOCOL.md` §5.2) into `block_peer`, and D-010 point 3 and
D-011 rule 3 delete that path outright. An invalidly signed message is dropped as
a *malformed message* by §6.4 — which is a size-and-shape fact about the bytes,
not a verdict about the sender — and nothing further follows. An
`EquivocationProof` is evidence for a human and reaches this layer not at all:
the transport neither constructs one, consumes one, nor changes any behaviour on
seeing one. The lobby founder contradiction of §7.4 was never such a proof and
likewise produces nothing here.

### 6.7 Peer scoring — honest status

`Behaviour::with_peer_score(PeerScoreParams, PeerScoreThresholds)` exists and is
callable; `set_topic_params` installs per-topic parameters
[SOURCE `libp2p-gossipsub-0.50.0/src/behaviour.rs:1051,1083`, lines 922 and 954 in
the `0.49.5` the probe used]. It compiles and runs with the defaults in
`probe-netstack`.

But **the defaults do almost nothing for us**: `PeerScoreParams::default()` has
`topics: HashMap::new()` [SOURCE `src/peer_score/params.rs:164-183`, byte-identical in `0.50.0`], so the
per-topic terms — including P₄, `invalid_message_deliveries_weight`, the term a
`Reject` actually feeds — contribute **zero** until a `TopicScoreParams` is
installed for each topic. Only `app_specific_weight`, IP-colocation
(`-5.0` above 10 peers per IP) and the behaviour penalty are live by default.

So: scoring is enabled from day one with defaults, which buys IP-colocation and
behaviour penalties, and the §6.6 application rate limits are what actually
controls spam in v1. Deriving safe `TopicScoreParams` for our topics requires
knowing real message rates and mesh sizes, which we do not have. Getting them wrong
is worse than not setting them — badly tuned scoring graylists honest peers.
Carried as OQ-5, to be set in Phase 8 against measured traffic.

---

## 7. Snapshot sync for a newly joined client

`SPEC_CS.md` §3: *a newly connected client requests a snapshot of already-existing
tables from several peers over libp2p, and only then continues with live GossipSub
synchronisation.*

### 7.1 Mechanism

`request_response::cbor::Behaviour<SnapshotRequest, SnapshotResponse>` on
`SNAPSHOT_PROTOCOL` (`PROTOCOL.md` §13). This is a genuine one-shot RPC: timeouts,
correlation and size caps come free, and CBOR aligns with the canonical
serialisation of `SPEC_CS.md` §12. It is **not** a gossip message, for two reasons:
a lobby with hundreds of tables blows past any sane gossip frame, and a snapshot is
addressed to one peer, not broadcast.

```rust
let codec = request_response::cbor::codec::Codec::<SnapshotRequest, SnapshotResponse>::default()
    .set_request_size_maximum(SNAPSHOT_REQ_MAX)    // values: PROTOCOL.md §13
    .set_response_size_maximum(SNAPSHOT_RESP_MAX)
let snapshot = request_response::Behaviour::with_codec(
    codec,
    [(SNAPSHOT_PROTOCOL, request_response::ProtocolSupport::Full)],   // string: PROTOCOL.md §13
    request_response::Config::default()
        .with_request_timeout(Duration::from_secs(15))
        .with_max_concurrent_streams(8),
);
```

The size caps are **mandatory overrides**: the codec defaults are 1 MiB request /
10 MiB response [SOURCE `libp2p-request-response-0.30.0/src/cbor.rs:77-78`], far too
generous for a lobby snapshot and a free memory-exhaustion lever. The request
payload is ~45 B and capped by `PROTOCOL.md` §9.3, so `SNAPSHOT_REQ_MAX` is
envelope headroom and nothing more; slack in a request cap is DoS surface, not
safety margin.

> Verification: [COMPILED+RUN] exactly this construction, for both the snapshot and
> the join protocol, in `probe-netstack`; [SOURCE] `with_codec` at
> `libp2p-request-response-0.30.0/src/lib.rs:395`, the setters at `src/cbor.rs:96,102`
> (`0.29.0`: 395 and 97, 103). One change in `0.30.0` touches a custom codec only:
> the `Codec` trait no longer uses `#[async_trait]` and its methods return
> `impl Future` (`src/codec.rs:38-69`); the `cbor` codec above is the crate's own
> and is used unchanged.

### 7.2 Who is asked, and how many

* **K = 4** peers, with a floor of 2 usable responses to consider the snapshot done.
* Chosen from peers that have completed `identify` and advertise `/p2p-poker/1`.
* Prefer peers in **distinct IPv4 /24 prefixes** and, where known, discovered from
  **different DHT responders** — a cheap and partial defence against being handed
  four Sybils by one flooder. **Neither half is implemented, and the first can no
  longer be implemented here as written**: this client never sees a provider's
  addresses, so it has no prefix to compare (§4.5). The second still could be —
  `FoundProviders` is emitted once per responding node, so multiplicity is
  observable — but nothing records it. `S1-AC` covers the same gap on the dial
  path.
* Ask all K concurrently. 15 s timeout each.
* If fewer than 2 respond, retry once with a fresh set, then continue on live
  gossip alone and show "lobby syncing" in the GUI. Never block the UI.
* Re-run a snapshot after any period of >5 minutes with zero lobby traffic, and
  after reconnecting from a full disconnect.

### 7.3 What a snapshot contains

**The request and response bodies are `LOBBY_SNAPSHOT_REQUEST` and
`LOBBY_SNAPSHOT_RESPONSE`, defined in `PROTOCOL.md` §7.5, and their fields are
not reproduced here** (D-011 rule 1). An earlier revision of this line carried a
two-field struct of its own invention which had already fallen behind that
definition in both directions — it invented a `protocol_version` field the wire
does not carry and omitted the `request_nonce` and `truncated` fields it does.
That is precisely the drift D-011 exists to delete, and it is why the shape is
now a pointer.

What this document owns is the transport behaviour around it. The response
carries **complete, independently signed `LOBBY_TABLE_AD` events exactly as they
were gossiped** — the responder forwards the original signed bytes and never
re-serialises, re-signs or summarises them.

**The reason this rule was given has changed, and the rule has not.** It used to
be that a re-serialised advert has a different `event_hash` and that the hash of
the advert was load-bearing downstream. Since D-013 it is not: `advert_hash`
enters no chained hash at all (§0.5.6). The rule survives on two grounds that were
always the stronger ones and are now the only ones. **First, the responder cannot
re-sign.** It does not hold the table key, so any byte it changes destroys the
founder's signature and the ad is rejected by the receiver's own `verify_strict`
— a responder that re-serialises is a responder whose entire snapshot is
discarded. **Second, the parameters must survive the hop intact**, because the
joiner recomputes `table_params_hash` from this very object, and that value *is*
load-bearing downstream. A responder that summarised an ad — dropping a field it
thought was display-only — would hand the joiner a value no other participant
derives, which is §0.5.6's defect re-created one layer out. **Forwarding is not an
optimisation here; it is the only thing a responder is permitted to do.**

Four caps apply, all valued in `PROTOCOL.md` §13:
`SNAPSHOT_MAX_ADS` on the count, `SNAPSHOT_RESP_MAX` on the total,
`TABLE_AD_MAX` on each ad's **payload**, and `TABLE_AD_SIGNED_MAX` on each
**complete signed** ad. The count and the total were chosen together and the
relation between them is this document's, so it is stated here rather than the
numbers: **`SNAPSHOT_MAX_ADS × TABLE_AD_SIGNED_MAX` must fit inside
`SNAPSHOT_RESP_MAX` with room left for the array and envelope overhead.** Any
future change to one of the three has to preserve that inequality.

A snapshot carries **no game state**, no hand history, no player secrets, no
cryptographic material. It is a set of table advertisements and nothing else.

### 7.4 Conflict resolution

1. **Every ad is validated independently**, by the rules of §6.4. An ad's validity
   is a property of the ad, never of who sent it.
2. **Union, not vote.** The result is the union of all valid ads from all
   responders. An ad reported by one peer is exactly as valid as one reported by
   all four. *Counting is never evidence.*
3. **Per `table_id`, keep the validly signed ad with the highest `timestamp`.** An
   ad with a lower `timestamp` for a `table_id` we already hold is discarded, which
   also blunts replay of stale ads (`SPEC_CS.md` §14).
4. **Two different validly signed adverts from the same `table_public_key` with
   the same `timestamp_unix_ms` and different bodies are a founder
   contradiction.** The table is marked `EQUIVOCATION`, both are retained as
   evidence, and it is never joinable. This is **not** an `EquivocationProof` in
   the `PROTOCOL.md` §5.2 sense — lobby messages are unchained
   (`chain_scope = 0`, §6.4), and §5.2's predicate is defined only over chained
   events — and it **must not be called one**. It is a local lobby-hygiene rule
   and its entire consequence is that **we decline to display one
   self-contradictory table as joinable.** It is not eviction and produces none:
   the founder is not disconnected, not refused a connection, not recorded and
   not block-listed, and nothing about any *other* table it advertises changes.
   Refusing to act on data that contradicts itself is not a verdict about the
   peer who sent it. It attributes nothing under D-005, and under D-010 there
   would be nothing for an attribution to cause even if it did. The meaning it
   does have comes from the *founder's own two signatures*, not from any count of
   peers.
5. **An ad signed by any key other than its `table_public_key` is discarded.**
6. Local **expiry of lobby entries** then proceeds by §10.3 (relative freshness),
   not by what a snapshot said. This expires an advert, never a peer.

**The merge result is a local view, in D-012's sense, and nothing else.** Every
input to it is per-receiver: which K peers we happened to ask, which of them
happened to answer, what each happened to hold, and where our own clock happened
to be. So no part of the result may enter a state hash, a roster hash, a chained
event body or a hand's genesis (§0.5.2, §1.2 prohibition 8). Concretely, none of
these is canonical: the set of tables we display, the ordering we display them
in, whether we are still waiting on responders, the `EQUIVOCATION` mark of rule 4
— which never leaves this client — and **which copy of a founder's re-broadcast
advert we currently hold**. That last one was the site where the corpus did not
hold the line; it is closed, and the reason it is worth naming even now is that
the copy we hold **is still read**, to recompute a value from it. What changed is
that the value recomputed no longer varies between copies (§0.5.6).

Rule 2 is the same rule stated for the merge itself, and it was already right
before D-012: *counting is never evidence*. An ad reported by one responder is
exactly as valid as one reported by four, because validity comes from the
founder's signature and not from a tally that two honest clients would take
differently. A merge that weighted by responder count would be manufacturing a
per-receiver quantity and then believing it.

### 7.5 Why a snapshot peer cannot become an authority

A responder's only powers are **omission** and **delay**. It cannot:

* create a table ad — it lacks the founder's application key;
* alter a table ad — the signature covers the canonical bytes;
* revoke a table ad — a `LOBBY_TABLE_REMOVE` must be signed by the founder, and a
  table also disappears by TTL with no cooperation at all (§10.3);
* make us prefer its ads — nothing in the merge is weighted by sender;
* learn anything about a hand — snapshots carry no game state.

Omission is bounded by asking K independent peers and by live gossip converging
within seconds afterwards. It is **not eliminated**: a client whose entire peer set
is controlled by one adversary sees whatever that adversary chooses to show. That
is an eclipse attack, it is a **liveness and visibility** failure rather than an
integrity failure — the adversary still cannot forge a table or a hand — and it is
**not solved** at this layer (§12).

That distinction is exactly what D-012 protects, and it is only sound while the
snapshot result stays a local view. The moment any part of the merge became
canonical, a responder's power would stop being omission and delay and start
being **fork**: it would only have to show two clients different things to make
them derive different state, which is a far cheaper attack than the eclipse
above and needs no control of the peer set at all. §7.4's closing rule is what
keeps that shut. **The one place where the corpus had not finished shutting it is
now shut** (§0.5.6), and the shape of that fix is worth carrying into any future
merge rule: it was closed by making the canonical value **the same in every copy**,
not by making every client hold the same copy. A convergence rule at this layer
would have been the wrong fix, and this document was right not to invent one.

---

## 8. Per-table transport

`SPEC_CS.md` §1: *the poker game state is sent only between the participants of
that table, over direct libp2p streams, never over the Mainline DHT.* The DHT the
assignment names is history (the note at the head of this document); the rule
binds the public Kademlia that replaced it, and §8.3 applies it.

### 8.1 Mechanism

`libp2p-stream` on `TABLE_PROTOCOL` (`PROTOCOL.md` §13): long-lived, bidirectional, both
sides push. `request-response` cannot express server push and one substream per
event would be wasteful, and a hand-written `NetworkBehaviour` +
`ConnectionHandler` is explicitly not warranted (`SPEC_CS.md` §2 asks for a minimal
addition over an existing crate, not a new protocol).

```rust
// TABLE_PROTOCOL is a two-sided constant; its string lives in PROTOCOL.md §13
// and is declared once, in the crate's `constants` module.
use crate::constants::TABLE_PROTOCOL;   // StreamProtocol

let mut control  = swarm.behaviour().table_streams.new_control();   // Control: Clone + Send
let mut incoming = control.accept(TABLE_PROTOCOL)?;                 // Stream<Item = (PeerId, Stream)>
// outbound: control.open_stream(peer, TABLE_PROTOCOL).await?   — auto-dials if not connected
```

Three operational rules that follow from the crate:

* **Hand every accepted stream to a spawned task immediately.** The crate's own
  README warns it will *drop* streams if the application falls behind on the
  incoming iterator.
* **Dropping `IncomingStreams` de-registers the protocol** and later inbound
  negotiations fail — hold it for the process lifetime.
* `OpenStreamError` is `#[non_exhaustive]`; a wildcard arm is mandatory.

The crate is **`0.5.0-alpha` and semver-exempt**. It is contained behind the
`net/streams.rs` trait (§1.3) so that replacing it is a one-file change.

> Verification: [COMPILED+RUN] `new_control()` and `accept(TABLE_PROTOCOL)` in
> `probe-netstack`; [RESEARCH] `LIBP2P.md` §7 for the backpressure and
> de-registration behaviour and the alpha risk. Re-read on 2026-09-15 against
> `libp2p-stream-0.5.0-alpha` (the probe had `0.4.0-alpha`): `new_control` at
> `src/behaviour.rs:44`, `accept` at `src/control.rs:70-73`, `open_stream` at
> `:44-48` dialling a disconnected peer through the behaviour
> (`src/behaviour.rs:136-137`), `OpenStreamError` `#[non_exhaustive]` at
> `src/control.rs:80-81`, and the README's warnings — streams dropped when the
> application falls behind, the protocol de-registered when the handle is dropped
> — word for word as before. There is no `Drop` impl behind the second: a closed
> channel is pruned (`src/shared.rs:56-57`), and the channel holds no buffer, so a
> stream that finds it full is dropped (`:63`, `:87-88`).

### 8.2 Topology: full mesh, and it is a security requirement

Every participant of a table maintains a direct stream to **every other**
participant. `n(n−1)/2` connections; 45 for a ten-seat table, 1 for heads-up.

A star through the table founder would be cheaper and is **rejected**: it would
make the founder a relay of other players' signed events, which (a) lets the
founder selectively withhold or delay events, and (b) weakens equivocation
detection, because equivocation is detected precisely by two participants
comparing what the *same signer* sent them. If everything passes through one node,
that node chooses what everyone compares. `SPEC_CS.md` §14 requires equivocation to
produce a cryptographic proof; that requires every participant to receive every
event from its own signer over a path the signer chose.

A peer **may** additionally forward a signed event it received, as a resilience
measure when a direct pair link is broken. Forwarding never changes validity: the
signature and the hash chain decide, and a forwarded event is accepted on exactly
the same terms as a directly received one.

### 8.3 What must never carry table traffic

| Channel | Why not |
|---|---|
| **The public DHT** (Kademlia today, Mainline when this was written) | it is world-readable, and a provider record is a `PeerId` and addresses — nothing else fits and nothing there is confidential. That it now authenticates *who announced* (§4.1) changes nothing about this row: `SPEC_CS.md` §1 forbids table traffic there outright, and the record is a hint about where to connect, never a carrier. |
| **GossipSub lobby topic** | it is world-readable by every client in the lobby, which would publish table traffic to non-participants, and a hand's **table-wide** shuffle traffic (18 KB heads-up, 54 KB six-handed [RESEARCH `MENTAL_POKER.md` §5.1]) would flood a topic sized for 1 KiB ads. The table-wide figure is the right one here, because a broadcast topic carries every seat's traffic to everyone; it is the wrong one against a per-circuit relay cap, which is the error §9.5 corrects. |
| **A relay, as an authority** | permitted as a byte pipe (D-001) and never as a participant: it holds no key share, sees no plaintext, arbitrates nothing. |

### 8.4 Framing, admission and membership

* **Join** goes over `request-response` on `JOIN_PROTOCOL`
  (`JOIN_REQUEST` / `JOIN_ACCEPT` / `JOIN_REJECT`, whose bodies and receiver
  validation are `PROTOCOL.md` §4.3's and are not reproduced here), bounded by
  `JOIN_REQ_MAX` and `JOIN_RESP_MAX` (`PROTOCOL.md` §13), 20 s timeout. The two
  are deliberately asymmetric, and the reason is this document's: `JOIN_REQUEST`
  is a small fixed body under `PROTOCOL.md` §9.3's payload cap so its transport
  cap is envelope headroom only, while `JOIN_ACCEPT` embeds a complete signed
  advert (`TABLE_AD_SIGNED_MAX`) plus a full roster and needs the room.
  `PLAYER_LIST` and `TABLE_READY` do **not** travel on this RPC — see
  `PROTOCOL.md` §4.3.
* **Framing on the table stream is two-sided and `PROTOCOL.md` §2's**, which
  fixes it in that document's channel table and bounds it by `TABLE_FRAME_MAX`
  (`PROTOCOL.md` §13); this document does not reproduce either (D-011 rule 1).
  An earlier revision claimed the framing as "ours, not the transport's" and
  restated it, which was wrong twice: a length-prefix encoding both peers must
  agree on is a wire fact, and the sizing argument for the cap already lives in
  `PROTOCOL.md` §13 alongside the value. What is genuinely this document's is
  where the check happens: **the transport reads the `u32` prefix and refuses the
  frame before allocating for the body**, so an over-cap prefix costs four bytes
  and never a buffer. It refuses the frame on size alone and reports nothing
  about the sender (§0.2, §11.3), and it applies no other test — no clock, no
  ordering, no membership beyond the gate below (§0.5.4). `TABLE_FRAME_MAX` is a
  hard bound for the fuzzer (`SPEC_CS.md` §27).
* **Membership gate — admission, not eviction.** A table stream from a `PeerId`
  that is not an admitted participant of that `table_id` is closed immediately,
  before any body is read. Membership comes from the signed `PLAYER_LIST` /
  `TABLE_READY` of the application protocol, never from the transport. This is
  the transport declining to allocate a stream to a peer the signed roster does
  not list; it is **not** the removal of a seated player, which this layer may
  never do on any evidence (§0.1, §1.2 prohibition 7). A seated participant whose
  stream drops is reported as a disconnect and remains seated.
* **Connection loss is a hint, not a verdict.** When a table stream drops, the
  network layer reports the fact and nothing more. Whether that seat is absent is
  decided above, by the D-006 timeout certificate signed by the **required voter
  set `V`** (`PROTOCOL.md` §8.3) — **and whenever `|V| < 2` that certificate has
  no effect, so D-008 leaves the deadline advisory and no verdict is produced at
  all; the transport layer's behaviour is unchanged either way**. At two seats
  `|V| = 1` always, so a heads-up action deadline is a UI countdown that produces
  no signed state transition at all (D-007 point 1, `PROTOCOL.md` §8.3), and the
  transport must not invent one to fill the gap. At larger tables `|V|` can fall
  below two as well, once earlier completed certificates have attributed seats,
  so the rule is scoped on `|V|` and **never on `n`**: `n` is the quantity an
  attacker cannot shrink and `|V|` is the one it can (D-008). Where `|V| >= 2` an
  action timeout is an auto check/fold and never an abort, and no timeout of any
  kind ends the tournament or the cash game. Only a client that is gone or
  withholding decryption shares reaches the D-005 abort path — and under **D-010
  that abort is neutral**: stacks return to their start-of-hand values, the
  signed attribution of which peer failed is recorded as evidence and nothing
  acts on it, and no seat is removed. The transport must
  not shortcut any of that, and nothing in this layer may key any behaviour on the
  seat count.

---

## 9. NAT traversal and reachability

### 9.1 The structural finding that orders everything else

**DCUtR only engages on a connection that is already relayed.**
`libp2p-dcutr-0.15.0` installs its real handler only when `is_relayed(addr)` —
literally `addr.iter().any(|p| p == Protocol::P2pCircuit)` — and installs
`dummy::ConnectionHandler` otherwise. So: no relay ⇒ no DCUtR ⇒ no coordinated hole
punch. A relay is on the critical path for *establishing* many NAT-to-NAT
connections even when the resulting connection ends up fully direct.

> Verification: [SOURCE] `libp2p-dcutr-0.15.0/src/behaviour.rs:180,215,386-387`
> (`handle_established_{in,out}bound_connection` and `is_relayed`), `:46` for
> `MAX_NUMBER_OF_UPGRADE_ATTEMPTS = 3`, `:341,356-387` for the candidate LRU fed
> only by `FromSwarm::NewExternalAddrCandidate`. Every line is one below its
> `0.14.1` number (179, 214, 385, 45, 340, 355-386) and none changed; what feeds
> the LRU did change, through `identify` (§5.5).

This does **not** contradict D-004 layer 1: blind *mutual dialling* needs no relay
at all, because the DHT delivers both addresses to both peers symmetrically. Layer
1 is not DCUtR; it is two clients dialling each other at the same time. DCUtR is
layer 2, and layer 2 needs a relay.

> **The symmetry has a precondition it did not have under Mainline, and it makes
> this finding worse rather than better.** A Mainline announce published an
> `IP:port` whether or not that address worked, so a NATed peer could always put
> *something* in the DHT and layer 1 always had two addresses to work with. A
> provider record publishes `external_addresses`, and `start_providing` is gated
> on holding one — so a NATed peer is not in the lobby at all until a relay has
> given it a circuit address.
>
> So the relay is now on the critical path for layer 1 as well, by a completely
> different route from the one this section found: not because dialling needs it,
> but because *appearing* does. §9.7 layer 0 and §2 state it; it is recorded here
> because a reader who takes this paragraph at face value would conclude that
> layer 1 survives a relay-less world, and it no longer does.

### 9.2 AutoNAT: v2 only

`libp2p 0.57.0`'s single `autonat` feature enables **both** v1 and v2 (the
sub-crate's `default = ["v1","v2"]`, `libp2p-autonat-0.16.0/Cargo.toml:47-50`, and
the umbrella does not disable defaults) — as `0.56.0`'s did.
We use **v2 only**:

* v1 stayed `Unknown` with `confidence: 0` for the entire measured run, because
  `only_global_ips: true` refuses LAN peers as probe servers, and its confidence
  time is `boot_delay 15 s + 3 × retry_interval 90 s ≈ 5 minutes` [MEASURED +
  SOURCE]. Startup must never gate on it.
* v2's `probe_interval` default is 5 s with `max_candidates` 10, and its server
  **always dials back over a freshly allocated port**, which removes v1's false
  "public" verdicts on already-hole-punched ports.
* v2 has **no private-address guard** and was measured confirming an RFC 1918
  address as external. Filtering is ours (§5.6). This is mandatory, not advisory:
  publishing a private address to the DHT poisons the lobby for everyone.

Configuration used: `Config::default().with_probe_interval(30 s).with_max_candidates(8)`.
Both the client and the server behaviour are enabled — a publicly reachable client
answering AutoNAT probes for others costs almost nothing and the network needs
servers to exist.

> Verification: [COMPILED+RUN] both behaviours constructed in `probe-netstack`;
> [MEASURED + SOURCE] `NAT_AND_DISCOVERY.md` §2.2–§2.4, `LIBP2P.md` §3. Re-read on
> 2026-09-15 against `libp2p-autonat-0.16.0`: v1's `boot_delay` 15 s,
> `retry_interval` 90 s and `only_global_ips: true` (`src/v1/behaviour.rs:95, 96,
> 105`), v2's client defaults `max_candidates: 10` and `probe_interval` 5 s
> (`src/v2/client/behaviour.rs:54-55`), and the server's dial-back on a freshly
> allocated port (`src/v2/server/behaviour.rs:116-120`), all as stated. One default
> did change — both v2 behaviours now default their random generator to `StdRng`
> where the client's was `OsRng` — and it does not reach this client, which hands
> the client its own OS generator (`net::swarm::build`).

### 9.3 Reachability classes and what each can do

| Class | Sees the lobby | Dialable | Can play | Notes |
|---|---|---|---|---|
| Public IP / port-forwarded | yes | yes | yes | can volunteer as a relay (D-002); materially helps everyone |
| Full-cone / endpoint-independent NAT | yes | after a punch | yes — layers 1, 2 | the measured development machine: endpoint-independent mapping, port-preserving, address-and-port-dependent filtering [MEASURED] |
| Port-restricted cone NAT | yes | after a punch | yes — layers 1, 2 | both sides must transmit outward first |
| Symmetric NAT (one side) | yes | no | usually yes, via the other side's punch | |
| **Symmetric NAT (both sides)** | yes (layer 3) | no | **only over a raised-limit relay (D-002); otherwise no** | the case that genuinely fails, stated as D-004 requires |
| CGNAT | yes | no | as symmetric, usually | commonly symmetric; no port forward possible |
| **UDP blocked entirely** | **yes** | no | lobby over TCP; playing needs a relay | **corrected 2026-09-02**: discovery is `libp2p-kad` on this swarm's transports, so it falls back to TCP with everything else. The old "no" was Mainline's UDP-only socket. |
| IPv6-only | **untested** | — | — | the `mainline` crate that made this a flat "no" is gone, and the client now binds `/ip6/::` on both transports (§5.8). Nothing has been measured against a **global** IPv6 address; `S1-Y`. |

### 9.4 DCUtR, with the honest numbers

`libp2p-dcutr 0.15.0`. Public surface is `Behaviour::new(local_peer_id)` and a
single `Event { remote_peer_id, result: Result<ConnectionId, Error> }` — there is
no "attempt started" signal.

Measured success in the wild: **70 % ± 7.1 %** over more than 4.4 million attempts
across 85 000+ networks and 167 countries (arXiv:2510.27500). **TCP and QUIC are
statistically indistinguishable, both ≈70 %** — so QUIC is chosen for 1-RTT,
multiplexing and no head-of-line blocking, *not* because UDP punches better.
**97.6 %** of successes happen on the **first** attempt, so retries buy almost
nothing; plan for the relay instead of hammering. Failure is **sticky and
peer-specific** — cache the verdict per peer and go straight to relay next time.
Regional variance is severe: individual reporters saw 1–10 %, and one saw zero
across ten people, both attributed to symmetric NAT prevalence. **A 70 % global
average can be near zero for a specific pair of players.**

`MAX_NUMBER_OF_UPGRADE_ATTEMPTS = 3`, then the event's `Err` and the connection
stays relayed. The attempts-exceeded cause is a variant of a private enum inside
the public `Error`, so it can be read only from the message, *Giving up after 3
dial attempts* (`src/behaviour.rs:55-69`, `:128-140`, unchanged from `0.14.1`
apart from a line's shift). "Still relayed" is a normal steady state and must be
shown as such (§2.2), never as an error.

> Verification: [RESEARCH] `NAT_AND_DISCOVERY.md` §4 and §4.1 (paper fetched;
> a second, similar-titled arXiv identifier surfaced in search and was *not* used
> because it could not be fetched); [SOURCE] the crate constants and API.

### 9.5 Circuit Relay v2 — permitted, and split in two

**D-001 governs: relays are permitted.** A relay carries an already end-to-end
encrypted, mutually authenticated libp2p stream. It cannot read a poker event
(the transport terminates at the peers), cannot forge or alter one (every event is
application-signed and every receiver re-validates independently), cannot become an
authority (it is not a party to any table session and holds no key share), and
cannot decrypt a card (it holds no share of the joint deck key). A malicious relay
is at worst a network adversary already in the threat model.

Its real costs, which `THREAT_MODEL.md` must carry: **metadata exposure** (who
talks to whom, when, how much, for how long), a **liveness dependency** (if every
reachable relay is down, a CGNAT-only client cannot connect — the UI must say "no
direct route and no relay available", never disguise it), and a **DoS lever** (a
relay operator can drop a specific peer; a relayed connection loss is handled
exactly like any other disconnect, per D-005/D-006).

**The two roles must never be blurred** (D-001 addendum):

| Role | Who can serve it | Limits |
|---|---|---|
| **Rendezvous for a hole punch** — carry the DCUtR coordination, then get out of the way | any public relay, including the many public kubo nodes that enable `Swarm.RelayService` by default | the defaults are *sized for exactly this*: `max_circuit_duration` 2 min, `max_circuit_bytes` 128 KiB **as a bidirectional total for the whole circuit** (§16.1), `reservation_duration` 1 h, `max_reservations` 128, `max_circuits` 16, `max_circuits_per_peer` 4 — the *values* are identical in kubo and in `rust-libp2p`; on the byte accounting the two documentations disagree, see the caveat below |
| **Carrying a whole session** when DCUtR fails on both ends | only a relay whose operator raised its own limits — i.e. a D-002 volunteer poker client | the **2-minute `max_circuit_duration`** is what breaks first: a session lasts far longer than two minutes, so a public IPFS relay **will** reset the connection mid-hand, which is an *engineered abort attack against ourselves* under `SPEC_CS.md` §19. The byte cap does not rescue the case either — bidirectional, it is spent after roughly five to seven hands (§16.1) — but the duration is what a reader should expect to hit. |

**The byte arithmetic, corrected twice.** The first correction: an earlier
revision of this row compared *table-wide* shuffle traffic (~18 KB heads-up,
~54 KB six-handed) against `max_circuit_bytes`, which is a **per-circuit** cap. A
table is a full mesh (§8.2), so one circuit connects exactly one pair and carries
only those two peers' own steps and proofs. The second correction is the
*direction*, and it halves the budget: **`max_circuit_bytes` is not per
direction.** `libp2p-relay 0.22.0` relays a circuit with a single `CopyFuture`
holding one `bytes_sent: u64`, and **both** `forward_data` calls — src→dst and
dst→src — increment that one counter before it is compared against the cap. The
crate's own quickcheck asserts the failure condition as `a.len() + b.len() >
max_circuit_bytes`, i.e. the two directions summed. The cap is therefore a
**bidirectional total for the whole circuit**, and every figure below is computed
on that basis:

| Quantity | Value | Basis |
|---|---:|---|
| `ShuffleProof<52>` + `MaskedDeck<52>`, one shuffler | 8 979 B | 5 547 + 3 432, measured [RESEARCH `MENTAL_POKER.md` §5.1] |
| Shuffle traffic over **one circuit**, **both directions together**, per hand | **17 958 B** | `2 × 8 979`; each of the two peers on that circuit sends its own step and proof once, and one counter sees both. **Independent of `n`** |
| Hands against a 131 072 B public-relay budget, shuffle only | **~7** | 131 072 / 17 958 = 7.3 |
| The same including the signed event stream | **~5 hands** | order-of-magnitude only; measure under OQ-7 |
| A relayed peer's **total** per-hand outbound at an `n`-seat table | `(n−1) × 8 979 B` | 44 895 B at six seats — a bandwidth figure, spread over `n−1` separate circuits and therefore `n−1` separate budgets, **never** a single cap |
| The binding public-relay limit | **`max_circuit_duration` = 120 s**, not the byte cap | a session lasts far longer than two minutes |

So the public-relay byte budget is **half** what an earlier revision of this
document claimed — roughly five to seven hands, not ten to fourteen — and the
duration is still what breaks first, since a 120 s circuit expires long before
the fifth hand. The failure mode is unchanged; the reason in the earliest text
was wrong, and the "per direction" arithmetic that replaced it was wrong too
(§16.1).

**This strengthens the two-role split of D-001's addendum; it does not weaken
it.** That split rests on a public relay's defaults being sized for a hole-punch
coordination and nothing more. Halving the effective byte budget removes the last
reading under which a public relay might have carried a short session by
accident: at 131 072 B bidirectional, one circuit is out of budget after about
seven hands even if the duration limit were lifted, so *neither* default leaves
room for a session. A relay that carries a whole session is still only a D-002
volunteer with raised limits, and the two roles stay exactly as far apart as the
table above says.

**One caveat about third-party relays.** kubo's `docs/config.md` describes its
equivalent `ConnectionDataLimit` as applying "in each direction". Either the Go
and Rust implementations genuinely differ here or that wording is loose; the Go
source has not been read for this document, so it is not settled. It costs us
nothing either way: our own D-002 relay is `rust-libp2p`, so the Rust accounting
binds us, and for a third-party relay the stricter reading — one bidirectional
budget — is the safe assumption, because assuming the looser one and being wrong
drops a circuit mid-hand.

Two further facts to reconcile, because the research documents differ in emphasis
and D-001 outranks:

* `NAT_AND_DISCOVERY.md` §3.2 records, as a negative result, that **no curated
  public list** of open Circuit Relay v2 servers exists for third-party
  applications, and rejects hardcoding third-party relay addresses as "a central
  fallback server wearing a costume".
* D-001's addendum records that public relays nonetheless **exist in quantity**,
  because kubo enables the relay service by default on every publicly reachable
  node, and that no shipped list is needed to find them.

Both are true and they are about different things: relays are abundant, a curated
*list* is not. D-001 permits shipping a bootstrap list; this document does not use
one, because we have a better source that costs nothing and is self-healing:

**Relay candidate sources, in priority order:**

1. Peers we are already connected to whose `identify` reports
   `/libp2p/circuit/relay/0.2.0/hop` — free, and observed working at runtime
   [MEASURED `NAT_AND_DISCOVERY.md` §2.1].
2. `get_providers` on the relay key (§3.2) — the `/libp2p/relay` namespace that
   go-libp2p's own AutoRelay advertises under, where public relays and D-002
   volunteers both appear, asked every discovery cycle while this client holds no
   reservation and dialled by the same bridge as §4; a peer whose `identify`
   shows the hop protocol is asked for a reservation. Emergent, self-healing,
   nothing compiled in, nobody structurally privileged. Until 2026-09-15 this item
   named `get_peers(RELAY_INFOHASH)`, a Mainline infohash for volunteers alone,
   which left the build in `56b0b50`.
3. A compiled-in bootstrap relay list — **permitted by D-001**, not shipped in v1,
   and if ever added it must be visibly labelled and always outranked by runtime
   discovery. The one name the client does compile in,
   `/dnsaddr/bootstrap.libp2p.io`, is its way on to the public DHT (§2.1 step 4)
   and not a relay list: those nodes advertise the hop protocol and refuse
   reservations, measured and recorded in `NEXT.md` as a correction to D-001's
   addendum.

The client **must read the `Limit` the relay returns** rather than assume:
`relay::client::Event::{ReservationReqAccepted, OutboundCircuitEstablished,
InboundCircuitEstablished}` all carry `limit: Option<Limit>` with public
`duration()` / `data_in_bytes()` accessors. **`data_in_bytes()` is the same
bidirectional total**: the relay fills the wire field from its own
`max_circuit_bytes` (`src/protocol/inbound_hop.rs:85,138`) and `Limit` copies it
verbatim (`src/protocol.rs:41,49,58`), so the number a relay returns must be
compared against both directions summed, never against one. `Limit` itself is not
re-exported — its module is private — so the value is read off the event fields and
the type cannot be named in this client's code.

A circuit whose advertised limits cannot carry a hand must not be used to sit
down over — **and under D-012 that sentence needs its subject stated, because a
`Limit` is a per-receiver quantity and seating is canonical (§0.5.5).** It is
*this* client declining to send a `JOIN_REQUEST` at all, and saying so honestly,
rather than joining a hand it will drop out of mid-street. It is never a
participant refusing, delaying or conditioning a signed seating event for
somebody else on what its own circuit to them advertises: two participants
holding different circuits to the same joiner would then produce different
rosters, and the table would never start. Seating is signed events every
participant accepts (§8.4), and a seated peer whose circuit degrades is a
disconnect and stays seated.

A reservation is a **lease, not a state**: expect periodic
`ReservationReqAccepted { renewal: true, .. }` and treat its absence as loss of
reachability.

Multiaddr forms (byte-exact, `Protocol::P2pCircuit` is multicodec 290, string
`p2p-circuit`):

```
reservation (listen):  /<relay transport addr>/p2p/<RELAY_ID>/p2p-circuit
dial through a relay:  /<relay transport addr>/p2p/<RELAY_ID>/p2p-circuit/p2p/<DEST_ID>
```

More than one `p2p-circuit` in an address is rejected —
`Error::MultipleCircuitRelayProtocolsUnsupported`, no relay chaining.

Several relay **server** `Event` variants (`ReservationReqAcceptFailed`,
`ReservationReqDenyFailed`, `CircuitReqDenyFailed`, `CircuitReqOutboundConnectFailed`,
`CircuitReqAcceptFailed`) are `#[deprecated]` upstream — build no logic on them.

> Verification: [SOURCE] `libp2p-relay-0.22.0/src/protocol.rs:31-36,39-52`,
> `src/behaviour.rs:163` (`impl Default for Config`, `max_circuit_bytes: 1 << 17`),
> `src/priv_client/transport.rs:266-313` (address parsing, the
> multiple-`p2p-circuit` refusal at `:276-281`); **the bidirectional accounting** in
> `src/copy_future.rs:41-48` (one `bytes_sent: u64` on `CopyFuture`), `:78` (the
> single comparison), `:88-96` and `:98-106` (both `forward_data` calls increment
> that one counter), and `:241` (the crate's own quickcheck asserting
> `a.len() + b.len() > max_circuit_bytes`); the returned `Limit` in
> `src/protocol/inbound_hop.rs:85,138` and `src/protocol.rs:41,49,58`. Re-read
> 2026-09-15 against 0.22.0: `protocol.rs` and `copy_future.rs` are unchanged
> from 0.21.1 apart from test formatting, so the accounting and the figures below
> stand as they were measured.
> [COMPILED+RUN] the multiaddr round trips in `LIBP2P.md` §4; [RESEARCH] D-001
> addendum for the kubo defaults, `MENTAL_POKER.md` §5.1 for the per-hand bytes.

### 9.6 Acting as a relay (D-002)

A client that AutoNAT v2 confirms as publicly reachable **may** run the Circuit
Relay v2 **server** with raised limits, so a relayed connection can carry a whole
session rather than only a hole-punch coordination. This is the only way to get an
unlimited relay without the project operating infrastructure.

**Off by default.** Enabled only by an explicit, visible setting, with a first-run
disclosure in plain language: strangers' poker traffic will cross the user's
connection, at the user's bandwidth cost. The disclosure must state the ceilings
**in concrete terms**, because the raised limits below are large and a user who
agrees to "help relay" is agreeing to these numbers: *up to 128 circuits open at
the same time, each allowed to carry up to 1 GiB — counted in both directions
together, §16.1 — and to stay open for up to an hour, and up to 64 peers holding a
reservation through you.*
Slot and bandwidth ceilings are user-visible and user-settable, and the network
status panel shows how many peers are currently being relayed.

> **The client is not this section, checked 2026-09-15 (`S1-FK`).** Every client
> is built with `RelayRole::Volunteer`, and there is no setting, no disclosure
> and no count of relayed peers. `swarm::relay_config` pushes no
> `PokerPeersOnly` — nothing in `src/` implements `libp2p::relay::RateLimiter` —
> so the crate's own per-peer and per-IP limiters are the whole of the
> admission. Its numbers are 128 reservations (4 per peer), 64 circuits (4 per
> peer, `RELAY_MAX_CIRCUITS_PER_PEER`), an hour per circuit
> (`RELAY_RESERVATION`) and `RELAY_MAX_CIRCUIT_BYTES`, not the block below. A
> client with a confirmed external address therefore serves any libp2p peer
> within those numbers: the open relay the next paragraph describes. What
> follows is what D-002 requires, not what runs.

**The trap: Circuit Relay v2 is not protocol-selective.** `HOP_PROTOCOL_NAME` and
`STOP_PROTOCOL_NAME` are compile-time constants
(`/libp2p/circuit/relay/0.2.0/hop`, `/stop`) and cannot be renamed, and the
`Behaviour` decides accept-or-deny purely on resource limits and rate limiters —
there is no application ACL hook. Left alone, enabling the server makes the user an
**open relay for the entire libp2p network**, IPFS traffic included.

**The usable hook** is `Config::reservation_rate_limiters` and
`Config::circuit_src_rate_limiters`. The module `behaviour::rate_limiter` is
`pub(crate)`, but the trait itself is **re-exported at the crate root**
(`libp2p-relay-0.22.0/src/lib.rs:42-44`), so `libp2p::relay::RateLimiter` is a public,
nameable, implementable trait:

```rust
pub trait RateLimiter: Send {
    fn try_next(&mut self, peer: PeerId, addr: &Multiaddr, now: Instant) -> bool;
}
```

The crate also carries a blanket implementation
`impl<T: FnMut(PeerId, &Multiaddr, Instant) -> bool + Send> RateLimiter for T`
(`src/behaviour/rate_limiter.rs:56`), so a closure would coerce — but **a named
type is used instead**, per D-002's 2026-08-28 correction: admission control holds
shared mutable state (the admitted-peer set, each peer's tier, and that peer's
live-circuit count), and a closure would have to capture all of it awkwardly. The
earlier justification for the closure — that the trait could not be named from
outside — was simply false, and it is the false *reason*, not the working code,
that these documents were carrying.

The third parameter is `web_time::Instant`, so a `web-time = "1"` dependency is
needed just to write the signature. **Both** vectors must be gated: one governs
who may reserve a slot to become reachable through us, the other who may open a
circuit through us.

```rust
struct PokerPeersOnly {
    known: Arc<Mutex<HashSet<PeerId>>>,   // + tier and live-circuit counts per peer
}

impl libp2p::relay::RateLimiter for PokerPeersOnly {
    fn try_next(&mut self, peer: PeerId, _addr: &Multiaddr, _now: web_time::Instant) -> bool { … }
}

let mut relay_cfg = relay::Config {
    max_circuit_duration:      Duration::from_secs(3600),
    max_circuit_bytes:         1 << 30,        // 1 GiB
    max_reservations:          64,
    max_reservations_per_peer: 4,              // crate default
    reservation_duration:      Duration::from_secs(3600),   // crate default
    max_circuits:              128,
    max_circuits_per_peer:     9,              // = MAX_SEATS − 1
    ..Default::default()
};
relay_cfg.reservation_rate_limiters.push(Box::new(PokerPeersOnly::new(&admitted)));
relay_cfg.circuit_src_rate_limiters.push(Box::new(PokerPeersOnly::new(&admitted)));
```

**Every scalar field is stated, so no other document restates a subset.** The
`..Default::default()` therefore fills exactly two things — the crate's own
per-peer and per-IP rate-limiter vectors — and that is deliberate: our
`PokerPeersOnly` is pushed *alongside* them, never in place of them. Two of the
stated fields are corrections rather than choices:

* **`max_circuits_per_peer = 9` is forced.** A relayed peer at a ten-seat table
  needs one circuit per table-mate (§8.2 full mesh, `MAX_SEATS − 1`). The crate
  refuses a circuit when the *requesting* peer is already an end — source or
  destination — of **more** than `max_circuits_per_peer` circuits
  (`src/behaviour.rs:694-695`, `num_circuits_of_peer` at `:946-951`), so the
  default of 4 refuses a peer's sixth, and the D-002 relay as previously written
  could not carry the case it exists for. Because the comparison is `>`, 9 admits
  a peer's tenth: one spare over the nine a full table needs. Until 2026-09-15
  this bullet said the default *refuses the fifth*, which reads the check as `>=`;
  the conclusion was right and the count one short.
* **`max_circuits = 128`** lets one volunteer serve roughly 14 relayed peers at
  full tables (128 / 9), which is the number the consent disclosure above is about.

> Verification: [SOURCE] `libp2p-relay-0.22.0/src/lib.rs:42-44` (the `RateLimiter`
> re-export), `src/behaviour/rate_limiter.rs:38-39` (the trait and `try_next`),
> `:56` (the blanket impl), `src/behaviour.rs:124-166` (`impl Default for Config`,
> giving `max_reservations 128`, `max_reservations_per_peer 4`,
> `reservation_duration` 1 h, `max_circuits 16`, `max_circuits_per_peer 4`,
> `max_circuit_duration` 2 min, `max_circuit_bytes: 1 << 17`, and in both limiter
> vectors a per-peer limiter of 30 tokens refilled one per 2 min and a per-IP one of
> 60 refilled one per minute), `:562-583` and `:694-703` (the admission checks,
> `>` per peer and `>=` in total), `src/behaviour/handler.rs:428-463` (the limits
> handed to each circuit). [COMPILED+RUN] `probe-netstack` builds a `relay::Config`
> with both rate-limiter vectors populated over a shared admitted-peer set,
> `relay::Behaviour::new` accepts it, and the swarm builds and runs. D-002
> independently verified the raised limits print back correctly. **The probe used
> the closure form; the named-type form above has not itself been compiled** —
> it is the same trait and the blanket impl proves the signature, but that is an
> inference, and Phase 7 must compile it.
>
> **One behaviour arrived with 0.22.0 that this section must carry.** A relay
> server now starts with its hop protocol **not advertised** (`Status::Disable`,
> `src/behaviour.rs:319`; the handler denies inbound hop streams while disabled,
> `src/behaviour/handler.rs:522`) and turns advertisement on by itself once the
> swarm holds a confirmed external address (`:366-405`), or when
> `set_status(Some(..))` says so (`:329`); the change is reported as
> `Event::StatusChanged` (`:251`), so a match over every `relay::Event` variant
> has one more to name. For D-002 it is close to the rule this section wants —
> only a confirmed-reachable client offers a relay — and not the same rule: the
> crate counts **any** confirmed external address, and a client behind a NAT
> holds one as soon as a relay accepts its reservation, because the relay client
> confirms the circuit address (`src/priv_client.rs:308`). Relayed inbound
> connections get a handler that denies every stream
> (`src/behaviour.rs:469-471`), so such a client serves the hop protocol only over
> a direct connection — the LAN, or one DCUtR has upgraded.

#### Settling the open decision: relay admission

`DECISIONS.md` leaves this open and names `NETWORK_STACK.md` as the place to settle
it: admit by **`identify` protocol name**, or by **lobby presence**?

**Decision: admit by `identify` protocol name, with escalation by lobby presence.**

* **Tier A — admitted.** The peer's `identify` response lists `/p2p-poker/1`. It
  gets a reservation and is refused a **third** concurrent live circuit, plus a
  per-peer byte ceiling well below the global one.
* **Tier B — escalated.** The peer has additionally been seen publishing a
  validly-signed lobby message, or is a participant of a table we are also in. It
  gets the full raised limits, up to 9 concurrent live circuits.
* **Everyone else is refused**, which keeps generic IPFS traffic off the user's
  line — the thing D-002 actually requires.

**D-010, D-011 and D-012 do not reach this admission control, and it stays
exactly as specified.** For D-010 and D-011 the reason is the boundary of §0.2;
for D-012 it is that a tier is a **local view that stays local** — it is computed
from what this client currently sees connected and currently sees in its lobby,
it governs only this client's own uplink, and it is never gossiped, never
persisted, never signed and never hashed (§0.5.2). Two honest volunteers
assigning the same peer different tiers is normal and costs nothing, precisely
because nothing downstream reads a tier. Refusing a stranger a relay
reservation is (a) not eviction — the stranger keeps every connection and every
seat it had, and is no less able to play than before; (b) not driven by any
protocol proof — `PokerPeersOnly::try_next` sees a `PeerId`, a `Multiaddr` and a
tier, and never an `EquivocationProof`, a timeout certificate or an abort record,
and no such object is an input to it in any form; and (c) a decision about
**this user's own uplink**, which D-002 requires the user to consent to
explicitly and which the user may withdraw at any time by turning relaying off.
The protocol may not choose for the user; here the user has chosen, and what they
chose is how much of their own bandwidth to give away. An implementer must not
"apply D-010" by deleting the admission set: doing so turns the client into an
open relay for the entire libp2p network, which is the one outcome D-002 exists
to prevent.

The tiers are likewise not a reputation system and must not become one. Tier B is
reached by *having been seen doing something ordinary* — publishing a valid lobby
message, or sharing a table with us — and never by anything's absence. **There is
no tier below A.** A peer is admitted or it is not, and a peer that is not
admitted is in the same position as every one of the billions of hosts we have
never heard of. A peer's tier falls back to A only by the ordinary expiry of what
put it in B — its lobby presence ageing out under §10.3, or the shared table
ending — and **never as a consequence of anything it is accused of.** Nothing
records a peer as refused: the set is live state rebuilt from what is currently
connected and currently in the lobby, never a persisted list of names (§11.5).

**The per-tier circuit ceiling is not a `Config` field.** `max_circuits_per_peer`
is a single global number and is pinned at 9 above, because a Tier B peer at a
ten-seat table needs all nine. The 2-versus-9 distinction is therefore enforced
**inside the `PokerPeersOnly` implementation of `libp2p::relay::RateLimiter`**,
which sees the requesting `PeerId` and can count that peer's live circuits: Tier A
is refused beyond 2, Tier B beyond 9. A reader must not think the `Config` field
does it — the `Config` cannot express a per-tier ceiling at all, and assuming it
can is how a Tier A stranger ends up with nine circuits.

Why not lobby presence alone: it is **circular** for exactly the user D-002 exists
to help. A brand-new client behind CGNAT cannot appear in the lobby until it has
connectivity, and it cannot get connectivity without a relay. Requiring lobby
presence for admission locks out the case the feature was built for. D-002's open
question — *can a never-before-seen CGNAT peer obtain a reservation at all?* — is
answered **yes**, deliberately.

The trade-off, stated plainly: **an `identify` protocol name is self-asserted and
free to forge.** Anyone can run a libp2p node that advertises `/p2p-poker/1` and be
relayed. So the ACL is **not a security boundary**; it is a *scope limiter*. What
actually bounds abuse is the resource ceiling — total reservations, per-peer
circuits, per-peer and global byte and bandwidth caps, and the default rate
limiters (1 reservation per peer per 2 min, 1 per IP per min) which stay in place
alongside our own `PokerPeersOnly`. The user-visible setting must say this
honestly: "relay
only for this application" means "only for peers claiming to run this application".

**A race that must be handled.** `PokerPeersOnly::try_next` runs when the
reservation request arrives, and it can arrive before `identify` has completed on that
connection, in which case an honest peer is denied. And a denied reservation is not
retried for us: on a reservation error the client handler forwards the error to
the transport listener and sets its reservation state back to `None`, and the
listener does `self.close(Err(Error::Reservation(e)))` and emits
`TransportEvent::ListenerClosed`
[SOURCE `libp2p-relay-0.22.0/src/priv_client/handler.rs:297-303,521-523`,
`src/priv_client/transport.rs:408,334-345`]. So **the application must observe
`SwarmEvent::ListenerClosed` for a circuit listener and re-issue `listen_on` with
backoff itself.** Whether the race actually occurs in practice, and with what
frequency, is unmeasured — carried as OQ-6.

Since `libp2p-relay 0.22.0` the client behaviour also acts on a closed circuit
listener by itself: it expires that listener's external address
(`ToSwarm::ExternalAddrExpired`, unless another listener still uses the same
connection) and resets the connection's reservation state
(`src/priv_client.rs:173-205`, `src/priv_client/handler.rs:255-257`). A re-issued
`listen_on` therefore starts from a clean reservation, and a circuit address this
client no longer holds leaves the swarm's external set: it is gone at once from the
`identify` banner and from the record `libp2p-kad` hands out when asked, and from
the next `ADD_PROVIDER` this client sends (§10.1 says when that is).

### 9.7 The four layers of D-004, mapped onto mechanisms

D-004 requires the lobby to be visible **even if every client is behind NAT**.

| Layer | Mechanism | What it needs | Gives |
|---|---|---|---|
| **0** | Kademlia lookup and announce | **the lookup half is outbound only and survives an all-NAT world; the announce half does not.** A provider record carries `external_addresses`, so `start_providing` is gated on holding one, and a NATed client has none until layer 3 gives it a circuit address. Also unlike the Mainline client it replaced (`mainline`'s `Dht::client()`, which never served), this node *does* serve queries once it has a confirmed external address (§11.4.3). | *seeing* other players unconditionally; *being seen* only after layer 3 |
| **1** | **Mutual dialling** | both peers provide the key and both call `get_providers`, so both learn the other's addresses at roughly the same time; both dial, and the outbound packets open each NAT mapping. **No relay, no coordination server** — the DHT delivered the information symmetrically. Needs endpoint-independent mapping on at least one side. The port rule of the old §4.4 is gone: a provider record carries every address the swarm holds. **But see layer 0** — this symmetry now presupposes that both peers got themselves announced, which a NATed peer cannot do without layer 3. | a direct connection, with nothing but the DHT |
| **2** | DCUtR over a public relay | a relay for the coordination only; public relays are adequate and free here | a direct connection when blind mutual dialling does not converge |
| **3** | **Lobby gossip over a public relay** | a public relay's 128 KiB / 2 min budget — the 128 KiB is a bidirectional total (§16.1), which is still ample for a few hundred bytes per table ad, and the 2-minute reset is survivable because the client simply reconnects | **lobby visibility with no punch succeeding anywhere.** This is the floor under D-003 and what makes the requirement unconditional |
| **4** | A D-002 relay with raised limits | a volunteering publicly reachable poker client | *playing a hand* when both players are unreachable and no punch worked |

Layer 1 was directly supported by measurement on the development machine: one local
UDP socket against nine STUN servers on seven distinct IPs returned the **same
external `IP:port`** every time, and the external port equalled the local port —
endpoint-independent mapping, port-preserving, on a public (non-CGNAT) residential
address [MEASURED]. One router is not a survey, but that is the common home case
and the case layer 1 is built for.

**The case that genuinely fails**, stated as D-004 requires rather than hidden:
**symmetric NAT on both ends**. A different external port per destination defeats
address prediction, so layers 1 and 2 fail. Those players still see the lobby
(layer 3) and can still play if a raised-limit relay is available (layer 4). If
neither is available, they cannot play.

### 9.8 mDNS and the same-router case

Two clients behind the same router is a case this project explicitly cares about,
and the DHT path handles it **badly**: both players' provider records carry the
*same* public IP with different ports, or only circuit addresses through a relay,
so a dial between them over the public address needs NAT hairpinning, which many
consumer routers do poorly, and otherwise the two neighbours meet through a relay.

`libp2p::mdns::tokio::Behaviour` is therefore **mandatory, not decorative**. It was
measured discovering the LAN peer on all interfaces, and mDNS records carry the
full `/p2p/<PeerId>`, so no handshake-identity step is needed for that path.
Service `_p2p._udp.local`, multicast `224.0.0.251`. The default
`query_interval` is **5 minutes**, far too slow for a lobby — two players starting
a minute apart would wait — so it is lowered to **15 s**.

The guarantee comes from never needing the router at all: both peers advertise
their RFC 1918 addresses in `identify.listen_addrs` and in mDNS and dial each other
directly on the L2 segment. This is why the dial filter (§4.5) and the publish
filter (§5.6) must stay separate.

Hazard measured on this developer machine: a Hyper-V "Default Switch" address
`172.20.160.1` was advertised, dialled, and burned a full handshake timeout because
the virtual switch is isolated. Dial candidates concurrently (libp2p already races
them), cap the per-address timeout, and exclude known-virtual interface prefixes
from what we *publish*.

> Verification: [MEASURED+COMPILED] `NAT_AND_DISCOVERY.md` §5.1–§5.3;
> [COMPILED] `mdns::Config { query_interval: 15 s }` in `probe-netstack`.

### 9.9 UPnP

`libp2p::upnp::tokio::Behaviour` (IGD port mapping) is enabled as a best-effort
nicety: when it works, the client becomes publicly reachable and helps everyone.
It is never depended on, because AutoNAT + DCUtR + relay is the real path. The
umbrella `libp2p 0.57.0` requires `^0.7.0` and `Cargo.lock` resolves
`libp2p-upnp 0.7.0`; until 2026-09-15 this paragraph said `0.6.0` existed outside
the old umbrella's `^0.5.0` range, which was true of `0.56.0` and is moot now.
`0.6.0`'s one API change, kept in `0.7.0`, makes the external-address events
struct variants carrying both the local and the external address.

---

## 10. Heartbeat, TTL, and self-healing expiry

Three independent expiry layers. **None** may depend on a departing peer announcing
anything — a crashed or killed client must vanish without cooperation
(`SPEC_CS.md` §1).

### 10.1 The DHT layer — a provider record, and the loop that is not ours

**This section is inverted, not merely stale, and that is why it was rewritten
first.** It said *"nothing in `mainline` re-announces for us … the application
owns the loop"* and specified a **10-minute** re-announce cadence chosen against
a measured ~45-minute record lifetime. Both halves are now wrong, in opposite
directions: `libp2p-kad` **does** own the republish loop, and its interval is
**12 hours**. An implementer who built the old 10-minute application loop would
be re-publishing seventy-two times more often than the crate does, against a
record that outlives the old one by a factor of about sixty.

| Parameter | Value | Basis |
|---|---|---|
| Provider record TTL | **48 h** | `libp2p-kad 0.49.0` `behaviour.rs:232` [SOURCE] (233 in `0.48.0`) |
| Re-announce interval | **12 h**, and the crate runs it | `behaviour.rs:231`, `jobs.rs:268-279` [SOURCE] |
| First re-announce | **now + 12 h**, not now | `AddProviderJob::new` sets its first deadline a full interval out [SOURCE] |
| Explicit announce | the moment an external address first exists, then every **300 s** until confirmed | `run::LobbyAnnounce`, `REANNOUNCE_EVERY`. Until 2026-09-15 this row read *once … latched by `in_public_lobby`*, and the latch was the defect (`S1-FI`) |
| Re-run `get_providers` | **every 60 s** | `run::run`'s discovery timer |
| Un-announce | **never, and it could not help** | `stop_providing` is never called, and is documented local-only: remote copies run their own 48 h clock (`behaviour.rs:1047-1054`) [SOURCE] |

Three consequences follow, and none of them is the old section's.

**Until the record is confirmed the client walks it again; after that, a session
shorter than twelve hours walks it no more.** The crate's job does not fire inside
twelve hours, so the client repeats the explicit `start_providing` every 300 s
until two things have happened: a walk of this session has finished having
reached at least one node, which is what hands the record on, and a node has
since answered a lookup of the lobby key with this client's own record. The first
walk happens seconds after the relay reservation, when the routing table is at
its thinnest; the next ones are made against a fuller one.

**This paragraph said *announces exactly once … nothing repairs it* until
2026-09-15, and for a client behind a NAT that is what ran** — although `79ea1d5`
had written the repair on 2026-09-03. Two things kept it from running, both an
intent recorded as a result. The relay-circuit arm, where a NATed client's first
announcement comes from, set the confirmed flag the moment `start_providing`
returned `Ok`, and the re-walk waits for that flag to be false. And the
confirmation — this client seeing its own `PeerId` among the lobby's providers —
was answered by the client's own store: `start_providing` puts the record there
first, and `get_providers` reports that store as a `FoundProviders` before any
request leaves, with empty statistics (`libp2p-kad-0.49.0` `behaviour.rs:1024-1034`,
`1060-1105`; `the_local_store_names_this_client_before_any_request_is_made` pins
it). A sighting now counts only from an answer a request was made for, and only
after delivery. The nodes that can send one are go-libp2p's, which do not filter
the asker out of a provider list; a rust-libp2p node does (`behaviour.rs:1252`).

Measured on 2026-09-15 with one headless client and `--no-mdns`, each run under an
identity of its own. Before the fix (`announce-before-121050`, 482.7 s): the
circuit at 4.4 s, *announced in the public lobby* once at 16.7 s, and no second
walk in the 466 s after, though one was due from 304 s. After the latch fix
(`announce-after-122327`, 720 s): one walk, confirmed at once — and that run is
why delivery is required: a fresh identity's first sighting from a node came half
a second after the dispatch and twenty-two seconds before its walk had sent the
record anywhere, from a source neither library explains. Instrumented
(`announce-probe-123805`, 420 s): the walk at 4.0 s, this client's own store
answering the lookup issued with it at once with no request made, the walk
finished at 15.0 s having reached 81 nodes, and the first node to return the
record at 62.9 s. With the rule as committed (`announce-final-125214`, 422.7 s):
the walk at 3.0 s, finished at 16.0 s having reached 84 nodes, confirmed at 62.7 s
on the first node's answer after the store's own, and one walk in the whole run.
A re-walk was not seen live, because a node returned the record inside a minute
every time; the 300 s path is held by `an_unconfirmed_lobby_announcement_is_walked_again`.
**What a confirmation does not prove:** that the record is this session's. One stored by an earlier session under the same identity is returned
just the same until it expires, with that session's addresses.

**Leaving does not un-announce.** A player who closes the client stays in the
lobby, by persistent `PeerId` and address, for up to 48 hours. That is not a
defect to fix at this layer — `stop_providing` is local and would change nothing
remotely — it is the reason the discovery loop dials at most
`DIALS_PER_ANSWER = 8` fresh providers per answer rather than everything an answer
contains, and it is a disclosure, which §3.5 states.

**The 45-minute figure and `OQ-1` are both obsolete, and their replacement is
not measured.** 48 h and 12 h are `libp2p-kad`'s **own** defaults, the same in `0.48.0` and `0.49.0`. The
nodes that actually store our record on the Amino DHT are overwhelmingly
go-libp2p, whose `ProvideValidity` governs the real lifetime, and that constant
was not read here. So the operative TTL is **[UNMEASURED]**, and the honest
statement is that it is bounded above by 48 h by our own crate's expiry stamp
and otherwise unknown. `OQ-1` is re-aimed at that question rather than closed.

### 10.2 libp2p connection layer

* `ping::Behaviour` on every connection gives liveness.
* `with_idle_connection_timeout(60 s)` — the default is much shorter and silently
  closes quiet lobby connections.
* Keep-alive must stay **well under 30 s**: consumer-router UDP mappings commonly
  expire in 30–120 s. `ping`'s default interval is suitable; do not raise it. QUIC
  `keep_alive_interval` is set to 5 s and `max_idle_timeout` to 30 s.
* Relay reservations are leases: absence of a periodic
  `ReservationReqAccepted { renewal: true, .. }` means loss of reachability
  through that relay, and the client re-reserves or picks another.

### 10.3 Lobby application layer — the authoritative one

| Item | TTL | Re-broadcast |
|---|---|---|
| `LOBBY_TABLE_AD` | `AD_TTL_MS` | every `AD_REBROADCAST_MS` by the table founder — the values are `PROTOCOL.md` §13's and are chosen so the TTL is 3× the re-broadcast, giving margin against gossip jitter and one missed beat |
| Player presence | `PRESENCE_TTL_MS` | every `PRESENCE_HEARTBEAT_MS`, in the same 3× relation |

Rules:

* On expiry, drop the entry locally. **TTL expiry is the only cleanup path.**
  `LOBBY_TABLE_REMOVE` is an optimisation for the polite case and never a
  precondition for cleanup, and no `TABLE_CLOSE` is required.
* **No peer may revoke another peer's advert.** A `LOBBY_TABLE_REMOVE` is
  accepted only when it is signed by the advert's own `table_public_key`; a
  removal signed by anything else is discarded exactly like any other
  wrongly-signed lobby message (§6.4 step 2, `PROTOCOL.md` §7.2). This is what
  makes the abandoned-formation case of `PROTOCOL.md` §4.3 safe: when a founder
  disappears before
  `TABLE_READY` completes, nobody holds the table key, so nobody can revoke the
  advert — and nobody needs to. It leaves every lobby by `AD_TTL_MS` with
  no cooperation from anybody, and a client holding a `JOIN_ACCEPT` for a table
  whose advert has expired must stop displaying that table as joinable.
* The signed `expires_at` is what the lobby honours — never a peer's word that a
  table is gone.
* **Clock skew is an attack surface, and the rules that answer it are
  `PROTOCOL.md` §7.2's**: relative freshness measured from local receipt rather
  than absolute time, `expires_at` as an upper bound only, a skew allowance on
  `timestamp`, and strict `timestamp` monotonicity per `table_id`. Those
  predicates are not reproduced here (D-011 rule 1); what is this document's is
  why the transport wants them — an absolute-time expiry lets a malicious peer
  pin a table in every lobby forever, and a receipt-relative one cannot be
  pinned at all because it does not consult the sender's clock.
* **Every one of those tests reads the local clock, so every one of them is a
  per-receiver quantity, and the result is a local view.** That is admissible
  because lobby traffic is unchained and this view is displayed rather than
  hashed (§0.5.4). Two honest clients holding different table sets, or different
  copies of one table's advert, are both correct. What must not happen is any of
  it being read as agreed state — §0.5.6 records the one place downstream where
  the corpus once did exactly that, and its disposition: closed in the Phase 3
  gate, so that this paragraph's "both correct" now holds all the way down
  instead of stopping one layer short.

### 10.4 What these TTLs do **not** govern

A *seated* player's absence. That is D-005/D-006/D-007/D-008 protocol state —
absent seat, auto check/fold, timeout certificate, hand abort with signed
attribution, and none of that machinery at all wherever the required voter set has
fewer than two members, which is always so at two seats and reachable at any seat
count (D-008, §8.4) — decided
above this layer by signed events, never by a network timer. The lobby TTLs govern
only *lobby visibility*: whether a table and a player still appear in the list.
Conflating the two would let a network hiccup fold a hand, which §1.2 rule 3
forbids.

Nor does an expiry here remove anyone. A lobby entry ageing out is this client
forgetting a *hint* it has stopped hearing; it takes nothing from the peer, is
not persisted, and reverses itself the moment the peer advertises again. Under
D-010 the attribution an abort records has no automatic consequence at any layer,
and at this one it has never had any: the transport does not read abort records
(§0.1, §11.5).

**And under D-012 it could not govern a table's state even if somebody wanted it
to.** A TTL firing is a judgement made by one client's timer about one client's
receipts. Two honest clients on the same table disagree about it routinely — one
missed a beat of gossip, one has a slower clock, one joined a minute later — so
it is a per-receiver quantity by construction and may not enter a state hash, a
roster hash, a chained body or a genesis (§0.5.2, §1.2 prohibition 8). It drives
exactly one thing: whether this client shows a table or a player in its own
list. A table disappearing from the lobby says nothing about whether a hand at
that table is in progress, who is seated at it, or whose turn it is; those are
decided by chained events every participant accepted, and the lobby is not a
second route to any of them.

---

## 11. Resource limits

`SPEC_CS.md` §17 and §27: assume a fully modified adversary who can craft arbitrary
packets, and bound every message and every collection.

### 11.1 libp2p connection and memory limits

```rust
connection_limits::ConnectionLimits::default()
    .with_max_pending_incoming(Some(32))
    .with_max_pending_outgoing(Some(64))
    .with_max_established_incoming(Some(128))
    .with_max_established_outgoing(Some(128))
    .with_max_established(Some(192))
    .with_max_established_per_peer(Some(2))
```

plus `memory_connection_limits::Behaviour::with_max_percentage(0.25)` — refuse new
connections once the process holds a quarter of system memory.

`with_max_established_per_peer(2)` allows exactly the relayed-plus-direct pair that
exists during a DCUtR upgrade, and no more.

> Verification: [COMPILED+RUN] in `probe-netstack`; [SOURCE] the six setter names at
> `libp2p-connection-limits-0.7.0/src/lib.rs:189-225` (188-224 in `0.6.0`), and
> `with_max_percentage` at `libp2p-memory-connection-limits-0.6.0/src/lib.rs:99`.

### 11.2 Discovery budget

**Six of these seven rows had no counterpart in the client, and the one that did
carries a different number.** The table below is what runs; the old table is kept
underneath it, because the difference between "we chose not to" and "nobody
built it" is the point, and a deleted row cannot say which it was.

| Budget | Value | Where |
|---|---|---|
| Fresh providers dialled per DHT answer | **8** | `run::DIALS_PER_ANSWER` |
| Discovery cycle | **60 s** | `run.rs`, and every cycle pays a full `α` = 3 Kademlia walk to completion (§11.4) |
| Retries per provider within a session | a provider that did not answer is dialled again after **60 s**, at most **8** such re-dials per DHT answer, counted apart from the fresh dials so a lobby of ghosts cannot spend discovery on providers already failed; a provider whose dial *the far end* failed inside the process's first **120 s** may be looked at again after **10 s**, from **128** such looks for the whole life of the process. `PeerCondition::DisconnectedAndNotDialing` still refuses a second dial while one is in flight, and `dialled_lobby` maps each provider to when it was last dialled, trimmed past 512 to the peers still connected. **This row said *0 … a `dialled_lobby` set of 512 suppresses the rest* until 2026-09-15**; that was true before `REDIAL_AFTER` existed, and the old set had refused a live peer for the life of the process after one failed dial | `run::REDIAL_AFTER`, `REDIALS_PER_ANSWER`, `FAST_REDIAL_AFTER`, `FAST_REDIAL_WINDOW`, `FAST_REDIAL_BUDGET` |
| Whole-query timeout | **60 s** | `libp2p-kad` `QueryConfig::default`, re-set to the same value in `net::swarm::build` |
| Per-peer timeout inside a query | **10 s** | `libp2p-kad` `query/peers/closest.rs:86` — the old "per-address dial timeout" row's number, in a different place and meaning a different thing |
| Peers asked for a snapshot | 4 | §7.2 |

**Not implemented, and named rather than dropped:**

* *"DHT candidates dialled per cycle ≤ 64"* — the real figure is **8**, and it
  was reduced to that after a measurement: a lobby key holds providers who left
  up to 48 hours ago, dialling them all filled a finite connection budget with
  the dead, and a table stopped forming at all.
* *"Concurrent dials from DHT hints ≤ 8"* — there is no concurrency cap. Eight
  dials are issued and the swarm runs them as it likes; the number coincides with
  `DIALS_PER_ANSWER` by accident of both being 8, not by design.
* *"Per-address dial timeout 10 s"* — nothing sets a dial timeout. The 10 s above
  is Kademlia's per-peer query timeout and does not bound a dial.
* *"Dials to the same `/24` per cycle ≤ 4"* — **no such rule exists**, and it was
  the one anti-Sybil budget in the table. See §4.5, where the address-level
  filters lost their home for the same reason: this client never sees a
  provider's addresses.
* *"Bootstrap retry backoff 2 s, 5 s, 15 s, 60 s, then 5 min"* — `libp2p-kad`
  runs a **5-minute periodic bootstrap** and there is no backoff ladder.

<details><summary>The old table, for the record</summary>

| Budget | Value |
|---|---|
| DHT candidates dialled per `get_peers` cycle | ≤ 64 |
| Concurrent dials from DHT hints | ≤ 8 |
| Per-address dial timeout | 10 s |
| Retries per DHT-derived address within a session | 0 (the next cycle is the retry) |
| Dials to the same `/24` per cycle | ≤ 4 |
| Peers asked for a snapshot | 4 |
| Bootstrap retry backoff | 2 s, 5 s, 15 s, 60 s, then 5 min |

</details>

### 11.3 Message size caps

**`PROTOCOL.md` §13 is the single normative home for every value below and this
table carries none of them** (D-011 rule 1). An earlier revision reproduced all
thirteen and warned in its own opening line that the copy could go stale, which is
precisely the shape D-011 was written to delete: a document that has to tell the
reader its own table might be wrong is telling the reader to go elsewhere, so it
should send them there and stop. What this table owns is the map — **which cap
the transport enforces at which point in the stack** — because that is a
transport fact and lives nowhere else.

| Where the transport enforces it | Constant | Enforcement point |
|---|---|---|
| GossipSub transmit/receive | `GOSSIP_MAX_TRANSMIT` | `gossipsub::Config::max_transmit_size` (§6.2) |
| Any lobby message (application) | `LOBBY_MSG_MAX` | application ceiling, checked after decode (§6.4 step 1, §6.5) |
| `LOBBY_TABLE_AD` payload | `TABLE_AD_MAX` | §6.4 step 1, §6.5 |
| A complete signed advert, forwarded or embedded | `TABLE_AD_SIGNED_MAX` | snapshot elements (§7.3) and `JOIN_ACCEPT`'s `advert_event` (§6.5) |
| Lobby chat payload | `LOBBY_CHAT_MAX` | §6.4 step 1, §6.5 |
| Snapshot request | `SNAPSHOT_REQ_MAX` | `set_request_size_maximum` (§7.1) |
| Snapshot response | `SNAPSHOT_RESP_MAX` | `set_response_size_maximum` (§7.1) — a **mandatory override** of the codec's 10 MiB default |
| Ads in one snapshot | `SNAPSHOT_MAX_ADS` | counted before allocation (§7.3) |
| Join request / response | `JOIN_REQ_MAX`, `JOIN_RESP_MAX` | the join RPC codec (§8.4) |
| Table stream frame | `TABLE_FRAME_MAX` | our own `u32` length prefix, checked before the body is read (§8.4) |
| One embedded evidence element | `MAX_EMBEDDED_EVENT` | inside the frame, by the protocol layer (`PROTOCOL.md` §9.3) |
| Relay control protocol | `MAX_MESSAGE_SIZE` | **the crate's constant, not ours** — 4 096 B, [SOURCE] `libp2p-relay-0.22.0/src/protocol.rs:36`, unchanged from `0.21.1`. It is stated here because it is not in `PROTOCOL.md` §13 and cannot be: we neither choose it nor negotiate it |

Every one of these parsers is a fuzz target (`SPEC_CS.md` §27): no input may crash,
allocate unboundedly, read out of bounds, or bypass schema validation. Collections
are length-capped **before** allocation, not after.

A cap breach is a **size fact about the bytes**, never a verdict about the sender:
the message is dropped, or the frame refused, and nothing else follows (§0.2).
An over-cap frame from a seated participant does not unseat them and a stream of
them does not block them; if it is sustained it is a rate fact, and the §6.6
backpressure ladder answers it as a rate fact.

### 11.4 Kademlia limits — and the one that decides whether a lobby stays findable

**This section used to bound a Mainline announce. `56b0b50` replaced it with a
libp2p Kademlia provider record (§3), and the limits below are that crate's.**
The version is the one in `Cargo.lock`, `libp2p-kad 0.49.0`, read in the
registry rather than on docs.rs — which matters here, because two of its own doc
comments disagree with its code. This section was written against `0.48.0` and
re-read line by line against `0.49.0` on 2026-09-15: **no default and no behaviour
below moved**; the line numbers are the new ones. The changes `0.49.0` does carry
— stream timeouts on `futures-timer` (still 10 s), a `prost` codec behind the same
16 KiB limit, `GetRecordError::QuorumFailed` removed, `QueryStats` and
`ProgressStep` made `Copy` — touch none of it.

#### 11.4.1 The numbers

| Bound | Default in `libp2p-kad 0.49.0` | Does this client change it? |
|---|---|---|
| Provider record TTL | **48 h** (`behaviour.rs:232`) | no |
| Provider re-publication interval | **12 h** (`behaviour.rs:231`) | no |
| Value-record TTL | 48 h (`behaviour.rs:227`) | no — the client stores no value records |
| Value-record replication interval | 1 h (`behaviour.rs:228`) | no |
| Value-record publication interval | 22 h (`behaviour.rs:229`) | no |
| Replication factor `k` | **20** = `K_VALUE` (`lib.rs:91`, `query.rs:274`) | no |
| Query parallelism `α` | **3** = `ALPHA_VALUE` (`lib.rs:101`, `query.rs:275`) | no |
| Disjoint query paths | off (`query.rs:276`) | no |
| Whole-query timeout | 60 s (`query.rs:273`) | **set — to 60 s**, the same value, on both behaviours in `net::swarm::build` |
| Per-peer timeout inside a query | 10 s (`query/peers/closest.rs:86`) | no |
| Closest peers a lookup resolves before ending | `k` = 20 (`query.rs:144-156`) | no |
| Local peers a lookup is seeded from | ≤ 20 (`query/peers/closest.rs:125`) | no |
| Concurrent queries | **unbounded** for queries we start; background jobs stop at 100 in the pool and add ≤ 10 per poll (`jobs.rs:80,83`; `behaviour.rs:2556-2574`) | no |
| Providers stored per key | **20** = `K_VALUE` (`record/store/memory.rs:69`) | no |
| Keys with provider records the store holds | 1 024 (`record/store/memory.rs:68`) — the check counts every key with any provider, other peers' included, though the field is named for the provided ones (`:144-151`) | no |
| Value records stored | 1 024, each under 65 KiB — a value of exactly 65 536 bytes is refused (`record/store/memory.rs:66-67`, `:114`) | no |
| k-bucket size | 20 (`kbucket.rs:101`) | no |
| Pending-replacement timeout | 60 s (`kbucket.rs:102`) | no |
| Kademlia packet size | 16 KiB (`protocol.rs:51`) | no |
| Periodic bootstrap | 5 min (`behaviour.rs:235`) | no |
| Write-back caching | on, 1 peer (`behaviour.rs:234`) | no |
| Mode at construction | `Client`, automatic (`behaviour.rs:512-513`) | **set — see §11.4.3** |

**Read the code, not the doc comment.** `set_record_ttl` documents "36 hours"
and `set_publication_interval` documents "24 hours" (`behaviour.rs:293-294`,
`:343`); `Config::new` writes 48 h and 22 h, and `0.49.0` still has both comments
wrong. Nothing here depends on either, but a reader who trusts the prose gets both
wrong.

#### 11.4.2 What the client sets, and what it inherits

**One line of Kademlia configuration, and it changes nothing.** `net::swarm`
builds both behaviours with `kad::Config::new(<protocol name>)` and calls
`set_query_timeout(Duration::from_secs(60))` on each — which is exactly what
`QueryConfig::default()` already carries. The store is `MemoryStore::new`, so
`MemoryStoreConfig::default()` in full. There is no `set_replication_factor`,
`set_parallelism`, `set_provider_record_ttl`,
`set_provider_publication_interval`, `set_kbucket_size`,
`set_kbucket_pending_timeout`, `set_caching`, `set_max_packet_size`,
`disjoint_query_paths` or `set_periodic_bootstrap_interval` call anywhere in
`src/`.

That is stated plainly because the section this replaces described a page of
hardening — a deny-all request filter, a four-field `ServerSettings` clamp — and
there is no counterpart to describe. **The only Kademlia knob this client turns
is `set_mode`.** Every quantity above is the crate's choice, and the paragraphs
below are what those choices cost a lobby, not what we tuned them to.

#### 11.4.3 Client mode and server mode, and which this client is when

A `kad::Behaviour` in **server** mode advertises the protocol on inbound
substreams and answers `FIND_NODE`, `GET_PROVIDERS`, `ADD_PROVIDER`, `PUT_VALUE`
and `GET_VALUE` for strangers; in **client** mode the inbound upgrade is
`upgrade::DeniedUpgrade` (`handler.rs:607-612`) and it answers nothing, while
still making every query of its own. A behaviour is constructed as
`Mode::Client` with automatic mode on (`behaviour.rs:512-513`), and automatic
means: client while there is no confirmed external address, server once there is
one (`determine_mode_from_external_addresses`, `behaviour.rs:1168-1216`, run on a
change of the external set at `:2687`).

This client uses all three positions:

* **`set_mode(None)` at build** (`net::swarm::build`) — the public behaviour follows
  libp2p's own rule. Pinning it to client was tried and measured: two clients
  each announced in the lobby and each read it about a hundred and thirty times
  over ten minutes, and never found each other, because a node that answers
  nobody is added to nobody's routing table and its walks never converge.
* **`set_mode(Some(Mode::Client))` while a table this client sits at is closed**
  — every seat taken, or a tournament that has started (`run::dht_effort`,
  `run::table_is_closed`). Server mode is bandwidth spent on the whole public
  network, and at a table being played that is bandwidth taken from the game.
  Announcing and looking up continue; only the service to strangers stops. This
  bullet said *while seated* until 2026-09-15; the code's test is the closed
  table, so a client waiting at an open one still serves.
* **`set_mode(None)` again once no table of this client is closed** — restoring
  the rule, not asserting server: a NATed client must not claim to serve queries
  it cannot be reached for.

A player pinned to client mode is still **findable**: a provider record is held
by the `k` closest server nodes, not by the announcer. What client mode costs is
this node's contribution to everyone else's lookups.

The second, private behaviour (`/p2p-poker/kad/1`, `swarm::PokerBehaviour::kademlia`) is
constructed and never driven — no `set_mode`, no query — so it sits at the
constructor default and does nothing at all.

#### 11.4.4 What this means for a lobby that must stay findable

**Inside twelve hours the crate never republishes, so the client repairs its own
first walk.** `AddProviderJob::new` sets its first deadline to *now + interval*
(`jobs.rs:268-279`), and the interval is the 12 h default. The explicit
`start_providing` at the moment the client first has an external address happens
seconds after a relay reservation, when the routing table is at its thinnest, and
it is walked again every 300 s until a walk has handed the record to somebody and
a node has returned it (§10.1). This paragraph said *announces exactly once …
nothing repairs it* until 2026-09-15; for a client behind a NAT that was true of
the code, for the two reasons §10.1 gives (`S1-FI`).

**Leaving does not un-announce, and could not.** The client never calls
`stop_providing`, and that call is local anyway (`behaviour.rs:1047-1054`) —
remote copies run out their own 48 h clock. So the lobby key accumulates
providers who left up to two days ago. That is not a defect to be fixed at this
layer; it is the reason the discovery loop dials at most `DIALS_PER_ANSWER = 8`
fresh providers per answer rather than everything an answer contains.

**Twenty nodes hold it, twenty providers each, and the twenty-first is dropped
in silence.** A record goes to the `k` = 20 closest peers, and each caps its
provider list for a key at `max_providers_per_key` = 20, discarding a further
provider with `Ok(())` — no error reaches the announcer
(`record/store/memory.rs:170-175`). Above roughly twenty concurrent players no
single storing node holds the whole lobby, and which subset it holds is
first-come. A lookup asks many nodes, so this degrades rather than breaks, but it
is why the lobby must be read repeatedly rather than once.

**A lookup returns as many providers as it meets, and this client never stops
one early.** Each responding node returns every non-expired provider record it
holds for the key except the asker (`behaviour.rs:1242-1252`), and each response
is emitted immediately as `GetProvidersOk::FoundProviders`
(`behaviour.rs:2381-2397`), an empty one included. There is no provider ceiling;
the walk ends when the `k` = 20 closest peers are resolved, when every peer it
could reach has been contacted (`query/peers/closest.rs:374-380`), at the 60 s
timeout, or when the caller calls `QueryMut::finish()` (`behaviour.rs:3377-3379`)
— **which this client does not do**. Every 60 s discovery cycle therefore pays a
full `α` = 3 walk to completion, per key.

**An unresponsive peer has no expiry.** Addresses are dropped one at a time on
dial failure and the last one is kept on purpose (`behaviour.rs:2001-2016`); a
peer leaves the routing table only when it is the least-recently-connected
disconnected node in a full 20-slot bucket and a replacement has been pending for
60 s (`kbucket/bucket.rs:326-365` inserts the replacement, `:220-274` applies it,
and a node that reconnects first keeps its place, `:299`), or when the application
removes it — which this client never does. A table that has gone stale is
refreshed by the 5 min periodic bootstrap, not by anything ageing entries out.

> Verification: [SOURCE] `libp2p-kad-0.49.0/src/behaviour.rs:223-238, 512-513,
> 1047-1054, 1168-1216, 1242-1252, 2001-2016, 2365-2399, 2556-2574, 3377-3379`;
> `src/lib.rs:91,101`; `src/query.rs:144-156, 270-279`;
> `src/query/peers/closest.rs:81-89, 125, 374-380`; `src/record/store/memory.rs:63-72,
> 143-184`; `src/kbucket.rs:98-105`; `src/kbucket/bucket.rs:220-274, 299, 326-365`;
> `src/jobs.rs:80-83, 268-279`; `src/handler.rs:607-612`; `src/protocol.rs:51`.
> Version pinned by `Cargo.lock` (`libp2p-kad 0.49.0`); the `0.48.0` ranges this
> block cited until 2026-09-15 were `behaviour.rs:224-239, 513-514, 1047-1054,
> 1168-1198, 1238-1248, 1998-2013, 2362-2396, 2553-2571, 3382-3384` and
> `kbucket.rs:97-104`, the rest unchanged. Client side, by name because line
> numbers into it go stale: `net::swarm::build` (both behaviours, the query
> timeouts, `set_mode(None)`), `run::DIALS_PER_ANSWER`, `run::dht_effort`, and the
> provider-answer arm of `run::run`. [MEASURED] the pinned-client-mode failure in
> §11.4.3 is the run recorded in the `set_mode(None)` comment in
> `net::swarm::build`.

### 11.5 Eviction: what the transport may do on its own, and what it may never do

This section replaces the "Blocklist" section an earlier revision carried, which
called `block_peer(PeerId)` on a **proven protocol violation** — an invalid
application signature, or an `EquivocationProof` as `PROTOCOL.md` §5.2 defines it.
**D-010 point 3 and D-011 rule 3 delete that path.** It is the single change this
document was missing, and D-011 named its absence a blocker.

#### 11.5.1 Forbidden: automated eviction driven by a protocol proof

> No `block_peer` call, no disconnect, no dial refusal, no unseating, no
> persisted mark and no allow-list or block-list entry may be produced by any of:
> an `EquivocationProof` (`PROTOCOL.md` §5.2), a timeout certificate
> (`PROTOCOL.md` §8.3), a `HAND_ABORT` attribution, a `DISPUTE`, an invalid
> application signature, a `STATE_HASH` divergence, or a count of any of them.

**That box is unchanged by D-015 as well, and the reason is worth one line rather
than a sweep note.** Two of the seven items it enumerates — an `EquivocationProof`
and a timeout certificate — are objects nothing produces in version 1, so those two
clauses are vacuously satisfied. They are **not** deleted: a vacuous prohibition costs
nothing, and the alternative is that a later version reinstating the object finds this
box silent about it. The five items that remain — a `HAND_ABORT` attribution, a
`DISPUTE`, an invalid application signature, a `STATE_HASH` divergence, and a count of
any of them — are all still produced, so the box is load-bearing today.

**That box is unchanged by D-014, and this is the paragraph an implementer who
has just read `STATE_MACHINE.md` T64 needs.** D-014 permits one automated
removal, on a tier-1 self-authenticating finding, and every item in the box above
— including *an invalid application signature*, which is a tier-1 trigger by name
— stays forbidden here regardless. The reason is that a D-014 removal is a
**table** disposition and not a transport one: it takes the offender's seat
(T64, T65, I34), blinds its stack off, and reaches canonical state through
`HAND_INIT(k+1)`'s collective body. It closes no socket, refuses no dial,
withdraws no relay reservation and writes no name to disk. **Arriving at T64 is
not licence to call `block_peer`**, and there is no path from a tier-1 verdict
into this section: the removed player's connections are handled exactly as any
other peer's until §11.5.3's user chooses otherwise. §0.6 states the exception in
full and is the normative record of it in this document.

The transport does not read those objects at all. It cannot construct one, it has
no branch that consumes one, and an implementer who finds themselves passing a
proof down into `net/` has crossed the boundary of §1.2 prohibition 7 and should
stop. What a proof is for is stated in D-010 point 4: it is how a human, or a
later version with sound machinery, adjudicates. A permanent, verifiable record
in the transcript is the whole of the consequence in this version.

Why this is not a softness. Four adversarial passes each ended with an *honest*
peer's chips taken and its key block-listed for doing exactly what the protocol
required, because the equivocation predicate kept being satisfiable by mandatory
honest behaviour (D-009 rule 1, violated five times). Automated eviction was the
second prize that made those attacks worth mounting. With it gone, a modified
client's best outcome against an honest peer at this layer is a wasted hand —
which is what a flaky connection produces anyway, and which the layer must
survive regardless.

#### 11.5.2 Kept: the transport defending its own resources

These are not eviction and they are not weakened by anything above. Each is
driven by a fact about **our own** memory, sockets, budget or consent, and none
needs any belief about whether the peer is honest:

| Defence | Trigger | Effect | Where |
|---|---|---|---|
| Connection limits | our pending/established connection counts | refuse the new connection | §11.1 |
| Memory limit | process holds ≥ 25 % of system memory | refuse new connections | §11.1 |
| Discovery budget | fresh dials and re-dials per DHT answer, and a provider's cooldown | do not dial | §11.2 |
| DHT-derived address filter | an address from the public DHT carries a private or reserved IP | the dial does not use it | §4.5 |
| Size and canonicality | over cap, non-canonical, unparseable | drop the message or refuse the frame | §6.4, §11.3 |
| Lobby rate budget | this socket exceeds its token bucket | `Ignore` the excess → stop dialling → disconnect | §6.6 |
| GossipSub peer scoring | IP colocation, behaviour penalty (defaults only in v1) | the mesh deprioritises the peer | §6.7 |
| Relay admission (D-002) | the peer is not in the admitted set, or is over its tier's circuit ceiling | refuse the reservation or the circuit | §9.6 |
| Relay capacity | the circuit's advertised `Limit` cannot carry a hand | **we** do not sit down over it and say so; never a refusal of anyone else's seat (§0.5.5) | §9.5 |
| Table membership | the signed roster does not list this `PeerId` | close the stream unread | §8.4 |
| Kademlia client mode at a table | a table this client sits at is closed: every seat taken, or a tournament that has started | `set_mode(Some(Mode::Client))`: no stranger's DHT query is answered until no table of this client is closed | §11.4.3 |

Until 2026-09-15 the last row was *Mainline DHT request filter — any inbound DHT
request at all — deny-all `RequestFilter`*. That filter left with the `mainline`
crate, and the public Kademlia answers strangers' queries on purpose (§11.4.3), so
the row that replaces it is the one moment the client stops answering them.

Three properties hold across every row and are what make them safe to keep:

1. **No refusal is persisted.** No row writes a peer's name to disk as refused.
   Every one is live state rebuilt from the current connection and lobby sets, or
   a standing rule that applies equally to everybody (client mode at a table
   answers nobody's query, naming nobody), and every one lifts as soon as the
   condition behind it does. The one persistent per-peer list this layer keeps —
   §4.6's record of peers that previously completed a poker handshake, dialled
   first on the next start — is a *preference*, never a refusal: being absent
   from it costs a peer nothing, and nothing can put a peer on a persisted list
   of names except §11.5.3.
2. **Nothing is a mark.** A peer refused by any row may retry, reconnect, be
   dialled again on the next discovery cycle, and sit at the same table. Nothing
   in the GUI may present any of these as a sanction, an accusation or a
   reputation score, because none of them is evidence of anything.
3. **Nothing consults a proof.** Every trigger above is a number we measured
   about our own process. If a proposed defence needs to know whether a peer
   cheated, it belongs in §11.5.1 and is forbidden.

#### 11.5.3 Kept: the user's own choice, and it is the only thing that may block

`libp2p::allow_block_list::Behaviour<BlockedPeers>` stays in the behaviour struct
as `user_blocklist` (§5.5). The crate is a non-optional dependency and is linked
whether or not we use it (§5.1), so the constraint cannot be expressed by removing
it — it is expressed by the field name and by this rule:

> **`block_peer` has exactly one caller: an explicit action the user took in the
> GUI.** No protocol event, no proof, no counter, no heuristic and no background
> task may call it. The list is persisted in the user's profile, is shown to the
> user in full, and every entry is removable by the user.

This is D-010's own mitigation and D-011 rule 3's closing sentence: *the user may
always choose not to play with someone; the protocol may not choose for them.*
The client should give the user what they need to make that choice well — D-010
asks for a per-identity abort count visible in the lobby — and then let them
decide. Presenting the count is informing; acting on it automatically is the
thing that is forbidden.

Two honest limits on what a user block achieves, so nobody over-reads it:

* It is **local**. It is one user's client declining to connect to one `PeerId`.
  It is never gossiped, never shared, never aggregated, and no other client learns
  of it. There is no network-wide ban and none may be built (`SPEC_CS.md` §18).
* Identities are free (§12.9). A blocked peer can generate a new keypair in
  milliseconds and reappear. The block is a convenience for the user, not a
  security boundary, and the GUI must not imply otherwise.

---

## 12. What this layer explicitly does **not** solve

Per `SPEC_CS.md` §18 and its closing paragraph, and D-004/D-005's requirement that
real limits be stated rather than hidden. **No claim is made here that the system
makes cheating impossible.**

**Belongs to a higher layer, by design (§1.2):** deck creation, shuffling and
shuffle proofs; card secrecy; board reveal timing; poker rule legality; pot and
side-pot arithmetic; showdown; who won; the hash-chain transcript; replay and
equivocation detection; the disconnect/abort handling of D-005 and the timeout
certificate of D-006 — which since **D-015** is a specification the higher layer keeps
and does not implement, a fact that changes nothing here, because this layer never
carried the object's semantics, only its bytes. The network layer carries the bytes and
nothing else.
**And it does nothing with the verdicts those layers produce**: under D-010 and
D-011 a proof is evidence for a human, so it reaches this layer as no input at
all (§0.1, §11.5.1).

**Nothing above is exercised over libp2p before Phase 7.** Every one of Phases
3–6 runs the poker and cryptographic stack on the `InMemoryTransport` of §1.3.2
and on nothing else, so an unspecified or under-determined harness does not merely
delay testing — it **blocks Phases 3–6**, and it is the only place
`SPEC_CS.md` §25's eleven malicious peers can be run at all before there is a
network.

**Not solved by anybody, and the honest list:**

1. **Symmetric NAT on both ends, with no raised-limit relay available.** Those
   players see the lobby but cannot play. Real, stated in D-004, not fixed here.
2. ~~**UDP blocked entirely.** The Mainline DHT is UDP-only, so discovery dies
   with it.~~ **No longer true, in the client's favour.** Kademlia is a
   `libp2p` behaviour on the same transports as everything else, so a
   UDP-blocked network falls back to TCP for *discovery* exactly as it does for
   the transport. What remains true is that TCP-only play depends on a relay,
   which is item 8.
3. ~~**IPv6-only networks.** `mainline` is IPv4-only.~~ **The cause is gone and
   the limitation partly survived it**: libp2p is dual-stack, but the client
   bound `/ip4/0.0.0.0` and nothing else until 2026-09-02. It now binds
   `/ip6/::` on both transports (§5.8, `S1-Y`). Still **unmeasured against a
   global IPv6 address** — the development machine has ULAs and loopback only —
   so an IPv6-only network is no longer excluded by construction, but neither is
   it demonstrated to work.
4. **Eclipse.** A client whose entire peer set is adversarial sees whatever that
   adversary shows it. The mitigations in §4.6 and §7.2 raise the cost; they do not
   close it. The adversary still cannot forge a table ad or a hand.
5. **Discovery-layer DoS / lobby-key pollution.** Anyone can provide the lobby
   key with junk. We bound our own exposure — `DIALS_PER_ANSWER = 8` — and we
   cannot clean the DHT. Two things changed under Kademlia and they pull in
   opposite directions: a provider record must name the **announcer's own**
   authenticated `PeerId`, so a flooder must spend one identity per entry rather
   than writing arbitrary addresses; but each storing node keeps only **twenty**
   providers per key and **ignores the twenty-first** rather than evicting the
   oldest, so a flooder that gets in first is not displaced by real players.
   §4.6's multiplicity defence is specified and **not implemented**.
6. **Privacy of participation.** A fixed public rendezvous key publishes the
   player's **persistent `PeerId`** together with every address the swarm holds,
   to a stable and publicly computable set of storing nodes, and leaves it there
   for up to **48 hours after the player closes the client** — with no way to
   withdraw it, because `stop_providing` is local. It is worse than the Mainline
   figure this item used to quote in every dimension except one: nobody can
   announce *somebody else's* identity. §3.5 derives the whole of it. Mitigations
   exist (consent screen, direct invite, announce only while looking for a game);
   **none of them eliminates it**, and the *"announce only while looking"* one is
   weaker than it sounds, because leaving does not take the record back.
7. **Relay metadata.** A relay learns who talks to whom, when and how much (D-001).
8. **Relay availability.** If no relay is reachable, a CGNAT-only client cannot
   connect. The UI reports it honestly; the layer does not fix it.
9. **Sybil and multi-account.** Identities are free. Nothing here counts humans.
   This is also why a user block (§11.5.3) is a convenience and not a boundary:
   the blocked identity costs milliseconds to replace.

    **9a. A peer proven to have cheated is not removed by this layer, by
    design.** D-010 and D-011 forbid it, and the honest consequence is that a
    modified client can keep connecting, keep gossiping and keep sitting down
    however many proofs stand against it. What it cannot do is forge an event,
    read a card, take a chip on an abort, or cause an honest peer to be evicted —
    and the last of those is what four passes showed the eviction path actually
    delivered. The remedy is the transcript, which is permanent and public, plus
    the user's own decision (§11.5.3). Stated here rather than left to inference,
    because an implementer who expects the transport to remove a cheater will
    otherwise assume the code is missing.
10. **Collusion out of band, endpoint malware, screen sharing, physical coercion,
    traffic analysis, deliberate disconnection.** `SPEC_CS.md` §18 lists these as
    outside the protocol's reach and this document does not claim otherwise.
11. **Generalisation of the NAT measurements.** One friendly residential network
    was measured. The 70 % ± 7.1 % DCUtR figure is the number that matters for other
    players, and it can be near zero for a specific pair.
12. **Real two-network connectivity has never been tested.** Everything in §4 and
    §9 is API ground truth plus single-host runtime plus one live DHT round trip
    between two processes behind the same NAT. The D-003 / D-004 acceptance test —
    two cold clients on unrelated networks, neither told anything about the other,
    both seeing the same open tables, and the D-004 variant with both behind NAT and
    the D-002 relay disabled — is Phase 8 work and is the criterion this design is
    judged by. It has **not** been passed yet.

    **That paragraph is the Phase 0 record, and its first sentence is no longer
    true (noted 2026-09-15).** Its one live DHT round trip was a Mainline announce.
    With the provider record, two machines on different VLANs, with a firewall
    blocking inbound connections between them, each listed the other's tables —
    and multicast does not cross a VLAN, so the DHT found them. `NEXT.md` records
    that as D-003's test passed by the awkward case. What
    is still untested is a NAT **between** the two, which needs an endpoint
    outside the building, so D-004's variant is not demonstrated.

    **The acceptance test runs on a `CUSTOM` two-seat table**, not on
    `RATED_SNG_POKERTH_V1`. That preset is fully specified but pins `seats = 10`
    and `min_players_to_start = 10`, while `SPEC_CS.md` §32 requires two-player
    heads-up as the first supported mode and §1.3 scopes the MVP at `nlhe/2-6`; it
    is therefore **not playable by the MVP** and becomes playable when `nlhe/7-10`
    lands (`PROTOCOL.md` §13, `STATE_MACHINE.md` §9.5). Judging D-003 and D-004 by
    a configuration the MVP cannot ship would test something nobody can run.

    Two consequences of that choice must be stated rather than discovered later:
    at two seats the required voter set has exactly one member, so `|V| < 2` and
    the deadline machinery does not apply at all (D-007, D-008, §8.4), and the
    acceptance test therefore exercises the transport in exactly the mode where a
    stalled peer has no in-protocol remedy; and a two-seat full mesh is one
    circuit, which is the least demanding case for §9.5's relay arithmetic in
    *circuit count* and therefore proves the least about it — though not in bytes
    per circuit, because under §16.1's bidirectional accounting every circuit
    carries the same `2 × 8 979 B` of shuffle per hand at every table size.

---

## 13. Open questions carried forward

| # | Question | Blocks | Owner phase |
|---|---|---|---|
| **OQ-1** | **Re-aimed 2026-09-02.** The ~45 min figure and the 10-minute re-announce are both gone with Mainline. The open question is now: **what provider-record TTL do the go-libp2p nodes that actually store our record apply?** `libp2p-kad`'s own default is 48 h (in `0.48.0` and `0.49.0` alike), but the storing node stamps expiry from *its* config, and the Amino DHT is overwhelmingly go-libp2p. The operative number is unknown and bounds both §10.1 and §3.5's disclosure. Do not quote 48 h as measured. | §10.1, §3.5 | 8 |
| **OQ-2** | Adopt an epoch-rotating **namespace** for privacy? The same trade as the epoch-rotating infohash was — cross-version compatibility, a midnight rotation race, no help against a live observer — **plus one new cost this mechanism adds**: with a 48 h record TTL the previous day's key stays populated for two more days, so rotation blunts a historical crawl far less than it looks. Still **not** adopted. | §3.5 | later |
| **OQ-3** | ~~Does the one-port rule hold, and does QUIC-then-TCP fallback on a single DHT hint behave?~~ **Closed by the change of mechanism, not by an answer.** A provider record carries every address the swarm holds, so there is no single hint to fall back from and no port to reconcile. The same numeric port is still bound on QUIC, TCP and now IPv6, but for a human writing one router rule rather than for discovery (§4.4). | §4.4 | — |
| **OQ-4** | ~~Ship libp2p Kademlia after all?~~ **Answered by `56b0b50`: it is shipped, and it is the only discovery mechanism.** The cost this question named — a second eclipse surface — was accepted rather than avoided, and the reason is in §5.7: a Mainline announce carries one `IP:port`, so the mechanism `SPEC_CS.md` §1 names cannot make a NATed player findable at all. | §5.7 | — |
| **OQ-5** | `TopicScoreParams` values for both lobby topics. Defaults leave the per-topic terms — including the invalid-message penalty a `Reject` feeds — at zero. Requires measured message rates and mesh sizes; badly tuned scoring graylists honest peers. | §6.7 | 8 |
| **OQ-6** | How often does the relay admission race actually deny an honest peer (reservation request arriving before `identify` completes)? A denial closes the circuit listener with no automatic retry, so our own `listen_on` backoff must cover it. | §9.6 | 7 |
| **OQ-7** | What is the real per-hand byte count over **one** relayed circuit **counting both directions together**, measured against the `Limit` a real relay actually returns? `max_circuit_bytes` is a single bidirectional counter per circuit (§16.1), so a measurement that records one direction and doubles the headroom is wrong by a factor of two — measure the sum, and compare it against `Limit::data_in_bytes()`, which is the same bidirectional figure. The estimate is `2 × 8 979 = 17 958 B` of shuffle plus the signed event stream, order ~20 KB per hand per circuit, against a 131 072 B public-relay budget — roughly five to seven hands, so the binding public-relay limit is still the 120 s `max_circuit_duration` and not the byte cap (§9.5). The measurement is what gives the "refuse to seat rather than start a hand that will drop" rule a number. | §9.5 | 5 → 8 |
| **OQ-8** | What fraction of real peers does AutoNAT v2 confirm as publicly reachable? This sizes the D-002 volunteer relay pool, which is the project's real single point of failure. | §9.6, §12.8 | 8 |
| **OQ-9** | ~~Upstream `mainline`: `announce_peer_detailed` returning `PutOutcome { stored_at }`.~~ **Void — the crate is gone.** The underlying need survives and is sharper: `libp2p-kad`'s `StartProviding` result says the query ended, not how many nodes stored the record — it comes back `Ok` from a walk that asked nobody — so the GUI's health indicator still has no real number. **A thin first walk is repaired now**: the announcement is walked again every 300 s until a node returns this client's own record after a walk has handed it on (§10.1). Until 2026-09-15 this row said *there is no second announce to repair it*, and for a client behind a NAT that was so (`S1-FI`). What stays open is the number. | §2.1 step 5, §10.1 | 8 |
| **OQ-10** | Licence for the project. Unrelated to this document but still open in `DECISIONS.md`. | publication | — |

---

## 14. Constants

**Every two-sided constant is defined in `PROTOCOL.md` §13 and is not restated
here. Changing any of them is a protocol-version change.** That includes every
value this section used to carry: `PROTOCOL_VERSION`, the protocol and topic
strings, the two derivation strings and their rendezvous keys, and every size,
TTL and interval that both sides must agree on. The names in `PROTOCOL.md` §13
are the names a Rust `constants` module carries, and no value has two names
anywhere in the corpus.

> **That last sentence was false until 2026-09-02, from the other end.**
> `PROTOCOL.md` §13 went on defining `LOBBY_DERIVATION_STRING =
> "p2p-poker/mainline-lobby/v1"` and a 20-byte `LOBBY_INFOHASH` four days after
> `56b0b50`, while `run::lobby_namespace` and §3.2 used
> `"p2p-poker/main-lobby/v1"` and the 34-byte multihash `12207e3429…` — one name,
> two values, on the one constant two independent implementations find each other
> by. `S1-AB` corrected §13, which now carries `LOBBY_NAMESPACE_KEY` and
> `RELAY_NAMESPACE_KEY` beside their derivation strings.

Earlier revisions of this document and of `PROTOCOL.md` gave several of these
values two different names and two different numbers; that is
`PHASE0_REVIEW.md` C-1, and one home is the fix.

**Under D-011 rule 1 this discipline reaches every table in the document.** §6.5
and §11.3 each reproduced the size constants in full — §11.3 while warning in its
own first line that the copy might be stale — and both are now maps of *where the
transport enforces which cap*, with the values left to `PROTOCOL.md` §13.
Likewise §6.1's topic strings, §6.2's `max_transmit_size`, §7.1's and §8.1's
protocol strings, §8.4's join and frame caps and §10.3's TTLs now appear by
constant name only. The D-012 sweep found four further copies that were not
constants but definitions — the lobby validation checklist, the snapshot bodies,
the table-stream framing and the lobby freshness rules — and §0.3 lists all seven
sites with their owners.

**One deliberate exception, argued in place:** §3.2 keeps the literal lobby and
relay rendezvous keys — `PROTOCOL.md` §13's `LOBBY_NAMESPACE_KEY` and
`RELAY_NAMESPACE_KEY` — alongside their derivation, because those two values are
*derived rather than chosen*, one shell command resolves any disagreement between
the copies, and `the_lobby_rendezvous_key_is_the_published_one` pins the bytes.
Until `56b0b50` the pair was `LOBBY_INFOHASH` and `RELAY_INFOHASH`. Nothing else
in this document carries a two-sided value.

What remains here is the genuinely **local** tuning — values a peer may change
without breaking interoperability, because no other peer parses or depends on
them.

| Local constant | Value | Where |
|---|---|---|
| GossipSub `mesh_n` / `mesh_n_low` / `mesh_n_high` / `mesh_outbound_min` | 8 / 6 / 12 / 3 | §6.2 |
| GossipSub `heartbeat_interval` | 1 s | §6.2 |
| GossipSub `duplicate_cache_time` | 120 s | §6.2 — must exceed `AD_REBROADCAST_MS` with margin |
| GossipSub `flood_publish` | `false` | §6.2 |
| Per-peer lobby rate budgets (ads, presence, chat, bytes) | §6.6's table | §6.6 |
| Discovery budget: providers dialled per DHT answer, the discovery cycle, query timeouts | §11.2's table | §11.2 — five rows stood here until 2026-09-15 (candidates per cycle, concurrent dials, a per-address timeout, a `/24` cap, a bootstrap backoff ladder); they were the Mainline-era budget, and §11.2 keeps them as the record of what was never built |
| `connection_limits` (pending in/out, established in/out, total, per peer) | 32 / 64 / 128 / 128 / 192 / 2 | §11.1 |
| `memory_connection_limits` share of system memory | 0.25 | §11.1 |
| Relay `Config` (D-002 volunteer) | §9.6's block | §9.6 — local, because a relay's limits are its operator's choice; a client reads the `Limit` the relay actually returns rather than assuming |

A note on the boundary, because it is not obvious in one case: `GOSSIP_MAX_TRANSMIT`
*looks* like local tuning and is not. A peer configured at 64 KiB rejects a larger
frame outright, so the value is two-sided and lives in `PROTOCOL.md` §13. The mesh
parameters beside it genuinely are local: a denser mesh costs only its owner.

---

## 15. Decision register — where each binding decision is honoured

| Decision | Sections |
|---|---|
| **D-001** relays permitted; costs documented; two roles never blurred | §9.5, §9.7, §12.7, §12.8 |
| **D-002** publicly reachable client volunteers as relay, off by default, admission via a **named type implementing `libp2p::relay::RateLimiter`** — the trait is re-exported at the crate root (`libp2p-relay-0.22.0/src/lib.rs:42-44`), the type holds the live admitted-peer set, and it is installed into **both** `reservation_rate_limiters` and `circuit_src_rate_limiters`; open question settled | §9.6 |
| **D-003** global lobby visibility is the acceptance criterion; the bridge is the load-bearing step; there is no port to announce, because a provider record carries the swarm's external addresses — D-003's *announce `Some(external_quic_port)`* was Mainline's rule and is history | §2, §4, §4.4, §12 |
| **D-004** lobby visible even if every client is behind NAT; four layers; symmetric-both-ends stated | §9.7, §9.3, §12.1 |
| **D-005** absent seat and mid-hand abort — **not** a transport concern. Its forfeiture rule is **revised by D-010**: an abort is neutral, stacks are restored, and the signed attribution has no automatic consequence at any layer | §1.2 rule 3, §8.4, §10.4 |
| **D-006** action timeout is auto check/fold, never an abort; timeout certificate; no timeout ends the game — **as corrected by D-007 and generalised by D-008** | §1.2 rule 3, §8.4, §10.4 |
| **D-007** corrects D-006: at two seats the action deadline is advisory, a fold-effect timeout certificate is forbidden, and no document may claim the certificate protects a two-seat table. The transport layer's behaviour is unchanged — connection loss stays a hint at every table size — and the transport must not invent a substitute verdict | §8.4, §1.2 rule 3, §10.4, §12 |
| **D-008** generalises D-007: every rule that weakens, disables or gates the timeout certificate is scoped on the **size of the required voter set `V`** and **never on `n`**, the seat count. A certificate whose required voter set has fewer than two members has no effect; a seat leaves `V` only once a completed, valid certificate names it, so being voted against is not exclusion. This layer states no rule scoped on `n`, and the transport's own behaviour is unchanged at every size of `V` | §8.4, §1.2 rule 3, §10.4, §12 |
| **D-009** rule 1 — mandatory honest behaviour may never satisfy the equivocation predicate. The slot key is `PROTOCOL.md` §5.2's and this document reproduces no part of it; the `InMemoryTransport` conflict injector is written against that section rather than against a copy, because a harness built on a stale copy tests the copy. Rule 3 — no security property is stated as an absence from the dependency tree: the register of §5.1.1 states what each crate *is* and flags the two unaudited ones, and `libp2p-allow-block-list` is explicitly recorded as unavoidable in the link (§5.1) rather than claimed absent | §1.3.2(iii), §5.1, §5.1.1 |
| **D-010** point 3 — **no automated eviction.** Every `block_peer` on a protocol proof is deleted from this document; a proof produces no transport action of any kind. Point 2's neutral attribution is recorded where the transport touches the abort path. What is *kept* is every defence whose reason is a resource, rate, size, admission, capacity or membership fact, plus the user's own explicit choice — and the boundary between the two is drawn once, in §0.2, so an implementer cannot read "no eviction" as "no defences" | §0 (sweep table §0.4), §1.2 prohibition 7, §5.5, §6.6, §7.4, §8.4, §10.4, §11.5, §12.9a |
| **D-011** rule 1 — this document is the normative owner of transport, discovery and connectivity, and restates nothing another document owns. Seven restatements are now pointers, listed site by site in §0.3: the slot key, the unchained sentinel envelope, every two-sided size constant, the lobby validation checklist, the snapshot request and response bodies, the table-stream framing and its sizing argument, and the lobby freshness and skew rules. Rule 2 — the slot key is that one tuple and appears here **only** by reference. Rule 3 — D-010 point 3 binds this layer, which is what §0 exists to record | §0, §0.3, §1.3.2(iii), §6.4, §6.5, §7.3, §8.4, §10.3, §11.3, §11.5, §14 |
| **D-012** — no canonical state from a per-receiver quantity. This layer produces almost all of them, so the rule lands here as a constraint on outputs: prohibition 8 in §1.2, the catalogue of every per-receiver quantity and where each may go in §0.5.2, the one transport value that legitimately sits in a chained body and why it is safe in §0.5.3, clock-derived acceptance confined to unchained lobby traffic in §0.5.4, relay capacity gating only our own request for a seat in §0.5.5, and the snapshot merge and the lobby TTLs confirmed as local views in §7.4, §7.5 and §10.4. **The one site reported and not fixed is now fixed**, by `PROTOCOL.md` §3.1 under D-013's J1 rule: `advert_hash` no longer reaches `GENESIS(0)`, `session_id` or `ctx`, `table_params_hash` replaced it, and this layer has **no** remaining per-receiver quantity with a canonical destination — §0.5.6 records the disposition and why neither remedy this document proposed was the one adopted | §0.5, §1.2 prohibition 8, §2.2, §6.4, §7.3, §7.4, §7.5, §8.4, §9.5, §9.6, §10.3, §10.4, §11.5.2 |
| **D-013** — liveness is inherited from the chain, not from a seat's status. The rule, the required set and its notation are `PROTOCOL.md` §3.2's and this document reproduces none of them. What lands here is one prohibition and one retirement. **The prohibition (§0.5.7):** connection state, drop time, relay status and keep-alive results may never contribute to the participation record that decides who must emit next hand — they are §0.5.2's first row, they agree with the chain most of the time, and that is what makes substituting them attractive and the resulting fork rare and hard to reproduce. This is §1.2 prohibition 8 with a **new consumer**, not a new rule: the prohibition is unchanged, but a wrong answer now decides a required emitter set rather than a status nothing acts on, so its failure mode moved from a wasted hand to `THREAT_MODEL.md`'s **X36**. **Re-entry is signed, never sensed:** a reconnected socket, a re-established reservation or a peer reappearing in a snapshot is not a re-entry and may not be reported as one. **The retirement:** §0.5.6's open site, and with it the last qualification on §0.5.2's merged-lobby-view row | §0.5.6, §0.5.7, §1.2 prohibition 8, §1.3.1, §7.4, §8.4, §10.4, §11.5 |
| **D-014** — a cheater is removed from the table on self-authenticating evidence. **This layer's disposition is: no change to any behaviour, and a change to two scope words.** D-014 narrows D-010 point 3 above this layer and nowhere in it; §0.1 and §1.2 prohibition 7 stated that rule in the every-layer form D-011 rule 3 gave it, so until this pass this document — carrying **zero** occurrences of `D-014` — asserted the negation of a binding decision rather than merely omitting it. Both now say **at this layer**, and both name the exception without restating it (D-011 rule 1: the decision is `DECISIONS.md`'s, the wire `PROTOCOL.md` §4.0/§4.9's, the seat `STATE_MACHINE.md` T64/T65/I34's, the proofs `CRYPTOGRAPHY.md` §8.1's). The exception is **one** input — a tier-1 finding: an event signed by the accused whose illegality any peer decides alone from that event's own bytes — and **one** consequence: the loss of a **seat**. No socket is closed, no dial refused, no reservation withdrawn and no name persisted by it; `block_peer` keeps its single caller, the user (§11.5.3). Every liveness and attribution judgement this layer can make stays prohibited as an input to any removal, at any layer (§0.6.3), and every §0.2 "kept" defence — resource, rate, size, admission, capacity, membership, user choice — is untouched and must stay visibly separate from it. §11.5.1 additionally carries the paragraph that stops an implementer arriving from T64 reading it as licence to block | §0, §0.1, §0.2, §0.6, §1.2 prohibition 7, §11.5.1, §11.5.3 |

---

## 16. Objections to the fix plan

`PHASE0_FIXPLAN.md` is applied as written throughout this document. One ruling was
applied under objection; the objection was **upheld** in `PHASE1_VERIFY.md` (N1,
and the assessment of this section in Part 3.5), and the corrected numbers are now
carried in §9.5, §9.7 layer 3 and OQ-7. The record stays here because the
superseded figure was quoted by four documents and a reader who meets it elsewhere
needs to find out where it went.

### 16.1 A-8 — `max_circuit_bytes` is per circuit, but **not** per direction

**Status: objection upheld, correction applied.** §9.5 no longer carries the
per-direction figure. What follows is the record of why.

**The ruling.** A-8 stated that *"`max_circuit_bytes` is per circuit and per
direction"*, and derived from that a public-relay budget of ~14 hands per
direction (131 072 / 8 979). An earlier revision of §9.5 carried those numbers
verbatim, as instructed.

**The objection.** The "per direction" half does not hold in
`libp2p-relay 0.21.1`, nor in the `0.22.0` the build uses since `S1-FF`, whose
`src/copy_future.rs` is the same file apart from test formatting. A circuit is
relayed by a single `CopyFuture` holding one `bytes_sent: u64` counter, and
**both** directions increment that one counter before it is compared against the
cap:

```rust
// src/copy_future.rs:41-48, 78, 88-106 (the same lines in 0.21.1 and 0.22.0)
if this.max_circuit_bytes > 0 && this.bytes_sent > this.max_circuit_bytes { … }
let src_status = match forward_data(&mut this.src, &mut this.dst, cx) { … this.bytes_sent += i … };
let dst_status = match forward_data(&mut this.dst, &mut this.src, cx) { … this.bytes_sent += i … };
```

The crate's own quickcheck states the same thing from the outside: on the
`"Max circuit bytes reached."` error it asserts
`a.len() + b.len() > max_circuit_bytes as usize` (`src/copy_future.rs:241`) —
the two directions summed against one cap.

The cap is therefore per circuit and **bidirectional**. A-8's citation
(`src/behaviour.rs`, `impl Default for Config`; `src/behaviour/handler.rs`)
establishes the default value and that the value is handed to each circuit; it
does not reach the accounting, which lives in `src/copy_future.rs`.

**What changed when the objection was upheld.** Over one relayed circuit between
two seats, each seat sends its own step and proof once per hand, so the circuit
carries `2 × 8 979 = 17 958 B` of shuffle per hand, not 8 979 B. The
public-relay budget is **~7 hands**, not ~14, and the "including the signed
event stream" figure halved with it, from ~10 hands to ~5. Every other number in
A-8 is unaffected: `8 979 B` per shuffler is correct, and `(n−1) × 8 979 B` as a
relayed peer's total per-hand outbound is correct — that figure is spread over
`n−1` circuits, so the halving does not touch it. Decisively, **the ruling's
conclusion survives**: at 7 hands as at 14, the byte cap is not what binds, and
the 120 s `max_circuit_duration` still is. The correction changes a comfort
margin, not a design decision — and it changes it in the direction that
*strengthens* D-001's addendum, since a public relay is now short of budget on
both of its defaults rather than only on one (§9.5).

**Why it was worth recording.** OQ-7 asks for this number to be measured against
a real relay's returned `Limit`. Whoever runs that measurement would have read
the accounting as per-direction, measured one direction, and concluded there was
twice the headroom there is; OQ-7 is now stated as a bidirectional measurement
for exactly that reason. `SPEC_CS.md` §36 forbids carrying a claim stronger than
its evidence, and "per direction" was one such claim, small as its consequence is
here.

**The one place the question is not settled.** kubo's `docs/config.md` describes
`ConnectionDataLimit` as applying "in each direction". Either the Go and Rust
implementations differ or that wording is loose; the Go source has not been read
for this document and no claim is made about it. Our own relay is Rust, so the
Rust behaviour binds us; against a third-party relay the stricter reading is
assumed (§9.5).
