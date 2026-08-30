# Decisions

Binding project decisions made by the project owner, outside and above the
research documents. A research document may argue against one of these; the
decision still stands, and the document must record the decision and its
consequences rather than contradict it.

Each decision is numbered and permanent. If one is reversed, it is superseded
by a new numbered decision, never edited away.

---

## D-001 — Circuit Relay v2 is permitted

**Date:** 2026-08-28
**Decided by:** project owner
**Status:** accepted

### The question

`docs/SPEC_CS.md` section 1 requires relay fallback via Circuit Relay v2 so that
clients behind CGNAT can still play. Section 3 forbids creating a hidden central
fallback server. These pull against each other: a relay only exists if somebody
runs one, and shipping a list of relay addresses with the client is, on a strict
reading, infrastructure the project depends on.

### The decision

**Relays are allowed.** The client may use Circuit Relay v2 as a connectivity
fallback, and may ship with a bootstrap list of relay addresses. This is not
treated as a violation of section 3.

### Why this does not weaken the security model

A relay carries an already end-to-end encrypted, mutually authenticated libp2p
stream. It is a byte pipe. Specifically, a relay operator:

- cannot read poker events — the transport is Noise or TLS 1.3 between the two
  endpoints, terminated at the peers, not at the relay;
- cannot forge or alter a poker event — every event is signed with the sender's
  application key, and every receiver re-validates the signature and the hash
  chain independently (`SPEC_CS.md` sections 12, 13, 14);
- cannot become an authority over the lobby or over a hand — it never
  participates in the protocol, holds no key share, and is not a party to any
  table session;
- cannot decrypt a card — it holds no share of the joint deck key.

So a malicious relay is, at worst, a network adversary that already appears in
the threat model.

### What it does cost, and what must therefore be documented

1. **Metadata exposure.** A relay learns which peers talk to each other, when,
   how much and for how long. That is a real privacy loss and belongs in
   `THREAT_MODEL.md` under traffic analysis, not glossed over.
2. **Liveness dependency.** If every reachable relay is down, a CGNAT-only
   client cannot connect at all. The failure mode must be reported honestly in
   the UI ("no direct route and no relay available"), never disguised.
3. **A denial-of-service lever.** A relay operator can drop a specific peer.
   The protocol must treat a relayed connection loss the same as any other
   disconnect, and the disconnect/abort handling of `SPEC_CS.md` section 19 has
   to cover it.

### Addendum, 2026-08-28 — what public relays actually are, measured

The question came up whether public relays are already running on the internet
and can simply be used. They are, in large numbers, but their default limits
make them unusable as a session transport. This was verified in source, not
assumed.

**Public relays exist and need no hardcoded list.** In kubo (the reference IPFS
implementation) `Swarm.RelayService.Enabled` defaults to `true`, so every
publicly reachable kubo node offers Circuit Relay v2 to the network, and
`Swarm.RelayClient.Enabled` also defaults to `true`, so a client discovers
public relays from the network on its own when it detects it is unreachable.
Verification: `docs/config.md` on the kubo master branch, sections
`Swarm.RelayService.Enabled` and `Swarm.RelayClient.Enabled`.

**Their default limits are two minutes and 128 KiB per relayed connection.**
Both the Go and the Rust implementation ship the same numbers:

| Limit | kubo default | rust-libp2p `relay::Config` default |
|---|---|---|
| Relayed connection duration | `"2m"` | `max_circuit_duration: 2 * 60 s` |
| Relayed data, each direction | `131072` (128 KiB) | `max_circuit_bytes: 1 << 17` |
| Reservation TTL | `"1h"` | `reservation_duration: 60 * 60 s` |
| Max reservations | `128` | `max_reservations: 128` |
| Max circuits | `16` | `max_circuits: 16` |
| Per peer | — | `max_circuits_per_peer: 4` |

Verification: kubo `docs/config.md` (master); rust-libp2p
`libp2p-relay 0.21.1`, `src/behaviour.rs`, `impl Default for Config`, read from
the unpacked crate source.

**Consequence.** Two minutes and 128 KiB is sized for exactly one thing:
coordinating a DCUtR hole punch, after which the peers talk directly and the
relay drops out. It is not one poker hand. A single hand carries several
shuffle proofs plus the signed event stream, and a session lasts far longer
than two minutes. A public IPFS relay will reset the connection mid-hand.

So the relay story splits in two, and the documents must not blur them:

1. **Rendezvous for hole punching.** Public relays are entirely adequate, cost
   us no infrastructure, and need no shipped address list — AutoNAT plus the
   standard relay discovery finds them. This is the common path.
2. **Carrying a whole session, when DCUtR fails on both ends.** Public relays
   cannot do this. It needs a relay that has raised its own limits. In
   `rust-libp2p` every field of `relay::Config` is `pub`, so
   `max_circuit_duration` and `max_circuit_bytes` are ours to set on a relay we
   control.

This is what makes the "any publicly reachable peer volunteers as a relay"
constraint below load-bearing rather than decorative: a publicly reachable
poker client running the relay server with limits raised for our own protocol
is the only way to get an unlimited relay without operating infrastructure.
Whether that is acceptable — a client spending its own bandwidth relaying
strangers' games — is an OPEN QUESTION for the owner, and it must be a visible,
consenting setting, never silently on.

### Constraints that remain in force

- A relay is **never** trusted with poker content, never given a key share,
  never asked to arbitrate a dispute, and never used as a lobby authority.
- The client must prefer a direct connection: AutoNAT to learn reachability,
  then DCUtR hole punching, and only then relay. A relayed connection is a
  fallback, not the default path.
- Any publicly reachable peer should be able to volunteer as a relay, and
  relays discovered at runtime are preferred over the shipped list, so the
  shipped list is a bootstrap convenience rather than a permanent dependency.
- The relay set must be visible to the user in the network status panel
  (`SPEC_CS.md` section 22), including whether the current connection is direct
  or relayed.

---

## D-002 — A publicly reachable client volunteers as a relay

**Date:** 2026-08-28
**Decided by:** project owner
**Status:** accepted
**Depends on:** D-001

### The decision

A client that AutoNAT reports as publicly reachable runs the Circuit Relay v2
**server** and relays other players' games over its own connection, with limits
raised well above the defaults so a relayed connection can carry a whole
session rather than only a hole-punch coordination.

This is what makes CGNAT-to-CGNAT play possible at all without the project
operating any infrastructure, since public IPFS relays cap a relayed connection
at 2 minutes and 128 KiB (see the D-001 addendum).

### Verified: the limits are ours to set

`relay::Config` exposes every field publicly, and raising them from outside the
crate compiles and runs. Verification: probe crate against `libp2p 0.56.0`
with the `relay` feature, `cargo check` clean, binary run, printing

```
Config { max_reservations: 512, max_reservations_per_peer: 4,
         reservation_duration: 3600s, reservation_rate_limiters: "[3 rate limiters]",
         max_circuits: 16, max_circuits_per_peer: 4,
         max_circuit_duration: 3600s, max_circuit_bytes: 1073741824, ... }
```

### The trap, and the way out

**Circuit Relay v2 is not protocol-selective.** The hop and stop protocol names
are compile-time constants in `libp2p-relay 0.21.1`:

```rust
pub const HOP_PROTOCOL_NAME: StreamProtocol =
    StreamProtocol::new("/libp2p/circuit/relay/0.2.0/hop");
```

They cannot be renamed. Enabling the relay server therefore advertises the
standard, network-wide relay protocol, and the `Behaviour` itself decides
accept-or-deny purely on resource limits and rate limiters — it has no
application ACL hook. Verification: `src/behaviour.rs`, the
`handler::Event::ReservationReqReceived` arm, read in full. Left alone, we
would be running an **open relay for the entire libp2p world**, IPFS traffic
included, on the user's line. That is not what was agreed to.

**The usable hook is `Config::reservation_rate_limiters` and
`Config::circuit_src_rate_limiters`.**

> **Correction, 2026-08-28.** An earlier version of this decision said the
> element type "sits in a `pub(crate)` module and so cannot be named from
> outside", and recommended coercing a closure through the crate's blanket
> `impl<T: FnMut(..)> RateLimiter for T`. **That was wrong**, and the Phase 0
> review (finding B-2) caught it. The module is `pub(crate)`, but the trait is
> re-exported at the crate root:
>
> ```rust
> // libp2p-relay-0.21.1/src/lib.rs:42
> pub use behaviour::{rate_limiter::RateLimiter, Behaviour, CircuitId, Config, Event, StatusCode};
> ```
>
> so `libp2p::relay::RateLimiter` is nameable and implementable. The error came
> from reading `behaviour.rs` and not checking `lib.rs` for re-exports. The
> closure route did compile, so nothing built on the wrong reason - but the
> right construction is better, because admission control needs shared mutable
> state and a closure would have to capture it awkwardly.

Admission control is therefore a **named type** implementing the trait, holding
the live set of peers the lobby has seen:

```rust
struct PokerPeersOnly { known: Arc<Mutex<HashSet<PeerId>>> }

impl libp2p::relay::RateLimiter for PokerPeersOnly {
    fn try_next(&mut self, peer: PeerId, _addr: &Multiaddr, _now: web_time::Instant) -> bool {
        self.known.lock().map(|k| k.contains(&peer)).unwrap_or(false)
    }
}
```

Verified by compiling and running exactly this against `libp2p 0.56.0`:

```
stranger admitted: false
our peer admitted: true
reservation limiters: 3, circuit limiters: 3
```

Both vectors must be gated - one governs who may reserve a slot to become
reachable through us, the other who may open a circuit through us. The third
parameter is `web_time::Instant`, so a `web-time = "1"` dependency is needed to
write the signature.

It sees the `PeerId` and the `Multiaddr` of whoever asks, and refuses
anyone who is not one of ours.

### Required behaviour

1. Relaying is **off by default** and turned on by an explicit, visible setting.
   The first run must disclose plainly what it means: strangers' poker traffic
   crossing the user's connection, at the user's bandwidth cost.
2. When on, the relay serves **only our own network**. The admission closure
   admits a peer only once it is known to be a poker peer — identified by our
   protocol name through `identify`, or already seen in the lobby. Unknown
   peers are refused. Without this the client is an open relay.
3. Bandwidth and slot ceilings are user-visible and user-settable, and the
   network status panel shows how many peers are currently being relayed.
4. The relay never sees plaintext and never becomes an authority — the
   constraints of D-001 all continue to apply.

### Open question

Whether a peer that has never been seen before, and is behind CGNAT, can obtain
a reservation at all. Admission by `identify` protocol name lets a stranger in
as long as they run our client, which is the point; admission by lobby presence
is stricter but keeps a brand-new client out. `NETWORK_STACK.md` must settle
this and state the trade-off.

---

## D-003 — Global lobby visibility is the acceptance criterion

**Date:** 2026-08-28
**Stated by:** project owner
**Status:** accepted

### The requirement, in the owner's words

Everyone running this application, anywhere on the internet, must see which
tables are open in the public lobby.

That is the bar the discovery design is measured against. Not "usually", not
"if the network cooperates" — a client that starts anywhere and reaches the
internet sees the same list of open tables as everyone else.

### Why this is achievable, and the part that matters

**Seeing the lobby does not require being reachable.** This is the point that
makes the requirement far easier than the connectivity discussion in D-001 and
D-002 suggests. A client behind NAT or CGNAT dials *outward*, and outbound
connections work everywhere. Once it has one connection into the GossipSub
mesh it receives every table advertisement and can publish its own. Inbound
reachability, hole punching and relays matter for *playing a hand* against
another unreachable peer — not for *seeing* the lobby.

So the chain is:

```
announce_peer(LOBBY_INFOHASH)   -> we are findable
get_peers(LOBBY_INFOHASH)       -> a list of IP:port, unverified, a hint only
dial each over QUIC             -> identity settled by the handshake
subscribe the lobby topic       -> live table ads from the whole network
snapshot request to a few peers -> the tables that already existed
```

### Verified: the DHT-to-libp2p bridge

The load-bearing step is turning a bare `IP:port` with no PeerId into an
authenticated connection. Verified by compiling and running against
`libp2p 0.56.0`:

```rust
let from_dht = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 43117);
let addr: Multiaddr = Multiaddr::empty()
    .with(Protocol::Ip4(*from_dht.ip()))
    .with(Protocol::Udp(from_dht.port()))
    .with(Protocol::QuicV1);
let opts = DialOpts::unknown_peer_id().address(addr).build();
// -> /ip4/203.0.113.7/udp/43117/quic-v1, peer_id None
```

`mainline`'s `get_peers` returns exactly `Vec<SocketAddrV4>`, which is the input
this takes. Identity comes from the Noise/TLS handshake, never from the DHT
entry — which is what `SPEC_CS.md` section 1 requires when it says the DHT list
is a hint about where to try, never a claim about who is there.

### The one precondition, stated honestly

The mesh only forms if **at least one client is dialable**. If every user were
behind CGNAT and nobody relayed, no client could reach another and no lobby
would exist. This is not a theoretical worry for a small user base — it is the
single realistic way this requirement fails, and it is exactly why D-002 was
accepted.

Mitigation: any publicly reachable client is automatically useful to everyone
else, both as a dial target and, under D-002, as a relay. The client should
therefore report its own reachability prominently, because a user who can open
a port materially helps the whole network.

### Which port to announce

`mainline::Dht::announce_peer(info_hash, port: Option<u16>)`, per its own doc
comment: the peer is announced on the process's IP as the DHT sees it, and

- `None` sets BEP 5 `implied_port`, so remote nodes record **the source port of
  our DHT packet** — the DHT socket's NAT-mapped port, which is not our QUIC
  port;
- `Some(p)` records the literal port `p`, which must be our *externally*
  reachable QUIC port, not the internal one.

So we announce `Some(external_quic_port)`, learning the external port from
`identify`'s observed address or AutoNAT once we have any connection; announces
repeat periodically, so the first one being imperfect is survivable. For a
publicly reachable peer, internal and external port are the same and this is
simply correct — and those are precisely the entries worth dialing. A NATed
peer that announces a port nobody can reach produces a dead entry, which costs
a dial timeout, not a correctness failure. The DHT list is a hint; dead hints
are expected.

Note for implementation: the synchronous `Dht::announce_peer` is marked
`#[deprecated(note = "use the async API via Dht::as_async() instead")]`, so the
async `AsyncDht` API is the one to build on.

### Acceptance test

Two clients on unrelated networks, neither told anything about the other, both
started cold. Within a bounded time both show the same set of open tables, and
a table opened by either appears in the other's lobby. This test belongs in the
Phase 8 integration suite and is the criterion this decision is judged by.

---

## D-004 — The lobby must be visible even if every client is behind NAT

**Date:** 2026-08-28
**Stated by:** project owner
**Status:** accepted
**Sharpens:** D-003

### The requirement

D-003 said the lobby must be visible to everyone. The owner sharpened it: that
must hold **even in a world where no client at all is publicly reachable**. The
precondition D-003 recorded — "at least one client must be dialable" — is not
accepted as a get-out. Design for the all-NAT case.

### Why this is achievable

It is achievable, and it does not need anything the project has to operate. The
answer is four independent mechanisms, tried in order. The first needs nothing
beyond the DHT.

**Layer 0 — the DHT does not care about NAT.** Mainline DHT is millions of
publicly reachable BitTorrent nodes, and our client only ever talks to them
outbound. `get_peers(LOBBY_INFOHASH)` returns the other poker clients' external
`IP:port` whether or not we are reachable and whether or not they are. Announce
works the same way: the receiving node records the source address it actually
saw. This is exactly why BitTorrent swarms form at all when most peers are
behind NAT. `mainline` supports this directly with `Dht::client()`, which
participates without serving.

So *discovering that other players exist, and where they appear from*, already
survives an all-NAT world. Nothing else is required for that step.

**Layer 1 — mutual dialing, which is hole punching with no relay at all.**
Both A and B announce, and both call `get_peers`, so **both independently learn
the other's external address at roughly the same time**. If both then dial, the
outbound packets open the NAT mapping on each side and the connection
completes. There is no coordination server in this, because the DHT already
delivered the information symmetrically. This is classic UDP hole punching, and
it is the mechanism that makes the all-NAT case work.

It requires endpoint-independent mapping on at least one side. Measured on the
development machine, using one local UDP socket against four different STUN
servers:

```
stun.l.google.com     -> ('198.51.100.17', 64707)
stun1.l.google.com    -> ('198.51.100.17', 64707)
stun.cloudflare.com   -> ('198.51.100.17', 64707)
stun.nextcloud.com    -> ('198.51.100.17', 64707)
local socket bound to port 64707
```

The same external port to every destination is endpoint-independent mapping,
and the external port equals the local port, so this NAT also preserves port
numbers. The external address is a public one, not `100.64.0.0/10`, so this
machine is not behind CGNAT. One router is not a survey — but this is the
common home case, and it is the case layer 1 is built for.

**Layer 2 — DCUtR over a public relay**, when blind mutual dialing does not
converge because of timing or address-dependent filtering. Public relays are
abundant and free (see the D-001 addendum), and their 2-minute, 128 KiB limits
are sized for exactly this: coordinate the punch, then get out of the way.

**Layer 3 — lobby gossip over a public relay.** A table advertisement is a few
hundred bytes. Even a public relay's 128 KiB budget carries a great deal of
lobby traffic, and the 2-minute reset is survivable for gossip because the
client simply reconnects. So in the worst case, where no punch succeeds
anywhere, **lobby visibility still survives on public relays alone**. This is
the floor under D-003, and it is what makes the requirement unconditional.

**Layer 4 — a D-002 relay with raised limits**, needed only to *play a hand*
when both players are unreachable and no punch worked. Playing is where the
2-minute limit actually bites; reading the lobby is not.

### The case that genuinely fails

Symmetric NAT on both ends — a different external port per destination —
defeats address prediction, so layers 1 and 2 fail. Those players fall through
to layers 3 and 4: they still see the lobby, and they can still play if a
relay with raised limits is available. If neither is, they cannot play. That
limit is real and belongs in `THREAT_MODEL.md` and in the user-facing
limitations, not hidden.

### Consequence for the announced port

Layer 1 only works if the port announced in the DHT is the external port of our
QUIC socket. On a port-preserving NAT like the one measured above, the local
port is already correct. Elsewhere it is not, so the client announces its local
QUIC port on the first pass and corrects it as soon as `identify` reports an
observed address or AutoNAT resolves. Announces repeat periodically, so a wrong
first announce self-corrects within one cycle and costs other peers a dial
timeout in the meantime.

### Acceptance test, revised

The D-003 test, run with **both** clients behind NAT and neither port-forwarded,
with the relay of D-002 disabled, so that only layers 0 to 3 are in play. Both
must still see the same set of open tables.

---

## D-005 — A disconnected player keeps their seat and is blinded off

**Date:** 2026-08-28
**Stated by:** project owner
**Status:** accepted, with one part deviating from live rules by necessity

### The requirement

In both tournaments and cash games, if a player disconnects the game must go on
for everyone else. The absent player's seat keeps its stack, and the blinds
(and antes) eat it, as international tournament rules prescribe for an absent
player.

### Part one: the game continues. Fully achievable.

From the next hand onward the absent player is simply **not a party to the
cryptography**. They are not in the joint key, they are not dealt cards, and
nothing about the protocol waits for them. Their seat is a chip pile that pays
its blinds and antes and folds when action reaches it. The stack drains one
orbit at a time until it is gone and the seat busts, exactly as it would live.
On reconnect they rejoin the key-holder set at the next hand boundary.

This costs nothing and needs no new cryptography, because a player who is never
dealt in never has to open anything.

### The one deviation from live rules, and why it is forced

Under TDA-style rules an absent player is still dealt in, and if their stack is
smaller than the blind they are all-in for it and their hand goes to showdown —
they can win. We cannot do that. Opening their cards at showdown needs their
decryption share, and they are not there to publish it. Nobody else can produce
it, and any mechanism that let the others produce it would be exactly the
mechanism `SPEC_CS.md` section 19 forbids: a way for a subset of players to
decrypt an absent player's hole cards. That is not a corner case to trade away,
it is the attack — collude, "disconnect" a player, read their cards.

So an absent seat posts its blind as dead money and takes no cards. It cannot
win the hand it pays for. The practical difference from live rules is small:
absent players lose their blinds either way, and only rarely win an all-in.
The difference must still be documented, not smoothed over.

### Part two: the hand already in progress. This is the hard one.

A player who vanishes mid-hand was dealt in, so they hold a share of the n-of-n
deck key. Until they publish, **no further card can be opened by anyone** —
not the flop, not the other players' showdown cards. The hand cannot be
finished by the remaining players. This is inherent to the construction, and
`SPEC_CS.md` section 19 already anticipates it: define a timeout, a hand abort,
evidence of which peer failed, and a penalty.

Two cases:

1. **The hand can still be decided without opening anything.** Everyone else
   folds to one player. Then it completes normally — no shares are needed.
   Notably, a player who quits to escape a loss does not escape if the others
   simply fold.
2. **Anything else** — a timeout expires, the hand aborts with a signed record
   of which peer failed to publish.

### What happens to the chips on abort, and why

This choice matters more than it looks, because the two obvious options each
open a different exploit.

- **Restore every stack to its start-of-hand value.** Chip-conserving and
  simple, but it hands every player a free escape: a player about to lose a big
  pot disconnects, the hand is voided, and they get their money back. That is
  an *in-protocol* exploit, available to anyone, for free, with no special
  capability. Unacceptable.
- **The absent player forfeits what they committed; it is distributed to the
  remaining players in proportion to their own contributions.** Chip-conserving,
  and it removes the escape entirely — quitting costs exactly what folding
  would have cost. Its weakness is a DoS incentive: an opponent who could knock
  a player offline right after a big bet would collect it.

**We take the second.** The rage-quit escape is an in-protocol exploit that
anyone can use at will and must be closed; knocking a peer off the network is
an out-of-protocol attack that the threat model already lists as out of scope,
and it requires real capability against the victim's connection. Closing the
free exploit at the cost of a documented, expensive one is the right trade.

It is also the closest chip-conserving analogue to the live rule: in a real
game a player who cannot act has their hand folded and loses what they bet.
Here the hand cannot continue, so their commitment goes to the players who were
still in it.

### What must follow

- The timeout, the abort, and the attribution are protocol events, signed and
  in the transcript, so every participant can verify who failed and when.
- Repeated aborts attributable to one identity are visible to everyone. In play
  money that is the whole penalty, and it is enough; `SPEC_CS.md` section 18
  forbids claiming more.
- No anti-disconnect mechanism may ever expose an absent player's hole cards.
  This stands above the convenience of finishing a hand, per section 19.
- `STATE_MACHINE.md` must carry the absent-seat states, the timeout states and
  the abort path; `THREAT_MODEL.md` must carry both exploits above, with the
  DoS one classified honestly as out of scope rather than solved.

---

## D-006 — An action timeout is an auto check/fold, never an abort

**Date:** 2026-08-28
**Stated by:** project owner
**Status:** accepted
**Corrects a conflation in:** D-005

### The requirement

A player who fails to decide inside the time limit is automatically checked or
folded, the betting round finishes among the other players, the next street
card comes, and the next hand starts. A timeout must **never** tear down the
tournament or the cash game.

### The distinction D-005 blurred, and which matters

D-005 treated "timeout" as one thing. It is two, and they have nothing in
common but the word.

**An action timeout is not a cryptographic event at all.** Publishing a
decryption share is something the *client* does automatically as a protocol
step — it is not a decision and it never waits for the human. So a player who
walks away from the keyboard is still fully cooperating cryptographically: the
board opens on schedule, showdowns work, everything proceeds. The only thing
missing is a betting decision, and poker has had an answer to that forever.

| | Human absent, client running | Client gone or withholding |
|---|---|---|
| Betting decision | auto check/fold | auto check/fold |
| Decryption shares | published normally | **not published** |
| The hand | **continues to the end** | cannot open further cards |
| Consequence | next street, next hand | D-005: abort, attribute, next hand |

Only the right-hand column is hard. The left-hand column — which is the
overwhelmingly common case, the player who steps away — costs nothing.

### The rule

1. Each seat has a decision deadline. On expiry the engine applies **check** if
   nothing is owed, **fold** if facing a bet. Never fold a hand that could check
   for free.
