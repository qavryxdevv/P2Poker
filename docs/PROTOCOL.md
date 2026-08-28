# PROTOCOL.md — the versioned wire protocol

Phase 1 output. Binding spec sections: `SPEC_CS.md` §4, §12, §13, §14, §15, §16,
§17, §20, §27. Binding owner decisions: `DECISIONS.md` D-001 … **D-011**.

**D-007 corrects D-006 and wins over it**: an action deadline is advisory and a
fold-effect timeout certificate is forbidden where the certificate cannot be
trusted, and no claim in this document may say the certificate protects such a
table.

**D-008 corrects D-007's scoping and wins over it**: the condition is **`|V|`,
the size of the required voter set — never `n`, the seat count**. A certificate
with `|V| < 2` has no effect, and a seat leaves `V` only once a completed, valid
certificate already names it. D-007's heads-up rules survive unchanged as the
`|V| = 1` instance of the general rule. **Any rule in this document still scoped
on `n` is a defect**; §8.3 is the normative home and §8.5 is what makes it
verifiable.

**D-010 revises D-005's chip rule and wins over it, and it is the decision that
shapes this revision.** Four adversarial passes each found a worse defect than
the last, and every severe one ended in an *honest* peer's chips being forfeited
and its key blocked for following the protocol. D-010 removes the prize rather
than patching the fourth mouth of one defect. Stated once here, prominently, so
that no later section has to repeat it and no reader has to assemble it:

> **Normative, canonical for this document. Consuming a proof, a certificate, an
> attribution or an abort moves no chips and removes no player in this version.**
>
> 1. **An abort is neutral.** Every `HAND_ABORT`, whatever its `cause` and
>    whoever is named in its `attributed` set, restores every stack to its
>    start-of-hand value. **No message defined in this document carries a
>    forfeiture instruction**, and no receiver rule in it applies one.
> 2. **Attribution is evidence with no automatic consequence.** The transcript
>    still records which peer failed to publish what, signed and verifiable by
>    anyone. Nothing in this document acts on that record.
> 3. **No automated penalty and no automated eviction.** No rule here
>    block-lists, unseats, refuses a seat to, or otherwise penalises a peer on
>    the strength of an `EquivocationProof`, a `TIMEOUT_CERT`, a `DISPUTE` or an
>    attribution. Sitting out a repeat offender is a **user** decision.
> 4. Proofs and certificates are still **produced** — they are how a human, or a
>    later version, adjudicates — but no consumer of one moves a chip or removes
>    a player. Whether they should be produced at all in the MVP is an open
>    question the owner has not answered; §12 records it and does not answer it
>    either.
>
> **The accepted cost, stated plainly rather than hidden (`SPEC_CS.md` §18).**
> The rage-quit escape returns, and not only below D-008's floor: at every table
> size and on every abort path, a player losing a big pot can stall, disconnect
> or fault the table and get their chips back. D-005 closed that with forfeiture;
> D-010 reopens it knowingly, because forfeiture was measured over four passes to
> take chips from honest players, and an exploit that harms an honest player is
> worse than one that merely lets a dishonest player escape a loss. `THREAT_MODEL.md` §8 limitation 4 carries
> it as an unfixed limitation and §4.10 states the trade in full.

Two structural consequences follow, and both are stated where they belong rather
than here: `HAND_ABORT cause = 5` is deleted, because an unchained event may no
longer terminate a stage (§5.2, §4.10); and a divergence reconciliation is its
own chained stage rather than a second copy of the checkpoint's (§4.9, §6.3).

**D-011 is the shape of this revision, and it decides three things this document
had left to another document or to a later pass.**

> 1. **The anti-replay slot key is §5.2.1 and only §5.2.1.** It is a literal
>    eight-component tuple with field indices, it contains `event_type`, and
>    every other document in the corpus references that section by number and
>    reproduces no part of it. §4.11 carries the check of all 39 message types
>    against it, per type rather than per stage-kind group. This is D-009 rule 1's
>    sixth attempt; the five that failed were prose.
> 2. **The terminal stage of an aborted hand is witness-independent, has no
>    `stage_hash`, and yields `TERMINAL(k) = ABORT_TERMINAL(k)`, a function of
>    `GENESIS(k)` alone** (§3.1, §3.2, §4.10). It cannot deadlock, because no
>    seat's silence blocks it, and it cannot fork, because it reads nothing the
>    peers disagree about. `GENESIS(k+1)` therefore exists on every path and the
>    next hand can start. P3 is settled here; G1 is closed by point 1.
> 3. **Nothing consumes an `EquivocationProof`, at any layer** (§5.2, §4.0). No
>    transition takes one as input, no `cause` value carries one, and no
>    block-list, allow-list, unseating or seat refusal is driven by one — in this
>    document or in any other (D-010 point 3, D-011 rule 3).

**This document owns the wire and nothing else** (D-011 rule 1): message shapes,
the event envelope, the chain, sequence numbers, the anti-replay slot, canonical
bytes, and what a receiver validates. It does not specify state or transitions
(`docs/STATE_MACHINE.md`), the mental-poker constructions
(`docs/CRYPTOGRAPHY.md`), the transport, discovery or connectivity stack
(`docs/NETWORK_STACK.md`), or the threat classification
(`docs/THREAT_MODEL.md`). Where it must refer to those, **it refers by section
number and restates nothing.** §14 notes 16–20 list what was deleted from this
revision on that rule and what replaced each copy.

## How to read the evidence tags

Everything factual here rests on the Phase 0 research documents in
`docs/research/`. Where a claim came from a compiler or from crate source, the
tag says which research document verified it:

* **[LIBP2P §n]** — `docs/research/LIBP2P.md`, section n
* **[CRYPTO §n]** — `docs/research/CRYPTO_LIBS.md`
* **[MENTAL §n]** — `docs/research/MENTAL_POKER.md`
* **[RULES §n]** — `docs/research/POKER_RULES.md`
* **[NAT §n]** — `docs/research/NAT_AND_DISCOVERY.md`
* **[DHT §n]** — `docs/research/MAINLINE_DHT.md`

A claim with **no** tag is a design decision made in this document. Those are the
ones to argue with. Unresolved design decisions are marked **OPEN QUESTION Q-nn**
and collected in §12. Per `SPEC_CS.md` §36 an open question is preferred to an
invention.

**All seven research documents listed in the Phase 0 brief exist and were read.
No gap to report on that account.** `docs/GUI_STACK.md` research exists but is not
load-bearing for this document.

### What this protocol does not claim

Per `SPEC_CS.md` §18 and §36: this protocol does **not** make all cheating
impossible. **`THREAT_MODEL.md` states, per class, what is prevented, what is
detected, and what is neither** — it is the owner of that classification and this
document no longer keeps a copy of it (D-011 rule 1, §14 note 20). §11 here is a
routing table from an attack to the mechanism in *this* document that acts on it,
and it reaches no verdict of its own.

---

## 1. Version, negotiation and the shape of the wire

**The module this document governs** (`SPEC_CS.md` §23, interim mapping until
`docs/ARCHITECTURE.md` exists at the start of Phase 2): `src/protocol/` —
`messages.rs`, `serialization.rs`, `signatures.rs`, `transcript.rs`. Nothing in
this document is implemented inside `src/net/`, `src/poker/` or
`src/mental_poker/`; that separation is §23's binding requirement.

### 1.1 Versioning model

There are two version numbers and they do different jobs.

| Name | Type | Where | Meaning |
|---|---|---|---|
| `PROTOCOL_MAJOR` | compile-time constant, `1` | libp2p protocol name strings, GossipSub topic string | wire-format epoch. Two peers with different majors cannot negotiate anything; they never meet, because the protocol strings differ. |
| `protocol_version` | `u16`, currently `1` | every signed envelope, every table advertisement | the exact rule set and encoding in force for this session. Must equal the value pinned by the table advertisement for every event of that table. |

For `PROTOCOL_MAJOR = 1` there are **six** strings, exactly:

```
identify protocol             /p2p-poker/1
GossipSub lobby topic         /p2p-poker/lobby/1        (IdentTopic, not Sha256Topic)
GossipSub lobby chat topic    /p2p-poker/lobby-chat/1   (IdentTopic; see §7.7)
lobby snapshot RPC            /p2p-poker/lobby-snapshot/1
join RPC                      /p2p-poker/join/1
table event stream            /p2p-poker/table/1
```

`/p2p-poker/join/1` is new: the join exchange is a one-shot request-response RPC,
not table-stream traffic (C-7 of `docs/research/PHASE0_FIXPLAN.md`; see §1.4 and
§4.3). `/p2p-poker/lobby-chat/1` carries `SPEC_CS.md` §22's lobby chat (§7.7).

`IdentTopic` and `Sha256Topic` both exist in `libp2p-gossipsub` 0.49.5; `IdentTopic`
sends the topic string in the clear, which is correct here because the string is
public anyway and debuggable. [LIBP2P §6]

The identify protocol name doubles as the relay-admission discriminator of D-002:
a peer that advertises `/p2p-poker/1` through `identify` is a poker peer. This is
the first of the two candidate admission rules D-002 left open; settling it is
`NETWORK_STACK.md`'s job, not this document's.

### 1.2 HELLO / CAPABILITIES

The handshake runs on a freshly opened `/p2p-poker/table/1` stream, immediately
after the libp2p security handshake completes, before any other message. It is
symmetric: each side sends `HELLO`, then each side sends `CAPABILITIES`.

**Why an application handshake at all, when libp2p already authenticated the
connection.** Because `SPEC_CS.md` §20 requires three separate identities and
forbids treating the libp2p connection as authentication of application data:

| Identity | Key | Established by | Lifetime |
|---|---|---|---|
| libp2p `PeerId` | libp2p Ed25519 keypair, persisted as a protobuf blob [LIBP2P §2] | Noise / TLS 1.3 handshake | installation |
| poker application identity | our own Ed25519 keypair, `ed25519-dalek` 3.0.0 [CRYPTO §2.1] | `HELLO` signature | installation |
| table / session identity | `table_public_key` (Ed25519) and `session_id` | `LOBBY_TABLE_AD` and `TABLE_READY` | one table |

`HELLO` is what binds the second to the first. Nothing else does.

**The binding, precisely.** `HELLO`'s payload contains `peer_id_self` — the
sender's own libp2p `PeerId` — and the whole body is signed with the sender's
*application* key. The receiver accepts only if `peer_id_self` equals the PeerId
the libp2p handshake authenticated on this connection. An attacker who records
Alice's `HELLO` and replays it on his own connection fails, because on his
connection the transport-authenticated remote PeerId is his, not Alice's. He
cannot produce a `HELLO` naming his own PeerId under Alice's application key
without Alice's application secret key. No round trip and no challenge–response
is needed for this property.

`peer_id_remote` is also included and must equal the local PeerId of the receiver.
This kills a relay-side splice in which one connection's `HELLO` is fed into a
different connection to a different destination.

`nonce` is 32 fresh bytes from `getrandom::SysRng` [CRYPTO §1.1] and is that
side's contribution to the connection nonce.

**`CAPABILITIES`** is sent second, once each side has the other's `HELLO`. It
carries the *negotiated result*, so a mismatch is caught immediately rather than
at first use:

* `chosen_protocol_version` = `min(local_max, remote_max)`, and it must be ≥ both
  sides' `protocol_version_min`. If not, the connection is closed with no further
  messages.
* `connection_nonce = BLAKE3_dom("p2p-poker v1 connection", [nonce_initiator,
  nonce_responder, peer_id_initiator_bytes, peer_id_responder_bytes])` where the
  initiator is the side that opened the stream. Both sides compute it; each
  states it in `CAPABILITIES`; a mismatch closes the connection.
* `capabilities` = the intersection of the two advertised sets.

Both `HELLO` and `CAPABILITIES` use the same signed envelope as everything else
(§2), but they are **unchained** events: `chain_scope = 0`, `event_class = 0`,
`table_id = ZERO32`, `hand_id = 0xFFFF_FFFF_FFFF_FFFF`,
`previous_event_hash = ZERO32`, `sequence = 0` (§2.3). They occupy no stage slot
in any chain and can never take part in an `EquivocationProof` (§5.2).

Their anti-replay is therefore not the envelope's `sequence`. It is the
`connection_nonce` binding plus a per-connection counter held in the stream's own
state machine: `HELLO` exactly once per side per stream and before anything else,
`CAPABILITIES` exactly once per side and only after both `HELLO`s. A second
message of either type on one stream closes the stream.

**A `CAPABILITIES` exchange authorises nothing about a table.** Joining a table
is a separate, table-scoped handshake (§4.3). A connection may carry several
table sessions.

### 1.3 Capability names

A capability is an ASCII name, `[a-z0-9/._-]{1,32}`, at most 32 per peer,
transmitted as a sorted, de-duplicated array of byte strings. Unknown names are
ignored — that is what makes new capabilities a *minor*-version change (§10).

Defined for version 1:

| Name | Meaning |
|---|---|
| `nlhe/2-6` | can play No-Limit Hold'em, 2 to 6 seats. MVP scope, `SPEC_CS.md` §32. |
| `nlhe/7-10` | can play 7 to 10 seats (the `RATED_SNG_POKERTH_V1` preset needs this) |
| `deck/bs-bg12-secp256k1/1` | supports the Barnett–Smart + Bayer–Groth deck of `docs/CRYPTOGRAPHY.md` [MENTAL §10] |
| `lobby/snapshot/1` | will serve `LOBBY_SNAPSHOT_REQUEST` |
| `relay/volunteer/1` | is running the D-002 relay server with raised limits |

A table session requires that every seated participant advertise
`deck/bs-bg12-secp256k1/1` and a seat-count capability covering the table's
`max_players`. That check happens at `TABLE_READY`, not at `CAPABILITIES`.

### 1.4 Framing and transport mapping

Five channels carry protocol messages, and each message type is legal on exactly
one of them. A message arriving on the wrong channel is dropped and its sender
is rate-limited; it is not processed.

**Which libp2p behaviour carries each channel, with what settings, is
`NETWORK_STACK.md`'s** (D-011 rule 1). The behaviour names, the GossipSub
validation settings and the crate versions that stood in the middle column here
are deleted; this table now carries the protocol string, which is a wire
identifier, and the framing, which is ours and survives a change of crate.

| Channel | Protocol string | Framing |
|---|---|---|
| **Lobby broadcast** | `/p2p-poker/lobby/1` | one `SignedEvent` per broadcast message, no extra framing |
| **Lobby chat broadcast** | `/p2p-poker/lobby-chat/1` (§7.7) | one `SignedEvent` per broadcast message |
| **Lobby RPC** | `/p2p-poker/lobby-snapshot/1` | the RPC codec's own framing; `SNAPSHOT_REQ_MAX` / `SNAPSHOT_RESP_MAX` per §13 |
| **Join RPC** | `/p2p-poker/join/1` | the RPC codec's own framing; `JOIN_REQ_MAX` / `JOIN_RESP_MAX` per §13; 20 s timeout |
| **Table mesh** | `/p2p-poker/table/1` | `u32` big-endian length prefix, then exactly that many bytes of `SignedEvent`. Nothing else. |

**Which message is legal on which channel.** This is a table rather than prose so
that adding a channel or a message type cannot leave it stale. §4.11 repeats it
per message code.

| Message | Channel |
|---|---|
| `HELLO`, `CAPABILITIES` | table mesh (`/p2p-poker/table/1`), before anything else on that stream |
| `LOBBY_TABLE_AD`, `LOBBY_TABLE_REMOVE`, `LOBBY_PLAYER_PRESENCE` | lobby broadcast |
| `LOBBY_CHAT` | lobby chat broadcast |
| `LOBBY_SNAPSHOT_REQUEST`, `LOBBY_SNAPSHOT_RESPONSE` | lobby RPC |
| `JOIN_REQUEST`, `JOIN_ACCEPT`, `JOIN_REJECT` | join RPC |
| everything else (`PLAYER_LIST`, `TABLE_READY`, and all of groups 3–8) | table mesh |

**The one requirement this document places on the transport**, stated as a
requirement rather than as a crate choice: the table channel must be long-lived
and bidirectional, because both sides push and no message type is a response to
another. Which crate provides that, and whether it is semver-exempt, is
`NETWORK_STACK.md`'s. The framing above is ours, so replacing the crate does not
change the wire.

**A transport-layer message signature is never the application signature.** A
transport that authenticates the peer identity owning the connection
authenticates "which socket said this". `SPEC_CS.md` §12's signature is over
canonical CBOR by the **application** key (§2.4). Both are required and neither
substitutes for the other; a receiver that skips §4.0 step 9 because the
transport already authenticated the sender has accepted a forged action.
`NETWORK_STACK.md` owns the transport half.

### 1.5 The table is a full mesh; forwarding is allowed

Every seated participant holds a `/p2p-poker/table/1` stream to every other
seated participant. There is no forwarding node, no host, and no star topology.
For 10 seats that is 45 connections, which is unremarkable.

A participant **may** forward a `SignedEvent` it received to any other
participant of the same table. Forwarding cannot forge anything — the signature
and the hash chain are checked identically whether an event arrived from its
author or from a third party — and it has two benefits:

1. it defeats selective censorship, in which an author sends an event to five of
   six peers and stalls the sixth into a spurious timeout;
2. it makes equivocation *more* likely to be detected, because two conflicting
   copies of one sequence slot are more likely to meet at one receiver (§5).

A receiver must therefore be idempotent: a byte-identical event received twice is
a no-op, not a duplicate-message violation.

Whether a full mesh is *required* before a table may start, or whether a pair
that cannot connect directly may still be seated, is **OPEN QUESTION Q-03**.

**The connectivity floor is `NETWORK_STACK.md`'s** (D-011 rule 1). The paragraph
that stood here restated it — which NAT combinations cannot be traversed, what a
relay changes, and what D-004's layer 3 still guarantees. What this document is
normative about is the consequence for the wire, and it is one sentence: **a full
mesh is a precondition of seating, so a pair that cannot connect cannot both be
in the roster that `TABLE_READY` freezes.** Whether that precondition should be
relaxed is Q-03.

---

## 2. Canonical serialisation and the exact signed bytes

### 2.1 The encoding: `minicbor` 2.3.0, arrays only

`SPEC_CS.md` §12 requires canonical serialisation and forbids signing
non-canonical JSON. RFC 8949 §4.2.1 "Core Deterministic Encoding Requirements"
demands shortest-form integers, definite lengths only, and map keys sorted by
their encoded bytes.

The Phase 0 evaluation compiled and ran all four Rust CBOR candidates against all
three requirements [CRYPTO §4]:

| Crate | Shortest ints | Definite lengths | Sorted map keys |
|---|---|---|---|
| `serde_cbor` 0.11.2 | rejected: unmaintained + stack-overflow CVE | | |
| `ciborium` 0.2.2 | yes | **no** | **no** |
| `cbor4ii` 1.2.2 | yes | **no** | **no** |
| `minicbor` 2.3.0 | yes | yes | n/a — **we use no maps** |
| `dcbor` 0.25.2 | yes | yes | yes |

`ciborium` and `cbor4ii` emit indefinite-length arrays whenever serde hands them
an iterator with no exact size hint, and order `HashMap` keys by a per-process
random seed — measured, two runs of the same binary produced different bytes for
the same logical value. [CRYPTO §4.2, §4.3] A `HashMap` on a signing or hashing
path is therefore a live protocol bug, not a theoretical one: two honest clients
would compute different `STATE_HASH` values for identical state and open a
dispute against each other.

**The chosen scheme removes the problem instead of solving it: there are no maps.**
Every signed or hashed structure is a definite-length CBOR *array* with a field
order fixed by the derive. There is nothing left to sort and no encoder setting
anyone can get wrong.

`dcbor` 0.25.2 is the only fully RFC 8949 §4.2 conformant crate and additionally
*rejects* non-deterministic input on decode, but it pulls `chrono` and 15 further
crates including the Windows time-zone stack, against `SPEC_CS.md` §28.
[CRYPTO §4.4] It is kept as a **dev-dependency differential oracle** (§2.7).

### 2.2 Mandatory encoding rules

These are protocol rules, not style. Violating any of them is a major-version
break in disguise.

1. `#[cbor(array)]` on every signed or hashed struct. Never `#[cbor(map)]`.
2. No `HashMap`, `HashSet`, or any other unordered container in a signed or
   hashed type. Where a mapping is unavoidable it is a `Vec<(K, V)>` sorted by
   encoded key, and sortedness is validated on decode.
3. **No floating-point values anywhere in the protocol.** Chips are integers
   [RULES A0]; timeouts are integer milliseconds. Floats bring
   preferred-float-shortening and NaN canonicalisation rules we refuse to
   litigate. (`ciborium` shortens `1.0f64` to half precision `f93c00` — correct
   per RFC 8949 and exactly the class of subtlety to avoid. [CRYPTO §4.6])
4. Byte arrays use `#[cbor(with = "minicbor::bytes")]`. Without it a `Vec<u8>`
   encodes as a CBOR *array of integers* (`83010203`) rather than a byte string
   (`43010203`) — roughly double the size, and a different byte string, so it
   silently changes every hash. [CRYPTO §4.6]
5. Field indices `#[n(..)]` are **append-only forever** across the life of a
   major version, and in practice never change at all, because appending a field
   changes the array length and therefore every historical signature (§10.2).
6. Enumerations are encoded as their `u16` discriminant, never as a string.

Rules 1–4 are mechanically checkable and should be enforced by a test that
reflects over every signed type, not by a review checklist. [CRYPTO §11.6]

### 2.3 The three nesting levels

A wire message is three length-prefixed byte strings nested inside one another.
This looks verbose and is deliberate: each level has exactly one job, each is
independently canonicality-gated, and the outer parser is small enough to fuzz
exhaustively.

```
SignedEvent                  #[cbor(array)]        <- what is on the wire
  n(0) body       : bytes    canonical CBOR of EventBody
  n(1) signature  : bytes[64]

EventBody                    #[cbor(array)]        <- what is signed and hashed
  n(0)  protocol_version    : u16
  n(1)  table_id            : bytes[32]
  n(2)  hand_id             : u64
  n(3)  sequence            : u64
  n(4)  sender_public_key   : bytes[32]
  n(5)  event_type          : u16
  n(6)  payload             : bytes                canonical CBOR of the per-type payload
  n(7)  previous_event_hash : bytes[32]
  n(8)  emitted_at_unix_ms  : u64                  ADVISORY ONLY, see 2.6
  n(9)  next_deadline_ms    : u32                  NORMATIVE, see §8
  n(10) chain_scope         : u8                   1 = chained, 0 = unchained
  n(11) event_class         : u8                   0 = ordinary, 1 = TIMEOUT_VOTE,
                                                   2 = TIMEOUT_CERT

<payload>                    #[cbor(array)]        <- one struct per event_type
```

This is the field list `SPEC_CS.md` §12 requires, plus the two discriminators
`n(10)` and `n(11)`. `timestamp/deadline information` is split into the two
fields `n(8)` and `n(9)` because they have opposite trust properties (§2.6, §8).

**`chain_scope` and `event_class`, exactly.**

| Field | Type | Values |
|---|---|---|
| `chain_scope` | `u8` | `1` = the event occupies a stage slot in a hand chain or in the setup chain; `0` = unchained (handshake, join, lobby) |
| `event_class` | `u8` | `0` = ordinary chain event; `1` = `TIMEOUT_VOTE`; `2` = `TIMEOUT_CERT`. Meaningless and always `0` when `chain_scope == 0`. For classes `1` and `2` the class is **not** the whole slot key: the subject is part of it too (§5.2). |

An **unchained** event (`chain_scope = 0`) must carry `table_id = ZERO32`,
`hand_id = 0xFFFF_FFFF_FFFF_FFFF`, `previous_event_hash = ZERO32`,
`sequence = 0`. The table it concerns, where it concerns one, is named **in the
payload**, never in the envelope. A receiver rejects an unchained event carrying
anything else in those four fields.

Unchained message types, exhaustively: `HELLO`, `CAPABILITIES`, `JOIN_REQUEST`,
`JOIN_ACCEPT`, `JOIN_REJECT`, `PLAYER_LIST`, `LOBBY_TABLE_AD`,
`LOBBY_TABLE_REMOVE`, `LOBBY_PLAYER_PRESENCE`, `LOBBY_SNAPSHOT_REQUEST`,
`LOBBY_SNAPSHOT_RESPONSE`, `LOBBY_CHAT`, **`DISPUTE`**. Everything else is
chained.

`DISPUTE` is on this list and it is the only table-mesh message that is, which is
worth one sentence because an earlier draft had it chained. `chain_scope = 1`
means *the event occupies a stage slot*, and `DISPUTE` occupies none by its own
definition: §4.9 makes it legal outside its stage and emittable at any time, and
§3.2's `stage_hash` is taken over `event_class = 0` or `event_class = 2` events of
a stage that has an emitter set, which a dispute never joins. So it was chained
and in no chain. Worse, §6.3 *requires* an honest peer to emit two of them in one
hand — one at step 2, one at step 4(a) — and with no `sequence` rule to separate
them both landed in one slot, which is §5.2's equivocation predicate satisfied
exactly, by mandatory honest behaviour, against an honest key. Moving `DISPUTE`
to `chain_scope = 0` closes that the same way A-4 closed it for lobby and join
traffic: §5.2 cannot reach an unchained event at all.

The two fields exist for one reason each, and both are load-bearing:
`chain_scope` keeps the equivocation predicate of §5.2 off traffic that has no
stage slot, and `event_class` gives a `TIMEOUT_VOTE` its own slot so that a voter
at a collective stage does not equivocate against its own contribution (§4.8).
Neither is optional.

Neither is *sufficient* either, and that is why §5.2 states the slot key as a
rule rather than leaving it to be read off this table. A vote about seat `A` and
a vote about seat `B` at one stage are two events one honest voter is entitled to
emit, so the class alone would put them in one slot; §5.2 therefore extends the
key with the subject for `event_class` 1 and 2. **A message type whose envelope
lets one honest sender produce two bodies in one slot is a defect in the type,
not in the sender** — the property is stated normatively in §5.2 and every new
chained type is checked against it before it is added here.

Field indices are append-only forever (§2.2 rule 5), so `n(10)` and `n(11)` sit
after `next_deadline_ms` rather than beside the fields they are related to.

The signature cannot live inside the bytes it signs, hence the two-level split
between `SignedEvent` and `EventBody`. `EventBody` is transported as an opaque
byte string precisely so that the receiver can verify the signature over the
**exact bytes received** and never over a re-encoding (§2.5).

Declaring the payload as an opaque byte string, rather than as a CBOR-tagged
union, means the envelope parser does not need to know any event type. That
keeps the code that touches unvalidated network data tiny, and it makes the
fuzzing target of `SPEC_CS.md` §27 a single function.

Reference shape, from the compiled Phase 0 probe [CRYPTO §4.6], adapted:

```rust
#[derive(minicbor::Encode, minicbor::Decode, PartialEq, Debug, Clone)]
#[cbor(array)]
pub struct EventBody {
    #[n(0)] pub protocol_version: u16,
    #[cbor(n(1), with = "minicbor::bytes")] pub table_id: [u8; 32],
    #[n(2)] pub hand_id: u64,
    #[n(3)] pub sequence: u64,
    #[cbor(n(4), with = "minicbor::bytes")] pub sender_public_key: [u8; 32],
    #[n(5)] pub event_type: u16,
    #[cbor(n(6), with = "minicbor::bytes")] pub payload: Vec<u8>,
    #[cbor(n(7), with = "minicbor::bytes")] pub previous_event_hash: [u8; 32],
    #[n(8)] pub emitted_at_unix_ms: u64,
    #[n(9)] pub next_deadline_ms: u32,
    #[n(10)] pub chain_scope: u8,
    #[n(11)] pub event_class: u8,
}
```

### 2.4 The exact byte string that is signed

```
DOMAIN_EVENT = "p2p-poker/v1/event" NUL-padded to 24 bytes

hex: 70 32 70 2d 70 6f 6b 65 72 2f 76 31 2f 65 76 65 6e 74 00 00 00 00 00 00
     |<---------- 18 ASCII bytes ---------->|<---- 6 NUL ---->|

TO_BE_SIGNED = DOMAIN_EVENT || u32_be(len(body_bytes)) || body_bytes
```

The same 24 hex bytes are printed in §13. They are the normative value; the
separators are slashes, **not** the spaces used by the `derive_key` domain
strings of §2.8, and the difference is deliberate — see §2.8's first table row.

`body_bytes` is the canonical CBOR encoding of `EventBody`. The length prefix is
redundant given a fixed-length domain tag but costs four bytes and removes any
argument about concatenation ambiguity.

The signature is `ed25519-dalek` 3.0.0 over `TO_BE_SIGNED`.

**Verification is `VerifyingKey::verify_strict`, never `verify`.** Plain `verify`
accepts signatures under small-order and non-canonical public keys, which permits
signature malleability, and a malleable signature is an equivocation hole under
§14: two distinct byte strings validating for one logical event. [CRYPTO §2.1]

`ed25519-dalek`'s `hazmat` feature is never enabled and a key is never
reconstructed from separately sourced secret and public halves — that is the shape
of RUSTSEC-2022-0093, fixed in ≥ 2.0 but still the wrong habit. [CRYPTO §2.1]

### 2.5 The canonicality gate

`minicbor` is a codec, not a validator: it accepts non-preferred integer forms,
indefinite lengths and trailing garbage. [CRYPTO §4.7] Left alone that is an
equivocation hole, because a hostile peer could hand two byte encodings of one
logical event to two different peers, and neither would notice.

**Rule: decode, re-encode, and require byte equality — before any signature check
and at every one of the three nesting levels.**

```rust
/// Bytes that do not re-encode to themselves are rejected BEFORE the signature
/// check, so a hostile peer cannot smuggle two byte encodings of one logical
/// event past the equivocation detector.
fn decode_canonical<T>(bytes: &[u8]) -> Result<T, ProtoError>
where T: for<'b> minicbor::Decode<'b, ()> + minicbor::Encode<()> {
    let v: T = minicbor::decode(bytes).map_err(|_| ProtoError::Malformed)?;
    if minicbor::to_vec(&v).map_err(|_| ProtoError::Malformed)? != bytes {
        return Err(ProtoError::NonCanonical);
    }
    Ok(v)
}
```

The Phase 0 probe proved this catches all three hostile encodings — non-preferred
integers, indefinite-length arrays, trailing bytes — and that truncated inputs at
every length are rejected without a panic. [CRYPTO §4.7]

**The signature is always verified over the exact received `body_bytes`, never
over a re-encoding.** Canonicalise-then-verify would let a peer's signature
migrate onto bytes it never signed.

Ordering matters and is normative: **gate, then verify, then interpret.**

### 2.6 Timestamps are advisory and never a validity condition

`emitted_at_unix_ms` is in the signed body, so it is covered by the signature and
enters the transcript hash. It is **never read by the state machine, never
compared against a local clock for chained hand events, and never a reason to
reject an event.**

This is not squeamishness. If a receiver rejected an event whose timestamp fell
outside a skew window, then two honest peers with 30 seconds of clock offset
would accept different sets of events and diverge — manufacturing exactly the
§15 dispute the field was supposed to help with. There is no trusted clock here;
a wall-clock value in a consensus-relevant predicate is a bug.

The field exists for two legitimate uses: human-readable hand histories, and
after-the-fact forensics. Both are read-only.

The two exceptions, both outside the hand chain, both with purely local effect:

* `LOBBY_TABLE_AD` carries `timestamp` and `expires_at` in its payload and these
  *are* checked, because a lobby entry must expire without cooperation from a
  crashed peer. The effect is local eviction from one client's lobby list. It
  never rejects a chained event and never diverges game state. [NAT §7.3]
* `HELLO` freshness is bounded loosely to limit replay of very old handshakes.
  Failure closes one connection.

Everything time-dependent that *does* affect state goes through `next_deadline_ms`
and the timeout certificate of §8, which use relative durations and unanimity
within the required voter set `V` rather than absolute time — and which have an
effect only where `|V| >= 2` (§8.3, D-008).

### 2.7 How determinism is tested

`minicbor` gives determinism by construction under the rules of §2.2, but "by
construction" is a claim to be tested, not asserted.

1. **Round-trip identity** on every signed type: `decode(encode(v)) == v` and
   `encode(decode(b)) == b`, as a proptest over arbitrary well-formed values.
2. **The canonicality gate fires** on each of: non-preferred integer encoding,
   indefinite-length array, trailing bytes, duplicated field, truncation at every
   prefix length. These are regression tests with fixed vectors, taken from the
   Phase 0 probe output. [CRYPTO §4.7]
3. **Second-source conformance.** `dcbor` 0.25.2 as a `[dev-dependencies]` entry:
   assert that our encoder's output is accepted by `dcbor::CBOR::try_from_data`
   and re-encodes to itself. This gives independent RFC 8949 §4.2 checking
   without putting `dcbor` on the hot path. [CRYPTO §4.4] Note that this makes
   `cargo deny check advisories` fail on `paste 1.0.15` / RUSTSEC-2024-0436
   (unmaintained, no vulnerability, proc-macro helper, dev-only, never in a
   shipped binary) and needs a scoped `ignore` entry in `deny.toml`.
4. **Cross-machine determinism.** The same fixed input vector must encode to the
   same bytes on every target we publish binaries for. This is a CI matrix job,
   not an assumption — `rs_poker`'s BMI2 path is a live example of the same
   source producing two code paths on two peers. [RULES A′4]
5. **A reflection test over every signed type** that fails on a `HashMap`, a
   float, or a missing `#[cbor(array)]`. [CRYPTO §11.6]

### 2.8 Hashes and domain separation

All protocol hashes are BLAKE3 1.8.7. **Why BLAKE3, what else is in the tree and
what is unaudited about it are constructions, and `CRYPTOGRAPHY.md` §3.2 owns
them** (D-011 rule 1). The four-reason list and the audit limitation that stood
here were a copy of that section and are deleted; nothing in this document
depends on them. One consequence is wire-visible and is therefore stated here: if
the mental-poker proof system mandates a specific hash for Fiat–Shamir, that
mandate wins for those bytes, and BLAKE3 governs only the hashes **this document
defines** — the register below and nothing else.

**Every protocol hash is domain-separated and length-prefixed.** Concatenating
variable-length fields without length prefixes is ambiguous: `"AB" ‖ "C"` and
`"A" ‖ "BC"` are the same byte string, so two different logical events could
collide to one transcript hash — a direct §13/§14 break.

```rust
/// The one hash constructor in the protocol.
fn h(domain: &'static str, parts: &[&[u8]]) -> [u8; 32] {
    let key = blake3::derive_key(domain, b"p2p-poker/v1");
    let mut hasher = blake3::Hasher::new_keyed(&key);
    for p in parts {
        hasher.update(&(p.len() as u64).to_be_bytes());  // 8-byte big-endian
        hasher.update(p);
    }
    *hasher.finalize().as_bytes()
}
```

Both properties were asserted in the Phase 0 probe: length prefixing makes
`h(D, ["AB","C"]) != h(D, ["A","BC"])`, and two domains over identical parts
differ. [CRYPTO §3.3]

Complete list of domain strings for `protocol_version = 1`. Adding one is a minor
change; changing or removing one is a major change.

