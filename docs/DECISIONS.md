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
- a message whose chain parent does not exist, or whose signer is not a party to
  this table.

These are safe to act on the moment they arrive. Nothing about them can differ
between honest receivers.

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

### What the players see

`SPEC_CS.md` §22's information window, naming the removed player, the tier, and
what the evidence was — "invalid shuffle proof", "signature did not verify",
"raise below the minimum after checkpoint 4". A removal that cannot be explained
in one sentence to the other players should not be automatic.

The window must also state that the hand was voided and that no chips changed
hands, so nobody reads a void as a loss.

### What this does not become

This is **not** a route back to D-010's forfeiture or to accusation-driven
eviction. Specifically:

- no removal on a timeout, a missing publication, or any liveness judgement;
- no removal on an equivocation proof, whose predicate has failed five times;
- no removal on `attributed`, on a vote, on a certificate, or on any quorum;
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

## Open decisions

| # | Question | Blocking |
|---|---|---|
| — | Open-source licence for the project (MIT / Apache-2.0 / dual / GPL-3.0 / AGPL-3.0) | Nothing yet; needed before publication |
| — | Relay admission: `identify` protocol name, or lobby presence (see D-002) | `NETWORK_STACK.md` |
| J-1 | **DONE in this pass.** `PROTOCOL.md` has dropped `advert_hash` from `GENESIS(0)`, `session_id` and hence `ctx`, and §3.1 now carries a normative `table_params_hash` box — domain `p2p-poker v1 table-params`, twenty-five parts in `LOBBY_TABLE_AD` field order, with `timestamp_unix_ms` and `expires_at_unix_ms` excluded by name. J1(b) is closed at three checked places: §7.2 receiver rule 7, `PLAYER_LIST`'s `n(1)` and `TABLE_READY`'s `n(2)`, both of which changed from `advert_hash` to `table_params_hash` and are now **checked** rather than carried and ignored. | — |
| J-2 | **DONE in this pass.** `PROTOCOL.md` §3.2 carries the general rule and the set `P(k)`; §4.4 instantiates `R(HAND_INIT, k) = P(k-1)`; `dealt_in ⊆ P(k-1)` is normative; `RNG_COMMIT`, `RNG_REVEAL`, `STATE_HASH` and `STATE_ACK` were swept off status words too, and §2.9 is the sweep that enumerates all nine collective sets. | — |
| D-014-1 | Integrate D-014 across the corpus: the two evidence tiers, the checkpoint precondition for tier 2, the one-way exit, the dead seat blinded off, and the information window. Touches all five specification documents. | all — **not started** |
| D-014-2 | The mirror test D-014 requires: under every legal interleaving, no honest peer is evictable. Blocks shipping the feature, not writing it. Specified as a test in `THREAT_MODEL.md` §5.5 beside the eleven `SPEC_CS.md` §25 peers, and its planned home is `tests/adversarial/no_honest_eviction.rs`. | adversarial suite |
| D-014-3 | **New, opened by D-014's integration, and it is the rule D-014 newly makes load-bearing rather than a packaging question. How does a removal reach canonical state?** D-014's safety argument is that *"every honest peer reaches the same verdict from data it already holds"* — which is true of the **verdict** and not of the **holding**. Whether the offending message reached this peer is a per-receiver fact; a seat's `status` is canonical state and enters `state_hash` (D-012, `STATE_MACHINE.md` I30). A peer that removes a seat on a message a second honest peer never received has forked the table, in the exact direction D-012 exists to forbid. So the *evidence* must itself be chained: a message type, a stage kind under `PROTOCOL.md` §3.2's principle, a chain position, and an answer for a peer that never received the offending message. **Second half, same owner:** `PROTOCOL.md` §6.1's `PublicTableState` hashes `sitting_out` and no longer hashes `absent`, so `SeatStatus::Removed` (`STATE_MACHINE.md` §2.4) is canonical per-seat state that **no vector hashes** — two peers disagreeing about a removal agree on every hashed vector, because a removed seat posts the same blinds and holds the same stack as an active seat that folds. A `removed` vector, or a defined use of the two existing bits, is a wire change and `PROTOCOL.md`'s call. Until both are answered, `STATE_MACHINE.md` T64/T65 are specified and have no event to fire them, which is the honest state to leave it in. | `PROTOCOL.md` §4.10/§4.11, §6.1 — **blocking for D-014** |
| N1 | **DONE in this pass, engine half, and it is the fifth consecutive instance of the rule: the worst defect was on the path the previous fix newly made load-bearing.** K-9 installed the solitary freeze and a latch; **the freeze released itself two ways, and both left `I33(c)` passing.** (1) **T53** fired on a reconciliation-round stage whose required emitter set at a solitary peer is `{self}`: the frozen peer published one re-derived value, the stage completed over its required set, every value in it agreed with itself, the freeze lifted and the latch cleared — then another solitary hand, another contradiction, forever. **Fix: T53 gains a conjunct on the *completed stage's signers*, not on the set** — *"carries values signed by at least two distinct seats"* — which is the one form of the test that does not move when `PROTOCOL.md` changes the set's definition. It is inert wherever the set has two or more members. **T54 and T60 needed nothing**: both require two distinct values to remain, and one signer fills one slot with one value. (2) **T50 set no latch at all**, so a solitary peer contradicted by a *`state_hash` mismatch* rather than by an out-of-set event thawed on the timer and re-froze at the next checkpoint 8, every `hand_deadline_ms`. **Fix: T50 sets `solitary_contradicted` when `\|checkpoint.required\| == 1`** — read off the open checkpoint, present tense, because `Event::StateHash` carries no `hand_id` and none is wanted: `checkpoint.required` **is** the regime for the stage being compared, snapshotted when the checkpoint opened. **And `I33(c)` is re-scoped from the latch to the freeze**, because a clause about a latch is vacuously true on both loops: *between any two entries into `Diverged` with a hand dealt between them there is a completed reconciliation stage carrying values signed by at least two distinct seats*. §12.1's fourth obligation is restated to match — name every path back to playing, **including the repair the fix installed for itself**. **And the rule this fix newly made load-bearing, checked in the same pass and found broken: `I33(b)` forbade the frozen peer to produce *any* `Effect::Publish`, which is the one thing T53's stage consumes** — so the exit N1 had just made the load-bearing one could never fire and the freeze had no reachable repair, which is `L4`'s shape one transition along. I33(b) now exempts exactly the §6.3 step 2–3 traffic — `DISPUTE`, the reconciliation-round `STATE_HASH` and its `STATE_ACK` — which opens no card, moves no chip and completes no stage *of the hand*, and which T52 has always accepted from this phase; and it is what makes the two-signer conjunct satisfiable, the frozen peer's own value being one of the two. | `STATE_MACHINE.md` §5.2 (T50, T53), §9.3 condition 0.6, I33(a)(b)(c) |
| N1-a | **New, owed to `PROTOCOL.md` by N1's fix, and it decides whether one exit exists rather than whether it is correct.** T53 now needs a reconciliation-round `STATE_HASH` signed by a seat other than the frozen peer. At a solitary peer that seat is **outside** `P`, so the exit is reachable only if §4.9 admits an out-of-set copy into a **reconciliation round** — the same widening the checkpoint-8 box already makes for the checkpoint itself (*"accepted, compared and retained from any occupied roster seat"*), extended by one stage. **Both answers are safe and the engine is written for either**: admitted, a genuine fork can repair and the table plays on; not admitted, T53 is unreachable in the solitary case, every solitary freeze ends at T54, T60, T57 or T61 and therefore at `TableClosed` through §9.3 condition 0.6. What must not happen is the third state — the exit described and unreachable, which is the shape the Phase 3 gate found for T62 (`L4`). One sentence in §4.9 closes it either way. | `PROTOCOL.md` §4.9 — low |
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
| N-1e | **`PROTOCOL.md` and `STATE_MACHINE.md` both landed `N1` in this pass and they agree; what is left is one conjunct in the engine's floor, and it is provable.** Wire half: §4.9's reconciliation stage is required of `R(c) ∪ W`, never fewer than two seats, and §6.3 step 3 states exhaustively that nothing else releases the freeze or the latch — so **T53's new two-signer conjunct and the wire's floor are the same rule stated twice**, which is belt and braces rather than a conflict, and §6.3 step 3 is the normative owner (D-011 rule 1). Engine half: T50 now latches when solitary and T62 reads `solitary_at`. **The gap is `solitary_at`'s lemma.** §2.6 proves *if the wire's record says hand `k` was solitary then `solitary_at(k)` holds*, from *the record says so exactly when §5.3 step 4 read a one-member `signed_this_hand` at hand `k`'s init*. `PROTOCOL.md` §3.2 now writes the regime test as the **disjunction** `P(k-1) == {self} ∨ P(k) == {self}`, and the second disjunct is the one K1's own walk lands on first — the hand `P` narrows in has three seats in `P(k-1)` and one in `P(k)`, and it is that hand's checkpoint 8 that mismatches. For it, `solitary_since` is written at hand `k+1`'s init, so `j = k+1` and `j <= k` fails: **the lemma is false for exactly the hand the fix exists for.** The repair is one character of slack and it is exact, not conservative: a hand `k` satisfying the second disjunct has `P(k) == {self}`, which is hand `k+1`'s required set, so hand `k+1`'s init reads a one-member set and `j <= k+1` always — hence **`solitary_at(k) := solitary_since == Some(j) ∧ j - 1 <= k <= hand_id`**, with one hand of slack and no more. Also owed, editorially: §10 row 20's *"where it was T50 the table plays on, as before"* is now false for the checkpoint-8-when-solitary route. | `STATE_MACHINE.md` §2.6, §10 — **blocking for K-9** |
| N-4 | **Two representations of one past-tense predicate. `STATE_MACHINE.md` §2.6 answered it correctly with a monotone floor and an ordering lemma; `PROTOCOL.md` §3.2 then sharpened the predicate under it, and the lemma needs the one conjunct `N-1e` names.** The shape of the answer is right and is worth recording as the pattern for this class: **the wire's record is exact, the engine's field is a floor beneath it, and the invariant is an ordering between the two rather than an equality** — which is how a rule split across two owners can keep two structures without either being wrong, provided the direction of the inequality is stated and asserted (I33(a)). What is left is the second disjunct of §3.2's regime test, `P(k) == {self}`, which the floor's write site does not see; `N-1e` gives the exact repair and the proof that one hand of slack suffices. **The residual after that is nil** — the clearing question that opened this item is closed by the field never being cleared. | `STATE_MACHINE.md` §2.6 — **with `N-1e`** |
| N-5e | **Owed to `STATE_MACHINE.md` by `PROTOCOL.md`'s `N5` fix, and it is two lines.** §4.9 now defines a **readmission set `A`**: a stale `0x0804 PLAYER_SIT_IN`, or a stale checkpoint-8 `STATE_HASH` that **agrees** with this receiver's value, carries its sender into the next hand init instead of being dropped, because at a peer in the solitary regime every window is zero-width and D-013's readmission promise was otherwise void exactly where D-013 made it a steady state. §4.4 states the consequence: `R(HAND_INIT, k) = P(k-1) ∪ A`, with `A` read once and cleared there. The engine's hand init (§5.3 step 4) must read the same union into `signed_this_hand`'s successor and `dealt_in`, and `A` must be cleared in the same step. **This is also the disposition of `K-3b`**, whose *"a seat returning from silence needs no `PLAYER_SIT_IN` at all"* was true only inside a window that closes on an event emitted concurrently with it. | `STATE_MACHINE.md` §5.3 step 4 — medium |
| N-8 | **D-014 cites an information window `SPEC_CS.md` §22 does not contain.** §22 is the GUI section and its nearest element is the table window's protocol/security status. The content is carried correctly by `STATE_MACHINE.md` T64 through `Effect::Fault` and I27, so nothing is lost — but a binding decision points at a section that does not hold the thing it points at. Cite §22's status element, or state that the window is new and specified by T64's record. | `DECISIONS.md` D-014 — low |
| N-9 | **`CRYPTOGRAPHY.md` carries zero D-014 while owning three of tier 1's six clauses.** Tier 1 names a failed shuffle proof, a failed decryption-share proof and a failed key-ownership proof; §8's verification rules and §6.5's target table are where those verifiers live, and `THREAT_MODEL.md` X37 names *"a proof verifier stricter than the prover"* as the mechanism that turns an honest message into tier-1 evidence with no checkpoint to wait for. `PROTOCOL.md` §4.0's anti-eviction box now states the exception and names those verifiers by step, which makes the omission louder rather than quieter. `NETWORK_STACK.md`'s zero is defensible and unrecorded (D-014 has no transport consequence; §0.1's forbidden row is unaffected) — one row each. | `CRYPTOGRAPHY.md` §8, §0; `NETWORK_STACK.md` §0.1 — low |

**Settled and removed from this list:** the `DISPUTE` self-equivocation question
(review N4) is answered by D-009 rule 1 — the slot key must include every field
that legitimately varies — and is no longer open.
