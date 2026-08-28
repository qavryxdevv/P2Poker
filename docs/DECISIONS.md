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
`Config::circuit_src_rate_limiters`.** Their element type sits in a
`pub(crate)` module and so cannot be named from outside, but the crate carries
a blanket implementation

```rust
impl<T: FnMut(PeerId, &Multiaddr, Instant) -> bool + Send> RateLimiter for T
```

so a closure coerces into the vector without ever naming the trait. Verified by
compiling exactly that: the probe pushed a per-`PeerId` closure and the printed
config shows three reservation limiters instead of the default two. Note the
third parameter is `web_time::Instant`, so a `web-time = "1"` dependency is
needed to write the closure's signature.

That closure is our admission control: it sees the `PeerId` and the
`Multiaddr` of whoever asks, and returns false for anyone who is not one of
ours. Both vectors must be gated — one governs who may reserve a slot to become
reachable through us, the other who may open a circuit through us.

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

## Open decisions

| # | Question | Blocking |
|---|---|---|
| — | Open-source licence for the project (MIT / Apache-2.0 / dual / GPL-3.0 / AGPL-3.0) | Nothing yet; needed before publication |
| — | Relay admission: `identify` protocol name, or lobby presence (see D-002) | `NETWORK_STACK.md` |