| Domain string | Used for |
|---|---|
| `p2p-poker/v1/event` | the 24-byte `DOMAIN_EVENT` signature prefix (§2.4). **This is a literal prefix, not a `derive_key` domain**, which is why its separators differ from every other row in this table. Its exact bytes are in §2.4 and §13. |
| `p2p-poker v1 transcript` | `event_hash` (§3.2) |
| `p2p-poker v1 stage` | `stage_hash` (§3.2) |
| `p2p-poker v1 genesis` | the genesis hash of each chain (§3.1) |
| `p2p-poker v1 abort-terminal` | `ABORT_TERMINAL(k)`, the terminal value of an aborted chain (§3.1) |
| `p2p-poker v1 state` | `STATE_HASH` (§6) |
| `p2p-poker v1 roster` | `roster_hash` (§3.1) |
| `p2p-poker v1 rng-commit` | `RNG_COMMIT` commitment (§4.4) |
| `p2p-poker v1 rng-beacon` | the combined seed from `RNG_REVEAL` (§4.4) |
| `p2p-poker v1 deck-commit` | `DECK_COMMIT` digest (§4.5) |
| `p2p-poker v1 deck-ctx` | the `ctx` byte string handed to the deck library (§4.5) |
| `p2p-poker v1 session` | `session_id` (§4.3) |
| `p2p-poker v1 connection` | `connection_nonce` (§1.2) |
| `p2p-poker v1 table-id` | reserved; see §4.1 — `table_id` is currently the table public key itself |
| `p2p-poker v1 advert` | reserved; unused in version 1. `advert_hash` is everywhere the `event_hash` of the `LOBBY_TABLE_AD`, never a separate digest. |
| `p2p-poker v1 timeout-cert` | the certificate subject digest (§8.3) |

**This register is the single register for the whole corpus.** A document that
needs a domain string takes it from here; a document that invents one has a bug.

**Retired, never valid.** These strings appeared in drafts of other documents and
are not domain strings of this protocol. A receiver that ever sees a hash
constructed under one of them is looking at a non-conforming implementation.

| Retired string | Where it appeared | Replaced by |
|---|---|---|
| `p2p-poker v1 rng-seed` | `CRYPTOGRAPHY.md` §7.3 | `p2p-poker v1 rng-beacon` |
| `p2p-poker/seat-beacon/v1` | `STATE_MACHINE.md` §7.9 | `p2p-poker v1 rng-beacon` |
| `p2p-poker v1 event` (spaces) | this document's own §2.8 table | `p2p-poker/v1/event`, and it is a literal prefix, not a `derive_key` domain (§2.4, B-4) |

---

## 3. The hash-chain transcript (`SPEC_CS.md` §13)

### 3.1 Chains, stages and genesis

A **chain** is the ordered sequence of events for one `(table_id, hand_id)`.
`hand_id = 0` is the *setup chain*, covering everything from `JOIN_REQUEST` to
`TABLE_READY` plus the seating beacon. `hand_id = 1, 2, …` are the hands.

Chains are linked end to end, so the whole table session is one hash chain:

```
GENESIS(0) → setup chain → TERMINAL(0) ─┐
                                        ├→ GENESIS(1) → hand 1 → TERMINAL(1) ─┐
                                                                              ├→ GENESIS(2) → …
```

```
GENESIS(0) = h("p2p-poker v1 genesis",
               [ u16_be(protocol_version), table_id, u64_be(0),
                 table_public_key, advert_hash, ZERO32 ])

GENESIS(k) = h("p2p-poker v1 genesis",                              for k >= 1
               [ u16_be(protocol_version), table_id, u64_be(k),
                 session_id, roster_hash(k), TERMINAL(k-1) ])

roster_hash(k) = h("p2p-poker v1 roster",
                   [ for each seat s in ascending seat index:
                       u8(s) || app_public_key[s] || u64_be(stack_at_hand_start[s])
                       || u8(seat_flags[s]) ])
```

`advert_hash` is the `event_hash` of the `LOBBY_TABLE_AD` the participants joined
under, so the agreed table parameters are bound into the very first link — every
participant provably joined the same advertised game, which is what §4 of the spec
means by "so that everyone agrees on them before the first hand is dealt".

**`TERMINAL(k)`, normatively, and this is the disposition of P3 and G1.** A chain
ends in exactly one of two ways and each has its own terminal value:

```
TERMINAL(k) = stage_hash of the HAND_COMPLETE stage          the hand was decided
            = ABORT_TERMINAL(k)                              the hand was aborted

ABORT_TERMINAL(k) = h("p2p-poker v1 abort-terminal",
                      [ u16_be(protocol_version), table_id, u64_be(k),
                        GENESIS(k) ])
```

`TERMINAL(0)` for the setup chain is the `stage_hash` of the `TABLE_READY` stage.

**Why an aborted hand's terminal is a function of its genesis and of nothing
else.** `GENESIS(k+1)` is a function of `TERMINAL(k)`, so if two peers derive
different `TERMINAL(k)` the table cannot start another hand — which is exactly
the deadlock P3 named and the fork G1 created. An abort is, by definition, the
outcome in which the peers **could not agree about the middle of the hand**:
which stage stalled, which contributions arrived, which seat was silent. Deriving
`TERMINAL(k)` from any of those quantities derives it from the one thing that is
in dispute. `GENESIS(k)` is not in dispute: every peer agreed it before the hand
started, it is the link that every event of hand `k` chains to, and it carries
`session_id`, `roster_hash(k)` and `TERMINAL(k-1)` transitively. So on the abort
path `TERMINAL(k)` is a function of state every peer already agrees on, exactly
as the commission requires, and `GENESIS(k+1)` exists no matter where, when, or
by whom the hand was aborted.

Chip conservation carries the rest: an abort restores every stack to its
start-of-hand value (D-010, §4.10), so `roster_hash(k+1)`'s stack vector equals
`roster_hash(k)`'s and no peer needs the aborted hand's content to build the next
genesis.

**The cost, stated rather than hidden.** An aborted hand's events are bound to
`GENESIS(k)` and to each other, and each is individually signed, so none of them
can be forged or altered. But they are **not** bound into `GENESIS(k+1)`, so a
party replaying the session chain cannot tell from the chain alone whether an
aborted hand's transcript is complete. That is the price of not deriving a
terminal hash from a disputed quantity, and it costs nothing that depends on it:
an aborted hand moves no chips, awards no pot and changes no stack, so no later
value is computed from its content. `HAND_COMPLETE` — the only path on which a
hand's content changes anything — keeps the ordinary `stage_hash` and keeps the
full binding.

`session_id` is defined in §4.3. Because it is inside `GENESIS(k)` and every event
of hand `k` chains transitively to `GENESIS(k)`, every event is bound to the
session without needing a `session_nonce` field in the envelope. That satisfies
`SPEC_CS.md` §14's "session nonce" requirement structurally.

**The chain is *not* per-sender.** A per-sender chain would let a peer's history
be reordered relative to another's. It is one chain per hand, shared.

### 3.2 Stages: single-writer, collective, and witness-independent terminal

The `sequence` field is the **stage index**, starting at 0 within each chain. It
is shared by every event of the same stage.

Each stage has a *type* and a *required emitter set*, both of which every peer
derives deterministically from the state after the previous stage. **Three
shapes**, the third of which is used by exactly one message type and is the
disposition of P3:

**The stage-kind principle, normative.** Which shape a stage takes is not a free
choice and is not decided per message type by taste:

> **A stage whose body is a pure function of the state before it is *collective*.
> A stage whose body carries a choice its emitter is entitled to make is
> *single-writer*.**

The reason is `SPEC_CS.md` §4: a single writer over a derived body holds a veto
over a transition that the spec requires to be automatic and deterministic, which
makes that writer an authority. A collective derived stage costs `n` copies of a
small message instead of one and removes the veto. That is the trade and it is
taken. Any future message type is classified by this rule and by nothing else.

There is one exception and it is not a taste exception either: a stage that
exists **because** a required emitter is silent cannot have a required emitter
set, and that is the third shape below.

Applying it: `HAND_INIT`, `HAND_COMPLETE` and `TIMEOUT_CERT` are **collective**,
because every field of each is derived. `HAND_ABORT` is
**witness-independent terminal**. `SHUFFLE_STEP`,
`SHUFFLE_PROOF`, every `ACTION_*`, and the voluntary `PLAYER_SIT_OUT` /
`PLAYER_SIT_IN` / `PLAYER_LEAVE` are single-writer, because the permutation, the
betting decision and the seat decision are genuine choices.

**Single-writer stage.** Exactly one seat may emit exactly one event. Examples:
every `ACTION_*`, `SHUFFLE_STEP`, `SHUFFLE_PROOF`, `PLAYER_SIT_OUT`.

```
stage_hash(s) = h("p2p-poker v1 stage",
                  [ u64_be(s), u16_be(stage_type), u8(writer_seat), event_hash ])
```

**Collective stage.** A fixed set `R` of seats each emit exactly one event, and
the stage is complete only when every seat in `R` has been heard. Examples:
`TABLE_READY`, `RNG_COMMIT`, `HAND_INIT`, `DECK_INIT`, `DEAL_PRIVATE`,
`BOARD_REVEAL`, `STATE_HASH`, `TIMEOUT_CERT`, `HAND_COMPLETE`.

```
stage_hash(s) = h("p2p-poker v1 stage",
                  [ u64_be(s), u16_be(stage_type),
                    for each seat s_i in R in ascending seat index:
                        u8(s_i) || event_hash(s_i) ])
```

**Witness-independent terminal stage — normative, and the disposition of P3.**
Used by `HAND_ABORT` and by nothing else.

> A witness-independent terminal stage has **no required emitter set**. Any seat
> of the hand's `HAND_INIT` set may emit its copy; every such seat normally does;
> and the stage **closes at a receiver on the first copy that verifies and passes
> §4.10's acceptance gate**. Waiting for a second copy is never required and a
> silent seat never blocks it.
>
> It is the **last** stage of its chain. Nothing chains from it, so it has **no
> `stage_hash`** — it is the one stage in this protocol that has none, and it
> needs none, because `TERMINAL(k)` on the abort path is `ABORT_TERMINAL(k)`
> (§3.1), a function of `GENESIS(k)` alone.
>
> Its `sequence` is the index of the stage that stalled: the successor of the last
> stage complete **at the emitter**, chained from that stage's `stage_hash`. Two
> peers that disagree about which stage stalled therefore emit at two different
> `sequence` values, and that disagreement is harmless — it changes nothing that
> anything downstream reads, because `ABORT_TERMINAL(k)` does not contain
> `sequence`.

**Why P3 could not be settled any other way.** The abort's old required emitter
set was "the `HAND_INIT` set minus the seats this abort attributes". On the
`hand_deadline_ms` path the abort attributes nobody (§8.4), so the set contained
the silent seat whose silence is the reason the abort exists: the stage could not
complete, `TERMINAL(k)` was never defined, and `GENESIS(k+1)` never existed.
Deriving the set from *who was heard* instead was the other candidate, and it
forks — peers that heard different subsets derive different sets. A stage with no
set at all disposes of both horns, and it costs nothing, because every field of
the abort body is a function of `GENESIS(k)` and of the receiver's own timer, so
there is nothing a witness set would have been protecting.

Every event of stage `s` carries `previous_event_hash = stage_hash(s-1)`, and
`stage_hash(-1) = GENESIS(hand_id)`.

**Which events enter a `stage_hash`.** `stage_hash(s)` is computed over the
events of `event_class = 0` when the stage closes normally, and over the
certificates of `event_class = 2` when the stage closes by certificate (§8.3).
Votes (`event_class = 1`) never enter a `stage_hash`; they are carried inside the
certificate. A terminal `HAND_ABORT` enters no `stage_hash` at all, because the
stage it closes has none. A stage therefore has exactly one closing form in the
chain, even though a vote and a real event may both exist for the same
`sequence` — and, at an abandoned stage, a partial set of contributions and the
abort that abandons it may both exist for the same `sequence` too. Those are two
slots, not one, because `event_type` is in §5.2.1's key.

```
event_hash = h("p2p-poker v1 transcript", [ body_bytes ])
```

**`event_hash` deliberately excludes the signature.** Ed25519 signatures are
deterministic per RFC 8032, but a malicious signer can choose a different nonce
and produce a second valid signature over the same body. Hashing only the body
means that cannot fork the chain; two signatures over one body are one event.

**Why collective stages exist.** The alternative — a strictly linear single-writer
chain everywhere — would serialise phases that are naturally parallel. Six-handed,
the reveal traffic alone (`DEAL_PRIVATE` plus three `BOARD_REVEAL` streets) would
become 24 extra serial round trips — **an estimated** ~1.2 s of pure latency **at
an assumed 100 ms RTT**, on top of the ~420 ms of measured crypto [MENTAL §5.1].
The latency half of that figure is an estimate, not a measurement; no
two-network test has been run, and `CRYPTOGRAPHY.md` §6.5 holds the measurement
plan and the phase in which each of `SPEC_CS.md` §33's targets is measured.
Collective stages cost
nothing in determinism — the required set and the ordering rule are both fixed
functions of state — and they let the whole phase complete in one round trip.

A stage does not advance until it is complete. That is the synchronisation point
the deadline machinery of §8 attaches to.

**`SHUFFLE_STEP` and `SHUFFLE_PROOF` are two consecutive single-writer stages by
the same seat.** They are usually written into the same TCP/QUIC send, but they
are two chain links, because `SPEC_CS.md` §16 names them separately and because a
deck must never be usable before its proof has verified. The rule is explicit:
*the output deck of `SHUFFLE_STEP` at stage `s` may not be used as the input to
anything until `SHUFFLE_PROOF` at stage `s+1` has verified.* If the proof fails,
`INVALID_SHUFFLE_PROOF` is raised (`SPEC_CS.md` §8), the hand stops, and the peer
is attributed.

### 3.3 What is in the transcript

Everything that a verifier needs and nothing that could open a card.

**In:**

* every `SignedEvent` of the chain, with its signature, in stage order;
* the aggregate mental-poker public key and every player's per-hand public key
  with its Schnorr ownership proof (`DECK_INIT`);
* every masked deck and every Bayer–Groth shuffle proof (`SHUFFLE_STEP`,
  `SHUFFLE_PROOF`) — 3432 B and 5547 B respectively for a 52-card deck
  [MENTAL §5.1];
* every reveal token published, with its Chaum–Pedersen DLEQ proof — but see
  §3.4 for exactly which tokens those are;
* every betting action, with the actor, the amount, and the stage;
* every `STATE_HASH` and `STATE_ACK`;
* every timeout vote and certificate;
* the final `HAND_COMPLETE` or `HAND_ABORT` with the per-seat chip deltas.

**Kept beside the transcript, but not part of it:** every `DISPUTE` received for
the hand. A dispute is unchained (§2.3, §4.9), so it occupies no stage and enters
no `stage_hash`; it is stored alongside the chain because §6.4's divergence report
needs it and because a dispute's *contents* — an `EquivocationProof`, a signed
`STATE_HASH` — are verifiable on their own. Nothing about the chain's integrity
depends on which disputes were kept.

**Out — never, under any circumstance:**

* any player's mental-poker secret key `sk_i`;
* any shuffle permutation or masking randomness;
* an `RNG_COMMIT` pre-image before its `RNG_REVEAL` stage has opened;
* a player's **own** reveal token for their **own** hole card, unless the rules
  have forced a showdown reveal;
* any decrypted card value that the rules have not opened;
* private keys of any kind, the profile passphrase, the DEK or KEK
  (`SPEC_CS.md` §21).

### 3.4 Why no secret that could reconstruct another player's cards may enter

This is the load-bearing paragraph of the whole design, so it is stated
mechanically rather than as a principle.

A card at deck index `i` is an ElGamal ciphertext `(c1, c2)` under the aggregate
public key `apk = Σ pk_j` over all `n` parties to the hand. Opening it requires
the **aggregate reveal token** `Σ_j (sk_j · c1)`, i.e. one token from every one of
the `n` parties. With `n-1` tokens the plaintext is information-theoretically no
closer than with zero, which was verified empirically: with two players, either
single token alone yields `None` from the open operation, and only both together
yield the card. [MENTAL §4.1, tests T7c/T7d/T7e]

So the rule that keeps hole cards private is exactly one counting rule:

> **For any deck index that the poker rules have not opened, the transcript must
> contain at most `n-1` reveal tokens.**

Everything follows from it.

This counting rule is the **only** basis on which hole-card privacy is claimed,
and `DEAL_PRIVATE` is a broadcast (§4.6). `CRYPTOGRAPHY.md` §2.7 and §2.9
described it as point-to-point and rested privacy on "the other seats receive no
token at all"; that reason is false under the shipped design and was corrected to
match this section (C-4). Nothing here changed.

* **Hole cards.** For seat `P`'s hole card at index `i`, `DEAL_PRIVATE` publishes
  the tokens of all seats *except* `P`. That is `n-1` tokens: everyone can see
  them, and nobody but `P` can complete the sum. `P` adds its own token locally
  and reads its card. Broadcasting the `n-1` tokens is therefore not a leak, and
  it has a large benefit — the transcript already contains everything needed for a
  later showdown, so a showdown is `P` publishing one token, not a fresh round.
* **Mucking becomes free.** A player who does not want to show simply never
  publishes its own token. Nobody can compute it. This is why the mucking
  question of [RULES A8] is a *policy* question here and not a cryptographic one.
* **Board cards.** The flop indices get all `n` tokens at the flop stage and not
  before. Publishing a token for a turn or river index during the flop stage is a
  detectable protocol violation and is rejected and attributed, even though a
  single early token leaks nothing by itself — because it lowers the coalition
  size needed to open the card early from `n` to `n-1`.
* **The residual coalition.** `n-1` colluding players can open any card the
  remaining honest player can open, at the moment that player publishes. That is
  inherent to `n`-of-`n` and no protocol change fixes it; with one honest player
  left there is nothing to protect. `SPEC_CS.md` §18 already places out-of-band
  collusion outside what cryptography can solve.
* **Why not `t`-of-`n`.** The obvious cure for the disconnect problem is a
  threshold scheme so `t < n` players can finish a hand without the absent one.
  That is **forbidden**: with `t < n`, any `t` colluding players can decrypt
  *every* hole card at the table. It directly violates `SPEC_CS.md` §35 and §19's
  own instruction never to trade disconnect robustness for early decryption.
  [MENTAL §8] D-005 reaches the same conclusion from the rules side.

The second class of secret is the shuffle. A Bayer–Groth proof is zero-knowledge:
it proves the output deck is a permutation and re-randomisation of the input deck
without revealing the permutation. [MENTAL §2.2] The permutation and the masking
scalars are never transmitted, so the transcript proves the shuffle was honest
without letting anyone replay it.

The third is the RNG beacon. `RNG_COMMIT` publishes `h("p2p-poker v1 rng-commit",
[r_i, salt_i])` and nothing else; `r_i` and `salt_i` appear only in the
`RNG_REVEAL` stage, which is a single collective stage that closes atomically, so
no player learns another's contribution while still able to change its own.

### 3.5 What a verifier can check after the hand

Given only the transcript, the table advertisement, and **an implementation of the
engine — its own**, since no canonical reference engine is defined anywhere in
this corpus (§6.3 case (c), OQ-A) — an independent third party who was never at
the table can check all of the following, with no secret input:

1. **Structure.** Every event's canonicality gate passes; every signature verifies
   under the `sender_public_key` in its own body with `verify_strict`; every
   `previous_event_hash` equals the computed `stage_hash` of the preceding stage;
   `GENESIS(k)` chains to `TERMINAL(k-1)`, where `TERMINAL` is §3.1's — the
   `HAND_COMPLETE` stage's `stage_hash` on a decided hand, and `ABORT_TERMINAL(k-1)`
   on an aborted one, which a verifier recomputes from `GENESIS(k-1)` alone. The
   terminal `HAND_ABORT` of an aborted hand has no `stage_hash` and is checked as
   a signed event chained to the last complete stage of its own chain, not as a
   link into the next.
2. **Authorship and order.** Who performed each action and in what order — this is
   `SPEC_CS.md` §13's first two requirements, and it holds because the emitter set
   of every stage is a deterministic function of prior state.
3. **History integrity.** Nobody changed history: any alteration of any body
   changes its `event_hash`, hence the `stage_hash`, hence every subsequent
   `previous_event_hash`.
4. **Shuffle validity.** Every Bayer–Groth proof verifies against its stated input
   and output decks, so no card was added, removed, or altered during any shuffle
   (`SPEC_CS.md` §8). Verified empirically against a deck with a duplicated card
   and an honest proof: rejected. [MENTAL §4.1, test T4]
5. **Deck consistency.** The revealed board cards decrypt from the committed final
   deck, and the deck-index → role map (§4.5) was fixed before the shuffle chain
   started, so nobody chose after the fact which index was "the flop".
6. **Reveal validity.** Every reveal token carries a Chaum–Pedersen DLEQ proof
   that it was computed with the same `sk_i` as the published `pk_i`. A token
   verified against the wrong public key, or replayed onto a different card, is
   rejected. [MENTAL §4.1, tests T7a/T7b]
7. **Rule correctness.** Re-running the deterministic engine over the action
   sequence reproduces every `STATE_HASH` in the transcript, the pot and side-pot
   construction, the winners of each pot, the odd-chip distribution, and the final
   chip deltas of `HAND_COMPLETE`. Chip conservation
   (`sum(stacks) == total_chips_in_play`) holds at every step. [RULES A0, A7, A8]
8. **Attribution of failure, where attribution exists.** If the hand ended in
   `HAND_ABORT` with a non-empty `attributed` set, which seat failed to publish
   which event at which stage, proved by the timeout certificate of every seat in
   `V(subject)`. This is available for a **single** silent seat and only where
   `|V(subject)| >= 2` (§8.3, D-008). Where `|V| < 2` — which at two dealt-in
   seats is always — there is no attribution at all, and when two or more seats
   stop at once there is none either (§8.4, Q-02); in both cases the abort
   carries `attributed = []` and the transcript shows what was expected and not
   served without naming a culprit. A verifier checks `|V|` and the exclusion
   chain itself (§8.5); it must never infer either from the seat count.

   **What a verifier learns from this check is a fact, not a disposition
   (D-010).** No chip moved on the abort whatever `attributed` contains, so
   "which seat failed" is a statement about the transcript and never an
   explanation of the final stacks — those are the start-of-hand stacks in every
   case. A verifier that reads attribution as an accounting entry has read this
   version's `HAND_ABORT` as the previous version's.

All of this requires that somebody kept the transcript. Whether it is persisted
to the profile directory by default is **OPEN QUESTION Q-06**; without
persistence there is no artefact at all, and §6.3 case (c)'s diagnostic claim has nothing
behind it.

What a verifier **cannot** check, stated because §36 forbids smoothing this over:

* **That its result is anyone else's result.** Checks 1–6 are engine-independent:
  they are signatures, hashes and proofs, and any two correct implementations
  agree on them. Check 7 is not. It re-runs *an* engine, and the corpus defines no
  canonical one, no versioning rule that ties one to `protocol_version`, and no
  procedure by which two parties establish they ran the same one. So two verifiers
  disagreeing about check 7 reproduces §6.3 case (c) offline instead of resolving
  it, and a third party's verdict binds nobody. This is **OQ-A** (§12), and until
  it is settled a verifier's rule-correctness result is a diagnosis, not an
  adjudication.

* **Whether a mucked hand was better than the winner's**, if the table's
  `showdown_policy` permits mucking. The transcript then supports the weaker but
  precise claim: *the award was correct given the set of players who did not
  forfeit*. Under `MANDATORY_REVEAL` this gap does not exist. This is exactly the
  trade [RULES A8] identified and it is **OPEN QUESTION Q-01**.
* **Whether the deck was random rather than merely unpredictable-to-each-party.**
  The composition of `n` permutations is uniform as long as at least one player is
  honest; if all `n` collude there is no honest randomness and no proof can create
  it. [MENTAL §6]
* **Anything about out-of-band collusion, screen sharing, malware on a player's
  own machine, multi-accounting, or coercion.** `SPEC_CS.md` §18.
* **That a player's client is not leaking their own cards to a confederate.**

---

## 4. The message catalogue

39 message types. For each: its `event_type` code, its channel, who may emit it,
who receives it, exactly when it is legal, its payload fields with types and
limits, and what a receiver must check before acting.

### 4.0 Universal validation, and its order

Every receiver runs this sequence on every incoming `SignedEvent`, in this order,
on every channel. The order is normative: it puts cheap checks before expensive
ones so a hostile peer cannot make us do work, and it puts the canonicality gate
before the signature so two encodings of one event cannot both be accepted.

| # | Check | Failure |
|---|---|---|
| 1 | Frame length ≤ the channel's cap (§9) | drop, rate-limit source |
| 2 | Decode `SignedEvent`, canonicality gate | drop, count against sender |
| 3 | `signature` is exactly 64 bytes; `body` ≤ cap | drop |
| 4 | Decode `EventBody` from `body` bytes, canonicality gate | drop |
| 5 | `protocol_version` equals the session's pinned version | drop |
| 6 | `event_type` is known **and** legal on this channel | drop |
| 7 | `payload` length ≤ the per-type cap of §9 | drop |
| 8 | Sender is a permitted emitter in this context (roster member for a table event; any peer for a lobby event) | drop |
| 9 | `verify_strict(sender_public_key, TO_BE_SIGNED, signature)` | **protocol violation**, attributable |
| 10 | `chain_scope` matches the `event_type`'s entry in §4.11, and if `chain_scope == 0` the envelope carries the sentinels of §2.3 exactly | drop |
| 10a | Anti-replay: for a chained event, look up **`slot(E)` exactly as §5.2.1 defines it** — this step reproduces no part of that tuple and reads it whole; for an unchained event, the per-type rule in §5.2.1's box | violation or duplicate-drop |
| 11 | Decode the payload struct, canonicality gate, per-field range checks | violation |
| 12 | Stage legality: is this `event_type` from this seat expected at this `sequence`? **And the stage-contribution rule: a seat that has already contributed to a stage may not contribute to it again under a different `event_type`** — the one exception is the terminal `HAND_ABORT` of §4.10, which by construction lands at a stage its emitter has usually already contributed to, and which is also the one event exempt from step 10a's chain-position rule (§4.10) | violation |
| 13 | Semantic legality: the poker engine re-validates the action from scratch | violation |
| 14 | Cryptographic proof verification (shuffle proof ≈ 42 ms, DLEQ ≈ 0.1 ms) | violation, `INVALID_SHUFFLE_PROOF` |
| 15 | Apply to state | — |

Steps 1–8 are pure parsing and cost microseconds. Step 9 is one Ed25519
verification, and **no figure for it has been measured** — signature-verification
throughput is one of `SPEC_CS.md` §33's seven profiling targets, it is scheduled
for Phase 4, and the per-hand event count it must be multiplied by comes from §3.
`CRYPTOGRAPHY.md` §6.5 carries the target table. Step 14 is the only step known
to be expensive, at roughly 42 ms for a 52-card
Bayer–Groth proof [MENTAL §5.1], and it is reached only for an event that is
already signed by a roster member and expected at exactly this stage — so an
outsider cannot make us verify a proof at all.

Steps 14 and 15 must not run on the GUI thread (`SPEC_CS.md` §33). [MENTAL §5.4]

"**Protocol violation**" is a defined outcome, not a synonym for "error": the
offending `SignedEvent` is retained as evidence, the hand stops, and the sender is
attributed in a `DISPUTE`. **Nothing further follows automatically (D-010).**

The clause deleted from that sentence read: *"and — for a signature or proof
failure — the peer is added to `libp2p::allow_block_list`, which is always
compiled in and needs no cargo feature."* That is an eviction applied by the
protocol on the strength of an attribution, which D-010 point 3 forbids, so it
goes. What survives is the resource bound: §9.5 may rate-limit or disconnect a
source that floods malformed or unverifiable frames, keyed on **volume**, not on
fault, and identical for a buggy peer and a hostile one. It is not a sanction for
cheating and it never keys on `attributed`. Block-listing an identity is a
**user** decision, made in the UI from evidence the UI shows.

**Normative, and it binds every layer including the transport (D-011 rule 3).**

> **No document in this corpus may block, evict, unseat, refuse or allow-list a
> peer on the strength of a protocol proof, an attribution, a fault record or a
> failed verification.** That includes the transport layer: an
> `EquivocationProof`, an invalid signature and a failed shuffle proof are each
> evidence, and none of them is an input to any peer-blocking decision anywhere.
> `NETWORK_STACK.md` §6.6 and §11.5 assert the opposite today and are wrong;
> a block list may be present and compiled in, but its only trigger is the
> **user**. §9.5's volume-keyed ladder is untouched by this and is the whole of
> what remains automatic.

### 4.1 `table_id`, and who has authority

`table_id` is 32 bytes and **is** the table's Ed25519 public key,
`table_public_key`. It appears in the envelope of every **chained** event of that
table; an unchained event carries the sentinel `ZERO32` there and names the table
in its payload instead (§2.3). Consequences:

* a `LOBBY_TABLE_AD` is self-authenticating without needing the envelope's
  `table_id` at all: check its signature against `sender_public_key`, and
  `sender_public_key` **is** the `table_public_key`. No lookup, no
  trust-on-first-use;
* two tables cannot collide unless someone breaks Ed25519;
* the table key is generated fresh per table from `getrandom::SysRng` and is
  discarded when the table closes.

**The table key is an identity, never an authority.** It signs exactly three
things: `LOBBY_TABLE_AD`, `LOBBY_TABLE_REMOVE`, and `JOIN_ACCEPT` / `JOIN_REJECT`.
Once `TABLE_READY` is signed by every seated participant, the roster is frozen and
the table key has no further privilege whatsoever. It signs no hand event, holds
no key share, and cannot arbitrate a dispute. `SPEC_CS.md` §15's prohibition on
"the host is always right" is enforced by the key simply having nothing to say
after seating.

The domain string `p2p-poker v1 table-id` is reserved for a future derived
identifier and is unused in version 1.

### 4.2 Group 0 — session handshake (channel: table stream)

Codes `0x0000`–`0x00FF`.

---

**`0x0001 HELLO`**

*Direction:* each side of a newly opened `/p2p-poker/table/1` stream → the other.
*Legal:* exactly once per stream, as the first message, from each side.
*Envelope:* **unchained** — `chain_scope = 0`, `event_class = 0`,
`table_id = ZERO32`, `hand_id = 0xFFFF_FFFF_FFFF_FFFF`, `sequence = 0`,
`previous_event_hash = ZERO32`, `next_deadline_ms = HANDSHAKE_DEADLINE_MS`.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) protocol_version_min` | `u16` | ≥ 1 |
| `n(1) protocol_version_max` | `u16` | ≥ `protocol_version_min` |
| `n(2) peer_id_self` | `bytes` | ≤ 42 B; the sender's libp2p `PeerId` |
| `n(3) peer_id_remote` | `bytes` | ≤ 42 B; the receiver's libp2p `PeerId` |
| `n(4) nonce` | `bytes[32]` | fresh from `getrandom::SysRng` |
| `n(5) capabilities` | `Vec<bytes>` | ≤ 32 entries, each ≤ 32 B, sorted, unique, `[a-z0-9/._-]` only |
| `n(6) client_name` | `bytes` | ≤ 64 B, valid UTF-8, no control characters. Display only. Never parsed for behaviour. |

*Receiver must validate:* §4.0 steps 1–11; **`peer_id_self` equals the PeerId the
libp2p handshake authenticated for the remote end of this connection** (this is
the whole point of the message); `peer_id_remote` equals our own PeerId;
`protocol_version_max ≥ our min` and `protocol_version_min ≤ our max`;
`capabilities` sorted and unique; no `HELLO` already received on this stream.
Failure closes the stream.

---

**`0x0002 CAPABILITIES`**

*Direction:* both sides, after receiving the peer's `HELLO`.
*Legal:* exactly once per stream, after our `HELLO` and the peer's.
*Envelope:* the same unchained envelope as `HELLO` — `chain_scope = 0`,
`event_class = 0`, `table_id = ZERO32`, `hand_id = 0xFFFF_FFFF_FFFF_FFFF`,
`sequence = 0`, `previous_event_hash = ZERO32`. Ordering within the stream is
enforced by the stream's own state machine, not by `sequence` (§1.2).

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) chosen_protocol_version` | `u16` | must equal `min(our_max, their_max)` |
| `n(1) connection_nonce` | `bytes[32]` | must equal our own computation (§1.2) |
| `n(2) agreed_capabilities` | `Vec<bytes>` | ≤ 32, sorted, unique; must equal the set intersection |

*Receiver must validate:* all three fields recomputed locally and compared. Any
mismatch closes the connection — this is a bug or an attack, and there is no
degraded mode. From here the connection is *established* and may carry table
messages.

### 4.3 Group 2 — table formation (channel: join RPC, except `PLAYER_LIST` and `TABLE_READY`)

Codes `0x0200`–`0x02FF`. Group 1 (lobby) is specified in §7 because it has its own
transport, TTL and anti-spam rules.

`JOIN_REQUEST`, `JOIN_ACCEPT` and `JOIN_REJECT` run over
`request_response::cbor::Behaviour` on `/p2p-poker/join/1` (§1.4). Join is a
one-shot RPC with a natural timeout and free size caps, and it avoids opening a
table stream to a peer that has not been admitted. `PLAYER_LIST` stays on the
table mesh, because it is a broadcast to already-admitted peers, and
`TABLE_READY` stays a collective chained stage on the table mesh.

All three join messages and `PLAYER_LIST` are **unchained** (§2.3):
`chain_scope = 0`, `event_class = 0`, `table_id = ZERO32`,
`hand_id = 0xFFFF_FFFF_FFFF_FFFF`, `previous_event_hash = ZERO32`,
`sequence = 0`. The table each concerns is named in its payload.

---

**`0x0201 JOIN_REQUEST`**

*Direction:* a joining player → the table founder (the holder of `table_id`).
*Legal:* on an established connection, when the referenced advertisement has not
expired and the table has not reached `TABLE_READY`.
*Envelope:* **unchained** — `chain_scope = 0`, `event_class = 0`,
`table_id = ZERO32`, `hand_id = 0xFFFF_FFFF_FFFF_FFFF`, `sequence = 0`,
`previous_event_hash = ZERO32`. The table being joined is named by the payload's
`n(8) table_id`, never by the envelope, so that the join RPC occupies no stage
slot and can never appear in an `EquivocationProof` (§5.2).

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) advert_hash` | `bytes[32]` | `event_hash` of the `LOBBY_TABLE_AD` being joined |
| `n(1) app_public_key` | `bytes[32]` | must equal the envelope's `sender_public_key` |
| `n(2) peer_id` | `bytes` | ≤ 42 B, must equal the connection's authenticated remote PeerId |
| `n(3) display_name` | `bytes` | ≤ 32 B UTF-8, no control characters |
| `n(4) requested_seat` | `Option<u8>` | `< max_players`, or absent for "any" |
| `n(5) password_proof` | `Option<bytes[32]>` | present iff the advert has `password_required`; see below |
| `n(6) buyin` | `u64` | within `[min_buyin, max_buyin]` of the advert |
| `n(7) join_nonce` | `bytes[32]` | fresh; the anti-replay for this RPC, since the envelope's `sequence` is a sentinel |
| `n(8) table_id` | `bytes[32]` | the table being joined; must equal the `table_public_key` of the advert named by `advert_hash` |

`password_proof = h("p2p-poker v1 session", [ password_utf8, table_id,
join_nonce ])`. This is a possession proof, not a secret transfer, and it is
per-join so it does not replay to another table. It is **not** a password
strength mechanism; a weak table password is guessable offline by anyone who sees
one proof, and the UI must say so.

*Receiver must validate:* the advert hash names an advert this founder actually
signed and which has not expired; `n(8) table_id` equals that advert's
`table_public_key`; `app_public_key == sender_public_key`;
`peer_id` matches the connection; the buy-in is in range; the seat is free; the
password proof recomputes; this `app_public_key` is not already seated; the table
is not full; `display_name` is well-formed UTF-8 (and is treated as untrusted
display data forever — it is never an identifier).

---

**`0x0202 JOIN_ACCEPT`**

*Direction:* founder → joiner. Signed by the **table key**
(`sender_public_key == table_public_key`; the envelope's `table_id` is the
unchained sentinel `ZERO32`, so the table's identity here is the signing key).
*Legal:* in response to a valid `JOIN_REQUEST`.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) request_hash` | `bytes[32]` | `event_hash` of the `JOIN_REQUEST` |
| `n(1) seat` | `u8` | `< max_players` |
| `n(2) advert_event` | `bytes` | ≤ `TABLE_AD_SIGNED_MAX` = 1 536 B; the complete `SignedEvent` of the `LOBBY_TABLE_AD`, repeated verbatim so the joiner is not relying on a gossip copy and can re-verify the table key's signature itself |
| `n(3) roster_so_far` | `Vec<SeatEntry>` | ≤ `MAX_SEATS` entries |