2. The auto-action is a real, signed protocol event in the transcript, not a
   local UI convenience. Every peer derives it identically from the same state,
   so it stays deterministic and verifiable per `SPEC_CS.md` sections 11 and 13.
3. The betting round then continues among the remaining players, the street card
   opens, and the hand plays out normally. Nothing aborts and nothing waits.
4. After a configured number of consecutive auto-actions the seat is marked
   sitting out and enters the D-005 absent-seat state: it keeps its stack, pays
   its blinds and antes, takes no cards, and drains until it busts. The player
   can sit back in at a hand boundary.
5. **No timeout of any kind ends the tournament or the cash game.** At worst a
   single hand aborts, and only in the D-005 case where shares stop arriving.
   The next hand begins immediately afterwards, automatically, per
   `SPEC_CS.md` section 4.

### Agreeing that a deadline passed, without a trusted clock

This is the part that needs care, because there is no server to say "time is
up" and clocks disagree. One peer asserting a timeout cannot be enough — it
would let anyone steal the action from a player who was about to act.

A timeout takes effect through a **timeout certificate**: every other player
still active in the hand signs an assertion that the deadline for a given seat,
at a given sequence and chaining from a given `previous_event_hash`, expired.
The certificate is what the state machine consumes; the auto check/fold is its
effect. Because it names the parent event, it cannot be replayed into a
different position in the hand.

Requiring the other active players to be unanimous also settles the race
cleanly. If the slow player's action arrives at any one of them first, that
peer will not sign, so no certificate forms and the action stands. If none of
them accepted an action, the certificate forms and the action is too late.
There is no window in which both a valid action and a valid certificate exist
for the same parent.

`PROTOCOL.md` must specify the certificate's exact fields and validation, and
`STATE_MACHINE.md` must carry the deadline as explicit state rather than as a
wall-clock read inside the engine — the engine stays free of clocks, and time
enters only as a signed event.

---

---

## D-007 — Heads-up action deadlines are advisory; D-006's certificate does not work at two seats

**Date:** 2026-08-28
**Status:** accepted, and it records a limitation rather than a solution
**Corrects:** D-006
**Source:** Phase 0 adversarial review, finding A-1

### What D-006 got wrong

D-006 said a timeout takes effect through a certificate signed by "every other
still-active player", and claimed unanimity both prevents one peer from
stealing the action and settles the race against a late action.

At a two-seat table the set "every other dealt-in seat" has **exactly one
member: the opponent.** Unanimity and "one peer asserting time is up" are the
same sentence heads-up. And `SPEC_CS.md` section 32 mandates heads-up as the
*first* implemented mode, so the flaw sits precisely where the project starts.

The concrete attack: Alice faces a bet and is deciding. Mallory, running a
modified client, signs a timeout vote against Alice, assembles a complete
certificate from her own single signature, and folds Alice's hand. Alice's real
action arrives but is not equivocation - a different signer produced it - so no
evidence exists against Mallory. Alice disputes; the dispute path resolves
through the same certificate machinery, which requires Mallory's signature.
She does not sign, and the dispute cannot resolve. The hand deadline has the
same single-signer requirement and never fires either.

So heads-up, a modified opponent can fold any hand at will and make every
dispute unresolvable. That is not the bounded "auto check/fold" D-006 described.

### The underlying result, stated plainly

Two peers, with no trusted clock and no third party, cannot agree that a
deadline passed. Any rule strong enough to punish a stalling opponent is also
strong enough for a malicious opponent to invoke against an honest one. There
is no arrangement of signatures that gives both:

- protection against an opponent who folds your hand by declaring a false
  timeout, and
- protection against an opponent who stalls to void a hand they are losing.

This is not an implementation gap to be closed later. It is a property of the
setting, and `SPEC_CS.md` sections 18 and 36 require saying so rather than
inventing a construction that appears to solve it.

### The decision

1. **At two seats, an action deadline is advisory.** The UI counts it down; no
   signed state transition follows from it. Fold-effect timeout certificates
   are forbidden at `n = 2`.
2. The remedy against a stalling heads-up opponent is to leave the table. The
   transcript shows whose action was missing, which is socially attributable
   and feeds reputation, but it is not cryptographically enforceable against a
   determined opponent, and no document may claim otherwise.
3. At `n >= 3` the certificate stands, but `THREAT_MODEL.md` must describe the
   requirement honestly as **all other dealt-in seats**, never as "a malicious
   majority" - the latter reads as though several colluders were needed when
   the real bar is one at `n = 2` and two at `n = 3`.
4. The dispute path may **not** be defined in terms of the certificate
   machinery when the accused peer is a required signer of that certificate.
   That is circular at every table size, not only heads-up. Replacing it is an
   OPEN QUESTION, listed below, and it blocks nothing in Phase 2 because the
   engine takes the certificate as an input event either way.

### Consequence for the plan

Play money makes this tolerable: the worst case is a wasted hand against
somebody running a modified client, and they are visible in the transcript.
For real money it would not be tolerable, which is one more reason the spec's
play-money-first sequencing is right.

---

---

## D-008 — The timeout certificate is scoped on the voter set, not on the seat count

**Date:** 2026-08-28
**Status:** accepted
**Corrects:** D-006, and generalises D-007
**Source:** Phase 1 verification, finding N3 — a defect no review finding named

### The hole

D-007 said the certificate is unsafe at two seats and scoped the protections on
`n == 2`. **That scoping is the bug.** The attack does not need a two-seat
table; it needs a two-member *voter set*, and a modified client can manufacture
one at any table size.

The required voter set `V` was defined as the dealt-in seats, minus the
subject, minus any seat excluded for being "named as the subject of an
outstanding, older unmet deadline". Nothing bounded how many seats could be
excluded, and a seat is *named* by any peer's unilateral vote — which the
protocol itself concedes is unprovable when the voter lies.

So at a six-seat table one modified client votes against four seats, then
declares the fifth. `V` is now `{attacker}`. A certificate completes on that
single signature, and under D-005 the victim's committed chips are forfeited to
the remaining players — meaning to the attacker. Every protection was scoped on
`n`, and `n` is still six, so nothing rejects it.

This is worse than A-1, which it descends from: A-1 cost a folded hand at two
seats, this takes chips at any size.

### The decision

1. **Every rule that weakens, disables or gates the certificate is scoped on
   `|V|`, never on `n`.** Wherever a document says "at `n = 2`", it must say
   "when `|V| < 2`". This single change is what closes the attack, because it
   makes the protection follow the quantity the attacker can actually
   manipulate.
2. **A certificate with `|V| < 2` has no effect.** It is not an error and not
   evidence; the deadline simply stays advisory, exactly as D-007 has it
   heads-up. Liveness is not owed here — `SPEC_CS.md` section 19 already ranks
   security above finishing a hand conveniently.
3. **A seat may be excluded from `V` only once a *completed, valid* certificate
   names it.** Being voted against is not exclusion. This makes shrinking `V`
   inductive: each exclusion needs a certificate, each certificate needed
   `|V| >= 2` at the time it formed, so `V` cannot be collapsed by assertion.
4. The same scoping applies to the hand deadline, to `kind = 2` certificates
   and to anything else the certificate machinery gates. A rule scoped on `n`
   anywhere in the corpus is to be treated as a defect.

### Why this is the right shape

The attacker's lever was the gap between the *nominal* quorum, which looks
large, and the *effective* quorum, which they can shrink. Scoping on `|V|`
removes the gap by construction: whatever `V` ends up being, the floor applies
to it. Requiring completed certificates for exclusion then stops `V` from being
shrunk without paying the floor each time.

Neither rule needs new cryptography, and both are checkable by a verifier
replaying the transcript.

---

---

## D-009 — Three systemic rules, from defects that kept coming back

**Date:** 2026-08-28
**Status:** accepted
**Source:** Phase 1 verification pass 2, findings M1, M2 and M3
**Reinforces:** D-008

Three separate review passes each found a *worse* defect than the pass before.
Two of the three were the same defect wearing a different message name. These
rules exist so the class stops recurring, rather than being patched a fourth
time.

### Rule 1 — mandatory honest behaviour may never satisfy the equivocation predicate

The same defect has now appeared three times: **A-4** (lobby and join traffic),
**N4** (`DISPUTE`), and **M2** (`TIMEOUT_VOTE`). In each case the anti-replay
slot key was too coarse, so an honest peer doing what the protocol *requires*
signed two different bodies into one slot — which is exactly the predicate that
proves equivocation. The consequence is not cosmetic: M2 ends with an honest
voter's chips forfeited under D-005 and their key on a block list, for
following the rules.

M2's shape is worth stating, because it is the sharpest. `TIMEOUT_VOTE` carries
`sequence = subject_sequence` and `event_class = 1`, with the subject in the
body, while the anti-replay index is `(stage, seat, event_class)` — capacity
one. The protocol then specifies two simultaneous subjects as *normal*. An
honest voter voting against both signs two bodies in one slot, and manufactures
evidence against itself.

**The rule.** The slot key of the equivocation predicate must include every
field that legitimately varies for a given signer at a given stage. For
`TIMEOUT_VOTE` that means the subject seat is part of the key, not part of the
body only. More generally:

> No sequence of actions that the protocol requires of an honest peer may
> produce a valid `EquivocationProof` against that peer.

This is a property every message type must be checked against **before** it is
added, and the check belongs in the adversarial suite as a standing test, not
in a document. `SPEC_CS.md` section 25 already requires a `CheaterEquivocation`
peer; the mirror test — that an *honest* peer never generates a proof against
itself under any legal interleaving — is the one that catches this class.

### Rule 2 — a below-floor certificate is inert everywhere, with no exceptions

D-008 point 2 said a certificate with `|V| < 2` has no effect. **M1** shows the
corpus did not carry that through: `PROTOCOL.md` has such a certificate as
"inert, not chained, the hand does not end here", while `STATE_MACHINE.md`
accepts it and produces an `AbortRecord`. `THREAT_MODEL.md` holds both sides.

This is consensus-critical — it is the engine's accept predicate for an event a
modified client can emit at will, and the two documents give opposite answers.
It also quietly reopens the escape D-005 closed: a losing heads-up player gets a
one-message, on-demand hand void with stacks restored.

**The rule.** `PROTOCOL.md`'s reading wins. A certificate below the floor is
inert in every document and at every table size: not chained, not evidence, no
terminating effect, no `AbortRecord`, no forfeiture. It is silently ignored.
Any transition, guard or invariant that gives one an effect is a defect,
including for `kind = Crypto` and for the hand deadline.

Liveness is not owed here. `SPEC_CS.md` section 19 already ranks security above
finishing a hand conveniently, and D-007 already accepts that a heads-up
deadline cannot be enforced at all.

### Rule 3 — state the discipline we can enforce, never an absence we cannot

**M3**: the claim "`SmallRng` is not compiled in" is false. `rand 0.9.5` lists
`small_rng` among its **default** features, and four dependencies take `rand`
with defaults — `hickory-proto`, `hickory-resolver`, `igd-next` and `yamux`,
reached through libp2p's `dns`, `upnp` and `yamux`. Verified by reading
`rand-0.9.5/Cargo.toml` and `cargo tree --edges normal -i rand@0.9.5`.

This is the third time a claimed *absence* in the dependency tree has turned
out false: B-1, B-3, and now M3. The pattern is that an absence verified in an
isolated probe does not survive integration, and nobody re-checks it.

**The rule.** No security property of this project may be stated as the absence
of something from the dependency tree. State the discipline our own code
follows, and enforce it mechanically.

Done, for this case: `src/security/rng.rs` documents that `SmallRng` **is** in
the binary and cannot be removed, and
`tests::our_own_code_uses_no_generator_but_the_os_one` scans this crate's own
sources on every test run and fails the build on `SmallRng`, `StdRng`,
`thread_rng`, `from_seed`, `seed_from_u64` or `rand::rngs`. The gate was
verified to bite by injecting a violation:

```
SPEC_CS.md section 7 forbids these generators for cryptographic use;
draw from security::rng::fill instead:
  poker\actions.rs:8: SmallRng
test result: FAILED. 0 passed; 1 failed
```

and to pass again once the violation was removed. A gate that has never been
seen to fail is not a gate.

---

---

## D-010 — The MVP has no automated forfeiture and no automated eviction

**Date:** 2026-08-28
**Status:** accepted
**Revises:** D-005's chip rule on abort — which was my own reasoning, not a
requirement the owner stated. D-005's actual requirement, that the game
continues and the absent seat is blinded off, is untouched.
**Source:** the pattern across four review passes

### The observation that forces this

Four adversarial passes have run. Each found a defect worse than the one
before: A-1, then N3, then M2, then P1. D-009 rule 1 — that mandatory honest
behaviour must not manufacture equivocation evidence — has now been violated a
**fourth** time, by a different message each time: lobby traffic, `DISPUTE`,
`TIMEOUT_VOTE`, and now `STATE_HASH` re-emission on the *happy path* of
divergence recovery.

The severe findings all end the same way. Counting the word across the four
verification reports: 6, 5, 14, 12 occurrences of *forfeit*. Every one of the
worst defects terminates in **the honest peer's chips being taken, and its key
blocked**, for doing what the protocol requires.

That is not forty defects. It is one defect with many mouths. Automated
forfeiture is the prize that makes all of these attacks worth mounting, and
automated eviction is the second prize. Patching the fourth mouth will not stop
a fifth, because the machinery that has to be right — timeout certificates,
equivocation proofs, dispute resolution, attribution, and forfeiture computed
from all of them — is a consensus protocol, and we keep discovering that we
have not specified one correctly.

### The decision

For the MVP, an abort is **neutral**:

1. **Stacks are restored to their start-of-hand values.** No chips move on an
   abort, in any direction, for any cause.
2. **Attribution is recorded as evidence, and has no automatic consequence.**
   The transcript still says which peer failed to publish, and that record is
   still signed and verifiable. Nothing acts on it automatically.
3. **No automated eviction.** A peer is not block-listed, unseated or penalised
   by the protocol on the strength of a proof.
4. Equivocation proofs and timeout certificates remain *specified and produced*
   — they are how a human or a later version adjudicates — but consuming one
   never moves a chip or removes a player in this version.

The whole attack class evaporates, because there is nothing to win. The worst
an adversary achieves is a wasted hand, which is the same outcome as a flaky
network connection, and which the protocol must survive anyway.

### The cost, stated plainly

**The rage-quit escape returns.** A player who is losing a big pot can stall or
disconnect, the hand aborts, and their chips come back. D-005 closed that with
forfeiture; this reopens it.

That is a real regression and it is accepted knowingly, because the alternative
has been measured and is worse: forfeiture has produced four rounds of attacks
that take chips from *honest* players, and an exploit that harms an honest
player is worse than one that merely lets a dishonest player escape a loss.

Mitigations that do not require the fragile machinery:

- Repeated aborts attributable to one identity are plainly visible in the
  transcript, to everyone, forever. In play money that is the whole penalty,
  and `SPEC_CS.md` section 18 forbids claiming more.
- The client should show a per-identity abort count in the lobby, so players can
  decline to sit with someone who does it.
- Sitting a repeat aborter out is a *user* decision, not a protocol action.

This is exactly the trade `SPEC_CS.md` section 18 asks to be made explicit
rather than hidden: some cheating is prevented, some is detected, and some —
here, escaping a losing hand — is merely visible.

### Why this is the right time

`SPEC_CS.md` section 32 requires heads-up play money first and expansion only
once the protocol works. D-007 already established that a heads-up deadline
cannot be enforced at all, so for the very first supported mode the forfeiture
machinery was never going to work anyway. Building it before the simple case
runs is the wrong order.

### What this deletes, and what it unblocks

Deleted from the MVP: the forfeiture arithmetic, automatic attribution
consequences, and the eviction path. Whole branches of the dispute machinery
lose their purpose and go with them.

Of the four blockers the last verification named, this dissolves rather than
fixes most: **P1**, **P2** and **P4** cost nothing once a proof moves no chips,
and **P3**'s terminus no longer needs to be peer-deterministic about *who*
failed, only *that* the hand ended. **P5** — the frozen `Diverged` phase — still
needs a liveness exit, and now has an obvious one: abort neutrally.

### When this is revisited

Before real money, and not before the certificate, equivocation and dispute
machinery has gone a full adversarial pass with no new defect of this class.
That will be a new numbered decision with its own evidence; this one is not
quietly upgraded.

---

---

## D-011 — One normative owner per concept, and the slot key written down once

**Date:** 2026-08-28
**Status:** accepted
**Source:** Phase 2 gate, blockers P3, G1, G2, G3
**Reinforces:** D-009, D-010

### What the fifth pass showed

D-010 worked on the axis it was aimed at. No defect in that pass moves an
honest peer's chips, and the corpus got smaller. But four blockers remain, and
the gate's own summary is the useful part: three of them are **one
disagreement seen from three angles**. `PROTOCOL.md` and `STATE_MACHINE.md` do
not agree on the terminal stage's shape, on its sequence number, or on what may
trigger it — and all three determine where one hand ends and the next begins.

That is not three defects either. It is the consequence of two documents both
being normative about the same thing. Every pass they are edited by different
agents, and every pass they drift apart again. Five passes is enough evidence.

The fourth blocker is mine: `NETWORK_STACK.md` was left out of the D-010 pass
on the judgement that it was unaffected. It was not — D-010 point 3 forbids
automated eviction, and eviction is a transport action. That document has zero
mentions of D-010 and ten places that block a peer on a proof, which is exactly
what turns a liveness bug into an attack on an honest peer.

### Rule 1 — one normative owner per concept

- **`PROTOCOL.md` owns the wire**: message shapes, the event envelope, the
  chain, sequence numbers, the anti-replay slot, canonical bytes, and what a
  receiver validates.
- **`STATE_MACHINE.md` owns state and transitions**: phases, guards, legal
  actions, pots and invariants.
- **`CRYPTOGRAPHY.md` owns the constructions.**
- **`NETWORK_STACK.md` owns transport, discovery and connectivity.**
- **`THREAT_MODEL.md` owns classifications**, and restates nothing.

A document may **reference** another's definition and must not restate it. Where
a restatement exists today, it is deleted and replaced with a pointer naming the
section. Where two documents currently disagree, the owner wins — which for
P3, G1 and G2 means `PROTOCOL.md` decides the terminal stage, and
`STATE_MACHINE.md` follows it.

This is the rule that stops the drift, because a copy is what drifts.

### Rule 2 — the anti-replay slot key is one concrete tuple, written once

D-009 rule 1 said the slot key must include every field that legitimately
varies. It was right and it has now been violated a **fifth** time: **G1**, the
terminal `HAND_ABORT`, because `event_type` is not in the key, so an honest peer
that already contributed to a stalled stage signs a second body into an occupied
slot and incriminates itself. The rule was prose, and prose was re-derived
differently by each editor.

So the key stops being a principle and becomes a definition, in `PROTOCOL.md`,
in one place, as a literal tuple, including `event_type` and every other field
that legitimately varies. Every other document points at it by section number
and never reproduces it. Any message whose honest emission can collide in that
key is a defect in the message, not in the key.

### Rule 3 — D-010 point 3 binds every layer, including transport

No automated eviction anywhere: no `block_peer`, no unseating, no allow/block
list driven by a protocol proof, in any document. A proof is evidence for a
human. The user may always choose not to play with someone; the protocol may
not choose for them.

### Why this is the last structural rule

Five passes have each found the same two shapes: a copy that drifted, and a
consequence that made an attack worth mounting. D-010 removed the consequences.
D-011 removes the copies. If a sixth pass finds a defect of either shape, the
right response is not a sixth rule but to cut the mechanism out of the MVP
entirely, as D-010 did.

The poker layer is not implicated in any of this and has been ready for two
passes: the phases, the transitions, the determinism contract, the betting
rules, the pot arithmetic. Implementation continues there while this closes.

---

---

## D-012 — Canonical state is never derived from a per-receiver quantity

**Date:** 2026-08-28
**Status:** accepted
**Source:** Phase 2 gate, second attempt — blockers H1 and H2
**Reinforces:** D-011

### What the sixth pass showed

D-011 worked. For the first time in six passes **no two documents disagree**,
every one of the previous blockers is closed by an edit that names the finding
and says what it gives up, and — the structural result — **all twenty phases
have a reachable exit under total silence**. The slot key lives at one site,
all thirty-nine message types are clean against it, eviction has zero survivors
at any layer, and chips are conserved on every path in both modes.

Neither of D-011's two named shapes appears: no copy drifted, and no
consequence makes an attack worth mounting. So the "cut the mechanism" clause
is not triggered. What appeared is a third shape, and it is worth naming
because it is the one a fix creates rather than leaves behind:

> **The defect is on the path the previous fix newly made load-bearing.**

P3's witness-independent terminal stage — which is what unfroze eleven phases —
made "which copy of the abort did this peer accept" a **per-receiver** fact for
the first time. A line elsewhere had been deriving canonical state from a field
of that copy, and that line was correct when every completing peer held the
same body. It is not correct now.

### The rule

**No canonical state — nothing that enters a state hash, a roster hash, a
chained event body, or the next hand's genesis — may be derived from a quantity
that can differ between honest receivers.**

Canonical state changes only through a chained event that every participant has
accepted. Everything else is a local view, and belongs in the structure the
state machine already separates for that purpose.

Two consequences, which are the two blockers:

1. **H1.** `STATE_MACHINE.md` T46 sets a seat's `status := Absent` from the
   accepted `HAND_ABORT` copy's `attributed` field. Two honest peers can accept
   different copies — the certificate path carries `[C]`, the hand-deadline path
   carries `[]`, and no abort-versus-abort precedence rule exists. Stacks and
   `TERMINAL(k)` still agree, but the next hand's `dealt_in` and `bb_seat`
   differ, so its collective stage never completes and the table never plays
   again. `PROTOCOL.md` §4.10 already forbids exactly this: "no receiver may
   derive a seat's state from it."

   **T46 loses that line.** A seat's status changes only through a chained
   event. An abort records who failed as evidence, and under D-010 evidence has
   no automatic consequence — so deriving a status from it was never going to be
   right.

2. **H2.** `seat_flags` appears **once in the whole corpus**, inside
   `roster_hash(k)`, and is never defined. It feeds `GENESIS(k)` and therefore
   every event, and no default is pickable: two implementers guessing
   differently share no verifying event at all.

   **Resolve it in the safe direction: `seat_flags` is removed from
   `roster_hash(k)`.** The roster is the ordered list of seated player
   identities and nothing else. Anything mutable — sitting out, absent, away —
   is either established by a chained event, in which case it is already in the
   chain and need not be hashed again, or it is a local view, in which case it
   must not be hashed at all. Removing the field closes half of H1 as a side
   effect, because it is the other route by which a per-receiver status could
   reach the genesis.

### The process rule this also produces

Twice now a sweep has missed a document, and both times it became a blocker:
`NETWORK_STACK.md` had zero mentions of D-010 while holding ten places that
blocked a peer, and now `CRYPTOGRAPHY.md` has zero mentions of D-011 while its
ownership table still declares the pre-D-011 discipline. Both were left out on
my judgement that the decision did not touch them. Both times that judgement
was wrong.

**Every decision sweep covers every document, including `CONTRIBUTING.md` and
`DEPENDENCIES.md`.** The cost of including a document that turns out to be
unaffected is one agent reading it and reporting nothing; the cost of excluding
one has now twice been a blocking defect. A cheap check that occasionally finds
nothing beats a judgement call that has failed twice.

### Where this leaves the project

The gap is two fields wide, not four mechanisms wide. The poker layer — twenty
phases, fifty-six live transitions, twenty-nine invariants, the determinism
contract, the betting rules and the pot arithmetic — has been ready for three
passes and is already implemented and under test.

---

---

## D-013 — Liveness is inherited from the chain, not from a seat's status

**Date:** 2026-08-28
**Status:** accepted
**Source:** Phase 2 gate, third attempt — blockers J1 and J2
**Reinforces:** D-012

### First, a correction

D-012 said the cost of deleting T46's status derivation was that "a silent seat
is dealt in every hand and stalls each one until the hand deadline". **That
understated it, and the gate found out why.**

`HAND_INIT`'s required emitter set is "every seat that will be `dealt_in`, plus
every occupied seat that is absent or sitting out". So marking a seat `Absent`
never removed it from the set in the first place, and the machine's actual fixed
point is:

