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

## Open decisions

| # | Question | Blocking |
|---|---|---|
| — | Open-source licence for the project (MIT / Apache-2.0 / dual / GPL-3.0 / AGPL-3.0) | Nothing yet; needed before publication |
| — | Relay admission: `identify` protocol name, or lobby presence (see D-002) | `NETWORK_STACK.md` |
| — | A named, versioned reference engine, since four sections claim disputes are "deterministically adjudicable by any third party running the reference engine" and no such engine is defined (verify N2) | `PROTOCOL.md` |
| — | `DISPUTE` is chained, unsequenced and legal at any time, so the two disputes an honest peer is *required* to emit are an equivocation proof against itself (verify N4) | `PROTOCOL.md` |
| — | A dispute path that does not require the accused peer's signature (D-007 point 4, review A-1) | `PROTOCOL.md` |
| — | Drop the libp2p `dns` and `kad` features and run all discovery through Mainline DHT, including relay volunteers under a second infohash? Removes both hickory advisories and ~12 crates from the build; costs access to the public relay commons, which thins D-004's floor. See `research/INTEGRATION.md` section 3. | Nothing yet |