`SeatEntry` = `#[cbor(array)] { n(0) seat: u8, n(1) app_public_key: bytes[32],
n(2) peer_id: bytes(≤42), n(3) display_name: bytes(≤32), n(4) buyin: u64 }`.

*Receiver must validate:* `sender_public_key` equals the `table_public_key` of
the advert it asked to join; the signature; that
`advert_event` passes §4.0 in full and its `event_hash` equals the `advert_hash`
the joiner sent; that the seat is not already
taken in `roster_so_far`; that the roster has no duplicate `app_public_key` and no
duplicate `seat`. **The joiner then connects directly to every peer in the roster
and runs the §1.2 handshake with each.** It does not take the founder's word for
who is at the table — see `TABLE_READY`.

---

**`0x0203 JOIN_REJECT`**

*Direction:* founder → joiner. Signed by the table key.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) request_hash` | `bytes[32]` | |
| `n(1) reason` | `u16` | enumerated: `1` table full, `2` seat taken, `3` bad password, `4` buy-in out of range, `5` advert expired, `6` banned, `7` capability mismatch, `8` already seated |
| `n(2) retry_after_ms` | `u32` | ≤ 3 600 000; advisory |

*Receiver must validate:* signature and `request_hash`. The reason code is
advisory: a rejection is never proof of anything, since the founder may lie. The
UI shows it as a claim, not a fact.

---

**`0x0204 PLAYER_LIST`**

*Direction:* founder → every joiner, broadcast on the table mesh. Signed by the
table key.
*Legal:* whenever the roster changes during formation, and once immediately
before `TABLE_READY`.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) roster` | `Vec<SeatEntry>` | ≤ `MAX_SEATS`, sorted by `seat`, unique seats, unique keys |
| `n(1) advert_hash` | `bytes[32]` | |
| `n(2) list_serial` | `u64` | strictly increasing per table |

*Receiver must validate:* signature by the table key; sorted, unique;
`list_serial` strictly greater than the last accepted one (this is the anti-replay
for a message that is not yet in a hash chain); every entry's `app_public_key`
is one this client has completed a §1.2 handshake with, or is one it must now
connect to.

A `PLAYER_LIST` is a **proposal**, not a fact. It becomes fact only when every
listed seat signs `TABLE_READY` over it.

---

**`0x0205 TABLE_READY`**

*Direction:* **collective stage 0 of the setup chain (`hand_id = 0`)**. Every
seated participant emits exactly one, to every other.
*Legal:* once `PLAYER_LIST` names a roster of at least `min_players_to_start` and
this client has an established connection to every other listed seat.
*Envelope:* `hand_id = 0`, `sequence = 0`, `previous_event_hash = GENESIS(0)`.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) roster_hash` | `bytes[32]` | `roster_hash(0)` computed from the `PLAYER_LIST` |
| `n(1) list_serial` | `u64` | the serial being ratified |
| `n(2) advert_hash` | `bytes[32]` | |
| `n(3) my_seat` | `u8` | must equal the sender's seat in that roster |
| `n(4) capability_set` | `Vec<bytes>` | ≤ 32; repeated inside the signed table scope so the roster binds capabilities |

*Receiver must validate:* the sender is in the roster at `my_seat`; `roster_hash`
recomputes; every seat's `capability_set` contains `deck/bs-bg12-secp256k1/1` and
a seat-count capability covering `max_players`. If any seat's capabilities are
insufficient, the table does not start and the founder must re-form it.

**This is the moment the table becomes real.** The set of `n` `TABLE_READY`
events is unanimous ratification of the roster by every participant, so from here
on nobody can be added, removed or reordered without a new table. The founder's
special position ends here.

```
session_id = h("p2p-poker v1 session",
               [ table_id, advert_hash, roster_hash(0),
                 for each seat s ascending: event_hash(TABLE_READY from s) ])
```

Every subsequent hand's `GENESIS(k)` contains `session_id`, so every hand event is
bound to this exact roster ratification. Two tables with the same participants and
the same advertisement still get different `session_id`s, because the `HELLO`
nonces feed the connections and the `join_nonce`s feed the join requests whose
hashes are in the roster chain.

**`session_id` is the object `SPEC_CS.md` §14 and §20 call the *session nonce*.**
There is one name for it, `session_id`, and it is defined here. `session_nonce`
is **not** a separate field, is not carried in any envelope or payload, and must
not appear as a field name in any document of this corpus (C-2). Every event is
bound to it transitively through `GENESIS(k)`, and the deck proofs are bound to
it directly through `ctx` (§4.5).

**If formation never completes.** If `TABLE_READY` has not completed within
`join_deadline_ms` of the first `JOIN_ACCEPT` — the founder vanished, or too few
players ever arrived — formation is abandoned. Every holder of a `JOIN_ACCEPT`
discards it and its seat reservation; **no chips have moved, because a buy-in
enters the ledger only at the first `HAND_INIT`** (§4.4's `ledger_delta`), and
formation produces no chained event except `TABLE_READY`, which by definition has
not happened. The advertisement is not revoked by anyone —
`LOBBY_TABLE_REMOVE` requires the table key, which is exactly what is missing —
and it disappears from every lobby by `AD_TTL_MS` with no cooperation from
anybody. A client that holds a `JOIN_ACCEPT` for a table whose advert has expired
must not display the table as joinable. `STATE_MACHINE.md` T4 reaches the same
terminal state from the engine side; the engine's `TableClosed` and the lobby's
TTL expiry are the same event seen from two layers.

### 4.4 Group 3 — hand setup and the deck (channel: table mesh)

Codes `0x0300`–`0x03FF`.

**Recorded spec deviation — where `RNG_COMMIT` / `RNG_REVEAL` sit, and why.**
This is a deviation from a binding spec section and is recorded as such, in the
same form `CRYPTOGRAPHY.md` §7.2 uses for the `OsRng` rename. The single
deviation register for the corpus is `THREAT_MODEL.md` §9.1.1; this is entry 2 in
it. `SPEC_CS.md` §16 lists them
after `HAND_INIT`. They are placed in the **setup chain** instead, run exactly
once per table, for seat assignment and the initial button position only. The
reason is a Phase 0 finding: *the shuffle chain is already the per-hand
distributed randomness.* Each player applies a secret permutation and fresh
re-randomisation drawn from the OS CSPRNG, so the final order is the composition
of all `n` permutations and is uniform as long as one player is honest — no
player and no coalition of `n-1` controls it. The last shuffler gains nothing,
because it sees only ElGamal ciphertexts under the aggregate key and has no
information about which slot holds which card, so there is nothing to bias
towards. [MENTAL §6] A per-hand commit/reveal beacon would add two stages of
latency per hand and buy nothing. The commit/reveal construction is still needed
for the non-deck randomness, which is exactly what it is used for.

---

**`0x0301 RNG_COMMIT`**

*Direction:* collective stage 1 of the setup chain; every seated participant.
*Legal:* after `TABLE_READY` is complete.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) commitment` | `bytes[32]` | the canonical five-part commitment below |

```
commitment_i = h("p2p-poker v1 rng-commit",
                 [ table_id, session_id, committer_app_public_key, r_i, salt_i ])
```

This is the canonical form for the whole corpus (C-2). The earlier two-part
`h(..., [r_i, salt_i])` is deleted: it bound neither the table, nor the session,
nor the committer, so a commitment could be lifted from one table into another.

`r_i` and `salt_i` are each 32 bytes from `getrandom::SysRng`. `OsRng` no longer
exists in the current `rand` / `rand_core` line; the OS source is
`getrandom::SysRng`.

**No absence is claimed here, and the sentence that claimed one is deleted (M3,
D-009 rule 3).** It read: *"`SmallRng` is absent because rand 0.8's `small_rng`
feature is not enabled."* The reason clause was scoped to the 0.8 major and the
claim was not, and the claim is false. `rand 0.9.5` lists `small_rng` among its
**default** features, and four dependencies take `rand` with defaults —
`hickory-proto` and `hickory-resolver` (reached through libp2p's `dns`),
`igd-next` (through `upnp`) and `yamux` — so **`SmallRng` *is* compiled into the
binary and no choice open to this project removes it** short of dropping libp2p
features the client needs. `CRYPTOGRAPHY.md` §7.2 and `THREAT_MODEL.md` A7 state
the same thing in the same terms; this was the third document and the last
holdout. Separately and still true: **`rand 0.8.8` is also in the runtime tree**,
via `ark-std 0.5.0` with `rand_chacha 0.3.1` and `rand_core 0.6.4`, and `StdRng`
is used by `ziffle` for deterministic nothing-up-my-sleeve public constants only.
Three `rand` majors are present altogether.

What is claimed instead is the **discipline**, because a discipline is
enforceable and an absence in somebody else's dependency graph is not:

> *This crate's own code draws cryptographic randomness only from the operating
> system's CSPRNG.*

It is enforced mechanically rather than by review. `src/security/rng.rs` is the
single source — `pub fn fill(dest: &mut [u8])` over `getrandom::SysRng` — and its
`tests::our_own_code_uses_no_generator_but_the_os_one` walks every `.rs` file
under `src/` on each test run and **fails the build** on `SmallRng`, `StdRng`,
`thread_rng`, `from_seed`, `seed_from_u64` or `rand::rngs`. That file is the one
exemption, because it must name the identifiers in order to forbid them. The gate
was verified to bite by injecting a violation and watching the run fail with
`SPEC_CS.md section 7 forbids these generators for cryptographic use; draw from
security::rng::fill instead`, then pass again once the violation was removed — a
gate that has never been seen to fail is not a gate. `SPEC_CS.md` §7 is therefore
enforced by that test and by `CRYPTOGRAPHY.md` §12 item 6's CI lint, and **no
structural guarantee from the dependency graph is claimed or available**.
[CRYPTO §1] The `OsRng` rename is deviation 1 in `THREAT_MODEL.md` §9.1.1's
register and is recorded in `docs/CRYPTOGRAPHY.md` §7.2.

*Receiver must validate:* exactly 32 bytes; exactly one commitment per seat; the
stage is not already complete.

---

**`0x0302 RNG_REVEAL`**

*Direction:* collective stage 2 of the setup chain; every seated participant.
*Legal:* only once the `RNG_COMMIT` stage is **complete**, i.e. every seat's
commitment has been received and chained. This is what makes the commitment
binding: no player sees any `r_j` while still able to change its own.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) r` | `bytes[32]` | |
| `n(1) salt` | `bytes[32]` | |

*Receiver must validate:* the five-part `commitment_i` of stage 1 recomputes from
`(table_id, session_id, that seat's app public key, r, salt)` and equals the
commitment that seat published. A mismatch, or a failure to reveal, is a protocol
violation attributed to that seat, and the table does not start.

```
seed = h("p2p-poker v1 rng-beacon", [ r_1, …, r_n ])     ascending by seat index
```

This is the canonical combine for the whole corpus (C-2). The retired forms are
`CRYPTOGRAPHY.md` §7.3's `p2p-poker v1 rng-seed` and `STATE_MACHINE.md` §7.9's
unprefixed `BLAKE3("p2p-poker/seat-beacon/v1" ‖ …)`; both are listed as never
valid in §2.8. The seed carries no `session_id` part of its own — each `r_s` is
already bound to the session by its stage-1 commitment, and the beacon's own
events are bound to `GENESIS(0)` by the chain.

The seed determines the seat permutation and the initial button position, by a
deterministic rule specified in `STATE_MACHINE.md`. Nothing else. It is not used
for cards.

---

**`0x0303 HAND_INIT`**

*Direction:* **collective stage 0 of chain `hand_id = k`**, `k ≥ 1`. The required
emitter set is every seat that will be `dealt_in`, plus every occupied seat that
is absent or sitting out and therefore posts dead money. Each such seat computes
the byte-identical body from the state after `TERMINAL(k-1)`, signs its own copy
under its own application key, and emits it. There is no writer. (The button
position may still be a dead, empty seat under the TDA dead-button rule.
[RULES A1.3])
*Legal:* immediately after `TERMINAL(k-1)`, with no human input. `SPEC_CS.md` §4
requires the next hand to start automatically and deterministically, not to be
"driven by whoever clicks first". [RULES A9]

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) hand_id` | `u64` | must equal the envelope's `hand_id` |
| `n(1) button_position` | `u8` | `< max_players` |
| `n(2) sb_position` | `u8` | `< max_players` |
| `n(3) bb_seat` | `u8` | `< max_players`, must be an occupied, non-absent seat |
| `n(4) level` | `u16` | |
| `n(5) small_blind` | `u64` | |
| `n(6) big_blind` | `u64` | `== 2 * small_blind` |
| `n(7) ante` | `u64` | `0` in version 1 |
| `n(8) dealt_in` | `Vec<u8>` | ≤ `MAX_SEATS`, ascending, unique; the cryptographic parties to this hand |
| `n(9) stacks` | `Vec<u64>` | one per occupied seat, ascending by seat |
| `n(10) roster_hash` | `bytes[32]` | `roster_hash(k)` |
| `n(11) ledger_delta` | `Vec<(u8, i64)>` | ≤ `MAX_SEATS`, ascending by seat, unique; the per-seat ledger change applied at this hand boundary — positive for a buy-in, negative for a departing stack |

**`HAND_INIT` announces nothing and decides nothing.** Every field is a pure
function of `TERMINAL(k-1)` and the table parameters, so every receiver
recomputes all of them and rejects a copy in which any field differs. This is
exactly why the stage is collective and not single-writer (§3.2's stage-kind
principle): a body with no choices in it must not give one seat a veto over the
hand-to-hand transition that `SPEC_CS.md` §4 requires to be automatic. A seat
that emits a wrong `HAND_INIT` commits an attributable protocol violation, and
its copy simply does not complete the stage for it.

`ledger_delta` is what makes the chip ledger verifiable. `ledger_in` is the sum
of every accepted seat-entry's buy-in and `ledger_out` the sum of every accepted
seat-exit's removed stack; both change **only** at a hand boundary and only
through this field, both are inside `state_hash` (§6.1), and every receiver
recomputes the vector from the accepted join and `PLAYER_LEAVE` events and
rejects a copy that disagrees. This is what turns `STATE_MACHINE.md`'s I1 from
"total chips are constant" — false in cash mode — into the ledger identity
`Σ stack + Σ committed_hand == ledger_in − ledger_out` (C-9). A buy-in enters the
ledger here and nowhere earlier, which is why abandoned table formation moves no
chips (§4.3).

`dealt_in` excludes absent and sitting-out seats per D-005: an absent seat keeps
its stack, pays its blinds and antes as dead money, takes no cards, and is not a
party to the cryptography. It cannot win the blind it posts — a documented,
forced deviation from TDA rules, because any workaround is precisely the
collude-and-disconnect attack `SPEC_CS.md` §19 forbids.

---

**`0x0304 DECK_INIT`**

*Direction:* collective stage 1 of hand `k`; every seat in `dealt_in`.
*Legal:* after `HAND_INIT`.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) hand_public_key` | `bytes` | 33 B compressed secp256k1 point; the per-hand mental-poker `pk_i` |
| `n(1) ownership_proof` | `bytes` | 65 B Schnorr proof of knowledge of `sk_i` |

Sizes are the measured `ziffle` 0.1.0 encodings. [MENTAL §5.1]

*Receiver must validate:* exact lengths; deserialise the point with the
library's validating mode (`Validate::Yes`) so on-curve checks run [MENTAL §7];
the Schnorr proof verifies against `pk_i` **and against the hand's `ctx`** (§4.5);
exactly one entry per `dealt_in` seat. A fresh keypair per hand is mandatory —
reusing one across hands would let a `ctx`-stripped proof migrate.

When the stage completes, every peer derives `apk = Σ pk_i` over the `dealt_in`
seats in ascending seat order, and the canonical unmasked 52-card deck, both by
deterministic rules in `docs/CRYPTOGRAPHY.md`. Neither is transmitted.

---

**`0x0305 SHUFFLE_STEP`**

*Direction:* single-writer stage; the shufflers are the `dealt_in` seats in
ascending seat order, one stage-pair each.
*Legal:* stage `2 + 2j` of hand `k` for the `j`-th shuffler, after the previous
shuffler's `SHUFFLE_PROOF` has verified.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) shuffle_round` | `u8` | `== j`, `< MAX_SEATS` |
| `n(1) deck` | `bytes` | exactly 3432 B for 52 cards (66 B per card) |

---

**`0x0306 SHUFFLE_PROOF`**

*Direction:* single-writer stage `3 + 2j`, same seat as the preceding
`SHUFFLE_STEP`.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) shuffle_round` | `u8` | must equal the preceding `SHUFFLE_STEP`'s |
| `n(1) input_deck_hash` | `bytes[32]` | `h("p2p-poker v1 deck-commit", [input deck bytes])` |
| `n(2) output_deck_hash` | `bytes[32]` | over the preceding `SHUFFLE_STEP`'s deck |
| `n(3) proof` | `bytes` | ≤ 8192 B; 5547 B for a 52-card Bayer–Groth proof |

*Receiver must validate:* `input_deck_hash` matches the deck this receiver
already holds as the shuffler's input, and `output_deck_hash` matches the deck
from stage `2+2j` — this binds the proof to the exact transition and is the
defence against a proof lifted from another hand; then verify the proof itself
against `(input_deck, output_deck, apk, ctx)`.

Failure is `INVALID_SHUFFLE_PROOF`: the hand stops immediately and is not played
on (`SPEC_CS.md` §8), the peer is attributed as the source of the protocol
failure, and a `DISPUTE` carrying the offending event is broadcast.

**Cost, measured.** Proving a 52-card shuffle takes 94–111 ms and verifying takes
37–43 ms in the recommended library, and the numbers are flat in the number of
players because the deck is always 52 cards. A whole hand's own crypto work for
one node is ~195 ms heads-up and ~420 ms six-handed; those are measurements.

**Hand start-up latency is an estimate, not a measurement.** Roughly
`n × (100 ms + one-way latency)`: **an estimated ~340 ms heads-up and ~1.1 s
six-handed, at an assumed 100 ms RTT.** No two-network test has been run
(`NETWORK_STACK.md` §12.12), the figure is one of `SPEC_CS.md` §33's seven
profiling targets, and `CRYPTOGRAPHY.md` §6.5 holds the target table and names
Phase 8 as where it is measured. The estimate must also be revised upward for
C-5: making `HAND_INIT` collective replaces one message with `n` and adds one
collective round trip to hand start-up. [MENTAL §5.1, §5.4]

---

**`0x0307 DECK_COMMIT`**

*Direction:* collective stage, after the last `SHUFFLE_PROOF`; every `dealt_in`
seat.
*Legal:* once every shuffler has produced a verified `SHUFFLE_STEP` /
`SHUFFLE_PROOF` pair.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) final_deck_hash` | `bytes[32]` | `h("p2p-poker v1 deck-commit", [final deck bytes])` |
| `n(1) index_map_hash` | `bytes[32]` | over the canonical deck-index → role map (§4.5) |
| `n(2) apk` | `bytes` | 33 B; the aggregate public key each peer derived |

*Receiver must validate:* all three equal this receiver's own values. Any
disagreement here means two peers hold different decks and the hand cannot
continue; it goes straight to the §6 divergence procedure. This stage is a cheap
barrier that catches deck disagreement *before* any card is opened, which is the
only point at which it is still cheap to catch.

### 4.5 The `ctx` binding and the deck-index map

Two things this protocol must supply that the deck library does not.

**The `ctx` byte string.** The library binds every proof to a caller-supplied
context, and Phase 0 confirmed that proofs do not transfer across different
contexts. But that is only as strong as what we put in it, and getting it wrong
is our bug, not the library's. [MENTAL §9, risk 3] It is:

```
ctx = h("p2p-poker v1 deck-ctx", [
          u16_be(protocol_version),   //  2 B
          table_id,                   // 32 B  the table's Ed25519 public key
          session_id,                 // 32 B  = SPEC_CS.md §14/§20 "session nonce"
          u64_be(hand_id),            //  8 B
          u64_be(sequence),           //  8 B  the chain stage index of the event carrying the proof
          [ u8(shuffle_round) ],      //  1 B  0-based position of this shuffler in the chain;
                                      //       0xFF for every ctx that is not a shuffle step
          sender_public_key           // 32 B  the emitter's application Ed25519 key
      ])
```

where `h` is §2.8's constructor: `blake3::derive_key(domain, b"p2p-poker/v1")` as
the key, then for each part an 8-byte big-endian length prefix followed by the
part. The result is 32 bytes and is what is handed to the deck library. **The
separator is the length prefix; no other separator, delimiter or padding
exists.** Field order is exactly as listed and is part of the protocol.

`shuffle_round = 0xFF` applies to the `ctx` used for `DECK_INIT` ownership proofs
and for reveal-token DLEQ proofs; those are not shuffle-chain steps and must not
share a `ctx` with one.

**This is the normative construction for the whole corpus** and it supersedes
`CRYPTOGRAPHY.md` §6.4's raw-concatenation form, which is deleted (C-2). Hashing
rather than concatenating is what makes the input fixed-length and reuses the one
audited length-prefixed, domain-separated hasher; `ziffle` hashes whatever it is
given (`SHA-256(ctx)`), so a 32-byte input loses nothing. `CRYPTOGRAPHY.md` §6.4
reproduces this block for the reader and does not own it.

Including `sequence`, `shuffle_round` and `sender_public_key` means a proof is
bound not only to the hand but to the exact stage, the exact position in the
shuffle chain, and the exact shuffler. The adversarial suite must contain a test
that replays a valid shuffle proof from hand `n` into hand `n+1` and asserts
rejection, and another that replays seat 2's proof as seat 3's. Whether this
binding is *sufficient* is a separate, still-open question — `CRYPTOGRAPHY.md`
OQ-3 and `THREAT_MODEL.md` OQ3 — which this construction does not answer.

**The deck-index → role map.** This must be fixed *before* the shuffle chain
starts, or a malicious last shuffler could argue after the fact about which index
is "the button's first hole card". [MENTAL §6] It is a pure function of state, so
there is nothing to manipulate:

Let `D = [d_0, …, d_{m-1}]` be `dealt_in` ordered clockwise starting from the
first dealt-in seat strictly clockwise of `button_position` — that is, normal
deal order, small blind first.

| Deck index | Role |
|---|---|
| `0 … m-1` | first hole card of `d_0 … d_{m-1}` |
| `m … 2m-1` | second hole card of `d_0 … d_{m-1}` |
| `2m`, `2m+1`, `2m+2` | flop |
| `2m+3` | turn |
| `2m+4` | river |
| `2m+5 … 51` | unused; no token for these indices is ever legal |

`index_map_hash = h("p2p-poker v1 deck-commit", [ u8(m), for i in 0..2m+5:
u8(i) || u8(role_code(i)) || u8(owner_seat_or_0xFF(i)) ])`.

**There are no burn cards.** A burn exists to defeat physical marked-card and
edge-sorting attacks; there are no physical cards here. A burn that is never
opened is indistinguishable from an unused index, so burning is a no-op that only
consumes indices and adds a place to get the map wrong. This is a deliberate
departure from live procedure, it is entry 3 in the deviation register of
`THREAT_MODEL.md` §9.1.1, and it is not a free per-document choice: a burn costs
a deck position, changes this map, and therefore changes `index_map_hash` inside
`DECK_COMMIT`, so two conforming clients with different maps would produce a
guaranteed `DECK_COMMIT` mismatch every hand — a manufactured §15 dispute that
after §6.3 faults the table. `CRYPTOGRAPHY.md` §2.4 and §2.8 carried a burn
layout and were corrected to reproduce this table (C-3). This section owns the
map; no other document may restate it independently.

The symbol for the dealt-in count is **`m`** in every document of this corpus.

### 4.6 Group 4 — dealing and revealing (channel: table mesh)

Codes `0x0400`–`0x04FF`.

A **reveal contribution** is the pair `(token, dleq_proof)`, 33 B and 98 B in the
recommended library. [MENTAL §5.1] It is always carried as:

```
RevealEntry  #[cbor(array)]
  n(0) deck_index : u8       < 52
  n(1) token      : bytes    33 B
  n(2) proof      : bytes    98 B
```

Entries in any message are sorted ascending by `deck_index` and must be unique;
an unsorted or duplicated list is a canonicality violation and the message is
dropped.

---

**`0x0401 DEAL_PRIVATE`**

*Direction:* collective stage after `DECK_COMMIT`; every `dealt_in` seat, to
every other.
*Legal:* only after the `DECK_COMMIT` stage is complete.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) entries` | `Vec<RevealEntry>` | **exactly** the hole-card indices of every dealt-in seat *other than the sender*: `2(m-1)` entries |

*Receiver must validate:* the index set is **exactly** the required set — no
more, no fewer. Fewer is a failure to cooperate; more means the sender published
a token for its own card (harmless but wrong) or for a board index (an attempt to
open the board early, a violation). Then every DLEQ proof verifies against that
seat's `pk_i` from `DECK_INIT` and against the ciphertext at that index in the
committed final deck.

When the stage completes, each seat holds `m-1` tokens for each of its own two
indices, adds its own, and reads its cards. Every other seat holds `m-1` tokens
for those indices and, by §3.4, learns nothing.

The word "private" in the message name is `SPEC_CS.md` §16's; the message itself
is broadcast. What is private is the *result*, and it is private because one token
is missing, not because the message was.

**Broadcast is the canonical shape and this section owns it.** `CRYPTOGRAPHY.md`
§2.7 and §2.9 specified point-to-point delivery to the card's owner and rested
privacy on "the other seats receive no token at all"; that reason is false under
the shipped design, and both sections were corrected to broadcast and to the
counting argument of §3.4 (C-4). The consequences of broadcast are load-bearing:
the transcript already holds `m-1` tokens per hole index, so a showdown is one
message per revealing seat rather than a fresh round, and mucking is a policy
question rather than a cryptographic one.

---

**`0x0402 BOARD_REVEAL`**

*Direction:* collective stage at the start of each post-flop street; every
`dealt_in` seat, including folded seats.
*Legal:* only when the preceding betting round has closed and the street's
`STATE_ACK` stage is complete. Never earlier.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) street` | `u16` | `3` flop, `4` turn, `5` river |
| `n(1) entries` | `Vec<RevealEntry>` | exactly the indices of that street: 3 for the flop, 1 for the turn, 1 for the river |

*Receiver must validate:* the street matches the engine's current street exactly;
the index set is exactly the street's indices under the committed `index_map`; all
DLEQ proofs verify. **A token for a later street is rejected and attributed**, per
`SPEC_CS.md` §10 and §17's "premature reading of the board".

Folded players are still `dealt_in` and must still publish. That is the price of
`n`-of-`n`: a folded player holds a key share until the hand ends. A folded player
who goes silent stalls the hand exactly as an active one would, and is handled by
§8.

When the stage completes, every peer has all `m` tokens for those indices and
opens the cards. The cards themselves are never transmitted — they are derived
identically by everyone, which is why a receiver can never be shown a
cryptographically unverified card (`SPEC_CS.md` §22).

---

**`0x0403 SHOWDOWN_REVEAL`**

*Direction:* collective stage at showdown; every seat required to show.
*Legal:* only at showdown, and only for the sender's **own** hole-card indices.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) entries` | `Vec<RevealEntry>` | exactly the sender's own two hole-card indices |

*Receiver must validate:* the indices are exactly the sender's own; the proofs
verify; the sender is in the required-to-show set for this showdown.

Because the other `m-1` tokens for those indices are already in the transcript
from `DEAL_PRIVATE`, this one message opens the hand publicly. That is the whole
showdown: 2 × 131 bytes.

The required-to-show set follows the rules, not preference: with at least one
player all-in and betting complete, every live player must show and none may muck
(TDA 16); in a non-all-in showdown the order starts with the last aggressor on the
river, or with the first player to act on the river if it was checked through, and
proceeds clockwise (TDA 17-A). [RULES A8]

---

**`0x0404 SHOWDOWN_MUCK`**

*Direction:* collective stage at showdown, as the alternative to
`SHOWDOWN_REVEAL` from the same seat, so that the collective stage's required set
is still exactly determined.
*Legal:* **only if the table's `showdown_policy` is `TDA_MUCK`.** Under
`MANDATORY_REVEAL`, which is version 1's default, this message is never legal and
is a protocol violation.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) forfeit` | `bool` | must be `true`; irrevocably forfeits all claim to every pot in this hand |

**Exclusivity is normative, not a convention.** A seat emits exactly one of
`SHOWDOWN_REVEAL` and `SHOWDOWN_MUCK` at the showdown stage. They are the one
pair of `event_class = 0` types that share a `sequence`, so a seat that emitted
both would have put two distinct bodies in one slot and manufactured an
`EquivocationProof` against itself (§5.2). The exclusivity is what keeps that
slot at capacity one. It is safe to leave it there, because unlike the deadline
classes no honest interleaving reaches it: nothing in this protocol ever requires
or permits a seat to both show and muck, so a seat that emitted both did
something no rule allows and the resulting proof is the predicate working, not a
false positive. That is the distinction §5.2's property draws, and this pair sits
on the correct side of it.

Whether `TDA_MUCK` is offered at all is **OPEN QUESTION Q-01** (§12). The
mechanism is specified now because it costs nothing to specify and because the
collective-stage required set must be well-defined either way; it is not enabled.

### 4.7 Group 5 — betting actions (channel: table mesh)

Codes `0x0500`–`0x05FF`. Each is a single-writer stage whose writer is
`player_to_act`.

All five share a common prefix so the anti-replay and legality checks are one code
path:

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) street` | `u16` | must equal the engine's current street |
| `n(1) seat` | `u8` | must equal `player_to_act` and the sender's seat |
| `n(2) action_index` | `u32` | count of actions so far in this hand; must equal the engine's |

and then per type:

| Code | Name | Extra fields | Legal when |
|---|---|---|---|
| `0x0501` | `ACTION_CHECK` | — | `to_call == 0` |
| `0x0502` | `ACTION_CALL` | — | `to_call > 0`. Moves `min(to_call, stack)`. `stack <= to_call` is an all-in call for less. |
| `0x0503` | `ACTION_BET` | `n(3) amount: u64` | `current_bet == 0` and (`amount >= big_blind` or `amount == stack`) |
| `0x0504` | `ACTION_RAISE` | `n(3) raise_to: u64` | `current_bet > 0`, `can_reopen(p)`, and (`raise_to >= current_bet + last_full_raise` or `raise_to == committed_this_round[p] + stack[p]`) |
| `0x0505` | `ACTION_FOLD` | — | always |

**`raise_to` is a total commitment for the round, never an increment.** TDA 43-B:
"Without other clarifying information, declaring raise and an amount is the total
bet." Carrying a total on the wire removes an entire class of ambiguity. [RULES A3]

`ACTION_BET`'s `amount` is likewise the total for the round, which on a street
where `current_bet == 0` is the same number.

*Receiver must validate:* everything above, by **re-running the engine locally
from its own state**. `SPEC_CS.md` §11 is explicit that the engine must not trust
that the counterparty sends legal actions. The specific predicates are
`docs/POKER_RULES.md` A3, A4 and A5, in particular

```
can_reopen(p)  ⇔  !acted_this_round[p]
               ∨  (current_bet - committed_this_round[p]) >= last_full_raise
```

which handles the incomplete all-in raise, cumulative short all-ins, and the big
blind's option in one expression, and which was checked against all of TDA's own
published Illustration Addendum examples for rule 47. [RULES A5]

An illegal action is a protocol violation attributable to its signer — **it is not
a state transition, and it never becomes one.** The receiver does not "correct" it.

### 4.8 Group 6 — deadlines (channel: table mesh)

Codes `0x0600`–`0x06FF`. Full semantics in §8.

---

**`0x0601 TIMEOUT_VOTE`**

*Direction:* any seat in the required voter set → all.
*Legal:* only when the local monotonic timer for the subject stage has expired
(§8.2) **and** this client has not accepted a valid event for that stage **from
`subject_seat`**.

**The legality condition is per subject, normatively (M5).** The condition is
*"nothing from `subject_seat` at that stage"*, and it is stated in exactly those
words here, in the receiver check below, and in §8.3. The reading discarded is
the one an earlier revision of this line carried — *"has not accepted any valid
event for that stage"*, i.e. nothing from **anyone**. It is discarded because
every cryptographic stage is collective (§4.11), so at every stage a `kind = 2`
vote could concern, the emitter has by construction already accepted events from
the seats that did contribute; under that reading no `kind = 2` vote is ever
legal, the whole cryptographic-step deadline path is unreachable, and §8.4's
simultaneous-failure section describes a state that cannot arise. A gate that
deletes the machinery it guards is not the gate that was meant. The per-subject
reading is normative; the any-event reading appears nowhere in this corpus except
in this paragraph, which records its withdrawal.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) subject_sequence` | `u64` | the stage that failed to complete |
| `n(1) subject_seat` | `u8` | the seat that failed to emit |
| `n(2) subject_event_type` | `u16` | what was expected from that seat |
| `n(3) parent_event_hash` | `bytes[32]` | `stage_hash(subject_sequence - 1)` |
| `n(4) deadline_ms` | `u32` | the `next_deadline_ms` carried by the parent stage's events |
| `n(5) kind` | `u16` | `1` = action deadline, `2` = cryptographic-step deadline |

*Envelope:* `sequence = subject_sequence`,
`previous_event_hash = parent_event_hash`, `chain_scope = 1` and
**`event_class = 1`**. The class puts the vote in its own slot, so a voter that
has already emitted its own event at a collective stage `s` can vote about stage
`s` without equivocating against itself. A vote *references* the stage it is
about; it does not occupy it.

**The vote's anti-replay slot key includes its subject (D-009 rule 1).** The key
is §5.2.1's, whole; this section reproduces no part of it and adds none. What
matters here is *why* `subject(E)` has an arm for `event_class == 1`, because the
reason is a rule of this section: the class axis alone is not enough — §8.4
specifies two simultaneous subjects at one stage as a normal state, an honest
voter whose timers expired against both seats is *entitled* to emit a vote about
each, and with the subject carried only in the body those two votes would be two
distinct bodies in one capacity-one slot — §5.2's predicate satisfied against an
honest key by behaviour this document describes as ordinary. With the subject in
the key they are two events in two slots and there is no proof to build.

Once the subject is in the key the slot is back to capacity one, because every
remaining field of a vote is a function of `(subject_sequence, subject_seat)`:
`n(2) subject_event_type` is what the stage's emitter set owed that seat,
`n(3) parent_event_hash` is `stage_hash(subject_sequence - 1)`, `n(4) deadline_ms`
is the parent's `next_deadline_ms`, and `n(5) kind` is fixed by the stage — a
stage is either a betting stage or a cryptographic stage, never both, so one
voter can never legitimately hold both kinds against one subject at one stage.
The one field that varies freely is the advisory `emitted_at_unix_ms`, which is
why §5.2's re-emission rule is normative: a peer that must send its vote again
sends the stored bytes and never re-signs.

**There is no mutual exclusion between a vote and an action, and none is
claimed.** A `TIMEOUT_VOTE` is signed by the voter and the action it is about is
signed by the subject: they are two events by two different keys and constitute
no proof against anyone. After the `event_class` discriminator they are not even
syntactically comparable, because they sit in different slots. See §5.2 and §8.3.

*Receiver must validate:* the voter is in the required voter set for that subject
(§8.3); `parent_event_hash` matches this receiver's own `stage_hash`;
`deadline_ms` matches the parent's; the receiver has not itself accepted an event
for that stage **from `subject_seat`** — the same per-subject condition the
emitter gate above states, in the same words.

---

**`0x0602 TIMEOUT_CERT`**

*Direction:* **collective stage**; the required emitter set is `V(subject)`, the
same set that had to vote. Each voter emits its own certificate.
*Legal:* only when `|V(subject)| >= 2` **and** a `TIMEOUT_VOTE` has been collected
from **every** seat in that set. A certificate with `|V| < 2` is **inert** — not
chained, not evidence, no terminating effect, silently ignored. §8.3's boxed
below-the-floor rule is canonical for that and this line restates nothing beyond
the pointer.
*Envelope:* `chain_scope = 1`, **`event_class = 2`**, `sequence` and
`previous_event_hash` as for any event of the subject stage.

**The certificate's slot key includes its subject too, and for the same reason.**
The key is the six-tuple plus the payload's `n(0) subject_digest` (§5.2). The
`subject_digest` is the certificate's own identifier of the subject: it commits
to `subject_seat`, `subject_event_type`, `parent_event_hash`, `deadline_ms` and
`kind` at once, so two certificates by one emitter at one stage are in one slot
if and only if they concern the same subject under the same deadline. The case
that needs it is real rather than hypothetical: at a stage where seats `A` and
`B` have each voted against the other — two honest, mutually partitioned peers is
enough — a third seat whose timers expired against both is entitled to certify
both, and with the subject carried only in the body its two certificates would be
an `EquivocationProof` against it. D-009 rule 1 governs classes 1 and 2 alike;
the rule is *every field that legitimately varies for one signer at one stage
belongs in the key*, and for a certificate that field is the subject.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) subject_digest` | `bytes[32]` | `h("p2p-poker v1 timeout-cert", [u64_be(subject_sequence), u8(subject_seat), u16_be(subject_event_type), parent_event_hash, u32_be(deadline_ms), u16_be(kind)])` |
| `n(1) votes` | `Vec<bytes>` | ≤ `MAX_SEATS - 1` entries, each a complete `SignedEvent` of a `TIMEOUT_VOTE`, sorted ascending by voter seat |