> silent seat stays `Active` → dealt in → required emitter → stage stalls →
> hand deadline at 600 000 ms → every stack restored → next hand → **identical
> state**.

Ten minutes per iteration, unbounded, and no seat can ever bust because every
stack is restored, so no end condition can fire either. That is not a slow
table. **The table makes no progress, ever.** D-012's cost model was wrong and
is corrected here rather than left standing.

### J2 — the rule

The circularity is the whole problem: a seat's status may change only through a
chained event (D-012 / I30), and a silent seat emits nothing, so nothing can
ever change its status, so it is required forever.

The way out is to stop asking about status at all:

> **A seat is required to emit in hand `k+1` only if it signed at least one
> chained event during hand `k`.** For the first hand, the required set is the
> signers of `TABLE_READY`.

Liveness is inherited from demonstrated participation in the agreed chain, not
from a status field. This is not a per-receiver quantity: "did seat `s` sign
anything in hand `k`" is a function of the accepted chain, and after H1's fix
the terminal stage is witness-independent, so honest peers agree on it.

Consequences, all of them intended:

- A seat that goes silent stalls exactly **one** hand — the one it went silent
  in — and is then skipped. Every later hand proceeds among the seats that are
  actually there.
- It keeps its seat and its stack, and the blinds eat it, which is exactly
  D-005 and exactly what the owner asked for. It now genuinely busts, so the
  tournament can end.
- It rejoins by signing a chained event, which requires it to be alive. Nothing
  else can put it back, and nothing else needs to.
- No certificate, no vote, no proof, no attribution. This is deliberately not
  the machinery of D-006 to D-008, which D-010 made inert and `OQ-F` still
  questions.

### J1 — the rule

`advert_hash` enters `GENESIS(0)` and `session_id`, and it is a per-receiver
quantity: rule 6 requires each re-broadcast to carry a strictly greater
timestamp, so two honest players who join thirty seconds apart hash different
advertisements, derive different `GENESIS(0)`, and no `TABLE_READY` verifies.
`TABLE_READY` carries the field and nothing ever checks it.

> **`advert_hash` is removed from `GENESIS(0)`, `session_id` and `ctx`, and
> replaced by a hash of the agreed table parameters** — preset id, seats,
> starting stack, blind schedule, ante, timing values — and nothing else.

Every joiner sees the same parameters whichever re-broadcast reached them,
because the parameters are what they agreed to; the timestamp is not. This also
closes J1(b), where nothing forbade the founder re-signing with *different*
parameters and forking the blinds, because a changed parameter now changes the
hash and every peer sees it.

### The process finding, which is D-011's own shadow

`NETWORK_STACK.md` found J1, wrote it up in full, named both fixes, and marked
it a blocker for `PROTOCOL.md`. `PROTOCOL.md` never acted. The gate's phrasing
is worth keeping:

> **A finding correctly assigned across an owner boundary is a finding nobody
> owns.**

D-011 gave every concept one owner so that copies would stop drifting. It also
created a way for work to fall between owners, because the document that *finds*
a defect is now often not the document that may *fix* it.

**So: a defect a document assigns to another document's owner is recorded in
this file's open list in the same pass that finds it.** Not only in the finding
document, where it is invisible to the editor who has to act on it. The open
list is the one place every editor of every document is required to read.

---

---

## D-014 — A cheater is removed from the table on self-authenticating evidence

**Date:** 2026-08-28
**Decided by:** project owner
**Status:** accepted
**Narrows:** D-010 point 3, which was over-broad

### The requirement

A player who cheats, or tries to alter the game illegally, is removed from the
table. The hand they attacked is voided and play continues without them. An
information window tells the other players that the anti-cheat mechanism
removed that player.

### Why D-010 banned this, and why it was too broad

D-010 removed automated eviction after four review passes in which **every**
severe defect ended the same way: an honest peer's chips forfeited and its key
blocked, for following the protocol. Counting *forfeit* across the verification
reports: 6, 5, 14, 12.

But look at what those defects actually had in common. Every one of them turned
on a judgement that **two honest peers can reach differently**:

- "peer X did not publish before the deadline" — needs a clock nobody shares;
- "peer X equivocated" — needed a slot key that was wrong five times running;
- "peer X caused this abort" — the `attributed` field, which H1 showed is
  per-receiver;
- "the voter set is complete" — which N3 showed one client can shrink at will.

Not one of them was "peer X sent a message that is provably illegal". That class
was never the problem, and banning it along with the rest was an
over-generalisation on my part. The owner's requirement is the narrow class, and
the narrow class is sound.

### The distinction that makes it safe

> **Evidence is self-authenticating when it is a message signed by the accused,
> whose illegality any peer can decide alone, from that message plus state the
> peers provably share.**

An honest peer cannot be framed by such evidence, because framing it would mean
forging the victim's signature over an illegal message. There is no vote, no
quorum, no timing, no per-receiver judgement — the three things that produced
every previous defect are all absent. Every honest peer reaches the same verdict
from data it already holds, and a third party replaying the transcript reaches
it too.

That is a genuinely different object from an accusation, and it is why this can
be automated when accusation cannot.

### Two tiers, because state-dependence is where this could still go wrong

**Tier 1 — self-contained.** Illegality is decidable from the offending message
alone, with no reference to game state:

- a signature that does not verify under the sender's key;
- a non-canonical encoding (`PROTOCOL.md` §2.5's gate);
- a malformed message, an out-of-range field, a card index outside `0..=51`;
- a failed shuffle proof, a failed decryption-share proof, a failed
  key-ownership proof;
- a deck that gains, loses or duplicates a card across a shuffle;
- a message whose signer is not a party to this table.

These are safe to act on the moment they arrive. Nothing about them can differ
between honest receivers.

**One clause is deleted from that list and may not return: *a message whose chain
parent does not exist*.** It stood in the sixth bullet above, beside the
not-a-party clause, and it does not belong in tier 1 at all: **whether a parent
exists is decidable only against the receiver's own store.** An honest peer that
has not yet received the parent — one dropped frame, one reordering, one slow
relay — computes *no such parent* where every other peer computes *present*, and
it computes it about a message the accused signed legally. One dropped frame
would remove an honest player: the exact failure the two tiers exist to prevent,
sitting in the tier meant to act on arrival with no checkpoint to wait for. The
code deleted it first — `src/security/validation.rs` has no
`SelfContained::ParentUnknown`, and its
`no_tier_one_violation_depends_on_the_receivers_store` test is the tripwire — and
`PROTOCOL.md` (§4.0's box, §4.9's acceptance gate) and `THREAT_MODEL.md` (§5.1,
§5.2 row 13) are corrected in the same pass. A missing parent remains a reason to
**buffer or reject** the event, which is all it ever was; it is not evidence
against anybody. `THREAT_MODEL.md` **§5.1.1** states the general admissibility
test a future tier-1 addition must pass, and is its normative owner
(D-011 rule 1): a clause is tier 1 only if its verdict reads nothing the receiver
stores, cannot change with what the network did, and needs no second message.
The list above still has **six bullets** — the count `CRYPTOGRAPHY.md` §8.1 cites,
of which three are that document's — and they expand to exactly the **nine**
variants of `SelfContained` in the code, each answered by the cryptography or by
the encoder and none by a store.

**Tier 2 — state-dependent.** Illegality is decidable only against game state:
an out-of-turn action, a raise below the minimum, a bet larger than the stack, a
showdown claim that contradicts the board.

These are **only** safe once the state they are judged against is agreed. If two
peers have diverged — and K1 in the open list is exactly such a divergence — an
honest peer's perfectly legal action looks illegal to a peer whose state has
drifted, and the anti-cheat would evict the honest player. That is the same
failure mode D-010 closed, arriving by a new road.

> **Rule: a tier-2 eviction requires the accused's message to be judged against
> state fixed by a checkpoint both peers have signed.** Before that point a
> tier-2 violation is recorded and the hand is voided, but nobody is removed.

Tier 1 needs no checkpoint. Tier 2 waits for one.

### What removal does

1. **The attacked hand is voided.** Stacks return to their start-of-hand values,
   exactly as D-010's neutral abort already specifies. Nothing new is needed.
2. **The offender leaves the protocol immediately.** It is no longer a key
   holder, is not dealt in, is not in any required emitter set, and cannot act.
   Under D-013 it would have dropped out of the emitter set anyway once it
   stopped signing; this is the same exit, taken at once and for cause.
3. **Its seat becomes dead and its stack is blinded off**, precisely as D-005
   treats an absent seat, until the stack is gone. This keeps chip conservation
   exact — removing a stack from a tournament would change the chip total and
   break invariant I1 — and it lets the tournament reach its end condition
   normally.
4. **It cannot rejoin the table.** Unlike an absent seat, this exit is one-way.
5. **The evidence is kept in the transcript**, so the removal is replayable and
   checkable by anyone afterwards, including by the accused.

### The one exception: the setup chain removes nobody (`G6-R4`)

**Points 1 to 5 apply to a hand. In the setup chain — `hand_id == 0`, before the
first hand exists — a tier-1 finding removes nobody, and the table simply does not
start.** The evidence is still recorded and the finding is still a finding; what
does not happen is the unseating.

This was decided by `STATE_MACHINE.md` T66 and by `NETWORK_STACK.md`, correctly,
and it is written here because a reader following the authority order found a
decision saying *removes its sender from the table* and a transition saying
*nothing else — no seat is removed*, with no recorded reconciliation. It is
numbered now after four consecutive passes named it and none numbered it.

**Why the exception is forced rather than chosen.** `roster_hash(k)`'s seat vector
is frozen at `TABLE_READY` (`PROTOCOL.md` §3.1), and the seat vector is an input to
`GENESIS(0)` and to every chained hash after it. Removing a seat in the setup chain
therefore does not punish the offender — it gives every peer that saw the evidence
a different genesis from every peer that did not, which is a **fork of the table's
identity** produced by an anti-cheat rule. That is a strictly worse outcome than
the one the exception accepts.

**And the exception costs nothing, which is why it is safe to take.** Three things
are all true at `hand_id == 0` and none survives into hand 1:

* **No chips exist.** `ledger_in == 0`, no buy-in has been accepted, no stack has
  been dealt. There is nothing for the offender to win and nothing for an honest
  seat to lose, so D-010's forfeiture concern has no object here.
* **The table does not start anyway.** A tier-1 finding in the setup chain is a
  seat that produced a bad proof, a bad signature or a bad encoding in the beacon;
  its stage never completes, the beacon stalls, and the formation times out on the
  local lobby timer (T4) exactly as it did before D-014 existed. The offender is
  excluded by the table failing to form — the same outcome removal would produce,
  reached without touching a hash.
* **There is nothing to void.** Point 1 voids the attacked hand; at `hand_id == 0`
  there is no hand, so points 1 through 4 have no referent and only point 5, the
  evidence, does.

It follows that the information window of *What the players see* — the required
addition to `SPEC_CS.md` §22 (`N8`) — has no setup-chain case: it reports a
removal and a voided hand, and here there is neither. A table that fails to form
is reported by whatever `SPEC_CS.md` §22 already says about formation, and the
required addition's four elements are unchanged.

**What this is not.** It is not a tier boundary and not a weakening of tier 1: the
verdict is unchanged, self-authenticating and reached identically by every peer.
Only the **consequence** is suspended, and only where the consequence would edit a
frozen roster. From hand 1 onward points 1 to 5 apply in full. Nor is it a licence
to re-form the table without the offender by fiat — a table that fails to form is
re-formed by its founder under a **new table key**, which is what forming a table
means (`PROTOCOL.md` §4.3).

### What the players see

An information window naming the removed player, the tier, and
what the evidence was — "invalid shuffle proof", "signature did not verify",
"raise below the minimum after checkpoint 4". A removal that cannot be explained
in one sentence to the other players should not be automatic.

The window must also state that the hand was voided and that no chips changed
hands, so nobody reads a void as a loss.

**That window is a required addition to `SPEC_CS.md` §22 and is not an element
§22 contains — `N8`, and the wording is corrected here because this decision is
where the wrong citation originated.** The text that stood here said *"`SPEC_CS.md`
§22's information window"*, as though pointing at something already specified.
§22's GUI tree is *network status*, *lobby* and a *poker table* ending at `timer`
and `protocol/security status`, with one further sentence forbidding the display
of a cryptographically unverified card; no anti-cheat window is among them.
`SPEC_CS.md` is the owner's document and is not edited from here, so what this
decision states is the **requirement**, and no document in the corpus may cite the
window as present until the owner adds it. The four things the addition must say
are fixed and are the paragraph above. `STATE_MACHINE.md` T64's cell and
`THREAT_MODEL.md` §9.2 already carry it in this form; the open list's `N-8` row
below says the same and no longer offers the alternative of citing §22's status
element, which would be citing a different element for a different purpose.

### What this does not become

This is **not** a route back to D-010's forfeiture or to accusation-driven
eviction. Specifically:

- no removal on a timeout, a missing publication, or any liveness judgement;
- no removal on an equivocation proof, whose predicate has failed five times;
- no removal on `attributed`, on a vote, on a certificate, or on any quorum;
- **no removal on any check that reads this receiver's own store**, however cheap
  and however local it looks — the parent check was exactly that, and *local* is
  not the same property as *receiver-independent* (`THREAT_MODEL.md` §5.1.1);
- the offender's chips are never awarded to anyone — they are blinded off.

If a proposed removal cannot point at a message the accused signed, it is not a
D-014 removal.

### The check this needs before it ships

The recurring lesson of eight passes is that the worst defect appears on the
path a fix newly made load-bearing. This decision makes **the correctness of
every validator** load-bearing in a way it was not before: a validator that is
too strict now ejects honest players rather than merely rejecting a message.

So the adversarial suite must carry the mirror of `SPEC_CS.md` §25's cheater
list: for every legal action an honest client can emit, under every legal
interleaving, **no honest peer is ever evictable**. That test is the gate on
this feature, and it is the same shape as D-009 rule 1's mirror test.

---

---

## D-015 — The timeout machinery is not produced in the MVP

**Date:** 2026-08-29
**Decided by:** project owner
**Status:** accepted
**Answers:** OQ-F
**Completes:** D-006, D-007, D-008, D-010

### The decision

`TIMEOUT_VOTE`, `TIMEOUT_CERT` and `EquivocationProof` are **not produced** in
version 1. They stay specified, so a later version can add them without a wire
break, and nothing emits or consumes one.

### What prompted it

The anti-replay store was measured. Worst case per hand:

| class | entries | resident |
|---|---:|---:|
| ordinary events | 41 130 | 2.2 MiB |
| timeout votes | 184 320 | 9.8 MiB |
| timeout certificates | 184 320 | 16.3 MiB |
| **total** | | **28.3 MiB** |

At the protocol's limit of eight concurrent table sessions that is ~226 MiB.
**Twenty-six of those twenty-eight MiB are the two timeout classes**, and D-010
had already made them consequence-free: consuming a certificate or a proof
moves no chip and removes no player. So the client was carrying its largest
attacker-influenced allocation for machinery that does nothing.

### Why this is possible at all, which is the part worth checking

It looks as though removing the certificate must leave an away-from-keyboard
player able to stall a table, since the certificate was how peers *agreed* a
deadline had passed. It does not, and the reason is D-006's own observation
taken one step further.

Publishing a decryption share is something the **client** does automatically; it
never waits for the human. So is emitting an action. A player who has walked
away from the keyboard is still running a cooperating client, and that client
can emit **its own** auto check/fold when its own timer expires. That is a
single-writer event by the seat itself — it needs no vote, no quorum and no
agreement, because the seat is present and speaking. Only the human is absent.

That leaves exactly one other case: a client that emits nothing at all, whether
it crashed, lost its network or is deliberately silent. D-013 already answers
it — the seat stalls one hand, is then outside `P(k+1)`, and every later hand
proceeds without it while its stack is blinded off.

So the two cases are covered by two mechanisms that already exist, and the
certificate sat between them covering neither:

| | client emits | who resolves it |
|---|---|---|
| Human away, client running | yes | the seat's own auto check/fold |
| Client gone or silent | no | D-013 drops it from `P(k+1)` |

A client that is running but deliberately refuses to act while still publishing
shares behaves as the second row: it stalls one hand and is then skipped. That
costs a hand and is visible in the transcript, which under D-010 was already all
that would have happened.

### What it removes

- 26 MiB of the 28 MiB worst-case anti-replay footprint, and with it the
  client's largest attacker-influenced allocation.
- Two of the three anti-replay classes, and both subject axes.
- The machinery that produced a defect in **five consecutive review passes** —
  A-1, N3, M2, and D-009 rule 1's recurrences — every one of which turned on
  who was required to sign what, and by when, with no shared clock.

### What it costs, stated plainly

1. **No cryptographic record of who timed out.** The transcript still shows
   whose event is missing from a stage, which is what a human or a later version
   would read anyway, but there is no signed assertion about it.
2. **A later version cannot adjudicate today's transcripts** for timeout
   questions, because the evidence was never produced. That is accepted: under
   D-010 the evidence had no consequence, so nothing was going to be adjudicated
   from it.
3. **An equivocation is detected and not provable to a third party.** The store
   still finds it — two bodies in one slot — and the finding still stops the
   hand under D-014's tier 1, which acts on the accused's own signed messages
   and needs no proof object. What is lost is handing that proof to somebody who
   was not there.

Point 3 is the one to weigh before real money, and it is recorded in the open
list rather than treated as settled.

### What stays

- The slot key keeps every component, including both subject forms. Narrowing
  it is how five passes went wrong, and a key that is right costs nothing when
  nothing populates it.
- `docs/PROTOCOL.md` keeps the message definitions and their codes, so adding
  them later is not a wire break.
- D-014's anti-cheat is untouched. It never rested on the timeout machinery: its
  evidence is a message the accused signed.

---

## D-016 — The vendored `ziffle` is forked

Taken 2026-08-29. Closes `ZR-1`, which the verdict raised and explicitly declined
to decide, because it is a project-shape decision and not a cryptographic one.

### The decision

**Fork.** `vendor/ziffle/` is no longer the crates.io 0.1.0 tarball. The four
changes of condition C-12 are applied, each marked `FORK(x)` in the source and
each recorded in `vendor/ziffle/PROVENANCE.md`.

### Why now rather than later

Every item is a **wire-format** change, and the wire format is frozen by the
first proof that is persisted or exchanged. Before that, the change costs a
morning; after it, it costs a migration with two mutually unverifiable halves of
the network in the middle. There is no third moment at which this gets cheaper.

The two that force it:

* the transcript framed its lengths with `usize::to_be_bytes()`, eight bytes on
  x86-64 and four on wasm32, so two clients of different word widths derive
  different challenges from the same statement and neither can verify the
  other's proofs — a certain break the day anything runs in a browser;
* `RevealTokenProof` bound a card's first ciphertext coordinate and dropped the
  second, so a token issued for one card opened every card sharing that
  coordinate, and a malicious shuffler can arrange the collision. Demonstrated
  end to end at 52 cards.

Neither is a break of Bayer–Groth. Both are the library proving exactly what it
claims where the claim is not what a poker game needs.

### What it costs, stated plainly

* We are on a **private wire format** that no upstream fix will match. An
  upstream release is now a merge, not an upgrade.
* ziffle's own 16 unit tests and 15 doctests are no longer a check on the code we
  run without re-verification. They were re-run after the fork and pass; that has
  to happen after every future change, and `PROVENANCE.md` says so.
* Interoperability with the one other project using the crate is forfeited. It
  was never a goal, and this makes it a non-goal on the record.

### What did not change, deliberately

The verdict listed, as optional, deriving both sub-challenges from one joint
transcript state. **It is not done.** Two reviewers independently found the
existing fork to be the paper's own parallel composition and sound.
Restructuring something a review found sound is risk without benefit, and a
review is only worth having if its positive findings are respected as well as
its negative ones.

### What this does not decide

The verdict's §6 bar for **real money** is untouched and unmet: nobody proved
witness-extended emulation, and nobody re-derived the soundness bound at `m = 1`.
The fork makes the library fit for the play-money MVP under conditions C-1 to
C-15. It does not move the other bar an inch, and no amount of engineering time
buys items 1, 2, 3 and 6 of that list.

## D-017 — Surviving a disconnection mid-hand: **analysed, and withdrawn**

**Status: WITHDRAWN by the owner on 2026-08-29, the same day it was raised.
Nothing here is to be built.** What ships is the fallback that already exists:
heads-up needs no mechanism at all, and multi-way the hand aborts with `T46`
restoring every stack to its start-of-hand value.

The analysis is kept rather than deleted for two reasons. It records that
sharing **key** shares with outside nodes is unsafe, so nobody arrives at it
again by the obvious route; and it records the one correction that would make a
variant of it sound, so if the question returns the work does not start from
nothing.

Raised by the owner as two proposals. The first would have been adoptable with
one correction that is the whole of why it is safe; the second is declined
outright and would stay declined.

### What was already solved, and is worth stating first

**Heads-up, a rage quit needs no cryptography at all.** The absent seat auto
check/folds (D-006), `only_one_live` awards the pot, and — in that function's own
words — *the survivor wins every pot it is eligible for and never reveals a
card*. Nothing is decrypted because nothing has to be.

The gap is narrower than it first looks: **three or more seats, one gone, two
still in at showdown.** The board needs a reveal token from every key in the
aggregate, and one of them has left. That is where `n`-of-`n` bites, and it is the
only place it does.

### The correction: share tokens, not key shares

The proposal was to give outside nodes `t`-of-`n` shares of the **deck key**. That
does not work and `SPEC_CS.md` §35 already says so — *n-of-n stands; abort
neutrally*. The reason is worth writing down rather than citing:

**A share of the key is a share of the key.** Every card sits under one aggregate
key, so there is no construction that grants the ability to open index 12 and
withholds index 3. *"They only decrypt the board"* is a policy, and the nodes hold
what is needed to ignore it. Three colluding helpers read hole cards. And §18
places Sybil-without-identity outside the threat model, so an attacker may
simply **be** all five helpers.

**Sharing the five board reveal tokens instead inverts every one of those
properties:**

| | shares of the key | shares of the board tokens |
|---|---|---|
| three colluding helpers get | the key, hence hole cards | five values that open nothing alone |
| helpers plus one seated player | everything | still nothing — the other seat's token is missing |
| Sybil across all helpers | fatal | **harmless** |

The remaining players still gate each other: `n`-of-`n` among those **present**
holds unchanged. Even helpers who publish their shares at hand start achieve
nothing, because the board still cannot open before its street without the other
seat's token. And the departing player's own hole cards are untouched — the
helpers hold nothing about them, by construction rather than by promise.

### Why this and not simply "publish your tokens as you leave"

The simpler form — a client publishes its five board tokens as its last act —
handles a **graceful** exit and nothing else. A power cut, a crash or a killed
process publishes nothing, and those are exactly the cases the rule exists for.
Shares distributed at hand **start** survive all of them.

### The condition, which is the owner's, and it is load-bearing rather than tidy

**Only when helpers are reachable, and only helpers who are not seated in this
hand.** Both halves are the owner's and the second is the one that carries
weight, so it is worth being precise about why — the first reading of it was that
it is good hygiene, and it is not: without it the scheme is a break.

**Seated helpers collapse the gate.** Suppose seats `A`, `B` and `C`, and `C`
shares its board tokens with `A` and `B`. `A` and `B` between them now hold their
own tokens **and** `C`'s, which is every token the board needs. They can open the
flop, the turn and the river **before a single bet is made**, while `C` is still
sitting at the table playing. That is not a degradation of the property; it is
`SPEC_CS.md` §35's main invariant — *no participant may learn the future board* —
failing outright.

Two further reasons, either sufficient on its own:

* **A seated helper shares the failure domain.** The common cause of a
  disconnection is a router, an ISP or a Wi-Fi link, and the whole point of the
  helpers is to survive the event that removed the player. Helpers inside the
  same table are hit by the same table's trouble.
* **A seated helper has an interest in the outcome.** A player-helper can decline
  to reconstruct precisely when reconstructing would hand the pot to somebody
  else. A disinterested node has no reason to withhold, which is the only sense
  in which it is more trustworthy — and it is enough.

**With no eligible helpers, the fallback is the one already built:** the hand
aborts and `T46` restores every stack to its start-of-hand value, so no chip
crosses between seats and the tournament plays on. A mechanism that cannot be
attempted is not a mechanism that fails.

### What it still needs, and it is not optional

Verifiable secret sharing — Feldman or Pedersen over the same group — so that a
malicious departing player cannot distribute garbage and reach the same abort by
a longer route. That is an established, peer-reviewed construction and is
therefore admissible under §36; a hand-rolled sharing scheme here would not be.

### The second proposal is declined: time-lock puzzles

**The puzzle protects a *board* token, and sequential-squaring difficulty is
measured in the fastest available multiplier, not in the holder's.** An opponent
with better hardware opens the lock before that street arrives, and knowing the
river during flop betting is a total break of the game. The mechanism hands the
advantage to whoever has the faster processor, which is the opposite of its
purpose.

Two further reasons, either sufficient on its own:

* The modulus is generated by the party asserting the difficulty. A cheat picks
  one with a shortcut. Provable difficulty needs a VDF with a trusted setup or
  class groups, which is research-grade work.
* It is not needed for the case that motivated it. Heads-up is already solved by
  D-006, and multi-way is solved above without any new cryptographic assumption.

**No mechanism here is claimed to make cheating impossible.** What was claimed,
had it been built, is narrower and checkable: a disconnection stops costing the
hand when eligible helpers are present, and costs exactly one aborted hand with
every chip restored when they are not.

### What actually ships, since this is withdrawn

The second half of that sentence, and only it. A disconnection costs one hand.
Every chip goes back. The tournament plays on, and no seat is removed for it —
which is D-010, and is why the cost of the withdrawal is small.

## D-018 — Twenty-three normative terms used and never defined, and what happens to each

Found 2026-08-29 by reading every normative source for the table-formation path
in parallel and checking each against the code, rather than implementing from a
single reading.

### Why this is a decision and not a bug list

`role_code` was normative, undefined, and unnoticed for seven passes, because
the construction that used it **reads complete**. So does every entry below. A
term that looks defined elsewhere is invisible to a reader and fatal to a second
implementation, and the failure it produces — two conforming clients that cannot
agree and cannot say why — has no error message and nothing to attribute.

The rule this establishes: **a normative term is defined in its owning document
before code depends on it.** Where the code has to move first, the guess is
written down as a guess.

### Closed here, with the reasoning in the owning document

| Term | Disposition |
|---|---|
| **U1** `stack_at_hand_start[s]` at `k = 0` | **The ratified roster's `SeatEntry.buyin`** (§3.1). The severe one: it was defined only as `TERMINAL(k-1)`'s final stacks, `TERMINAL(-1)` does not exist, and the value feeds `roster_hash(0)` → `session_id` → every hand's `GENESIS(k)`. Two guesses diverge on every event of every hand, silently. `SeatEntry.buyin` is not chosen by elimination: it is signed under the table key, ratified by a collective stage, and in a tournament forced to a single value by §7.2. |
| **U4** `next_deadline_ms` for formation messages | **`0` for the unchained ones, `crypto_step_timeout_ms` for `TABLE_READY`** (§8.2). The field is normative and a wrong value is an invalid event, and the stage-kind table had no row covering any of them. `0` because they arm nothing: the join RPC has its own transport timeout, the handshake is covered once by `HELLO`, and a `PLAYER_LIST` obliges nobody. `TABLE_READY` is the exception because the beacon follows it — which also gains the row it never had. |
| **U8** `SeatEntry.buyin`'s admissible range | **Within the advert's `[min_buyin, max_buyin]`, and equal to `start_stack` in a tournament** (§3.1, in the same paragraph as U1). It had to be closed with U1: an unconstrained buy-in in the roster is an unconstrained starting stack inside `roster_hash(0)`. |
| **U17** `peer_id` uniqueness in a roster | **Required** (§4.3). Every `SeatEntry` carried it and no rule read it — carried and never checked. It stops one node holding several seats, and therefore the accidental case of one person joining a table twice. **It is not one person per seat**, and that is stated where the rule is: two machines present two PeerIds and are indistinguishable from two people, which is Sybil without an identity layer and is outside `SPEC_CS.md` §18. One key per seat and one node per seat are enforceable and enforced; one person per seat is neither claimed nor achievable. |

### Open, and blocking the named work

| Term | Blocks | Severity |
|---|---|---|
| **U5** `password_utf8` — no encoding, length, normalisation or trim rule | password-protected tables | High. Two clients normalising differently produce different proofs and the join fails as *bad password*, which is the one diagnosis that will send a user looking at their keyboard. |
| **U11** *"the parameters it joined under"* — never pinned to a stored object | three of §4.3's comparisons | High. `LobbyStore` overwrites the held advert on every accepted re-broadcast, so *"the advert"* is a moving target unless the joiner retains its own copy. |
| **U13** `stage_type` in `stage_hash` | every chained stage | High. The tests already pass event codes, which is a resolution with no documentary backing. |
| **U16** `P(0)` on a `TABLE_READY` stage that has not completed | the whole setup chain | High. Three sections give three readings, one of which is per-receiver. |
| **U18** *"payload bytes, verbatim"* in `table_params_hash` | the digest itself | High. Decoded content or the CBOR byte string as encoded? The implementation here takes the decoded content, which is a guess and is recorded as one. |
| **U19** *"input deck bytes"* / *"final deck bytes"* | `SHUFFLE_PROOF`, `DECK_COMMIT` | High. No serialisation named, and the open deck never appears on the wire. |
| **U20** the canonical 52-card ordering | anything that names a card | High, and circular: `PROTOCOL.md` defers to `CRYPTOGRAPHY.md` and back. `src/poker/state.rs` fixes `rank*4 + suit` and `tests/deck_constants.rs` pins ziffle's open deck; neither is a document. |
| **U2** `banned`, **U3** capability mismatch | two `JOIN_REJECT` reason codes | Low. Both are advisory and a rejection proves nothing. |
| **U6** *"established connection"* | `JOIN_REQUEST` legality | Medium, and circular: it is defined as the post-`CAPABILITIES` state of the very stream the join RPC exists to avoid opening. |
| **U7** seat allocation when `requested_seat` is absent | the founder's own rule | Medium. Nothing binds `JOIN_ACCEPT n(1)` to a request that asked for a seat. |
| **U9** *"a capability covering `max_players`"* | `TABLE_READY` | Medium. `nlhe/2-6` and `nlhe/7-10` exist; no rule maps a seat count onto them. |
| **U10** retention for the seen-`join_nonce` set and the last `list_serial` | anti-replay | Medium, and it is a growth surface: remotely growable at `JOIN_REQ_MAX` per request, in the one part of the pipeline §5.3 does not bound. |
| **U12** `TABLE_READY n(4) capability_set` element rules | `TABLE_READY` | Medium. Only a count; `HELLO n(5)` spells out everything this one omits. |
| **U14** *"the first `JOIN_ACCEPT`"* as the abandonment anchor | formation timeout | Medium. Founder's first issued or each joiner's first received — the two give peers different absolute deadlines. |
| **U15** *"the table does not start"* | two failure paths | Medium. No message, state or terminal value, and §4.1 ends the table key's authority at `TABLE_READY`, so there is nothing left to re-form with. |
| **U21** *"occupied seat"* | `HAND_INIT` | Medium. Used normatively three times against a status vocabulary that changed when `Absent` was deleted. |
| **U22** *person*, *identity layer*, *per-identity abort count* | the threat model's own scope | Medium. The absence of an identity layer is the stated reason an attack class is out of scope, and nothing says what one would provide. The abort count is named as the entire remaining sanction with no field behind it. |
| **U23** the formation vocabulary of `STATE_MACHINE.md` | the engine's formation phases | Medium. |

### What this does not claim

That the list is complete. It is what six parallel readings of five documents
found, and the method that found it — read every source independently, then
reconcile — is the method that should be repeated for the hand path and the
dispute path before either is built.

## Open decisions

### How to read the identifiers in this list (`Q5`) — two live `P` series, and which is which

**The problem, stated before the table because it changes what the table means.**
Two independent defect series have been using the bare labels `P1 … P8` at the
same time, in the same files, and three of them collide outright — **`P3`, `P5`
and `P8`**. `PROTOCOL.md` contains two different `P3`s: l. 83 *"P3 is settled
here"* is the witness-independent terminal stage, and l. 5833 *"this is `P3`"* is
the hand-deadline floor. `STATE_MACHINE.md` T50 cites *"(P5, T57)"* meaning the
frozen `Diverged` phase, while this list's `P5` was `NETWORK_STACK.md`'s
prohibition. §2.3 cites *"(P8, §9.4, Q6)"* meaning deferred mid-session seat
entry, while this list's `P8` was a disposition error in `D-014-1`. The failure
mode is not confusion, it is an editor **fixing the wrong item** — which is
exactly how `N-4` came to quote a predicate that had been superseded.

**The convention, and it is one rule.** *An item is prefixed with the gate that
opened it, except the first, which keeps the bare labels it has held longest.*

| D-015-1 | Apply D-015 across the corpus: nothing emits or consumes `TIMEOUT_VOTE`, `TIMEOUT_CERT` or `EquivocationProof` in version 1, while their definitions and codes stay. Touches `PROTOCOL.md`, `STATE_MACHINE.md`, `THREAT_MODEL.md`, `CRYPTOGRAPHY.md`, `NETWORK_STACK.md`. | all — **done**. `PROTOCOL.md`: header box (defined, not produced, receiver drops at §4.0 step 6 / step 11), §4.8, §4.11's two rows and interleaving rows 31–32, §5.2.4, §5.3 (the two subject-class structures are **not allocated**, and the bound they would have needed is recorded — 26 of 28.3 MiB), §8.3, §8.4, §8.5, §10.1's re-entry route, `OQ-F` closed. `STATE_MACHINE.md`: `Event::TimeoutCertificate` deleted, **T16, T22, T27, T34, T41, T44** deleted (57 live transitions), `certified_subjects` deleted from `TableState`, **I29 retired** (33 invariants), §12.1.1 re-derived for all twenty phases with each exit reported. `THREAT_MODEL.md`: §5.3's D-015 reclassification block (X10, X30, X31 → **CP**; X7 → **DNA** uniformly; X8 and X16 marked), §5.4 totals, **§9.1.0 item 5** carrying the three costs. `CRYPTOGRAPHY.md`: **one** occurrence, §2.10's `|V|` branch, edited (plus §2.9's sequence line and three consequential clauses). `NETWORK_STACK.md`: **zero** assumptions — every occurrence is a prohibition, kept in full and marked vacuous for two of its items. |
| D-015-2 | Before real money: an equivocation is now detected but not provable to a third party who was not present. Decide whether that is acceptable, or reinstate the proof object with the machinery reviewed. | a later numbered decision |
| F-1 | **D-015 made a previously redundant answer to Q-01 load-bearing.** Deleting T44 left §8.5 auto-emission covering only the *action* deadline, so phase 14 is kept human-free solely by `showdown_policy = MANDATORY_REVEAL`. If Q-01 answers `TDA_MUCK`, the showdown gains a human decision with no auto-rule and no certificate, and its only exit becomes T57 - up to an hour per hand. Either answer Q-01 as MANDATORY_REVEAL and record the dependency, or give the muck decision its own auto-rule. | `STATE_MACHINE.md`, and Q-01 |
| Series | Opened by | Labels | What they are about |
|---|---|---|---|
| **The original `P` series** — **keeps bare `P1 … P8`** | `research/PHASE2_GATE.md` and `PHASE1_VERIFY3.md` | `P1`, `P2`, … `P8` | **Phase and reachability defects in the state machine.** `P1` unreachable phases, `P3` the witness-independent terminal stage, `P5` `Diverged` with no liveness exit, `P8` mid-session seat entry deferred rather than described. Cited throughout `STATE_MACHINE.md` (its numbered gate list, T50, §2.3, §9.4) and in `PROTOCOL.md` §3.2, §4.4, §4.11 |
| **The fourth Phase 3 gate's `P` series** — **renamed `G4-P1 … G4-P8`** | `research/PHASE3_GATE4.md` | `G4-P1`, `G4-P2`, … `G4-P8` | **Receiver, wire and corpus-integration defects.** `G4-P1` one checkpoint slot where the wire needs three, `G4-P2` the replayed checkpoint-8 stall, `G4-P3` `hand_deadline_ms` not scaling with the seat count, `G4-P4` the parent clause wrongly in tier 1, `G4-P5` `NETWORK_STACK.md`'s prohibition versus D-014, `G4-P8` the `D-014-1` disposition error |
| **The fifth Phase 3 gate's series** — **`G5-Q1 … G5-Q6`** | `research/PHASE3_GATE5.md` | `G5-Q1` … `G5-Q6` | This pass's findings. `Q` did not collide with a live `P`, but it does collide with `STATE_MACHINE.md`'s own **open questions `Q1 … Q6`** (§9.4, §2.3 cite `Q6` as a question, not a defect), so the same prefix rule applies to it from the start |

**The second series is the one renamed, and the reason is cost, not seniority.**
The original `P` labels are load-bearing inside `STATE_MACHINE.md`'s normative
text and in the archived gate reports, which are records and are not edited;
renaming them would falsify the evidence trail. The fourth gate's series is a
year younger, lives mostly in this list, and its cross-references are countable.

**Rows in the table below now carry the prefixed form, with the old label kept in
the cell** so a reader searching for the string they remember still lands. **What
is not done here, and is filed as its own row:** the bare `P2` and `P3` citations
inside `PROTOCOL.md` that belong to the fourth gate — the deadline-floor sites at
§8.2, §9.4, §13 and §12 — are another owner's text and are not edited from here
(D-011 rule 1, D-013's process rule).

**The same collision exists in a second form and is worth naming while the
convention is being written: bare versus hyphenated.** `N1`/`N-1e`, `N4`/`N-4`,
`N8`/`N-8`, `N9`/`N-9` and `P4`/`P-4` are five pairs of identifiers for one defect
each, in one list, and in three of the five the pass that updated one row left the
other saying the opposite. **A hyphen is not a namespace.** New rows take the
prefixed form and no row may be re-opened under a punctuation variant of a label
that already exists.

| # | Question | Blocking |
|---|---|---|
| — | Open-source licence for the project (MIT / Apache-2.0 / dual / GPL-3.0 / AGPL-3.0) | Nothing yet; needed before publication |
| — | Relay admission: `identify` protocol name, or lobby presence (see D-002) | `NETWORK_STACK.md` |
| G7-S3 | **DONE in `PROTOCOL.md` this pass, and the general check it forced is the part worth keeping.** §13's `RATED_SNG_POKERTH_V1` block pinned **none** of `n(1) mode`, `n(7) min_buyin` and `n(8) max_buyin`, all three of which are parts of `table_params_hash` (§3.1); §7.2 gave each only a range. Two clients both correctly implementing the preset therefore computed **different** `table_params_hash` values, **neither wrong**, and could not join each other's table — `G6-R1`'s defect a third time, and invisible for the same reason each time: a **missing** line reads as an omission, never as a contradiction, and §7.2 rule 3's *"the name implies §13's exact values"* had no values to imply. **The three are forced, not chosen:** `mode = 2` follows from `n(9) start_stack`, which §7.2 admits in tournament modes only, and a Sit-and-Go's buy-in **is** its starting stack, so `n(7) = n(8) = n(9) = 10 000`. **What landed:** the three values pinned; §7.2 rule 2 now enforces `min_buyin == max_buyin == start_stack` in tournament modes and `small_blind == blind_schedule.first_small_blind`, so neither pair can be filled in independently ever again; and **both** of §13's named-configuration blocks are re-written indexed to `n(…)`, listing all twenty-five parts including the ones §7.2 admits a single legal value for — so the audit is a diff against §3.1's box and a part with no line is visible at a glance. **The walk of all twenty-five found two more, both fixed:** the rated block called `n(11)` *"seats"*, a second name for a value in the section whose own preamble says no value has two names; and `n(4) small_blind` is a **distinct part** from `n(13.2) first_small_blind`, which every configuration pinned only one of. Two duplicate copies of the rated values are removed rather than corrected, which is `G7-S1`'s own lesson applied inside this document: §7.2's `BlindSchedule` paragraph carried a third copy of four of them, and §13 carried a second copy of §8.2's floor formulae **and** of its per-seat table. **One note for whoever closes `G7-S1`,** since it is the same numbers in another owner's file: `STATE_MACHINE.md` §9.1 is the corpus's remaining second copy of the rated preset, and if it is kept rather than replaced by a reference it inherits `G7-S3` — it pins `mode` but **not** `n(7) min_buyin` or `n(8) max_buyin`, and it calls `n(11)` *"seats"*. Deleting it is the fix that closes both findings at once. | `PROTOCOL.md` §13, §7.2 — **closed**; the code half is `G7-S9`, and `STATE_MACHINE.md` §9.1 folds into `G7-S1` |
| G7-S6 | **DONE. `n = 8` was missing from both of `PROTOCOL.md` §8.2's tables** — floor `1 977 000`, `REOPENING_COST` `175 000`, admitted minimum `2 152 000`. The rows are added. The value was **not** in doubt: `src/poker/tournament.rs`'s `the_deadline_floor_matches_the_published_derivation` already enumerates `(8, 1_977_000)`, so the code carried the row the document it cites did not. Filed rather than merely fixed because that is the shape: a test that reproduces *"the table §8.2 derives, exactly"* passed while the table had one fewer row than the test. | `PROTOCOL.md` §8.2 — **closed** |
| G7-S7 | **`PROTOCOL.md`'s three sites are fixed; six in `THREAT_MODEL.md` are owed and are not edited from here (D-011 rule 1).** `RATED_SNG_POKERTH_V1`'s `hand_deadline_ms` is **3 300 000 ms — fifty-five minutes**, and the corpus still prices the stall at *"ten minutes"*, which was the retired 600 000. `PROTOCOL.md`: §3.2 said *"38 minutes at ten seats"* — that is the **floor**, `2 297 000`, not the preset's deadline, so it was wrong in a second way; §8.3's *"waiting ten minutes for the same outcome"* now names `hand_deadline_ms` and no number at all, which is the better fix wherever the magnitude is not the point; §14 note 4's *"ten minutes rather than thirty seconds"* is corrected. **Owed:** `THREAT_MODEL.md` l. 1248 (X34), 1819, 1830, 1851–1852, 1905 and 2255 — all six price an escape or a stall in *"ten minutes"*, X8's and X34's are quoted to users, and 2255 states it as *"beyond the ten minutes of `hand_deadline_ms`"*, which is a claim about a named constant rather than an aside. `STATE_MACHINE.md`'s dozen are already `G4-P3-e`'s second point and are not re-filed here. **None is a correctness claim and all are wrong**, which is exactly why they survive passes: nothing tests prose. | `THREAT_MODEL.md` (six sites); `STATE_MACHINE.md` under `G4-P3-e` — **low, and entirely an editor's** |
| G7-S9 | **What the code owes for a two-seat table, now that §13 has a heads-up *reference configuration* and not a named preset. `G6-R6`'s two lines have landed; this is larger.** `src/poker/tournament.rs` renamed the constant to `HEADS_UP_CUSTOM_2P`, its `id` is `"CUSTOM"`, and the *"named so that two clients can agree on it without negotiating"* comment is gone — so `G6-R6`'s wire-name half is **closed**. What replaces the name is a **complete advert**, and that is what does not exist: a `Preset` carries **14** of §3.1's 25 parts, and the 11 it does not carry — `n(0) game`, `n(1) mode`, `n(4) small_blind`, `n(5) big_blind`, `n(7) min_buyin`, `n(8) max_buyin`, `n(13.0) blind_schedule.mode`, `n(20) button_rule`, `n(21) odd_chip_rule`, `n(22) showdown_policy`, `n(24) deck_suite` — are all hashed. A client that defaults any of them computes a `table_params_hash` no other client reproduces: `G7-S3` in the code instead of in a document. **Three things are owed, none of them a value.** (1) The `LOBBY_TABLE_AD` payload type owns the 25 parts, not `Preset`; the single-legal-value fields are filled **explicitly**, because a part nobody writes is a part nobody notices is wrong. (2) `Preset::id` is `&'static str`, so a third preset name is representable and §7.2 rule 3 catches it only at a receiver — it must be `constants::PresetId`, which already exists as the closed two-value enum, so this client cannot emit a name a conforming joiner rejects. (3) **Units are part of the hash:** `Preset` stores `*_sec` and every timing part is hashed as `u32_be(…_ms)`; a client that hashes seconds sits at a different table, silently. Plus `G6-R6`'s outstanding test — every advert this client emits carries a `preset_id` inside the enum — which cannot be written until an advert can be built. | `src/protocol/messages.rs` (no advert payload type yet), `src/poker/tournament.rs` — **medium; it is the first thing the wire needs and it is where `G7-S3` will land next** |
| G8-C3 | `PROTOCOL.md` §7.2 gives `n(13.1) every_n_hands` no lower bound, so a conforming advert may carry `0` and every peer divides by zero computing the blind level - deterministically, at the same moment, from a signed advert the founder chose. Confirmed: rustc refuses to compile the constant case. The code now refuses such an advert in `Preset::validate` and reads zero as *never raise* if one ever reaches the engine, but the wire format still permits it. §9.4 claims this is *"reported for that owner list under D-013"* and no such row existed, which is `G6-R2` again. | `PROTOCOL.md` |
| G6-R6 | **DECIDED in this pass: the name is dropped from the wire, not pinned in §13, and one line of the fix is owed to the code.** `HEADS_UP_PLAY_MONEY_V1` was a named preset no document defined, while §7.2 rule 3 makes a preset *name* imply exact values and only `RATED_SNG_POKERTH_V1` is pinned — the `G6-R1` hazard a second time. **Pinning was rejected on provenance.** A `preset_id` is a claim about values carried by a name, and it earns its cost only where an outside authority fixes the values: PokerTH's rated-game settings check does that for the rated preset and there is **no analogous authority for a two-seat rated game**, so pinning would manufacture the appearance of provenance for numbers this corpus invented, and would add a second value set that two documents, a source file and a fixture must be kept in step on — which is how `G6-R1` happened. **What landed instead**, all in `PROTOCOL.md`: §7.2 `n(2)` is now a **closed two-value enum** (`RATED_SNG_POKERTH_V1`, `CUSTOM`) and rule 3 rejects any third value **on sight, known or unknown** — which closes the hazard for every future invented name and not just this one; §13 carries the twelve values as a **heads-up reference configuration**, explicitly *not* a `preset_id`, with every value justified: all but two are the rated preset's, and `small_blind_cap = 10 000` is the rated preset's own **rule** rather than a new number — PokerTH caps at `seats × start_stack ÷ 2`, which is 50 000 at ten seats and 10 000 at two, so the same formula at a different seat count is not a deviation. Only `min_players_to_start = 2` and `hand_deadline_ms = 1 200 000` are genuinely this configuration's, and the second clears the new `HAND_DEADLINE_MIN(2) = 1 042 000` and buys `REOPENINGS = 7`. Advertising as `CUSTOM` costs nothing because every value is in the advert's fields and bound into `table_params_hash`, so a joiner reads numbers instead of trusting a name. **Owed to the code, and it is two lines.** `src/poker/tournament.rs` may keep the constant and the name; its `id` may not be `"HEADS_UP_PLAY_MONEY_V1"` on the wire — it advertises `"CUSTOM"` — and its doc comment *"named so that two clients can agree on it without negotiating"* is the exact claim this row refuses and must go: two clients agree on this table by reading the advert's fields, which is why the name costs nothing to drop. Add a test asserting that every advert this client emits carries a `preset_id` inside the two-value enum; the existing `for p in [RATED_SNG_POKERTH_V1, HEADS_UP_PLAY_MONEY_V1]` loops are unaffected, since they test values and not identity. | `src/poker/tournament.rs` — **the document half is closed; the wire-name half is a two-line code change** |
| G6-R2 | **The `N-1e` boundary-window residual, filed — this row is the thing whose absence was the finding.** `STATE_MACHINE.md` §2.6 l. 704 ends its residual *"It is recorded on `DECISIONS.md`'s open list rather than fixed here"* and no such row existed. **The residual.** §5.3 step 8 reads `signed_this_hand` at hand `m`'s init, and since `K-3b` that set spans the **boundary window** — `P(m-1)` together with every seat heard from between the two hand inits. A seat that signed only in the window makes the set two-membered, step 8 writes nothing, and `solitary_since` can be `None` for a hand whose retained record says solitary by the second disjunct of `P(k-1) == {self} ∨ P(k) == {self}`. **It costs no detection, and the disposition is to leave it**: a non-empty window means another seat signed a chained event this peer accepted, so hand `m` is not solitary at this peer either and hand `m`'s stage 0 is required of a set containing that seat — if the seat is really there no fork exists, and if it is not, stage 0 stalls and the hand aborts loudly at `hand_deadline_ms`. What is lost is a freeze on an **older** hand; what replaces it is a stall on the **current** one. The closure exists and is refused for cause: widening step 8 to read the boundary checkpoint's `required` snapshot — `P(m-1)` exactly, and retained — is a **second read of a second quantity for one question**, the defect `K-7` and `L7` each cost a pass, and this residual is loud where the off-by-one it replaced was silent. Revisit only if a measured trace shows the stall path failing to fire. | `STATE_MACHINE.md` §5.3 step 8 — **low; recorded and deliberately not fixed, and the record now exists** |
| G6-R2-m | **The method half of `G6-R2`, and it is a rule for `research/`, not a defect in a document.** `PHASE3_GATE5.md` read *"recorded on `DECISIONS.md`'s open list"* inside a fix and graded the residual *"Recorded rather than fixed, which is the right call"* **without opening the file it pointed at**. A filing claim is exactly the class of statement the method distrusts inside a disposition column, and it was accepted because of where it appeared. **The rule: a claim that something is filed is checked against the file, wherever the claim appears, and a gate that grades a residual as correctly filed states the row's identifier.** **The audit this pass ran, so the rule arrives with a measurement rather than as a resolution.** Every *"recorded / filed on `DECISIONS.md`'s open list"* claim in `PROTOCOL.md` was checked against this file: **nine claims, nine true.** Six name an identifier and each identifier exists — `K-3b` (§4.10; it is inside `K-3`'s row and named again in `K-9` and `N-5e`, not a row of its own, which is the weakest of the nine and still a real record), `M-1` (§4.11), `J-3` (§6.1), `OQ-A` (§6.3), `K-8` (§14 note 23), `N-1e`/`N-4` (§14). Three name none and each is findable by subject: §3.1's *when the departing stack is subtracted* is `K-8`, §8.2's *two ways of closing the reopening residual* is `G4-P3-r` and is now cited by name, and §11's *founder advertising a deadline every legal hand exceeds* is `G4-P3-e`'s third point. `PROTOCOL.md` was clean; the one false claim in the corpus was `STATE_MACHINE.md` §2.6's, and it is the row above. **A claim that names its identifier is checkable in one grep and a claim that does not is not** — new claims name the row. | `research/` convention — **adopted; audit ran, 9 of 9 true** |
| G6-R4 | **DONE in this pass. D-014 now carries the setup-chain exception**, as a named subsection after *What removal does*: at `hand_id == 0` a tier-1 finding removes nobody and the table does not start. It is forced rather than chosen — `roster_hash(k)`'s seat vector is frozen at `TABLE_READY` (`PROTOCOL.md` §3.1), so a removal there forks the table's identity between the peers that saw the evidence and those that did not — and it costs nothing, because `ledger_in == 0`, there is no hand to void, and the beacon stalls to T4 exactly as it did before D-014. The tier boundary and the verdict are untouched; only the consequence is suspended, and only where it would edit a frozen roster. `STATE_MACHINE.md` T66 and `NETWORK_STACK.md` had already decided it correctly; what was missing for four consecutive passes was the line in the decision itself. | — **closed** |
| G5-Q6 (was `Q6` in the gate report) | **DECIDED in this pass: headroom for one reopening is required.** The label is prefixed on arrival under the convention above: bare `Q6` is already `STATE_MACHINE.md` §11's open question *may a seat be added after `Seating`*, cited five times there, and this list's own rule is that the fifth gate's `Q` findings take `G5-` from the start. §7.2 rule 2a accepted an advert at exactly `HAND_DEADLINE_FLOOR(n)`, which leaves `REOPENINGS = 0`, so the **first** raise that reopens the action could carry a hand past its own deadline and end it with `cause = 1` and `attributed = []` — `P3`'s harm one raise later, on a table where everyone is honest and the only thing that happened is a re-raise. The alternative — declare a zero-headroom table legal and say its hands abort when reopened — was rejected: the floor budgets a hand nobody re-raises, which is legal and is not poker, and §8.2 already refuses exactly this harm one step lower down. **What landed**, in `PROTOCOL.md` §8.2, §7.2 and §13: `REOPENING_COST(n) = (n − 1)(action_timeout_ms + action_grace_ms)` names the denominator `REOPENINGS` already had, `HAND_DEADLINE_MIN(n) = HAND_DEADLINE_FLOOR(n) + REOPENING_COST(n)` is the **admitted minimum** rule 2a now compares against, and `n(17) >= HAND_DEADLINE_MIN(n)` is exactly `REOPENINGS >= 1`. **No quantity is added** and `HAND_DEADLINE_FLOOR` keeps its meaning everywhere else. Minima: n=2 → 1 042 000, n=4 → 1 412 000, n=6 → 1 782 000, n=10 → 2 522 000, all under the 3 600 000 cap, so the configuration space stays non-empty at every seat count. The preset's 3 300 000 and the heads-up configuration's 1 200 000 both clear it unchanged. **`G4-P3-r` is not closed by this** — a hand with more reopenings than the founder bought still aborts on a legal path; the bound moved from zero to one and the shape of the residual did not move at all. **Owed:** `STATE_MACHINE.md` §9.4's config validator must compare against `HAND_DEADLINE_MIN`, not the floor — this is the same validator line `G4-P3-e` already names, now with a different right-hand side. | `STATE_MACHINE.md` §9.4 (validator line, with `G4-P3-e`) — **the `PROTOCOL.md` half is closed** |
| G6-R7 | **A wrong cross-reference repeated corpus-wide, found while fixing `G6-R6` and `G5-Q6`, and it points at a section that has no such rules.** `PROTOCOL.md`'s advert-admission rules are numbered 1, 2, 2a, 3 … 7 in **§7.2**; §9.4 is *Collection bounds* and contains a table and some string rules and no numbered rule at all. The corpus cites the same list both ways: *"§7.2 rule 6"*, *"§7.2 rule 7"* — correct — and *"§9.4 rule 2a"*, *"§9.4 rule 3"* — pointing at nothing. **`PROTOCOL.md`'s six sites are corrected in this pass** and §7.2 now carries one line saying the rules are its own and are cited as *"§7.2 rule N"*. **What is owed elsewhere**, not edited from here (D-011 rule 1): `STATE_MACHINE.md` l. 3582, l. 4376 (a code comment) and l. 4393; this document's own `G4-P3-e` and `G5-Q5-x` rows, whose text is left as written because they are the record of a pass and are corrected by this row rather than rewritten. The cost is small and entirely an editor's: a reader who follows the citation finds collection bounds and concludes the check is not written down. `G5-Q5-x` should absorb this into its citation sweep rather than run a second one. | `STATE_MACHINE.md`; folds into `G5-Q5-x`'s sweep — **low** |
| J-1 | **DONE in this pass.** `PROTOCOL.md` has dropped `advert_hash` from `GENESIS(0)`, `session_id` and hence `ctx`, and §3.1 now carries a normative `table_params_hash` box — domain `p2p-poker v1 table-params`, twenty-five parts in `LOBBY_TABLE_AD` field order, with `timestamp_unix_ms` and `expires_at_unix_ms` excluded by name. J1(b) is closed at three checked places: §7.2 receiver rule 7, `PLAYER_LIST`'s `n(1)` and `TABLE_READY`'s `n(2)`, both of which changed from `advert_hash` to `table_params_hash` and are now **checked** rather than carried and ignored. | — |
| J-2 | **DONE in this pass.** `PROTOCOL.md` §3.2 carries the general rule and the set `P(k)`; §4.4 instantiates `R(HAND_INIT, k) = P(k-1)`; `dealt_in ⊆ P(k-1)` is normative; `RNG_COMMIT`, `RNG_REVEAL`, `STATE_HASH` and `STATE_ACK` were swept off status words too, and §2.9 is the sweep that enumerates all nine collective sets. | — |
| D-014-1 | Integrate D-014 across the corpus: the two evidence tiers, the checkpoint precondition for tier 2, the one-way exit, the dead seat blinded off, and the information window. Touches all five specification documents. **The disposition `all — not started` was wrong and is `G4-P8`: four of the five had landed it, and one had landed nothing.** Per document, with occurrence counts as the cheap check that a reader can repeat: **`PROTOCOL.md` (37) — done as the wire half.** §4.0's normative box states both tiers and the tier-2 checkpoint precondition and is the box a receiver implementer reads; §4.9's `kind = 3 CHEAT_EVIDENCE` box is the message; §11 routes it. What is *not* done there is `D-014-3`, which is a separate row and is blocking. **`STATE_MACHINE.md` (51) — done as the engine half.** T64, T65 and I34 carry the removal, the one-way exit and the blinded-off stack; T64's cell also carries `N8` correctly. Blocked, with `PROTOCOL.md`, on `D-014-3`: the rows are specified and no event fires them. **`THREAT_MODEL.md` (33) — done, and it says so itself** (§9.2's `D-014-1` row): §5.1's D&A definition, §5.2's re-classification table, §5.3's X37, §5.5's tier column and the mirror gate. **`CRYPTOGRAPHY.md` (23) — done, and the open list's `N-9` is stale in saying otherwise.** §8.1 *"What a failed proof proves, and what it does not (D-014)"* is a section written for this decision, and §8.1.5 adds the rule D-014 makes load-bearing here — a verifier stricter than the prover now ejects a person. **`NETWORK_STACK.md` — DONE in this pass, and it was the last one.** It was not merely at zero: §0.1 and §1.2 prohibition 7 stated the no-unseating rule in the corpus-wide, every-layer form D-014 tier 1 narrows, so the document asserted the negation of a binding decision. Both are re-scoped to *at this layer*, §11.5.1 is unchanged plus the T64 paragraph, **new §0.6** is the layer's D-014 pass, and §15 has a D-014 register row. **No transport behaviour changed** — the whole edit is a scope word and a boundary. That was `G4-P5`, which is now closed. That is `G4-P5`, filed as its own row below with line numbers, and it is not merely an omission: §0.1 and §1.2 prohibition 7 state the no-unseating rule in the corpus-wide form D-014 tier 1 narrows, so the document is not silent about D-014, it contradicts it. **`SPEC_CS.md` (0)** is the owner's document and is owed one thing, the §22 information window of `N8`. | **all five done** — `NETWORK_STACK.md` was the remainder and closed in this pass; `PROTOCOL.md` and `STATE_MACHINE.md` stay blocked on `D-014-3` |
| D-014-2 | The mirror test D-014 requires: under every legal interleaving, no honest peer is evictable. Blocks shipping the feature, not writing it. Specified as a test in `THREAT_MODEL.md` §5.5 beside the eleven `SPEC_CS.md` §25 peers, and its planned home is `tests/adversarial/no_honest_eviction.rs`. | adversarial suite |
| D-014-3 | **New, opened by D-014's integration, and it is the rule D-014 newly makes load-bearing rather than a packaging question. How does a removal reach canonical state?** D-014's safety argument is that *"every honest peer reaches the same verdict from data it already holds"* — which is true of the **verdict** and not of the **holding**. Whether the offending message reached this peer is a per-receiver fact; a seat's `status` is canonical state and enters `state_hash` (D-012, `STATE_MACHINE.md` I30). A peer that removes a seat on a message a second honest peer never received has forked the table, in the exact direction D-012 exists to forbid. So the *evidence* must itself be chained: a message type, a stage kind under `PROTOCOL.md` §3.2's principle, a chain position, and an answer for a peer that never received the offending message. **Second half, same owner:** `PROTOCOL.md` §6.1's `PublicTableState` hashes `sitting_out` and no longer hashes `absent`, so `SeatStatus::Removed` (`STATE_MACHINE.md` §2.4) is canonical per-seat state that **no vector hashes** — two peers disagreeing about a removal agree on every hashed vector, because a removed seat posts the same blinds and holds the same stack as an active seat that folds. A `removed` vector, or a defined use of the two existing bits, is a wire change and `PROTOCOL.md`'s call. Until both are answered, `STATE_MACHINE.md` T64/T65 are specified and have no event to fire them, which is the honest state to leave it in. | `PROTOCOL.md` §4.10/§4.11, §6.1 — **blocking for D-014** |
| N1 | **DONE in this pass, engine half, and it is the fifth consecutive instance of the rule: the worst defect was on the path the previous fix newly made load-bearing.** K-9 installed the solitary freeze and a latch; **the freeze released itself two ways, and both left `I33(c)` passing.** (1) **T53** fired on a reconciliation-round stage whose required emitter set at a solitary peer is `{self}`: the frozen peer published one re-derived value, the stage completed over its required set, every value in it agreed with itself, the freeze lifted and the latch cleared — then another solitary hand, another contradiction, forever. **Fix: T53 gains a conjunct on the *completed stage's signers*, not on the set** — *"carries values signed by at least two distinct seats"* — which is the one form of the test that does not move when `PROTOCOL.md` changes the set's definition. It is inert wherever the set has two or more members. **T54 and T60 needed nothing**: both require two distinct values to remain, and one signer fills one slot with one value. (2) **T50 set no latch at all**, so a solitary peer contradicted by a *`state_hash` mismatch* rather than by an out-of-set event thawed on the timer and re-froze at the next checkpoint 8, every `hand_deadline_ms`. **Fix: T50 sets `solitary_contradicted` when `\|checkpoint.required\| == 1`** — read off the open checkpoint, present tense, because `Event::StateHash` carries no `hand_id` and none is wanted: `checkpoint.required` **is** the regime for the stage being compared, snapshotted when the checkpoint opened. **And `I33(c)` is re-scoped from the latch to the freeze**, because a clause about a latch is vacuously true on both loops: *between any two entries into `Diverged` with a hand dealt between them there is a completed reconciliation stage carrying values signed by at least two distinct seats*. §12.1's fourth obligation is restated to match — name every path back to playing, **including the repair the fix installed for itself**. **And the rule this fix newly made load-bearing, checked in the same pass and found broken: `I33(b)` forbade the frozen peer to produce *any* `Effect::Publish`, which is the one thing T53's stage consumes** — so the exit N1 had just made the load-bearing one could never fire and the freeze had no reachable repair, which is `L4`'s shape one transition along. I33(b) now exempts exactly the §6.3 step 2–3 traffic — `DISPUTE`, the reconciliation-round `STATE_HASH` and its `STATE_ACK` — which opens no card, moves no chip and completes no stage *of the hand*, and which T52 has always accepted from this phase; and it is what makes the two-signer conjunct satisfiable, the frozen peer's own value being one of the two. | `STATE_MACHINE.md` §5.2 (T50, T53), §9.3 condition 0.6, I33(a)(b)(c) |
| N1-a | **New, owed to `PROTOCOL.md` by N1's fix, and it decides whether one exit exists rather than whether it is correct.** T53 now needs a reconciliation-round `STATE_HASH` signed by a seat other than the frozen peer. At a solitary peer that seat is **outside** `P`, so the exit is reachable only if §4.9 admits an out-of-set copy into a **reconciliation round** — the same widening the checkpoint-8 box already makes for the checkpoint itself (*"accepted, compared and retained from any occupied roster seat"*), extended by one stage. **Both answers are safe and the engine is written for either**: admitted, a genuine fork can repair and the table plays on; not admitted, T53 is unreachable in the solitary case, every solitary freeze ends at T54, T60, T57 or T61 and therefore at `TableClosed` through §9.3 condition 0.6. What must not happen is the third state — the exit described and unreachable, which is the shape the Phase 3 gate found for T62 (`L4`). One sentence in §4.9 closes it either way. **CHOSEN in this pass, by the document that has to write T53: *admit*.** `STATE_MACHINE.md` §5.2's solitary box records the choice and the reason — a reconciliation round re-derives the same checkpoint body over the same stage, so §4.9's own reason for admitting an out-of-set copy at round 0 (*"its body is a claim about who the participants are"*) does not weaken at `r >= 1`, and refusing there would be two rules for one stage separated by a `sequence` offset. Admitting costs nothing, because the admission alone releases nothing: T53 still requires the round to be complete, unanimous and signed by **two distinct seats**, and `A` cannot thaw a freeze. Refusing would make T53 unreachable in precisely the case the freeze exists for, so every solitary freeze would end at `TableClosed` through §9.3 condition 0.6 — safe, but a table closed on a fork that could have been repaired. **What is still owed is one sentence in §4.9 saying so**, because the engine now depends on the answer rather than tolerating both. | `PROTOCOL.md` §4.9 — low, one sentence, and the choice is made |
| N4 | **DONE in this pass. Two non-equivalent memories served one past-tense test; they are now one memory and a floor.** `PROTOCOL.md` §4.0 step 10b's `RetainedHand.was_solitary` (LRU, 4 096 hands) and `STATE_MACHINE.md`'s `solitary_since` both answered *"was hand `k` solitary?"*, and they disagreed after a **re-entry**: `solitary_since` was cleared to `None` on regime exit, so hands 5–7 solitary, hand 8 not, hands 9–11 solitary left the engine evaluating `9 <= 6` for an event naming hand 6 while the wire's record said *solitary* — **the wire says freeze and the engine says reject**, and I21 discards the only evidence of the fork. The contiguity argument §2.6 gave for one `u64` is **false and is deleted**: `P` grows through a `PLAYER_SIT_IN` and can shrink again at the next hand init, so the solitary hands are a *union* of intervals. **Resolution: the retained record is the sole authority on which hands were solitary; `solitary_since` becomes monotone — written once, never cleared — and is a floor under it**, with `solitary_at(k) := solitary_since == Some(j) ∧ j <= k <= hand_id` read by T62 and T63 only. The lemma is in §2.6 and is asserted as **I33(a)** in place of the deleted interval property: *if the record says hand `k` was solitary, `solitary_at(k)` holds*, so the engine never rejects an event the wire freezes on and the conjunction of the two tests is the wire's test. The two memories forget differently — an evicted record answers *"not solitary"*, the floor forgets nothing — and **both errors point at *no freeze*, never at a freeze on the wrong hand**, which is the property that makes an ordered pair acceptable where an unordered pair was not. | `STATE_MACHINE.md` §2.6, §5.3 step 8, §5.2 T62, I33(a) |
| N4-a | **New, owed to `PROTOCOL.md` by N4's resolution, and it is an off-by-one between two fields of one struct.** The engine now defers to `RetainedHand.was_solitary` as the **sole authority**, so its definition is load-bearing for T62. `RetainedHand` carries `was_solitary: bool` beside `p: SeatSet`, described as *"`P(hand_id)` as this receiver derived it"* — and **`was_solitary` for hand `k` is `\|P(k-1)\| == 1`, the set read at hand `k`'s init, not `\|p\| == 1`**. The two differ exactly on the **last hand of every solitary regime**: a `PLAYER_SIT_IN` accepted during hand `k` adds its sender to `P(k)`, so `\|p\| == 2` while hand `k` was dealt solitary. An implementer who derives the boolean from the neighbouring field — the obvious thing to do, since it looks redundant — answers *"not solitary"* for the one hand a contradicting event is most likely to name, and §4.0 step 10b drops it. Either state the derivation in §4.0 step 10b in one sentence, or delete the boolean and store `P(hand_id - 1)`'s cardinality under a name that cannot be confused with `p`. | `PROTOCOL.md` §4.0 step 10b — low, and the cost of getting it wrong is a silent missed detection |
| D-014-t | **DONE in this pass, and it is one word in a list.** §5.3 hand-init **step 6** posted antes for every seat with chips that is `Active`, `SittingOut` or `Absent` — and **not** `Removed`, while §2.4 says the new status *"behaves like `Absent` for the blinds (steps 6–7)"* and D-014 point 3 requires the offender's stack to be **blinded off**. Step 7 was already right because it is positional and reads no status; step 6 read a status word and listed three of the four. Inert in `RATED_SNG_POKERTH_V1`, where the ante is `0`, and live at any §9.4 custom table that sets one — which is why it was found by reading the list rather than by playing a hand. `Removed` is added. The claim it protects is §12.1.1's: the removed seat's stack strictly decreases every orbit, so it busts in a bounded number of hands and §9.3 condition 1 stays reachable. | `STATE_MACHINE.md` §5.3 step 6 — **closed** |
| N8 | **DONE as a record, not as an edit, because the target is the owner's document.** D-014 says the removal is explained by *"`SPEC_CS.md` §22's information window"*. **§22 does not contain one**: its GUI tree ends at `timer` and `protocol/security status`, and no anti-cheat window is listed. It is therefore a **required addition to §22** — naming the removed player, the tier, what the evidence was, and that the hand was voided and no chips changed hands — and it is recorded as one in `THREAT_MODEL.md` §9.2 (row `N8`) and in `STATE_MACHINE.md` T64's cell, which no longer cites it as an existing element. Nothing in the corpus may cite it as present until the owner adds it. | `SPEC_CS.md` §22 — owner's edit; recorded in `THREAT_MODEL.md`, `STATE_MACHINE.md` |
| L8 | **DONE in this pass, three passes after it was first named, and the delay is the finding.** `CONTRIBUTING.md` and `DEPENDENCIES.md` both carried sweep records stopping at D-012 with **zero** occurrences of D-013 and D-014. Both now carry dated D-013/D-014 sweep records reporting counts before and after. `CONTRIBUTING.md` additionally: a fourth numbered failure of its own §2.5 rule — *the same two files, skipped again, by the two sweeps that followed the section that forbids skipping them* — the header paragraph a contributor reads first, saying that **a deferred documentation defect does not decay gracefully, it is read as current by everyone who arrives after it**, and two new §3 checklist items (7a for D-013's *no progress path reads a status word*; 5a for D-014's four admissibility questions, with item 5 narrowed rather than replaced). `DEPENDENCIES.md`'s one substantive finding: **D-014 makes `ziffle`'s false-*reject* behaviour load-bearing against a person**, so §3.1's OQ-1 review now discharges *nobody is ejected wrongly* as well as deck integrity, and §5.5's `libp2p-allow-block-list` note is extended to say D-014 does **not** reopen transport-layer blocking. The re-grading rule is written down where it failed: a documentation defect that survives one pass is re-graded, not re-deferred. | `CONTRIBUTING.md`, `DEPENDENCIES.md` — **closed** |
| N9 | **DONE in this pass.** `CRYPTOGRAPHY.md` carried **zero** occurrences of D-014 while owning **three of tier 1's six clauses** — the shuffle proof, the decryption-share proof and the key-ownership proof. New **§8.1** states, for each, what a verification failure **proves** (evidence against its signer, computed by a pure function of the message and *agreed* public inputs, hence **no quorum, no vote, no timing — which is why tier 1 can act on arrival**) and what it does **not**: a failure says *either the prover cheated or we are not looking at the same inputs*, so `apk`, the input deck and `ctx` must be accepted chain content or the verdict is a divergence and not a removal; it does not localise which card; a **missing** token is silence and never evidence; and a token that verifies but arrives for an undue index is **tier 2**, not tier 1. §8.1.4 records that D-014's *deck gains or loses a card* clause is the **same verdict** as the shuffle proof and not a second check — a hand-rolled conservation check is a second verifier, which now costs a player their seat rather than a message. **§8.1.5 is the rule this document owed the decision:** *a removal may rest only on a verifier that ran and returned `invalid`, never on a verification that could not be performed* — a decode failure, a truncated body, a caught panic or a missing input rejects or defers and produces no evidence. Also: §12 obligation `0`, and pointers at §2.1, §6.1 and §8 rule 3. | `CRYPTOGRAPHY.md` §8.1 — **closed** |
| K-9-b | **Correction to the K-9 row above, recorded because that row is what a future editor will re-derive from.** K-9 justified one `u64` with *"the regime is **contiguous** … the solitary hands are an interval"*. **That is false** — see `N4` — and the fix does not restore the interval, it stops needing one. Read the K-9 row for what landed and this row for what the argument was worth. | `DECISIONS.md` — editorial |
| K-9 | **DONE in this pass.** `STATE_MACHINE.md` carries the engine half: **no new phase** — the target is `Diverged`, which is what `PROTOCOL.md` §6.3 step 1's freeze already is — and **T62** is the transition, guarded in the **past tense** on `solitary_since <= e.hand_id <= hand_id`. `solitary_since: Option<u64>` (§2.6) replaces the retained per-hand map the gate report proposed, because the regime is **contiguous**: inside it, `P` can only grow through an accepted `0x0804 PLAYER_SIT_IN`, so the solitary hands are an interval and one `u64` answers the past-tense test with no cap and no eviction policy. The regime test is `\|signed_this_hand\| == 1` and **not** `== {me}`, because `step` may not read `LocalView`; the two are the same predicate exactly because a peer's own emission counts into its own set, which is the sub-question K-1 answered. **T63 is the row the count would otherwise hide: `TableClosed` stops being absorbing for this one event class**, because a solitary hand self-completes and the contradicting event normally arrives after the drain has finished — the peer that reached `TableClosed` first would otherwise keep a private "tournament won" and discard the only evidence against it; T63 retracts `settlement.tournament_winner`. **And the defect this fix newly made load-bearing, found and closed in the same pass: without a latch the freeze is undone by the timer that ends the hand it froze in** — T57 or T61 fires, T46 restores, T47 deals another solitary hand, the next contradicting event freezes again, every `hand_deadline_ms`, forever, with every row of §12.1 passing. That is J2's fixed point one link out. **§9.3 condition 0.6** and **I33(c)** close it; only T53 clears the latch. Also delivered: `signed_this_hand` includes self and excludes `PLAYER_LEAVE` (§5.3 step 4(i) and (ii)), and §5.3 step 9's `\|dealt_in\| == 1 → Settling` is **unchanged** — the drain is preserved, which is what §3.2 requires and what the rejected alternative would have deleted. Counts: 57 → **62** transitions (T62–T66), 32 → **34** invariants (I33, I34). | `STATE_MACHINE.md` §2.4, §2.6, §4.1, §5.2, §5.3, §9.3, §10, §12.1 |
| L4 | **`PROTOCOL.md`'s this pass; recorded here with the engine's acceptance criterion, because the two halves are useless apart.** The solitary freeze's trigger is a chained event naming a hand this receiver has **finished** — a solitary hand self-completes every stage and passes through `Settling` and `HandComplete` at local-computation speed, so the contradicting event normally arrives after the hand it contradicts has closed. `STATE_MACHINE.md` T62's guard is written in the past tense against `solitary_since` and asserted by I33(a). **The criterion: `PROTOCOL.md` §4.0 must evaluate a stale-hand chained event rather than drop it, at a step that reaches step 12, and §3.2's regime test must read "a peer *was* in the solitary regime for hand `k`".** If the staleness step lands after step 12, or stays scoped on the receiver's current `hand_id`, **T62 is unreachable and K-9 is not discharged** — the rule would be correct and never fire, which is the shape the Phase 3 gate found. An editor who takes the present-tense reading must delete T62 in the same edit and say what replaces it. | `PROTOCOL.md` §4.0, §3.2 — **blocking for K-9** |
| K-9-a | **DONE.** `PROTOCOL.md` §6.3 step 1 carries the withdrawal in terms — the *"there is no checkpoint on that path to compare hashes at"* justification is deleted and replaced by the two-complementary-triggers paragraph, and §3.2 no longer carries it at all. §4.9's reconciliation floor is what now makes the checkpoint that exists useful to the freeze rather than fatal to it (`N1`). | — |
| L7 | **DONE in this pass.** `STATE_MACHINE.md` §9.3 condition 2 reads `\|dealt_in\| == 0` and conditions 2, 3 and 4 are now §5.3 step 9's three branches in step 9's order on step 9's quantity. The way into `Paused` and the way out of it read one predicate at last — K-7 fixed the exit a pass earlier and this is the entry. **Condition 0.6 was added in the same section by K-9**, and the §9.3 header now says the conditions are evaluated after hand init has run rather than after step 0, which is what makes reading `dealt_in` there well-defined. | `STATE_MACHINE.md` §9.3 |
| L8 (as it stood) | **Original text, kept for the trail; the disposition is the `L8` row above.** **Not acted on here; recorded so it is owned.** `CONTRIBUTING.md` and `DEPENDENCIES.md` carry dated sweep records that stop at **D-012** and **zero** occurrences of D-013 or D-014, while both cite D-012's process rule that every decision sweep covers every document. One row each recording *"no change, and here is why"* discharges it. `THREAT_MODEL.md` §9.2 now reports its own coverage counts rather than asserting a sweep, which is the same discipline applied where it could be applied in this pass. | `CONTRIBUTING.md`, `DEPENDENCIES.md` — low |
| G4-P4 (was `P-4`, and `P4` in the gate report) | **DONE in this pass; the documents have caught up with the code, and one row had to be re-classified rather than edited.** The clause *the event chains to a parent that does not exist* is deleted from **D-014's own tier-1 list** (above, with the reason and a pointer to the general test), from `PROTOCOL.md` **§4.0's normative box** (which now also states that it may not return, and why) and **§4.9's acceptance gate** for `cause = 6` tier 1 (whose list is now closed and enumerated), and from `THREAT_MODEL.md` **§5.1's D&A cell**. `PROTOCOL.md` §3.4's *why the carrier has to be unchained* argument keeps its point on the three examples that survive. **`THREAT_MODEL.md` §5.2 row 13 could not simply lose a clause**: *replay of old actions* was tier **1** *because of* the parent check, so removing it leaves the row with no tier — which is the right answer, since **CP** already answers replay (the signed body binds `hand_id`, `sequence` and `previous_event_hash`, so a replayed event is valid at exactly one position and is rejected and named). A replay also illegal against checkpoint-fixed state reaches tier **2** by that route and its own precondition. **New `THREAT_MODEL.md` §5.1.1** states the general admissibility test, because the reasoning is what a future addition must be held to and this document owns the classification scheme: a clause is tier 1 only if deciding it reads nothing the receiver stores, cannot change with what the network did, and needs no second message, count, vote or certificate — *local* and *receiver-independent* being different properties, which is the confusion the parent check arrived in. Original text follows. `SelfContained::ParentUnknown` was deleted from `src/security/validation.rs`: whether a parent exists is decidable only against the receiver own store, so one dropped frame would remove an honest player - the exact failure D-014 two tiers exist to prevent, sitting in the tier meant to be safe. The documents still carry it as tier 1: `DECISIONS.md` D-014 own text, `PROTOCOL.md` l. 1987 and l. 3777, `THREAT_MODEL.md` l. 1005 and l. 1082. Filed here because the code moved first and D-013 process rule says a cross-owner change is recorded in the pass that makes it. | `PROTOCOL.md`, `THREAT_MODEL.md`, and D-014 above |
| M-1 | **DONE in this pass.** `PROTOCOL.md` §4.11's prose no longer contradicts its own table: four table-mesh messages carry `chain_scope = 0` — `HELLO`, `CAPABILITIES`, `PLAYER_LIST`, `DISPUTE` — and the sentence that claimed one now says which claim was actually intended (*`DISPUTE` is the only message of groups 3–8 with `chain_scope = 0`*, which is what makes it the one message legal outside its stage) and directs the sentinel check to §2.3's exhaustive list or to the table, never to prose. Build-order item 3 is unblocked. | — |
| K-1 | **DONE in this pass — by detection, not by prevention.** `PROTOCOL.md` §3.2's justifying sentence is **deleted**: it inverted P3's property and the inversion was the whole defect. What replaces it states the truth — *`P(k)` is agreed exactly where it is inert and per-receiver exactly where it acts* — and records that **no construction removes the residual**: ratifying the stalled stage needs a collective step at the point collectivity failed, ratifying it in the abort body is the same per-receiver quantity renamed and inadmissible under D-012, and refusing to narrow `P` restores D-013's fixed point. The adopted rule is the **solitary-stage rule** (§3.2, §4.0 step 12a, §6.3 step 1's second entry condition): a peer whose required emitter set for a collective stage is `{self}` alone may still deal — the drain path and §12.1.2's ending survive intact — but a chained event of that hand from a seat outside `P(k-1)`, other than `PLAYER_SIT_IN`, is a **state divergence and not a rejection**; the peer freezes, completes no stage, awards no pot and evaluates no end condition. It is unilateral, needs no reply, and fires on the *first* solitary hand, because the peer that did not narrow is still emitting. The rejected alternative was flooring `\|P(k)\|` at 2, which prevents the fork but deletes the drain — no bust, no end condition, D-013's fixed point from the other side — and strands the paused peer. **Residual, stated in §3.2:** a *bidirectional* partition still ends in two peers each draining the other, which is what D-013's drain does under a partition however `P` is defined and carries no wire evidence to separate it from the case the drain exists for. **The sub-question is answered: a peer's own emission counts into its own `signed_this_hand`**, by reductio — the base case is *the signers of `TABLE_READY`*, and under the other reading a peer is never in its own `P` and so is never a permitted emitter of the `HAND_INIT` it is required to emit. Also settled: **`PLAYER_LEAVE` counts into no `P`** (it would make every announced departure a required emitter of the next hand). `Q-10` stays open and is now labelled as unclosable by ratification. `STATE_MACHINE.md` owes the engine half: `signed_this_hand` includes self, and the solitary regime must reach a frozen phase rather than `Settling`. | `STATE_MACHINE.md` §5.3 step 4, I31 |
| K-1 (cont.) | **Original text.** **BLOCKER. `P(k)` is a per-receiver quantity on the one path where it narrows, and its payoff is a silent permanent fork.** `PROTOCOL.md` §3.2 justifies D-013 with *"the terminal stage of a chain is witness-independent, so two honest peers that reach `TERMINAL(k)` have accepted the same chain-`k` prefix and derive the same `P(k)`"*. That inverts P3's property: `ABORT_TERMINAL(k)` is a function of `GENESIS(k)` alone **precisely so that peers with different prefixes reach it**. §3.2 also proves that `P` can narrow **only** at a stalled stage 0 — i.e. only on a hand that aborted — so `P(k)` is agreed exactly where it is inert and per-receiver exactly where it acts. Two peers that once disagree about `dealt_in` each reject the other's `HAND_INIT` copy, so each holds `P = {self}` from the next hand, every collective stage becomes self-completing, and both play the drain path alone to a §9.3 condition 1 "tournament won" naming themselves. One dropped frame is enough; no invariant, deadline, equivocation predicate or chip-conservation check fires. Also undefined and deciding which failure occurs: **whether a peer's own emission counts into its own `signed_this_hand`** — counts ⇒ fork, does not count ⇒ `Paused`. This is `Q-10` / `Q8`, which both documents defer to the other. Owner decision needed: ratify the stalled stage, or floor `\|P(k)\|` at 2. | — |
| K-2 | **DONE in this pass.** `PROTOCOL.md` §4.10 carries a normative **hand boundary window** box and `Q-09` is closed in §12. Chain: **chain `k`**, the hand that just ended, `hand_id = k` — chain `k+1` is refused because a boundary event would then chain from a `GENESIS(k+1)` that a `PLAYER_LEAVE` is an input to. `sequence = BOUNDARY_SEQUENCE_BASE + sender_seat`, `BOUNDARY_SEQUENCE_BASE = 4 096` (§13), disjoint from every stage index since `MAX_STAGES_PER_HAND = 2 048`; one reserved slot per seat, hence **one boundary event per seat per boundary** and capacity one by construction. Parent: `previous_event_hash = TERMINAL(k)` for **every** event of the window — `stage_hash(BOUNDARY_SEQUENCE_BASE - 1) := TERMINAL(k)`, beside `stage_hash(-1) = GENESIS(hand_id)` — because a seat signs without knowing who else will emit, so a running parent is not computable; the window is a fan, not a chain, and has **no `stage_hash`**, nothing chaining from it. This is the second exemption from §4.0 step 10a's chain-position rule and the count is stated. Total order: ascending seat index, which the `sequence` rule already fixes. Window closes at this receiver's acceptance of a complete `HAND_INIT(k+1)`. **The roster half is closed by deletion, not by definition:** §3.1 now says normatively that the seat vector of `roster_hash(k)` is **frozen at `TABLE_READY` for the life of the table** — which §4.3 already said in terms — so `PLAYER_LEAVE` removes no seat, `GENESIS(k)` is a function of `TERMINAL(k-1)` and session constants alone, and a window disagreement can no longer fork the genesis. It reaches canonical state only through `HAND_INIT(k+1)`'s collective byte-identical body, where it **stalls stage 0** loudly. **That routes it onto the one path where `P` narrows, so K-2's disposition depends on K-1's solitary-stage rule and the two must be read together**; both documents say so. §4.11 rows 37–39 and the honest-interleaving rows 37–39 cite the box. | — |
| K-2 (cont.) | **Original text.** **BLOCKER. `Q-09` is a wire question, not a placement question, and it sits under the sole re-entry path.** §12 grades it *"a placement decision, not a wire change"*. `hand_id` and `sequence` are inside `TO_BE_SIGNED` (§2.4), are two of the eight slot-key components §4.0 step 10a reads whole, fix `previous_event_hash` through §3.2, and are what §4.0 step 12 asks about. Nothing assigns a total order when two seats emit boundary events at one boundary. Under D-013 a seat outside `P(k)` may legally emit **exactly one** chained event — `PLAYER_SIT_IN` — so the entire re-entry mechanism runs through the message whose chain position is undefined; and an accepted `PLAYER_LEAVE` changes `roster_hash(k)`, so a placement disagreement forks `GENESIS(k)` outright. Fix: §4.10 states chain, `sequence`, parent and total order for `0x0803`/`0x0804`/`0x0805` in one paragraph; §4.11 rows 37–39 cite it; `Q-09` closes. | — |
| K-3 | **BLOCKER. A drain hand places no checkpoint, so the corpus's only cross-peer detection route is inert in the regime D-013 made a steady state.** §6.2's checkpoints are *"these points and no others"*; 2–7 all require `DECK_COMMIT` or a betting round, and `STATE_MACHINE.md` §5.3 step 9 sends `\|dealt_in\| == 1` straight to `Settling`. §12.1.2 says a silent opponent produces **about forty consecutive drain hands**. §5.2.4's promise — *"the next checkpoint shows two `state_hash` values; §6.3 runs; the outcome is identical either way"* — has no next checkpoint there, and the **tournament result itself is never checkpointed**. Fix, small: checkpoint `8` after `HAND_COMPLETE` on any hand that placed no other checkpoint. It opens nothing and converts K-1 from silent to a `cause = 4` faulted table. **K-3b, same class:** `HandComplete` is zero-width (T47 *"derived, immediate, no external input"*, §8.3 *"the protocol does not wait for `hand_delay_ms`"*), and it is one of only two phases that can accept a `PLAYER_SIT_IN` — the other being `Paused`, which needs every seat silent. So D-013's *"it rejoins by signing … that is the entire test"* has no window in which the test can be taken. | `PROTOCOL.md` §6.2; `STATE_MACHINE.md` §5.2 T47/T59, §5.3 step 9, §8.3 |
| K-4 | **`THREAT_MODEL.md` §5.4 still carries D-012's superseded cost model, which D-013 exists to correct.** It reads *"nothing marks a seat absent automatically any more, so a silent seat is dealt in every hand and stalls each one to the hand deadline **until a human acts**"* — three things now wrong: the cost is one or two stalls then normal play, no human acts or can, and it cites `Q7`, which `STATE_MACHINE.md` closed by dissolving it. Also §7.3(b)'s *"On reconnect they rejoin the key-holder set at the next hand boundary"*: nothing rejoins automatically under D-013. The document carries **one** occurrence of `D-013`. | `THREAT_MODEL.md` — medium |
| K-5 | **DONE in this pass.** `PROTOCOL.md` §3.1 replaces `GENESIS(0)`'s slot-4 `table_public_key` with `ZERO32`. The sentinel rather than a deletion, so both genesis forms keep **six parts in the same slots** under one domain string: `h` (§2.8) carries no arity, and two part-lists of different lengths under one domain are two preimages a future editor would have to reason about, for a part that binds nothing. `ZERO32` in slot 4 reads as *no session yet* beside slot 6's *no previous terminal*, and `session_id` cannot stand there because §4.3 derives it from the `TABLE_READY` events of the chain `GENESIS(0)` opens. §3.1's exclusion argument for `protocol_version` and `table_id` is unaffected: slots 1 and 2 carry them, once each. | — |
| K-5 (cont.) | **Original text.** **`GENESIS(0)` hashes the same 32 bytes as two separate parts.** §4.1 is normative that `table_id` **is** `table_public_key`, and both are in §3.1's part list. Harmless, but an implementer will pass two variables and eventually pass two different ones, and §3.1's stated reason for excluding `table_id` from `table_params_hash` — *"`GENESIS(0)` already carries both as separate parts"* — rests on the duplicate. Delete the part or state the redundancy as deliberate; pre-release wire revision on the same footing as `cause = 5` and the `absent` vector. | — |
| K-6 | **DECIDED, and the remaining edit is `CRYPTOGRAPHY.md`'s.** `PROTOCOL.md` §4.4 is the **normative owner** of `commitment_i` and of the `seed` combine, and now says so and says why: these are wire values — the payloads of `0x0301` and `0x0302`, with the receiver check written against them in that section — and §6.4's `ctx` blocks were deleted under the same rule in the H7 sweep. **Owed by `CRYPTOGRAPHY.md` §7.3:** delete the two code blocks and the sentence *"was corrected to match this section"*, keep points 1–5, the recorded `SPEC_CS.md` §16 deviation and the D-012 check, and reference `PROTOCOL.md` §4.4 by number (D-011 rule 1). Recorded here rather than made there, per D-013's process rule. | `CRYPTOGRAPHY.md` §7.3 — low |
| K-6 (cont.) | **Original text.** **Two documents each claim to be the canonical source of `commitment_i` and `seed`.** `PROTOCOL.md` §4.4: *"This is the canonical form for the whole corpus (C-2)."* `CRYPTOGRAPHY.md` §7.3, reproducing both blocks: *"This five-part binding is the canonical one; the two-part form that `PROTOCOL.md` §4.4 previously carried … **was corrected to match this section**."* Byte-identical today, which is exactly the condition D-011 rule 1 was adopted for. §6.4's `ctx` blocks were deleted under that rule in the H7 sweep; §7.3's were not. Fix: `PROTOCOL.md` owns the construction, `CRYPTOGRAPHY.md` §7.3 keeps points 1–5 and loses the two code blocks. | — |
| K-7 | **The one status-defined gate on table progression left.** `STATE_MACHINE.md` T59's exit from `Paused` is guarded on *"at least two seats are again **willing** and have chips"*, where *willing* is `status == Active ∧ stack > 0`, while §5.3 step 4 — running in the same transition's side effects — decides who is dealt in on participation. One seat signs `PLAYER_SIT_IN`, three silent `Active` seats pass the guard, step 4 yields `\|dealt_in\| == 1` and a drain hand. Not an `R`, so J2 does not reopen; but it is the last status word on a progress path after D-013's sweep. Restate on `\|{s : s ∈ signed_this_hand ∧ stack[s] > 0}\| ≥ 2`, or say the disagreement is deliberate. | `STATE_MACHINE.md` §5.2 T59 — low |
| K-8 | **New, opened by K-2's fix. When is a departing seat's stack subtracted?** `PROTOCOL.md` §3.1 now freezes the roster's seat vector, so `roster_hash(k)`'s stack vector is `TERMINAL(k-1)`'s `n(5) final_stacks` **and nothing else**, and a `PLAYER_LEAVE` moves chips only through `HAND_INIT`'s `n(11) ledger_delta` into `ledger_out`. What is not stated anywhere is which terminal first shows the departed seat at zero: `HAND_ABORT`'s `n(5) final_stacks` is *the start-of-hand stacks, always* (D-010), so it cannot be that one, and `HAND_COMPLETE` recomputes from the engine. Until it is stated, C-9's ledger identity `Σ stack + Σ committed == ledger_in − ledger_out` is checkable only if two implementations happen to pick the same hand. §3.1 deliberately declines to answer it, because answering it wrongly is how a redundant hash becomes a fork. **Small, and it is a `STATE_MACHINE.md` derivation with a `PROTOCOL.md` receiver check.** | `STATE_MACHINE.md`, `PROTOCOL.md` §4.10 — medium |
| K-9 (as it stood) | **Original text. Owed by K-1's fix. The engine half of the solitary regime.** `PROTOCOL.md` §3.2 now says a peer whose required emitter set is `{self}` alone freezes on a contradicting chained event rather than completing the stage. `STATE_MACHINE.md` §5.3 step 9 currently routes `\|dealt_in\| == 1` **straight to `Settling`**, with no phase in which that freeze can happen and no transition that consumes it — the same gap `K-3b` names for `PLAYER_SIT_IN`. Also owed there, in one sentence each: `signed_this_hand` **includes the peer's own emissions** (K-1's sub-question, now answered on the wire side), and `PLAYER_LEAVE` **does not** enter it. | `STATE_MACHINE.md` §5.3 steps 4 and 9, I31 |
| C-1..3 | **Not specification defects; blocking for `src/protocol/`.** (1) `src/protocol/signatures.rs`'s `Domain` register matches `PROTOCOL.md` §2.8 in **no** string — `stage-hash`/`state-hash`/`table-parameters`/`deck-proof` against §2.8's `stage`/`state`/`table-params`/`deck-ctx`, two variants not in the register, twelve register entries with no variant, and `Domain::Event`'s string is one §2.8 lists as **retired, never valid**. (2) `serialization::hash_domain` is not §2.8's `h`: `new_derive_key(ctx)` instead of `new_keyed(derive_key(domain, b"p2p-poker/v1"))`, and single-part where every construction in the corpus is multi-part with a length prefix **per part**. (3) `signatures::sign`/`verify` sign a 32-byte digest, not §2.4's `TO_BE_SIGNED = DOMAIN_EVENT ‖ u32_be(len) ‖ body_bytes`. Each would surface as a cross-implementation failure, never as a test failure, because the current tests assert self-consistency. | `src/protocol/` |
| J-3 | **DONE (Phase 3 gate).** `STATE_MACHINE.md` §2.4 took the third option — keep the variant, declare it *"unreachable in this version, deliberately"*, and enumerate every reader as a labelled trap, with the warning an implementer needs: *"An implementer who finds a status the engine never sets and wires it to a local liveness signal … has forked the chain at every checkpoint."* Two of the five readers it lists have since been deleted by `PROTOCOL.md` §6.1 and §4.4, so the list over-warns by two entries; one editorial line at the next touch. Original text: **`SeatStatus::Absent` is now unreachable *and* unhashed.** `PROTOCOL.md` §6.1 has deleted the `absent` vector from `PublicTableState`, so it no longer enters `state_hash`, and under D-013 no rule in the corpus needs to know whether a seat is away. The variant and its remaining readers are `STATE_MACHINE.md`'s: §5.3 step 4's *"`SittingOut`, `Absent` and `Busted` seats get `dealt_in = false`"*, steps 6–7's ante and blind posting for `Absent` seats, and T59's guard `status ∈ {Active, SittingOut, Absent}`. Retire the variant with its name recorded, as the corpus does for T8/T12/T55/T56, or state precisely what still sets it. | `STATE_MACHINE.md` |
| J-4 | **STILL OPEN after the Phase 3 gate — not acted on, and stale by content as well as by disposition.** §0.5.6's two proposed remedies (*"the founder's `PLAYER_LIST` value made normative"*, or *"`advert_hash` dropped … in favour of `table_public_key`"*) are **neither of them the fix that was adopted**, so a reader sent there gets a live blocker notice, a wrong problem statement and two wrong remedies. Five further sites move with it: §0.5.2's row, l. 1496, l. 1529, l. 2220 and the **D-012 coverage row at l. 2668**, the document's own compliance record. `NETWORK_STACK.md` carries **zero** occurrences of `D-013`. Original text: **§0.5.6 and its D-012 coverage row still report J1 as an open survivor** — *"One site is reported and not fixed: `advert_hash` reaches `GENESIS(0)` and `session_id` … and the fix belongs to `PROTOCOL.md` §3.1 and §4.3"*. It is fixed. §0.5.6 should record the disposition and point at `PROTOCOL.md` §3.1's `table_params_hash` box, and §0.5.2's merged-lobby-view row should lose the sentence that names §0.5.6 as not yet safe. This is the second half of the hand-off D-013's process finding is about: the finding document must be told when the owner acted. | `NETWORK_STACK.md` |
| J-5 | **STILL OPEN after the Phase 3 gate.** Discipline item 1 is now true, and was false when written; nothing is owed on the wire, only the record, so a future editor does not read a once-wrong claim as evidence the check was once run. `CRYPTOGRAPHY.md` carries **zero** occurrences of `D-013` and was not opened in this pass — the same omission that produced D-012's process rule, in the same document, one decision later. See also K-6. Original text: **The `ctx` construction is now clean and `CRYPTOGRAPHY.md` should say so.** Its discipline item 1 states the condition for the Fiat–Shamir binding — no field that could differ between honest receivers — without knowing the condition was violated transitively through `session_id`. With J1 fixed, every part of `ctx` is a constant or accepted chained content. Also: `PROTOCOL.md` §4.5's stale claim that `CRYPTOGRAPHY.md` §6.4 *"reproduces this block"* is corrected on `PROTOCOL.md`'s side (J6); nothing is owed back. | `CRYPTOGRAPHY.md` — low |
| OQ-A | A named, versioned reference engine, since several sections claim disputes are "deterministically adjudicable by any third party running the reference engine" and no such engine is defined (review N2). Interim answer: withdraw the claim; the engine is defined when the crate has a tagged release. | `PROTOCOL.md` |
| OQ-D | A dispute path that does not require the accused peer's signature — circular at every table size, not only heads-up (D-007 point 4, review A-1). Interim answer under D-010: a dispute that cannot resolve ends the hand neutrally, so the circularity costs a hand rather than a stalemate. | `PROTOCOL.md` |
| OQ-F | Whether `TIMEOUT_VOTE`, `TIMEOUT_CERT` and `EquivocationProof` should still be *produced* in the MVP now that D-010 gives them no effect, or be deferred wholesale until the machinery is sound. Producing them keeps the transcript adjudicable later; deferring them removes four passes' worth of surface. | `PROTOCOL.md`, `STATE_MACHINE.md` |
| — | Drop the libp2p `dns` and `kad` features and run all discovery through Mainline DHT, including relay volunteers under a second infohash? Removes both hickory advisories and ~12 crates from the build; costs access to the public relay commons, which thins D-004's floor. See `research/INTEGRATION.md` section 3. | Nothing yet |
| N-1e | **DONE — `STATE_MACHINE.md` §2.6 landed the repair, and the form it took is not the form the text below proposed.** §2.6 now reads **`solitary_at(k) := solitary_since == Some(j) ∧ j <= k + 1 ∧ k <= hand_id`**, with the lemma restated over both disjuncts of `PROTOCOL.md` §3.2's regime test and the second-disjunct case proved explicitly (a hand `k` with `P(k) == {self}` is hand `k+1`'s required set, so hand `k+1`'s init reads a one-member `signed_this_hand` and `j <= k + 1` always). §10 row 20 and §2.6's own list carry it, T62 reads it, and `I33(a)` asserts the ordering. **The slack is written on `j`, not on `k`:** the row below proposed *`j - 1 <= k <= hand_id`*, which is the same set of hands and the wrong place to put the inequality, because the floor is a claim about `solitary_since` and an editor re-deriving it from a bound on `k` re-derives it wrongly. **Anyone reading this row for the predicate must take the `j <= k + 1` form**; the paragraph below is kept for the derivation only. Original text follows. **`PROTOCOL.md` and `STATE_MACHINE.md` both landed `N1` in this pass and they agree; what is left is one conjunct in the engine's floor, and it is provable.** Wire half: §4.9's reconciliation stage is required of `R(c) ∪ W`, never fewer than two seats, and §6.3 step 3 states exhaustively that nothing else releases the freeze or the latch — so **T53's new two-signer conjunct and the wire's floor are the same rule stated twice**, which is belt and braces rather than a conflict, and §6.3 step 3 is the normative owner (D-011 rule 1). Engine half: T50 now latches when solitary and T62 reads `solitary_at`. **The gap is `solitary_at`'s lemma.** §2.6 proves *if the wire's record says hand `k` was solitary then `solitary_at(k)` holds*, from *the record says so exactly when §5.3 step 4 read a one-member `signed_this_hand` at hand `k`'s init*. `PROTOCOL.md` §3.2 now writes the regime test as the **disjunction** `P(k-1) == {self} ∨ P(k) == {self}`, and the second disjunct is the one K1's own walk lands on first — the hand `P` narrows in has three seats in `P(k-1)` and one in `P(k)`, and it is that hand's checkpoint 8 that mismatches. For it, `solitary_since` is written at hand `k+1`'s init, so `j = k+1` and `j <= k` fails: **the lemma is false for exactly the hand the fix exists for.** The repair is one character of slack and it is exact, not conservative: a hand `k` satisfying the second disjunct has `P(k) == {self}`, which is hand `k+1`'s required set, so hand `k+1`'s init reads a one-member set and `j <= k+1` always — hence **`solitary_at(k) := solitary_since == Some(j) ∧ j - 1 <= k <= hand_id`**, with one hand of slack and no more. Also owed, editorially: §10 row 20's *"where it was T50 the table plays on, as before"* is now false for the checkpoint-8-when-solitary route. | `STATE_MACHINE.md` §2.6, §10 — **blocking for K-9** |
| N-4 | **Two representations of one past-tense predicate. `STATE_MACHINE.md` §2.6 answered it correctly with a monotone floor and an ordering lemma; `PROTOCOL.md` §3.2 then sharpened the predicate under it, and the lemma needs the one conjunct `N-1e` names.** The shape of the answer is right and is worth recording as the pattern for this class: **the wire's record is exact, the engine's field is a floor beneath it, and the invariant is an ordering between the two rather than an equality** — which is how a rule split across two owners can keep two structures without either being wrong, provided the direction of the inequality is stated and asserted (I33(a)). What is left is the second disjunct of §3.2's regime test, `P(k) == {self}`, which the floor's write site does not see; `N-1e` gives the exact repair and the proof that one hand of slack suffices. **The residual after that is nil** — the clearing question that opened this item is closed by the field never being cleared. **DONE, and one quotation in this row was wrong and is corrected here.** The resolution this row used to state was *"`solitary_at(k) := solitary_since == Some(j) ∧ j <= k <= hand_id`, read by T62 and T63 only"*. **`j <= k` is the superseded predicate** — it is precisely the conjunct `N-1e` proved false for the hand the fix exists for, and an editor following the authority order reads `DECISIONS.md` before `STATE_MACHINE.md` and would have re-derived it. **The predicate in force is `solitary_at(k) := solitary_since == Some(j) ∧ j <= k + 1 ∧ k <= hand_id`** (`STATE_MACHINE.md` §2.6, which is its normative owner under D-011 rule 1; this row states it once and adds nothing). Everything else in this row stands: the pattern — *the wire's record is exact, the engine's field is a floor beneath it, and the invariant is an ordering between the two* — is right and is the reason the split survived a sharpening on the wire side without either half being wrong. | `STATE_MACHINE.md` §2.6 — **closed with `N-1e`** |
| N-5e | **SUPERSEDED by `P2-e` in this pass — read that row, not this one. The union below was deleted: `R(HAND_INIT, k)` stays `P(k-1)` and `A` widens only the *accepted* emitter set, because `A` is written from a stale event for which §4.0 step 10a is skipped, so a replayed agreeing copy re-enlarged a required set once per hand. Original text follows. Owed to `STATE_MACHINE.md` by `PROTOCOL.md`'s `N5` fix, and it is two lines.** §4.9 now defines a **readmission set `A`**: a stale `0x0804 PLAYER_SIT_IN`, or a stale checkpoint-8 `STATE_HASH` that **agrees** with this receiver's value, carries its sender into the next hand init instead of being dropped, because at a peer in the solitary regime every window is zero-width and D-013's readmission promise was otherwise void exactly where D-013 made it a steady state. §4.4 states the consequence: `R(HAND_INIT, k) = P(k-1) ∪ A`, with `A` read once and cleared there. The engine's hand init (§5.3 step 4) must read the same union into `signed_this_hand`'s successor and `dealt_in`, and `A` must be cleared in the same step. **This is also the disposition of `K-3b`**, whose *"a seat returning from silence needs no `PLAYER_SIT_IN` at all"* was true only inside a window that closes on an event emitted concurrently with it. | `STATE_MACHINE.md` §5.3 step 4 — medium — **superseded, see `P2-e`** |
| N-8 | **DONE in `DECISIONS.md` in this pass; what remains is the owner's edit, and the row now says the same thing everywhere it appears.** D-014 cited *"`SPEC_CS.md` §22's information window"*; §22 is the GUI section, its tree ends at `timer` and `protocol/security status`, and no anti-cheat window is in it. **The disposition is a required addition to §22, never a citation of an existing element** — the alternative this row used to offer, *"cite §22's status element"*, is **withdrawn**: the status element is a different element serving a different purpose, and citing it would have replaced a citation that outran its source with one that merely missed. D-014's *What the players see* now states the requirement rather than the citation. `STATE_MACHINE.md` T64's cell and `THREAT_MODEL.md` §9.2 already read this way, so all four sites agree. **What is still owed is `SPEC_CS.md` §22 itself**, and nothing in the corpus may cite the window as present until the owner adds it: the removed player, the tier, what the evidence was, and that the hand was voided and no chips changed hands. | `SPEC_CS.md` §22 — **owner's edit**; the corpus's four records are consistent and closed |
| N-9 | **CLOSED. Both halves are done, and this row was stale in the pass that said so about its own twin.** `CRYPTOGRAPHY.md` half: **done** — the row `N9` above records §8.1 in full, and the count is no longer zero. `NETWORK_STACK.md` half: **done in this pass, and the aside below was wrong.** This row recorded that document's zero as *"defensible and unrecorded (D-014 has no transport consequence; §0.1's forbidden row is unaffected)"*. **§0.1 is precisely the row that was affected**, because it stated D-010 point 3 in the every-layer form D-014 narrows, which is not an omission but a contradiction; that is what `G4-P5` filed and what `P5`'s disposition now closes. **The lesson the duplication carries is the reason this row is not simply deleted:** `N9` and `N-9` are two identifiers for one defect in one file, the pass that rewrote `N9` to say *"the open list's `N-9` is stale in saying otherwise"* did not then edit `N-9`, and a reader arriving at the hyphenated row would have re-opened a closed item. That is the namespace collision `G5-Q5` names, in its second form — bare versus hyphenated — and the same pairing exists for `N1`/`N-1e`, `N4`/`N-4`, `N8`/`N-8` and `P4`/`P-4`. `G5-Q5`'s row below states the convention that ends it. Original text follows. **`CRYPTOGRAPHY.md` carries zero D-014 while owning three of tier 1's six clauses.** Tier 1 names a failed shuffle proof, a failed decryption-share proof and a failed key-ownership proof; §8's verification rules and §6.5's target table are where those verifiers live, and `THREAT_MODEL.md` X37 names *"a proof verifier stricter than the prover"* as the mechanism that turns an honest message into tier-1 evidence with no checkpoint to wait for. `PROTOCOL.md` §4.0's anti-eviction box now states the exception and names those verifiers by step, which makes the omission louder rather than quieter. `NETWORK_STACK.md`'s zero is defensible and unrecorded (D-014 has no transport consequence; §0.1's forbidden row is unaffected) — one row each. | `CRYPTOGRAPHY.md` §8, §0; `NETWORK_STACK.md` §0.1 — **closed, both halves** |
| G4-P5 (was `P5`) | **DONE in this pass, four passes after it was first named, and the delay is half the finding.** `NETWORK_STACK.md` now carries D-014, and the edit is the re-scoping this row specified, not a weakening: **no transport behaviour changed.** (1) §0.1's box says *"No automated eviction **at this layer**, ever"* and names the exception without restating it — a tier-1 finding removes its sender from the **table** and produces no transport action of any kind. (2) §1.2 prohibition 7 gains *at this layer* and the sentence that D-014 *"does not amend this prohibition and cannot reach it"*, with *an invalid application signature* called out as a tier-1 trigger above this layer and never an input here. (3) §11.5.1's box is **unchanged**, and gains the paragraph an implementer arriving from `STATE_MACHINE.md` T64 needs: arriving at T64 is **not** licence to call `block_peer`. Plus **new §0.6**, the layer's D-014 pass — what changed (a scope word, tabulated site by site), the exception stated exactly, the two properties that make it safe (no quorum/vote/timing; no receiver state), **§0.6.3's table of every judgement this layer can cheaply make and why each still removes nobody**, the separation of §0.2's kept defences from any of it, and §0.6.4 on D-013 (participation is inherited, never sensed: a silent socket is not a removal and a reconnected one is not a re-entry). §0's heading and preamble become *"D-009 to D-014"* and *"four times"*; §0.2's forbidden row and its corollaries name the boundary; §15 gains a D-014 register row. **The other half of the finding is the process one:** three passes named this file with line numbers and none opened it, and the reason it survived is that a **contradiction reads as an omission** in a count — zero occurrences of `D-014` looks like *behind the sweep* and was in fact *asserting the negation*. `CONTRIBUTING.md` §2.5's re-grading rule is the mechanism that should have caught it. Original text follows. **`NETWORK_STACK.md` forbids what D-014 tier 1 now permits, it carries zero occurrences of `D-014`, and it was the only file the last pass did not open. Three line numbers, and the exception it needs is one clause wide.** (1) **line 97, §0.1 *"The rule, stated once"*** — *"No automated eviction, anywhere: no `block_peer`, no unseating, and no allow-list or block-list populated by the poker protocol"*, explicitly *"as D-011 rule 3 extends it to every layer"*. (2) **line 541, §1.2 prohibition 7** — *"remove, block, unseat, refuse or penalise a peer on the strength of a protocol proof"*, whose named triggers include *"an invalid application signature"*, which is a **tier-1 trigger by name**. (3) **line 2536, §11.5.1's box** — *"no unseating"* among the transport actions no `EquivocationProof`, `HAND_ABORT` attribution or *invalid application signature* may produce. Lines 135, 1768 and 2544 point back at prohibition 7 and follow whatever it says. **The narrow exception, exactly, and it is a re-scoping rather than a weakening.** Sites (1) and (2) state a **corpus-wide, every-layer** rule; D-014 narrowed D-010 point 3 for **one** layer and one input. Both should say what `PROTOCOL.md` §4.0's box and `DEPENDENCIES.md` §5.5 already say — that the transport prohibition is **absolute and unamended**, and that the one exception lives above it: a **tier-1** finding, an event *signed by the accused* whose illegality any peer decides from that event's own bytes, **removes its sender from the table** and never from the transport. Site (3) needs the opposite edit and it is the more useful one: §11.5.1 is genuinely transport-only and stays as written, plus one sentence saying that a D-014 tier-1 removal is a **table** disposition (`STATE_MACHINE.md` T64/T65) and produces **no** transport action — so an implementer who reaches T64 does not read it as licence to call `block_peer`. **This also corrects `N-9`'s aside**, which recorded `NETWORK_STACK.md`'s zero as *"defensible and unrecorded … §0.1's forbidden row is unaffected"*: §0.1 is precisely the row that is affected, because it is the one stated in the every-layer form. **And it is the whole of `D-014-1`'s remainder** — the other four specification documents have landed D-014 (see that row). | `NETWORK_STACK.md` §0.1, §1.2 prohibition 7, §11.5.1 — **medium, and it is a document asserting the negation of a binding decision** |
| G4-P2-e (was `P2-e`) | **Owed to `STATE_MACHINE.md` by `PROTOCOL.md`'s `P2` fix, and it supersedes `N-5e` in one clause.** `N-5e` asked the engine to read `R(HAND_INIT, k) = P(k-1) ∪ A` into `signed_this_hand`'s successor and into `dealt_in`. **`P2` deletes that union from the required set**: §4.9's readmission set `A` now widens the hand-init stage's **accepted** emitter set only, and `R` stays `P(k-1)`. The engine half is therefore *smaller* than `N-5e` described: §5.3 step 4 keeps `dealt_in ⊆ P(k-1)` unchanged and adds nothing, and what it must instead do is **accept** a `HAND_INIT` copy from a seat in `A` — counting it into `signed_this_hand` for chain `k`, so the returning seat is a required emitter at hand `k+1` — without letting it satisfy the stage's completion test, which stays `heard ⊇ P(k-1)`. `A` is still cleared at that step. **Why it matters that the engine takes the same half:** an engine that keeps the union enlarges a required set from a replayable input, which is the defect exactly, and the two documents would then disagree about who must speak before stage 0 completes — the disagreement that stalls stage 0 is supposed to be *between peers*, not between the two halves of one peer. `PROTOCOL.md` §4.9 and §4.4 carry the reasoning and are not restated there (D-011 rule 1). | `STATE_MACHINE.md` §5.3 step 4, §5.2 — **medium; it replaces `N-5e`, which should be read as superseded and not as a second instruction** |
| G4-P3-e (was `P3-e`) | **Owed to `STATE_MACHINE.md` and `THREAT_MODEL.md` by `PROTOCOL.md`'s `P3` fix, and one line of it is a config-validation rule that is wrong today.** §9.4's config validator carries `hand_deadline_ms >= 10 * action_timeout_ms`, which at the preset is `200 000` — a **twelfth** of the floor at ten seats and below it at every seat count, so the check passes exactly the configurations `P3` shows are unplayable. It must become `hand_deadline_ms >= HAND_DEADLINE_FLOOR(seats)` with the formula referenced, never restated (D-011 rule 2); the derivation and the joiner's advert check are `PROTOCOL.md` §8.2 and §9.4 rule 2a. Second, the preset figure moved — `RATED_SNG_POKERTH_V1`'s `hand_deadline_ms` is **3 300 000**, not 600 000 — and `STATE_MACHINE.md` states *"600 000 ms"*, *"ten minutes"* or *"about five minutes"* as a cost in roughly a dozen places (§2.6, §5.2 T61, §8.6, §9.3, §9.4, §12.1); none is a correctness claim, all are now wrong, and §12.1's residual bounds in particular are quoted to users. Third, `THREAT_MODEL.md` has no entry for **a founder advertising a deadline every legal hand exceeds** — free, keyless, and it reaches the abort path that attributes nobody; `PROTOCOL.md` §11 now carries the row and the catalogue should carry the attacker.<br><br>**Status added by `G7-S3`'s pass, appended rather than rewritten (`G6-R7`'s convention).** The third point — the `THREAT_MODEL.md` row for the below-floor founder — is **still open**, and `PROTOCOL.md` §11 now cites **this row by identifier** rather than saying *"filed in `DECISIONS.md`'s open list"*, which is `G6-R2-m`'s rule applied to the one claim in `PROTOCOL.md` that did not name its row. §11's cell is also sharpened to what the catalogue entry has to say: the capability required is **none** — `hand_deadline_ms` is a signed parameter the founder simply chooses — and there is **no second line of defence**, because once a seat is taken the value is inside `table_params_hash` and every peer holds it, so §7.2 rule 2a is the whole of the mitigation and it is spent before the seat exists. The right-hand side of the validator line moved again since this row was written: it is `HAND_DEADLINE_MIN`, not `HAND_DEADLINE_FLOOR` (`G5-Q6`). And `THREAT_MODEL.md`'s **own** six *"ten minutes"* sites are `G7-S7`, a separate row, since this one names only `STATE_MACHINE.md`'s. | `STATE_MACHINE.md` §9.4 (the validator line), §2.6/§5.2/§8.6/§9.3/§12.1 (the figure); `THREAT_MODEL.md` §5.2 — **medium; the validator line is the part that is actively wrong rather than merely stale** |
| G4-P3-r (was `P3-r`) | **The residual `P3` does not close, stated because a bounded residual left unnamed is read next pass as a closed one.** `HAND_DEADLINE_FLOOR(n)` budgets the no-re-raise walk. A raise that reopens the action entitles up to `n − 1` further actions, and the headroom a table advertises above its floor buys `REOPENINGS = ⌊(hand_deadline_ms − FLOOR(n)) ÷ ((n − 1)(action_timeout_ms + action_grace_ms))⌋` of them — **four** at the new preset at ten seats. A hand with more still aborts on a legal path with `cause = 1` and `attributed = []`, which is `P3`'s own shape at a rarer size. **Two closures exist and neither was taken here.** (a) **A cap on reopening raises per betting round.** It bounds the hand exactly and needs no new wire quantity, but it is a change to the rules of the game — `research/POKER_RULES.md`'s and the project owner's, not this corpus's — and a hard cap is a real NLHE deviation. (b) **A deadline that extends deterministically on accepted chain content**, by `(n − 1)(action_timeout_ms + action_grace_ms)` per accepted reopening `ACTION_RAISE` of chain `k`. It is exactly computable from the same accepted prefix at every peer, so it is not a per-receiver quantity and D-012 does not reach it — but it makes the whole-hand deadline a function of the chain rather than a constant, which is a new mechanism on the one timer §8.2 anchors §4.10's abort buffering to, and adding a mechanism to a timer that already carries R-1's anchoring argument is the kind of edit that should be taken deliberately and not as a corollary. **Until one is chosen, the floor plus the founder's headroom is the whole of the answer, and the bound is four reopenings at the preset.** | `research/POKER_RULES.md` / project owner for (a); `PROTOCOL.md` §8.2 for (b) — **low frequency, same failure mode as `P3`** |
| G4-P8-a (was `P8-a`) | **`PROTOCOL.md` §4.0's anti-eviction box accused two compliant sections and the accusation is deleted in this pass; recorded because the next reader would have re-fixed something already fixed.** The box said *"`NETWORK_STACK.md` §6.6 and §11.5 assert the opposite today and are wrong"*. Both had been corrected before this pass: §6.6 now states *"a proven protocol violation produces no transport action at all"* and §11.5.1 forbids the `block_peer` path in a box of its own. **The class is `N8`'s, pointing the other way** — `N8` is a citation that outran its source, this is an accusation that outlived its cause — and both are the same failure to re-read a cross-document claim after the other document moved. No further edit is owed; `P5` is what `NETWORK_STACK.md` still owes and it is a different clause. | `PROTOCOL.md` §4.0 — **closed in this pass** |
| P7-w | **Owed to `PROTOCOL.md` by `P7`, and it is the half `STATE_MACHINE.md` cannot take. `PublicTableState` is `#[cbor(array)]`, so field order **is** the encoding, and `PROTOCOL.md` gives it twice and differently: §6.1's table places the four `Vec<bool>` flag vectors between `committed_this_hand` and `current_bet` and appends `signed_this_hand` last, while §2.9's D-012 sweep enumeration (l. 795) lists `signed_this_hand` before *"the `Vec<bool>` flag vectors"*. Two transcriptions in different orders are two different `state_hash` values for one state, and every checkpoint in the corpus then reports a divergence between two peers that agree about the game. **`STATE_MACHINE.md` §3.3 has named §6.1's table normative for the engine and adds no fourth listing**, which settles what an implementer reads; what was still owed is that `PROTOCOL.md` **say which of its own two lists is normative in one place and delete the other**, per D-011 rule 1. §2.9's is a list of what was swept and was the one to go. **This row also corrects a filing claim**: §3.3 said the defect *"is recorded on `DECISIONS.md`'s open list in this pass"* and it was not; it is recorded now. **DONE in this pass, and by deletion rather than by alignment.** §2.9's enumeration 3 no longer lists any field — it reads *every field of §6.1's table*, states that a sweep enumerates verdicts and never order, and says why a further listing is a defect on sight. §6.1 gained a normative box above its table: the table **is** the field order, `#[cbor(array)]` means order is the encoding, it is the only field-order statement this document makes, §3.3 adopts it for the engine, and a transcriber reads it and nothing else. Two listings became one, so the class cannot recur by drift. | `PROTOCOL.md` §2.9 and §6.1 — **closed**; `src/protocol/messages.rs` transcribes §6.1's table |
| P2-e | **DONE in this pass.** `STATE_MACHINE.md` §5.3 step 4 derives `dealt_in` from `signed_this_hand` alone and §5.3 step 8 reads `\|signed_this_hand\| == 1` for `solitary_since`; the union `admitted := signed_this_hand ∪ readmit` is deleted from both. `readmit` and **T67** are kept and reach **no guard**: the set is handed to the protocol layer at hand init as the accepted-emitter widening for that one stage and cleared at step 8. T67 is kept rather than deleted for one reason only, and it is the one thing the wire cannot do — its `status ∉ {Removed, Empty}` conjunct is what keeps §4.9's `A` from being the re-entry route D-014's one-way exit forbids (I34(a)). §2.6, §2.8, §4.1, §5.2's solitary box, I31(b), I33(a) and §12.1 follow. | `STATE_MACHINE.md` — **closed** |
| Q4-e | **DONE in this pass.** `CheckpointState` carried `values: u8` and no `state_hash`, so T49's *"equals this peer's own derivation"*, T50's *"two distinct values now exist"* and T51's *"every value agrees"* had no term in the record to read — uncomputable on the `agreed` and `boundary` slots `P1` added for them, and, in T50's case, uncomputable everywhere, since a count of distinct values cannot be maintained without the values. `values` is deleted; `own: Hash` and `dissent: Option<Hash>` replace it. T49 is `e.state_hash == c.own`, T50 is `e.state_hash != c.own` with `c.dissent := c.dissent.or(Some(e.state_hash))`, T51 is `c.dissent.is_none()`. Bound unchanged in shape — three fixed records, 65 bytes each more than before, nothing keyed on a sender-chosen quantity. | `STATE_MACHINE.md` §2.6, §5.2 — **closed** |
| G5-Q5 | **DONE in this pass, as a convention plus a rename, and the convention is stated above the table because it changes how the table is read.** Two live series were both using bare `P1 … P8`, colliding on **`P3`, `P5` and `P8`** in the same files — `PROTOCOL.md` carries two different `P3`s (l. 83, the witness-independent terminal stage; l. 5833, the hand-deadline floor), `STATE_MACHINE.md` T50 cites an older `P5` and §2.3 an older `P8`. **The fourth Phase 3 gate's series is the one renamed**, to `G4-P1 … G4-P8`, because the original series is load-bearing inside `STATE_MACHINE.md`'s normative text and inside archived gate reports that are records and are not edited; renaming those would falsify the trail. The fifth gate's findings take `G5-Q1 … G5-Q6` from the start, since bare `Q1 … Q6` collide with `STATE_MACHINE.md`'s own open **questions**. Six rows in this list carry the prefixed form with the old label kept in the cell so a search for the remembered string still lands. **A second form of the same collision is named in the same place:** `N1`/`N-1e`, `N4`/`N-4`, `N8`/`N-8`, `N9`/`N-9` and `P4`/`P-4` are five pairs of identifiers for one defect each, and in three of the five a pass updated one row and left its twin asserting the opposite — **a hyphen is not a namespace**, and no row may be re-opened under a punctuation variant of an existing label. | `DECISIONS.md` — **closed**; the residual sweep is `G5-Q5-x` |
| G5-Q5-x | **Owed to `PROTOCOL.md` by `G5-Q5`, and it is a citation sweep rather than a defect.** The bare `P2` and `P3` citations inside `PROTOCOL.md` that belong to the **fourth gate** — the deadline-floor sites at §8.2 (*"this is `P3`"*), §9.4 rule 2a, §13's `n(17)` row, §12's `Q-10` neighbourhood, §4.0 step 10b's `P2` — now read as the **original** `P3`/`P2` under the convention above, which is the wrong item: the original `P3` is the witness-independent terminal stage, cited eight times in the same file. Each should become `G4-P3` / `G4-P2`. Not edited from here (D-011 rule 1); filed in the pass that made the convention (D-013's process rule). Same sweep, smaller: `STATE_MACHINE.md`'s `P2-e`/`P3-e`/`P7`/`P7-w` citations, which are the fourth gate's. **Until the sweep runs, the disambiguator is the subject, not the label**: a `P3` about a *deadline* is `G4-P3`, a `P3` about a *terminal stage* is the original. | `PROTOCOL.md` §8.2, §9.4, §13, §4.0; `STATE_MACHINE.md` — low, and it is a correctness question only for an editor |
| ZR-1 | **The `ziffle` review has reported and the decision it forces is: fork the vendored crate now, or ship upstream-identical bytes?** `docs/research/ZIFFLE_VERDICT.md` closes `CRYPTOGRAPHY.md` **OQ-2** (the forked Fiat-Shamir transcript is sound - two reviewers, one by argument, one by executing a cross-statement graft that was rejected) and **OQ-5** (the `Transcript` assert is unreachable by an adversary, argued and measured), and it discharges **OQ-1** substantially but not wholly: the algebra is right line by line, twenty-two attack families against an independent forger produced no forgery, and an exhaustive 2^16 hybrid of the two strongest strategies was rejected - but **nobody proved witness-extended emulation and nobody re-derived the soundness bound at `m = 1`**, which OQ-1 asked for by name. The verdict is **fit for the play-money MVP only with conditions C-1 ... C-15**, and **not fit for real money** on a separate and much higher bar (§6 there). **What the owner has to decide, and why it cannot wait.** Two defects are wire-format changes: the transcript frames its lengths with `usize::to_be_bytes()`, so a 32-bit or wasm client derives *different challenges from the same proof* and the table splits into two mutually unverifiable halves; and `RevealTokenProof` binds a card's `c1` and drops its `c2`, which was demonstrated end to end at N = 52 to let one card's reveal tokens decrypt a different card. Both are two-line fixes in a vendored fork. **The wire format is frozen by the first proof that is persisted or exchanged**, so this is nearly free now and expensive later; the same fork would carry the dead `N > 1` guard, the wrong comment at `lib.rs:1000`, and optionally a joint-state challenge derivation and a protocol tag on `Transcript::init`. **Against forking:** it puts the project on a private wire format no upstream fix will match, ziffle's own 16 unit tests and 15 doctests stop being a check on the code we actually run without re-verification, and it forfeits interoperability with the one other project using the crate. **What does not depend on the answer:** conditions C-0 to C-11, C-13 and C-15 are required either way, and the three-line structural deck check (C-3 - no `c1` is the identity, the `c1` are pairwise distinct, no `c1` is carried over from `prev`, `next != prev`, at 0.019 ms against ~36 ms of proof verification) is required even if the fork happens, because on its own it closes the only three attacks in the review that let a player see a card he should not. **CLOSED 2026-08-29 by D-016: fork.** The reasoning is there and is not repeated here (D-011 rule 1). `src/mental_poker/` is no longer blocked and is written. | `docs/DECISIONS.md` D-016; `vendor/ziffle/PROVENANCE.md` - **closed** |
| ZR-2 | **Three of the verdict's conditions have landed and the rest have not, so the count is written down rather than left to be inferred.** **C-0** (vendor `ziffle` 0.1.0 at `vendor/ziffle/` with `[patch.crates-io]`): done - nine files, sha256 `ba79285…` recomputed locally and agreed with the pre-vendoring `Cargo.lock` and with the verdict, tree byte-identical to the registry checkout, `cargo build` and `cargo test` clean, provenance at `vendor/ziffle/PROVENANCE.md`. **C-7** (freeze `open_deck` and the Pedersen key): done - `tests/deck_constants.rs`, five tests, both D-10 digests reproduce from the vendored tree and one of them is additionally bound to the deck ziffle actually deals. **C-14** (register the fallback): done - `DEPENDENCIES.md` §11. `CRYPTOGRAPHY.md` §9's *"vendored into the repository at `vendor/ziffle/` with a `[patch.crates.io]` entry"* and its §11 checklist item 1 are now statements of fact rather than of intent; that document is not edited from here (D-011 rule 1) because nothing in it is wrong. **Updated 2026-08-29.** C-1 to C-6 and C-8 to C-15 have since landed: the wire boundary with its length gate and canonicality comparison (C-1, C-2), the structural deck check that closes the three attacks letting a player see a card he should not (C-3), the key-set check before the ownership proof (C-4), the `ctx` construction site with its comment (C-5), the chain discipline and the C-11 note where the chain is driven (C-6, C-11), the establish-before-verify rule as a type (C-8), one proof per seat per position (C-9), the soundness fault that is never attributed to a peer (C-10), the fork (C-12, and D-016), the parameters built once per process (C-13), and the attacks as regression tests against the real backend (C-15). What remains of C-14 is the differential test against the registered fallback, which needs the fallback linked in and is not started. | `docs/research/ZIFFLE_VERDICT.md` §5; `src/mental_poker/` - **no longer blocking** |
| ZR-3 | **Two licence-hygiene questions owed to `paritytech/mental-poker`, and nobody is assigned to ask them.** `DEPENDENCIES.md` §11.3 owns the text; the open part is that it is an action with a third party, not a document edit, and its whole value is in being asked while nothing is on fire. (1) `deck/`, `deck-secp256k1/`, `ez/` and `play/` ship no licence text and the repository has no root LICENSE file, though every crate declares `MIT OR Apache-2.0` - and two of those four are crates we would take. (2) The README's *"Through 2025, this crate is licensed under either of…"* can be read as a term limit on the grant rather than a copyright year. One GitHub issue, two questions; record the answers and the issue URL in §11.3. **Why it is on this list and not merely in the register:** the project has already moved organisation twice and its own comments cite a URL that 404s, so the maintainer being reachable is the assumption, and an answer that arrives after the fallback is needed is worth nothing. | `DEPENDENCIES.md` §11.3 - unassigned; not blocking, cheap now, expensive later |
| ZR-4 | **The vendored `ziffle`'s `LICENSE-MIT` is somebody else's licence, and the disposition is recorded but two follow-ups are not done.** The file opens *"Copyright (c) 2015 The cargo-readme Developers"*, so the MIT branch of the dual licence grants nothing for this crate; `vendor/ziffle/PROVENANCE.md` §4 and `DEPENDENCIES.md` §6 note 5 take **Apache-2.0** and rely on it alone, which needs nothing from upstream and is not blocked. Open: (a) a `deny.toml` clarification pinning ziffle to `Apache-2.0` rather than accepting the declared dual expression - which cannot be written until `deny.toml` exists at all (`DEPENDENCIES.md` §7); (b) one courtesy issue upstream. **The file itself is not to be edited** - a third party's licence text stays byte-identical to what was published, including when it is wrong, and `PROVENANCE.md` §2.2's per-file digests are what would expose an edit. | `DEPENDENCIES.md` §6 note 5, §7; `vendor/ziffle/PROVENANCE.md` §4 - low, and (a) is gated on Phase 7 |

**Settled and removed from this list:** the `DISPUTE` self-equivocation question
(review N4) is answered by D-009 rule 1 — the slot key must include every field
that legitimately varies — and is no longer open.

---

## D-019 — A Tox group carries the table; libp2p keeps the lobby

**Date:** 2026-08-30
**Decided by:** project owner
**Status:** accepted
**Depends on:** D-001, D-002, D-004
**Consequence:** the client becomes **GPL-3.0**. See "The price" below.

### The decision

Once a table is formed, its game traffic leaves libp2p and rides a **Tox NGC
group** created by the founder, whose `chat_id` is published as part of the
table's advertisement in the lobby. Everything up to that point — discovery, the
lobby, the join RPC, the roster, the ratification that produces `session_id` —
stays on libp2p exactly as it is.

* The founder creates the group and is its admin.
* A player joining the table is invited into the group.
* Leaving the table leaves the group.
* The founder removes anybody no longer seated or connected.
* Ordinary Tox group messages carry **human chat only**.
* The game protocol rides **custom lossless packets**.

### Why, and what was argued against it

The problem is relay capacity. A public libp2p relay grants 128 KiB and two
minutes per circuit (D-001's addendum, measured again on 2026-08-30 against the
relays this client now finds in the DHT), and a hand costs 18 KB heads-up and
54 KB six-handed (`MENTAL_POKER.md` §5.1). That is about seven heads-up hands per
circuit and two at six seats.

The case **against** was put and is recorded here rather than lost: the limit
only bites while a connection stays relayed, DCUtR upgrades roughly 70% of them
to direct (`NAT_AND_DISCOVERY.md` §4), and a single reachable player running this
client offers a relay with our own raised limits — twelve hands at a full table
(`swarm.rs`'s `RELAY_MAX_CIRCUIT_BYTES`, derived from `per_hand_bytes` rather
than picked). So the exposure is the intersection of "the punch failed" and "no
volunteer within reach".

**The owner's answer, which is the decision:** that intersection is not
acceptable to depend on, because it requires somebody to have forwarded a port,
and a client whose playability rests on that is a client most people cannot use.
A transport that does not have a per-circuit byte cap is worth its price.

### The price, stated plainly because it is a one-way door

**`c-toxcore` is GPL-3.0, not LGPL.** Verified 2026-08-30 by reading the
repository's own `LICENSE`: *"GNU GENERAL PUBLIC LICENSE, Version 3, 29 June
2007"*, and the closing note recommends the Lesser GPL for anyone who wants to
permit linking with proprietary applications. Linking it makes **this whole
client GPL-3.0**.

Consequences, none of which are reversible once the code ships:

1. `DECISIONS.md`'s open item **"the project licence has never been chosen"** is
   closed by this decision rather than by a decision about licensing. MIT,
   Apache-2.0 and the dual form are foreclosed.
2. `DEPENDENCIES.md` §6's finding — *"the tree is permissive throughout … the
   only appearance of GPL in the tree is as the unchosen half of a dual
   licence"* — stops being true. It must be rewritten, not quietly left.
3. `rs_poker` is Apache-2.0-only, which **is** compatible with GPL-3.0 in that
   direction, so nothing in the existing tree blocks the choice.

The pure-Rust `tox-rs/tox` was considered as a way round the C toolchain and is
**not** a way round the licence: it is GPLv3+ as well, and its own README says
the client part is still being worked on, so NGC is not there to use.

### The risks this carries, from the owner's own measurements

Recorded because they are the reason to expect trouble in a particular place,
and because they were measured in the Python predecessor of this client and that
code no longer exists in the tree.

* **A Tox NGC group is findable only while it is new.** Measured 2026-08-27:
  a host up 20 s was found in 31 s; a host up 6 minutes was never found in 300 s.
  The cause is in `Messenger.c` — a group's onion key is `random_bytes()` per
  start, so two members search two different neighbourhoods.
  **This design does not depend on that path**: the `chat_id` travels in the
  lobby advertisement and members arrive by invitation, so group discovery
  through Tox's DHT is not on the critical path. That is the single most
  important reason the narrowed proposal is buildable where the earlier one was
  not.
* A patched `libtoxcore` fixing the above existed and its benefit was never
  established — one-to-one over two runs, and cross-network never measured.
* Routing between two clients behind one NAT is **not** the wall; that
  correction was made 2026-08-28 after a fresh group handshaked in 83 s.

### What does not change

Everything in the owner's list that is not the transport is already this
project's design and is not re-decided here: the envelope's `table_id`,
`hand_id`, `sequence`, sender, type and payload are `EventBody`'s own six
fields; replay and ordering are `protocol::antireplay` and the chained events;
the deterministic state machine is `STATE_MACHINE.md`; snapshot-then-catch-up is
`protocol::checkpoint`; signing every critical action and **never trusting that a
message arrived over the channel** is the rule the protocol was built on; and an
admin who may remove a player but may not rewrite a hand's history is D-014.

The transport changes. The protocol does not.

### Requirements the owner has set for the Tox side

Recorded here rather than left in a chat log, because they are constraints on
the implementation and not preferences:

* **The Tox instance opens its own port, through NAT-PMP and UPnP, and both are
  on without anybody choosing them.** Not an option, not a setting, not a build
  flag somebody has to remember: `tox_options_set_local_discovery_enabled` and
  the UPnP/NAT-PMP options are set by this client, and the vendored
  `libtoxcore` is built with the support compiled in. A player behind a router
  that would have opened a port for them, and did not because a checkbox was
  off, is a player who cannot host — and the whole reason for D-019 is not
  depending on somebody having configured their router by hand.
* **Only game data.** Human chat stays on ordinary group messages; the protocol
  rides custom lossless packets.
* The group is a **closed** one: the founder invites, removes anybody no longer
  seated, and the `chat_id` reaches players through the lobby advertisement
  rather than through Tox's own group discovery — which is what makes this
  buildable at all (see the risks above).

### What is built first, and why in that order

1. **A transport seam.** `TableSession`, `GameProtocol` and `StateMachine` must
   not name libp2p or Tox. This is cheap now and expensive after the hand is
   wired, and it is what makes the Tox work additive instead of invasive.
2. The hand over the existing transport, so there is something to carry.
3. `libtoxcore` built, the FFI, and the group.

Stated because the alternative — write the Tox layer first and wire the hand
into it — leaves the project with no working game and a second network stack at
the same time.

---

## D-020 — the showdown is held on screen before the next hand

Decided 2026-08-30, on the owner's instruction: **at a showdown the cards of
the opponents who had to show stay visible for about five seconds before the
next hand begins.**

A player who cannot see what beat them cannot learn anything from the hand, and
a client that snaps straight to the next deal is one that has thrown that away
to save five seconds nobody wanted saved.

### Only the seats that showed, and that is arithmetic rather than etiquette

The owner's clarification, and it is worth stating why it is not a rule this
client has to be trusted to keep. A seat that folded, and a seat that reached
the showdown and mucked, **never emits its own reveal share**. Its two cards are
therefore one share short for every other seat at the table, permanently and by
the counting argument of `PROTOCOL.md` §3.4 — there is no complete token set for
them anywhere, and no client, honest or modified, can open them.

So the screen holds exactly what `SHOWDOWN_REVEAL` put on the transcript: the
seats that were required to show, or chose to. A folded seat's holes stay backs,
not because the interface declines to draw them, but because there is nothing to
draw and no path by which there could be.

### It is a local hold, not a protocol stage

The delay is **not** a message and **not** a stage. Every client holds its own
screen for the same interval and then emits `HAND_INIT` for hand `k+1`. Nothing
in the chain has to agree about it.

That is a deliberate choice over the alternative, which was a stage that ends
the pause. The reasons:

* `HAND_INIT` is a **collective** stage, so it completes when every required
  seat has spoken and not before. A client whose hold ran long, or short, or
  who was looking at another table, is simply late — and late is already what
  the stage is built to tolerate. Nothing forks.
* A stage that ended the pause would be a stage a seat could refuse to write,
  which is one more place to stall a table on purpose. The pause has no
  consequence for anybody's chips, so it must not have a consequence for
  whether the game continues.
* The interval is therefore a **display setting**, and a player who wants
  fifteen seconds or none can have it without being out of protocol.

### What it constrains

* The hand driver must keep the revealed cards reachable after `HAND_COMPLETE`
  rather than dropping them with the hand's state — the showdown's contents are
  what the screen is holding.
* The client must not begin hand `k+1` while its own hold is running, or the
  cards vanish from under the player at the moment the new deal repaints.
* A hand that ends with **everybody folding to one seat** has no showdown and
  no cards to hold. Holding a blank table for five seconds is worse than not
  holding it, so that hand gets **a beat and not the hold** — 800 ms as
  shipped, enough that the table does not jump straight into the next deal and
  short enough that nobody is waiting on nothing. The number is in
  `src/net/run.rs`'s `Ended::pause`, beside the five seconds, so the two are
  read together. The same is true of a showdown in which every
  losing seat mucked: there is one hand to look at, which is the winner's, and
  whether that is worth five seconds is the same question as any other showdown.
* The hold covers the **revealed** hole cards and the board together, because a
  hand is read from both. Nothing about it changes which cards exist.

---

## D-021 — the showdown runs in TDA order, and a beaten hand may muck

Decided 2026-08-30, on the owner's instruction. This **resolves** the open
question carried as `STATE_MACHINE.md` Q1 and `PROTOCOL.md` Q-01, whose named
default until now was `MandatoryReveal` — chosen as implementation order, not
as a decision. The decision is:

    config.showdown_policy = TdaMuckWithForfeiture

### The order

The last **aggressor** of the final betting round shows first. If nobody bet on
that round, the first seat still in the hand, clockwise from the button, shows
first. Everybody else follows clockwise from there. This is TDA's rule and it
is what a tournament player expects; a client that opened every hand at once
would be showing cards in an order the table did not agree to.

A seat that is not first to show, and cannot beat what is already exposed, may
**muck**: it forfeits every pot, irrevocably, and its cards are never opened.
That is `STATE_MACHINE.md`'s T43, which exists only under this policy. A seat
that *can* beat what is exposed and wants the pot must show.

### Why this is cheap here, contrary to what §7.7 assumed

`STATE_MACHINE.md` §7.7 kept `MandatoryReveal` as the MVP because it is *"the
only option that needs no additional cryptography"*. Under the shipped design
that is not the distinguishing property, because **a muck is the absence of a
message and not the presence of one**.

Every hole card is one reveal share short for everybody except its owner
(`PROTOCOL.md` §3.4). Showing means publishing your own share; mucking means
not publishing it. There is nothing to forge, nothing to verify and no new
primitive: a mucked hand cannot be opened by an honest peer, a modified peer, or
every other peer at the table acting together. The forfeiture is enforced the
same way — a seat with no share on the transcript is a seat no settlement can
award a pot to.

### The order is an emission discipline, not extra stages

**Corrected before any code was written.** The first draft of this decision said
the showdown becomes a sequence of single-writer stages, one per seat. That is
wrong against `PROTOCOL.md` §4.6, which makes the showdown **one collective
stage**: `SHOWDOWN_REVEAL` and `SHOWDOWN_MUCK` are the one pair of
`event_class = 0` types that share a `sequence`, and a seat emits exactly one of
them — that exclusivity is what keeps the slot at capacity one, and splitting
the stage would have thrown it away for nothing.

TDA order is achieved by **when each client speaks**, not by where its message
sits. The stage completes when every required seat has spoken; it does not care
in what order they did. So:

* the first-to-show emits at once;
* every other client waits until it has seen the reveals of the seats ahead of
  it in TDA order, and then emits its own reveal or its muck.

A seat that speaks out of turn has committed a live-poker irregularity and
nothing more: it cannot see a card it was not going to see, cannot claim a pot
it did not win, and cannot stall anybody. There is nothing to enforce and so
nothing is enforced.

The required-to-show set must still be **exactly determined before the stage
opens**, because a collective stage needs its required set — §4.6 says as much.
It is every seat still live at the showdown; each of them owes one message.

### What it does cost, stated plainly

* **The hand driver has to track the aggressor.** `poker::actions` deliberately
  does not (`src/poker/actions.rs:113`) — its legality predicate has no use for
  it. The driver keeps it, because the showdown order is the one place the
  identity of the last aggressor changes what happens.
* **A stalled showdown is a stalled hand.** A seat that neither shows nor mucks
  is the ordinary crypto-stall case and ends at the hand deadline (T57), with
  every stack restored. No new terminus. Note that the emission discipline makes
  a *slow* showdown normal, so the deadline has to be the hand's and not a tight
  per-message one.
* **`HAND_COMPLETE` ranks only the hands that were shown.** Under
  `MandatoryReveal` the settlement is derivable from a transcript that contains
  every hand; here it is derivable from a transcript that contains the shown
  ones and a muck for each of the rest, which is equally complete and equally
  checkable — but it is a different derivation and the two must not be confused.

### Mucking is illegal when anybody is all in

TDA 16, and `PROTOCOL.md` §4.6 states it: *with at least one player all-in and
betting complete, every live player must show and none may muck.* So the policy
does not simply replace `MandatoryReveal` — it selects between the two per hand,
and an all-in showdown is a mandatory-reveal showdown. A client that offered the
muck button there would be offering an action the receiver must refuse.

### The client may muck for its owner

The owner's words were *"if they find out they have lost, they muck"*. So a
client that is not first to show and whose hand cannot beat what is exposed
mucks **by default**, without asking. TDA permits showing anyway and the option
stays on the screen, because a player who wants to show a bluff is entitled to,
and a client that silently mucked it would have taken a decision that was not
its own.

### Interaction with D-020

The five-second hold starts when the **showdown is over** — every seat has
shown or mucked — and not at the first reveal. An **aborted** hand takes the
short beat rather than the hold, for the same reason a fold-out does: there is
nothing on the table to read. Holding from the first show
would freeze the table in the middle of a sequence the player is watching
unfold. What is held is what the transcript ended with: the hands that were
shown, and the board.