*Receiver must validate:* every embedded vote independently passes §4.0 steps
2–11; all votes carry the same subject; the voter set is exactly `V(subject)` as
§8.3 defines it, with no duplicates, and **every seat absent from it is absent
because a completed, valid certificate in this hand already names that seat as
attributed** — a `V` shrunk by bare votes is not a `V`; `|V(subject)| >= 2`;
`subject_digest` recomputes; the emitter is itself a member of `V(subject)`.

**The certificate stage is collective.** The required emitter set is `V(subject)`;
each voter emits its own certificate carrying the same votes in the same order;
`stage_hash` is the collective form of §3.2 over the events of `event_class = 2`.
There is no timer, no tie-break and no assembler privilege. Certificate bodies
differ only in `sender_public_key` and `emitted_at_unix_ms`, which no longer
matters, because the collective `stage_hash` is taken over the whole set rather
than over one chosen copy.

This replaces the earlier rule, which had every peer wait `CERT_SETTLE_MS` and
then chain the certificate from the lowest seat index that had emitted a valid
one. That rule put a wall-clock read back inside the chain-building rule, which
D-006 and §8.2 forbid; `CERT_SETTLE_MS` is deleted from §13, and Q-04 is closed
by this ruling (§12).

### 4.9 Group 7 — synchronisation and disputes

Codes `0x0700`–`0x07FF`. Semantics in §6.

**`0x0701 STATE_HASH`** — collective stage. `n(0) checkpoint: u16`,
`n(1) state_hash: bytes[32]`, `n(2) transcript_head: bytes[32]`.

**A reconciliation round is its own stage, never a second copy of the
checkpoint's.** §6.3 step 3 has every peer re-derive its state after exchanging
the events it was missing, and publish the result. That value is a *different*
body by construction — a divergence that reconciles is exactly one in which at
least one peer's second value differs from its first — so writing it into the
disputed checkpoint's slot would put two distinct bodies of one honest signer
into one capacity-one slot. That is §5.2's equivocation predicate satisfied by
behaviour §6.3 **requires**, against the peer whose only fault was a dropped
stream, and D-009 rule 1 forbids it in terms.

So, normatively:

> A reconciliation `STATE_HASH` is a **new collective stage** with a **new
> `sequence`**, chained from the disputed checkpoint's `stage_hash`. Its payload
> is the ordinary `STATE_HASH` payload and its `n(0) checkpoint` is the
> checkpoint being re-derived, so a reader can tell which checkpoint it settles.
> Its required emitter set is the emitter set of that checkpoint. Successive
> rounds take successive `sequence` values, so a peer that reconciles twice
> occupies two slots and never two bodies in one. `STATE_ACK` follows a
> reconciliation round exactly as it follows a checkpoint.

The value stays chained, which matters for a second reason beyond the slot: the
decision it drives — resume the hand, or end it — is a consensus decision, and
§5.2's ordering rule forbids taking a consensus decision from an event the
engine's ordering buffer cannot place. A chained stage is placeable; an unchained
carrier is not.

Two alternatives were considered and rejected. **Carrying the re-derived value as
`DISPUTE` evidence** is nearly free, since §6.3 step 2 already obliges every peer
to emit a dispute carrying its own `STATE_HASH` — but it makes exactly that
consensus decision turn on an unchained event, which §5.2 now forbids. **Adding a
`checkpoint_round` component to §5.2's slot key** is the smallest edit, but it
would extend the canonical predicate a third time, for a case a fresh `sequence`
separates for free, and that key is carrying two special cases already. A new
stage changes no predicate at all.

**`0x0702 STATE_ACK`** — collective stage immediately following. `n(0)
checkpoint: u16`, `n(1) agreed_state_hash: bytes[32]`, `n(2) checkpoint_hash:
bytes[32]` where `checkpoint_hash` is the `stage_hash` of the `STATE_HASH` stage.

**`0x0703 DISPUTE`** — may be emitted by any participant at any time on the table
mesh, and is the only message that is legal outside its stage.

*Envelope:* **unchained** — `chain_scope = 0`, `event_class = 0`,
`table_id = ZERO32`, `hand_id = 0xFFFF_FFFF_FFFF_FFFF`, `sequence = 0`,
`previous_event_hash = ZERO32` (§2.3). The table and hand a dispute concerns are
named by the payload's `n(5)` and `n(6)`, never by the envelope, so that a
dispute occupies no stage slot and **can never appear in an `EquivocationProof`
(§5.2)**. That is the whole point of the classification: §6.3 obliges an honest
peer to emit two disputes in one hand, and a slot that cannot be occupied cannot
be occupied twice.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) kind` | `u16` | see §6.3 |
| `n(1) accused` | `Option<bytes[32]>` | app public key, when the kind names one |
| `n(2) at_sequence` | `u64` | the stage the dispute is *about*; it does not place the dispute anywhere |
| `n(3) evidence` | `Vec<bytes>` | ≤ 4 entries, each a complete `SignedEvent`, each ≤ `MAX_EMBEDDED_EVENT` = 32 768 B |
| `n(4) note` | `bytes` | ≤ 256 B UTF-8; human text, never parsed |
| `n(5) table_id` | `bytes[32]` | the table this dispute concerns; must equal the `table_public_key` of the session it is received on |
| `n(6) hand_id` | `u64` | the hand this dispute concerns |

`n(5)` and `n(6)` are appended rather than placed first because §2.2 rule 5 makes
field indices append-only; `JOIN_REQUEST`'s `n(8) table_id` was added the same way
for the same reason.

**Anti-replay, since the envelope's `sequence` is a sentinel.** A `DISPUTE` is
de-duplicated by `event_hash` within its `(sender_public_key, table_id, hand_id)`
scope: a byte-identical repeat is idempotent, as §1.5 requires of every forwarded
event. A dispute naming a `table_id` other than the receiving session's, or a
`hand_id` that is neither the current hand nor a completed hand of this session,
is dropped. At most `MAX_DISPUTES_PER_SENDER_PER_HAND = 8` distinct disputes are
accepted from one sender for one hand; beyond that the sender is rate-limited
under §9.5. No nonce field is needed and none is added — replay of an unchained,
idempotent, table-and-hand-bound message achieves nothing.

*Receiver must validate:* the sender is a participant of the named table;
`n(5) table_id` matches the session; `n(6) hand_id` is in scope; every evidence
entry independently passes §4.0 steps 2–11; `note` is well-formed UTF-8 under
§9.4's string rules and is **never parsed, never an identifier, and never an input
to any state transition** — a dispute's `note` is attacker-controlled text
(Q-05).

**A dispute is not itself evidence of anything.** It is a carrier. What carries
weight is what it contains: an `EquivocationProof` (§5.2) verifies on its own two
byte strings, and a `STATE_HASH` verifies as a signed event. A `DISPUTE` with an
empty or non-verifying `evidence` list changes no state and attributes nobody. It
is cheap to emit, which is exactly why it must be cheap to ignore.

### 4.10 Group 8 — termination and seat state

Codes `0x0800`–`0x08FF`.

---

**`0x0801 HAND_COMPLETE`**

*Direction:* **collective stage**, terminal for the hand; the required emitter set
is the same set that emitted `HAND_INIT`. Every field is derived, so by §3.2's
stage-kind principle there is no writer: each seat computes the byte-identical
body, signs its own copy, and emits it.
*Legal:* when the engine reports the hand decided.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) pots` | `Vec<PotAward>` | ≤ `MAX_SEATS` |
| `n(1) refunds` | `Vec<(u8, u64)>` | ≤ `MAX_SEATS`, uncalled excess returned before pots are awarded |
| `n(2) deltas` | `Vec<i64>` | one per occupied seat ascending; must sum to 0 |
| `n(3) final_stacks` | `Vec<u64>` | one per occupied seat ascending |
| `n(4) busted` | `Vec<u8>` | seats reaching 0, ascending |
| `n(5) state_hash` | `bytes[32]` | the end-of-hand state hash |

`PotAward` = `{ n(0) size: u64, n(1) eligible: Vec<u8>, n(2) winners: Vec<u8>,
n(3) odd_chips: Vec<u8> }`, all seat lists ascending and unique.

*Receiver must validate:* every field recomputed from its own engine, including
the pot layering, the eligible sets, the winners, and the clockwise-from-the-
button odd-chip distribution [RULES A7, A8]; `sum(deltas) == 0`;
`sum(final_stacks) + sum(committed_hand) == ledger_in − ledger_out` (C-9's ledger
identity, which in tournament mode reduces to the old
`sum(final_stacks) == total_chips_in_play`).

**A mismatch is not accepted; the mismatching copy is rejected under §4.0 and the
stage simply does not complete for that emitter. It does not open a §6
divergence.** That is the point of making the stage collective: a peer that
derives a different result is caught at the stage, by rejection, rather than
later at a checkpoint by a divergence freeze.

**A third-party evaluator's raw rank value must never appear here or in
`STATE_HASH`.** The numeric encoding is an internal detail of one crate version;
a table regeneration or a version bump would change the hash on some peers and
manufacture false disputes. Only the derived result — the winner sets and the chip
deltas — is canonical. [RULES A′4]

---

**`0x0802 HAND_ABORT`**

*Direction:* **witness-independent terminal stage** (§3.2), terminal for the hand.
There is **no required emitter set**: any seat of this hand's `HAND_INIT` set may
emit its copy, every such seat normally does, and the stage closes at a receiver
on the first copy that verifies and passes the acceptance gate below. Every field
is derived, so there is no writer either.

**P3 is settled here, normatively, and this cell is the answer the previous
revision declined to give.**

> **The terminal stage of an aborted hand is witness-independent (§3.2), it has
> no `stage_hash`, and `TERMINAL(k) = ABORT_TERMINAL(k)` (§3.1).** No seat's
> silence can block it, no two peers can derive different terminal values from
> it, and `GENESIS(k+1)` therefore exists on every abort path at every table
> size. `STATE_MACHINE.md`'s witness-independent shape is **adopted**; its
> `stage_hash` formula is not needed and is not adopted, because the terminal
> value is taken from §3.1 rather than from the stage.

The cell this replaces read "the same set that emitted `HAND_INIT`, minus every
seat named in this abort's own `n(1) attributed`", and it deadlocked exactly as
the gate said: on the `hand_deadline_ms` path `attributed` is empty, so the
silent seat stayed in the required set and the stage could never complete.
`STATE_MACHINE.md` §4.1 reached the right shape first and this document owns the
ruling, so the ruling is made here (D-011 rule 1).

**Its `sequence`, and G1 — the defect the previous pass's own P3 fix created.**
A stalled stage has no `stage_hash`, so the abort cannot chain from it; it chains
from the last stage complete at its emitter and therefore carries **the stalled
stage's own index** as its `sequence`. There is no third option. Every honest
peer that already contributed to that stalled stage has consequently signed a
second `chain_scope = 1`, `event_class = 0` body at that `(sequence, sender)`.

Under the slot key as it stood, those were **one slot**: the abort was rejected
at §4.0 step 10a by every receiver including its own emitter — so `TERMINAL(k)`
was again never defined, by a new route — and it was simultaneously a verifying
`EquivocationProof` against the honest peer the protocol *required* to emit it.
That is D-009 rule 1's sixth occurrence and it is closed by **§5.2.1's key, which
contains `event_type`** (D-011 rule 2). `DECK_INIT` at `s` and `HAND_ABORT` at
`s` are two slots. Both halves — the rejection and the self-incrimination — are
gone, and they are gone by the definition rather than by a rule about this one
message type, which is the point of writing the key down once.

**One abort per emitter per hand.** A peer emits at most one terminal
`HAND_ABORT` for a given `hand_id`, at one `sequence`. It cannot vary the
`sequence` to produce a second, because the first ends the hand at it.

*Legal:* only on one of the terminal triggers enumerated below, **every one of
which is chained content or the emitter's own expired timer**: a `kind = 2`
`TIMEOUT_CERT` that closed a stage, the expiry of `hand_deadline_ms`, a failed
shuffle or reveal proof, or the §6 divergence procedure. An unchained event never
makes this message legal (§5.2).

**Acceptance gate — normative, and it is what stops a witness-independent
terminal from becoming a one-message hand void.** A receiver accepts a terminal
`HAND_ABORT` only when the trigger for its `cause` is present **in that
receiver's own state**:

| `cause` | The receiver's own trigger | Before that trigger |
|---|---|---|
| `1`, certified-subject path (`cert_hash = Some`) | it holds, or `n(3) evidence` carries, the named `kind = 2` `TIMEOUT_CERT` with `\|V\| >= 2` | reject |
| `1`, hand-deadline path (`attributed = []`, `cert_hash = None`) | its **own** `hand_deadline_ms` has expired (§8.2) | **buffer, do not reject** |
| `1`, §6.3 case (b) | it has itself reached case (b), **or** its own `hand_deadline_ms` has expired | **buffer, do not reject** |
| `2`, `3` | `n(3) evidence` verifies — the failing `SHUFFLE_PROOF` or reveal proof carries its own disproof | accept at once |
| `4` | it is itself in the §6.3 case (c) terminus | **buffer, do not reject** |

**Buffer, never reject, is the load-bearing half.** Rejecting a premature abort
would put the deadlock back: a peer whose timer runs a few hundred milliseconds
behind would refuse the artefact and both would wait for each other. Buffering
costs the clock spread and nothing else, because §8.2 anchors every peer's
`hand_deadline_ms` to the **same agreed event**, `TERMINAL(k-1)` — which is why
R-1's fix is a correctness precondition of this gate and not a tidy-up.

And an early abort buys an attacker nothing: it is held until the receiver's own
deadline expires, at which point the receiver would have aborted anyway. The only
thing a peer can do by emitting one early is to abort the hand at the deadline,
which it could equally do by going silent (`THREAT_MODEL.md` X8). No new escape is opened.

**Its position in the chain is checked loosely, and that is deliberate.** A
receiver whose own chain head for hand `k` differs from the abort's
`previous_event_hash` still accepts it, provided the abort's `sequence` is a
stage index of hand `k` and every other check passes. Two peers can honestly
disagree about which stage stalled — one heard a contribution the other did not —
and a strict parent check would have each reject the other's artefact, which is
the deadlock again by a third route. This is the **one** exemption from §4.0
step 10a's chain-position rule in the whole document, and it is safe for exactly
one reason: **nothing downstream reads the abort's `sequence` or its parent.**
`ABORT_TERMINAL(k)` is a function of `GENESIS(k)`, so an abort accepted at the
"wrong" stage index yields the same terminal value as one accepted at the right
one. The `sequence` and the parent are a record of where this emitter believed
the hand stopped, and nothing more.

**Precedence against `HAND_COMPLETE`.** A hand is decided or aborted, never both.
A receiver that holds a complete `HAND_COMPLETE` stage for hand `k` discards
every `HAND_ABORT` for that hand; a receiver that applied an abort and later
accepts a complete `HAND_COMPLETE` stage for the same hand replaces its terminal
with that stage's `stage_hash`. `HAND_COMPLETE` wins because it is collective:
it completes only if **every** present seat emitted its copy, so a completing
`HAND_COMPLETE` proves that no seat was silent, which is the premise every abort
rests on. The race is narrow — it needs a peer's deadline to expire between its
own `HAND_COMPLETE` emission and the arrival of the last other copy — and it is
resolved without a vote, a timer or a tie-break.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) cause` | `u16` | `1` failure to publish a required cryptographic contribution; `2` invalid shuffle proof; `3` invalid reveal proof; `4` unresolvable state divergence. **Value `5` (equivocation proven) is deleted** — see §5.2's ordering rule and the note below. Every surviving value is a function of chained content that the ordering buffer can place, which is what makes two honest peers derive one body |
| `n(1) attributed` | `Vec<bytes[32]>` | ≤ `MAX_SEATS` app public keys, ascending by encoded bytes; may be empty, and is empty for `cause = 4` and for `cause = 1` on each of the three paths listed below. **Evidence only — nothing in this document reads it to move a chip or to remove a player (D-010)** |
| `n(2) cert_hash` | `Option<bytes[32]>` | `event_hash` of the `TIMEOUT_CERT`. Required for `cause = 1` **when a certificate with an effect exists**, which is the certified-subject path and only that; `None` on every `attributed = []` path, where unanimity was by construction never reached and no certificate can exist |
| `n(3) evidence` | `Vec<bytes>` | ≤ 2 `SignedEvent`s, each ≤ `MAX_EMBEDDED_EVENT` = 32 768 B; required for `cause` 2 and 3, which are the two causes a single chained event proves on its own |
| `n(4) deltas` | `Vec<i64>` | **all zeroes, always (D-010).** An abort moves no chips, so there is no per-seat delta to carry; the field is a vector of `0` of length `\|occupied seats\|`. It is kept rather than removed so that `HAND_ABORT` and `HAND_COMPLETE` stay directly comparable to a verifier, and so that "the deltas sum to zero" stays one receiver check across both terminal messages rather than two |
| `n(5) final_stacks` | `Vec<u64>` | **the start-of-hand stacks, always (D-010)** — that is, `stack_at_hand_start[s]` for every occupied seat ascending, exactly the values already bound into `roster_hash(k)` and hence into `GENESIS(k)`. Every peer holds them from the genesis of the hand, so this field is a function of agreed state and two honest peers cannot derive it differently |

*Receiver must validate:* the `cause` is one of the four defined values; **the
acceptance gate above passes** — the trigger for that cause is present in this
receiver's own state, and where the gate says *buffer* the event is held, not
rejected; `cert_hash` is present exactly on the certified-subject path and `None`
everywhere else; `evidence` is present exactly for causes 2 and 3; and — the
check that makes D-010 enforceable at the receiver rather than trusted at the
emitter — **`n(4) deltas` is all zeroes and `n(5) final_stacks` equals this
receiver's own `stack_at_hand_start` vector**. A copy that moves a chip is
rejected under §4.0 like any other mismatch, whoever signed it and whatever it
names. That is what "no message carries a forfeiture instruction" means
operationally: not that emitters are asked not to write one, but that a receiver
will not accept one.

**A note on the wire format.** Deleting an enumerated value and fixing two field
contents is, by §10.2, a major-version matter. `PROTOCOL_MAJOR = 1` has not
shipped and no peer is emitting `cause = 5`, so this is a revision of version 1's
definition rather than a break — but it must land before the first release, and
after that it would need a major bump.

**Chip handling on abort: neutral, always, in every case (D-010).** Every stack
is restored to its start-of-hand value. `n(4) deltas` is all zeroes,
`n(5) final_stacks` is the start-of-hand stacks, and this holds for every `cause`
and whoever is named in `attributed`. No chips move between seats on an abort, in
any direction, for any reason. Chip conservation is trivial on this path rather
than derived: the sum is the sum it was at `HAND_INIT`.

**What that deletes, quoted so the corpus can be checked against it.** This
section previously carried D-005's forfeiture rule:

> *"The attributed player forfeits what they committed to the pot, and it is
> distributed to the remaining players in proportion to their own contributions.
> It is **not** restored."*

Deleted with it: the distribution arithmetic and its clockwise remainder loop;
the `attributed = []` carve-outs, which now describe every abort rather than two
exceptions; every rule that made `attributed` an input to a chip calculation; and
the sentence that put attributed seats into the absent state as a consequence of
being attributed. `attributed` survives as a signed record of who did not publish
what, and **nothing in this protocol reads it.**

**Why, and what it costs — the trade `SPEC_CS.md` §18 requires be made explicit.**
D-005's reasoning was sound in isolation: forfeiture closed the rage-quit escape
exactly, because quitting then cost precisely what folding would have cost. What
four adversarial passes measured is that the same machinery — timeout
certificates, equivocation proofs, dispute resolution, attribution, and
forfeiture computed from all of them — is a consensus protocol this corpus has
failed to specify correctly four times, and that each failure ended with an
**honest** peer's committed chips being taken for doing exactly what the protocol
required of it. An exploit that harms an honest player is worse than one that
lets a dishonest player escape a loss, so D-010 takes the second.

The cost is that **the rage-quit escape returns, everywhere**, and it must not be
described as anything smaller. It is no longer bounded to `|V| < 2`: at every
table size, on every abort path, a player facing a losing pot can stall until
`hand_deadline_ms`, or disconnect, or publish a false `state_hash` and fault the
table (§6.4), and recover its commitment in each case. Nothing in the protocol
charges for it. The only pressure left is the one play money could ever have
delivered: repeated aborts attributable to one identity are visible in the
transcript, to everyone, permanently, and declining to sit with such a player is a
**user** decision (D-010 point 3). `THREAT_MODEL.md` §8 limitation 4 carries this and
`THREAT_MODEL.md` must carry it as one too.

**The `cause` values are evidence, not dispositions.** They no longer select
between chip outcomes, because there is exactly one chip outcome. What they still
do is record which of four things ended the hand, for a reader and for a future
adjudicator:

| `cause` | Ends the hand when | `attributed` |
|---|---|---|
| `1` failure to publish | a `kind = 2` `TIMEOUT_CERT` with `\|V\| >= 2` closed a stage (§8.3); **or** `hand_deadline_ms` expired with no certificate that had an effect (§8.4); **or** §6.3 case (b) — a peer is missing events no holder will serve | the certified subject on the first path, **empty** on the other two |
| `2` invalid shuffle proof | a `SHUFFLE_PROOF` failed verification (§4.0 step 14, `INVALID_SHUFFLE_PROOF`) | the shuffler |
| `3` invalid reveal proof | a Chaum–Pedersen DLEQ failed at `DEAL_PRIVATE`, `BOARD_REVEAL` or `SHOWDOWN_REVEAL` — the hand aborts here rather than stalling into a deadline (`CRYPTOGRAPHY.md` §8 rule 1) | the revealer |
| `4` unresolvable divergence | §6.3 case (c): transcripts byte-identical after reconciliation and derived states still differ | **empty, always** |

Three of the four paths therefore carry `attributed = []`, which is no longer
worth a table of exceptions: an abort that names nobody and an abort that names
someone have identical effects.

**§6.3 case (b) is now well formed, and it is the third `attributed = []` path
(P6).** §6.3 specified it as `cause = 1` with `attributed = []` and no
certificate — a shape this section's `cert_hash` rule forbade and its
"exactly two such cases" enumeration excluded, so an implementer met a terminal
outcome no legal event could express and no transition consumed. D-010 dissolves
it rather than repairing it: the disposition is **abort neutrally**, exactly like
every other abort, with `cause = 1`, `attributed = []`, `cert_hash = None`, stacks
restored, and the table **not** faulted. It needed a rule of its own only while
causes selected between chip outcomes. Folding it into `cause = 4` was the
alternative and was rejected, because case (b) is a different fact about the world
— events exist and are being withheld — from case (c), where nothing is missing
and the derivations still differ, and recording which fact it was is the `cause`
field's only remaining job. **OQ-D is untouched**: no adjudication of the
withholding is specified here, and none is claimed.

`cause = 4` remains the one cause that **faults the table** — no further hand is
dealt (§6.4). That is a liveness consequence, not a chip consequence, and it is
the only asymmetry left between the four causes.

**A hand abort never ends the tournament or the cash game.** Per D-006 point 5,
the next hand begins immediately afterwards, automatically, from
`GENESIS(k+1)` — which exists on every abort path because `TERMINAL(k)` is
`ABORT_TERMINAL(k)` (§3.1).

**What an abort does to a seat's state is `STATE_MACHINE.md`'s, not this
document's** (D-011 rule 1). The paragraph that stood here restated D-005's
absent-seat rule — that a silent seat is absent from the next hand because it is
silent and not because an abort named it, that it is left by the peer
reappearing, and that no seat is unseated by it. `STATE_MACHINE.md` §8.7 owns
that and says it. What this document is normative about, and repeats nowhere
else, is the wire fact underneath it: **`n(1) attributed` is not read by any rule
in this document, and no receiver may derive a seat's state from it** (D-010
point 2).

---

**`0x0803 PLAYER_SIT_OUT`** — single-writer stage at a hand boundary only.
`n(0) reason: u16` (`1` voluntary, `2` derived from consecutive auto-actions).
The derived case needs no message at all — after `MAX_CONSECUTIVE_AUTO_ACTIONS`
every peer marks the seat sitting out identically (D-006 point 4) — so this
message exists for the voluntary case and as an explicit record. A seat that is
sitting out keeps its stack, pays its blinds and antes, takes no cards, and drains
until it busts.

**`0x0804 PLAYER_SIT_IN`** — single-writer stage at a hand boundary only. No
fields beyond the envelope. Legal only from a seat currently sitting out. Takes
effect from the next `HAND_INIT`, never mid-hand.

**`0x0805 PLAYER_LEAVE`** — single-writer stage at a hand boundary, and **only**
there. `n(0) reason: u16` (`1` voluntary, `2` client shutting down). A leave is
never *required*: a client that vanishes must be handled identically, because a
departing peer announcing anything can never be a precondition. [NAT §7.3]

**The "courtesy notice that is not chained" clause is deleted (M4).** It read:
*"or accepted at any time as a courtesy notice that is not chained"*, and it
contradicted both of the places that state this message's envelope — §2.3's
exhaustive list of unchained types, which does not name `PLAYER_LEAVE`, and
§4.11's row, which gives it `chain_scope = 1`, "single", "the seat", under a
heading whose normative sentence is that `DISPUTE` is the one table-mesh message
with `chain_scope = 0`. A mid-hand courtesy leave had no envelope it could
legally carry: as `chain_scope = 0` it is an unchained type missing from an
exhaustive list and is rejected under §2.3; as `chain_scope = 1` it occupies a
stage slot outside its stage, and a seat that sent one mid-hand and a real
`PLAYER_LEAVE` at the boundary would have signed two bodies with no rule
assigning them distinct `sequence` values — the defect class §5.2 closed for
`DISPUTE` and §4.8 closed for `TIMEOUT_VOTE`, in a third message type.

The clause is deleted rather than repaired because §4.10 already says what makes
it unnecessary: a leave is never required, and a client that vanishes mid-hand is
handled identically whether it announced anything or not. **A `PLAYER_LEAVE`
received outside a hand boundary is rejected under §4.0 like any other
out-of-stage chained event**; the intent it was carrying is expressed by the
seat's silence and disposed of by §8. Nothing is lost: the only thing the
courtesy notice bought was a UI hint, and a UI hint may not have an envelope no
rule in this document defines.

### 4.11 Summary table

`chain_scope` is the envelope field of §2.3: `1` means the message occupies a
stage slot in a chain and is subject to the equivocation predicate of §5.2, `0`
means it does not and never can be.

| Code | Name | Channel | `chain_scope` | Stage kind | Emitter |
|---|---|---|---|---|---|
| `0x0001` | `HELLO` | table mesh | 0 | — | each side of a connection |
| `0x0002` | `CAPABILITIES` | table mesh | 0 | — | each side of a connection |
| `0x0101` | `LOBBY_TABLE_AD` | lobby broadcast | 0 | — | table key |
| `0x0102` | `LOBBY_TABLE_REMOVE` | lobby broadcast | 0 | — | table key |
| `0x0103` | `LOBBY_PLAYER_PRESENCE` | lobby broadcast | 0 | — | any peer |
| `0x0104` | `LOBBY_SNAPSHOT_REQUEST` | lobby RPC | 0 | — | any peer |
| `0x0105` | `LOBBY_SNAPSHOT_RESPONSE` | lobby RPC | 0 | — | any peer |
| `0x0106` | `LOBBY_CHAT` | lobby chat broadcast | 0 | — | any peer |
| `0x0201` | `JOIN_REQUEST` | join RPC | 0 | — | joiner |
| `0x0202` | `JOIN_ACCEPT` | join RPC | 0 | — | table key |
| `0x0203` | `JOIN_REJECT` | join RPC | 0 | — | table key |
| `0x0204` | `PLAYER_LIST` | table mesh | 0 | — | table key |
| `0x0205` | `TABLE_READY` | table mesh | 1 | collective | all seats |
| `0x0301` | `RNG_COMMIT` | table mesh | 1 | collective | all seats |
| `0x0302` | `RNG_REVEAL` | table mesh | 1 | collective | all seats |
| `0x0303` | `HAND_INIT` | table mesh | 1 | collective | all present seats |
| `0x0304` | `DECK_INIT` | table mesh | 1 | collective | dealt-in seats |
| `0x0305` | `SHUFFLE_STEP` | table mesh | 1 | single | shuffler `j` |
| `0x0306` | `SHUFFLE_PROOF` | table mesh | 1 | single | shuffler `j` |
| `0x0307` | `DECK_COMMIT` | table mesh | 1 | collective | dealt-in seats |
| `0x0401` | `DEAL_PRIVATE` | table mesh | 1 | collective | dealt-in seats |
| `0x0402` | `BOARD_REVEAL` | table mesh | 1 | collective | dealt-in seats |
| `0x0403` | `SHOWDOWN_REVEAL` | table mesh | 1 | collective | showdown seats |
| `0x0404` | `SHOWDOWN_MUCK` | table mesh | 1 | collective | showdown seats (policy-gated) |
| `0x0501` | `ACTION_CHECK` | table mesh | 1 | single | `player_to_act` |
| `0x0502` | `ACTION_CALL` | table mesh | 1 | single | `player_to_act` |
| `0x0503` | `ACTION_BET` | table mesh | 1 | single | `player_to_act` |
| `0x0504` | `ACTION_RAISE` | table mesh | 1 | single | `player_to_act` |
| `0x0505` | `ACTION_FOLD` | table mesh | 1 | single | `player_to_act` |
| `0x0601` | `TIMEOUT_VOTE` | table mesh | 1 (`event_class = 1`, keyed also on `subject_seat`) | — | required voters |
| `0x0602` | `TIMEOUT_CERT` | table mesh | 1 (`event_class = 2`, keyed also on `subject_digest`) | collective | required voters |
| `0x0701` | `STATE_HASH` | table mesh | 1 | collective | all present seats |
| `0x0702` | `STATE_ACK` | table mesh | 1 | collective | all present seats |
| `0x0703` | `DISPUTE` | table mesh | **0** | out-of-stage | any participant |
| `0x0801` | `HAND_COMPLETE` | table mesh | 1 | collective | all present seats |
| `0x0802` | `HAND_ABORT` | table mesh | 1 | **witness-independent terminal** | any seat of the `HAND_INIT` set; no required set (§3.2, §4.10) |
| `0x0803` | `PLAYER_SIT_OUT` | table mesh | 1 | single | the seat |
| `0x0804` | `PLAYER_SIT_IN` | table mesh | 1 | single | the seat |
| `0x0805` | `PLAYER_LEAVE` | table mesh | 1 | single | the seat |

39 message types. Lobby chat is on `/p2p-poker/lobby-chat/1` and not on the lobby
topic, so §1.4's "a message on the wrong channel is dropped" rule covers it like
any other.

**`DISPUTE` is the one table-mesh message with `chain_scope = 0`,** and this
column is the normative statement of it; §2.3's exhaustive list and §4.9's
envelope paragraph say the same thing and must be kept in step with this row. Its
`Stage kind` is "out-of-stage" precisely because it occupies no slot — the two
cells are consistent, not in tension. `PLAYER_LEAVE` is on the chained side of
that line with no exception of any kind (M4, §4.10).

#### The honest-interleaving check, all 39 types, against §5.2.1's key

**This table is derived per type from the slot key of §5.2.1, not from stage-kind
groups.** The grouped form that stood here certified two defects with one
sentence — P1's `STATE_HASH` and G1's `HAND_ABORT`, both in the row whose reason
was "capacity one by the stage rule" — because the group is not what the
predicate indexes. The question asked of each row is the only question that
matters: **can one honest peer, doing what this document requires or permits of
it, produce two `SignedEvent`s whose §5.2.1 keys are equal and whose
`event_hash`es differ?**

The column *Varies within one `(hand_id, sender)`* names what an honest emitter
may legitimately change between two emissions, and the verdict is clean exactly
when some key component changes with it.

| # | Type(s) | Required / permitted honest emissions | Varies within one `(hand_id, sender)` | Verdict |
|---:|---|---|---|---|
| 1–13 | `HELLO`, `CAPABILITIES`, `JOIN_REQUEST`, `JOIN_ACCEPT`, `JOIN_REJECT`, `PLAYER_LIST`, the six `LOBBY_*`, `DISPUTE` | any number, any time | — | **Outside the predicate.** `chain_scope = 0` occupies no slot; per-type anti-replay in §5.2.1's box |
| 14 | `TABLE_READY` | one per seat, setup chain | — | **Clean** |
| 15 | `RNG_COMMIT` | one per seat, own stage | — | **Clean** |
| 16 | `RNG_REVEAL` | one per seat, the next stage | `sequence` | **Clean** |
| 17 | `HAND_INIT` | one derived copy per present seat | — | **Clean.** A `HAND_INIT` that stalls is disposed of by a `HAND_ABORT` at `HAND_INIT`'s own `sequence`, which differs in `event_type` — row 36 |
| 18 | `DECK_INIT` | one per dealt-in seat | — | **Clean** |
| 19 | `SHUFFLE_STEP` | one, single-writer | — | **Clean** |
| 20 | `SHUFFLE_PROOF` | one, the next stage | `sequence` | **Clean** |
| 21 | `DECK_COMMIT` | one per dealt-in seat | — | **Clean** |
| 22 | `DEAL_PRIVATE` | one per dealt-in seat, **exactly** `2(m−1)` entries in one body | — | **Clean.** The "exactly" is what forbids incremental publication, which would be two bodies at one stage |
| 23 | `BOARD_REVEAL` | one per dealt-in seat per street | `sequence` — each street its own stage | **Clean** |
| 24 | `SHOWDOWN_REVEAL` | one per showdown seat | — | **Clean** |
| 25 | `SHOWDOWN_MUCK` | one per showdown seat, mutually exclusive with row 24 by `showdown_policy` (§4.6) | `event_type`, if a seat emits both | **Clean.** Since `event_type` entered the key the pair is two slots, so a seat emitting both is a **stage violation** under §4.0 step 12, not an equivocation. §4.6's exclusivity stays normative and is what rejects the second (§5.2.1) |
| 26–30 | `ACTION_CHECK`, `ACTION_CALL`, `ACTION_BET`, `ACTION_RAISE`, `ACTION_FOLD` | one per turn, single-writer | `event_type`, if a seat claims two actions for one turn | **Clean.** Same change as row 25 and the same price: two *different* action types at one `sequence` are a stage violation, not a proof; two bodies of the **same** type — two `ACTION_RAISE` with different amounts — are still an equivocation (§5.2.1) |
| 31 | `TIMEOUT_VOTE` | one per subject per stage; two simultaneous subjects are **normal** (§8.4) | `subject_seat` | **Clean since M2**, by the subject axis in the key |
| 32 | `TIMEOUT_CERT` | one per `subject_digest` per stage | `subject_digest` | **Clean since M2**, by the subject axis. Its one variable field `n(1) votes` is pinned by the receiver check of §4.8 |
| 33 | `STATE_HASH` | one per checkpoint, **plus one per reconciliation round** — a required re-emission with *changed* content, the only one in the corpus | `sequence`, and only because §4.9 gives each reconciliation round `s_ckpt + r` | **Clean since P1, and clean by that rule alone.** Not by the stage rule: an editor who deletes §4.9's normative box reopens P1 the same day |
| 34 | `STATE_ACK` | one per checkpoint and one per reconciliation round | `sequence`, same rule | **Clean since P1**, same reason |
| 35 | `HAND_COMPLETE` | one derived copy per present seat, at a fresh `sequence` — the last stage completed, so nothing occupies it | — | **Clean** |
| 36 | `HAND_ABORT` | **one per emitter per hand**, at the stalled stage's own `sequence`, where the emitter has usually already contributed | `event_type` — against the contribution it shares a `(sequence, class)` with | **Clean since G1, and only because `event_type` is in the key (§5.2.1, D-011 rule 2).** This row is the reason the key was rewritten. Under the seven-tuple it read FAILS: the abort collided with its own emitter's contribution, was rejected at §4.0 step 10a, and was a verifying proof against an honest peer |
| 37 | `PLAYER_SIT_OUT` | one per seat, hand boundary only, single-writer | — | **Clean since M4** |
| 38 | `PLAYER_SIT_IN` | same | — | **Clean since M4** |
| 39 | `PLAYER_LEAVE` | same, and **only** there — the courtesy-notice clause is deleted | — | **Clean since M4** |

**39 rows, numbered 1 to 39, one per message type of the table above.** The
numbering replaces the group arithmetic that stood here (`13 + 11 + 2 + 10 + 1 +
2 = 39`), because that arithmetic was correct on both occasions it covered a
wrong verdict. A row added to the summary table without a row here is visible as
a gap in the numbering, and a row here whose verdict rests on a rule names that
rule so the rule cannot be deleted by an editor who does not know what it carries
(rows 33, 34, 36).

**Two verdicts changed direction in this revision and both are recorded as
changes rather than restated as if they had always read that way:** row 36 went
from FAILS to clean, and rows 25–30 went from "clean, and the exclusivity is what
holds it" to "clean, and the collision is now a rejection rather than a proof".
The second is the price of the first and §5.2.1 argues it.

Codes `0xF000`–`0xFFFF` are reserved for private and experimental use and are
**never** accepted inside a table session, regardless of capability negotiation.

---

## 5. Replay and equivocation (`SPEC_CS.md` §14)

### 5.1 Which field stops which attack

| Attack (§14 / §17) | Stopped by | Mechanism |
|---|---|---|
| replay of an old signed message | `previous_event_hash` + `sequence` | a replayed event's `previous_event_hash` does not equal the current `stage_hash(s-1)`, because the chain has moved on |
| duplicate message | an `event_hash` set per **slot**, and the slot key is §5.2.1's — read whole, reproduced nowhere | a byte-identical repeat is idempotent; a differing repeat in one slot is equivocation. §4.11 carries the per-type check that says why each key component is there |
| out-of-order message | `sequence` = stage index | an event for stage `s+2` is buffered, never applied, until stage `s+1` completes |
| message from another hand | `hand_id` **and** `previous_event_hash` | the chain of hand `k` cannot link to hand `k'` |
| message from another table | `table_id` for chained events; the payload's table field for unchained ones | `table_id` is the table's public key, so it cannot be forged; an unchained message names its table in the payload and is bound to it by the signature over the whole body (§2.3) |
| replay of a *shuffle proof* from another hand | the `ctx` binding (§4.5) | the proof itself will not verify under a different `ctx` |
| replay of a *reveal token* onto another card | DLEQ proof binds the token to the ciphertext | verified empirically [MENTAL §4.1 T7b] |
| replay of a lobby advertisement | `timestamp` monotonicity per `table_id`, plus `expires_at` | a lower `timestamp` for a known `table_id` is discarded [NAT §7.3] |
| replay of a `PLAYER_LIST` | `list_serial` strictly increasing | |
| impersonating another participant | `sender_public_key` + `verify_strict` | and the roster fixes which keys are seated |
| signature malleability | `verify_strict` only, and `event_hash` excludes the signature | [CRYPTO §2.1] |
| two byte encodings of one event | the canonicality gate, run **before** the signature check | [CRYPTO §4.7] |
| equivocation | §5.2 | |
| session nonce reuse across sessions | `session_id` inside `GENESIS(k)` | transitively binds every event |

### 5.2 Equivocation, exactly

#### 5.2.1 The anti-replay slot key — the one place it is written (D-011 rule 2)

**NORMATIVE. This box is the definition of the anti-replay slot for the whole
corpus. `STATE_MACHINE.md`, `CRYPTOGRAPHY.md`, `NETWORK_STACK.md` and
`THREAT_MODEL.md` reference this section by number and reproduce none of it —
not the tuple, not a subset of it, not a paraphrase of it. §4.0 step 10a, §5.3's
stored state and §4.11's per-type check all read this tuple and nothing else.**

> ```
> slot(E)  is defined only for  E.chain_scope == 1 , and is the 8-tuple
>
> slot(E) = ( E.protocol_version        u16     envelope n(0)
>           , E.table_id                [u8;32] envelope n(1)
>           , E.hand_id                 u64     envelope n(2)
>           , E.sequence                u64     envelope n(3)
>           , E.sender_public_key       [u8;32] envelope n(4)
>           , E.event_class             u8      envelope n(11)
>           , E.event_type              u16     envelope n(5)
>           , subject(E)                        see below
>           )
>
> subject(E) = ()                             when E.event_class == 0
>            = ( payload n(1) subject_seat )   when E.event_class == 1  (0x0601)
>            = ( payload n(0) subject_digest ) when E.event_class == 2  (0x0602)
> ```
>
> **Deliberately absent, each for a stated reason, and none of them may be
> added by any document:**
>
> * `previous_event_hash` — a peer that chains one body to two parents at one
>   `sequence` is forking the chain, and that must remain an equivocation. Putting
>   the parent in the key would excuse it.
> * `payload` and `emitted_at_unix_ms` and `next_deadline_ms` — the key must have
>   capacity **one**, so it may contain only fields that legitimately vary. A key
>   containing the body is a key no two events ever share, and a predicate over it
>   is vacuous. This is also why §5.2.3's re-emission obligation exists.
> * `chain_scope` — it is not a key component but the predicate's precondition.
>   `chain_scope == 0` events occupy no slot at all, ever.
>
> **`event_class` is implied by `event_type`** — `0x0601` is class 1, `0x0602` is
> class 2, every other type is class 0 — and it is carried in the key anyway,
> because §5.3 partitions its stored state by class and because a coarser key
> would be the unsafe direction: dropping a component merges slots and
> manufactures false positives against honest peers, which is the failure D-009
> rule 1 exists to prevent.
>
> **Unchained events.** `chain_scope == 0` events are outside this predicate
> entirely and can never appear in an `EquivocationProof`. Their anti-replay is
> per-type: `timestamp_unix_ms` strict monotonicity per `table_id` for lobby
> adverts, `list_serial` for `PLAYER_LIST`, `join_nonce` for the join RPC,
> `connection_nonce` plus a per-connection counter for the handshake, and for
> `DISPUTE` idempotent de-duplication by `event_hash` within its
> `(sender_public_key, table_id, hand_id)` scope, bounded by
> `MAX_DISPUTES_PER_SENDER_PER_HAND` (§4.9).

**Why `event_type` is in the key, and what it costs.** This is D-011 rule 2 and
it is the sixth attempt at D-009 rule 1. The five previous attempts failed
because the key was prose that each editor re-derived; this one is a tuple with
field indices, and every other document points at it.

`event_type` is the component G1 needed. The terminal `HAND_ABORT` of §4.10 is
written at the **stalled stage's own `sequence`** — it must be, because a stage
that never completed has no `stage_hash` to chain a successor from — so every
honest peer that already contributed to that stage signs a second `class = 0`
body at that `(sequence, sender)`. Without `event_type` those are one slot: the
abort is rejected at §4.0 step 10a by every receiver including its own emitter,
and it is simultaneously a verifying `EquivocationProof` against the honest peer
that the protocol *required* to emit it. With `event_type` they are two slots and
both halves are gone. See §4.10 and §4.11's per-type check.

The price is paid in two places and is stated rather than hidden:

* **Two different `ACTION_*` types claimed for one turn are no longer one slot.**
  "I folded" to one peer and "I raised" to another are `0x0505` and `0x0504`, so
  they no longer form an `EquivocationProof`. Two `ACTION_RAISE` events with
  different amounts still do, because the type is equal and the payload is not.
* **`SHOWDOWN_REVEAL` against `SHOWDOWN_MUCK`** likewise stops being an
  equivocation and becomes a validity rejection only.

Both remain **rejected**, by §4.0 step 12 and §3.2's stage rule: a stage admits
one contribution per seat, whatever type it carries, and the second is a
violation at every receiver that holds the first. Both remain **detected** by the
route §5.2.4 describes for every equivocation under D-010 — the peers that
accepted different first copies diverge, the next checkpoint shows two hashes,
§6.3 runs, the hand ends neutrally. And both remain **evidence**: a `DISPUTE` is
a carrier (§4.9) and may carry any two conflicting signed events, whether or not
they meet this predicate. What is lost is the *label*, and under D-010 the label
carries no consequence: a proof ends no hand, moves no chip and evicts nobody, so
the difference between "an `EquivocationProof` names this key" and "two signed
events by this key contradict each other" is one of vocabulary, not of effect.
Weighed against a key that makes honest peers incriminate themselves — five
times, in five passes — that is the right side of the trade, and D-011 rule 2
makes it binding rather than discretionary.

**A message whose honest emission can collide in this key is a defect in the
message, not in the key** (D-011 rule 2). §4.11 carries the check for all 39
types and is where a new type is checked before it is added.

#### 5.2.2 The predicate

**Definition, canonical for the whole corpus.** `THREAT_MODEL.md` G7 quotes it;
no document restates it in its own words.

> Peer `K` equivocates when two `SignedEvent`s `E1 != E2` exist such that both
> pass the canonicality gate, both verify under `K`'s public key with
> `verify_strict`, both carry `chain_scope == 1`, `slot(E1) == slot(E2)` under
> §5.2.1, and `event_hash(E1) != event_hash(E2)`.

#### 5.2.3 The property every chained message type must satisfy

**Normative. This is D-009 rule 1 and it is canonical for the corpus; the other
four documents point here and do not restate it in their own words.**

> **No sequence of emissions that this protocol requires or permits of an honest
> peer may produce two `SignedEvent`s in one slot, and therefore none may produce
> a valid `EquivocationProof` against that peer.**
>
> The slot key must contain every field that legitimately varies for one signer
> at one stage. Where a message type lets one honest sender emit two events that
> differ only in a field the key omits, the defect is in the key, not in the
> sender, and it is closed by extending the key — never by asking the sender to
> emit less than the protocol tells it to.
>
> Two obligations follow and both are normative. **(1) Re-emission is
> re-transmission, never re-signing.** A peer that must send an event again sends
> the stored bytes; `emitted_at_unix_ms` is advisory (§2.6) and varies freely, so
> a re-signed copy is a second body in the same slot and is indistinguishable
> from an equivocation. **(2) A new chained message type is checked against this
> property before it is added to §4.11**, and the check is recorded in the table
> below §4.11's summary. A type that fails it does not ship with a caveat; it
> ships with a longer key.

**Why the subject is in the key (D-009 rule 1, defect M2).** `TIMEOUT_VOTE`
carries `sequence = subject_sequence` and named its subject only in the body, so
which seat a vote was *about* did not change its slot. §8.4 specifies two
simultaneous subjects at one stage as normal — every cryptographic stage is
collective, so two seats failing at one stage is the ordinary shape of the case,
not an edge — and an honest voter whose timers expired against both is entitled,
by §4.8, to vote about each. Those two votes were two distinct bodies in one
capacity-one slot: this predicate satisfied against an honest key by behaviour
this document calls ordinary, and, through this section's then-existing
`cause = 5` abort and `STATE_MACHINE.md` §8.6's then-existing forfeiture formula,
that honest voter's committed chips moved to whoever filed the proof. Both of
those consumers are now deleted — the abort by the ordering rule below, the
formula by D-010 — so the same defect today would cost a hand rather than a
stack. **The key stays extended regardless**: D-009 rule 1 is a property of the
predicate, not of whatever happens to be reading it, and a proof naming an honest
peer is a lie in a permanent record even when no chip follows it.
Two seats a Sybil controls stalling at one stage was the whole of the
attack. `TIMEOUT_CERT` had the same shape one level up, reachable by two honest
mutually partitioned seats voting against each other. Extending the key with
`subject_seat` and `subject_digest` closes both: the two events are in two slots
and there is nothing to prove.

**The key is longer; verification is still self-contained.** For
`event_class` 1 and 2 a checker decodes one field from a payload it already holds
— `subject_seat` is a `u8`, `subject_digest` is 32 bytes — under the same
canonicality gate the whole body passed. It still needs no table state, no
transcript and no knowledge of the game, which is the property that makes the
proof usable by anyone, forever.

**`DISPUTE` is deliberately on the unchained side of this line, and that is a
correctness requirement, not a convenience.** §6.3 step 2 obliges *every* peer to
emit `DISPUTE { kind = 1 STATE_DIVERGENCE }`, and step 4(a) obliges the same peer
to emit a second one carrying the `EquivocationProof`. Both concern the same hand
and, before this fix, both would have been chained with no rule assigning them
distinct `sequence` values — two bodies in one slot under §5.2.1's key, which
is this predicate satisfied against an honest peer by behaviour the protocol
*requires* of it. **A predicate that mandatory honest behaviour can satisfy is not
a predicate; it is a false-positive generator.** With `chain_scope = 0` and the
sentinel envelope of §2.3 the slot does not exist, so no number of honest disputes
can manufacture evidence against the peer that filed them. This is the same defect
class A-4 closed for lobby and join traffic, and it is now closed for the one
message type whose entire purpose is to carry evidence.

This definition works because of the stage rule, **restricted to chained
events**: a sender emits at most one event per stage per class per type per
subject, so the slot key of §5.2.1 has capacity one for `chain_scope == 1`, and
two distinct bodies in one slot is a contradiction the sender created and signed.
The capacity-one claim is the load-bearing one, and it is exactly what the
property above obliges each new type to re-establish. Two earlier revisions of
this paragraph stated it on too short a tuple: on the six-tuple, which was true
of every ordinary event and false of a timeout vote (M2), and then on the
seven-tuple, which was true of every ordinary event and false of the terminal
abort (G1). Unchained traffic has no such slot — a `JOIN_REQUEST`,
two successive honest lobby adverts and a `HELLO` all sit outside any chain — and
applying the predicate there would manufacture false positives against honest
peers. That is why `chain_scope` exists and why the sentinel envelope values of
§2.3 are mandatory rather than cosmetic: the scope discriminator alone would
still leave a `JOIN_REQUEST` carrying a real `table_id`, which is a collision
waiting for a future chained message type.

It catches:

* **two different bodies of one message type at one turn or one stage** — two
  `ACTION_RAISE` events for one turn with different amounts, two `DECK_INIT`
  contributions with different shares, two `STATE_HASH` values for one
  reconciliation round;
* **the same event chained to two different parents** — a deliberate fork —
  because `previous_event_hash` is inside the body and, deliberately, not in the
  key (§5.2.1).

It no longer catches two *different* `event_type`s at one turn — "I folded" to
one peer and "I raised" to another. That pair is now a validity rejection at
every receiver rather than a proof, for the reason and at the price §5.2.1
states.

**What it does not catch, and never did.** A `TIMEOUT_VOTE` and a real event for
the same stage are **not** an equivocation. The vote is signed by the voter and
the action is signed by the subject: they are two events by two different keys
and constitute no proof against anyone. Since A-5 they are not even in the same
slot, because a vote carries `event_class = 1` and the action `event_class = 0`.
The earlier claim that this pair made the D-006 timeout race safe was false under
this document's own predicate and is deleted; §8.3 states what actually settles
the race.

**Normative — an unchained event has no consensus effect. This is canonical for
the corpus, it is what makes the engine's total order complete, and it is the
disposition of defect P2.**

> **No `chain_scope = 0` event may terminate a stage, end a hand, move a chip, or
> change any value that enters a `stage_hash`, a `STATE_HASH`, or a
> `HAND_COMPLETE` / `HAND_ABORT` body.** An unchained event is *evidence and
> transport*: it is validated, retained, shown to the user and written beside the
> transcript (§3.3), and the engine's state is bit-identical whether it arrived,
> arrived late, or never arrived at all.

The reason is mechanical rather than aesthetic. `STATE_MACHINE.md` §3.2 removes
network arrival order with an ordering buffer keyed on
`(table_id, hand_id, sequence, previous_event_hash)`, and §2.3 obliges every
unchained event to carry a **sentinel in all four** of those fields. The buffer
therefore cannot place an unchained event anywhere in the order; it reaches the
engine at whatever local moment the network delivered it. Any consensus effect
granted to such an event is an effect whose position in the order **the attacker
chooses, by choosing a delivery order** — and two honest peers then derive
different bodies for one terminal stage.

**The concrete failure this closes.** At a four-seat table, seat `X` stalls at a
crypto stage with `|V(X)| = 3` and a `kind = 2` certificate completes;
independently, a genuine `EquivocationProof` against seat `Y` is in flight. A peer
that applies the certificate first derives
`HAND_ABORT{cause = 1, attributed = [X], cert_hash = Some(h)}`; a peer that
applies the proof first derives `HAND_ABORT{cause = 5, attributed = [Y]}`. §4.10
obliges every receiver to recompute every field, so each rejects the other's copy
and **two honest peers end one hand with two different terminal bodies**. Under
the shape in force when this was written that also meant `TERMINAL(k)` was never
defined; since §3.1 that particular consequence is gone, because
`ABORT_TERMINAL(k)` does not read the abort body at all. The defect the ordering
rule closes is the one that survives regardless: two peers whose *state* after
the hand differs. And it remains true that if `TERMINAL(k)` did depend on the
body, `GENESIS(k+1)` — a function of it — would never exist, so the session could not start
another hand either. Neither artefact is forged; both are genuine. The attacker
supplies only the delivery order.

**Both dispositions were available, and the second is taken.** Either define a
position in the order for unchained events — the natural candidate is "applied at
the earliest stage boundary at or after the later of the proof's two embedded
events' `sequence`" — or take away their ability to terminate a stage. The first
buys a consensus rule this corpus has now failed to specify correctly four times,
and buys it for a mechanism that under D-010 achieves nothing: a proof that moves
no chips and evicts nobody has no terminal effect worth ordering. Deleting the
effect is the smaller specification and it is the one D-010 makes available.
`SPEC_CS.md` §36 prefers it to a construction.

**What it deletes: `HAND_ABORT cause = 5`.** A verifying `EquivocationProof` no
longer ends a hand from any phase, and `5` is removed from §4.10's `cause`
enumeration. Every surviving cause is a function of chained content, which the
ordering buffer *can* place, so two honest peers derive one terminal body from one
total order. The sentence that used to stand here — *"A verifying
`EquivocationProof` naming a seated participant of this table aborts the hand with
`cause = 5` from any phase in which a hand is live, not only from a divergence"* —
is withdrawn in full.

#### 5.2.4 What happens instead when a peer equivocates

Stated so nothing is left to infer, and this is also the route by which the two
cross-type conflicts §5.2.1 no longer labels as equivocation are caught.

The two conflicting events share a slot — or, when they differ in `event_type`,
the same stage cell — so §4.0 step 10a or step 12 rejects whichever
reaches each peer second, as a violation; peers that accepted different first
copies now hold different state; the next checkpoint (§6.2) shows two `state_hash`
values; §6.3 runs; and the hand ends through that **chained** path, neutrally,
like every other abort. **The outcome is identical either way**, which is what
makes the labelling difference §5.2.1 accepts affordable. The proof is still built, still broadcast in a `DISPUTE`,
still retained forever and still shown to the user. What it no longer does is
decide anything. And if no peer ever diverges, there was nothing to decide — an
equivocation nobody's state disagreed about cost nobody anything.

**The proof object.**

```
EquivocationProof  #[cbor(array)]
  n(0) accused    : bytes[32]     the equivocating public key
  n(1) event_a    : bytes         complete SignedEvent, <= 32768 B
  n(2) event_b    : bytes         complete SignedEvent, <= 32768 B
```

Carried as the payload of a `DISPUTE` with `kind = EQUIVOCATION`. Whether it is
also broadcast on the lobby topic, so that peers who were never at the table hold
durable evidence, is **OPEN QUESTION Q-05**.

**Verification is self-contained.** A checker needs nothing but the two byte
strings and Ed25519: gate both, verify both signatures under `accused`, check that
both carry `chain_scope == 1`, check that `protocol_version`, `table_id`,
`hand_id`, `sequence`, `event_class` and `sender_public_key` agree, **and, when
`event_class` is 1 or 2, decode the one subject field named in the slot key and
check that it agrees too**, then check the two `event_hash`es differ. No table
state, no transcript, no knowledge of the game. That is the property that makes
it usable as evidence by anyone, forever. A checker that skips the subject field
accepts proofs that name honest voters, which is the whole of defect M2.

**What it proves, and what it does not.** It proves that the holder of that
Ed25519 secret key produced two conflicting signed statements for one slot. It
does **not** prove which one is "real", it does not prove intent, and it does not
distinguish a cheating player from a player whose key was stolen or whose client
ran twice against the same profile. The last case is a genuine false-positive
source and the client must make it impossible to run two instances against one
profile directory. Conclusions drawn beyond "this key signed two conflicting
things" are unwarranted, and the UI must not draw them.

**Consequence, under D-010: none that is automatic.** The proof is retained, kept
beside the transcript (§3.3), broadcast in a `DISPUTE`, and shown to the user. The
hand does **not** abort on it, no chip moves, the accused is not unseated, and no
key is added to any block list. The sentence this paragraph carried — *"The hand
aborts with `cause = 5`, the accused is attributed, the peer is added to
`libp2p::allow_block_list` [LIBP2P §1], and the proof is retained"* — specified
three consequences and two of them are deleted: the abort by the ordering rule
above, the block list by D-010 point 3. What survives is what play money could
ever have delivered anyway — a public, permanent, independently verifiable record
— and whether to sit with that key again is a **user** decision.
`SPEC_CS.md` §18 forbids claiming more; this now claims less than it did.

**Normative — how a proof is treated. This box is canonical for the corpus;
every other document references §5.2 and reproduces none of it. It is the single
answer to G2, and it is stated as a plain negative because that is what it is.**

> A verifying `EquivocationProof` is **retained as evidence, in every phase, at
> every table, and is never silently discarded** — whether or not a hand is live,
> whether or not the accused is a seated participant, and whether or not anyone
> has diverged. Verification needs no table state, no transcript and no knowledge
> of the game, so there is never a reason to refuse to hold one.
>
> **Nothing consumes it.** There is no transition, in any document, that takes an
> `EquivocationProof` as its input event. It ends no hand, moves no chip, unseats
> nobody, block-lists nobody, refuses nobody a seat, and produces no
> `AbortRecord`. Whether it is forwarded beyond the table is Q-05.
>
> **There is no wire representation for an equivocation-caused abort and none
> will be added.** §4.10's `cause` enumeration is `1`–`4`; value `5` is deleted
> and the code point is not reused. An engine alphabet containing an
> `AbortKind::Equivocation`, or a transition producing one, names an outcome no
> legal `HAND_ABORT` can carry, and is a defect in that document.

**G2, decided.** The gate found `STATE_MACHINE.md` T55 consuming a proof, ending
a hand with it, and producing an `AbortRecord{kind: Equivocation}` whose `cause`
value this document had deleted, while T55 and T56 both asserted that "the
accused is blocked at the protocol layer (`PROTOCOL.md` §5.2)". All three
statements are wrong and this box is why. The simple answer is the true one:
under D-010 a proof has no consequence, so **nothing consumes it**. `PROTOCOL.md`
owns the wire and therefore owns what a message may cause; `STATE_MACHINE.md`
owns the transitions and follows this box (D-011 rule 1). T55 loses its
`HandAborted` destination and its `AbortRecord` and becomes T56's shape; the
"blocked at the protocol layer" sentence is false in both rows and in
`NETWORK_STACK.md` §6.6 and §11.5, because **no layer blocks anything on a proof**
(D-010 point 3, D-011 rule 3).

Why retention is stated this widely, even though nothing follows from it:
equivocation is *detected* precisely when things look fine to the detector — it is
two peers comparing what they each received, which is §1.5's forwarding rule
working as designed — so the common case is a proof arriving with no divergence in
sight. An engine that only held proofs raised inside its divergence phase would
discard the strongest evidence this protocol can produce, in the case where it
most often appears, for want of a place to put it. The evidence is the whole point
of producing the proof at all now that no transition consumes it, which is exactly
the question §12 records and does not answer.

### 5.3 Bounded anti-replay state

Anti-replay must not become a memory-exhaustion vector (`SPEC_CS.md` §17, §27).

**The stored state is one structure per `event_class`, and each is indexed by
exactly the components of §5.2.1's key that vary within one `(table_id, hand_id,
sender)` — nothing more and nothing less.** The three fixed components
(`protocol_version`, `table_id`, `hand_id`) are the scope of the structure rather
than part of its index, and `sender_public_key` is the seat. That is the whole
derivation; this section defines no key of its own.

* **`event_class == 0`.** One `Vec<Option<event_hash>>` per table per hand,
  indexed by `(stage, seat, event_type_slot)`, where `event_type_slot` has
  capacity **2** — a seat's own contribution to stage `s`, and the terminal
  `HAND_ABORT` of §4.10, which lands at a stage the seat has usually already
  contributed to and is the only second `class = 0` type an honest emitter
  produces at one `sequence` (§4.11 row 36). Bounded by `MAX_STAGES_PER_HAND ×
  MAX_SEATS × 2`. `MAX_STAGES_PER_HAND = 2048`; a legitimate hand uses well
  under 200 stages and fills the second cell at most once, in the hand's last
  stage. **The `event_type` axis is not optional**: without it the abort and the
  contribution collide, which is defect G1.
* **`event_class` 1 and 2.** A per-stage map keyed by `(seat, subject_seat)` for
  votes and `(seat, subject_digest)` for certificates, **allocated only for a
  stage at which such an event has actually been accepted**. At most `MAX_SEATS ×
  (MAX_SEATS − 1) = 90` entries per stage per class, and the number of stages is
  bounded as above. `event_type` needs no axis here because each class holds
  exactly one type. **The subject axis is not optional**: without it two votes by
  one honest voter about two seats at one stage collide, which is defect M2.
  Sparse rather than dense because the dense form is `MAX_STAGES_PER_HAND ×
  MAX_SEATS × 2 × MAX_SEATS` slots for a structure a legitimate hand populates a
  handful of times — the deadline classes only ever touch a stage that stalled.

**Why the three structures are separate rather than one map (R-4).** The
sentence that stood here said the `event_class` axis "is what lets a seat hold
both its own contribution at stage `s` and a `TIMEOUT_VOTE` about stage `s`",
immediately after an index that did not contain `event_class`. That was true in
effect and wrong in wording: the class axis is expressed by *these being three
structures*, not by a component of any one index. The separation is the axis.
* Per table, per hand, per sender: a count of accepted distinct `DISPUTE`s and
  their `event_hash`es, bounded by `MAX_DISPUTES_PER_SENDER_PER_HAND = 8`
  (§4.9). `DISPUTE` is unchained, so it has no slot in the array above and needs
  its own bound; without one, the one message type that is legal at any time
  would be the one unbounded allocation in the section.
* A hand exceeding `MAX_STAGES_PER_HAND` aborts with `cause = 4`.
* Finished hands keep only `TERMINAL(k)` and the terminal body — the
  `HAND_COMPLETE` body, or the accepted `HAND_ABORT` copy — in memory;
  the full transcript is written to the profile directory and dropped from RAM.
* The lobby keeps at most `MAX_TRACKED_TABLES = 4096` `(table_id, last_timestamp)`
  pairs in an LRU, and at most `MAX_TRACKED_PRESENCE = 8192` peers.

---

## 6. `STATE_HASH`, `STATE_ACK` and divergence (`SPEC_CS.md` §15)

### 6.1 What is hashed

```
state_hash = h("p2p-poker v1 state", [ canonical_cbor(PublicTableState) ])
```

`PublicTableState` is a `#[cbor(array)]` struct containing exactly:

| Field | Notes |
|---|---|
| `protocol_version`, `table_id`, `hand_id`, `checkpoint` | |
| `roster` | `Vec<(seat, app_public_key, stack)>`, ascending by seat |
| `button_position`, `sb_position`, `bb_seat` | positions, which may be empty seats [RULES A1.3] |
| `level`, `small_blind`, `big_blind`, `ante` | |
| `street` | `u16` |
| `board` | `Vec<u8>` card codes, length 0/3/4/5 [RULES A2] |
| `committed_this_round`, `committed_this_hand` | `Vec<u64>` by seat |
| `folded`, `all_in`, `acted_this_round`, `sitting_out`, `absent` | `Vec<bool>` by seat |
| `current_bet`, `last_full_raise` | |
| `player_to_act` | `Option<u8>` |
| `pots` | derived `Vec<PotView { size, eligible }>` [RULES A7] |
| `deck_commitment` | `final_deck_hash` from `DECK_COMMIT`, or 32 zero bytes before it |
| `ledger_in`, `ledger_out` | `u64` each; the running totals of accepted buy-ins and of removed stacks (§4.4's `ledger_delta`). They are inside `state_hash` because otherwise two peers could disagree about the ledger and never detect it (C-9) |
| `transcript_head` | `stage_hash` of the last completed stage |

Card code: suits clubs = 0, diamonds = 1, hearts = 2, spades = 3; rank index
2 → 0 … A → 12; `code = rank_index * 4 + suit_index`, so `0 ≤ code ≤ 51`. This is
the protocol's encoding and it must match the deck layer's card → group-element
map.

**Deliberately excluded, each for a reason:**

* wall-clock times of any kind — §2.6;
* any hand-evaluator score — [RULES A′4], its numeric encoding is a crate
  implementation detail;
* hole cards, reveal tokens, or anything a peer holds but others do not;
* PeerIds, multiaddrs, connection state, relay status — network facts, not game
  facts, and they legitimately differ per peer;
* display names — they are untrusted strings and must never affect state.

Including the derived `pots` is deliberate redundancy: they are a pure function of
`committed_this_hand` and `folded`, so including them makes an engine divergence
in the pot layering visible at the checkpoint instead of at the award.

### 6.2 Checkpoints

`STATE_HASH` is emitted as a collective stage, followed immediately by a
`STATE_ACK` collective stage, at these points and no others — plus, when a
divergence is being resolved, the **reconciliation stage** of §4.9 and §6.3 step
3, which re-derives one of these seven and carries its `checkpoint` number rather
than a number of its own:

| `checkpoint` | When |
|---|---|
| `1` | after `TABLE_READY` completes |
| `2` | after `DECK_COMMIT` completes |
| `3` | after the pre-flop betting round closes |
| `4` | after the flop betting round closes |
| `5` | after the turn betting round closes |
| `6` | after the river betting round closes |
| `7` | immediately before `SHOWDOWN_REVEAL` |

**No checkpoint is placed after a hole card has been opened to anyone but its
owner.** The rule is about *public* opening and is stated that way rather than in
the shorter form an earlier revision used — "no checkpoint is placed after any
hole card has been opened" — which is false on its face against the table
directly above it: every player opens its own two cards at `DEAL_PRIVATE`, and
`DEAL_PRIVATE` precedes checkpoints 3 through 7. `THREAT_MODEL.md` X29 carries the
same correction in the same words; this is the section that states the rule
normatively and the two must stay in step.

The reconciliation stage does not weaken that rule, because it opens nothing and
places no new checkpoint: it re-derives a checkpoint that was already placed, at
the point the table is frozen at, and its position in the hand is fixed by the
checkpoint it settles rather than chosen by an emitter.

A checkpoint after the cards are known **to more than their owner** is an abort
trigger the loser can pull with full information: a peer that publishes a
`state_hash` it did not derive forces the §6.3 case (c) path, and until this fix
checkpoint 7 sat immediately before `HAND_COMPLETE`, which is after the showdown.
Moving it before `SHOWDOWN_REVEAL` does not close the exploit — see §6.4 and X29 —
but it forces the attacker to commit to burning the table while it still knows
only its own hand. That is the property the rule protects, and knowing one's own
two cards does not threaten it: every seat has that knowledge, symmetrically, from
`DEAL_PRIVATE` onwards.

`HAND_COMPLETE` needs no checkpoint of its own, because it is now a collective
derived stage (§3.2, §4.10): every receiver recomputes every field, and a
mismatching copy is rejected at the stage rather than caught later at a
checkpoint.

`STATE_HASH` publishes each peer's own computed hash. `STATE_ACK` then confirms
that this peer saw the complete set and that every member of it agreed — giving a
definite, chained "we all agreed here" point that a later dispute can name. The
two-stage form matters: a peer that publishes a matching `STATE_HASH` but then
refuses to `STATE_ACK` is stalling, and is handled by §8 rather than by ambiguity.

A `BOARD_REVEAL` stage runs only after the preceding street's `STATE_ACK` stage
completes. That is what guarantees no card opens while peers disagree about the
state.

### 6.3 Divergence: the resolution procedure

"The host is always right" is explicitly forbidden (`SPEC_CS.md` §15). There is no
host. The procedure is:

**Step 1 — freeze.** The moment any peer observes two distinct `state_hash`
values at one checkpoint, it stops accepting and stops emitting hand events. No
card opens, no action is applied, no chips move. Silently continuing is what §15
forbids. **Which phase the engine is in while frozen, and which transitions carry
steps 2–4, is `STATE_MACHINE.md`'s** (D-011 rule 1); the sentence that named the
phase and mapped the steps onto its transitions is deleted. What this section
specifies is the **messages** the procedure emits and the **stages** they occupy,
and nothing else.

**Step 2 — declare.** Every peer broadcasts `DISPUTE { kind = 1 STATE_DIVERGENCE,
at_sequence = the checkpoint stage, table_id, hand_id, evidence = its own
STATE_HASH event }` and, on request, its full transcript for the hand over the
table mesh.

**The two disputes this procedure requires are not an equivocation.** Step 2
obliges every peer to emit one, and step 4(a) obliges the same peer to emit a
second; a peer that does both has done exactly what this section demands.
`DISPUTE` carries `chain_scope = 0` (§2.3, §4.9), so §5.2's predicate cannot
reach it, and no number of disputes from one honest key produces an
`EquivocationProof` against that key. Emitting the second dispute is mandatory,
not optional-and-risky, and an implementation that withholds it to stay safe is
wrong.

**Step 3 — reconcile transcripts, then re-derive in a new stage.** Peers exchange
the events they are missing. Every event is individually verifiable, so this step
cannot be poisoned: an event either gates, verifies and chains, or it is
discarded. Most real divergences end here — a peer missed one event because of a
dropped stream and fills the gap.

Each peer then publishes a fresh `STATE_HASH` for the disputed checkpoint **at a
new `sequence`**, chained from that checkpoint's `stage_hash`, in the
reconciliation stage §4.9 defines. That stage is the artefact the engine consumes:
the divergence is **resolved** when the stage completes carrying one value, and
**unresolved** when it completes carrying two, and the engine needs no other
signal.

**The sentence deleted here read "Play resumes from the checkpoint with no further
action."** It was the opposite of what the engine needed — `STATE_MACHINE.md`'s
two resolution transitions were left triggered by an event no wire rule produced —
and the obvious repair, re-emitting into the disputed checkpoint's own slot, is
worse than the gap: it is two distinct bodies of one honest signer in one
capacity-one slot, and it manufactures an `EquivocationProof` against the peer
whose only fault was a dropped stream (P1, D-009 rule 1). A new `sequence` costs
one stage and closes both halves at once.

**Step 4 — classify what remains.** After reconciliation, exactly one of:

**(a) The transcripts differ, and there exist two conflicting signed events at one
slot.** Then an `EquivocationProof` exists at the earliest diverging stage. It is
constructed (§5.2), broadcast in a `DISPUTE`, and **retained as evidence**. It
does not end the hand and it moves no chips: an unchained event has no consensus
effect (§5.2), and under D-010 no consumer of a proof moves a chip or removes a
player. The hand ends through this procedure's own chained path — step 3's
reconciliation stage settling on two values, which is case (c) — and it ends
neutrally like every other abort.

The sentences deleted here read: *"and the hand aborts with `cause = 5`,
attributed to the equivocator. The equivocator's committed chips are forfeited per
D-005. This is the attack case, and it is fully resolved with cryptographic
evidence."* The first two are deleted by §5.2's ordering rule and by D-010. The
third was always too strong, and goes with them: the evidence resolves *that a
key signed two conflicting things*, which is all it ever proved. It never
resolved which of the two was real, nor intent, nor a stolen key, nor a client
run twice against one profile — §5.2 says so directly two paragraphs above, and
"fully resolved" contradicted it.

**(b) The transcripts differ, but only because one peer is missing events it
cannot obtain** — every holder refuses to serve them. **The hand aborts
neutrally**: `cause = 1`, `attributed = []`, `cert_hash = None`, every stack
restored to its start-of-hand value, and the table is **not** faulted. §4.10
carries this as the third `attributed = []` path and its `cert_hash` rule now
permits it.

That is a change of shape, not of outcome. This case previously specified
`cause = 1` with `attributed = []` and no certificate — which §4.10's `cert_hash`
rule forbade and its "exactly two such cases" enumeration excluded, so it named a
terminal outcome no legal event could carry and no transition consumed (P6).
D-010 dissolves it rather than repairing it: with every abort neutral there is no
chip disposition left for the case to be special about, so it needs no rule of its
own beyond being listed. `STATE_MACHINE.md` needs the matching transition out of
its divergence phase; it is the same neutral terminus as case (c), differing only
in the `cause` recorded and in the table not being faulted.

The transcript records which peers were asked for which events and did not serve
them, which is publicly readable but is **not adjudicable inside the protocol**,
because any adjudication that requires the accused peer's signature is circular at
every table size (D-007 point 4). A dispute path that does not require the accused
peer's signature does not exist and is not designed here: **OQ-D** (§12). D-010
does not close OQ-D — it only makes the cost of leaving it open a wasted hand
rather than a stalemate over chips.

**(c) The transcripts are byte-identical after reconciliation, and derived states
still differ.** No signature distinguishes an implementation bug from a client
lying about its own derived state, and there is no observer-independent
derivation at run time — every peer derives with its own engine, so "attribute
whoever disagrees with the derivation" is a vote over the facts, which
`SPEC_CS.md` §15 forbids. The hand therefore aborts with `cause = 4`,
`attributed = []`, stacks restored, and the table is faulted (§6.4). No peer is
removed and no peer is named. The evidence — the unanimous transcript plus every
peer's signed `STATE_HASH` — is written to the profile directory, where **it is
preserved and is sufficient for a human, or for a future adjudicator, to diagnose
the divergence. No adjudication procedure is specified.** Live adjudication is
not available and is not claimed; neither is offline adjudication.

**Why that sentence is weaker than it used to be.** It previously said the
evidence is "publicly adjudicable offline by anyone who runs the reference
engine over it, forever". **There is no reference engine.** No document in this
corpus defines one, versions one against `protocol_version`, or says how two
parties establish that they ran the same one. `STATE_MACHINE.md` specifies *the*
deterministic engine as something every client implements — which is precisely
the plurality this case (c) exists to describe — not as a normative artefact a
third party runs, and D-4 defers the source tree to `docs/ARCHITECTURE.md`, so
there is not even a named binary. Moving a derivation offline does not create the
observer-independent reference that the paragraph above spends its length
establishing does not exist; it only changes when the disagreement happens.
Defining that reference is **OQ-A** (§12) and is carried in `DECISIONS.md`'s open
list. Until it is defined, what the artefact supports is diagnosis, not
adjudication, and `SPEC_CS.md` §36 forbids the stronger word.

**This is the same disposition at every `n`.** An earlier draft of this section
resolved case (c) by *unanimity minus one*: if every peer but one derived the same
`state_hash`, the odd peer out was removed from the table. That rule is deleted.
It bought nothing real — naming the culprit requires the observer-independent
derivation that does not exist — and it cost an attack in which `n-1` colluders
falsely claim a divergent state hash, evict one honest player and take their
committed chips under D-005's forfeiture, available at `n = 3` with two
colluders. D-010 has since removed both halves of that payoff — there is no
eviction and no forfeiture to collect — but the rule stays deleted, because a
rule that names a culprit from a vote over the facts is forbidden by
`SPEC_CS.md` §15 whatever it pays. One liar can now fault any table instead. That is a liveness and
griefing cost and it is accepted in preference to an integrity cost: losing a
table is recoverable, having your chips taken by a coalition is not.
`THREAT_MODEL.md` X29 carries what remains as a named, unsolved attack.

### 6.4 A faulted table

`cause = 4` faults the table **at every `n`**: it is closed, no further hand is
dealt, every stack is restored to its start-of-hand value, and the client writes a
reproducible divergence report to the profile directory containing the full
transcript, every peer's `STATE_HASH`, and its own derived `PublicTableState`.
That report is the bug report, and it is also the diagnostic material of
§6.3 case (c). The UI must say plainly that the game ended because the
participants could not agree, and must not guess at fault. **How the engine
reaches this state, from which phase and with which fault record, is
`STATE_MACHINE.md`'s** (D-011 rule 1); the sentence that named the phase and the
fault variant is deleted.

**`cause = 4` still yields a `TERMINAL(k)`.** Faulting the table means no further
hand is dealt; it does not mean the chain has no end. The abort is a
witness-independent terminal stage like every other (§3.2) and
`TERMINAL(k) = ABORT_TERMINAL(k)` (§3.1), so the transcript closes cleanly and a
verifier can check it end to end.

**Restoration is no longer this cause's peculiarity; it is every abort's
disposition (D-010, §4.10).** The clause that called `cause = 4` "the one
`AbortKind` settled by restoration rather than by the §8.6 forfeiture formula" is
deleted, because there is no forfeiture formula left for anything to be an
exception to. What remains peculiar to `cause = 4` is the *table fault*: it is the
only cause after which no further hand is dealt.

**Restoration here is not safe, and now it is not safe anywhere.** A peer that
publishes a false `state_hash` at a checkpoint can fault any table at any time and
recover its own commitment. That is an in-protocol exploit, it is **not closed**,
and `THREAT_MODEL.md` X29 carries it. Under D-005 it was the one path on which a
losing player could recover its chips; under D-010 it is one of several, and it is
now the *expensive* one, since stalling to `hand_deadline_ms` recovers the same
chips without costing the attacker the table. That does not make it smaller — it
makes the class larger, which §4.10 states in full and `THREAT_MODEL.md` X8 carries. **Two**
things bound this particular member of it and neither removes it: it costs the
attacker the table and, in a tournament, their own equity in it; and checkpoint 7
sits before `SHOWDOWN_REVEAL` (§6.2), so the attacker must commit before learning
whether it won.

A third bound was previously claimed here and is **withdrawn**: *"the evidence is
deterministically adjudicable offline by any third party running the reference
engine over the unanimous transcript."* No reference engine exists to run — see
§6.3 case (c) and **OQ-A** (§12). What survives is weaker and is all that may be
claimed: the evidence is preserved and is sufficient for a human, or for a future
adjudicator, to diagnose the divergence; no adjudication procedure is specified.
That is a diagnostic property, not a bound on the attack, which is why the count
here is two and not three. `THREAT_MODEL.md` X29 must not quote the withdrawn
sentence as one of its three bounds.

The alternatives — forfeiting an unnamed party's commitment, or settling from the
last `STATE_ACK`-agreed checkpoint — each need a numbered owner decision and
neither is adopted here. **Q-07** (§12), which D-010 narrows rather than closes:
the forfeiture half is now decided *against* for the MVP by a numbered decision,
so what Q-07 still asks is whether settling from the last agreed checkpoint is
worth building, and that question is unchanged and unanswered.

---

## 7. The lobby protocol

**Every message in this section is unchained**, stated once here rather than
repeated per message type: `chain_scope = 0`, `event_class = 0`,
`table_id = ZERO32`, `hand_id = 0xFFFF_FFFF_FFFF_FFFF`, `sequence = 0`,
`previous_event_hash = ZERO32` (§2.3). None of them can take part in an
`EquivocationProof` (§5.2), and their anti-replay is per-type — advert
`timestamp_unix_ms` monotonicity per `table_id`, the snapshot `nonce`, the chat
sender's own `timestamp_unix_ms`.

Nothing is lost by zeroing the envelope's `table_id`: an advert's table identity
is `sender_public_key == table_public_key`, which is stronger, and every other
lobby message names the table it concerns in its payload.

`NETWORK_STACK.md` §7.4's rule for two conflicting adverts from one table key is a
**local lobby-hygiene rule, not an `EquivocationProof`**, precisely because lobby
messages are unchained; that section was corrected to say so.

### 7.1 Topic and transport

* GossipSub topic: `IdentTopic::new("/p2p-poker/lobby/1")`. [LIBP2P §6]
* `MessageAuthenticity::Signed(libp2p_keypair)` and
  `ValidationMode::Strict` — transport hygiene, not the application signature.
* `validate_messages()` on, so nothing is forwarded until the application has
  checked the *application* signature and the expiry, and then calls
  `report_message_validation_result` with `Accept`, `Reject` (which applies the
  peer-score penalty) or `Ignore` (which does not). [LIBP2P §6]
* `message_id_fn` **must** be overridden to a hash of `(data, topic)`. The default
  message id is `source ‖ seqno`, so one peer can republish byte-identical content
  under fresh sequence numbers forever and it is never deduplicated. That single
  default makes lobby spam nearly free. [LIBP2P §6]
* `flood_publish(false)`. The default `true` sends every publish to every known
  peer in the topic rather than to the mesh — an amplification lever we do not want
  on a public lobby. [LIBP2P §6]
* `duplicate_cache_time(120 s)`, which must exceed the advertisement rebroadcast
  interval.
* Mesh: `mesh_n = 8`, `mesh_n_low = 6`, `mesh_n_high = 12`,
  `mesh_outbound_min = 3`. The last raises the cost of an eclipse attack by
  inbound-only Sybils. [LIBP2P §6]
* `max_transmit_size` = `GOSSIP_MAX_TRANSMIT` = 65 536 bytes. **This is a
  two-sided protocol constant, not a tuning knob**: a peer with a different value
  rejects our frames. Changing it is a major-version change. [LIBP2P §6, open
  item 2] The value is `libp2p-gossipsub` 0.49.5's own default
  (`src/config.rs:244-246`), and pinning there maximises interoperability.
* `GOSSIP_MAX_TRANSMIT` is the **transport** ceiling and is a backstop, never the
  operative limit. The **application** ceiling for any lobby message of any type
  is `LOBBY_MSG_MAX` = 8 192 bytes, and the per-message payload caps of §9.3 are
  tighter still. Conflating the two is what produced the earlier
  `LOBBY_MAX_MESSAGE = 16 384`, which is deleted.

The lobby chat topic `/p2p-poker/lobby-chat/1` (§7.7) is a second `IdentTopic`
with the same settings; it is separate so that a client can subscribe to tables
without subscribing to chat.

### 7.2 `0x0101 LOBBY_TABLE_AD`

Signed by the table key: `sender_public_key` **is** the `table_public_key`, and
that is the advert's whole table identity — the envelope's `table_id` is the
unchained sentinel `ZERO32` (head of §7). The payload carries
`SPEC_CS.md` §4's field list plus the parameters the table needs to be
unambiguous before the first card exists.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) game` | `u16` | `1` = NLHE. Nothing else is defined in version 1. |
| `n(1) mode` | `u16` | `1` = `CASH_PLAY_MONEY`, `2` = `TOURNAMENT_SNG_PLAY_MONEY` |
| `n(2) preset_id` | `bytes` | ≤ 32 B ASCII; `"RATED_SNG_POKERTH_V1"` or `"CUSTOM"` |
| `n(3) table_name` | `bytes` | ≤ 64 B UTF-8, no control characters; display only |
| `n(4) small_blind` | `u64` | ≥ 1 |
| `n(5) big_blind` | `u64` | `== 2 * small_blind` |
| `n(6) ante` | `u64` | `0` in version 1 |
| `n(7) min_buyin` | `u64` | ≥ `big_blind` |
| `n(8) max_buyin` | `u64` | ≥ `min_buyin` |
| `n(9) start_stack` | `u64` | tournament modes only; `0` for cash |
| `n(10) players` | `u8` | currently seated, ≤ `max_players`; advisory |
| `n(11) max_players` | `u8` | `2 ≤ max_players ≤ 10` [RULES B1] |
| `n(12) min_players_to_start` | `u8` | `2 ≤ … ≤ max_players` |
| `n(13) blind_schedule` | `BlindSchedule` | see below |
| `n(14) action_timeout_ms` | `u32` | `5_000 ≤ … ≤ 300_000` [RULES B3] |
| `n(15) action_grace_ms` | `u32` | `≤ 30_000` |
| `n(16) crypto_step_timeout_ms` | `u32` | `1_000 ≤ … ≤ 120_000` |
| `n(17) hand_deadline_ms` | `u32` | `≤ 3_600_000` |
| `n(18) join_deadline_ms` | `u32` | `≤ 3_600_000` |
| `n(19) hand_delay_ms` | `u32` | `≤ 60_000` |
| `n(20) button_rule` | `u16` | `1` = `DEAD_BUTTON` (only value in version 1) |
| `n(21) odd_chip_rule` | `u16` | `1` = `FIRST_SEAT_LEFT_OF_BUTTON` (only value) |
| `n(22) showdown_policy` | `u16` | `1` = `MANDATORY_REVEAL` (default), `2` = `TDA_MUCK` — see Q-01 |
| `n(23) password_required` | `bool` | |
| `n(24) deck_suite` | `bytes` | ≤ 32 B; must be `"bs-bg12-secp256k1/1"` in version 1 |
| `n(25) founder_app_key` | `bytes[32]` | the founder's application key |
| `n(26) founder_peer_id` | `bytes` | ≤ 42 B; where to send `JOIN_REQUEST` |
| `n(27) timestamp_unix_ms` | `u64` | |
| `n(28) expires_at_unix_ms` | `u64` | `> timestamp_unix_ms` |

`BlindSchedule` = `#[cbor(array)] { n(0) mode: u16, n(1) every_n_hands: u16,
n(2) first_small_blind: u64, n(3) small_blind_cap: u64 }` with `mode = 1` meaning
`DOUBLE_EVERY_N_HANDS`. For `RATED_SNG_POKERTH_V1` the values are
`every_n_hands = 11`, `first_small_blind = 50`, `small_blind_cap = 50_000`, so
`small_blind(h) = min(50 · 2^(⌊(h-1)/11⌋), 50_000)` and `big_blind = 2 · small_blind`,
with `seats = 10`, `start_stack = 10_000`, `ante = 0`. Every one of those numbers
is enforced by PokerTH's own rated-game settings check, which is the strongest
available evidence of what "the rated preset" means. [RULES B1, B2, B4]

*Receiver must validate, before the advert is shown to a user or stored:*

1. `verify_strict` passes under `sender_public_key`, which is taken as the
   `table_public_key`; the envelope carries the unchained sentinels of §2.3;
2. every numeric range above, including `big_blind == 2 * small_blind`,
   `min_buyin ≤ max_buyin`, `2 ≤ max_players ≤ 10`,
   `min_players_to_start ≤ max_players`;
3. `preset_id == "RATED_SNG_POKERTH_V1"` implies the preset's exact values, or
   the advert is rejected — a preset name that does not carry the preset's values
   is a lie about what game is being offered;
4. `deck_suite` is a suite this client supports;
5. `expires_at_unix_ms > timestamp_unix_ms`, and `expires_at` is **not more than
   `MAX_AD_LIFETIME_MS = 300_000` ahead of local time**, and `timestamp` is not
   more than `MAX_CLOCK_SKEW_MS = 120_000` in the future. Without the first bound a
   malicious peer pins a table into every lobby forever, which is free spam and
   exactly what §4 requires us to prevent. [NAT §7.3]
6. if a `LOBBY_TABLE_AD` for this `table_id` is already held, `timestamp_unix_ms`
   must be strictly greater than the held one, or the message is discarded. This
   blunts replay of stale adverts. [NAT §7.3]

**Local eviction uses relative freshness, not absolute time.** An entry is dropped
`AD_TTL_MS = 90_000` after it was *received*, and `expires_at` is only an upper
bound on how long we are willing to hold it at all. This makes lobby liveness
independent of clock agreement. [NAT §7.3]

The table owner rebroadcasts every `AD_REBROADCAST_MS = 30_000` — a 3× margin
against GossipSub jitter and one missed beat.

### 7.3 `0x0102 LOBBY_TABLE_REMOVE`

Signed by the table key. `n(0) advert_hash: bytes[32]` — the `event_hash` of the
`LOBBY_TABLE_AD` being withdrawn, exactly as everywhere else —
`n(1) reason: u16` (`1` started, `2` closed, `3` full),
`n(2) timestamp_unix_ms: u64`.

This is an **optimisation for the polite case and never a precondition**. A
crashed client must vanish from every lobby without cooperation, which is exactly
what the TTL does. A receiver that never sees a `LOBBY_TABLE_REMOVE` behaves
identically 90 seconds later. [NAT §7.3]

### 7.4 `0x0103 LOBBY_PLAYER_PRESENCE`

Signed by the peer's application key. `n(0) peer_id: bytes(≤42)`,
`n(1) display_name: bytes(≤32)`, `n(2) capabilities: Vec<bytes>(≤32)`,
`n(3) timestamp_unix_ms: u64`, `n(4) reachability: u16` (`0` unknown,
`1` public, `2` behind NAT, `3` relayed).

TTL `PRESENCE_TTL_MS = 120_000`, heartbeat every
`PRESENCE_HEARTBEAT_MS = 40_000`. [NAT §7.3] The presence list is a convenience
for the lobby UI and is **never** an input to any game decision.

`reachability` is self-reported and therefore worthless as a security claim; it is
a hint for dial ordering only. A client's own reachability comes from AutoNAT v2,
and it should be shown prominently in its own UI, because a user who can open a
port materially helps everyone else (D-003).

### 7.5 Snapshot for a newly joined client

`SPEC_CS.md` §3 requires a new client to request a snapshot of existing tables from
several peers before relying on live GossipSub. This runs over
`request_response::cbor` on `/p2p-poker/lobby-snapshot/1`, **not** over GossipSub,
because a lobby with hundreds of tables would blow past any sane gossip frame.
[LIBP2P §6]

**`0x0104 LOBBY_SNAPSHOT_REQUEST`** — `n(0) max_tables: u16` (≤ 128),
`n(1) since_unix_ms: u64` (0 for everything), `n(2) nonce: bytes[32]`.

**`0x0105 LOBBY_SNAPSHOT_RESPONSE`** — `n(0) request_nonce: bytes[32]`,
`n(1) adverts: Vec<bytes>` (≤ `SNAPSHOT_MAX_ADS` = 128 entries, each a complete
`SignedEvent` of a `LOBBY_TABLE_AD`, each ≤ `TABLE_AD_SIGNED_MAX` = 1 536 B),
`n(2) truncated: bool`.

The response is a container of independently signed adverts. **The responder is
not trusted for anything.** Each embedded advert is validated by §7.2's full
checklist as if it had arrived over gossip; the responder cannot invent a table,
cannot alter one, and cannot extend one's lifetime. It can omit tables — so the
client queries `SNAPSHOT_PEER_COUNT = 4` independent peers and takes the union,
which makes omission by any single peer harmless. It can also send stale adverts,
which the `expires_at` and freshness checks discard.

Codec limits are set explicitly: `set_request_size_maximum(SNAPSHOT_REQ_MAX)` =
1 024 and `set_response_size_maximum(SNAPSHOT_RESP_MAX)` = 262 144. The response
fits by construction: `128 × 1 536 = 196 608` B plus array and envelope overhead.
[LIBP2P §7]

### 7.6 Anti-spam

`SPEC_CS.md` §4 requires expiry of stale adverts and protection against spamming.
The layers, cheapest first:

1. **Expiry** — §7.2's TTL, which costs nothing and removes the persistent-spam
   category entirely.
2. **Per-key rate limits**, enforced locally by every client:
   `MAX_ADS_PER_TABLE_KEY_PER_MIN = 4`, `MAX_ADS_PER_PEER_PER_MIN = 20`,
   `MAX_PRESENCE_PER_PEER_PER_MIN = 4`, and for `LOBBY_CHAT` **1 message per 2 s
   with a burst of 5, per remote `PeerId`**, matching `NETWORK_STACK.md` §6.6.
   Exceeding a limit gets
   `MessageAcceptance::Reject`, which applies the GossipSub P₄ score penalty to the
   forwarder as well.
3. **Bounded caches** — `MAX_TRACKED_TABLES = 4096`, `MAX_TRACKED_PRESENCE = 8192`,
   both LRU. A full cache evicts; it never grows.
4. **Content deduplication** through the overridden `message_id_fn` (§7.1),
   without which republication of identical bytes is free.
5. **Subscription filtering** — `MaxCountSubscriptionFilter` so a peer cannot
   subscribe us to thousands of junk topics. [LIBP2P §6]
6. **GossipSub peer scoring** (`PeerScoreParams`, `TopicScoreParams`) is the
   proper long-term answer and is exported by the crate. It is not configured in
   this document and is Phase 8 work. [LIBP2P §6]

**What none of this stops:** a Sybil with `k` fresh Ed25519 keys gets `k` times the
budget, and keys are free. Rate limits raise the cost of noise; they do not create
scarcity. Real Sybil resistance needs an identity or reputation layer, which
`SPEC_CS.md` §18 explicitly places outside the protocol. The honest statement is
that the lobby is spammable and the mitigations are bounds on the damage, not a
solution.

There is a related, larger exposure that belongs in `THREAT_MODEL.md` rather than
here: a fixed public `LOBBY_INFOHASH` publishes each player's IP address to
roughly 100 arbitrary internet hosts per announce cycle, and Phase 0 observed a
stranger re-announcing under a freshly generated random infohash within 24
minutes — DHT crawling seen first-hand, not hypothesised. [DHT §7] That is a
discovery-layer property, not a lobby-protocol one, but a reader of this document
should not come away thinking the lobby is private.

### 7.7 `0x0106 LOBBY_CHAT`

`SPEC_CS.md` §22 requires a chat pane in the lobby, so the message type exists
here rather than only in the transport document.

*Channel:* lobby chat broadcast — GossipSub on
`LOBBY_CHAT_TOPIC = /p2p-poker/lobby-chat/1`, not the lobby topic.
*Signed by:* the sender's application key.
*Envelope:* unchained, per the rule at the head of §7 (`chain_scope = 0`).

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) display_name` | `bytes` | ≤ 32 B UTF-8, §9.4 string rules |
| `n(1) text` | `bytes` | ≤ 512 B UTF-8, §9.4 string rules |
| `n(2) timestamp_unix_ms` | `u64` | advisory; strictly greater than this sender's previous, for anti-replay |

Payload cap `LOBBY_CHAT_MAX = 2 048 B`.

**Both string fields are display-only, never identifiers, never parsed, never
inputs to a state transition, and must reject control characters
`U+0000`–`U+001F`, `U+007F` and the bidi overrides `U+202A`–`U+202E`,
`U+2066`–`U+2069`.** These are the §9.4 rules, restated at the message rather than
inherited, because chat is the one message class that is pure attacker-controlled
UTF-8 reaching a UI and is therefore the field most likely to be attacked.

Chat carries **no security claim of any kind**. It is authenticated as coming from
an application key and nothing more; impersonation by display name is trivial and
expected, and a client must never present a chat line as evidence about who a
player is.

---

## 8. Deadlines and timeouts

### 8.1 The distinction that matters

D-006 corrects a conflation in D-005: "timeout" is two unrelated things.

| | Human absent, client running | Client gone or withholding |
|---|---|---|
| Betting decision | auto check/fold | auto check/fold |
| Decryption shares | published normally | **not published** |
| The hand | **continues to the end** | cannot open further cards |
| Consequence | next street, next hand | abort, attribute, next hand |

Publishing a reveal token is an automatic client step. It is not a decision and it
never waits for the human. So a player who walks away from the keyboard is still
fully cooperating cryptographically: the board opens on schedule, showdowns work,
the hand plays out. Only a betting decision is missing, and poker has always had
an answer to that.

Only the right-hand column is hard, and it is the rarer case.

**No timeout of any kind ends the tournament or the cash game.** At worst one hand
aborts, and only in the right-hand column. The next hand begins immediately
afterwards, automatically (D-006 point 5, `SPEC_CS.md` §4).

### 8.2 Agreeing that a deadline passed, without a trusted clock

**Deadlines are relative durations, never absolute times.**

Every event carries `next_deadline_ms` in its envelope: the duration within which
the *next* stage must complete. A receiver starts a local **monotonic** timer when
it accepts the event that completes stage `s`, and the deadline for stage `s+1`
expires `next_deadline_ms` later on that timer.

This removes clock synchronisation from the problem entirely. Two peers' timers
differ only by the propagation delay between them plus their processing time —
tens or hundreds of milliseconds — not by their clock offset, which can be hours.
`action_grace_ms` absorbs that spread, and it must absorb the P2P round trip,
relay hops for CGNAT peers, and signature verification. Every peer must use the
identical constant, because peers disagreeing about whether a timeout fired is a
consensus fault, not a UX detail. [RULES B4]

`next_deadline_ms` is **normative, not the emitter's choice.** Its value is a
deterministic function of the table parameters and the kind of the next stage:

| Next stage kind | `next_deadline_ms` |
|---|---|
| a betting action | `action_timeout_ms + action_grace_ms` |
| any cryptographic contribution (`DECK_INIT`, `SHUFFLE_*`, `DECK_COMMIT`, `DEAL_PRIVATE`, `BOARD_REVEAL`, `SHOWDOWN_*`) | `crypto_step_timeout_ms` |
| `STATE_HASH` / `STATE_ACK` | `crypto_step_timeout_ms` |
| a hand boundary (`HAND_INIT` after `hand_delay_ms`) | `hand_delay_ms + crypto_step_timeout_ms` |

A peer that writes a different value emits an invalid event. There is nothing to
negotiate and nothing to game.

**The whole-hand limit `hand_deadline_ms` runs on the same relative basis from
`TERMINAL(k-1)` — normative, and this is R-1's disposition.** The timer for hand
`k` starts when the peer accepts the event that fixes `TERMINAL(k-1)`: the
completing copy of the `HAND_COMPLETE` stage of hand `k-1`, or the terminal
`HAND_ABORT` of hand `k-1`, or, for a table's first hand, the completing copy of
the `TABLE_READY` stage. It is **not** started by `HAND_INIT`.

Two reasons, and the second is a correctness precondition rather than a
preference:

1. **A stalled `HAND_INIT` is then covered.** Under the old reading a `HAND_INIT`
   collective stage that never completed started no timer and was covered by
   none: §4.3's abandonment timer stops at `TABLE_READY`, and from hand 2 onward
   there is no other. A seat that vanishes between two hands froze the table
   permanently. Now the timer is already running when `HAND_INIT` is due, so the
   stall aborts hand `k` like any other and `GENESIS(k+1)` follows (§3.1).
2. **It anchors every peer's deadline to one agreed event**, which is what makes
   §4.10's acceptance gate safe. That gate has a receiver *buffer* an abort until
   its own deadline expires; buffering is bounded only if every peer's timer
   started at the same point, and `TERMINAL(k-1)` is the last thing every peer
   provably agreed on. `HAND_INIT` is not: peers accept different copies of it at
   different moments, and a peer that never accepts one would never start.

The window covers `hand_delay_ms` plus the whole of hand `k`; the constant in §13
is set on that basis and is not rescaled by this ruling. `STATE_MACHINE.md` §5.2
already reads it this way, so this sentence removes a corpus disagreement rather
than creating one.

If it expires the hand aborts under §8.4 with `cause = 1`,
`attributed = []` and `cert_hash = None`: **it produces no certificate and names
nobody.** The earlier form of this sentence — *"produces a certificate with
`kind = 2` naming every seat that has an outstanding contribution"* — was the
attribute-every-non-voting-seat rule that §8.4 deletes, and it is withdrawn. The
hand deadline is scoped on nothing, because it attributes nobody; that is what
makes it safe at every `|V|` (§8.4, D-008 point 4).

**The engine contains no clock.** Time enters the state machine only as a signed
`TIMEOUT_CERT`. `STATE_MACHINE.md` must carry the deadline as explicit state
rather than as a wall-clock read inside the engine (D-006). Since the certificate
stage became collective (§4.8) this is true without exception: `CERT_SETTLE_MS`
was the one wall-clock read left inside the chain-building rule and it is gone.

**One residual, stated rather than hidden.** A stage can close two ways — by the
subject's own event, or by certificates — and which one happened is chain content.
Two peers agree on it only if at least one required voter is honest and refuses to
vote after accepting the action. Where `|V| >= 2` that is the honest-voter
assumption already in force. Where `|V| < 2` there is no such voter, and D-008's
floor applies, so the ambiguity cannot arise: no certificate of either kind takes
effect at all (§8.3).

### 8.3 The timeout certificate

One peer asserting "time is up" cannot be enough — it would let anyone steal the
action from a player who was about to act.

**Everything in this section is scoped on `|V|`, the size of the required voter
set, and never on `n`, the seat count.** That is D-008, it is binding, and the
scoping *is* the protection. The seat count is not a quantity an attacker can
change; the effective voter set is. A rule written on `n` therefore guards
something nobody was attacking, which is how the A-1 heads-up attack survived
D-007 at every table size. Wherever an earlier draft of this document said "at
`n = 2`", it now says "when `|V| < 2`".

**Required voter set** `V(subject)`, normatively:

> `V(subject)` is the dealt-in seats, minus `subject`, minus every seat that a
> **completed, valid certificate earlier in this hand already names as
> attributed**. Nothing else removes a seat from `V`.
>
> **Being voted against is not exclusion.** A `TIMEOUT_VOTE` is one peer's
> unilateral assertion, this section concedes below that a lying voter is
> unprovable, and a rule that let an assertion shrink `V` would let one modified
> client shrink `V` to itself.

With no prior attribution `|V| = n - 1`, where `n = |dealt_in|`:

| dealt-in seats `n`, nothing yet attributed | required voters |
|---:|---:|
| 2 | 1 |
| 3 | 2 |
| 10 | 9 |

**That table is the nominal set, not the operative rule.** The operative rule is
the boxed definition above and the floor below, both of which read `|V|`. A
reader who takes `|V| = n - 1` as a rule rather than as a starting value has
reconstructed the defect D-008 exists to close.

The set is the same for `kind = 1` (action deadline) and `kind = 2`
(cryptographic-step deadline). **Folded seats are included**, because a folded
seat still holds a key share and must remain responsive until the hand ends; a
folded seat that goes silent is itself a subject.

**Normative — the below-the-floor rule. This paragraph is canonical for the whole
corpus (D-008 point 2, restated and marked as canonical by D-009 rule 2).
`STATE_MACHINE.md`, `THREAT_MODEL.md`, `CRYPTOGRAPHY.md` and `NETWORK_STACK.md`
point here and do not restate it in their own words.**

> A `TIMEOUT_CERT` whose required voter set has fewer than two members is
> **inert**, at every table size, for **both** kinds, in every document.
>
> It is **not chained**, **not evidence**, and has **no terminating effect**: it
> attributes nobody, closes no stage, produces no auto-action, moves no chips,
> ends no hand, produces **no `AbortRecord` of any kind**, and triggers **no
> forfeiture**. It is silently ignored. It is not an error either — a peer with
> an incomplete view can legitimately compute a smaller `V` than its neighbours —
> so emitting one is not a protocol violation and no seat is sanctioned for it.
> The deadline it was about stays advisory.
>
> This holds for `kind = 2` and the cryptographic-step deadline exactly as it
> holds for `kind = 1`, and it is not weakened by the hand deadline: a hand that
> cannot proceed ends at `hand_deadline_ms` under §8.4, on a local timer that
> needs no certificate, with `cause = 1`, `attributed = []`, `cert_hash = None`
> and stacks restored. **Liveness is not owed here.** `SPEC_CS.md` §19 ranks
> security above finishing a hand conveniently, and an effect one signature can
> manufacture is the thing D-008 exists to leave nowhere.

Unfolded into the two kinds, so that neither can be read as an exception to the
box above:

* **`kind = 1` with `|V| < 2`.** Inert. The action deadline is a UI countdown
  that produces no signed state transition (D-007 point 1). Nothing is chained,
  no seat is auto-folded, `consecutive_auto_actions` does not increment. At two
  dealt-in seats `|V| = 1` always, so this reproduces D-007's heads-up rule
  exactly — as a consequence of the general scoping, not as a special case. An
  earlier revision of this bullet said "invalid; every receiver rejects it",
  which contradicted the effect table three paragraphs below and gave the two
  kinds two dispositions where D-009 rule 2 gives them one. The disposition is
  uniform: ignored, not an error.
* **`kind = 2` with `|V| < 2`.** Inert, identically. The hand does not end here;
  it ends at `hand_deadline_ms` under §8.4, with `cause = 1`, `attributed = []`,
  `cert_hash = None` and stacks restored.

**The cost of that second bullet, stated rather than hidden.** Where `|V| < 2` a
stalled hand now waits `hand_deadline_ms` (600 000 ms in
`RATED_SNG_POKERTH_V1`) instead of `crypto_step_timeout_ms` (30 000 ms). The
outcome is identical — `cause = 1`, nobody attributed, stacks restored — only the
wait is longer, and the alternative is a certificate with an effect that one
signature can manufacture.

**The rage-quit half of this paragraph has been overtaken by D-010 and must not
be read as a `|V| < 2` limitation any more.** It said the escape D-005 closed was
"reopened wherever `|V| < 2`". Under D-010 an abort is neutral at every `|V|` and
on every path, so the escape is open everywhere: a player facing a losing pot goes
silent, waits `hand_deadline_ms`, and recovers its commitment, at two seats or at
ten. §4.10 states that cost in full and `THREAT_MODEL.md` X8 carries it. What remains specific to
this section is only the *wait*.

The floor itself is unaffected by that, and the reasoning for it has to be
restated because its old form appealed to chips. It read: letting one peer's
unilateral assertion take another's chips is a positive-gain attack on an honest
player, so a liveness loss is preferred to a theft. There is no theft left to
prefer against. What a below-floor certificate would still buy is a **one-message,
on-demand hand void**, manufacturable by a single signature at any moment — which
is strictly better for an attacker than waiting ten minutes for the same outcome,
and which D-009 rule 2 forbids on its own terms. The floor stands, for a smaller
and more honest reason than it used to have. The disposition question this paragraph
used to defer to a letter is **closed**: D-010 decided restoration, for the MVP,
everywhere. What stays open is what a later version does once the machinery is
sound, and D-010's own “when this is revisited” clause carries it (§12).

**Why the exclusion rule is inductive, and why that closes the attack.** Each
exclusion costs a *completed* certificate; each completed certificate needed
`|V| >= 2` at the moment it formed; so `V` cannot be shrunk by assertion at any
table size. The attack D-008 records — one modified client at six seats votes
against four seats to reduce `V` for the fifth to `{itself}`, completes a
`kind = 2` certificate on its own single signature, and collects the victim's
committed chips under D-005's forfeiture — is now rejected three times over: the
four uncertified votes remove nobody from `V`; had they removed anyone, `|V| = 1`
puts the resulting certificate under the no-effect rule; and under D-010 there is
no longer anything to collect, because an abort moves no chips. The first two are
the structural fix and are what this section relies on. The third is a
consequence of a decision that could be revisited, so it is named last and is not
load-bearing — a rule that is only safe because the payoff is currently zero is
not a safe rule. Neither half needs new
cryptography and both are checkable by a third party replaying the transcript
(§8.5).

The review's other candidate — requiring the subject's own counter-signature — is
rejected: a subject who is absent cannot counter-sign, so it converts every
genuine timeout into a deadlock, and D-007 already recorded that no arrangement of
signatures gives both properties. `SPEC_CS.md` §36 forbids inventing a
construction here, so none is invented.

**What settles the race between a late action and a certificate.** An honest
voter votes about a subject only when nothing has arrived **from that subject**
at that stage — the per-subject condition §4.8 states normatively, in the same
words, in its emitter gate and its receiver check. It is per subject and not per
stage: at a collective stage the voter has by construction accepted events from
the seats that did contribute, and a gate that counted those would never open at
all. So: if at least one required voter is honest and saw the action, no
certificate forms and the action stands. If every required voter is dishonest, or none saw the action, a
certificate forms. **There is no artefact that settles the race when a voter lies
about what it saw.** A `TIMEOUT_VOTE` and the action it is about are signed by
different keys and sit in different `event_class` slots (§4.8, §5.2), so a peer
that both accepted an action and voted that it timed out has not equivocated and
nothing is provable from the two bodies. Whether a voter should be obliged to
publish a signed `ACTION_SEEN` first, making a lying voter provable, is **Q-08**
(§12); it is a design change with a cost in messages and latency and it is not
adopted.

**Effect of a certificate:**

| Certificate | Effect |
|---|---|
| `kind = 1`, `\|V\| >= 2` | the engine applies **check** if nothing is owed and **fold** if facing a bet. Never fold a hand that could check for free (D-006 point 1). The betting round continues among the remaining players, the street card opens, and the hand plays out normally. Nothing aborts and nothing waits. |
| `kind = 2`, `\|V\| >= 2` | `HAND_ABORT` with `cause = 1`, `subject_seat` named in `attributed` **as evidence only**, and **no chips moved** — stacks restored to their start-of-hand values like every other abort (D-010, §4.10). The row previously read "chips forfeited per D-005"; that is deleted |
| either kind, `\|V\| < 2` | **no effect.** Not an error, not evidence, not chained; no `AbortRecord`; silently ignored. The boxed rule above is canonical and this row restates nothing beyond it. A hand that cannot proceed ends instead at `hand_deadline_ms` under §8.4, with `cause = 1`, `attributed = []`, `cert_hash = None` and stacks restored. |

After `MAX_CONSECUTIVE_AUTO_ACTIONS = 3` certificates against one seat, that seat
is marked sitting out and enters the D-005 absent-seat state at the next hand
boundary. This path exists only where `|V| >= 2`: below the floor there is no
action timeout certificate with any effect, so `consecutive_auto_actions` never
increments from a timeout and such a seat is never marked sitting out by the
deadline path. It can still sit out voluntarily. When the counter does fire, the
seat keeps its stack, pays its blinds and antes, takes no cards, and drains until
it busts. The player can sit back in at a hand boundary with `PLAYER_SIT_IN`
(D-006 point 4).

**This is not an eviction and D-010 point 3 does not reach it.** Being marked
sitting out removes nobody: the seat is kept, the stack is kept, no chip is
transferred, and the player rejoins at the next hand boundary by their own choice.
It is D-006's rule for a player who is not answering, it is the live-poker
behaviour for an absent seat, and it is reached from repeated missed decisions
rather than from a finding of fault. What D-010 forbids is unseating or
block-listing a peer *on the strength of a proof*; nothing here does that, and no
`EquivocationProof` or `DISPUTE` is an input to this counter.

**A reading of D-006 that must be stated.** D-006 point 2 says the auto-action is
"a real, signed protocol event in the transcript" and also that "every peer
derives it identically from the same state". Those two pull in opposite
directions, because the absent player cannot sign an action they did not take and
nobody else may sign in their name. This document resolves it as: **the
certificate is the signed transcript event, carrying `|V|` signatures; the
check-or-fold is its deterministic effect, derived by every peer.** Both halves of
D-006 point 2 are satisfied and nothing is signed on an absent player's behalf.
This resolution applies where `|V| >= 2` only; below the floor D-007 and D-008
supersede D-006 and there is no auto-action event of any kind.

### 8.4 Simultaneous failures

If two seats become subjects at once, neither certificate can reach unanimity,
because each required voter set contains the other subject.

**Two subjects at one stage is a normal state and voting about both is safe.**
That was not true when this section was written: a voter that did the obvious
thing — vote against each seat that owed it something — signed two bodies in one
anti-replay slot, and its own votes were an `EquivocationProof` against it. The
subject is now part of the slot key (§5.2, §4.8), so an honest voter emits one
vote per subject and each sits in its own slot. **A voter is not asked to choose
one subject, and no rule anywhere in this document limits it to one vote per
stage.** That mattered: the cheap alternative — "at most one vote per stage, take
the lowest failing seat" — would have made the deadlock argument below depend on
a tie-break rule rather than on the voter sets, and it would have asked an honest
peer to withhold information it holds in order to stay safe, which §6.3 already
records as the wrong shape of fix.

**The exclusion rule this section used to carry is deleted.** It read:

> *Rule: a seat already named as the subject of an outstanding, older unmet
> deadline is excluded from `V`.*

Nothing bounded how many seats it could remove, and nothing required that the
"outstanding unmet deadline" ever produce a certificate — a seat became "named as
the subject" the moment any peer emitted a `TIMEOUT_VOTE` against it (§4.8),
which §8.3 concedes is unprovable when the voter lies. One modified client at six
seats could therefore vote against four seats, declare the fifth, and complete a
`kind = 2` certificate on its own single signature, forfeiting the victim's
committed chips under D-005. That is the A-1 attack at every table size, and it
survived D-007 only because every rule that would have rejected it was scoped on
`n`. D-010 has since removed the chips from the end of that sentence; the rule
stays deleted anyway, because what the attack still buys — an on-demand hand void
manufactured by one signature — is an effect D-008 exists to leave nowhere,
whatever it pays. D-008 deletes the rule and replaces it with §8.3's inductive exclusion: **a
seat leaves `V` only once a completed, valid certificate already names it as
attributed.**

**Two simultaneous subjects therefore deadlock, by design.** Neither certificate
can form, because neither voter set can be reduced. That is the price of the
fix and it is a price worth paying: the exclusion rule bought only a faster path
to an abort that the whole-hand limit already reaches, and it cost the integrity
of the certificate at every table size.

The deadlock is resolved by the whole-hand limit and by nothing else. When
`hand_deadline_ms` expires the hand aborts with `cause = 1`, `attributed = []`,
`cert_hash = None`, and stacks restored. **Nobody is named**, because a seat that
did not vote may be silent, partitioned or simply slow, and naming a partitioned
honest seat puts a false statement in a permanent record. That reason used to be
stated as "D-005 forfeiture against a partitioned honest seat is a positive-gain
attack on that seat"; under D-010 no attribution moves a chip, so the reason is
now about the truth of the record rather than about its payoff — which is the
weaker and the correct reason, and it is enough. Every abort returns exactly
`committed_hand[s]` to every seat `s` and moves no chips between seats (§4.10),
whether it names anybody or not.

**This abort path is the one piece of the deadline machinery that is safe at
every `|V|`, including `|V| = 0`,** because it names nobody and moves nothing
between seats. It needs no voter set, no certificate and no unanimity; it is a
local timer expiry that every peer reaches from the same signed `HAND_INIT` and
the same relative duration. That is why D-008's scoping rule has nothing to
weaken here: there is no attribution to gate. It is also the path every
`|V| < 2` stall now takes (§8.3).

**It covers hands only, and that is the whole extent of the claim.**
`hand_deadline_ms` runs from `TERMINAL(k-1)` (§8.2, R-1), so it covers hand `k`
in full — including a `HAND_INIT` that never completes, which under the old
"from `HAND_INIT`" reading was covered by nothing from hand 2 onward. It still
says nothing about a stall in the **setup chain** (`hand_id = 0`, `JOIN_REQUEST`
through `TABLE_READY`), because `TERMINAL(0)` does not exist until that chain
ends. A table that never forms is not aborted by this path and needs no
certificate: §4.3's formation-abandonment rule disposes of it with a local timer
and the advert's `AD_TTL_MS`, with no cooperation from anybody and nothing
signed. Between the two rules every stall from `JOIN_REQUEST` to the last hand is
covered, with no gap at the join between them.

**Normative — the join deadline. This sentence is canonical for the corpus;
`STATE_MACHINE.md` reproduces it and does not restate it in its own words
(N9(b)).**

> **There is no join-deadline certificate and no `DeadlineKind::Join`.**
> `TIMEOUT_VOTE` and `TIMEOUT_CERT` are defined only over stages of a hand chain
> and are never emitted in the setup chain (`hand_id = 0`). A table that fails to
> form is disposed of by §4.3's formation-abandonment rule — a **local
> lobby-layer timer** and the advert's `AD_TTL_MS` — which signs nothing, chains
> nothing, and needs no voter set, no unanimity and no `|V|` floor because it
> attributes nobody and moves no chips.
>
> The wire consequence, which is all this document states: **no wire message
> produces a join-deadline certificate, so no engine alphabet may contain a
> variant for one.** What transition replaces it, out of which phase, and with
> what guard is `STATE_MACHINE.md`'s (D-011 rule 1); the paragraph that specified
> that transition here is deleted.

None of that is a gap to be filled by inventing a certificate kind, and none
should be invented — `SPEC_CS.md` §36 prefers the plain local timer to a
construction, and a certificate would need a voter set that a table which never
formed does not have.

**This is a safe default, not a solution.** It lets two colluding seats void a
hand by going silent together, at no cost to either. The earlier rule — attribute
*every* non-voting seat — is deleted, because it converted a partition into a
confiscation.

D-006 did not settle simultaneous failure, and neither does this. **OPEN QUESTION
Q-02** stays open with the above as its written interim behaviour; the same
question is `STATE_MACHINE.md` Q3 and is carried as blocking in
`THREAT_MODEL.md` §9.2 (OQ-E).

### 8.5 How a timeout becomes evidence

The certificate is a permanent, self-contained artefact. A third party with the
transcript can check, with no table state: every embedded vote's canonicality and
signature; that the voters are exactly the required set derived from the
transcript; that the voters are exactly `V(subject)` **as §8.3 defines it**,
including that every seat missing from `V` was removed by a completed, valid
certificate that the same transcript contains; that `parent_event_hash` is the
correct `stage_hash`; and that `deadline_ms` matches the parent's
`next_deadline_ms`. That last check is what makes D-008's inductive exclusion
verifiable rather than merely stated: a certificate assembled over a `V` shrunk
by bare votes fails it, offline, forever.

Where `|V| >= 2` the certificate therefore proves that every seat still required
to vote independently observed the subject fail to act within a deadline the
subject itself could compute. **Where `|V| < 2` it proves only that one peer said
so, which is why §8.3 gives it no effect of any kind.** The count that matters is
`|V|`, not the seat count: a certificate at a ten-seat table whose voter set has
been reduced to one member is exactly as worthless as a heads-up one, and the
verifier treats them identically.

What it does **not** prove: that the subject's client was malicious rather than
disconnected, crashed, or DoSed. `SPEC_CS.md` §18 places denial of service outside
the protocol, and D-001 requires that a relayed connection loss be treated exactly
like any other disconnect. Repeated aborts attributable to one identity are
visible to everyone; in play money that is the whole penalty, and §18 forbids
claiming more.

**And under D-010 no protocol action follows from any of it.** A completed
certificate closes the stage it was about, and its attribution is written to the
transcript; that is the end of what this document does with it. It costs the
subject no chips and does not remove it from the table. The certificate is
produced because it is the artefact a human or a later version adjudicates from —
whether producing it is worth its cost while nothing consumes it is the open
question §12 records.

---

## 9. Size limits and resource bounds (`SPEC_CS.md` §17, §27)

### 9.1 Why an explicit cap even though the codec is safe

`minicbor` validates a claimed length against the remaining input **before**
allocating. Measured with a counting global allocator: a byte string claiming 4
GiB with 0 bytes present allocated **0 bytes** and returned an error; so did one
claiming `u64::MAX`; so did an array claiming 4 GiB of elements; 20 000 levels of
nesting allocated 12 bytes and did not overflow the stack. [CRYPTO §4.8]

That is a good property and it is **not a substitute for a frame cap.** It bounds
memory per message; it does not bound the number of messages, the CPU spent
verifying signatures and proofs, or the bandwidth. Every limit below is a hard
protocol constant.

### 9.2 Frame and channel limits

Names and values are §13's; this table is the per-channel view of them.

| Constant | Value | Notes |
|---|---|---|
| `GOSSIP_MAX_TRANSMIT` | 65 536 B | GossipSub `max_transmit_size`; two-sided. The crate default, pinned there for interoperability |
| `LOBBY_MSG_MAX` | 8 192 B | application ceiling for any lobby message of any type; the transport limit above is a backstop, never the operative limit |
| `SNAPSHOT_REQ_MAX` | 1 024 B | `set_request_size_maximum` on `/p2p-poker/lobby-snapshot/1` |
| `SNAPSHOT_RESP_MAX` | 262 144 B | `set_response_size_maximum`; fits `128 × 1 536 = 196 608` plus overhead |
| `JOIN_REQ_MAX` | 4 096 B | `set_request_size_maximum` on `/p2p-poker/join/1`; `JOIN_REQUEST`'s own payload cap is 512 B, so this is envelope headroom only |
| `JOIN_RESP_MAX` | 16 384 B | `set_response_size_maximum`; `JOIN_ACCEPT` embeds a whole advert (≤ 1 536 B) plus a ten-entry roster |
| `TABLE_FRAME_MAX` | 262 144 B | `u32` length prefix on `/p2p-poker/table/1` |
| `MAX_EMBEDDED_EVENT` | 32 768 B | per-element cap for an embedded `SignedEvent` in `DISPUTE` and `HAND_ABORT` |
| `TABLE_AD_SIGNED_MAX` | 1 536 B | a complete `SignedEvent` of an advert, wherever one is embedded or forwarded |
| `MAX_BODY` | frame − 128 B | `EventBody` bytes |
| `MAX_PAYLOAD` | body − 256 B | per-type caps below are tighter and are the ones that apply |

`TABLE_FRAME_MAX` is **not** sized by the shuffle objects. It is sized by
`DISPUTE`, whose four evidence entries of `MAX_EMBEDDED_EVENT` each already exceed
128 KiB, and by `HAND_ABORT`'s 80 000 B. A 131 072 B frame could not carry the
protocol's own evidence-bearing message; the shuffle objects are an order of
magnitude smaller and are not the binding case.

A length prefix greater than the cap closes the stream immediately, without
reading the body.

### 9.3 Per-message payload caps

| Message | Cap (B) | Typical (B) |
|---|---|---|
| `HELLO` | 1 024 | ~200 |
| `CAPABILITIES` | 1 024 | ~150 |
| `LOBBY_TABLE_AD` (`TABLE_AD_MAX`) | 1 024 | ~350; the 29 fields of §7.2 total under 500 B at worst case |
| `LOBBY_TABLE_REMOVE` | 128 | ~50 |
| `LOBBY_PLAYER_PRESENCE` | 512 | ~150 |
| `LOBBY_SNAPSHOT_REQUEST` | 128 | ~45 |
| `LOBBY_SNAPSHOT_RESPONSE` | 262 144 | ≤ 128 × ~500; the hard ceiling is ≤ 128 × 1 536 = 196 608 |
| `LOBBY_CHAT` | 2 048 | ~120 |
| `JOIN_REQUEST` | 512 | ~180 |
| `JOIN_ACCEPT` | 8 192 | ~1 800 |
| `JOIN_REJECT` | 128 | ~45 |
| `PLAYER_LIST` | 2 048 | ~1 200 |
| `TABLE_READY` | 1 024 | ~200 |
| `RNG_COMMIT` | 64 | 34 |
| `RNG_REVEAL` | 128 | 68 |
| `HAND_INIT` | 512 | ~140 |
| `DECK_INIT` | 256 | 102 |
| `SHUFFLE_STEP` | 8 192 | 3 435 |
| `SHUFFLE_PROOF` | 16 384 | 5 615 |
| `DECK_COMMIT` | 256 | 101 |
| `DEAL_PRIVATE` | 4 096 | ≤ 18 × 132 = 2 376 |
| `BOARD_REVEAL` | 1 024 | ≤ 3 × 132 = 396 |
| `SHOWDOWN_REVEAL` | 512 | 264 |
| `SHOWDOWN_MUCK` | 64 | ~10 |
| `ACTION_*` | 64 | ~20 |
| `TIMEOUT_VOTE` | 256 | ~60 |
| `TIMEOUT_CERT` | 8 192 | ≤ 9 × ~250 |
| `STATE_HASH` | 128 | 70 |
| `STATE_ACK` | 128 | 102 |
| `DISPUTE` | 140 000 | ≤ 4 × 32 768 = 131 072 plus envelope overhead |
| `HAND_COMPLETE` | 4 096 | ~600 |
| `HAND_ABORT` | 80 000 | ~400, or up to 2 × 32 768 when it carries evidence |
| `PLAYER_SIT_OUT` / `_SIT_IN` / `_LEAVE` | 64 | ~10 |

Typical sizes for the cryptographic objects are the measured `ziffle` 0.1.0
encodings: `ShuffleProof<52>` 5 547 B, `MaskedDeck<52>` 3 432 B, `PublicKey` 33 B,
`OwnershipProof` 65 B, `RevealToken` 33 B, `RevealTokenProof` 98 B. [MENTAL §5.1]

**Relay budget arithmetic, corrected twice.** A table is a full mesh, so one
circuit connects exactly one pair; the earlier comparison of table-wide traffic
against a per-circuit cap was wrong in kind, not only in magnitude. The
replacement claim — that `max_circuit_bytes` is per circuit **and per direction**
— is **also wrong**, and is corrected here.

**`max_circuit_bytes` is a single bidirectional total for the whole circuit.**
Verified in source: `libp2p-relay 0.21.1` relays a circuit with one `CopyFuture`
holding one `bytes_sent: u64` counter (`src/copy_future.rs:41-48`), and **both**
`forward_data` calls — src→dst at lines 88–95 and dst→src at lines 97–104 —
increment that same counter before it is compared against the cap. The default
131 072 is therefore 128 KiB of **combined** traffic, not 128 KiB each way, and
every figure derived from it halves.

Note that kubo's `docs/config.md` describes its `ConnectionDataLimit` as applying
"in each direction". Either the Go and Rust implementations differ or that wording
is loose; we did not determine which. Our own relay is Rust, so the Rust behaviour
binds us, and for a third-party relay of unknown implementation the stricter
reading — one shared bidirectional budget — is the safe assumption and is the one
used below.

| Quantity | Value | Basis |
|---|---:|---|
| `ShuffleProof<52>` + `MaskedDeck<52>`, one shuffler | 8 979 B | 5 547 + 3 432, measured [MENTAL §5.1] |
| Shuffle traffic over **one circuit, both directions**, per hand | **17 958 B** | 2 × 8 979: this peer's own step and proof outbound, the far peer's inbound, both against one counter; **independent of `n`** |
| Hands per circuit against a 131 072 B public-relay budget, shuffle only | **~7** | 131 072 / 17 958 |
| The same including the signed event stream | **~5 hands** | order-of-magnitude; to be measured, and the measurement must be **bidirectional** |
| A relayed peer's **total** per-hand outbound at an `n`-seat table | `(n-1) × 8 979 B` | 44 895 B at six seats — a bandwidth figure, spread over `n-1` separate circuits, never a single cap. Unaffected by the correction, because it counts one direction of `n-1` circuits, not one circuit. |
| The binding public-relay limit | **`max_circuit_duration = 120 s`**, not the byte cap | a session lasts far longer than two minutes |

So a public relay is unusable for a poker session, but the reason is the two-minute
circuit **duration**, not the byte budget. That conclusion survives the correction
— it held at ~14 hands and it holds at ~7 — which is why this is a corrected figure
rather than a changed decision. The D-002 relay configuration that fixes both is
transport-local and lives in `NETWORK_STACK.md` §9.6; it is not restated here.

### 9.4 Collection bounds

Every `Vec` in every payload has a hard maximum, checked before the elements are
processed.

| Collection | Max | Rule |
|---|---|---|
| `MAX_SEATS` | 10 | [RULES B1], PokerTH's own table maximum |
| roster / `SeatEntry` lists | `MAX_SEATS` | sorted by seat, unique seats, unique keys |
| `board` | 5 | and length ∈ {0,3,4,5} at street boundaries [RULES A2] |
| `dealt_in` | `MAX_SEATS` | ascending, unique |
| `stacks`, `deltas`, `final_stacks`, `committed_*`, boolean vectors | `MAX_SEATS` | length must equal the occupied-seat count exactly |
| `pots` | `MAX_SEATS` | at most one side pot per all-in level |
| `RevealEntry` per message | 25 | `2 × MAX_SEATS + 5` |
| `deck_index` | < 52 | |
| votes in `TIMEOUT_CERT` | `MAX_SEATS - 1` = 9 | ascending by voter seat, unique |
| `evidence` in `DISPUTE` | 4 | each ≤ `MAX_EMBEDDED_EVENT` = 32 768 B |
| `MAX_DISPUTES_PER_SENDER_PER_HAND` | 8 | accepted distinct `DISPUTE`s from one sender for one hand; `DISPUTE` is unchained and has no stage slot to bound it (§4.9, §5.3) |
| `capabilities` | 32 | each name ≤ 32 B, sorted, unique |
| adverts in a snapshot (`SNAPSHOT_MAX_ADS`) | 128 | each a complete `SignedEvent` ≤ `TABLE_AD_SIGNED_MAX` = 1 536 B; 128 × 1 536 = 196 608 B, inside `SNAPSHOT_RESP_MAX` |
| `ledger_delta` in `HAND_INIT` | `MAX_SEATS` | ascending by seat, unique |
| `MAX_STAGES_PER_HAND` | 2 048 | exceeding it aborts the hand |
| `MAX_TRACKED_TABLES` | 4 096 | LRU |
| `MAX_TRACKED_PRESENCE` | 8 192 | LRU |
| `MAX_CBOR_NESTING_DEPTH` | 8 | our own limit, well above the 3 levels we use |

String fields: `display_name` ≤ 32 B, `table_name` ≤ 64 B, `client_name` ≤ 64 B,
`note` ≤ 256 B, `LOBBY_CHAT`'s `text` ≤ 512 B. All must be valid UTF-8 with no
control characters (`U+0000`–
`U+001F`, `U+007F`, and the bidi overrides `U+202A`–`U+202E`, `U+2066`–`U+2069`).
All are display-only and are never identifiers, never parsed, and never inputs to
any state transition.

### 9.5 Connection and rate bounds

**Connection limits are transport bounds and `NETWORK_STACK.md` owns them**
(D-011 rule 1). The four `libp2p::connection_limits` values and the dial-attempt
cap that stood here — `max_pending_incoming`, `max_established_incoming`,
`max_established_per_peer`, `memory_connection_limits`, and dial attempts per DHT
lookup round — are deleted, together with the note about which cargo feature they
need. They are not two-sided: a peer that sets them differently is not
incompatible with anyone.

What survives is the bounds that are **protocol** bounds, because they decide
whether an event is accepted:

| Bound | Value |
|---|---|
| table-stream events per peer per second | 64, then throttle |
| shuffle proofs verified per peer per hand | 1 per shuffle round; a second is a violation |
| concurrent table sessions per client | 8 |

**Every bound in this section is keyed on volume, never on fault** (D-010
point 3, D-011 rule 3). Throttling a peer that floods is a resource decision and
is identical for a buggy peer and a hostile one; it is not a sanction, it never
reads `attributed`, and no proof of any kind feeds it.

### 9.6 Fuzzing obligations (`SPEC_CS.md` §27)

The parser must never crash, allocate without bound, execute anything, read out
of bounds, or bypass schema validation, for any input.

**Every change to anything enumerated in this section is a security-critical
change under `SPEC_CS.md` §31 and requires a reproducing regression test to land
first. See `docs/CONTRIBUTING.md`.**

That pointer is load-bearing, so this is what must be on the other end of it.
`docs/CONTRIBUTING.md` carries `SPEC_CS.md` §31 in full: small logical commits; no
deletion or rewrite of large parts of a working implementation without written
justification; and — the clause the sentence above depends on — **a regression
test that reproduces the problem must land before any security-critical change.**
The process rules live there rather than here because a wire specification is the
wrong place to keep them, which is how they get skipped; the security-critical
clause is repeated here, and in `CRYPTOGRAPHY.md` §12, because those are the two
sections that enumerate what counts as security-critical. If the file is absent
or does not carry that clause, this sentence is unsatisfied and the gap is in the
file, not in the pointer.

Required `cargo-fuzz` targets:

1. `SignedEvent` decode + canonicality gate — the single outer entry point.
2. `EventBody` decode + gate.
3. Each of the 39 payload decoders, dispatched by `event_type`.
4. The deck library's deserialisers: `ShuffleProof`, `MaskedDeck`, `RevealToken`,
   `OwnershipProof`. Phase 0 explicitly did **not** fuzz these and flagged it as
   an open risk, and there is a known `assert!` panic path in that library's
   transcript code if a serialised element exceeds a 256-byte buffer —
   unreachable for 33-byte points, but a panic on network-derived data. [MENTAL
   §4.1, §9 risk 5] Deserialise everything with the library's validating mode.
5. The `TIMEOUT_CERT` verifier, which decodes nested `SignedEvent`s and is the
   deepest recursion the protocol has.

**One standing test that is not a fuzz target, required by D-009 rule 1.**
`SPEC_CS.md` §25 already requires a `CheaterEquivocation` peer. The mirror of it
is the test that catches the class §5.2's property names: an **honest** peer,
driven through every legal interleaving the protocol permits — including two
simultaneous timeout subjects at one collective stage, the case that produced
defect M2 — must never emit two events that share a slot key, and no
`EquivocationProof` against it may verify. The property belongs in the
adversarial suite as a standing assertion rather than in prose, because prose is
what let it pass three review passes. Every new chained message type added to
§4.11 extends that test's alphabet.

---

## 10. Wire compatibility

### 10.1 What may change in a minor version

A minor bump keeps `protocol_version` and every protocol string. Old and new
clients interoperate.

* **New `event_type` codes**, provided they are only emitted after the
  corresponding capability has been negotiated, and provided the lobby channel
  ignores unknown types rather than rejecting the sender. Inside a table session
  an unknown `event_type` is still a violation, because the roster's capability
  set already established what everyone speaks.
* **New capability names.** Unknown names are ignored by construction (§1.3).
* **New `preset_id` values and new parameter values within the declared ranges.**
  Presets are carried by value in the advertisement, so a new preset is data, not
  schema.
* **New enumerated values** in `reason`, `cause`, `mode`, `game`, `button_rule`,
  `odd_chip_rule`, `showdown_policy` — but only where the receiver's behaviour on
  an unknown value is already specified as "reject the advert" (lobby) or "reject
  the event" (table). A new `game` value simply makes older clients skip that
  table, which is correct.
* **Local policy**: rate limits tightened, dial budgets, cache sizes, GossipSub
  mesh parameters, timers that are not two-sided constants, relay policy under
  D-001/D-002.
* **New domain strings** for new hashes. Existing ones are frozen.
* **Any change to the deck library's internals** that does not change the
  serialised sizes or the verification result — which is exactly why it sits
  behind our own trait [MENTAL §10].

### 10.2 What forces a major version

The canonicality gate makes this list shorter and sharper than in most protocols,
and the reason is worth stating: **appending a field to a `#[cbor(array)]` struct
changes the array length, and therefore the canonical bytes, and therefore every
signature and every hash over it.** An old client re-encoding a new struct
produces different bytes, the gate fires, and the event is rejected. There is no
"ignore unknown trailing fields" behaviour and there cannot be one, because
tolerating trailing data is precisely the equivocation hole the gate exists to
close.

So:

* **any change to the field set of `EventBody`, `SignedEvent`, or any payload
  struct** — adding, removing, reordering, or retyping a field;
* the `DOMAIN_EVENT` prefix, the signature scheme, or the verification mode;
* the hash function, or any existing domain string;
* the canonical encoding rules of §2.2;
* the stage-numbering rules, the `stage_hash` construction, or the genesis
  construction;
* the contents of `PublicTableState` or the set of checkpoints;
* the card encoding or the deck-index → role map;
* `GOSSIP_MAX_TRANSMIT`, `LOBBY_MSG_MAX`, `TABLE_FRAME_MAX`, or any per-message cap **in the
  loosening direction** (tightening a cap only rejects messages a conforming peer
  would not send, but loosening one means new peers send frames old peers drop, so
  both directions are treated as major for safety);
* `MAX_SEATS` or any collection bound;
* the GossipSub topic string or any libp2p protocol string;
* the deck suite identifier, the curve, or the shuffle argument;
* removing a capability, an `event_type`, or an enumerated value that peers may
  already be emitting.

A major bump changes `PROTOCOL_MAJOR`, hence every protocol string, hence old and
new clients never negotiate. That is the intended behaviour: silent partial
incompatibility in a signed, hash-chained protocol is far worse than a clean
refusal to connect.

### 10.3 Version pinning within a session

`protocol_version` is fixed by the `LOBBY_TABLE_AD` for the whole life of a table.
Every envelope of that table must carry exactly that value; a mismatch is a
violation, not a renegotiation. A client that supports versions 1 and 2 runs
version 1 at a version-1 table for the table's whole life, including hands dealt
after it has upgraded internally.

---

## 11. Which wire mechanism stops which attack

**`THREAT_MODEL.md` owns the classification of what this system prevents,
detects and merely bounds, and this document restates none of it** (D-011
rule 1). What stood here was a three-part copy of that classification — a
"cryptographically prevented" table, a "detected and attributed" table, and
eleven bullets of "neither prevented nor solved". Every one of those statements
lives in `THREAT_MODEL.md` §8 and its X-numbered attack rows, and keeping a
second copy in a wire specification is exactly the drift D-011 exists to stop:
the two copies were edited by different passes and the survivors of the D-010
sweep were found in the gaps between them.

What is left here is the only part no other document owns — **the map from an
attack to the mechanism in *this* document that acts on it**. It is a routing
table, not a verdict: it says where to look, and `THREAT_MODEL.md` says what the
outcome is.

| Attack | The wire mechanism, in this document | Classified in |
|---|---|---|
| reading unrevealed hole cards, or the board early | reveal-token publication is street-gated (§4.6); no secret that could reconstruct a card enters the transcript (§3.4) | `THREAT_MODEL.md` |
| forging or altering another participant's action | the signed byte string of §2.4 and `verify_strict` at §4.0 step 9 | `THREAT_MODEL.md` |
| altering hand history | the chain of §3.1–§3.2 | `THREAT_MODEL.md` |
| a fake, duplicate or removed card | proof verification at §4.0 step 14 | `THREAT_MODEL.md` |
| replaying a shuffle proof into another hand or seat | the `ctx` binding (§4.5) | `THREAT_MODEL.md` |
| replaying a reveal token onto a different card | the DLEQ proof (§4.6) | `THREAT_MODEL.md` |
| replay of any chained event | `previous_event_hash` and `sequence` (§5.1) | `THREAT_MODEL.md` |
| replay of unchained traffic | the per-type rules in §5.2.1's box | `THREAT_MODEL.md` |
| an illegal or out-of-turn action | §4.0 steps 12 and 13 | `THREAT_MODEL.md` |
| a false stack, pot or award | `STATE_HASH` at every checkpoint (§6.2) and full recomputation of `HAND_COMPLETE` (§4.10) | `THREAT_MODEL.md` |
| a `HAND_ABORT` that moves a chip | §4.10's receiver check on `n(4) deltas` and `n(5) final_stacks` | `THREAT_MODEL.md` |
| sending different histories to different peers | the slot key and predicate of §5.2.1–§5.2.2, and the divergence path of §6.3 | `THREAT_MODEL.md` |
| two byte encodings of one event | the canonicality gate of §2.5, run before the signature | `THREAT_MODEL.md` |
| malformed or oversized frames | the caps of §9 and the fuzzing obligations of §9.6 | `THREAT_MODEL.md` |
| stalling a hand to force an abort | `hand_deadline_ms` from `TERMINAL(k-1)` (§8.2) and the terminal stage of §4.10 | `THREAT_MODEL.md` X8 |
| faulting a table with a false `state_hash` | §6.3 case (c) and §6.4 | `THREAT_MODEL.md` X29 |

**Three statements of this document's own limits, kept here because they are
about the wire and not about the threat model:**

* **A mechanism above is a *check*, never a *sanction*.** Every row ends in a
  rejection, a buffered event or a signed record. No row ends in a chip moving, a
  seat being taken or a key being blocked (D-010, D-011 rule 3). A reader who
  reads the left column as a list of things that are punished has read the wrong
  document.
* **§11's old bullet list is where the rage-quit escape was disclosed**, and the
  disclosure is not weakened by moving it: `THREAT_MODEL.md` §8 limitation 4 and
  X8 carry it in full, in the terms `SPEC_CS.md` §18 requires, and §4.10 states
  the wire half — every abort restores every stack, at every table size, on every
  path, and nothing in this protocol charges for it.
* **What this document assumes and does not verify** is the deck layer's
  soundness. Phase 0 verified that the chosen implementation rejects thirteen
  specific attacks, which is not soundness; a line-by-line review against the
  Bayer–Groth paper is a prerequisite [MENTAL §9 risk 1]. If that assumption
  fails, the first four rows of the table above fail with it. `CRYPTOGRAPHY.md`
  owns the assessment; this sentence records only that this document's rows
  depend on it.


## 12. Open questions

| # | Question | Blocks | Owner |
|---|---|---|---|
| **Q-01** | Is `showdown_policy = TDA_MUCK` offered at all, or is `MANDATORY_REVEAL` the only permitted value? Mucking preserves live poker's strategic value but weakens §13 verification from "the award was correct" to "the award was correct given who did not forfeit" — a colluding pair could have one player muck a winner. Mandatory reveal is fully verifiable but leaks strictly more than real poker does, which is itself a long-run edge. [RULES A8] | `STATE_MACHINE.md`, the engine's showdown path | project owner; belongs in `DECISIONS.md` |
| **Q-02** | **How is a multi-subject deadline certificate constructed, and whom does it attribute?** `signers == participants \ {subject}` is unachievable when two or more seats are simultaneously unresponsive, because each required voter set contains the other subject. **Interim behaviour, written into §8.4 and revised by D-008:** the exclusion rule that removed already-subject seats from `V` is **deleted** — it let one modified client shrink `V` to itself at any table size (N3) — so simultaneous subjects now deadlock until `hand_deadline_ms`, when the hand aborts with `cause = 1`, `attributed = []`, `cert_hash = None`, stacks restored. Two colluding seats can still void a hand for free. D-006 specified unanimity for the single-subject case only. Same question as `STATE_MACHINE.md` Q3; carried as blocking in `THREAT_MODEL.md` §9.2 as OQ-E. | `STATE_MACHINE.md`, Phase 4 | project owner |
| **Q-03** | Should `TABLE_READY` require every participant to have completed a §1.2 handshake with every other, or is founder-mediated introduction acceptable when a pair cannot connect directly? Requiring a full mesh is the safe answer and is what §1.5 specifies, but it means one unreachable pair prevents a table that would otherwise form. Relates to D-004's symmetric-NAT case. | `NETWORK_STACK.md`, §1.5 | project owner |
| **Q-04** | **CLOSED.** Should the certificate stage be collective instead of a `CERT_SETTLE_MS` timer with a lowest-seat tie-break? **Answer: collective** (§4.8). A certificate's body is a pure function of the votes, so by §3.2's stage-kind principle it carries no choice and must not have a single writer; the emitter set is `V(subject)`; the timer, the tie-break and the chain fork all disappear together, and `CERT_SETTLE_MS` is deleted from §13. | — | closed by the Phase 0 fix plan, C-10 |
| **Q-05** | Does a `DISPUTE` need to be gossiped to the whole lobby, or only within the table mesh? Lobby-wide gossip gives non-participants durable evidence of equivocation, which is the only reputational pressure play money has; it also creates a defamation and spam vector, since a `DISPUTE` is cheap to emit and its `note` is attacker-controlled text. | `THREAT_MODEL.md`, §7.6 | project owner |
| **Q-06** | Should the per-hand transcript be persisted in full to the profile directory by default? It is the only artefact behind §6.3 case (c)'s diagnostic claim — the transcript is necessary for it, and, pending **OQ-A**, not sufficient — and it is small (~20 KB heads-up, ~60 KB six-handed). But it is also a permanent record of every hand every opponent played, which has its own privacy cost. | `storage/`, `THREAT_MODEL.md` | project owner |
| **Q-07** | On `HAND_ABORT cause = 4` (unresolvable divergence) the chips are restored, because no peer can be attributed, so any single peer has a free escape from a losing pot at the price of the table (§6.4). The alternatives — forfeiting an unnamed party's commitment, or settling from the last `STATE_ACK`-agreed checkpoint — each need a numbered decision and neither is adopted here. D-010 decides the first half **against** for the MVP; what stays open is whether settling from the last agreed checkpoint is worth building. Formerly this document's `OQ-D` (R-2). | §6.4, `STATE_MACHINE.md` | project owner |
| **Q-08** | Should a required voter be obliged to publish a signed `ACTION_SEEN { sequence, event_hash }` before it may vote, so that vote-and-seen are two events by one key in one slot and a lying voter becomes provable (§8.3)? A design change with a cost in messages and latency; not adopted. Formerly this document's `OQ-C` (R-2). | §8.3, §8.4 | project owner |

### The corpus-wide open questions — `DECISIONS.md`'s letters, adopted (R-2)

**`DECISIONS.md` is authority for the `OQ-*` series and this document adopts its
letters and its wording** (D-011 rule 1). Three letters exist corpus-wide —
**OQ-A**, **OQ-D**, **OQ-F** — and this document uses no others. The parallel
scheme that stood here, in which `OQ-A`, `OQ-B`, `OQ-C`, `OQ-D` and the
disambiguated `OQ-F(engine)` / `OQ-F(produce)` named different questions than the
same letters name in `DECISIONS.md`, is deleted. A reader following a letter from
one document to the other now lands on the same question.

**The mapping, recorded once so an old cross-reference can still be followed, and
not to be maintained:**

| This document's former label | Now |
|---|---|
| `OQ-F` / `OQ-F(engine)` — the reference engine | **OQ-A** |
| `OQ-B` — a dispute path not needing the accused's signature | **OQ-D** |
| `OQ-F(produce)` — produce the machinery at all? | **OQ-F** (unchanged) |
| `OQ-A` — restoration versus forfeiture below the floor | **closed by D-010**, which decided restoration everywhere for the MVP; D-010's "when this is revisited" clause carries what is left |
| `OQ-D` — settle from the last agreed checkpoint | **Q-07**, this document's local series |
| `OQ-C` — an obligatory `ACTION_SEEN` before a vote | **Q-08**, this document's local series |

`OQ-E` is `THREAT_MODEL.md`'s and names the same question as **Q-02** above; it
is referenced, never redefined here. `STATE_MACHINE.md` cross-references written
against the old scheme are rewritten by that document against this table.

---

**OQ-A** — the reference engine. `DECISIONS.md`'s wording, quoted rather than
paraphrased:

> A named, versioned reference engine, since several sections claim disputes are
> "deterministically adjudicable by any third party running the reference engine"
> and no such engine is defined (review N2). Interim answer: withdraw the claim;
> the engine is defined when the crate has a tagged release.

Where this document depends on it: §3.5 check 7 and its "cannot check" note, §6.3
case (c), §6.4's withdrawn third bound, and §11's diagnostic claim. All four
carry the weakened form already — the evidence is preserved and is sufficient for
a human or a future adjudicator to **diagnose** the divergence; no adjudication
procedure is specified, and `SPEC_CS.md` §36 forbids the stronger word. Answering
OQ-A would restore a real third bound on the §6.4 attack; until then that bound
does not exist and must not be counted.

**OQ-D** — a dispute path that does not require the accused peer's signature.
`DECISIONS.md`'s wording:

> A dispute path that does not require the accused peer's signature — circular at
> every table size, not only heads-up (D-007 point 4, review A-1). Interim answer
> under D-010: a dispute that cannot resolve ends the hand neutrally, so the
> circularity costs a hand rather than a stalemate.

Where this document depends on it: §6.3 case (b), whose whole disposition rests
on that circularity, and §4.10's note that no adjudication of a withholding is
specified and none is claimed.

**OQ-F** — whether the machinery is produced at all. `DECISIONS.md`'s wording:

> Whether `TIMEOUT_VOTE`, `TIMEOUT_CERT` and `EquivocationProof` should still be
> *produced* in the MVP now that D-010 gives them no effect, or be deferred
> wholesale until the machinery is sound. Producing them keeps the transcript
> adjudicable later; deferring them removes four passes' worth of surface.

What deferring would remove **from this document**, listed so the scope of the
decision is visible: the two deadline classes and their subject axes in §5.2.1's
key, the matching structures in §5.3, `V(subject)` and its inductive exclusion
rule in §8.3, §8.5's offline verifier, and the `EquivocationProof` object in
§5.2. No transition in this version consumes any of them. It is recorded and
**not answered**: it is a scope decision for the owner, and it changes what
ships rather than what is true.


### Closed elsewhere, recorded here so the answer is not lost

`STATE_MACHINE.md` **Q2** — whose key signs a derived `HAND_INIT` /
`HAND_COMPLETE`, and is a counter-signature needed — is **closed** by the
collective-stage form of §3.2 and §4.4: every present seat signs its own
byte-identical copy under its own application key, so there is no unsigned event
to design and the counter-signature the question asked about is the mechanism.

### Carried forward from `DECISIONS.md`, unresolved and untouched here

* **Relay admission** — `identify` protocol name versus lobby presence (D-002).
  This document uses the identify name as the discriminator in §1.1 for the
  purpose of naming the protocol string, and does **not** settle the admission
  policy. `NETWORK_STACK.md` must.
* **Open-source licence.** Not this document's concern, but it constrains the deck
  library choice: `zshuffle` was rejected in part because GPL-3.0-only would force
  the whole client to GPL-3.0 [MENTAL §4.3].

---

## 13. Constants

**This section is the single normative home for every two-sided constant in the
corpus.** `NETWORK_STACK.md` §14 points here and restates none of them; a value
that appears in two documents with two values is exactly the defect this
consolidation removes. The names are the ones a Rust `constants` module would
carry, which is why several differ from earlier drafts of this document:
`LOBBY_MAX_MESSAGE`, `TABLE_MAX_FRAME`, `SNAPSHOT_MAX_REQUEST` and
`SNAPSHOT_MAX_RESPONSE` are retired in favour of `GOSSIP_MAX_TRANSMIT` /
`LOBBY_MSG_MAX`, `TABLE_FRAME_MAX`, `SNAPSHOT_REQ_MAX` and `SNAPSHOT_RESP_MAX`.
**No value has two names anywhere in the corpus.**

Values marked `(local)` are not two-sided: a client may tune them without a
protocol-version change. Everything else is two-sided, and changing it is a
major-version change (§10.2).

```
PROTOCOL_VERSION                = 1
PROTOCOL_MAJOR                  = 1

IDENTIFY_PROTOCOL               = "/p2p-poker/1"
LOBBY_TOPIC                     = "/p2p-poker/lobby/1"
LOBBY_CHAT_TOPIC                = "/p2p-poker/lobby-chat/1"
SNAPSHOT_PROTOCOL               = "/p2p-poker/lobby-snapshot/1"
JOIN_PROTOCOL                   = "/p2p-poker/join/1"
TABLE_PROTOCOL                  = "/p2p-poker/table/1"

LOBBY_DERIVATION_STRING         = "p2p-poker/mainline-lobby/v1"
LOBBY_INFOHASH                  = fd7c0d69433e32e425db3ca2b7d7718928739f01
RELAY_DERIVATION_STRING         = "p2p-poker/mainline-relay/v1"
RELAY_INFOHASH                  = 9c18d8c80f69de3aa079b2ef519bc4bbb67e1cc1

DOMAIN_EVENT                    = "p2p-poker/v1/event" NUL-padded to 24 bytes
  hex: 70 32 70 2d 70 6f 6b 65 72 2f 76 31 2f 65 76 65 6e 74 00 00 00 00 00 00
       |<---------- 18 ASCII bytes ---------->|<---- 6 NUL ---->|
  TO_BE_SIGNED = DOMAIN_EVENT || u32_be(len(body_bytes)) || body_bytes
  This is a literal signature prefix, NOT a blake3::derive_key domain; its
  separators are slashes, unlike every string in the §2.8 register.

hash                            = BLAKE3, keyed via derive_key, length-prefixed
signature                       = Ed25519, verify_strict only
encoding                        = minicbor 2.3.0, #[cbor(array)], no maps, no floats

MAX_SEATS                       = 10
MAX_STAGES_PER_HAND             = 2 048
MAX_CBOR_NESTING_DEPTH          = 8
MAX_DISPUTES_PER_SENDER_PER_HAND = 8            (DISPUTE is unchained; §4.9, §5.3)

GOSSIP_MAX_TRANSMIT             = 65 536 B      (GossipSub max_transmit_size)
LOBBY_MSG_MAX                   = 8 192 B       (application ceiling, any lobby message)
TABLE_AD_MAX                    = 1 024 B       (LOBBY_TABLE_AD payload)
TABLE_AD_SIGNED_MAX             = 1 536 B       (a complete SignedEvent of an advert)
LOBBY_CHAT_MAX                  = 2 048 B       (LOBBY_CHAT payload)
SNAPSHOT_REQ_MAX                = 1 024 B
SNAPSHOT_RESP_MAX               = 262 144 B
SNAPSHOT_MAX_ADS                = 128
JOIN_REQ_MAX                    = 4 096 B
JOIN_RESP_MAX                   = 16 384 B
TABLE_FRAME_MAX                 = 262 144 B     (u32 length prefix on the table stream)
MAX_EMBEDDED_EVENT              = 32 768 B      (one embedded SignedEvent as evidence)

AD_TTL_MS                       = 90 000        (since receipt)
AD_REBROADCAST_MS               = 30 000
MAX_AD_LIFETIME_MS              = 300 000       (bound on expires_at vs local time)
MAX_CLOCK_SKEW_MS               = 120 000
PRESENCE_TTL_MS                 = 120 000
PRESENCE_HEARTBEAT_MS           = 40 000
REANNOUNCE_INTERVAL_MS          = 600 000
IDLE_CONNECTION_TIMEOUT_MS      = 60 000
MDNS_QUERY_INTERVAL_MS          = 15 000
SNAPSHOT_PEER_COUNT             = 4
MAX_TRACKED_TABLES              = 4 096         (local)
MAX_TRACKED_PRESENCE            = 8 192         (local)
MAX_ADS_PER_TABLE_KEY_PER_MIN   = 4             (local)
MAX_ADS_PER_PEER_PER_MIN        = 20            (local)
MAX_PRESENCE_PER_PEER_PER_MIN   = 4             (local)

HANDSHAKE_DEADLINE_MS           = 15 000
MAX_CONSECUTIVE_AUTO_ACTIONS    = 3

RATED_SNG_POKERTH_V1:
  seats                         = 10
  min_players_to_start          = 10
  start_stack                   = 10 000
  first_small_blind             = 50
  big_blind                     = 2 * small_blind
  ante                          = 0
  blind_raise                   = DOUBLE_EVERY_N_HANDS, every 11 hands
  small_blind_cap               = 50 000
  button_rule                   = DEAD_BUTTON
  odd_chip_rule                 = FIRST_SEAT_LEFT_OF_BUTTON
  showdown_policy               = MANDATORY_REVEAL       (pending Q-01)
  action_timeout_ms             = 20 000
  action_grace_ms               = 5 000
  crypto_step_timeout_ms        = 30 000
  hand_deadline_ms              = 600 000
  join_deadline_ms              = 120 000
  hand_delay_ms                 = 7 000
  password                      = none
```

The `RATED_SNG_POKERTH_V1` values, their PokerTH provenance, and the four
`[OUR CHOICE]` timing values are documented in [RULES B1–B5]. `crypto_step_timeout_ms`
is this document's addition; it has no PokerTH analogue and must comfortably
exceed one shuffle prove-and-propagate cycle, measured at ~100 ms of proving plus
network latency [MENTAL §5.1].

**`RATED_SNG_POKERTH_V1` is fully specified and is not playable by the MVP.** It
pins `seats = 10` and `min_players_to_start = 10`, while `SPEC_CS.md` §32 requires
two-player heads-up as the first supported mode and §1.3 scopes the MVP at
`nlhe/2-6`. The MVP ships `CUSTOM` tables; `RATED_SNG_POKERTH_V1` becomes playable
when `nlhe/7-10` lands. This is stated rather than left to inference because a
reader who does not notice will build the wrong acceptance test —
`STATE_MACHINE.md` §9.5 carries the same statement next to its heads-up
reachability analysis, and the Phase 8 acceptance test for D-003/D-004 uses a
`CUSTOM` two-seat table.

**Retired constants.** `LOBBY_MAX_MESSAGE`, `TABLE_MAX_FRAME`,
`SNAPSHOT_MAX_REQUEST` and `SNAPSHOT_MAX_RESPONSE` are renamed as listed at the
head of this section. `CERT_SETTLE_MS = 2 000` is **deleted**: the certificate
stage is collective (§4.8), so there is no settle window and no tie-break, and the
constant was the last wall-clock read inside the chain-building rule.

**Not here.** The D-002 relay configuration (`max_circuit_duration`,
`max_circuit_bytes`, `max_reservations`, `max_circuits`, `max_circuits_per_peer`
and the rest) is transport-local rather than two-sided and lives in
`NETWORK_STACK.md` §9.6. It is not restated here, and §9.3's relay arithmetic
points at it.

---

## 14. Objections and recorded consequences

Every ruling of `docs/research/PHASE0_FIXPLAN.md` addressed to this document was
applied as written. Three of them were applied over an objection, recorded here
rather than resolved in the text, because five editors producing one consistent
corpus matters more than any one editor's judgement. Notes 4 and 5 record two
consequences of applying `DECISIONS.md` D-008 and of closing the Phase 1
verification's finding N4, where following the decision literally changed
behaviour this document had previously specified. Notes 6 to 11 do the same for
`DECISIONS.md` **D-009** and for the second verification pass's findings M2, M4,
M5 and N9(a): a longer anti-replay slot key, a normative re-emission rule, one
withdrawn reading of `TIMEOUT_VOTE`'s legality, one deleted clause, and one
corrected emitter cell. Notes 12 to 15 do the same for **D-010** and for the
fourth verification pass's findings P1, P2, P6 and M3. Notes 16 to 20 do the same
for **D-011** and for the Phase 2 gate's findings G1, G2, P3, R-1, R-2 and R-4 —
this revision decides the terminal stage, rewrites the anti-replay slot key as a
literal tuple, and deletes this document's copies of what other documents own.
All are stated so that the other documents can be checked against them rather
than against the text they replace.

**1. C-2's canonical RNG beacon combine drops the session binding this document
carried.** The plan's canonical form is
`seed = h("p2p-poker v1 rng-beacon", [ r_1, …, r_n ])`, and §4.4 now carries it
verbatim. The form this document previously carried was
`h("p2p-poker v1 rng-beacon", [ session_id, for each seat s ascending: r_s ])`.
C-2's per-document instructions for `PROTOCOL.md` name only §4.5's `ctx`, §4.3's
`session_id`, §4.4's `RNG_COMMIT` commitment and §2.8's retired list; they do not
mention the combine, so the divergence between this document's form and the
canonical one may not have been noticed when the ruling was written. Applying the
canonical form was still the right call — `STATE_MACHINE.md` §7.9 is being edited
to carry exactly it, and leaving two forms in the corpus is the defect C-2 exists
to remove — but the objection is that the ruling removes a direct binding without
saying so. The binding is not lost: each `r_s` is bound to `(table_id,
session_id, committer)` by its own stage-1 commitment, and the beacon's own events
chain to `GENESIS(0)`, which contains `advert_hash` and `roster_hash(0)`. It is
now indirect rather than direct, and that should be an explicit choice rather than
a side effect.

**2. C-7 says there are "now five" protocol strings; §1.1 lists six.** C-7 adds
`/p2p-poker/join/1` to the four strings this document carried and states the new
total as five. C-8, applied to the same section's constant register, adds
`LOBBY_CHAT_TOPIC = /p2p-poker/lobby-chat/1`, which is also a protocol string for
`PROTOCOL_MAJOR = 1`. §1.1 therefore lists six and says six. The two rulings are
individually right and their arithmetic does not compose; the count in C-7 is
stale rather than wrong.

**3. A-1 says the `cert_hash` requirement for `cause = 1` stands; A-7 specifies a
`cause = 1` abort with `cert_hash = None`.** Both are applied: §4.10 requires
`cert_hash` for `cause = 1` with a single named exception, the §8.4
`hand_deadline_ms` path, where unanimity was by construction never reached and no
certificate can exist. Reading A-1's sentence as absolute would make A-7's interim
behaviour unrepresentable, so it is read as scoped to the certificate path it was
written about. If that reading is wrong, §4.10 and §8.4 need a different interim
disposition, not a different field rule.

**4. D-008 point 2, applied literally, changes what ends a stalled hand below the
floor — and the corpus must follow this text, not the text it replaced.** D-008
says a certificate with `|V| < 2` "has no effect. It is not an error and not
evidence; the deadline simply stays advisory." This document previously said
something weaker for `kind = 2`: that such a certificate *is* accepted, with
`attributed = []`, and *does* end the hand. Those two cannot both be true — ending
a hand is an effect, and it is an effect one signature would be manufacturing,
which is the exact shape D-008 exists to forbid. So the certificate is now inert
below the floor and the hand ends instead on §8.4's `hand_deadline_ms` path.

The *outcome* is unchanged in every respect that touches chips: `cause = 1`,
`attributed = []`, `cert_hash = None`, stacks restored to their start-of-hand
values. What changes is the trigger and the wait — `hand_deadline_ms` rather than
`crypto_step_timeout_ms`, ten minutes rather than thirty seconds under
`RATED_SNG_POKERTH_V1`. Two visible consequences: §4.10's `attributed = []` table
has three cases collapsed to two, since the old "`cause = 1` at `n = 2`" row was
the `n`-scoped statement of a case that is now `|V| < 2` and reaches the same
disposition through the hand-deadline row; and `STATE_MACHINE.md` needs no
separate two-seat abort transition, only the hand-deadline one.

Recorded rather than silently taken, because it is a behaviour change and not a
rewording, and because the alternative reading — keep the certificate's
hand-ending effect and gate only the attribution — is defensible on liveness
grounds. It was rejected because it leaves an effect that `|V| = 1` can produce,
and D-008's whole method is to leave none.

**5. Making `DISPUTE` unchained is the version of N4's fix that adds nothing.**
The verification offered two: `chain_scope = 0` with the §2.3 sentinels and its
own anti-replay, or a distinct `event_class = 3` plus a normative `sequence`
rule. The second was rejected. It keeps `DISPUTE` in a slot system it never
belonged to and then needs a new rule to say which slot each of §6.3's two
mandatory disputes occupies — and any such rule is one more thing that can be got
wrong in a way that manufactures evidence against an honest peer, which is the
defect being fixed. `chain_scope = 0` is what §2.3's own definition already
implies for a message that is "legal outside its stage", it reuses the sentinel
and per-type anti-replay machinery A-4 built, and it needs no new envelope value.
Its cost is one new bound, `MAX_DISPUTES_PER_SENDER_PER_HAND` (§4.9, §5.3, §13),
because an unchained message has no stage array to bound it. That cost is paid
explicitly rather than left implicit.

**6. D-009 rule 1 is applied by lengthening the key, and the two cheaper fixes
were rejected (M2).** The verification offered three dispositions for
`TIMEOUT_VOTE`: move it to `chain_scope = 0` with §2.3's sentinels, extend the
slot key with the subject, or make one vote per stage normative with a
deterministic subject-selection rule. D-009 rule 1 picks the second in terms —
"the subject seat is part of the key, not part of the body only" — and this
document applies it to `event_class` 2 as well, because the rule it states is
general and the certificate has the same shape one level up.

The first was rejected because unchaining the vote costs more than it saves: a
vote is a stage event with a real `sequence` and a real parent, §3.2's collective
`stage_hash` is taken over `event_class = 2` events, and §8.5's offline verifier
walks the chain to recompute `V`. Taking the votes out of the chain would need
that machinery rebuilt on payload-carried fields, which is a large change to
close a small hole. `DISPUTE` was the opposite case — it occupied no slot to
begin with — which is why the same finding took opposite dispositions for the two
message types.

The third was rejected on the ruling's own reasoning. "At most one vote per
stage, lowest failing seat" asks an honest peer to withhold something it knows in
order to avoid manufacturing evidence against itself, and D-009 rule 1 says in
terms that in such a case the defect is in the key and not in the sender. It also
makes §8.4's deadlock argument depend on a tie-break rule rather than on the
voter sets, which is a weaker footing for the one section that has already been
rewritten twice.

**7. The cost of the longer key, stated rather than hidden.** Three things get
slightly more expensive. `EquivocationProof` verification now decodes one payload
field for `event_class` 1 and 2 — a `u8` or a 32-byte string, under the gate the
body already passed — so the proof is still self-contained but is no longer
literally "the envelope and nothing else". §5.3's anti-replay state gains a
subject axis for the two deadline classes, held sparsely and only for stages that
actually stalled, because the dense form would be `MAX_SEATS` times larger than
the array it replaces for a structure a legitimate hand barely touches. And every
future chained message type now carries a proof obligation before it may be added
to §4.11. All three are paid deliberately. The alternative is a predicate that
mandatory honest behaviour satisfies, which is not a predicate.

**8. Re-emission is re-transmission — a rule that was implicit and is now
normative (§5.2).** `emitted_at_unix_ms` is advisory and varies freely, so a peer
that re-signs an event it has already sent produces a second distinct body in the
same slot and is indistinguishable from an equivocator. §1.5's forwarding rule
always assumed byte-identical repeats; nothing said the *originator* was under
the same obligation. It is now said, because the slot-key property depends on it
for every chained type, not only the two that gained a subject axis.

**9. M5 is decided for the receiver's reading, and the emitter's is withdrawn
(§4.8).** The two sentences said different things: "has not accepted any valid
event for that stage" against "has not accepted an event for that stage from
`subject_seat`". The per-subject condition is normative in both places. The
any-event reading was discarded rather than reconciled because it is not merely
stricter — at a collective stage it is never satisfiable, so it deletes the
`kind = 2` deadline path entirely and makes §8.4 a section about an impossible
state. Choosing it would have closed M2 by accident, by removing the machinery M2
lives in, and that is not a fix. Recorded here because it is the sentence, not
the disposition, that an implementer would otherwise have had to guess.

**10. `PLAYER_LEAVE`'s courtesy clause is deleted, not given an envelope (M4).**
The alternative was to add `PLAYER_LEAVE` to §2.3's exhaustive unchained list and
to §4.11 as a second `chain_scope = 0` row, correcting the "one table-mesh
message" sentence with it. That was rejected: the clause bought a UI hint,
§4.10's own text already says a leave is never required and a vanishing client
must be handled identically, and adding a second unchained table-mesh type would
have needed its own per-type anti-replay bound for a message with nothing to
protect. Deleting it leaves `PLAYER_LEAVE` exactly what §2.3 and §4.11 always
said it was: chained, single-writer, at a hand boundary. A mid-hand leave is
expressed by silence and disposed of by §8.

**11. `HAND_ABORT`'s emitter set is stated on `attributed`, and §4.11's cell
followed §4.10 rather than the reverse (N9(a)).** The two cells disagreed —
"minus any seat whose failure is the reason for the abort" against "all present
seats" — and only one of them is implementable: a collective stage completes when
every required emitter is heard, and the attributed seat is by construction
silent, so the summary table's reading deadlocked the abort path behind the
failure it exists to dispose of. The subtraction is now written on the body's own
`n(1) attributed` field rather than on the prose phrase, so that every peer
derives the same emitter set from the same bytes with no judgement call about
what "the reason for the abort" means.

**Superseded by note 16.** The subtraction was the right repair of the *disagreement*
and the wrong repair of the *defect*: it made both cells implementable and left
the `attributed = []` paths — which are three of the four — with the silent seat
still inside the required set. `HAND_ABORT` no longer has a required emitter set
at all (§3.2, §4.10), so nothing is subtracted from anything and this note stands
only as the record of a step on the way.

**12. D-010 is applied by deletion, and this is the list of what left (§4.10,
§4.0, §5.2, §6.3, §6.4, §8.3, §8.4, §8.5, §11).** In order of how much depended
on it: the forfeiture rule on `HAND_ABORT` and its distribution arithmetic; the
`attributed = []` carve-outs, which now describe every abort; the addition of a
key to `libp2p::allow_block_list` on a signature or proof failure (§4.0) and on a
verifying `EquivocationProof` (§5.2); the refusal of a seat to an accused key; and
the consequence clause of every attribution. `n(4) deltas` is now fixed at all
zeroes and `n(5) final_stacks` at the start-of-hand stacks, which makes both a
function of state bound into `GENESIS(k)`.

Two things were deliberately **not** deleted, and both should be checked by
whoever disagrees. `attributed` stays in the chained body, as a signed record: it
costs six bytes a seat, D-010 point 2 keeps attribution as evidence, and removing
it would leave `cause = 1` unable to say which of two silent seats the certificate
was about. And D-008's `|V| >= 2` floor stays, although its old justification —
that a below-floor certificate lets one signature take an honest peer's chips — is
gone with the chips; §8.3 restates the reason on what a below-floor certificate
would still buy, which is a one-message on-demand hand void. A rule kept for a
reason that has evaporated is a rule waiting to be deleted by someone who does not
know why it is there.

The objection to recording: D-010 accepts a real regression, the rage-quit escape,
and this document now says so in five places rather than one, which reads as
labouring the point. It is deliberate. The previous four passes each found the
same defect wearing a different message's name because the consequence was stated
once and inferred everywhere else.

**13. `HAND_ABORT cause = 5` is deleted rather than given a position in the order
(P2).** The verification offered both dispositions and preferred neither. An
`EquivocationProof` travels in a `DISPUTE`, which N4 correctly made unchained, so
all four components of the engine's ordering key are sentinels and the buffer
cannot place it; two honest peers therefore applied it at different chain
positions and derived incompatible terminal bodies, chosen by the attacker's
delivery order. The candidate ordering rule — apply the proof at the earliest
stage boundary at or after the later of its two embedded events' `sequence`, which
is a pure function of the proof's own bytes — is sound as far as it goes, and it
was still rejected: it adds a consensus rule to a corpus that has misspecified one
four times, in order to preserve a terminal effect that under D-010 achieves
nothing. §5.2 now carries the general rule (no unchained event has any consensus
effect), which covers `DISPUTE` and any future unchained type as well, rather than
a special case for one proof.

The cost is that an equivocator is no longer stopped mid-hand. It is bounded: the
divergence the equivocation causes still ends the hand through §6.3's chained
path, the proof is still produced and retained, and under D-010 stopping the hand
sooner would have moved no chips anyway. **`STATE_MACHINE.md` must follow**: the
transition that consumes an `EquivocationProof` into a hand abort has no
counterpart here any more, and an engine alphabet containing a variant no wire
rule produces is the defect class N9(b) and P4 both closed.

**14. The divergence re-emission gets a new stage rather than a longer slot key
(P1, §4.9, §6.3 step 3).** Three candidates existed: its own stage, carriage as
`DISPUTE` evidence, or a `checkpoint_round` component added to §5.2's slot key.
The third is the smallest edit and is what D-009 rule 1's own wording suggests —
close it by extending the key — and it was still rejected, because a fresh
`sequence` separates the two bodies for free and the canonical predicate is
already carrying two special cases; a third would have to be quoted in five
documents. The second was rejected by note 13's own rule: it would make a
consensus decision turn on an unchained event.

Recorded because it is a wire change, not a clarification: a divergence
reconciliation now occupies a stage that did not exist before, and
`STATE_MACHINE.md`'s two resolution transitions consume *that* stage's completion.
Their previous trigger — the phrase "transcript reconciliation completes" and then
a re-emission this document never specified — was an event no wire rule produced.

**15. The `OQ-*` label collision is resolved by adopting `DECISIONS.md`'s letters
(R-2), and the objection recorded here is withdrawn.** The note that stood here
declined to renumber, on the reasoning that a wire specification is the wrong
place to settle a numbering owned by the decisions document and that renumbering
would break `STATE_MACHINE.md`'s references. Under D-011 rule 1 the first half is
exactly backwards: `DECISIONS.md` **is** the owner, so following it is not this
document settling a numbering, it is this document ceasing to keep a competing
one. The second half is real and is paid: §12 carries the old-to-new mapping once
so an existing cross-reference can be followed, and `STATE_MACHINE.md` rewrites
its own references against that table. Five passes recorded some form of this
collision without an editor taking it; this edit takes it.

**16. The terminal stage is decided here, and it is decided against both of the
shapes the gate proposed (P3, G1).** The gate ranked two corrections for G1:
move the abort to `s+1` behind a sentinel, or give it `event_class = 3`. Neither
is taken. The first needs a new `stage_hash` rule for an abandoned stage and
leaves the abort's own `sequence` a quantity peers can disagree about; the second
extends an envelope field §2.3 calls exhaustive and needs a `subject(E)` arm.
What is taken instead is **the key of D-011 rule 2** — `event_type` in the slot,
which the decision mandates anyway — plus a **witness-independent terminal
stage** with **no `stage_hash`** and a `TERMINAL(k)` taken from `GENESIS(k)`
(§3.1). That is one new hash function and one new stage shape, against a new
envelope value or a new chaining rule, and it closes P3 and both halves of G1 in
the same edit.

The consequence worth recording, because it is a real loss and it is not
recoverable later without a wire change: **an aborted hand's transcript is no
longer bound into the next hand's genesis.** §3.1 argues that nothing depends on
it, and nothing does today — an abort moves no chip and awards no pot, so no
later value is a function of the aborted hand's content. If a future version
gives an abort a chip consequence, that binding has to come back, and it cannot
come back as a hash of a disputed quantity. It would need a *decided* quantity,
which is what a reconciliation round produces (§4.9), and that is a larger design
than this one.

**17. `event_type` enters the slot key and two detections become rejections
(D-011 rule 2, §5.2.1, §4.11 rows 25–30).** The key is now capacity-one for every
one of the 39 types with no exception and no caveat, which is what D-009 rule 1
has asked for six times. The price is that two *different* `event_type`s from one
signer at one `sequence` no longer form an `EquivocationProof`: fold-versus-raise
for one turn, and reveal-versus-muck at one showdown. Both are still rejected by
§4.0 step 12, both still cause the divergence that ends the hand, and both are
still carriable as `DISPUTE` evidence — what is lost is the label, and under
D-010 the label carries no consequence.

The objection to be recorded honestly is that this is a *narrowing of a security
predicate*, and the corpus has narrowed that predicate in every pass. Four of
those narrowings were extensions of the key to stop false positives; this one is
an extension that also causes false negatives, which is a different direction. It
is taken because D-011 rule 2 is binding and states `event_type` explicitly, and
because the alternative — a derived `slot_type` that is the identity on 37 types
and merges the alternatives on the other two — is a second concept in a place
whose whole purpose is to have exactly one. If a later pass finds the false
negatives matter, the smaller fix is to make the five `ACTION_*` codes one
`event_type` with the kind in the payload, which restores fold-versus-raise as an
equivocation and shrinks the catalogue from 39 to 35.

**18. Nothing consumes an `EquivocationProof`, stated as a plain negative (G2).**
The box in §5.2 previously said the proof "is not consumed by any transition"
while `STATE_MACHINE.md` T55 consumed one and ended a hand with it. Under D-011
rule 1 the wire document decides what a message may cause, so the negative is the
answer and it is now stated in the form that cannot be read as an omission: there
is no `cause` value, none will be added, and an engine variant for one names an
outcome no legal message can carry. The alternative — overruling §5.2 and letting
a proof end a hand — was available and needed a numbered decision, and it
reopens P2's two-honest-peers fork exactly as filed. It is not taken.

**19. `hand_deadline_ms` starts at `TERMINAL(k-1)` (R-1), and that is a
precondition rather than a preference.** The gate called this "one sentence in
one document" and it would have been, before note 16. §4.10's acceptance gate has
a receiver **buffer** a terminal abort until its own deadline expires, and
buffering is bounded only if every peer's timer started from the same agreed
event. `HAND_INIT` is not one: a peer that never accepts a copy never starts. So
R-1's fix is load-bearing for the terminal stage, not only for the stalled-init
hole it was filed about.

**20. Seven copies of other documents' material are deleted rather than
corrected (D-011 rule 1).** §11's three-part restatement of `THREAT_MODEL.md`'s
classification, replaced by a routing table from an attack to the mechanism in
*this* document; §2.8's BLAKE3 rationale and audit limitation, which are
`CRYPTOGRAPHY.md` §3.2's; §1.4's libp2p behaviour column and its GossipSub and
`libp2p-stream` notes, and §1.5's connectivity floor, and §9.5's
`connection_limits` table, all `NETWORK_STACK.md`'s; §4.10's absent-seat paragraph
and §8.4's join-timeout transition paragraph and §6.3/§6.4's two `Diverged`-phase
sentences, all `STATE_MACHINE.md`'s. In each case a pointer replaces the copy and
the wire fact underneath it — which is what this document is actually normative
about — is stated in one sentence. Three partial restatements of the slot key,
in §4.0 step 10a, §4.8 and §5.1, are replaced by a reference to §5.2.1.

The objection: some of those copies were load-bearing for a *reader*, and §11 in
particular was where a reader of this document alone learned that the rage-quit
escape is open. That reader now has to open `THREAT_MODEL.md`. It is accepted
because five passes measured what the copies cost — they drift, and the D-010
sweep's three survivors were found in exactly the gaps between them — and because
a wire specification that also classifies threats will be edited by whoever is
fixing the wire.
